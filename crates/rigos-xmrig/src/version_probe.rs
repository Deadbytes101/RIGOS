use rigos_core::{Diagnostic, Observation};
use std::path::Path;

#[cfg(unix)]
mod platform {
    use super::*;
    use nix::{
        sys::signal::{Signal, killpg},
        unistd::Pid,
    };
    use std::{
        fs,
        io::Read,
        os::unix::{fs::MetadataExt, process::CommandExt},
        process::{Child, Command, Stdio},
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    const TIMEOUT: Duration = Duration::from_secs(2);
    const TERM_GRACE: Duration = Duration::from_millis(250);
    const REAP_LIMIT: Duration = Duration::from_secs(1);
    const OUTPUT_LIMIT: usize = 64 * 1024;

    pub(super) struct ProbeJobHandle {
        child: Child,
        process_group: Pid,
    }

    impl ProbeJobHandle {
        fn terminate_and_reap(&mut self) {
            let _ = killpg(self.process_group, Signal::SIGTERM);
            if !wait_until(&mut self.child, TERM_GRACE) {
                let _ = killpg(self.process_group, Signal::SIGKILL);
                let _ = wait_until(&mut self.child, REAP_LIMIT);
            }
            let _ = self.child.try_wait();
        }
    }

    fn wait_until(child: &mut Child, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return true,
                Err(_) => return true,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Ok(None) => return false,
            }
        }
    }

    fn validate(path: &Path) -> Result<std::path::PathBuf, &'static str> {
        let path = fs::canonicalize(path).map_err(|_| "not_found")?;
        let metadata = fs::metadata(&path).map_err(|_| "metadata_unavailable")?;
        if !metadata.is_file() {
            return Err("not_regular_file");
        }
        if metadata.mode() & 0o6000 != 0 {
            return Err("privileged_executable");
        }
        if metadata.mode() & 0o111 == 0 {
            return Err("not_executable");
        }
        let mut file = fs::File::open(&path).map_err(|_| "unreadable")?;
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)
            .map_err(|_| "invalid_executable")?;
        if magic != *b"\x7fELF" {
            return Err("script_or_wrapper");
        }
        Ok(path)
    }

    fn capture(mut input: impl Read + Send + 'static) -> mpsc::Receiver<Vec<u8>> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut limited = (&mut input).take((OUTPUT_LIMIT + 1) as u64);
            let mut bytes = Vec::new();
            let _ = limited.read_to_end(&mut bytes);
            let _ = tx.send(bytes);
        });
        rx
    }

    pub(super) fn run(
        path: &Path,
        active_pid: Option<u32>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Observation<String> {
        let path = match validate(path) {
            Ok(v) => v,
            Err(reason) => {
                diagnostics.push(Diagnostic::warning(
                    "xmrig.version_probe.rejected",
                    "xmrig_version_probe",
                    "The XMRig executable is not safe to probe.",
                ));
                return Observation::Unavailable {
                    reason: reason.into(),
                };
            }
        };
        let mut command = Command::new(path);
        command
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .process_group(0);
        // SAFETY: the closure uses only async-signal-safe libc calls and does not allocate.
        unsafe {
            command.pre_exec(|| {
                if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                let cpu = libc::rlimit {
                    rlim_cur: 3,
                    rlim_max: 3,
                };
                let address_space = libc::rlimit {
                    rlim_cur: 256 * 1024 * 1024,
                    rlim_max: 256 * 1024 * 1024,
                };
                let open_files = libc::rlimit {
                    rlim_cur: 16,
                    rlim_max: 16,
                };
                if libc::setrlimit(libc::RLIMIT_CPU, &cpu) != 0
                    || libc::setrlimit(libc::RLIMIT_AS, &address_space) != 0
                    || libc::setrlimit(libc::RLIMIT_NOFILE, &open_files) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = match command.spawn() {
            Ok(v) => v,
            Err(_) => {
                return Observation::Unavailable {
                    reason: "probe_spawn_failed".into(),
                };
            }
        };
        if active_pid == Some(child.id()) {
            let _ = child.kill();
            let _ = child.wait();
            return Observation::Unavailable {
                reason: "process_identity_collision".into(),
            };
        }
        let stdout = capture(child.stdout.take().expect("piped stdout"));
        let stderr = capture(child.stderr.take().expect("piped stderr"));
        let group = Pid::from_raw(child.id() as i32);
        let mut job = ProbeJobHandle {
            child,
            process_group: group,
        };
        let deadline = Instant::now() + TIMEOUT;
        let status = loop {
            match job.child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                _ => break None,
            }
        };
        if status.is_none() {
            job.terminate_and_reap();
            diagnostics.push(Diagnostic::warning(
                "xmrig.version_probe.timeout",
                "xmrig_version_probe",
                "The isolated XMRig version probe exceeded its execution deadline.",
            ));
            return Observation::Unavailable {
                reason: "probe_timeout".into(),
            };
        }
        let out = stdout.recv_timeout(REAP_LIMIT).unwrap_or_default();
        let err = stderr.recv_timeout(REAP_LIMIT).unwrap_or_default();
        if out.len().saturating_add(err.len()) > OUTPUT_LIMIT {
            diagnostics.push(Diagnostic::warning(
                "xmrig.version_probe.output_truncated",
                "xmrig_version_probe",
                "The XMRig version probe exceeded its output limit.",
            ));
            return Observation::Unavailable {
                reason: "probe_output_limit".into(),
            };
        }
        if !status.is_some_and(|v| v.success()) {
            return Observation::Unavailable {
                reason: "probe_nonzero_exit".into(),
            };
        }
        let text = String::from_utf8_lossy(&out);
        let version = text.split_whitespace().find(|word| {
            word.chars().next().is_some_and(|v| v.is_ascii_digit()) && word.contains('.')
        });
        version
            .map(|value| Observation::Observed {
                value: value
                    .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.')
                    .into(),
                source: "executable_probe".into(),
            })
            .unwrap_or_else(|| Observation::Unavailable {
                reason: "unrecognized_version_output".into(),
            })
    }
}

pub fn probe(
    path: &Path,
    active_pid: Option<u32>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Observation<String> {
    #[cfg(unix)]
    {
        platform::run(path, active_pid, diagnostics)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, active_pid);
        diagnostics.push(Diagnostic::warning(
            "xmrig.version_probe.unsupported_platform",
            "xmrig_version_probe",
            "Executable probing is supported only on Linux.",
        ));
        Observation::Unsupported {
            reason: "non_linux_platform".into(),
        }
    }
}
