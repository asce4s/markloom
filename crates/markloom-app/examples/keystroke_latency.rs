//! Dev-profile cost of one keystroke's preview work.
//!
//! Builds an engine with system fonts, then times 11 one-character edits of
//! the showcase: compile, `plan_raster`, draw of up to three pages, and
//! `to_bgra`. Prints the median and exits non-zero when it is at least 150 ms.

use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use markloom_app::preview_pipeline::{plan_raster, scale_bits, to_bgra};
use markloom_core::parse;
use markloom_mermaid::DiagramService;
use markloom_style::PresetRegistry;
use markloom_typeset::{Engine, EngineOptions};

fn main() {
    let showcase =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/showcase/index.md");
    let base = showcase.parent().expect("showcase directory").to_path_buf();
    let mut text = std::fs::read_to_string(&showcase).unwrap_or_else(|err| {
        eprintln!("read {}: {err}", showcase.display());
        std::process::exit(1);
    });
    let style = PresetRegistry::builtin()
        .resolve("github", &[])
        .expect("github preset");
    let mut engine = Engine::new(
        EngineOptions { system_fonts: true },
        Arc::new(DiagramService::with_defaults()),
    );

    let mut samples = Vec::with_capacity(11);
    for index in 0..11 {
        text.push(char::from(b'a' + u8::try_from(index).unwrap_or(0)));
        let started = Instant::now();
        let compiled = match engine.compile(&parse(&text), &style, &base) {
            Ok(compiled) => compiled,
            Err(err) => {
                eprintln!("compile: {err}");
                std::process::exit(1);
            }
        };
        let count = compiled.page_count();
        let hashes: Vec<u128> = (0..count).map(|page| compiled.page_hash(page)).collect();
        let last = count.saturating_sub(1).min(2);
        let plan = plan_raster(&[], &hashes, (0, last), scale_bits(1.0));
        for key in plan.draw.iter().take(3) {
            let bitmap = compiled.render_page_premultiplied(key.index, 1.0);
            black_box(to_bgra(&bitmap.rgba));
        }
        black_box(&plan);
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    samples.sort_by(|left, right| left.total_cmp(right));
    let median = samples[samples.len() / 2];
    println!("keystroke_latency median {median:.1} ms");
    if median >= 150.0 {
        std::process::exit(1);
    }
}
