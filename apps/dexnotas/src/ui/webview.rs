//! The preview as a web page (Android's WebView, `WebPreview.kt`): each frame says where it goes
//! (the part of the window `menus::preview` left for it), whether it shows (not while a menu or
//! a dialog of the window is over it: the WebView is drawn above everything egui draws), what it
//! shows and where it scrolls to follow the editor.

use std::time::{Duration, Instant};

use egui::{Order, Rect};

use crate::document::Document;
use crate::platform::web;
use crate::preview::Source;

/// What the WebView shows.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Shown {
    doc: u64,
    /// The buffer version shown.
    version: u64,
    dark: bool,
    font: u32,
    source: Source,
}

#[derive(Default)]
pub struct Web {
    /// Where the view was last put, in pixels (`false`: hidden).
    placed: Option<(bool, [i32; 4])>,
    shown: Option<Shown>,
    /// The editor's first line last followed.
    line: Option<usize>,
    /// An HTML document changed at: it reloads after a pause in typing.
    changed_at: Option<Instant>,
}

/// An HTML document is reloaded this long after the last change.
const HTML_PAUSE: Duration = Duration::from_millis(600);

/// Whether a window of egui (a menu, a dialog, a tooltip) is over `rect`.
fn covered(ctx: &egui::Context, rect: Rect) -> bool {
    ctx.memory(|m| {
        m.areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|l| matches!(l.order, Order::Foreground | Order::Tooltip | Order::Debug))
            .filter_map(|l| m.area_rect(l.id))
            .any(|r| r.intersects(rect))
    })
}

fn base_of(doc: &Document) -> String {
    match doc.path.as_ref().and_then(|p| p.parent()) {
        Some(dir) => {
            let mut s = String::from("file://");
            for c in dir.to_string_lossy().chars() {
                match c {
                    ' ' => s.push_str("%20"),
                    '#' => s.push_str("%23"),
                    '?' => s.push_str("%3F"),
                    '%' => s.push_str("%25"),
                    c => s.push(c),
                }
            }
            s.push('/');
            s
        }
        None => String::new(),
    }
}

impl Web {
    fn place(&mut self, visible: bool, rect: [i32; 4]) {
        let want = (visible, rect);
        if self.placed == Some(want) || (!visible && self.placed.is_none_or(|(v, _)| !v)) {
            return;
        }
        web::place(visible, rect);
        self.placed = Some(want);
    }

    pub fn hide(&mut self) {
        self.place(false, [0; 4]);
    }

    /// The end of a frame. `want`: the preview's place this frame and whether it follows the
    /// editor (`None`: no web preview this frame).
    pub fn sync(&mut self, ctx: &egui::Context, want: Option<(Rect, bool)>, doc: Option<&mut Document>, dark: bool, font_size: f32) {
        let (Some((rect, follow)), Some(doc)) = (want, doc) else {
            self.hide();
            return;
        };
        let Some(source) = Source::of(doc.language) else {
            self.hide();
            return;
        };
        let font = (font_size + 1.0).round() as u32;
        let version = doc.buffer.version();
        let same_page = self.shown.is_some_and(|s| s.doc == doc.id && s.dark == dark && s.font == font && s.source == source);
        match source {
            Source::Markdown => {
                // The worker's HTML (`Parsed::web`), as soon as there is one; newer ones as they come.
                let ready = doc.preview.parsed.as_ref().and_then(|p| p.web.as_deref()).map(|html| (html, doc.preview.version));
                if let Some((body, parsed_version)) = ready {
                    if !same_page {
                        let page = crate::preview::web::markdown_page(body, dark, font as f32);
                        let bg = if dark { crate::ui::theme::palette(true).editor_bg } else { egui::Color32::WHITE };
                        web::load(&page, &base_of(doc), &format!("#{:02x}{:02x}{:02x}", bg.r(), bg.g(), bg.b()), false);
                        self.shown = Some(Shown { doc: doc.id, version: parsed_version, dark, font, source });
                        self.line = None;
                    } else if self.shown.is_some_and(|s| s.version != parsed_version) {
                        web::update(body);
                        if let Some(s) = self.shown.as_mut() {
                            s.version = parsed_version;
                        }
                    }
                }
            }
            Source::Html => {
                let stale = self.shown.is_some_and(|s| s.version != version);
                if !same_page || stale {
                    let now = Instant::now();
                    let since = *self.changed_at.get_or_insert(now);
                    if !same_page || now.duration_since(since) >= HTML_PAUSE {
                        let page = crate::preview::web::html_page(&doc.text());
                        web::load(&page, &base_of(doc), "#ffffff", same_page);
                        self.shown = Some(Shown { doc: doc.id, version, dark, font, source });
                        self.changed_at = None;
                        if !same_page {
                            self.line = None;
                        }
                    } else {
                        ctx.request_repaint_after(HTML_PAUSE);
                    }
                }
            }
        }
        if self.shown.is_none_or(|s| s.doc != doc.id) {
            // Nothing of this document to show yet.
            self.hide();
            return;
        }

        let ppp = ctx.pixels_per_point();
        let px = [
            (rect.min.x * ppp).round() as i32,
            (rect.min.y * ppp).round() as i32,
            (rect.width() * ppp).round() as i32,
            (rect.height() * ppp).round() as i32,
        ];
        let visible = !covered(ctx, rect) && px[2] > 0 && px[3] > 0;
        self.place(visible, px);

        if follow {
            let top = doc.view.top_line;
            if self.line != Some(top) {
                self.line = Some(top);
                match source {
                    Source::Markdown => web::scroll(Some(top), 0.0),
                    Source::Html => {
                        let lines = doc.buffer.rope().len_lines().max(1);
                        web::scroll(None, top as f32 / lines as f32);
                    }
                }
            }
        } else {
            self.line = None;
        }
    }

    /// Forget the page (the setting was turned off).
    pub fn clear(&mut self) {
        self.hide();
        if self.shown.take().is_some() {
            web::clear();
        }
    }
}
