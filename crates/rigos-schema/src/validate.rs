use serde_json::Value;
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let expected = [
        (
            "machine.json",
            "machine.inspect",
            "dbyte.rigos.machine-snapshot/v1",
        ),
        (
            "miner.json",
            "miner.inspect",
            "dbyte.rigos.miner-snapshot/v1",
        ),
        ("doctor.json", "doctor", "dbyte.rigos.doctor-report/v1"),
    ];
    let Some(root) = env::args_os().nth(1) else {
        eprintln!("usage: validate-cli-output DIRECTORY");
        return ExitCode::from(2);
    };
    for (name, command, data_schema) in expected {
        let path = std::path::Path::new(&root).join(name);
        let Ok(text) = fs::read_to_string(&path) else {
            eprintln!("cannot read {}", path.display());
            return ExitCode::from(1);
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            eprintln!("invalid JSON in {}", path.display());
            return ExitCode::from(1);
        };
        if value.get("schema").and_then(Value::as_str) != Some("dbyte.rigos.cli-envelope/v1")
            || value.get("command").and_then(Value::as_str) != Some(command)
            || value.get("data_schema").and_then(Value::as_str) != Some(data_schema)
            || value.get("data").is_none()
        {
            eprintln!("contract mismatch in {}", path.display());
            return ExitCode::from(1);
        }
        for sentinel in ["Authorization: Bearer", "access-token", "SENTINEL_SECRET"] {
            if text.contains(sentinel) {
                eprintln!("secret sentinel in {}", path.display());
                return ExitCode::from(1);
            }
        }
    }
    ExitCode::SUCCESS
}
