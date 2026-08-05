//! HTML rendering from the versioned semantic IR.
//!
//! The plain [`render_html`](crate::render_html) path renders the internal
//! `Document` and is what the terminal-adjacent consumers use. This path
//! renders [`DocumentIr`] instead so the emitted markup carries the IR's stable
//! block identifiers. Frontends need those to map a scroll position, a search
//! hit, or a text selection back to a semantic block without re-parsing.

use mdstar_core::document_ir::{BlockIr, DocumentIr, InlineIr, ListItemIr};

/// Render a document to a fragment of semantic HTML.
///
/// The fragment carries structure only. Presentation is the frontend's
/// concern: the macOS app injects a stylesheet built from the reader's
/// appearance and font preferences, while standalone consumers can use
/// [`base_stylesheet`].
pub fn render_document_ir(document: &DocumentIr) -> String {
    let mut out = String::new();
    for block in &document.blocks {
        render_block(block, &mut out);
    }
    out
}

fn render_block(block: &BlockIr, out: &mut String) {
    let id = escape_html(&block.id);
    match block.kind.as_str() {
        "heading" => {
            let level = block.level.unwrap_or(1).clamp(1, 6);
            let anchor = heading_anchor(&inline_plain_text(&block.inlines));
            out.push_str(&format!(
                "<h{level} id=\"{id}\" data-anchor=\"{}\">{}</h{level}>\n",
                escape_html(&anchor),
                render_inlines(&block.inlines)
            ));
        }

        "paragraph" => {
            out.push_str(&format!(
                "<p id=\"{id}\">{}</p>\n",
                render_inlines(&block.inlines)
            ));
        }

        "blockquote" => {
            out.push_str(&format!("<blockquote id=\"{id}\">\n"));
            for child in &block.children {
                render_block(child, out);
            }
            out.push_str("</blockquote>\n");
        }

        "list" => {
            let ordered = block.ordered.unwrap_or(false);
            // A list whose items carry checked state is a task list; it gets a
            // class so the stylesheet can drop the default markers.
            let is_task_list = block.items.iter().any(|item| item.checked.is_some());
            let class = if is_task_list {
                " class=\"task-list\""
            } else {
                ""
            };
            if ordered {
                let start_attr = match block.start {
                    Some(n) if n != 1 => format!(" start=\"{n}\""),
                    _ => String::new(),
                };
                out.push_str(&format!("<ol id=\"{id}\"{class}{start_attr}>\n"));
            } else {
                out.push_str(&format!("<ul id=\"{id}\"{class}>\n"));
            }
            for item in &block.items {
                render_list_item(item, out);
            }
            out.push_str(if ordered { "</ol>\n" } else { "</ul>\n" });
        }

        "code" => {
            let code = crate::highlight::highlight_code(
                block.code.as_deref().unwrap_or_default(),
                block.language.as_deref(),
            );
            match block.language.as_deref().filter(|lang| !lang.is_empty()) {
                Some(language) => out.push_str(&format!(
                    "<pre id=\"{id}\" data-language=\"{0}\"><code class=\"language-{0}\">{code}</code></pre>\n",
                    escape_html(language)
                )),
                None => out.push_str(&format!("<pre id=\"{id}\"><code>{code}</code></pre>\n")),
            }
        }

        "table" => {
            out.push_str(&format!(
                "<div class=\"table-scroll\"><table id=\"{id}\">\n"
            ));
            if !block.headers.is_empty() {
                out.push_str("<thead><tr>");
                for header in &block.headers {
                    out.push_str(&format!("<th>{}</th>", render_inlines(header)));
                }
                out.push_str("</tr></thead>\n");
            }
            out.push_str("<tbody>\n");
            for row in &block.rows {
                out.push_str("<tr>");
                for cell in row {
                    out.push_str(&format!("<td>{}</td>", render_inlines(cell)));
                }
                out.push_str("</tr>\n");
            }
            out.push_str("</tbody>\n</table></div>\n");
        }

        "thematic_break" => out.push_str(&format!("<hr id=\"{id}\" />\n")),

        "math" => out.push_str(&format!(
            "<div id=\"{id}\" class=\"math math-display\">\\[{}\\]</div>\n",
            escape_html(block.raw.as_deref().unwrap_or_default())
        )),

        // Documents are untrusted input and this markup ends up in a web
        // view, so raw HTML is reduced to a presentational allowlist.
        "html" => out.push_str(&crate::sanitize::sanitize_html(
            block.raw.as_deref().unwrap_or_default(),
        )),

        // Frontmatter is metadata; frontends present it separately if at all.
        "frontmatter" => {}

        _ => {
            for child in &block.children {
                render_block(child, out);
            }
        }
    }
}

fn render_list_item(item: &ListItemIr, out: &mut String) {
    match item.checked {
        Some(checked) => {
            let checked_attr = if checked { " checked" } else { "" };
            let state = if checked { "done" } else { "todo" };
            out.push_str(&format!(
                "<li class=\"task-item task-{state}\"><input type=\"checkbox\" disabled{checked_attr} />"
            ));
        }
        None => out.push_str("<li>"),
    }
    for child in &item.children {
        render_block(child, out);
    }
    out.push_str("</li>\n");
}

fn render_inlines(inlines: &[InlineIr]) -> String {
    inlines.iter().map(render_inline).collect()
}

fn render_inline(inline: &InlineIr) -> String {
    let text = inline.text.as_deref().unwrap_or_default();
    match inline.kind.as_str() {
        "text" => escape_html(text),
        "strong" => format!("<strong>{}</strong>", render_inlines(&inline.children)),
        "emphasis" => format!("<em>{}</em>", render_inlines(&inline.children)),
        "delete" => format!("<del>{}</del>", render_inlines(&inline.children)),
        "code" => format!("<code>{}</code>", escape_html(text)),
        "math" => format!(
            "<span class=\"math math-inline\">\\({}\\)</span>",
            escape_html(text)
        ),
        "link" => {
            let title_attr = inline
                .title
                .as_deref()
                .map(|title| format!(" title=\"{}\"", escape_html(title)))
                .unwrap_or_default();
            format!(
                "<a href=\"{}\"{title_attr}>{}</a>",
                escape_html(inline.url.as_deref().unwrap_or_default()),
                render_inlines(&inline.children)
            )
        }
        "image" => {
            let title_attr = inline
                .title
                .as_deref()
                .map(|title| format!(" title=\"{}\"", escape_html(title)))
                .unwrap_or_default();
            format!(
                "<img src=\"{}\" alt=\"{}\"{title_attr} />",
                escape_html(inline.url.as_deref().unwrap_or_default()),
                escape_html(inline.alt.as_deref().unwrap_or_default())
            )
        }
        "html" => crate::sanitize::sanitize_html(text),
        "hard_break" => "<br />\n".to_string(),
        _ => render_inlines(&inline.children),
    }
}

/// Structural stylesheet shared by every HTML consumer.
///
/// Colours and metrics are expressed as custom properties so a frontend can
/// retheme the document by redefining the variables rather than restating the
/// rules. The macOS reader overrides them from its appearance and font
/// preferences.
pub fn base_stylesheet() -> &'static str {
    include_str!("reader.css")
}

fn inline_plain_text(inlines: &[InlineIr]) -> String {
    let mut out = String::new();
    for inline in inlines {
        if let Some(text) = &inline.text {
            out.push_str(text);
        }
        out.push_str(&inline_plain_text(&inline.children));
    }
    out
}

/// Mirrors the anchor slugs the IR publishes in its outline, so in-page links
/// resolve to the same targets the sidebar navigates to.
fn heading_anchor(text: &str) -> String {
    let anchor = text
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = anchor.trim_matches('-').replace("--", "-");
    if compact.is_empty() {
        "section".to_string()
    } else {
        compact
    }
}

fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdstar_core::document_ir::parse_document_ir_with_diagnostics;

    fn render(markdown: &str) -> String {
        let ir = parse_document_ir_with_diagnostics(markdown, "/tmp/test.md").unwrap();
        render_document_ir(&ir)
    }

    #[test]
    fn blocks_carry_stable_identifiers() {
        let html = render("# Title\n\nA paragraph.\n");
        // Frontends anchor scroll position and annotations to these.
        assert!(html.contains("<h1 id=\"heading-"), "got: {html}");
        assert!(html.contains("<p id=\"paragraph-"), "got: {html}");
    }

    #[test]
    fn heading_anchor_matches_outline_slug() {
        let markdown = "## Core Objectives\n";
        let ir = parse_document_ir_with_diagnostics(markdown, "/tmp/test.md").unwrap();
        let html = render_document_ir(&ir);
        let anchor = &ir.outline[0].anchor;
        assert!(
            html.contains(&format!("data-anchor=\"{anchor}\"")),
            "anchor {anchor} missing from: {html}"
        );
    }

    #[test]
    fn task_lists_are_marked_and_checked() {
        let html = render("- [x] done\n- [ ] todo\n");
        assert!(html.contains("class=\"task-list\""));
        assert!(html.contains("task-done"));
        assert!(html.contains("task-todo"));
        assert!(html.contains("checked"));
    }

    #[test]
    fn code_fence_exposes_language() {
        let html = render("```rust\nfn main() {}\n```\n");
        assert!(html.contains("data-language=\"rust\""));
        assert!(html.contains("class=\"language-rust\""));
    }

    #[test]
    fn tables_render_head_and_body() {
        let html = render("| A | B |\n| - | - |\n| 1 | 2 |\n");
        assert!(html.contains("<thead>"));
        assert!(html.contains("<tbody>"));
        assert!(html.contains("<th>A</th>"));
        assert!(html.contains("<td>1</td>"));
    }

    #[test]
    fn inline_markup_is_escaped_not_injected() {
        let html = render("A <script>alert(1)</script> line.\n");
        assert!(!html.contains("<script>alert"), "got: {html}");
    }

    #[test]
    fn emphasis_and_links_render() {
        let html = render("**bold** and *italic* and [link](https://example.com)\n");
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
        assert!(html.contains("href=\"https://example.com\""));
    }

    #[test]
    fn stylesheet_defines_theming_variables() {
        let css = base_stylesheet();
        assert!(css.contains("--reader-font-size"));
        assert!(css.contains("--reader-text"));
    }
}
