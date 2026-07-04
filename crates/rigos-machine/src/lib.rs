#![forbid(unsafe_code)]

use rigos_core::{Diagnostic, InspectionResult, Observation};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

pub const MACHINE_SCHEMA: &str = "rigos.machine-snapshot/v1";

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
    pub vendor_ids: Vec<String>,
    pub families: Vec<u32>,
    pub models: Vec<u32>,
    pub steppings: Vec<u32>,
    pub physical_cores: Observation<u32>,
    pub threads_per_core: Observation<u32>,
    pub flags: Vec<String>,
    pub caches: Vec<CacheSnapshotV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CacheSnapshotV1 {
    pub level: u32,
    pub cache_type: String,
    pub size_bytes: Observation<u64>,
    pub coherency_line_size_bytes: Observation<u64>,
    pub shared_cpu_list: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PressureWindowV1 {
    pub avg10: f64,
    pub avg60: f64,
    pub avg300: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemoryPressureV1 {
    pub some: Option<PressureWindowV1>,
    pub full: Option<PressureWindowV1>,
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
    pub memory_pressure: Observation<MemoryPressureV1>,
    pub temperatures: Vec<TemperatureSensorV1>,
}

pub fn inspect(ctx: &MachineContext) -> InspectionResult<MachineSnapshotV1> {
    let mut diagnostics = Vec::new();
    let cpu = inspect_cpu(
        &ctx.proc_root.join("cpuinfo"),
        &ctx.sys_root.join("devices/system/cpu"),
        &mut diagnostics,
    );
    let memory = inspect_memory(&ctx.proc_root.join("meminfo"), &mut diagnostics);
    let memory_pressure = inspect_memory_pressure(&ctx.proc_root.join("pressure/memory"));
    let mut temperatures = inspect_hwmon(&ctx.sys_root.join("class/hwmon"), &mut diagnostics);
    temperatures.sort_by(|a, b| a.name.cmp(&b.name).then(a.label.cmp(&b.label)));
    InspectionResult::success(
        MachineSnapshotV1 {
            architecture: std::env::consts::ARCH.into(),
            cpu,
            memory,
            memory_pressure,
            temperatures,
        },
        diagnostics,
    )
}

fn inspect_cpu(
    path: &Path,
    sys_cpu_root: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> CpuSnapshotV1 {
    let Ok(text) = fs::read_to_string(path) else {
        diagnostics.push(Diagnostic::error(
            "machine.cpu.unavailable",
            "machine_cpu",
            "CPU metadata is unavailable.",
        ));
        return CpuSnapshotV1 {
            logical_processors: 0,
            model_names: vec![],
            vendor_ids: vec![],
            families: vec![],
            models: vec![],
            steppings: vec![],
            physical_cores: Observation::Unavailable {
                reason: "cpuinfo_unavailable".into(),
            },
            threads_per_core: Observation::Unavailable {
                reason: "cpuinfo_unavailable".into(),
            },
            flags: vec![],
            caches: vec![],
        };
    };
    let mut processors = 0u32;
    let mut models = BTreeSet::new();
    let mut vendors = BTreeSet::new();
    let mut families = BTreeSet::new();
    let mut model_numbers = BTreeSet::new();
    let mut steppings = BTreeSet::new();
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
            "vendor_id" => {
                vendors.insert(value.trim().to_owned());
            }
            "cpu family" => {
                if let Ok(value) = value.trim().parse() {
                    families.insert(value);
                }
            }
            "model" => {
                if let Ok(value) = value.trim().parse() {
                    model_numbers.insert(value);
                }
            }
            "stepping" => {
                if let Ok(value) = value.trim().parse() {
                    steppings.insert(value);
                }
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
    let (physical_cores, threads_per_core) = inspect_topology(sys_cpu_root, processors);
    CpuSnapshotV1 {
        logical_processors: processors,
        model_names: models.into_iter().collect(),
        vendor_ids: vendors.into_iter().collect(),
        families: families.into_iter().collect(),
        models: model_numbers.into_iter().collect(),
        steppings: steppings.into_iter().collect(),
        physical_cores,
        threads_per_core,
        flags: flags.into_iter().collect(),
        caches: inspect_caches(&sys_cpu_root.join("cpu0/cache")),
    }
}

fn inspect_topology(root: &Path, logical: u32) -> (Observation<u32>, Observation<u32>) {
    let Ok(entries) = fs::read_dir(root) else {
        return unavailable_topology("sysfs_unavailable");
    };
    let mut cores = BTreeSet::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name
            .strip_prefix("cpu")
            .and_then(|v| v.parse::<u32>().ok())
            .is_none()
        {
            continue;
        }
        let topology = entry.path().join("topology");
        let Some(core) = read_trimmed(&topology.join("core_id"))
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
        else {
            continue;
        };
        let package = read_trimmed(&topology.join("physical_package_id"))
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        cores.insert((package, core));
    }
    if cores.is_empty() {
        return unavailable_topology("topology_unavailable");
    }
    let physical = cores.len() as u32;
    (
        Observation::Observed {
            value: physical,
            source: "sysfs_topology".into(),
        },
        Observation::Observed {
            value: logical.checked_div(physical).unwrap_or(0),
            source: "sysfs_topology".into(),
        },
    )
}

fn unavailable_topology(reason: &str) -> (Observation<u32>, Observation<u32>) {
    (
        Observation::Unavailable {
            reason: reason.into(),
        },
        Observation::Unavailable {
            reason: reason.into(),
        },
    )
}

fn parse_cache_size(value: &str) -> Option<u64> {
    let value = value.trim();
    let (digits, multiplier) = match value.as_bytes().last().copied() {
        Some(b'K' | b'k') => (&value[..value.len() - 1], 1024),
        Some(b'M' | b'm') => (&value[..value.len() - 1], 1024 * 1024),
        Some(b'G' | b'g') => (&value[..value.len() - 1], 1024 * 1024 * 1024),
        _ => (value, 1),
    };
    digits.parse::<u64>().ok()?.checked_mul(multiplier)
}

fn inspect_caches(root: &Path) -> Vec<CacheSnapshotV1> {
    let Ok(entries) = fs::read_dir(root) else {
        return vec![];
    };
    let mut caches = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("index") {
            continue;
        }
        let path = entry.path();
        let Some(level) = read_trimmed(&path.join("level"))
            .ok()
            .and_then(|v| v.parse().ok())
        else {
            continue;
        };
        let size_bytes = read_trimmed(&path.join("size"))
            .ok()
            .and_then(|v| parse_cache_size(&v))
            .map(|value| Observation::Observed {
                value,
                source: "sysfs_cache".into(),
            })
            .unwrap_or(Observation::Unavailable {
                reason: "cache_size_unavailable".into(),
            });
        let line = read_trimmed(&path.join("coherency_line_size"))
            .ok()
            .and_then(|v| v.parse().ok())
            .map(|value| Observation::Observed {
                value,
                source: "sysfs_cache".into(),
            })
            .unwrap_or(Observation::Unavailable {
                reason: "cache_line_unavailable".into(),
            });
        caches.push(CacheSnapshotV1 {
            level,
            cache_type: read_trimmed(&path.join("type")).unwrap_or_else(|_| "unknown".into()),
            size_bytes,
            coherency_line_size_bytes: line,
            shared_cpu_list: read_trimmed(&path.join("shared_cpu_list")).ok(),
        });
    }
    caches.sort_by(|a, b| (a.level, &a.cache_type).cmp(&(b.level, &b.cache_type)));
    caches
}

fn inspect_memory_pressure(path: &Path) -> Observation<MemoryPressureV1> {
    let Ok(text) = fs::read_to_string(path) else {
        return Observation::Unavailable {
            reason: "psi_unavailable".into(),
        };
    };
    let parse = |kind: &str| -> Option<PressureWindowV1> {
        let line = text.lines().find(|line| line.starts_with(kind))?;
        let mut values = std::collections::BTreeMap::new();
        for item in line.split_whitespace().skip(1) {
            let (key, value) = item.split_once('=')?;
            if key != "total" {
                values.insert(key, value.parse::<f64>().ok()?);
            }
        }
        let result = PressureWindowV1 {
            avg10: *values.get("avg10")?,
            avg60: *values.get("avg60")?,
            avg300: *values.get("avg300")?,
        };
        (result.avg10.is_finite() && result.avg60.is_finite() && result.avg300.is_finite())
            .then_some(result)
    };
    let value = MemoryPressureV1 {
        some: parse("some"),
        full: parse("full"),
    };
    if value.some.is_none() && value.full.is_none() {
        Observation::Unavailable {
            reason: "psi_malformed".into(),
        }
    } else {
        Observation::Observed {
            value,
            source: "proc_pressure".into(),
        }
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
        let topology = root.join("sys/devices/system/cpu/cpu0/topology");
        let cache = root.join("sys/devices/system/cpu/cpu0/cache/index0");
        fs::create_dir_all(&proc_root).unwrap();
        fs::create_dir_all(&hwmon).unwrap();
        fs::create_dir_all(&topology).unwrap();
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir_all(proc_root.join("pressure")).unwrap();
        fs::write(
            proc_root.join("cpuinfo"),
            "processor: 0\nvendor_id: AuthenticAMD\ncpu family: 16\nmodel: 4\nmodel name: Test CPU\nstepping: 3\nflags: aes sse2\n",
        )
        .unwrap();
        fs::write(
            proc_root.join("meminfo"),
            "MemTotal: 1024 kB\nMemAvailable: 512 kB\nHugePages_Total: 4\nHugePages_Free: 2\nHugepagesize: 2048 kB\n",
        )
        .unwrap();
        fs::write(hwmon.join("name"), "k10temp\n").unwrap();
        fs::write(hwmon.join("temp1_input"), "72500\n").unwrap();
        fs::write(topology.join("core_id"), "0\n").unwrap();
        fs::write(topology.join("physical_package_id"), "0\n").unwrap();
        fs::write(cache.join("level"), "1\n").unwrap();
        fs::write(cache.join("type"), "Data\n").unwrap();
        fs::write(cache.join("size"), "64K\n").unwrap();
        fs::write(cache.join("coherency_line_size"), "64\n").unwrap();
        fs::write(cache.join("shared_cpu_list"), "0\n").unwrap();
        fs::write(proc_root.join("pressure/memory"), "some avg10=0.10 avg60=0.20 avg300=0.30 total=10\nfull avg10=0.00 avg60=0.01 avg300=0.02 total=1\n").unwrap();
        let result = inspect(&MachineContext {
            proc_root,
            sys_root: root.join("sys"),
        });
        let _ = fs::remove_dir_all(root);
        let snapshot = result.value.unwrap();
        assert_eq!(snapshot.cpu.logical_processors, 1);
        assert_eq!(snapshot.cpu.families, vec![16]);
        assert_eq!(snapshot.cpu.models, vec![4]);
        assert_eq!(snapshot.cpu.steppings, vec![3]);
        assert_eq!(snapshot.cpu.caches.len(), 1);
        assert!(matches!(
            snapshot.cpu.physical_cores,
            Observation::Observed { value: 1, .. }
        ));
        assert!(matches!(
            snapshot.memory_pressure,
            Observation::Observed { .. }
        ));
        assert_eq!(snapshot.temperatures.len(), 1);
        assert!(result.diagnostics.is_empty());
    }
}
