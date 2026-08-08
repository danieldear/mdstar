use mdstar_core::expand_emoji_text;

/// Enrich prose while leaving literal Markdown regions untouched.
///
/// mdansi intentionally focuses on CommonMark presentation. This small pass
/// adds the richer text behavior shared with the HTML renderer without
/// rewriting fenced code, inline code, HTML/autolinks, or link destinations.
pub(crate) fn enrich_markdown(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut fence: Option<(char, usize)> = None;
    let mut display_math: Option<(String, &'static str)> = None;

    for raw_line in source.split_inclusive('\n') {
        let line = raw_line.trim_end_matches('\n');
        let newline = if raw_line.ends_with('\n') { "\n" } else { "" };

        if let Some((body, closing)) = display_math.as_mut() {
            if line.trim() == *closing {
                output.push_str(&render_display_math(body));
                output.push_str(newline);
                display_math = None;
            } else {
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(line);
            }
            continue;
        }

        if let Some((marker, width)) = fence {
            output.push_str(raw_line);
            if is_fence_close(line, marker, width) {
                fence = None;
            }
            continue;
        }

        if let Some((marker, width)) = fence_start(line) {
            fence = Some((marker, width));
            output.push_str(raw_line);
            continue;
        }

        match line.trim() {
            "$$" => {
                display_math = Some((String::new(), "$$"));
                continue;
            }
            r"\[" => {
                display_math = Some((String::new(), r"\]"));
                continue;
            }
            _ => {}
        }

        output.push_str(&enrich_inline_line(line));
        output.push_str(newline);
    }

    if let Some((body, closing)) = display_math {
        // Malformed source should remain inspectable instead of silently
        // losing its opening delimiter.
        let opening = if closing == "$$" { "$$" } else { r"\[" };
        output.push_str(opening);
        output.push('\n');
        output.push_str(&body);
    }

    output
}

fn fence_start(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let width = trimmed
        .chars()
        .take_while(|character| *character == marker)
        .count();
    (width >= 3).then_some((marker, width))
}

fn is_fence_close(line: &str, marker: char, width: usize) -> bool {
    let trimmed = line.trim_start();
    let count = trimmed
        .chars()
        .take_while(|character| *character == marker)
        .count();
    count >= width && trimmed[count..].trim().is_empty()
}

fn enrich_inline_line(line: &str) -> String {
    if let Some(reference_end) = reference_definition_prefix_end(line) {
        let mut output = enrich_inline_fragment(&line[..reference_end]);
        output.push_str(&line[reference_end..]);
        return output;
    }
    enrich_inline_fragment(line)
}

fn reference_definition_prefix_end(line: &str) -> Option<usize> {
    let trimmed_start = line.len() - line.trim_start().len();
    let trimmed = &line[trimmed_start..];
    if !trimmed.starts_with('[') || trimmed.starts_with("[^") {
        return None;
    }
    trimmed.find("]:").map(|index| trimmed_start + index + 2)
}

fn enrich_inline_fragment(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut plain_start = 0;
    let mut index = 0;

    while index < input.len() {
        if input.as_bytes()[index] == b'`' {
            let width = input[index..]
                .bytes()
                .take_while(|byte| *byte == b'`')
                .count();
            if let Some(end) = find_code_span_end(input, index + width, width) {
                flush_prose(&mut output, &input[plain_start..index]);
                output.push_str(&input[index..end]);
                index = end;
                plain_start = end;
                continue;
            }
        }

        if input[index..].starts_with("](")
            && let Some(end) = find_link_destination_end(input, index + 1)
        {
            flush_prose(&mut output, &input[plain_start..index + 1]);
            output.push_str(&input[index + 1..end]);
            index = end;
            plain_start = end;
            continue;
        }

        if let Some(prefix_len) = url_prefix_len(&input[index..]) {
            flush_prose(&mut output, &input[plain_start..index]);
            let end = find_url_end(input, index + prefix_len);
            output.push_str(&input[index..end]);
            index = end;
            plain_start = end;
            continue;
        }

        if input.as_bytes()[index] == b'<'
            && let Some(relative_end) = input[index + 1..].find('>')
        {
            let end = index + 1 + relative_end + 1;
            flush_prose(&mut output, &input[plain_start..index]);
            output.push_str(&input[index..end]);
            index = end;
            plain_start = end;
            continue;
        }

        if input[index..].starts_with(r"\(")
            && !is_escaped(input, index)
            && let Some(relative_end) = input[index + 2..].find(r"\)")
        {
            let math_end = index + 2 + relative_end;
            flush_prose(&mut output, &input[plain_start..index]);
            output.push_str(&format_latex_for_terminal(&input[index + 2..math_end]));
            index = math_end + 2;
            plain_start = index;
            continue;
        }

        if input.as_bytes()[index] == b'$' && !is_escaped(input, index) {
            let width = if input[index..].starts_with("$$") {
                2
            } else {
                1
            };
            if let Some(end) = find_math_end(input, index + width, width) {
                if width == 1 && !valid_inline_math_boundaries(input, index + width, end - width) {
                    index += width;
                    continue;
                }
                flush_prose(&mut output, &input[plain_start..index]);
                output.push_str(&format_latex_for_terminal(
                    &input[index + width..end - width],
                ));
                index = end;
                plain_start = end;
                continue;
            }
        }

        index += input[index..]
            .chars()
            .next()
            .expect("index is a character boundary")
            .len_utf8();
    }

    flush_prose(&mut output, &input[plain_start..]);
    output
}

fn flush_prose(output: &mut String, prose: &str) {
    output.push_str(&expand_emoji_text(prose));
}

fn find_code_span_end(input: &str, mut index: usize, width: usize) -> Option<usize> {
    while index < input.len() {
        if input.as_bytes()[index] == b'`' {
            let count = input[index..]
                .bytes()
                .take_while(|byte| *byte == b'`')
                .count();
            if count == width {
                return Some(index + width);
            }
            index += count;
        } else {
            index += input[index..].chars().next()?.len_utf8();
        }
    }
    None
}

fn find_link_destination_end(input: &str, start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = start;
    while index < input.len() {
        let character = input[index..].chars().next()?;
        if character == '\\' {
            index += character.len_utf8();
            if index < input.len() {
                index += input[index..].chars().next()?.len_utf8();
            }
            continue;
        }
        match character {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
        index += character.len_utf8();
    }
    None
}

fn url_prefix_len(input: &str) -> Option<usize> {
    ["https://", "http://", "mailto:"]
        .into_iter()
        .find(|prefix| input.starts_with(prefix))
        .map(str::len)
}

fn find_url_end(input: &str, mut index: usize) -> usize {
    while index < input.len() {
        let character = input[index..]
            .chars()
            .next()
            .expect("index is a character boundary");
        if character.is_whitespace() || character == '>' {
            break;
        }
        index += character.len_utf8();
    }
    index
}

fn find_math_end(input: &str, mut index: usize, width: usize) -> Option<usize> {
    let delimiter = if width == 2 { "$$" } else { "$" };
    while index < input.len() {
        let relative = input[index..].find(delimiter)?;
        let candidate = index + relative;
        if !is_escaped(input, candidate) {
            return Some(candidate + width);
        }
        index = candidate + width;
    }
    None
}

fn valid_inline_math_boundaries(input: &str, content_start: usize, content_end: usize) -> bool {
    content_start < content_end
        && input[content_start..]
            .chars()
            .next()
            .is_some_and(|character| !character.is_whitespace())
        && input[..content_end]
            .chars()
            .next_back()
            .is_some_and(|character| !character.is_whitespace())
}

fn is_escaped(input: &str, index: usize) -> bool {
    let backslashes = input[..index]
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'\\')
        .count();
    backslashes % 2 == 1
}

fn render_display_math(latex: &str) -> String {
    let rendered = format_latex_for_terminal(latex);
    format!("\n{rendered}\n")
}

pub(crate) fn format_latex_for_terminal(latex: &str) -> String {
    LatexFormatter::new(latex).format().trim().to_string()
}

struct LatexFormatter {
    characters: Vec<char>,
    index: usize,
}

impl LatexFormatter {
    fn new(input: &str) -> Self {
        Self {
            characters: input.chars().collect(),
            index: 0,
        }
    }

    fn format(mut self) -> String {
        self.parse_until(None)
    }

    fn parse_until(&mut self, terminator: Option<char>) -> String {
        let mut output = String::new();
        while let Some(character) = self.peek() {
            if Some(character) == terminator {
                self.index += 1;
                break;
            }
            match character {
                '\\' => output.push_str(&self.parse_command()),
                '^' => {
                    self.index += 1;
                    let script = self.parse_script();
                    output.push_str(&script_notation(&script, true));
                }
                '_' => {
                    self.index += 1;
                    let script = self.parse_script();
                    output.push_str(&script_notation(&script, false));
                }
                '{' => {
                    self.index += 1;
                    output.push_str(&self.parse_until(Some('}')));
                }
                '-' => {
                    self.index += 1;
                    output.push('−');
                }
                _ => {
                    self.index += 1;
                    output.push(character);
                }
            }
        }
        output
    }

    fn parse_command(&mut self) -> String {
        self.index += 1;
        let start = self.index;
        while self.peek().is_some_and(char::is_alphabetic) {
            self.index += 1;
        }

        if start == self.index {
            return match self.next() {
                Some(',' | ';' | ':' | ' ') => " ".to_string(),
                Some('!') => String::new(),
                Some(character) => character.to_string(),
                None => "\\".to_string(),
            };
        }

        let command: String = self.characters[start..self.index].iter().collect();
        match command.as_str() {
            "frac" | "dfrac" | "tfrac" => {
                let numerator = self.parse_required_group();
                let denominator = self.parse_required_group();
                format!("({numerator})⁄({denominator})")
            }
            "sqrt" => {
                let value = self.parse_required_group();
                if value.chars().count() == 1 {
                    format!("√{value}")
                } else {
                    format!("√({value})")
                }
            }
            "text" | "textrm" | "mathrm" | "mathbf" | "operatorname" => self.parse_required_group(),
            "left" | "right" => String::new(),
            "quad" | "qquad" => " ".to_string(),
            "pm" => "±".to_string(),
            "mp" => "∓".to_string(),
            "times" => "×".to_string(),
            "cdot" => "·".to_string(),
            "div" => "÷".to_string(),
            "le" | "leq" => "≤".to_string(),
            "ge" | "geq" => "≥".to_string(),
            "ne" | "neq" => "≠".to_string(),
            "approx" => "≈".to_string(),
            "to" | "rightarrow" => "→".to_string(),
            "leftarrow" => "←".to_string(),
            "infty" => "∞".to_string(),
            "sum" => "∑".to_string(),
            "prod" => "∏".to_string(),
            "int" => "∫".to_string(),
            "partial" => "∂".to_string(),
            "nabla" => "∇".to_string(),
            "alpha" => "α".to_string(),
            "beta" => "β".to_string(),
            "gamma" => "γ".to_string(),
            "delta" => "δ".to_string(),
            "epsilon" => "ε".to_string(),
            "theta" => "θ".to_string(),
            "lambda" => "λ".to_string(),
            "mu" => "μ".to_string(),
            "pi" => "π".to_string(),
            "rho" => "ρ".to_string(),
            "sigma" => "σ".to_string(),
            "phi" => "φ".to_string(),
            "omega" => "ω".to_string(),
            "Gamma" => "Γ".to_string(),
            "Delta" => "Δ".to_string(),
            "Theta" => "Θ".to_string(),
            "Lambda" => "Λ".to_string(),
            "Pi" => "Π".to_string(),
            "Sigma" => "Σ".to_string(),
            "Phi" => "Φ".to_string(),
            "Omega" => "Ω".to_string(),
            "lim" => "lim".to_string(),
            _ => format!("\\{command}"),
        }
    }

    fn parse_required_group(&mut self) -> String {
        self.skip_whitespace();
        if self.peek() == Some('{') {
            self.index += 1;
            self.parse_until(Some('}'))
        } else {
            self.next()
                .map_or_else(String::new, |value| value.to_string())
        }
    }

    fn parse_script(&mut self) -> String {
        self.skip_whitespace();
        if self.peek() == Some('{') {
            self.index += 1;
            self.parse_until(Some('}'))
        } else if self.peek() == Some('\\') {
            self.parse_command()
        } else {
            self.next()
                .map_or_else(String::new, |value| value.to_string())
        }
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.index += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.characters.get(self.index).copied()
    }

    fn next(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.index += 1;
        Some(character)
    }
}

fn script_notation(script: &str, superscript: bool) -> String {
    let converted: Option<String> = script
        .chars()
        .map(|character| {
            if superscript {
                superscript_character(character)
            } else {
                subscript_character(character)
            }
        })
        .collect();

    converted.unwrap_or_else(|| {
        let marker = if superscript { '^' } else { '_' };
        format!("{marker}({script})")
    })
}

fn superscript_character(character: char) -> Option<char> {
    Some(match character {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '−' | '-' => '⁻',
        '(' => '⁽',
        ')' => '⁾',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        _ => return None,
    })
}

fn subscript_character(character: char) -> Option<char> {
    Some(match character {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '−' | '-' => '₋',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'i' => 'ᵢ',
        'o' => 'ₒ',
        'r' => 'ᵣ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_code_urls_and_link_destinations() {
        let source = r#"Text :rocket: [label :wink:](https://example.com/:cry:) https://example.com/:yum:

`:wink: $E = mc^2$`

```text
:cry: \(x = \frac{1}{2}\)
```
"#;

        let enriched = enrich_markdown(source);
        assert!(enriched.contains("Text 🚀 [label 😉](https://example.com/:cry:)"));
        assert!(enriched.contains("https://example.com/:yum:"));
        assert!(enriched.contains("`:wink: $E = mc^2$`"));
        assert!(enriched.contains(":cry: \\(x = \\frac{1}{2}\\)"));
    }

    #[test]
    fn formats_inline_and_display_math() {
        let source = "Inline \\(E = mc^2\\)\n\n$$\nf(x) = \\frac{1}{\\sqrt{2\\pi\\sigma^2}}\n$$\n";
        let enriched = enrich_markdown(source);
        assert!(enriched.contains("E = mc²"), "got: {enriched}");
        assert!(enriched.contains("(1)⁄(√(2πσ²))"), "got: {enriched}");
        assert!(!enriched.contains(r"\frac"), "got: {enriched}");
    }

    #[test]
    fn does_not_treat_currency_as_inline_math() {
        let source = "The price moved from $5 to $10 while $E = mc^2$ stayed valid.";
        let enriched = enrich_markdown(source);
        assert!(enriched.contains("$5 to $10"), "got: {enriched}");
        assert!(enriched.contains("E = mc²"), "got: {enriched}");
    }
}
