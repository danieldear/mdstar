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
    cmd("Heading 1", "Large section heading", Icon::Type, "h1 title", "# ", (2, 2)),
    cmd("Heading 2", "Medium section heading", Icon::Type, "h2 subtitle", "## ", (3, 3)),
    cmd("Heading 3", "Small section heading", Icon::Type, "h3", "### ", (4, 4)),
    cmd("Bulleted List", "Create a simple list", Icon::List, "ul bullet unordered", "- ", (2, 2)),
    cmd("Numbered List", "Create an ordered list", Icon::List, "ol ordered number", "1. ", (3, 3)),
    cmd("Task", "Create a Markdown checkbox", Icon::Check, "todo checkbox checklist", "- [ ] ", (6, 6)),
    cmd("Quote", "Capture a quotation or callout", Icon::Edit, "blockquote callout", "> ", (2, 2)),
    cmd("Code Block", "Insert a fenced code block", Icon::Command, "code fence pre", "```\n\n```", (4, 4)),
    cmd(
        "Table",
        "Insert a two-column table",
        Icon::Grid,
        "table grid",
        "| Column | Column |\n| --- | --- |\n|  |  |",
        (2, 8),
    ),
    cmd("Link", "Insert a Markdown link", Icon::Link, "url href", "[text](https://)", (1, 5)),
    cmd("Image", "Insert an image reference", Icon::Image, "picture img", "![alt text](image.png)", (2, 10)),
    cmd(
        "Mermaid Diagram",
        "Insert a Mermaid diagram block",
        Icon::Sliders,
        "diagram graph chart flow",
        "```mermaid\ngraph TD\n  A --> B\n```",
        (22, 29),
    ),
    cmd("Divider", "Insert a horizontal rule", Icon::Minus, "hr rule separator line", "---\n", (4, 4)),
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
    let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    let rest = &line[indent.len()..];
    let (marker, content) = if let Some(after) = ["- [ ] ", "- [x] ", "* [ ] ", "* [x] "]
        .iter()
        .find_map(|m| rest.strip_prefix(m).map(|c| (m[..2].to_owned() + "[ ] ", c)))
    {
        after
    } else if let Some(c) = ["- ", "* ", "+ ", "> "]
        .iter()
        .find_map(|m| rest.strip_prefix(m).map(|c| ((*m).to_owned(), c)))
    {
        c
    } else {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let after = rest[digits.len()..].strip_prefix(". ").or_else(|| rest[digits.len()..].strip_prefix(") "))?;
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
        assert_eq!(matching("list").len(), 3, "bulleted, numbered and task (checklist)");
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
            assert!(c.select.0 <= c.select.1 && c.select.1 <= c.insert.len(), "{}", c.title);
            assert!(c.insert.is_char_boundary(c.select.0) && c.insert.is_char_boundary(c.select.1));
        }
        assert_eq!(&COMMANDS[9].insert[1..5], "text");
        assert_eq!(&COMMANDS[11].insert[22..29], "A --> B");
        assert_eq!(&COMMANDS[13].insert[11..19], "Untitled");
    }
}
