//! Find and replace: plain text or regular expressions, case, whole words, capture groups.
//!
//! Positions are byte offsets into the text; the editor converts them to characters
//! ([`char_to_byte`], [`byte_to_char`]).

use std::ops::Range;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

/// What to look for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub pattern: String,
    /// `pattern` is a regular expression (else plain text).
    pub regex: bool,
    pub match_case: bool,
    pub whole_word: bool,
}

impl Query {
    /// The regular expression this query stands for, or why it is not valid.
    pub fn compile(&self) -> Result<Regex, String> {
        if self.pattern.is_empty() {
            return Err(String::new());
        }
        let body = if self.regex { self.pattern.clone() } else { regex::escape(&self.pattern) };
        let body = if self.whole_word { format!(r"\b(?:{body})\b") } else { body };
        RegexBuilder::new(&body)
            .case_insensitive(!self.match_case)
            .multi_line(true)
            .build()
            .map_err(|e| e.to_string())
    }
}

/// Every match, in order (empty matches are skipped: they select nothing).
pub fn find_all(text: &str, re: &Regex) -> Vec<Range<usize>> {
    re.find_iter(text).filter(|m| !m.is_empty()).map(|m| m.range()).collect()
}

/// The next match after `from` (or before it, `backwards`), wrapping around the document.
pub fn find_next(text: &str, re: &Regex, from: usize, backwards: bool) -> Option<Range<usize>> {
    let all = find_all(text, re);
    if backwards {
        all.iter().rev().find(|r| r.end <= from).or_else(|| all.last()).cloned()
    } else {
        all.iter().find(|r| r.start >= from).or_else(|| all.first()).cloned()
    }
}

/// What `range` (a match of `re`) is replaced by: `replacement` with `$1`, `${name}` expanded in
/// regex mode, as typed otherwise.
pub fn replacement_for(text: &str, re: &Regex, range: Range<usize>, replacement: &str, regex_mode: bool) -> String {
    if !regex_mode {
        return replacement.to_owned();
    }
    let unescaped = unescape(replacement);
    match re.captures_at(text, range.start) {
        Some(caps) if caps.get(0).is_some_and(|m| m.range() == range) => {
            let mut out = String::new();
            caps.expand(&unescaped, &mut out);
            out
        }
        _ => unescaped,
    }
}

/// `text` with every match replaced, and how many there were.
pub fn replace_all(text: &str, re: &Regex, replacement: &str, regex_mode: bool) -> (String, usize) {
    let count = find_all(text, re).len();
    if count == 0 {
        return (text.to_owned(), 0);
    }
    let out = if regex_mode {
        let unescaped = unescape(replacement);
        re.replace_all(text, |caps: &regex::Captures<'_>| {
            if caps.get(0).is_some_and(|m| m.is_empty()) {
                return String::new();
            }
            let mut out = String::new();
            caps.expand(&unescaped, &mut out);
            out
        })
        .into_owned()
    } else {
        re.replace_all(text, regex::NoExpand(replacement)).into_owned()
    };
    (out, count)
}

/// `\n`, `\t` and `\\` in a regex-mode replacement.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The byte offset of character `index` (the end for an index past it).
pub fn char_to_byte(text: &str, index: usize) -> usize {
    text.char_indices().nth(index).map_or(text.len(), |(b, _)| b)
}

/// The character index of byte offset `byte` (rounded down to a character boundary).
pub fn byte_to_char(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    text[..byte].chars().count()
}

/// The 1-based line and column (in characters) of byte offset `byte`.
pub fn line_col(text: &str, byte: usize) -> (usize, usize) {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    let before = &text[..byte];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[line_start..].chars().count() + 1)
}

/// The byte range of 1-based `line` (without its newline); the last line for one past the end.
pub fn line_range(text: &str, line: usize) -> Range<usize> {
    let mut start = 0;
    for (i, l) in text.split('\n').enumerate() {
        if i + 1 == line.max(1) {
            return start..start + l.len();
        }
        start += l.len() + 1;
    }
    let last = text.rfind('\n').map_or(0, |i| i + 1);
    last..text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(pattern: &str, regex: bool, match_case: bool, whole_word: bool) -> Query {
        Query { pattern: pattern.into(), regex, match_case, whole_word }
    }

    #[test]
    fn plain_text_is_escaped_and_case_follows_the_option() {
        let re = q("a.b", false, false, false).compile().unwrap();
        assert_eq!(find_all("a.b axb A.B", &re), vec![0..3, 8..11]);
        let re = q("a.b", false, true, false).compile().unwrap();
        assert_eq!(find_all("a.b A.B", &re), vec![0..3]);
    }

    #[test]
    fn whole_words() {
        let re = q("cat", false, false, true).compile().unwrap();
        assert_eq!(find_all("cat concat cat.", &re), vec![0..3, 11..14]);
    }

    #[test]
    fn regex_with_groups() {
        let re = q(r"(\w+)@(\w+)", true, false, false).compile().unwrap();
        let (out, n) = replace_all("a@b c@d", &re, "$2 at $1", true);
        assert_eq!((out.as_str(), n), ("b at a d at c", 2));
        let r = find_next("a@b c@d", &re, 1, false).unwrap();
        assert_eq!(replacement_for("a@b c@d", &re, r, "[$1]", true), "[c]");
    }

    #[test]
    fn plain_replacement_keeps_dollars() {
        let re = q("x", false, false, false).compile().unwrap();
        assert_eq!(replace_all("x", &re, "$1", false).0, "$1");
    }

    #[test]
    fn escapes_in_regex_replacement() {
        let re = q(", ", true, false, false).compile().unwrap();
        assert_eq!(replace_all("a, b", &re, r"\n", true).0, "a\nb");
    }

    #[test]
    fn next_wraps_both_ways() {
        let re = q("o", false, false, false).compile().unwrap();
        let t = "o.o.o";
        assert_eq!(find_next(t, &re, 1, false), Some(2..3));
        assert_eq!(find_next(t, &re, 5, false), Some(0..1));
        assert_eq!(find_next(t, &re, 2, true), Some(0..1));
        assert_eq!(find_next(t, &re, 0, true), Some(4..5));
    }

    #[test]
    fn invalid_regex_is_an_error() {
        assert!(q("(", true, false, false).compile().is_err());
        assert!(q("", false, false, false).compile().is_err());
    }

    #[test]
    fn positions() {
        let t = "añb\ncd";
        assert_eq!(char_to_byte(t, 2), 3);
        assert_eq!(byte_to_char(t, 3), 2);
        assert_eq!(line_col(t, 6), (2, 2));
        assert_eq!(line_range(t, 2), 5..7);
        assert_eq!(line_range(t, 1), 0..4);
        assert_eq!(line_range(t, 9), 5..7);
    }
}
