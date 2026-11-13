//! The editor: egui's text field with syntax colours, line numbers, the current line and the find
//! matches painted behind the text.

use std::ops::Range;
use std::sync::Arc;

use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::{Align, Color32, FontId, Galley, Id, Rect, Sense, Shape, TextBuffer, Ui, UiBuilder, pos2, vec2};

use super::theme::{self, Palette};
use crate::document::Document;
use crate::find::{byte_to_char, char_to_byte};
use crate::syntax::{self, Kind, LanguageId};

/// The highlighting of the last text laid out, so a frame that changed nothing reuses it.
#[derive(Default)]
pub struct HighlightCache {
    key: Option<(u64, usize, Option<LanguageId>)>,
    spans: Vec<(Range<usize>, Kind)>,
}

impl HighlightCache {
    fn spans(&mut self, text: &str, language: Option<LanguageId>) -> &[(Range<usize>, Kind)] {
        let key = (crate::document::hash_text(text), text.len(), language);
        if self.key != Some(key) {
            self.spans = match language {
                Some(l) => syntax::highlight(l, text),
                None => Vec::new(),
            };
            self.key = Some(key);
        }
        &self.spans
    }
}

/// The text as egui sections, coloured.
fn layout_job(text: &str, spans: &[(Range<usize>, Kind)], font: &FontId, palette: &Palette, dark: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    let plain = TextFormat::simple(font.clone(), palette.text);
    let mut at = 0;
    for (range, kind) in spans {
        if range.start < at || range.end > text.len() || !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
            continue;
        }
        if range.start > at {
            job.append(&text[at..range.start], 0.0, plain.clone());
        }
        job.append(&text[range.clone()], 0.0, TextFormat::simple(font.clone(), theme::syntax(*kind, dark)));
        at = range.end;
    }
    if at < text.len() {
        job.append(&text[at..], 0.0, plain);
    }
    job
}

/// What the frame's editor did.
pub struct Output {
    /// The text was changed by typing.
    pub changed: bool,
    /// The selection (anchor, cursor), in characters.
    pub cursor: Option<(usize, usize)>,
    pub focused: bool,
}

/// Requests to the editor for this frame.
#[derive(Default)]
pub struct Requests {
    pub focus: bool,
    /// Select this (characters) and bring it into view.
    pub select: Option<(usize, usize)>,
    /// Find matches to mark (bytes), and which one is current.
    pub marks: Vec<Range<usize>>,
    pub current_mark: Option<Range<usize>>,
}

pub struct Look {
    pub font: FontId,
    pub dark: bool,
    pub wrap: bool,
    pub line_numbers: bool,
    pub highlight_line: bool,
}

pub fn editor_id(doc: u64) -> Id {
    Id::unique(("dexnotas-editor", doc))
}

/// Draw `doc` filling `ui`.
pub fn show(ui: &mut Ui, doc: &mut Document, cache: &mut HighlightCache, look: &Look, requests: Requests) -> Output {
    let palette = theme::palette(look.dark);
    let id = editor_id(doc.id);
    let full = ui.available_rect_before_wrap();
    ui.painter().rect_filled(full, 0.0, palette.editor_bg);

    // The gutter: wide enough for the last line's number.
    let lines = doc.text.matches('\n').count() + 1;
    let digit_width = ui.ctx().fonts_mut(|f| f.glyph_width(&look.font, '0'));
    let gutter_width = if look.line_numbers { (lines.to_string().len().max(2) as f32) * digit_width + 18.0 } else { 6.0 };
    let gutter = Rect::from_min_size(full.min, vec2(gutter_width, full.height()));
    let text_rect = Rect::from_min_max(pos2(full.min.x + gutter_width, full.min.y), full.max);

    if let Some((a, b)) = requests.select {
        if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), id) {
            state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
            state.store(ui.ctx(), id);
        } else {
            let mut state = egui::text_edit::TextEditState::default();
            state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
            state.store(ui.ctx(), id);
        }
    }
    if requests.focus {
        ui.memory_mut(|m| m.request_focus(id));
    }

    let mut child = ui.new_child(UiBuilder::new().max_rect(text_rect));
    let mut placed: Option<(Arc<Galley>, egui::Pos2, Rect)> = None;
    let mut changed = false;
    let mut cursor = None;
    let wrap = look.wrap;
    let scroll = if wrap { egui::ScrollArea::vertical() } else { egui::ScrollArea::both() };
    scroll
        .id_salt(("dexnotas-scroll", doc.id))
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let background = ui.painter().add(Shape::Noop);
            let language = if doc.text.len() <= syntax::MAX_HIGHLIGHT_BYTES { doc.language } else { None };
            let font = look.font.clone();
            let dark = look.dark;
            let mut layouter = |ui: &Ui, buf: &dyn TextBuffer, wrap_width: f32| -> Arc<Galley> {
                let text = buf.as_str();
                let mut job = layout_job(text, cache.spans(text, language), &font, &palette, dark);
                job.wrap.max_width = if wrap { wrap_width } else { f32::INFINITY };
                ui.ctx().fonts_mut(|f| f.layout_job(job))
            };
            let rows = ((text_rect.height() / ui.fonts_mut(|f| f.row_height(&look.font))) as usize).max(1);
            let output = egui::TextEdit::multiline(&mut doc.text)
                .id(id)
                .code_editor()
                .font(look.font.clone())
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::symmetric(4, 2))
                .desired_width(if wrap { ui.available_width() } else { f32::INFINITY })
                .desired_rows(rows)
                .lock_focus(true)
                .layouter(&mut layouter)
                .show(ui);
            changed = output.response.changed();
            cursor = output.cursor_range.map(|r| (r.secondary.index.0, r.primary.index.0));
            let galley = output.galley.clone();
            let origin = output.galley_pos;

            // Behind the text: the current line and the find matches.
            let mut shapes = Vec::new();
            let full_width = ui.max_rect().x_range().max.max(origin.x + galley.size().x) - origin.x;
            if look.highlight_line && let Some((_, at)) = cursor {
                let r = galley.pos_from_cursor(CCursor::new(at));
                shapes.push(Shape::rect_filled(
                    Rect::from_min_size(pos2(ui.max_rect().left(), origin.y + r.min.y), vec2(full_width.max(ui.max_rect().width()) + 4.0, r.height())),
                    0.0,
                    palette.current_line,
                ));
            }
            for (i, m) in requests.marks.iter().enumerate().take(5000) {
                let current = requests.current_mark.as_ref() == Some(m);
                let a = galley.pos_from_cursor(CCursor::new(byte_to_char(&doc.text, m.start)));
                let b = galley.pos_from_cursor(CCursor::new(byte_to_char(&doc.text, m.end)));
                let color = if current { palette.find_current } else { palette.find_match };
                if (a.min.y - b.min.y).abs() < 1.0 {
                    shapes.push(Shape::rect_filled(Rect::from_min_max(origin + a.min.to_vec2(), origin + vec2(b.min.x.max(a.min.x + 2.0), b.max.y)), 2.0, color));
                } else {
                    shapes.push(Shape::rect_filled(Rect::from_min_max(origin + a.min.to_vec2(), origin + vec2(galley.size().x.max(a.min.x + 4.0), a.max.y)), 2.0, color));
                }
                let _ = i;
            }
            ui.painter().set(background, Shape::Vec(shapes));

            if requests.select.is_some() && let Some((_, at)) = cursor {
                let r = galley.pos_from_cursor(CCursor::new(at)).translate(origin.to_vec2());
                ui.scroll_to_rect(r.expand(8.0), Some(Align::Center));
            }
            placed = Some((galley, origin, ui.clip_rect()));
        });
    ui.advance_cursor_after_rect(full);

    // Line numbers, at the rows' heights (wrapped rows get none).
    if let Some((galley, origin, _clip)) = placed {
        let painter = ui.painter_at(gutter);
        painter.rect_filled(gutter, 0.0, palette.gutter_bg);
        if look.line_numbers {
            let current_line = cursor.map(|(_, at)| crate::find::line_col(&doc.text, char_to_byte(&doc.text, at)).0);
            let mut line = 1;
            let mut starts_line = true;
            for row in &galley.rows {
                let y = origin.y + row.pos.y;
                if starts_line && y + row.rect().height() >= gutter.top() && y <= gutter.bottom() {
                    let color = if Some(line) == current_line { palette.gutter_current } else { palette.gutter_text };
                    painter.text(pos2(gutter.right() - 8.0, y), egui::Align2::RIGHT_TOP, line.to_string(), look.font.clone(), color);
                }
                if row.ends_with_newline {
                    line += 1;
                    starts_line = true;
                } else {
                    starts_line = false;
                }
            }
        }
        painter.vline(gutter.right() - 1.0, gutter.y_range(), egui::Stroke::new(1.0, palette.gutter_bg.gamma_multiply(0.9)));
    }

    let focused = ui.memory(|m| m.has_focus(id));
    if changed {
        doc.changed();
    }
    if let Some(c) = cursor {
        doc.cursor = c;
    }
    let _ = Color32::TRANSPARENT;
    let _ = Sense::hover();
    Output { changed, cursor, focused }
}
