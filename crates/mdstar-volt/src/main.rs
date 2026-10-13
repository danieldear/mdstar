//! MD Star reader drawn natively with Volt UI Kit.
//!
//! ```text
//! mdstar-volt [FILE.md | FOLDER] [--theme light|dark|system]
//! mdstar-volt FILE.md --snapshot out.png [--size 1100x800] [--scale 2] [--theme dark]
//! ```

mod app;
mod find;
mod render;
mod settings;
mod view;
mod workspace;

use std::path::{Path, PathBuf};

use volt_ui_kit::{
    ThemeMode, UiTheme,
    declarative::{Bindings, Handlers, Runtime},
    native::{
        Application, Command, CommandRegistry, RasterPainter, Shortcut, WindowBackground,
        WindowChrome,
        system::{DesktopCommand, DesktopConfig, DesktopMenu},
    },
};

pub use app::App;
use settings::{Appearance, Settings, SidebarTab};

pub const MARKDOWN: [&str; 5] = ["md", "markdown", "mdown", "mkd", "txt"];

fn theme(app: &App, system: ThemeMode) -> UiTheme {
    app.system_dark.set(!matches!(system, ThemeMode::Light));
    UiTheme::from_palette(app.theme().palette())
}

fn has_doc(app: &App) -> bool {
    app.doc.is_some()
}

fn commands() -> CommandRegistry<App> {
    let primary = Shortcut::primary;
    CommandRegistry::new()
        .command(Command::new("settings", "Settings…", App::open_settings).shortcut(primary(',')))
        .command(Command::new("open", "Open…", App::choose_file).shortcut(primary('o')))
        .command(
            Command::new("add-folder", "Add Folder to Workspace…", App::choose_folder)
                .shortcut(primary('o').shift(true)),
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
            Command::new("find-next", "Find Next", |app: &mut App| app.find_step(true))
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
            Command::new("sidebar", "Toggle Sidebar", App::toggle_sidebar)
                .shortcut(primary('\\')),
        )
        .command(
            Command::new("files", "Show Files", |app: &mut App| {
                app.show_sidebar(SidebarTab::Files)
            })
            .shortcut(primary('1')),
        )
        .command(
            Command::new("outline", "Show Outline", |app: &mut App| {
                app.show_sidebar(SidebarTab::Outline)
            })
            .shortcut(primary('2')),
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
                item("open", "Open…"),
                item("add-folder", "Add Folder to Workspace…"),
                sep(),
                item("reload", "Reload"),
            ],
        ))
        .menu(DesktopMenu::new(
            "Edit",
            [
                item("find", "Find…"),
                item("find-next", "Find Next"),
                item("find-previous", "Find Previous"),
            ],
        ))
        .menu(DesktopMenu::new(
            "View",
            [
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
            [item("back", "Back"), item("forward", "Forward")],
        ))
}

struct Args {
    file: Option<PathBuf>,
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
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        file: None,
        appearance: None,
        snapshot: None,
        size: [1100., 800.],
        scale: 2.,
        scroll: 0.,
        find: None,
        settings: false,
        tab: None,
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
            file if !file.starts_with('-') => a.file = Some(PathBuf::from(file)),
            other => return Err(format!("unknown option {other}")),
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
        if !runtime
            .dispatch_virtual_windows(handlers, app)
            .map_err(|e| e.to_string())?
        {
            break;
        }
        let (doc, next) = view::view(app).build().map_err(|e| e.to_string())?.into_parts();
        runtime
            .replace_document(doc, &next, &Bindings::default())
            .map_err(|e| e.to_string())?;
        *handlers = next;
    }
    Ok(())
}

/// Render one frame offscreen (GPU when available) to a PNG.
fn snapshot(app: &mut App, path: &Path, size: [f32; 2], scale: f32, scroll: f32) -> Result<(), String> {
    let (doc, mut handlers) = view::view(app).build().map_err(|e| e.to_string())?.into_parts();
    let mut runtime = Runtime::new(doc, &handlers, &Bindings::default()).map_err(|e| e.to_string())?;
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
    runtime.settle_animations();
    let ui = theme(app, ThemeMode::Light);
    let batches = runtime.paint_ui(&mut painter, &ui);
    let clear = ui.resolve("@base-300").unwrap_or([0., 0., 0., 1.]);
    let pixmap = match volt_ui_kit::native::GpuOffscreenRenderer::new() {
        Ok(mut gpu) => gpu.render(&mut painter, &batches, clear)?,
        Err(_) => painter.rasterize(&batches, clear)?,
    };
    pixmap.save_png(path).map_err(|e| e.to_string())?;
    println!("Rendered {} elements to {}", runtime.boxes().len(), path.display());
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = args()?;
    let snapshot_path = args.snapshot.clone();
    // Snapshots never read or write the user's settings.
    let settings_path = snapshot_path.is_none().then(settings::settings_path).flatten();
    let mut settings = settings_path.as_deref().map(Settings::load).unwrap_or_default();
    if let Some(appearance) = args.appearance {
        settings.appearance = appearance;
    }
    let mut app = App::new(settings, settings_path);
    match args.file {
        Some(path) if path.is_dir() => app.add_folder(path),
        Some(path) => app.open(path),
        None => {}
    }
    if let Some(path) = &snapshot_path {
        if let Some(query) = args.find {
            app.open_find();
            app.set_query(query);
        }
        if args.settings {
            app.open_settings();
        }
        if let Some(tab) = args.tab {
            app.show_sidebar(tab);
        }
        return Ok(snapshot(&mut app, path, args.size, args.scale, args.scroll)?);
    }
    Application::new("MD Star", app, view::view)
        .size(1180, 820)
        .chrome(WindowChrome::Overlay)
        .background(WindowBackground::Blur { opacity: 0.6 })
        .theme_with(theme)
        .commands(commands())
        .desktop(menus(), |_: &mut App, _| {})
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use app::{LinkAction, link_action};

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
        assert_eq!(link_action("#Install", here), LinkAction::Anchor("install".into()));
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
            assert_eq!(link_action(unsafe_target, here), LinkAction::Ignore, "{unsafe_target}");
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
        let mut app = App::default();
        app.open(file);
        let (rows, last) = {
            let units = app.doc.as_ref().unwrap().units();
            (units.len(), render::unit_key(&units, units.len() - 1))
        };
        assert!(rows > 6000, "{rows} rows");
        app.jump_to(rows - 1);
        let (runtime, _) = settle_view(&mut app);
        assert_eq!(app.window.end, rows, "window {:?}", app.window);
        assert!(runtime.boxes().iter().any(|b| b.id == last), "the last row {last} is built");
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
        let mut app = App::default();
        app.open(file);
        settle_view(&mut app);
        app.open_find();
        app.set_query("needle".into());
        let find = app.find.as_ref().unwrap();
        assert_eq!(find.summary(), "1 of 2");
        let target = find.current_hit().unwrap().row;
        assert!(target >= 400);
        let (runtime, _) = settle_view(&mut app);
        assert!(app.window.contains(&target), "{:?} holds {target}", app.window);
        assert!(runtime.boxes().iter().any(|b| b.id == "find-field"));
        app.find_step(true);
        assert_eq!(app.find.as_ref().unwrap().summary(), "2 of 2");
        app.close_find();
        assert!(app.find.is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn every_markdown_fixture_builds_with_every_panel_and_theme() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files = vec![root.join("README.md"), root.join("tests/fixtures/sample.md")];
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
            app.open_find();
            app.set_query("the".into());
            for (tab, sidebar, settings, typeface) in [
                (SidebarTab::Files, true, false, "System"),
                (SidebarTab::Outline, true, true, "New York"),
                (SidebarTab::Files, false, false, "Monospaced"),
            ] {
                app.settings.sidebar_tab = tab;
                app.settings.sidebar = sidebar;
                app.settings.typeface = typeface.into();
                app.show_settings = settings;
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
        let mut app = App::default();
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
