pub mod document_ir;
pub mod highlight;
pub mod sanitize;

pub use document_ir::{base_stylesheet, render_document_ir};
pub use sanitize::sanitize_html;

use mdstar_core::{Block, Inline, ListItem};

pub fn render_html(doc: &mdstar_core::Document) -> String {
    let mut out = String::new();
    for block in &doc.blocks {
        render_block(block, &mut out);
    }
    out
}

fn render_block(block: &Block, out: &mut String) {
    match block {
        Block::Heading { level, children } => {
            let level = level.clamp(&1, &6);
            out.push_str(&format!(
                "<h{level}>{}</h{level}>\n",
                render_inlines(children)
            ));
        }

        Block::Paragraph { children } => {
            out.push_str(&format!("<p>{}</p>\n", render_inlines(children)));
        }

        Block::BlockQuote(blocks) => {
            out.push_str("<blockquote>\n");
            for b in blocks {
                render_block(b, out);
            }
            out.push_str("</blockquote>\n");
        }

        Block::List {
            ordered,
            start,
            items,
        } => {
            if *ordered {
                let start_attr = match start {
                    Some(n) if *n != 1 => format!(" start=\"{n}\""),
                    _ => String::new(),
                };
                out.push_str(&format!("<ol{start_attr}>\n"));
            } else {
                out.push_str("<ul>\n");
            }
            for item in items {
                render_list_item(item, out);
            }
            if *ordered {
                out.push_str("</ol>\n");
            } else {
                out.push_str("</ul>\n");
            }
        }

        Block::CodeFence { language, code, .. } => {
            if let Some(lang) = language {
                out.push_str(&format!(
                    "<pre><code class=\"language-{}\">{}</code></pre>\n",
                    escape_html(lang),
                    escape_html(code)
                ));
            } else {
                out.push_str(&format!("<pre><code>{}</code></pre>\n", escape_html(code)));
            }
        }

        Block::Table { headers, rows } => {
            out.push_str("<table>\n");
            if !headers.is_empty() {
                out.push_str("<thead><tr>");
                for header in headers {
                    out.push_str(&format!("<th>{}</th>", render_inlines(header)));
                }
                out.push_str("</tr></thead>\n");
            }
            out.push_str("<tbody>\n");
            for row in rows {
                out.push_str("<tr>");
                for cell in row {
                    out.push_str(&format!("<td>{}</td>", render_inlines(cell)));
                }
                out.push_str("</tr>\n");
            }
            out.push_str("</tbody>\n</table>\n");
        }

        Block::ThematicBreak => out.push_str("<hr />\n"),

        Block::Html(html) => out.push_str(&sanitize::sanitize_html(html)),

        Block::Math(math) => {
            out.push_str(&format!(
                "<div class=\"math math-display\">\\[{}\\]</div>\n",
                escape_html(math)
            ));
        }

        Block::Frontmatter(_) => {
            // Frontmatter is metadata — not rendered into the HTML body.
        }
    }
}

fn render_list_item(item: &ListItem, out: &mut String) {
    if let Some(checked) = item.checked {
        let attr = if checked { " checked" } else { "" };
        out.push_str(&format!("<li><input type=\"checkbox\" disabled{attr}> "));
    } else {
        out.push_str("<li>");
    }
    for child in &item.children {
        render_block(child, out);
    }
    out.push_str("</li>\n");
}

fn render_inlines(inlines: &[Inline]) -> String {
    inlines.iter().map(render_inline).collect()
}

fn render_inline(inline: &Inline) -> String {
    match inline {
        Inline::Text(text) => escape_html(text),
        Inline::Strong(children) => format!("<strong>{}</strong>", render_inlines(children)),
        Inline::Emphasis(children) => format!("<em>{}</em>", render_inlines(children)),
        Inline::Delete(children) => format!("<del>{}</del>", render_inlines(children)),
        Inline::Code(code) => format!("<code>{}</code>", escape_html(code)),
        Inline::Math(math) => format!(
            "<span class=\"math math-inline\">\\({}\\)</span>",
            escape_html(math)
        ),
        Inline::Link {
            children,
            url,
            title,
        } => {
            let title_attr = title
                .as_deref()
                .map(|t| format!(" title=\"{}\"", escape_html(t)))
                .unwrap_or_default();
            format!(
                "<a href=\"{}\"{}>{}</a>",
                escape_html(url),
                title_attr,
                render_inlines(children)
            )
        }
        Inline::Image { alt, url, title } => {
            let title_attr = title
                .as_deref()
                .map(|t| format!(" title=\"{}\"", escape_html(t)))
                .unwrap_or_default();
            format!(
                "<img src=\"{}\" alt=\"{}\"{} />",
                escape_html(url),
                escape_html(alt),
                title_attr
            )
        }
        Inline::Html(html) => sanitize::sanitize_html(html),
        Inline::SoftBreak => "\n".to_string(),
        Inline::HardBreak => "<br />\n".to_string(),
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
