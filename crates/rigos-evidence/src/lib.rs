#![forbid(unsafe_code)]

use rigos_schema::{REDACTION_REPORT_SCHEMA, RedactionReportV1};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const REDACTION_POLICY: &str = "rigos.validation-redaction/v1";

#[derive(Debug, Error)]
pub enum EvidenceError {
    #[error("I/O operation failed")]
    Io(#[from] std::io::Error),
    #[error("JSON parsing failed")]
    Json(#[from] serde_json::Error),
    #[error("invalid recipient: {0}")]
    InvalidRecipient(String),
    #[error("forbidden content remains in {0}")]
    ForbiddenContent(String),
    #[error("unsupported evidence file: {0}")]
    UnsupportedFile(String),
}

#[derive(Debug, serde::Serialize)]
pub struct RecipientSet {
    pub recipients: Vec<String>,
    pub recipient_set_sha256: String,
}

pub fn load_recipients(path: &Path) -> Result<RecipientSet, EvidenceError> {
    let mut recipients = BTreeSet::new();
    for raw in fs::read_to_string(path)?.lines() {
        let value = raw.trim();
        if value.is_empty() || value.starts_with('#') {
            continue;
        }
        if value.starts_with("AGE-SECRET-KEY-")
            || value.starts_with("age1ssh")
            || value.starts_with("age1plugin")
        {
            return Err(EvidenceError::InvalidRecipient(
                "unsupported or private recipient form".into(),
            ));
        }
        if !value.starts_with("age1")
            || value.len() < 50
            || !value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return Err(EvidenceError::InvalidRecipient(
                "expected native age X25519 recipient".into(),
            ));
        }
        if !recipients.insert(value.to_owned()) {
            return Err(EvidenceError::InvalidRecipient(
                "duplicate recipient".into(),
            ));
        }
    }
    if recipients.is_empty() {
        return Err(EvidenceError::InvalidRecipient(
            "recipient set is empty".into(),
        ));
    }
    let recipients: Vec<_> = recipients.into_iter().collect();
    let canonical = format!("{}\n", recipients.join("\n"));
    Ok(RecipientSet {
        recipients,
        recipient_set_sha256: hex::encode(Sha256::digest(canonical.as_bytes())),
    })
}

#[derive(Default)]
struct RedactionState {
    counts: BTreeMap<String, u64>,
}

#[derive(serde::Deserialize)]
struct PrivateIdentityContext {
    hostname: String,
    username: String,
    home: String,
}

impl RedactionState {
    fn replace(&mut self, category: &str) {
        *self.counts.entry(category.into()).or_default() += 1;
    }
}

fn secret_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "access-token"
            | "access_token"
            | "authorization"
            | "token"
            | "password"
            | "private_key"
            | "wallet"
            | "user"
            | "username"
    )
}

fn redact_json(
    value: &mut Value,
    state: &mut RedactionState,
    private: &PrivateIdentityContext,
    alias: &str,
) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if secret_key(key) {
                    *child = Value::String("<REDACTED_SECRET>".into());
                    state.replace("credential");
                } else {
                    redact_json(child, state, private, alias);
                }
            }
        }
        Value::Array(values) => values
            .iter_mut()
            .for_each(|v| redact_json(v, state, private, alias)),
        Value::String(text) => *text = redact_text(text, state, private, alias),
        _ => {}
    }
}

fn redact_text(
    input: &str,
    state: &mut RedactionState,
    private: &PrivateIdentityContext,
    alias: &str,
) -> String {
    let normalized = input
        .replace(&private.home, "<HOME>")
        .replace(&private.hostname, alias)
        .replace(&private.username, "<USER>");
    if normalized != input {
        state.replace("machine_identity");
    }
    let mut output = String::with_capacity(input.len());
    let mut token = String::new();
    let flush = |token: &mut String, output: &mut String, state: &mut RedactionState| {
        if token.is_empty() {
            return;
        }
        let replacement = if token.contains("AGE-SECRET-KEY-")
            || token.to_ascii_lowercase().starts_with("bearer")
        {
            state.replace("credential");
            "<REDACTED_SECRET>".into()
        } else if token.contains('@') && (token.contains("://") || token.contains(':')) {
            state.replace("mining_identity");
            token
                .rsplit_once('@')
                .map(|(_, host)| format!("<MINING_IDENTITY>@{host}"))
                .unwrap_or_else(|| token.clone())
        } else if token.parse::<std::net::IpAddr>().is_ok() {
            state.replace("ip_address");
            "<IP>".into()
        } else {
            token.clone()
        };
        output.push_str(&replacement);
        token.clear();
    };
    for character in normalized.chars() {
        if character.is_whitespace() {
            flush(&mut token, &mut output, state);
            output.push(character);
        } else {
            token.push(character);
        }
    }
    flush(&mut token, &mut output, state);
    output
}

fn forbidden(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "sentinel_secret",
        "authorization:",
        "bearer ",
        "age-secret-key-",
        "-----begin private key-----",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

pub fn sanitize_approved(
    raw: &Path,
    public: &Path,
    node_alias: &str,
) -> Result<RedactionReportV1, EvidenceError> {
    const APPROVED: &[&str] = &[
        "inventory/os-release.txt",
        "inventory/uname.txt",
        "inventory/lscpu.txt",
        "inventory/runtime-libraries.txt",
        "inspection/machine-inspect.json",
        "inspection/miner-stopped.json",
        "inspection/miner-running-no-api.json",
        "inspection/miner-running-loopback-api.json",
        "inspection/doctor.json",
        "mutation/before.sha256",
        "mutation/after.sha256",
        "mutation/comparison.txt",
        "verification/verify.log",
        "verification/schema-validation.txt",
        "verification/secret-scan.txt",
        "verification/probe-timeout.json",
        "verification/probe-processes-after.txt",
    ];
    fs::create_dir_all(public)?;
    let private: PrivateIdentityContext =
        serde_json::from_str(&fs::read_to_string(raw.join("raw-meta/privacy.json"))?)?;
    let mut state = RedactionState::default();
    let mut inputs = 0u64;
    let mut outputs = 0u64;
    let json_failures = 0u64;
    for relative in APPROVED {
        let source = raw.join(relative);
        if !source.is_file() {
            continue;
        }
        inputs += 1;
        let text = fs::read_to_string(&source)?;
        let sanitized = if relative.ends_with(".json") {
            match serde_json::from_str::<Value>(&text) {
                Ok(mut value) => {
                    redact_json(&mut value, &mut state, &private, node_alias);
                    format!("{}\n", serde_json::to_string_pretty(&value)?)
                }
                Err(error) => {
                    return Err(error.into());
                }
            }
        } else {
            format!("{}\n", redact_text(&text, &mut state, &private, node_alias))
        };
        if forbidden(&sanitized) {
            return Err(EvidenceError::ForbiddenContent((*relative).into()));
        }
        let destination = public.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, sanitized)?;
        outputs += 1;
    }
    Ok(RedactionReportV1 {
        schema: REDACTION_REPORT_SCHEMA.into(),
        policy: REDACTION_POLICY.into(),
        input_file_count: inputs,
        output_file_count: outputs,
        replacements: state.counts,
        rejected_file_count: 0,
        json_parse_failures: json_failures,
        remaining_forbidden_patterns: 0,
    })
}

pub fn sha256_file(path: &Path) -> Result<String, EvidenceError> {
    Ok(hex::encode(Sha256::digest(fs::read(path)?)))
}

pub fn approved_public_files(root: &Path) -> Result<Vec<PathBuf>, EvidenceError> {
    fn walk(dir: &Path, root: &Path, files: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(&path, root, files)?;
            } else {
                files.push(path.strip_prefix(root).unwrap().to_owned());
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp(name: &str) -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("rigos-evidence-{name}-{id}"))
    }

    #[test]
    fn recipients_are_sorted_and_hashed() {
        let path = temp("recipients");
        let a = format!("age1{}", "a".repeat(58));
        let b = format!("age1{}", "b".repeat(58));
        fs::write(&path, format!("{b}\n{a}\n")).unwrap();
        let result = load_recipients(&path).unwrap();
        let _ = fs::remove_file(path);
        assert_eq!(result.recipients, vec![a, b]);
        assert_eq!(result.recipient_set_sha256.len(), 64);
    }

    #[test]
    fn private_identity_is_rejected() {
        let path = temp("secret");
        fs::write(&path, "AGE-SECRET-KEY-1ABC\n").unwrap();
        assert!(load_recipients(&path).is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn json_redaction_preserves_shape_and_removes_secret() {
        let raw = temp("raw");
        let public = temp("public");
        fs::create_dir_all(raw.join("inspection")).unwrap();
        fs::write(
            raw.join("inspection/doctor.json"),
            r#"{"access-token":"SENTINEL_SECRET","host":"private-host","path":"/home/private-user/file","value":1}"#,
        )
        .unwrap();
        fs::create_dir_all(raw.join("raw-meta")).unwrap();
        fs::write(
            raw.join("raw-meta/privacy.json"),
            r#"{"hostname":"private-host","username":"private-user","home":"/home/private-user"}"#,
        )
        .unwrap();
        let report = sanitize_approved(&raw, &public, "rig01").unwrap();
        let text = fs::read_to_string(public.join("inspection/doctor.json")).unwrap();
        assert!(!text.contains("SENTINEL_SECRET"));
        assert!(!text.contains("private-host"));
        assert!(!text.contains("private-user"));
        assert!(text.contains("rig01"));
        assert_eq!(report.remaining_forbidden_patterns, 0);
        let _ = fs::remove_dir_all(raw);
        let _ = fs::remove_dir_all(public);
    }
}
