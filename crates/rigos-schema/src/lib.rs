#![forbid(unsafe_code)]

use rigos_core::{CliEnvelope, Diagnostic};
use rigos_machine::MachineSnapshotV1;
use rigos_xmrig::MinerSnapshotV1;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const BUILD_MANIFEST_SCHEMA: &str = "dbyte.rigos.build-manifest/v1";
pub const VALIDATION_MANIFEST_SCHEMA: &str = "dbyte.rigos.physical-validation-manifest/v1";
pub const VALIDATION_RESULT_SCHEMA: &str = "dbyte.rigos.physical-validation-result/v1";
pub const REDACTION_REPORT_SCHEMA: &str = "dbyte.rigos.redaction-report/v1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BuildManifestV1 {
    pub schema: String,
    pub artifact: String,
    pub release_candidate: String,
    pub git_commit: String,
    pub git_tree_clean: bool,
    pub target: String,
    pub build_os: String,
    pub kernel: String,
    pub rustc: String,
    pub cargo: String,
    pub build_profile: String,
    pub binary_sha256: String,
    pub schemas_sha256: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidationCheckV1 {
    pub id: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PhysicalValidationResultV1 {
    pub schema: String,
    pub run_id: String,
    pub overall: String,
    pub checks: Vec<ValidationCheckV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RedactionReportV1 {
    pub schema: String,
    pub policy: String,
    pub input_file_count: u64,
    pub output_file_count: u64,
    pub replacements: BTreeMap<String, u64>,
    pub rejected_file_count: u64,
    pub json_parse_failures: u64,
    pub remaining_forbidden_patterns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuthoritativeBinaryV1 {
    pub name: String,
    pub sha256: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidationNodeV1 {
    pub alias: String,
    pub hardware_class: String,
    pub architecture: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidationRuntimeV1 {
    pub distribution: String,
    pub distribution_major: u32,
    pub kernel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PrivateArchiveV1 {
    pub retained: bool,
    pub format: String,
    pub encryption_schema: String,
    pub recipient_set_id: String,
    pub recipient_set_sha256: String,
    pub recipient_count: u32,
    pub ciphertext_sha256: String,
    pub ciphertext_size_bytes: u64,
    pub decryptability_verified: bool,
    pub location_disclosed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PhysicalValidationManifestV1 {
    pub schema: String,
    pub run_id: String,
    pub release_candidate: String,
    pub source_commit: String,
    pub authoritative_binary: AuthoritativeBinaryV1,
    pub node: ValidationNodeV1,
    pub runtime: ValidationRuntimeV1,
    pub started_at: String,
    pub completed_at: String,
    pub result: String,
    pub public_evidence_sha256: BTreeMap<String, String>,
    pub private_archive: PrivateArchiveV1,
    pub redaction_policy: String,
}

pub const DOCTOR_SCHEMA: &str = "dbyte.rigos.doctor-report/v1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DoctorCheckV1 {
    pub id: String,
    pub status: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DoctorReportV1 {
    pub checks: Vec<DoctorCheckV1>,
}

pub fn schemas() -> BTreeMap<&'static str, serde_json::Value> {
    let mut output = BTreeMap::new();
    output.insert(
        "cli-envelope-v1.schema.json",
        serde_json::to_value(schema_for!(CliEnvelope<serde_json::Value>)).unwrap(),
    );
    output.insert(
        "machine-snapshot-v1.schema.json",
        serde_json::to_value(schema_for!(MachineSnapshotV1)).unwrap(),
    );
    output.insert(
        "miner-snapshot-v1.schema.json",
        serde_json::to_value(schema_for!(MinerSnapshotV1)).unwrap(),
    );
    output.insert(
        "doctor-report-v1.schema.json",
        serde_json::to_value(schema_for!(DoctorReportV1)).unwrap(),
    );
    output.insert(
        "build-manifest-v1.schema.json",
        serde_json::to_value(schema_for!(BuildManifestV1)).unwrap(),
    );
    output.insert(
        "physical-validation-manifest-v1.schema.json",
        serde_json::to_value(schema_for!(PhysicalValidationManifestV1)).unwrap(),
    );
    output.insert(
        "physical-validation-result-v1.schema.json",
        serde_json::to_value(schema_for!(PhysicalValidationResultV1)).unwrap(),
    );
    output.insert(
        "redaction-report-v1.schema.json",
        serde_json::to_value(schema_for!(RedactionReportV1)).unwrap(),
    );
    output
}

pub fn doctor(
    machine_diagnostics: &[Diagnostic],
    miner_diagnostics: &[Diagnostic],
) -> DoctorReportV1 {
    let mut checks = vec![
        DoctorCheckV1 {
            id: "machine.inspect".into(),
            status: if machine_diagnostics.is_empty() {
                "pass"
            } else {
                "warning"
            }
            .into(),
            summary: format!("{} diagnostic(s)", machine_diagnostics.len()),
        },
        DoctorCheckV1 {
            id: "miner.inspect".into(),
            status: if miner_diagnostics.is_empty() {
                "pass"
            } else {
                "warning"
            }
            .into(),
            summary: format!("{} diagnostic(s)", miner_diagnostics.len()),
        },
        DoctorCheckV1 {
            id: "mutation.boundary".into(),
            status: "pass".into(),
            summary: "v0.0.1 exposes inspection operations only".into(),
        },
    ];
    checks.sort_by(|a, b| a.id.cmp(&b.id));
    DoctorReportV1 { checks }
}
