//! Folder of Markdown files, open buffers, and disk events.

use std::path::{Component, Path, PathBuf};

use notify::EventKind;

/// One open file. Dirty means the buffer differs from the last read or save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buffer {
    pub relative: PathBuf,
    pub text: String,
    pub disk: String,
    pub conflict: bool,
    pub missing: bool,
}

impl Buffer {
    /// True when the editor text is not what was last loaded or saved.
    pub fn is_dirty(&self) -> bool {
        self.text != self.disk
    }
}

/// What a watcher event did to the open buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskChange {
    None,
    Reloaded,
    Conflict,
    Removed,
    Rescanned,
}

/// Something the file watcher observed inside the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
}

/// Map a notify event into a workspace event. Other kinds are ignored.
pub fn disk_event(kind: EventKind, path: PathBuf) -> Option<DiskEvent> {
    match kind {
        EventKind::Create(_) => Some(DiskEvent::Created(path)),
        EventKind::Modify(_) => Some(DiskEvent::Modified(path)),
        EventKind::Remove(_) => Some(DiskEvent::Removed(path)),
        _ => None,
    }
}

/// Moves a path to the trash. Tests substitute their own.
pub trait FileTrash {
    /// Trash `path`. The file should leave the workspace folder.
    fn trash(&self, path: &Path) -> Result<(), String>;
}

/// The platform trash.
pub struct SystemTrash;

impl FileTrash for SystemTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        trash::delete(path).map_err(|error| error.to_string())
    }
}

/// An open folder filtered to Markdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    root: PathBuf,
    files: Vec<PathBuf>,
    buffers: Vec<Buffer>,
    active: Option<usize>,
}

impl Workspace {
    /// Scan `root` for `.md` and `.markdown` files.
    pub fn open(root: &Path) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("open {}: {error}", root.display()))?;
        let mut workspace = Self {
            root,
            files: Vec::new(),
            buffers: Vec::new(),
            active: None,
        };
        workspace.rescan()?;
        Ok(workspace)
    }

    /// Absolute workspace root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Relative Markdown paths, sorted.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Open buffers.
    pub fn buffers(&self) -> &[Buffer] {
        &self.buffers
    }

    /// Index of the active buffer.
    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    /// The active buffer.
    pub fn active(&self) -> Option<&Buffer> {
        self.active.and_then(|index| self.buffers.get(index))
    }

    /// Replace the active buffer text.
    pub fn set_text(&mut self, text: impl Into<String>) {
        if let Some(buffer) = self.active.and_then(|index| self.buffers.get_mut(index)) {
            buffer.text = text.into();
        }
    }

    /// Select an existing buffer or read it from disk.
    pub fn open_file(&mut self, relative: &Path) -> Result<(), String> {
        self.check_relative(relative)?;
        if let Some(index) = self
            .buffers
            .iter()
            .position(|buffer| buffer.relative == relative)
        {
            self.active = Some(index);
            return Ok(());
        }
        let absolute = self.root.join(relative);
        let bytes = std::fs::read(&absolute)
            .map_err(|error| format!("read {}: {error}", absolute.display()))?;
        let text = String::from_utf8(bytes)
            .map_err(|_| format!("{} is not UTF-8 text", relative.display()))?;
        self.buffers.push(Buffer {
            relative: relative.to_path_buf(),
            text: text.clone(),
            disk: text,
            conflict: false,
            missing: false,
        });
        self.active = Some(self.buffers.len() - 1);
        Ok(())
    }

    /// Select a buffer by index.
    pub fn select(&mut self, index: usize) -> Result<(), String> {
        if index >= self.buffers.len() {
            return Err(format!("no buffer {index}"));
        }
        self.active = Some(index);
        Ok(())
    }

    /// Write the active buffer and clear its dirty and conflict flags.
    pub fn save_active(&mut self) -> Result<(), String> {
        let Some(index) = self.active else {
            return Err("no file to save".into());
        };
        let buffer = self
            .buffers
            .get_mut(index)
            .ok_or_else(|| "no file to save".to_string())?;
        let absolute = self.root.join(&buffer.relative);
        if let Some(parent) = absolute.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        std::fs::write(&absolute, &buffer.text)
            .map_err(|error| format!("write {}: {error}", absolute.display()))?;
        buffer.disk.clone_from(&buffer.text);
        buffer.conflict = false;
        buffer.missing = false;
        self.rescan()
    }

    /// Create an empty Markdown file and open it.
    pub fn create_file(&mut self, relative: &Path) -> Result<(), String> {
        self.check_relative(relative)?;
        let absolute = self.root.join(relative);
        if absolute.exists() {
            return Err(format!("{} already exists", relative.display()));
        }
        if let Some(parent) = absolute.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        std::fs::write(&absolute, "")
            .map_err(|error| format!("write {}: {error}", absolute.display()))?;
        self.rescan()?;
        self.open_file(relative)
    }

    /// Rename or move a Markdown file inside the workspace.
    pub fn rename(&mut self, from: &Path, to: &Path) -> Result<(), String> {
        self.check_relative(from)?;
        self.check_relative(to)?;
        let source = self.root.join(from);
        let dest = self.root.join(to);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        std::fs::rename(&source, &dest).map_err(|error| format!("rename: {error}"))?;
        if let Some(buffer) = self
            .buffers
            .iter_mut()
            .find(|buffer| buffer.relative == from)
        {
            buffer.relative = to.to_path_buf();
        }
        self.rescan()
    }

    /// Trash a file and drop it from the tree and the open buffers.
    pub fn delete(&mut self, relative: &Path, trash: &impl FileTrash) -> Result<(), String> {
        self.check_relative(relative)?;
        let absolute = self.root.join(relative);
        trash.trash(&absolute)?;
        self.buffers.retain(|buffer| buffer.relative != relative);
        if self.active.is_some_and(|index| index >= self.buffers.len()) {
            self.active = self.buffers.len().checked_sub(1);
        }
        self.rescan()
    }

    /// Open a folder, or a file's parent folder with that file selected.
    pub fn open_path(path: &Path) -> Result<(Self, Option<PathBuf>), String> {
        let path = path
            .canonicalize()
            .map_err(|error| format!("open {}: {error}", path.display()))?;
        if path.is_dir() {
            return Ok((Self::open(&path)?, None));
        }
        let parent = path
            .parent()
            .ok_or_else(|| format!("open {}: no parent", path.display()))?;
        let mut workspace = Self::open(parent)?;
        let relative = path
            .strip_prefix(workspace.root())
            .map_err(|error| error.to_string())?
            .to_path_buf();
        workspace.open_file(&relative)?;
        Ok((workspace, Some(relative)))
    }

    /// Relative paths of buffers that differ from disk.
    pub fn has_unsaved(&self) -> Vec<PathBuf> {
        self.buffers
            .iter()
            .filter(|buffer| buffer.is_dirty())
            .map(|buffer| buffer.relative.clone())
            .collect()
    }

    /// Apply one watcher event. A dirty buffer is not overwritten.
    pub fn apply_disk(&mut self, event: DiskEvent) -> Result<DiskChange, String> {
        let relative = match &event {
            DiskEvent::Created(path) | DiskEvent::Modified(path) | DiskEvent::Removed(path) => {
                self.relative_of(path)
            }
        };
        let Some(relative) = relative else {
            self.rescan()?;
            return Ok(DiskChange::Rescanned);
        };
        if !is_markdown(&relative) {
            return Ok(DiskChange::None);
        }
        match event {
            DiskEvent::Created(_) => {
                self.rescan()?;
                Ok(DiskChange::Rescanned)
            }
            DiskEvent::Removed(_) => {
                let had_buffer = self
                    .buffers
                    .iter()
                    .any(|buffer| buffer.relative == relative);
                if let Some(buffer) = self
                    .buffers
                    .iter_mut()
                    .find(|buffer| buffer.relative == relative)
                {
                    if buffer.is_dirty() {
                        buffer.missing = true;
                    } else {
                        let relative = relative.clone();
                        self.buffers.retain(|buffer| buffer.relative != relative);
                        if self.active.is_some_and(|index| index >= self.buffers.len()) {
                            self.active = self.buffers.len().checked_sub(1);
                        }
                    }
                }
                self.rescan()?;
                Ok(if had_buffer {
                    DiskChange::Removed
                } else {
                    DiskChange::Rescanned
                })
            }
            DiskEvent::Modified(_) => {
                let absolute = self.root.join(&relative);
                let Ok(bytes) = std::fs::read(&absolute) else {
                    self.rescan()?;
                    return Ok(DiskChange::Rescanned);
                };
                let Ok(disk) = String::from_utf8(bytes) else {
                    return Ok(DiskChange::None);
                };
                let Some(buffer) = self
                    .buffers
                    .iter_mut()
                    .find(|buffer| buffer.relative == relative)
                else {
                    return Ok(DiskChange::None);
                };
                if disk == buffer.disk {
                    return Ok(DiskChange::None);
                }
                if buffer.is_dirty() {
                    buffer.conflict = true;
                    Ok(DiskChange::Conflict)
                } else {
                    buffer.text.clone_from(&disk);
                    buffer.disk = disk;
                    buffer.conflict = false;
                    Ok(DiskChange::Reloaded)
                }
            }
        }
    }

    fn rescan(&mut self) -> Result<(), String> {
        let mut files = Vec::new();
        walk(&self.root, &self.root, &mut files)?;
        files.sort();
        self.files = files;
        Ok(())
    }

    fn check_relative(&self, path: &Path) -> Result<(), String> {
        if path.as_os_str().is_empty()
            || path.is_absolute()
            || path
                .components()
                .any(|component| component == Component::ParentDir)
            || !is_markdown(path)
        {
            return Err(format!(
                "{} is not a markdown path in the workspace",
                path.display()
            ));
        }
        Ok(())
    }

    fn relative_of(&self, path: &Path) -> Option<PathBuf> {
        path.strip_prefix(&self.root)
            .ok()
            .map(Path::to_path_buf)
            .or_else(|| {
                path.canonicalize().ok().and_then(|absolute| {
                    absolute
                        .strip_prefix(&self.root)
                        .ok()
                        .map(Path::to_path_buf)
                })
            })
    }
}

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("md" | "markdown")
    )
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|error| format!("read {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read {}: {error}", dir.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if is_markdown(&path)
            && let Ok(relative) = path.strip_prefix(root)
        {
            out.push(relative.to_path_buf());
        }
    }
    Ok(())
}
