//! An open document: its text, where it is stored and how.

use std::hash::{Hash, Hasher};
use std::io::Write as _;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use ropey::Rope;

use crate::buffer::{Buffer, Group};
use crate::syntax::LanguageId;
use crate::text::{self, Encoding, Format};

pub struct Document {
    /// Stable for the session: tabs, saved contents and macros refer to it.
    pub id: u64,
    pub path: Option<PathBuf>,
    /// "new 1", "new 2"… for a document never saved.
    pub untitled: Option<u32>,
    pub buffer: Buffer,
    pub format: Format,
    /// `None`: plain text.
    pub language: Option<LanguageId>,
    /// The language was chosen in the menu (a rename doesn't change it).
    pub language_chosen: bool,
    /// The buffer state of the text as last saved or read (`u64::MAX`: none, it is changed).
    saved_state: u64,
    /// The hash of the text as last saved or read (kept with the session).
    saved_hash: u64,
    /// Some bytes were not valid in the encoding when the file was read.
    pub lossy: bool,
    /// The file's modification time when it was read or saved.
    pub disk_time: Option<SystemTime>,
    /// The selection (anchor, cursor), in characters.
    pub cursor: (usize, usize),
    /// The editor's scroll position and state (`ui::code_editor`).
    pub view: crate::ui::code_editor::View,
    /// Syntax colours (`highlight`).
    pub highlight: crate::highlight::Colours,
    /// The Markdown or HTML preview (`preview`).
    pub preview: crate::preview::State,
    /// The whole text as one string, for searches (rebuilt when the text changed).
    flat: Option<(u64, Arc<String>)>,
}

pub fn hash_text(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

pub fn hash_rope(rope: &Rope) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for chunk in rope.chunks() {
        h.write(chunk.as_bytes());
    }
    // As `hash_text` does after the bytes.
    h.write_u8(0xff);
    h.finish()
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl Document {
    fn with(id: u64, path: Option<PathBuf>, untitled: Option<u32>, text: String, format: Format) -> Document {
        let buffer = Buffer::from_rope(Rope::from(text));
        let saved_hash = hash_rope(buffer.rope());
        Document {
            id,
            path,
            untitled,
            saved_state: buffer.state(),
            buffer,
            format,
            language: None,
            language_chosen: false,
            saved_hash,
            lossy: false,
            disk_time: None,
            cursor: (0, 0),
            view: Default::default(),
            highlight: Default::default(),
            preview: Default::default(),
            flat: None,
        }
    }

    pub fn new(id: u64, untitled: u32) -> Document {
        Document::with(id, None, Some(untitled), String::new(), Format::default())
    }

    /// A document with `text` (tests, restored sessions).
    pub fn from_text(id: u64, untitled: Option<u32>, text: &str) -> Document {
        Document::with(id, None, untitled, text.to_owned(), Format::default())
    }

    /// Read `path`.
    pub fn open(id: u64, path: &Path) -> Result<Document, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let decoded = text::decode(&bytes);
        drop(bytes);
        let language = crate::syntax::detect(Some(path), first_line(&decoded.text));
        let mut d = Document::with(id, Some(path.to_path_buf()), None, decoded.text, decoded.format);
        d.language = language;
        d.lossy = decoded.lossy;
        d.disk_time = modified(path);
        Ok(d)
    }

    pub fn dirty(&self) -> bool {
        self.buffer.state() != self.saved_state
    }

    pub fn saved_hash(&self) -> u64 {
        self.saved_hash
    }

    /// The text was restored from a session: unchanged if it is the text the file had.
    pub fn set_saved_hash(&mut self, hash: u64) {
        self.saved_hash = hash;
        self.saved_state = if hash_rope(self.buffer.rope()) == hash { self.buffer.state() } else { u64::MAX };
    }

    /// The document no longer matches its file (a change of encoding or line ending).
    pub fn mark_changed(&mut self) {
        self.saved_state = u64::MAX;
    }

    pub fn len_chars(&self) -> usize {
        self.buffer.len_chars()
    }

    /// The whole text (shared; rebuilt only after a change).
    pub fn text(&mut self) -> Arc<String> {
        let version = self.buffer.version();
        if let Some((v, s)) = &self.flat
            && *v == version
        {
            return s.clone();
        }
        let s = Arc::new(self.buffer.rope().to_string());
        self.flat = Some((version, s.clone()));
        s
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
        let rope = self.buffer.rope();
        let mut first = String::new();
        for line in rope.lines().take(50) {
            let l: String = line.chars().take(200).collect();
            if !l.trim().is_empty() {
                first = l;
                break;
            }
        }
        let first: String = first.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').take(40).collect();
        let first = first.trim();
        if first.is_empty() { format!("{}.{ext}", self.title()) } else { format!("{first}.{ext}") }
    }

    /// Replace `range` (characters) with `text`, selecting `after`.
    pub fn replace(&mut self, range: Range<usize>, text: &str, after: (usize, usize), group: Group) {
        let before = self.cursor;
        self.buffer.replace(range, text, before, after, group);
        self.cursor = after;
    }

    /// Replace the whole text (a command's result), as one undo step.
    pub fn set_text(&mut self, text: &str, after: (usize, usize)) {
        let len = self.buffer.len_chars();
        if self.text().as_str() == text {
            self.cursor = after;
            return;
        }
        self.replace(0..len, text, after, Group::Alone);
    }

    pub fn undo(&mut self) -> bool {
        match self.buffer.undo() {
            Some(sel) => {
                self.cursor = sel;
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.buffer.redo() {
            Some(sel) => {
                self.cursor = sel;
                true
            }
            None => false,
        }
    }

    /// Write the text to `path` in the document's format, and make it the document's file.
    pub fn save_to(&mut self, path: &Path) -> Result<(), String> {
        let bytes = text::encode_rope(self.buffer.rope(), self.format);
        write_atomic(path, &bytes)?;
        if self.path.as_deref() != Some(path) {
            self.path = Some(path.to_path_buf());
            self.untitled = None;
            if !self.language_chosen {
                let text = self.buffer.slice_string(0..self.buffer.len_chars().min(400));
                self.language = crate::syntax::detect(Some(path), &text).or(self.language);
            }
        }
        self.saved_state = self.buffer.state();
        self.buffer.seal();
        self.saved_hash = hash_rope(self.buffer.rope());
        self.lossy = false;
        self.disk_time = modified(path);
        Ok(())
    }

    /// Read the file again as `encoding` (`None`: detect it), dropping the changes.
    pub fn reload(&mut self, encoding: Option<Encoding>) -> Result<(), String> {
        let Some(path) = self.path.clone() else { return Err(crate::i18n::t("not_saved_yet").to_owned()) };
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let decoded = match encoding {
            Some(e) => text::decode_as(&bytes, e, false),
            None => text::decode(&bytes),
        };
        drop(bytes);
        self.buffer.reset(Rope::from(decoded.text));
        self.format = decoded.format;
        self.lossy = decoded.lossy;
        self.saved_state = self.buffer.state();
        self.saved_hash = hash_rope(self.buffer.rope());
        self.disk_time = modified(&path);
        let len = self.buffer.len_chars();
        self.cursor = (self.cursor.0.min(len), self.cursor.1.min(len));
        Ok(())
    }

    /// The file was changed by something else since it was read or saved.
    pub fn changed_on_disk(&self) -> bool {
        match (&self.path, self.disk_time) {
            (Some(p), Some(t)) => modified(p).is_some_and(|now| now != t),
            _ => false,
        }
    }

    pub fn missing_on_disk(&self) -> bool {
        self.path.as_ref().is_some_and(|p| !p.exists())
    }

    /// An empty new document (closing it loses nothing).
    pub fn is_blank(&self) -> bool {
        self.path.is_none() && self.buffer.len_chars() == 0
    }
}

fn first_line(text: &str) -> &str {
    let end = text.find('\n').unwrap_or(text.len()).min(400);
    let mut end = end.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Write `bytes` to `path` through a hidden file in the same folder renamed into place, so a
/// failure never leaves half a file (and Android's document mirror is written back once).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_atomic_with(path, |f| f.write_all(bytes))
}

/// [`write_atomic`] with the contents written by `write`.
pub fn write_atomic_with(path: &Path, write: impl FnOnce(&mut std::io::BufWriter<std::fs::File>) -> std::io::Result<()>) -> Result<(), String> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.dexnotas-{}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        write(&mut f)?;
        f.flush()?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.to_string());
    }
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
        assert_eq!(d.text().as_str(), "{\n}");
        assert_eq!(d.format.encoding, Encoding::Utf16Le);
        assert_eq!(d.language, Some(LanguageId::Json));
        assert!(!d.dirty());
        d.replace(3..3, "\n", (4, 4), Group::Typing);
        assert!(d.dirty());
        d.save_to(&path).unwrap();
        assert!(!d.dirty());
        assert_eq!(std::fs::read(&path).unwrap(), b"\xFF\xFE{\x00\r\x00\n\x00}\x00\r\x00\n\x00");
        d.replace(4..4, "x", (5, 5), Group::Typing);
        assert!(d.dirty());
        d.undo();
        assert!(!d.dirty(), "back to the saved text");
    }

    #[test]
    fn suggested_names() {
        let mut d = Document::from_text(1, Some(3), "\n  Shopping list: eggs\nmilk");
        assert_eq!(d.suggested_name(), "Shopping list eggs.txt");
        d.language = Some(LanguageId::Python);
        let len = d.len_chars();
        d.replace(0..len, "", (0, 0), Group::Alone);
        assert!(d.suggested_name().ends_with(" 3.py"));
    }

    #[test]
    fn hashes_agree() {
        let text = "uno\ndos ñ\n".repeat(5000);
        assert_eq!(hash_rope(&Rope::from_str(&text)), hash_text(&text));
    }
}
