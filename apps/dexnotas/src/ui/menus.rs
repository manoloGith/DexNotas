//! The window's parts: the menu bar, the tabs, the find bar, the results of a search in all tabs,
//! the status bar, the editor and the dialogs.

use egui::{Button, Color32, Key, RichText, Ui};

use super::editor::{self, Look, Requests};
use super::{Action, App, Dialog, theme};
use crate::find::{self, char_to_byte};
use crate::i18n::{Language, t, tf};
use crate::macros::{Command, Macro, Repeat};
use crate::syntax::LanguageId;
use crate::text::{Encoding, LineEnding};

fn item(ui: &mut Ui, label: &str, shortcut: &str) -> bool {
    let button = if shortcut.is_empty() { Button::new(label) } else { Button::new(label).shortcut_text(shortcut) };
    ui.add(button).clicked()
}

fn check(ui: &mut Ui, value: &mut bool, label: &str) -> bool {
    let before = *value;
    ui.checkbox(value, label);
    before != *value
}

pub fn window(app: &mut App, ui: &mut Ui) {
    let pal = theme::palette(app.settings.dark);
    egui::Panel::top("menu")
        .frame(egui::Frame::new().fill(pal.bar).inner_margin(egui::Margin::symmetric(6, 2)))
        .show(ui, |ui| menu_bar(app, ui));
    let tabs: Vec<craft_android::tabs::Tab> = app
        .docs
        .iter()
        .enumerate()
        .map(|(i, d)| craft_android::tabs::Tab::new(d.id, d.title(), d.dirty, i == app.active))
        .collect();
    let strip = craft_android::tabs::TabStrip::new(&tabs).new_tooltip(t("new_tab")).close_tooltip(t("close_tab")).show_panel(ui);
    match strip.action {
        Some(craft_android::tabs::TabAction::Activate(id)) => app.actions.push(Action::Activate(id)),
        Some(craft_android::tabs::TabAction::Close(id)) => app.actions.push(Action::CloseTab(id)),
        Some(craft_android::tabs::TabAction::New) => app.actions.push(Action::New),
        None => {}
    }
    // Right click on a tab: its menu (the strip itself takes the other buttons).
    let pointer = ui.input(|i| i.pointer.interact_pos());
    if ui.input(|i| i.pointer.secondary_pressed())
        && let Some(pos) = pointer
        && let Some((id, _, _)) = strip.tabs.iter().find(|(_, rect, _)| rect.contains(pos))
    {
        app.tab_menu = Some((*id, pos));
    }
    tab_menu(app, ui.ctx());
    egui::Panel::bottom("status")
        .frame(egui::Frame::new().fill(pal.bar).inner_margin(egui::Margin::symmetric(10, 3)))
        .show(ui, |ui| status_bar(app, ui));
    if app.hits_open {
        egui::Panel::bottom("hits")
            .resizable(true)
            .default_size(160.0)
            .frame(egui::Frame::new().fill(pal.bar).inner_margin(6))
            .show(ui, |ui| hits(app, ui));
    }
    if app.find.open {
        egui::Panel::top("find")
            .frame(egui::Frame::new().fill(pal.bar).inner_margin(egui::Margin::symmetric(8, 4)))
            .show(ui, |ui| find_bar(app, ui));
    }
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let look = Look {
            font: egui::FontId::monospace(app.settings.font_size),
            dark: app.settings.dark,
            wrap: app.settings.word_wrap,
            line_numbers: app.settings.line_numbers,
            highlight_line: app.settings.highlight_line,
        };
        let marks = if app.find.open && !app.find.query.pattern.is_empty() {
            match (app.find.query.compile(), app.docs.get(app.active)) {
                (Ok(re), Some(d)) if d.text.len() < 8 * 1024 * 1024 => find::find_all(&d.text, &re),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        if app.find.open && !app.find.query.pattern.is_empty() && app.find.query.compile().is_ok() {
            app.find.note = tf("matches", marks.len());
        }
        let requests = Requests {
            focus: std::mem::take(&mut app.focus_editor) && app.dialog.is_none() && !app.find.focus,
            select: app.select.take(),
            current_mark: app.find.current.clone(),
            marks,
        };
        let active = app.active;
        if let Some(doc) = app.docs.get_mut(active) {
            let out = editor::show(ui, doc, &mut app.cache, &look, requests);
            app.editor_focused = out.focused;
            if out.changed {
                app.find.current = None;
                app.cache_changed();
            } else if out.cursor.is_some() && out.cursor != app.last_cursor {
                app.touched_cursor();
            }
            app.last_cursor = out.cursor;
        }
    });
    dialogs(app, ui.ctx());
}

fn tab_menu(app: &mut App, ctx: &egui::Context) {
    let Some((id, pos)) = app.tab_menu else { return };
    let area = egui::Area::new(egui::Id::unique("tab-menu")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
        egui::Frame::menu(ui.style()).show(ui, |ui| {
            ui.set_min_width(180.0);
            let mut chosen = false;
            if item(ui, t("close_tab"), "") {
                app.actions.push(Action::CloseTab(id));
                chosen = true;
            }
            if item(ui, t("close_others"), "") {
                app.actions.push(Action::Activate(id));
                app.actions.push(Action::CloseOthers);
                chosen = true;
            }
            if let Some(path) = app.docs.iter().find(|d| d.id == id).and_then(|d| d.path.clone())
                && item(ui, t("copy_path"), "")
            {
                ui.ctx().copy_text(path.display().to_string());
                chosen = true;
            }
            chosen
        })
        .inner
    });
    let clicked_elsewhere = ctx.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer() && !ctx.input(|i| i.pointer.secondary_pressed());
    if area.inner || clicked_elsewhere || ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.tab_menu = None;
    }
}

fn menu_bar(app: &mut App, ui: &mut Ui) {
    let a = &mut app.actions;
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button(t("menu_file"), |ui| {
            if item(ui, t("new"), "Ctrl+N") {
                a.push(Action::New);
            }
            if item(ui, t("open"), "Ctrl+O") {
                a.push(Action::Open);
            }
            ui.menu_button(t("recent"), |ui| {
                if app.recent.is_empty() {
                    ui.label(t("none"));
                }
                for p in &app.recent {
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    if ui.button(name).on_hover_text(p.display().to_string()).clicked() {
                        a.push(Action::OpenPath(p.clone()));
                    }
                }
            });
            ui.separator();
            if item(ui, t("save"), "Ctrl+S") {
                a.push(Action::Save);
            }
            if item(ui, t("save_as"), "Ctrl+Shift+S") {
                a.push(Action::SaveAs);
            }
            if item(ui, t("save_all"), "") {
                a.push(Action::SaveAll);
            }
            if item(ui, t("reload"), "") {
                a.push(Action::Reload);
            }
            ui.separator();
            if cfg!(target_os = "android") && item(ui, t("share"), "") {
                a.push(Action::Share);
            }
            if let Some(d) = app.docs.get(app.active)
                && item(ui, t("close_tab"), "Ctrl+W")
            {
                a.push(Action::CloseTab(d.id));
            }
            if item(ui, t("close_others"), "") {
                a.push(Action::CloseOthers);
            }
            if item(ui, t("close_all"), "") {
                a.push(Action::CloseAll);
            }
            ui.separator();
            if item(ui, t("quit"), "") {
                a.push(Action::Quit);
            }
        });
        ui.menu_button(t("menu_edit"), |ui| {
            if item(ui, t("undo"), "Ctrl+Z") {
                a.push(Action::Undo);
            }
            if item(ui, t("redo"), "Ctrl+Shift+Z") {
                a.push(Action::Redo);
            }
            ui.separator();
            if item(ui, t("select_all"), "Ctrl+A") {
                a.push(Action::Edit(Command::SelectAll));
            }
            ui.separator();
            if item(ui, t("upper"), "Ctrl+Shift+U") {
                a.push(Action::Edit(Command::Upper));
            }
            if item(ui, t("lower"), "Ctrl+U") {
                a.push(Action::Edit(Command::Lower));
            }
            ui.separator();
            if item(ui, t("toggle_comment"), "Ctrl+/") {
                a.push(Action::Edit(Command::ToggleComment));
            }
            if item(ui, t("duplicate_lines"), "Ctrl+D") {
                a.push(Action::Edit(Command::DuplicateLines));
            }
            if item(ui, t("delete_lines"), "Ctrl+Shift+K") {
                a.push(Action::Edit(Command::DeleteLines));
            }
            if item(ui, t("indent"), "Tab") {
                a.push(Action::Edit(Command::Indent));
            }
            if item(ui, t("outdent"), "Shift+Tab") {
                a.push(Action::Edit(Command::Outdent));
            }
            if item(ui, t("sort_asc"), "") {
                a.push(Action::Edit(Command::SortAscending));
            }
            if item(ui, t("sort_desc"), "") {
                a.push(Action::Edit(Command::SortDescending));
            }
            if item(ui, t("trim_trailing"), "") {
                a.push(Action::Edit(Command::TrimTrailing));
            }
        });
        ui.menu_button(t("menu_search"), |ui| {
            if item(ui, t("find"), "Ctrl+F") {
                a.push(Action::ShowFind { replace: false });
            }
            if item(ui, t("replace"), "Ctrl+H") {
                a.push(Action::ShowFind { replace: true });
            }
            if item(ui, t("find_next"), "F3") {
                a.push(Action::Edit(Command::FindNext { query: app.find.query.clone(), backwards: false }));
            }
            if item(ui, t("find_prev"), "Shift+F3") {
                a.push(Action::Edit(Command::FindNext { query: app.find.query.clone(), backwards: true }));
            }
            if item(ui, t("find_in_tabs"), "Ctrl+Shift+F") {
                a.push(Action::FindInTabs);
            }
            ui.separator();
            if item(ui, t("go_to_line"), "Ctrl+G") {
                a.push(Action::GoToLineDialog);
            }
        });
        ui.menu_button(t("menu_view"), |ui| {
            check(ui, &mut app.settings.word_wrap, t("word_wrap"));
            check(ui, &mut app.settings.line_numbers, t("line_numbers"));
            check(ui, &mut app.settings.highlight_line, t("highlight_line"));
            check(ui, &mut app.settings.dark, t("dark_theme"));
            ui.separator();
            if item(ui, t("zoom_in"), "Ctrl++") {
                a.push(Action::Zoom(1.0));
            }
            if item(ui, t("zoom_out"), "Ctrl+-") {
                a.push(Action::Zoom(-1.0));
            }
            if item(ui, t("zoom_reset"), "Ctrl+0") {
                a.push(Action::ZoomReset);
            }
        });
        ui.menu_button(t("menu_encoding"), |ui| {
            let current = app.docs.get(app.active).map(|d| d.format);
            ui.menu_button(t("reopen_as"), |ui| {
                for e in Encoding::ALL {
                    if ui.button(e.label()).clicked() {
                        a.push(Action::ReopenAs(e));
                    }
                }
            });
            ui.menu_button(t("convert_to"), |ui| {
                for e in Encoding::ALL {
                    let on = current.is_some_and(|f| f.encoding == e);
                    if ui.radio(on, e.label()).clicked() {
                        a.push(Action::ConvertTo(e));
                    }
                }
            });
            let unicode = current.is_some_and(|f| f.encoding.is_unicode());
            if ui.add_enabled(unicode, egui::Checkbox::new(&mut current.is_some_and(|f| f.bom), t("bom"))).clicked() {
                a.push(Action::ToggleBom);
            }
            ui.separator();
            ui.label(RichText::new(t("line_endings")).weak());
            for e in LineEnding::ALL {
                let on = current.is_some_and(|f| f.line_ending == e);
                if ui.radio(on, e.label()).clicked() {
                    a.push(Action::SetLineEnding(e));
                }
            }
        });
        ui.menu_button(t("menu_language"), |ui| {
            let current = app.docs.get(app.active).and_then(|d| d.language);
            egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                if ui.radio(current.is_none(), t("plain_text")).clicked() {
                    a.push(Action::SetLanguage(None));
                }
                for l in LanguageId::ALL {
                    if ui.radio(current == Some(l), l.label()).clicked() {
                        a.push(Action::SetLanguage(Some(l)));
                    }
                }
            });
        });
        ui.menu_button(t("menu_tools"), |ui| {
            ui.label(RichText::new("JSON").weak());
            if item(ui, t("json_pretty"), "Ctrl+Alt+J") {
                a.push(Action::Edit(Command::JsonPretty));
            }
            if item(ui, t("json_minify"), "") {
                a.push(Action::Edit(Command::JsonMinify));
            }
            if item(ui, t("json_check"), "") {
                a.push(Action::JsonCheck);
            }
            ui.separator();
            ui.label(RichText::new("XML / HTML / SVG").weak());
            if item(ui, t("xml_pretty"), "Ctrl+Alt+X") {
                a.push(Action::Edit(Command::XmlPretty));
            }
            if item(ui, t("xml_minify"), "") {
                a.push(Action::Edit(Command::XmlMinify));
            }
            if item(ui, t("xml_check"), "") {
                a.push(Action::XmlCheck);
            }
            ui.separator();
            if item(ui, t("yaml_check"), "") {
                a.push(Action::YamlCheck);
            }
            if item(ui, t("toml_check"), "") {
                a.push(Action::TomlCheck);
            }
            ui.separator();
            ui.label(RichText::new(t("preferences")).weak());
            check(ui, &mut app.settings.indent_spaces, t("indent_spaces"));
            ui.horizontal(|ui| {
                ui.label(t("indent_width"));
                for w in [2, 4, 8] {
                    if ui.selectable_label(app.settings.indent_width == w, w.to_string()).clicked() {
                        app.settings.indent_width = w;
                    }
                }
            });
            check(ui, &mut app.settings.auto_indent, t("auto_indent"));
            check(ui, &mut app.settings.autosave_files, t("autosave_files"));
            ui.menu_button(t("interface_language"), |ui| {
                for (l, label) in [(Language::Auto, t("system_language")), (Language::Spanish, "Español"), (Language::English, "English")] {
                    if ui.radio(app.settings.language == l, label).clicked() {
                        app.settings.language = l;
                    }
                }
            });
        });
        ui.menu_button(t("menu_macros"), |ui| {
            let recording = app.recording.is_some();
            if item(ui, if recording { t("stop_recording") } else { t("start_recording") }, "Ctrl+Shift+R") {
                a.push(Action::ToggleRecord);
            }
            let has = !app.last_macro.is_empty() && !recording;
            if ui.add_enabled(has, Button::new(t("play")).shortcut_text("Ctrl+Shift+P")).clicked() {
                a.push(Action::Play(Repeat::Times(1)));
            }
            if ui.add_enabled(has, Button::new(t("play_times"))).clicked() {
                a.push(Action::PlayTimesDialog);
            }
            if ui.add_enabled(has, Button::new(t("play_to_end"))).clicked() {
                a.push(Action::Play(Repeat::ToEnd));
            }
            if ui.add_enabled(has, Button::new(t("save_macro"))).clicked() {
                a.push(Action::SaveMacroDialog);
            }
            if !app.macros.is_empty() {
                ui.separator();
                for (i, m) in app.macros.iter().enumerate() {
                    let keys = m.slot.map(|s| format!("Ctrl+Alt+{s}")).unwrap_or_default();
                    if ui.add_enabled(!recording, Button::new(&m.name).shortcut_text(keys)).clicked() {
                        a.push(Action::PlaySaved(i));
                    }
                }
            }
            ui.separator();
            if item(ui, t("manage_macros"), "") {
                a.push(Action::ManageMacros);
            }
        });
        ui.menu_button(t("menu_help"), |ui| {
            if item(ui, t("about"), "") {
                a.push(Action::About);
            }
        });
    });
}

fn find_bar(app: &mut App, ui: &mut Ui) {
    let pal = theme::palette(app.settings.dark);
    let mut push = Vec::new();
    ui.horizontal_wrapped(|ui| {
        let field = ui.add(egui::TextEdit::singleline(&mut app.find.query.pattern).hint_text(t("find_hint")).desired_width(260.0).id(egui::Id::unique("find-field")));
        if std::mem::take(&mut app.find.focus) {
            field.request_focus();
        }
        if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            let backwards = ui.input(|i| i.modifiers.shift);
            push.push(Action::Edit(Command::FindNext { query: app.find.query.clone(), backwards }));
            field.request_focus();
        }
        if field.changed() {
            app.find.current = None;
        }
        ui.toggle_value(&mut app.find.query.match_case, "Aa").on_hover_text(t("match_case"));
        ui.toggle_value(&mut app.find.query.whole_word, "W").on_hover_text(t("whole_word"));
        ui.toggle_value(&mut app.find.query.regex, ".*").on_hover_text(t("regex"));
        if ui.button(RichText::new("▲").monospace()).on_hover_text(t("find_prev")).clicked() {
            push.push(Action::Edit(Command::FindNext { query: app.find.query.clone(), backwards: true }));
        }
        if ui.button(RichText::new("▼").monospace()).on_hover_text(t("find_next")).clicked() {
            push.push(Action::Edit(Command::FindNext { query: app.find.query.clone(), backwards: false }));
        }
        if ui.button(t("find_in_tabs_short")).clicked() {
            push.push(Action::FindInTabs);
        }
        ui.toggle_value(&mut app.find.replace, t("replace"));
        let error = app.find.query.compile().err().filter(|e| !e.is_empty());
        match error {
            Some(e) => {
                ui.colored_label(pal.error, e.lines().last().unwrap_or("").trim().to_owned());
            }
            None => {
                ui.label(RichText::new(&app.find.note).weak());
            }
        }
        if ui.button(RichText::new("×").monospace()).on_hover_text(t("close")).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
            app.find.open = false;
            app.find.current = None;
            app.focus_editor = true;
        }
    });
    if app.find.replace {
        ui.horizontal_wrapped(|ui| {
            let field = ui.add(egui::TextEdit::singleline(&mut app.find.replacement).hint_text(t("replace_hint")).desired_width(260.0));
            let query = app.find.query.clone();
            let replacement = app.find.replacement.clone();
            if (field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) || ui.button(t("replace_one")).clicked() {
                push.push(Action::Edit(Command::ReplaceNext { query: query.clone(), replacement: replacement.clone() }));
            }
            if ui.button(t("replace_all")).clicked() {
                push.push(Action::Edit(Command::ReplaceAll { query, replacement }));
            }
            if app.find.query.regex {
                ui.label(RichText::new(t("groups_hint")).weak());
            }
        });
    }
    app.actions.extend(push);
}

fn hits(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.strong(tf("matches_in_tabs", app.hits.len()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(RichText::new("×").monospace()).clicked() {
                app.hits_open = false;
            }
        });
    });
    let mut go = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (i, h) in app.hits.iter().enumerate().take(5000) {
            let label = format!("{}:{}  {}", h.title, h.line, h.text.trim());
            if ui.add(egui::Label::new(RichText::new(label).monospace()).truncate().sense(egui::Sense::click())).clicked() {
                go = Some(i);
            }
        }
    });
    if let Some(i) = go {
        let (doc, range) = (app.hits[i].doc, app.hits[i].range.clone());
        if let Some(index) = app.index_of(doc) {
            app.activate(index);
            app.find.current = Some(range.clone());
            app.select_bytes(range);
            app.focus_editor = true;
        }
    }
}

fn status_bar(app: &mut App, ui: &mut Ui) {
    let pal = theme::palette(app.settings.dark);
    ui.horizontal(|ui| {
        if app.recording.is_some() {
            ui.colored_label(pal.recording, RichText::new(format!("● {}", t("rec"))).monospace());
        } else if app.playback.is_some() {
            ui.colored_label(pal.recording, RichText::new(format!("▶ {}", t("playing"))).monospace());
        }
        if let Some((text, at, error)) = &app.status {
            if at.elapsed().as_secs() < 8 {
                let color = if *error { pal.error } else { ui.visuals().text_color() };
                ui.add(egui::Label::new(RichText::new(text).color(color)).truncate());
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(8));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let Some(d) = app.docs.get(app.active) else { return };
            let f = d.format;
            ui.label(f.line_ending.short());
            ui.separator();
            ui.label(if f.bom { format!("{} BOM", f.encoding.short()) } else { f.encoding.short().to_owned() });
            ui.separator();
            ui.label(d.language.map_or(t("plain_text"), LanguageId::label));
            ui.separator();
            let (a, b) = d.cursor;
            let at = char_to_byte(&d.text, b);
            let (line, col) = find::line_col(&d.text, at);
            let lines = d.text.matches('\n').count() + 1;
            if a != b {
                ui.label(tf("selected", a.abs_diff(b)));
                ui.separator();
            }
            ui.label(format!("{} {line}, {} {col}  ·  {} {lines}", t("ln"), t("col"), t("lines")));
        });
    });
}

fn dialogs(app: &mut App, ctx: &egui::Context) {
    let Some(dialog) = app.dialog.take() else { return };
    let mut keep = true;
    let mut next: Option<Dialog> = None;
    let modal = egui::Modal::new(egui::Id::unique("dexnotas-dialog")).show(ctx, |ui| {
        ui.set_max_width(460.0);
        match &dialog {
            Dialog::ConfirmClose { doc } => {
                let title = app.docs.iter().find(|d| d.id == *doc).map(|d| d.title()).unwrap_or_default();
                ui.heading(t("unsaved_title"));
                ui.label(tf("unsaved_text", title));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t("save")).clicked() {
                        if let Some(i) = app.index_of(*doc) {
                            app.save_doc(i, true);
                        }
                        keep = false;
                    }
                    if ui.button(t("dont_save")).clicked() {
                        if let Some(i) = app.index_of(*doc) {
                            app.remove_tab(i);
                        }
                        keep = false;
                    }
                    if ui.button(t("cancel")).clicked() {
                        keep = false;
                    }
                });
            }
            Dialog::GoToLine(text) => {
                let mut text = text.clone();
                let lines = app.docs.get(app.active).map_or(1, |d| d.text.matches('\n').count() + 1);
                ui.heading(t("go_to_line"));
                ui.label(tf("line_range", lines));
                let field = ui.text_edit_singleline(&mut text);
                field.request_focus();
                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                ui.horizontal(|ui| {
                    if ui.button(t("go")).clicked() || enter {
                        if let Ok(n) = text.trim().parse::<usize>() {
                            app.actions.push(Action::Edit(Command::GoToLine(n.max(1))));
                        }
                        keep = false;
                    }
                    if ui.button(t("cancel")).clicked() {
                        keep = false;
                    }
                });
                if keep {
                    next = Some(Dialog::GoToLine(text));
                }
            }
            Dialog::PlayTimes(text) => {
                let mut text = text.clone();
                ui.heading(t("play_times"));
                let field = ui.text_edit_singleline(&mut text);
                field.request_focus();
                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                ui.horizontal(|ui| {
                    if ui.button(t("play")).clicked() || enter {
                        if let Ok(n) = text.trim().parse::<usize>() {
                            app.actions.push(Action::Play(Repeat::Times(n.clamp(1, crate::macros::MAX_RUNS))));
                        }
                        keep = false;
                    }
                    if ui.button(t("cancel")).clicked() {
                        keep = false;
                    }
                });
                if keep {
                    next = Some(Dialog::PlayTimes(text));
                }
            }
            Dialog::SaveMacro(name) => {
                let mut name = name.clone();
                ui.heading(t("save_macro"));
                ui.label(t("macro_name"));
                let field = ui.text_edit_singleline(&mut name);
                field.request_focus();
                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                ui.horizontal(|ui| {
                    if (ui.button(t("save")).clicked() || enter) && !name.trim().is_empty() {
                        let used: Vec<u8> = app.macros.iter().filter_map(|m| m.slot).collect();
                        let slot = (1..=9).find(|s| !used.contains(s));
                        app.macros.push(Macro { name: name.trim().to_owned(), steps: app.last_macro.clone(), slot });
                        app.persist();
                        keep = false;
                    }
                    if ui.button(t("cancel")).clicked() {
                        keep = false;
                    }
                });
                if keep {
                    next = Some(Dialog::SaveMacro(name));
                }
            }
            Dialog::ManageMacros => {
                ui.heading(t("manage_macros"));
                if app.macros.is_empty() {
                    ui.label(t("no_macros"));
                }
                let mut remove = None;
                let mut changed = false;
                egui::Grid::new("macros").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
                    for (i, m) in app.macros.iter_mut().enumerate() {
                        changed |= ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(180.0)).changed();
                        ui.label(tf("steps", m.steps.len()));
                        let current = m.slot.map(|s| format!("Ctrl+Alt+{s}")).unwrap_or_else(|| t("no_shortcut").to_owned());
                        egui::ComboBox::from_id_salt(("slot", i)).selected_text(current).show_ui(ui, |ui| {
                            changed |= ui.selectable_value(&mut m.slot, None, t("no_shortcut")).changed();
                            for s in 1..=9u8 {
                                changed |= ui.selectable_value(&mut m.slot, Some(s), format!("Ctrl+Alt+{s}")).changed();
                            }
                        });
                        if ui.button(t("delete")).clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = remove {
                    app.macros.remove(i);
                    changed = true;
                }
                // One macro per shortcut.
                if changed {
                    let mut seen = Vec::new();
                    for m in app.macros.iter_mut().rev() {
                        if let Some(s) = m.slot {
                            if seen.contains(&s) {
                                m.slot = None;
                            } else {
                                seen.push(s);
                            }
                        }
                    }
                    app.touched();
                }
                ui.add_space(6.0);
                if ui.button(t("close")).clicked() {
                    keep = false;
                }
                if keep {
                    next = Some(Dialog::ManageMacros);
                }
            }
            Dialog::ChangedOnDisk { doc } => {
                let title = app.docs.iter().find(|d| d.id == *doc).map(|d| d.title()).unwrap_or_default();
                ui.heading(t("changed_on_disk"));
                ui.label(tf("changed_on_disk_text", title));
                ui.horizontal(|ui| {
                    if ui.button(t("reload")).clicked() {
                        if let Some(i) = app.index_of(*doc) {
                            let e = app.docs[i].format.encoding;
                            if let Err(err) = app.docs[i].reload(Some(e)) {
                                app.set_status(err, true);
                            }
                        }
                        keep = false;
                    }
                    if ui.button(t("keep_mine")).clicked() {
                        if let Some(d) = app.docs.iter_mut().find(|d| d.id == *doc) {
                            d.disk_time = d.path.as_ref().and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
                        }
                        keep = false;
                    }
                });
            }
            Dialog::Message { title, text } => {
                ui.heading(title);
                ui.label(text);
                ui.add_space(6.0);
                if ui.button(t("ok")).clicked() {
                    keep = false;
                }
            }
            Dialog::About => {
                ui.heading(format!("DexNotas {}", env!("CARGO_PKG_VERSION")));
                ui.label(t("about_text"));
                ui.add_space(6.0);
                ui.label(RichText::new(t("about_credits")).weak());
                ui.add_space(6.0);
                if ui.button(t("ok")).clicked() {
                    keep = false;
                }
            }
        }
    });
    if modal.should_close() {
        keep = false;
    }
    if keep {
        app.dialog = Some(next.unwrap_or(dialog));
    } else {
        app.focus_editor = true;
        if app.dialog.is_none() {
            app.dialog = None;
        }
    }
    let _ = Color32::TRANSPARENT;
}
