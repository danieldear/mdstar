//! Find in document: case-insensitive matches over the rendered text.

use std::ops::Range;

/// Matches stop being counted past this, so a one-letter query in a huge
/// document stays fast; the count then reads "1000+".
pub const MAX_HITS: usize = 1000;

/// A match: the document row and its position among that row's matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub row: usize,
    pub nth: usize,
}

#[derive(Default)]
pub struct Find {
    pub query: String,
    pub hits: Vec<Hit>,
    /// Index into `hits` of the current match.
    pub current: usize,
}

impl Find {
    pub fn current_hit(&self) -> Option<Hit> {
        self.hits.get(self.current).copied()
    }

    /// "3 of 12", "No matches", or nothing before a query is typed.
    pub fn summary(&self) -> String {
        if self.query.trim().is_empty() {
            String::new()
        } else if self.hits.is_empty() {
            "No matches".into()
        } else if self.hits.len() >= MAX_HITS {
            format!("{} of {}+", self.current + 1, MAX_HITS)
        } else {
            format!("{} of {}", self.current + 1, self.hits.len())
        }
    }

    pub fn step(&mut self, forward: bool) {
        let n = self.hits.len();
        if n > 0 {
            self.current = if forward {
                (self.current + 1) % n
            } else {
                (self.current + n - 1) % n
            };
        }
    }

    /// Recount over each row's texts, keeping the current match near the
    /// first row on screen.
    pub fn recount(&mut self, rows: impl Iterator<Item = Vec<String>>, first_visible: usize) {
        self.hits.clear();
        self.current = 0;
        let needle = needle(&self.query);
        if needle.is_empty() {
            return;
        }
        'rows: for (row, texts) in rows.enumerate() {
            let mut nth = 0;
            for text in texts {
                for _ in matches(&text, &needle) {
                    self.hits.push(Hit { row, nth });
                    nth += 1;
                    if self.hits.len() >= MAX_HITS {
                        break 'rows;
                    }
                }
            }
        }
        self.current = self
            .hits
            .iter()
            .position(|h| h.row >= first_visible)
            .unwrap_or(0);
    }
}

/// The query folded for matching; empty when it is only whitespace.
pub fn needle(query: &str) -> Vec<char> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    query.chars().flat_map(char::to_lowercase).collect()
}

/// Byte ranges of `needle` in `text`, ignoring case, not overlapping.
pub fn matches(text: &str, needle: &[char]) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        // Compare the folded text starting at char i with the needle.
        let mut k = 0;
        let mut j = i;
        let mut ok = true;
        while k < needle.len() {
            let Some(&(_, c)) = chars.get(j) else {
                ok = false;
                break;
            };
            for folded in c.to_lowercase() {
                if needle.get(k) != Some(&folded) {
                    ok = false;
                    break;
                }
                k += 1;
            }
            if !ok {
                break;
            }
            j += 1;
        }
        if ok && j > i {
            let end = chars.get(j).map_or(text.len(), |&(at, _)| at);
            out.push(chars[i].0..end);
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_ignores_case_and_keeps_byte_ranges_on_char_boundaries() {
        let n = needle("ÉTÉ");
        let text = "un été, Été!";
        let found = matches(text, &n);
        assert_eq!(found.len(), 2);
        for r in &found {
            assert_eq!(text[r.clone()].to_lowercase(), "été");
        }
        assert_eq!(matches("aaaa", &needle("aa")), [0..2, 2..4]);
        assert!(matches("abc", &needle("   ")).is_empty());
    }

    #[test]
    fn recount_numbers_matches_per_row_and_starts_near_the_viewport() {
        let mut f = Find {
            query: "x".into(),
            ..Find::default()
        };
        let rows = vec![
            vec!["x and x".to_string()],
            vec!["none".to_string()],
            vec!["x".to_string(), "more x".to_string()],
        ];
        f.recount(rows.clone().into_iter(), 2);
        assert_eq!(
            f.hits,
            [
                Hit { row: 0, nth: 0 },
                Hit { row: 0, nth: 1 },
                Hit { row: 2, nth: 0 },
                Hit { row: 2, nth: 1 },
            ]
        );
        assert_eq!(f.current, 2);
        assert_eq!(f.summary(), "3 of 4");
        f.step(true);
        f.step(true);
        assert_eq!(f.current, 0, "wraps around");
        f.step(false);
        assert_eq!(f.current, 3);
    }
}
