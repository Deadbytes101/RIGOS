use rigos_config::import_hive_style;

#[test]
fn synthetic_hive_fixture_maps_without_runtime_identity() {
    let input = include_bytes!("fixtures/hive-xmrig.json");
    let (sheet, provenance) = import_hive_style(input, "hive-xmrig.json").unwrap();
    assert_eq!(sheet.backend, "xmrig");
    assert_eq!(sheet.pools.len(), 2);
    assert!(sheet.pools[0].tls);
    assert_eq!(sheet.cpu.max_threads_hint, 75);
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
        r#"{"items":[{"miner":"xmrig","pool_urls":["pool.invalid:1",7],"pool_ssl":[false,false]}]}"#,
        r#"{"items":[{"miner":"xmrig","pool_urls":["pool.invalid:1","backup.invalid:2"],"pool_ssl":[false]}]}"#,
        r#"{"items":[{"miner":"xmrig","pool_urls":["pool.invalid:1"],"pool_ssl":["false"]}]}"#,
        r#"{"items":[{"miner":"xmrig","pool_urls":["stratum+ssl://pool.invalid:1"],"pool_ssl":false}]}"#,
    ];
    for input in cases {
        assert!(import_hive_style(input.as_bytes(), "hive-xmrig.json").is_err());
    }
}
