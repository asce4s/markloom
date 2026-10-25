//! The open workspace, its editor, and the cursor the preview follows.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use gpui_kit::component::input::{EditorState, InputEvent, Position};
use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, Subscription, Window};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::text::{self, Snippet, TextStats};
use crate::workspace::{self, DiskChange, Workspace};

/// Something the rest of the window needs to hear about the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentEvent {
    TextChanged,
    CursorLineChanged(u32),
    Saved,
    Reloaded,
    Conflict,
    Error(String),
}

/// Workspace, editor, watcher, and the last cursor line that was broadcast.
pub struct Document {
    workspace: Workspace,
    editor: Entity<EditorState>,
    stats: TextStats,
    cursor: (u32, u32),
    last_line: u32,
    generation: u64,
    watch_rx: Option<mpsc::Receiver<notify::Event>>,
    _watcher: Option<RecommendedWatcher>,
    watching: bool,
    _editor_events: Subscription,
    _editor_cursor: Subscription,
}

impl EventEmitter<DocumentEvent> for Document {}

impl Document {
    /// Open a folder, or a Markdown file's folder with that file selected.
    pub fn open(path: &Path, window: &mut Window, cx: &mut Context<Self>) -> Result<Self, String> {
        let (workspace, _) = Workspace::open_path(path)?;
        Self::from_workspace(workspace, window, cx)
    }

    /// Editor the centre column renders.
    pub fn editor(&self) -> &Entity<EditorState> {
        &self.editor
    }

    /// Open folder and buffers.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Active buffer text. Empty when nothing is open.
    pub fn text(&self) -> &str {
        self.workspace
            .active()
            .map(|buffer| buffer.text.as_str())
            .unwrap_or("")
    }

    /// Word and line counts for the status bar.
    pub fn stats(&self) -> TextStats {
        self.stats
    }

    /// Cursor as 1-based `(line, column)`.
    pub fn cursor(&self) -> (u32, u32) {
        self.cursor
    }

    /// Bumped on every text change. Preview uses its own generation.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// False when the folder could not be watched.
    pub fn watching(&self) -> bool {
        self.watching
    }

    /// Replace the editor text and mark the buffer dirty.
    ///
    /// `EditorState::set_value` does not emit [`InputEvent::Change`], so this
    /// is the path tests and reloads use. Typing still arrives through that event.
    pub fn set_value(
        &mut self,
        text: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.load_text(text.into(), window, cx);
    }

    /// Move the caret. `line` and `col` are 1-based.
    pub fn set_cursor(&mut self, line: u32, col: u32, window: &mut Window, cx: &mut Context<Self>) {
        let position = Position::new(line.saturating_sub(1), col.saturating_sub(1));
        self.editor.update(cx, |editor, cx| {
            editor.set_cursor_position(position, window, cx);
        });
        self.note_cursor(position, cx);
    }

    /// Write the active buffer. An own save does not reload the editor.
    pub fn save(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.workspace.save_active()?;
        cx.emit(DocumentEvent::Saved);
        Ok(())
    }

    /// Write every dirty buffer. The editor's cursor stays where it is.
    pub fn save_dirty(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.workspace.save_dirty()?;
        cx.emit(DocumentEvent::Saved);
        cx.notify();
        Ok(())
    }

    /// Open one workspace file into the editor.
    pub fn open_file(
        &mut self,
        relative: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Err(error) = self.workspace.open_file(relative) {
            cx.emit(DocumentEvent::Error(error.clone()));
            return Err(error);
        }
        let text = self.text().to_string();
        self.load_text(text, window, cx);
        Ok(())
    }

    /// Rename a workspace file and keep the editor on the same buffer.
    pub fn rename_file(
        &mut self,
        from: &Path,
        to: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let active = self
            .workspace
            .active()
            .map(|buffer| buffer.relative.clone());
        if let Err(error) = self.workspace.rename(from, to) {
            cx.emit(DocumentEvent::Error(error.clone()));
            return Err(error);
        }
        if active.as_deref() == Some(from) || active.as_deref() == Some(to) {
            let text = self.text().to_string();
            self.load_text(text, window, cx);
        }
        cx.notify();
        Ok(())
    }

    /// Create a Markdown file and show it.
    pub fn create_file(
        &mut self,
        relative: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.workspace.create_file(relative)?;
        let text = self.text().to_string();
        self.load_text(text, window, cx);
        Ok(())
    }

    /// Move a file to the trash and show whatever buffer remains.
    pub fn delete_file(
        &mut self,
        relative: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.workspace.delete(relative, &workspace::SystemTrash)?;
        let text = self.text().to_string();
        self.load_text(text, window, cx);
        Ok(())
    }

    /// Open another folder or file in this same document.
    pub fn open_path(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (workspace, _) = Workspace::open_path(path)?;
        self.install_workspace(workspace, window, cx)
    }

    /// Show one already-open buffer.
    pub fn select_buffer(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.workspace.select(index)?;
        let text = self.text().to_string();
        self.load_text(text, window, cx);
        Ok(())
    }

    /// Insert a snippet at the caret and select its placeholder.
    pub fn insert_snippet(&mut self, kind: Snippet, window: &mut Window, cx: &mut Context<Self>) {
        let (body, placeholder) = text::snippet(kind);
        let cursor = self.editor.read(cx).cursor();
        self.editor.update(cx, |editor, cx| {
            editor.insert(body.as_str(), window, cx);
            let start = cursor + placeholder.start;
            let end = cursor + placeholder.end;
            editor.set_selected_range(start..end, cx);
        });
        // `EditorState::insert` does not emit `InputEvent::Change`, so the
        // workspace would otherwise keep the text from before the snippet.
        let text = self.editor.read(cx).value().to_string();
        self.apply_typed(text, cx);
    }

    /// Apply watcher events. Call this from the window; it needs the window
    /// only when a clean buffer was reloaded from disk.
    pub fn drain_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rx) = &self.watch_rx else {
            return;
        };
        let events: Vec<_> = rx.try_iter().collect();
        let active_before = self
            .workspace
            .active()
            .map(|buffer| buffer.relative.clone());
        let mut reload_editor = false;
        for event in events {
            let kind = event.kind;
            for path in event.paths {
                let Some(disk) = workspace::disk_event(kind, path) else {
                    continue;
                };
                self.ingest(disk, &active_before, &mut reload_editor, cx);
            }
        }
        self.finish_watch(active_before, reload_editor, window, cx);
    }

    /// Apply one disk event the same way [`Self::drain_watch`] does.
    pub fn apply_disk_event(
        &mut self,
        event: workspace::DiskEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_before = self
            .workspace
            .active()
            .map(|buffer| buffer.relative.clone());
        let mut reload_editor = false;
        self.ingest(event, &active_before, &mut reload_editor, cx);
        self.finish_watch(active_before, reload_editor, window, cx);
    }

    fn ingest(
        &mut self,
        disk: workspace::DiskEvent,
        active_before: &Option<PathBuf>,
        reload_editor: &mut bool,
        cx: &mut Context<Self>,
    ) {
        let relative = match &disk {
            workspace::DiskEvent::Created(path)
            | workspace::DiskEvent::Modified(path)
            | workspace::DiskEvent::Removed(path) => self.workspace.relative_of(path),
        };
        match self.workspace.apply_disk(disk) {
            Ok(DiskChange::None) => {}
            Ok(DiskChange::Reloaded) => {
                if relative.as_ref() == active_before.as_ref() {
                    *reload_editor = true;
                    cx.emit(DocumentEvent::Reloaded);
                }
            }
            Ok(DiskChange::Conflict) => cx.emit(DocumentEvent::Conflict),
            Ok(DiskChange::Removed) => {
                cx.emit(DocumentEvent::Error("File removed".into()));
            }
            Ok(DiskChange::Rescanned) => cx.notify(),
            Err(error) => cx.emit(DocumentEvent::Error(error)),
        }
    }

    fn finish_watch(
        &mut self,
        active_before: Option<PathBuf>,
        reload_editor: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_after = self
            .workspace
            .active()
            .map(|buffer| buffer.relative.clone());
        if reload_editor || active_before != active_after {
            let text = self.text().to_string();
            self.load_text(text, window, cx);
        }
    }

    fn from_workspace(
        workspace: Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, String> {
        let text = workspace
            .active()
            .map(|buffer| buffer.text.clone())
            .unwrap_or_default();
        let editor = cx.new(|cx| {
            let mut editor = EditorState::new(window, cx)
                .line_number(true)
                .folding(true)
                .soft_wrap(true)
                .searchable(true);
            editor.set_highlighter("markdown", cx);
            editor.set_value(text.clone(), window, cx);
            editor
        });
        let position = editor.read(cx).cursor_position();
        let cursor = one_based(position);
        let (watch_rx, watcher) = watch(workspace.root());
        let watching = watch_rx.is_some();
        let editor_events = cx.subscribe(&editor, |this, editor, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = editor.read(cx).value().to_string();
                this.apply_typed(text, cx);
            }
        });
        let editor_cursor = cx.observe(&editor, |this, editor, cx| {
            this.note_cursor(editor.read(cx).cursor_position(), cx);
        });
        Ok(Self {
            stats: text::stats(&text),
            workspace,
            editor,
            cursor,
            last_line: cursor.0,
            generation: 0,
            watch_rx,
            _watcher: watcher,
            watching,
            _editor_events: editor_events,
            _editor_cursor: editor_cursor,
        })
    }

    fn install_workspace(
        &mut self,
        workspace: Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (watch_rx, watcher) = watch(workspace.root());
        self.watching = watch_rx.is_some();
        self.watch_rx = watch_rx;
        self._watcher = watcher;
        self.workspace = workspace;
        let text = self.text().to_string();
        self.load_text(text, window, cx);
        Ok(())
    }

    fn load_text(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.set_value(text.clone(), window, cx);
        });
        self.workspace.set_text(text.clone());
        self.stats = text::stats(&text);
        self.generation = self.generation.wrapping_add(1);
        self.note_cursor(self.editor.read(cx).cursor_position(), cx);
        cx.emit(DocumentEvent::TextChanged);
    }

    fn apply_typed(&mut self, text: String, cx: &mut Context<Self>) {
        self.workspace.set_text(text.clone());
        self.stats = text::stats(&text);
        self.generation = self.generation.wrapping_add(1);
        cx.emit(DocumentEvent::TextChanged);
    }

    fn note_cursor(&mut self, position: Position, cx: &mut Context<Self>) {
        let cursor = one_based(position);
        self.cursor = cursor;
        if cursor.0 != self.last_line {
            self.last_line = cursor.0;
            cx.emit(DocumentEvent::CursorLineChanged(cursor.0));
        }
    }
}

fn one_based(position: Position) -> (u32, u32) {
    (
        position.line.saturating_add(1),
        position.character.saturating_add(1),
    )
}

fn watch(
    root: &Path,
) -> (
    Option<mpsc::Receiver<notify::Event>>,
    Option<RecommendedWatcher>,
) {
    let (tx, rx) = mpsc::channel();
    let mut watcher =
        match notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
            if let Ok(event) = result {
                let _ = tx.send(event);
            }
        }) {
            Ok(watcher) => watcher,
            Err(_) => return (None, None),
        };
    if watcher.watch(root, RecursiveMode::Recursive).is_err() {
        return (None, None);
    }
    (Some(rx), Some(watcher))
}
