//! Reader state and the actions that change it.

use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    ops::Range,
    path::{Path, PathBuf},
};

use mdstar_core::document_ir::{
    BlockIr, DOCUMENT_IR_SCHEMA_VERSION, DocumentIr, InlineIr, parse_document_ir_with_diagnostics,
};
use volt_ui_kit::{
    declarative::{EditKey, TextSelection},
    graphics::Image,
    native::{
        UiSender, clipboard,
        file_dialog::{self, FileDialogOptions},
        files::{SavePolicy, save_text_file},
        load_image,
    },
};

use crate::{
    compose::{self, Slash},
    find::Find,
    render::{self, action},
    settings::{Settings, SidebarTab, Theme},
    workspace::{self, FileKind},
};

/// Sidebar rows built before the list reports what fits.
pub const SIDE_WINDOW: Range<usize> = 0..40;
/// Text files larger than this are not opened.
const MAX_TEXT: u64 = 16 * 1024 * 1024;
/// CSV rows shown as a table.
const MAX_CSV_ROWS: usize = 5000;

/// How the document area shows the current document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// The rendered document only.
    #[default]
    Preview,
    /// Source on the left, live preview on the right.
    Split,
    /// The source only.
    Source,
}

/// Work finished off the UI thread.
#[derive(Debug)]
pub enum Msg {
    /// Typing paused: save if no edit came after edit number `n`.
    Autosave(u64),
    /// Files changed on disk; `structural` when files appeared, went or
    /// were renamed.
    Changed { paths: Vec<PathBuf>, structural: bool },
    /// Changes settled: rescan the workspace if no later change came.
    Rescan(u64),
    /// The system asked to open these files or folders (Finder, the Dock).
    Open(Vec<PathBuf>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pane {
    Editor,
    Preview,
}
/// How long a synced scroll suppresses syncing back.
const SYNC_ECHO: std::time::Duration = std::time::Duration::from_millis(300);

/// How long typing must pause before an autosave.
const AUTOSAVE: std::time::Duration = std::time::Duration::from_millis(1200);

/// What a context menu was opened on.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuTarget {
    Row(usize),
    File(PathBuf),
    /// An outline entry, by its document row.
    Heading(usize),
    Tab(usize),
}

pub struct Doc {
    pub path: PathBuf,
    pub kind: FileKind,
    pub ir: DocumentIr,
    pub title: String,
    /// Words for Markdown, lines for code.
    pub words: usize,
    pub images: HashMap<String, Image>,
    /// The text being edited, and the text last read from or written to disk.
    pub source: String,
    pub saved: String,
}

impl Doc {
    pub fn units(&self) -> Vec<render::Unit<'_>> {
        render::units(&self.ir.blocks)
    }
    pub fn dirty(&self) -> bool {
        self.source != self.saved
    }
    /// Text documents can be edited; images cannot.
    pub fn editable(&self) -> bool {
        self.kind != FileKind::Image
    }
}

/// An open document tab. The active tab's document lives in [`App`]; the
/// others keep theirs here until they are shown again.
#[derive(Default)]
pub struct Tab {
    pub path: Option<PathBuf>,
    pub title: String,
    saved: Option<TabState>,
}

#[derive(Default)]
struct TabState {
    doc: Option<Doc>,
    error: Option<String>,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
    first_visible: usize,
}

#[derive(Default)]
pub struct App {
    pub settings: Settings,
    /// Where settings are saved; `None` keeps them in memory (tests).
    pub settings_path: Option<PathBuf>,
    /// The system appearance, as last reported to the theme callback.
    pub system_dark: Cell<bool>,
    pub show_settings: bool,
    /// The toolbar's quick Appearance popover.
    pub appearance_open: bool,
    /// The settings dropdown that is open.
    pub open_select: Option<&'static str>,
    /// Installed font families, loaded when settings first open.
    pub fonts: Vec<String>,

    /// Workspace folders, scanned, and the file tree's state.
    pub tree: Vec<workspace::Node>,
    pub expanded: HashSet<PathBuf>,
    pub filter: String,
    pub side_window: Range<usize>,

    pub tabs: Vec<Tab>,
    pub active: usize,

    // The active tab.
    pub doc: Option<Doc>,
    pub error: Option<String>,
    /// Document rows currently built, the first row on screen, and the row
    /// to jump to with the request number that triggers the jump.
    pub window: Range<usize>,
    pub first_visible: usize,
    pub jump: (usize, u32),
    /// Bumped per shown document so row measurements start fresh.
    pub generation: u64,
    pub back: Vec<PathBuf>,
    pub forward: Vec<PathBuf>,
    /// The code block whose Copy button was pressed last.
    pub copied: Option<String>,

    pub find: Option<Find>,
    pub menu: Option<(MenuTarget, [f32; 2])>,

    pub mode: Mode,
    /// The editor's share of a split view.
    pub split: f32,
    /// Delivers autosave timers; `None` in tests and snapshots.
    pub sender: Option<UiSender<Msg>>,
    /// Edits made so far, so a stale autosave timer does nothing.
    edits: u64,
    pub save_error: Option<String>,

    /// The editor's last reported selection, and a selection to apply:
    /// (start, end, request number).
    pub selection: Option<TextSelection>,
    pub select: (usize, usize, u32),
    pub slash: Option<Slash>,
    /// A `/` the reader dismissed with Escape, so it stays closed.
    slash_dismissed: Option<usize>,

    /// Text selected in the rendered document.
    pub selected_text: String,

    /// Which pane led the last scroll sync, and when: the other pane's echo
    /// of that scroll is not synced back.
    sync: Option<(Pane, std::time::Instant)>,
    /// Scroll the editor to this byte when the request number changes.
    pub editor_scroll: (usize, u32),

    /// Watches workspace folders and open files for outside changes.
    watcher: Option<notify::RecommendedWatcher>,
    watched: Vec<(PathBuf, bool)>,
    rescans: u64,
}

impl App {
    pub fn new(settings: Settings, settings_path: Option<PathBuf>) -> Self {
        let mut app = App {
            expanded: settings.expanded.iter().cloned().collect(),
            side_window: SIDE_WINDOW,
            split: 0.5,
            tabs: vec![Tab::default()],
            settings,
            settings_path,
            ..App::default()
        };
        app.rescan();
        app
    }

    // Settings

    pub fn theme(&self) -> Theme {
        self.settings.theme(self.system_dark.get())
    }

    /// The accent in use: a fixed colour, the system's, or `None` for the
    /// theme's own.
    pub fn accent(&self) -> Option<[f32; 4]> {
        use crate::settings::Accent;
        match self.settings.accent {
            Accent::Theme => None,
            #[cfg(target_os = "macos")]
            Accent::System => crate::macos::system_accent(),
            #[cfg(not(target_os = "macos"))]
            Accent::System => None,
            fixed => fixed.fixed(self.theme().dark()).and_then(volt_ui_kit::parse_hex),
        }
    }

    /// Change settings and save them.
    pub fn configure(&mut self, change: impl FnOnce(&mut Settings)) {
        change(&mut self.settings);
        self.save_settings();
    }

    fn save_settings(&mut self) {
        self.settings.expanded = self.expanded.iter().cloned().collect();
        self.settings.expanded.sort();
        if let Some(path) = &self.settings_path
            && let Err(e) = self.settings.save(path)
        {
            eprintln!("MD Star: could not save settings to {}: {e}", path.display());
        }
    }

    pub fn open_settings(&mut self) {
        if self.fonts.is_empty() {
            self.fonts = volt_ui_kit::native::font_families();
        }
        self.menu = None;
        self.show_settings = true;
    }

    pub fn zoom(&mut self, step: i32) {
        if step == 0 {
            self.configure(|s| s.font_size = crate::settings::DEFAULT_SIZE);
        } else {
            self.configure(|s| s.zoom(step));
        }
    }

    /// The sidebar divider moved: keep the width, saved with other settings.
    pub fn resize_sidebar(&mut self, width: f32) {
        self.settings.sidebar_width = width.round().clamp(
            *crate::settings::SIDEBAR_WIDTHS.start(),
            *crate::settings::SIDEBAR_WIDTHS.end(),
        );
    }

    /// Save settings changed without saving (the sidebar width), on quit.
    pub fn save_settings_now(&mut self) {
        self.save_settings();
    }

    pub fn toggle_sidebar(&mut self) {
        self.configure(|s| s.sidebar = !s.sidebar);
    }

    pub fn show_sidebar(&mut self, tab: SidebarTab) {
        self.filter.clear();
        self.side_window = SIDE_WINDOW;
        self.configure(|s| {
            s.sidebar = true;
            s.sidebar_tab = tab;
        });
    }

    // Documents

    /// Show `path` in the current tab.
    pub fn open(&mut self, path: PathBuf) {
        self.menu = None;
        self.ensure_tab();
        self.flush();
        self.save_error = None;
        match load(&path) {
            Ok(doc) => {
                self.window = 0..doc.units().len().min(30);
                self.tabs[self.active].title = file_name(&path);
                self.doc = Some(doc);
                self.error = None;
                self.settings.remember(&path);
                self.reveal_in_tree(&path);
                self.save_settings();
            }
            Err(e) => {
                self.doc = None;
                self.tabs[self.active].title = file_name(&path);
                self.error = Some(e);
            }
        }
        self.tabs[self.active].path = Some(path);
        self.sync_watches();
        self.copied = None;
        self.generation = self.generation.wrapping_add(1);
        self.first_visible = 0;
        self.jump = (0, self.jump.1.wrapping_add(1));
        self.recount();
    }

    pub fn path(&self) -> Option<&Path> {
        self.doc.as_ref().map(|d| d.path.as_path())
    }

    pub fn reload(&mut self) {
        let path = self.tabs.get(self.active).and_then(|t| t.path.clone());
        if let Some(path) = path {
            let first = self.first_visible;
            self.open(path);
            // Keep the reader's place.
            self.jump.0 = first;
            self.first_visible = first;
        }
    }

    /// Open… : a file opens in a tab, a folder becomes the workspace.
    pub fn choose(&mut self) {
        match file_dialog::open_file_or_folder(&FileDialogOptions::new("Open")) {
            Ok(Some(path)) if path.is_dir() => self.open_folder(path),
            Ok(Some(path)) => self.open_in_tab(path),
            Ok(None) => {}
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn choose_file(&mut self) {
        match file_dialog::open_file(&FileDialogOptions::new("Open Document")) {
            Ok(Some(path)) => self.open_in_tab(path),
            Ok(None) => {}
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Create a Markdown file and open it.
    pub fn new_document(&mut self) {
        let options = FileDialogOptions::new("New Markdown Document").suggested_name("Untitled.md");
        match file_dialog::save_file(&options) {
            Ok(Some(mut path)) => {
                if !workspace::is_markdown(&path) {
                    path.set_extension("md");
                }
                if !path.exists() {
                    let title = path
                        .file_stem()
                        .map_or_else(|| "Untitled".into(), |s| s.to_string_lossy().into_owned());
                    if let Err(e) = std::fs::write(&path, format!("# {title}\n\n")) {
                        self.error = Some(format!("{} could not be created: {e}", path.display()));
                        return;
                    }
                }
                self.rescan();
                self.open_in_tab(path);
            }
            Ok(None) => {}
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Open a document in the current tab, remembering the current one for
    /// Back. A document already open in another tab is shown there instead.
    pub fn visit(&mut self, path: PathBuf) {
        if let Some(i) = self.tab_with(&path)
            && i != self.active
        {
            self.switch_tab(i);
            return;
        }
        if let Some(current) = self.path()
            && current != path
        {
            self.back.push(current.to_path_buf());
            self.forward.clear();
        }
        self.open(path);
    }

    pub fn go_back(&mut self) {
        if let Some(path) = self.back.pop() {
            if let Some(current) = self.path() {
                self.forward.push(current.to_path_buf());
            }
            self.open(path);
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(path) = self.forward.pop() {
            if let Some(current) = self.path() {
                self.back.push(current.to_path_buf());
            }
            self.open(path);
        }
    }

    pub fn jump_to(&mut self, row: usize) {
        self.first_visible = row;
        self.jump = (row, self.jump.1.wrapping_add(1));
    }

    /// Follow a clicked link (see [`link_action`]). Other documents open in
    /// a new tab, or the tab already showing them.
    pub fn follow_link(&mut self, target: String) {
        match link_action(&target, self.path()) {
            LinkAction::Anchor(anchor) => self.jump_to_anchor(&anchor),
            LinkAction::Document(path, anchor) => {
                if path.is_file() {
                    self.open_in_tab(path);
                    if let Some(anchor) = anchor {
                        self.jump_to_anchor(&anchor);
                    }
                }
            }
            LinkAction::External(url) => open_external(&url),
            LinkAction::Ignore => {}
        }
    }

    fn jump_to_anchor(&mut self, anchor: &str) {
        let anchor = anchor.to_ascii_lowercase();
        let row = self.doc.as_ref().and_then(|d| {
            let id = &d.ir.outline.iter().find(|item| item.anchor == anchor)?.id;
            heading_rows(&d.units()).remove(id)
        });
        if let Some(row) = row {
            self.jump_to(row);
        }
    }

    pub fn copy(&mut self, text: &str, code: Option<String>) {
        if clipboard::set_text(text).is_ok() {
            self.copied = code;
        }
    }

    // Editing

    /// Whether the editor is showing.
    pub fn editing(&self) -> bool {
        self.mode != Mode::Preview && self.doc.as_ref().is_some_and(Doc::editable)
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.menu = None;
        self.mode = mode;
    }

    /// Edit or stop editing: Markdown edits beside its preview, other text
    /// on its own.
    pub fn toggle_edit(&mut self) {
        let markdown = self.doc.as_ref().is_some_and(|d| d.kind == FileKind::Markdown);
        self.set_mode(match self.mode {
            Mode::Preview if markdown => Mode::Split,
            Mode::Preview => Mode::Source,
            _ => Mode::Preview,
        });
    }

    /// The editor's text changed: re-render the preview and schedule a save.
    pub fn edit(&mut self, text: String) {
        let Some(doc) = &mut self.doc else { return };
        if doc.source == text {
            return;
        }
        match build(&doc.path, doc.kind, text) {
            Ok(mut next) => {
                next.saved = std::mem::take(&mut doc.saved);
                // Reuse loaded images unless the document refers to new ones.
                if same_images(&next.ir, &doc.ir) {
                    next.images = std::mem::take(&mut doc.images);
                } else {
                    next.images = load_images(&next.ir, &next.path);
                }
                *doc = next;
            }
            Err(e) => self.save_error = Some(e),
        }
        let rows = self.doc.as_ref().map_or(0, |d| d.units().len());
        if self.window.end > rows {
            self.window = self.window.start.min(rows)..rows;
        }
        self.recount();
        self.edits += 1;
        if let Some(sender) = self.sender.clone() {
            let n = self.edits;
            std::thread::spawn(move || {
                std::thread::sleep(AUTOSAVE);
                let _ = sender.send(Msg::Autosave(n));
            });
        }
    }

    pub fn message(&mut self, msg: Msg) {
        match msg {
            Msg::Autosave(n) if n == self.edits => {
                self.save();
            }
            Msg::Autosave(_) => {}
            Msg::Changed { paths, structural } => self.changed_on_disk(paths, structural),
            Msg::Rescan(n) if n == self.rescans => {
                self.rescan();
            }
            Msg::Rescan(_) => {}
            Msg::Open(paths) => {
                for path in paths {
                    if path.is_dir() {
                        self.open_folder(path);
                    } else {
                        self.open_in_tab(path);
                    }
                }
            }
        }
    }

    // Watching

    /// Watch the workspace folders, and open files outside them, so outside
    /// changes show up. Only with a message sender (the running app).
    pub fn sync_watches(&mut self) {
        let Some(sender) = self.sender.clone() else { return };
        let mut wanted: Vec<(PathBuf, bool)> = self.tree.iter().map(|r| (r.path.clone(), true)).collect();
        let open = self
            .path()
            .map(Path::to_path_buf)
            .into_iter()
            .chain(self.tabs.iter().filter_map(|t| t.path.clone()));
        for path in open {
            if !wanted.iter().any(|(w, deep)| *deep && path.starts_with(w)) && !wanted.iter().any(|(w, _)| *w == path) {
                wanted.push((path, false));
            }
        }
        if wanted == self.watched && self.watcher.is_some() {
            return;
        }
        use notify::{EventKind, RecursiveMode, Watcher, event::ModifyKind};
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };
            let structural = match event.kind {
                EventKind::Create(_) | EventKind::Remove(_) => true,
                EventKind::Modify(ModifyKind::Name(_)) => true,
                EventKind::Modify(_) | EventKind::Any | EventKind::Other => false,
                EventKind::Access(_) => return,
            };
            let _ = sender.send(Msg::Changed {
                paths: event.paths,
                structural,
            });
        });
        let Ok(mut watcher) = watcher else { return };
        for (path, deep) in &wanted {
            let mode = if *deep { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
            let _ = watcher.watch(path, mode);
        }
        self.watcher = Some(watcher);
        self.watched = wanted;
    }

    /// Reload open documents that changed on disk, keeping their place, and
    /// rescan the tree when files came or went. Edits are never discarded:
    /// a document with unsaved edits shows the conflict banner instead.
    fn changed_on_disk(&mut self, paths: Vec<PathBuf>, structural: bool) {
        for path in &paths {
            let changed = |doc: &Doc| {
                doc.path == *path
                    && std::fs::read_to_string(path).is_ok_and(|disk| disk != doc.saved)
            };
            if self.doc.as_ref().is_some_and(&changed) {
                if self.doc.as_ref().is_some_and(Doc::dirty) {
                    self.save_error = Some(format!(
                        "{} changed on disk since it was opened.",
                        file_name(path)
                    ));
                } else {
                    self.reload();
                }
            }
            for tab in &mut self.tabs {
                let Some(state) = tab.saved.as_mut() else { continue };
                if state.doc.as_ref().is_some_and(|d| changed(d) && !d.dirty())
                    && let Ok(doc) = load(path)
                {
                    state.doc = Some(doc);
                }
            }
        }
        let in_tree = paths
            .iter()
            .any(|p| self.tree.iter().any(|root| p.starts_with(&root.path)));
        if structural && in_tree {
            self.rescans += 1;
            if let Some(sender) = self.sender.clone() {
                let n = self.rescans;
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                    let _ = sender.send(Msg::Rescan(n));
                });
            }
        }
    }

    /// Write the current document if it changed. Returns false on failure.
    pub fn save(&mut self) -> bool {
        let Some(doc) = &mut self.doc else { return true };
        match write(doc) {
            Ok(()) => {
                self.save_error = None;
                true
            }
            Err(e) => {
                self.save_error = Some(e);
                false
            }
        }
    }

    /// Save the current document before leaving it.
    fn flush(&mut self) {
        if self.doc.as_ref().is_some_and(Doc::dirty) {
            self.save();
        }
    }

    /// Save every tab, before quitting.
    pub fn save_all(&mut self) -> bool {
        let mut ok = self.save();
        for tab in &mut self.tabs {
            if let Some(doc) = tab.saved.as_mut().and_then(|s| s.doc.as_mut()) {
                ok &= write(doc).is_ok();
            }
        }
        ok
    }

    /// The file changed on disk since it was read: keep this version.
    pub fn overwrite(&mut self) {
        if let Some(doc) = &mut self.doc {
            doc.saved = std::fs::read_to_string(&doc.path).unwrap_or_default();
        }
        self.save();
    }

    /// The file changed on disk: drop the edits and show the file.
    pub fn discard_edits(&mut self) {
        if let Some(doc) = &mut self.doc {
            doc.source = doc.saved.clone();
        }
        self.save_error = None;
        self.reload();
    }

    // Writing helpers

    /// The editor's selection moved: open, update or close the `/` menu,
    /// and in a split view keep the edited block in the preview.
    pub fn selection_changed(&mut self, s: TextSelection) {
        let moved = self.selection.is_none_or(|old| old.caret_at != s.caret_at);
        let scrolled = self.selection.is_some_and(|old| old.top != s.top);
        self.selection = Some(s);
        if self.mode == Mode::Split && !self.echo(Pane::Editor) {
            if scrolled {
                self.sync_preview_to(s.top);
            } else if moved {
                self.follow_caret(s.caret_at);
            }
        }
        let found = self
            .doc
            .as_ref()
            .filter(|_| s.start == s.end && s.caret_at <= self.doc.as_ref().map_or(0, |d| d.source.len()))
            .and_then(|d| compose::slash_at(&d.source, s.caret_at))
            .filter(|(start, _)| self.slash_dismissed != Some(*start));
        self.slash = found.map(|(start, query)| {
            let index = match &self.slash {
                Some(open) if open.start == start && open.query == query => open.index,
                _ => 0,
            };
            // Below the caret, or above it when the editor has no room.
            let height = compose::menu_height(compose::matching(&query).len());
            let below = s.caret[1] + s.caret[3] + 6.;
            let bottom = s.bounds[1] + s.bounds[3];
            let y = if below + height > bottom && s.caret[1] - height - 6. >= s.bounds[1] {
                s.caret[1] - height - 6.
            } else {
                below
            };
            Slash {
                start,
                query,
                index,
                at: [s.caret[0], y],
            }
        });
        if self.slash.is_none() && self.slash_dismissed.is_some_and(|d| {
            self.doc.as_ref().is_none_or(|doc| compose::slash_at(&doc.source, s.caret_at).is_none_or(|(at, _)| at != d))
        }) {
            self.slash_dismissed = None;
        }
    }

    /// Whether `pane` is moving only because the other pane just synced it.
    fn echo(&self, pane: Pane) -> bool {
        self.sync
            .is_some_and(|(leader, at)| leader != pane && at.elapsed() < SYNC_ECHO)
    }

    /// The editor scrolled: show the block at its top line in the preview.
    fn sync_preview_to(&mut self, top: usize) {
        let Some(doc) = &self.doc else { return };
        let units = doc.units();
        if let Some(row) = units
            .iter()
            .position(|u| render::source_range(u).is_some_and(|r| top < r.end))
        {
            self.sync = Some((Pane::Editor, std::time::Instant::now()));
            self.jump_to(row);
        }
    }

    /// The preview scrolled: bring its first block to the editor's top.
    pub fn preview_scrolled(&mut self) {
        if self.mode != Mode::Split || self.echo(Pane::Preview) {
            return;
        }
        let Some(doc) = &self.doc else { return };
        let units = doc.units();
        let Some(range) = units.get(self.first_visible).and_then(render::source_range) else {
            return;
        };
        if self.selection.is_some_and(|s| s.top == range.start) {
            return;
        }
        self.sync = Some((Pane::Preview, std::time::Instant::now()));
        self.editor_scroll = (range.start, self.editor_scroll.1.wrapping_add(1));
    }

    /// Scroll the preview to the block the caret is in, if it is off screen.
    fn follow_caret(&mut self, caret: usize) {
        let Some(doc) = &self.doc else { return };
        let units = doc.units();
        let row = units
            .iter()
            .position(|u| render::source_range(u).is_some_and(|r| caret <= r.end));
        let Some(row) = row.or_else(|| units.len().checked_sub(1)) else {
            return;
        };
        let visible = self.first_visible..self.window.end.saturating_sub(4).max(self.first_visible + 1);
        if !visible.contains(&row) {
            self.sync = Some((Pane::Editor, std::time::Instant::now()));
            self.jump_to(row.saturating_sub(1));
        }
    }

    /// Keys the editor hands over: arrows and Enter for an open menu, Enter
    /// at the end of a list item.
    pub fn editor_keys(&self) -> Vec<EditKey> {
        if self.slash.is_some() {
            return vec![EditKey::Up, EditKey::Down, EditKey::Enter];
        }
        let continues = match (&self.doc, self.selection) {
            (Some(doc), Some(s)) if s.start == s.end && s.caret_at <= doc.source.len() => {
                compose::continue_list(&doc.source, s.caret_at).is_some()
            }
            _ => false,
        };
        if continues { vec![EditKey::Enter] } else { Vec::new() }
    }

    pub fn editor_key(&mut self, key: EditKey) {
        if let Some(slash) = &mut self.slash {
            let n = compose::matching(&slash.query).len().max(1);
            match key {
                EditKey::Up => slash.index = (slash.index + n - 1) % n,
                EditKey::Down => slash.index = (slash.index + 1) % n,
                EditKey::Enter => {
                    let index = slash.index;
                    self.apply_slash(index);
                }
                _ => {}
            }
            return;
        }
        if key == EditKey::Enter {
            self.continue_list();
        }
    }

    /// Escape: close the `/` menu, leaving what was typed.
    pub fn cancel_slash(&mut self) {
        if let Some(slash) = self.slash.take() {
            self.slash_dismissed = Some(slash.start);
        }
    }

    /// Replace `/query` with command `index` of the matching commands.
    pub fn apply_slash(&mut self, index: usize) {
        let (Some(slash), Some(doc), Some(s)) = (self.slash.take(), &self.doc, self.selection) else {
            return;
        };
        let Some(command) = compose::matching(&slash.query).get(index).copied() else {
            return;
        };
        let end = s.caret_at.min(doc.source.len());
        let mut text = doc.source.clone();
        text.replace_range(slash.start..end, command.insert);
        let (a, b) = command.select;
        self.replace_text(text, slash.start + a, slash.start + b);
    }

    fn continue_list(&mut self) {
        let (Some(doc), Some(s)) = (&self.doc, self.selection) else { return };
        let caret = s.caret_at.min(doc.source.len());
        let Some(marker) = compose::continue_list(&doc.source, caret) else {
            return;
        };
        let mut text = doc.source.clone();
        if marker.is_empty() {
            // Enter on an empty item ends the list: clear the marker.
            let line_start = text[..caret].rfind('\n').map_or(0, |i| i + 1);
            text.replace_range(line_start..caret, "");
            self.replace_text(text, line_start, line_start);
        } else {
            let insert = format!("\n{marker}");
            text.insert_str(caret, &insert);
            let at = caret + insert.len();
            self.replace_text(text, at, at);
        }
    }

    /// Wrap the selection in `before` and `after` (⌘B, ⌘I), or insert the
    /// pair with the caret between when nothing is selected.
    pub fn wrap_selection(&mut self, before: &str, after: &str) {
        if !self.editing() {
            return;
        }
        let (Some(doc), Some(s)) = (&self.doc, self.selection) else { return };
        let (start, end) = (s.start.min(doc.source.len()), s.end.min(doc.source.len()));
        let selected = &doc.source[start..end];
        let mut text = doc.source.clone();
        // Already wrapped: unwrap.
        if selected.len() >= before.len() + after.len()
            && selected.starts_with(before)
            && selected.ends_with(after)
        {
            let inner = selected[before.len()..selected.len() - after.len()].to_owned();
            text.replace_range(start..end, &inner);
            self.replace_text(text, start, start + inner.len());
            return;
        }
        text.replace_range(start..end, &format!("{before}{selected}{after}"));
        self.replace_text(text, start + before.len(), end + before.len());
    }

    /// Insert a link around the selection, selecting the URL to type over.
    pub fn insert_link(&mut self) {
        if !self.editing() {
            return;
        }
        let (Some(doc), Some(s)) = (&self.doc, self.selection) else { return };
        let (start, end) = (s.start.min(doc.source.len()), s.end.min(doc.source.len()));
        let label = if start == end { "text" } else { &doc.source[start..end] };
        let link = format!("[{label}](https://)");
        let mut text = doc.source.clone();
        text.replace_range(start..end, &link);
        let url = start + label.len() + 3;
        self.replace_text(text, url, url + 8);
    }

    /// Replace the whole text from a writing helper and select `start..end`.
    fn replace_text(&mut self, text: String, start: usize, end: usize) {
        self.edit(text);
        self.select = (start, end, self.select.2.wrapping_add(1));
    }

    /// Whether tab `i` has unsaved edits.
    pub fn tab_dirty(&self, i: usize) -> bool {
        if i == self.active {
            return self.doc.as_ref().is_some_and(Doc::dirty);
        }
        self.tabs
            .get(i)
            .and_then(|t| t.saved.as_ref())
            .and_then(|s| s.doc.as_ref())
            .is_some_and(Doc::dirty)
    }

    // Tabs

    fn ensure_tab(&mut self) {
        if self.tabs.is_empty() {
            self.tabs.push(Tab::default());
            self.active = 0;
        }
    }

    pub fn tab_with(&self, path: &Path) -> Option<usize> {
        self.tabs.iter().position(|t| t.path.as_deref() == Some(path))
    }

    /// Show `path` in its tab, or in a new one. An empty current tab is used
    /// rather than adding another.
    pub fn open_in_tab(&mut self, path: PathBuf) {
        if let Some(i) = self.tab_with(&path) {
            self.switch_tab(i);
            return;
        }
        if self.doc.is_some() || self.error.is_some() {
            self.new_tab();
        }
        self.open(path);
    }

    /// An empty tab after the current one.
    pub fn new_tab(&mut self) {
        self.ensure_tab();
        self.flush();
        let state = self.stash();
        self.tabs[self.active].saved = Some(state);
        self.active += 1;
        self.tabs.insert(self.active, Tab::default());
        self.restore(TabState::default());
    }

    pub fn switch_tab(&mut self, i: usize) {
        if i == self.active || i >= self.tabs.len() {
            return;
        }
        self.flush();
        let state = self.stash();
        self.tabs[self.active].saved = Some(state);
        self.active = i;
        let state = self.tabs[i].saved.take().unwrap_or_default();
        self.restore(state);
    }

    /// Step through tabs, wrapping around.
    pub fn cycle_tab(&mut self, step: isize) {
        let n = self.tabs.len() as isize;
        if n > 1 {
            self.switch_tab((self.active as isize + step).rem_euclid(n) as usize);
        }
    }

    pub fn close_tab(&mut self, i: usize) {
        self.menu = None;
        if i >= self.tabs.len() {
            return;
        }
        if i == self.active {
            self.flush();
        } else if let Some(doc) = self.tabs[i].saved.as_mut().and_then(|s| s.doc.as_mut()) {
            let _ = write(doc);
        }
        if self.tabs.len() == 1 {
            self.stash();
            self.tabs[0] = Tab::default();
            self.restore(TabState::default());
            return;
        }
        self.tabs.remove(i);
        if i == self.active {
            self.stash();
            self.active = i.min(self.tabs.len() - 1);
            let state = self.tabs[self.active].saved.take().unwrap_or_default();
            self.restore(state);
        } else if i < self.active {
            self.active -= 1;
        }
    }

    pub fn close_other_tabs(&mut self, keep: usize) {
        if keep >= self.tabs.len() {
            return;
        }
        self.switch_tab(keep);
        let tab = self.tabs.swap_remove(keep);
        self.tabs = vec![tab];
        self.active = 0;
    }

    fn stash(&mut self) -> TabState {
        TabState {
            doc: self.doc.take(),
            error: self.error.take(),
            back: std::mem::take(&mut self.back),
            forward: std::mem::take(&mut self.forward),
            first_visible: self.first_visible,
        }
    }

    fn restore(&mut self, state: TabState) {
        self.save_error = None;
        self.window = 0..state.doc.as_ref().map_or(0, |d| d.units().len().min(30));
        self.doc = state.doc;
        self.error = state.error;
        self.back = state.back;
        self.forward = state.forward;
        self.copied = None;
        self.menu = None;
        self.generation = self.generation.wrapping_add(1);
        self.jump_to(state.first_visible);
        self.recount();
    }

    // Workspace

    /// Rescan workspace folders. Without any, the open document's folder
    /// stands in so the tree is never empty while reading.
    pub fn rescan(&mut self) {
        let mut roots: Vec<PathBuf> = self.settings.folders.clone();
        if let Some(path) = self.path()
            && !roots.iter().any(|r| path.starts_with(r))
            && let Some(parent) = path.parent()
        {
            roots.push(parent.to_path_buf());
        }
        self.tree = roots.iter().filter(|r| r.is_dir()).map(|r| workspace::scan(r)).collect();
        self.sync_watches();
    }

    /// Whether `path` is a folder the user added (rather than the open
    /// document's folder shown in their absence).
    pub fn is_workspace_folder(&self, path: &Path) -> bool {
        self.settings.folders.iter().any(|f| f == path)
    }

    fn reveal_in_tree(&mut self, path: &Path) {
        if !self.tree.iter().any(|root| path.starts_with(&root.path)) {
            self.rescan();
        }
        for root in &self.tree {
            if path.starts_with(&root.path) {
                let mut dir = path.parent();
                while let Some(d) = dir {
                    if d == root.path {
                        break;
                    }
                    self.expanded.insert(d.to_path_buf());
                    dir = d.parent();
                }
            }
        }
    }

    pub fn choose_folder(&mut self) {
        match file_dialog::open_folder(&FileDialogOptions::new("Open Folder")) {
            Ok(Some(path)) => self.open_folder(path),
            Ok(None) => {}
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn choose_folder_to_add(&mut self) {
        match file_dialog::open_folder(&FileDialogOptions::new("Add Folder to Workspace")) {
            Ok(Some(path)) => self.add_folder(path),
            Ok(None) => {}
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Make `path` the workspace, and show its README when nothing is open.
    pub fn open_folder(&mut self, path: PathBuf) {
        self.settings.folders = vec![path.clone()];
        self.show_sidebar(SidebarTab::Files);
        self.rescan();
        if self.doc.is_none()
            && let Some(readme) = readme(&path)
        {
            self.open(readme);
        }
    }

    pub fn add_folder(&mut self, path: PathBuf) {
        if !self.settings.folders.contains(&path) {
            self.settings.folders.push(path);
        }
        self.show_sidebar(SidebarTab::Files);
        self.rescan();
    }

    pub fn remove_folder(&mut self, path: &Path) {
        self.configure(|s| s.folders.retain(|f| f != path));
        self.rescan();
    }

    pub fn toggle_folder(&mut self, path: PathBuf) {
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
        self.save_settings();
    }

    // Find

    pub fn open_find(&mut self) {
        if self.doc.is_none() {
            return;
        }
        self.menu = None;
        match &self.find {
            // ⌘F again from the open bar steps to the next match.
            Some(_) => self.find_step(true),
            None => self.find = Some(Find::default()),
        }
    }

    pub fn close_find(&mut self) {
        self.find = None;
    }

    pub fn set_query(&mut self, query: String) {
        if let Some(find) = &mut self.find {
            find.query = query;
        }
        self.recount();
        self.reveal_match();
    }

    pub fn find_step(&mut self, forward: bool) {
        if let Some(find) = &mut self.find {
            find.step(forward);
        }
        self.reveal_match();
    }

    fn recount(&mut self) {
        let Some(find) = &mut self.find else { return };
        match &self.doc {
            Some(doc) => {
                let units = doc.units();
                find.recount(units.iter().map(render::unit_texts), self.first_visible);
            }
            None => find.recount(std::iter::empty(), 0),
        }
    }

    /// Scroll the current match into view when it is off screen.
    fn reveal_match(&mut self) {
        let Some(hit) = self.find.as_ref().and_then(Find::current_hit) else {
            return;
        };
        let on_screen = self.first_visible + 1..self.window.end.saturating_sub(5);
        if !on_screen.contains(&hit.row) {
            self.jump_to(hit.row.saturating_sub(1));
        }
    }

    // Context menus

    /// A right-click: the native menu on macOS (in the running app), the
    /// in-window menu otherwise.
    pub fn open_menu(&mut self, target: MenuTarget, at: [f32; 2]) {
        #[cfg(target_os = "macos")]
        if self.sender.is_some() {
            self.native_menu(target);
            return;
        }
        self.menu = Some((target, at));
    }

    /// Text selected in the rendered document changed.
    pub fn text_selected(&mut self, text: String) {
        #[cfg(target_os = "macos")]
        {
            crate::macos::offer_selection_to_services();
            crate::macos::set_selection(&text);
        }
        self.selected_text = text;
    }

    #[cfg(target_os = "macos")]
    fn native_menu(&mut self, target: MenuTarget) {
        use crate::macos::{self, Entry};
        let item = |title: &str, tag: u8| Entry::Item {
            title: title.into(),
            tag: isize::from(tag),
            enabled: true,
        };
        let mut entries = Vec::new();
        let selected = self.selected_text.trim().to_owned();
        let quoted = {
            let short: String = selected.chars().take(24).collect();
            if short.len() < selected.len() { format!("“{short}…”") } else { format!("“{short}”") }
        };
        if matches!(target, MenuTarget::Row(_)) && !selected.is_empty() {
            entries.extend([
                item("Copy", text_action::COPY),
                item(&format!("Look Up {quoted}"), text_action::LOOK_UP),
                item(&format!("Translate {quoted}"), text_action::TRANSLATE),
                item("Search with Google", text_action::SEARCH),
                Entry::Separator,
                item("Share…", text_action::SHARE),
                Entry::Submenu {
                    title: "Speech".into(),
                    items: vec![
                        item("Start Speaking", text_action::SPEAK),
                        item("Stop Speaking", text_action::STOP_SPEAKING),
                    ],
                },
                Entry::Services,
                Entry::Separator,
            ]);
        }
        match &target {
            MenuTarget::Row(row) => {
                let units = self.doc.as_ref().map(Doc::units).unwrap_or_default();
                let unit = units.get(*row);
                if unit.is_some_and(|u| u.code().is_some()) {
                    entries.push(item("Copy Code", action::COPY_CODE));
                } else {
                    entries.push(item("Copy Block", action::COPY_TEXT));
                }
                if unit.is_some_and(|u| u.heading().is_some()) {
                    entries.push(item("Copy Link to Section", action::COPY_LINK));
                }
                entries.extend([
                    Entry::Separator,
                    item("Find…", action::FIND),
                    item(if self.editing() { "Stop Editing" } else { "Edit" }, text_action::EDIT),
                    Entry::Separator,
                    item("Reload", action::RELOAD),
                    item("Reveal in Finder", action::REVEAL),
                ]);
            }
            MenuTarget::File(path) => {
                if path.is_dir() {
                    entries.push(item("Expand or Collapse", file_action::OPEN));
                } else {
                    entries.extend([
                        item("Open", file_action::OPEN),
                        item("Open in New Tab", file_action::OPEN_TAB),
                        Entry::Separator,
                    ]);
                }
                entries.extend([
                    item("Reveal in Finder", file_action::REVEAL),
                    item("Copy Path", file_action::COPY_PATH),
                ]);
                if self.tree.iter().any(|r| r.path == *path) {
                    entries.push(item("Refresh", file_action::REFRESH));
                    if self.is_workspace_folder(path) {
                        entries.extend([Entry::Separator, item("Remove from Workspace", file_action::REMOVE)]);
                    }
                }
            }
            MenuTarget::Heading(_) => entries.extend([
                item("Go to Section", heading_action::GO),
                item("Copy Link to Section", heading_action::COPY_LINK),
                item("Copy Title", heading_action::COPY_TITLE),
            ]),
            MenuTarget::Tab(_) => entries.extend([
                item("Close Tab", tab_action::CLOSE),
                item("Close Other Tabs", tab_action::CLOSE_OTHERS),
                Entry::Separator,
                item("Copy Path", tab_action::COPY_PATH),
                item("Reveal in Finder", tab_action::REVEAL),
            ]),
        }
        let Some((tag, anchor)) = macos::popup(&entries) else { return };
        let Ok(tag) = u8::try_from(tag) else { return };
        match tag {
            text_action::COPY => self.copy(&selected, None),
            text_action::LOOK_UP => macos::look_up(&selected, &anchor),
            text_action::TRANSLATE => open_external(&format!(
                "https://translate.google.com/?sl=auto&op=translate&text={}",
                encode(&selected)
            )),
            text_action::SEARCH => open_external(&format!("https://www.google.com/search?q={}", encode(&selected))),
            text_action::SHARE => macos::share(&selected, &anchor),
            text_action::SPEAK => macos::speak(&selected),
            text_action::STOP_SPEAKING => macos::stop_speaking(),
            text_action::EDIT => self.toggle_edit(),
            _ => match target {
                MenuTarget::Row(row) => self.row_action(row, tag),
                MenuTarget::File(path) => self.file_action(path, tag),
                MenuTarget::Heading(row) => self.heading_action(row, tag),
                MenuTarget::Tab(i) => self.tab_action(i, tag),
            },
        }
    }

    pub fn row_action(&mut self, row: usize, id: u8) {
        self.menu = None;
        let Some(doc) = &self.doc else { return };
        let units = doc.units();
        let Some(unit) = units.get(row) else { return };
        match id {
            action::COPY_TEXT => {
                let text = render::unit_texts(unit).join("\n");
                self.copy(text.trim(), None);
            }
            action::COPY_CODE => {
                if let Some(block) = unit.code() {
                    let source = block.code.clone().or_else(|| block.raw.clone()).unwrap_or_default();
                    self.copy(&source, None);
                }
            }
            action::COPY_LINK => {
                if let Some(link) = unit.heading().and_then(|h| section_link(doc, &h.id)) {
                    self.copy(&link, None);
                }
            }
            action::FIND => self.open_find(),
            action::RELOAD => self.reload(),
            action::REVEAL => {
                if let Some(path) = self.path().map(Path::to_path_buf) {
                    reveal(&path);
                }
            }
            _ => {}
        }
    }

    pub fn file_action(&mut self, path: PathBuf, id: u8) {
        self.menu = None;
        match id {
            file_action::OPEN => {
                if path.is_dir() {
                    self.toggle_folder(path);
                } else {
                    self.visit(path);
                }
            }
            file_action::OPEN_TAB => self.open_in_tab(path),
            file_action::REVEAL => reveal(&path),
            file_action::COPY_PATH => self.copy(&path.to_string_lossy(), None),
            file_action::REFRESH => self.rescan(),
            file_action::REMOVE => self.remove_folder(&path),
            _ => {}
        }
    }

    pub fn heading_action(&mut self, row: usize, id: u8) {
        self.menu = None;
        match id {
            heading_action::GO => self.jump_to(row),
            heading_action::COPY_LINK | heading_action::COPY_TITLE => {
                let Some(doc) = &self.doc else { return };
                let units = doc.units();
                let Some(heading) = units.get(row).and_then(|u| u.heading()) else {
                    return;
                };
                let text = if id == heading_action::COPY_LINK {
                    section_link(doc, &heading.id)
                } else {
                    doc.ir
                        .outline
                        .iter()
                        .find(|o| o.id == heading.id)
                        .map(|o| o.text.clone())
                };
                if let Some(text) = text {
                    self.copy(&text, None);
                }
            }
            _ => {}
        }
    }

    pub fn tab_action(&mut self, i: usize, id: u8) {
        self.menu = None;
        let path = self.tabs.get(i).and_then(|t| t.path.clone());
        match id {
            tab_action::CLOSE => self.close_tab(i),
            tab_action::CLOSE_OTHERS => self.close_other_tabs(i),
            tab_action::COPY_PATH => {
                if let Some(path) = path {
                    self.copy(&path.to_string_lossy(), None);
                }
            }
            tab_action::REVEAL => {
                if let Some(path) = path {
                    reveal(&path);
                }
            }
            _ => {}
        }
    }
}

/// Native menu actions on selected text (tags apart from row actions).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod text_action {
    pub const COPY: u8 = 100;
    pub const LOOK_UP: u8 = 101;
    pub const TRANSLATE: u8 = 102;
    pub const SEARCH: u8 = 103;
    pub const SHARE: u8 = 104;
    pub const SPEAK: u8 = 105;
    pub const STOP_SPEAKING: u8 = 106;
    pub const EDIT: u8 = 107;
}

/// Percent-encode text for a URL query.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes().take(2000) {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// File tree context menu actions.
pub mod file_action {
    pub const OPEN: u8 = 0;
    pub const REVEAL: u8 = 1;
    pub const COPY_PATH: u8 = 2;
    pub const REFRESH: u8 = 3;
    pub const REMOVE: u8 = 4;
    pub const OPEN_TAB: u8 = 5;
}

/// Outline context menu actions.
pub mod heading_action {
    pub const GO: u8 = 0;
    pub const COPY_LINK: u8 = 1;
    pub const COPY_TITLE: u8 = 2;
}

/// Tab context menu actions.
pub mod tab_action {
    pub const CLOSE: u8 = 0;
    pub const CLOSE_OTHERS: u8 = 1;
    pub const COPY_PATH: u8 = 2;
    pub const REVEAL: u8 = 3;
}

/// Write a document's edits, refusing to overwrite changes made on disk
/// since it was read.
fn write(doc: &mut Doc) -> Result<(), String> {
    if !doc.dirty() {
        return Ok(());
    }
    match save_text_file(&doc.path, &doc.source, SavePolicy::MatchContents(&doc.saved)) {
        Ok(()) => {
            doc.saved = doc.source.clone();
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(format!(
            "{} changed on disk since it was opened.",
            file_name(&doc.path)
        )),
        Err(e) => Err(format!("{} could not be saved: {e}", file_name(&doc.path))),
    }
}

fn same_images(a: &DocumentIr, b: &DocumentIr) -> bool {
    let (mut x, mut y) = (Vec::new(), Vec::new());
    render::image_urls(&a.blocks, &mut x);
    render::image_urls(&b.blocks, &mut y);
    x == y
}

/// Read and prepare a document of any supported kind.
fn load(path: &Path) -> Result<Doc, String> {
    let name = file_name(path);
    let kind = workspace::kind(path).unwrap_or(FileKind::Code("text"));
    if kind == FileKind::Image {
        if load_image(path).is_err() {
            return Err(format!("{name} is not an image MD Star can show."));
        }
        let ir = single(
            path,
            BlockIr {
                kind: "paragraph".into(),
                inlines: vec![InlineIr {
                    id: "image".into(),
                    range: None,
                    kind: "image".into(),
                    text: None,
                    children: Vec::new(),
                    url: Some(name.clone()),
                    title: None,
                    alt: Some(name.clone()),
                }],
                ..block("image", "paragraph")
            },
        );
        return Ok(Doc {
            images: load_images(&ir, path),
            path: path.to_path_buf(),
            kind,
            ir,
            title: name,
            words: 0,
            source: String::new(),
            saved: String::new(),
        });
    }
    let size = std::fs::metadata(path)
        .map_err(|e| format!("{} could not be opened: {e}", path.display()))?
        .len();
    if size > MAX_TEXT {
        return Err(format!("{name} is too large to show ({} MB).", size / (1024 * 1024)));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{} could not be opened: {e}", path.display()))?;
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return Err(format!("{name} is a binary file."));
    }
    let source = String::from_utf8_lossy(&bytes).into_owned();
    let mut doc = build(path, kind, source)?;
    doc.saved = doc.source.clone();
    doc.images = load_images(&doc.ir, path);
    Ok(doc)
}

/// A document from its text. `saved` and `images` are left for the caller.
fn build(path: &Path, kind: FileKind, source: String) -> Result<Doc, String> {
    let name = file_name(path);
    let (ir, title, words) = match kind {
        FileKind::Markdown => {
            let ir = parse_document_ir_with_diagnostics(&source, &path.to_string_lossy())
                .map_err(|e| format!("{name} could not be read as Markdown: {e}"))?;
            let title = ir
                .outline
                .iter()
                .find(|item| item.level == 1)
                .map(|item| item.text.clone())
                .unwrap_or_else(|| name.clone());
            let words = render::word_count(&ir.blocks);
            (ir, title, words)
        }
        FileKind::Code("csv") => {
            let lines = source.lines().count();
            (single(path, csv_table(&source, path)), name.clone(), lines)
        }
        FileKind::Code(language) => {
            // Show JSON formatted when it is minified onto a few lines.
            let text = match language {
                "json" if source.lines().count() < 4 && source.len() > 200 => {
                    serde_json::from_str::<serde_json::Value>(&source)
                        .ok()
                        .and_then(|v| serde_json::to_string_pretty(&v).ok())
                        .unwrap_or_else(|| source.clone())
                }
                _ => source.clone(),
            };
            let lines = text.lines().count();
            let code = BlockIr {
                language: Some(language.into()),
                code: Some(text.trim_end_matches('\n').to_owned()),
                ..block("code", "code")
            };
            (single(path, code), name.clone(), lines)
        }
        FileKind::Image => return Err(format!("{name} is an image.")),
    };
    Ok(Doc {
        images: HashMap::new(),
        path: path.to_path_buf(),
        kind,
        ir,
        title,
        words,
        source,
        saved: String::new(),
    })
}

fn block(id: &str, kind: &str) -> BlockIr {
    BlockIr {
        id: id.into(),
        range: None,
        kind: kind.into(),
        level: None,
        inlines: Vec::new(),
        children: Vec::new(),
        ordered: None,
        start: None,
        items: Vec::new(),
        language: None,
        meta: None,
        code: None,
        headers: Vec::new(),
        rows: Vec::new(),
        raw: None,
    }
}

fn single(path: &Path, block: BlockIr) -> DocumentIr {
    DocumentIr {
        schema_version: DOCUMENT_IR_SCHEMA_VERSION,
        document_id: path.to_string_lossy().into_owned(),
        origin: path.to_string_lossy().into_owned(),
        blocks: vec![block],
        outline: Vec::new(),
        diagnostics: Vec::new(),
    }
}

/// A CSV or TSV file as a table, the first row as its header.
fn csv_table(source: &str, path: &Path) -> BlockIr {
    let tab = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("tsv"));
    let cell = |text: String| -> Vec<InlineIr> {
        vec![InlineIr {
            id: String::new(),
            range: None,
            kind: "text".into(),
            text: Some(text),
            children: Vec::new(),
            url: None,
            title: None,
            alt: None,
        }]
    };
    let mut rows = source
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(MAX_CSV_ROWS + 1)
        .map(|line| split_csv(line, if tab { '\t' } else { ',' }).into_iter().map(cell).collect::<Vec<_>>());
    let headers = rows.next().unwrap_or_default();
    BlockIr {
        headers,
        rows: rows.collect(),
        ..block("table", "table")
    }
}

/// One CSV line's fields; quoted fields may hold the separator and `""`.
fn split_csv(line: &str, separator: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == separator && !quoted => out.push(std::mem::take(&mut field)),
            c => field.push(c),
        }
    }
    out.push(field);
    out
}

/// A folder's README, if it has one.
fn readme(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.is_file()
                && p.file_stem()
                    .is_some_and(|s| s.eq_ignore_ascii_case("readme"))
                && workspace::is_markdown(p)
        })
}

/// `file.md#anchor` for a heading, the form Markdown links use.
fn section_link(doc: &Doc, id: &str) -> Option<String> {
    let anchor = &doc.ir.outline.iter().find(|o| o.id == id)?.anchor;
    Some(format!("{}#{anchor}", file_name(&doc.path)))
}

#[derive(Debug, PartialEq)]
pub enum LinkAction {
    /// `#heading` in this document.
    Anchor(String),
    /// A file the reader can show, next to this one, optionally at a heading.
    Document(PathBuf, Option<String>),
    /// A web or mail link for the default app.
    External(String),
    Ignore,
}

/// What a link target means. Only http(s) and mailto leave the app; other
/// schemes (file:, javascript:, custom app schemes) and files the reader
/// cannot show are ignored, so a document cannot launch arbitrary apps.
pub fn link_action(target: &str, current: Option<&Path>) -> LinkAction {
    let target = target.trim();
    if let Some(anchor) = target.strip_prefix('#') {
        return LinkAction::Anchor(anchor.to_ascii_lowercase());
    }
    let lower = target.to_ascii_lowercase();
    if ["http://", "https://", "mailto:"]
        .iter()
        .any(|s| lower.starts_with(s))
    {
        return LinkAction::External(target.to_string());
    }
    if lower.contains(':') || target.is_empty() {
        return LinkAction::Ignore;
    }
    let (file, anchor) = match target.split_once('#') {
        Some((file, anchor)) => (file, Some(anchor.to_ascii_lowercase())),
        None => (target, None),
    };
    let base = current.and_then(Path::parent).unwrap_or(Path::new("."));
    let path = base.join(file.replace("%20", " "));
    if workspace::kind(&path).is_some() {
        LinkAction::Document(path, anchor)
    } else {
        LinkAction::Ignore
    }
}

/// Heading block id to its document row.
pub fn heading_rows(units: &[render::Unit]) -> HashMap<String, usize> {
    units
        .iter()
        .enumerate()
        .filter_map(|(row, unit)| unit.heading().map(|b| (b.id.clone(), row)))
        .collect()
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| "Untitled".into(), |n| n.to_string_lossy().into_owned())
}

fn open_external(target: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(target).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", target])
        .spawn();
}

/// Show a file in Finder (or the platform's file manager).
fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .spawn();
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer")
        .arg("/select,")
        .arg(path)
        .spawn();
}

/// Local PNG/JPEG images referenced by the document, resolved next to it.
fn load_images(doc: &DocumentIr, path: &Path) -> HashMap<String, Image> {
    let mut urls = Vec::new();
    render::image_urls(&doc.blocks, &mut urls);
    let base = path.parent().unwrap_or(Path::new("."));
    urls.into_iter()
        .filter(|url| !url.contains("://"))
        .filter_map(|url| {
            let file = base.join(url.split(['?', '#']).next().unwrap_or(&url));
            load_image(&file).ok().map(|image| (url, image))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_fields_respect_quotes() {
        assert_eq!(split_csv(r#"a,"b, c","d ""q""",e"#, ','), ["a", "b, c", "d \"q\"", "e"]);
        assert_eq!(split_csv("x\ty", '\t'), ["x", "y"]);
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdstar-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn tabs_keep_their_documents_history_and_place() {
        let dir = temp("tabs");
        for (name, body) in [("a.md", "# A\n\n[b](b.md)\n"), ("b.md", "# B\n"), ("c.md", "# C\n")] {
            std::fs::write(dir.join(name), body).unwrap();
        }
        let mut app = App::new(Settings::default(), None);
        app.open(dir.join("a.md"));
        app.first_visible = 3;
        app.follow_link("b.md".into());
        assert_eq!(app.tabs.len(), 2, "a linked document opens in a new tab");
        assert_eq!(app.active, 1);
        app.visit(dir.join("c.md"));
        assert_eq!(app.back, [dir.join("b.md")]);
        app.switch_tab(0);
        assert_eq!(app.path(), Some(dir.join("a.md").as_path()));
        assert_eq!(app.jump.0, 3, "the first tab returns to its place");
        assert!(app.back.is_empty(), "history belongs to its tab");
        app.follow_link("b.md".into());
        assert_eq!((app.tabs.len(), app.active), (3, 1), "new tabs open beside the current one");
        app.open_in_tab(dir.join("c.md"));
        assert_eq!(app.active, 2, "an open document switches to its tab");
        app.close_tab(2);
        assert_eq!((app.tabs.len(), app.active), (2, 1));
        assert_eq!(app.path(), Some(dir.join("b.md").as_path()));
        app.close_other_tabs(1);
        assert_eq!(app.tabs.len(), 1);
        app.close_tab(0);
        assert!(app.doc.is_none() && app.tabs.len() == 1);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn edits_render_live_save_and_refuse_to_overwrite_outside_changes() {
        let dir = temp("edit");
        let file = dir.join("note.md");
        std::fs::write(&file, "# Note\n").unwrap();
        let mut app = App::new(Settings::default(), None);
        app.open(file.clone());
        app.toggle_edit();
        assert_eq!(app.mode, Mode::Split);
        assert!(app.editing());
        app.edit("# Note\n\n## Added\n".into());
        let doc = app.doc.as_ref().unwrap();
        assert!(doc.dirty());
        assert_eq!(doc.ir.outline.len(), 2, "the preview follows the source");
        assert!(app.tab_dirty(0));
        assert!(app.save());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Note\n\n## Added\n");
        assert!(!app.doc.as_ref().unwrap().dirty());
        // Someone else edits the file; our next save must not clobber it.
        std::fs::write(&file, "# Theirs\n").unwrap();
        app.edit("# Mine\n".into());
        assert!(!app.save());
        assert!(app.save_error.as_deref().unwrap().contains("changed on disk"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Theirs\n");
        app.overwrite();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Mine\n");
        // Leaving a document saves it.
        app.edit("# Mine, again\n".into());
        app.new_tab();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Mine, again\n");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn outside_changes_reload_clean_documents_and_flag_edited_ones() {
        let dir = temp("watch");
        let file = dir.join("w.md");
        std::fs::write(&file, "# One\n").unwrap();
        let mut app = App::new(Settings::default(), None);
        app.open(file.clone());
        app.first_visible = 0;
        // Our own save is not a change.
        std::fs::write(&file, "# One\n").unwrap();
        app.message(Msg::Changed {
            paths: vec![file.clone()],
            structural: false,
        });
        assert_eq!(app.doc.as_ref().unwrap().title, "One");
        std::fs::write(&file, "# Two\n").unwrap();
        app.message(Msg::Changed {
            paths: vec![file.clone()],
            structural: false,
        });
        assert_eq!(app.doc.as_ref().unwrap().title, "Two", "a clean document reloads");
        app.edit("# Mine\n".into());
        std::fs::write(&file, "# Three\n").unwrap();
        app.message(Msg::Changed {
            paths: vec![file.clone()],
            structural: false,
        });
        assert_eq!(app.doc.as_ref().unwrap().source, "# Mine\n", "edits are kept");
        assert!(app.save_error.as_deref().unwrap().contains("changed on disk"));
        std::fs::remove_dir_all(dir).ok();
    }

    fn caret(at: usize) -> TextSelection {
        TextSelection {
            start: at,
            end: at,
            caret_at: at,
            caret: [100., 200., 1., 20.],
            bounds: [0., 0., 800., 900.],
            top: 0,
        }
    }

    #[test]
    fn slash_menu_lists_and_formatting_edit_the_source() {
        let dir = temp("compose");
        let file = dir.join("c.md");
        std::fs::write(&file, "Intro\n").unwrap();
        let mut app = App::new(Settings::default(), None);
        app.open(file);
        app.set_mode(Mode::Split);
        // Typing "/" at the start of a line opens the menu under the caret.
        app.edit("Intro\n/".into());
        app.selection_changed(caret(7));
        let slash = app.slash.clone().expect("menu open");
        assert_eq!((slash.start, slash.at), (6, [100., 226.]));
        assert_eq!(app.editor_keys(), [EditKey::Up, EditKey::Down, EditKey::Enter]);
        // Narrow to "task", choose it with Enter.
        app.edit("Intro\n/task".into());
        app.selection_changed(caret(11));
        app.editor_key(EditKey::Down);
        app.editor_key(EditKey::Up);
        app.editor_key(EditKey::Enter);
        assert_eq!(app.doc.as_ref().unwrap().source, "Intro\n- [ ] ");
        assert!(app.slash.is_none());
        assert_eq!((app.select.0, app.select.1), (12, 12));
        // Enter at the end of an item continues the list; on an empty item it ends it.
        app.edit("Intro\n- [ ] milk".into());
        app.selection_changed(caret(16));
        assert_eq!(app.editor_keys(), [EditKey::Enter]);
        app.editor_key(EditKey::Enter);
        assert_eq!(app.doc.as_ref().unwrap().source, "Intro\n- [ ] milk\n- [ ] ");
        app.selection_changed(caret(23));
        app.editor_key(EditKey::Enter);
        assert_eq!(app.doc.as_ref().unwrap().source, "Intro\n- [ ] milk\n");
        // Escape keeps the typed text and the menu closed until the line changes.
        app.edit("Intro\n/h".into());
        app.selection_changed(caret(8));
        assert!(app.slash.is_some());
        app.cancel_slash();
        app.selection_changed(TextSelection {
            caret: [0.; 4],
            ..caret(8)
        });
        assert!(app.slash.is_none());
        // Bold wraps the selection and toggles back.
        app.edit("make this bold".into());
        app.selection_changed(TextSelection {
            start: 10,
            end: 14,
            caret_at: 14,
            ..caret(14)
        });
        app.wrap_selection("**", "**");
        assert_eq!(app.doc.as_ref().unwrap().source, "make this **bold**");
        app.selection_changed(TextSelection {
            start: 10,
            end: 18,
            caret_at: 18,
            ..caret(18)
        });
        app.wrap_selection("**", "**");
        assert_eq!(app.doc.as_ref().unwrap().source, "make this bold");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn folders_open_their_readme_and_show_every_supported_file() {
        let dir = temp("folder");
        std::fs::write(dir.join("README.md"), "# Read me\n").unwrap();
        std::fs::write(dir.join("data.json"), r#"{"a":[1,2,3],"b":{"c":"d"}}"#).unwrap();
        std::fs::write(dir.join("table.csv"), "name,n\nx,1\n\"y, z\",2\n").unwrap();
        std::fs::write(dir.join("blob.bin"), [0u8, 1, 2]).unwrap();
        let mut app = App::new(Settings::default(), None);
        app.open_folder(dir.clone());
        assert_eq!(app.doc.as_ref().unwrap().title, "Read me");
        let names: Vec<&str> = app.tree[0]
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(names, ["data.json", "README.md", "table.csv"]);
        app.visit(dir.join("data.json"));
        let doc = app.doc.as_ref().unwrap();
        assert_eq!(doc.kind, FileKind::Code("json"));
        let table = app.doc.as_ref().map(|d| d.units().len());
        assert_eq!(table, Some(1));
        app.visit(dir.join("table.csv"));
        let block = &app.doc.as_ref().unwrap().ir.blocks[0];
        assert_eq!((block.kind.as_str(), block.rows.len()), ("table", 2));
        app.visit(dir.join("blob.bin"));
        assert!(app.error.as_deref().unwrap().contains("binary"));
        std::fs::remove_dir_all(dir).ok();
    }
}
