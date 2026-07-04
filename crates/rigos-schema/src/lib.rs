#![forbid(unsafe_code)]

use rigos_core::{CliEnvelope, Diagnostic};
use rigos_machine::MachineSnapshotV1;
use rigos_xmrig::MinerSnapshotV1;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
