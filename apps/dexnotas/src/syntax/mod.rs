//! Syntax highlighting with tree-sitter.
//!
//! The languages, their detection and the grammar setup come from Notepad Studio
//! (https://github.com/alesloa/notepad-studio, `src/syntax`, Apache-2.0, copyright the Notepad
//! Studio contributors). The highlight loop and the colours are DexNotas'.

mod grammars;

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

/// A guard against a request far larger than the highlighting thread is ever given
/// (`highlight::WHOLE_LIMIT`, or a window around the screen).
pub const MAX_HIGHLIGHT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageId {
    Rust,
    Python,
    C,
    CSharp,
    JavaScript,
    TypeScript,
    Tsx,
    Json,
    Markdown,
    Bash,
    PowerShell,
    Go,
    Java,
    Swift,
    Ruby,
    Php,
    Cpp,
    Lua,
    Html,
    Css,
    Scss,
    Yaml,
    Toml,
    Xml,
    Sql,
}

impl LanguageId {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::C => "C",
            Self::CSharp => "C#",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Tsx => "TSX",
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
            Self::Bash => "Shell",
            Self::PowerShell => "PowerShell",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::Swift => "Swift",
            Self::Ruby => "Ruby",
            Self::Php => "PHP",
            Self::Cpp => "C++",
            Self::Lua => "Lua",
            Self::Html => "HTML",
            Self::Css => "CSS",
            Self::Scss => "SCSS",
            Self::Yaml => "YAML",
            Self::Toml => "TOML",
            Self::Xml => "XML",
            Self::Sql => "SQL",
        }
    }

    /// Every language, in the menu's (alphabetical) order.
    pub const ALL: [LanguageId; 25] = [
        Self::Bash,
        Self::C,
        Self::CSharp,
        Self::Cpp,
        Self::Css,
        Self::Go,
        Self::Html,
        Self::Java,
        Self::JavaScript,
        Self::Json,
        Self::Lua,
        Self::Markdown,
        Self::Php,
        Self::PowerShell,
        Self::Python,
        Self::Ruby,
        Self::Rust,
        Self::Scss,
        Self::Sql,
        Self::Swift,
        Self::Toml,
        Self::Tsx,
        Self::TypeScript,
        Self::Xml,
        Self::Yaml,
    ];

    /// The usual extension, for a new file's suggested name.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Rust => "rs",
            Self::Python => "py",
            Self::C => "c",
            Self::CSharp => "cs",
            Self::JavaScript => "js",
            Self::TypeScript => "ts",
            Self::Tsx => "tsx",
            Self::Json => "json",
            Self::Markdown => "md",
            Self::Bash => "sh",
            Self::PowerShell => "ps1",
            Self::Go => "go",
            Self::Java => "java",
            Self::Swift => "swift",
            Self::Ruby => "rb",
            Self::Php => "php",
            Self::Cpp => "cpp",
            Self::Lua => "lua",
            Self::Html => "html",
            Self::Css => "css",
            Self::Scss => "scss",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Xml => "xml",
            Self::Sql => "sql",
        }
    }

    /// The line comment, for Toggle comment (`None`: the language has only block comments).
    pub fn line_comment(self) -> Option<&'static str> {
        match self {
            Self::Rust | Self::C | Self::CSharp | Self::JavaScript | Self::TypeScript | Self::Tsx | Self::Go
            | Self::Java | Self::Swift | Self::Php | Self::Cpp | Self::Scss | Self::Json => Some("//"),
            Self::Python | Self::Bash | Self::PowerShell | Self::Ruby | Self::Yaml | Self::Toml => Some("#"),
            Self::Lua | Self::Sql => Some("--"),
            Self::Markdown | Self::Html | Self::Css | Self::Xml => None,
        }
    }
}

/// The language of a file, from its name, else from its first line (`#!`).
pub fn detect(path: Option<&Path>, text: &str) -> Option<LanguageId> {
    path.and_then(language_for_path).or_else(|| language_for_shebang(text))
}

pub fn language_for_path(path: &Path) -> Option<LanguageId> {
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
        let lower = name.to_ascii_lowercase();
        if lower == "rakefile" || lower == "gemfile" {
            return Some(LanguageId::Ruby);
        }
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => LanguageId::Rust,
        "py" | "pyi" | "pyw" => LanguageId::Python,
        "c" => LanguageId::C,
        "cs" => LanguageId::CSharp,
        "js" | "mjs" | "cjs" | "jsx" => LanguageId::JavaScript,
        "ts" | "mts" | "cts" => LanguageId::TypeScript,
        "tsx" => LanguageId::Tsx,
        "json" | "jsonc" | "json5" | "jsonl" | "ndjson" | "geojson" | "webmanifest" | "har" => LanguageId::Json,
        "md" | "markdown" | "mdx" | "mkd" | "mdown" => LanguageId::Markdown,
        "sh" | "bash" | "zsh" | "ksh" => LanguageId::Bash,
        "ps1" | "psm1" | "psd1" => LanguageId::PowerShell,
        "go" => LanguageId::Go,
        "java" => LanguageId::Java,
        "swift" => LanguageId::Swift,
        "rb" | "rake" | "gemspec" => LanguageId::Ruby,
        "php" | "phtml" => LanguageId::Php,
        "cpp" | "cxx" | "cc" | "h" | "hpp" | "hxx" | "hh" | "ino" => LanguageId::Cpp,
        "lua" => LanguageId::Lua,
        "html" | "htm" | "shtml" | "xht" | "vue" | "svelte" => LanguageId::Html,
        "css" | "less" => LanguageId::Css,
        "scss" => LanguageId::Scss,
        "yaml" | "yml" => LanguageId::Yaml,
        "toml" | "ini" => LanguageId::Toml,
        "xml" | "svg" | "xhtml" | "xsl" | "xslt" | "xsd" | "wsdl" | "rss" | "atom" | "plist" | "csproj" | "vbproj" | "fsproj" | "props"
        | "targets" | "xaml" | "resx" | "kml" | "gpx" | "fxml" | "nuspec" | "config" => LanguageId::Xml,
        "sql" => LanguageId::Sql,
        _ => return None,
    })
}

/// From the first line (`#!/usr/bin/env python3`).
pub fn language_for_shebang(content: &str) -> Option<LanguageId> {
    let first_line = content.lines().next()?;
    if !first_line.starts_with("#!") {
        return None;
    }
    let line = first_line.to_ascii_lowercase();
    if line.contains("bash") || line.contains("/sh") || line.contains("zsh") || line.contains("ksh") {
        Some(LanguageId::Bash)
    } else if line.contains("python") {
        Some(LanguageId::Python)
    } else if line.contains("node") {
        Some(LanguageId::JavaScript)
    } else if line.contains("ruby") {
        Some(LanguageId::Ruby)
    } else if line.contains("php") {
        Some(LanguageId::Php)
    } else if line.contains("pwsh") || line.contains("powershell") {
        Some(LanguageId::PowerShell)
    } else if line.contains("lua") {
        Some(LanguageId::Lua)
    } else {
        None
    }
}

/// What a piece of code is, for its colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Keyword,
    String,
    Comment,
    Number,
    Type,
    Function,
    Constant,
    Property,
    Attribute,
}

/// tree-sitter's highlight names; the highlighter reports positions in this list.
const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "function",
    "function.builtin",
    "function.macro",
    "function.method",
    "keyword",
    "label",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "text.title",
    "text.literal",
    "text.uri",
    "text.reference",
];

fn kind_of(index: usize) -> Option<Kind> {
    Some(match *HIGHLIGHT_NAMES.get(index)? {
        "attribute" => Kind::Attribute,
        "comment" => Kind::Comment,
        "constant" | "constant.builtin" => Kind::Constant,
        "constructor" | "function" | "function.builtin" | "function.macro" | "function.method" | "label" => {
            Kind::Function
        }
        "keyword" | "tag" => Kind::Keyword,
        "number" => Kind::Number,
        "property" | "text.reference" => Kind::Property,
        "string" | "string.special" | "text.literal" | "text.uri" => Kind::String,
        "type" | "type.builtin" | "text.title" => Kind::Type,
        _ => return None,
    })
}

thread_local! {
    static CONFIGS: RefCell<HashMap<LanguageId, Option<HighlightConfiguration>>> = RefCell::new(HashMap::new());
    static HIGHLIGHTER: RefCell<Highlighter> = RefCell::new(Highlighter::new());
}

/// The coloured pieces of `source` (byte ranges, in order, not overlapping). Empty when the
/// document is too large or the grammar fails.
pub fn highlight(language: LanguageId, source: &str) -> Vec<(Range<usize>, Kind)> {
    if source.len() > MAX_HIGHLIGHT_BYTES {
        return Vec::new();
    }
    HIGHLIGHTER.with(|highlighter| {
        CONFIGS.with(|configs| {
            let (Ok(mut highlighter), Ok(mut configs)) = (highlighter.try_borrow_mut(), configs.try_borrow_mut()) else {
                return Vec::new();
            };
            let config = configs.entry(language).or_insert_with(|| {
                let mut config = grammars::build_config(language)?;
                config.configure(HIGHLIGHT_NAMES);
                Some(config)
            });
            let Some(config) = config.as_ref() else { return Vec::new() };
            let Ok(events) = highlighter.highlight(config, source.as_bytes(), None, |_| None) else {
                return Vec::new();
            };
            let mut spans = Vec::new();
            let mut stack: Vec<Option<Kind>> = Vec::new();
            for event in events {
                match event {
                    Ok(HighlightEvent::HighlightStart(h)) => stack.push(kind_of(h.0)),
                    Ok(HighlightEvent::HighlightEnd) => {
                        stack.pop();
                    }
                    Ok(HighlightEvent::Source { start, end }) => {
                        // The innermost highlight that has a colour.
                        if let Some(kind) = stack.iter().rev().find_map(|k| *k)
                            && start < end
                        {
                            spans.push((start..end, kind));
                        }
                    }
                    Err(_) => return Vec::new(),
                }
            }
            spans
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(language: LanguageId, source: &str, token: &str, kind: Kind) -> bool {
        highlight(language, source)
            .iter()
            .any(|(r, k)| *k == kind && source.get(r.clone()).is_some_and(|t| t.contains(token)))
    }

    #[test]
    fn every_grammar_builds_and_colours_something() {
        let samples: &[(LanguageId, &str)] = &[
            (LanguageId::Rust, "fn main() { let x = 1; }"),
            (LanguageId::Python, "def f():\n    return 'x'"),
            (LanguageId::C, "int main() { return 0; }"),
            (LanguageId::CSharp, "class A { int x = 1; }"),
            (LanguageId::JavaScript, "const a = 'x';"),
            (LanguageId::TypeScript, "let a: number = 1;"),
            (LanguageId::Tsx, "const a = <div title=\"x\"/>;"),
            (LanguageId::Json, "{\"a\": 1}"),
            (LanguageId::Markdown, "# Title\n"),
            (LanguageId::Bash, "echo \"hi\" # c"),
            (LanguageId::PowerShell, "$a = 'x' # c"),
            (LanguageId::Go, "package main\nfunc main() {}"),
            (LanguageId::Java, "class A { int x = 1; }"),
            (LanguageId::Swift, "let a = \"x\""),
            (LanguageId::Ruby, "def f; 'x'; end"),
            (LanguageId::Php, "<?php echo 'x'; ?>"),
            (LanguageId::Cpp, "int main() { return 0; }"),
            (LanguageId::Lua, "local a = 'x'"),
            (LanguageId::Html, "<p class=\"x\">hi</p>"),
            (LanguageId::Css, "a { color: red; }"),
            (LanguageId::Scss, "a { color: red; } // c"),
            (LanguageId::Yaml, "a: 'x' # c"),
            (LanguageId::Toml, "a = \"x\" # c"),
            (LanguageId::Xml, "<a b=\"c\">d</a>"),
            (LanguageId::Sql, "SELECT 1 FROM t; -- c"),
        ];
        assert_eq!(samples.len(), LanguageId::ALL.len());
        for (language, source) in samples {
            assert!(!highlight(*language, source).is_empty(), "{language:?}");
        }
    }

    #[test]
    fn kinds() {
        assert!(has(LanguageId::Rust, "fn main() { return 42; } // note", "fn", Kind::Keyword));
        assert!(has(LanguageId::Rust, "fn main() { return 42; } // note", "42", Kind::Constant));
        assert!(has(LanguageId::Rust, "fn main() { return 42; } // note", "note", Kind::Comment));
        assert!(has(LanguageId::Json, "{\"a\": \"b\"}", "b", Kind::String));
    }

    #[test]
    fn detection() {
        assert_eq!(detect(Some(Path::new("a/b.RS")), ""), Some(LanguageId::Rust));
        assert_eq!(detect(None, "#!/usr/bin/env python3\n"), Some(LanguageId::Python));
        assert_eq!(detect(Some(Path::new("notes.txt")), "hello"), None);
    }
}

#[cfg(test)]
mod declared_extensions {
    /// DexFiles learns what DexNotas opens from the manifest: every extension DexNotas colours
    /// must be declared there.
    #[test]
    fn the_manifest_declares_every_highlighted_extension() {
        let manifest = include_str!("../../android/app/src/main/AndroidManifest.xml");
        let at = manifest.find("io.github.dexfiles.extensions").unwrap_or(0);
        let value = manifest[at..].split("android:value=\"").nth(1).and_then(|v| v.split('"').next()).unwrap_or("");
        let declared: Vec<&str> = value.split(',').map(str::trim).collect();
        let source = include_str!("mod.rs");
        let start = source.find("pub fn language_for_path").unwrap_or(0);
        let end = source[start..].find("\n}\n").map_or(source.len(), |e| start + e);
        let missing: Vec<&str> = source[start..end]
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric()) && !matches!(*e, "rakefile" | "gemfile"))
            .filter(|e| !declared.contains(e))
            .collect();
        assert!(missing.is_empty(), "not declared in AndroidManifest.xml: {missing:?}");
        let mut seen = std::collections::HashSet::new();
        let repeated: Vec<&&str> = declared.iter().filter(|e| !seen.insert(**e)).collect();
        assert!(repeated.is_empty(), "declared twice: {repeated:?}");
    }
}
