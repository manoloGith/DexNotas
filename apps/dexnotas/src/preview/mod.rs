//! The preview of Markdown and HTML documents: the text is parsed (on a thread of its own) into
//! blocks of styled text that `ui::preview` draws next to the editor or instead of it.
//!
//! HTML is shown the way a reader would see its content (headings, paragraphs, lists, tables,
//! links, images, code), without CSS or scripts.

pub mod html;
pub mod markdown;
pub mod web;

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use crate::syntax::{Kind, LanguageId};

/// Where the preview is, for one document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Mode {
    /// Only the editor.
    #[default]
    Off,
    /// The editor and the preview side by side (one above the other in a narrow window).
    Split,
    /// Only the preview.
    Only,
}

/// What the text is, for the preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Markdown,
    Html,
}

impl Source {
    pub fn of(language: Option<LanguageId>) -> Option<Source> {
        match language? {
            LanguageId::Markdown => Some(Source::Markdown),
            LanguageId::Html => Some(Source::Html),
            _ => None,
        }
    }
}

/// How a piece of text looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub underline: bool,
    /// Superscript or subscript (drawn smaller).
    pub small: bool,
    /// Highlighted (`<mark>`).
    pub mark: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text { text: String, style: Style, link: Option<u32> },
    /// An image: its address as written, and its description.
    Image { src: String, alt: String, link: Option<u32> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// A task list item: done or not.
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading { level: u8, inlines: Vec<Inline> },
    Paragraph(Vec<Inline>),
    Code { language: Option<LanguageId>, text: String, spans: Vec<(Range<usize>, Kind)> },
    Quote(Vec<Block>),
    /// `start`: the first number of a numbered list.
    List { start: Option<u64>, items: Vec<Item> },
    Rule,
    Table { aligns: Vec<Align>, head: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>> },
}

/// A parsed document.
#[derive(Debug, Default)]
pub struct Parsed {
    pub blocks: Vec<Block>,
    /// The line (0-based) each block starts at in the text.
    pub lines: Vec<usize>,
    /// The targets of links (`Inline::Text::link` is an index here).
    pub links: Vec<String>,
    /// The folder relative images are read from.
    pub base: Option<PathBuf>,
    /// The Markdown as HTML, for the web preview (when asked for).
    pub web: Option<String>,
}

impl Parsed {
    pub fn parse(source: Source, text: &str, base: Option<&Path>, web: bool) -> Parsed {
        let mut parsed = match source {
            Source::Markdown => markdown::parse(text),
            Source::Html => html::parse(text),
        };
        if web && source == Source::Markdown {
            parsed.web = Some(web::markdown_body(text));
        }
        parsed.base = base.map(Path::to_path_buf);
        for block in &mut parsed.blocks {
            colour_code(block);
        }
        parsed
    }

    /// Where image `src` is read from (`None`: not a file, as an address on the web).
    pub fn image_uri(&self, src: &str) -> Option<String> {
        let src = src.trim();
        if src.is_empty() || src.starts_with("data:") {
            return None;
        }
        if let Some(rest) = src.strip_prefix("file://") {
            return Some(format!("file://{rest}"));
        }
        if src.contains("://") {
            return None;
        }
        let path = Path::new(src);
        let path = if path.is_absolute() { path.to_path_buf() } else { self.base.as_ref()?.join(percent_decode(src)) };
        Some(format!("file://{}", path.display()))
    }
}

/// Code blocks with a language get the editor's colours (not huge ones).
fn colour_code(block: &mut Block) {
    match block {
        Block::Code { language: Some(language), text, spans } if text.len() <= 256 * 1024 => {
            *spans = crate::syntax::highlight(*language, text);
        }
        Block::Quote(blocks) => blocks.iter_mut().for_each(colour_code),
        Block::List { items, .. } => items.iter_mut().flat_map(|i| i.blocks.iter_mut()).for_each(colour_code),
        _ => {}
    }
}

/// The language of a code block's info string (```rust, ```py, language-js…).
pub fn code_language(info: &str) -> Option<LanguageId> {
    let word = info.split([' ', ',', '{', '\t']).next().unwrap_or("").trim().to_ascii_lowercase();
    let word = word.strip_prefix("language-").or_else(|| word.strip_prefix("lang-")).unwrap_or(&word);
    let ext = match word {
        "" => return None,
        "rust" => "rs",
        "python" | "python3" => "py",
        "javascript" | "node" | "jsx" => "js",
        "typescript" | "tsx" => "ts",
        "shell" | "bash" | "zsh" | "console" | "shellsession" => "sh",
        "csharp" | "c#" => "cs",
        "c++" => "cpp",
        "ruby" => "rb",
        "golang" => "go",
        "powershell" | "pwsh" => "ps1",
        "markdown" => "md",
        "yml" => "yaml",
        "postgres" | "mysql" | "sqlite" | "plsql" => "sql",
        "svg" | "xsl" => "xml",
        other => other,
    };
    crate::syntax::language_for_path(Path::new(&format!("x.{ext}"))).or_else(|| crate::syntax::hljs::by_alias(word).map(LanguageId::Hljs))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(h) = bytes.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(std::str::from_utf8(h).unwrap_or("zz"), 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

/// Counts lines up to growing byte offsets (offsets must not go back).
pub(crate) struct LineCounter<'a> {
    text: &'a [u8],
    at: usize,
    line: usize,
}

impl<'a> LineCounter<'a> {
    pub fn new(text: &'a str) -> Self {
        LineCounter { text: text.as_bytes(), at: 0, line: 0 }
    }

    pub fn line_at(&mut self, offset: usize) -> usize {
        let offset = offset.min(self.text.len());
        if offset > self.at {
            self.line += self.text[self.at..offset].iter().filter(|b| **b == b'\n').count();
            self.at = offset;
        }
        self.line
    }
}

/// A document's preview: its mode and what was last parsed.
#[derive(Default)]
pub struct State {
    pub mode: Mode,
    pub parsed: Option<Arc<Parsed>>,
    /// The buffer version `parsed` is of.
    pub version: u64,
    /// The version being parsed.
    pending: Option<u64>,
    /// When the text was first seen different from what was parsed.
    changed_at: Option<Instant>,
    /// The drawn height of each block (an estimate for those not drawn yet), and the width they
    /// were drawn at.
    pub heights: Vec<f32>,
    pub width: f32,
    /// The editor's first line the preview last followed.
    pub followed_line: Option<usize>,
}

impl State {
    pub fn is_open(&self) -> bool {
        self.mode != Mode::Off
    }
}

struct Job {
    doc: u64,
    version: u64,
    source: Source,
    text: Arc<String>,
    base: Option<PathBuf>,
    web: bool,
}

/// Parses previews on a thread of its own.
pub struct Worker {
    jobs: Sender<Job>,
    results: Receiver<(u64, u64, Arc<Parsed>)>,
}

/// Texts larger than this wait for a pause in typing before they are parsed again.
const QUICK: usize = 256 * 1024;
const PAUSE: Duration = Duration::from_millis(350);

impl Worker {
    pub fn new(ctx: egui::Context) -> Worker {
        let (jobs, job_rx) = channel::<Job>();
        let (result_tx, results) = channel();
        let spawned = std::thread::Builder::new().name("dexnotas-preview".into()).spawn(move || {
            while let Ok(mut job) = job_rx.recv() {
                // Only the newest job matters.
                while let Ok(next) = job_rx.try_recv() {
                    job = next;
                }
                let parsed = Parsed::parse(job.source, &job.text, job.base.as_deref(), job.web);
                if result_tx.send((job.doc, job.version, Arc::new(parsed))).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        if let Err(e) = spawned {
            log::error!("dexnotas: the preview thread could not start: {e}");
        }
        Worker { jobs, results }
    }

    /// Take finished previews.
    pub fn receive(&self, docs: &mut [crate::document::Document]) {
        while let Ok((id, version, parsed)) = self.results.try_recv() {
            if let Some(doc) = docs.iter_mut().find(|d| d.id == id) {
                let state = &mut doc.preview;
                if state.pending == Some(version) {
                    state.pending = None;
                }
                if state.heights.len() != parsed.blocks.len() {
                    state.heights.clear();
                }
                state.parsed = Some(parsed);
                state.version = version;
            }
        }
    }

    /// Ask for the preview of `doc` if its text changed (after a pause for long texts).
    pub fn update(&self, doc: &mut crate::document::Document, ctx: &egui::Context, web: bool) {
        let Some(source) = Source::of(doc.language) else { return };
        let version = doc.buffer.version();
        let state = &doc.preview;
        let have_web = state.parsed.as_ref().is_some_and(|p| p.web.is_some() || source != Source::Markdown);
        if state.parsed.is_some() && state.version == version && (have_web || !web) || state.pending == Some(version) {
            return;
        }
        let now = Instant::now();
        let changed_at = *doc.preview.changed_at.get_or_insert(now);
        if doc.preview.parsed.is_some() && doc.buffer.len_bytes() > QUICK && now.duration_since(changed_at) < PAUSE {
            ctx.request_repaint_after(PAUSE);
            return;
        }
        doc.preview.changed_at = None;
        doc.preview.pending = Some(version);
        let base = doc.path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf);
        let job = Job { doc: doc.id, version, source, text: doc.text(), base, web };
        if self.jobs.send(job).is_err() {
            doc.preview.pending = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_languages() {
        assert_eq!(code_language("rust"), Some(LanguageId::Rust));
        assert_eq!(code_language("py title=x"), Some(LanguageId::Python));
        assert_eq!(code_language("language-js"), Some(LanguageId::JavaScript));
        assert_eq!(code_language(""), None);
    }

    #[test]
    fn image_addresses() {
        let p = Parsed { base: Some(PathBuf::from("/docs")), ..Default::default() };
        assert_eq!(p.image_uri("img/a%20b.png").as_deref(), Some("file:///docs/img/a b.png"));
        assert_eq!(p.image_uri("https://x.org/a.png"), None);
        assert_eq!(p.image_uri("/abs.png").as_deref(), Some("file:///abs.png"));
    }
}
