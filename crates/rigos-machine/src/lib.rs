#![forbid(unsafe_code)]

use rigos_core::{Diagnostic, InspectionResult, Observation};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

pub const MACHINE_SCHEMA: &str = "dbyte.rigos.machine-snapshot/v1";

#[derive(Debug, Clone)]
pub struct MachineContext {
    pub proc_root: PathBuf,
    pub sys_root: PathBuf,
}

impl Default for MachineContext {
    fn default() -> Self {
        Self {
            proc_root: "/proc".into(),
            sys_root: "/sys".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CpuSnapshotV1 {
    pub logical_processors: u32,
    pub model_names: Vec<String>,
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemorySnapshotV1 {
    pub memory_total_bytes: Observation<u64>,
    pub memory_available_bytes: Observation<u64>,
    pub huge_pages_total: Observation<u64>,
    pub huge_pages_free: Observation<u64>,
    pub huge_page_size_bytes: Observation<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TemperatureSensorV1 {
    pub name: String,
    pub label: Option<String>,
    pub temperature_millicelsius: Observation<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MachineSnapshotV1 {
    pub architecture: String,
    pub cpu: CpuSnapshotV1,
    pub memory: MemorySnapshotV1,
    pub temperatures: Vec<TemperatureSensorV1>,
}

pub fn inspect(ctx: &MachineContext) -> InspectionResult<MachineSnapshotV1> {
    let mut diagnostics = Vec::new();
    let cpu = inspect_cpu(&ctx.proc_root.join("cpuinfo"), &mut diagnostics);
    let memory = inspect_memory(&ctx.proc_root.join("meminfo"), &mut diagnostics);
    let mut temperatures = inspect_hwmon(&ctx.sys_root.join("class/hwmon"), &mut diagnostics);
    temperatures.sort_by(|a, b| a.name.cmp(&b.name).then(a.label.cmp(&b.label)));
    InspectionResult::success(
        MachineSnapshotV1 {
            architecture: std::env::consts::ARCH.into(),
            cpu,
            memory,
            temperatures,
        },
        diagnostics,
    )
}

fn inspect_cpu(path: &Path, diagnostics: &mut Vec<Diagnostic>) -> CpuSnapshotV1 {
    let Ok(text) = fs::read_to_string(path) else {
        diagnostics.push(Diagnostic::error(
            "machine.cpu.unavailable",
            "machine_cpu",
            "CPU metadata is unavailable.",
        ));
        return CpuSnapshotV1 {
            logical_processors: 0,
            model_names: vec![],
            flags: vec![],
        };
    };
    let mut processors = 0u32;
    let mut models = BTreeSet::new();
    let mut flags = BTreeSet::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "processor" => processors = processors.saturating_add(1),
            "model name" => {
                models.insert(value.trim().to_owned());
            }
            "flags" => flags.extend(value.split_whitespace().map(str::to_owned)),
            _ => {}
        }
    }
    if processors == 0 {
        diagnostics.push(Diagnostic::warning(
            "machine.cpu.malformed",
            "machine_cpu",
            "No logical processors were found in cpuinfo.",
        ));
    }
    CpuSnapshotV1 {
        logical_processors: processors,
        model_names: models.into_iter().collect(),
        flags: flags.into_iter().collect(),
    }
}

fn observed_kib(values: &std::collections::BTreeMap<String, u64>, key: &str) -> Observation<u64> {
    values
        .get(key)
        .and_then(|v| v.checked_mul(1024))
        .map(|value| Observation::Observed {
            value,
            source: "proc_meminfo".into(),
        })
        .unwrap_or_else(|| Observation::Unavailable {
            reason: "missing_or_overflow".into(),
        })
}

fn observed_count(values: &std::collections::BTreeMap<String, u64>, key: &str) -> Observation<u64> {
    values
        .get(key)
        .copied()
        .map(|value| Observation::Observed {
            value,
            source: "proc_meminfo".into(),
        })
        .unwrap_or_else(|| Observation::Unavailable {
            reason: "missing".into(),
        })
}

fn inspect_memory(path: &Path, diagnostics: &mut Vec<Diagnostic>) -> MemorySnapshotV1 {
    let mut values = std::collections::BTreeMap::new();
    match fs::read_to_string(path) {
        Ok(text) => {
            for line in text.lines() {
                if let Some((key, rest)) = line.split_once(':') {
                    if let Some(raw) = rest.split_whitespace().next() {
                        if let Ok(value) = raw.parse() {
                            values.insert(key.to_owned(), value);
                        }
                    }
                }
            }
        }
        Err(_) => diagnostics.push(Diagnostic::error(
            "machine.memory.unavailable",
            "machine_memory",
            "Memory metadata is unavailable.",
        )),
    }
    MemorySnapshotV1 {
        memory_total_bytes: observed_kib(&values, "MemTotal"),
        memory_available_bytes: observed_kib(&values, "MemAvailable"),
        huge_pages_total: observed_count(&values, "HugePages_Total"),
        huge_pages_free: observed_count(&values, "HugePages_Free"),
        huge_page_size_bytes: observed_kib(&values, "Hugepagesize"),
    }
}

fn read_trimmed(path: &Path) -> io::Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_owned())
}

fn inspect_hwmon(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> Vec<TemperatureSensorV1> {
    let entries = match fs::read_dir(root) {
        Ok(v) => v,
        Err(e) => {
            diagnostics.push(Diagnostic::warning(
                "machine.hwmon.unavailable",
                "machine_hwmon",
                format!("Hardware monitoring is unavailable: {}.", e.kind()),
            ));
            return vec![];
        }
    };
    let mut output = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        let name = read_trimmed(&dir.join("name"))
            .unwrap_or_else(|_| entry.file_name().to_string_lossy().into_owned());
        let Ok(files) = fs::read_dir(&dir) else {
            continue;
        };
        for file in files.flatten() {
            let filename = file.file_name().to_string_lossy().into_owned();
            let Some(index) = filename
                .strip_prefix("temp")
                .and_then(|s| s.strip_suffix("_input"))
            else {
                continue;
            };
            let value = match read_trimmed(&file.path()).and_then(|s| {
                s.parse::<i64>()
                    .map_err(|_| io::ErrorKind::InvalidData.into())
            }) {
                Ok(value) => Observation::Observed {
                    value,
                    source: "hwmon".into(),
                },
                Err(e) => Observation::Unavailable {
                    reason: e.kind().to_string(),
                },
            };
            output.push(TemperatureSensorV1 {
                name: name.clone(),
                label: read_trimmed(&dir.join(format!("temp{index}_label"))).ok(),
                temperature_millicelsius: value,
            });
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_meminfo_units() {
        let mut values = std::collections::BTreeMap::new();
        values.insert("MemTotal".into(), 1024);
        assert_eq!(
            observed_kib(&values, "MemTotal"),
            Observation::Observed {
                value: 1_048_576,
                source: "proc_meminfo".into()
            }
        );
    }

    #[test]
    fn inspects_synthetic_proc_and_hwmon() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rigos-machine-{unique}"));
        let proc_root = root.join("proc");
        let hwmon = root.join("sys/class/hwmon/hwmon0");
        fs::create_dir_all(&proc_root).unwrap();
        fs::create_dir_all(&hwmon).unwrap();
        fs::write(
            proc_root.join("cpuinfo"),
            "processor: 0\nmodel name: Test CPU\nflags: aes sse2\n",
        )
        .unwrap();
        fs::write(
            proc_root.join("meminfo"),
            "MemTotal: 1024 kB\nMemAvailable: 512 kB\nHugePages_Total: 4\nHugePages_Free: 2\nHugepagesize: 2048 kB\n",
        )
        .unwrap();
        fs::write(hwmon.join("name"), "k10temp\n").unwrap();
        fs::write(hwmon.join("temp1_input"), "72500\n").unwrap();
        let result = inspect(&MachineContext {
            proc_root,
            sys_root: root.join("sys"),
        });
        let _ = fs::remove_dir_all(root);
        let snapshot = result.value.unwrap();
        assert_eq!(snapshot.cpu.logical_processors, 1);
        assert_eq!(snapshot.temperatures.len(), 1);
        assert!(result.diagnostics.is_empty());
    }
}
