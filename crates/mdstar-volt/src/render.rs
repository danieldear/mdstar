//! Markdown document IR to Volt elements.

use std::collections::HashMap;

use mdstar_core::document_ir::{BlockIr, InlineIr};
use volt_ui_kit::{graphics::Image, ui::prelude::*};

use crate::App;

/// Volt validates trees up to 4096 elements; stop well before that and say so.
const ELEMENT_BUDGET: usize = 3600;
/// Each text element holds at most 8 KiB.
const TEXT_LIMIT: usize = 8000;
pub const BODY: f32 = 16.;
const LINE: f32 = 1.7;
pub const MEASURE: f32 = 680.;
const CODE_SIZE: f32 = 13.;
const CODE_LINE: f32 = CODE_SIZE * 1.2;

pub struct Ctx<'a> {
    pub images: &'a HashMap<String, Image>,
    /// The code block whose Copy button was pressed last.
    pub copied: Option<&'a str>,
    elements: usize,
    pub truncated: bool,
}
impl<'a> Ctx<'a> {
    pub fn new(images: &'a HashMap<String, Image>, copied: Option<&'a str>) -> Self {
        Self {
            images,
            copied,
            elements: 0,
            truncated: false,
        }
    }
    pub fn elements(&self) -> usize {
        self.elements
    }
    fn count(&mut self, n: usize) {
        self.elements += n;
    }
}

/// Render top-level blocks, stopping early (and flagging it) for documents
/// larger than the element budget.
pub fn blocks(blocks: &[BlockIr], ctx: &mut Ctx) -> Vec<Element<App>> {
    let mut out = Vec::with_capacity(blocks.len());
    for (i, block) in blocks.iter().enumerate() {
        if ctx.elements > ELEMENT_BUDGET {
            ctx.truncated = true;
            break;
        }
        if let Some(element) = block_element(block, ctx, 0, i == 0) {
            out.push(element);
        }
    }
    out
}

fn block_element(
    block: &BlockIr,
    ctx: &mut Ctx,
    depth: usize,
    first: bool,
) -> Option<Element<App>> {
    ctx.count(1);
    let key = key_of(&block.id);
    Some(match block.kind.as_str() {
        "heading" => heading(block, key, ctx, first),
        "paragraph" => {
            if let Some(image) = lone_image(&block.inlines) {
                return Some(image_block(image, key, ctx));
            }
            paragraph(&block.inlines, BODY, ctx).key(key).into()
        }
        "blockquote" => {
            let children = child_blocks(&block.children, ctx, depth);
            ctx.count(2);
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
        "list" => list(block, key, ctx, depth),
        "code" => code_block(
            block.code.as_deref().unwrap_or(""),
            block.language.as_deref(),
            &key,
            ctx,
        ),
        "math" => code_block(block.raw.as_deref().unwrap_or(""), Some("math"), &key, ctx),
        "table" => table(block, key, ctx),
        "thematic_break" => vstack![divider().background(ColorToken::Border)]
            .key(key)
            .padding(14., Edge::Vertical)
            .into(),
        "html" => {
            let raw = block.raw.as_deref().unwrap_or("").trim();
            if raw.is_empty() || raw.starts_with("<!--") {
                return None;
            }
            text(clip(raw, TEXT_LIMIT))
                .key(key)
                .font_family("SF Mono")
                .font_size(12.)
                .color(ColorToken::Muted)
                .into()
        }
        // YAML/TOML front matter is metadata, not document content.
        "frontmatter" => return None,
        _ => return None,
    })
}

fn child_blocks(blocks: &[BlockIr], ctx: &mut Ctx, depth: usize) -> Vec<Element<App>> {
    blocks
        .iter()
        .filter_map(|b| block_element(b, ctx, depth + 1, false))
        .collect()
}

fn heading(block: &BlockIr, key: String, ctx: &mut Ctx, first: bool) -> Element<App> {
    let level = usize::from(block.level.unwrap_or(1).clamp(1, 6));
    let size = [32., 24., 19., 16.5, 15., 14.][level - 1];
    let top = if first {
        0.
    } else {
        [28., 30., 20., 14., 12., 12.][level - 1]
    };
    let title = paragraph(&block.inlines, size, ctx)
        .font_family("SF Pro Display")
        .font_weight(if level <= 2 { 700 } else { 600 })
        .letter_spacing(if level <= 2 { -0.012 } else { 0. })
        .line_height(1.25);
    ctx.count(1);
    vstack![title].key(key).padding(top, Edge::Top).into()
}

fn list(block: &BlockIr, key: String, ctx: &mut Ctx, depth: usize) -> Element<App> {
    let ordered = block.ordered.unwrap_or(false);
    let start = block.start.unwrap_or(1);
    let items: Vec<Element<App>> = block
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            ctx.count(3);
            let marker: Element<App> = match item.checked {
                Some(done) => vstack![task_box(done)].padding(5., Edge::Top).into(),
                None if ordered => text(format!("{}.", start as usize + i))
                    .color(ColorToken::Muted)
                    .font_size(BODY)
                    .line_height(LINE)
                    .text_align(TextAlign::End)
                    .fill_width()
                    .into(),
                None => text(if depth.is_multiple_of(2) {
                    "•"
                } else {
                    "◦"
                })
                .color(ColorToken::Muted)
                .font_size(BODY)
                .line_height(LINE)
                .into(),
            };
            let body = child_blocks(&item.children, ctx, depth);
            hstack![
                vstack![marker]
                    .width(if ordered { 24. } else { 14. })
                    .flex(0.),
                vstack(body).gap(6., Axis::Vertical).flex(1.),
            ]
            .key(key_of(&item.id))
            .gap(10., Axis::Horizontal)
            .into()
        })
        .collect();
    vstack(items)
        .key(key)
        .gap(4., Axis::Vertical)
        .padding(2., Edge::Left)
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

fn code_block(code: &str, language: Option<&str>, key: &str, ctx: &mut Ctx) -> Element<App> {
    ctx.count(6);
    let copied = ctx.copied == Some(key);
    let source = code.to_string();
    let id = key.to_string();
    let header = hstack![
        text(language.unwrap_or("text").to_uppercase())
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
            .color(if copied {
                ColorToken::Success
            } else {
                ColorToken::Muted
            })
            .on_click(move |app: &mut App| {
                if volt_ui_kit::native::clipboard::set_text(&source).is_ok() {
                    app.copied = Some(id.clone());
                }
            }),
    ]
    .height(34.)
    .padding(10., Edge::Left)
    .padding(6., Edge::Right)
    .align(Align::Center)
    .border_sides(Sides::BOTTOM)
    .border_color(ColorToken::Border)
    .border_width(1.);
    let mut parts: Vec<Element<App>> = vec![header.into()];
    // Read-only text areas: selectable, Cmd/Ctrl+C copies a selection.
    for (i, chunk) in chunks(code).into_iter().enumerate() {
        ctx.count(1);
        let lines = chunk.lines().count().max(1) as f32;
        parts.push(
            text_area(chunk)
                .key(format!("{key}-code-{i}"))
                .read_only(true)
                .on_change(|_: &mut App, _| {})
                .fill_width()
                .height(lines * CODE_LINE + 24.)
                .font_family("SF Mono")
                .font_size(CODE_SIZE)
                .background("#00000000")
                .border_width(0.)
                .rounded(0.)
                .into(),
        );
    }
    vstack(parts)
        .key(key)
        .background(ColorToken::Base200)
        .border_color(ColorToken::Border)
        .border_width(1.)
        .rounded(10.)
        .clip_contents(true)
        .into()
}

fn table(block: &BlockIr, key: String, ctx: &mut Ctx) -> Element<App> {
    let columns = block
        .rows
        .iter()
        .map(Vec::len)
        .chain([block.headers.len()])
        .max()
        .unwrap_or(0)
        .max(1);
    // One grid, so every row shares the same column widths.
    let mut cells: Vec<Element<App>> = Vec::new();
    let last = block.rows.len();
    for (r, row) in std::iter::once(&block.headers)
        .chain(&block.rows)
        .enumerate()
    {
        for i in 0..columns {
            ctx.count(2);
            let inlines = row.get(i).map(Vec::as_slice).unwrap_or(&[]);
            let mut text = paragraph(inlines, 14., ctx).line_height(1.5);
            if r == 0 {
                text = text.font_weight(600);
            }
            let mut cell = vstack![text]
                .padding(14., Edge::Horizontal)
                .padding(9., Edge::Vertical);
            if r < last {
                cell = cell
                    .border_sides(Sides::BOTTOM)
                    .border_color(ColorToken::Border)
                    .border_width(1.);
            }
            if r == 0 {
                cell = cell.background(ColorToken::Base200);
            }
            cells.push(cell.into());
        }
    }
    grid(cells)
        .columns(columns as u16)
        .key(key)
        .border_color(ColorToken::Border)
        .border_width(1.)
        .rounded(10.)
        .clip_contents(true)
        .into()
}

fn image_block(image: &InlineIr, key: String, ctx: &mut Ctx) -> Element<App> {
    let url = image.url.as_deref().unwrap_or("");
    let alt = image.alt.as_deref().unwrap_or("");
    match ctx.images.get(url) {
        Some(img) => {
            let (w, h) = (img.width() as f32, img.height() as f32);
            // Retina screenshots are usually 2x: show them at their point size.
            let width = (w / 2.).max(w.min(320.)).min(MEASURE);
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

/// A paragraph's inlines as one rich text, cut at a text element's 8 KiB.
fn paragraph(inlines: &[InlineIr], size: f32, ctx: &mut Ctx) -> volt_ui_kit::ui::Text<App> {
    let _ = ctx;
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
            "strong" => spans(
                &inline.children,
                Style {
                    bold: true,
                    ..style
                },
                out,
            ),
            "emphasis" => spans(
                &inline.children,
                Style {
                    italic: true,
                    ..style
                },
                out,
            ),
            "delete" => spans(
                &inline.children,
                Style {
                    strike: true,
                    ..style
                },
                out,
            ),
            "link" => spans(
                &inline.children,
                Style {
                    link: inline.url.as_deref(),
                    ..style
                },
                out,
            ),
            "code" | "math" => out.push((
                text.to_string(),
                Style {
                    code: true,
                    ..style
                },
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

/// Code split at line boundaries into text-element-sized chunks.
fn chunks(code: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    for line in code.split('\n') {
        let line = clip(line, TEXT_LIMIT - 1);
        let last = out.last_mut().unwrap();
        if !last.is_empty() && last.len() + line.len() + 1 > TEXT_LIMIT {
            out.push(String::new());
        }
        let last = out.last_mut().unwrap();
        if !last.is_empty() {
            last.push('\n');
        }
        last.push_str(line);
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
        .take(120)
        .collect();
    if key.is_empty() { "block".into() } else { key }
}
