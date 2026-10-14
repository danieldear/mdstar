//! `md`'s renderer: Markdown as styled terminal text, from the same parser
//! and syntax colours as the app, with Mermaid diagrams drawn in text.

use mdstar_core::{
    document_ir::{BlockIr, InlineIr, parse_document_ir_with_diagnostics},
    expand_emoji_text,
    latex::to_unicode,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::highlight;

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Column to wrap prose at.
    pub width: usize,
    /// ANSI colours, styles and hyperlinks; plain text without.
    pub color: bool,
    /// Mermaid diagrams in ASCII instead of box-drawing characters.
    pub ascii_mermaid: bool,
    /// Clickable links (OSC 8), for terminals rather than the viewer.
    pub hyperlinks: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            width: 100,
            color: true,
            ascii_mermaid: false,
            hyperlinks: true,
        }
    }
}

/// `source` rendered for a terminal, ending in a newline.
pub fn render(source: &str, o: Options) -> String {
    let Ok(ir) = parse_document_ir_with_diagnostics(source, "md") else {
        return source.to_owned();
    };
    let r = Renderer { o };
    let mut out = r.blocks(&ir.blocks, o.width.max(20), 0, false).join("\n");
    out.push('\n');
    out
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Style {
    bold: bool,
    italic: bool,
    dim: bool,
    strike: bool,
    underline: bool,
    fg: Option<Fg>,
}

#[derive(Clone, Copy, PartialEq)]
enum Fg {
    Ansi(u8),
    Rgb(u8, u8, u8),
}

const YELLOW: Fg = Fg::Ansi(33);
const CYAN: Fg = Fg::Ansi(36);
const BLUE: Fg = Fg::Ansi(34);
const GREEN: Fg = Fg::Ansi(32);
const GREY: Fg = Fg::Ansi(90);

impl Style {
    fn fg(fg: Fg) -> Self {
        Self {
            fg: Some(fg),
            ..Self::default()
        }
    }
    fn dim() -> Self {
        Self {
            dim: true,
            ..Self::default()
        }
    }
    /// `self` with `over`'s attributes added.
    fn with(self, over: Style) -> Self {
        Self {
            bold: self.bold || over.bold,
            italic: self.italic || over.italic,
            dim: self.dim || over.dim,
            strike: self.strike || over.strike,
            underline: self.underline || over.underline,
            fg: over.fg.or(self.fg),
        }
    }
    fn sgr(self) -> String {
        let mut codes: Vec<String> = Vec::new();
        for (on, code) in [
            (self.bold, "1"),
            (self.dim, "2"),
            (self.italic, "3"),
            (self.underline, "4"),
            (self.strike, "9"),
        ] {
            if on {
                codes.push(code.into());
            }
        }
        match self.fg {
            Some(Fg::Ansi(n)) => codes.push(n.to_string()),
            Some(Fg::Rgb(r, g, b)) => codes.push(format!("38;2;{r};{g};{b}")),
            None => {}
        }
        codes.join(";")
    }
}

/// A run of text in one style, maybe a link.
#[derive(Clone)]
struct Span {
    text: String,
    style: Style,
    link: Option<String>,
}

/// A wrapped line: painted text and its width in columns.
type Line = (String, usize);

struct Renderer {
    o: Options,
}

impl Renderer {
    fn paint(&self, text: &str, style: Style, link: Option<&str>) -> String {
        if !self.o.color || text.is_empty() {
            return text.to_owned();
        }
        let sgr = style.sgr();
        let body = if sgr.is_empty() {
            text.to_owned()
        } else {
            format!("\x1b[{sgr}m{text}\x1b[0m")
        };
        match link.filter(|_| self.o.hyperlinks) {
            Some(url) => format!("\x1b]8;;{url}\x1b\\{body}\x1b]8;;\x1b\\"),
            None => body,
        }
    }

    fn blocks(&self, blocks: &[BlockIr], width: usize, depth: usize, tight: bool) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for b in blocks {
            let lines = self.block(b, width, depth);
            if lines.is_empty() {
                continue;
            }
            if !out.is_empty() && !tight {
                out.push(String::new());
            }
            out.extend(lines);
        }
        out
    }

    fn block(&self, b: &BlockIr, width: usize, depth: usize) -> Vec<String> {
        match b.kind.as_str() {
            "heading" => self.heading(b, width),
            "paragraph" => {
                if let [image] = b.inlines.as_slice()
                    && image.kind == "image"
                {
                    let alt = image.alt.as_deref().unwrap_or("image");
                    return vec![self.paint(
                        &format!("[image: {alt}]"),
                        Style::dim(),
                        image.url.as_deref(),
                    )];
                }
                let mut spans = Vec::new();
                self.spans(&b.inlines, Style::default(), None, &mut spans);
                self.wrap(&spans, width).into_iter().map(|l| l.0).collect()
            }
            "blockquote" => {
                let bar = self.paint(
                    if self.o.color { "│ " } else { "> " },
                    Style::fg(GREY),
                    None,
                );
                self.blocks(&b.children, width.saturating_sub(2).max(10), depth, false)
                    .into_iter()
                    .map(|l| format!("{bar}{l}"))
                    .collect()
            }
            "list" => self.list(b, width, depth),
            "code" => {
                let code = b.code.as_deref().unwrap_or("");
                let language = b.language.as_deref().unwrap_or("");
                if language.eq_ignore_ascii_case("mermaid") {
                    self.mermaid(code, width)
                } else {
                    self.code(language, code)
                }
            }
            "math" => to_unicode(b.raw.as_deref().unwrap_or(""))
                .lines()
                .map(|l| {
                    let style = Style {
                        italic: true,
                        ..Style::default()
                    };
                    format!("    {}", self.paint(l, style, None))
                })
                .collect(),
            "table" => self.table(b, width),
            "thematic_break" => vec![self.paint(&"─".repeat(width.min(60)), Style::fg(GREY), None)],
            "html" | "frontmatter" => b
                .raw
                .as_deref()
                .unwrap_or("")
                .trim_end()
                .lines()
                .map(|l| self.paint(l, Style::dim(), None))
                .collect(),
            "footnote_definition" => {
                let label = format!("[{}]", b.label.as_deref().unwrap_or("?"));
                let marker = self.paint(&label, Style::fg(CYAN), None);
                self.hanging(&marker, label.width(), &b.children, width, depth, true)
            }
            _ => Vec::new(),
        }
    }

    fn heading(&self, b: &BlockIr, width: usize) -> Vec<String> {
        let level = b.level.unwrap_or(1);
        let base = Style {
            bold: true,
            fg: match level {
                1 | 2 => Some(YELLOW),
                3 => Some(CYAN),
                _ => None,
            },
            ..Style::default()
        };
        let mut spans = Vec::new();
        if !self.o.color && level >= 3 {
            spans.push(Span {
                text: format!("{} ", "#".repeat(usize::from(level))),
                style: base,
                link: None,
            });
        }
        self.spans(&b.inlines, base, None, &mut spans);
        let lines = self.wrap(&spans, width);
        let rule_width = lines.iter().map(|l| l.1).max().unwrap_or(0).min(width);
        let mut out: Vec<String> = lines.into_iter().map(|l| l.0).collect();
        let rule = match (level, self.o.color) {
            (1, true) => Some("═"),
            (2, true) => Some("─"),
            (1, false) => Some("="),
            (2, false) => Some("-"),
            _ => None,
        };
        if let Some(rule) = rule {
            out.push(self.paint(&rule.repeat(rule_width), Style::fg(GREY), None));
        }
        out
    }

    fn list(&self, b: &BlockIr, width: usize, depth: usize) -> Vec<String> {
        let start = b.start.unwrap_or(1) as usize;
        let ordered = b.ordered == Some(true);
        let number_width = (start + b.items.len().max(1) - 1).to_string().len() + 1;
        // The parser keeps no tight/loose flag: a list is loose when an item
        // holds more than its text and nested lists.
        let loose = b
            .items
            .iter()
            .any(|i| i.children.iter().skip(1).any(|c| c.kind != "list"));
        let mut out = Vec::new();
        for (i, item) in b.items.iter().enumerate() {
            let (marker, marker_width) = match item.checked {
                Some(done) => {
                    let (glyph, plain, style) = if done {
                        ("☑", "[x]", Style::fg(GREEN))
                    } else {
                        ("☐", "[ ]", Style::fg(GREY))
                    };
                    let text = if self.o.color { glyph } else { plain };
                    (self.paint(text, style, None), text.width())
                }
                None if ordered => {
                    let n = format!("{}.", start + i);
                    let padded = format!("{n:>number_width$}");
                    (self.paint(&padded, Style::fg(CYAN), None), number_width)
                }
                None => {
                    let bullet = ["•", "◦", "▪"][depth.min(2)];
                    (self.paint(bullet, Style::fg(CYAN), None), 1)
                }
            };
            if loose && i > 0 {
                out.push(String::new());
            }
            out.extend(self.hanging(
                &marker,
                marker_width,
                &item.children,
                width,
                depth + 1,
                !loose,
            ));
        }
        out
    }

    /// Blocks after a marker, with following lines indented under the text.
    fn hanging(
        &self,
        marker: &str,
        marker_width: usize,
        blocks: &[BlockIr],
        width: usize,
        depth: usize,
        tight: bool,
    ) -> Vec<String> {
        let indent = marker_width + 1;
        let inner = self.blocks(blocks, width.saturating_sub(indent).max(10), depth, tight);
        if inner.is_empty() {
            return vec![marker.to_owned()];
        }
        inner
            .into_iter()
            .enumerate()
            .map(|(i, l)| {
                if i == 0 {
                    format!("{marker} {l}")
                } else if l.is_empty() {
                    l
                } else {
                    format!("{}{l}", " ".repeat(indent))
                }
            })
            .collect()
    }

    fn code(&self, language: &str, code: &str) -> Vec<String> {
        let label = if language.is_empty() {
            "───".to_owned()
        } else {
            format!("─── {language}")
        };
        let mut out = vec![format!("  {}", self.paint(&label, Style::fg(GREY), None))];
        let code = code.trim_end_matches('\n');
        if !self.o.color {
            out.extend(code.split('\n').map(|l| format!("    {l}")));
            return out;
        }
        // Colour runs over the whole block, then split into lines.
        let mut runs: Vec<(usize, Option<highlight::Token>)> = Vec::new();
        let mut at = 0;
        for (range, token) in highlight::tokens(language, code, 200_000) {
            if range.start > at {
                runs.push((range.start, None));
            }
            runs.push((range.end, Some(token)));
            at = range.end;
        }
        runs.push((code.len(), None));
        let mut line = String::from("    ");
        let mut start = 0;
        for (end, token) in runs {
            let end = end.min(code.len());
            if end <= start {
                continue;
            }
            let style = match token {
                Some(t) => Style::fg(rgb(highlight::color(t, true))),
                None => Style::default(),
            };
            for (i, piece) in code[start..end].split('\n').enumerate() {
                if i > 0 {
                    out.push(std::mem::replace(&mut line, String::from("    ")));
                }
                line.push_str(&self.paint(piece, style, None));
            }
            start = end;
        }
        out.push(line);
        out
    }

    fn mermaid(&self, code: &str, width: usize) -> Vec<String> {
        // The Sugiyama layout draws flowcharts with loops the right way up;
        // the native one is still needed for subgraphs.
        let backend = if code.contains("subgraph") {
            mermaid_text::layout::layered::LayoutBackend::Native
        } else {
            mermaid_text::layout::layered::LayoutBackend::Sugiyama
        };
        let options = mermaid_text::RenderOptions {
            max_width: Some(width.saturating_sub(4).max(20)),
            ascii: self.o.ascii_mermaid,
            color: self.o.color,
            backend,
            ..Default::default()
        };
        match mermaid_text::render_with_options(code, &options) {
            Ok(drawing) => {
                let label = self.paint("─── mermaid", Style::fg(GREY), None);
                std::iter::once(format!("  {label}"))
                    .chain(drawing.trim_end().lines().map(|l| format!("    {l}")))
                    .collect()
            }
            Err(e) => {
                let mut out = self.code("mermaid", code);
                out.push(self.paint(&format!("  (diagram not drawn: {e})"), Style::dim(), None));
                out
            }
        }
    }

    fn table(&self, b: &BlockIr, width: usize) -> Vec<String> {
        let columns = b
            .rows
            .iter()
            .map(Vec::len)
            .chain([b.headers.len()])
            .max()
            .unwrap_or(0);
        if columns == 0 {
            return Vec::new();
        }
        let cell = |row: &[Vec<InlineIr>], i: usize, header: bool| {
            let mut spans = Vec::new();
            let base = Style {
                bold: header,
                ..Style::default()
            };
            if let Some(inlines) = row.get(i) {
                self.spans(inlines, base, None, &mut spans);
            }
            spans
        };
        let mut rows: Vec<Vec<Vec<Span>>> =
            vec![(0..columns).map(|i| cell(&b.headers, i, true)).collect()];
        for r in &b.rows {
            rows.push((0..columns).map(|i| cell(r, i, false)).collect());
        }
        // Natural widths, then the widest columns give way until it fits.
        let mut widths: Vec<usize> = (0..columns)
            .map(|i| {
                rows.iter()
                    .map(|r| spans_width(&r[i]))
                    .max()
                    .unwrap_or(0)
                    .max(3)
            })
            .collect();
        let room = width.saturating_sub(3 * columns + 1);
        while widths.iter().sum::<usize>() > room {
            let (i, w) = widths
                .iter()
                .enumerate()
                .max_by_key(|(_, w)| **w)
                .map(|(i, w)| (i, *w))
                .unwrap();
            if w <= 6 {
                break;
            }
            widths[i] -= 1;
        }
        let border = |left: &str, mid: &str, right: &str| {
            let inner: Vec<String> = widths.iter().map(|w| "─".repeat(w + 2)).collect();
            self.paint(
                &format!("{left}{}{right}", inner.join(mid)),
                Style::fg(GREY),
                None,
            )
        };
        let bar = self.paint("│", Style::fg(GREY), None);
        let mut out = vec![border("┌", "┬", "┐")];
        for (r, row) in rows.iter().enumerate() {
            let wrapped: Vec<Vec<Line>> = row
                .iter()
                .zip(&widths)
                .map(|(spans, w)| self.wrap(spans, *w))
                .collect();
            let height = wrapped.iter().map(Vec::len).max().unwrap_or(1).max(1);
            for k in 0..height {
                let mut line = bar.clone();
                for (i, cell) in wrapped.iter().enumerate() {
                    let (text, used) = cell.get(k).cloned().unwrap_or_default();
                    let free = widths[i].saturating_sub(used);
                    let (before, after) = match b.align.get(i).map(String::as_str) {
                        Some("right") => (free, 0),
                        Some("center") => (free / 2, free - free / 2),
                        _ => (0, free),
                    };
                    line.push_str(&format!(
                        " {}{text}{} {bar}",
                        " ".repeat(before),
                        " ".repeat(after)
                    ));
                }
                out.push(line);
            }
            if r == 0 {
                out.push(border("├", "┼", "┤"));
            }
        }
        out.push(border("└", "┴", "┘"));
        out
    }

    fn spans(&self, inlines: &[InlineIr], style: Style, link: Option<&str>, out: &mut Vec<Span>) {
        let push = |out: &mut Vec<Span>, text: String, style: Style, link: Option<&str>| {
            out.push(Span {
                text,
                style,
                link: link.map(str::to_owned),
            })
        };
        for i in inlines {
            let text = i.text.as_deref().unwrap_or("");
            match i.kind.as_str() {
                // Soft line breaks are spaces; hard breaks come separately.
                "text" => push(
                    out,
                    expand_emoji_text(&text.replace('\n', " ")),
                    style,
                    link,
                ),
                "emphasis" => self.spans(
                    &i.children,
                    style.with(Style {
                        italic: true,
                        ..Style::default()
                    }),
                    link,
                    out,
                ),
                "strong" => self.spans(
                    &i.children,
                    style.with(Style {
                        bold: true,
                        ..Style::default()
                    }),
                    link,
                    out,
                ),
                "delete" => self.spans(
                    &i.children,
                    style.with(Style {
                        strike: true,
                        dim: true,
                        ..Style::default()
                    }),
                    link,
                    out,
                ),
                "code" => {
                    let shown = if self.o.color {
                        text.to_owned()
                    } else {
                        format!("`{text}`")
                    };
                    push(out, shown, style.with(Style::fg(CYAN)), link)
                }
                "math" => push(
                    out,
                    to_unicode(text),
                    style.with(Style {
                        italic: true,
                        ..Style::default()
                    }),
                    link,
                ),
                "link" => {
                    let url = i.url.as_deref();
                    let link_style = style.with(Style {
                        underline: true,
                        fg: Some(BLUE),
                        ..Style::default()
                    });
                    let start = out.len();
                    self.spans(&i.children, link_style, url.or(link), out);
                    if !self.o.color
                        && let Some(url) = url
                    {
                        let shown: String = out[start..].iter().map(|s| s.text.as_str()).collect();
                        if shown != url && !url.starts_with('#') {
                            push(out, format!(" ({url})"), style, None);
                        }
                    }
                }
                "image" => {
                    let alt = i.alt.as_deref().unwrap_or("image");
                    push(
                        out,
                        format!("[image: {alt}]"),
                        style.with(Style::dim()),
                        i.url.as_deref(),
                    )
                }
                "html" => push(out, text.to_owned(), style.with(Style::dim()), link),
                "hard_break" => push(out, "\n".into(), style, None),
                "footnote_reference" => {
                    push(out, format!("[{text}]"), style.with(Style::fg(CYAN)), link)
                }
                _ => {
                    if i.children.is_empty() {
                        push(out, text.to_owned(), style, link);
                    } else {
                        self.spans(&i.children, style, link, out);
                    }
                }
            }
        }
    }

    /// Spans wrapped at `width` columns. Words never split unless one is
    /// wider than a whole line.
    fn wrap(&self, spans: &[Span], width: usize) -> Vec<Line> {
        let width = width.max(1);
        // Words: runs of non-space fragments, possibly across styles.
        enum Tok {
            Word(Vec<(String, Style, Option<String>)>),
            Space,
            Break,
        }
        let mut toks: Vec<Tok> = Vec::new();
        let mut word: Vec<(String, Style, Option<String>)> = Vec::new();
        let flush = |word: &mut Vec<(String, Style, Option<String>)>, toks: &mut Vec<Tok>| {
            if !word.is_empty() {
                toks.push(Tok::Word(std::mem::take(word)));
            }
        };
        for s in spans {
            let mut frag = String::new();
            for c in s.text.chars() {
                if c == '\n' || c.is_whitespace() {
                    if !frag.is_empty() {
                        word.push((std::mem::take(&mut frag), s.style, s.link.clone()));
                    }
                    flush(&mut word, &mut toks);
                    toks.push(if c == '\n' { Tok::Break } else { Tok::Space });
                } else {
                    frag.push(c);
                }
            }
            if !frag.is_empty() {
                word.push((frag, s.style, s.link.clone()));
            }
        }
        flush(&mut word, &mut toks);

        let mut lines: Vec<Line> = Vec::new();
        let mut line = String::new();
        let mut used = 0;
        let mut pending_space = false;
        for tok in toks {
            match tok {
                Tok::Space => pending_space = used > 0,
                Tok::Break => {
                    lines.push((std::mem::take(&mut line), used));
                    used = 0;
                    pending_space = false;
                }
                Tok::Word(frags) => {
                    let w: usize = frags.iter().map(|f| f.0.width()).sum();
                    if used > 0 && used + usize::from(pending_space) + w > width {
                        lines.push((std::mem::take(&mut line), used));
                        used = 0;
                        pending_space = false;
                    }
                    if pending_space {
                        line.push(' ');
                        used += 1;
                        pending_space = false;
                    }
                    for (text, style, link) in frags {
                        // Hard-split only words wider than a whole line.
                        let mut piece = String::new();
                        for c in text.chars() {
                            let cw = c.width().unwrap_or(0);
                            if used + cw > width && used > 0 && w > width {
                                line.push_str(&self.paint(
                                    &std::mem::take(&mut piece),
                                    style,
                                    link.as_deref(),
                                ));
                                lines.push((std::mem::take(&mut line), used));
                                used = 0;
                            }
                            piece.push(c);
                            used += cw;
                        }
                        line.push_str(&self.paint(&piece, style, link.as_deref()));
                    }
                }
            }
        }
        if used > 0 || lines.is_empty() {
            lines.push((line, used));
        }
        lines
    }
}

fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.text.width()).sum()
}

fn rgb(hex: &str) -> Fg {
    let v = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    Fg::Rgb(v(1), v(3), v(5))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(source: &str, width: usize) -> String {
        render(
            source,
            Options {
                width,
                color: false,
                ..Options::default()
            },
        )
    }

    #[test]
    fn prose_wraps_and_keeps_inline_meaning() {
        let out = plain(
            "Some **bold**, `code` and a [link](https://example.com) with $E = mc^2$ :rocket:.",
            30,
        );
        assert!(out.lines().all(|l| l.width() <= 30), "{out}");
        let joined = out.replace('\n', " ");
        for needle in [
            "bold,",
            "`code`",
            "link (https://example.com)",
            "E = mc²",
            "🚀",
        ] {
            assert!(joined.contains(needle), "{needle} missing in {out}");
        }
    }

    #[test]
    fn headings_lists_tasks_and_quotes() {
        let out = plain(
            "# Title\n\n- one\n  - nested\n- [x] done\n- [ ] open\n\n1. first\n2. second\n\n> quoted",
            60,
        );
        assert!(out.starts_with("Title\n====="), "{out}");
        assert!(
            out.contains("• one\n  ◦ nested\n[x] done"),
            "tight lists stay tight:\n{out}"
        );
        let soft = plain("> A quoted line\n> that continues.", 60);
        assert_eq!(soft, "> A quoted line that continues.\n");
        let long = plain("8. eight\n9. nine\n10. ten", 40);
        assert!(
            long.contains(" 9. nine\n10. ten"),
            "numbers align right:\n{long}"
        );
        for needle in [
            "• one",
            "  ◦ nested",
            "[x] done",
            "[ ] open",
            "1. first\n2. second",
            "> quoted",
        ] {
            assert!(out.contains(needle), "{needle} missing in\n{out}");
        }
    }

    #[test]
    fn tables_align_and_fit() {
        let out = plain(
            "| Name | Value | Notes |\n|:-----|------:|:-----:|\n| Alpha | 1 | first |\n| Beta | 22 | second row |",
            80,
        );
        assert!(out.contains("│ Name  │ Value │   Notes    │"), "{out}");
        assert!(out.contains("│ Alpha │     1 │   first    │"), "{out}");
        let narrow = plain(
            "| a | b |\n|---|---|\n| a long cell that must wrap | another long cell to wrap |",
            30,
        );
        assert!(narrow.lines().all(|l| l.width() <= 30), "{narrow}");
    }

    #[test]
    fn code_math_mermaid_and_footnotes() {
        let out = plain(
            "```rust\nfn main() {}\n```\n\n$$\n\\sum_{i=1}^{n} x_i^2\n$$\n\n```mermaid\ngraph TD\n  A --> B\n```\n\nText[^1].\n\n[^1]: The note.",
            80,
        );
        for needle in [
            "─── rust",
            "    fn main() {}",
            "∑",
            "─── mermaid",
            "Text[1].",
            "[1] The note.",
        ] {
            assert!(out.contains(needle), "{needle} missing in\n{out}");
        }
        assert!(!out.contains('\x1b'), "plain output has no escapes");
        let coloured = render("```rust\nfn main() {}\n```", Options::default());
        assert!(coloured.contains("\x1b[38;2;"), "code is coloured");
    }
}
