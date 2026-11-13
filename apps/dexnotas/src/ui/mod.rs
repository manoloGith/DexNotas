//! The window: menus, tabs, the find bar, the editor and the status bar, and everything they do.

pub mod code_editor;
mod menus;
pub mod preview;
pub mod theme;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use egui::{Key, KeyboardShortcut, Modifiers};

use crate::document::Document;
use crate::find::{self, Query, byte_to_char, char_to_byte};
use crate::i18n::{t, tf};
use crate::macros::{self, Command, Macro, Repeat, Step};
use crate::platform::{self, Picked};
use crate::session::{self, Session};
use crate::settings::Settings;
use crate::syntax::LanguageId;
use crate::text::{Encoding, LineEnding};

/// Everything the menus, the shortcuts and the bars can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    New,
    Open,
    OpenPath(PathBuf),
    Save,
    SaveAs,
    SaveAll,
    Reload,
    CloseTab(u64),
    CloseOthers,
    CloseAll,
    Activate(u64),
    NextTab(bool),
    Share,
    Quit,
    Undo,
    Redo,
    Edit(Command),
    ShowFind { replace: bool },
    FindInTabs,
    GoToLineDialog,
    ReopenAs(Encoding),
    ConvertTo(Encoding),
    ToggleBom,
    SetLineEnding(LineEnding),
    SetLanguage(Option<LanguageId>),
    JsonCheck,
    XmlCheck,
    YamlCheck,
    TomlCheck,
    Zoom(f32),
    ZoomReset,
    ToggleRecord,
    Play(Repeat),
    PlayTimesDialog,
    SaveMacroDialog,
    PlaySaved(usize),
    ManageMacros,
    About,
    /// Where the preview of the active document is.
    SetPreview(crate::preview::Mode),
    /// Open the preview beside the editor, or close it.
    TogglePreview,
    /// A link in the preview was clicked.
    FollowLink(String),
}

/// A window over the editor.
enum Dialog {
    /// Document `doc` has unsaved changes; close it (and then do `then`).
    ConfirmClose { doc: u64 },
    GoToLine(String),
    PlayTimes(String),
    SaveMacro(String),
    ManageMacros,
    /// Document `doc` changed on disk while it has unsaved changes.
    ChangedOnDisk { doc: u64 },
    Message { title: String, text: String },
    About,
}

/// The find bar.
#[derive(Default)]
struct FindBar {
    open: bool,
    replace: bool,
    query: Query,
    replacement: String,
    focus: bool,
    /// The query's error, or the number of matches.
    note: String,
    /// The match last selected (bytes).
    current: Option<std::ops::Range<usize>>,
}

/// A line found by "Find in all tabs".
struct Hit {
    doc: u64,
    title: String,
    line: usize,
    text: String,
    range: std::ops::Range<usize>,
}

/// A macro being played.
struct Playback {
    steps: Vec<Step>,
    index: usize,
    repeat: Repeat,
    runs: usize,
    /// The cursor when the current run started (for "until the end").
    run_start: usize,
}

pub struct App {
    docs: Vec<Document>,
    active: usize,
    next_id: u64,
    next_untitled: u32,
    settings: Settings,
    applied: Option<Settings>,
    session: Session,
    data_dir: PathBuf,
    recent: Vec<PathBuf>,
    macros: Vec<Macro>,
    /// The session changed and should be written (when it changed).
    unsaved_since: Option<Instant>,
    picker_tx: Sender<Picked>,
    picker_rx: Receiver<Picked>,
    status: Option<(String, Instant, bool)>,
    dialog: Option<Dialog>,
    find: FindBar,
    hits: Vec<Hit>,
    hits_open: bool,
    actions: Vec<Action>,
    highlighter: crate::highlight::Worker,
    previewer: crate::preview::Worker,
    /// Editor requests for the next frame.
    select: Option<(usize, usize)>,
    focus_editor: bool,
    editor_focused: bool,
    /// Macro recording: the steps so far.
    recording: Option<Vec<Step>>,
    /// The last recorded macro (Play).
    last_macro: Vec<Step>,
    playback: Option<Playback>,
    /// Input to add to the next frame (a macro's keys).
    pending_events: Vec<egui::Event>,
    /// Input was added this frame and the editor has not seen it yet.
    events_in_flight: bool,
    checked_disk: Instant,
    dirty_since: Option<Instant>,
    /// The find bar's count, and what it was counted for.
    match_count: Option<((u64, u64, Query), Option<usize>)>,
    /// The tab whose menu is open (right click), and where.
    tab_menu: Option<(u64, egui::Pos2)>,
    ctx: egui::Context,
}

fn shortcut(modifiers: Modifiers, key: Key) -> KeyboardShortcut {
    KeyboardShortcut::new(modifiers, key)
}

const CTRL: Modifiers = Modifiers::COMMAND;
const CTRL_SHIFT: Modifiers = Modifiers { shift: true, ..Modifiers::COMMAND };
const CTRL_ALT: Modifiers = Modifiers { alt: true, ..Modifiers::COMMAND };

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Vec<PathBuf>) -> App {
        let data_dir = session::data_dir();
        let settings: Settings = session::read_json(&data_dir.join("settings.json")).unwrap_or_default();
        crate::i18n::set_language(settings.language);
        let macros: Vec<Macro> = session::read_json(&data_dir.join("macros.json")).unwrap_or_default();
        let mut session = Session::new(data_dir.clone());
        let (docs, active, next_untitled, recent, problems) = session.restore();
        let next_id = docs.iter().map(|d| d.id).max().unwrap_or(0) + 1;
        let (picker_tx, picker_rx) = channel();
        let mut app = App {
            docs,
            active,
            next_id,
            next_untitled,
            settings,
            applied: None,
            session,
            data_dir,
            recent,
            macros,
            unsaved_since: None,
            picker_tx,
            picker_rx,
            status: None,
            dialog: None,
            find: FindBar::default(),
            hits: Vec::new(),
            hits_open: false,
            actions: Vec::new(),
            highlighter: crate::highlight::Worker::new(cc.egui_ctx.clone()),
            previewer: crate::preview::Worker::new(cc.egui_ctx.clone()),
            select: None,
            focus_editor: true,
            editor_focused: false,
            recording: None,
            last_macro: Vec::new(),
            playback: None,
            pending_events: Vec::new(),
            events_in_flight: false,
            checked_disk: Instant::now(),
            dirty_since: None,
            match_count: None,
            tab_menu: None,
            ctx: cc.egui_ctx.clone(),
        };
        if !problems.is_empty() {
            app.dialog = Some(Dialog::Message { title: t("not_reopened").to_owned(), text: problems.join("\n") });
        }
        for path in open {
            app.open_path(&path);
        }
        if app.docs.is_empty() {
            app.new_doc();
        }
        if let Some(d) = app.docs.get(app.active) {
            app.select = Some(d.cursor);
        }
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        preview::install_fonts(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        app
    }

    /// The window title (DeX's taskbar).
    pub fn title(&self) -> String {
        match self.docs.get(self.active) {
            Some(d) => format!("{}{} - DexNotas", if d.dirty() { "*" } else { "" }, d.title()),
            None => "DexNotas".to_owned(),
        }
    }

    // ── documents ──────────────────────────────────────────────────────────────────────────

    fn doc(&self) -> Option<&Document> {
        self.docs.get(self.active)
    }

    fn doc_mut(&mut self) -> Option<&mut Document> {
        self.docs.get_mut(self.active)
    }

    fn index_of(&self, id: u64) -> Option<usize> {
        self.docs.iter().position(|d| d.id == id)
    }

    fn touched(&mut self) {
        self.unsaved_since.get_or_insert_with(Instant::now);
    }

    /// The number of matches in the find bar, worked out again when the query or the text
    /// changed (and only for documents up to 64 MB: above that, Find next still works).
    fn update_match_count(&mut self) {
        if !self.find.open || self.find.query.pattern.is_empty() {
            return;
        }
        let Ok(re) = self.find.query.compile() else { return };
        let Some(d) = self.docs.get_mut(self.active) else { return };
        let key = (d.id, d.buffer.version(), self.find.query.clone());
        if self.match_count.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let n = if d.buffer.len_bytes() <= 64 * 1024 * 1024 { Some(find::find_all(&d.text(), &re).len()) } else { None };
        self.find.note = match n {
            Some(n) => tf("matches", n),
            None => String::new(),
        };
        self.match_count = Some((key, n));
    }

    fn activate(&mut self, index: usize) {
        if index < self.docs.len() && index != self.active {
            self.active = index;
            self.select = Some(self.docs[index].cursor);
            self.focus_editor = true;
            self.find.current = None;
            self.touched();
        }
    }

    fn new_doc(&mut self) {
        let doc = Document::new(self.next_id, self.next_untitled);
        self.next_id += 1;
        self.next_untitled += 1;
        self.docs.push(doc);
        self.active = self.docs.len() - 1;
        self.focus_editor = true;
        self.touched();
    }

    fn set_status(&mut self, text: impl Into<String>, error: bool) {
        self.status = Some((text.into(), Instant::now(), error));
    }

    fn remember_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(15);
    }

    fn open_path(&mut self, path: &Path) {
        if let Some(i) = self.docs.iter().position(|d| d.path.as_deref() == Some(path)) {
            self.activate(i);
            return;
        }
        match Document::open(self.next_id, path) {
            Ok(doc) => {
                self.next_id += 1;
                if doc.lossy {
                    self.set_status(tf("lossy", doc.format.encoding.label()), true);
                }
                // A blank new document makes room for the file (as Notepad++ does).
                if self.docs.len() == 1 && self.docs[0].is_blank() {
                    self.docs.clear();
                }
                self.docs.push(doc);
                self.active = self.docs.len() - 1;
                self.select = Some((0, 0));
                self.focus_editor = true;
                self.remember_recent(path);
                self.touched();
            }
            Err(e) => {
                self.recent.retain(|p| p != path);
                self.dialog = Some(Dialog::Message { title: t("cannot_open").to_owned(), text: format!("{}\n\n{e}", path.display()) });
            }
        }
    }

    fn save_doc(&mut self, index: usize, close_after: bool) {
        let Some(doc) = self.docs.get_mut(index) else { return };
        let Some(path) = doc.path.clone() else {
            let (id, name) = (doc.id, doc.suggested_name());
            platform::pick_save(&self.ctx, self.picker_tx.clone(), id, &name, close_after);
            return;
        };
        self.save_to(index, &path, close_after);
    }

    fn save_to(&mut self, index: usize, path: &Path, close_after: bool) {
        let Some(doc) = self.docs.get_mut(index) else { return };
        if !doc.format.encoding.is_unicode() && !crate::text::fits(&doc.text(), doc.format.encoding) {
            let label = doc.format.encoding.label();
            self.set_status(tf("unmappable", label), true);
        }
        let Some(doc) = self.docs.get_mut(index) else { return };
        match doc.save_to(path) {
            Ok(()) => {
                let (id, title) = (doc.id, doc.title());
                self.remember_recent(path);
                self.set_status(tf("saved", title), false);
                self.touched();
                if close_after && let Some(i) = self.index_of(id) {
                    self.remove_tab(i);
                }
            }
            Err(e) => self.dialog = Some(Dialog::Message { title: t("cannot_save").to_owned(), text: format!("{}\n\n{e}", path.display()) }),
        }
    }

    fn remove_tab(&mut self, index: usize) {
        if index >= self.docs.len() {
            return;
        }
        self.docs.remove(index);
        if self.docs.is_empty() {
            self.new_doc();
        }
        if self.active >= self.docs.len() || index < self.active {
            self.active = self.active.saturating_sub(1).min(self.docs.len() - 1);
        }
        self.select = self.docs.get(self.active).map(|d| d.cursor);
        self.focus_editor = true;
        self.touched();
    }

    fn close_tab(&mut self, id: u64) {
        let Some(i) = self.index_of(id) else { return };
        if self.docs[i].dirty() && !self.docs[i].is_blank() {
            self.activate(i);
            self.dialog = Some(Dialog::ConfirmClose { doc: id });
        } else {
            self.remove_tab(i);
        }
    }

    /// The selection of the active editor (characters, in order).
    fn selection(&self) -> (usize, usize) {
        let (a, b) = self.doc().map_or((0, 0), |d| d.cursor);
        (a.min(b), a.max(b))
    }

    fn select_bytes(&mut self, range: std::ops::Range<usize>) {
        if let Some(d) = self.doc() {
            let rope = d.buffer.rope();
            let (a, b) = (range.start.min(rope.len_bytes()), range.end.min(rope.len_bytes()));
            self.select = Some((rope.byte_to_char(a), rope.byte_to_char(b)));
        }
    }

    // ── persistence ────────────────────────────────────────────────────────────────────────

    /// Write the session, the settings and the macros now.
    pub fn persist(&mut self) {
        if let Err(e) = self.session.save(&self.docs, self.active, self.next_untitled, &self.recent) {
            log::error!("dexnotas: the session could not be saved: {e}");
            self.set_status(tf("session_not_saved", e), true);
        }
        let _ = session::write_json(&self.data_dir.join("settings.json"), &self.settings);
        let _ = session::write_json(&self.data_dir.join("macros.json"), &self.macros);
        self.unsaved_since = None;
    }

    fn periodic(&mut self) {
        // Typing touches the session: write it a moment after the last change.
        if let Some(since) = self.unsaved_since {
            if since.elapsed() >= Duration::from_millis(1500) {
                self.persist();
            } else {
                self.ctx.request_repaint_after(Duration::from_millis(1600) - since.elapsed().min(Duration::from_millis(1500)));
            }
        }
        // Optional: documents with a file are saved to it too.
        if self.settings.autosave_files
            && let Some(since) = self.dirty_since
            && since.elapsed() >= Duration::from_secs(2)
        {
            self.dirty_since = None;
            for i in 0..self.docs.len() {
                if self.docs[i].dirty() && self.docs[i].path.is_some() {
                    self.save_doc(i, false);
                }
            }
        }
        // A file changed by another app.
        if self.checked_disk.elapsed() >= Duration::from_secs(3) {
            self.checked_disk = Instant::now();
            for i in 0..self.docs.len() {
                if !self.docs[i].changed_on_disk() || self.docs[i].missing_on_disk() {
                    continue;
                }
                if self.docs[i].dirty() {
                    if self.dialog.is_none() {
                        self.dialog = Some(Dialog::ChangedOnDisk { doc: self.docs[i].id });
                    }
                } else {
                    let title = self.docs[i].title();
                    let encoding = self.docs[i].format.encoding;
                    match self.docs[i].reload(Some(encoding)) {
                        Ok(()) => self.set_status(tf("reloaded", title), false),
                        Err(e) => self.set_status(e, true),
                    }
                }
            }
            self.ctx.request_repaint_after(Duration::from_secs(3));
        }
    }

    // ── actions ────────────────────────────────────────────────────────────────────────────

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let push = |a: Action, s: KeyboardShortcut, ctx: &egui::Context, out: &mut Vec<Action>| {
            if ctx.input_mut(|i| i.consume_shortcut(&s)) {
                out.push(a);
            }
        };
        let mut out = Vec::new();
        push(Action::SaveAs, shortcut(CTRL_SHIFT, Key::S), ctx, &mut out);
        push(Action::Save, shortcut(CTRL, Key::S), ctx, &mut out);
        push(Action::New, shortcut(CTRL, Key::N), ctx, &mut out);
        push(Action::Open, shortcut(CTRL, Key::O), ctx, &mut out);
        if let Some(d) = self.doc() {
            push(Action::CloseTab(d.id), shortcut(CTRL, Key::W), ctx, &mut out);
        }
        push(Action::NextTab(false), shortcut(CTRL_SHIFT, Key::Tab), ctx, &mut out);
        push(Action::NextTab(true), shortcut(CTRL, Key::Tab), ctx, &mut out);
        push(Action::ShowFind { replace: false }, shortcut(CTRL, Key::F), ctx, &mut out);
        push(Action::ShowFind { replace: true }, shortcut(CTRL, Key::H), ctx, &mut out);
        push(Action::FindInTabs, shortcut(CTRL_SHIFT, Key::F), ctx, &mut out);
        push(Action::GoToLineDialog, shortcut(CTRL, Key::G), ctx, &mut out);
        push(Action::Edit(Command::DuplicateLines), shortcut(CTRL, Key::D), ctx, &mut out);
        push(Action::Edit(Command::DeleteLines), shortcut(CTRL_SHIFT, Key::K), ctx, &mut out);
        push(Action::Edit(Command::ToggleComment), shortcut(CTRL, Key::Slash), ctx, &mut out);
        push(Action::Edit(Command::Upper), shortcut(CTRL_SHIFT, Key::U), ctx, &mut out);
        push(Action::Edit(Command::Lower), shortcut(CTRL, Key::U), ctx, &mut out);
        push(Action::Edit(Command::JsonPretty), shortcut(CTRL_ALT, Key::J), ctx, &mut out);
        push(Action::Edit(Command::XmlPretty), shortcut(CTRL_ALT, Key::X), ctx, &mut out);
        push(Action::Zoom(1.0), shortcut(CTRL, Key::Equals), ctx, &mut out);
        push(Action::Zoom(1.0), shortcut(CTRL, Key::Plus), ctx, &mut out);
        push(Action::Zoom(-1.0), shortcut(CTRL, Key::Minus), ctx, &mut out);
        push(Action::ZoomReset, shortcut(CTRL, Key::Num0), ctx, &mut out);
        push(Action::ToggleRecord, shortcut(CTRL_SHIFT, Key::R), ctx, &mut out);
        push(Action::Play(Repeat::Times(1)), shortcut(CTRL_SHIFT, Key::P), ctx, &mut out);
        push(Action::TogglePreview, shortcut(CTRL_SHIFT, Key::V), ctx, &mut out);
        for (slot, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9].into_iter().enumerate() {
            if let Some(i) = self.macros.iter().position(|m| m.slot == Some(slot as u8 + 1)) {
                push(Action::PlaySaved(i), shortcut(CTRL_ALT, key), ctx, &mut out);
            }
        }
        if self.find.open || self.find.current.is_some() {
            push(Action::Edit(Command::FindNext { query: self.find.query.clone(), backwards: true }), shortcut(Modifiers::SHIFT, Key::F3), ctx, &mut out);
            push(Action::Edit(Command::FindNext { query: self.find.query.clone(), backwards: false }), shortcut(Modifiers::NONE, Key::F3), ctx, &mut out);
        }
        // Ctrl+wheel and pinch zoom the text.
        let zoom = ctx.input(|i| i.zoom_delta());
        if (zoom - 1.0).abs() > 0.001 {
            self.settings.font_size = (self.settings.font_size * zoom).clamp(Settings::MIN_FONT, Settings::MAX_FONT);
        }
        self.actions.extend(out);
    }

    fn handle(&mut self, action: Action) {
        match action {
            Action::New => self.new_doc(),
            Action::Open => platform::pick_open(&self.ctx, self.picker_tx.clone()),
            Action::OpenPath(p) => self.open_path(&p),
            Action::Save => self.save_doc(self.active, false),
            Action::SaveAs => {
                if let Some(d) = self.doc() {
                    platform::pick_save(&self.ctx, self.picker_tx.clone(), d.id, &d.suggested_name(), false);
                }
            }
            Action::SaveAll => {
                for i in 0..self.docs.len() {
                    if self.docs[i].dirty() && self.docs[i].path.is_some() {
                        self.save_doc(i, false);
                    }
                }
            }
            Action::Reload => {
                let encoding = self.doc().map(|d| d.format.encoding);
                if let Some(d) = self.doc_mut()
                    && let Err(e) = d.reload(encoding)
                {
                    self.set_status(e, true);
                }
            }
            Action::CloseTab(id) => self.close_tab(id),
            Action::CloseOthers => {
                let keep = self.doc().map(|d| d.id);
                let ids: Vec<u64> = self.docs.iter().map(|d| d.id).filter(|id| Some(*id) != keep).collect();
                for id in ids {
                    if let Some(i) = self.index_of(id)
                        && !self.docs[i].dirty()
                    {
                        self.remove_tab(i);
                    }
                }
                if self.docs.iter().any(|d| d.dirty() && Some(d.id) != keep) {
                    self.set_status(t("kept_unsaved"), false);
                }
            }
            Action::CloseAll => {
                let ids: Vec<u64> = self.docs.iter().filter(|d| !d.dirty()).map(|d| d.id).collect();
                for id in ids {
                    if let Some(i) = self.index_of(id) {
                        self.remove_tab(i);
                    }
                }
                if self.docs.iter().any(|d| d.dirty()) {
                    self.set_status(t("kept_unsaved"), false);
                }
            }
            Action::Activate(id) => {
                if let Some(i) = self.index_of(id) {
                    self.activate(i);
                }
            }
            Action::NextTab(forward) => {
                let n = self.docs.len();
                if n > 1 {
                    let i = if forward { (self.active + 1) % n } else { (self.active + n - 1) % n };
                    self.activate(i);
                }
            }
            Action::Share => {
                if let Some(d) = self.doc() {
                    let (title, text) = (d.title(), d.buffer.rope().to_string());
                    if !platform::share_text(&title, &text) {
                        self.ctx.copy_text(text);
                        self.set_status(t("copied_instead"), false);
                    }
                }
            }
            Action::Quit => {
                self.persist();
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Action::Undo | Action::Redo => {
                let redo = action == Action::Redo;
                if let Some(d) = self.doc_mut() {
                    let done = if redo { d.redo() } else { d.undo() };
                    if done {
                        let sel = d.cursor;
                        self.select = Some(sel);
                        self.edited();
                    }
                }
                self.focus_editor = true;
            }
            Action::Edit(command) => {
                if let Some(rec) = self.recording.as_mut() {
                    rec.push(Step::Command(command.clone()));
                }
                self.run_command(&command);
            }
            Action::ShowFind { replace } => {
                self.find.open = true;
                self.find.replace = replace;
                self.find.focus = true;
                // The selection (one line) is what to look for.
                if let Some(d) = self.doc() {
                    let (a, b) = self.selection();
                    if b > a && b - a < 1000 {
                        let selected = d.buffer.slice_string(a..b);
                        if !selected.contains('\n') {
                            self.find.query.pattern = if self.find.query.regex { regex::escape(&selected) } else { selected };
                        }
                    }
                }
            }
            Action::FindInTabs => {
                self.find.open = true;
                self.find_in_tabs();
            }
            Action::GoToLineDialog => self.dialog = Some(Dialog::GoToLine(String::new())),
            Action::ReopenAs(encoding) => {
                let dirty = self.doc().is_some_and(|d| d.dirty());
                if dirty {
                    self.set_status(t("save_first"), true);
                } else if let Some(d) = self.doc_mut() {
                    match d.reload(Some(encoding)) {
                        Ok(()) => {
                            let lossy = d.lossy;
                            self.touched();
                            if lossy {
                                self.set_status(tf("lossy", encoding.label()), true);
                            }
                        }
                        Err(e) => self.set_status(e, true),
                    }
                }
            }
            Action::ConvertTo(encoding) => {
                if let Some(d) = self.doc_mut() {
                    d.format.encoding = encoding;
                    if !encoding.is_unicode() {
                        d.format.bom = false;
                    }
                    let fits = encoding.is_unicode() || crate::text::fits(&d.text(), encoding);
                    d.mark_changed();
                    self.touched();
                    if !fits {
                        self.set_status(tf("unmappable", encoding.label()), true);
                    }
                }
            }
            Action::ToggleBom => {
                if let Some(d) = self.doc_mut()
                    && d.format.encoding.is_unicode()
                {
                    d.format.bom = !d.format.bom;
                    d.mark_changed();
                    self.touched();
                }
            }
            Action::SetLineEnding(e) => {
                if let Some(d) = self.doc_mut()
                    && d.format.line_ending != e
                {
                    d.format.line_ending = e;
                    d.mark_changed();
                    self.touched();
                }
            }
            Action::SetLanguage(l) => {
                if let Some(d) = self.doc_mut() {
                    d.language = l;
                    d.language_chosen = true;
                    self.touched();
                }
            }
            Action::JsonCheck => self.check(crate::format::json_check, "JSON"),
            Action::XmlCheck => self.check(crate::format::xml_check, "XML"),
            Action::YamlCheck => self.check(crate::format::yaml_check, "YAML"),
            Action::TomlCheck => self.check(crate::format::toml_check, "TOML"),
            Action::Zoom(step) => self.settings.font_size = (self.settings.font_size + step).clamp(Settings::MIN_FONT, Settings::MAX_FONT),
            Action::ZoomReset => self.settings.font_size = Settings::default().font_size,
            Action::ToggleRecord => match self.recording.take() {
                Some(steps) => {
                    let n = steps.len();
                    self.last_macro = steps;
                    self.set_status(tf("recorded", n), false);
                }
                None => {
                    self.playback = None;
                    self.recording = Some(Vec::new());
                    self.focus_editor = true;
                    self.set_status(t("recording"), false);
                }
            },
            Action::Play(repeat) => {
                let steps = self.last_macro.clone();
                self.start_playback(steps, repeat);
            }
            Action::PlaySaved(i) => {
                if let Some(m) = self.macros.get(i) {
                    let steps = m.steps.clone();
                    self.start_playback(steps, Repeat::Times(1));
                }
            }
            Action::PlayTimesDialog => self.dialog = Some(Dialog::PlayTimes("10".to_owned())),
            Action::SaveMacroDialog => {
                if self.last_macro.is_empty() {
                    self.set_status(t("nothing_recorded"), true);
                } else {
                    self.dialog = Some(Dialog::SaveMacro(format!("{} {}", t("macro"), self.macros.len() + 1)));
                }
            }
            Action::ManageMacros => self.dialog = Some(Dialog::ManageMacros),
            Action::About => self.dialog = Some(Dialog::About),
            Action::SetPreview(mode) => self.set_preview(mode),
            Action::TogglePreview => {
                let mode = self.doc().map_or(crate::preview::Mode::Off, |d| d.preview.mode);
                self.set_preview(if mode == crate::preview::Mode::Off { crate::preview::Mode::Split } else { crate::preview::Mode::Off });
            }
            Action::FollowLink(link) => self.follow_link(&link),
        }
    }

    fn set_preview(&mut self, mode: crate::preview::Mode) {
        let Some(d) = self.doc_mut() else { return };
        if crate::preview::Source::of(d.language).is_none() {
            self.set_status(t("preview_hint"), false);
            return;
        }
        d.preview.mode = mode;
        d.preview.followed_line = None;
        if mode != crate::preview::Mode::Only {
            self.focus_editor = true;
        }
    }

    /// A link in the preview: a web address opens in the browser, a file next to the document
    /// opens in a tab.
    fn follow_link(&mut self, link: &str) {
        let link = link.trim();
        if link.is_empty() || link.starts_with('#') {
            return;
        }
        let lower = link.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:") {
            self.ctx.open_url(egui::OpenUrl::new_tab(link));
            return;
        }
        let path = link.split('#').next().unwrap_or(link);
        let path = path.strip_prefix("file://").unwrap_or(path);
        let base = self.doc().and_then(|d| d.path.as_ref()).and_then(|p| p.parent()).map(Path::to_path_buf);
        let target = if Path::new(path).is_absolute() { PathBuf::from(path) } else if let Some(base) = base { base.join(path) } else { PathBuf::from(path) };
        if target.is_file() {
            self.open_path(&target);
        } else {
            self.set_status(tf("link_not_found", link), true);
        }
    }

    fn check(&mut self, f: fn(&str) -> Result<(), crate::format::Problem>, what: &str) {
        let Some(d) = self.doc_mut() else { return };
        let text = d.text();
        match f(&text) {
            Ok(()) => self.set_status(tf("valid", what), false),
            Err(p) => {
                if let Some(line) = p.line {
                    let r = find::line_range(&text, line);
                    let col = p.column.unwrap_or(1).saturating_sub(1);
                    let at = byte_to_char(&text, r.start) + col.min(text[r].chars().count());
                    self.select = Some((at, at));
                    self.focus_editor = true;
                }
                self.set_status(format!("{what}: {p}"), true);
            }
        }
    }

    /// Run an editing command on the active document (from a menu, a shortcut or a macro).
    /// Run an editing command on the active document (from a menu, a shortcut or a macro).
    fn run_command(&mut self, command: &Command) {
        use crate::edit_ops;
        let (a, b) = self.selection();
        let indent = self.settings.indent_unit();
        let width = self.settings.indent_width;
        match command {
            Command::SelectAll => {
                let len = self.doc().map_or(0, Document::len_chars);
                self.select = Some((0, len));
                self.focus_editor = true;
                return;
            }
            Command::GoToLine(line) => {
                if let Some(d) = self.doc() {
                    let rope = d.buffer.rope();
                    let at = rope.line_to_char(line.saturating_sub(1).min(rope.len_lines().saturating_sub(1)));
                    self.select = Some((at, at));
                    self.focus_editor = true;
                }
                return;
            }
            Command::FindNext { query, backwards } => return self.find_next(query, *backwards),
            Command::ReplaceNext { query, replacement } => return self.replace_next(query, replacement),
            Command::ReplaceAll { query, replacement } => return self.replace_all(query, replacement),
            _ => {}
        }
        let Some(doc) = self.docs.get_mut(self.active) else { return };
        let line_comment = doc.language.and_then(LanguageId::line_comment).unwrap_or("#");
        let rope = doc.buffer.rope();
        let len = rope.len_chars();
        // The part a command works on: the whole lines of the selection, the selection, or the
        // whole document; the command sees only that text.
        let lines_region = {
            let first = rope.char_to_line(a.min(len));
            let end_at = if b > a && b <= len && rope.char(b - 1) == '\n' { b - 1 } else { b.min(len) };
            let last = rope.char_to_line(end_at);
            let start = rope.line_to_char(first);
            let end = if last + 1 < rope.len_lines() { rope.line_to_char(last + 1) } else { len };
            start..end
        };
        let selection_or_all = if a == b { 0..len } else { a..b };
        let whole = 0..len;
        let multi_line = rope.slice(a.min(len)..b.min(len)).chars().any(|c| c == '\n');
        type Op<'a> = Box<dyn Fn(&str, std::ops::Range<usize>) -> Result<(String, std::ops::Range<usize>), String> + 'a>;
        let (region, op): (std::ops::Range<usize>, Op<'_>) = match command {
            Command::Upper => (selection_or_all.clone(), Box::new(|t: &str, _| Ok(edit_ops::change_case(t, 0..t.chars().count(), true)))),
            Command::Lower => (selection_or_all.clone(), Box::new(|t: &str, _| Ok(edit_ops::change_case(t, 0..t.chars().count(), false)))),
            Command::ToggleComment => (lines_region.clone(), Box::new(move |t: &str, s| Ok(edit_ops::toggle_comment(t, s, line_comment)))),
            Command::DuplicateLines => (lines_region.clone(), Box::new(|t: &str, s| Ok(edit_ops::duplicate_lines(t, s)))),
            Command::DeleteLines => (lines_region.clone(), Box::new(|t: &str, s| Ok(edit_ops::delete_lines(t, s)))),
            Command::SortAscending | Command::SortDescending => {
                let desc = *command == Command::SortDescending;
                let region = if multi_line { lines_region.clone() } else { whole.clone() };
                (region, Box::new(move |t: &str, _| Ok(edit_ops::sort_lines(t, 0..0, desc))))
            }
            Command::TrimTrailing => (whole.clone(), Box::new(|t: &str, s| Ok((edit_ops::trim_trailing(t), s.start..s.start)))),
            Command::Indent => (lines_region.clone(), Box::new(|t: &str, _| Ok(edit_ops::indent_lines(t, 0..t.chars().count(), &indent, false)))),
            Command::Outdent => (lines_region.clone(), Box::new(|t: &str, _| Ok(edit_ops::indent_lines(t, 0..t.chars().count(), &indent, true)))),
            Command::JsonPretty => (selection_or_all.clone(), Box::new(|t: &str, _| crate::format::json_pretty(t, width).map(|s| { let n = s.chars().count(); (s, 0..n) }).map_err(|p| format!("JSON: {p}")))),
            Command::JsonMinify => (selection_or_all.clone(), Box::new(|t: &str, _| crate::format::json_minify(t).map(|s| { let n = s.chars().count(); (s, 0..n) }).map_err(|p| format!("JSON: {p}")))),
            Command::XmlPretty => (selection_or_all.clone(), Box::new(|t: &str, _| crate::format::xml_reformat(t, width).map(|s| { let n = s.chars().count(); (s, 0..n) }).map_err(|p| format!("XML: {p}")))),
            Command::XmlMinify => (selection_or_all.clone(), Box::new(|t: &str, _| crate::format::xml_reformat(t, 0).map(|s| { let n = s.chars().count(); (s, 0..n) }).map_err(|p| format!("XML: {p}")))),
            _ => return,
        };
        let text = doc.buffer.slice_string(region.clone());
        let rel = a.saturating_sub(region.start).min(region.end - region.start)..b.saturating_sub(region.start).min(region.end - region.start);
        match op(&text, rel) {
            Ok((new, sel)) => {
                if new != text {
                    // A whole-document format keeps the cursor where it was; otherwise the
                    // result is selected.
                    let after = if a == b && region == whole && matches!(command, Command::JsonPretty | Command::JsonMinify | Command::XmlPretty | Command::XmlMinify | Command::TrimTrailing | Command::Upper | Command::Lower) {
                        let at = a.min(region.start + new.chars().count());
                        (at, at)
                    } else {
                        (region.start + sel.start, region.start + sel.end)
                    };
                    doc.replace(region, &new, after, crate::buffer::Group::Alone);
                    self.select = Some(after);
                    self.edited();
                }
                self.focus_editor = true;
            }
            Err(e) => self.set_status(e, true),
        }
    }

    /// The active document's text changed.
    fn edited(&mut self) {
        self.touched();
        self.dirty_since.get_or_insert_with(Instant::now);
    }

    // ── find ───────────────────────────────────────────────────────────────────────────────

    fn compile(&mut self, query: &Query) -> Option<regex::Regex> {
        match query.compile() {
            Ok(re) => Some(re),
            Err(e) => {
                self.find.note = e;
                None
            }
        }
    }

    fn find_next(&mut self, query: &Query, backwards: bool) {
        let Some(re) = self.compile(query) else { return };
        let (a, b) = self.selection();
        let Some(d) = self.doc_mut() else { return };
        let text = d.text();
        let from = if backwards { char_to_byte(&text, a) } else { char_to_byte(&text, b) };
        match find::find_next(&text, &re, from, backwards) {
            Some(r) => {
                self.find.current = Some(r.clone());
                self.select_bytes(r);
            }
            None => {
                self.find.current = None;
                self.set_status(tf("not_found", &query.pattern), true);
            }
        }
    }

    fn replace_next(&mut self, query: &Query, replacement: &str) {
        let Some(re) = self.compile(query) else { return };
        let (a, b) = self.selection();
        let Some(d) = self.doc_mut() else { return };
        let text = d.text();
        let (ba, bb) = (char_to_byte(&text, a), char_to_byte(&text, b));
        // The selection is a match: replace it, then find the next one.
        if ba < bb && re.find_at(&text, ba).is_some_and(|m| m.range() == (ba..bb)) {
            let with = find::replacement_for(&text, &re, ba..bb, replacement, query.regex);
            let end = a + with.chars().count();
            d.replace(a..b, &with, (end, end), crate::buffer::Group::Alone);
            let text = d.text();
            let from = char_to_byte(&text, end);
            let next = find::find_next(&text, &re, from, false);
            self.edited();
            match next {
                Some(r) => {
                    self.find.current = Some(r.clone());
                    self.select_bytes(r);
                }
                None => self.select = Some((end, end)),
            }
        } else {
            self.find_next(query, false);
        }
    }

    fn replace_all(&mut self, query: &Query, replacement: &str) {
        let Some(re) = self.compile(query) else { return };
        let (a, _) = self.selection();
        let Some(d) = self.doc_mut() else { return };
        let text = d.text();
        let (new, n) = find::replace_all(&text, &re, replacement, query.regex);
        if n > 0 {
            let at = a.min(new.chars().count());
            d.set_text(&new, (at, at));
            self.edited();
        }
        self.find.current = None;
        self.set_status(tf("replaced", n), false);
    }

    fn find_in_tabs(&mut self) {
        self.hits.clear();
        self.hits_open = true;
        let Some(re) = self.compile(&self.find.query.clone()) else { return };
        for d in &mut self.docs {
            let text = d.text();
            let rope = d.buffer.rope();
            for r in find::find_all(&text, &re).into_iter().take(2000) {
                let line = rope.byte_to_line(r.start);
                let line_text: String = rope.line(line).chars().take(200).collect();
                self.hits.push(Hit { doc: d.id, title: d.title(), line: line + 1, text: line_text.trim_end().to_owned(), range: r });
            }
        }
        let n = self.hits.len();
        self.find.note = tf("matches_in_tabs", n);
    }

    // ── macros ─────────────────────────────────────────────────────────────────────────────

    fn start_playback(&mut self, steps: Vec<Step>, repeat: Repeat) {
        if self.recording.is_some() {
            self.set_status(t("stop_recording_first"), true);
            return;
        }
        if steps.is_empty() {
            self.set_status(t("nothing_recorded"), true);
            return;
        }
        let run_start = self.selection().1;
        self.playback = Some(Playback { steps, index: 0, repeat, runs: 0, run_start });
        self.focus_editor = true;
    }

    /// One step of the macro being played: the keys up to the next command go to the editor in
    /// the next frame; a command runs now.
    fn advance_playback(&mut self) {
        if self.events_in_flight || !self.pending_events.is_empty() {
            return;
        }
        let Some(mut p) = self.playback.take() else { return };
        self.ctx.request_repaint();
        if p.index >= p.steps.len() {
            p.runs += 1;
            let cursor = self.selection().1;
            let len = self.doc().map_or(0, Document::len_chars);
            let more = match p.repeat {
                Repeat::Times(n) => p.runs < n,
                Repeat::ToEnd => cursor < len && cursor != p.run_start && p.runs < macros::MAX_RUNS,
            };
            if !more {
                self.set_status(tf("played", p.runs), false);
                return;
            }
            p.index = 0;
            p.run_start = cursor;
        }
        match &p.steps[p.index] {
            Step::Command(c) => {
                let c = c.clone();
                p.index += 1;
                self.playback = Some(p);
                self.run_command(&c);
            }
            _ => {
                while let Some(step) = p.steps.get(p.index) {
                    let Some(events) = macros::events_for(step) else { break };
                    self.pending_events.extend(events);
                    p.index += 1;
                }
                self.focus_editor = true;
                self.playback = Some(p);
            }
        }
    }

    // ── frame ──────────────────────────────────────────────────────────────────────────────

    fn apply_settings(&mut self, ctx: &egui::Context) {
        if self.applied.as_ref() == Some(&self.settings) {
            return;
        }
        crate::i18n::set_language(self.settings.language);
        ctx.set_visuals(if self.settings.dark { egui::Visuals::dark() } else { egui::Visuals::light() });
        self.applied = Some(self.settings.clone());
        self.touched();
    }

    fn closing(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) {
            // Android's Back (craft-android makes it a close request) first closes what is open
            // over the editor: a dialog, a menu, the search results, the find bar.
            if cfg!(target_os = "android") && (self.dialog.is_some() || self.tab_menu.is_some() || self.hits_open || self.find.open) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if self.dialog.take().is_none() && self.tab_menu.take().is_none() {
                    if self.hits_open {
                        self.hits_open = false;
                    } else {
                        self.find.open = false;
                        self.find.current = None;
                    }
                }
                self.focus_editor = true;
                return;
            }
            // Closing the window never asks: the tabs and their unsaved text come back next time.
            self.recording = None;
            self.persist();
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_settings(ctx);
        self.highlighter.receive(&mut self.docs);
        self.previewer.receive(&mut self.docs);
        for path in platform::opened() {
            self.open_path(&path);
        }
        while let Ok(picked) = self.picker_rx.try_recv() {
            match picked {
                Picked::Open(paths) => {
                    for p in paths {
                        self.open_path(&p);
                    }
                }
                Picked::SaveAs { doc, path, close } => {
                    if let Some(i) = self.index_of(doc) {
                        self.save_to(i, &path, close);
                    }
                }
            }
        }
        if self.dialog.is_none() {
            self.shortcuts(ctx);
        }
        // Recording: what reaches the editor (shortcuts were taken out above; menu commands are
        // recorded where they run).
        if let Some(rec) = self.recording.as_mut()
            && self.editor_focused
        {
            ctx.input(|i| macros::record_events(rec, &i.events));
        }
        let actions = std::mem::take(&mut self.actions);
        for a in actions {
            self.handle(a);
        }
        self.advance_playback();
        self.periodic();
        self.closing(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        menus::window(self, ui);
        let actions = std::mem::take(&mut self.actions);
        for a in actions {
            self.handle(a);
        }
        self.events_in_flight = false;
        if self.playback.is_some() {
            ui.ctx().request_repaint();
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.persist();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist();
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        // A macro's input goes to the editor (which also does the indent and Tab).
        if !self.pending_events.is_empty() {
            raw.events.append(&mut self.pending_events);
            self.events_in_flight = true;
        }
    }
}
