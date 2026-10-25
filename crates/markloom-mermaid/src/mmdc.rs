//! Optional `mmdc` (Mermaid CLI) backend.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use markloom_style::{DiagramStyle, MermaidTheme};

use crate::service::{DiagramError, DiagramRenderer};

/// Runs an `mmdc` binary. [`MmdcBackend::detect`] looks it up on `PATH`.
pub struct MmdcBackend {
    bin: PathBuf,
}

impl MmdcBackend {
    pub fn new(bin: PathBuf) -> Self {
        Self { bin }
    }

    /// The first `mmdc` executable on `PATH`, if any.
    pub fn detect() -> Option<Self> {
        let path = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("mmdc");
            if candidate.is_file() {
                return Some(Self::new(candidate));
            }
        }
        None
    }
}

impl DiagramRenderer for MmdcBackend {
    fn render_svg(&self, source: &str, style: &DiagramStyle) -> Result<String, DiagramError> {
        let dir = std::env::temp_dir().join(format!(
            "markloom-mmdc-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).map_err(|error| DiagramError::Backend(error.to_string()))?;
        let input = dir.join("diagram.mmd");
        let output = dir.join("diagram.svg");
        fs::write(&input, source).map_err(|error| DiagramError::Backend(error.to_string()))?;
        let background = match style.background {
            Some(color) => format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b),
            None => "transparent".to_string(),
        };
        let mut child = Command::new(&self.bin)
            .arg("-i")
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .arg("-t")
            .arg(theme_name(style.theme))
            .arg("-b")
            .arg(background)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| DiagramError::Backend(error.to_string()))?;
        let status = wait_timeout(&mut child, Duration::from_secs(15))?;
        if !status.success() {
            let _ = fs::remove_dir_all(&dir);
            return Err(DiagramError::Backend(format!("mmdc exited with {status}")));
        }
        let svg = fs::read_to_string(&output).map_err(|error| {
            let _ = fs::remove_dir_all(&dir);
            DiagramError::Backend(error.to_string())
        })?;
        let _ = fs::remove_dir_all(&dir);
        Ok(svg)
    }
}

fn theme_name(theme: MermaidTheme) -> &'static str {
    match theme {
        MermaidTheme::Default => "default",
        MermaidTheme::Neutral => "neutral",
        MermaidTheme::Dark => "dark",
        MermaidTheme::Forest => "forest",
    }
}

fn wait_timeout(
    child: &mut std::process::Child,
    limit: Duration,
) -> Result<std::process::ExitStatus, DiagramError> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(DiagramError::Backend("mmdc timed out after 15s".into()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => return Err(DiagramError::Backend(error.to_string())),
        }
    }
}
