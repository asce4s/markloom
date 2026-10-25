//! In-process renderer. `mermaid-rs-renderer` won the Milestone 0 comparison.

use markloom_style::{DiagramStyle, MermaidTheme};
use mermaid_rs_renderer::{RenderOptions, Theme};

use crate::service::{DiagramError, DiagramRenderer};

/// Renders with the crate chosen in the spike findings.
pub struct RustBackend;

impl DiagramRenderer for RustBackend {
    fn render_svg(&self, source: &str, style: &DiagramStyle) -> Result<String, DiagramError> {
        let mut theme = match style.theme {
            MermaidTheme::Default => Theme::mermaid_default(),
            MermaidTheme::Neutral => Theme::neutral(),
            MermaidTheme::Dark => Theme::dark(),
            MermaidTheme::Forest => Theme::forest(),
        };
        // Embedded Typst fonts include DejaVu Sans Mono, not the classic sans stack.
        theme.font_family = "DejaVu Sans Mono".to_string();
        if let Some(color) = style.background {
            theme.background = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
        }
        let options = RenderOptions {
            theme,
            layout: mermaid_rs_renderer::LayoutConfig::default(),
        };
        mermaid_rs_renderer::render_with_options(source, options)
            .map_err(|error| classify(error.to_string()))
    }
}

fn classify(message: String) -> DiagramError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("unsupported") {
        DiagramError::Unsupported(message)
    } else {
        DiagramError::Syntax(message)
    }
}
