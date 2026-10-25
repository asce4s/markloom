fn main() {
    let report = preview_latency::compile_and_render();
    println!("{report}");
    if !report.passed {
        std::process::exit(1);
    }
}
