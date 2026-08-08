mod enrich;

use mdansi::{
    RenderOptions as MdansiRenderOptions, Renderer as MdansiRenderer, Theme as MdansiTheme,
};
use mdstar_core::{Block, Document};
use mermaid_text::{RenderOptions as MermaidRenderOptions, render_with_options as render_mermaid};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderOptions {
    pub width: usize,
    pub color: bool,
    pub ascii_mermaid: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 100,
            color: true,
            ascii_mermaid: false,
        }
    }
}

pub fn render_markdown(source: &str, options: RenderOptions) -> String {
    let enriched = enrich::enrich_markdown(source);
    let preprocessed = preprocess_mermaid_blocks(&enriched, options);
    let mdansi_options = MdansiRenderOptions {
        width: options.width,
        highlight: options.color,
        hyperlinks: options.color,
        plain: !options.color,
        code_wrap: false,
        // Never drop table content; prefer wide rows over truncation.
        table_truncate: false,
        ..Default::default()
    };

    let renderer = MdansiRenderer::new(MdansiTheme::default(), mdansi_options);
    let rendered = renderer.render(&preprocessed);
    let transformed = transform_code_blocks(&rendered, options.width, options.color);
    fix_headings(&transformed, options.color)
}

/// Strip ANSI/VT100 escape sequences, returning plain display text.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            result.push(ch);
            continue;
        }
        match chars.peek() {
            Some(&'[') => {
                chars.next();
                // CSI sequence — skip until the final byte (an ASCII letter)
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            Some(&']') => {
                chars.next();
                // OSC sequence — skip until BEL or ESC \
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    result
}

/// Find the byte offset of the first occurrence of `needle` in `haystack`.
fn find_first_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Find the byte offset of the last occurrence of `needle` in `haystack`.
fn find_last_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .enumerate()
        .rev()
        .find(|(_, w)| *w == needle)
        .map(|(i, _)| i)
}

/// Extract the language label from a box top border line (already ANSI-stripped).
/// e.g. `╭─ rust ─────╮`  →  `Some("rust")`
///      `╭─────────────╮`  →  `None`
fn extract_border_language(stripped_top: &str) -> Option<String> {
    let inner = stripped_top.trim_start_matches('╭').trim_end_matches('╮');
    let content: String = inner
        .chars()
        .skip_while(|&c| c == '─' || c == ' ')
        .collect();
    let lang: String = content
        .chars()
        .take_while(|&c| c != '─' && c != ' ')
        .collect();
    if lang.is_empty() { None } else { Some(lang) }
}

/// After applying a background colour escape at the start of a line, any
/// `\x1b[0m` (reset) in the content will wipe out the background.  This
/// function re-inserts `bg_escape` after every reset so the background
/// remains active across syntax-highlighting colour resets.
fn reinsert_bg_after_resets(content: &str, bg_escape: &str) -> String {
    const RESET: &str = "\x1b[0m";
    let mut result = String::with_capacity(content.len() + 64);
    let mut remaining = content;
    while let Some(pos) = remaining.find(RESET) {
        result.push_str(&remaining[..pos + RESET.len()]);
        result.push_str(bg_escape);
        remaining = &remaining[pos + RESET.len()..];
    }
    result.push_str(remaining);
    result
}

/// Replace mdansi's box-drawing code block borders with a styled background block.
///
/// Color mode: a right-aligned language badge on a header bar, then content
/// lines with 2-space inner padding on a dark background that fills to `width`,
/// closed by an empty padding line.
///
/// Plain mode: a `─── lang` label and 4-space indented content.
fn transform_code_blocks(output: &str, width: usize, color: bool) -> String {
    const VERT_SPACE: &[u8] = "│ ".as_bytes();
    const SPACE_VERT: &[u8] = " │".as_bytes();

    // Two-tone: slightly lighter header bar + darker code area.
    const HEADER_BG: &str = "\x1b[48;5;238m";
    const CODE_BG: &str = "\x1b[48;5;235m";
    const LANG_FG: &str = "\x1b[38;5;244m"; // muted grey label text
    const RESET: &str = "\x1b[0m";

    let mut result = String::with_capacity(output.len() + 4096);
    let mut in_box = false;
    let mut current_lang: Option<String> = None;

    for raw_line in output.split_inclusive('\n') {
        let line = raw_line.trim_end_matches('\n');
        let stripped = strip_ansi(line);

        if !in_box {
            if stripped.starts_with('╭') && stripped.ends_with('╮') {
                in_box = true;
                current_lang = extract_border_language(&stripped);
                if color {
                    let lang_len = current_lang.as_deref().map(|l| l.len() + 1).unwrap_or(0);
                    let spaces = width.saturating_sub(lang_len);
                    result.push_str(HEADER_BG);
                    for _ in 0..spaces {
                        result.push(' ');
                    }
                    if let Some(ref l) = current_lang {
                        result.push_str(LANG_FG);
                        result.push_str(l);
                        result.push_str(RESET);
                        result.push_str(HEADER_BG);
                        result.push(' ');
                    }
                    result.push_str(RESET);
                    result.push('\n');
                } else {
                    if let Some(ref lang) = current_lang {
                        result.push_str(&format!("  ─── {lang}\n"));
                    }
                }
                continue;
            }

            // mdansi renders single-line code blocks as bare `│ content` (no box borders).
            // Strip the leading border and apply our styling.
            if stripped.starts_with("│ ") && !stripped.ends_with('╯') {
                let p = "│ ".len();
                let inner_stripped = &stripped[p..];
                if color {
                    let inner_display_width = inner_stripped.chars().count();
                    let right_fill = width.saturating_sub(inner_display_width + 4);
                    result.push_str(CODE_BG);
                    result.push_str("  ");
                    result.push_str(inner_stripped);
                    for _ in 0..right_fill {
                        result.push(' ');
                    }
                    result.push_str("  ");
                    result.push_str(RESET);
                } else {
                    result.push_str("    ");
                    result.push_str(inner_stripped);
                }
                if raw_line.ends_with('\n') {
                    result.push('\n');
                }
                continue;
            }

            result.push_str(raw_line);
            continue;
        }

        if stripped.starts_with('╰') && stripped.ends_with('╯') {
            in_box = false;
            current_lang = None;
            if color {
                // Empty bottom padding line keeps the block visually closed.
                result.push_str(CODE_BG);
                for _ in 0..width {
                    result.push(' ');
                }
                result.push_str(RESET);
                result.push('\n');
            }
            result.push('\n');
            continue;
        }

        // Match content lines with both borders intact, and also truncated
        // lines (mdansi omits the closing │ when content reaches the box edge).
        let is_content_line = (stripped.starts_with("│ ") && stripped.ends_with(" │"))
            || (stripped.starts_with("│ ") && !stripped.starts_with("│ │"));
        if is_content_line {
            // Diagram ASCII art should NOT inherit mdansi's syntax-highlight colours
            // (typically green) — strip them so the art renders in the terminal's
            // default foreground on our dark background.
            let is_diagram = matches!(current_lang.as_deref(), Some("mermaid") | Some("diagram"));

            if color {
                let raw_bytes = line.as_bytes();
                let has_closing_border = stripped.ends_with(" │");
                let inner_raw = match find_first_bytes(raw_bytes, VERT_SPACE) {
                    Some(start) => {
                        let content_start = start + VERT_SPACE.len();
                        if has_closing_border {
                            match find_last_bytes(raw_bytes, SPACE_VERT) {
                                Some(end) if content_start <= end => &line[content_start..end],
                                _ => &line[content_start..],
                            }
                        } else {
                            &line[content_start..]
                        }
                    }
                    None => {
                        let p = "│ ".len();
                        let s = if has_closing_border { " │".len() } else { 0 };
                        &stripped[p..stripped.len() - s]
                    }
                };

                // For diagram blocks, discard syntax-highlighting ANSI codes so
                // the box-drawing characters appear in the default foreground colour.
                let inner_clean: String;
                let (inner, inner_display_width) = if is_diagram {
                    inner_clean = strip_ansi(inner_raw);
                    (inner_clean.as_str(), inner_clean.chars().count())
                } else {
                    let w = strip_ansi(inner_raw).chars().count();
                    (inner_raw, w)
                };

                let inner_with_bg = if is_diagram {
                    inner.to_string()
                } else {
                    reinsert_bg_after_resets(inner, CODE_BG)
                };

                // 2-space inner padding on both sides; remaining space fills the bg.
                let right_fill = width.saturating_sub(inner_display_width + 4);

                result.push_str(CODE_BG);
                result.push_str("  ");
                result.push_str(&inner_with_bg);
                for _ in 0..right_fill {
                    result.push(' ');
                }
                result.push_str("  ");
                result.push_str(RESET);
            } else {
                let p = "│ ".len();
                let has_closing = stripped.ends_with(" │");
                let s = if has_closing { " │".len() } else { 0 };
                let inner = &stripped[p..stripped.len() - s];
                result.push_str("    ");
                result.push_str(inner);
            }

            if raw_line.ends_with('\n') {
                result.push('\n');
            }
        } else {
            result.push_str(raw_line);
        }
    }

    result
}

/// Strip `#` markers from heading lines and add decorative underlines for H1/H2.
fn fix_headings(output: &str, color: bool) -> String {
    let mut result = String::with_capacity(output.len() + 512);

    for raw_line in output.split_inclusive('\n') {
        let line = raw_line.trim_end_matches('\n');
        let stripped = strip_ansi(line);
        let trimmed = stripped.trim_start();

        let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
        if hash_count > 0 && hash_count <= 6 {
            let after_hashes = &trimmed[hash_count..];
            if after_hashes.starts_with(' ') {
                let text = after_hashes.trim_start_matches(' ');
                let text_width = text.chars().count().max(2);

                if color {
                    let marker = "#".repeat(hash_count) + " ";
                    if let Some(pos) = line.find(marker.as_str()) {
                        result.push_str(&line[..pos]);
                        result.push_str(&line[pos + marker.len()..]);
                    } else {
                        result.push_str(line);
                    }
                } else {
                    result.push_str(text);
                }
                if raw_line.ends_with('\n') {
                    result.push('\n');
                }

                // Decorative underline for H1 and H2.
                match hash_count {
                    1 => {
                        if color {
                            result.push_str("\x1b[38;5;240m");
                            for _ in 0..text_width {
                                result.push('═');
                            }
                            result.push_str("\x1b[0m");
                        } else {
                            for _ in 0..text_width {
                                result.push('=');
                            }
                        }
                        result.push('\n');
                    }
                    2 => {
                        if color {
                            result.push_str("\x1b[38;5;240m");
                            for _ in 0..text_width {
                                result.push('─');
                            }
                            result.push_str("\x1b[0m");
                        } else {
                            for _ in 0..text_width {
                                result.push('-');
                            }
                        }
                        result.push('\n');
                    }
                    _ => {}
                }
                continue;
            }
        }

        result.push_str(raw_line);
    }

    result
}

pub fn preprocess_mermaid_blocks(source: &str, options: RenderOptions) -> String {
    let mut output = String::new();
    let mut current_mermaid = String::new();
    let mut mermaid_fence: Option<(char, usize)> = None;

    for raw_line in source.split_inclusive('\n') {
        let line = raw_line.trim_end_matches('\n');

        if let Some((marker, marker_len)) = mermaid_fence {
            if is_fence_close(line, marker, marker_len) {
                output.push_str("```mermaid\n");
                let rendered = render_mermaid_block(&current_mermaid, options);
                output.push_str(&rendered);
                if !rendered.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str("```\n");
                current_mermaid.clear();
                mermaid_fence = None;
            } else {
                current_mermaid.push_str(line);
                current_mermaid.push('\n');
            }
            continue;
        }

        if let Some((marker, marker_len, language)) = parse_fence_start(line)
            && language.eq_ignore_ascii_case("mermaid")
        {
            mermaid_fence = Some((marker, marker_len));
            continue;
        }

        output.push_str(raw_line);
    }

    if mermaid_fence.is_some() {
        // Unclosed mermaid fence at EOF — render it the same way a closed fence would.
        output.push_str("```mermaid\n");
        let rendered = render_mermaid_block(&current_mermaid, options);
        output.push_str(&rendered);
        if !rendered.ends_with('\n') {
            output.push('\n');
        }
        output.push_str("```\n");
    }

    output
}

pub fn render_terminal(doc: &Document, options: RenderOptions) -> String {
    let markdown = document_to_markdown(doc);
    render_markdown(&markdown, options)
}

fn render_mermaid_block(source: &str, options: RenderOptions) -> String {
    let mermaid_options = MermaidRenderOptions {
        max_width: Some(options.width.saturating_sub(4).max(20)),
        ascii: options.ascii_mermaid,
        color: false,
        ..Default::default()
    };

    match render_mermaid(source.trim(), &mermaid_options) {
        Ok(rendered) => rendered,
        Err(error) => {
            let msg = error.to_string();
            let short = msg.strip_prefix("parse error: ").unwrap_or(&msg);
            let truncated: String = short.chars().take(60).collect();
            let suffix = if short.chars().count() > 60 {
                "…"
            } else {
                ""
            };
            // Trailing newline forces mdansi to render this as a multi-line
            // box (╭…╮ / │…│ / ╰…╯) rather than the single-line bare-border
            // format (│ content — no closing border or box top/bottom).
            format!("[diagram error: {truncated}{suffix}]\n")
        }
    }
}

fn parse_fence_start(line: &str) -> Option<(char, usize, &str)> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }

    let marker_len = trimmed.chars().take_while(|ch| *ch == marker).count();
    if marker_len < 3 {
        return None;
    }

    let language = trimmed[marker_len..]
        .split_whitespace()
        .next()
        .unwrap_or_default();

    Some((marker, marker_len, language))
}

fn is_fence_close(line: &str, marker: char, marker_len: usize) -> bool {
    let trimmed = line.trim_start();
    let count = trimmed.chars().take_while(|ch| *ch == marker).count();
    count >= marker_len && trimmed[count..].trim().is_empty()
}

fn inline_to_md(inline: &mdstar_core::Inline) -> String {
    use mdstar_core::Inline;
    match inline {
        Inline::Text(t) => t.clone(),
        Inline::Strong(children) => format!("**{}**", inlines_to_md(children)),
        Inline::Emphasis(children) => format!("*{}*", inlines_to_md(children)),
        Inline::Delete(children) => format!("~~{}~~", inlines_to_md(children)),
        Inline::Code(code) => format!("`{code}`"),
        Inline::Math(math) => format!("${math}$"),
        Inline::Link {
            children,
            url,
            title,
        } => {
            let t = title
                .as_deref()
                .map(|s| format!(" \"{s}\""))
                .unwrap_or_default();
            format!("[{}]({url}{t})", inlines_to_md(children))
        }
        Inline::Image { alt, url, title } => {
            let t = title
                .as_deref()
                .map(|s| format!(" \"{s}\""))
                .unwrap_or_default();
            format!("![{alt}]({url}{t})")
        }
        Inline::Html(html) => html.clone(),
        Inline::SoftBreak => "\n".to_string(),
        Inline::HardBreak => "  \n".to_string(),
    }
}

fn inlines_to_md(inlines: &[mdstar_core::Inline]) -> String {
    inlines.iter().map(inline_to_md).collect()
}

fn escape_table_cell(text: String) -> String {
    text.replace('|', "\\|").replace('\n', "<br>")
}

fn document_to_markdown(doc: &Document) -> String {
    let mut output = String::new();
    for block in &doc.blocks {
        block_to_md(block, 0, &mut output);
    }
    output
}

fn block_to_md(block: &Block, list_depth: usize, output: &mut String) {
    let indent = "  ".repeat(list_depth);
    match block {
        Block::Heading { level, children } => {
            output.push_str(&format!(
                "{}{} {}\n\n",
                indent,
                "#".repeat(*level as usize),
                inlines_to_md(children)
            ));
        }
        Block::Paragraph { children } => {
            output.push_str(&format!("{}{}\n\n", indent, inlines_to_md(children)));
        }
        Block::BlockQuote(blocks) => {
            let mut inner = String::new();
            for b in blocks {
                block_to_md(b, 0, &mut inner);
            }
            for line in inner.lines() {
                output.push_str(&format!("{}> {line}\n", indent));
            }
            output.push('\n');
        }
        Block::List {
            ordered,
            start,
            items,
        } => {
            let mut counter = start.unwrap_or(1);
            for item in items {
                // Task-list checkbox prefix
                let checkbox = match item.checked {
                    Some(true) => "[x] ",
                    Some(false) => "[ ] ",
                    None => "",
                };
                let bullet = if *ordered {
                    let b = format!("{counter}. {checkbox}");
                    counter += 1;
                    b
                } else {
                    format!("- {checkbox}")
                };

                // Flatten the item's block children into a single string,
                // then prefix the first line with the bullet.
                let mut item_text = String::new();
                for child in &item.children {
                    block_to_md(child, 0, &mut item_text);
                }
                let trimmed = item_text.trim_end();
                let mut lines = trimmed.lines();
                if let Some(first) = lines.next() {
                    output.push_str(&format!("{indent}{bullet}{first}\n"));
                }
                for rest in lines {
                    output.push_str(&format!("{indent}  {rest}\n"));
                }
            }
            output.push('\n');
        }
        Block::CodeFence { language, code, .. } => {
            output.push_str(&format!("{indent}```"));
            if let Some(lang) = language {
                output.push_str(lang);
            }
            output.push('\n');
            for line in code.lines() {
                output.push_str(&format!("{indent}{line}\n"));
            }
            output.push_str(&format!("{indent}```\n\n"));
        }
        Block::Table { headers, rows } => {
            if headers.is_empty() {
                return;
            }
            let header_strs: Vec<String> = headers
                .iter()
                .map(|c| escape_table_cell(inlines_to_md(c)))
                .collect();
            output.push_str(&format!("{indent}| {} |\n", header_strs.join(" | ")));
            output.push_str(&format!(
                "{indent}|{}|\n",
                header_strs
                    .iter()
                    .map(|_| " --- ")
                    .collect::<Vec<_>>()
                    .join("|")
            ));
            for row in rows {
                let cells: Vec<String> = row
                    .iter()
                    .map(|c| escape_table_cell(inlines_to_md(c)))
                    .collect();
                output.push_str(&format!("{indent}| {} |\n", cells.join(" | ")));
            }
            output.push('\n');
        }
        Block::ThematicBreak => output.push_str(&format!("{indent}---\n\n")),
        Block::Html(html) => output.push_str(html),
        Block::Math(math) => output.push_str(&format!("{indent}$$\n{math}\n$$\n\n")),
        Block::Frontmatter(_) => {} // omit from rendered output
    }
}
