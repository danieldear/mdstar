//! A small syntax highlighter: comments, strings, numbers, keywords, types,
//! keys and markup tags for the languages the reader shows. It colours by
//! token shape rather than parsing, which is what a reader needs.

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Comment,
    String,
    Number,
    Keyword,
    Type,
    /// Object keys, properties and markup tag names.
    Key,
    /// Markup attributes and section headers.
    Attribute,
    Inserted,
    Deleted,
}

/// Token colours for light and dark themes.
pub fn color(token: Token, dark: bool) -> &'static str {
    match (token, dark) {
        (Token::Comment, false) => "#6A737D",
        (Token::Comment, true) => "#8B949E",
        (Token::String, false) => "#B4321C",
        (Token::String, true) => "#FF9E85",
        (Token::Number, false) => "#2C35C9",
        (Token::Number, true) => "#B6A2FF",
        (Token::Keyword, false) => "#A32682",
        (Token::Keyword, true) => "#FF7AB8",
        (Token::Type, false) => "#0B6E86",
        (Token::Type, true) => "#63D3E6",
        (Token::Key, false) => "#0550AE",
        (Token::Key, true) => "#79B8FF",
        (Token::Attribute, false) => "#8A5300",
        (Token::Attribute, true) => "#FFCB6B",
        (Token::Inserted, false) => "#1A7F37",
        (Token::Inserted, true) => "#7EE787",
        (Token::Deleted, false) => "#CF222E",
        (Token::Deleted, true) => "#FFA198",
    }
}

struct Lang {
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    quotes: &'static str,
    keywords: &'static [&'static str],
    extra: &'static [&'static str],
    /// Capitalised identifiers are types.
    capital_types: bool,
    case_insensitive: bool,
}

const C_LIKE: [&str; 64] = [
    "as", "async", "await", "break", "case", "catch", "class", "const", "continue", "crate",
    "default", "defer", "delete", "do", "dyn", "else", "enum", "export", "extends", "extern",
    "false", "final", "finally", "fn", "for", "func", "function", "go", "guard", "if", "impl",
    "implements", "import", "in", "interface", "let", "loop", "match", "mod", "move", "mut", "new",
    "nil", "null", "package", "private", "protected", "pub", "public", "return", "self", "Self",
    "static", "struct", "super", "switch", "this", "throw", "trait", "true", "try", "type", "use",
    "var",
];
const MORE_C: [&str; 18] = [
    "void", "while", "where", "yield", "unsafe", "typeof", "instanceof", "undefined", "of", "from",
    "override", "virtual", "namespace", "using", "template", "typename", "val", "when",
];
const SCRIPT: [&str; 44] = [
    "and", "as", "assert", "async", "await", "break", "case", "class", "continue", "def", "del",
    "do", "done", "elif", "else", "end", "esac", "except", "export", "False", "fi", "finally",
    "for", "from", "function", "if", "import", "in", "is", "lambda", "local", "None", "not", "or",
    "pass", "raise", "return", "then", "True", "try", "while", "with", "yield", "nil",
];
const SQL: [&str; 40] = [
    "select", "from", "where", "insert", "into", "values", "update", "set", "delete", "create",
    "table", "drop", "alter", "add", "join", "left", "right", "inner", "outer", "on", "and", "or",
    "not", "null", "is", "as", "order", "by", "group", "having", "limit", "offset", "primary",
    "key", "index", "distinct", "union", "case", "when", "then",
];
const DATA: [&str; 6] = ["true", "false", "null", "yes", "no", "nil"];

fn lang(language: &str) -> Option<Lang> {
    let c_like = |capital_types| Lang {
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        quotes: "\"'`",
        keywords: &C_LIKE,
        extra: &MORE_C,
        capital_types,
        case_insensitive: false,
    };
    Some(match language.to_ascii_lowercase().as_str() {
        "rust" | "rs" | "swift" | "kotlin" | "kt" | "java" | "go" | "c" | "cpp" | "c++" | "h"
        | "objc" | "csharp" | "cs" | "scala" | "dart" | "zig" => c_like(true),
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx" | "php" => c_like(true),
        "css" | "scss" | "less" => Lang {
            line_comments: &[],
            block_comment: Some(("/*", "*/")),
            quotes: "\"'",
            keywords: &[],
            extra: &[],
            capital_types: false,
            case_insensitive: false,
        },
        "python" | "py" | "ruby" | "rb" | "shell" | "sh" | "bash" | "zsh" | "fish" | "console"
        | "dockerfile" | "makefile" | "perl" | "r" | "elixir" => Lang {
            line_comments: &["#"],
            block_comment: None,
            quotes: "\"'`",
            keywords: &SCRIPT,
            extra: &[],
            capital_types: false,
            case_insensitive: false,
        },
        "lua" => Lang {
            line_comments: &["--"],
            block_comment: Some(("--[[", "]]")),
            quotes: "\"'",
            keywords: &SCRIPT,
            extra: &[],
            capital_types: false,
            case_insensitive: false,
        },
        "sql" => Lang {
            line_comments: &["--"],
            block_comment: Some(("/*", "*/")),
            quotes: "'\"",
            keywords: &SQL,
            extra: &[],
            capital_types: false,
            case_insensitive: true,
        },
        "yaml" | "yml" | "toml" | "ini" | "conf" | "properties" | "env" => Lang {
            line_comments: &["#", ";"],
            block_comment: None,
            quotes: "\"'",
            keywords: &DATA,
            extra: &[],
            capital_types: false,
            case_insensitive: false,
        },
        "json" | "jsonc" | "json5" => Lang {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            quotes: "\"",
            keywords: &DATA,
            extra: &[],
            capital_types: false,
            case_insensitive: false,
        },
        _ => return None,
    })
}

/// Coloured byte ranges of `code` in `language`, in order, at most `max`.
pub fn tokens(language: &str, code: &str, max: usize) -> Vec<(Range<usize>, Token)> {
    let lower = language.to_ascii_lowercase();
    let mut out = match lower.as_str() {
        "xml" | "html" | "htm" | "svg" | "plist" | "xhtml" | "vue" => markup(code),
        "diff" | "patch" => diff(code),
        "csv" | "tsv" | "text" | "txt" | "plain" | "markdown" | "md" => Vec::new(),
        _ => match lang(&lower) {
            Some(l) => generic(code, &l, &lower),
            None => Vec::new(),
        },
    };
    out.truncate(max);
    out
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn generic(code: &str, l: &Lang, language: &str) -> Vec<(Range<usize>, Token)> {
    let keys = matches!(language, "json" | "jsonc" | "json5" | "yaml" | "yml" | "toml" | "ini" | "conf" | "properties" | "env");
    let sections = matches!(language, "toml" | "ini" | "conf");
    let bytes = code.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line_start = true;
    while i < code.len() {
        let rest = &code[i..];
        let c = rest.chars().next().unwrap();
        if c == '\n' {
            line_start = true;
            i += 1;
            continue;
        }
        if let Some((open, close)) = l.block_comment
            && rest.starts_with(open)
        {
            let end = rest[open.len()..]
                .find(close)
                .map_or(code.len(), |e| i + open.len() + e + close.len());
            out.push((i..end, Token::Comment));
            i = end;
            continue;
        }
        if l.line_comments.iter().any(|p| rest.starts_with(p))
            // `#` starts a comment only at a word boundary (not `a#b`).
            && (!rest.starts_with('#') || i == 0 || !is_ident(code[..i].chars().last().unwrap_or(' ')))
            // `;` is a comment only at the start of an INI line.
            && (!rest.starts_with(';') || line_start)
        {
            let end = rest.find('\n').map_or(code.len(), |e| i + e);
            out.push((i..end, Token::Comment));
            i = end;
            continue;
        }
        if sections && line_start && c == '[' {
            let end = rest.find('\n').map_or(code.len(), |e| i + e);
            out.push((i..end, Token::Attribute));
            i = end;
            continue;
        }
        // Rust lifetimes (`'a`, `'static`) are not character literals.
        if c == '\'' && matches!(language, "rust" | "rs") {
            let after = &rest[1..];
            let literal = after.starts_with('\\')
                || after.char_indices().nth(1).is_some_and(|(_, d)| d == '\'');
            if !literal {
                let len = 1 + after.find(|d: char| !is_ident(d)).unwrap_or(after.len());
                out.push((i..i + len, Token::Keyword));
                i += len;
                line_start = false;
                continue;
            }
        }
        if l.quotes.contains(c) {
            let mut j = i + c.len_utf8();
            while j < code.len() {
                let d = code[j..].chars().next().unwrap();
                if d == '\\' {
                    j += 1 + code[j + 1..].chars().next().map_or(0, char::len_utf8);
                    continue;
                }
                j += d.len_utf8();
                if d == c || (d == '\n' && c != '`') {
                    break;
                }
            }
            let j = j.min(code.len());
            // A JSON string followed by `:` is a key.
            let token = if keys && code[j..].trim_start_matches([' ', '\t']).starts_with(':') {
                Token::Key
            } else {
                Token::String
            };
            out.push((i..j, token));
            i = j;
            line_start = false;
            continue;
        }
        if c.is_ascii_digit() && (i == 0 || !is_ident(code[..i].chars().last().unwrap_or(' '))) {
            let mut j = i;
            while j < code.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'.' || bytes[j] == b'_') {
                j += 1;
            }
            out.push((i..j, Token::Number));
            i = j;
            line_start = false;
            continue;
        }
        if is_ident(c) {
            let mut j = i;
            for d in rest.chars() {
                if !(is_ident(d) || (keys && d == '-')) {
                    break;
                }
                j += d.len_utf8();
            }
            let word = &code[i..j];
            let after = code[j..].trim_start_matches([' ', '\t']);
            let token = if keys && line_start && (after.starts_with(':') || after.starts_with('=')) {
                Some(Token::Key)
            } else if keyword(l, word) {
                Some(Token::Keyword)
            } else if l.capital_types && word.chars().next().is_some_and(char::is_uppercase) {
                Some(Token::Type)
            } else {
                None
            };
            if let Some(token) = token {
                out.push((i..j, token));
            }
            i = j;
            line_start = false;
            continue;
        }
        if !c.is_whitespace() && c != '-' {
            line_start = false;
        }
        i += c.len_utf8();
    }
    out
}

fn keyword(l: &Lang, word: &str) -> bool {
    if l.case_insensitive {
        let lower = word.to_ascii_lowercase();
        l.keywords.contains(&lower.as_str())
    } else {
        l.keywords.contains(&word) || l.extra.contains(&word)
    }
}

/// Tags, attribute names, attribute values and comments.
fn markup(code: &str) -> Vec<(Range<usize>, Token)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(lt) = code[i..].find('<') {
        let start = i + lt;
        let rest = &code[start..];
        if rest.starts_with("<!--") {
            let end = rest.find("-->").map_or(code.len(), |e| start + e + 3);
            out.push((start..end, Token::Comment));
            i = end;
            continue;
        }
        let end = rest.find('>').map_or(code.len(), |e| start + e + 1);
        let tag = &code[start..end];
        // The tag name, after `<`, `</` or `<?`.
        let name_start = start + 1 + tag[1..].find(|c: char| c.is_alphanumeric()).unwrap_or(0);
        let name_end = code[name_start..end]
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .map_or(end, |e| name_start + e);
        if name_end > name_start {
            out.push((name_start..name_end, Token::Key));
        }
        let mut j = name_end;
        while j < end {
            let c = code[j..].chars().next().unwrap();
            if c == '"' || c == '\'' {
                let close = code[j + 1..end].find(c).map_or(end, |e| j + 1 + e + 1);
                out.push((j..close, Token::String));
                j = close;
            } else if is_ident(c) {
                let stop = code[j..end]
                    .find(|d: char| !(is_ident(d) || d == '-' || d == ':'))
                    .map_or(end, |e| j + e);
                out.push((j..stop, Token::Attribute));
                j = stop;
            } else {
                j += c.len_utf8();
            }
        }
        i = end;
    }
    out
}

fn diff(code: &str) -> Vec<(Range<usize>, Token)> {
    let mut out = Vec::new();
    let mut at = 0;
    for line in code.split_inclusive('\n') {
        let end = at + line.trim_end_matches('\n').len();
        let token = if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
            Some(Token::Attribute)
        } else if line.starts_with('+') {
            Some(Token::Inserted)
        } else if line.starts_with('-') {
            Some(Token::Deleted)
        } else {
            None
        };
        if let Some(token) = token
            && end > at
        {
            out.push((at..end, token));
        }
        at += line.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans<'a>(language: &str, code: &'a str) -> Vec<(&'a str, Token)> {
        tokens(language, code, 1000)
            .into_iter()
            .map(|(r, t)| (&code[r], t))
            .collect()
    }

    #[test]
    fn rust_comments_strings_keywords_types_and_numbers() {
        let got = spans("rust", "// hi\nlet x: Vec<u8> = \"a\\\"b\"; 42 /* c */");
        assert_eq!(
            got,
            [
                ("// hi", Token::Comment),
                ("let", Token::Keyword),
                ("Vec", Token::Type),
                ("\"a\\\"b\"", Token::String),
                ("42", Token::Number),
                ("/* c */", Token::Comment),
            ]
        );
    }

    #[test]
    fn rust_lifetimes_are_not_strings() {
        assert_eq!(
            spans("rust", "fn f<'a>(x: &'static str) -> char { 'x' }"),
            [
                ("fn", Token::Keyword),
                ("'a", Token::Keyword),
                ("'static", Token::Keyword),
                ("'x'", Token::String),
            ]
        );
    }

    #[test]
    fn json_keys_differ_from_string_values() {
        let got = spans("json", r#"{"name": "volt", "n": 1.5, "ok": true}"#);
        assert_eq!(
            got,
            [
                ("\"name\"", Token::Key),
                ("\"volt\"", Token::String),
                ("\"n\"", Token::Key),
                ("1.5", Token::Number),
                ("\"ok\"", Token::Key),
                ("true", Token::Keyword),
            ]
        );
    }

    #[test]
    fn yaml_toml_shell_markup_and_diff() {
        assert_eq!(
            spans("yaml", "name: volt # note\nlist:\n  - 3"),
            [
                ("name", Token::Key),
                ("# note", Token::Comment),
                ("list", Token::Key),
                ("3", Token::Number)
            ]
        );
        assert_eq!(
            spans("toml", "[package]\nname = \"x\""),
            [
                ("[package]", Token::Attribute),
                ("name", Token::Key),
                ("\"x\"", Token::String)
            ]
        );
        assert_eq!(
            spans("shell", "echo a#b # c"),
            [("# c", Token::Comment)]
        );
        assert_eq!(
            spans("xml", "<a href=\"x\"><!-- c --></a>"),
            [
                ("a", Token::Key),
                ("href", Token::Attribute),
                ("\"x\"", Token::String),
                ("<!-- c -->", Token::Comment),
                ("a", Token::Key),
            ]
        );
        assert_eq!(
            spans("diff", "@@ -1 +1 @@\n-old\n+new\n same"),
            [
                ("@@ -1 +1 @@", Token::Attribute),
                ("-old", Token::Deleted),
                ("+new", Token::Inserted)
            ]
        );
        assert!(spans("brainfuck", "+-.").is_empty());
    }
}
