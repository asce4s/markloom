//! Files sidebar. It does not rebuild when the editor text changes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::list::ListItem;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::tree::{self, TreeItem, TreeState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement,
    Render, Styled, Subscription, Window, div, px,
};

use crate::document::{Document, DocumentEvent};
use crate::probe;

pub struct FilesColumn {
    document: Entity<Document>,
    tree: Entity<TreeState>,
    paths: Vec<PathBuf>,
    _events: Subscription,
}

impl FilesColumn {
    pub fn new(document: Entity<Document>, cx: &mut Context<Self>) -> Self {
        let tree = cx.new(|cx| TreeState::new(cx));
        let events = cx.subscribe(&document, |_, _, event: &DocumentEvent, cx| match event {
            DocumentEvent::TextChanged | DocumentEvent::CursorLineChanged(_) => {}
            _ => cx.notify(),
        });
        Self {
            document,
            tree,
            paths: Vec::new(),
            _events: events,
        }
    }

    fn sync_tree(&mut self, cx: &mut Context<Self>) {
        let paths = self.document.read(cx).workspace().files().to_vec();
        if paths == self.paths {
            return;
        }
        self.paths = paths;
        let items = items_under(&self.paths, Path::new(""));
        self.tree.update(cx, |tree, cx| tree.set_items(items, cx));
    }
}

impl Render for FilesColumn {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_tree(cx);
        let watching = self.document.read(cx).watching();
        let document = self.document.clone();
        let open = document.clone();
        v_flex()
            .id("files")
            .on_prepaint(|bounds, _, _| probe::record("files", bounds))
            .size_full()
            .bg(cx.theme().background)
            .when(!watching, |column| {
                column.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("File watching unavailable"),
                )
            })
            .child(
                div().flex_1().min_h(px(0.)).child(
                    tree::tree(&self.tree, move |_, entry, _, _, _| {
                        let id = entry.item().id.clone();
                        let open = open.clone();
                        ListItem::new(id.clone())
                            .child(entry.item().label.clone())
                            .on_click(move |_, window, cx| {
                                if id.ends_with('/') {
                                    return;
                                }
                                let path = PathBuf::from(id.as_ref());
                                open.update(cx, |document, cx| {
                                    let _ = document.open_file(&path, window, cx);
                                });
                            })
                    })
                    .context_menu(move |_, entry, menu, _, _| {
                        file_menu(menu, entry.item().id.as_ref(), document.clone())
                    }),
                ),
            )
    }
}

/// Context-menu labels for one tree id. Folders have none.
pub fn file_action_labels(id: &str) -> &'static [&'static str] {
    if id.ends_with('/') {
        &[]
    } else {
        &["Rename", "Delete"]
    }
}

fn file_menu(menu: PopupMenu, id: &str, document: Entity<Document>) -> PopupMenu {
    let mut menu = menu;
    for label in file_action_labels(id) {
        let path = PathBuf::from(id);
        let document = document.clone();
        menu = match *label {
            "Rename" => menu.item(PopupMenuItem::new("Rename").on_click(move |_, window, cx| {
                ask_rename(path.clone(), document.clone(), window, cx);
            })),
            "Delete" => menu.item(PopupMenuItem::new("Delete").on_click(move |_, window, cx| {
                let path = path.clone();
                document.update(cx, |document, cx| {
                    let _ = document.delete_file(&path, window, cx);
                });
            })),
            _ => menu,
        };
    }
    menu
}

fn ask_rename(from: PathBuf, document: Entity<Document>, window: &mut Window, cx: &mut App) {
    let current = from
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = cx.new(|cx| InputState::new(window, cx).default_value(current));
    window.open_dialog(cx, move |dialog, _, _| {
        let field = name.clone();
        let typed = name.clone();
        let document = document.clone();
        let from = from.clone();
        dialog
            .title("Rename")
            .child(
                div()
                    .id("rename-name")
                    .on_prepaint(|bounds, _, _| probe::record("rename-name", bounds))
                    .child(Input::new(&field)),
            )
            .footer(
                DialogFooter::new().child(
                    div()
                        .id("confirm-rename")
                        .on_prepaint(|bounds, _, _| probe::record("confirm-rename", bounds))
                        .child(
                            Button::new("confirm-rename-button")
                                .label("Rename")
                                .primary()
                                .on_click(move |_, window, cx| {
                                    let text = typed.read(cx).value().trim().to_string();
                                    let to = rename_target(&from, &text);
                                    document.update(cx, |document, cx| {
                                        let _ = document.rename_file(&from, &to, window, cx);
                                    });
                                    window.close_dialog(cx);
                                }),
                        ),
                ),
            )
    });
}

fn rename_target(from: &Path, text: &str) -> PathBuf {
    if text.contains('/') || text.contains('\\') {
        PathBuf::from(text)
    } else {
        from.parent().unwrap_or(Path::new("")).join(text)
    }
}

fn items_under(files: &[PathBuf], prefix: &Path) -> Vec<TreeItem> {
    let mut dirs = BTreeSet::new();
    let mut leaves = BTreeSet::new();
    for file in files {
        let rest = file.strip_prefix(prefix).unwrap_or(file.as_path());
        let mut parts = rest.components();
        let Some(first) = parts.next() else {
            continue;
        };
        let name = first.as_os_str().to_os_string();
        if parts.next().is_some() {
            dirs.insert(name);
        } else {
            leaves.insert(name);
        }
    }
    let mut items = Vec::new();
    for dir in dirs {
        let path = prefix.join(&dir);
        let id = format!("{}/", path.display());
        let label = dir.to_string_lossy().into_owned();
        items.push(
            TreeItem::new(id, label)
                .children(items_under(files, &path))
                .expanded(true),
        );
    }
    for leaf in leaves {
        let path = prefix.join(&leaf);
        let label = leaf.to_string_lossy().into_owned();
        items.push(TreeItem::new(path.display().to_string(), label));
    }
    items
}
