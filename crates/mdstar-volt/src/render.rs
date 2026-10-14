//! Markdown document IR to Volt elements.
//!
//! A document becomes a flat list of [`Unit`]s, the rows of a virtualised
//! list: only the units near the viewport are built, so documents of any
//! length render. Long lists, tables and code blocks split into several
//! units so no single row grows past the element budget.
//!
//! Every text a row shows comes from [`unit_texts`] in the same order the
//! row renders it, so find can count matches without building rows and the
//! renderer can highlight the same matches.

use std::{cell::Cell, collections::HashMap, ops::Range};

use mdstar_core::{
    document_ir::{BlockIr, InlineIr},
    expand_emoji_text,
    latex::to_unicode,
};
use volt_ui_kit::{graphics::Image, ui::prelude::*};

use crate::{
    App,
    diagram::{self, Diagram},
    find::Hit,
    highlight::{self, Token},
};

/// Each text element holds at most 8 KiB.
const TEXT_LIMIT: usize = 8000;
/// Rows per table unit and lines per code unit.
const TABLE_ROWS: usize = 40;
const CODE_LINES: usize = 80;
const CODE_HEADER: f32 = 34.;
/// Space around a drawn diagram inside its card.
const DIAGRAM_PAD: f32 = 18.;
/// Heading sizes relative to the body size, H1 to H6.
const HEADING_SCALE: [f32; 6] = [2.0, 1.5, 1.25, 1.1, 1.0, 0.9];
/// Space above each heading level, in body sizes.
const HEADING_GAP: [f32; 6] = [2.0, 2.1, 1.6, 1.3, 1.1, 1.1];
/// Page side margins: generous when there is room, tighter in narrow panes.
pub fn margin_x(page_width: Option<f32>) -> f32 {
    match page_width {
        Some(w) if w < 560. => 18.,
        Some(w) if w < 760. => 32.,
        _ => 56.,
    }
}
/// The narrowest a table column gets before the table scrolls sideways
/// instead of breaking words.
const MIN_COLUMN: f32 = 110.;
pub const MARGIN_TOP: f32 = 44.;
pub const MARGIN_BOTTOM: f32 = 120.;

/// Typography from the reader's settings.
pub struct Typography {
    pub size: f32,
    pub line: f32,
    /// The widest the text column grows.
    pub measure: f32,
    /// The text column's width on this page: the measure, or less when the
    /// pane is narrower.
    pub room: f32,
    /// The page's side margins.
    pub margin: f32,
    pub family: String,
    pub code_family: String,
}

impl Typography {
    fn code_size(&self) -> f32 {
        (self.size * 0.82).round().max(10.)
    }
    fn code_line(&self) -> f32 {
        self.code_size() * 1.55
    }
    fn heading_size(&self, level: usize) -> f32 {
        (self.size * HEADING_SCALE[level - 1]).round()
    }
    fn gap(&self) -> f32 {
        self.size
    }
    /// The list marker column; list text starts this far in plus the gap.
    fn gutter(&self) -> f32 {
        (self.size * 1.4).round()
    }
}

/// Find matches to highlight while rendering.
pub struct Highlight<'a> {
    pub needle: &'a [char],
    pub current: Option<Hit>,
    pub mark: &'a str,
    pub mark_current: &'a str,
    /// The row being rendered and the matches already shown in it.
    row: Cell<usize>,
    seen: Cell<usize>,
}

impl<'a> Highlight<'a> {
    pub fn new(
        needle: &'a [char],
        current: Option<Hit>,
        mark: &'a str,
        mark_current: &'a str,
    ) -> Self {
        Self {
            needle,
            current,
            mark,
            mark_current,
            row: Cell::new(0),
            seen: Cell::new(0),
        }
    }

    /// Matches in the next text of the row, as (byte range, is current).
    fn take(&self, text: &str) -> Vec<(Range<usize>, bool)> {
        let found = crate::find::matches(text, self.needle);
        let first = self.seen.get();
        self.seen.set(first + found.len());
        let row = self.row.get();
        found
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                let current = self.current
                    == Some(Hit {
                        row,
                        nth: first + i,
                    });
                (r, current)
            })
            .collect()
    }
}

pub struct Ctx<'a> {
    pub images: &'a HashMap<String, Image>,
    /// Dark theme: picks syntax colours.
    pub dark: bool,
    /// A source file rather than prose: code shows line numbers and does
    /// not wrap.
    pub code_view: bool,
    /// The code block whose Copy button was pressed last.
    pub copied: Option<&'a str>,
    pub typo: Typography,
    pub find: Option<Highlight<'a>>,
    /// The row whose context menu is open, and where.
    pub menu: Option<(usize, [f32; 2])>,
    /// Interactive controls such as Copy buttons; off for printed pages.
    pub controls: bool,
}

/// One row of the virtualised document.
pub enum Unit<'a> {
    Block(&'a BlockIr),
    /// One item of a top-level list.
    Item {
        list: &'a BlockIr,
        index: usize,
    },
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
        /// The 1-based number of the run's first line.
        first_line: usize,
    },
}

impl Unit<'_> {
    /// The heading block, if this unit is one.
    pub fn heading(&self) -> Option<&BlockIr> {
        match self {
            Unit::Block(b) if b.kind == "heading" => Some(b),
            _ => None,
        }
    }
    /// The whole code block, if this unit is part of one.
    pub fn code(&self) -> Option<&BlockIr> {
        match self {
            Unit::Code { block, .. } => Some(block),
            Unit::Block(b) if is_mermaid(b) || b.kind == "math" => Some(b),
            _ => None,
        }
    }
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
            "code" if is_mermaid(block) => out.push(Unit::Block(block)),
            "math" => out.push(Unit::Block(block)),
            "code" => {
                let parts = chunks(code_source(block));
                let count = parts.len();
                let mut first_line = 1;
                for (part, text) in parts.into_iter().enumerate() {
                    let lines = text.split('\n').count();
                    out.push(Unit::Code {
                        block,
                        text,
                        part,
                        last: part + 1 == count,
                        first_line,
                    });
                    first_line += lines;
                }
            }
            _ => out.push(Unit::Block(block)),
        }
    }
    out
}

/// Display math as readable Unicode.
fn math_text(block: &BlockIr) -> String {
    to_unicode(block.raw.as_deref().unwrap_or(""))
}

/// A fenced Mermaid block, drawn as a diagram.
fn is_mermaid(block: &BlockIr) -> bool {
    block.kind == "code"
        && block
            .language
            .as_deref()
            .is_some_and(|l| l.eq_ignore_ascii_case("mermaid"))
}

/// The scale a Mermaid block is drawn at on this page.
fn diagram_scale(d: &Diagram, t: &Typography) -> f32 {
    diagram::scale(d, t.size, t.room - 2. * DIAGRAM_PAD - 2.)
}

fn code_source(block: &BlockIr) -> &str {
    if block.kind == "math" {
        block.raw.as_deref().unwrap_or("")
    } else {
        block.code.as_deref().unwrap_or("")
    }
}

/// The byte range of the source a unit renders, when the parser gave one.
pub fn source_range(unit: &Unit) -> Option<Range<usize>> {
    let range = match unit {
        Unit::Block(b) | Unit::Table { table: b, .. } | Unit::Code { block: b, .. } => {
            b.range.as_ref()
        }
        Unit::Item { list, index } => list.items[*index].range.as_ref(),
    }?;
    Some(range.start.byte_offset..range.end.byte_offset)
}

/// Vertical space above unit `i`.
pub fn space_above(units: &[Unit], i: usize, t: &Typography) -> f32 {
    if i == 0 {
        return 0.;
    }
    match &units[i] {
        Unit::Item { index, .. } if *index > 0 => (t.size * 0.4).round(),
        Unit::Table { first: false, .. } | Unit::Code { part: 1.., .. } => 0.,
        Unit::Block(b) if b.kind == "heading" => {
            (t.size * HEADING_GAP[usize::from(b.level.unwrap_or(1).clamp(1, 6)) - 1]).round()
        }
        _ => t.gap(),
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

/// Context menu actions for a document row.
pub mod action {
    pub const COPY_TEXT: u8 = 0;
    pub const COPY_LINK: u8 = 1;
    pub const COPY_CODE: u8 = 2;
    pub const FIND: u8 = 3;
    pub const RELOAD: u8 = 5;
    pub const REVEAL: u8 = 6;
}

fn row_menu(unit: &Unit) -> Vec<MenuItem<u8>> {
    let mut items = Vec::new();
    if unit.code().is_some() {
        items.push(menu_item(action::COPY_CODE, "Copy Code").icon(Icon::Copy));
    } else {
        items.push(menu_item(action::COPY_TEXT, "Copy Text").icon(Icon::Copy));
    }
    if unit.heading().is_some() {
        items.push(menu_item(action::COPY_LINK, "Copy Link to Section").icon(Icon::Link));
    }
    items.extend([
        menu_separator(),
        menu_item(action::FIND, "Find…")
            .shortcut("⌘F")
            .icon(Icon::Search),
        menu_separator(),
        menu_item(action::RELOAD, "Reload")
            .shortcut("⌘R")
            .icon(Icon::Refresh),
        menu_item(action::REVEAL, "Reveal in Finder").icon(Icon::Folder),
    ]);
    items
}

/// Row `i` of the document: the unit, centred on the reading measure, with
/// its spacing, the page margins and its context menu.
pub fn row(units: &[Unit], i: usize, ctx: &Ctx) -> Element<App> {
    if let Some(find) = &ctx.find {
        find.row.set(i);
        find.seen.set(0);
    }
    let t = &ctx.typo;
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
            first_line,
        } => code_part(block, text, *part, *last, *first_line, ctx),
    };
    let mut top = space_above(units, i, t);
    let mut bottom = 0.;
    if i == 0 {
        top += MARGIN_TOP;
    }
    if i + 1 == units.len() {
        bottom += MARGIN_BOTTOM;
    }
    let page = vstack![vstack![content].width_fraction(1.).max_width(t.measure)]
        .key(format!("{}-page", unit_key(units, i)))
        .align(Align::Center)
        .padding(t.margin, Edge::Horizontal)
        .padding(top, Edge::Top)
        .padding(bottom, Edge::Bottom);
    let open = ctx.menu.filter(|(row, _)| *row == i);
    context_menu(page, row_menu(&units[i]))
        .key(unit_key(units, i))
        .open(open.is_some())
        .position(open.map_or([0.; 2], |(_, at)| at))
        .on_context_open(move |app: &mut App, at| app.open_menu(crate::app::MenuTarget::Row(i), at))
        .on_open_change(|app: &mut App, open| {
            if !open {
                app.menu = None;
            }
        })
        .on_action(move |app: &mut App, id| app.row_action(i, id))
        .into()
}

/// An estimate of row `i`'s height, refined once the row is measured.
pub fn estimate(units: &[Unit], i: usize, images: &HashMap<String, Image>, t: &Typography) -> f32 {
    let text = |chars: usize, size: f32, line: f32| {
        let per_line = (t.room / (size * 0.5)).max(10.);
        (chars as f32 / per_line).ceil().max(1.) * size * line
    };
    let body = match &units[i] {
        Unit::Block(b) => block_estimate(b, images, &text, t),
        Unit::Item { list, index } => list.items[*index]
            .children
            .iter()
            .map(|c| block_estimate(c, images, &text, t))
            .sum(),
        Unit::Table { rows, first, .. } => {
            (rows.len() + usize::from(*first)) as f32 * (t.size * 0.875 * 1.5 + 19.)
        }
        Unit::Code { text, part, .. } => {
            text.lines().count().max(1) as f32 * t.code_line()
                + 28.
                + if *part == 0 { CODE_HEADER } else { 0. }
        }
    };
    let mut extra = space_above(units, i, t);
    if i == 0 {
        extra += MARGIN_TOP;
    }
    if i + 1 == units.len() {
        extra += MARGIN_BOTTOM;
    }
    (body + extra).max(1.)
}

fn block_estimate(
    b: &BlockIr,
    images: &HashMap<String, Image>,
    text: &dyn Fn(usize, f32, f32) -> f32,
    t: &Typography,
) -> f32 {
    match b.kind.as_str() {
        "heading" => {
            let level = usize::from(b.level.unwrap_or(1).clamp(1, 6));
            text(plain_len(&b.inlines), t.heading_size(level), 1.25)
        }
        "paragraph" => match lone_image(&b.inlines).and_then(|i| images.get(i.url.as_deref()?)) {
            Some(img) => {
                let (w, h) = (img.width() as f32, img.height() as f32);
                let width = image_width(w, t.room);
                width * h / w.max(1.) + 40.
            }
            None => text(plain_len(&b.inlines), t.size, t.line),
        },
        "blockquote" | "footnote_definition" => b
            .children
            .iter()
            .map(|c| block_estimate(c, images, text, t) + 10.)
            .sum(),
        "list" => b
            .items
            .iter()
            .map(|item| {
                item.children
                    .iter()
                    .map(|c| block_estimate(c, images, text, t))
                    .sum::<f32>()
                    + 6.
            })
            .sum(),
        "thematic_break" => 30.,
        "math" => text(math_text(b).chars().count(), (t.size * 1.15).round(), 1.5) + 8.,
        "code" if is_mermaid(b) => {
            let d = diagram::diagram(code_source(b));
            let body = match diagram::size(&d) {
                Some([_, h]) => h * diagram_scale(&d, t),
                None => code_source(b).lines().count().max(1) as f32 * t.code_line(),
            };
            body + CODE_HEADER + 2. * DIAGRAM_PAD
        }
        _ => text(b.raw.as_deref().map_or(40, str::len), 12., 1.4),
    }
}

fn plain_len(inlines: &[InlineIr]) -> usize {
    inlines
        .iter()
        .map(|i| i.text.as_deref().map_or(0, str::len) + plain_len(&i.children))
        .sum()
}

/// The texts unit `unit` shows, in the order it renders them.
pub fn unit_texts(unit: &Unit) -> Vec<String> {
    let mut out = Vec::new();
    match unit {
        Unit::Block(b) => block_texts(b, &mut out),
        Unit::Item { list, index } => {
            for child in &list.items[*index].children {
                block_texts(child, &mut out);
            }
        }
        Unit::Table {
            table, rows, first, ..
        } => table_texts(table, rows.clone(), *first, &mut out),
        Unit::Code { text, .. } => out.push(text.clone()),
    }
    out
}

fn block_texts(b: &BlockIr, out: &mut Vec<String>) {
    match b.kind.as_str() {
        "heading" => out.push(inline_text(&b.inlines)),
        "paragraph" => {
            if lone_image(&b.inlines).is_none() {
                out.push(inline_text(&b.inlines));
            }
        }
        "blockquote" | "footnote_definition" => b.children.iter().for_each(|c| block_texts(c, out)),
        "list" => b
            .items
            .iter()
            .flat_map(|i| &i.children)
            .for_each(|c| block_texts(c, out)),
        // Drawn diagrams have no searchable text; ones that fail show code.
        "code" if is_mermaid(b) => {
            if let Diagram::Error(_) = *diagram::diagram(code_source(b)) {
                out.extend(chunks(code_source(b)));
            }
        }
        "math" => out.push(math_text(b)),
        "code" => out.extend(chunks(code_source(b))),
        "table" => table_texts(b, 0..b.rows.len(), true, out),
        "html" => out.push(clip(b.raw.as_deref().unwrap_or("").trim(), TEXT_LIMIT).to_owned()),
        _ => {}
    }
}

fn table_texts(table: &BlockIr, rows: Range<usize>, first: bool, out: &mut Vec<String>) {
    let columns = table_columns(table);
    let mut row = |cells: &[Vec<InlineIr>]| {
        for i in 0..columns {
            out.push(inline_text(cells.get(i).map(Vec::as_slice).unwrap_or(&[])));
        }
    };
    if first {
        row(&table.headers);
    }
    for r in rows {
        row(&table.rows[r]);
    }
}

/// A paragraph's text exactly as the renderer shows it.
fn inline_text(inlines: &[InlineIr]) -> String {
    pieces(inlines).into_iter().map(|(text, _)| text).collect()
}

fn block_element(block: &BlockIr, ctx: &Ctx, depth: usize) -> Element<App> {
    let key = key_of(&block.id);
    let t = &ctx.typo;
    match block.kind.as_str() {
        "heading" => heading(block, key, ctx),
        "paragraph" => {
            if let Some(image) = lone_image(&block.inlines) {
                return image_block(image, key, ctx);
            }
            paragraph(&block.inlines, t.size, ctx).key(key).into()
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
                    .background(ColorToken::Primary.alpha(50))
                    .rounded(2.),
                vstack(children).gap(t.gap() * 0.6, Axis::Vertical).flex(1.),
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
                .gap((t.size * 0.4).round(), Axis::Vertical)
                .into()
        }
        "code" if is_mermaid(block) => diagram_card(block, ctx),
        "math" => {
            let shown = math_text(block);
            let size = (t.size * 1.15).round();
            vstack![
                marked(&shown, vec![(shown.len(), None)], ctx)
                    .key(format!("{key}-math"))
                    .font_size(size)
                    .line_height(1.5)
                    .italic(true)
                    .text_align(TextAlign::Center)
            ]
            .key(key)
            .align(Align::Center)
            .padding(4., Edge::Vertical)
            .into()
        }
        // Nested code (inside lists and quotes) renders whole.
        "code" => code_whole(block, ctx),
        "table" => table_part(block, 0..block.rows.len(), true, true, ctx),
        "footnote_definition" => {
            let children: Vec<Element<App>> = block
                .children
                .iter()
                .map(|b| block_element(b, ctx, depth + 1))
                .collect();
            hstack![
                text(footnote_mark(block.label.as_deref().unwrap_or("")))
                    .color(ColorToken::Primary)
                    .selectable(false)
                    .width(t.gutter())
                    .flex(0.),
                vstack(children).gap(t.gap() * 0.4, Axis::Vertical).flex(1.),
            ]
            .key(key)
            .align(Align::Start)
            .color(ColorToken::Muted)
            .into()
        }
        "thematic_break" => vstack![divider().background(ColorToken::Border)]
            .key(key)
            .padding(t.size * 0.9, Edge::Vertical)
            .into(),
        "html" => {
            let raw = clip(block.raw.as_deref().unwrap_or("").trim(), TEXT_LIMIT);
            marked(raw, vec![(raw.len(), None)], ctx)
                .key(key)
                .font_family(t.code_family.clone())
                .font_size(t.code_size())
                .color(ColorToken::Muted)
                .into()
        }
        _ => vstack![].key(key).into(),
    }
}

fn heading(block: &BlockIr, key: String, ctx: &Ctx) -> Element<App> {
    let level = usize::from(block.level.unwrap_or(1).clamp(1, 6));
    let t = &ctx.typo;
    let system = t.family == "SF";
    paragraph(&block.inlines, t.heading_size(level), ctx)
        .key(key)
        .font_weight(if level <= 2 { 700 } else { 600 })
        .letter_spacing(if system && level <= 2 { -0.012 } else { 0. })
        .line_height(1.25)
        .color(if level >= 5 {
            ColorToken::Muted
        } else {
            ColorToken::BaseContent
        })
        .into()
}

fn list_item(list: &BlockIr, index: usize, ctx: &Ctx, depth: usize) -> Element<App> {
    let t = &ctx.typo;
    let ordered = list.ordered.unwrap_or(false);
    let item = &list.items[index];
    // Bullets, numbers and checkboxes share one right-aligned gutter so
    // every kind of list indents its text by the same amount.
    let marker: Element<App> = match item.checked {
        Some(done) => vstack![task_box(done)]
            .padding(((t.size * t.line - 16.) / 2.).max(0.), Edge::Top)
            .into(),
        None if ordered => text(format!("{}.", list.start.unwrap_or(1) as usize + index))
            .color(ColorToken::Muted)
            .font_size(t.size)
            .line_height(t.line)
            .selectable(false)
            .into(),
        None => text(if depth.is_multiple_of(2) {
            "•"
        } else {
            "◦"
        })
        .color(ColorToken::Muted)
        .font_size(t.size)
        .line_height(t.line)
        .selectable(false)
        .into(),
    };
    let body: Vec<Element<App>> = item
        .children
        .iter()
        .map(|b| block_element(b, ctx, depth + 1))
        .collect();
    hstack![
        vstack![marker].width(t.gutter()).align(Align::End).flex(0.),
        vstack(body)
            .gap((t.size * 0.4).round(), Axis::Vertical)
            .flex(1.),
    ]
    .key(key_of(&item.id))
    .gap(10., Axis::Horizontal)
    .into()
}

fn task_box(done: bool) -> Element<App> {
    let mut b = vstack(if done {
        vec![
            icon(Icon::Check)
                .size(11.)
                .color(ColorToken::PrimaryContent)
                .into(),
        ]
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
        b = b
            .background(ColorToken::Primary)
            .border_color(ColorToken::Primary);
    }
    b.into()
}

/// One part of a code block. The first part carries the language and a
/// Copy button for the whole block; parts join into one rounded frame.
/// A whole code block in one element.
fn code_whole(block: &BlockIr, ctx: &Ctx) -> Element<App> {
    let parts = chunks(code_source(block));
    let count = parts.len();
    let mut first_line = 1;
    let parts: Vec<Element<App>> = parts
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let part = code_part(block, text, i, i + 1 == count, first_line, ctx);
            first_line += text.split('\n').count();
            part
        })
        .collect();
    vstack(parts).key(key_of(&block.id)).into()
}

/// A Mermaid block as a diagram in a code-style card, or as code with a
/// note when it cannot be drawn.
fn diagram_card(block: &BlockIr, ctx: &Ctx) -> Element<App> {
    let key = key_of(&block.id);
    let t = &ctx.typo;
    let d = diagram::diagram(code_source(block));
    if let Diagram::Error(e) = &*d {
        return vstack![
            code_whole(block, ctx),
            text(format!("This Mermaid diagram could not be drawn: {e}"))
                .font_size(12.)
                .color(ColorToken::Muted)
                .selectable(false),
        ]
        .key(format!("{key}-diagram"))
        .gap(6., Axis::Vertical)
        .into();
    }
    let s = diagram_scale(&d, t);
    let mut drawing = hstack![diagram::view(&d, s, &t.code_family, t.code_size())]
        .key(format!("{key}-drawing"))
        .justify(Justify::Center)
        .padding(DIAGRAM_PAD, Edge::All)
        .clip_contents(true);
    // Shrunk as far as it reads well and still wider than the page: scroll.
    if diagram::size(&d).is_some_and(|[w, _]| w * s > t.room - 2. * DIAGRAM_PAD - 2.) {
        drawing = drawing.justify(Justify::Start).scroll(Scroll::Horizontal);
    }
    vstack![code_header(block, ctx), drawing]
        .key(key)
        .background(ColorToken::Base200)
        .border_color(ColorToken::Border)
        .border_width(1.)
        .rounded(10.)
        .clip_contents(true)
        .into()
}

/// A code block's header: its language and a Copy button.
fn code_header(block: &BlockIr, ctx: &Ctx) -> Element<App> {
    let key = key_of(&block.id);
    let copied = ctx.copied == Some(key.as_str());
    let id = key.clone();
    let source = code_source(block).to_owned();
    let language = if block.kind == "math" {
        "math"
    } else {
        block.language.as_deref().unwrap_or("text")
    };
    let mut parts: Vec<Element<App>> = vec![
        text(language.to_uppercase())
            .font_size(10.5)
            .font_weight(600)
            .letter_spacing(0.06)
            .color(ColorToken::Muted)
            .selectable(false)
            .into(),
        spacer().into(),
    ];
    if ctx.controls {
        parts.push(
            button(if copied { "Copied" } else { "Copy" })
                .key(format!("{key}-copy"))
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Small)
                .font_size(12.)
                .color(if copied {
                    ColorToken::Success
                } else {
                    ColorToken::Muted
                })
                .on_click(move |app: &mut App| app.copy(&source, Some(id.clone())))
                .into(),
        );
    }
    hstack(parts)
        .height(CODE_HEADER)
        .padding(14., Edge::Left)
        .padding(6., Edge::Right)
        .align(Align::Center)
        .border_sides(Sides::BOTTOM)
        .border_color(ColorToken::Border)
        .border_width(1.)
        .into()
}

fn code_part(
    block: &BlockIr,
    code: &str,
    part: usize,
    last: bool,
    first_line: usize,
    ctx: &Ctx,
) -> Element<App> {
    let key = key_of(&block.id);
    let t = &ctx.typo;
    let first = part == 0;
    let mut parts: Vec<Element<App>> = Vec::new();
    if first {
        parts.push(code_header(block, ctx));
    }
    let code = if code.is_empty() { " " } else { code };
    let language = block
        .language
        .as_deref()
        .unwrap_or(if block.kind == "math" { "latex" } else { "" });
    let body = styled(
        code,
        syntax_runs(language, code, ctx.dark),
        ctx,
        &t.code_family,
    )
    .key(format!("{key}-text-{part}"))
    .font_family(t.code_family.clone())
    .font_size(t.code_size())
    .line_height(1.55)
    .no_wrap(ctx.code_view);
    let content: Element<App> = if ctx.code_view {
        // Line numbers in a gutter; code does not wrap, so rows line up.
        let lines = code.split('\n').count();
        let numbers: Vec<String> = (first_line..first_line + lines)
            .map(|n| n.to_string())
            .collect();
        hstack![
            text(numbers.join("\n"))
                .font_family(t.code_family.clone())
                .font_size(t.code_size())
                .line_height(1.55)
                .no_wrap(true)
                .text_align(TextAlign::End)
                .color(ColorToken::Muted.alpha(70))
                .selectable(false)
                .width(gutter_width(first_line + lines, t.code_size()))
                .flex(0.),
            vstack![body].flex(1.).clip_contents(true),
        ]
        .gap(16., Axis::Horizontal)
        .into()
    } else {
        body.into()
    };
    parts.push(
        vstack![content]
            .key(format!("{key}-code-{part}"))
            .padding(16., Edge::Horizontal)
            .padding(if first { 12. } else { 0. }, Edge::Top)
            .padding(if last { 14. } else { 0. }, Edge::Bottom)
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

/// Width of a line-number gutter that fits `last`.
fn gutter_width(last: usize, size: f32) -> f32 {
    (last.max(10).ilog10() + 1) as f32 * size * 0.62 + 4.
}

/// Syntax colours as style runs: (end offset, style) covering `code`.
fn syntax_runs(language: &str, code: &str, dark: bool) -> Vec<(usize, Option<Style<'static>>)> {
    // Room is left for find matches under the 1024-span limit.
    let tokens = highlight::tokens(language, code, 600);
    let mut runs = Vec::with_capacity(tokens.len() * 2 + 1);
    let mut at = 0;
    for (range, token) in tokens {
        if range.start < at || range.end > code.len() {
            continue;
        }
        if range.start > at {
            runs.push((range.start, None));
        }
        runs.push((
            range.end,
            Some(Style {
                color: Some(highlight::color(token, dark)),
                italic: token == Token::Comment,
                ..Style::default()
            }),
        ));
        at = range.end;
    }
    runs.push((code.len(), None));
    runs
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

fn table_columns(table: &BlockIr) -> usize {
    table
        .rows
        .iter()
        .map(Vec::len)
        .chain([table.headers.len()])
        .max()
        .unwrap_or(0)
        .max(1)
}

/// Table rows `rows` (with the header on the first part); parts join into
/// one rounded frame and share column widths.
fn table_part(
    table: &BlockIr,
    rows: Range<usize>,
    first: bool,
    last: bool,
    ctx: &Ctx,
) -> Element<App> {
    let columns = table_columns(table);
    let size = (ctx.typo.size * 0.875).round();
    let total = table.rows.len();
    let mut cells: Vec<Element<App>> = Vec::new();
    let table_key = key_of(&table.id);
    let mut row = |cols: &[Vec<InlineIr>], header: bool, rule: bool, r: usize| {
        for i in 0..columns {
            let inlines = cols.get(i).map(Vec::as_slice).unwrap_or(&[]);
            let (align, text_align) = match table.align.get(i).map(String::as_str) {
                Some("right") => (Align::End, TextAlign::End),
                Some("center") => (Align::Center, TextAlign::Center),
                _ => (Align::Start, TextAlign::Start),
            };
            let mut text = paragraph(inlines, size, ctx)
                .key(format!("{table_key}-cell-{r}-{i}"))
                .line_height(1.5)
                .text_align(text_align);
            if header {
                text = text.font_weight(600);
            }
            let mut cell = vstack![text]
                .align(align)
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
        row(&table.headers, true, total > 0, 0);
    }
    for r in rows.clone() {
        row(&table.rows[r], false, r + 1 < total, r + 1);
    }
    let sides = match (first, last) {
        (true, true) => Sides::ALL,
        (true, false) => Sides::TOP | Sides::LEFT | Sides::RIGHT,
        (false, true) => Sides::BOTTOM | Sides::LEFT | Sides::RIGHT,
        (false, false) => Sides::LEFT | Sides::RIGHT,
    };
    let table = grid(cells)
        .columns(columns as u16)
        .key(format!("{}-rows-{}", key_of(&table.id), rows.start))
        .border_sides(sides)
        .border_color(ColorToken::Border)
        .border_width(1.)
        .rounded_on(10., corners(first, last))
        .clip_contents(true);
    // Too many columns for the page: keep words whole and scroll sideways.
    let least = columns as f32 * MIN_COLUMN;
    if least > ctx.typo.room {
        return hstack![table.width(least).flex(0.)]
            .key(format!("{table_key}-scroll-{}", rows.start))
            .scroll(Scroll::Horizontal)
            .into();
    }
    table.into()
}

/// Retina screenshots are usually 2x: show them at their point size.
fn image_width(w: f32, measure: f32) -> f32 {
    (w / 2.).max(w.min(320.)).min(measure)
}

fn image_block(image: &InlineIr, key: String, ctx: &Ctx) -> Element<App> {
    let url = image.url.as_deref().unwrap_or("");
    let alt = image.alt.as_deref().unwrap_or("");
    match ctx.images.get(url) {
        Some(img) => {
            let (w, h) = (img.width() as f32, img.height() as f32);
            let width = image_width(w, ctx.typo.room);
            let mut parts: Vec<Element<App>> = vec![
                vstack![]
                    .width(width)
                    .height(width * h / w.max(1.))
                    .background_image(img.clone().background().fit(ImageFit::Contain))
                    .rounded(10.)
                    .clip_contents(true)
                    .border_color(ColorToken::Border)
                    .border_width(1.)
                    .accessible_label(if alt.is_empty() { "Image" } else { alt })
                    .into(),
            ];
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
        None => text(format!(
            "[image: {}]",
            if alt.is_empty() { url } else { alt }
        ))
        .key(key)
        .font_size(13.)
        .color(ColorToken::Muted)
        .into(),
    }
}

/// A paragraph's inlines as one rich text, cut at a text element's 8 KiB,
/// with find matches highlighted.
fn paragraph(inlines: &[InlineIr], size: f32, ctx: &Ctx) -> volt_ui_kit::ui::Text<App> {
    let out = pieces(inlines);
    let has_links = out.iter().any(|(_, s)| s.link.is_some());
    let text: String = out.iter().map(|(t, _)| t.as_str()).collect();
    let mut runs = Vec::with_capacity(out.len());
    let mut at = 0;
    for (piece, style) in &out {
        at += piece.len();
        runs.push((at, Some(*style)));
    }
    let paragraph = styled(&text, runs, ctx, &ctx.typo.code_family)
        .font_size(size)
        .line_height(ctx.typo.line);
    if has_links {
        paragraph.on_link(App::follow_link)
    } else {
        paragraph
    }
}

/// Plain text (code, html) with find matches highlighted.
fn marked(text: &str, runs: Vec<(usize, Option<Style>)>, ctx: &Ctx) -> volt_ui_kit::ui::Text<App> {
    styled(text, runs, ctx, &ctx.typo.code_family)
}

/// `text` split into spans: `runs` gives each run's end offset and style
/// (`None` for plain text), and find matches cut runs further. The text is
/// cut at the 8 KiB element limit.
fn styled(
    text: &str,
    runs: Vec<(usize, Option<Style>)>,
    ctx: &Ctx,
    code_family: &str,
) -> volt_ui_kit::ui::Text<App> {
    let marks = ctx.find.as_ref().map_or_else(Vec::new, |f| f.take(text));
    let limit = clip(text, TEXT_LIMIT).len();
    // Every offset where a span starts or ends.
    let mut cuts: Vec<usize> = runs.iter().map(|(end, _)| *end).collect();
    for (r, _) in &marks {
        cuts.extend([r.start, r.end]);
    }
    cuts.push(limit);
    cuts.retain(|c| *c <= limit);
    cuts.sort_unstable();
    cuts.dedup();
    let mut spans = Vec::with_capacity(cuts.len());
    let mut start = 0;
    for end in cuts {
        if end <= start {
            continue;
        }
        let style = runs
            .iter()
            .find(|(run_end, _)| *run_end > start)
            .and_then(|(_, s)| *s)
            .unwrap_or_default();
        let mark = marks
            .iter()
            .find(|(r, _)| r.start <= start && start < r.end)
            .map(|(_, current)| *current);
        let mut s = style.span(text[start..end].to_owned(), code_family);
        if let Some(current) = mark {
            s = s.background(if current {
                ctx.find.as_ref().unwrap().mark_current
            } else {
                ctx.find.as_ref().unwrap().mark
            });
        }
        spans.push(s);
        start = end;
    }
    if limit < text.len() {
        spans.push(span("…"));
    }
    rich_text(spans)
}

#[derive(Clone, Copy, Default)]
struct Style<'a> {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: Option<&'a str>,
    color: Option<&'static str>,
}
impl Style<'_> {
    fn span(self, text: String, code_family: &str) -> Span {
        let mut s = span(text);
        if self.code {
            s = s.code().family(code_family);
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
        if let Some(color) = self.color {
            s = s.color(color);
        }
        s
    }
}

/// Inline content flattened into styled pieces of text.
fn pieces(inlines: &[InlineIr]) -> Vec<(String, Style<'_>)> {
    let mut out = Vec::new();
    collect(inlines, Style::default(), &mut out);
    out.retain(|(t, _)| !t.is_empty());
    out
}

fn collect<'a>(inlines: &'a [InlineIr], style: Style<'a>, out: &mut Vec<(String, Style<'a>)>) {
    for inline in inlines {
        let text = inline.text.as_deref().unwrap_or("");
        match inline.kind.as_str() {
            // Soft line breaks inside a paragraph are spaces.
            "text" => out.push((expand_emoji_text(&text.replace('\n', " ")), style)),
            "strong" => collect(
                &inline.children,
                Style {
                    bold: true,
                    ..style
                },
                out,
            ),
            "emphasis" => collect(
                &inline.children,
                Style {
                    italic: true,
                    ..style
                },
                out,
            ),
            "delete" => collect(
                &inline.children,
                Style {
                    strike: true,
                    ..style
                },
                out,
            ),
            "link" => collect(
                &inline.children,
                Style {
                    link: inline.url.as_deref(),
                    ..style
                },
                out,
            ),
            // Narrow no-break spaces pad the code background on both sides.
            "math" => out.push((
                to_unicode(text),
                Style {
                    italic: true,
                    ..style
                },
            )),
            "code" => out.push((
                format!("\u{202F}{text}\u{202F}"),
                Style {
                    code: true,
                    ..style
                },
            )),
            "hard_break" => out.push(("\n".into(), style)),
            "footnote_reference" => out.push((
                footnote_mark(text),
                Style {
                    color: Some("@primary"),
                    ..style
                },
            )),
            "image" => out.push((
                format!("[{}]", inline.alt.as_deref().unwrap_or("image")),
                style,
            )),
            "html" => {
                if text.eq_ignore_ascii_case("<br>") || text.eq_ignore_ascii_case("<br/>") {
                    out.push(("\n".into(), style));
                }
            }
            _ => collect(&inline.children, style, out),
        }
    }
}

/// A paragraph that is just one image (ignoring whitespace).
/// A footnote's mark: superscript digits for numeric labels, else [label].
fn footnote_mark(label: &str) -> String {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    if !label.is_empty() && label.bytes().all(|b| b.is_ascii_digit()) {
        label
            .bytes()
            .map(|b| DIGITS[usize::from(b - b'0')])
            .collect()
    } else {
        format!("[{label}]")
    }
}

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
                i.text
                    .as_deref()
                    .map_or(0, |t| t.split_whitespace().count())
                    + inlines(&i.children)
            })
            .sum()
    }
    blocks
        .iter()
        .map(|b| {
            inlines(&b.inlines)
                + b.code
                    .as_deref()
                    .map_or(0, |c| c.split_whitespace().count())
                + word_count(&b.children)
                + b.items
                    .iter()
                    .map(|i| word_count(&i.children))
                    .sum::<usize>()
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
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(110)
        .collect();
    if key.is_empty() { "block".into() } else { key }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdstar_core::document_ir::parse_document_ir_with_diagnostics;

    /// Find counts matches with `unit_texts`; the renderer must show the same
    /// texts, or highlights drift from the count.
    #[test]
    fn find_counts_the_text_rows_render() {
        let source = "# Find me\n\nA *find* paragraph with `find` code and [find](#x).\n\n- find one\n- two find\n\n| find | b |\n|---|---|\n| c | FIND |\n\n```\nfind\nfind\n```\n";
        let doc = parse_document_ir_with_diagnostics(source, "t.md").unwrap();
        let units = units(&doc.blocks);
        let needle = crate::find::needle("find");
        let counted: usize = units
            .iter()
            .map(|u| {
                unit_texts(u)
                    .iter()
                    .map(|t| crate::find::matches(t, &needle).len())
                    .sum::<usize>()
            })
            .sum();
        assert_eq!(counted, 10);
        let images = HashMap::new();
        let ctx = Ctx {
            images: &images,
            dark: false,
            code_view: false,
            copied: None,
            typo: Typography {
                size: 16.,
                line: 1.6,
                measure: 700.,
                room: 700.,
                margin: 56.,
                family: "SF".into(),
                code_family: "SF Mono".into(),
            },
            find: Some(Highlight::new(&needle, None, "#FF0", "#F80")),
            menu: None,
            controls: true,
        };
        let mut shown = 0;
        for i in 0..units.len() {
            let _ = row(&units, i, &ctx);
            shown += ctx.find.as_ref().unwrap().seen.get();
        }
        assert_eq!(shown, counted);
    }
}
