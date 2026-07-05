#[path = "../src/lib_entry.rs"]
mod compatibility;

use compatibility::import_hive_style;
use serde_json::{Value, json};

#[test]
fn synthetic_hive_fixture_maps_without_runtime_identity() {
    let input = include_bytes!("fixtures/hive-xmrig.json");
    let (sheet, provenance) = import_hive_style(input, "hive-xmrig.json").unwrap();
    assert_eq!(sheet.backend, "xmrig");
    assert_eq!(sheet.algorithm, "rx/0");
    assert_eq!(sheet.worker_template, "{node_name}");
    assert_eq!(sheet.pools.len(), 2);
    assert!(sheet.pools[0].tls);
    assert_eq!(sheet.cpu.max_threads_hint, 75);
    assert!(sheet.cpu.huge_pages);
    assert_eq!(sheet.identity_ref, "hive-wal-fixture-wallet-ref");
    assert_eq!(provenance.warnings.len(), 1);
    assert!(
        !serde_json::to_string(&sheet)
            .unwrap()
            .contains("RIG_PASSWD")
    );
}

#[test]
fn hive_envelope_rejects_ambiguous_or_invalid_pool_contracts() {
    let cases = [
        r#"{"items":[]}"#,
        r#"{"items":[{},{}]}"#,
        r#"{"items":[{"miner":"xmrig","pool_urls":["pool.invalid:1",7],"pool_ssl":[false,false],"miner_config":{"algo":"rx/0","url":"%URL%","template":"%WAL%","pass":"%WORKER_NAME%"}}]}"#,
        r#"{"items":[{"miner":"xmrig","pool_urls":["pool.invalid:1","backup.invalid:2"],"pool_ssl":[false],"miner_config":{"algo":"rx/0","url":"%URL%","template":"%WAL%","pass":"%WORKER_NAME%"}}]}"#,
        r#"{"items":[{"miner":"xmrig","pool_urls":["pool.invalid:1"],"pool_ssl":["false"],"miner_config":{"algo":"rx/0","url":"%URL%","template":"%WAL%","pass":"%WORKER_NAME%"}}]}"#,
        r#"{"items":[{"miner":"xmrig","pool_urls":["stratum+ssl://pool.invalid:1"],"pool_ssl":false,"miner_config":{"algo":"rx/0","url":"%URL%","template":"%WAL%","pass":"%WORKER_NAME%"}}]}"#,
        r#"{"items":[{"miner":"xmrig","pool_urls":["pool.invalid:1"],"pool_ssl":false,"miner_config":{"algo":"rx/0","url":"pool.invalid:1","template":"%WAL%","pass":"%WORKER_NAME%"}}]}"#,
        r#"{"items":[{"miner":"xmrig","pool_urls":["pool.invalid:1"],"pool_ssl":false,"miner_config":{"algo":"rx/0","url":"%URL%","template":"%WAL%","pass":"%WORKER_NAME%","dangerous":"value"}}]}"#,
    ];
    for input in cases {
        assert!(import_hive_style(input.as_bytes(), "hive-xmrig.json").is_err());
    }
}

fn real_shape(cpu: Value, fork: Value, hugepages: Value, huge_pages: bool) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "items": [{
            "name": "Synthetic XMR",
            "coin": "XMR",
            "miner": "xmrig-new",
            "pool_urls": ["pool.invalid:10001"],
            "pool_ssl": false,
            "wal_id": "fixture-wallet-ref",
            "miner_config": {
                "cpu": cpu,
                "fork": fork,
                "hugepages": hugepages,
                "algo": "rx/0",
                "url": "%URL%",
                "template": "%WAL%",
                "pass": "%WORKER_NAME%",
                "cpu_config": format!("\"huge-pages\": {huge_pages}"),
                "user_config": format!("\"cpu\": {{\"huge-pages\": {huge_pages}}}\n\"api\": {{\"worker-id\": \"fixture\"}}")
            }
        }]
    }))
    .unwrap()
}

#[test]
fn cpu_and_fork_markers_are_strict() {
    assert!(import_hive_style(&real_shape(json!(1), json!("xmrig"), json!(1280), true), "fixture.json").is_ok());
    assert!(import_hive_style(&real_shape(json!(true), json!("xmrig"), json!(1280), true), "fixture.json").is_ok());
    for cpu in [json!(0), json!(false), json!(2), json!("1")] {
        assert!(import_hive_style(&real_shape(cpu, json!("xmrig"), json!(1280), true), "fixture.json").is_err());
    }
    assert!(import_hive_style(&real_shape(json!(1), json!("other"), json!(1280), true), "fixture.json").is_err());
}

#[test]
fn hugepages_maps_and_conflicts_fail() {
    let (disabled, _) = import_hive_style(
        &real_shape(json!(1), json!("xmrig"), json!(0), false),
        "fixture.json",
    )
    .unwrap();
    assert!(!disabled.cpu.huge_pages);
    assert!(import_hive_style(&real_shape(json!(1), json!("xmrig"), json!(1280), false), "fixture.json").is_err());
}

#[test]
fn unknown_nested_field_remains_rejected() {
    let mut value: Value = serde_json::from_slice(&real_shape(
        json!(1),
        json!("xmrig"),
        json!(1280),
        true,
    ))
    .unwrap();
    value["items"][0]["miner_config"]["unknown"] = json!(true);
    assert!(import_hive_style(&serde_json::to_vec(&value).unwrap(), "fixture.json").is_err());
}
