//! Reader state and the actions that change it.

use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    ops::Range,
    path::{Path, PathBuf},
};

use mdstar_core::document_ir::{DocumentIr, parse_document_ir_with_diagnostics};
use volt_ui_kit::{
    graphics::Image,
    native::{
        clipboard,
        file_dialog::{self, FileDialogOptions},
        load_image,
    },
};

use crate::{
    find::Find,
    render::{self, action},
    settings::{Settings, SidebarTab, Theme},
    workspace,
};

/// Sidebar rows built before the list reports what fits.
pub const SIDE_WINDOW: Range<usize> = 0..40;

/// What a context menu was opened on.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuTarget {
    Row(usize),
    File(PathBuf),
    /// An outline entry, by its document row.
    Heading(usize),
}

pub struct Doc {
    pub path: PathBuf,
    pub ir: DocumentIr,
    pub title: String,
    pub words: usize,
    pub images: HashMap<String, Image>,
}

impl Doc {
    pub fn units(&self) -> Vec<render::Unit<'_>> {
        render::units(&self.ir.blocks)
    }
}

#[derive(Default)]
pub struct App {
    pub settings: Settings,
    /// Where settings are saved; `None` keeps them in memory (tests).
    pub settings_path: Option<PathBuf>,
    /// The system appearance, as last reported to the theme callback.
    pub system_dark: Cell<bool>,
    pub show_settings: bool,
    /// The settings dropdown that is open.
    pub open_select: Option<&'static str>,
    /// Installed font families, loaded when settings first open.
    pub fonts: Vec<String>,

    /// Workspace folders, scanned, and the file tree's state.
    pub tree: Vec<workspace::Node>,
    pub expanded: HashSet<PathBuf>,
    pub filter: String,
    pub side_window: Range<usize>,

    pub doc: Option<Doc>,
    pub error: Option<String>,
    /// Document rows currently built, the first row on screen, and the row
    /// to jump to with the request number that triggers the jump.
    pub window: Range<usize>,
    pub first_visible: usize,
    pub jump: (usize, u32),
    /// Bumped per loaded document so row measurements start fresh.
    pub generation: u64,
    pub back: Vec<PathBuf>,
    pub forward: Vec<PathBuf>,
    /// The code block whose Copy button was pressed last.
    pub copied: Option<String>,

    pub find: Option<Find>,
    pub menu: Option<(MenuTarget, [f32; 2])>,
}

impl App {
    pub fn new(settings: Settings, settings_path: Option<PathBuf>) -> Self {
        let mut app = App {
            expanded: settings.expanded.iter().cloned().collect(),
            side_window: SIDE_WINDOW,
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

    pub fn open(&mut self, path: PathBuf) {
        self.menu = None;
        match std::fs::read_to_string(&path) {
            Ok(source) => match parse_document_ir_with_diagnostics(&source, &path.to_string_lossy()) {
                Ok(ir) => {
                    let title = ir
                        .outline
                        .iter()
                        .find(|item| item.level == 1)
                        .map(|item| item.text.clone())
                        .unwrap_or_else(|| file_name(&path));
                    let doc = Doc {
                        images: load_images(&ir, &path),
                        words: render::word_count(&ir.blocks),
                        title,
                        ir,
                        path: path.clone(),
                    };
                    self.window = 0..doc.units().len().min(30);
                    self.doc = Some(doc);
                    self.error = None;
                    self.copied = None;
                    self.generation = self.generation.wrapping_add(1);
                    self.first_visible = 0;
                    self.jump = (0, self.jump.1.wrapping_add(1));
                    self.settings.remember(&path);
                    self.reveal_in_tree(&path);
                    self.save_settings();
                    self.recount();
                }
                Err(e) => {
                    self.doc = None;
                    self.error = Some(format!("{} could not be read as Markdown: {e}", file_name(&path)));
                }
            },
            Err(e) => {
                self.doc = None;
                self.error = Some(format!("{} could not be opened: {e}", path.display()));
            }
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.doc.as_ref().map(|d| d.path.as_path())
    }

    pub fn reload(&mut self) {
        if let Some(path) = self.path().map(Path::to_path_buf) {
            let first = self.first_visible;
            self.open(path);
            // Keep the reader's place.
            self.jump.0 = first;
            self.first_visible = first;
        }
    }

    pub fn choose_file(&mut self) {
        match file_dialog::open_file(&FileDialogOptions::new("Open Markdown")) {
            Ok(Some(path)) => self.visit(path),
            Ok(None) => {}
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Open a document, remembering the current one for Back.
    pub fn visit(&mut self, path: PathBuf) {
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

    /// Follow a clicked link (see [`link_action`]).
    pub fn follow_link(&mut self, target: String) {
        match link_action(&target, self.path()) {
            LinkAction::Anchor(anchor) => self.jump_to_anchor(&anchor),
            LinkAction::Document(path, anchor) => {
                if path.is_file() {
                    self.visit(path);
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
        match file_dialog::open_folder(&FileDialogOptions::new("Add Folder to Workspace")) {
            Ok(Some(path)) => self.add_folder(path),
            Ok(None) => {}
            Err(e) => self.error = Some(e.to_string()),
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
        let (Some(find), Some(doc)) = (&mut self.find, &self.doc) else {
            return;
        };
        let units = doc.units();
        find.recount(units.iter().map(render::unit_texts), self.first_visible);
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

    pub fn open_menu(&mut self, target: MenuTarget, at: [f32; 2]) {
        self.menu = Some((target, at));
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
}

/// File tree context menu actions.
pub mod file_action {
    pub const OPEN: u8 = 0;
    pub const REVEAL: u8 = 1;
    pub const COPY_PATH: u8 = 2;
    pub const REFRESH: u8 = 3;
    pub const REMOVE: u8 = 4;
}

/// Outline context menu actions.
pub mod heading_action {
    pub const GO: u8 = 0;
    pub const COPY_LINK: u8 = 1;
    pub const COPY_TITLE: u8 = 2;
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
    /// A Markdown file next to this one, optionally at a heading.
    Document(PathBuf, Option<String>),
    /// A web or mail link for the default app.
    External(String),
    Ignore,
}

/// What a link target means. Only http(s) and mailto leave the app; other
/// schemes (file:, javascript:, custom app schemes) and non-Markdown files
/// are ignored, so a document cannot launch arbitrary apps or files.
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
    if workspace::is_markdown(&path) {
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
