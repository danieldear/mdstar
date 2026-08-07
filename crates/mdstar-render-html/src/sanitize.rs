//! Sanitizer for raw HTML embedded in Markdown.
//!
//! Markdown may contain literal HTML. Passing it through was harmless while the
//! macOS reader drew text itself, but the HTML frontend renders inside a web
//! view, where a document could otherwise run script simply by being opened.
//! Documents are untrusted input, so the markup is reduced to a small
//! presentational allowlist before it reaches any renderer.

/// Tags a document may use for presentation. Anything outside this list is
/// escaped so it shows as literal text rather than being interpreted.
const ALLOWED_TAGS: &[&str] = &[
    "b",
    "strong",
    "i",
    "em",
    "u",
    "s",
    "del",
    "ins",
    "mark",
    "small",
    "sub",
    "sup",
    "code",
    "kbd",
    "samp",
    "var",
    "abbr",
    "cite",
    "q",
    "dfn",
    "br",
    "hr",
    "p",
    "div",
    "span",
    "blockquote",
    "pre",
    "ul",
    "ol",
    "li",
    "dl",
    "dt",
    "dd",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "th",
    "td",
    "caption",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "details",
    "summary",
    "figure",
    "figcaption",
    "picture",
    "source",
    "a",
    "img",
];

/// Attributes that carry no script capability. Everything else — notably every
/// `on*` event handler — is dropped.
const ALLOWED_ATTRIBUTES: &[&str] = &[
    "href",
    "src",
    "alt",
    "title",
    "width",
    "height",
    "colspan",
    "rowspan",
    "align",
    "open",
    "srcset",
    "type",
    "checked",
    "disabled",
    "start",
    "id",
    "class",
    "data-language",
    "data-anchor",
];

/// Attributes whose presence alone is meaningful; rendered without a value.
const BOOLEAN_ATTRIBUTES: &[&str] = &["checked", "disabled", "open"];

/// URL schemes permitted in `href`/`src`.
const ALLOWED_SCHEMES: &[&str] = &["http:", "https:", "mailto:", "file:", "data:image/"];

/// Reduce raw HTML to the allowlist above.
pub fn sanitize_html(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != '<' {
            push_escaped(chars[index], &mut out);
            index += 1;
            continue;
        }

        let Some(close) = find_tag_end(&chars, index) else {
            // Unterminated '<' is literal text.
            out.push_str("&lt;");
            index += 1;
            continue;
        };

        let raw_tag: String = chars[index + 1..close].iter().collect();
        match sanitize_tag(&raw_tag) {
            Some(tag) => out.push_str(&tag),
            None => {
                // Disallowed element: show its source rather than run it.
                out.push_str("&lt;");
                for ch in &chars[index + 1..close] {
                    push_escaped(*ch, &mut out);
                }
                out.push_str("&gt;");
            }
        }
        index = close + 1;
    }

    out
}

fn find_tag_end(chars: &[char], start: usize) -> Option<usize> {
    let mut index = start + 1;
    let mut quote: Option<char> = None;
    while index < chars.len() {
        let ch = chars[index];
        match quote {
            Some(active) if ch == active => quote = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch == '>' => return Some(index),
            None => {}
        }
        index += 1;
    }
    None
}

fn sanitize_tag(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with('?') {
        // Comments, doctypes and processing instructions are dropped entirely.
        return Some(String::new());
    }

    let is_closing = trimmed.starts_with('/');
    let body = if is_closing { &trimmed[1..] } else { trimmed };
    let self_closing = body.trim_end().ends_with('/');
    let body = body.trim_end().trim_end_matches('/');

    let mut parts = body.splitn(2, |c: char| c.is_whitespace());
    let name = parts.next().unwrap_or_default().to_ascii_lowercase();
    if !ALLOWED_TAGS.contains(&name.as_str()) {
        return None;
    }

    if is_closing {
        return Some(format!("</{name}>"));
    }

    let mut out = String::from("<");
    out.push_str(&name);
    for (key, value) in parse_attributes(parts.next().unwrap_or_default()) {
        let key = key.to_ascii_lowercase();
        if !ALLOWED_ATTRIBUTES.contains(&key.as_str()) {
            continue;
        }
        if (key == "href" || key == "src") && !is_safe_url(&value) {
            continue;
        }
        out.push(' ');
        out.push_str(&key);
        if BOOLEAN_ATTRIBUTES.contains(&key.as_str()) && value.is_empty() {
            continue;
        }
        out.push_str("=\"");
        for ch in value.chars() {
            push_escaped(ch, &mut out);
        }
        out.push('"');
    }
    if self_closing {
        out.push_str(" /");
    }
    out.push('>');
    Some(out)
}

fn parse_attributes(input: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = input.chars().collect();
    let mut attributes = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }

        let name_start = index;
        while index < chars.len() && !chars[index].is_whitespace() && chars[index] != '=' {
            index += 1;
        }
        let name: String = chars[name_start..index].iter().collect();
        if name.is_empty() {
            index += 1;
            continue;
        }

        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }

        if index < chars.len() && chars[index] == '=' {
            index += 1;
            while index < chars.len() && chars[index].is_whitespace() {
                index += 1;
            }
            let value = if index < chars.len() && (chars[index] == '"' || chars[index] == '\'') {
                let quote = chars[index];
                index += 1;
                let start = index;
                while index < chars.len() && chars[index] != quote {
                    index += 1;
                }
                let value: String = chars[start..index].iter().collect();
                index += 1;
                value
            } else {
                let start = index;
                while index < chars.len() && !chars[index].is_whitespace() {
                    index += 1;
                }
                chars[start..index].iter().collect()
            };
            attributes.push((name, value));
        } else {
            attributes.push((name, String::new()));
        }
    }

    attributes
}

fn is_safe_url(value: &str) -> bool {
    let trimmed = value.trim().to_ascii_lowercase();
    // Relative URLs and fragments carry no scheme and are safe.
    if !trimmed.contains(':') {
        return true;
    }
    if trimmed.starts_with('#') || trimmed.starts_with('/') {
        return true;
    }
    ALLOWED_SCHEMES
        .iter()
        .any(|scheme| trimmed.starts_with(scheme))
}

fn push_escaped(ch: char, out: &mut String) {
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        '\'' => out.push_str("&#39;"),
        _ => out.push(ch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_script_elements() {
        let output = sanitize_html("before <script>alert(1)</script> after");
        assert!(!output.contains("<script"), "got: {output}");
        assert!(output.contains("before"));
        assert!(output.contains("after"));
    }

    #[test]
    fn strips_event_handler_attributes() {
        let output = sanitize_html("<img src=\"a.png\" onerror=\"alert(1)\" />");
        assert!(output.contains("src=\"a.png\""));
        assert!(
            !output.to_ascii_lowercase().contains("onerror"),
            "got: {output}"
        );
    }

    #[test]
    fn rejects_javascript_urls() {
        let output = sanitize_html("<a href=\"javascript:alert(1)\">x</a>");
        assert!(!output.contains("javascript:"), "got: {output}");
        assert!(
            output.contains("<a>") || output.contains("<a >"),
            "got: {output}"
        );
    }

    #[test]
    fn strips_iframes_and_objects() {
        for markup in [
            "<iframe src=\"http://x\"></iframe>",
            "<object data=\"x\"></object>",
        ] {
            let output = sanitize_html(markup);
            assert!(!output.contains("<iframe"), "got: {output}");
            assert!(!output.contains("<object"), "got: {output}");
        }
    }

    #[test]
    fn keeps_presentational_markup() {
        let output = sanitize_html(
            "line<br /> <kbd>K</kbd> <sub>2</sub> <details open><summary>s</summary>x</details>",
        );
        assert!(output.contains("<br />"));
        assert!(output.contains("<kbd>"));
        assert!(output.contains("<sub>"));
        assert!(output.contains("<details open>"));
    }

    #[test]
    fn keeps_safe_links_and_images() {
        let output = sanitize_html(
            "<a href=\"https://example.com\" title=\"t\">x</a><img src=\"pic.png\" alt=\"a\" />",
        );
        assert!(output.contains("href=\"https://example.com\""));
        assert!(output.contains("title=\"t\""));
        assert!(output.contains("src=\"pic.png\""));
        assert!(output.contains("alt=\"a\""));
    }

    #[test]
    fn drops_comments_and_doctypes() {
        let output = sanitize_html("<!-- secret --><!DOCTYPE html>visible");
        assert!(output.contains("visible"));
        assert!(!output.contains("secret"), "got: {output}");
    }

    #[test]
    fn unterminated_angle_bracket_is_literal() {
        let output = sanitize_html("5 < 7 and 9 > 2");
        assert!(output.contains("&lt;"));
        assert!(output.contains("&gt;"));
    }
}
