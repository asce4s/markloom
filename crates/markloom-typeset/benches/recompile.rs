//! One-character edits of a 20-page document: compile plus page 0 at 2 px/pt.
//!
//! `cargo bench -p markloom-typeset` prints the median and exits non-zero at
//! 150 ms or above.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use markloom_core::parse;
use markloom_mermaid::DiagramService;
use markloom_style::PresetRegistry;
use markloom_typeset::{Engine, EngineOptions};

fn main() {
    let mut markdown = twenty_page_markdown();
    let mut current = '0';
    let style = PresetRegistry::builtin()
        .resolve("github", &[])
        .expect("github preset");
    let mut engine = Engine::new(
        EngineOptions {
            system_fonts: false,
        },
        Arc::new(DiagramService::with_defaults()),
    );
    let base = Path::new(".");
    let warmup = engine
        .compile(&parse(&markdown), &style, base)
        .expect("warmup compile");
    let pages = warmup.page_count();
    if pages < 20 {
        eprintln!("fixture has {pages} pages, want at least 20");
        std::process::exit(1);
    }
    drop(warmup);

    let mut samples = Vec::with_capacity(11);
    for next in ['1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b'] {
        markdown = markdown.replacen(&format!("EDIT{current}"), &format!("EDIT{next}"), 1);
        current = next;
        let started = Instant::now();
        let compiled = engine
            .compile(&parse(&markdown), &style, base)
            .expect("recompile");
        let _bitmap = compiled.render_page(0, 2.0);
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(|left, right| left.partial_cmp(right).expect("finite sample"));
    let median = samples[samples.len() / 2];
    println!("pages {pages}");
    println!("median {median:.2} ms");
    if median >= 150.0 {
        eprintln!("median {median:.2} ms is not under 150 ms");
        std::process::exit(1);
    }
}

fn twenty_page_markdown() -> String {
    let mut markdown = String::from(
        "# Latency\n\nEDIT0 sits in the first paragraph so a one-character edit is local.\n\n",
    );
    markdown.push_str(
        "| Column | Kind | Notes |\n| --- | --- | --- |\n| alpha | text | row |\n| beta | code | row |\n| gamma | table | row |\n\n",
    );
    markdown.push_str("```rust\nfn main() {\n    println!(\"markloom\");\n}\n```\n\n");
    let sentence = "The quick brown fox jumps over the lazy dog while the preview recompiles a long document. ";
    for section in 1..=32 {
        markdown.push_str(&format!("## Section {section}\n\n"));
        for paragraph in 0..6 {
            markdown.push_str(&format!("Paragraph {section}.{paragraph}. "));
            for _ in 0..8 {
                markdown.push_str(sentence);
            }
            markdown.push_str("\n\n");
        }
    }
    markdown
}
