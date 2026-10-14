//! `md`: the terminal viewer and live-preview editor. The app bundle links
//! `Contents/MacOS/md` to the app's executable, and `main` runs this instead
//! of opening a window when the program is started under that name.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{IsTerminal, Stdout, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::terminal::{Options as RenderOptions, render as render_markdown};
use ansi_to_tui::IntoText;
use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
    poll, read,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Text;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use ratatui_textarea::TextArea;

// ─── CLI argument definition ──────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(
    name = "md",
    version,
    about = "MD Star — fast terminal viewer and live-preview editor.\n\
             Run without flags for the TUI, or pass --plain/--no-color for piped output.\n\
             Launch the desktop GUI with --app."
)]
pub struct Cli {
    /// Path to the markdown file to view or edit.
    pub path: PathBuf,

    /// Wrap width for terminal output.
    #[arg(long)]
    pub width: Option<usize>,

    /// Disable ANSI colour output.
    #[arg(long)]
    pub no_color: bool,

    /// Render Mermaid diagrams using ASCII only (no Unicode box-drawing).
    #[arg(long)]
    pub ascii_mermaid: bool,

    /// Skip the interactive TUI; always print rendered output.
    #[arg(long)]
    pub no_tui: bool,

    /// Force plain-text mode (implies --no-color and --no-tui).
    #[arg(long)]
    pub plain: bool,

    /// Do not pipe output through a pager (non-TUI mode only).
    #[arg(long)]
    pub no_pager: bool,

    /// Pager command override, e.g. `less -R` or `bat -pp`.
    #[arg(long)]
    pub pager: Option<String>,

    // Hidden: --app/--gui are handled before parsing; --cli/--no-gui are the
    // old unified binary's way of asking for this mode.
    #[arg(long, hide = true)]
    pub app: bool,
    #[arg(long, hide = true)]
    pub gui: bool,
    #[arg(long, hide = true)]
    pub cli: bool,
    #[arg(long, hide = true)]
    pub no_gui: bool,
}

/// Whether the program was started as `md` (through the bundle's link or a
/// link to it on the PATH) rather than as the app.
pub fn invoked_as_md(arg0: Option<&OsStr>) -> bool {
    arg0.and_then(|a| Path::new(a).file_name()) == Some(OsStr::new("md"))
}

/// Runs `md` and exits: `--app`/`--gui` opens the files in the app instead.
pub fn main() -> ! {
    let args: Vec<OsString> = env::args_os().collect();
    let result = if args.iter().skip(1).any(|a| a == "--app" || a == "--gui") {
        open_in_app(&args[1..])
    } else {
        run(args)
    };
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
    std::process::exit(0)
}

/// Hands the files to the app through Launch Services, so it opens (or
/// comes forward) like any other launch instead of as a child of the shell.
fn open_in_app(args: &[OsString]) -> Result<()> {
    let files = args
        .iter()
        .filter(|a| !a.to_string_lossy().starts_with("--"));
    let status = Command::new("/usr/bin/open")
        .args(["-b", crate::BUNDLE_ID])
        .args(files)
        .status()
        .context("failed to run open")?;
    anyhow::ensure!(status.success(), "open exited with status: {status}");
    Ok(())
}

// ─── TUI state ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiMode {
    Preview,
    Edit,
}

struct TuiApp {
    path: PathBuf,
    render_options: RenderOptions,
    source: String,
    rendered_text: Text<'static>,
    preview_scroll: u16,
    status: String,
    mode: UiMode,
    editor: TextArea<'static>,
    dirty: bool,
}

impl TuiApp {
    fn load(path: PathBuf, options: RenderOptions) -> Result<Self> {
        let source = fs::read_to_string(&path)
            .with_context(|| format!("failed to read markdown file: {}", path.display()))?;
        let mut app = Self {
            path,
            render_options: options,
            source,
            rendered_text: Text::raw(String::new()),
            preview_scroll: 0,
            status: String::new(),
            mode: UiMode::Preview,
            editor: TextArea::default(),
            dirty: false,
        };
        app.editor = build_editor(&app.source);
        app.refresh_preview();
        app.set_status("loaded");
        Ok(app)
    }

    fn refresh_preview(&mut self) {
        let ansi = render_markdown(&self.source, self.render_options);
        self.rendered_text = ansi
            .as_str()
            .into_text()
            .unwrap_or_else(|_| Text::raw(ansi));
    }

    fn file_summary(&self) -> String {
        format!(
            "{} lines, {} bytes",
            self.source.lines().count(),
            self.source.len()
        )
    }

    fn set_status(&mut self, msg: &str) {
        self.status = format!("{msg} | {}", self.file_summary());
    }

    fn reload_from_disk(&mut self) -> Result<()> {
        self.source = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read markdown file: {}", self.path.display()))?;
        self.editor = build_editor(&self.source);
        self.refresh_preview();
        self.preview_scroll = 0;
        self.dirty = false;
        self.set_status("reloaded from disk");
        Ok(())
    }

    fn save_to_disk(&mut self) -> Result<()> {
        fs::write(&self.path, &self.source)
            .with_context(|| format!("failed to write markdown file: {}", self.path.display()))?;
        self.dirty = false;
        self.set_status("saved");
        Ok(())
    }

    fn enter_edit_mode(&mut self) {
        self.mode = UiMode::Edit;
        self.editor = build_editor(&self.source);
        self.set_status("edit mode: live preview");
    }

    fn exit_edit_mode(&mut self) -> Result<()> {
        let had_changes = self.dirty;
        if had_changes {
            self.save_to_disk()?;
        }
        self.mode = UiMode::Preview;
        self.set_status(if had_changes {
            "edit mode closed; auto-saved"
        } else {
            "edit mode closed"
        });
        Ok(())
    }

    fn apply_editor_change(&mut self) {
        self.source = self.editor.lines().join("\n");
        self.refresh_preview();
        self.dirty = true;
        self.set_status("editing");
    }
}

fn build_editor(source: &str) -> TextArea<'static> {
    let mut ta = TextArea::from(source.split('\n'));
    ta.set_block(
        Block::default()
            .title("Source Editor")
            .borders(Borders::ALL),
    );
    ta.set_cursor_line_style(Style::default().bg(Color::DarkGray));
    ta
}

// ─── CLI entry point ──────────────────────────────────────────────────────────

fn run(args: Vec<OsString>) -> Result<()> {
    let cli = Cli::parse_from(normalize_legacy_args(args));
    let options = build_render_options(&cli);

    if should_use_tui(&cli) {
        // The viewer's ANSI parser stops at link escapes; links stay styled.
        let options = RenderOptions {
            hyperlinks: false,
            ..options
        };
        return run_tui(cli.path, options);
    }

    let source = fs::read_to_string(&cli.path)
        .with_context(|| format!("failed to read markdown file: {}", cli.path.display()))?;
    let output = render_markdown(&source, options);

    let use_pager = should_use_pager(cli.no_pager || cli.plain, cli.pager.is_some());
    emit_output(output, use_pager, cli.pager)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn build_render_options(cli: &Cli) -> RenderOptions {
    RenderOptions {
        // The terminal's width (less the viewer's frame), up to 100 columns.
        width: cli.width.unwrap_or_else(|| {
            crossterm::terminal::size()
                .map(|(columns, _)| usize::from(columns).saturating_sub(4).clamp(40, 100))
                .unwrap_or(100)
        }),
        color: !(cli.no_color || cli.plain),
        ascii_mermaid: cli.ascii_mermaid,
        ..RenderOptions::default()
    }
}

fn normalize_legacy_args(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut v: Vec<OsString> = args.into_iter().collect();
    // Accept `md view <file>` as an alias for `md <file>`
    if v.get(1).is_some_and(|a| a == OsStr::new("view")) {
        v.remove(1);
    }
    v
}

#[cfg(test)]
mod viewer_tests {
    use ansi_to_tui::IntoText;

    #[test]
    fn the_viewer_shows_styled_text_without_raw_escapes() {
        let ansi = crate::terminal::render(
            "# Title\n\nA [link](https://example.com) and `code`.",
            crate::terminal::Options {
                hyperlinks: false,
                ..Default::default()
            },
        );
        let text = ansi.as_str().into_text().unwrap();
        let shown: String = text
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert!(shown.contains("A link and code."), "{shown:?}");
        assert!(
            !shown.contains('\x1b') && !shown.contains("8;;"),
            "{shown:?}"
        );
    }
}

fn should_use_tui(cli: &Cli) -> bool {
    if cli.no_tui || cli.plain || cli.pager.is_some() {
        return false;
    }
    std::io::stdout().is_terminal() && std::io::stdin().is_terminal()
}

fn run_tui(path: PathBuf, options: RenderOptions) -> Result<()> {
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend).context("failed to initialize terminal")?;
    term.clear().context("failed to clear terminal")?;

    let mut app = TuiApp::load(path, options)?;
    let result = run_tui_loop(&mut term, &mut app);
    restore_terminal(&mut term).and(result)
}

fn run_tui_loop(term: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut TuiApp) -> Result<()> {
    loop {
        term.draw(|f| draw_ui(f, app))
            .context("failed to draw frame")?;
        if !poll(Duration::from_millis(200)).context("failed to poll events")? {
            continue;
        }

        let Event::Key(key) = read().context("failed to read event")? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        let quit = match app.mode {
            UiMode::Preview => handle_preview_key(app, key)?,
            UiMode::Edit => handle_edit_key(app, key)?,
        };
        if quit {
            break;
        }
    }
    Ok(())
}

fn handle_preview_key(app: &mut TuiApp, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
        KeyCode::Down | KeyCode::Char('j') => {
            app.preview_scroll = app.preview_scroll.saturating_add(1)
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.preview_scroll = app.preview_scroll.saturating_sub(1)
        }
        KeyCode::PageDown => app.preview_scroll = app.preview_scroll.saturating_add(12),
        KeyCode::PageUp => app.preview_scroll = app.preview_scroll.saturating_sub(12),
        KeyCode::Home => app.preview_scroll = 0,
        KeyCode::End => app.preview_scroll = u16::MAX,
        KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::SHIFT) => {
            app.preview_scroll = u16::MAX
        }
        KeyCode::Char('g') => app.preview_scroll = 0,
        KeyCode::Char('r') => app.reload_from_disk()?,
        KeyCode::Char('e') => app.enter_edit_mode(),
        _ => {}
    }
    Ok(false)
}

fn handle_edit_key(app: &mut TuiApp, key: KeyEvent) -> Result<bool> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && matches!(key.code, KeyCode::Char('q')) {
        return Ok(true);
    }
    if ctrl && matches!(key.code, KeyCode::Char('s')) {
        app.save_to_disk()?;
        return Ok(false);
    }
    if ctrl && matches!(key.code, KeyCode::Char('r')) {
        app.reload_from_disk()?;
        return Ok(false);
    }
    if matches!(key.code, KeyCode::Esc) {
        app.exit_edit_mode()?;
        return Ok(false);
    }
    if app.editor.input(key) {
        app.apply_editor_change();
    }
    Ok(false)
}

fn draw_ui(frame: &mut Frame, app: &TuiApp) {
    let [main_area, status_area] = *Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area())
    else {
        return;
    };

    match app.mode {
        UiMode::Preview => {
            let title = format!("{}  [q quit · e edit · r reload]", app.path.display());
            frame.render_widget(
                Paragraph::new(app.rendered_text.clone())
                    .block(Block::default().title(title).borders(Borders::ALL))
                    .scroll((app.preview_scroll, 0))
                    .wrap(Wrap { trim: false }),
                main_area,
            );
        }
        UiMode::Edit => {
            let [editor_area, preview_area] = *Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(main_area)
            else {
                return;
            };

            frame.render_widget(&app.editor, editor_area);
            let scroll = u16::try_from(app.editor.cursor().0).unwrap_or(u16::MAX);
            frame.render_widget(
                Paragraph::new(app.rendered_text.clone())
                    .block(Block::default().title("Live Preview").borders(Borders::ALL))
                    .scroll((scroll, 0))
                    .wrap(Wrap { trim: false }),
                preview_area,
            );
        }
    }

    let dirty = if app.dirty { "*" } else { "" };
    let mode = match app.mode {
        UiMode::Preview => "VIEW",
        UiMode::Edit => "EDIT",
    };
    let hint = match app.mode {
        UiMode::Preview => "j/k scroll  PgUp/Dn jump  g/G top/bottom  e edit  r reload",
        UiMode::Edit => "type to edit  Ctrl+S save  Esc close-edit  Ctrl+Q quit",
    };
    frame.render_widget(
        Paragraph::new(format!(
            "[{mode}{dirty}] {} | {} | {hint}",
            app.path.display(),
            app.status
        ))
        .style(Style::default().fg(Color::Black).bg(Color::Cyan)),
        status_area,
    );
}

fn restore_terminal(term: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(
        term.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .context("failed to leave alternate screen")?;
    term.show_cursor().context("failed to restore cursor")?;
    Ok(())
}

fn should_use_pager(no_pager: bool, pager_override: bool) -> bool {
    if no_pager {
        return false;
    }
    pager_override || std::io::stdout().is_terminal()
}

fn emit_output(output: String, use_pager: bool, pager_override: Option<String>) -> Result<()> {
    if !use_pager {
        print!("{output}");
        return Ok(());
    }
    let spec = pager_override
        .or_else(|| env::var("PAGER").ok())
        .unwrap_or_else(|| "less -R".to_string());
    match pipe_to_pager(&spec, &output) {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("warning: pager unavailable ({e}), writing to stdout");
            print!("{output}");
            Ok(())
        }
    }
}

fn pipe_to_pager(spec: &str, output: &str) -> Result<()> {
    let parts =
        shell_words::split(spec).with_context(|| format!("invalid pager command: {spec}"))?;
    anyhow::ensure!(!parts.is_empty(), "pager command is empty");

    let mut child = Command::new(&parts[0])
        .args(&parts[1..])
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start pager '{}'", parts[0]))?;

    child
        .stdin
        .as_mut()
        .context("failed to open pager stdin")?
        .write_all(output.as_bytes())
        .context("failed to write to pager")?;

    let status = child.wait().context("failed waiting for pager")?;
    anyhow::ensure!(status.success(), "pager exited with status: {status}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md_mode_follows_the_name_the_program_was_run_as() {
        for md in [
            "md",
            "/Users/me/.cargo/bin/md",
            "/Applications/MD Star.app/Contents/MacOS/md",
        ] {
            assert!(invoked_as_md(Some(OsStr::new(md))), "{md}");
        }
        for app in [
            "MD Star",
            "/Applications/MD Star.app/Contents/MacOS/MD Star",
            "mdstar-volt",
            "mdx",
        ] {
            assert!(!invoked_as_md(Some(OsStr::new(app))), "{app}");
        }
        assert!(!invoked_as_md(None));
    }

    #[test]
    fn view_alias_and_old_mode_flags_are_accepted() {
        let args = ["md", "view", "notes.md", "--cli", "--no-gui"].map(OsString::from);
        let cli = Cli::try_parse_from(normalize_legacy_args(args)).unwrap();
        assert_eq!(cli.path, PathBuf::from("notes.md"));
    }
}
