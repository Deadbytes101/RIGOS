#![forbid(unsafe_code)]

use rigos_config::{FlightSheet, IdentityRecord, RigProfile};
use rigos_core::{CliEnvelope, Diagnostic};
use rigos_machine::MachineSnapshotV1;
use rigos_pool::PoolProfile;
use rigos_xmrig::MinerSnapshotV1;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const BUILD_MANIFEST_SCHEMA: &str = "rigos.build-manifest/v1";
pub const VALIDATION_MANIFEST_SCHEMA: &str = "rigos.physical-validation-manifest/v1";
pub const VALIDATION_RESULT_SCHEMA: &str = "rigos.physical-validation-result/v1";
pub const REDACTION_REPORT_SCHEMA: &str = "rigos.redaction-report/v1";
pub const ABOUT_SCHEMA: &str = "rigos.about/v1";
pub const LICENSES_SCHEMA: &str = "rigos.licenses/v1";
pub const COMPONENT_PROVENANCE_SCHEMA: &str = "rigos.component-provenance/v1";
pub const IMAGE_LAYOUT_SCHEMA: &str = "rigos.image-layout/v1";
pub const IMAGE_BUILD_MANIFEST_SCHEMA: &str = "rigos.image-build-manifest/v1";
pub const STATE_LAYOUT_SCHEMA: &str = "rigos.state-layout/v1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ReleaseInfoV1 {
    pub schema: String,
    pub product: String,
    pub product_version: String,
    pub image_id: String,
    pub image_version: String,
    pub image_channel: String,
    pub variant: String,
    pub architecture: String,
    pub base_id: String,
    pub base_version_id: String,
    pub build_id: String,
    pub build_commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ComponentProvenanceV1 {
    pub schema: String,
    pub component: String,
    pub version: String,
    pub source: String,
    pub modified: bool,
    pub architecture: String,
    pub artifact: String,
    pub archive_sha256: String,
    pub binary_sha256: String,
    pub license: String,
    pub upstream_donation_behavior: String,
    pub rigos_receives_donation: bool,
    pub rigos_fee_percent: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct AboutReportV1 {
    pub release: ReleaseInfoV1,
    pub subscription: String,
    pub worker_limit: String,
    pub mining_fee_percent: u32,
    pub cloud_dependency: String,
    pub bundled_miner: ComponentProvenanceV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LicenseEntryV1 {
    pub component: String,
    pub license: String,
    pub notice_path: String,
    pub license_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LicensesReportV1 {
    pub entries: Vec<LicenseEntryV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ImagePartitionV1 {
    pub number: u32,
    pub label: String,
    pub type_guid: String,
    pub unique_guid: String,
    pub start_lba: u64,
    pub minimum_size_lba: u64,
    pub filesystem: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ImageLayoutV1 {
    pub schema: String,
    pub image_version: String,
    pub image_id: String,
    pub partition_table: String,
    pub disk_guid: String,
    pub logical_sector_size: u32,
    pub minimum_media_size_bytes: u64,
    pub alignment_lba: u64,
    pub final_state_partition: u32,
    pub build_commit: String,
    pub root_payload_sha256: String,
    pub partitions: Vec<ImagePartitionV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct StateLayoutV1 {
    pub schema: String,
    pub image_version: String,
    pub initialization_state: String,
    pub partition_number: u32,
    pub partition_start_lba: u64,
    pub partition_end_lba: u64,
    pub filesystem_type: String,
    pub filesystem_uuid: String,
    pub state_capacity_bytes: u64,
    pub authoritative_image_commit: String,
    pub initialized_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ImageBuildManifestV1 {
    pub schema: String,
    pub product: String,
    pub product_version: String,
    pub image_id: String,
    pub image_version: String,
    pub image_channel: String,
    pub source_commit: String,
    pub source_date_epoch: u64,
    pub target: String,
    pub base: String,
    pub kernel: String,
    pub artifact: String,
    pub artifact_sha256: String,
    pub artifact_size_bytes: u64,
    pub root_a_sha256: String,
    pub root_b_sha256: String,
    pub root_payload_sha256: String,
    pub layout: ImageLayoutV1,
    pub components: Vec<ComponentProvenanceV1>,
    pub tools: BTreeMap<String, String>,
}

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

pub const DOCTOR_SCHEMA: &str = "rigos.doctor-report/v1";

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
    output.insert(
        "pool-profile-v1.schema.json",
        serde_json::to_value(schema_for!(PoolProfile)).unwrap(),
    );
    output.insert(
        "about-v1.schema.json",
        serde_json::to_value(schema_for!(AboutReportV1)).unwrap(),
    );
    output.insert(
        "licenses-v1.schema.json",
        serde_json::to_value(schema_for!(LicensesReportV1)).unwrap(),
    );
    output.insert(
        "component-provenance-v1.schema.json",
        serde_json::to_value(schema_for!(ComponentProvenanceV1)).unwrap(),
    );
    output.insert(
        "image-layout-v1.schema.json",
        serde_json::to_value(schema_for!(ImageLayoutV1)).unwrap(),
    );
    output.insert(
        "image-build-manifest-v1.schema.json",
        serde_json::to_value(schema_for!(ImageBuildManifestV1)).unwrap(),
    );
    output.insert(
        "state-layout-v1.schema.json",
        serde_json::to_value(schema_for!(StateLayoutV1)).unwrap(),
    );
    output.insert(
        "rig-profile-v1.schema.json",
        serde_json::to_value(schema_for!(RigProfile)).unwrap(),
    );
    output.insert(
        "flight-sheet-v1.schema.json",
        serde_json::to_value(schema_for!(FlightSheet)).unwrap(),
    );
    output.insert(
        "identity-v1.schema.json",
        serde_json::to_value(schema_for!(IdentityRecord)).unwrap(),
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
