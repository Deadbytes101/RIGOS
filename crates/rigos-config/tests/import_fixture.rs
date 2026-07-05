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
