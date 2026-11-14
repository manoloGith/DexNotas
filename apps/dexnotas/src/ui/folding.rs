//! Code folding by indentation, like VS Code's default: a line can be folded when the next line
//! with text is indented further; the region runs to the last line indented further than it.
//! It works the same in every language (tree-sitter or highlight.js) and in plain text.
//!
//! Folds are kept as byte ranges that follow the edits (as the colours do), so they stay on their
//! lines while typing above them; an edit inside a folded region opens it.

use std::ops::Range;

use ropey::Rope;

use crate::buffer::{Buffer, ByteEdit};

/// A folded region.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Fold {
    /// The bytes of the hidden lines (from the start of the line after the header to the start
    /// of the line after the region, or the end of the text).
    hidden: Range<usize>,
}

#[derive(Default)]
pub struct Folds {
    folds: Vec<Fold>,
    /// The text version the folds belong to.
    version: u64,
    /// The hidden lines, merged and in order, for that version.
    lines: Vec<Range<usize>>,
}

/// Columns of indentation of `line` (a tab counts 4); `None` for a blank line.
fn indent(rope: &Rope, line: usize) -> Option<usize> {
    let mut cols = 0;
    for c in rope.line(line).chars() {
        match c {
            ' ' => cols += 1,
            '\t' => cols += 4 - cols % 4,
            '\n' | '\r' => return None,
            _ => return Some(cols),
        }
    }
    None
}

/// Whether `line` starts a region (cheap: looks only at the next lines with text).
pub fn foldable(rope: &Rope, line: usize) -> bool {
    let Some(own) = indent(rope, line) else { return false };
    let lines = rope.len_lines();
    (line + 1..lines.min(line + 200)).find_map(|j| indent(rope, j)).is_some_and(|next| next > own)
}

/// The last line of the region `line` starts.
pub fn region(rope: &Rope, line: usize) -> Option<usize> {
    let own = indent(rope, line)?;
    let mut last = line;
    for j in line + 1..rope.len_lines() {
        match indent(rope, j) {
            None => continue,
            Some(d) if d <= own => break,
            Some(_) => last = j,
        }
    }
    (last > line).then_some(last)
}

impl Folds {
    pub fn is_empty(&self) -> bool {
        self.folds.is_empty()
    }

    /// Follow the edits since the folds were last placed, and work out the hidden lines.
    pub fn sync(&mut self, buffer: &Buffer) {
        let version = buffer.version();
        if version == self.version && (self.folds.is_empty() || !self.lines.is_empty()) {
            return;
        }
        match buffer.byte_edits_since(self.version) {
            Some(edits) => {
                for e in edits {
                    self.follow(e);
                }
            }
            None => self.folds.clear(),
        }
        self.version = version;
        self.relines(buffer.rope());
    }

    fn follow(&mut self, e: ByteEdit) {
        let end = e.at + e.removed;
        let delta = e.inserted as isize - e.removed as isize;
        let shift = |x: usize| (x as isize + delta).max(0) as usize;
        self.folds.retain_mut(|f| {
            // Inside the hidden lines (or across their edges): the fold opens.
            let touches = if e.removed == 0 { e.at > f.hidden.start && e.at < f.hidden.end } else { e.at < f.hidden.end && end > f.hidden.start };
            if touches {
                return false;
            }
            if end <= f.hidden.start {
                f.hidden = shift(f.hidden.start)..shift(f.hidden.end);
            }
            true
        });
    }

    fn relines(&mut self, rope: &Rope) {
        let len = rope.len_bytes();
        let total = rope.len_lines();
        let mut ranges: Vec<Range<usize>> = self
            .folds
            .iter()
            .filter(|f| f.hidden.start < f.hidden.end && f.hidden.end <= len)
            .map(|f| {
                let first = rope.byte_to_line(f.hidden.start);
                let end = if f.hidden.end >= len { total } else { rope.byte_to_line(f.hidden.end) };
                first..end.max(first)
            })
            .filter(|r| !r.is_empty())
            .collect();
        ranges.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for r in ranges {
            match merged.last_mut() {
                Some(m) if r.start < m.end => m.end = m.end.max(r.end),
                _ => merged.push(r),
            }
        }
        self.lines = merged;
    }

    fn range_of(&self, line: usize) -> Option<&Range<usize>> {
        let i = self.lines.partition_point(|r| r.end <= line);
        self.lines.get(i).filter(|r| r.start <= line)
    }

    /// Whether `line` is hidden in a fold.
    pub fn hidden(&self, line: usize) -> bool {
        self.range_of(line).is_some()
    }

    /// The next line on screen after `line`.
    pub fn next(&self, line: usize) -> usize {
        let n = line + 1;
        self.range_of(n).map_or(n, |r| r.end)
    }

    /// The line on screen before `line` (`None` at the top).
    pub fn prev(&self, line: usize) -> Option<usize> {
        let p = line.checked_sub(1)?;
        Some(self.range_of(p).map_or(p, |r| r.start.saturating_sub(1)))
    }

    /// The line on screen that holds `line` (itself, or the header of the fold hiding it).
    pub fn shown(&self, line: usize) -> usize {
        self.range_of(line).map_or(line, |r| r.start.saturating_sub(1))
    }

    /// Whether `line` heads a folded region (shown with +).
    pub fn is_folded(&self, line: usize) -> bool {
        self.range_of(line + 1).is_some_and(|r| r.start == line + 1)
    }

    fn add(&mut self, rope: &Rope, header: usize, last: usize) {
        let len = rope.len_bytes();
        let start = rope.line_to_byte(header + 1);
        let end = if last + 1 >= rope.len_lines() { len } else { rope.line_to_byte(last + 1) };
        if start < end && !self.folds.iter().any(|f| f.hidden.start == start) {
            self.folds.push(Fold { hidden: start..end });
        }
    }

    /// Fold the region `line` starts; false if it starts none.
    pub fn fold(&mut self, rope: &Rope, line: usize) -> bool {
        let Some(last) = region(rope, line) else { return false };
        self.add(rope, line, last);
        self.relines(rope);
        true
    }

    /// Open the fold `line` heads.
    pub fn unfold(&mut self, rope: &Rope, line: usize) -> bool {
        if line + 1 >= rope.len_lines() {
            return false;
        }
        let start = rope.line_to_byte(line + 1);
        let before = self.folds.len();
        self.folds.retain(|f| f.hidden.start != start);
        self.relines(rope);
        self.folds.len() != before
    }

    /// Fold or open `line`'s region.
    pub fn toggle(&mut self, rope: &Rope, line: usize) {
        if !self.unfold(rope, line) {
            self.fold(rope, line);
        }
    }

    /// Open every fold that hides `line`.
    pub fn reveal(&mut self, rope: &Rope, line: usize) {
        if !self.hidden(line) {
            return;
        }
        let at = rope.line_to_byte(line.min(rope.len_lines().saturating_sub(1)));
        self.folds.retain(|f| !(f.hidden.start <= at && at < f.hidden.end));
        self.relines(rope);
    }

    /// Fold the innermost region holding `line` (or starting at it).
    pub fn fold_around(&mut self, rope: &Rope, line: usize) -> bool {
        if !self.is_folded(line) && self.fold(rope, line) {
            return true;
        }
        let Some(own) = (line..rope.len_lines()).find_map(|j| indent(rope, j)) else { return false };
        let mut probe = line;
        while probe > 0 {
            probe -= 1;
            if indent(rope, probe).is_some_and(|d| d < own) && region(rope, probe).is_some_and(|last| last >= line) {
                if !self.is_folded(probe) {
                    self.fold(rope, probe);
                }
                return true;
            }
        }
        false
    }

    /// Fold every region (nested ones too), in one pass.
    pub fn fold_all(&mut self, rope: &Rope) {
        // Open regions: (header, its indent, the last line with text inside it).
        let mut open: Vec<(usize, usize, usize)> = Vec::new();
        let mut found: Vec<(usize, usize)> = Vec::new();
        for j in 0..rope.len_lines() {
            let Some(d) = indent(rope, j) else { continue };
            while let Some(&(h, hd, last)) = open.last() {
                if hd < d {
                    break;
                }
                open.pop();
                if last > h {
                    found.push((h, last));
                }
            }
            for o in open.iter_mut() {
                o.2 = j;
            }
            open.push((j, d, j));
        }
        for (h, _, last) in open {
            if last > h {
                found.push((h, last));
            }
        }
        for (h, last) in found {
            self.add(rope, h, last);
        }
        self.relines(rope);
    }

    pub fn clear(&mut self) {
        self.folds.clear();
        self.lines.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Group;

    const SRC: &str = "fn a() {\n    one();\n    if x {\n        two();\n    }\n}\n\nfn b() {\n    three();\n}\n";

    #[test]
    fn regions_by_indent() {
        let rope = Rope::from_str(SRC);
        assert!(foldable(&rope, 0));
        assert!(!foldable(&rope, 1));
        assert_eq!(region(&rope, 0), Some(4));
        assert_eq!(region(&rope, 2), Some(3));
        assert_eq!(region(&rope, 7), Some(8));
        assert_eq!(region(&rope, 6), None);
    }

    #[test]
    fn folding_hides_and_follows_edits() {
        let mut buffer = Buffer::new(SRC);
        let mut f = Folds::default();
        f.sync(&buffer);
        assert!(f.fold(buffer.rope(), 0));
        assert!(f.is_folded(0));
        assert!(f.hidden(1) && f.hidden(4) && !f.hidden(5));
        assert_eq!(f.next(0), 5);
        assert_eq!(f.prev(5), Some(0));
        // A line typed above moves the fold down with its text.
        buffer.replace(0..0, "// top\n", (0, 0), (7, 7), Group::Alone);
        f.sync(&buffer);
        assert!(f.is_folded(1) && f.hidden(2) && !f.hidden(6));
        // Typing at the end of the header keeps it folded.
        let header_end = buffer.rope().line_to_char(2) - 1;
        buffer.replace(header_end..header_end, " ", (0, 0), (0, 0), Group::Typing);
        f.sync(&buffer);
        assert!(f.is_folded(1));
        // An edit inside opens it.
        let inside = buffer.rope().line_to_char(3) + 2;
        buffer.replace(inside..inside, "x", (0, 0), (0, 0), Group::Typing);
        f.sync(&buffer);
        assert!(!f.is_folded(1) && f.is_empty());
    }

    #[test]
    fn fold_all_and_around() {
        let rope = Rope::from_str(SRC);
        let mut f = Folds::default();
        f.fold_all(&rope);
        assert!(f.is_folded(0) && f.is_folded(7));
        assert_eq!(f.next(0), 5);
        f.clear();
        assert!(f.fold_around(&rope, 3));
        assert!(f.is_folded(2) && !f.is_folded(0));
        f.reveal(&rope, 3);
        assert!(f.is_empty());
    }
}
