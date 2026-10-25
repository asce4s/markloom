use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use markloom_app::settings::{self, AppTheme, Settings};
use markloom_app::workspace::{DiskEvent, FileTrash, Workspace};

struct MemoryTrash {
    calls: Mutex<Vec<PathBuf>>,
}

impl FileTrash for MemoryTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(path.to_path_buf());
        fs::remove_file(path).map_err(|error| error.to_string())
    }
}

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("markloom-ws-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn scan_open_save_and_rename() {
    let root = scratch("scan");
    fs::write(root.join("a.md"), "hello\n").unwrap();
    fs::write(root.join("notes.txt"), "skip").unwrap();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("sub/b.markdown"), "b\n").unwrap();
    let mut workspace = Workspace::open(&root).unwrap();
    assert_eq!(
        workspace.files(),
        &[PathBuf::from("a.md"), PathBuf::from("sub/b.markdown")]
    );
    workspace.open_file(Path::new("a.md")).unwrap();
    assert!(!workspace.active().unwrap().is_dirty());
    workspace.set_text("changed\n");
    assert!(workspace.active().unwrap().is_dirty());
    workspace.save_active().unwrap();
    assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "changed\n");
    assert!(!workspace.active().unwrap().is_dirty());
    workspace
        .rename(Path::new("a.md"), Path::new("sub/c.md"))
        .unwrap();
    assert!(workspace.files().contains(&PathBuf::from("sub/c.md")));
    assert_eq!(
        workspace.active().unwrap().relative,
        PathBuf::from("sub/c.md")
    );
}

#[test]
fn delete_calls_trash_and_drops_the_path() {
    let root = scratch("trash");
    fs::write(root.join("gone.md"), "x").unwrap();
    let mut workspace = Workspace::open(&root).unwrap();
    let trash = MemoryTrash {
        calls: Mutex::new(Vec::new()),
    };
    workspace.delete(Path::new("gone.md"), &trash).unwrap();
    assert!(workspace.files().is_empty());
    let calls = trash.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].ends_with("gone.md"));
}

#[test]
fn external_edit_of_dirty_buffer_is_a_conflict() {
    let root = scratch("conflict");
    fs::write(root.join("a.md"), "one\n").unwrap();
    let mut workspace = Workspace::open(&root).unwrap();
    workspace.open_file(Path::new("a.md")).unwrap();
    workspace.set_text("dirty\n");
    fs::write(root.join("a.md"), "disk\n").unwrap();
    workspace
        .apply_disk(DiskEvent::Modified(root.join("a.md")))
        .unwrap();
    let buffer = workspace.active().unwrap();
    assert!(buffer.conflict);
    assert_eq!(buffer.text, "dirty\n");
}

#[test]
fn external_edit_of_clean_buffer_reloads() {
    let root = scratch("reload");
    fs::write(root.join("a.md"), "one\n").unwrap();
    let mut workspace = Workspace::open(&root).unwrap();
    workspace.open_file(Path::new("a.md")).unwrap();
    fs::write(root.join("a.md"), "two\n").unwrap();
    workspace
        .apply_disk(DiskEvent::Modified(root.join("a.md")))
        .unwrap();
    assert_eq!(workspace.active().unwrap().text, "two\n");
    assert!(!workspace.active().unwrap().conflict);
}

#[test]
fn settings_round_trip_and_recent_cap() {
    let root = scratch("settings");
    let path = root.join("settings.toml");
    let mut settings = Settings {
        theme: AppTheme::Dark,
        autosave: true,
        ..Settings::default()
    };
    for index in 0..10 {
        settings::remember(&mut settings, &root.join(format!("w{index}")));
    }
    assert_eq!(settings.recent.len(), 8);
    assert!(settings::should_autosave(&settings, true));
    assert!(!settings::should_autosave(&settings, false));
    settings::save(&path, &settings).unwrap();
    let loaded = settings::load(&path);
    assert_eq!(loaded.theme, AppTheme::Dark);
    assert!(loaded.autosave);
    assert_eq!(loaded.recent.len(), 8);
    assert_eq!(
        settings::load(&root.join("missing.toml")).theme,
        AppTheme::Light
    );
}
