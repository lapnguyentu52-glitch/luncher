#[test]
fn f64_timestamp_survives_json_roundtrip() {
    // Regression: serde_json cần feature `float_roundtrip` — parse mặc định
    // làm timestamp trôi precision (.0049865 → .0049863) → flaky PartialEq.
    // 1000 giá trị as_secs_f64 khác nhau phải roundtrip qua file đúng bit.
    let dir = std::env::temp_dir().join(format!("repro-rt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let svc = antares_storage::StorageService::new(&dir);
    let base = 1_790_951_514u64;
    for i in 0..1000u64 {
        let nanos = (i * 997_303) % 1_000_000_000;
        let dur = std::time::Duration::new(base, nanos as u32);
        let v = dur.as_secs_f64();
        let handle = svc.scoped(antares_storage::ScopedRoot::AppData);
        handle.write_json_atomic("t.json", &v).unwrap();
        let back: f64 = handle.read_json("t.json").unwrap();
        assert_eq!(v, back, "nanos={nanos} written={} back={:?}", format!("{v}"), back);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
