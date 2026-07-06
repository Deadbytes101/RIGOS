#![forbid(unsafe_code)]

use chrono::{SecondsFormat, Utc};
use rigos_schema::{
    HugePageAuthorityStatusV1, HugePageAuthorityV1, PERFORMANCE_STATUS_SCHEMA, PerformanceStatusV1,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    thread,
    time::Duration,
};
use uuid::Uuid;

pub const RX0_TARGET_PAGES: u64 = 1280;
pub const EXPECTED_HUGE_PAGE_SIZE_BYTES: u64 = 2 * 1024 * 1024;
pub const MEMORY_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;
const POLL_ATTEMPTS: usize = 50;
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
pub enum AuthorityError {
    #[error("configuration is unreadable: {0}")]
    Configuration(String),
    #[error("machine truth is unavailable: {0}")]
    Machine(String),
    #[error("miner control failed: {0}")]
    Miner(String),
    #[error("performance status publication failed: {0}")]
    Status(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerformancePolicy {
    pub requested: bool,
    pub algorithm: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySnapshot {
    pub available_bytes: u64,
    pub huge_page_size_bytes: u64,
    pub huge_pages_total: u64,
    pub nr_hugepages: Option<u64>,
}

pub trait HugePageKernel {
    fn snapshot(&mut self) -> Result<MemorySnapshot, AuthorityError>;
    fn write_nr_hugepages(&mut self, pages: u64) -> Result<(), AuthorityError>;
    fn wait(&mut self, duration: Duration);
}

pub trait MinerControl {
    fn is_active(&mut self) -> Result<bool, AuthorityError>;
    fn stop(&mut self) -> Result<(), AuthorityError>;
    fn start_no_block(&mut self) -> Result<(), AuthorityError>;
}

pub struct ProcKernel {
    pub meminfo: PathBuf,
    pub nr_hugepages: PathBuf,
}

impl Default for ProcKernel {
    fn default() -> Self {
        Self {
            meminfo: "/proc/meminfo".into(),
            nr_hugepages: "/proc/sys/vm/nr_hugepages".into(),
        }
    }
}

impl HugePageKernel for ProcKernel {
    fn snapshot(&mut self) -> Result<MemorySnapshot, AuthorityError> {
        let text = fs::read_to_string(&self.meminfo)
            .map_err(|error| AuthorityError::Machine(format!("meminfo: {error}")))?;
        let nr_hugepages = match fs::read_to_string(&self.nr_hugepages) {
            Ok(value) => Some(
                value
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| AuthorityError::Machine("nr_hugepages is malformed".into()))?,
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(AuthorityError::Machine(format!(
                    "nr_hugepages read: {error}"
                )));
            }
        };
        parse_meminfo(&text, nr_hugepages)
    }

    fn write_nr_hugepages(&mut self, pages: u64) -> Result<(), AuthorityError> {
        fs::write(&self.nr_hugepages, format!("{pages}\n"))
            .map_err(|error| AuthorityError::Machine(format!("nr_hugepages write: {error}")))
    }

    fn wait(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
}

pub struct SystemdMinerControl {
    pub executable: PathBuf,
}

impl Default for SystemdMinerControl {
    fn default() -> Self {
        Self {
            executable: "/usr/bin/systemctl".into(),
        }
    }
}

impl SystemdMinerControl {
    fn status(&self, args: &[&str]) -> Result<ExitStatus, AuthorityError> {
        Command::new(&self.executable)
            .args(args)
            .status()
            .map_err(|error| AuthorityError::Miner(error.to_string()))
    }
}

impl MinerControl for SystemdMinerControl {
    fn is_active(&mut self) -> Result<bool, AuthorityError> {
        let status = self.status(&["is-active", "--quiet", "rigos-miner.service"])?;
        match status.code() {
            Some(0) => Ok(true),
            Some(3) => Ok(false),
            code => Err(AuthorityError::Miner(format!(
                "is-active returned {code:?}"
            ))),
        }
    }

    fn stop(&mut self) -> Result<(), AuthorityError> {
        let status = self.status(&["stop", "rigos-miner.service"])?;
        if status.success() && !self.is_active()? {
            Ok(())
        } else {
            Err(AuthorityError::Miner("miner did not stop".into()))
        }
    }

    fn start_no_block(&mut self) -> Result<(), AuthorityError> {
        let status = self.status(&["--no-block", "start", "rigos-miner.service"])?;
        if status.success() {
            Ok(())
        } else {
            Err(AuthorityError::Miner(format!(
                "start returned {:?}",
                status.code()
            )))
        }
    }
}

pub fn parse_meminfo(
    text: &str,
    nr_hugepages: Option<u64>,
) -> Result<MemorySnapshot, AuthorityError> {
    let mut values = BTreeMap::new();
    for line in text.lines() {
        let Some((name, raw)) = line.split_once(':') else {
            continue;
        };
        let mut fields = raw.split_whitespace();
        let Some(value) = fields.next().and_then(|value| value.parse::<u64>().ok()) else {
            continue;
        };
        let multiplier = match fields.next() {
            Some("kB") => 1024,
            None => 1,
            _ => continue,
        };
        if let Some(bytes) = value.checked_mul(multiplier) {
            values.insert(name, bytes);
        }
    }
    let required = |name: &str| {
        values
            .get(name)
            .copied()
            .ok_or_else(|| AuthorityError::Machine(format!("meminfo field {name} is unavailable")))
    };
    let snapshot = MemorySnapshot {
        available_bytes: required("MemAvailable")?,
        huge_page_size_bytes: required("Hugepagesize")?,
        huge_pages_total: required("HugePages_Total")?,
        nr_hugepages,
    };
    if let Some(nr) = snapshot.nr_hugepages {
        if nr != snapshot.huge_pages_total {
            return Err(AuthorityError::Machine(
                "nr_hugepages disagrees with HugePages_Total".into(),
            ));
        }
    }
    Ok(snapshot)
}

pub fn parse_policy(policy: &[u8], xmrig: &[u8]) -> Result<PerformancePolicy, AuthorityError> {
    let policy: Value = serde_json::from_slice(policy)
        .map_err(|error| AuthorityError::Configuration(format!("policy JSON: {error}")))?;
    if policy.get("schema").and_then(Value::as_str) != Some("rigos.policy/v1") {
        return Err(AuthorityError::Configuration(
            "policy schema is not rigos.policy/v1".into(),
        ));
    }
    if !matches!(
        policy.get("miner_start_mode").and_then(Value::as_str),
        Some("manual" | "on_boot")
    ) {
        return Err(AuthorityError::Configuration(
            "miner_start_mode is invalid".into(),
        ));
    }
    let xmrig: Value = serde_json::from_slice(xmrig)
        .map_err(|error| AuthorityError::Configuration(format!("XMRig JSON: {error}")))?;
    let requested = xmrig
        .pointer("/cpu/huge-pages")
        .and_then(Value::as_bool)
        .ok_or_else(|| AuthorityError::Configuration("cpu.huge-pages is missing".into()))?;
    let pools = xmrig
        .get("pools")
        .and_then(Value::as_array)
        .filter(|pools| !pools.is_empty())
        .ok_or_else(|| AuthorityError::Configuration("pools are missing".into()))?;
    let algorithms: BTreeSet<_> = pools
        .iter()
        .map(|pool| {
            pool.get("algo")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| AuthorityError::Configuration("pool algorithm is missing".into()))
        })
        .collect::<Result<_, _>>()?;
    if algorithms.len() != 1 {
        return Err(AuthorityError::Configuration(
            "pool algorithms are ambiguous".into(),
        ));
    }
    Ok(PerformancePolicy {
        requested,
        algorithm: algorithms.into_iter().next().unwrap_or_default(),
    })
}

pub fn apply_huge_page_policy<K: HugePageKernel>(
    policy: &PerformancePolicy,
    kernel: &mut K,
) -> Result<HugePageAuthorityV1, AuthorityError> {
    let before = kernel.snapshot()?;
    if before.huge_page_size_bytes != EXPECTED_HUGE_PAGE_SIZE_BYTES {
        return Ok(outcome(
            policy.requested,
            0,
            before.huge_pages_total,
            &before,
            HugePageAuthorityStatusV1::DegradedUnsupported,
            Some("unsupported_huge_page_size"),
        ));
    }
    let Some(current) = before.nr_hugepages else {
        return Ok(outcome(
            policy.requested,
            0,
            before.huge_pages_total,
            &before,
            HugePageAuthorityStatusV1::DegradedUnsupported,
            Some("nr_hugepages_unavailable"),
        ));
    };
    if !policy.requested {
        let actual = write_and_read_back(kernel, current, 0)?;
        let status = if actual == 0 {
            HugePageAuthorityStatusV1::Disabled
        } else {
            HugePageAuthorityStatusV1::DegradedReleaseIncomplete
        };
        let reason = (actual != 0).then_some("kernel_release_incomplete");
        return Ok(outcome(false, 0, actual, &before, status, reason));
    }
    if policy.algorithm != "rx/0" {
        return Ok(outcome(
            true,
            0,
            current,
            &before,
            HugePageAuthorityStatusV1::DegradedUnsupported,
            Some("unsupported_algorithm"),
        ));
    }
    if current >= RX0_TARGET_PAGES {
        return Ok(outcome(
            true,
            RX0_TARGET_PAGES,
            current,
            &before,
            HugePageAuthorityStatusV1::Ready,
            None,
        ));
    }
    let usable = before.available_bytes.saturating_sub(MEMORY_RESERVE_BYTES);
    let safe_pages = usable / before.huge_page_size_bytes;
    let attempted = RX0_TARGET_PAGES.min(safe_pages);
    let actual = write_and_read_back(kernel, current, attempted)?;
    let (status, reason) = if attempted < RX0_TARGET_PAGES {
        (
            HugePageAuthorityStatusV1::DegradedInsufficientMemory,
            Some(if actual < attempted {
                "safe_attempt_partially_allocated"
            } else {
                "memory_reserve_limited_attempt"
            }),
        )
    } else if actual >= RX0_TARGET_PAGES {
        (HugePageAuthorityStatusV1::Ready, None)
    } else if actual == 0 {
        (
            HugePageAuthorityStatusV1::DegradedUnavailable,
            Some("kernel_allocation_unavailable"),
        )
    } else {
        (
            HugePageAuthorityStatusV1::DegradedPartialAllocation,
            Some("kernel_partial_allocation"),
        )
    };
    Ok(outcome(true, attempted, actual, &before, status, reason))
}

fn write_and_read_back<K: HugePageKernel>(
    kernel: &mut K,
    current: u64,
    requested: u64,
) -> Result<u64, AuthorityError> {
    if current != requested {
        kernel.write_nr_hugepages(requested)?;
    }
    let mut actual = current;
    for attempt in 0..POLL_ATTEMPTS {
        let snapshot = kernel.snapshot()?;
        actual = snapshot.nr_hugepages.ok_or_else(|| {
            AuthorityError::Machine("nr_hugepages disappeared after write".into())
        })?;
        if actual == requested {
            break;
        }
        if attempt + 1 < POLL_ATTEMPTS {
            kernel.wait(POLL_INTERVAL);
        }
    }
    Ok(actual)
}

fn outcome(
    requested: bool,
    attempted_pages: u64,
    actual_pages: u64,
    before: &MemorySnapshot,
    status: HugePageAuthorityStatusV1,
    reason: Option<&str>,
) -> HugePageAuthorityV1 {
    let target_pages = if requested { RX0_TARGET_PAGES } else { 0 };
    let allocation_percent_of_target = if target_pages == 0 {
        if actual_pages == 0 { 100.0 } else { 0.0 }
    } else {
        actual_pages as f64 * 100.0 / target_pages as f64
    };
    HugePageAuthorityV1 {
        requested,
        target_pages,
        attempted_pages,
        actual_pages,
        huge_page_size_bytes: before.huge_page_size_bytes,
        memory_available_before_bytes: before.available_bytes,
        reserve_bytes: MEMORY_RESERVE_BYTES,
        allocation_percent_of_target,
        status,
        reason: reason.map(str::to_owned),
    }
}

pub struct AuthorityPaths {
    pub state_root: PathBuf,
    pub status: PathBuf,
    pub boot_id: PathBuf,
}

impl Default for AuthorityPaths {
    fn default() -> Self {
        Self {
            state_root: "/var/lib/rigos".into(),
            status: "/run/rigos/performance-status.json".into(),
            boot_id: "/proc/sys/kernel/random/boot_id".into(),
        }
    }
}

pub fn execute<K: HugePageKernel, C: MinerControl>(
    paths: &AuthorityPaths,
    kernel: &mut K,
    miner: &mut C,
) -> Result<PerformanceStatusV1, AuthorityError> {
    match fs::remove_file(&paths.status) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(AuthorityError::Status(error.to_string())),
    }
    let current = paths.state_root.join("current");
    let revision_target = fs::read_link(&current)
        .map_err(|error| AuthorityError::Configuration(format!("current revision: {error}")))?;
    let revision = revision_target
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AuthorityError::Configuration("current revision is invalid".into()))?
        .to_owned();
    let policy_bytes = fs::read(current.join("policy.json"))
        .map_err(|error| AuthorityError::Configuration(format!("policy: {error}")))?;
    let xmrig_bytes = fs::read(current.join("xmrig.json"))
        .map_err(|error| AuthorityError::Configuration(format!("XMRig config: {error}")))?;
    let policy = parse_policy(&policy_bytes, &xmrig_bytes)?;
    let (huge_pages, restore_miner) = apply_with_miner(&policy, kernel, miner)?;
    let status = PerformanceStatusV1 {
        schema: PERFORMANCE_STATUS_SCHEMA.into(),
        boot_id: read_trimmed(&paths.boot_id, "boot ID")?,
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        config_revision: revision,
        algorithm: Some(policy.algorithm),
        huge_pages,
    };
    write_status_verified(&paths.status, &status)?;
    restore_miner_if_needed(miner, restore_miner)?;
    Ok(status)
}

pub fn apply_with_miner<K: HugePageKernel, C: MinerControl>(
    policy: &PerformancePolicy,
    kernel: &mut K,
    miner: &mut C,
) -> Result<(HugePageAuthorityV1, bool), AuthorityError> {
    let active = miner.is_active()?;
    let will_mutate = !policy.requested || policy.algorithm == "rx/0";
    if active && will_mutate {
        miner.stop()?;
    }
    let result = apply_huge_page_policy(policy, kernel)?;
    Ok((result, active && will_mutate))
}

pub fn restore_miner_if_needed<C: MinerControl>(
    miner: &mut C,
    restore: bool,
) -> Result<(), AuthorityError> {
    if restore {
        miner.start_no_block()?;
    }
    Ok(())
}

fn read_trimmed(path: &Path, name: &str) -> Result<String, AuthorityError> {
    let value = fs::read_to_string(path)
        .map_err(|error| AuthorityError::Machine(format!("{name}: {error}")))?;
    let value = value.trim();
    if value.is_empty() {
        Err(AuthorityError::Machine(format!("{name} is empty")))
    } else {
        Ok(value.to_owned())
    }
}

pub fn write_status_verified(
    path: &Path,
    status: &PerformanceStatusV1,
) -> Result<(), AuthorityError> {
    let parent = path
        .parent()
        .ok_or_else(|| AuthorityError::Status("status path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(|error| AuthorityError::Status(error.to_string()))?;
    let temporary = parent.join(format!(".performance-status-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        let mut file = options
            .open(&temporary)
            .map_err(|error| AuthorityError::Status(error.to_string()))?;
        serde_json::to_writer(&mut file, status)
            .map_err(|error| AuthorityError::Status(error.to_string()))?;
        file.write_all(b"\n")
            .and_then(|_| file.sync_all())
            .map_err(|error| AuthorityError::Status(error.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o644))
                .map_err(|error| AuthorityError::Status(error.to_string()))?;
        }
        fs::rename(&temporary, path).map_err(|error| AuthorityError::Status(error.to_string()))?;
        #[cfg(unix)]
        fs::File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|error| AuthorityError::Status(error.to_string()))?;
        let observed: PerformanceStatusV1 = serde_json::from_slice(
            &fs::read(path).map_err(|error| AuthorityError::Status(error.to_string()))?,
        )
        .map_err(|error| AuthorityError::Status(error.to_string()))?;
        if &observed != status {
            return Err(AuthorityError::Status("status read-back mismatch".into()));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct FakeKernel {
        snapshots: VecDeque<MemorySnapshot>,
        last: MemorySnapshot,
        writes: Vec<u64>,
        waits: usize,
    }

    struct FakeMiner {
        active: bool,
        calls: Vec<&'static str>,
    }

    struct FailingKernel {
        before: MemorySnapshot,
    }

    impl HugePageKernel for FailingKernel {
        fn snapshot(&mut self) -> Result<MemorySnapshot, AuthorityError> {
            Ok(self.before.clone())
        }

        fn write_nr_hugepages(&mut self, _pages: u64) -> Result<(), AuthorityError> {
            Err(AuthorityError::Machine("synthetic write failure".into()))
        }

        fn wait(&mut self, _duration: Duration) {}
    }

    impl MinerControl for FakeMiner {
        fn is_active(&mut self) -> Result<bool, AuthorityError> {
            self.calls.push("is_active");
            Ok(self.active)
        }

        fn stop(&mut self) -> Result<(), AuthorityError> {
            self.calls.push("stop");
            self.active = false;
            Ok(())
        }

        fn start_no_block(&mut self) -> Result<(), AuthorityError> {
            self.calls.push("start");
            self.active = true;
            Ok(())
        }
    }

    impl FakeKernel {
        fn new(snapshots: Vec<MemorySnapshot>) -> Self {
            let last = snapshots.last().cloned().unwrap();
            Self {
                snapshots: snapshots.into(),
                last,
                writes: vec![],
                waits: 0,
            }
        }
    }

    impl HugePageKernel for FakeKernel {
        fn snapshot(&mut self) -> Result<MemorySnapshot, AuthorityError> {
            if let Some(value) = self.snapshots.pop_front() {
                self.last = value;
            }
            Ok(self.last.clone())
        }

        fn write_nr_hugepages(&mut self, pages: u64) -> Result<(), AuthorityError> {
            self.writes.push(pages);
            Ok(())
        }

        fn wait(&mut self, _duration: Duration) {
            self.waits += 1;
        }
    }

    fn memory(available: u64, pages: u64) -> MemorySnapshot {
        MemorySnapshot {
            available_bytes: available,
            huge_page_size_bytes: EXPECTED_HUGE_PAGE_SIZE_BYTES,
            huge_pages_total: pages,
            nr_hugepages: Some(pages),
        }
    }

    fn policy(requested: bool) -> PerformancePolicy {
        PerformancePolicy {
            requested,
            algorithm: "rx/0".into(),
        }
    }

    #[test]
    fn parses_meminfo_units_and_rejects_disagreement() {
        let text = "MemAvailable: 4096 kB\nHugePages_Total: 2\nHugepagesize: 2048 kB\n";
        let parsed = parse_meminfo(text, Some(2)).unwrap();
        assert_eq!(parsed.available_bytes, 4 * 1024 * 1024);
        assert_eq!(parsed.huge_page_size_bytes, EXPECTED_HUGE_PAGE_SIZE_BYTES);
        assert!(parse_meminfo(text, Some(1)).is_err());
    }

    #[test]
    fn full_allocation_is_ready() {
        let available = MEMORY_RESERVE_BYTES + RX0_TARGET_PAGES * EXPECTED_HUGE_PAGE_SIZE_BYTES;
        let mut kernel = FakeKernel::new(vec![
            memory(available, 0),
            memory(available, RX0_TARGET_PAGES),
        ]);
        let result = apply_huge_page_policy(&policy(true), &mut kernel).unwrap();
        assert_eq!(result.status, HugePageAuthorityStatusV1::Ready);
        assert_eq!(kernel.writes, vec![RX0_TARGET_PAGES]);
    }

    #[test]
    fn memory_gate_uses_safe_partial_target() {
        let safe = 704;
        let available = MEMORY_RESERVE_BYTES + safe * EXPECTED_HUGE_PAGE_SIZE_BYTES;
        let mut kernel = FakeKernel::new(vec![memory(available, 0), memory(available, 690)]);
        let result = apply_huge_page_policy(&policy(true), &mut kernel).unwrap();
        assert_eq!(
            result.status,
            HugePageAuthorityStatusV1::DegradedInsufficientMemory
        );
        assert_eq!(result.target_pages, RX0_TARGET_PAGES);
        assert_eq!(result.attempted_pages, safe);
        assert_eq!(result.actual_pages, 690);
        assert_eq!(kernel.writes, vec![safe]);
    }

    #[test]
    fn full_attempt_reports_partial_or_zero_truth() {
        let available = MEMORY_RESERVE_BYTES + RX0_TARGET_PAGES * EXPECTED_HUGE_PAGE_SIZE_BYTES;
        let mut partial = FakeKernel::new(vec![memory(available, 0), memory(available, 896)]);
        assert_eq!(
            apply_huge_page_policy(&policy(true), &mut partial)
                .unwrap()
                .status,
            HugePageAuthorityStatusV1::DegradedPartialAllocation
        );
        let mut zero = FakeKernel::new(vec![memory(available, 0), memory(available, 0)]);
        assert_eq!(
            apply_huge_page_policy(&policy(true), &mut zero)
                .unwrap()
                .status,
            HugePageAuthorityStatusV1::DegradedUnavailable
        );
        assert_eq!(zero.writes, vec![RX0_TARGET_PAGES]);
        assert_eq!(zero.waits, POLL_ATTEMPTS - 1);
    }

    #[test]
    fn disabled_releases_and_verifies() {
        let available = 4 * MEMORY_RESERVE_BYTES;
        let mut released = FakeKernel::new(vec![memory(available, 12), memory(available, 0)]);
        assert_eq!(
            apply_huge_page_policy(&policy(false), &mut released)
                .unwrap()
                .status,
            HugePageAuthorityStatusV1::Disabled
        );
        assert_eq!(released.writes, vec![0]);
        let mut incomplete = FakeKernel::new(vec![memory(available, 12), memory(available, 4)]);
        assert_eq!(
            apply_huge_page_policy(&policy(false), &mut incomplete)
                .unwrap()
                .status,
            HugePageAuthorityStatusV1::DegradedReleaseIncomplete
        );
    }

    #[test]
    fn unsupported_algorithm_and_page_size_do_not_write() {
        let mut kernel = FakeKernel::new(vec![memory(4 * MEMORY_RESERVE_BYTES, 0)]);
        let unsupported = PerformancePolicy {
            requested: true,
            algorithm: "other".into(),
        };
        assert_eq!(
            apply_huge_page_policy(&unsupported, &mut kernel)
                .unwrap()
                .status,
            HugePageAuthorityStatusV1::DegradedUnsupported
        );
        assert!(kernel.writes.is_empty());
        let mut snapshot = memory(4 * MEMORY_RESERVE_BYTES, 0);
        snapshot.huge_page_size_bytes = 1024 * 1024;
        let mut kernel = FakeKernel::new(vec![snapshot]);
        assert_eq!(
            apply_huge_page_policy(&policy(true), &mut kernel)
                .unwrap()
                .status,
            HugePageAuthorityStatusV1::DegradedUnsupported
        );
        assert!(kernel.writes.is_empty());
    }

    #[test]
    fn configuration_is_strict_and_algorithms_must_agree() {
        let policy = br#"{"schema":"rigos.policy/v1","miner_start_mode":"on_boot"}"#;
        let config = br#"{"cpu":{"huge-pages":true},"pools":[{"algo":"rx/0"}]}"#;
        assert_eq!(parse_policy(policy, config).unwrap().algorithm, "rx/0");
        let ambiguous = br#"{"cpu":{"huge-pages":true},"pools":[{"algo":"rx/0"},{"algo":"rx/1"}]}"#;
        assert!(parse_policy(policy, ambiguous).is_err());
        assert!(parse_policy(br#"{}"#, config).is_err());
    }

    #[test]
    fn miner_is_stopped_for_mutation_and_restored_only_after_success() {
        let available = MEMORY_RESERVE_BYTES + RX0_TARGET_PAGES * EXPECTED_HUGE_PAGE_SIZE_BYTES;
        let mut kernel = FakeKernel::new(vec![
            memory(available, 0),
            memory(available, RX0_TARGET_PAGES),
        ]);
        let mut miner = FakeMiner {
            active: true,
            calls: vec![],
        };
        let (_, restore) = apply_with_miner(&policy(true), &mut kernel, &mut miner).unwrap();
        assert!(restore);
        assert_eq!(miner.calls, vec!["is_active", "stop"]);
        restore_miner_if_needed(&mut miner, restore).unwrap();
        assert_eq!(miner.calls, vec!["is_active", "stop", "start"]);

        let mut kernel = FakeKernel::new(vec![memory(available, 0)]);
        let mut inactive = FakeMiner {
            active: false,
            calls: vec![],
        };
        let (_, restore) = apply_with_miner(&policy(true), &mut kernel, &mut inactive).unwrap();
        assert!(!restore);
        assert_eq!(inactive.calls, vec!["is_active"]);
    }

    #[test]
    fn atomic_status_round_trip_preserves_authoritative_truth() {
        let root = std::env::temp_dir().join(format!("rigos-performance-{}", Uuid::new_v4()));
        let path = root.join("performance-status.json");
        let status = PerformanceStatusV1 {
            schema: PERFORMANCE_STATUS_SCHEMA.into(),
            boot_id: "boot".into(),
            generated_at: "2026-07-06T00:00:00.000Z".into(),
            config_revision: "revision".into(),
            algorithm: Some("rx/0".into()),
            huge_pages: outcome(
                true,
                RX0_TARGET_PAGES,
                RX0_TARGET_PAGES,
                &memory(4 * MEMORY_RESERVE_BYTES, RX0_TARGET_PAGES),
                HugePageAuthorityStatusV1::Ready,
                None,
            ),
        };
        write_status_verified(&path, &status).unwrap();
        let observed: PerformanceStatusV1 =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(observed, status);
        assert!(fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with('.')
        }));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o644
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn hard_failure_leaves_previously_active_miner_stopped() {
        let available = MEMORY_RESERVE_BYTES + RX0_TARGET_PAGES * EXPECTED_HUGE_PAGE_SIZE_BYTES;
        let mut kernel = FailingKernel {
            before: memory(available, 0),
        };
        let mut miner = FakeMiner {
            active: true,
            calls: vec![],
        };
        assert!(apply_with_miner(&policy(true), &mut kernel, &mut miner).is_err());
        assert!(!miner.active);
        assert_eq!(miner.calls, vec!["is_active", "stop"]);
    }
}
