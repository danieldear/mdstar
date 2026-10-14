//! LaTeX math to readable Unicode.

/// LaTeX math as plain Unicode text: Greek letters, operators, super- and
/// subscripts where Unicode has them, fractions as (a)⁄(b). Good enough to
/// read math in a terminal or a text label; not a typesetter.
pub fn to_unicode(latex: &str) -> String {
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
        '=' => '⁼',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'i' => 'ⁱ',
        'j' => 'ʲ',
        'k' => 'ᵏ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'n' => 'ⁿ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'z' => 'ᶻ',
        'T' => 'ᵀ',
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
        '=' => '₌',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::to_unicode;

    #[test]
    fn scripts_fractions_and_symbols() {
        assert_eq!(to_unicode("E = mc^2"), "E = mc²");
        assert_eq!(
            to_unicode(r"\frac{1}{\sqrt{2\pi\sigma^2}}"),
            "(1)⁄(√(2πσ²))"
        );
        assert_eq!(to_unicode(r"\sum_{i=1}^{n} x_i^2"), "∑ᵢ₌₁ⁿ xᵢ²");
        assert_eq!(to_unicode(r"x_{max}^{T}"), "xₘₐₓᵀ");
        assert_eq!(
            to_unicode(r"y_{b}"),
            "y_(b)",
            "scripts Unicode lacks stay readable"
        );
    }
}
