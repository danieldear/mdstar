//! MD Star reader drawn natively with Volt UI Kit.
//!
//! ```text
//! mdstar-volt [FILE.md | FOLDER]... [--theme light|dark|system]
//! mdstar-volt FILE.md --snapshot out.png [--size 1100x800] [--scale 2] [--theme dark]
//! ```

#[cfg(target_os = "macos")]
use mdstar_volt::macos;
use mdstar_volt::{App, app, cli, export, settings, view};

use std::path::{Path, PathBuf};

use volt_ui_kit::{
    ThemeMode, UiTheme,
    declarative::{Bindings, Handlers, Runtime},
    native::{
        Application, CloseDecision, Command, CommandRegistry, RasterPainter, Shortcut,
        WindowChrome,
        system::{DesktopCommand, DesktopConfig, DesktopMenu},
    },
};

use app::Mode;
use settings::{Appearance, Settings, SidebarTab};

fn theme(app: &App, system: ThemeMode) -> UiTheme {
    app.system_dark.set(!matches!(system, ThemeMode::Light));
    UiTheme::from_palette(app.theme().palette(app.accent()))
}

fn has_doc(app: &App) -> bool {
    app.doc.is_some()
}

/// ⌘1 to ⌘9 show tabs by position, ⌘9 the last.
const TAB_KEYS: [(&str, char); 9] = [
    ("tab-1", '1'),
    ("tab-2", '2'),
    ("tab-3", '3'),
    ("tab-4", '4'),
    ("tab-5", '5'),
    ("tab-6", '6'),
    ("tab-7", '7'),
    ("tab-8", '8'),
    ("tab-9", '9'),
];

fn commands() -> CommandRegistry<App> {
    let primary = Shortcut::primary;
    let mut registry = CommandRegistry::new();
    for (n, (id, key)) in TAB_KEYS.into_iter().enumerate() {
        registry = registry.command(
            Command::new(id, format!("Show Tab {}", n + 1), move |app: &mut App| {
                let i = if n == 8 {
                    app.tabs.len().saturating_sub(1)
                } else {
                    n
                };
                app.switch_tab(i);
            })
            .shortcut(primary(key))
            .enabled_when(move |app: &App| app.tabs.len() > 1 && (n == 8 || n < app.tabs.len())),
        );
    }
    registry
        .command(Command::new("settings", "Settings…", App::open_settings).shortcut(primary(',')))
        .command(Command::new("new", "New Document…", App::new_document).shortcut(primary('n')))
        .command(Command::new("open", "Open…", App::choose).shortcut(primary('o')))
        .command(
            Command::new("open-folder", "Open Folder…", App::choose_folder)
                .shortcut(primary('o').shift(true)),
        )
        .command(Command::new(
            "add-folder",
            "Add Folder to Workspace…",
            App::choose_folder_to_add,
        ))
        .command(Command::new("new-tab", "New Tab", App::new_tab).shortcut(primary('t')))
        .command(
            Command::new("close-tab", "Close Tab", |app: &mut App| {
                app.close_tab(app.active)
            })
            .shortcut(primary('w')),
        )
        .command(
            Command::new("next-tab", "Show Next Tab", |app: &mut App| {
                app.cycle_tab(1)
            })
            .shortcut(primary(']').shift(true))
            .enabled_when(|app: &App| app.tabs.len() > 1),
        )
        .command(
            Command::new("previous-tab", "Show Previous Tab", |app: &mut App| {
                app.cycle_tab(-1)
            })
            .shortcut(primary('[').shift(true))
            .enabled_when(|app: &App| app.tabs.len() > 1),
        )
        .command(
            Command::new("save", "Save", |app: &mut App| {
                app.save();
            })
            .shortcut(primary('s'))
            .enabled_when(|app: &App| app.doc.as_ref().is_some_and(|d| d.dirty())),
        )
        .command(
            Command::new("bold", "Bold", |app: &mut App| {
                app.wrap_selection("**", "**")
            })
            .shortcut(primary('b'))
            .enabled_when(App::editing),
        )
        .command(
            Command::new("italic", "Italic", |app: &mut App| {
                app.wrap_selection("_", "_")
            })
            .shortcut(primary('i'))
            .enabled_when(App::editing),
        )
        .command(
            Command::new("inline-code", "Code", |app: &mut App| {
                app.wrap_selection("`", "`")
            })
            .shortcut(primary('e').shift(true))
            .enabled_when(App::editing),
        )
        .command(
            Command::new("link", "Link", App::insert_link)
                .shortcut(primary('k'))
                .enabled_when(App::editing),
        )
        .command(
            Command::new("toggle-edit", "Edit", App::toggle_edit)
                .shortcut(primary('e'))
                .enabled_when(|app: &App| app.doc.as_ref().is_some_and(|d| d.editable())),
        )
        .command(
            Command::new("toggle-source", "Toggle Source", App::toggle_edit)
                .shortcut(primary('/'))
                .enabled_when(|app: &App| app.doc.as_ref().is_some_and(|d| d.editable())),
        )
        .command(
            Command::new("mode-preview", "Preview", |app: &mut App| {
                app.set_mode(Mode::Preview)
            })
            .enabled_when(has_doc),
        )
        .command(
            Command::new("mode-split", "Edit with Preview", |app: &mut App| {
                app.set_mode(Mode::Split)
            })
            .enabled_when(|app: &App| app.doc.as_ref().is_some_and(|d| d.editable())),
        )
        .command(
            Command::new("mode-source", "Edit Source", |app: &mut App| {
                app.set_mode(Mode::Source)
            })
            .enabled_when(|app: &App| app.doc.as_ref().is_some_and(|d| d.editable())),
        )
        .command(
            Command::new("reload", "Reload", App::reload)
                .shortcut(primary('r'))
                .enabled_when(has_doc),
        )
        .command(
            Command::new("find", "Find…", App::open_find)
                .shortcut(primary('f'))
                .enabled_when(has_doc),
        )
        .command(
            Command::new("find-next", "Find Next", |app: &mut App| {
                app.find_step(true)
            })
            .shortcut(primary('g'))
            .enabled_when(|app: &App| app.find.as_ref().is_some_and(|f| !f.hits.is_empty())),
        )
        .command(
            Command::new("find-previous", "Find Previous", |app: &mut App| {
                app.find_step(false)
            })
            .shortcut(primary('g').shift(true))
            .enabled_when(|app: &App| app.find.as_ref().is_some_and(|f| !f.hits.is_empty())),
        )
        .command(
            Command::new("sidebar", "Toggle Sidebar", App::toggle_sidebar).shortcut(primary('\\')),
        )
        .command(
            Command::new("files", "Show Files", |app: &mut App| {
                app.show_sidebar(SidebarTab::Files)
            })
            .shortcut(primary('1').alt(true)),
        )
        .command(
            Command::new("outline", "Show Outline", |app: &mut App| {
                app.show_sidebar(SidebarTab::Outline)
            })
            .shortcut(primary('2').alt(true)),
        )
        .command(
            Command::new("bigger", "Bigger Text", |app: &mut App| app.zoom(1))
                .shortcut(primary('=')),
        )
        .command(
            Command::new("smaller", "Smaller Text", |app: &mut App| app.zoom(-1))
                .shortcut(primary('-')),
        )
        .command(
            Command::new("actual-size", "Actual Size", |app: &mut App| app.zoom(0))
                .shortcut(primary('0')),
        )
        .command(
            Command::new("appearance", "Toggle Light and Dark", |app: &mut App| {
                let dark = app.theme().dark();
                app.configure(|s| {
                    s.appearance = if dark {
                        Appearance::Light
                    } else {
                        Appearance::Dark
                    }
                })
            })
            .shortcut(primary('d').shift(true)),
        )
        .command(
            Command::new("back", "Back", App::go_back)
                .shortcut(primary('['))
                .enabled_when(|app: &App| !app.back.is_empty()),
        )
        .command(
            Command::new("forward", "Forward", App::go_forward)
                .shortcut(primary(']'))
                .enabled_when(|app: &App| !app.forward.is_empty()),
        )
}

/// The menu bar; items name commands registered in [`commands`].
fn menus() -> DesktopConfig {
    let item = DesktopCommand::new;
    let sep = DesktopCommand::separator;
    DesktopConfig::new("MD Star")
        .app_menu([item("settings", "Settings…")])
        .menu(DesktopMenu::new(
            "File",
            [
                item("new", "New Document…"),
                item("new-tab", "New Tab"),
                item("open", "Open…"),
                item("open-folder", "Open Folder…"),
                item("add-folder", "Add Folder to Workspace…"),
                sep(),
                item("save", "Save"),
                item("reload", "Reload"),
                sep(),
                item("close-tab", "Close Tab"),
            ],
        ))
        .menu(DesktopMenu::new(
            "Edit",
            [
                item("bold", "Bold"),
                item("italic", "Italic"),
                item("inline-code", "Code"),
                item("link", "Link"),
                sep(),
                item("find", "Find…"),
                item("find-next", "Find Next"),
                item("find-previous", "Find Previous"),
            ],
        ))
        .menu(DesktopMenu::new(
            "View",
            [
                item("mode-preview", "Preview"),
                item("mode-split", "Edit with Preview"),
                item("mode-source", "Edit Source"),
                item("toggle-edit", "Edit"),
                sep(),
                item("sidebar", "Toggle Sidebar"),
                item("files", "Show Files"),
                item("outline", "Show Outline"),
                sep(),
                item("bigger", "Bigger Text"),
                item("smaller", "Smaller Text"),
                item("actual-size", "Actual Size"),
                sep(),
                item("appearance", "Toggle Light and Dark"),
            ],
        ))
        .menu(DesktopMenu::new(
            "Go",
            [
                item("back", "Back"),
                item("forward", "Forward"),
                sep(),
                item("next-tab", "Show Next Tab"),
                item("previous-tab", "Show Previous Tab"),
            ],
        ))
}

struct Args {
    /// Files open in tabs; a folder becomes the workspace.
    files: Vec<PathBuf>,
    appearance: Option<Appearance>,
    snapshot: Option<PathBuf>,
    size: [f32; 2],
    scale: f32,
    /// Snapshot only: scroll the document down this many points first.
    scroll: f32,
    /// Snapshot only: open find with this query, or open settings.
    find: Option<String>,
    settings: bool,
    tab: Option<SidebarTab>,
    mode: Option<Mode>,
    /// Snapshot only: type `/` on a new last line to show the insert menu.
    slash: bool,
    /// Snapshot only: select all rendered text.
    select_all: bool,
    /// Snapshot only: open the Appearance popover.
    popover: bool,
    /// Snapshot only: hover the element with this key (shows tooltips).
    hover: Option<String>,
    /// Write the first file as PDF pages here instead of opening a window.
    pdf: Option<PathBuf>,
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        files: Vec::new(),
        appearance: None,
        snapshot: None,
        size: [1100., 800.],
        scale: 2.,
        scroll: 0.,
        find: None,
        settings: false,
        tab: None,
        mode: None,
        slash: false,
        select_all: false,
        popover: false,
        hover: None,
        pdf: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--theme" => {
                a.appearance = Some(match it.next().as_deref() {
                    Some("dark") => Appearance::Dark,
                    Some("light") => Appearance::Light,
                    Some("system") => Appearance::System,
                    _ => return Err("--theme takes light, dark or system".into()),
                })
            }
            "--snapshot" => a.snapshot = it.next().map(PathBuf::from),
            "--pdf" => a.pdf = it.next().map(PathBuf::from),
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
            "--find" => a.find = Some(it.next().ok_or("--find takes a query")?),
            "--settings" => a.settings = true,
            "--slash" => a.slash = true,
            "--select-all" => a.select_all = true,
            "--appearance" => a.popover = true,
            "--hover" => a.hover = it.next(),
            "--mode" => {
                a.mode = Some(match it.next().as_deref() {
                    Some("preview") => Mode::Preview,
                    Some("split") => Mode::Split,
                    Some("source") => Mode::Source,
                    _ => return Err("--mode takes preview, split or source".into()),
                })
            }
            "--tab" => {
                a.tab = Some(match it.next().as_deref() {
                    Some("files") => SidebarTab::Files,
                    Some("outline") => SidebarTab::Outline,
                    _ => return Err("--tab takes files or outline".into()),
                })
            }
            "-h" | "--help" => {
                println!(
                    "mdstar-volt [FILE.md | FOLDER] [--theme light|dark|system] [--snapshot PNG --size WxH --scale N --scroll PT --find QUERY --settings --tab files|outline]"
                );
                std::process::exit(0);
            }
            file if !file.starts_with('-') => a.files.push(PathBuf::from(file)),
            // Launched from Finder or the Dock, macOS may add its own flags
            // (such as -psn_…); never refuse to start over an option.
            other => eprintln!("MD Star: ignoring option {other}"),
        }
    }
    Ok(a)
}

/// Lay out, then rebuild while lazy lists ask for different rows, as the
/// window does after each change.
fn settle(
    runtime: &mut Runtime,
    handlers: &mut Handlers<'static, App>,
    app: &mut App,
    viewport: volt_ui_kit::Viewport,
    metrics: &mut RasterPainter,
) -> Result<(), String> {
    for _ in 0..16 {
        runtime
            .layout(viewport, &Bindings::default(), metrics)
            .map_err(|e| e.to_string())?;
        let windows = runtime
            .dispatch_virtual_windows(handlers, app)
            .map_err(|e| e.to_string())?;
        let selections = runtime
            .dispatch_editor_selections(handlers, app)
            .map_err(|e| e.to_string())?;
        let widths = runtime
            .dispatch_widths(handlers, app)
            .map_err(|e| e.to_string())?;
        if !windows && !selections && !widths {
            break;
        }
        let (doc, next) = view::view(app)
            .build()
            .map_err(|e| e.to_string())?
            .into_parts();
        runtime
            .replace_document(doc, &next, &Bindings::default())
            .map_err(|e| e.to_string())?;
        *handlers = next;
    }
    Ok(())
}

/// Render one frame offscreen (GPU when available) to a PNG.
fn snapshot(
    app: &mut App,
    path: &Path,
    size: [f32; 2],
    scale: f32,
    scroll: f32,
    select_all: bool,
    hover: Option<&str>,
) -> Result<(), String> {
    let (doc, mut handlers) = view::view(app)
        .build()
        .map_err(|e| e.to_string())?
        .into_parts();
    let mut runtime =
        Runtime::new(doc, &handlers, &Bindings::default()).map_err(|e| e.to_string())?;
    let viewport = volt_ui_kit::Viewport::new(size[0] * scale, size[1] * scale, scale)
        .ok_or("invalid snapshot size")?;
    let mut painter = RasterPainter::new(viewport);
    settle(&mut runtime, &mut handlers, app, viewport, &mut painter)?;
    // Wheel over the document column in steps, as a trackpad would.
    let mut left = scroll;
    while left > 0. {
        let step = left.min(400.);
        runtime.scroll_at((size[0] - 300.) * scale, size[1] * 0.5 * scale, [0., step]);
        settle(&mut runtime, &mut handlers, app, viewport, &mut painter)?;
        left -= step;
    }
    if select_all {
        runtime.select_all_text(&mut painter);
    }
    if let Some(key) = hover {
        let center = runtime.boxes().iter().find(|b| b.id == key).map(|b| {
            let [x, y, w, h] = b.bounds.bounds();
            [x + w / 2., y + h / 2.]
        });
        if let Some([x, y]) = center {
            runtime.set_tooltips_active(true);
            runtime.pointer_move(x, y);
            settle(&mut runtime, &mut handlers, app, viewport, &mut painter)?;
        }
    }
    runtime.settle_animations();
    let ui = theme(app, ThemeMode::Light);
    let batches = runtime.paint_ui(&mut painter, &ui);
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

/// Errors print as a plain line (not Rust's `Error: "…"`) and exit with 1.
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("MD Star: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if cli::invoked_as_md(std::env::args_os().next().as_deref()) {
        cli::main();
    }
    let args = args()?;
    if let Some(out) = &args.pdf {
        let file = args.files.first().ok_or("--pdf needs a file to export")?;
        // Name the file in full, so "not found" says where it looked.
        let file = &std::path::absolute(file)?;
        let options = export::Options {
            dark: matches!(args.appearance, Some(Appearance::Dark)),
            ..export::Options::default()
        };
        std::fs::write(out, export::pdf(file, options)?)?;
        println!("Exported {} to {}", file.display(), out.display());
        return Ok(());
    }
    let snapshot_path = args.snapshot.clone();
    // Snapshots never read or write the user's settings.
    let settings_path = snapshot_path
        .is_none()
        .then(settings::settings_path)
        .flatten();
    let mut settings = settings_path
        .as_deref()
        .map(Settings::load)
        .unwrap_or_default();
    if let Some(appearance) = args.appearance {
        settings.appearance = appearance;
    }
    let mut app = App::new(settings, settings_path);
    for path in args.files {
        if path.is_dir() {
            app.open_folder(path);
        } else {
            app.open_in_tab(path);
        }
    }
    if let Some(path) = &snapshot_path {
        if let Some(query) = args.find {
            app.open_find();
            app.set_query(query);
        }
        if args.settings {
            app.open_settings();
        }
        app.appearance_open = args.popover;
        if let Some(tab) = args.tab {
            app.show_sidebar(tab);
        }
        if let Some(mode) = args.mode {
            app.set_mode(mode);
        }
        if args.slash {
            app.set_mode(Mode::Split);
            let text = app
                .doc
                .as_ref()
                .map(|d| format!("{}\n/", d.source.trim_end()));
            if let Some(text) = text {
                let end = text.len();
                app.edit(text);
                app.select = (end, end, 1);
            }
        }
        return Ok(snapshot(
            &mut app,
            path,
            args.size,
            args.scale,
            args.scroll,
            args.select_all,
            args.hover.as_deref(),
        )?);
    }
    Application::new("MD Star", app, view::view)
        .size(1180, 820)
        .chrome(WindowChrome::Overlay)
        .background_from_state(|app: &App| app.settings.window_background())
        .theme_with(theme)
        .commands(commands())
        .desktop(menus(), |_: &mut App, _| {})
        .messages(
            |app: &mut App, sender| {
                let wake = sender.clone();
                mdstar_volt::diagram::set_waker(std::sync::Arc::new(move || {
                    let _ = wake.send(app::Msg::MediaReady);
                }));
                app.sender = Some(sender);
                app.sync_watches();
            },
            App::message,
        )
        .before_launch(|app: &mut App| {
            #[cfg(target_os = "macos")]
            if let Some(sender) = app.sender.clone() {
                macos::receive_opened_files(sender);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = app;
        })
        // Edits are saved as you go; save what remains before closing.
        .on_close_request(|app: &mut App, _| {
            app.save_all();
            app.save_settings_now();
            CloseDecision::Close
        })
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use app::{LinkAction, link_action};
    use mdstar_volt::{diagram, render};

    #[test]
    fn narrow_panes_tighten_margins_and_fit_diagrams() {
        let mut app = App::new(Settings::default(), None);
        let wide = view::typography(&app);
        assert_eq!(
            (wide.room, wide.margin),
            (wide.measure, 56.),
            "unknown width: the measure"
        );
        app.page_width = Some(500.);
        let narrow = view::typography(&app);
        assert_eq!(narrow.margin, 18.);
        assert_eq!(narrow.room, 500. - 36.);
        let flow = diagram::diagram(
            "flowchart LR\n  A[Alpha] --> B[Bravo] --> C[Charlie] --> D[Delta] --> E[Echo]",
        );
        let s = diagram::scale(&flow, 16., 300.);
        let [w, _] = diagram::size(&flow).unwrap();
        assert!(
            w > 300. && s >= diagram::MIN_SCALE,
            "too wide to fit: shrinks to the minimum, then scrolls"
        );
        assert!(
            (diagram::scale(&flow, 16., w * 0.9) * w - w * 0.9).abs() < 1.,
            "fits when it can"
        );
    }

    #[test]
    fn commands_and_menus_are_valid() {
        let registry = commands();
        registry.validate().unwrap();
        let menus = menus();
        menus.validate().unwrap();
        let ids: Vec<&str> = menus
            .app_commands
            .iter()
            .chain(menus.menus.iter().flat_map(|m| &m.commands))
            .filter(|c| !c.separator)
            .map(|c| c.id.as_str())
            .collect();
        let app = App::default();
        let known: Vec<&str> = registry.entries(&app).iter().map(|e| e.id).collect();
        for id in ids {
            assert!(known.contains(&id), "menu item {id} has no command");
        }
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

    fn settle_view(app: &mut App) -> (Runtime, Handlers<'static, App>) {
        let viewport = volt_ui_kit::Viewport::new(1000., 800., 1.).unwrap();
        let mut metrics = RasterPainter::new(viewport);
        let (doc, mut handlers) = view::view(app).build().unwrap().into_parts();
        let mut runtime = Runtime::new(doc, &handlers, &Bindings::default()).unwrap();
        settle(&mut runtime, &mut handlers, app, viewport, &mut metrics).unwrap();
        (runtime, handlers)
    }

    fn temp_doc(name: &str, source: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("mdstar-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(format!("{name}.md"));
        std::fs::write(&file, source).unwrap();
        (dir, file)
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
        let (dir, file) = temp_doc("long", &source);
        let mut app = App::new(Settings::default(), None);
        app.open(file);
        let (rows, last) = {
            let units = app.doc.as_ref().unwrap().units();
            (units.len(), render::unit_key(&units, units.len() - 1))
        };
        assert!(rows > 6000, "{rows} rows");
        app.jump_to(rows - 1);
        let (runtime, _) = settle_view(&mut app);
        assert_eq!(app.window.end, rows, "window {:?}", app.window);
        assert!(
            runtime.boxes().iter().any(|b| b.id == last),
            "the last row {last} is built"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    /// Find counts every match, and stepping to one far down the document
    /// scrolls it into view.
    #[test]
    fn find_reaches_matches_in_rows_not_yet_built() {
        let mut source = String::new();
        for i in 0..400 {
            source.push_str(&format!("Paragraph {i} of filler text.\n\n"));
        }
        source.push_str("The needle is here.\n\nAnd a second NEEDLE.\n");
        let (dir, file) = temp_doc("find", &source);
        let mut app = App::new(Settings::default(), None);
        app.open(file);
        settle_view(&mut app);
        app.open_find();
        app.set_query("needle".into());
        let find = app.find.as_ref().unwrap();
        assert_eq!(find.summary(), "1 of 2");
        let target = find.current_hit().unwrap().row;
        assert!(target >= 400);
        let (runtime, _) = settle_view(&mut app);
        assert!(
            app.window.contains(&target),
            "{:?} holds {target}",
            app.window
        );
        assert!(runtime.boxes().iter().any(|b| b.id == "find-field"));
        app.find_step(true);
        assert_eq!(app.find.as_ref().unwrap().summary(), "2 of 2");
        app.close_find();
        assert!(app.find.is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    /// Rendered text selects with the pointer across blocks, skips list
    /// markers, and reaches the app (for Copy, Look Up, Share…).
    #[test]
    fn rendered_text_selects_across_blocks_and_reports_it() {
        let (dir, file) = temp_doc(
            "select",
            "# Title\n\nFirst paragraph here.\n\n- one item\n- two item\n",
        );
        let mut app = App::new(Settings::default(), None);
        app.open(file);
        let viewport = volt_ui_kit::Viewport::new(1000., 800., 1.).unwrap();
        let mut metrics = RasterPainter::new(viewport);
        let (doc, mut handlers) = view::view(&app).build().unwrap().into_parts();
        let mut runtime = Runtime::new(doc, &handlers, &Bindings::default()).unwrap();
        settle(
            &mut runtime,
            &mut handlers,
            &mut app,
            viewport,
            &mut metrics,
        )
        .unwrap();
        let find = |runtime: &Runtime, needle: &str| {
            runtime
                .boxes()
                .iter()
                .filter(|b| b.text().starts_with(needle))
                .map(|b| b.bounds.bounds())
                .next()
                .unwrap()
        };
        let first = find(&runtime, "First");
        let item = find(&runtime, "two item");
        assert!(runtime.text_select_press(
            first[0] + 1.,
            first[1] + first[3] / 2.,
            1,
            false,
            &mut metrics
        ));
        assert!(runtime.text_select_drag(
            item[0] + item[2] - 1.,
            item[1] + item[3] / 2.,
            &mut metrics
        ));
        runtime.text_select_release();
        runtime
            .dispatch_text_selection(&mut handlers, &mut app)
            .unwrap();
        assert_eq!(
            app.selected_text,
            "First paragraph here.\none item\ntwo item"
        );
        // A double-click picks one word.
        runtime.text_select_press(
            first[0] + 60.,
            first[1] + first[3] / 2.,
            2,
            false,
            &mut metrics,
        );
        runtime.text_select_release();
        assert_eq!(runtime.selected_text(), Some("paragraph"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn every_markdown_fixture_builds_with_every_panel_and_theme() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files = vec![
            root.join("README.md"),
            root.join("tests/fixtures/sample.md"),
            root.join("tests/fixtures/mermaid/extended.md"),
        ];
        if let Ok(dir) = std::fs::read_dir(root.join("docs")) {
            files.extend(
                dir.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "md")),
            );
        }
        for file in files {
            let mut app = App::new(Settings::default(), None);
            app.open(file.clone());
            assert!(app.error.is_none(), "{}: {:?}", file.display(), app.error);
            app.open_find();
            app.set_query("the".into());
            for (tab, sidebar, settings, typeface, mode) in [
                (SidebarTab::Files, true, false, "System", Mode::Preview),
                (SidebarTab::Outline, true, true, "New York", Mode::Split),
                (SidebarTab::Files, false, false, "Monospaced", Mode::Source),
            ] {
                app.mode = mode;
                app.settings.sidebar_tab = tab;
                app.settings.sidebar = sidebar;
                app.settings.typeface = typeface.into();
                app.show_settings = settings;
                app.appearance_open = !settings;
                view::view(&app)
                    .build()
                    .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            }
        }
    }

    #[test]
    fn opening_a_document_shows_its_folder_and_reveals_it_in_the_tree() {
        let (dir, file) = temp_doc("tree", "# Tree\n");
        let nested = dir.join("a/b/c.md");
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
        std::fs::write(&nested, "# C\n").unwrap();
        let mut app = App::new(Settings::default(), None);
        app.open(file);
        assert_eq!(app.tree.len(), 1, "the document's folder stands in");
        app.visit(nested.clone());
        assert!(app.expanded.contains(&dir.join("a/b")));
        app.go_back();
        assert_eq!(app.path().unwrap().file_name().unwrap(), "tree.md");
        app.go_forward();
        assert_eq!(app.path(), Some(nested.as_path()));
        std::fs::remove_dir_all(dir).ok();
    }
}
