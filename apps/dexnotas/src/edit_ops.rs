//! Edit menu operations on the text and its selection (character positions): case, line
//! comments, duplicating, deleting and sorting lines, trailing spaces, indentation.

use std::ops::Range;

use crate::find::{byte_to_char, char_to_byte};

/// The result of an operation: the new text and what to select in it.
pub type Edited = (String, Range<usize>);

/// The byte range of the whole lines `sel` (bytes) touches, without the last newline.
fn line_span(text: &str, sel: Range<usize>) -> Range<usize> {
    let start = text[..sel.start].rfind('\n').map_or(0, |i| i + 1);
    // A selection that ends at the start of a line doesn't take that line.
    let end_at = if sel.end > sel.start && text[..sel.end].ends_with('\n') { sel.end - 1 } else { sel.end };
    let end = text[end_at..].find('\n').map_or(text.len(), |i| end_at + i);
    start..end.max(start)
}

fn bytes(text: &str, sel: &Range<usize>) -> Range<usize> {
    let a = char_to_byte(text, sel.start.min(sel.end));
    let b = char_to_byte(text, sel.start.max(sel.end));
    a..b
}

fn chars(text: &str, r: Range<usize>) -> Range<usize> {
    byte_to_char(text, r.start)..byte_to_char(text, r.end)
}

fn replace(text: &str, span: Range<usize>, with: &str) -> String {
    let mut out = String::with_capacity(text.len() + with.len());
    out.push_str(&text[..span.start]);
    out.push_str(with);
    out.push_str(&text[span.end..]);
    out
}

/// UPPER or lower case of the selection (of the whole text when nothing is selected).
pub fn change_case(text: &str, sel: Range<usize>, upper: bool) -> Edited {
    let b = bytes(text, &sel);
    let b = if b.is_empty() { 0..text.len() } else { b };
    let piece = &text[b.clone()];
    let changed = if upper { piece.to_uppercase() } else { piece.to_lowercase() };
    let out = replace(text, b.clone(), &changed);
    let r = b.start..b.start + changed.len();
    let sel = chars(&out, r);
    (out, sel)
}

/// Comment the selected lines with `prefix`, or uncomment them if they all are.
pub fn toggle_comment(text: &str, sel: Range<usize>, prefix: &str) -> Edited {
    let span = line_span(text, bytes(text, &sel));
    let lines: Vec<&str> = text[span.clone()].split('\n').collect();
    let all_commented = lines.iter().filter(|l| !l.trim().is_empty()).all(|l| l.trim_start().starts_with(prefix));
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let new: Vec<String> = lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                (*l).to_owned()
            } else if all_commented {
                let lead = l.len() - l.trim_start().len();
                let rest = &l[lead + prefix.len()..];
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                format!("{}{rest}", &l[..lead])
            } else {
                format!("{}{prefix} {}", &l[..indent], &l[indent..])
            }
        })
        .collect();
    let joined = new.join("\n");
    let out = replace(text, span.clone(), &joined);
    let r = span.start..span.start + joined.len();
    let sel = chars(&out, r);
    (out, sel)
}

/// The selected lines (or the cursor's) again, below.
pub fn duplicate_lines(text: &str, sel: Range<usize>) -> Edited {
    let span = line_span(text, bytes(text, &sel));
    let block = text[span.clone()].to_owned();
    let insert = format!("\n{block}");
    let out = replace(text, span.end..span.end, &insert);
    let start = span.end + 1;
    let sel = chars(&out, start..start + block.len());
    (out, sel)
}

/// Remove the selected lines (or the cursor's).
pub fn delete_lines(text: &str, sel: Range<usize>) -> Edited {
    let span = line_span(text, bytes(text, &sel));
    let (from, to) = if span.end < text.len() {
        (span.start, span.end + 1)
    } else if span.start > 0 {
        (span.start - 1, span.end)
    } else {
        (span.start, span.end)
    };
    let out = replace(text, from..to, "");
    let at = byte_to_char(&out, from.min(out.len()));
    (out, at..at)
}

/// Sort the selected lines (all lines when the selection is within one line).
pub fn sort_lines(text: &str, sel: Range<usize>, descending: bool) -> Edited {
    let b = bytes(text, &sel);
    let span = if text[b.clone()].contains('\n') { line_span(text, b) } else { 0..text.len() };
    let trailing_newline = span.end == text.len() && text.ends_with('\n');
    let body = if trailing_newline { &text[span.start..span.end - 1] } else { &text[span.clone()] };
    let mut lines: Vec<&str> = body.split('\n').collect();
    lines.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then(a.cmp(b)));
    if descending {
        lines.reverse();
    }
    let mut joined = lines.join("\n");
    if trailing_newline {
        joined.push('\n');
    }
    let out = replace(text, span.clone(), &joined);
    let sel = chars(&out, span.start..span.start + joined.len());
    (out, sel)
}

/// Remove spaces and tabs at the end of every line.
pub fn trim_trailing(text: &str) -> String {
    let mut out: String = text.split('\n').map(|l| l.trim_end_matches([' ', '\t'])).collect::<Vec<_>>().join("\n");
    if text.is_empty() {
        out.clear();
    }
    out
}

/// Indent (or, `outdent`, unindent) the selected lines by `unit`.
pub fn indent_lines(text: &str, sel: Range<usize>, unit: &str, outdent: bool) -> Edited {
    let span = line_span(text, bytes(text, &sel));
    let new: Vec<String> = text[span.clone()]
        .split('\n')
        .map(|l| {
            if outdent {
                if let Some(rest) = l.strip_prefix(unit) {
                    rest.to_owned()
                } else if let Some(rest) = l.strip_prefix('\t') {
                    rest.to_owned()
                } else {
                    l.trim_start_matches(' ').to_owned()
                }
            } else if l.is_empty() {
                String::new()
            } else {
                format!("{unit}{l}")
            }
        })
        .collect();
    let joined = new.join("\n");
    let out = replace(text, span.clone(), &joined);
    let sel = chars(&out, span.start..span.start + joined.len());
    (out, sel)
}

/// The spaces and tabs that start the line `char_index` is on (for the next line's indent).
pub fn leading_whitespace(text: &str, char_index: usize) -> String {
    let at = char_to_byte(text, char_index);
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    text[start..at].chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case() {
        assert_eq!(change_case("año día", 0..3, true), ("AÑO día".into(), 0..3));
        assert_eq!(change_case("ABC", 0..0, false).0, "abc");
    }

    #[test]
    fn comments_toggle() {
        let (t, _) = toggle_comment("  a\n  b\nc", 0..5, "//");
        assert_eq!(t, "  // a\n  // b\nc");
        let (t, _) = toggle_comment(&t, 0..8, "//");
        assert_eq!(t, "  a\n  b\nc");
        assert_eq!(toggle_comment("x\n\ny", 0..4, "#").0, "# x\n\n# y");
    }

    #[test]
    fn duplicate_delete() {
        assert_eq!(duplicate_lines("a\nb", 0..0).0, "a\na\nb");
        assert_eq!(delete_lines("a\nb\nc", 2..2).0, "a\nc");
        assert_eq!(delete_lines("a\nb", 2..2).0, "a");
        assert_eq!(delete_lines("a", 0..0).0, "");
    }

    #[test]
    fn sort() {
        assert_eq!(sort_lines("c\nb\nA\n", 0..0, false).0, "A\nb\nc\n");
        assert_eq!(sort_lines("x\nc\nb\na\ny", 2..7, false).0, "x\na\nb\nc\ny");
        assert_eq!(sort_lines("a\nb", 0..0, true).0, "b\na");
    }

    #[test]
    fn trailing_and_indent() {
        assert_eq!(trim_trailing("a  \nb\t\n"), "a\nb\n");
        assert_eq!(indent_lines("a\nb", 0..3, "    ", false).0, "    a\n    b");
        assert_eq!(indent_lines("    a\n\tb", 0..8, "    ", true).0, "a\nb");
        assert_eq!(leading_whitespace("  ab\n\tc", 4), "  ");
        assert_eq!(leading_whitespace("  ab\n\tc", 7), "\t");
    }
}
