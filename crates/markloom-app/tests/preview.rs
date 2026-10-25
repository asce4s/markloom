use markloom_app::preview::{
    DEBOUNCE, ExportGate, PageCache, Zoom, pages_to_rasterize, pixels_per_pt, scale_bits,
};

#[test]
fn zoom_clamps_and_fits() {
    assert!(
        (pixels_per_pt(Zoom::Percent(100.0), 800.0, 600.0, 400.0, 600.0, 1.0) - 1.0).abs() < 0.001
    );
    assert!(
        (pixels_per_pt(Zoom::Percent(200.0), 800.0, 600.0, 400.0, 600.0, 2.0) - 4.0).abs() < 0.001
    );
    assert!((pixels_per_pt(Zoom::FitWidth, 200.0, 800.0, 100.0, 400.0, 1.0) - 2.0).abs() < 0.001);
    assert!((pixels_per_pt(Zoom::FitPage, 100.0, 100.0, 200.0, 400.0, 1.0) - 0.25).abs() < 0.001);
    assert_eq!(DEBOUNCE.as_millis(), 120);
    assert_eq!(scale_bits(2.0), 2000);
}

#[test]
fn raster_window_includes_neighbours() {
    assert_eq!(pages_to_rasterize(0, 0, 5), 0..2);
    assert_eq!(pages_to_rasterize(3, 4, 10), 2..6);
    assert_eq!(pages_to_rasterize(0, 0, 0), 0..0);
}

#[test]
fn cache_keeps_hash_and_drops_other_pages() {
    let mut cache = PageCache::default();
    cache.insert(0, 7, 2000, b"a".to_vec());
    cache.insert(1, 7, 2000, b"b".to_vec());
    assert_eq!(cache.get(0, 7, 2000), Some(b"a".as_slice()));
    assert_eq!(cache.get(0, 8, 2000), None);
    cache.retain_pages(|index| index == 0);
    assert!(cache.get(1, 7, 2000).is_none());
}

#[test]
fn stale_export_needs_recompile() {
    let mut gate = ExportGate::default();
    assert!(!gate.is_stale());
    gate.note_edit();
    assert!(gate.is_stale());
    gate.note_compile();
    assert!(!gate.is_stale());
    gate.note_edit();
    assert!(gate.is_stale());
}
