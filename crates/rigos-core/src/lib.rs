#![forbid(unsafe_code)]

use chrono::{DateTime, SecondsFormat, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ENVELOPE_SCHEMA: &str = "rigos.cli-envelope/v1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Ok,
    Partial,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub component: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub context: BTreeMap<String, serde_json::Value>,
}

impl Diagnostic {
    pub fn warning(code: &str, component: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Warning,
            component: component.into(),
            message: message.into(),
            context: BTreeMap::new(),
        }
    }

    pub fn error(code: &str, component: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            component: component.into(),
            message: message.into(),
            context: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum Observation<T> {
    Observed { value: T, source: String },
    Unavailable { reason: String },
    Unsupported { reason: String },
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct BuildMetadata {
    pub rigosd_version: String,
    pub build_commit: String,
    pub target: String,
    pub profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_channel: Option<String>,
}

impl BuildMetadata {
    pub fn current() -> Self {
        Self {
            rigosd_version: env!("CARGO_PKG_VERSION").into(),
            build_commit: option_env!("RIGOS_BUILD_COMMIT")
                .unwrap_or("unknown")
                .into(),
            target: option_env!("RIGOS_BUILD_TARGET")
                .unwrap_or(std::env::consts::ARCH)
                .into(),
            profile: option_env!("RIGOS_BUILD_PROFILE")
                .unwrap_or("unknown")
                .into(),
            product_version: option_env!("RIGOS_PRODUCT_VERSION").map(str::to_owned),
            image_id: option_env!("RIGOS_IMAGE_ID").map(str::to_owned),
            image_version: option_env!("RIGOS_IMAGE_VERSION").map(str::to_owned),
            image_channel: option_env!("RIGOS_IMAGE_CHANNEL").map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CliEnvelope<T> {
    pub schema: String,
    pub command: String,
    pub status: ExecutionStatus,
    pub observed_at: String,
    pub data_schema: String,
    pub data: Option<T>,
    pub diagnostics: Vec<Diagnostic>,
    pub meta: BuildMetadata,
}

impl<T> CliEnvelope<T> {
    pub fn new(
        command: &str,
        data_schema: &str,
        data: Option<T>,
        mut diagnostics: Vec<Diagnostic>,
        fatal: bool,
    ) -> Self {
        diagnostics.sort_by(|a, b| (&a.code, &a.component).cmp(&(&b.code, &b.component)));
        let status = if fatal || data.is_none() {
            ExecutionStatus::Error
        } else if diagnostics.iter().any(|d| d.severity != Severity::Info) {
            ExecutionStatus::Partial
        } else {
            ExecutionStatus::Ok
        };
        Self {
            schema: ENVELOPE_SCHEMA.into(),
            command: command.into(),
            status,
            observed_at: timestamp(Utc::now()),
            data_schema: data_schema.into(),
            data,
            diagnostics,
            meta: BuildMetadata::current(),
        }
    }
}

pub fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub struct InspectionResult<T> {
    pub value: Option<T>,
    pub diagnostics: Vec<Diagnostic>,
    pub fatal: bool,
}

impl<T> InspectionResult<T> {
    pub fn success(value: T, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            value: Some(value),
            diagnostics,
            fatal: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_is_partial_on_warning() {
        let e = CliEnvelope::new(
            "doctor",
            "rigos.doctor-report/v1",
            Some(1),
            vec![Diagnostic::warning("test.warning", "test", "warning")],
            false,
        );
        assert_eq!(e.status, ExecutionStatus::Partial);
    }

    #[test]
    fn timestamp_is_utc_millis() {
        let value = DateTime::parse_from_rfc3339("2026-07-04T05:34:56.123Z").unwrap();
        assert_eq!(timestamp(value.into()), "2026-07-04T05:34:56.123Z");
    }
}
