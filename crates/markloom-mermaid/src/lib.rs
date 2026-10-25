//! Mermaid diagrams as SVG, with an optional `mmdc` fallback.

mod mmdc;
mod rust_backend;
mod service;

pub use mmdc::MmdcBackend;
pub use rust_backend::RustBackend;
pub use service::{DiagramError, DiagramRenderer, DiagramService};
