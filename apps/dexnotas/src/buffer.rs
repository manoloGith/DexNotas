//! The text of a document: a rope (`ropey`), so an edit costs the same in a 1 KB file as in a
//! 500 MB one, with undo and redo.
//!
//! Positions are character indices. Every edit replaces a range with new text; consecutive
//! typing is grouped into one undo step. Each state of the text has an id, so "unsaved changes"
//! is "the state is not the one saved", which costs nothing to check, and undoing back to the
//! saved text clears it.

use std::ops::Range;
use std::time::{Duration, Instant};

use ropey::Rope;

/// One replacement: at character `at`, `removed` was replaced by `inserted`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub at: usize,
    pub removed: String,
    pub inserted: String,
}

/// A replacement in bytes, for what follows the text (the highlighting).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteEdit {
    pub at: usize,
    pub removed: usize,
    pub inserted: usize,
}

/// How an edit joins the undo history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// Typing: joins the previous typing step if it continues it.
    Typing,
    /// Deleting with Backspace or Delete: joins the previous deletion step.
    Deleting,
    /// A step of its own (paste, a command, a replacement).
    Alone,
}

struct Step {
    edits: Vec<Edit>,
    /// The selection (anchor, cursor) before and after.
    before: (usize, usize),
    after: (usize, usize),
    /// The state the text was in before this step and after it.
    state_before: u64,
    state_after: u64,
    group: Group,
    at: Instant,
}

pub struct Buffer {
    rope: Rope,
    undo: Vec<Step>,
    redo: Vec<Step>,
    state: u64,
    next_state: u64,
    /// Bumped on every change, undo and redo included.
    version: u64,
    /// The byte edits of recent versions (`version` after the edit), oldest first.
    byte_edits: Vec<(u64, ByteEdit)>,
    /// The next edit starts a step of its own (the text was just saved).
    sealed: bool,
}

/// Steps further apart than this are not joined.
const JOIN: Duration = Duration::from_millis(1200);
/// Undo steps kept.
const MAX_UNDO: usize = 1000;

impl Buffer {
    pub fn new(text: &str) -> Buffer {
        Buffer::from_rope(Rope::from_str(text))
    }

    pub fn from_rope(rope: Rope) -> Buffer {
        Buffer { rope, undo: Vec::new(), redo: Vec::new(), state: 0, next_state: 1, version: 0, byte_edits: Vec::new(), sealed: false }
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    pub fn state(&self) -> u64 {
        self.state
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The text of `range` (characters).
    pub fn slice_string(&self, range: Range<usize>) -> String {
        let end = range.end.min(self.rope.len_chars());
        let start = range.start.min(end);
        self.rope.slice(start..end).to_string()
    }

    /// The byte edits made after `version`, in order (empty if they are no longer known).
    pub fn byte_edits_since(&self, version: u64) -> Option<Vec<ByteEdit>> {
        if version == self.version {
            return Some(Vec::new());
        }
        let first = self.byte_edits.first()?;
        // The edit that made version+1 must still be in the log.
        if first.0 > version + 1 {
            return None;
        }
        Some(self.byte_edits.iter().filter(|(v, _)| *v > version).map(|(_, e)| *e).collect())
    }

    fn apply(&mut self, at: usize, remove: usize, insert: &str) -> Edit {
        let end = (at + remove).min(self.rope.len_chars());
        let at = at.min(end);
        let removed = self.rope.slice(at..end).to_string();
        let byte_at = self.rope.char_to_byte(at);
        self.rope.remove(at..end);
        self.rope.insert(at, insert);
        self.version += 1;
        self.byte_edits.push((self.version, ByteEdit { at: byte_at, removed: removed.len(), inserted: insert.len() }));
        if self.byte_edits.len() > 4096 {
            self.byte_edits.drain(..2048);
        }
        Edit { at, removed, inserted: insert.to_owned() }
    }

    /// Replace `range` with `text`. `before` and `after` are the selections to restore on undo
    /// and redo.
    pub fn replace(&mut self, range: Range<usize>, text: &str, before: (usize, usize), after: (usize, usize), group: Group) {
        if range.is_empty() && text.is_empty() {
            return;
        }
        let edit = self.apply(range.start, range.end - range.start, text);
        self.redo.clear();
        let now = Instant::now();
        // Typing that continues the last step joins it.
        let sealed = std::mem::take(&mut self.sealed);
        if group != Group::Alone
            && !sealed
            && let Some(last) = self.undo.last_mut()
            && last.group == group
            && last.state_after == self.state
            && now.duration_since(last.at) < JOIN
            && last.edits.last().is_some_and(|e| continues(e, &edit, group))
            && !text.contains('\n')
        {
            last.edits.push(edit);
            last.after = after;
            last.at = now;
            let state = self.next_state;
            self.next_state += 1;
            last.state_after = state;
            self.state = state;
            return;
        }
        let state = self.next_state;
        self.next_state += 1;
        self.undo.push(Step { edits: vec![edit], before, after, state_before: self.state, state_after: state, group, at: now });
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.state = state;
    }

    /// The next edit starts a new undo step (call after saving, so undo can stop at the saved
    /// text).
    pub fn seal(&mut self) {
        self.sealed = true;
    }

    /// Undo the last step; the selection to show.
    pub fn undo(&mut self) -> Option<(usize, usize)> {
        let step = self.undo.pop()?;
        for e in step.edits.iter().rev() {
            self.apply(e.at, e.inserted.chars().count(), &e.removed);
        }
        self.state = step.state_before;
        let sel = step.before;
        self.sealed = true;
        self.redo.push(step);
        Some(sel)
    }

    /// Redo the last undone step; the selection to show.
    pub fn redo(&mut self) -> Option<(usize, usize)> {
        let mut step = self.redo.pop()?;
        for e in &step.edits {
            self.apply(e.at, e.removed.chars().count(), &e.inserted);
        }
        self.state = step.state_after;
        let sel = step.after;
        step.at = Instant::now() - JOIN;
        self.undo.push(step);
        Some(sel)
    }

    /// Mark the current state as a new one nothing can return to (the text was replaced from
    /// outside, as in a reload): the history is dropped.
    pub fn reset(&mut self, rope: Rope) {
        let len = self.rope.len_bytes();
        self.rope = rope;
        self.undo.clear();
        self.redo.clear();
        self.state = self.next_state;
        self.next_state += 1;
        self.version += 1;
        self.byte_edits.push((self.version, ByteEdit { at: 0, removed: len, inserted: self.rope.len_bytes() }));
    }
}

/// Whether `next` continues `prev` as typing (right after it) or deleting (Backspace or Delete
/// at the same place).
fn continues(prev: &Edit, next: &Edit, group: Group) -> bool {
    match group {
        Group::Typing => next.removed.is_empty() && next.at == prev.at + prev.inserted.chars().count(),
        Group::Deleting => {
            next.inserted.is_empty() && (next.at + next.removed.chars().count() == prev.at || next.at == prev.at)
        }
        Group::Alone => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_is_one_step_and_undo_returns_to_the_saved_state() {
        let mut b = Buffer::new("ab");
        let saved = b.state();
        b.replace(2..2, "c", (2, 2), (3, 3), Group::Typing);
        b.replace(3..3, "d", (3, 3), (4, 4), Group::Typing);
        assert_eq!(b.rope().to_string(), "abcd");
        assert_ne!(b.state(), saved);
        assert_eq!(b.undo(), Some((2, 2)));
        assert_eq!(b.rope().to_string(), "ab");
        assert_eq!(b.state(), saved);
        assert_eq!(b.redo(), Some((4, 4)));
        assert_eq!(b.rope().to_string(), "abcd");
        assert!(!b.can_redo());
    }

    #[test]
    fn a_new_line_or_a_paste_starts_a_step() {
        let mut b = Buffer::new("");
        b.replace(0..0, "a", (0, 0), (1, 1), Group::Typing);
        b.replace(1..1, "\n", (1, 1), (2, 2), Group::Typing);
        b.replace(2..2, "pasted", (2, 2), (8, 8), Group::Alone);
        b.undo();
        assert_eq!(b.rope().to_string(), "a\n");
        b.undo();
        assert_eq!(b.rope().to_string(), "a");
    }

    #[test]
    fn backspaces_join() {
        let mut b = Buffer::new("hello");
        b.replace(4..5, "", (5, 5), (4, 4), Group::Deleting);
        b.replace(3..4, "", (4, 4), (3, 3), Group::Deleting);
        assert_eq!(b.rope().to_string(), "hel");
        b.undo();
        assert_eq!(b.rope().to_string(), "hello");
    }

    #[test]
    fn byte_edits_follow_versions() {
        let mut b = Buffer::new("añb");
        let v = b.version();
        b.replace(2..2, "xyz", (2, 2), (5, 5), Group::Alone);
        let edits = b.byte_edits_since(v).unwrap();
        assert_eq!(edits, vec![ByteEdit { at: 3, removed: 0, inserted: 3 }]);
        assert_eq!(b.byte_edits_since(b.version()), Some(Vec::new()));
    }
}
