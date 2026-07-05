#![forbid(unsafe_code)]

#[path = "lib.rs"]
mod base;

pub use base::{
    ConfigDiagnostic, ConfigError, CpuPolicy, ExternalReference, FlightSheet, FlightSource,
    IdentityRecord, ImportProvenance, MAX_CONFIG_BYTES, MAX_SHEET_BYTES, MinerStartMode, Pool,
    Proposal, RigProfile, Threads, build_runtime, commit_revision, parse_flight_sheet,
    parse_rig_profile, safe_join, safe_json_basename, validate_identity,
};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

const MAX_HIVE_HUGEPAGES: u64 = 1_048_576;

pub fn import_hive_style(
    bytes: &[u8],
    filename: &str,
) -> Result<(FlightSheet, ImportProvenance), ConfigError> {
    let root: Value = serde_json::from_slice(bytes)
        .map_err(|_| compat_error(filename, None, "invalid external JSON"))?;
    let envelope = root
        .as_object()
        .ok_or_else(|| compat_error(filename, None, "external sheet must be an object"))?;

    if !envelope.contains_key("items") {
        return base::import_hive_style(bytes, filename);
    }

    let items = envelope
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| compat_error(filename, Some("items"), "items must be an array"))?;
    if items.len() != 1 {
        return Err(compat_error(
            filename,
            Some("items"),
            "exactly one Hive workload item is required",
        ));
    }
    let workload = items[0].as_object().ok_or_else(|| {
        compat_error(
            filename,
            Some("items"),
            "the Hive workload item must be an object",
        )
    })?;
    let miner_config = workload
        .get("miner_config")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            compat_error(
                filename,
                Some("miner_config"),
                "the Hive workload requires exactly one miner_config object",
            )
        })?;

    validate_keys(miner_config, filename)?;
    validate_cpu(miner_config.get("cpu"), filename)?;
    validate_fork(miner_config.get("fork"), filename)?;
    let hugepages = parse_hugepages(miner_config.get("hugepages"), filename)?;
    let cpu_policy = explicit_huge_pages(miner_config.get("cpu_config"), filename, "cpu_config")?;
    let user_policy =
        explicit_huge_pages(miner_config.get("user_config"), filename, "user_config")?;

    if let (Some(cpu_value), Some(user_value)) = (cpu_policy, user_policy) {
        if cpu_value != user_value {
            return Err(compat_error(
                filename,
                Some("hugepages"),
                "cpu_config and user_config disagree about huge pages",
            ));
        }
    }
    let explicit_policy = cpu_policy.or(user_policy);
    if let (Some(count), Some(enabled)) = (hugepages, explicit_policy) {
        if (count > 0) != enabled {
            return Err(compat_error(
                filename,
                Some("hugepages"),
                "hugepages conflicts with embedded huge-pages policy",
            ));
        }
    }

    let mut sanitized = root.clone();
    let sanitized_config = sanitized
        .as_object_mut()
        .and_then(|object| object.get_mut("items"))
        .and_then(Value::as_array_mut)
        .and_then(|values| values.first_mut())
        .and_then(Value::as_object_mut)
        .and_then(|object| object.get_mut("miner_config"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            compat_error(
                filename,
                Some("miner_config"),
                "miner_config changed during normalization",
            )
        })?;
    sanitized_config.remove("cpu");
    sanitized_config.remove("fork");
    sanitized_config.remove("hugepages");

    let normalized = serde_json::to_vec(&sanitized)
        .map_err(|_| compat_error(filename, None, "external sheet normalization failed"))?;
    let (mut sheet, mut provenance) = base::import_hive_style(&normalized, filename)?;
    if let Some(count) = hugepages {
        sheet.cpu.huge_pages = count > 0;
    }
    provenance.source_sha256 = hex::encode(Sha256::digest(bytes));
    Ok((sheet, provenance))
}

fn validate_keys(config: &Map<String, Value>, filename: &str) -> Result<(), ConfigError> {
    const ALLOWED: &[&str] = &[
        "algo",
        "url",
        "pass",
        "template",
        "cpu_config",
        "user_config",
        "cpu",
        "fork",
        "hugepages",
    ];
    if let Some(unknown) = config
        .keys()
        .find(|key| !ALLOWED.contains(&key.as_str()))
    {
        return Err(compat_error(
            filename,
            Some(unknown),
            "unsupported miner_config field",
        ));
    }
    Ok(())
}

fn validate_cpu(value: Option<&Value>, filename: &str) -> Result<(), ConfigError> {
    match value {
        None | Some(Value::Bool(true)) => Ok(()),
        Some(Value::Number(number)) if number.as_u64() == Some(1) => Ok(()),
        _ => Err(compat_error(
            filename,
            Some("cpu"),
            "miner_config cpu must be integer 1 or boolean true",
        )),
    }
}

fn validate_fork(value: Option<&Value>, filename: &str) -> Result<(), ConfigError> {
    match value {
        None => Ok(()),
        Some(Value::String(value)) if value == "xmrig" => Ok(()),
        _ => Err(compat_error(
            filename,
            Some("fork"),
            "miner_config fork must be xmrig",
        )),
    }
}

fn parse_hugepages(value: Option<&Value>, filename: &str) -> Result<Option<u64>, ConfigError> {
    match value {
        None => Ok(None),
        Some(Value::Number(number)) => match number.as_u64() {
            Some(value) if value <= MAX_HIVE_HUGEPAGES => Ok(Some(value)),
            _ => Err(compat_error(
                filename,
                Some("hugepages"),
                "miner_config hugepages is outside the supported range",
            )),
        },
        _ => Err(compat_error(
            filename,
            Some("hugepages"),
            "miner_config hugepages must be a non-negative integer",
        )),
    }
}

fn explicit_huge_pages(
    value: Option<&Value>,
    filename: &str,
    key: &str,
) -> Result<Option<bool>, ConfigError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if let Value::String(text) = value {
        if !text.contains("\"huge-pages\"") {
            return Ok(None);
        }
    }

    let parsed = match value {
        Value::Object(object) => Value::Object(object.clone()),
        Value::String(text) => Value::Object(parse_member_fragment(text, filename, key)?),
        _ => {
            return Err(compat_error(
                filename,
                Some(key),
                "embedded miner config must be an object or member fragment",
            ));
        }
    };
    let object = parsed
        .as_object()
        .ok_or_else(|| compat_error(filename, Some(key), "embedded miner config must be an object"))?;
    let cpu = object
        .get("cpu")
        .and_then(Value::as_object)
        .unwrap_or(object);
    match cpu.get("huge-pages") {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(compat_error(
            filename,
            Some(key),
            "embedded huge-pages must be boolean",
        )),
    }
}

fn parse_member_fragment(
    input: &str,
    filename: &str,
    key: &str,
) -> Result<Map<String, Value>, ConfigError> {
    let trimmed = input.trim();
    let direct = if trimmed.starts_with('{') {
        trimmed.to_owned()
    } else {
        format!("{{{trimmed}}}")
    };
    if let Ok(value) = serde_json::from_str::<Value>(&direct) {
        return value
            .as_object()
            .cloned()
            .ok_or_else(|| compat_error(filename, Some(key), "embedded config must be an object"));
    }
    let normalized = insert_member_commas(trimmed)
        .ok_or_else(|| compat_error(filename, Some(key), "embedded config is malformed"))?;
    serde_json::from_str::<Value>(&format!("{{{normalized}}}"))
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or_else(|| compat_error(filename, Some(key), "embedded config is malformed"))
}

fn insert_member_commas(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(input.len() + 8);
    let mut index = 0;
    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            output.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => {
                in_string = true;
                output.push(byte);
                index += 1;
            }
            b'{' | b'[' => {
                depth = depth.checked_add(1)?;
                output.push(byte);
                index += 1;
            }
            b'}' | b']' => {
                depth = depth.checked_sub(1)?;
                output.push(byte);
                index += 1;
            }
            value if value.is_ascii_whitespace() && depth == 0 => {
                let start = index;
                while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                output.extend_from_slice(&bytes[start..index]);
                if index < bytes.len() && bytes[index] == b'"' {
                    let previous = output
                        .iter()
                        .rev()
                        .copied()
                        .find(|value| !value.is_ascii_whitespace())?;
                    if matches!(previous, b'}' | b']' | b'"' | b'0'..=b'9' | b'e' | b'l') {
                        output.push(b',');
                    }
                }
            }
            _ => {
                output.push(byte);
                index += 1;
            }
        }
    }
    if in_string || depth != 0 {
        None
    } else {
        String::from_utf8(output).ok()
    }
}

fn compat_error(filename: &str, key: Option<&str>, message: &str) -> ConfigError {
    ConfigError {
        diagnostic: ConfigDiagnostic {
            code: "RIGOS_FLIGHT_SHEET_INVALID".into(),
            file: Some(filename.into()),
            line: None,
            key: key.map(str::to_owned),
            message: message.into(),
        },
    }
}
