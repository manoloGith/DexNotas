//! What survives closing the app: the open tabs with their unsaved text, the settings, the
//! recent files and the saved macros, as JSON files in the app's private storage.
//!
//! A tab whose file is saved keeps only its path; a new or changed one also keeps its text
//! (`session/<id>.txt`), so nothing typed is lost when the app is closed, killed by the system
//! or by DeX's close button, which an app cannot intercept.
//!
//! The session must survive updates of the app: it is read tab by tab, and what a version does not
//! understand (a language, an encoding, a field) is left at its default instead of losing the
//! tab; the last session that read well is kept as `session.bak.json`; and if neither can be read,
//! the texts in `session/` come back as tabs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::document::{self, Document};
use crate::syntax::LanguageId;
use crate::text::Format;

/// Where DexNotas keeps its files: `$XDG_DATA_HOME/DexNotas` (on Android craft-android points
/// `XDG_DATA_HOME` into the app's private storage), else `~/.local/share/DexNotas`.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("DexNotas")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TabEntry {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub untitled: Option<u32>,
    #[serde(default)]
    pub format: Format,
    #[serde(default, deserialize_with = "lenient_language")]
    pub language: Option<LanguageId>,
    #[serde(default)]
    pub language_chosen: bool,
    #[serde(default)]
    pub saved_hash: u64,
    /// The text is in `session/<id>.txt` (a new or changed document).
    #[serde(default)]
    pub stored: bool,
    #[serde(default)]
    pub cursor: (usize, usize),
    /// Where the Markdown/HTML preview was.
    #[serde(default)]
    pub preview: crate::preview::Mode,
}

/// A language this version doesn't know (written by a newer or older one) is plain text, not an
/// unreadable session.
fn lenient_language<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<LanguageId>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value.as_str().and_then(LanguageId::from_key))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionFile {
    pub tabs: Vec<TabEntry>,
    #[serde(default)]
    pub active: usize,
    #[serde(default)]
    pub next_untitled: u32,
    #[serde(default)]
    pub recent: Vec<PathBuf>,
}

/// A tab entry whatever it holds: as much of it as can be read, if at least its id.
fn tab_from(value: &serde_json::Value) -> Option<TabEntry> {
    if let Ok(tab) = serde_json::from_value::<TabEntry>(value.clone()) {
        return Some(tab);
    }
    let o = value.as_object()?;
    let field = |k: &str| o.get(k).cloned().unwrap_or(serde_json::Value::Null);
    Some(TabEntry {
        id: o.get("id")?.as_u64()?,
        path: serde_json::from_value(field("path")).unwrap_or(None),
        untitled: serde_json::from_value(field("untitled")).unwrap_or(None),
        format: serde_json::from_value(field("format")).unwrap_or_default(),
        language: field("language").as_str().and_then(LanguageId::from_key),
        language_chosen: field("language_chosen").as_bool().unwrap_or(false),
        saved_hash: field("saved_hash").as_u64().unwrap_or(0),
        stored: field("stored").as_bool().unwrap_or(false),
        cursor: serde_json::from_value(field("cursor")).unwrap_or_default(),
        preview: serde_json::from_value(field("preview")).unwrap_or_default(),
    })
}

/// A session file, read leniently (`None`: missing or not JSON at all).
fn read_session(path: &Path) -> Option<SessionFile> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("dexnotas: {} is not valid ({e})", path.display());
            return None;
        }
    };
    let field = |k: &str| value.get(k).cloned().unwrap_or(serde_json::Value::Null);
    let tabs = value.get("tabs")?.as_array()?.iter().filter_map(tab_from).collect();
    Some(SessionFile {
        tabs,
        active: field("active").as_u64().unwrap_or(0) as usize,
        next_untitled: field("next_untitled").as_u64().unwrap_or(1) as u32,
        recent: serde_json::from_value(field("recent")).unwrap_or_default(),
    })
}

pub struct Session {
    dir: PathBuf,
    /// The text of each stored tab as last written (`id`, version), to write only what changed.
    written: Vec<(u64, u64)>,
}

impl Session {
    pub fn new(dir: PathBuf) -> Session {
        Session { dir, written: Vec::new() }
    }

    fn session_dir(&self) -> PathBuf {
        self.dir.join("session")
    }

    fn content_path(&self, id: u64) -> PathBuf {
        self.session_dir().join(format!("{id}.txt"))
    }

    /// The documents of the last session, the active one, the next "new N" number and the recent
    /// files. Tabs whose file can no longer be read are left out (and named in the problems).
    pub fn restore(&mut self) -> (Vec<Document>, usize, u32, Vec<PathBuf>, Vec<String>) {
        let main = self.session_dir().join("session.json");
        let backup = self.session_dir().join("session.bak.json");
        let (file, readable) = match read_session(&main) {
            Some(file) => {
                // The last session that read well, should a later version not read the next one.
                let _ = std::fs::copy(&main, &backup);
                (file, true)
            }
            None => match read_session(&backup) {
                Some(file) => (file, true),
                None => (SessionFile::default(), !main.exists() && !backup.exists()),
            },
        };
        let mut docs = Vec::new();
        let mut problems = Vec::new();
        for entry in &file.tabs {
            let doc = if entry.stored {
                match std::fs::read_to_string(self.content_path(entry.id)) {
                    Ok(text) => {
                        let mut d = Document::from_text(entry.id, entry.untitled, &text);
                        drop(text);
                        d.path = entry.path.clone();
                        d.format = entry.format;
                        d.language = entry.language;
                        d.language_chosen = entry.language_chosen;
                        d.set_saved_hash(entry.saved_hash);
                        d.disk_time = d.path.as_ref().and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
                        self.written.push((d.id, d.buffer.version()));
                        Some(d)
                    }
                    Err(e) => {
                        problems.push(format!("{}: {e}", entry.path.as_ref().map_or_else(|| format!("#{}", entry.id), |p| p.display().to_string())));
                        None
                    }
                }
            } else if let Some(path) = &entry.path {
                match Document::open(entry.id, path) {
                    Ok(mut d) => {
                        if entry.language_chosen {
                            d.language = entry.language;
                            d.language_chosen = true;
                        }
                        Some(d)
                    }
                    Err(e) => {
                        problems.push(format!("{}: {e}", path.display()));
                        None
                    }
                }
            } else {
                None
            };
            if let Some(mut d) = doc {
                let len = d.len_chars();
                d.cursor = (entry.cursor.0.min(len), entry.cursor.1.min(len));
                d.preview.mode = entry.preview;
                docs.push(d);
            }
        }
        let mut next_untitled = file.next_untitled.max(1);
        if !readable {
            // No session could be read: the texts it kept come back as tabs rather than being
            // removed as leftovers by the next save.
            let mut ids: Vec<u64> = std::fs::read_dir(self.session_dir())
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".txt").and_then(|s| s.parse().ok()))
                .collect();
            ids.sort_unstable();
            for id in ids {
                if let Ok(text) = std::fs::read_to_string(self.content_path(id)) {
                    let mut d = Document::from_text(id, Some(next_untitled), &text);
                    next_untitled += 1;
                    d.mark_changed();
                    docs.push(d);
                }
            }
            if !docs.is_empty() {
                problems.push(crate::i18n::tf("session_recovered", docs.len()));
            }
        }
        let active = file.active.min(docs.len().saturating_sub(1));
        (docs, active, next_untitled, file.recent, problems)
    }

    /// Write the session: the tab list, and the text of every new or changed document that
    /// changed since the last write. Text files of closed tabs are removed.
    pub fn save(&mut self, docs: &[Document], active: usize, next_untitled: u32, recent: &[PathBuf]) -> Result<(), String> {
        let dir = self.session_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut tabs = Vec::with_capacity(docs.len());
        let mut keep = HashSet::new();
        for d in docs {
            let stored = d.dirty() || d.path.is_none();
            if stored {
                keep.insert(d.id);
                let current = (d.id, d.buffer.version());
                if !self.written.contains(&current) {
                    let rope = d.buffer.rope();
                    document::write_atomic_with(&self.content_path(d.id), |f| rope.write_to(f))?;
                    self.written.retain(|(id, _)| *id != d.id);
                    self.written.push(current);
                }
            }
            tabs.push(TabEntry {
                id: d.id,
                path: d.path.clone(),
                untitled: d.untitled,
                format: d.format,
                language: d.language,
                language_chosen: d.language_chosen,
                saved_hash: d.saved_hash(),
                stored,
                cursor: d.cursor,
                preview: d.preview.mode,
            });
        }
        let file = SessionFile { tabs, active, next_untitled, recent: recent.to_vec() };
        write_json(&dir.join("session.json"), &file)?;
        // Text of tabs that are closed, or saved since.
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some(id) = name.strip_suffix(".txt").and_then(|s| s.parse::<u64>().ok())
                    && !keep.contains(&id)
                {
                    let _ = std::fs::remove_file(e.path());
                    self.written.retain(|(i, _)| *i != id);
                }
            }
        }
        Ok(())
    }
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            log::warn!("dexnotas: {} is not valid ({e}); starting from defaults", path.display());
            None
        }
    }
}

/// A list in a JSON file, item by item: an item this version can't read is left out, not the
/// whole list (the saved macros, after an update).
pub fn read_json_list<T: for<'de> Deserialize<'de>>(path: &Path) -> Vec<T> {
    let Some(value) = read_json::<serde_json::Value>(path) else { return Vec::new() };
    let items = value.as_array().cloned().unwrap_or_default();
    let total = items.len();
    let list: Vec<T> = items.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect();
    if list.len() < total {
        log::warn!("dexnotas: {} of {total} items of {} could not be read", total - list.len(), path.display());
        // Keep the file as it was, in case a later version reads it.
        let _ = std::fs::copy(path, path.with_extension("json.old"));
    }
    list
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    document::write_atomic(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_come_back_with_their_unsaved_text() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("saved.md");
        std::fs::write(&file, "# saved\n").unwrap();

        let new_doc = Document::from_text(7, Some(1), "draft, never saved");
        let saved = Document::open(8, &file).unwrap();
        let mut edited = Document::open(9, &file).unwrap();
        let end = edited.len_chars();
        edited.replace(end..end, "more\n", (3, 5), crate::buffer::Group::Typing);

        let mut session = Session::new(dir.path().join("data"));
        session.save(&[new_doc, saved, edited], 2, 2, std::slice::from_ref(&file)).unwrap();

        let mut session = Session::new(dir.path().join("data"));
        let (docs, active, next, recent, problems) = session.restore();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(docs.len(), 3);
        assert_eq!((active, next), (2, 2));
        assert_eq!(recent, vec![file.clone()]);
        let mut docs = docs;
        assert_eq!(docs[0].text().as_str(), "draft, never saved");
        assert_eq!(docs[0].untitled, Some(1));
        assert!(!docs[1].dirty());
        assert_eq!(docs[1].text().as_str(), "# saved\n");
        assert_eq!(docs[2].text().as_str(), "# saved\nmore\n");
        assert!(docs[2].dirty(), "still unsaved");
        assert_eq!(docs[2].path.as_deref(), Some(file.as_path()));
        assert_eq!(docs[2].cursor, (3, 5));
    }

    #[test]
    fn closed_tabs_leave_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::new(dir.path().to_path_buf());
        let d = Document::from_text(1, Some(1), "x");
        session.save(std::slice::from_ref(&d), 0, 2, &[]).unwrap();
        assert!(dir.path().join("session/1.txt").exists());
        session.save(&[], 0, 2, &[]).unwrap();
        assert!(!dir.path().join("session/1.txt").exists());
    }

    #[test]
    fn a_session_from_another_version_still_restores() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let mut session = Session::new(data.clone());
        let a = Document::from_text(1, Some(1), "uno");
        let b = Document::from_text(2, Some(2), "dos");
        session.save(&[a, b], 1, 3, &[]).unwrap();
        // A newer version wrote a language, an encoding and a field this one doesn't know.
        let path = data.join("session/session.json");
        let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        v["tabs"][0]["language"] = "hljs:klingon".into();
        v["tabs"][1]["format"] = serde_json::json!({ "encoding": "Utf32", "bom": true });
        v["tabs"][1]["folds"] = serde_json::json!([1, 2]);
        v["theme"] = "solarized".into();
        std::fs::write(&path, v.to_string()).unwrap();
        let (mut docs, active, _, _, problems) = Session::new(data.clone()).restore();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(docs.len(), 2);
        assert_eq!(active, 1);
        assert_eq!(docs[0].language, None);
        assert_eq!(docs[1].text().as_str(), "dos");
        assert!(data.join("session/session.bak.json").exists());
    }

    #[test]
    fn an_unreadable_session_falls_back_then_recovers_the_texts() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let mut session = Session::new(data.clone());
        let a = Document::from_text(5, Some(1), "texto guardado");
        session.save(std::slice::from_ref(&a), 0, 2, &[]).unwrap();
        let _ = Session::new(data.clone()).restore(); // keeps session.bak.json
        std::fs::write(data.join("session/session.json"), "{ broken").unwrap();
        let (docs, ..) = Session::new(data.clone()).restore();
        assert_eq!(docs.len(), 1, "from the backup");
        std::fs::write(data.join("session/session.bak.json"), "also broken").unwrap();
        let (mut docs, _, _, _, problems) = Session::new(data.clone()).restore();
        assert_eq!(docs.len(), 1, "from the texts");
        assert_eq!(docs[0].text().as_str(), "texto guardado");
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn a_missing_file_is_reported_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("gone.txt");
        std::fs::write(&file, "x").unwrap();
        let d = Document::open(1, &file).unwrap();
        let mut session = Session::new(dir.path().join("data"));
        session.save(std::slice::from_ref(&d), 0, 1, &[]).unwrap();
        std::fs::remove_file(&file).unwrap();
        let (docs, _, _, _, problems) = Session::new(dir.path().join("data")).restore();
        assert!(docs.is_empty());
        assert_eq!(problems.len(), 1);
    }
}
