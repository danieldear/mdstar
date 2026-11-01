use std::ops::Range;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub row: usize,
    pub nth: usize,
}
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
