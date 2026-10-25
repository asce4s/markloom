//! Cached diagram rendering with a primary backend and an optional fallback.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use markloom_style::{DiagramStyle, MermaidTheme};

use crate::mmdc::MmdcBackend;
use crate::rust_backend::RustBackend;

/// A diagram renderer. Object-safe so the service can swap backends.
pub trait DiagramRenderer: Send + Sync {
    fn render_svg(&self, source: &str, style: &DiagramStyle) -> Result<String, DiagramError>;
}

/// Why a diagram did not become SVG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagramError {
    Syntax(String),
    Unsupported(String),
    Backend(String),
}

impl std::fmt::Display for DiagramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagramError::Syntax(message) => write!(f, "diagram syntax: {message}"),
            DiagramError::Unsupported(message) => write!(f, "diagram unsupported: {message}"),
            DiagramError::Backend(message) => write!(f, "diagram backend: {message}"),
        }
    }
}

impl std::error::Error for DiagramError {}

/// Renders diagrams and caches both successes and failures.
pub struct DiagramService {
    primary: Box<dyn DiagramRenderer>,
    fallback: Option<Box<dyn DiagramRenderer>>,
    cache: Mutex<HashMap<u64, Result<Arc<str>, DiagramError>>>,
}

impl DiagramService {
    pub fn new(
        primary: Box<dyn DiagramRenderer>,
        fallback: Option<Box<dyn DiagramRenderer>>,
    ) -> Self {
        Self {
            primary,
            fallback,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Rust renderer, plus `mmdc` when it is on `PATH`.
    pub fn with_defaults() -> Self {
        Self::new(
            Box::new(RustBackend),
            MmdcBackend::detect().map(|backend| Box::new(backend) as Box<dyn DiagramRenderer>),
        )
    }

    pub fn render(&self, source: &str, style: &DiagramStyle) -> Result<Arc<str>, DiagramError> {
        let key = cache_key(source, style);
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(hit) = cache.get(&key) {
            return hit.clone();
        }
        let rendered = self.render_uncached(source, style).map(Arc::from);
        cache.insert(key, rendered.clone());
        rendered
    }

    fn render_uncached(&self, source: &str, style: &DiagramStyle) -> Result<String, DiagramError> {
        match self.primary.render_svg(source, style) {
            Ok(svg) => Ok(svg),
            Err(primary_error) => match &self.fallback {
                Some(fallback) => fallback.render_svg(source, style).or(Err(primary_error)),
                None => Err(primary_error),
            },
        }
    }
}

fn cache_key(source: &str, style: &DiagramStyle) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    theme_tag(style.theme).hash(&mut hasher);
    match style.background {
        Some(color) => (color.r, color.g, color.b, color.a).hash(&mut hasher),
        None => 0u8.hash(&mut hasher),
    }
    hasher.finish()
}

fn theme_tag(theme: MermaidTheme) -> u8 {
    match theme {
        MermaidTheme::Default => 0,
        MermaidTheme::Neutral => 1,
        MermaidTheme::Dark => 2,
        MermaidTheme::Forest => 3,
    }
}
