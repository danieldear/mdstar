//! Lightweight syntax highlighting for fenced code.
//!
//! Deliberately approximate: one pass tags comments, strings, numbers and a
//! shared keyword set, which is enough to make code scannable without bundling
//! a grammar engine. Emitting the spans here rather than in a frontend keeps
//! every HTML consumer coloured the same way.

#[derive(Clone, Copy, PartialEq, Eq)]
enum Token {
    Comment,
    Str,
    Number,
    Keyword,
}

impl Token {
    fn class(self) -> &'static str {
        match self {
            Token::Comment => "tok-comment",
            Token::Str => "tok-string",
            Token::Number => "tok-number",
            Token::Keyword => "tok-keyword",
        }
    }
}

/// Escaped HTML for `code`, with recognised spans wrapped for styling.
pub fn highlight_code(code: &str, language: Option<&str>) -> String {
    let chars: Vec<char> = code.chars().collect();
    let spans = scan(&chars, language);

    let mut out = String::with_capacity(code.len() + spans.len() * 24);
    let mut index = 0;
    let mut span_index = 0;

    while index < chars.len() {
        if span_index < spans.len() && spans[span_index].0 == index {
            let (start, end, token) = spans[span_index];
            out.push_str("<span class=\"");
            out.push_str(token.class());
            out.push_str("\">");
            for ch in &chars[start..end.min(chars.len())] {
                push_escaped(*ch, &mut out);
            }
            out.push_str("</span>");
            index = end;
            span_index += 1;
            continue;
        }
        push_escaped(chars[index], &mut out);
        index += 1;
    }

    out
}

fn scan(chars: &[char], language: Option<&str>) -> Vec<(usize, usize, Token)> {
    let line_tokens = line_comment_tokens(language);
    let block_comments = uses_c_block_comments(language);
    let mut spans = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];

        if let Some(token) = line_tokens
            .iter()
            .find(|token| matches(token, chars, index))
        {
            let start = index;
            index += token.chars().count();
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            spans.push((start, index, Token::Comment));
            continue;
        }

        if block_comments && matches("/*", chars, index) {
            let start = index;
            index += 2;
            while index < chars.len() && !matches("*/", chars, index) {
                index += 1;
            }
            index = (index + 2).min(chars.len());
            spans.push((start, index, Token::Comment));
            continue;
        }

        if ch == '"' || ch == '\'' || ch == '`' {
            let quote = ch;
            let start = index;
            index += 1;
            while index < chars.len() {
                if chars[index] == '\\' {
                    index += 2;
                    continue;
                }
                if chars[index] == quote {
                    index += 1;
                    break;
                }
                if chars[index] == '\n' {
                    break;
                }
                index += 1;
            }
            spans.push((start, index.min(chars.len()), Token::Str));
            continue;
        }

        if ch.is_ascii_digit() {
            let start = index;
            while index < chars.len() && is_number_char(chars[index]) {
                index += 1;
            }
            spans.push((start, index, Token::Number));
            continue;
        }

        if ch.is_alphabetic() || ch == '_' {
            let start = index;
            while index < chars.len() && (chars[index].is_alphanumeric() || chars[index] == '_') {
                index += 1;
            }
            let word: String = chars[start..index].iter().collect();
            if KEYWORDS.contains(&word.as_str()) {
                spans.push((start, index, Token::Keyword));
            }
            continue;
        }

        index += 1;
    }

    spans
}

fn matches(token: &str, chars: &[char], index: usize) -> bool {
    let token: Vec<char> = token.chars().collect();
    if index + token.len() > chars.len() {
        return false;
    }
    (0..token.len()).all(|offset| chars[index + offset] == token[offset])
}

fn is_number_char(ch: char) -> bool {
    ch.is_ascii_hexdigit() || ch == '.' || ch == '_' || ch == 'x'
}

fn line_comment_tokens(language: Option<&str>) -> &'static [&'static str] {
    match language.unwrap_or_default().to_ascii_lowercase().as_str() {
        "python" | "py" | "ruby" | "rb" | "sh" | "bash" | "shell" | "zsh" | "yaml" | "yml"
        | "toml" | "r" | "perl" | "makefile" | "dockerfile" => &["#"],
        "sql" | "lua" | "haskell" | "hs" | "elm" => &["--"],
        "lisp" | "clojure" | "clj" | "scheme" | "ini" => &[";"],
        _ => &["//"],
    }
}

fn uses_c_block_comments(language: Option<&str>) -> bool {
    !matches!(
        language.unwrap_or_default().to_ascii_lowercase().as_str(),
        "python"
            | "py"
            | "ruby"
            | "rb"
            | "sh"
            | "bash"
            | "shell"
            | "yaml"
            | "yml"
            | "toml"
            | "lisp"
            | "clojure"
    )
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

const KEYWORDS: &[&str] = &[
    "fn",
    "func",
    "function",
    "def",
    "let",
    "var",
    "const",
    "val",
    "mut",
    "static",
    "class",
    "struct",
    "enum",
    "protocol",
    "interface",
    "trait",
    "impl",
    "extension",
    "public",
    "private",
    "protected",
    "internal",
    "pub",
    "package",
    "module",
    "namespace",
    "import",
    "from",
    "use",
    "using",
    "include",
    "require",
    "extends",
    "implements",
    "return",
    "if",
    "else",
    "elif",
    "for",
    "while",
    "loop",
    "match",
    "switch",
    "case",
    "default",
    "break",
    "continue",
    "do",
    "try",
    "catch",
    "throw",
    "throws",
    "finally",
    "async",
    "await",
    "yield",
    "defer",
    "guard",
    "in",
    "is",
    "as",
    "where",
    "true",
    "false",
    "nil",
    "null",
    "none",
    "self",
    "this",
    "super",
    "new",
    "delete",
    "type",
    "typedef",
    "typealias",
    "void",
    "int",
    "float",
    "double",
    "bool",
    "string",
    "and",
    "or",
    "not",
    "with",
    "lambda",
    "then",
    "end",
    "begin",
    "override",
    "final",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_comments_strings_numbers_and_keywords() {
        let html = highlight_code("// note\nlet x = \"s\" + 42;", Some("rust"));
        assert!(html.contains("tok-comment"), "got: {html}");
        assert!(html.contains("tok-string"), "got: {html}");
        assert!(html.contains("tok-number"), "got: {html}");
        assert!(html.contains("tok-keyword"), "got: {html}");
    }

    #[test]
    fn honours_per_language_comment_syntax() {
        let python = highlight_code("# a comment", Some("python"));
        assert!(python.contains("tok-comment"), "got: {python}");
        // '#' is not a comment in Rust, so it must not be tagged as one.
        let rust = highlight_code("# not a comment", Some("rust"));
        assert!(!rust.contains("tok-comment"), "got: {rust}");
    }

    #[test]
    fn escapes_markup_inside_code() {
        let html = highlight_code("let x = \"<script>\";", Some("rust"));
        assert!(!html.contains("<script>"), "got: {html}");
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn plain_text_is_untouched_but_escaped() {
        let html = highlight_code("a < b && c > d", None);
        assert!(html.contains("&lt;"));
        assert!(html.contains("&amp;&amp;"));
    }
}
