use std::{
    env,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

fn main() {
    if env::args().any(|arg| arg == "--descendant") {
        thread::sleep(Duration::from_secs(30));
        return;
    }
    let executable = env::current_exe().expect("current executable");
    #[allow(clippy::zombie_processes)] // Deliberate descendant used to prove process-group cleanup.
    let _child = Command::new(executable)
        .arg("--descendant")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn descendant");
    thread::sleep(Duration::from_secs(30));
}
