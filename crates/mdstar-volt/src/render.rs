//! Markdown document IR to Volt elements.
//!
//! A document becomes a flat list of [`Unit`]s, the rows of a virtualised
//! list: only the units near the viewport are built, so documents of any
//! length render. Long lists, tables and code blocks split into several
//! units so no single row grows past the element budget.

use std::{collections::HashMap, ops::Range};

use mdstar_core::document_ir::{BlockIr, InlineIr};
use volt_ui_kit::{graphics::Image, ui::prelude::*};

use crate::App;

/// Each text element holds at most 8 KiB.
const TEXT_LIMIT: usize = 8000;
pub const BODY: f32 = 16.;
const LINE: f32 = 1.7;
pub const MEASURE: f32 = 680.;
const CODE_SIZE: f32 = 13.;
const CODE_LINE: f32 = CODE_SIZE * 1.2;
const CODE_HEADER: f32 = 34.;
/// Rows per table unit and lines per code unit.
const TABLE_ROWS: usize = 40;
const CODE_LINES: usize = 80;
/// Space between ordinary blocks, and before each heading level.
const BLOCK_GAP: f32 = 16.;
/// Width of the list marker column; list text starts GUTTER + 10 in.
const GUTTER: f32 = 22.;
const HEADING_GAP: [f32; 6] = [32., 34., 24., 18., 14., 14.];
pub const MARGIN_X: f32 = 56.;
pub const MARGIN_TOP: f32 = 44.;
pub const MARGIN_BOTTOM: f32 = 96.;

pub struct Ctx<'a> {
    pub images: &'a HashMap<String, Image>,
    /// The code block whose Copy button was pressed last.
    pub copied: Option<&'a str>,
}

/// One row of the virtualised document.
pub enum Unit<'a> {
    Block(&'a BlockIr),
    /// One item of a top-level list.
    Item { list: &'a BlockIr, index: usize },
    /// A run of table rows; the first unit carries the header.
    Table {
        table: &'a BlockIr,
        rows: Range<usize>,
        first: bool,
        last: bool,
    },
    /// A run of code lines; the first unit carries the header.
    Code {
        block: &'a BlockIr,
        text: String,
        part: usize,
        last: bool,
    },
}

/// Split a document into units.
pub fn units(blocks: &[BlockIr]) -> Vec<Unit<'_>> {
    let mut out = Vec::with_capacity(blocks.len());
    for block in blocks {
        match block.kind.as_str() {
            "frontmatter" => {}
            "html" => {
                let raw = block.raw.as_deref().unwrap_or("").trim();
                if !raw.is_empty() && !raw.starts_with("<!--") {
                    out.push(Unit::Block(block));
                }
            }
            "list" if block.items.len() > 1 => {
                out.extend((0..block.items.len()).map(|index| Unit::Item { list: block, index }));
            }
            "table" => {
                let n = block.rows.len();
                let mut start = 0;
                loop {
                    let end = (start + TABLE_ROWS).min(n);
                    out.push(Unit::Table {
                        table: block,
                        rows: start..end,
                        first: start == 0,
                        last: end == n,
                    });
                    if end == n {
                        break;
                    }
                    start = end;
                }
            }
            "code" | "math" => {
                let source = if block.kind == "math" {
                    block.raw.as_deref().unwrap_or("")
                } else {
                    block.code.as_deref().unwrap_or("")
                };
                let parts = chunks(source);
                let count = parts.len();
                for (part, text) in parts.into_iter().enumerate() {
                    out.push(Unit::Code {
                        block,
                        text,
                        part,
                        last: part + 1 == count,
                    });
                }
            }
            _ => out.push(Unit::Block(block)),
        }
    }
    out
}

/// Vertical space above unit `i`.
pub fn space_above(units: &[Unit], i: usize) -> f32 {
    if i == 0 {
        return 0.;
    }
    match &units[i] {
        Unit::Item { index, .. } if *index > 0 => 6.,
        Unit::Table { first: false, .. } | Unit::Code { part: 1.., .. } => 0.,
        Unit::Block(b) if b.kind == "heading" => {
            HEADING_GAP[usize::from(b.level.unwrap_or(1).clamp(1, 6)) - 1]
        }
        _ => BLOCK_GAP,
    }
}

/// The unit's own key: stable for a given document.
pub fn unit_key(units: &[Unit], i: usize) -> String {
    match &units[i] {
        Unit::Block(b) => format!("u-{}", key_of(&b.id)),
        Unit::Item { list, index } => format!("u-{}", key_of(&list.items[*index].id)),
        Unit::Table { table, rows, .. } => format!("u-{}-r{}", key_of(&table.id), rows.start),
        Unit::Code { block, part, .. } => format!("u-{}-p{part}", key_of(&block.id)),
    }
}

/// Row `i` of the document: the unit, centred on the reading measure, with
/// its spacing and the page margins.
pub fn row(units: &[Unit], i: usize, ctx: &Ctx) -> Element<App> {
    let content = match &units[i] {
        Unit::Block(b) => block_element(b, ctx, 0),
        Unit::Item { list, index } => list_item(list, *index, ctx, 0),
        Unit::Table {
            table,
            rows,
            first,
            last,
        } => table_part(table, rows.clone(), *first, *last, ctx),
        Unit::Code {
            block,
            text,
            part,
            last,
        } => code_part(block, text, *part, *last, ctx),
    };
    let mut top = space_above(units, i);
    let mut bottom = 0.;
    if i == 0 {
        top += MARGIN_TOP;
    }
    if i + 1 == units.len() {
        bottom += MARGIN_BOTTOM;
    }
    vstack![vstack![content].width_fraction(1.).max_width(MEASURE)]
        .key(unit_key(units, i))
        .align(Align::Center)
        .padding(MARGIN_X, Edge::Horizontal)
        .padding(top, Edge::Top)
        .padding(bottom, Edge::Bottom)
        .into()
}

/// An estimate of row `i`'s height, refined once the row is measured.
pub fn estimate(units: &[Unit], i: usize, images: &HashMap<String, Image>) -> f32 {
    let text = |chars: usize, size: f32, line: f32| {
        let per_line = (MEASURE / (size * 0.5)).max(10.);
        (chars as f32 / per_line).ceil().max(1.) * size * line
    };
    let body = match &units[i] {
        Unit::Block(b) => block_estimate(b, images, &text),
        Unit::Item { list, index } => list.items[*index]
            .children
            .iter()
            .map(|c| block_estimate(c, images, &text))
            .sum(),
        Unit::Table { rows, first, .. } => {
            rows.len() as f32 * 41. + if *first { 41. } else { 0. }
        }
        Unit::Code { text, part, .. } => {
            text.lines().count().max(1) as f32 * CODE_LINE
                + 22.
                + if *part == 0 { CODE_HEADER } else { 0. }
        }
    };
    let mut extra = space_above(units, i);
    if i == 0 {
        extra += MARGIN_TOP;
    }
    if i + 1 == units.len() {
        extra += MARGIN_BOTTOM;
    }
    (body + extra).max(1.)
}

fn block_estimate(b: &BlockIr, images: &HashMap<String, Image>, text: &dyn Fn(usize, f32, f32) -> f32) -> f32 {
    match b.kind.as_str() {
        "heading" => {
            let size = [32., 24., 19., 16.5, 15., 14.][usize::from(b.level.unwrap_or(1).clamp(1, 6)) - 1];
            text(plain_len(&b.inlines), size, 1.25)
        }
        "paragraph" => match lone_image(&b.inlines).and_then(|i| images.get(i.url.as_deref()?)) {
            Some(img) => {
                let (w, h) = (img.width() as f32, img.height() as f32);
                let width = image_width(w);
                width * h / w.max(1.) + 40.
            }
            None => text(plain_len(&b.inlines), BODY, LINE),
        },
        "blockquote" => b.children.iter().map(|c| block_estimate(c, images, text) + 10.).sum(),
        "list" => b
            .items
            .iter()
            .map(|item| item.children.iter().map(|c| block_estimate(c, images, text)).sum::<f32>() + 6.)
            .sum(),
        "thematic_break" => 30.,
        _ => text(b.raw.as_deref().map_or(40, str::len), 12., 1.4),
    }
}

fn plain_len(inlines: &[InlineIr]) -> usize {
    inlines
        .iter()
        .map(|i| i.text.as_deref().map_or(0, str::len) + plain_len(&i.children))
        .sum()
}

fn block_element(block: &BlockIr, ctx: &Ctx, depth: usize) -> Element<App> {
    let key = key_of(&block.id);
    match block.kind.as_str() {
        "heading" => heading(block, key),
        "paragraph" => {
            if let Some(image) = lone_image(&block.inlines) {
                return image_block(image, key, ctx);
            }
            paragraph(&block.inlines, BODY).key(key).into()
        }
        "blockquote" => {
            let children: Vec<Element<App>> = block
                .children
                .iter()
                .map(|b| block_element(b, ctx, depth + 1))
                .collect();
            hstack![
                vstack![]
                    .width(3.)
                    .flex(0.)
                    .background(ColorToken::Primary.alpha(45))
                    .rounded(2.),
                vstack(children).gap(10., Axis::Vertical).flex(1.),
            ]
            .key(key)
            .gap(16., Axis::Horizontal)
            .align(Align::Stretch)
            .padding(2., Edge::Vertical)
            .color(ColorToken::Muted)
            .into()
        }
        "list" => {
            let items: Vec<Element<App>> = (0..block.items.len())
                .map(|i| list_item(block, i, ctx, depth))
                .collect();
            vstack(items)
                .key(key)
                .gap(6., Axis::Vertical)
                .into()
        }
        // Nested code (inside lists and quotes) renders whole.
        "code" | "math" => {
            let source = if block.kind == "math" {
                block.raw.as_deref().unwrap_or("")
            } else {
                block.code.as_deref().unwrap_or("")
            };
            let parts = chunks(source);
            let count = parts.len();
            let parts: Vec<Element<App>> = parts
                .iter()
                .enumerate()
                .map(|(i, text)| code_part(block, text, i, i + 1 == count, ctx))
                .collect();
            vstack(parts).key(key).into()
        }
        "table" => table_part(block, 0..block.rows.len(), true, true, ctx),
        "thematic_break" => vstack![divider().background(ColorToken::Border)]
            .key(key)
            .padding(14., Edge::Vertical)
            .into(),
        "html" => text(clip(block.raw.as_deref().unwrap_or("").trim(), TEXT_LIMIT))
            .key(key)
            .font_family("SF Mono")
            .font_size(12.)
            .color(ColorToken::Muted)
            .into(),
        _ => vstack![].key(key).into(),
    }
}

fn heading(block: &BlockIr, key: String) -> Element<App> {
    let level = usize::from(block.level.unwrap_or(1).clamp(1, 6));
    let size = [32., 24., 19., 16.5, 15., 14.][level - 1];
    paragraph(&block.inlines, size)
        .key(key)
        .font_family("SF Pro Display")
        .font_weight(if level <= 2 { 700 } else { 600 })
        .letter_spacing(if level <= 2 { -0.012 } else { 0. })
        .line_height(1.25)
        .into()
}

fn list_item(list: &BlockIr, index: usize, ctx: &Ctx, depth: usize) -> Element<App> {
    let ordered = list.ordered.unwrap_or(false);
    let item = &list.items[index];
    // Bullets, numbers and checkboxes share one right-aligned gutter so
    // every kind of list indents its text by the same amount.
    let marker: Element<App> = match item.checked {
        Some(done) => vstack![task_box(done)].padding(6., Edge::Top).into(),
        None if ordered => text(format!("{}.", list.start.unwrap_or(1) as usize + index))
            .color(ColorToken::Muted)
            .font_size(BODY)
            .line_height(LINE)
            .into(),
        None => text(if depth.is_multiple_of(2) { "•" } else { "◦" })
            .color(ColorToken::Muted)
            .font_size(BODY)
            .line_height(LINE)
            .into(),
    };
    let body: Vec<Element<App>> = item
        .children
        .iter()
        .map(|b| block_element(b, ctx, depth + 1))
        .collect();
    hstack![
        vstack![marker].width(GUTTER).align(Align::End).flex(0.),
        vstack(body).gap(6., Axis::Vertical).flex(1.),
    ]
    .key(key_of(&item.id))
    .gap(10., Axis::Horizontal)
    .into()
}

fn task_box(done: bool) -> Element<App> {
    let mut b = vstack(if done {
        vec![icon(Icon::Check).size(11.).color(ColorToken::PrimaryContent).into()]
    } else {
        vec![]
    })
    .width(16.)
    .height(16.)
    .rounded(4.)
    .flow(Flow::Center)
    .border_color(ColorToken::Muted.alpha(60))
    .border_width(1.5);
    if done {
        b = b.background(ColorToken::Primary).border_color(ColorToken::Primary);
    }
    b.into()
}

/// One part of a code block. The first part carries the language and a
/// Copy button for the whole block; parts join into one rounded frame.
fn code_part(block: &BlockIr, code: &str, part: usize, last: bool, ctx: &Ctx) -> Element<App> {
    let key = key_of(&block.id);
    let first = part == 0;
    let mut parts: Vec<Element<App>> = Vec::new();
    if first {
        let copied = ctx.copied == Some(key.as_str());
        let source = if block.kind == "math" {
            block.raw.clone().unwrap_or_default()
        } else {
            block.code.clone().unwrap_or_default()
        };
        let id = key.clone();
        let language = if block.kind == "math" {
            "math"
        } else {
            block.language.as_deref().unwrap_or("text")
        };
        parts.push(
            hstack![
                text(language.to_uppercase())
                    .font_size(10.5)
                    .font_weight(600)
                    .letter_spacing(0.06)
                    .color(ColorToken::Muted),
                spacer(),
                button(if copied { "Copied" } else { "Copy" })
                    .key(format!("{key}-copy"))
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Small)
                    .font_size(12.)
                    .color(if copied { ColorToken::Success } else { ColorToken::Muted })
                    .on_click(move |app: &mut App| {
                        if volt_ui_kit::native::clipboard::set_text(&source).is_ok() {
                            app.copied = Some(id.clone());
                        }
                    }),
            ]
            .height(CODE_HEADER)
            .padding(10., Edge::Left)
            .padding(6., Edge::Right)
            .align(Align::Center)
            .border_sides(Sides::BOTTOM)
            .border_color(ColorToken::Border)
            .border_width(1.)
            .into(),
        );
    }
    let lines = code.lines().count().max(1) as f32;
    // Read-only text area: selectable, Cmd/Ctrl+C copies a selection.
    parts.push(
        text_area(code)
            .key(format!("{key}-code-{part}"))
            .read_only(true)
            .on_change(|_: &mut App, _| {})
            .fill_width()
            .height(lines * CODE_LINE + if first && last { 22. } else { 12. })
            .font_family("SF Mono")
            .font_size(CODE_SIZE)
            .background("#00000000")
            .border_width(0.)
            .rounded(0.)
            .into(),
    );
    let sides = match (first, last) {
        (true, true) => Sides::ALL,
        (true, false) => Sides::TOP | Sides::LEFT | Sides::RIGHT,
        (false, true) => Sides::BOTTOM | Sides::LEFT | Sides::RIGHT,
        (false, false) => Sides::LEFT | Sides::RIGHT,
    };
    vstack(parts)
        .key(format!("{key}-part-{part}"))
        .background(ColorToken::Base200)
        .border_sides(sides)
        .border_color(ColorToken::Border)
        .border_width(1.)
        .rounded_on(10., corners(first, last))
        .clip_contents(true)
        .into()
}

/// Rounded corners for the first and last part of a split frame.
fn corners(first: bool, last: bool) -> Vec<Corner> {
    let mut out = Vec::new();
    if first {
        out.extend([Corner::TopLeft, Corner::TopRight]);
    }
    if last {
        out.extend([Corner::BottomLeft, Corner::BottomRight]);
    }
    out
}

/// Table rows `rows` (with the header on the first part); parts join into
/// one rounded frame and share column widths.
fn table_part(
    table: &BlockIr,
    rows: Range<usize>,
    first: bool,
    last: bool,
    _ctx: &Ctx,
) -> Element<App> {
    let columns = table
        .rows
        .iter()
        .map(Vec::len)
        .chain([table.headers.len()])
        .max()
        .unwrap_or(0)
        .max(1);
    let total = table.rows.len();
    let mut cells: Vec<Element<App>> = Vec::new();
    let mut row = |cols: &[Vec<InlineIr>], header: bool, rule: bool| {
        for i in 0..columns {
            let inlines = cols.get(i).map(Vec::as_slice).unwrap_or(&[]);
            let mut text = paragraph(inlines, 14.).line_height(1.5);
            if header {
                text = text.font_weight(600);
            }
            let mut cell = vstack![text]
                .padding(14., Edge::Horizontal)
                .padding(9., Edge::Vertical);
            if rule {
                cell = cell
                    .border_sides(Sides::BOTTOM)
                    .border_color(ColorToken::Border)
                    .border_width(1.);
            }
            if header {
                cell = cell.background(ColorToken::Base200);
            }
            cells.push(cell.into());
        }
    };
    if first {
        row(&table.headers, true, total > 0);
    }
    for r in rows.clone() {
        row(&table.rows[r], false, r + 1 < total);
    }
    let sides = match (first, last) {
        (true, true) => Sides::ALL,
        (true, false) => Sides::TOP | Sides::LEFT | Sides::RIGHT,
        (false, true) => Sides::BOTTOM | Sides::LEFT | Sides::RIGHT,
        (false, false) => Sides::LEFT | Sides::RIGHT,
    };
    grid(cells)
        .columns(columns as u16)
        .key(format!("{}-rows-{}", key_of(&table.id), rows.start))
        .border_sides(sides)
        .border_color(ColorToken::Border)
        .border_width(1.)
        .rounded_on(10., corners(first, last))
        .clip_contents(true)
        .into()
}

/// Retina screenshots are usually 2x: show them at their point size.
fn image_width(w: f32) -> f32 {
    (w / 2.).max(w.min(320.)).min(MEASURE)
}

fn image_block(image: &InlineIr, key: String, ctx: &Ctx) -> Element<App> {
    let url = image.url.as_deref().unwrap_or("");
    let alt = image.alt.as_deref().unwrap_or("");
    match ctx.images.get(url) {
        Some(img) => {
            let (w, h) = (img.width() as f32, img.height() as f32);
            let width = image_width(w);
            let mut parts: Vec<Element<App>> = vec![vstack![]
                .width(width)
                .height(width * h / w.max(1.))
                .background_image(img.clone().background().fit(ImageFit::Contain))
                .rounded(10.)
                .clip_contents(true)
                .border_color(ColorToken::Border)
                .border_width(1.)
                .accessible_label(if alt.is_empty() { "Image" } else { alt })
                .into()];
            if !alt.is_empty() {
                parts.push(
                    text(alt)
                        .font_size(12.5)
                        .color(ColorToken::Muted)
                        .text_align(TextAlign::Center)
                        .into(),
                );
            }
            vstack(parts)
                .key(key)
                .gap(8., Axis::Vertical)
                .align(Align::Center)
                .padding(6., Edge::Vertical)
                .into()
        }
        None => text(format!("[image: {}]", if alt.is_empty() { url } else { alt }))
            .key(key)
            .font_size(13.)
            .color(ColorToken::Muted)
            .into(),
    }
}

/// A paragraph's inlines as one rich text, cut at a text element's 8 KiB.
fn paragraph(inlines: &[InlineIr], size: f32) -> volt_ui_kit::ui::Text<App> {
    let mut out = Vec::new();
    spans(inlines, Style::default(), &mut out);
    let has_links = out.iter().any(|(_, s)| s.link.is_some());
    let mut used = 0;
    let mut kept = Vec::with_capacity(out.len());
    for (text, style) in out {
        if used + text.len() > TEXT_LIMIT {
            let cut = clip(&text, TEXT_LIMIT.saturating_sub(used));
            if !cut.is_empty() {
                kept.push(style.span(format!("{cut}…")));
            }
            break;
        }
        used += text.len();
        kept.push(style.span(text));
    }
    let paragraph = rich_text(kept).font_size(size).line_height(LINE);
    if has_links {
        paragraph.on_link(App::follow_link)
    } else {
        paragraph
    }
}

#[derive(Clone, Copy, Default)]
struct Style<'a> {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: Option<&'a str>,
}
impl Style<'_> {
    fn span(self, text: String) -> Span {
        let mut s = span(text);
        if self.code {
            s = s.code();
        }
        if self.bold {
            s = s.bold();
        }
        if self.italic {
            s = s.italic();
        }
        if self.strike {
            s = s.strikethrough();
        }
        if let Some(target) = self.link.filter(|t| !t.is_empty() && t.len() <= 2048) {
            s = s.link(target);
        }
        s
    }
}

fn spans<'a>(inlines: &'a [InlineIr], style: Style<'a>, out: &mut Vec<(String, Style<'a>)>) {
    for inline in inlines {
        let text = inline.text.as_deref().unwrap_or("");
        match inline.kind.as_str() {
            // Soft line breaks inside a paragraph are spaces.
            "text" => out.push((text.replace('\n', " "), style)),
            "strong" => spans(&inline.children, Style { bold: true, ..style }, out),
            "emphasis" => spans(&inline.children, Style { italic: true, ..style }, out),
            "delete" => spans(&inline.children, Style { strike: true, ..style }, out),
            "link" => spans(
                &inline.children,
                Style {
                    link: inline.url.as_deref(),
                    ..style
                },
                out,
            ),
            // Narrow no-break spaces pad the code background on both sides.
            "code" | "math" => out.push((
                format!("\u{202F}{text}\u{202F}"),
                Style { code: true, ..style },
            )),
            "hard_break" => out.push(("\n".into(), style)),
            "image" => out.push((
                format!("[{}]", inline.alt.as_deref().unwrap_or("image")),
                style,
            )),
            "html" => {
                if text.eq_ignore_ascii_case("<br>") || text.eq_ignore_ascii_case("<br/>") {
                    out.push(("\n".into(), style));
                }
            }
            _ => spans(&inline.children, style, out),
        }
    }
}

/// A paragraph that is just one image (ignoring whitespace).
fn lone_image(inlines: &[InlineIr]) -> Option<&InlineIr> {
    let mut found = None;
    for inline in inlines {
        match inline.kind.as_str() {
            "image" if found.is_none() => found = Some(inline),
            "text" if inline.text.as_deref().is_none_or(|t| t.trim().is_empty()) => {}
            _ => return None,
        }
    }
    found
}

/// Image URLs in the document, for loading local files once per document.
pub fn image_urls(blocks: &[BlockIr], out: &mut Vec<String>) {
    fn inlines(list: &[InlineIr], out: &mut Vec<String>) {
        for i in list {
            if i.kind == "image"
                && let Some(url) = &i.url
            {
                out.push(url.clone());
            }
            inlines(&i.children, out);
        }
    }
    for b in blocks {
        inlines(&b.inlines, out);
        image_urls(&b.children, out);
        for item in &b.items {
            image_urls(&item.children, out);
        }
        for row in b.rows.iter().chain([&b.headers]) {
            for cell in row {
                inlines(cell, out);
            }
        }
    }
}

/// Words in the document's prose and code, for the reading-time estimate.
pub fn word_count(blocks: &[BlockIr]) -> usize {
    fn inlines(list: &[InlineIr]) -> usize {
        list.iter()
            .map(|i| {
                i.text.as_deref().map_or(0, |t| t.split_whitespace().count()) + inlines(&i.children)
            })
            .sum()
    }
    blocks
        .iter()
        .map(|b| {
            inlines(&b.inlines)
                + b.code.as_deref().map_or(0, |c| c.split_whitespace().count())
                + word_count(&b.children)
                + b.items.iter().map(|i| word_count(&i.children)).sum::<usize>()
                + b.rows
                    .iter()
                    .chain([&b.headers])
                    .flatten()
                    .map(|c| inlines(c))
                    .sum::<usize>()
        })
        .sum()
}

/// Code split at line boundaries into parts of at most `CODE_LINES` lines
/// and one text element (8 KiB).
fn chunks(code: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut lines = 0;
    for line in code.split('\n') {
        let line = clip(line, TEXT_LIMIT - 1);
        let last = out.last_mut().unwrap();
        if !last.is_empty() && (last.len() + line.len() + 1 > TEXT_LIMIT || lines >= CODE_LINES) {
            out.push(String::new());
            lines = 0;
        }
        let last = out.last_mut().unwrap();
        if lines > 0 {
            last.push('\n');
        }
        last.push_str(line);
        lines += 1;
    }
    out
}

/// The longest prefix of at most `max` bytes, on a character boundary.
fn clip(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// mdstar IDs are already valid Volt keys; bound and sanitise defensively.
pub fn key_of(id: &str) -> String {
    let key: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .take(110)
        .collect();
    if key.is_empty() { "block".into() } else { key }
}
