//! The editor: lays out and paints only the lines on screen, so a document of any size scrolls
//! and edits as fast as a small one.
//!
//! - The text is the document's rope (`buffer`). Each frame, the lines from the top of the view
//!   down to its bottom are laid out (egui caches each line's layout between frames) and painted
//!   with their colours, the selection, the find matches and the cursor.
//! - A very long line (a minified JSON file, a log without line breaks) is never laid out whole:
//!   it is cut into pieces of [`CHUNK`] characters, and only the pieces on screen are laid out.
//! - Scrolling is by line, with the scroll bars drawn here: the height of the whole document is
//!   never needed.
//! - Input (keys, text, paste, the on-screen keyboard's composition, the mouse) is handled here
//!   and goes into the buffer as undoable edits.
//! - Regions can be folded (`folding`): the − and + in the gutter hide and show them, and every
//!   walk from line to line skips the hidden ones.

use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::text::{CCursor, LayoutJob, TextFormat};
use egui::{Align2, Color32, CursorIcon, Event, FontId, Galley, Id, Key, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};

use super::folding::{self, Folds};
use super::theme::{self, Palette};
use crate::buffer::Group;
use crate::document::Document;
use crate::syntax::Kind;

/// Lines longer than this (characters) are cut into pieces.
const LONG_LINE: usize = 4000;
/// The piece of a long line laid out at a time (without word wrap).
const CHUNK: usize = 1000;
const SCROLLBAR: f32 = 10.0;
const PAD_X: f32 = 4.0;

/// The editor's state for one document.
#[derive(Default)]
pub struct View {
    /// The first line on screen, and how many pixels of it are scrolled off the top.
    pub top_line: usize,
    pub top_offset: f32,
    pub scroll_x: f32,
    /// The x the cursor keeps when it moves up and down.
    preferred_x: Option<f32>,
    /// The on-screen keyboard's word being composed.
    preedit: Option<String>,
    /// Dragging selects: by characters, words or lines; the selection the drag started from.
    drag: Option<(Unit, (usize, usize))>,
    clicks: Option<(Instant, Pos2, u32)>,
    /// Bring the cursor into view after this frame's input.
    reveal: Option<Reveal>,
    /// The widest line seen (for the horizontal scroll bar).
    widest: f32,
    ime_generation: u64,
    had_focus: bool,
    /// Dragging a scroll bar: (vertical, where the drag started, the scroll it started at).
    bar_drag: Option<(bool, f32, f32)>,
    /// Folded regions.
    pub folds: Folds,
    /// The "…" after each folded line last painted (clicking one opens it), and its line.
    fold_pills: Vec<(Rect, usize)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Unit {
    Char,
    Word,
    Line,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Reveal {
    /// Just enough to show the cursor.
    Near,
    /// The cursor's line a third of the way down (a find match, go to line).
    Center,
}

/// How the editor looks and behaves.
pub struct Look {
    pub font: FontId,
    pub dark: bool,
    pub wrap: bool,
    pub line_numbers: bool,
    pub highlight_line: bool,
    pub indent_unit: String,
    pub indent_width: usize,
    pub auto_indent: bool,
    pub line_comment: Option<&'static str>,
    /// Show the − and + that fold regions.
    pub folding: bool,
}

/// Requests for this frame.
#[derive(Default)]
pub struct Requests {
    pub focus: bool,
    /// Select this (characters, anchor and cursor) and bring it into view.
    pub select: Option<(usize, usize)>,
    /// The find query, to mark its matches on screen.
    pub marks: Option<regex::Regex>,
    /// The current match (bytes).
    pub current_mark: Option<Range<usize>>,
}

/// What the frame's editor did.
pub struct Output {
    pub changed: bool,
    pub focused: bool,
    /// The bytes on screen (for the highlighting).
    pub visible_bytes: Range<usize>,
    /// Edits the editor asks the app to run (Tab on several lines).
    pub indent: Option<bool>,
    /// A right click (or a long press) on the text: where to open the context menu.
    pub context_menu: Option<Pos2>,
}

pub fn editor_id(doc: u64) -> Id {
    Id::unique(("dexnotas-editor", doc))
}

/// If the cursor is inside a folded region, open it.
fn unfold_at_cursor_line(doc: &mut Document) {
    if doc.view.folds.is_empty() {
        return;
    }
    let rope = doc.buffer.rope();
    let line = rope.char_to_line(doc.cursor.1.min(rope.len_chars()));
    let rope = rope.clone();
    doc.view.folds.reveal(&rope, line);
}

/// After folding: a cursor left inside the hidden lines goes to the end of the folded line.
fn cursor_out_of_folds(doc: &mut Document) {
    let rope = doc.buffer.rope();
    let line = rope.char_to_line(doc.cursor.1.min(rope.len_chars()));
    if doc.view.folds.hidden(line) {
        let header = doc.view.folds.shown(line);
        let end = rope.line_to_char(header) + rope.line(header).len_chars().saturating_sub(1);
        doc.cursor = (end, end);
        doc.view.preferred_x = None;
    }
}

/// Fold or open the region `line` starts.
pub fn toggle_fold(doc: &mut Document, line: usize) {
    doc.view.folds.sync(&doc.buffer);
    let rope = doc.buffer.rope().clone();
    doc.view.folds.toggle(&rope, line);
    cursor_out_of_folds(doc);
}

/// Fold the region around the cursor (Ctrl+Shift+[).
pub fn fold_at_cursor(doc: &mut Document) {
    doc.view.folds.sync(&doc.buffer);
    let rope = doc.buffer.rope().clone();
    let line = rope.char_to_line(doc.cursor.1.min(rope.len_chars()));
    doc.view.folds.fold_around(&rope, line);
    cursor_out_of_folds(doc);
}

/// Open the fold on the cursor's line (Ctrl+Shift+]).
pub fn unfold_at_cursor(doc: &mut Document) {
    doc.view.folds.sync(&doc.buffer);
    let rope = doc.buffer.rope().clone();
    let line = rope.char_to_line(doc.cursor.1.min(rope.len_chars()));
    doc.view.folds.unfold(&rope, line);
}

pub fn fold_all(doc: &mut Document) {
    doc.view.folds.sync(&doc.buffer);
    let rope = doc.buffer.rope().clone();
    doc.view.folds.fold_all(&rope);
    cursor_out_of_folds(doc);
}

pub fn unfold_all(doc: &mut Document) {
    doc.view.folds.clear();
}

/// A piece of a line, laid out.
struct Piece {
    galley: Arc<Galley>,
    /// Where it goes, from the line's top left (before horizontal scrolling).
    offset: Vec2,

}

/// A line on screen.
struct LineView {
    line: usize,
    /// Its first character in the document.
    start: usize,
    /// Its length, without the line break.
    len: usize,
    /// Its top on screen.
    top: f32,
    height: f32,
    pieces: Vec<Piece>,
    /// Laid out as fixed-width pieces (a long line): positions are worked out from the width of
    /// a character.
    chunked: bool,
}

/// Lays out lines.
struct Layout<'a> {
    ctx: egui::Context,
    font: FontId,
    row_height: f32,
    char_width: f32,
    /// The wrap width (`None`: no wrap).
    wrap: Option<f32>,
    palette: Palette,
    dark: bool,
    doc: &'a Document,
}

impl Layout<'_> {
    fn line_len(&self, line: usize) -> usize {
        let rope = self.doc.buffer.rope();
        let slice = rope.line(line);
        let mut len = slice.len_chars();
        if len > 0 && slice.char(len - 1) == '\n' {
            len -= 1;
        }
        len
    }

    /// Characters per row of a long line with word wrap.
    fn columns(&self) -> usize {
        match self.wrap {
            Some(w) => ((w / self.char_width) as usize).max(8),
            None => CHUNK,
        }
    }

    /// The height of `line` (without laying it out when it is long).
    fn height(&self, line: usize) -> f32 {
        let len = self.line_len(line);
        if len > LONG_LINE {
            return match self.wrap {
                Some(_) => len.div_ceil(self.columns()).max(1) as f32 * self.row_height,
                None => self.row_height,
            };
        }
        self.lay(line, None).pieces.first().map_or(self.row_height, |p| p.galley.size().y.max(self.row_height))
    }

    fn job(&self, text: &str, bytes_at: usize) -> LayoutJob {
        let mut job = LayoutJob::default();
        let plain = TextFormat::simple(self.font.clone(), self.palette.text);
        let spans = self.doc.highlight.spans_in(bytes_at..bytes_at + text.len());
        let mut at = 0;
        for (range, kind) in spans {
            let s = range.start.saturating_sub(bytes_at).max(at);
            let e = range.end.saturating_sub(bytes_at).min(text.len());
            if s >= e || !text.is_char_boundary(s) || !text.is_char_boundary(e) {
                continue;
            }
            if s > at {
                job.append(&text[at..s], 0.0, plain.clone());
            }
            job.append(&text[s..e], 0.0, TextFormat::simple(self.font.clone(), color(*kind, self.dark)));
            at = e;
        }
        if at < text.len() {
            job.append(&text[at..], 0.0, plain);
        }
        job
    }

    /// Lay out `line`. For a long line, only the pieces that meet `area` (relative to the line's
    /// top left, unscrolled) are laid out.
    fn lay(&self, line: usize, area: Option<Rect>) -> LineView {
        let rope = self.doc.buffer.rope();
        let start = rope.line_to_char(line);
        let len = self.line_len(line);
        if len <= LONG_LINE {
            let text = rope.slice(start..start + len).to_string();
            let mut job = self.job(&text, rope.char_to_byte(start));
            job.wrap.max_width = self.wrap.unwrap_or(f32::INFINITY);
            let galley = self.ctx.fonts_mut(|f| f.layout_job(job));
            let height = galley.size().y.max(self.row_height);
            return LineView { line, start, len, top: 0.0, height, pieces: vec![Piece { galley, offset: Vec2::ZERO }], chunked: false };
        }
        let cols = self.columns();
        let n = len.div_ceil(cols);
        let height = if self.wrap.is_some() { n as f32 * self.row_height } else { self.row_height };
        let wanted: Range<usize> = match (area, self.wrap) {
            (None, _) => 0..0,
            (Some(a), Some(_)) => {
                let first = (a.top().max(0.0) / self.row_height) as usize;
                let last = (a.bottom().max(0.0) / self.row_height) as usize + 1;
                first.min(n)..last.min(n)
            }
            (Some(a), None) => {
                let w = cols as f32 * self.char_width;
                let first = (a.left().max(0.0) / w) as usize;
                let last = (a.right().max(0.0) / w) as usize + 1;
                first.min(n)..last.min(n)
            }
        };
        let mut pieces = Vec::new();
        for k in wanted {
            let a = k * cols;
            let b = ((k + 1) * cols).min(len);
            let text = rope.slice(start + a..start + b).to_string();
            let job = self.job(&text, rope.char_to_byte(start + a));
            let galley = self.ctx.fonts_mut(|f| f.layout_job(job));
            let offset = if self.wrap.is_some() { vec2(0.0, k as f32 * self.row_height) } else { vec2(a as f32 * self.char_width, 0.0) };
            pieces.push(Piece { galley, offset });
        }
        LineView { line, start, len, top: 0.0, height, pieces, chunked: true }
    }
}

fn color(kind: Kind, dark: bool) -> Color32 {
    theme::syntax(kind, dark)
}

impl LineView {
    /// Where column `col` is, from the line's top left (unscrolled): the top of its row.
    fn pos_of(&self, col: usize, l: &Layout<'_>) -> Pos2 {
        if self.chunked {
            return match l.wrap {
                Some(_) => {
                    let cols = l.columns();
                    let (row, c) = if col == self.len && col > 0 && col.is_multiple_of(cols) { (col / cols - 1, cols) } else { (col / cols, col % cols) };
                    pos2(c as f32 * l.char_width, row as f32 * l.row_height)
                }
                None => pos2(col as f32 * l.char_width, 0.0),
            };
        }
        let p = &self.pieces[0];
        let r = p.galley.pos_from_cursor(CCursor::new(col.min(self.len)));
        pos2(r.min.x, r.min.y)
    }

    /// The column at `p` (relative to the line's top left, unscrolled).
    fn col_at(&self, p: Vec2, l: &Layout<'_>) -> usize {
        if self.chunked {
            let col = match l.wrap {
                Some(_) => {
                    let row = (p.y.max(0.0) / l.row_height) as usize;
                    row * l.columns() + ((p.x.max(0.0) / l.char_width).round() as usize).min(l.columns())
                }
                None => (p.x.max(0.0) / l.char_width).round() as usize,
            };
            return col.min(self.len);
        }
        self.pieces[0].galley.cursor_from_pos(p).index.0.min(self.len)
    }

    /// How many visual rows the line has.
    fn row_count(&self, l: &Layout<'_>) -> usize {
        if self.chunked {
            return match l.wrap {
                Some(_) => self.len.div_ceil(l.columns()).max(1),
                None => 1,
            };
        }
        self.pieces[0].galley.rows.len().max(1)
    }

    /// Row `i`: its top, height and columns.
    fn row(&self, i: usize, l: &Layout<'_>) -> (f32, f32, Range<usize>) {
        if self.chunked {
            return match l.wrap {
                Some(_) => {
                    let cols = l.columns();
                    (i as f32 * l.row_height, l.row_height, i * cols..((i + 1) * cols).min(self.len))
                }
                None => (0.0, l.row_height, 0..self.len),
            };
        }
        let rows = &self.pieces[0].galley.rows;
        let mut at = 0;
        for (k, row) in rows.iter().enumerate() {
            let n = row.row.char_count_excluding_newline().0;
            if k == i {
                let r = row.rect();
                return (r.min.y, r.height(), at..at + n);
            }
            at += n + usize::from(row.ends_with_newline);
        }
        (0.0, l.row_height, 0..self.len)
    }

    /// The row column `col` is on.
    fn row_of(&self, col: usize, l: &Layout<'_>) -> usize {
        if self.chunked {
            return match l.wrap {
                Some(_) => {
                    let cols = l.columns();
                    if col == self.len && col > 0 && col.is_multiple_of(cols) { col / cols - 1 } else { col / cols }
                }
                None => 0,
            };
        }
        let n = self.row_count(l);
        (0..n).find(|&i| {
            let (_, _, r) = self.row(i, l);
            col < r.end || (col == r.end && (i + 1 == n || r.is_empty()))
        })
        .unwrap_or(n - 1)
    }

    /// The rows between `top` and `bottom` (relative to the line's top).
    fn rows_between(&self, top: f32, bottom: f32, l: &Layout<'_>) -> Vec<(f32, f32, Range<usize>)> {
        let n = self.row_count(l);
        let (first, last) = if self.chunked && l.wrap.is_some() {
            ((top.max(0.0) / l.row_height) as usize, ((bottom.max(0.0) / l.row_height) as usize + 1).min(n))
        } else {
            (0, n)
        };
        (first.min(n)..last).map(|i| self.row(i, l)).filter(|(t, h, _)| t + h >= top && *t <= bottom).collect()
    }
}

/// The lines on screen, from the view's top line down to `height`.
fn visible(l: &Layout<'_>, view: &View, height: f32, width: f32) -> Vec<LineView> {
    let lines = l.doc.buffer.rope().len_lines();
    let mut out = Vec::new();
    let mut y = -view.top_offset;
    let mut line = view.top_line.min(lines.saturating_sub(1));
    while y < height && line < lines {
        let area = Rect::from_min_size(pos2(view.scroll_x, -y), vec2(width, height));
        let mut lv = l.lay(line, Some(area));
        lv.top = y;
        y += lv.height;
        out.push(lv);
        line = view.folds.next(line);
    }
    out
}

/// Scroll by `dy` pixels (positive: down).
fn scroll_by(l: &Layout<'_>, view: &mut View, dy: f32) {
    let lines = l.doc.buffer.rope().len_lines();
    let mut offset = view.top_offset + dy;
    let mut line = view.folds.shown(view.top_line.min(lines.saturating_sub(1)));
    while offset < 0.0
        && let Some(prev) = view.folds.prev(line)
    {
        line = prev;
        offset += l.height(line);
    }
    while view.folds.next(line) < lines {
        let h = l.height(line);
        if offset < h {
            break;
        }
        offset -= h;
        line = view.folds.next(line);
    }
    view.top_line = line;
    view.top_offset = offset.max(0.0);
    if view.folds.next(line) >= lines {
        view.top_offset = view.top_offset.min((l.height(line) - l.row_height).max(0.0));
    }
}

/// Bring character `at` into view.
fn reveal(l: &Layout<'_>, view: &mut View, at: usize, how: Reveal, height: f32, width: f32) {
    let rope = l.doc.buffer.rope();
    let line = rope.char_to_line(at.min(rope.len_chars()));
    let lv = l.lay(line, None);
    let p = lv.pos_of(at - lv.start, l);
    // Vertical.
    match how {
        Reveal::Center => {
            view.top_line = line;
            view.top_offset = p.y;
            scroll_by(l, view, -(height / 3.0));
        }
        Reveal::Near => {
            // Above the view: put it at the top.
            let above = line < view.top_line || (line == view.top_line && p.y < view.top_offset);
            if above {
                view.top_line = line;
                view.top_offset = p.y;
            } else {
                // Below the bottom: walk up from it until the view is full.
                let mut y = p.y + l.row_height;
                let mut first = line;
                let mut found = false;
                let mut probe = view.top_line;
                let mut acc = -view.top_offset;
                while probe <= line && acc < height {
                    if probe == line {
                        found = acc + p.y + l.row_height <= height;
                        break;
                    }
                    acc += l.height(probe);
                    probe = view.folds.next(probe);
                }
                if !found {
                    while y < height
                        && let Some(prev) = view.folds.prev(first)
                    {
                        first = prev;
                        y += l.height(first);
                    }
                    view.top_line = first;
                    view.top_offset = (y - height).max(0.0);
                }
            }
        }
    }
    // Horizontal.
    if l.wrap.is_none() {
        let margin = 4.0 * l.char_width;
        if p.x < view.scroll_x + margin {
            view.scroll_x = (p.x - margin).max(0.0);
        } else if p.x > view.scroll_x + width - margin {
            view.scroll_x = p.x - width + margin;
        }
    } else {
        view.scroll_x = 0.0;
    }
}

/// The character at screen point `p`.
fn char_at(l: &Layout<'_>, lines: &[LineView], view: &View, origin: Pos2, p: Pos2) -> usize {
    let local = p - origin + vec2(view.scroll_x, 0.0);
    let Some(first) = lines.first() else { return 0 };
    if local.y < first.top {
        return first.start + first.col_at(vec2(local.x, 0.0), l);
    }
    for lv in lines {
        if local.y < lv.top + lv.height {
            return lv.start + lv.col_at(vec2(local.x, local.y - lv.top), l);
        }
    }
    let last = &lines[lines.len() - 1];
    last.start + last.col_at(vec2(local.x, last.height - 1.0), l)
}

/// Character classes for word moves.
fn class(c: char) -> u8 {
    if c.is_alphanumeric() || c == '_' {
        2
    } else if c.is_whitespace() {
        0
    } else {
        1
    }
}

fn word_left(doc: &Document, at: usize) -> usize {
    let rope = doc.buffer.rope();
    let mut i = at;
    while i > 0 && class(rope.char(i - 1)) == 0 && rope.char(i - 1) != '\n' {
        i -= 1;
    }
    if i > 0 && rope.char(i - 1) == '\n' && i == at {
        return i - 1;
    }
    if i > 0 {
        let c = class(rope.char(i - 1));
        while i > 0 && class(rope.char(i - 1)) == c && rope.char(i - 1) != '\n' {
            i -= 1;
        }
    }
    i
}

fn word_right(doc: &Document, at: usize) -> usize {
    let rope = doc.buffer.rope();
    let len = rope.len_chars();
    let mut i = at;
    if i < len && rope.char(i) == '\n' {
        return i + 1;
    }
    if i < len {
        let c = class(rope.char(i));
        while i < len && class(rope.char(i)) == c && rope.char(i) != '\n' {
            i += 1;
        }
    }
    while i < len && class(rope.char(i)) == 0 && rope.char(i) != '\n' {
        i += 1;
    }
    i
}

/// The word around `at`.
fn word_at(doc: &Document, at: usize) -> (usize, usize) {
    let rope = doc.buffer.rope();
    let len = rope.len_chars();
    if len == 0 {
        return (0, 0);
    }
    let probe = if at < len && rope.char(at) != '\n' { at } else { at.saturating_sub(1) };
    let c = class(rope.char(probe));
    let (mut a, mut b) = (probe, probe + 1);
    while a > 0 && class(rope.char(a - 1)) == c && rope.char(a - 1) != '\n' {
        a -= 1;
    }
    while b < len && class(rope.char(b)) == c && rope.char(b) != '\n' {
        b += 1;
    }
    (a, b)
}

/// The line around `at`, with its line break.
fn line_at(doc: &Document, at: usize) -> (usize, usize) {
    let rope = doc.buffer.rope();
    let line = rope.char_to_line(at);
    let start = rope.line_to_char(line);
    let end = if line + 1 < rope.len_lines() { rope.line_to_char(line + 1) } else { rope.len_chars() };
    (start, end)
}

/// The cursor `rows` visual rows up (negative) or down from `at`, keeping `x`.
fn vertical(l: &Layout<'_>, at: usize, rows: isize, x: f32) -> usize {
    let rope = l.doc.buffer.rope();
    let lines = rope.len_lines();
    let mut line = rope.char_to_line(at);
    let mut lv = l.lay(line, None);
    let mut row = lv.row_of(at - lv.start, l);
    let mut left = rows;
    while left != 0 {
        if left > 0 {
            if row + 1 < lv.row_count(l) {
                row += 1;
            } else if l.doc.view.folds.next(line) < lines {
                line = l.doc.view.folds.next(line);
                lv = l.lay(line, None);
                row = 0;
            } else {
                return rope.len_chars();
            }
            left -= 1;
        } else {
            if row > 0 {
                row -= 1;
            } else if let Some(prev) = l.doc.view.folds.prev(line) {
                line = prev;
                lv = l.lay(line, None);
                row = lv.row_count(l) - 1;
            } else {
                return 0;
            }
            left += 1;
        }
    }
    let (top, h, _) = lv.row(row, l);
    lv.start + lv.col_at(vec2(x, top + h / 2.0), l)
}

/// Draw `doc` and handle its input.
pub fn show(ui: &mut Ui, doc: &mut Document, look: &Look, requests: Requests) -> Output {
    let palette = theme::palette(look.dark);
    let id = editor_id(doc.id);
    let full = ui.available_rect_before_wrap();
    let response = ui.interact(full, id, Sense::click_and_drag());
    let ctx = ui.ctx().clone();
    let row_height = ctx.fonts_mut(|f| f.row_height(&look.font));
    let char_width = ctx.fonts_mut(|f| f.glyph_width(&look.font, '0'));
    let lines_total = doc.buffer.rope().len_lines();
    // Follow the edits with the folds; the top of the view is never a hidden line.
    doc.view.folds.sync(&doc.buffer);
    if !doc.view.folds.is_empty() {
        let shown = doc.view.folds.shown(doc.view.top_line);
        if shown != doc.view.top_line {
            doc.view.top_line = shown;
            doc.view.top_offset = 0.0;
        }
    }
    let fold_width = if look.folding { (char_width * 2.0).max(16.0).round() } else { 0.0 };
    let numbers_width = if look.line_numbers { (lines_total.to_string().len().max(2) as f32) * char_width + 18.0 } else { 6.0 };
    let gutter_width = numbers_width + fold_width;
    let gutter = Rect::from_min_size(full.min, vec2(gutter_width, full.height()));
    let hbar = !look.wrap;
    let text_rect = Rect::from_min_max(
        pos2(full.min.x + gutter_width + PAD_X, full.min.y + 2.0),
        pos2(full.max.x - SCROLLBAR, full.max.y - if hbar { SCROLLBAR } else { 0.0 }),
    );
    let width = text_rect.width().max(10.0);
    let height = text_rect.height().max(row_height);
    let wrap = if look.wrap { Some(width - char_width) } else { None };

    if requests.focus || (response.clicked() || response.drag_started()) {
        ui.memory_mut(|m| m.request_focus(id));
    }
    let focused = ui.memory(|m| m.has_focus(id));
    if focused {
        ui.memory_mut(|m| m.set_focus_lock_filter(id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: false }));
    }
    if focused && !doc.view.had_focus {
        doc.view.ime_generation += 1;
    }
    doc.view.had_focus = focused;

    let mut changed = false;
    let mut indent_request = None;
    let mut context_menu = None;
    if let Some((a, b)) = requests.select {
        let len = doc.len_chars();
        doc.cursor = (a.min(len), b.min(len));
        doc.view.reveal = Some(Reveal::Center);
        doc.view.preferred_x = None;
        unfold_at_cursor_line(doc);
    }

    // ── keyboard and text ─────────────────────────────────────────────────────────────────
    if focused {
        let events = ui.input(|i| i.events.clone());
        let l = Layout { ctx: ctx.clone(), font: look.font.clone(), row_height, char_width, wrap, palette, dark: look.dark, doc };
        let mut ops: Vec<Op> = Vec::new();
        let mut cursor = l.doc.cursor;
        for event in &events {
            match event {
                Event::Text(t) if !t.is_empty() && !t.chars().any(|c| c.is_control() && c != '\t') => {
                    ops.push(Op::Insert(t.clone(), Group::Typing));
                }
                Event::Paste(t) => ops.push(Op::Insert(crate::text::normalize_newlines(t), Group::Alone)),
                Event::Copy | Event::Cut => {
                    let (a, b) = (cursor.0.min(cursor.1), cursor.0.max(cursor.1));
                    let (a, b) = if a == b { line_at(l.doc, a) } else { (a, b) };
                    ctx.copy_text(l.doc.buffer.slice_string(a..b));
                    if matches!(event, Event::Cut) {
                        ops.push(Op::Delete(a..b, Group::Alone));
                        cursor = (a, a);
                    }
                }
                Event::Ime(ime) => match ime {
                    egui::ImeEvent::Preedit { text, .. } => ops.push(Op::Preedit(text.clone())),
                    egui::ImeEvent::Commit(t) => {
                        ops.push(Op::Preedit(String::new()));
                        if !t.is_empty() {
                            ops.push(Op::Insert(crate::text::normalize_newlines(t), Group::Typing));
                        }
                    }
                    _ => {}
                },
                Event::Key { key, pressed: true, modifiers, .. } => {
                    if let Some(op) = key_op(*key, *modifiers) {
                        ops.push(op);
                    }
                }
                _ => {}
            }
        }
        drop(l);
        for op in ops {
            changed |= apply(doc, op, look, &ctx, row_height, char_width, wrap, palette, height, &mut indent_request);
        }
        // The cursor never stays inside a folded region (moving into one opens it).
        doc.view.folds.sync(&doc.buffer);
        unfold_at_cursor_line(doc);
    }

    // ── mouse ─────────────────────────────────────────────────────────────────────────────
    {
        let l = Layout { ctx: ctx.clone(), font: look.font.clone(), row_height, char_width, wrap, palette, dark: look.dark, doc };
        let lines = visible(&l, &doc.view, height, width);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let shift = ui.input(|i| i.modifiers.shift);
        let mut set_cursor = None;
        let mut drag = doc.view.drag;
        let mut clicks = doc.view.clicks;
        let on_text = |p: Pos2| text_rect.contains(p) || (p.x >= gutter.left() && p.x < text_rect.left() && p.y >= text_rect.top() && p.y <= text_rect.bottom());
        // A press and its release (or a double click) can arrive in the same frame, as with a
        // tap on a touch screen: count the presses in the events rather than asking whether the
        // button is down.
        let presses = ui.input(|i| {
            i.events
                .iter()
                .filter(|e| matches!(e, Event::PointerButton { button: egui::PointerButton::Primary, pressed: true, .. }))
                .count()
        });
        // A click on a − or + in the gutter, or on the "…" after a folded line.
        let in_fold_column = |p: Pos2| look.folding && p.x >= gutter.right() - fold_width && p.x < gutter.right() && p.y >= text_rect.top();
        let mut fold_click = None;
        if presses > 0
            && (response.contains_pointer() || response.clicked())
            && let Some(p) = pointer
        {
            if let Some((_, line)) = doc.view.fold_pills.iter().find(|(r, _)| r.contains(p)) {
                fold_click = Some(*line);
            } else if in_fold_column(p) {
                let y = p.y - text_rect.top();
                if let Some(lv) = lines.iter().find(|lv| y >= lv.top && y < lv.top + lv.height)
                    && (doc.view.folds.is_folded(lv.line) || folding::foldable(doc.buffer.rope(), lv.line))
                {
                    fold_click = Some(lv.line);
                }
            }
        }
        if presses > 0
            && fold_click.is_none()
            && (response.is_pointer_button_down_on() || response.contains_pointer() || response.clicked())
            && let Some(p) = pointer
            && on_text(p)
            && doc.view.bar_drag.is_none()
        {
            let at = char_at(&l, &lines, &doc.view, text_rect.min, p);
            let now = Instant::now();
            let mut count = 0;
            for _ in 0..presses {
                count = match clicks {
                    Some((t, q, n)) if now.duration_since(t) < Duration::from_millis(450) && (q - p).length() < 6.0 => n % 3 + 1,
                    _ => 1,
                };
                clicks = Some((now, p, count));
            }
            let in_gutter = p.x < text_rect.left();
            let unit = if in_gutter { Unit::Line } else { [Unit::Char, Unit::Word, Unit::Line][count as usize - 1] };
            let sel = match unit {
                Unit::Char => (at, at),
                Unit::Word => word_at(l.doc, at),
                Unit::Line => line_at(l.doc, at),
            };
            let sel = if shift && unit == Unit::Char { (l.doc.cursor.0, at) } else { sel };
            drag = Some((unit, sel));
            set_cursor = Some(sel);
        } else if response.dragged()
            && let (Some((unit, origin)), Some(p)) = (drag, pointer)
        {
            let at = char_at(&l, &lines, &doc.view, text_rect.min, p);
            let (a, b) = match unit {
                Unit::Char => (at, at),
                Unit::Word => word_at(l.doc, at),
                Unit::Line => line_at(l.doc, at),
            };
            let (o0, o1) = (origin.0.min(origin.1), origin.0.max(origin.1));
            let sel = if unit == Unit::Char {
                (origin.0, at)
            } else if a < o0 {
                (o1, a)
            } else {
                (o0, b.max(o1))
            };
            set_cursor = Some(sel);
            // Dragging past an edge scrolls.
            if p.y < text_rect.top() || p.y > text_rect.bottom() || p.x < text_rect.left() || p.x > text_rect.right() {
                ctx.request_repaint();
            }
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            drag = None;
        }
        drop(lines);
        drop(l);
        doc.view.drag = drag;
        doc.view.clicks = clicks;
        if let Some(line) = fold_click {
            toggle_fold(doc, line);
        }
        // Right click or long press: the context menu, with the cursor moved there unless it is
        // on the selection.
        let secondary = ui.input(|i| {
            i.events.iter().any(|e| matches!(e, Event::PointerButton { button: egui::PointerButton::Secondary, pressed: true, .. }))
        });
        if ((secondary && response.contains_pointer()) || response.long_touched())
            && let Some(p) = pointer.filter(|p| text_rect.contains(*p))
        {
            let l = Layout { ctx: ctx.clone(), font: look.font.clone(), row_height, char_width, wrap, palette, dark: look.dark, doc };
            let lines = visible(&l, &doc.view, height, width);
            let at = char_at(&l, &lines, &doc.view, text_rect.min, p);
            drop(lines);
            drop(l);
            let (a, b) = (doc.cursor.0.min(doc.cursor.1), doc.cursor.0.max(doc.cursor.1));
            if at < a || at > b || a == b {
                doc.cursor = (at, at);
                doc.view.preferred_x = None;
            }
            context_menu = Some(p);
            ui.memory_mut(|m| m.request_focus(id));
        }
        if let Some(sel) = set_cursor {
            if sel != doc.cursor {
                doc.cursor = sel;
            }
            doc.view.preferred_x = None;
            doc.view.reveal = Some(Reveal::Near);
        }
    }

    // ── scrolling ─────────────────────────────────────────────────────────────────────────
    {
        let mut view = std::mem::take(&mut doc.view);
        let l = Layout { ctx: ctx.clone(), font: look.font.clone(), row_height, char_width, wrap, palette, dark: look.dark, doc };
        if response.hovered() || response.contains_pointer() {
            let delta = ui.input(|i| i.smooth_scroll_delta);
            if delta.y != 0.0 {
                scroll_by(&l, &mut view, -delta.y);
            }
            if delta.x != 0.0 && wrap.is_none() {
                view.scroll_x = (view.scroll_x - delta.x).max(0.0);
            }
        }
        // Dragging the selection past the top or bottom.
        if view.drag.is_some()
            && let Some(p) = ui.input(|i| i.pointer.interact_pos())
        {
            if p.y < text_rect.top() {
                scroll_by(&l, &mut view, -row_height);
            } else if p.y > text_rect.bottom() {
                scroll_by(&l, &mut view, row_height);
            }
        }
        if let Some(how) = view.reveal.take() {
            reveal(&l, &mut view, l.doc.cursor.1, how, height, width);
        }
        drop(l);
        doc.view = view;
    }

    // ── paint ─────────────────────────────────────────────────────────────────────────────
    let painter = ui.painter_at(full);
    painter.rect_filled(full, 0.0, palette.editor_bg);
    let l = Layout { ctx: ctx.clone(), font: look.font.clone(), row_height, char_width, wrap, palette, dark: look.dark, doc };
    let lines = visible(&l, &doc.view, height, width);
    let text_painter = ui.painter_at(text_rect.expand2(vec2(PAD_X, 2.0)));
    let origin = text_rect.min - vec2(doc.view.scroll_x, 0.0);
    let rope = doc.buffer.rope();
    let (sa, sb) = (doc.cursor.0.min(doc.cursor.1), doc.cursor.0.max(doc.cursor.1));
    let head = doc.cursor.1.min(rope.len_chars());
    let head_line = rope.char_to_line(head);
    let mut cursor_rect = None;
    let mut widest: f32 = 0.0;
    let mut pills: Vec<(Rect, usize)> = Vec::new();
    // The line whose − or + is under the pointer (brighter).
    let fold_hover = ui.input(|i| i.pointer.hover_pos()).filter(|p| look.folding && p.x >= gutter.right() - fold_width && p.x < gutter.right()).and_then(|p| {
        let y = p.y - text_rect.top();
        lines.iter().find(|lv| y >= lv.top && y < lv.top + lv.height).map(|lv| lv.line)
    });

    // Find matches on screen (bytes).
    let mut marks: Vec<Range<usize>> = Vec::new();
    if let (Some(re), Some(first), Some(last)) = (&requests.marks, lines.first(), lines.last()) {
        let a = rope.char_to_byte(first.start);
        let b = rope.char_to_byte((last.start + last.len).min(rope.len_chars()));
        if b - a <= 2_000_000 {
            let text = rope.byte_slice(a..b).to_string();
            marks = re.find_iter(&text).filter(|m| !m.is_empty()).take(5000).map(|m| m.start() + a..m.end() + a).collect();
        }
    }

    for lv in &lines {
        let top = origin.y + lv.top;
        let rows = lv.rows_between(-lv.top, height - lv.top, &l);
        // The current line.
        if look.highlight_line && lv.line == head_line && sa == sb {
            let p = lv.pos_of(head - lv.start, &l);
            let row_h = rows.iter().find(|(t, _, _)| (*t - p.y).abs() < 0.5).map_or(row_height, |r| r.1);
            painter.rect_filled(Rect::from_min_size(pos2(text_rect.left() - PAD_X, top + p.y), vec2(text_rect.width() + PAD_X, row_h)), 0.0, palette.current_line);
        }
        // Selection and find matches, per row.
        let paint_range = |a: usize, b: usize, fill: Color32, through_end: bool| {
            for (row_top, row_h, cols) in &rows {
                let (ra, rb) = (cols.start, cols.end);
                if b < ra || a > rb || (a == b) {
                    continue;
                }
                let x0 = if a > ra { lv.pos_of(a, &l).x } else { lv.pos_of(ra, &l).x };
                let mut x1 = if b < rb { lv.pos_of(b, &l).x } else { lv.pos_of(rb, &l).x };
                if b > rb && through_end && rb == lv.len {
                    x1 += char_width * 0.6;
                }
                if x1 > x0 {
                    text_painter.rect_filled(Rect::from_min_max(pos2(origin.x + x0, top + row_top), pos2(origin.x + x1, top + row_top + row_h)), 2.0, fill);
                }
            }
        };
        let line_end = lv.start + lv.len;
        for m in &marks {
            let (ma, mb) = (rope.byte_to_char(m.start), rope.byte_to_char(m.end));
            if mb < lv.start || ma > line_end {
                continue;
            }
            let fill = if requests.current_mark.as_ref() == Some(m) { palette.find_current } else { palette.find_match };
            paint_range(ma.max(lv.start) - lv.start, mb.min(line_end) - lv.start, fill, false);
        }
        if sb > lv.start && sa <= line_end + 1 && sa != sb {
            let sel = if focused { ui.visuals().selection.bg_fill } else { ui.visuals().selection.bg_fill.gamma_multiply(0.5) };
            paint_range(sa.max(lv.start) - lv.start, sb.min(line_end + 1).min(line_end) - lv.start, sel, sb > line_end);
            if sb > line_end && lv.len == 0 {
                text_painter.rect_filled(Rect::from_min_size(pos2(origin.x, top), vec2(char_width * 0.6, row_height)), 0.0, sel);
            }
        }
        // The text.
        for p in &lv.pieces {
            let at = origin + vec2(p.offset.x, lv.top + p.offset.y);
            text_painter.galley(at, p.galley.clone(), palette.text);
            widest = widest.max(p.offset.x + p.galley.size().x);
        }
        if lv.chunked && wrap.is_none() {
            widest = widest.max(lv.len as f32 * char_width);
        }
        // The cursor.
        if lv.line == head_line {
            let p = lv.pos_of(head - lv.start, &l);
            let rect = Rect::from_min_size(pos2(origin.x + p.x, top + p.y), vec2(2.0, row_height));
            cursor_rect = Some(rect);
        }
        // Line number.
        if look.line_numbers {
            let color = if lv.line == head_line { palette.gutter_current } else { palette.gutter_text };
            let gp = ui.painter_at(gutter);
            gp.text(pos2(gutter.right() - fold_width - 8.0, top), Align2::RIGHT_TOP, (lv.line + 1).to_string(), look.font.clone(), color);
        }
        // Folding: − on a region's first line, + on a folded one, and "…" after it.
        if look.folding {
            let folded = doc.view.folds.is_folded(lv.line);
            if folded || folding::foldable(rope, lv.line) {
                let size = (row_height * 0.7).round().min(fold_width - 4.0);
                let center = pos2(gutter.right() - fold_width / 2.0 - 1.0, top + row_height / 2.0);
                let r = Rect::from_center_size(center, vec2(size, size));
                let gp = ui.painter_at(gutter);
                let color = if folded || fold_hover == Some(lv.line) { palette.gutter_current } else { palette.gutter_text.gamma_multiply(0.8) };
                let stroke = Stroke::new(1.2, color);
                gp.rect_stroke(r, 2.0, stroke, egui::StrokeKind::Inside);
                let arm = size * 0.28;
                gp.hline((center.x - arm)..=(center.x + arm), center.y, stroke);
                if folded {
                    gp.vline(center.x, (center.y - arm)..=(center.y + arm), stroke);
                }
            }
            if folded {
                let end = lv.pos_of(lv.len, &l);
                let pill = Rect::from_min_size(pos2(origin.x + end.x + char_width, top + end.y + row_height * 0.15), vec2(char_width * 3.0, row_height * 0.7));
                text_painter.rect_filled(pill, row_height * 0.35, palette.gutter_text.gamma_multiply(0.3));
                for k in [-1.0, 0.0, 1.0] {
                    text_painter.circle_filled(pill.center() + vec2(k * char_width * 0.7, 0.0), (row_height * 0.06).max(1.2), palette.text);
                }
                pills.push((pill, lv.line));
            }
        }
    }

    // The word being composed on the on-screen keyboard, over the text at the cursor.
    if let (Some(pre), Some(c)) = (&doc.view.preedit, cursor_rect)
        && !pre.is_empty()
    {
        let g = ctx.fonts_mut(|f| f.layout_no_wrap(pre.clone(), look.font.clone(), palette.text));
        let r = Rect::from_min_size(c.min, g.size());
        text_painter.rect_filled(r.expand(1.0), 2.0, palette.editor_bg);
        text_painter.galley(c.min, g.clone(), palette.text);
        text_painter.hline(r.x_range(), r.bottom(), Stroke::new(1.0, palette.text));
        cursor_rect = Some(Rect::from_min_size(pos2(r.right(), c.top()), c.size()));
    }
    if let Some(c) = cursor_rect
        && focused
    {
        text_painter.rect_filled(c, 0.0, ui.visuals().text_cursor.stroke.color);
    }
    if look.line_numbers {
        ui.painter_at(gutter).vline(gutter.right() - 1.0, gutter.y_range(), Stroke::new(1.0, palette.gutter_text.gamma_multiply(0.25)));
    }

    // The visible bytes, for the highlighting.
    let visible_bytes = match (lines.first(), lines.last()) {
        (Some(a), Some(b)) => rope.char_to_byte(a.start)..rope.char_to_byte((b.start + b.len).min(rope.len_chars())),
        _ => 0..0,
    };
    drop(lines);
    drop(l);
    doc.view.fold_pills = pills;
    // The widest line on screen sets how far the view scrolls sideways.
    doc.view.widest = widest;
    if wrap.is_none() {
        let max_scroll = (widest + 8.0 * char_width - width).max(0.0);
        doc.view.scroll_x = doc.view.scroll_x.min(max_scroll);
    }

    // ── scroll bars ───────────────────────────────────────────────────────────────────────
    scrollbars(ui, doc, full, text_rect, hbar, row_height, char_width, wrap, look, palette);

    // The on-screen keyboard follows the focused editor; the IME window goes to the cursor.
    if focused && let Some(c) = cursor_rect {
        ctx.output_mut(|o| o.ime = Some(egui::output::IMEOutput::new(id, doc.view.ime_generation, egui::IMEPurpose::Normal, text_rect, c)));
    }
    if response.hovered() {
        let on_pill = ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| doc.view.fold_pills.iter().any(|(r, _)| r.contains(p)));
        let on_marker = fold_hover.is_some_and(|line| doc.view.folds.is_folded(line) || folding::foldable(doc.buffer.rope(), line));
        ctx.set_cursor_icon(if on_pill || on_marker { CursorIcon::PointingHand } else { CursorIcon::Text });
    }
    ui.advance_cursor_after_rect(full);
    Output { changed, focused, visible_bytes, indent: indent_request, context_menu }
}

#[allow(clippy::too_many_arguments)]
fn scrollbars(ui: &mut Ui, doc: &mut Document, full: Rect, text_rect: Rect, hbar: bool, row_height: f32, char_width: f32, wrap: Option<f32>, look: &Look, palette: Palette) {
    let lines = doc.buffer.rope().len_lines().max(1);
    let track = Rect::from_min_max(pos2(full.right() - SCROLLBAR, full.top()), pos2(full.right(), full.bottom() - if hbar { SCROLLBAR } else { 0.0 }));
    let rows_on_screen = (text_rect.height() / row_height).max(1.0);
    let thumb_h = (track.height() * rows_on_screen / (lines as f32 + rows_on_screen)).clamp(24.0, track.height());
    let frac = doc.view.top_line as f32 / lines.saturating_sub(1).max(1) as f32;
    let thumb = Rect::from_min_size(pos2(track.left() + 2.0, track.top() + frac * (track.height() - thumb_h)), vec2(SCROLLBAR - 4.0, thumb_h));
    let vid = Id::unique(("dexnotas-vbar", doc.id));
    let vresp = ui.interact(track, vid, Sense::click_and_drag());
    let p = ui.painter_at(full);
    let active = vresp.hovered() || vresp.dragged();
    p.rect_filled(thumb, 3.0, palette.gutter_text.gamma_multiply(if active { 0.8 } else { 0.45 }));
    if vresp.drag_started() || vresp.clicked() {
        if let Some(pos) = vresp.interact_pointer_pos() {
            let f = ((pos.y - track.top() - thumb_h / 2.0) / (track.height() - thumb_h).max(1.0)).clamp(0.0, 1.0);
            doc.view.top_line = (f * lines.saturating_sub(1) as f32).round() as usize;
            doc.view.top_offset = 0.0;
        }
    } else if vresp.dragged()
        && let Some(pos) = vresp.interact_pointer_pos()
    {
        let f = ((pos.y - track.top() - thumb_h / 2.0) / (track.height() - thumb_h).max(1.0)).clamp(0.0, 1.0);
        doc.view.top_line = (f * lines.saturating_sub(1) as f32).round() as usize;
        doc.view.top_offset = 0.0;
    }
    if hbar && wrap.is_none() {
        let htrack = Rect::from_min_max(pos2(text_rect.left(), full.bottom() - SCROLLBAR), pos2(full.right() - SCROLLBAR, full.bottom()));
        let content = (doc.view.widest + 8.0 * char_width).max(text_rect.width());
        let tw = (htrack.width() * text_rect.width() / content).clamp(24.0, htrack.width());
        let max_scroll = (content - text_rect.width()).max(0.0);
        doc.view.scroll_x = doc.view.scroll_x.min(max_scroll.max(doc.view.scroll_x.min(content)));
        let hf = if max_scroll > 0.0 { doc.view.scroll_x / max_scroll } else { 0.0 };
        let thumb = Rect::from_min_size(pos2(htrack.left() + hf.clamp(0.0, 1.0) * (htrack.width() - tw), htrack.top() + 2.0), vec2(tw, SCROLLBAR - 4.0));
        let hid = Id::unique(("dexnotas-hbar", doc.id));
        let hresp = ui.interact(htrack, hid, Sense::click_and_drag());
        if max_scroll > 0.0 {
            p.rect_filled(thumb, 3.0, palette.gutter_text.gamma_multiply(if hresp.hovered() || hresp.dragged() { 0.8 } else { 0.45 }));
        }
        if (hresp.dragged() || hresp.clicked() || hresp.drag_started())
            && let Some(pos) = hresp.interact_pointer_pos()
        {
            let f = ((pos.x - htrack.left() - tw / 2.0) / (htrack.width() - tw).max(1.0)).clamp(0.0, 1.0);
            doc.view.scroll_x = f * max_scroll;
        }
    }
    let _ = look;
}

/// What one input does.
enum Op {
    Insert(String, Group),
    Delete(Range<usize>, Group),
    Preedit(String),
    Move { to: Motion, select: bool },
    Backspace { word: bool },
    DeleteForward { word: bool },
    Enter,
    Tab { outdent: bool },
    SelectAll,
    Undo,
    Redo,
}

#[derive(Clone, Copy)]
enum Motion {
    Left { word: bool },
    Right { word: bool },
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    DocStart,
    DocEnd,
}

fn key_op(key: Key, m: egui::Modifiers) -> Option<Op> {
    let ctrl = m.ctrl || m.command || m.mac_cmd;
    let select = m.shift;
    let mv = |to| Some(Op::Move { to, select });
    match key {
        Key::ArrowLeft => mv(Motion::Left { word: ctrl }),
        Key::ArrowRight => mv(Motion::Right { word: ctrl }),
        Key::ArrowUp if !ctrl => mv(Motion::Up),
        Key::ArrowDown if !ctrl => mv(Motion::Down),
        Key::PageUp => mv(Motion::PageUp),
        Key::PageDown => mv(Motion::PageDown),
        Key::Home => mv(if ctrl { Motion::DocStart } else { Motion::Home }),
        Key::End => mv(if ctrl { Motion::DocEnd } else { Motion::End }),
        Key::Backspace => Some(Op::Backspace { word: ctrl }),
        Key::Delete => Some(Op::DeleteForward { word: ctrl }),
        Key::Enter if !ctrl && !m.alt => Some(Op::Enter),
        Key::Tab if !ctrl && !m.alt => Some(Op::Tab { outdent: m.shift }),
        Key::A if ctrl && !m.shift => Some(Op::SelectAll),
        Key::Z if ctrl && !m.shift => Some(Op::Undo),
        Key::Z if ctrl && m.shift => Some(Op::Redo),
        Key::Y if ctrl => Some(Op::Redo),
        _ => None,
    }
}

/// Run one input on the document; whether the text changed.
#[allow(clippy::too_many_arguments)]
fn apply(
    doc: &mut Document,
    op: Op,
    look: &Look,
    ctx: &egui::Context,
    row_height: f32,
    char_width: f32,
    wrap: Option<f32>,
    palette: Palette,
    height: f32,
    indent_request: &mut Option<bool>,
) -> bool {
    let len = doc.len_chars();
    let (anchor, head) = (doc.cursor.0.min(len), doc.cursor.1.min(len));
    let (a, b) = (anchor.min(head), anchor.max(head));
    let mut changed = false;
    let mut keep_x = false;
    match op {
        Op::Insert(text, group) => {
            let end = a + text.chars().count();
            doc.replace(a..b, &text, (end, end), group);
            doc.view.preedit = None;
            changed = true;
        }
        Op::Delete(range, group) => {
            if !range.is_empty() {
                doc.replace(range.clone(), "", (range.start, range.start), group);
                changed = true;
            }
        }
        Op::Preedit(t) => doc.view.preedit = if t.is_empty() { None } else { Some(t) },
        Op::Backspace { word } => {
            if a != b {
                doc.replace(a..b, "", (a, a), Group::Alone);
                changed = true;
            } else if head > 0 {
                let from = if word {
                    word_left(doc, head)
                } else {
                    // Back over a whole indent step in leading spaces.
                    let rope = doc.buffer.rope();
                    let line_start = rope.line_to_char(rope.char_to_line(head));
                    let before = rope.slice(line_start..head).to_string();
                    let w = look.indent_width.max(1);
                    if !before.is_empty() && before.chars().all(|c| c == ' ') && look.indent_unit.starts_with(' ') {
                        let n = before.chars().count();
                        head - (if n.is_multiple_of(w) { w } else { n % w }).min(n)
                    } else {
                        head - 1
                    }
                };
                doc.replace(from..head, "", (from, from), Group::Deleting);
                changed = true;
            }
        }
        Op::DeleteForward { word } => {
            if a != b {
                doc.replace(a..b, "", (a, a), Group::Alone);
                changed = true;
            } else if head < len {
                let to = if word { word_right(doc, head) } else { head + 1 };
                doc.replace(head..to, "", (head, head), Group::Deleting);
                changed = true;
            }
        }
        Op::Enter => {
            let indent = if look.auto_indent {
                let rope = doc.buffer.rope();
                let line_start = rope.line_to_char(rope.char_to_line(a));
                rope.slice(line_start..a).chars().take_while(|c| *c == ' ' || *c == '\t').collect::<String>()
            } else {
                String::new()
            };
            let text = format!("\n{indent}");
            let end = a + text.chars().count();
            doc.replace(a..b, &text, (end, end), Group::Alone);
            changed = true;
        }
        Op::Tab { outdent } => {
            let multi_line = doc.buffer.rope().slice(a..b).chars().any(|c| c == '\n');
            if outdent || multi_line {
                *indent_request = Some(!outdent);
            } else {
                let text = if look.indent_unit.starts_with(' ') {
                    let rope = doc.buffer.rope();
                    let line_start = rope.line_to_char(rope.char_to_line(a));
                    let col = a - line_start;
                    let w = look.indent_width.max(1);
                    " ".repeat(w - col % w)
                } else {
                    "\t".to_owned()
                };
                let end = a + text.chars().count();
                doc.replace(a..b, &text, (end, end), Group::Typing);
                changed = true;
            }
        }
        Op::SelectAll => doc.cursor = (0, len),
        Op::Undo => changed = doc.undo(),
        Op::Redo => changed = doc.redo(),
        Op::Move { to, select } => {
            let l = Layout { ctx: ctx.clone(), font: look.font.clone(), row_height, char_width, wrap, palette, dark: look.dark, doc };
            let x = doc.view.preferred_x.unwrap_or_else(|| {
                let rope = l.doc.buffer.rope();
                let line = rope.char_to_line(head);
                let lv = l.lay(line, None);
                lv.pos_of(head - lv.start, &l).x
            });
            let collapse = a != b && !select;
            let target = match to {
                Motion::Left { .. } if collapse => a,
                Motion::Right { .. } if collapse => b,
                Motion::Left { word } => {
                    if word {
                        word_left(l.doc, head)
                    } else {
                        head.saturating_sub(1)
                    }
                }
                Motion::Right { word } => {
                    if word {
                        word_right(l.doc, head)
                    } else {
                        (head + 1).min(len)
                    }
                }
                Motion::Up => {
                    keep_x = true;
                    vertical(&l, head, -1, x)
                }
                Motion::Down => {
                    keep_x = true;
                    vertical(&l, head, 1, x)
                }
                Motion::PageUp => {
                    keep_x = true;
                    vertical(&l, head, -((height / row_height) as isize - 1).max(1), x)
                }
                Motion::PageDown => {
                    keep_x = true;
                    vertical(&l, head, ((height / row_height) as isize - 1).max(1), x)
                }
                Motion::Home => {
                    // To the first non-blank character, then to the start of the line.
                    let rope = l.doc.buffer.rope();
                    let start = rope.line_to_char(rope.char_to_line(head));
                    let first = start + rope.line(rope.char_to_line(head)).chars().take_while(|c| *c == ' ' || *c == '\t').count();
                    let first = first.min(start + l.line_len(rope.char_to_line(head)));
                    if head == first { start } else { first }
                }
                Motion::End => {
                    let rope = l.doc.buffer.rope();
                    let line = rope.char_to_line(head);
                    rope.line_to_char(line) + l.line_len(line)
                }
                Motion::DocStart => 0,
                Motion::DocEnd => len,
            };
            drop(l);
            doc.cursor = if select { (anchor, target) } else { (target, target) };
            if keep_x {
                doc.view.preferred_x = Some(x);
            }
        }
    }
    if !keep_x {
        doc.view.preferred_x = None;
    }
    doc.view.reveal = Some(Reveal::Near);
    let _ = look.line_comment;
    changed
}
