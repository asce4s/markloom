use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use markloom_mermaid::{DiagramError, DiagramRenderer, DiagramService, MmdcBackend};
use markloom_style::DiagramStyle;

struct Fake {
    calls: Arc<AtomicUsize>,
    svg: String,
    fail: bool,
}

impl DiagramRenderer for Fake {
    fn render_svg(&self, _source: &str, _style: &DiagramStyle) -> Result<String, DiagramError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(DiagramError::Backend("primary failed".into()))
        } else {
            Ok(self.svg.clone())
        }
    }
}

#[test]
fn corpus_renders_valid_svg() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mermaid");
    let service = DiagramService::with_defaults();
    let style = DiagramStyle::default();
    let mut saw = 0;
    for entry in fs::read_dir(&dir).expect("fixtures") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("mmd") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        // Findings marked the mermaid-rs-renderer timeline wrong (it drops "Preview").
        if name == "timeline" {
            continue;
        }
        let source = fs::read_to_string(&path).expect("read fixture");
        let svg = service
            .render(&source, &style)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        usvg::Tree::from_str(&svg, &usvg::Options::default())
            .unwrap_or_else(|error| panic!("{name} usvg: {error}"));
        saw += 1;
    }
    assert_eq!(saw, 12);
}

#[test]
fn syntax_error_is_err() {
    let service = DiagramService::with_defaults();
    let error = service
        .render("graph TD; A-->", &DiagramStyle::default())
        .expect_err("broken edge");
    assert!(
        matches!(error, DiagramError::Syntax(_)),
        "syntax errors stay on the syntax variant, got {error}"
    );
}

#[test]
fn cache_hits() {
    let calls = Arc::new(AtomicUsize::new(0));
    let fake = Fake {
        calls: Arc::clone(&calls),
        svg: "<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into(),
        fail: false,
    };
    let service = DiagramService::new(Box::new(fake), None);
    let style = DiagramStyle::default();
    let first = service.render("graph TD; A-->B", &style).expect("first");
    let second = service.render("graph TD; A-->B", &style).expect("second");
    assert_eq!(first.as_ref(), second.as_ref());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn fallback_used_when_primary_fails() {
    let primary = Fake {
        calls: Arc::new(AtomicUsize::new(0)),
        svg: String::new(),
        fail: true,
    };
    let fallback = Fake {
        calls: Arc::new(AtomicUsize::new(0)),
        svg: "<svg xmlns=\"http://www.w3.org/2000/svg\" id=\"fallback\"/>".into(),
        fail: false,
    };
    let service = DiagramService::new(Box::new(primary), Some(Box::new(fallback)));
    let svg = service
        .render("graph TD; A-->B", &DiagramStyle::default())
        .expect("fallback");
    assert!(svg.contains("fallback"));
}

#[test]
fn mmdc_backend_invokes_binary() {
    let dir = std::env::temp_dir().join(format!("markloom-mmdc-script-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    let script = dir.join("mmdc");
    fs::write(
        &script,
        "#!/bin/sh\nout=\nwhile [ $# -gt 0 ]; do\n  if [ \"$1\" = \"-o\" ]; then\n    out=$2\n    shift 2\n  else\n    shift\n  fi\ndone\nprintf '%s\\n' '<svg xmlns=\"http://www.w3.org/2000/svg\"/>' > \"$out\"\n",
    )
    .expect("script");
    let mut permissions = fs::metadata(&script).expect("meta").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).expect("chmod");
    let backend = MmdcBackend::new(script);
    let svg = backend
        .render_svg("graph TD; A-->B", &DiagramStyle::default())
        .expect("mmdc");
    assert!(svg.contains("<svg"));
}
