use markloom_app::preview_pipeline::{
    DEBOUNCE, PageKey, Zoom, pages_to_rasterize, pixels_per_pt, plan_raster, scale_bits, to_bgra,
    visible_span, zoom_in, zoom_out,
};

fn key(index: usize, hash: u128, scale_bits: u32) -> PageKey {
    PageKey {
        index,
        hash,
        scale_bits,
    }
}

#[test]
fn zoom_steps() {
    assert_eq!(zoom_in(Zoom::Percent(100.0), 0.0), Zoom::Percent(110.0));
    assert_eq!(zoom_in(Zoom::Percent(200.0), 0.0), Zoom::Percent(200.0));
    assert_eq!(zoom_out(Zoom::Percent(50.0), 0.0), Zoom::Percent(50.0));
    assert_eq!(zoom_in(Zoom::FitWidth, 83.0), Zoom::Percent(90.0));
    assert_eq!(zoom_out(Zoom::FitWidth, 83.0), Zoom::Percent(75.0));
}

#[test]
fn first_paint_draws_visible_plus_one() {
    let hashes = vec![1u128; 5];
    let plan = plan_raster(&[], &hashes, (2, 2), 1000);
    let drawn: Vec<usize> = plan.draw.iter().map(|page| page.index).collect();
    assert_eq!(drawn, vec![1, 2, 3]);
    assert!(plan.show.is_empty());
    assert!(plan.drop.is_empty());
}

#[test]
fn edit_keeps_old_until_new() {
    let old = key(2, 10, 1000);
    let hashes = vec![1, 1, 20, 1, 1];
    let plan = plan_raster(&[old], &hashes, (2, 2), 1000);
    assert!(
        plan.draw
            .iter()
            .any(|page| page.index == 2 && page.hash == 20)
    );
    let shown = plan
        .show
        .iter()
        .find(|(index, _)| *index == 2)
        .expect("page 2");
    assert_eq!(shown.1, old);
    assert!(plan.drop.is_empty());
}

#[test]
fn new_image_drops_old() {
    let old = key(2, 10, 1000);
    let new = key(2, 20, 1000);
    let hashes = vec![1, 1, 20, 1, 1];
    let plan = plan_raster(&[old, new], &hashes, (2, 2), 1000);
    assert!(plan.drop.contains(&old));
    assert!(!plan.drop.contains(&new));
    let shown = plan
        .show
        .iter()
        .find(|(index, _)| *index == 2)
        .expect("page 2");
    assert_eq!(shown.1, new);
}

#[test]
fn scale_change_shows_old_scaled() {
    let old = key(2, 10, 1000);
    let hashes = vec![1, 1, 10, 1, 1];
    let plan = plan_raster(&[old], &hashes, (2, 2), 2000);
    let shown = plan
        .show
        .iter()
        .find(|(index, _)| *index == 2)
        .expect("page 2");
    assert_eq!(shown.1, old);
    assert!(
        plan.draw
            .iter()
            .any(|page| { page.index == 2 && page.hash == 10 && page.scale_bits == 2000 })
    );
    assert!(!plan.drop.contains(&old));
}

#[test]
fn scroll_drops_far_pages() {
    let hashes = vec![1u128; 20];
    let held = key(0, 1, 1000);
    let plan = plan_raster(&[held], &hashes, (10, 10), 1000);
    assert!(plan.drop.contains(&held));
}

#[test]
fn large_document_cache_is_bounded() {
    let hashes = vec![1u128; 400];
    let held: Vec<PageKey> = (0..400).map(|index| key(index, 1, 1000)).collect();
    let plan = plan_raster(&held, &hashes, (200, 201), 1000);
    let kept = held.len() - plan.drop.len();
    assert!(kept <= 8, "kept {kept}");
}

#[test]
fn bgra_swaps_channels() {
    assert_eq!(to_bgra(&[1, 2, 3, 4]), vec![3, 2, 1, 4]);
}

#[test]
fn zoom_clamps_and_fits() {
    assert!(
        (pixels_per_pt(Zoom::Percent(100.0), 800.0, 600.0, 400.0, 600.0, 1.0) - 1.0).abs() < 0.001
    );
    assert_eq!(DEBOUNCE.as_millis(), 120);
    assert_eq!(scale_bits(2.0), 2000);
    assert_eq!(pages_to_rasterize(3, 4, 10), 2..6);
    assert_eq!(visible_span(75.0, 50.0, 50.0, 10), (1, 2));
}
