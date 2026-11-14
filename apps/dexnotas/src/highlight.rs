//! Syntax colours, worked out on a thread of their own so typing never waits for them.
//!
//! A document up to [`WHOLE_LIMIT`] is highlighted whole; a larger one around what is on screen
//! ([`WINDOW_MARGIN`] bytes each side). While an answer is on its way, the colours already known
//! follow the edits (they move with the text after each change), so nothing flickers.

use std::ops::Range;
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::buffer::ByteEdit;
use crate::syntax::{self, Kind, LanguageId};

/// Documents up to this size are highlighted whole (exact colours everywhere).
pub const WHOLE_LIMIT: usize = 4 * 1024 * 1024;
/// Around the screen, for larger documents.
pub const WINDOW_MARGIN: usize = 256 * 1024;
/// The same for highlight.js languages (slower: JavaScript, some 200 KB a second on a phone).
pub const HLJS_WHOLE_LIMIT: usize = 128 * 1024;
pub const HLJS_WINDOW_MARGIN: usize = 32 * 1024;

fn limits(language: LanguageId) -> (usize, usize) {
    if language.is_tree_sitter() { (WHOLE_LIMIT, WINDOW_MARGIN) } else { (HLJS_WHOLE_LIMIT, HLJS_WINDOW_MARGIN) }
}

/// A document's colours.
#[derive(Default)]
pub struct Colours {
    /// Byte ranges of the document, in order.
    spans: Vec<(Range<usize>, Kind)>,
    /// The text version the spans belong to (after following later edits).
    version: u64,
    /// The text version the last answer was worked out for: the colours are exact when it is
    /// the current one.
    exact: Option<u64>,
    /// The bytes highlighted.
    window: Range<usize>,
    language: Option<LanguageId>,
    /// A request is on its way (its version).
    pending: Option<u64>,
}

impl Colours {
    /// The spans that touch `bytes`.
    pub fn spans_in(&self, bytes: Range<usize>) -> &[(Range<usize>, Kind)] {
        let first = self.spans.partition_point(|(r, _)| r.end <= bytes.start);
        let last = self.spans.partition_point(|(r, _)| r.start < bytes.end);
        &self.spans[first..last.max(first)]
    }

    /// Move the spans with an edit: those after it shift, those it touches go.
    fn follow(&mut self, e: ByteEdit) {
        let end = e.at + e.removed;
        let delta = e.inserted as isize - e.removed as isize;
        let shift = |x: usize| (x as isize + delta).max(0) as usize;
        self.spans.retain_mut(|(r, _)| {
            if r.end <= e.at {
                true
            } else if r.start >= end {
                *r = shift(r.start)..shift(r.end);
                true
            } else {
                false
            }
        });
        if self.window.end >= end {
            self.window.end = shift(self.window.end);
        }
        if self.window.start > e.at {
            self.window.start = shift(self.window.start).min(self.window.end);
        }
    }
}

struct Request {
    doc: u64,
    version: u64,
    base: usize,
    text: String,
    language: LanguageId,
}

struct Answer {
    doc: u64,
    version: u64,
    window: Range<usize>,
    language: LanguageId,
    spans: Vec<(Range<usize>, Kind)>,
}

/// The highlighting thread.
pub struct Worker {
    tx: Sender<Request>,
    rx: Receiver<Answer>,
}

impl Worker {
    pub fn new(ctx: egui::Context) -> Worker {
        let (tx, requests) = channel::<Request>();
        let (answers, rx) = channel::<Answer>();
        let _ = std::thread::Builder::new().name("dexnotas-highlight".into()).spawn(move || {
            while let Ok(first) = requests.recv() {
                // Only the latest request of each document matters.
                let mut latest = vec![first];
                while let Ok(r) = requests.try_recv() {
                    latest.retain(|l: &Request| l.doc != r.doc);
                    latest.push(r);
                }
                for r in latest {
                    let spans = syntax::highlight(r.language, &r.text)
                        .into_iter()
                        .map(|(range, kind)| (range.start + r.base..range.end + r.base, kind))
                        .collect();
                    let window = r.base..r.base + r.text.len();
                    if answers.send(Answer { doc: r.doc, version: r.version, window, language: r.language, spans }).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            }
        });
        Worker { tx, rx }
    }

    /// Take the answers that arrived into their documents.
    pub fn receive(&self, docs: &mut [crate::document::Document]) {
        while let Ok(a) = self.rx.try_recv() {
            let Some(doc) = docs.iter_mut().find(|d| d.id == a.doc) else { continue };
            let c = &mut doc.highlight;
            if c.pending.is_some_and(|v| v == a.version) {
                c.pending = None;
            }
            if doc.language != Some(a.language) {
                continue;
            }
            let Some(edits) = doc.buffer.byte_edits_since(a.version) else { continue };
            c.spans = a.spans;
            c.window = a.window;
            c.language = Some(a.language);
            for e in edits {
                c.follow(e);
            }
            c.version = doc.buffer.version();
            c.exact = Some(a.version);
        }
    }

    /// Keep `doc`'s colours up to date for `visible` (bytes on screen).
    pub fn update(&self, doc: &mut crate::document::Document, visible: Range<usize>) {
        let version = doc.buffer.version();
        let len = doc.buffer.len_bytes();
        let Some(language) = doc.language else {
            doc.highlight = Colours::default();
            return;
        };
        let c = &mut doc.highlight;
        // Edits since the colours were last placed move them.
        if c.version != version {
            match doc.buffer.byte_edits_since(c.version) {
                Some(edits) => edits.into_iter().for_each(|e| c.follow(e)),
                None => c.spans.clear(),
            }
            c.version = version;
        }
        if c.language != Some(language) {
            c.spans.clear();
            c.window = 0..0;
            c.exact = None;
            c.language = Some(language);
        }
        let (whole_limit, margin) = limits(language);
        let whole = len <= whole_limit;
        let covered = whole || (c.window.start <= visible.start && c.window.end >= visible.end.min(len));
        if c.pending.is_some() || (c.exact == Some(version) && covered) {
            return;
        }
        let rope = doc.buffer.rope();
        let window = if whole {
            0..len
        } else {
            let start = rope.line_to_byte(rope.byte_to_line(visible.start.saturating_sub(margin)));
            let end_line = rope.byte_to_line(visible.end.saturating_add(margin).min(len));
            let end = if end_line + 1 >= rope.len_lines() { len } else { rope.line_to_byte(end_line + 1) };
            start..end
        };
        let text = rope.byte_slice(window.clone()).to_string();
        c.pending = Some(version);
        let _ = self.tx.send(Request { doc: doc.id, version, base: window.start, text, language });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_follow_edits() {
        let mut c = Colours { spans: vec![(0..2, Kind::Keyword), (5..8, Kind::String), (10..12, Kind::Comment)], window: 0..12, ..Default::default() };
        c.follow(ByteEdit { at: 3, removed: 0, inserted: 4 });
        assert_eq!(c.spans.iter().map(|s| s.0.clone()).collect::<Vec<_>>(), vec![0..2, 9..12, 14..16]);
        c.follow(ByteEdit { at: 10, removed: 1, inserted: 0 });
        assert_eq!(c.spans.iter().map(|s| s.0.clone()).collect::<Vec<_>>(), vec![0..2, 13..15]);
        assert_eq!(c.spans_in(12..20).len(), 1);
        assert_eq!(c.spans_in(2..13).len(), 0);
    }
}
