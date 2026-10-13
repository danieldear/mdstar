//! MD Star reader drawn natively with Volt UI Kit.
//!
//! ```text
//! mdstar-volt [FILE.md] [--theme light|dark|system]
//! mdstar-volt FILE.md --snapshot out.png [--size 1100x800] [--scale 2] [--theme dark]
//! ```

mod render;

use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
};

use mdstar_core::document_ir::{DocumentIr, parse_document_ir_with_diagnostics};
use volt_ui_kit::{
    ColorToken as Role, Palette, ThemeMode, UiTheme,
    declarative::{Bindings, Runtime},
    graphics::Image,
    native::{
        Application, Command, CommandRegistry, RasterPainter, Shortcut, WindowBackground,
        WindowChrome,
        file_dialog::{self, FileDialogOptions},
        load_image,
    },
    parse_hex,
    ui::prelude::*,
};

const TOOLBAR: f32 = 52.;
const SIDEBAR: f32 = 264.;
/// Leading space for the macOS traffic lights.
const TRAFFIC_LIGHTS: f32 = 76.;
const MARKDOWN: [&str; 5] = ["md", "markdown", "mdown", "mkd", "txt"];

#[derive(Default)]
pub struct App {
    path: Option<PathBuf>,
    doc: Option<DocumentIr>,
    error: Option<String>,
    images: HashMap<String, Image>,
    title: String,
    words: usize,
    outline: bool,
    /// Document rows currently built, the first row on screen, and the row
    /// to jump to with the request number that triggers the jump.
    window: Range<usize>,
    first_visible: usize,
    jump: (usize, u32),
    /// Bumped per loaded document so row measurements start fresh.
    generation: u64,
    /// Documents to return to with Back.
    history: Vec<PathBuf>,
    pub copied: Option<String>,
    /// Explicit theme; None follows the system.
    dark: Option<bool>,
    /// The system appearance, as last reported to the theme callback.
    system_dark: std::cell::Cell<bool>,
}

impl App {
    fn open(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(source) => {
                match parse_document_ir_with_diagnostics(&source, &path.to_string_lossy()) {
                    Ok(doc) => {
                        self.images = load_images(&doc, &path);
                        self.title = doc
                            .outline
                            .iter()
                            .find(|item| item.level == 1)
                            .map(|item| item.text.clone())
                            .unwrap_or_else(|| file_name(&path));
                        self.words = render::word_count(&doc.blocks);
                        self.outline = doc.outline.len() > 1;
                        self.window = 0..render::units(&doc.blocks).len().min(30);
                        self.doc = Some(doc);
                        self.error = None;
                        self.copied = None;
                        self.generation = self.generation.wrapping_add(1);

                        self.first_visible = 0;
                        self.jump = (0, self.jump.1.wrapping_add(1));
                    }
                    Err(e) => {
                        self.error = Some(format!("{} could not be parsed: {e}", file_name(&path)))
                    }
                }
            }
            Err(e) => self.error = Some(format!("{} could not be opened: {e}", path.display())),
        }
        self.path = Some(path);
    }
    fn reload(&mut self) {
        if let Some(path) = self.path.clone() {
            let first = self.first_visible;
            self.open(path);
            // Keep the reader's place when reloading.
            self.jump.0 = first;
            self.first_visible = first;
        }
    }
    fn choose_file(&mut self) {
        match file_dialog::open_file(&FileDialogOptions::new("Open Markdown")) {
            Ok(Some(path)) => self.visit(path),
            Ok(None) => {}
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    /// Open a document, remembering the current one for Back.
    fn visit(&mut self, path: PathBuf) {
        if let Some(current) = self.path.take()
            && current != path
        {
            self.history.push(current);
        }
        self.open(path);
    }
    fn back(&mut self) {
        if let Some(path) = self.history.pop() {
            self.open(path);
        }
    }
    fn jump_to(&mut self, row: usize) {
        self.first_visible = row;
        self.jump = (row, self.jump.1.wrapping_add(1));
    }
    /// Follow a clicked link (see [`link_action`]).
    pub fn follow_link(&mut self, target: String) {
        match link_action(&target, self.path.as_deref()) {
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
            let id = &d.outline.iter().find(|item| item.anchor == anchor)?.id;
            heading_rows(&render::units(&d.blocks)).remove(id)
        });
        if let Some(row) = row {
            self.jump_to(row);
        }
    }
    fn dark(&self) -> bool {
        self.dark.unwrap_or(self.system_dark.get())
    }
}

#[derive(Debug, PartialEq)]
enum LinkAction {
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
fn link_action(target: &str, current: Option<&Path>) -> LinkAction {
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
    let markdown = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MARKDOWN.contains(&e.to_ascii_lowercase().as_str()));
    if markdown {
        LinkAction::Document(path, anchor)
    } else {
        LinkAction::Ignore
    }
}

/// Heading block id to its document row.
fn heading_rows(units: &[render::Unit]) -> HashMap<String, usize> {
    units
        .iter()
        .enumerate()
        .filter_map(|(row, unit)| match unit {
            render::Unit::Block(b) if b.kind == "heading" => Some((b.id.clone(), row)),
            _ => None,
        })
        .collect()
}

fn file_name(path: &Path) -> String {
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

/// A quiet icon button with a tooltip.
fn icon_button(
    key: &str,
    glyph: Icon,
    label: &str,
    enabled: bool,
    f: impl FnMut(&mut App) + 'static,
) -> Element<App> {
    tooltip(
        label,
        zstack![
            button("")
                .key(key)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Small)
                .width(30.)
                .height(30.)
                .rounded(7.)
                .enabled(enabled)
                .accessible_label(label)
                .on_click(f),
            icon(glyph)
                .size(16.)
                .color(if enabled { Role::Muted } else { Role::Border }),
        ]
        .width(30.)
        .height(30.),
    )
    .key(format!("{key}-tip"))
    .into()
}

fn view(app: &App) -> Element<App> {
    let sidebar = app.outline && app.doc.as_ref().is_some_and(|d| d.outline.len() > 1);
    let content = vstack![toolbar(app, sidebar), body(app)]
        .flex(1.)
        .background(Role::Base100)
        .align(Align::Stretch);
    let root: Element<App> = match (&app.doc, sidebar) {
        (Some(doc), true) => hstack![outline(app, doc), content]
            .align(Align::Stretch)
            .into(),
        _ => content.into(),
    };
    vstack![root]
        .key("root")
        .align(Align::Stretch)
        .color(Role::BaseContent)
        .font_family("SF")
        .into()
}

fn toolbar(app: &App, sidebar: bool) -> Element<App> {
    let mut items: Vec<Element<App>> = Vec::new();
    if !sidebar {
        items.push(spacer().width(TRAFFIC_LIGHTS).flex(0.).into());
        if app.doc.as_ref().is_some_and(|d| d.outline.len() > 1) {
            items.push(icon_button(
                "show-outline",
                Icon::List,
                "Show outline  ⌘L",
                true,
                |app: &mut App| app.outline = true,
            ));
        }
    }
    if !app.history.is_empty() {
        items.push(icon_button(
            "back",
            Icon::ChevronLeft,
            "Back",
            true,
            App::back,
        ));
    }
    let (title, meta) = match (&app.doc, &app.path) {
        (Some(_), Some(path)) => {
            let minutes = app.words.div_ceil(230).max(1);
            (
                app.title.clone(),
                format!(
                    "{}  ·  {} words  ·  {} min read",
                    file_name(path),
                    group(app.words),
                    minutes
                ),
            )
        }
        _ => ("MD Star".into(), "Markdown reader".into()),
    };
    items.push(
        vstack![
            text(title).font_size(13.).font_weight(600).max_lines(1),
            text(meta).font_size(11.).color(Role::Muted).max_lines(1),
        ]
        .gap(1., Axis::Vertical)
        .padding(6., Edge::Left)
        .flex(1.)
        .into(),
    );
    items.push(icon_button(
        "reload",
        Icon::Refresh,
        "Reload  ⌘R",
        app.path.is_some(),
        App::reload,
    ));
    items.push(icon_button(
        "open",
        Icon::Folder,
        "Open…  ⌘O",
        true,
        App::choose_file,
    ));
    let dark = app.dark();
    items.push(icon_button(
        "theme",
        if dark { Icon::Sun } else { Icon::Moon },
        if dark {
            "Light appearance"
        } else {
            "Dark appearance"
        },
        true,
        move |app: &mut App| app.dark = Some(!dark),
    ));
    hstack(items)
        .key("toolbar")
        .height(TOOLBAR)
        .padding(14., Edge::Horizontal)
        .gap(4., Axis::Horizontal)
        .align(Align::Center)
        .drag_region(true)
        .border_sides(Sides::BOTTOM)
        .border_color(Role::Border)
        .border_width(1.)
        .into()
}

fn group(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn outline(app: &App, doc: &DocumentIr) -> Element<App> {
    let top = doc.outline.iter().map(|i| i.level).min().unwrap_or(1);
    let rows = heading_rows(&render::units(&doc.blocks));
    // The section being read: the last heading at or above the first row
    // on screen.
    let current = doc
        .outline
        .iter()
        .filter_map(|item| rows.get(&item.id).map(|row| (item, *row)))
        .filter(|(_, row)| *row <= app.first_visible)
        .max_by_key(|(_, row)| *row)
        .map(|(item, _)| item.id.clone());
    let items: Vec<Element<App>> = doc
        .outline
        .iter()
        .filter(|item| item.level <= top + 2)
        .filter_map(|item| {
            let row = *rows.get(&item.id)?;
            let id = render::key_of(&item.id);
            let depth = f32::from(item.level - top);
            let active = current.as_deref() == Some(item.id.as_str());
            Some(
                button(item.text.clone())
                    .key(format!("outline-{id}"))
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Small)
                    .label_align(Align::Start)
                    .label_inset(10. + 14. * depth)
                    .font_size(if depth == 0. { 13. } else { 12.5 })
                    .font_weight(if active { 600 } else { 400 })
                    .max_lines(1)
                    .color(if active || depth == 0. { Role::BaseContent } else { Role::Muted })
                    .background(if active {
                        Role::BaseContent.alpha(8)
                    } else {
                        "#00000000".into()
                    })
                    .rounded(7.)
                    .fill_width()
                    .on_click(move |app: &mut App| app.jump_to(row))
                    .into(),
            )
        })
        .collect();
    vstack![
        hstack![
            spacer(),
            icon_button(
                "hide-outline",
                Icon::List,
                "Hide outline  ⌘L",
                true,
                |app: &mut App| app.outline = false,
            ),
        ]
        .height(TOOLBAR)
        .padding(10., Edge::Right)
        .align(Align::Center)
        .drag_region(true),
        vstack![
            text("ON THIS PAGE")
                .font_size(10.5)
                .font_weight(600)
                .letter_spacing(0.07)
                .color(Role::Muted)
        ]
        .padding(20., Edge::Left)
        .padding(6., Edge::Bottom),
        vstack(items)
            .key("outline")
            .gap(1., Axis::Vertical)
            .padding(10., Edge::Horizontal)
            .padding(12., Edge::Bottom)
            .flex(1.)
            .scroll(Scroll::Vertical)
            .scrollbars(Scrollbars::Hidden),
    ]
    .key("sidebar")
    .width(SIDEBAR)
    .flex(0.)
    .background(Role::Base200.alpha(55))
    .border_sides(Sides::RIGHT)
    .border_color(Role::Border)
    .border_width(1.)
    .into()
}

fn body(app: &App) -> Element<App> {
    match (&app.doc, &app.error) {
        (_, Some(error)) => empty_state("Something went wrong", error),
        (None, None) => empty_state(
            "Open a Markdown file",
            "Drop a .md file below, or press ⌘O to choose one.",
        ),
        (Some(doc), None) => document(app, doc),
    }
}

fn empty_state(title: &str, detail: &str) -> Element<App> {
    vstack![
        text(title)
            .font_size(22.)
            .font_weight(700)
            .font_family("SF Pro Display"),
        text(detail)
            .color(Role::Muted)
            .text_align(TextAlign::Center)
            .max_width(440.),
        vstack![
            drop_zone()
                .key("drop")
                .title("Drop a Markdown file")
                .description(".md, .markdown or .txt")
                .accept(MARKDOWN)
                .multiple(false)
                .height(190.)
                .on_drop(|app: &mut App, paths| {
                    if let Some(path) = paths.into_iter().next() {
                        app.visit(path);
                    }
                })
                .on_browse(App::choose_file),
        ]
        .width(460.)
        .padding(10., Edge::Top),
    ]
    .key("empty")
    .flex(1.)
    .gap(14., Axis::Vertical)
    .align(Align::Center)
    .flow(Flow::Center)
    .padding(40., Edge::All)
    .into()
}

fn document(app: &App, doc: &DocumentIr) -> Element<App> {
    let units = render::units(&doc.blocks);
    let ctx = render::Ctx {
        images: &app.images,
        copied: app.copied.as_deref(),
    };
    let estimates: Vec<f32> = (0..units.len())
        .map(|i| render::estimate(&units, i, &app.images))
        .collect();
    if std::env::var_os("MDSTAR_DEBUG").is_some() {
        eprintln!("units {} window {:?}", units.len(), app.window);
    }
    lazy_list(units.len(), |i| render::row(&units, i, &ctx))
        .key("document")
        .estimates(estimates)
        .window(app.window.clone())
        .overscan(4)
        .generation(app.generation)
        .scroll_to_row(app.jump.0, app.jump.1)
        .on_window_change(|app: &mut App, window| {
            // Rows above the viewport are overscan; the first visible row
            // follows them (or is row 0 at the top).
            app.first_visible = if window.start == 0 { 0 } else { window.start + 4 };
            app.window = window;
        })
        .flex(1.)
        .into()
}

/// A neutral reading palette: zinc greys with a blue accent.
fn palette(dark: bool) -> Palette {
    let roles: &[(Role, &str)] = if dark {
        &[
            (Role::Base100, "#17171A"),
            (Role::Base200, "#1F1F23"),
            (Role::Base300, "#2A2A2F"),
            (Role::BaseContent, "#E7E7EA"),
            (Role::Muted, "#9C9CA6"),
            (Role::Border, "#2C2C32"),
            (Role::Primary, "#6AA6FF"),
            (Role::PrimaryContent, "#0B1220"),
            (Role::Accent, "#6AA6FF"),
            (Role::AccentContent, "#0B1220"),
            (Role::Info, "#6AA6FF"),
            (Role::Success, "#4ADE80"),
            (Role::Warning, "#FBBF24"),
            (Role::Error, "#F87171"),
        ]
    } else {
        &[
            (Role::Base100, "#FFFFFF"),
            (Role::Base200, "#F5F5F7"),
            (Role::Base300, "#E8E8EC"),
            (Role::BaseContent, "#1D1D20"),
            (Role::Muted, "#6E6E78"),
            (Role::Border, "#E6E6EA"),
            (Role::Primary, "#1F6FEB"),
            (Role::PrimaryContent, "#FFFFFF"),
            (Role::Accent, "#1F6FEB"),
            (Role::AccentContent, "#FFFFFF"),
            (Role::Info, "#1F6FEB"),
            (Role::Success, "#16A34A"),
            (Role::Warning, "#B45309"),
            (Role::Error, "#DC2626"),
        ]
    };
    let mut p = if dark {
        Palette::dark()
    } else {
        Palette::light()
    };
    for (role, hex) in roles {
        if let Some(c) = parse_hex(hex) {
            p.set(*role, c);
        }
    }
    p
}

fn theme(app: &App, system: ThemeMode) -> UiTheme {
    app.system_dark.set(!matches!(system, ThemeMode::Light));
    UiTheme::from_palette(palette(app.dark()))
}

fn commands() -> CommandRegistry<App> {
    CommandRegistry::new()
        .command(Command::new("open", "Open…", App::choose_file).shortcut(Shortcut::primary('o')))
        .command(
            Command::new("reload", "Reload", App::reload)
                .shortcut(Shortcut::primary('r'))
                .enabled_when(|app: &App| app.path.is_some()),
        )
        .command(
            Command::new("outline", "Toggle outline", |app: &mut App| {
                app.outline = !app.outline
            })
            .shortcut(Shortcut::primary('l'))
            .enabled_when(|app: &App| app.doc.as_ref().is_some_and(|d| d.outline.len() > 1)),
        )
        .command(
            // Volt shortcuts take letters and digits only, so Back has none yet.
            Command::new("back", "Back", App::back)
                .enabled_when(|app: &App| !app.history.is_empty()),
        )
}

struct Args {
    file: Option<PathBuf>,
    dark: Option<bool>,
    snapshot: Option<PathBuf>,
    size: [f32; 2],
    scale: f32,
    /// Snapshot only: scroll the document down this many points first.
    scroll: f32,
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        file: None,
        dark: None,
        snapshot: None,
        size: [1100., 800.],
        scale: 2.,
        scroll: 0.,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--theme" => match it.next().as_deref() {
                Some("dark") => a.dark = Some(true),
                Some("light") => a.dark = Some(false),
                Some("system") => a.dark = None,
                _ => return Err("--theme takes light, dark or system".into()),
            },
            "--snapshot" => a.snapshot = it.next().map(PathBuf::from),
            "--size" => {
                let v = it.next().ok_or("--size takes WIDTHxHEIGHT")?;
                let (w, h) = v.split_once('x').ok_or("--size takes WIDTHxHEIGHT")?;
                a.size = [
                    w.parse().map_err(|_| "bad width")?,
                    h.parse().map_err(|_| "bad height")?,
                ];
            }
            "--scroll" => {
                a.scroll = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--scroll takes points")?;
            }
            "--scale" => {
                a.scale = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--scale takes a number")?;
            }
            "-h" | "--help" => {
                println!(
                    "mdstar-volt [FILE.md] [--theme light|dark|system] [--snapshot PNG --size WxH --scale N]"
                );
                std::process::exit(0);
            }
            file if !file.starts_with('-') => a.file = Some(PathBuf::from(file)),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(a)
}

/// Render one frame offscreen (GPU when available) to a PNG.
fn snapshot(
    app: &mut App,
    path: &Path,
    size: [f32; 2],
    scale: f32,
    scroll: f32,
) -> Result<(), String> {
    let (doc, mut handlers) = view(app).build().map_err(|e| e.to_string())?.into_parts();
    let mut runtime =
        Runtime::new(doc, &handlers, &Bindings::default()).map_err(|e| e.to_string())?;
    let viewport = volt_ui_kit::Viewport::new(size[0] * scale, size[1] * scale, scale)
        .ok_or("invalid snapshot size")?;
    let mut painter = RasterPainter::new(viewport);
    // As the window does: lay out, then rebuild while the document asks for
    // a different window of rows.
    fn settle(
        runtime: &mut Runtime,
        handlers: &mut volt_ui_kit::declarative::Handlers<'static, App>,
        app: &mut App,
        viewport: volt_ui_kit::Viewport,
        painter: &mut RasterPainter,
    ) -> Result<(), String> {
        for _ in 0..16 {
            runtime
                .layout(viewport, &Bindings::default(), painter)
                .map_err(|e| e.to_string())?;
            if !runtime
                .dispatch_virtual_windows(handlers, app)
                .map_err(|e| e.to_string())?
            {
                break;
            }
            let (doc, next) = view(app).build().map_err(|e| e.to_string())?.into_parts();
            runtime
                .replace_document(doc, &next, &Bindings::default())
                .map_err(|e| e.to_string())?;
            *handlers = next;
        }
        Ok(())
    }
    settle(&mut runtime, &mut handlers, app, viewport, &mut painter)?;
    // Wheel over the document column in steps, as a trackpad would.
    let mut left = scroll;
    while left > 0. {
        let step = left.min(400.);
        runtime.scroll_at((size[0] - 300.) * scale, size[1] * 0.5 * scale, [0., step]);
        settle(&mut runtime, &mut handlers, app, viewport, &mut painter)?;
        left -= step;
    }
    runtime.settle_animations();
    if std::env::var_os("MDSTAR_DEBUG").is_some() {
        for id in ["document", "margins", "page"] {
            if let Some(b) = runtime.boxes().iter().find(|b| b.id == id) {
                eprintln!("{id} bounds {:?}", b.bounds.bounds());
            }
        }
    }
    let ui = theme(
        app,
        if app.dark() {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
    );
    let batches = runtime.paint_ui(&mut painter, &ui);
    // The window material stands in as the sidebar's base colour offscreen.
    let clear = ui.resolve("@base-300").unwrap_or([0., 0., 0., 1.]);
    let pixmap = match volt_ui_kit::native::GpuOffscreenRenderer::new() {
        Ok(mut gpu) => gpu.render(&mut painter, &batches, clear)?,
        Err(_) => painter.rasterize(&batches, clear)?,
    };
    pixmap.save_png(path).map_err(|e| e.to_string())?;
    println!(
        "Rendered {} elements to {}",
        runtime.boxes().len(),
        path.display()
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = args()?;
    let app_dark = args.dark;
    let mut app = App {
        dark: app_dark,
        ..App::default()
    };
    app.system_dark.set(true);
    if let Some(file) = args.file {
        app.open(file);
    }
    if let Some(path) = &args.snapshot {
        return Ok(snapshot(&mut app, path, args.size, args.scale, args.scroll)?);
    }
    Application::new("MD Star", app, view)
        .size(1180, 820)
        .chrome(WindowChrome::Overlay)
        .background(WindowBackground::Blur { opacity: 0.6 })
        .theme_with(theme)
        .commands(commands())
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_valid() {
        commands().validate().unwrap();
    }

    #[test]
    fn links_resolve_to_anchors_documents_or_safe_external_targets() {
        let here = Some(Path::new("/docs/guide/intro.md"));
        assert_eq!(
            link_action("#Install", here),
            LinkAction::Anchor("install".into())
        );
        assert_eq!(
            link_action("setup.md#First-Run", here),
            LinkAction::Document("/docs/guide/setup.md".into(), Some("first-run".into()))
        );
        assert_eq!(
            link_action("../api%20notes.markdown", here),
            LinkAction::Document("/docs/guide/../api notes.markdown".into(), None)
        );
        for url in ["https://example.com/a?b", "http://x.dev", "mailto:a@b.c"] {
            assert_eq!(link_action(url, here), LinkAction::External(url.into()));
        }
        for unsafe_target in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "vscode://open",
            "slack:open",
            "notes.pdf",
            "",
        ] {
            assert_eq!(
                link_action(unsafe_target, here),
                LinkAction::Ignore,
                "{unsafe_target}"
            );
        }
    }

    /// Long documents used to stop part way through once the view hit the
    /// element budget. Rows are now built on demand, so the last one is
    /// reachable however long the file is.
    #[test]
    fn the_end_of_a_very_long_document_is_reachable() {
        let mut source = String::new();
        for i in 0..1500 {
            source.push_str(&format!(
                "## Section {i}\n\nParagraph {i} with **bold** and `code`.\n\n- one\n- two\n\n```rust\nfn f{i}() {{}}\n```\n\n"
            ));
        }
        source.push_str("The very last line.\n");
        let dir = std::env::temp_dir().join(format!("mdstar-long-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("long.md");
        std::fs::write(&file, source).unwrap();
        let mut app = App::default();
        app.open(file);
        let (rows, last) = {
            let units = render::units(&app.doc.as_ref().unwrap().blocks);
            (units.len(), render::unit_key(&units, units.len() - 1))
        };
        assert!(rows > 6000, "{rows} rows");
        app.jump_to(rows - 1);
        let viewport = volt_ui_kit::Viewport::new(1000., 800., 1.).unwrap();
        let mut metrics = RasterPainter::new(viewport);
        let (doc, mut handlers) = view(&app).build().unwrap().into_parts();
        let mut runtime = Runtime::new(doc, &handlers, &Bindings::default()).unwrap();
        for _ in 0..16 {
            runtime
                .layout(viewport, &Bindings::default(), &mut metrics)
                .unwrap();
            if !runtime
                .dispatch_virtual_windows(&mut handlers, &mut app)
                .unwrap()
            {
                break;
            }
            let (doc, next) = view(&app).build().unwrap().into_parts();
            runtime
                .replace_document(doc, &next, &Bindings::default())
                .unwrap();
            handlers = next;
        }
        assert_eq!(app.window.end, rows, "window {:?}", app.window);
        assert!(
            runtime.boxes().iter().any(|b| b.id == last),
            "the last row {last} is built"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn every_markdown_fixture_builds_a_valid_view() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files = vec![
            root.join("README.md"),
            root.join("tests/fixtures/sample.md"),
        ];
        if let Ok(dir) = std::fs::read_dir(root.join("docs")) {
            files.extend(
                dir.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "md")),
            );
        }
        for file in files {
            let mut app = App::default();
            app.open(file.clone());
            assert!(app.error.is_none(), "{}: {:?}", file.display(), app.error);
            for outline in [true, false] {
                app.outline = outline;
                view(&app)
                    .build()
                    .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            }
        }
    }
}
