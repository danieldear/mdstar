//! Writing helpers for the source editor: the `/` insert menu, list
//! continuation on Enter, and wrapping a selection in Markdown markup.

use volt_ui_kit::ui::Icon;

pub struct SlashCommand {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub icon: Icon,
    keywords: &'static str,
    /// Text that replaces `/query`.
    pub insert: &'static str,
    /// Selection inside `insert` afterwards (caret when empty).
    pub select: (usize, usize),
}

const fn cmd(
    title: &'static str,
    subtitle: &'static str,
    icon: Icon,
    keywords: &'static str,
    insert: &'static str,
    select: (usize, usize),
) -> SlashCommand {
    SlashCommand {
        title,
        subtitle,
        icon,
        keywords,
        insert,
        select,
    }
}

pub const COMMANDS: [SlashCommand; 14] = [
    cmd(
        "Heading 1",
        "Large section heading",
        Icon::Type,
        "h1 title",
        "# ",
        (2, 2),
    ),
    cmd(
        "Heading 2",
        "Medium section heading",
        Icon::Type,
        "h2 subtitle",
        "## ",
        (3, 3),
    ),
    cmd(
        "Heading 3",
        "Small section heading",
        Icon::Type,
        "h3",
        "### ",
        (4, 4),
    ),
    cmd(
        "Bulleted List",
        "Create a simple list",
        Icon::List,
        "ul bullet unordered",
        "- ",
        (2, 2),
    ),
    cmd(
        "Numbered List",
        "Create an ordered list",
        Icon::List,
        "ol ordered number",
        "1. ",
        (3, 3),
    ),
    cmd(
        "Task",
        "Create a Markdown checkbox",
        Icon::Check,
        "todo checkbox checklist",
        "- [ ] ",
        (6, 6),
    ),
    cmd(
        "Quote",
        "Capture a quotation or callout",
        Icon::Edit,
        "blockquote callout",
        "> ",
        (2, 2),
    ),
    cmd(
        "Code Block",
        "Insert a fenced code block",
        Icon::Command,
        "code fence pre",
        "```\n\n```",
        (4, 4),
    ),
    cmd(
        "Table",
        "Insert a two-column table",
        Icon::Grid,
        "table grid",
        "| Column | Column |\n| --- | --- |\n|  |  |",
        (2, 8),
    ),
    cmd(
        "Link",
        "Insert a Markdown link",
        Icon::Link,
        "url href",
        "[text](https://)",
        (1, 5),
    ),
    cmd(
        "Image",
        "Insert an image reference",
        Icon::Image,
        "picture img",
        "![alt text](image.png)",
        (2, 10),
    ),
    cmd(
        "Mermaid Diagram",
        "Insert a Mermaid diagram block",
        Icon::Sliders,
        "diagram graph chart flow",
        "```mermaid\ngraph TD\n  A --> B\n```",
        (22, 29),
    ),
    cmd(
        "Divider",
        "Insert a horizontal rule",
        Icon::Minus,
        "hr rule separator line",
        "---\n",
        (4, 4),
    ),
    cmd(
        "Front Matter",
        "Insert YAML document metadata",
        Icon::FileText,
        "yaml metadata frontmatter",
        "---\ntitle: Untitled\n---\n",
        (11, 19),
    ),
];

/// An open `/` menu: where the `/` is, what follows it, the highlighted
/// command and where to draw the menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Slash {
    pub start: usize,
    pub query: String,
    pub index: usize,
    pub at: [f32; 2],
}

/// The drawn menu's height for `n` commands (rows, header and padding,
/// up to its scrolling limit).
pub fn menu_height(n: usize) -> f32 {
    (12. + 26. + n as f32 * 43.).min(MENU_MAX)
}
pub const MENU_MAX: f32 = 380.;

/// Commands whose title or keywords contain `query`.
pub fn matching(query: &str) -> Vec<&'static SlashCommand> {
    let q = query.to_lowercase();
    COMMANDS
        .iter()
        .filter(|c| {
            q.is_empty() || c.title.to_lowercase().contains(&q) || c.keywords.contains(q.as_str())
        })
        .collect()
}

/// The `/query` the caret ends, when it starts its line (after indentation).
pub fn slash_at(text: &str, caret: usize) -> Option<(usize, String)> {
    let line_start = text[..caret].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[line_start..caret];
    let trimmed = before.trim_start_matches([' ', '\t']);
    let query = trimmed.strip_prefix('/')?;
    if query.len() > 24 || !query.chars().all(|c| c.is_alphanumeric() || c == ' ') {
        return None;
    }
    if matching(query).is_empty() {
        return None;
    }
    Some((caret - trimmed.len(), query.to_owned()))
}

/// The list marker Enter should continue at `caret`: the next item's
/// prefix, or `None` when the line is not a list item. An empty item
/// returns `Some("")`: Enter ends the list instead.
pub fn continue_list(text: &str, caret: usize) -> Option<String> {
    let line_start = text[..caret].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[caret..].find('\n').map_or(text.len(), |i| caret + i);
    // Only at the end of the line, so Enter mid-line still splits it.
    if caret != line_end {
        return None;
    }
    let line = &text[line_start..line_end];
    let indent: String = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let rest = &line[indent.len()..];
    let (marker, content) = if let Some(after) = ["- [ ] ", "- [x] ", "* [ ] ", "* [x] "]
        .iter()
        .find_map(|m| {
            rest.strip_prefix(m)
                .map(|c| (m[..2].to_owned() + "[ ] ", c))
        }) {
        after
    } else if let Some(c) = ["- ", "* ", "+ ", "> "]
        .iter()
        .find_map(|m| rest.strip_prefix(m).map(|c| ((*m).to_owned(), c)))
    {
        c
    } else {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let after = rest[digits.len()..]
            .strip_prefix(". ")
            .or_else(|| rest[digits.len()..].strip_prefix(") "))?;
        if digits.is_empty() || digits.len() > 9 {
            return None;
        }
        let n: u64 = digits.parse().ok()?;
        let sep = &rest[digits.len()..digits.len() + 2];
        (format!("{}{sep}", n + 1), after)
    };
    if content.trim().is_empty() {
        return Some(String::new());
    }
    Some(format!("{indent}{marker}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_opens_only_at_the_start_of_a_line() {
        assert_eq!(slash_at("/", 1), Some((0, String::new())));
        assert_eq!(slash_at("intro\n  /hea", 12), Some((8, "hea".into())));
        assert_eq!(slash_at("a /b", 4), None, "mid-line slashes are text");
        assert_eq!(slash_at("/zzz", 4), None, "nothing matches");
        assert_eq!(slash_at("http://x", 7), None);
        assert_eq!(
            matching("list").len(),
            3,
            "bulleted, numbered and task (checklist)"
        );
        assert_eq!(matching("TODO")[0].title, "Task");
    }

    #[test]
    fn enter_continues_lists_and_ends_empty_items() {
        assert_eq!(continue_list("- one", 5).as_deref(), Some("- "));
        assert_eq!(continue_list("  * two", 7).as_deref(), Some("  * "));
        assert_eq!(continue_list("- [x] done", 10).as_deref(), Some("- [ ] "));
        assert_eq!(continue_list("9. nine", 7).as_deref(), Some("10. "));
        assert_eq!(continue_list("> quote", 7).as_deref(), Some("> "));
        assert_eq!(continue_list("- ", 2).as_deref(), Some(""));
        assert_eq!(continue_list("- one", 2), None, "mid-line");
        assert_eq!(continue_list("plain", 5), None);
        assert_eq!(continue_list("2024. year", 10).as_deref(), Some("2025. "));
    }

    #[test]
    fn command_selections_fall_inside_their_text() {
        for c in &COMMANDS {
            assert!(
                c.select.0 <= c.select.1 && c.select.1 <= c.insert.len(),
                "{}",
                c.title
            );
            assert!(c.insert.is_char_boundary(c.select.0) && c.insert.is_char_boundary(c.select.1));
        }
        assert_eq!(&COMMANDS[9].insert[1..5], "text");
        assert_eq!(&COMMANDS[11].insert[22..29], "A --> B");
        assert_eq!(&COMMANDS[13].insert[11..19], "Untitled");
    }
}

/// Colours for the source editor's Markdown, as palette indices.
pub mod tint {
    pub const HEADING: u8 = 0;
    pub const SYNTAX: u8 = 1;
    pub const CODE: u8 = 2;
    pub const LINK: u8 = 3;
    pub const COMMENT: u8 = 4;
    pub const MARKER: u8 = 5;
}

/// The editor palette for those colours.
pub fn source_palette(dark: bool) -> Vec<String> {
    use crate::highlight::{Token, color};
    vec![
        "@base-content".into(),
        "@muted".into(),
        color(Token::String, dark).into(),
        "@primary".into(),
        color(Token::Comment, dark).into(),
        "@primary".into(),
    ]
}

/// Colour ranges for Markdown source: heading lines, syntax markers, code,
/// link targets and comments. Ordered and non-overlapping.
pub fn source_tints(text: &str) -> Vec<(usize, usize, u8)> {
    let mut out: Vec<(usize, usize, u8)> = Vec::new();
    let push = |out: &mut Vec<(usize, usize, u8)>, start: usize, end: usize, tint: u8| {
        if start < end && out.last().is_none_or(|l| l.1 <= start) {
            out.push((start, end, tint));
        }
    };
    let mut fence: Option<&str> = None;
    let mut front_matter = text.starts_with("---\n");
    let mut at = 0;
    for (n, raw) in text.split_inclusive('\n').enumerate() {
        let line = raw.trim_end_matches(['\n', '\r']);
        let start = at;
        let end = at + line.len();
        at += raw.len();
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if front_matter {
            push(&mut out, start, end, tint::COMMENT);
            if n > 0 && line == "---" {
                front_matter = false;
            }
            continue;
        }
        if let Some(open) = fence {
            if trimmed.starts_with(open) {
                push(&mut out, start, end, tint::SYNTAX);
                fence = None;
            } else {
                push(&mut out, start, end, tint::CODE);
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(&trimmed[..3]);
            push(&mut out, start, end, tint::SYNTAX);
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            push(
                &mut out,
                start + indent,
                start + indent + hashes,
                tint::SYNTAX,
            );
            push(&mut out, start + indent + hashes, end, tint::HEADING);
            continue;
        }
        if !trimmed.is_empty()
            && ["---", "***", "___"].iter().any(|r| {
                trimmed
                    .chars()
                    .all(|c| c == r.chars().next().unwrap() || c == ' ')
                    && trimmed.len() >= 3
                    && trimmed.starts_with(&r[..1])
            })
        {
            push(&mut out, start, end, tint::SYNTAX);
            continue;
        }
        // Block markers: quotes, list bullets, numbers and task boxes.
        let mut body = start + indent;
        let rest = trimmed;
        let marker = if rest.starts_with("> ") || rest == ">" {
            1
        } else if ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] "]
            .iter()
            .any(|m| rest.starts_with(m))
        {
            6
        } else if ["- ", "* ", "+ "].iter().any(|m| rest.starts_with(m)) {
            2
        } else {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            if digits > 0
                && digits < 10
                && (rest[digits..].starts_with(". ") || rest[digits..].starts_with(") "))
            {
                digits + 2
            } else {
                0
            }
        };
        if marker > 0 {
            push(&mut out, body, body + marker, tint::MARKER);
            body += marker;
        }
        inline_tints(text, body, end, &mut out);
    }
    out
}

/// Inline code, link targets, emphasis markers, table pipes and comments.
fn inline_tints(text: &str, start: usize, end: usize, out: &mut Vec<(usize, usize, u8)>) {
    let line = &text[start..end];
    let bytes = line.as_bytes();
    let mut i = 0;
    let push = |out: &mut Vec<(usize, usize, u8)>, a: usize, b: usize, tint: u8| {
        if a < b && out.last().is_none_or(|l| l.1 <= start + a) {
            out.push((start + a, start + b, tint));
        }
    };
    while i < bytes.len() {
        match bytes[i] {
            b'`' => {
                let ticks = bytes[i..].iter().take_while(|b| **b == b'`').count();
                let close = line[i + ticks..]
                    .find(&line[i..i + ticks])
                    .map(|c| i + ticks + c + ticks);
                let to = close.unwrap_or(i + ticks);
                push(
                    out,
                    i,
                    to,
                    if close.is_some() {
                        tint::CODE
                    } else {
                        tint::SYNTAX
                    },
                );
                i = to;
            }
            b'<' if line[i..].starts_with("<!--") => {
                let to = line[i..].find("-->").map_or(line.len(), |c| i + c + 3);
                push(out, i, to, tint::COMMENT);
                i = to;
            }
            b']' if bytes.get(i + 1) == Some(&b'(') => {
                let to = line[i..].find(')').map_or(line.len(), |c| i + c + 1);
                push(out, i, i + 2, tint::SYNTAX);
                push(out, i + 2, to.saturating_sub(1).max(i + 2), tint::LINK);
                if to > i + 2 {
                    push(out, to - 1, to, tint::SYNTAX);
                }
                i = to;
            }
            b'[' | b'|' => {
                push(out, i, i + 1, tint::SYNTAX);
                i += 1;
            }
            b'*' | b'_' | b'~' => {
                let run = bytes[i..].iter().take_while(|b| **b == bytes[i]).count();
                // Markers next to a word, not a lone `_` inside one.
                let before = i.checked_sub(1).and_then(|p| bytes.get(p));
                let inside = bytes[i] == b'_'
                    && before.is_some_and(u8::is_ascii_alphanumeric)
                    && bytes.get(i + run).is_some_and(u8::is_ascii_alphanumeric);
                if !inside && (bytes[i] != b'~' || run >= 2) {
                    push(out, i, i + run, tint::SYNTAX);
                }
                i += run;
            }
            _ => i += line[i..].chars().next().map_or(1, char::len_utf8),
        }
    }
}

#[cfg(test)]
mod tint_tests {
    use super::*;

    fn named(text: &str) -> Vec<(&str, u8)> {
        source_tints(text)
            .into_iter()
            .map(|(a, b, t)| (&text[a..b], t))
            .collect()
    }

    #[test]
    fn source_tints_mark_markdown_structure() {
        assert_eq!(
            named("# Title\n- [ ] a `b` [c](d) **e** snake_case\n"),
            [
                ("#", tint::SYNTAX),
                (" Title", tint::HEADING),
                ("- [ ] ", tint::MARKER),
                ("`b`", tint::CODE),
                ("[", tint::SYNTAX),
                ("](", tint::SYNTAX),
                ("d", tint::LINK),
                (")", tint::SYNTAX),
                ("**", tint::SYNTAX),
                ("**", tint::SYNTAX),
            ]
        );
        assert_eq!(
            named("---\ntitle: x\n---\n```rust\nfn a() {}\n```\n> quote\n"),
            [
                ("---", tint::COMMENT),
                ("title: x", tint::COMMENT),
                ("---", tint::COMMENT),
                ("```rust", tint::SYNTAX),
                ("fn a() {}", tint::CODE),
                ("```", tint::SYNTAX),
                (">", tint::MARKER),
            ]
        );
        // Ranges never overlap and stay in order.
        let tints = source_tints("**a** `b` <!-- c --> |x|\n");
        assert!(tints.windows(2).all(|w| w[0].1 <= w[1].0));
    }
}
