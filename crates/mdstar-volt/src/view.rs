//! The window: sidebar, toolbar, find bar, document and settings.

use std::path::{Path, PathBuf};

use mdstar_core::document_ir::DocumentIr;
use volt_ui_kit::{ColorToken as Role, ui::prelude::*};

use crate::{
    MARKDOWN,
    app::{
        App, Doc, MenuTarget, Mode, file_action, file_name, heading_action, heading_rows,
        tab_action,
    },
    find,
    render::{self, Highlight, Typography},
    settings::{self, Appearance, CODE_FONTS, SidebarTab, TYPEFACES, Theme},
    workspace::{self, FileKind},
};

pub const TOOLBAR: f32 = 52.;
/// Leading space for the macOS traffic lights.
const TRAFFIC_LIGHTS: f32 = 78.;
const ROW: f32 = 28.;

pub fn view(app: &App) -> Element<App> {
    crate::diagram::begin_frame();
    let clear = "#00000000".to_string();
    let main = vstack![
        toolbar(app),
        tab_bar(app),
        save_banner(app),
        zstack![body(app), find_panel(app)]
            .key("stage")
            .align(Align::Stretch)
            .flex(1.),
    ]
    .key("main")
    .flex(1.)
    // A translucent window shows its own page-coloured clear through here.
    .background(if app.settings.translucent() {
        clear.clone()
    } else {
        Role::Base100.into()
    })
    .align(Align::Stretch);
    let content: Element<App> = if app.settings.sidebar {
        // Dragging the divider resizes the sidebar; it keeps that width as
        // the window resizes.
        split(sidebar(app), main)
            .key("panes")
            .first_size(app.settings.sidebar_width)
            .min_first(*settings::SIDEBAR_WIDTHS.start())
            .max_first(*settings::SIDEBAR_WIDTHS.end())
            .min_second(420.)
            .handle_size(8.)
            .handle_color(clear)
            .handle_style(SplitHandle::Reveal)
            .on_resize(App::resize_sidebar)
            .fill_width()
            .fill_height()
            .flex(1.)
            .into()
    } else {
        main.into()
    };
    zstack![
        content,
        slash_menu(app),
        appearance_popover(app),
        settings_dialog(app)
    ]
    .key("root")
    .align(Align::Stretch)
    .color(Role::BaseContent)
    .font_family("SF")
    .into()
}

/// A macOS-style segmented control: a quiet track with the chosen option
/// raised on a pill.
fn segmented<T: Copy + PartialEq + 'static>(
    key: &str,
    items: &[(T, &str)],
    selected: T,
    dark: bool,
    select: impl Fn(&mut App, T) + Clone + 'static,
) -> Element<App> {
    let buttons: Vec<Element<App>> = items
        .iter()
        .enumerate()
        .map(|(i, &(value, label))| {
            let chosen = value == selected;
            let select = select.clone();
            let mut b = button(label)
                .key(format!("{key}-{i}"))
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Small)
                .font_size(12.5)
                .font_weight(if chosen { 600 } else { 500 })
                .color(if chosen {
                    Role::BaseContent
                } else {
                    Role::Muted
                })
                .height(24.)
                .rounded(6.)
                .flex(1.)
                .accessible_label(label)
                .on_click(move |app: &mut App| select(app, value));
            if chosen {
                b = b
                    .background(if dark { Role::Base300 } else { Role::Base100 })
                    .border_color(Role::BaseContent.alpha(12))
                    .border_width(0.5);
            }
            b.into()
        })
        .collect();
    hstack(buttons)
        .key(key)
        .gap(2., Axis::Horizontal)
        .padding(2., Edge::All)
        .height(28.)
        .background(Role::BaseContent.alpha(7))
        .rounded(8.)
        .align(Align::Center)
        .into()
}

/// A quiet icon button with a tooltip.
pub fn icon_button(
    key: &str,
    glyph: Icon,
    label: &str,
    enabled: bool,
    active: bool,
    f: impl FnMut(&mut App) + 'static,
) -> Element<App> {
    let mut b = button("")
        .key(key)
        .variant(ButtonVariant::Ghost)
        .size(ControlSize::Small)
        .width(30.)
        .height(30.)
        .rounded(7.)
        .enabled(enabled)
        .accessible_label(label)
        .on_click(f);
    if active {
        b = b.background(Role::BaseContent.alpha(10));
    }
    tooltip(
        label,
        zstack![
            b,
            icon(glyph).size(17.).color(if !enabled {
                Role::Border
            } else if active {
                Role::BaseContent
            } else {
                Role::Muted
            }),
        ]
        .width(30.)
        .height(30.),
    )
    .key(format!("{key}-tip"))
    .into()
}

/// A list window clamped to the rows that exist.
fn within(window: &std::ops::Range<usize>, count: usize) -> std::ops::Range<usize> {
    window.start.min(count)..window.end.min(count).max(window.start.min(count))
}

// Toolbar

fn toolbar(app: &App) -> Element<App> {
    let mut items: Vec<Element<App>> = Vec::new();
    if !app.settings.sidebar {
        items.push(spacer().width(TRAFFIC_LIGHTS).flex(0.).into());
        items.push(icon_button(
            "show-sidebar",
            Icon::Sidebar,
            "Show Sidebar  ⌘\\",
            true,
            false,
            App::toggle_sidebar,
        ));
    }
    items.push(icon_button(
        "back",
        Icon::ChevronLeft,
        "Back  ⌘[",
        !app.back.is_empty(),
        false,
        App::go_back,
    ));
    items.push(icon_button(
        "forward",
        Icon::ChevronRight,
        "Forward  ⌘]",
        !app.forward.is_empty(),
        false,
        App::go_forward,
    ));
    let (title, meta) = match &app.doc {
        Some(doc) => {
            let name = file_name(&doc.path);
            let meta = match doc.kind {
                FileKind::Markdown => format!(
                    "{name}  ·  {} words  ·  {} min read",
                    group(doc.words),
                    doc.words.div_ceil(230).max(1)
                ),
                FileKind::Code("csv") => format!(
                    "{}  ·  Table  ·  {} lines",
                    folder(app, doc),
                    group(doc.words)
                ),
                FileKind::Code(language) => format!(
                    "{}  ·  {}  ·  {} lines",
                    folder(app, doc),
                    language_name(language),
                    group(doc.words)
                ),
                FileKind::Image => format!("{}  ·  Image", folder(app, doc)),
            };
            let meta = if doc.dirty() {
                format!("{meta}  ·  Edited")
            } else {
                meta
            };
            (doc.title.clone(), meta)
        }
        None => match &app.error {
            Some(_) => (
                app.tabs
                    .get(app.active)
                    .map_or("MD Star".into(), |t| t.title.clone()),
                "Couldn’t open".into(),
            ),
            None => ("MD Star".into(), "Markdown reader".into()),
        },
    };
    items.push(
        vstack![
            text(title).font_size(13.).font_weight(600).max_lines(1),
            text(meta).font_size(11.).color(Role::Muted).max_lines(1),
        ]
        .gap(1., Axis::Vertical)
        .padding(8., Edge::Left)
        .flex(1.)
        .into(),
    );
    items.push(mode_switch(app));
    items.push(spacer().width(8.).flex(0.).into());
    let dirty = app.doc.as_ref().is_some_and(Doc::dirty);
    items.push(icon_button(
        "save",
        Icon::Download,
        "Save  ⌘S",
        dirty,
        false,
        |app: &mut App| {
            app.save();
        },
    ));
    items.push(icon_button(
        "new",
        Icon::Plus,
        "New Document  ⌘N",
        true,
        false,
        App::new_document,
    ));
    items.push(icon_button(
        "open",
        Icon::Folder,
        "Open…  ⌘O",
        true,
        false,
        App::choose,
    ));
    items.push(icon_button(
        "find",
        Icon::Search,
        "Find  ⌘F",
        app.doc.is_some(),
        app.find.is_some(),
        |app: &mut App| {
            if app.find.is_some() {
                app.close_find()
            } else {
                app.open_find()
            }
        },
    ));
    items.push(icon_button(
        "reload",
        Icon::Refresh,
        "Reload  ⌘R",
        app.doc.is_some(),
        false,
        App::reload,
    ));
    items.push(spacer().width(6.).flex(0.).into());
    items.push(icon_button(
        "settings",
        Icon::Type,
        "Appearance",
        true,
        app.appearance_open || app.show_settings,
        |app: &mut App| app.appearance_open = !app.appearance_open,
    ));
    hstack(items)
        .key("toolbar")
        .height(TOOLBAR)
        .padding(12., Edge::Horizontal)
        .gap(2., Axis::Horizontal)
        .align(Align::Center)
        .drag_region(true)
        .border_sides(Sides::BOTTOM)
        .border_color(Role::Border)
        .border_width(1.)
        .into()
}

/// Where a document lives: its folder within the workspace, or `~/…`.
fn folder(app: &App, doc: &Doc) -> String {
    let dir = doc.path.parent().unwrap_or(&doc.path);
    for root in &app.tree {
        if let Ok(rel) = dir.strip_prefix(&root.path) {
            let rel = rel.display().to_string();
            return if rel.is_empty() {
                root.name.clone()
            } else {
                format!("{}/{rel}", root.name)
            };
        }
    }
    short_home(&dir.display().to_string())
}

fn language_name(language: &str) -> String {
    match language {
        "json" | "xml" | "html" | "yaml" | "toml" | "ini" | "sql" | "css" | "csv" => {
            language.to_uppercase()
        }
        "javascript" => "JavaScript".into(),
        "typescript" => "TypeScript".into(),
        "csharp" => "C#".into(),
        "cpp" => "C++".into(),
        "objc" => "Objective-C".into(),
        "text" => "Plain text".into(),
        other => {
            let mut c = other.chars();
            c.next()
                .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        }
    }
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

// Find

/// The find panel, floating over the top right of the document.
fn find_panel(app: &App) -> Element<App> {
    let Some(find) = &app.find else {
        return vstack![].key("find-layer").width(0.).height(0.).into();
    };
    let summary = find.summary();
    let panel = hstack![
        text_field(&find.query)
            .key("find-field")
            .placeholder("Find in document")
            .leading_icon(Icon::Search)
            .autofocus(true)
            .on_change(App::set_query)
            .on_submit(|app: &mut App| app.find_step(true))
            .on_cancel(App::close_find)
            .background(Role::BaseContent.alpha(6))
            .border_width(0.)
            .width(240.)
            .height(30.)
            .font_size(13.),
        text(if summary.is_empty() {
            " ".to_string()
        } else {
            summary.clone()
        })
        .key("find-count")
        .font_size(12.)
        .text_align(TextAlign::Center)
        .color(if summary == "No matches" {
            Role::Error
        } else {
            Role::Muted
        })
        .width(78.),
        icon_button(
            "find-previous",
            Icon::ChevronUp,
            "Previous Match  ⇧⌘G",
            !find.hits.is_empty(),
            false,
            |app: &mut App| app.find_step(false),
        ),
        icon_button(
            "find-next",
            Icon::ChevronDown,
            "Next Match  ⌘G",
            !find.hits.is_empty(),
            false,
            |app: &mut App| app.find_step(true),
        ),
        icon_button(
            "find-close",
            Icon::Close,
            "Close  Esc",
            true,
            false,
            App::close_find
        ),
    ]
    .key("find-panel")
    .gap(2., Axis::Horizontal)
    .padding(6., Edge::All)
    .align(Align::Center)
    .background(Role::Base100)
    .border_color(Role::Border)
    .border_width(1.)
    .rounded(12.)
    .shadow(Shadow::new().color("#00000026").blur(18.).offset(0., 6.));
    vstack![panel]
        .key("find-layer")
        .flow(Flow::TopRight)
        .padding(12., Edge::Top)
        .padding(18., Edge::Right)
        .scroll_passthrough(true)
        .into()
}

// Tabs

fn tab_bar(app: &App) -> Element<App> {
    if app.tabs.len() < 2 {
        return vstack![].key("tabs").height(0.).into();
    }
    let open = match &app.menu {
        Some((MenuTarget::Tab(i), at)) => Some((*i, *at)),
        _ => None,
    };
    let tabs: Vec<Element<App>> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let title = if tab.title.is_empty() {
                "New Tab".to_string()
            } else {
                tab.title.clone()
            };
            let title = if app.tab_dirty(i) {
                format!("• {title}")
            } else {
                title
            };
            tab_button(
                i,
                &title,
                i == app.active,
                open.filter(|(t, _)| *t == i).map(|(_, at)| at),
            )
        })
        .collect();
    hstack(tabs)
        .key("tabs")
        .height(38.)
        .gap(4., Axis::Horizontal)
        .padding(10., Edge::Horizontal)
        .padding(5., Edge::Vertical)
        .align(Align::Stretch)
        .background(Role::Base200.alpha(70))
        .border_sides(Sides::BOTTOM)
        .border_color(Role::Border)
        .border_width(1.)
        .into()
}

fn tab_button(i: usize, title: &str, active: bool, menu_at: Option<[f32; 2]>) -> Element<App> {
    let key = format!("tab-{i}");
    let mut label = button(title)
        .key(format!("{key}-button"))
        .variant(ButtonVariant::Ghost)
        .size(ControlSize::Small)
        .font_size(12.5)
        .font_weight(if active { 600 } else { 400 })
        .color(if active {
            Role::BaseContent
        } else {
            Role::Muted
        })
        .max_lines(1)
        .label_inset(26.)
        .rounded(7.)
        .width_fraction(1.)
        .height(28.)
        .accessible_label(format!("Tab {title}"))
        .on_click(move |app: &mut App| app.switch_tab(i));
    if active {
        label = label
            .background(Role::Base100)
            .border_color(Role::Border)
            .border_width(1.);
    }
    let close = zstack![
        button("")
            .key(format!("{key}-close"))
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Small)
            .width(20.)
            .height(20.)
            .rounded(5.)
            .accessible_label(format!("Close {title}"))
            .on_click(move |app: &mut App| app.close_tab(i)),
        icon(Icon::Close).size(11.).color(Role::Muted),
    ]
    .key(format!("{key}-close-layer"))
    .width(20.)
    .height(20.);
    let layer = zstack![
        label,
        hstack![close, spacer()]
            .key(format!("{key}-chrome"))
            .padding(6., Edge::Left)
            .align(Align::Center)
            .scroll_passthrough(true),
    ]
    .key(format!("{key}-layer"))
    .align(Align::Stretch)
    .width_fraction(1.)
    .height(28.);
    context_menu(
        layer,
        [
            menu_item(tab_action::CLOSE, "Close Tab").shortcut("⌘W"),
            menu_item(tab_action::CLOSE_OTHERS, "Close Other Tabs"),
            menu_separator(),
            menu_item(tab_action::COPY_PATH, "Copy Path").icon(Icon::Copy),
            menu_item(tab_action::REVEAL, "Reveal in Finder").icon(Icon::Folder),
        ],
    )
    .key(key)
    .flex(1.)
    .max_width(240.)
    .open(menu_at.is_some())
    .position(menu_at.unwrap_or_default())
    .on_context_open(move |app: &mut App, at| app.open_menu(MenuTarget::Tab(i), at))
    .on_open_change(|app: &mut App, open| {
        if !open {
            app.menu = None;
        }
    })
    .on_action(move |app: &mut App, id| app.tab_action(i, id))
    .into()
}

// Sidebar

fn sidebar(app: &App) -> Element<App> {
    let tab = app.settings.sidebar_tab;
    let list = match tab {
        SidebarTab::Files => files(app),
        SidebarTab::Outline => outline(app),
    };
    let sidebar_color = app.settings.sidebar_fill(app.theme());
    vstack![
        hstack![
            spacer(),
            icon_button(
                "hide-sidebar",
                Icon::Sidebar,
                "Hide Sidebar  ⌘\\",
                true,
                false,
                App::toggle_sidebar,
            ),
        ]
        .height(TOOLBAR)
        .padding(10., Edge::Right)
        .align(Align::Center)
        .drag_region(true),
        vstack![
            segmented(
                "sidebar-tab",
                &[
                    (SidebarTab::Files, "Files"),
                    (SidebarTab::Outline, "Outline")
                ],
                tab,
                app.theme().dark(),
                App::show_sidebar,
            ),
            text_field(&app.filter)
                .key("filter")
                .placeholder(match tab {
                    SidebarTab::Files => "Filter files",
                    SidebarTab::Outline => "Filter headings",
                })
                .leading_icon(Icon::Filter)
                .on_change(|app: &mut App, v| {
                    app.filter = v;
                    app.side_window = crate::app::SIDE_WINDOW;
                })
                .on_cancel(|app: &mut App| app.filter.clear())
                .background(Role::BaseContent.alpha(6))
                .border_width(0.)
                .height(32.)
                .font_size(13.)
                .fill_width(),
        ]
        .gap(8., Axis::Vertical)
        .padding(12., Edge::Horizontal)
        .padding(10., Edge::Bottom),
        list,
    ]
    .key("sidebar")
    .align(Align::Stretch)
    .background(sidebar_color)
    .border_sides(Sides::RIGHT)
    .border_color(Role::Border)
    .border_width(1.)
    .into()
}

fn side_hint(key: &str, message: &str) -> Element<App> {
    vstack![
        text(message)
            .font_size(12.5)
            .color(Role::Muted)
            .text_align(TextAlign::Center)
    ]
    .key(key)
    .padding(20., Edge::All)
    .flex(1.)
    .into()
}

fn files(app: &App) -> Element<App> {
    let rows = workspace::rows(&app.tree, &app.expanded, &app.filter);
    if app.tree.is_empty() {
        // The hint and its button sit together near the top of the list.
        return vstack![
            text("Open a folder to browse its documents here.")
                .font_size(12.5)
                .color(Role::Muted)
                .text_align(TextAlign::Center),
            button("Open Folder…")
                .key("add-folder")
                .size(ControlSize::Small)
                .on_click(App::choose_folder),
        ]
        .key("files")
        .gap(12., Axis::Vertical)
        .align(Align::Center)
        .padding(20., Edge::All)
        .flex(1.)
        .into();
    }
    if rows.iter().all(|r| r.depth == 0) && !app.filter.trim().is_empty() {
        return side_hint("files-empty", "No files match.");
    }
    let current = app.path().map(Path::to_path_buf);
    let open = match &app.menu {
        Some((MenuTarget::File(path), at)) => Some((path.clone(), *at)),
        _ => None,
    };
    let items: Vec<(PathBuf, String, usize, bool, bool, bool)> = rows
        .iter()
        .map(|r| {
            (
                r.node.path.clone(),
                r.node.name.clone(),
                r.depth,
                r.node.is_dir(),
                r.open,
                app.is_workspace_folder(&r.node.path),
            )
        })
        .collect();
    let count = items.len();
    let list = lazy_list(count, |i| {
        let (path, name, depth, dir, expanded, added) = &items[i];
        file_row(
            path,
            name,
            *depth,
            *dir,
            *expanded,
            *added,
            current.as_deref() == Some(path.as_path()),
            open.as_ref().filter(|(p, _)| p == path).map(|(_, at)| *at),
        )
    })
    .key("files")
    .estimate(ROW)
    .window(within(&app.side_window, count))
    .gap(1.)
    .on_window_change(|app: &mut App, window| app.side_window = window)
    .flex(1.);
    vstack![list]
        .key("files-pane")
        .padding(8., Edge::Horizontal)
        .padding(12., Edge::Bottom)
        .align(Align::Stretch)
        .flex(1.)
        .into()
}

#[allow(clippy::too_many_arguments)]
fn file_row(
    path: &Path,
    name: &str,
    depth: usize,
    dir: bool,
    expanded: bool,
    added: bool,
    selected: bool,
    menu_at: Option<[f32; 2]>,
) -> Element<App> {
    let key = format!("file-{}", path_key(path));
    let root = depth == 0;
    let indent = 8. + 14. * depth.saturating_sub(1) as f32;
    let target = path.to_path_buf();
    let click = target.clone();
    let mut label = button(name)
        .key(format!("{key}-button"))
        .variant(ButtonVariant::Ghost)
        .size(ControlSize::Small)
        .label_align(Align::Start)
        .label_inset(if root { 10. } else { indent + 44. })
        .font_size(if root { 11.5 } else { 13. })
        .font_weight(if root || selected { 600 } else { 400 })
        .max_lines(1)
        .color(if root { Role::Muted } else { Role::BaseContent })
        .rounded(6.)
        .height(ROW)
        .width_fraction(1.)
        .accessible_label(if dir {
            format!("Folder {name}")
        } else {
            format!("Open {name}")
        })
        .on_click(move |app: &mut App| {
            if root {
                return;
            }
            if click.is_dir() {
                app.toggle_folder(click.clone());
            } else {
                app.visit(click.clone());
            }
        });
    if selected {
        label = label.background(Role::Primary.alpha(22));
    }
    let mut layer: Vec<Element<App>> = vec![label.into()];
    if !root {
        let mut glyphs: Vec<Element<App>> = vec![spacer().width(indent).flex(0.).into()];
        glyphs.push(if dir {
            icon(if expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            })
            .size(12.)
            .color(Role::Muted)
            .into()
        } else {
            spacer().width(12.).flex(0.).into()
        });
        let glyph = if dir {
            Icon::Folder
        } else {
            match workspace::kind(path) {
                Some(FileKind::Markdown) => Icon::FileText,
                Some(FileKind::Image) => Icon::Image,
                _ => Icon::File,
            }
        };
        glyphs.push(
            icon(glyph)
                .size(15.)
                .color(if selected {
                    Role::Primary.into()
                } else if dir {
                    Role::Primary.alpha(80)
                } else {
                    Role::Muted.into()
                })
                .into(),
        );
        layer.push(
            hstack(glyphs)
                .key(format!("{key}-glyphs"))
                .gap(5., Axis::Horizontal)
                .align(Align::Center)
                .scroll_passthrough(true)
                .into(),
        );
    }
    let mut items = if dir {
        vec![menu_item(file_action::OPEN, "Expand or Collapse")]
    } else {
        vec![
            menu_item(file_action::OPEN, "Open"),
            menu_item(file_action::OPEN_TAB, "Open in New Tab"),
            menu_separator(),
        ]
    };
    items.extend([
        menu_item(file_action::REVEAL, "Reveal in Finder").icon(Icon::Folder),
        menu_item(file_action::COPY_PATH, "Copy Path").icon(Icon::Copy),
    ]);
    if root {
        items = vec![
            menu_item(file_action::REVEAL, "Reveal in Finder").icon(Icon::Folder),
            menu_item(file_action::COPY_PATH, "Copy Path").icon(Icon::Copy),
            menu_item(file_action::REFRESH, "Refresh").icon(Icon::Refresh),
        ];
        if added {
            items.extend([
                menu_separator(),
                menu_item(file_action::REMOVE, "Remove from Workspace").danger(true),
            ]);
        }
    }
    let menu_path = target.clone();
    context_menu(
        zstack(layer)
            .key(format!("{key}-layer"))
            .align(Align::Stretch)
            .width_fraction(1.)
            .height(ROW),
        items,
    )
    .key(key)
    .width_fraction(1.)
    .open(menu_at.is_some())
    .position(menu_at.unwrap_or_default())
    .on_context_open(move |app: &mut App, at| {
        app.open_menu(MenuTarget::File(menu_path.clone()), at)
    })
    .on_open_change(|app: &mut App, open| {
        if !open {
            app.menu = None;
        }
    })
    .on_action(move |app: &mut App, id| app.file_action(target.clone(), id))
    .into()
}

/// A stable key for a path: a hash, since paths can be long.
fn path_key(path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn outline(app: &App) -> Element<App> {
    let Some(doc) = &app.doc else {
        return side_hint("outline-empty", "Open a document to see its outline.");
    };
    let entries = outline_entries(&doc.ir, &app.filter);
    if entries.is_empty() {
        return side_hint(
            "outline-empty",
            if app.filter.trim().is_empty() {
                "This document has no headings."
            } else {
                "No headings match."
            },
        );
    }
    let rows = heading_rows(&doc.units());
    // The section being read: the last heading at or above the first row
    // on screen.
    let current = doc
        .ir
        .outline
        .iter()
        .filter_map(|item| rows.get(&item.id).map(|row| (item, *row)))
        .filter(|(_, row)| *row <= app.first_visible)
        .max_by_key(|(_, row)| *row)
        .map(|(item, _)| item.id.clone());
    let open = match &app.menu {
        Some((MenuTarget::Heading(row), at)) => Some((*row, *at)),
        _ => None,
    };
    let items: Vec<(usize, String, f32, bool)> = entries
        .into_iter()
        .filter_map(|(id, text, depth)| {
            Some((
                *rows.get(&id)?,
                text,
                depth,
                current.as_deref() == Some(id.as_str()),
            ))
        })
        .collect();
    let count = items.len();
    let list = lazy_list(count, |i| {
        let (row, label, depth, active) = &items[i];
        outline_row(
            *row,
            label,
            *depth,
            *active,
            open.filter(|(r, _)| r == row).map(|(_, at)| at),
        )
    })
    .key("outline")
    .estimate(ROW)
    .window(within(&app.side_window, count))
    .gap(1.)
    .on_window_change(|app: &mut App, window| app.side_window = window)
    .flex(1.);
    vstack![list]
        .key("outline-pane")
        .padding(8., Edge::Horizontal)
        .padding(12., Edge::Bottom)
        .align(Align::Stretch)
        .flex(1.)
        .into()
}

/// Outline headings (id, text, indent level) within three levels of the
/// top, filtered by `filter`.
fn outline_entries(ir: &DocumentIr, filter: &str) -> Vec<(String, String, f32)> {
    let top = ir.outline.iter().map(|i| i.level).min().unwrap_or(1);
    let filter = filter.trim().to_lowercase();
    ir.outline
        .iter()
        .filter(|item| item.level <= top + 3)
        .filter(|item| filter.is_empty() || item.text.to_lowercase().contains(&filter))
        .map(|item| {
            (
                item.id.clone(),
                item.text.clone(),
                f32::from(item.level - top),
            )
        })
        .collect()
}

fn outline_row(
    row: usize,
    label: &str,
    depth: f32,
    active: bool,
    menu_at: Option<[f32; 2]>,
) -> Element<App> {
    let target = button(label)
        .key(format!("outline-{row}-button"))
        .variant(ButtonVariant::Ghost)
        .size(ControlSize::Small)
        .label_align(Align::Start)
        .label_inset(10. + 14. * depth)
        .font_size(if depth == 0. { 13. } else { 12.5 })
        .font_weight(if active { 600 } else { 400 })
        .max_lines(1)
        .color(if active || depth == 0. {
            Role::BaseContent
        } else {
            Role::Muted
        })
        .background(if active {
            Role::BaseContent.alpha(9)
        } else {
            "#00000000".into()
        })
        .rounded(6.)
        .height(ROW)
        .width_fraction(1.)
        .on_click(move |app: &mut App| app.jump_to(row));
    context_menu(
        target,
        [
            menu_item(heading_action::GO, "Go to Section"),
            menu_item(heading_action::COPY_LINK, "Copy Link to Section").icon(Icon::Link),
            menu_item(heading_action::COPY_TITLE, "Copy Title").icon(Icon::Copy),
        ],
    )
    .key(format!("outline-{row}"))
    .width_fraction(1.)
    .open(menu_at.is_some())
    .position(menu_at.unwrap_or_default())
    .on_context_open(move |app: &mut App, at| app.open_menu(MenuTarget::Heading(row), at))
    .on_open_change(|app: &mut App, open| {
        if !open {
            app.menu = None;
        }
    })
    .on_action(move |app: &mut App, id| app.heading_action(row, id))
    .into()
}

// Document

fn body(app: &App) -> Element<App> {
    match (&app.doc, &app.error) {
        (_, Some(error)) => message("Couldn’t open the document", error),
        (None, None) => welcome(app),
        (Some(doc), None) if app.editing() && app.mode == Mode::Source => {
            // Prose keeps a readable measure; code uses the width.
            let column = if doc.kind == FileKind::Markdown {
                vstack![editor(app, doc)]
                    .width_fraction(1.)
                    .max_width(app.settings.width + 120.)
                    .flex(1.)
                    .align(Align::Stretch)
            } else {
                vstack![editor(app, doc)]
                    .width_fraction(1.)
                    .flex(1.)
                    .align(Align::Stretch)
            };
            vstack![column]
                .key("source-pane")
                .align(Align::Center)
                .flex(1.)
                .into()
        }
        (Some(doc), None) if app.editing() => split(
            vstack![editor(app, doc)]
                .key("editor-pane")
                .align(Align::Stretch)
                .border_sides(Sides::RIGHT)
                .border_color(Role::Border)
                .border_width(1.),
            vstack![document(app, doc)]
                .key("preview-pane")
                .align(Align::Stretch),
        )
        .key("split")
        .ratio(app.split)
        .min_first(280.)
        .min_second(280.)
        .handle_size(7.)
        .handle_color("#00000000")
        .on_resize(|app: &mut App, ratio| app.split = ratio)
        .fill_width()
        .fill_height()
        .flex(1.)
        .into(),
        (Some(doc), None) => document(app, doc),
    }
}

/// The source editor: the document's text in the code font.
fn editor(app: &App, doc: &Doc) -> Element<App> {
    let size = (app.settings.font_size * 0.875).round().max(11.);
    vstack![
        text_area(&doc.source)
            .key(format!("source-{}", path_key(&doc.path)))
            .placeholder("Start writing…")
            .soft_wrap(true)
            .tab_indents(true)
            .on_change(App::edit)
            .on_selection(App::selection_changed)
            .on_keys(app.editor_keys(), App::editor_key)
            .on_cancel(App::cancel_slash)
            .select(app.select.0..app.select.1, app.select.2)
            .scroll_to_offset(app.editor_scroll.0, app.editor_scroll.1)
            .highlights(
                if doc.kind == FileKind::Markdown {
                    crate::compose::source_palette(app.theme().dark())
                } else {
                    Vec::new()
                },
                if doc.kind == FileKind::Markdown {
                    crate::compose::source_tints(&doc.source)
                } else {
                    Vec::new()
                },
            )
            .font_family(app.settings.code_family())
            .font_size(size)
            .line_height(1.65)
            .background(Role::Base100)
            .border_width(0.)
            .rounded(0.)
            .fill_width()
            .fill_height()
            .flex(1.),
    ]
    .key("editor")
    .padding(22., Edge::Horizontal)
    .padding(16., Edge::Top)
    .align(Align::Stretch)
    .flex(1.)
    .into()
}

/// The `/` menu: Markdown to insert, under the caret.
fn slash_menu(app: &App) -> Element<App> {
    let Some(slash) = &app.slash else {
        return vstack![].key("slash-layer").width(0.).height(0.).into();
    };
    let commands = crate::compose::matching(&slash.query);
    let mut rows: Vec<Element<App>> = vec![
        vstack![
            text("INSERT MARKDOWN")
                .font_size(10.5)
                .font_weight(600)
                .letter_spacing(0.06)
                .color(Role::Muted)
        ]
        .padding(10., Edge::Horizontal)
        .padding(4., Edge::Vertical)
        .into(),
    ];
    for (i, command) in commands.iter().enumerate() {
        let chosen = i == slash.index;
        let mut b = button("")
            .key(format!("slash-{i}"))
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Small)
            .width_fraction(1.)
            .height(42.)
            .rounded(8.)
            .accessible_label(command.title)
            .on_click(move |app: &mut App| app.apply_slash(i));
        if chosen {
            b = b.background(Role::Primary.alpha(16));
        }
        rows.push(
            zstack![
                b,
                hstack![
                    icon(command.icon).size(16.).color(if chosen {
                        Role::Primary
                    } else {
                        Role::Muted
                    }),
                    vstack![
                        text(command.title).font_size(13.).font_weight(600),
                        text(command.subtitle).font_size(11.).color(Role::Muted),
                    ]
                    .gap(1., Axis::Vertical),
                ]
                .key(format!("slash-{i}-label"))
                .gap(12., Axis::Horizontal)
                .align(Align::Center)
                .padding(12., Edge::Left)
                .scroll_passthrough(true),
            ]
            .key(format!("slash-{i}-row"))
            .align(Align::Stretch)
            .width_fraction(1.)
            .height(42.)
            .into(),
        );
    }
    let menu = vstack(rows)
        .key("slash-menu")
        .width(290.)
        .max_height(crate::compose::MENU_MAX)
        .scroll(Scroll::Vertical)
        .gap(1., Axis::Vertical)
        .padding(6., Edge::All)
        .background(Role::Base100)
        .border_color(Role::Border)
        .border_width(1.)
        .rounded(12.)
        .shadow(Shadow::new().color("#0000002E").blur(22.).offset(0., 8.));
    vstack![menu]
        .key("slash-layer")
        .flow(Flow::TopLeft)
        .padding(slash.at[0].max(0.), Edge::Left)
        .padding(slash.at[1].max(0.), Edge::Top)
        .scroll_passthrough(true)
        .into()
}

/// Preview, split editor or source only.
fn mode_switch(app: &App) -> Element<App> {
    let enabled = app.doc.as_ref().is_some_and(Doc::editable);
    let current = if enabled { app.mode } else { Mode::Preview };
    let dark = app.theme().dark();
    let items: Vec<Element<App>> = [
        (Mode::Preview, Icon::Eye, "Preview  ⌘E"),
        (Mode::Split, Icon::Columns, "Edit with Preview  ⌘E"),
        (Mode::Source, Icon::Edit, "Edit Source"),
    ]
    .into_iter()
    .map(|(mode, glyph, label)| {
        let chosen = mode == current;
        let key = format!("mode-{mode:?}").to_lowercase();
        let mut b = button("")
            .key(key.clone())
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Small)
            .width(32.)
            .height(24.)
            .rounded(6.)
            .enabled(enabled)
            .accessible_label(label)
            .on_click(move |app: &mut App| app.set_mode(mode));
        if chosen {
            b = b
                .background(if dark { Role::Base300 } else { Role::Base100 })
                .border_color(Role::BaseContent.alpha(12))
                .border_width(0.5);
        }
        tooltip(
            label,
            zstack![
                b,
                icon(glyph).size(15.).color(if !enabled {
                    Role::Border
                } else if chosen {
                    Role::BaseContent
                } else {
                    Role::Muted
                }),
            ]
            .width(32.)
            .height(24.),
        )
        .key(format!("{key}-tip"))
        .into()
    })
    .collect();
    hstack(items)
        .key("mode")
        .gap(2., Axis::Horizontal)
        .padding(2., Edge::All)
        .height(28.)
        .background(Role::BaseContent.alpha(7))
        .rounded(8.)
        .align(Align::Center)
        .into()
}

/// Shown when a save failed, for example because the file changed on disk.
fn save_banner(app: &App) -> Element<App> {
    let Some(error) = &app.save_error else {
        return vstack![].key("save-banner").height(0.).into();
    };
    let conflict = error.contains("changed on disk");
    let mut items: Vec<Element<App>> = vec![
        icon(Icon::Warning).size(15.).color(Role::Warning).into(),
        text(error.clone())
            .font_size(12.5)
            .flex(1.)
            .max_lines(2)
            .into(),
    ];
    if conflict {
        items.push(
            button("Keep My Version")
                .key("save-overwrite")
                .size(ControlSize::Small)
                .font_size(12.)
                .on_click(App::overwrite)
                .into(),
        );
        items.push(
            button("Use Version on Disk")
                .key("save-discard")
                .size(ControlSize::Small)
                .font_size(12.)
                .on_click(App::discard_edits)
                .into(),
        );
    } else {
        items.push(
            button("Dismiss")
                .key("save-dismiss")
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Small)
                .font_size(12.)
                .on_click(|app: &mut App| app.save_error = None)
                .into(),
        );
    }
    hstack(items)
        .key("save-banner")
        .gap(10., Axis::Horizontal)
        .align(Align::Center)
        .padding(16., Edge::Horizontal)
        .padding(8., Edge::Vertical)
        .background(Role::Warning.alpha(14))
        .border_sides(Sides::BOTTOM)
        .border_color(Role::Border)
        .border_width(1.)
        .into()
}

pub fn typography(app: &App) -> Typography {
    // Source files and tables use the whole width.
    let wide = app
        .doc
        .as_ref()
        .is_some_and(|d| matches!(d.kind, FileKind::Code(_)));
    let measure = if wide { 1800. } else { app.settings.width };
    let margin = render::margin_x(app.page_width);
    Typography {
        size: app.settings.font_size,
        line: app.settings.line_height,
        measure,
        // The page's text column: the measure, or less in a narrow pane.
        room: app
            .page_width
            .map_or(measure, |w| (w - 2. * margin).clamp(160., measure)),
        margin,
        family: app.settings.body_family(),
        code_family: app.settings.code_family(),
    }
}

fn document(app: &App, doc: &Doc) -> Element<App> {
    let units = doc.units();
    let colors = app.theme().colors();
    let needle = app
        .find
        .as_ref()
        .map(|f| find::needle(&f.query))
        .unwrap_or_default();
    let ctx = render::Ctx {
        images: &doc.images,
        dark: app.theme().dark(),
        code_view: matches!(doc.kind, FileKind::Code(language) if language != "csv"),
        copied: app.copied.as_deref(),
        typo: typography(app),
        find: (!needle.is_empty()).then(|| {
            Highlight::new(
                &needle,
                app.find.as_ref().and_then(find::Find::current_hit),
                colors.mark,
                colors.mark_current,
            )
        }),
        menu: match &app.menu {
            Some((MenuTarget::Row(row), at)) => Some((*row, *at)),
            _ => None,
        },
        controls: true,
        presentation: crate::render::Presentation::Document,
        media: Some(&crate::render::MEDIA),
        on_link: None,
    };
    let estimates: Vec<f32> = (0..units.len())
        .map(|i| render::estimate(&units, i, &doc.images, &ctx.typo))
        .collect();
    let family = ctx.typo.family.clone();
    let list = lazy_list(units.len(), |i| render::row(&units, i, &ctx))
        .key("document")
        .estimates(estimates)
        .window(within(&app.window, units.len()))
        .overscan(4)
        .generation(app.generation)
        .scroll_to_row(app.jump.0, app.jump.1)
        .on_window_change(|app: &mut App, window| {
            // Rows above the viewport are overscan; the first visible row
            // follows them (or is row 0 at the top).
            let first = if window.start == 0 {
                0
            } else {
                window.start + 4
            };
            let scrolled = first != app.first_visible;
            app.first_visible = first;
            app.window = window;
            if scrolled {
                app.preview_scrolled();
            }
        })
        .font_family(family)
        // Rendered text can be selected and copied (⌘C, ⌘A, right-click).
        .selectable(true)
        .on_select_text(App::text_selected)
        .flex(1.);
    // The pane reports its width so wide content fits the room it has.
    vstack![list]
        .key("document-pane")
        .align(Align::Stretch)
        .flex(1.)
        .on_width(|app: &mut App, width| app.page_width = Some(width))
        .into()
}

fn message(title: &str, detail: &str) -> Element<App> {
    vstack![
        icon(Icon::Warning).size(30.).color(Role::Warning),
        text(title).font_size(20.).font_weight(700),
        text(detail)
            .color(Role::Muted)
            .text_align(TextAlign::Center)
            .max_width(440.),
        button("Open File…")
            .key("error-open")
            .variant(ButtonVariant::Primary)
            .on_click(App::choose_file),
    ]
    .key("message")
    .flex(1.)
    .gap(12., Axis::Vertical)
    .align(Align::Center)
    .flow(Flow::Center)
    .padding(40., Edge::All)
    .into()
}

fn welcome(app: &App) -> Element<App> {
    let mut parts: Vec<Element<App>> = vec![
        text("MD Star")
            .font_size(30.)
            .font_weight(700)
            .letter_spacing(-0.015)
            .into(),
        text("Read, browse and write Markdown, natively.")
            .font_size(15.)
            .color(Role::Muted)
            .into(),
        hstack![
            button("New Document")
                .key("welcome-new")
                .variant(ButtonVariant::Primary)
                .on_click(App::new_document),
            button("Open File…")
                .key("welcome-open")
                .on_click(App::choose_file),
            button("Open Folder…")
                .key("welcome-folder")
                .on_click(App::choose_folder),
        ]
        .gap(10., Axis::Horizontal)
        .padding(10., Edge::Top)
        .into(),
    ];
    let recent: Vec<PathBuf> = app
        .settings
        .recent
        .iter()
        .filter(|p| p.is_file())
        .take(6)
        .cloned()
        .collect();
    if !recent.is_empty() {
        let mut rows: Vec<Element<App>> = vec![
            text("RECENT")
                .font_size(10.5)
                .font_weight(600)
                .letter_spacing(0.07)
                .color(Role::Muted)
                .into(),
        ];
        for (i, path) in recent.into_iter().enumerate() {
            let folder = path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let label = format!("{}   {}", file_name(&path), short_home(&folder));
            rows.push(
                button(label)
                    .key(format!("recent-{i}"))
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Small)
                    .label_align(Align::Start)
                    .label_inset(10.)
                    .max_lines(1)
                    .font_size(13.)
                    .fill_width()
                    .on_click(move |app: &mut App| app.open_in_tab(path.clone()))
                    .into(),
            );
        }
        parts.push(
            vstack(rows)
                .key("recent")
                .gap(2., Axis::Vertical)
                .width(420.)
                .padding(26., Edge::Top)
                .into(),
        );
    }
    parts.push(
        vstack![
            drop_zone()
                .key("drop")
                .title("Drop a Markdown file")
                .description(".md, .markdown or .txt")
                .accept(MARKDOWN)
                .multiple(false)
                .height(150.)
                .on_drop(|app: &mut App, paths| {
                    if let Some(path) = paths.into_iter().next() {
                        app.visit(path);
                    }
                })
                .on_browse(App::choose_file),
        ]
        .width(420.)
        .padding(22., Edge::Top)
        .into(),
    );
    vstack(parts)
        .key("welcome")
        .flex(1.)
        .gap(6., Axis::Vertical)
        .align(Align::Center)
        .flow(Flow::Center)
        .padding(40., Edge::All)
        .into()
}

fn short_home(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && path.starts_with(&home) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_owned(),
    }
}

// Settings

fn settings_dialog(app: &App) -> Element<App> {
    let s = &app.settings;
    let section = |title: &str| -> Element<App> {
        text(title.to_uppercase())
            .font_size(10.5)
            .font_weight(600)
            .letter_spacing(0.07)
            .color(Role::Muted)
            .into()
    };
    let field = |label: &str, control: Element<App>| -> Element<App> {
        hstack![
            text(label).font_size(13.).width(118.),
            vstack![control].flex(1.).align(Align::Start),
        ]
        .align(Align::Center)
        .gap(12., Axis::Horizontal)
        .into()
    };
    let appearance = vstack![segmented(
        "set-appearance",
        &[
            (Appearance::System, "System"),
            (Appearance::Light, "Light"),
            (Appearance::Dark, "Dark"),
        ],
        s.appearance,
        app.theme().dark(),
        |app: &mut App, v| app.configure(|s| s.appearance = v),
    )]
    .width(260.);

    let mut faces: Vec<SelectItem<String>> = TYPEFACES
        .iter()
        .map(|t| select_item(t.name.to_string(), t.name))
        .collect();
    if !TYPEFACES.iter().any(|t| t.name == s.typeface) {
        faces.push(select_item(s.typeface.clone(), s.typeface.clone()));
    }
    let typeface = select(faces)
        .key("set-typeface")
        .selected(s.typeface.clone())
        .open(app.open_select == Some("typeface"))
        .on_select(|app: &mut App, v| app.configure(|s| s.typeface = v))
        .on_open_change(|app: &mut App, open| app.open_select = open.then_some("typeface"))
        .width(220.);
    let mut codes: Vec<SelectItem<String>> = CODE_FONTS
        .iter()
        .map(|t| select_item(t.name.to_string(), t.name))
        .collect();
    if !CODE_FONTS.iter().any(|t| t.name == s.code_font) {
        codes.push(select_item(s.code_font.clone(), s.code_font.clone()));
    }
    let code_font = select(codes)
        .key("set-code-font")
        .selected(s.code_font.clone())
        .open(app.open_select == Some("code"))
        .on_select(|app: &mut App, v| app.configure(|s| s.code_font = v))
        .on_open_change(|app: &mut App, open| app.open_select = open.then_some("code"))
        .width(220.);
    let size = hstack![
        icon_button(
            "set-smaller",
            Icon::Minus,
            "Smaller  ⌘−",
            s.font_size > settings::SIZES[0],
            false,
            |app: &mut App| app.zoom(-1)
        ),
        text(format!("{} pt", s.font_size))
            .font_size(13.)
            .text_align(TextAlign::Center)
            .width(52.),
        icon_button(
            "set-bigger",
            Icon::Plus,
            "Bigger  ⌘=",
            s.font_size < settings::SIZES[settings::SIZES.len() - 1],
            false,
            |app: &mut App| app.zoom(1),
        ),
        button("Reset")
            .key("set-size-reset")
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Small)
            .font_size(12.)
            .enabled(s.font_size != settings::DEFAULT_SIZE)
            .on_click(|app: &mut App| app.zoom(0)),
    ]
    .gap(4., Axis::Horizontal)
    .align(Align::Center);
    let spacing = hstack![
            slider(s.line_height)
                .key("set-line")
                .range(settings::LINE_HEIGHTS)
                .step(0.05)
                .on_change(
                    |app: &mut App, v| app.configure(|s| s.line_height = (v * 20.).round() / 20.)
                )
                .width(200.),
            text(format!("{:.2}", s.line_height))
                .font_size(12.)
                .color(Role::Muted),
        ]
    .gap(10., Axis::Horizontal)
    .align(Align::Center);
    let width = hstack![
        slider(s.width)
            .key("set-width")
            .range(settings::WIDTHS)
            .step(20.)
            .on_change(|app: &mut App, v| app.configure(|s| s.width = (v / 20.).round() * 20.))
            .width(200.),
        text(format!("{} pt", s.width as i32))
            .font_size(12.)
            .color(Role::Muted),
    ]
    .gap(10., Axis::Horizontal)
    .align(Align::Center);
    let content = vstack![
        section("Appearance"),
        field("Appearance", appearance.into()),
        field(
            "Light theme",
            swatches("set-light", &Theme::LIGHT, s.light_theme, [116., 54.])
        ),
        field(
            "Dark theme",
            swatches("set-dark", &Theme::DARK, s.dark_theme, [116., 54.])
        ),
        field("Accent", accents(app, "set")),
        field("Sidebar", glass_control(app, "set-glass")),
        field("Background", opacity_control(app)),
        field("Blur", blur_control(app)),
        divider().background(Role::Border),
        section("Text"),
        field("Typeface", typeface.into()),
        field("Code font", code_font.into()),
        field("Size", size.into()),
        field("Line spacing", spacing.into()),
        field("Line width", width.into()),
        preview(app),
        hstack![
            text("Text size also follows ⌘= and ⌘− while reading.")
                .font_size(11.5)
                .color(Role::Muted)
                .flex(1.),
            button("Done")
                .key("settings-done")
                .variant(ButtonVariant::Primary)
                .size(ControlSize::Small)
                .on_click(|app: &mut App| app.show_settings = false),
        ]
        .align(Align::Center)
        .padding(4., Edge::Top),
    ]
    .gap(14., Axis::Vertical);
    dialog(content)
        .key("settings-dialog")
        .title("Settings")
        .open(app.show_settings)
        .on_dismiss(|app: &mut App| {
            app.show_settings = false;
            app.open_select = None;
        })
        .width(560.)
        .rounded(14.)
        .background(Role::Base100)
        .into()
}

/// Window background opacity: below solid, the desktop shows through.
fn opacity_control(app: &App) -> Element<App> {
    let s = &app.settings;
    let percent = (s.opacity * 100.).round() as i32;
    hstack![
        slider(s.opacity)
            .key("set-opacity")
            .accessible_label("Background opacity")
            .range(settings::MIN_OPACITY..=1.)
            .step(0.01)
            .on_change(|app: &mut App, v| app.configure(|s| s.opacity = (v * 100.).round() / 100.))
            .width(200.),
        text(if percent >= 100 {
            "Solid".to_string()
        } else {
            format!("{percent}%")
        })
        .font_size(12.)
        .color(Role::Muted),
    ]
    .gap(10., Axis::Horizontal)
    .align(Align::Center)
    .into()
}

fn blur_control(app: &App) -> Element<App> {
    let s = &app.settings;
    toggle(
        if s.translucent() {
            "Blur what’s behind the window"
        } else {
            "Blur applies below a solid background"
        },
        s.blur,
    )
    .key("set-blur")
    .enabled(s.translucent())
    .font_size(12.5)
    .on_change(|app: &mut App, v| app.configure(|s| s.blur = v))
    .into()
}

/// Theme cards: each shows the theme's page, text and accent. `prefix`
/// keeps keys apart where the cards appear twice (settings, popover).
fn swatches(prefix: &str, themes: &[Theme], selected: Theme, size: [f32; 2]) -> Element<App> {
    let [w, h] = size;
    let cards: Vec<Element<App>> = themes
        .iter()
        .map(|&theme| {
            let c = theme.colors();
            let chosen = theme == selected;
            let light = Theme::LIGHT.contains(&theme);
            let key = format!("{prefix}-theme-{theme:?}").to_lowercase();
            zstack![
                button("")
                    .key(format!("{key}-button"))
                    .variant(ButtonVariant::Ghost)
                    .accessible_label(format!("{} theme", theme.name()))
                    .width(w)
                    .height(h)
                    .on_click(move |app: &mut App| {
                        app.configure(|s| {
                            if light {
                                s.light_theme = theme;
                                if s.appearance == Appearance::Dark {
                                    s.appearance = Appearance::Light;
                                }
                            } else {
                                s.dark_theme = theme;
                                if s.appearance == Appearance::Light {
                                    s.appearance = Appearance::Dark;
                                }
                            }
                        })
                    }),
                vstack![
                    hstack![
                        text("Aa").font_size(15.).font_weight(700).color(c.text),
                        spacer(),
                        vstack![]
                            .width(9.)
                            .height(9.)
                            .rounded(4.5)
                            .background(c.accent),
                    ]
                    .align(Align::Center),
                    text(theme.name())
                        .font_size(10.5)
                        .color(c.muted)
                        .max_lines(1),
                ]
                .key(format!("{key}-face"))
                .gap(2., Axis::Vertical)
                .padding(9., Edge::Horizontal)
                .padding(7., Edge::Vertical)
                .width(w)
                .height(h)
                .background(c.page)
                .rounded(9.)
                .border_color(if chosen {
                    Role::Primary.into()
                } else {
                    c.border.to_string()
                })
                .border_width(if chosen { 2. } else { 1. })
                .scroll_passthrough(true),
            ]
            .key(key)
            .width(w)
            .height(h)
            .into()
        })
        .collect();
    hstack(cards)
        .key(format!("{prefix}-themes"))
        .wrap(true)
        .gap(8., Axis::Horizontal)
        .gap(8., Axis::Vertical)
        .into()
}

/// Accent colours as round swatches, like System Settings.
fn accents(app: &App, prefix: &str) -> Element<App> {
    use crate::settings::Accent;
    let dark = app.theme().dark();
    let dots: Vec<Element<App>> = Accent::ALL
        .iter()
        .map(|&accent| {
            let chosen = app.settings.accent == accent;
            let color = match accent {
                Accent::Theme => app.theme().colors().accent.to_string(),
                #[cfg(target_os = "macos")]
                Accent::System => crate::macos::system_accent()
                    .map(|c| {
                        format!(
                            "#{:02X}{:02X}{:02X}",
                            (c[0] * 255.) as u8,
                            (c[1] * 255.) as u8,
                            (c[2] * 255.) as u8
                        )
                    })
                    .unwrap_or_else(|| "#007AFF".into()),
                #[cfg(not(target_os = "macos"))]
                Accent::System => "#007AFF".into(),
                fixed => fixed.fixed(dark).unwrap_or("#007AFF").to_string(),
            };
            let key = format!("{prefix}-accent-{accent:?}").to_lowercase();
            let mut dot = vstack![if chosen {
                icon(Icon::Check)
                    .size(11.)
                    .color(Role::PrimaryContent)
                    .into()
            } else {
                Element::from(spacer().width(0.).flex(0.))
            }]
            .key(format!("{key}-dot"))
            .width(20.)
            .height(20.)
            .rounded(10.)
            .flow(Flow::Center)
            .background(color)
            .scroll_passthrough(true);
            if chosen {
                dot = dot
                    .border_color(Role::BaseContent.alpha(40))
                    .border_width(2.);
            }
            tooltip(
                accent.name(),
                zstack![
                    button("")
                        .key(format!("{key}-button"))
                        .variant(ButtonVariant::Ghost)
                        .accessible_label(format!("{} accent", accent.name()))
                        .width(24.)
                        .height(24.)
                        .rounded(12.)
                        .on_click(move |app: &mut App| app.configure(|s| s.accent = accent)),
                    dot,
                ]
                .key(format!("{key}-layer"))
                .width(24.)
                .height(24.),
            )
            .key(key)
            .into()
        })
        .collect();
    hstack(dots)
        .key(format!("{prefix}-accents"))
        .gap(4., Axis::Horizontal)
        .wrap(true)
        .into()
}

fn glass_control(app: &App, key: &str) -> Element<App> {
    toggle("Translucent sidebar", app.settings.sidebar_glass)
        .key(key)
        .font_size(12.5)
        .on_change(|app: &mut App, v| app.configure(|s| s.sidebar_glass = v))
        .into()
}

/// The quick Appearance popover from the toolbar: mode, theme, accent,
/// size and glass, with a way into the full settings.
fn appearance_popover(app: &App) -> Element<App> {
    let s = &app.settings;
    let theme = app.theme();
    let (themes, selected): (&[Theme], Theme) = if theme.dark() {
        (&Theme::DARK, s.dark_theme)
    } else {
        (&Theme::LIGHT, s.light_theme)
    };
    let label = |t: &str| -> Element<App> {
        text(t.to_uppercase())
            .font_size(10.)
            .font_weight(600)
            .letter_spacing(0.06)
            .color(Role::Muted)
            .into()
    };
    let content = vstack![
        segmented(
            "pop-appearance",
            &[
                (Appearance::System, "Auto"),
                (Appearance::Light, "Light"),
                (Appearance::Dark, "Dark"),
            ],
            s.appearance,
            theme.dark(),
            |app: &mut App, v| app.configure(|s| s.appearance = v),
        ),
        label("Theme"),
        swatches("pop", themes, selected, [88., 50.]),
        label("Accent"),
        accents(app, "pop"),
        hstack![
            text("Text size").font_size(12.5).flex(1.),
            icon_button(
                "pop-smaller",
                Icon::Minus,
                "Smaller  ⌘−",
                s.font_size > settings::SIZES[0],
                false,
                |app: &mut App| app.zoom(-1)
            ),
            text(format!("{} pt", s.font_size))
                .font_size(12.5)
                .text_align(TextAlign::Center)
                .width(46.),
            icon_button(
                "pop-bigger",
                Icon::Plus,
                "Bigger  ⌘=",
                s.font_size < settings::SIZES[settings::SIZES.len() - 1],
                false,
                |app: &mut App| app.zoom(1),
            ),
        ]
        .align(Align::Center),
        glass_control(app, "pop-glass"),
        divider().background(Role::Border),
        button("More Settings…")
            .key("pop-more")
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Small)
            .font_size(12.5)
            .label_align(Align::Start)
            .fill_width()
            .on_click(|app: &mut App| {
                app.appearance_open = false;
                app.open_settings();
            }),
    ]
    .key("appearance")
    .gap(10., Axis::Vertical)
    .align(Align::Stretch);
    // Three 88 pt theme cards with their gaps, plus the card's padding.
    popover(content)
        .key("appearance-popover")
        .anchor("settings")
        .open(app.appearance_open)
        .on_dismiss(|app: &mut App| app.appearance_open = false)
        .width(3. * 88. + 2. * 8. + 2. * 16.)
        .rounded(14.)
        .background(Role::Base100)
        .into()
}

/// A sample of the chosen typography in the current theme.
fn preview(app: &App) -> Element<App> {
    let t = typography(app);
    vstack![
        text("The quick brown fox")
            .font_family(t.family.clone())
            .font_size((t.size * 1.25).round())
            .font_weight(700),
        rich_text([
            span("Body text uses your typeface at "),
            span(format!("{} pt", t.size)).bold(),
            span(", with "),
            span("\u{202F}inline code\u{202F}")
                .code()
                .family(t.code_family.clone()),
            span(" in the code font."),
        ])
        .font_family(t.family.clone())
        .font_size(t.size)
        .line_height(t.line),
    ]
    .key("settings-preview")
    .gap(6., Axis::Vertical)
    .padding(16., Edge::All)
    .background(Role::Base200.alpha(60))
    .border_color(Role::Border)
    .border_width(1.)
    .rounded(10.)
    .into()
}
