//! An open document: its text, where it is stored and how.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::syntax::LanguageId;
use crate::text::{self, Encoding, Format};

/// Files above this size are refused (the editor holds the whole text in memory and lays it out).
pub const MAX_OPEN_BYTES: u64 = 64 * 1024 * 1024;

pub struct Document {
    /// Stable for the session: tabs, saved contents and macros refer to it.
    pub id: u64,
    pub path: Option<PathBuf>,
    /// "new 1", "new 2"… for a document never saved.
    pub untitled: Option<u32>,
    pub text: String,
    pub format: Format,
    /// `None`: plain text.
    pub language: Option<LanguageId>,
    /// The language was chosen in the menu (a rename doesn't change it).
    pub language_chosen: bool,
    /// The hash of the text as last saved or read.
    saved_hash: u64,
    pub dirty: bool,
    /// Some bytes were not valid in the encoding when the file was read.
    pub lossy: bool,
    /// The file's modification time when it was read or saved.
    pub disk_time: Option<SystemTime>,
    /// Bumped on every change (the highlighting cache keys on it).
    pub version: u64,
    /// Where the cursor was (characters), to put it back.
    pub cursor: (usize, usize),
}

pub fn hash_text(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl Document {
    pub fn new(id: u64, untitled: u32) -> Document {
        Document {
            id,
            path: None,
            untitled: Some(untitled),
            text: String::new(),
            format: Format::default(),
            language: None,
            language_chosen: false,
            saved_hash: hash_text(""),
            dirty: false,
            lossy: false,
            disk_time: None,
            version: 0,
            cursor: (0, 0),
        }
    }

    /// Read `path`.
    pub fn open(id: u64, path: &Path) -> Result<Document, String> {
        let bytes = read(path)?;
        let decoded = text::decode(&bytes);
        let language = crate::syntax::detect(Some(path), &decoded.text);
        Ok(Document {
            id,
            path: Some(path.to_path_buf()),
            untitled: None,
            saved_hash: hash_text(&decoded.text),
            text: decoded.text,
            format: decoded.format,
            language,
            language_chosen: false,
            dirty: false,
            lossy: decoded.lossy,
            disk_time: modified(path),
            version: 0,
            cursor: (0, 0),
        })
    }

    pub fn title(&self) -> String {
        match (&self.path, self.untitled) {
            (Some(p), _) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            (None, Some(n)) => format!("{} {n}", crate::i18n::t("untitled")),
            (None, None) => crate::i18n::t("untitled").to_owned(),
        }
    }

    /// A name for Save As: the file's, or the first words of a new document.
    pub fn suggested_name(&self) -> String {
        if let Some(p) = &self.path {
            return p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        }
        let ext = self.language.map_or("txt", LanguageId::extension);
        let first: String = self
            .text
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
            .take(40)
            .collect();
        let first = first.trim();
        if first.is_empty() { format!("{}.{ext}", self.title()) } else { format!("{first}.{ext}") }
    }

    /// The text changed (in the editor or by a command).
    pub fn changed(&mut self) {
        self.version = self.version.wrapping_add(1);
        self.dirty = hash_text(&self.text) != self.saved_hash;
    }

    /// Replace the text (a command's result).
    pub fn set_text(&mut self, text: String) {
        if text != self.text {
            self.text = text;
            self.changed();
        }
    }

    /// Mark the current text as saved (used when restoring a session).
    pub fn set_saved_hash(&mut self, hash: u64) {
        self.saved_hash = hash;
        self.dirty = hash_text(&self.text) != hash;
    }

    pub fn saved_hash(&self) -> u64 {
        self.saved_hash
    }

    /// Write the text to `path` in the document's format, and make it the document's file.
    pub fn save_to(&mut self, path: &Path) -> Result<(), String> {
        let bytes = text::encode(&self.text, self.format);
        write_atomic(path, &bytes)?;
        if self.path.as_deref() != Some(path) {
            self.path = Some(path.to_path_buf());
            self.untitled = None;
            if !self.language_chosen {
                self.language = crate::syntax::detect(Some(path), &self.text).or(self.language);
            }
        }
        self.saved_hash = hash_text(&self.text);
        self.dirty = false;
        self.lossy = false;
        self.disk_time = modified(path);
        Ok(())
    }

    /// Read the file again as `encoding` (`None`: detect it), dropping the changes.
    pub fn reload(&mut self, encoding: Option<Encoding>) -> Result<(), String> {
        let Some(path) = self.path.clone() else { return Err(crate::i18n::t("not_saved_yet").to_owned()) };
        let bytes = read(&path)?;
        let decoded = match encoding {
            Some(e) => text::decode_as(&bytes, e, false),
            None => text::decode(&bytes),
        };
        self.text = decoded.text;
        self.format = decoded.format;
        self.lossy = decoded.lossy;
        self.saved_hash = hash_text(&self.text);
        self.dirty = false;
        self.disk_time = modified(&path);
        self.version = self.version.wrapping_add(1);
        Ok(())
    }

    /// The file was changed by something else since it was read or saved.
    pub fn changed_on_disk(&self) -> bool {
        match (&self.path, self.disk_time) {
            (Some(p), Some(t)) => modified(p).is_some_and(|now| now != t),
            _ => false,
        }
    }

    /// The file no longer exists.
    pub fn missing_on_disk(&self) -> bool {
        self.path.as_ref().is_some_and(|p| !p.exists())
    }

    /// The document is an empty new one (closing it loses nothing).
    pub fn is_blank(&self) -> bool {
        self.path.is_none() && self.text.is_empty()
    }
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_OPEN_BYTES {
        return Err(crate::i18n::tf("too_large", size / (1024 * 1024)));
    }
    std::fs::read(path).map_err(|e| e.to_string())
}

/// Write `bytes` to `path` through a hidden file in the same folder renamed into place, so a
/// failure never leaves half a file (and Android's document mirror is written back once).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.dexnotas-{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_edit_save_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        std::fs::write(&path, b"\xFF\xFE{\x00\r\x00\n\x00}\x00").unwrap();
        let mut d = Document::open(1, &path).unwrap();
        assert_eq!(d.text, "{\n}");
        assert_eq!(d.format.encoding, Encoding::Utf16Le);
        assert_eq!(d.language, Some(LanguageId::Json));
        assert!(!d.dirty);
        d.text.push('\n');
        d.changed();
        assert!(d.dirty);
        d.save_to(&path).unwrap();
        assert!(!d.dirty);
        assert_eq!(std::fs::read(&path).unwrap(), b"\xFF\xFE{\x00\r\x00\n\x00}\x00\r\x00\n\x00");
        d.text.pop();
        d.changed();
        d.text.push('\n');
        d.changed();
        assert!(!d.dirty, "back to the saved text");
    }

    #[test]
    fn suggested_names() {
        let mut d = Document::new(1, 3);
        d.text = "\n  Shopping list: eggs\nmilk".into();
        assert_eq!(d.suggested_name(), "Shopping list eggs.txt");
        d.language = Some(LanguageId::Python);
        d.text.clear();
        assert!(d.suggested_name().ends_with(" 3.py"));
    }
}
