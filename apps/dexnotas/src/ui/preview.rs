//! Drawing the Markdown/HTML preview (`crate::preview`): blocks of styled text in a scroll
//! area that lays out only the blocks on screen (the others keep the height they had, or an
//! estimate), so a long document scrolls as smoothly as a short one.

use std::ops::Range;
use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Galley, Pos2, Rect, ScrollArea, Sense, SizeHint, Stroke,
    TextureOptions, Ui, UiBuilder, Vec2, pos2, vec2,
};

use super::theme::{self, Palette};
use crate::document::Document;
use crate::preview::{Align, Block, Inline, Item, Parsed};

const PROSE: &str = "prose";
const PROSE_BOLD: &str = "prose-bold";

/// The preview's fonts: Inter (regular and bold; egui's own font has no bold), falling back to
/// egui's fonts for the characters Inter lacks.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("inter".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Inter-Regular.otf"))));
    fonts.font_data.insert("inter-bold".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Inter-Bold.otf"))));
    // The interface's font lacks arrows and shapes (→ ▸ ▲ ●) that the monospace one has: it
    // stands in for them everywhere.
    let mono = fonts.families.get(&FontFamily::Monospace).and_then(|m| m.first().cloned());
    if let Some(mono) = mono
        && let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional)
        && !prop.contains(&mono)
    {
        prop.push(mono);
    }
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (family, font) in [(PROSE, "inter"), (PROSE_BOLD, "inter-bold")] {
        let mut list = vec![font.to_owned()];
        list.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(family.into()), list);
    }
    ctx.set_fonts(fonts);
}

struct Look {
    palette: Palette,
    dark: bool,
    /// The body text size.
    size: f32,
    link: Color32,
    code_bg: Color32,
    weak: Color32,
}

impl Look {
    fn new(dark: bool, size: f32) -> Look {
        let palette = theme::palette(dark);
        Look {
            palette,
            dark,
            size,
            link: if dark { Color32::from_rgb(0x58, 0xa6, 0xff) } else { Color32::from_rgb(0x09, 0x69, 0xda) },
            code_bg: if dark { Color32::from_rgba_unmultiplied(255, 255, 255, 16) } else { Color32::from_rgba_unmultiplied(0, 0, 0, 12) },
            weak: palette.gutter_text,
        }
    }

    fn gap(&self) -> f32 {
        (self.size * 0.75).round()
    }
}

/// What happened in the preview this frame.
#[derive(Default)]
pub struct Output {
    /// A link was clicked: its target as written.
    pub link: Option<String>,
}

/// Draw the preview of `doc`. `follow`: keep the part the editor shows in view.
pub fn show(ui: &mut Ui, doc: &mut Document, dark: bool, font_size: f32, follow: bool) -> Output {
    let look = Look::new(dark, (font_size + 1.0).round());
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, look.palette.editor_bg);
    let mut out = Output::default();
    let Some(parsed) = doc.preview.parsed.clone() else {
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, "…", FontId::proportional(look.size * 2.0), look.weak);
        return out;
    };
    let margin = (look.size * 1.4).round().min(rect.width() * 0.06).max(8.0);
    let bar = ui.spacing().scroll.bar_width + 4.0;
    // Lines no longer than reads well; the text centred in a wide window.
    let width = (rect.width() - 2.0 * margin - bar).clamp(60.0, look.size * 56.0);
    let margin = ((rect.width() - bar - width) / 2.0).max(margin);
    let gap = look.gap();
    let state = &mut doc.preview;
    let n = parsed.blocks.len();
    if state.heights.len() != n || (state.width - width).abs() > 0.5 {
        state.heights = parsed.blocks.iter().map(|b| estimate(b, width, look.size)).collect();
        state.width = width;
    }

    // Follow the editor: the block at its first line, and how far into it.
    let mut offset = None;
    if follow && !parsed.lines.is_empty() {
        let top = doc.view.top_line;
        if state.followed_line != Some(top) {
            state.followed_line = Some(top);
            let i = parsed.lines.partition_point(|l| *l <= top).saturating_sub(1);
            let mut y: f32 = margin + state.heights[..i].iter().map(|h| h + gap).sum::<f32>();
            let start = parsed.lines[i];
            if top > start {
                let next = parsed.lines.get(i + 1).copied().unwrap_or(doc.buffer.rope().len_lines()).max(start + 1);
                y += state.heights[i] * ((top - start) as f32 / (next - start) as f32).min(1.0);
            }
            offset = Some(if top == 0 { 0.0 } else { (y - gap).max(0.0) });
        }
    } else {
        state.followed_line = None;
    }

    let mut area = ScrollArea::vertical().id_salt(("preview", doc.id)).auto_shrink([false, false]);
    if let Some(y) = offset {
        area = area.vertical_scroll_offset(y);
    }
    let heights = &mut state.heights;
    area.show_viewport(ui, |ui, viewport| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.set_min_width(rect.width() - bar);
        ui.add_space(margin);
        let mut y = margin;
        let left = ui.cursor().min.x + margin;
        for (i, block) in parsed.blocks.iter().enumerate() {
            let h = heights[i];
            let near = y + h >= viewport.min.y - 400.0 && y <= viewport.max.y + 400.0;
            if near {
                let top = ui.cursor().min.y;
                let child = Rect::from_min_size(pos2(left, top), vec2(width, f32::INFINITY));
                let drawn = ui.scope_builder(UiBuilder::new().max_rect(child), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    draw_block(ui, block, &parsed, &look, 0, &mut out);
                });
                heights[i] = drawn.response.rect.height();
            } else {
                ui.allocate_space(vec2(width, h));
            }
            y += heights[i] + gap;
            ui.add_space(gap);
        }
        ui.add_space(margin * 2.0);
    });
    out
}

/// A guess at a block's height before it is drawn.
fn estimate(block: &Block, width: f32, size: f32) -> f32 {
    let line = size * 1.35;
    let chars_per_line = (width / (size * 0.52)).max(10.0);
    let text_lines = |inlines: &[Inline]| -> f32 {
        let mut chars = 0usize;
        let mut images = 0.0;
        for i in inlines {
            match i {
                Inline::Text { text, .. } => chars += text.chars().count(),
                Inline::Image { .. } => images += 8.0,
            }
        }
        (chars as f32 / chars_per_line).ceil().max(1.0) + images
    };
    match block {
        Block::Heading { level, inlines } => text_lines(inlines) * line * heading_scale(*level) + size * 0.6,
        Block::Paragraph(inlines) => text_lines(inlines) * line,
        Block::Code { text, .. } => text.lines().count().max(1) as f32 * size * 1.25 + size,
        Block::Quote(blocks) => blocks.iter().map(|b| estimate(b, width - 16.0, size) + size * 0.75).sum::<f32>(),
        Block::List { items, .. } => {
            items.iter().flat_map(|i| i.blocks.iter()).map(|b| estimate(b, width - size * 2.0, size) + size * 0.3).sum::<f32>()
        }
        Block::Rule => size,
        Block::Table { rows, .. } => (rows.len() + 1) as f32 * line * 1.4,
    }
}

fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 1.9,
        2 => 1.5,
        3 => 1.25,
        4 => 1.1,
        5 => 1.0,
        _ => 0.9,
    }
}

fn draw_blocks(ui: &mut Ui, blocks: &[Block], parsed: &Parsed, look: &Look, depth: usize, out: &mut Output, gap: f32) {
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 {
            ui.add_space(gap);
        }
        draw_block(ui, block, parsed, look, depth, out);
    }
}

fn draw_block(ui: &mut Ui, block: &Block, parsed: &Parsed, look: &Look, depth: usize, out: &mut Output) {
    let width = ui.available_width();
    match block {
        Block::Heading { level, inlines } => {
            let size = look.size * heading_scale(*level);
            if *level <= 2 {
                ui.add_space(look.size * 0.3);
            }
            draw_inlines(ui, inlines, parsed, look, size, look.palette.text, true, out);
            if *level <= 2 {
                ui.add_space(look.size * 0.3);
                let (r, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
                ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, look.palette.separator));
            }
        }
        Block::Paragraph(inlines) => draw_inlines(ui, inlines, parsed, look, look.size, look.palette.text, false, out),
        Block::Code { text, spans, .. } => {
            let pad = (look.size * 0.6).round();
            let font = FontId::monospace((look.size * 0.88).round());
            let mut job = LayoutJob::default();
            job.wrap.max_width = width - 2.0 * pad;
            let plain = TextFormat { font_id: font.clone(), color: look.palette.text, ..Default::default() };
            let mut at = 0;
            for (range, kind) in spans {
                if range.start < at || range.end > text.len() || !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
                    continue;
                }
                job.append(&text[at..range.start], 0.0, plain.clone());
                job.append(&text[range.clone()], 0.0, TextFormat { color: theme::syntax(*kind, look.dark), ..plain.clone() });
                at = range.end;
            }
            job.append(&text[at..], 0.0, plain);
            let galley = ui.painter().layout_job(job);
            let size = vec2(width, galley.size().y + 2.0 * pad);
            let (r, _) = ui.allocate_exact_size(size, Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(6), look.code_bg);
            ui.painter().galley(r.min + vec2(pad, pad), galley, look.palette.text);
        }
        Block::Quote(blocks) => {
            let bar = 3.0;
            let indent = (look.size * 0.9).round();
            let top = ui.cursor().min;
            let inner = Rect::from_min_size(top + vec2(bar + indent, 0.0), vec2(width - bar - indent, f32::INFINITY));
            let drawn = ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                draw_blocks(ui, blocks, parsed, look, depth, out, look.gap());
            });
            let h = drawn.response.rect.height();
            ui.painter().rect_filled(Rect::from_min_size(top, vec2(bar, h)), 1.0, look.palette.separator);
        }
        Block::List { start, items } => {
            for (n, item) in items.iter().enumerate() {
                if n > 0 {
                    ui.add_space(look.size * 0.25);
                }
                draw_item(ui, item, start.map(|s| s + n as u64), parsed, look, depth, out);
            }
        }
        Block::Rule => {
            let (r, _) = ui.allocate_exact_size(vec2(width, look.size), Sense::hover());
            ui.painter().hline(r.x_range(), r.center().y, Stroke::new(2.0, look.palette.separator));
        }
        Block::Table { aligns, head, rows } => draw_table(ui, aligns, head, rows, parsed, look, out),
    }
}

fn draw_item(ui: &mut Ui, item: &Item, number: Option<u64>, parsed: &Parsed, look: &Look, depth: usize, out: &mut Output) {
    let width = ui.available_width();
    let indent = (look.size * if number.is_some_and(|n| n >= 10) { 2.2 } else { 1.7 }).round();
    let top = ui.cursor().min;
    let inner = Rect::from_min_size(top + vec2(indent, 0.0), vec2((width - indent).max(20.0), f32::INFINITY));
    ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        draw_blocks(ui, &item.blocks, parsed, look, depth + 1, out, look.size * 0.4);
    });
    // The marker, on the first line.
    let line = look.size * 1.3;
    let painter = ui.painter();
    let color = look.weak;
    match (item.task, number) {
        (Some(done), _) => {
            let s = (look.size * 0.85).round();
            let r = Rect::from_min_size(pos2(top.x + 1.0, top.y + (line - s) / 2.0), vec2(s, s));
            painter.rect_stroke(r, 3.0, Stroke::new(1.5, color), egui::StrokeKind::Inside);
            if done {
                painter.rect_filled(r, 3.0, look.link);
                let c = |x: f32, y: f32| pos2(r.min.x + x * s, r.min.y + y * s);
                painter.line(vec![c(0.22, 0.52), c(0.42, 0.72), c(0.78, 0.3)], Stroke::new(2.0, look.palette.editor_bg));
            }
        }
        (None, Some(n)) => {
            painter.text(pos2(top.x + indent - look.size * 0.35, top.y + line / 2.0), Align2::RIGHT_CENTER, format!("{n}."), FontId::new(look.size, FontFamily::Name(PROSE.into())), color);
        }
        (None, None) => {
            let c = pos2(top.x + indent * 0.45, top.y + line / 2.0);
            let r = look.size * 0.17;
            match depth % 3 {
                0 => painter.circle_filled(c, r, color),
                1 => painter.circle_stroke(c, r, Stroke::new(1.2, color)),
                _ => painter.rect_filled(Rect::from_center_size(c, vec2(r * 1.8, r * 1.8)), 0.0, color),
            };
        }
    }
}

/// A run of text: its galley and which characters are which link.
struct Laid {
    galley: Arc<Galley>,
    links: Vec<(Range<usize>, u32)>,
}

#[allow(clippy::too_many_arguments)]
fn layout(ui: &Ui, inlines: &[Inline], look: &Look, size: f32, color: Color32, bold: bool, width: f32, align: egui::Align) -> Laid {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    job.halign = align;
    let mut links = Vec::new();
    let mut chars = 0;
    for inline in inlines {
        let Inline::Text { text, style, link } = inline else { continue };
        let mut size = size;
        if style.small {
            size *= 0.75;
        }
        let family = if style.code {
            size *= 0.9;
            FontFamily::Monospace
        } else if bold || style.bold {
            FontFamily::Name(PROSE_BOLD.into())
        } else {
            FontFamily::Name(PROSE.into())
        };
        let color = if link.is_some() { look.link } else { color };
        let line = Stroke::new(1.0, color);
        let format = TextFormat {
            font_id: FontId::new(size, family),
            color,
            italics: style.italic,
            underline: if style.underline || link.is_some() { line } else { Stroke::NONE },
            strikethrough: if style.strike { line } else { Stroke::NONE },
            background: if style.code {
                look.code_bg
            } else if style.mark {
                Color32::from_rgba_unmultiplied(0xff, 0xd0, 0x40, if look.dark { 70 } else { 110 })
            } else {
                Color32::TRANSPARENT
            },
            valign: if style.small { egui::Align::TOP } else { egui::Align::BOTTOM },
            line_height: Some((size * 1.35).round()),
            ..Default::default()
        };
        let n = text.chars().count();
        if let Some(link) = link {
            links.push((chars..chars + n, *link));
        }
        chars += n;
        job.append(text, 0.0, format);
    }
    Laid { galley: ui.painter().layout_job(job), links }
}

/// Paint `laid` at `pos`, in `area`; a click on a link is reported in `out`.
fn paint(ui: &mut Ui, laid: Laid, pos: Pos2, area: Rect, parsed: &Parsed, out: &mut Output) {
    let sense = if laid.links.is_empty() { Sense::hover() } else { Sense::click() };
    let response = ui.interact(area, ui.scope_id().with(("text", area.min.x as i32, area.min.y as i32)), sense);
    if !laid.links.is_empty()
        && let Some(p) = response.hover_pos()
    {
        let at = laid.galley.cursor_from_pos(p - pos).index.0;
        // A position between two characters: the one under the pointer is the one before when
        // the pointer is in its right half.
        let hit = laid.links.iter().find(|(r, _)| r.contains(&at) || (at > 0 && r.contains(&(at - 1)) && at == r.end));
        if let Some((_, link)) = hit {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if response.clicked() {
                out.link = parsed.links.get(*link as usize).cloned();
            }
        }
    }
    ui.painter().galley(pos, laid.galley, Color32::PLACEHOLDER);
}

#[allow(clippy::too_many_arguments)]
fn draw_inlines(ui: &mut Ui, inlines: &[Inline], parsed: &Parsed, look: &Look, size: f32, color: Color32, bold: bool, out: &mut Output) {
    // Text runs between images.
    let mut start = 0;
    for (i, inline) in inlines.iter().enumerate() {
        if let Inline::Image { src, alt, link } = inline {
            text_run(ui, &inlines[start..i], parsed, look, size, color, bold, out);
            draw_image(ui, src, alt, *link, parsed, look, out);
            start = i + 1;
        }
    }
    text_run(ui, &inlines[start..], parsed, look, size, color, bold, out);
}

#[allow(clippy::too_many_arguments)]
fn text_run(ui: &mut Ui, inlines: &[Inline], parsed: &Parsed, look: &Look, size: f32, color: Color32, bold: bool, out: &mut Output) {
    if !inlines.iter().any(|i| matches!(i, Inline::Text { text, .. } if !text.trim().is_empty())) {
        return;
    }
    let width = ui.available_width();
    let laid = layout(ui, inlines, look, size, color, bold, width, egui::Align::LEFT);
    let (rect, _) = ui.allocate_exact_size(vec2(width, laid.galley.size().y), Sense::hover());
    paint(ui, laid, rect.min, rect, parsed, out);
}

fn draw_image(ui: &mut Ui, src: &str, alt: &str, link: Option<u32>, parsed: &Parsed, look: &Look, out: &mut Output) {
    let width = ui.available_width();
    if let Some(uri) = parsed.image_uri(src) {
        match ui.ctx().try_load_texture(&uri, TextureOptions::LINEAR, SizeHint::default()) {
            Ok(egui::load::TexturePoll::Ready { texture }) => {
                let mut size = texture.size;
                if size.x > width {
                    size *= width / size.x;
                }
                let (rect, response) = ui.allocate_exact_size(size, if link.is_some() { Sense::click() } else { Sense::hover() });
                egui::Image::from_texture(texture).paint_at(ui, rect);
                if let Some(link) = link {
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if response.clicked() {
                        out.link = parsed.links.get(link as usize).cloned();
                    }
                }
                return;
            }
            Ok(egui::load::TexturePoll::Pending { .. }) => {
                let (rect, _) = ui.allocate_exact_size(vec2(width, look.size * 2.0), Sense::hover());
                egui::Spinner::new().paint_at(ui, Rect::from_min_size(rect.min, Vec2::splat(look.size * 1.5)));
                return;
            }
            Err(_) => {}
        }
    }
    // Not loaded (on the web, missing or unreadable): its description.
    let label = if alt.trim().is_empty() { src } else { alt };
    let text = Inline::Text { text: format!("🖼 {label}"), style: crate::preview::Style { italic: true, ..Default::default() }, link };
    text_run(ui, std::slice::from_ref(&text), parsed, look, look.size, look.weak, false, out);
}

fn draw_table(ui: &mut Ui, aligns: &[Align], head: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>], parsed: &Parsed, look: &Look, out: &mut Output) {
    let columns = aligns.len().max(head.len()).max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if columns == 0 {
        return;
    }
    let width = ui.available_width();
    let pad = (look.size * 0.5).round();
    // Each column as wide as its widest cell on one line, shrunk to fit.
    let mut natural = vec![look.size * 2.0; columns];
    let all = std::iter::once(head).chain(rows.iter().map(Vec::as_slice));
    for (r, row) in all.enumerate() {
        for (c, cell) in row.iter().enumerate().take(columns) {
            let laid = layout(ui, cell, look, look.size, look.palette.text, r == 0 && !head.is_empty(), f32::INFINITY, egui::Align::LEFT);
            natural[c] = natural[c].max(laid.galley.size().x + 2.0 * pad);
        }
    }
    let total: f32 = natural.iter().sum();
    let widths: Vec<f32> = if total <= width {
        natural
    } else {
        // Share the room: narrow columns keep their width, wide ones split the rest.
        let fair = width / columns as f32;
        let small: f32 = natural.iter().filter(|w| **w <= fair).sum();
        let wide: f32 = natural.iter().filter(|w| **w > fair).sum();
        let room = (width - small).max(0.0);
        natural.iter().map(|w| if *w <= fair { *w } else { (w / wide * room).max(look.size * 3.0) }).collect()
    };
    let table_width: f32 = widths.iter().sum();
    let border = Stroke::new(1.0, look.palette.separator);
    let top = ui.cursor().min;
    let mut y = top.y;
    let rows_iter = (!head.is_empty()).then_some(head).into_iter().chain(rows.iter().map(Vec::as_slice));
    for (r, row) in rows_iter.enumerate() {
        let is_head = r == 0 && !head.is_empty();
        let laid: Vec<Laid> = (0..columns)
            .map(|c| {
                let cell = row.get(c).map(Vec::as_slice).unwrap_or(&[]);
                let align = match aligns.get(c) {
                    Some(Align::Center) => egui::Align::Center,
                    Some(Align::Right) => egui::Align::RIGHT,
                    _ => egui::Align::LEFT,
                };
                layout(ui, cell, look, look.size, look.palette.text, is_head, widths[c] - 2.0 * pad, align)
            })
            .collect();
        let h = laid.iter().map(|l| l.galley.size().y).fold(look.size, f32::max) + 2.0 * pad;
        let row_rect = Rect::from_min_size(pos2(top.x, y), vec2(table_width, h));
        if is_head {
            ui.painter().rect_filled(row_rect, 0.0, look.code_bg);
        } else if r % 2 == 0 {
            ui.painter().rect_filled(row_rect, 0.0, look.code_bg.gamma_multiply(0.5));
        }
        let mut x = top.x;
        for (c, l) in laid.into_iter().enumerate() {
            let w = widths[c];
            let cell = Rect::from_min_size(pos2(x + pad, y + pad), vec2(w - 2.0 * pad, h - 2.0 * pad));
            // Galleys aligned right or centre are laid out around x = 0.
            let offset = match aligns.get(c) {
                Some(Align::Center) => cell.width() / 2.0,
                Some(Align::Right) => cell.width(),
                _ => 0.0,
            };
            paint(ui, l, cell.min + vec2(offset, 0.0), cell, parsed, out);
            x += w;
            ui.painter().vline(x, row_rect.y_range(), border);
        }
        ui.painter().hline(row_rect.x_range(), row_rect.bottom(), border);
        y += h;
    }
    let table = Rect::from_min_max(top, pos2(top.x + table_width, y));
    ui.painter().vline(table.left(), table.y_range(), border);
    ui.painter().hline(table.x_range(), table.top(), border);
    ui.allocate_rect(Rect::from_min_size(top, vec2(width.min(table_width).max(1.0), y - top.y)), Sense::hover());
}
