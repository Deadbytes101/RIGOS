use std::{env, fs, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let write = env::args().any(|v| v == "--write");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("schemas");
    let mut mismatch = false;
    for (name, schema) in rigos_schema::schemas() {
        let rendered = format!(
            "{}\n",
            serde_json::to_string_pretty(&schema).expect("serialize schema")
        );
        let path = dir.join(name);
        if write {
            fs::write(path, rendered).expect("write schema");
        } else if fs::read_to_string(&path).ok().as_deref() != Some(&rendered) {
            eprintln!("schema drift: {}", path.display());
            mismatch = true;
        }
    }
    if mismatch {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
