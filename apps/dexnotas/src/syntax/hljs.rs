//! Highlighting with highlight.js (https://highlightjs.org, BSD-3-Clause) for the languages the
//! tree-sitter grammars don't cover: its own code, bundled (`assets/highlightjs/`, built by
//! `tools/highlightjs/build.sh`), run by QuickJS, one engine per thread that highlights.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

use rquickjs::{Context, Function, Runtime};

use super::Kind;
pub use super::hljs_langs::{LANGS, Lang};

const BUNDLE: &str = include_str!("../../assets/highlightjs/dexhljs.js");

/// The language at `index` of [`LANGS`].
pub fn lang(index: u16) -> Option<&'static Lang> {
    LANGS.get(index as usize)
}

fn index_by<F: Fn(&Lang) -> &'static [&'static str]>(field: F) -> HashMap<&'static str, u16> {
    let mut map = HashMap::new();
    for (i, l) in LANGS.iter().enumerate() {
        for key in field(l) {
            map.entry(*key).or_insert(i as u16);
        }
    }
    map
}

/// The language of a file extension (lower case).
pub fn by_extension(ext: &str) -> Option<u16> {
    static MAP: OnceLock<HashMap<&'static str, u16>> = OnceLock::new();
    MAP.get_or_init(|| index_by(|l| l.exts)).get(ext).copied()
}

/// The language of a whole file name (lower case).
pub fn by_file_name(name: &str) -> Option<u16> {
    static MAP: OnceLock<HashMap<&'static str, u16>> = OnceLock::new();
    MAP.get_or_init(|| index_by(|l| l.file_names)).get(name).copied()
}

/// The language a Markdown code block names (an alias or an extension).
pub fn by_alias(alias: &str) -> Option<u16> {
    static MAP: OnceLock<HashMap<&'static str, u16>> = OnceLock::new();
    MAP.get_or_init(|| index_by(|l| l.aliases)).get(alias).copied().or_else(|| by_extension(alias))
}

/// The language whose highlight.js name is `id`.
pub fn by_id(id: &str) -> Option<u16> {
    LANGS.iter().position(|l| l.id == id).map(|i| i as u16)
}

/// The colour of a highlight.js scope (`title.function`, `meta keyword`…).
pub fn kind_of(scope: &str) -> Option<Kind> {
    match scope {
        "title.function" | "title.function.invoke" | "function" | "title" => Some(Kind::Function),
        "title.class" | "title.class.inherited" | "class" | "type" | "built_in" | "section" => Some(Kind::Type),
        "literal" | "variable.constant" | "symbol" => Some(Kind::Constant),
        "variable" | "variable.language" | "template-variable" | "property" | "deletion" => Some(Kind::Property),
        "meta.keyword" | "meta keyword" | "keyword" | "selector-tag" | "doctag" | "bullet" | "name" => Some(Kind::Keyword),
        "meta.string" | "meta string" | "string" | "regexp" | "char.escape" | "addition" | "link" | "code" => Some(Kind::String),
        "comment" | "quote" => Some(Kind::Comment),
        "number" => Some(Kind::Number),
        "attr" | "attribute" | "selector-attr" | "selector-class" | "selector-id" | "selector-pseudo" | "meta" => {
            Some(Kind::Attribute)
        }
        "tag" | "params" | "operator" | "punctuation" | "subst" | "emphasis" | "strong" | "formula" => None,
        _ => kind_of_base(scope),
    }
}

fn kind_of_base(scope: &str) -> Option<Kind> {
    let base = scope.split(['.', ' ']).next().unwrap_or(scope);
    if base == scope { None } else { kind_of(base) }
}

struct Engine {
    _runtime: Runtime,
    context: Context,
    /// The colour of each scope number the bundle hands back.
    kinds: Vec<Option<Kind>>,
}

impl Engine {
    fn new() -> Option<Engine> {
        let runtime = Runtime::new().ok()?;
        runtime.set_memory_limit(256 * 1024 * 1024);
        runtime.set_max_stack_size(1024 * 1024);
        let context = Context::full(&runtime).ok()?;
        let loaded = context.with(|ctx| ctx.eval::<(), _>(BUNDLE).is_ok());
        if !loaded {
            log::error!("dexnotas: highlight.js could not be loaded");
            return None;
        }
        Some(Engine { _runtime: runtime, context, kinds: Vec::new() })
    }

    /// The coloured pieces of `text` in `language`, in UTF-16 units: (start, end, scope number).
    fn run(&mut self, text: &str, language: &str) -> Vec<(usize, usize, usize)> {
        let (answer, scopes) = self.context.with(|ctx| {
            let Ok(f) = ctx.globals().get::<_, Function>("dexHighlight") else { return (String::new(), None) };
            let answer: String = f.call((text, language)).unwrap_or_default();
            let scopes: Option<String> = ctx.globals().get::<_, Function>("dexScopes").ok().and_then(|f| f.call(()).ok());
            (answer, scopes)
        });
        if let Some(scopes) = scopes {
            let names: Vec<&str> = scopes.split('\n').collect();
            if names.len() != self.kinds.len() {
                self.kinds = names.iter().map(|s| kind_of(s)).collect();
            }
        }
        answer
            .split(';')
            .filter_map(|piece| {
                let mut it = piece.split(',').map(|n| n.parse::<usize>().ok());
                Some((it.next()??, it.next()??, it.next()??))
            })
            .collect()
    }
}

thread_local! {
    /// `None`: not started yet; `Some(None)`: it could not start.
    static ENGINE: RefCell<Option<Option<Engine>>> = const { RefCell::new(None) };
}

/// The coloured pieces of `source` (byte ranges, in order) in highlight.js language `index`.
pub fn highlight(index: u16, source: &str) -> Vec<(Range<usize>, Kind)> {
    let Some(lang) = lang(index) else { return Vec::new() };
    let pieces = ENGINE.with(|cell| {
        let Ok(mut slot) = cell.try_borrow_mut() else { return Vec::new() };
        let engine = slot.get_or_insert_with(Engine::new);
        match engine.as_mut() {
            Some(engine) => {
                let pieces = engine.run(source, lang.id);
                pieces.into_iter().filter_map(|(s, e, k)| Some((s, e, engine.kinds.get(k).copied().flatten()?))).collect()
            }
            None => Vec::new(),
        }
    });
    utf16_to_bytes(source, pieces)
}

/// UTF-16 ranges (in order) to byte ranges.
fn utf16_to_bytes(source: &str, pieces: Vec<(usize, usize, Kind)>) -> Vec<(Range<usize>, Kind)> {
    let mut out = Vec::with_capacity(pieces.len());
    let mut chars = source.char_indices().peekable();
    let (mut unit, mut byte) = (0usize, 0usize);
    // Move to UTF-16 unit `target` (or the end), giving the byte offset there.
    let mut seek = |target: usize, unit: &mut usize, byte: &mut usize| {
        while *unit < target {
            match chars.next() {
                Some((_, c)) => {
                    *unit += c.len_utf16();
                    *byte += c.len_utf8();
                }
                None => break,
            }
        }
        let _ = chars.peek();
        *byte
    };
    for (start, end, kind) in pieces {
        if start < unit || end <= start {
            continue;
        }
        let s = seek(start, &mut unit, &mut byte);
        let e = seek(end, &mut unit, &mut byte);
        if e > s {
            out.push((s..e, kind));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kotlin_is_coloured() {
        let i = by_extension("kt").unwrap();
        let src = "// á comment\nfun main() { val s = \"hola\" }\n";
        let spans = highlight(i, src);
        let text = |k: Kind| spans.iter().filter(|(_, kk)| *kk == k).map(|(r, _)| &src[r.clone()]).collect::<Vec<_>>();
        assert_eq!(text(Kind::Comment), vec!["// á comment"]);
        assert!(text(Kind::Keyword).contains(&"fun"));
        assert!(text(Kind::String).contains(&"\"hola\""));
    }

    #[test]
    fn tables() {
        assert_eq!(by_extension("hs").and_then(lang).map(|l| l.id), Some("haskell"));
        assert_eq!(by_file_name("dockerfile").and_then(lang).map(|l| l.id), Some("dockerfile"));
        assert_eq!(by_alias("golo").and_then(lang).map(|l| l.id), Some("golo"));
        assert_eq!(kind_of("title.function.invoke"), Some(Kind::Function));
        assert_eq!(kind_of("meta.prompt"), Some(Kind::Attribute));
    }
}
