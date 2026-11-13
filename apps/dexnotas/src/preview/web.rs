//! The preview as a web page, for the system's WebView (Android): Markdown becomes HTML with a
//! GitHub-like style, code coloured like the editor and markers of the source lines (so the page
//! can follow the editor); HTML documents are shown as they are, CSS and JavaScript included.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use super::code_language;
use crate::syntax::Kind;

const KINDS: [Kind; 9] =
    [Kind::Keyword, Kind::String, Kind::Comment, Kind::Number, Kind::Type, Kind::Function, Kind::Constant, Kind::Property, Kind::Attribute];

fn kind_class(kind: Kind) -> usize {
    KINDS.iter().position(|k| *k == kind).unwrap_or(0)
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM
        | Options::ENABLE_SUPERSCRIPT
        | Options::ENABLE_SUBSCRIPT
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_HEADING_ATTRIBUTES
}

pub fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
}

/// GitHub's heading ids: lower case, spaces as hyphens, without punctuation.
pub fn slug(text: &str) -> String {
    text.trim()
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                Some(c.to_lowercase().next().unwrap_or(c))
            } else if c.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

/// Code with the editor's colours (classes `k0`…`k8`).
fn code_html(language: Option<crate::syntax::LanguageId>, text: &str, out: &mut String) {
    out.push_str("<pre><code>");
    let spans = match language {
        Some(l) if text.len() <= 256 * 1024 => crate::syntax::highlight(l, text),
        _ => Vec::new(),
    };
    let mut at = 0;
    for (range, kind) in spans {
        if range.start < at || range.end > text.len() || !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
            continue;
        }
        escape(&text[at..range.start], out);
        out.push_str(&format!("<span class=\"k{}\">", kind_class(kind)));
        escape(&text[range.clone()], out);
        out.push_str("</span>");
        at = range.end;
    }
    escape(&text[at..], out);
    out.push_str("</code></pre>\n");
}

/// The HTML of a Markdown text (the page's content), with `data-line` markers.
pub fn markdown_body(text: &str) -> String {
    let mut lines = super::LineCounter::new(text);
    let mut events: Vec<Event> = Vec::new();
    let mut depth = 0usize;
    let mut parser = Parser::new_ext(text, options()).into_offset_iter();
    let marker = |line: usize| Event::Html(format!("<div class=\"dl\" data-line=\"{line}\"></div>\n").into());
    while let Some((event, range)) = parser.next() {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                if depth == 0 {
                    events.push(marker(lines.line_at(range.start)));
                }
                let language = match &kind {
                    CodeBlockKind::Fenced(info) => code_language(info),
                    CodeBlockKind::Indented => None,
                };
                let mut code = String::new();
                for (event, _) in parser.by_ref() {
                    match event {
                        Event::Text(t) => code.push_str(&t),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                let mut html = String::new();
                code_html(language, code.strip_suffix('\n').unwrap_or(&code), &mut html);
                events.push(Event::Html(html.into()));
            }
            Event::Start(Tag::Heading { level, id, classes, attrs }) => {
                if depth == 0 {
                    events.push(marker(lines.line_at(range.start)));
                }
                let mut inner = Vec::new();
                let mut plain = String::new();
                for (event, _) in parser.by_ref() {
                    match &event {
                        Event::End(TagEnd::Heading(_)) => break,
                        Event::Text(t) | Event::Code(t) => plain.push_str(t),
                        _ => {}
                    }
                    inner.push(event);
                }
                let id = id.map_or_else(|| slug(&plain), |i| i.to_string());
                let mut open = format!("<{level} id=\"");
                escape(&id, &mut open);
                open.push('"');
                if !classes.is_empty() {
                    open.push_str(" class=\"");
                    escape(&classes.join(" "), &mut open);
                    open.push('"');
                }
                for (k, v) in attrs {
                    open.push(' ');
                    escape(&k, &mut open);
                    if let Some(v) = v {
                        open.push_str("=\"");
                        escape(&v, &mut open);
                        open.push('"');
                    }
                }
                open.push('>');
                let mut html = open;
                pulldown_cmark::html::push_html(&mut html, inner.into_iter());
                html.push_str(&format!("</{level}>\n"));
                events.push(Event::Html(html.into()));
            }
            Event::Start(ref tag) => {
                let block = matches!(
                    tag,
                    Tag::Paragraph | Tag::BlockQuote(_) | Tag::List(_) | Tag::Table(_) | Tag::HtmlBlock | Tag::FootnoteDefinition(_) | Tag::MetadataBlock(_)
                );
                if block && depth == 0 {
                    events.push(marker(lines.line_at(range.start)));
                }
                let item = matches!(tag, Tag::Item);
                if block || item || matches!(tag, Tag::TableHead | Tag::TableRow | Tag::TableCell) {
                    depth += 1;
                }
                events.push(event);
                if item {
                    events.push(Event::Html(format!("<span class=\"dl\" data-line=\"{}\"></span>", lines.line_at(range.start)).into()));
                }
            }
            Event::End(ref tag) => {
                if matches!(
                    tag,
                    TagEnd::Paragraph
                        | TagEnd::BlockQuote(_)
                        | TagEnd::List(_)
                        | TagEnd::Table
                        | TagEnd::HtmlBlock
                        | TagEnd::FootnoteDefinition
                        | TagEnd::MetadataBlock(_)
                        | TagEnd::Item
                        | TagEnd::TableHead
                        | TagEnd::TableRow
                        | TagEnd::TableCell
                ) {
                    depth = depth.saturating_sub(1);
                }
                events.push(event);
            }
            Event::Rule if depth == 0 => {
                events.push(marker(lines.line_at(range.start)));
                events.push(event);
            }
            other => events.push(other),
        }
    }
    let mut html = String::with_capacity(text.len() * 3 / 2);
    pulldown_cmark::html::push_html(&mut html, events.into_iter());
    html
}

/// The script every page gets: following the editor and links inside the page.
const SCRIPT: &str = r#"
function dexSet(html) { var c = document.getElementById('dex'); if (c) c.innerHTML = html; }
function dexLine(n) {
  var marks = document.querySelectorAll('[data-line]'), best = null;
  for (var i = 0; i < marks.length; i++) { if (+marks[i].getAttribute('data-line') <= n) best = marks[i]; else break; }
  if (!best || n <= 0) { window.scrollTo(0, 0); return; }
  var target = best.tagName === 'SPAN' ? best.parentElement : best;
  window.scrollTo(0, Math.max(0, target.getBoundingClientRect().top + window.scrollY - 8));
}
function dexFraction(f) {
  var h = document.documentElement.scrollHeight - window.innerHeight;
  window.scrollTo(0, Math.max(0, h * f));
}
document.addEventListener('click', function (e) {
  var a = e.target.closest ? e.target.closest('a') : null;
  if (!a) return;
  var href = a.getAttribute('href') || '';
  if (href.charAt(0) === '#') {
    e.preventDefault();
    var id = decodeURIComponent(href.slice(1));
    var t = document.getElementById(id) || document.getElementsByName(id)[0];
    if (t) t.scrollIntoView();
  }
});
"#;

/// A whole page for a Markdown text.
pub fn markdown_page(body: &str, dark: bool, font_px: f32) -> String {
    let c = |rgb: egui::Color32| format!("#{:02x}{:02x}{:02x}", rgb.r(), rgb.g(), rgb.b());
    let palette = crate::ui::theme::palette(dark);
    let (bg, text, muted, border, code_bg, link) = if dark {
        (c(palette.editor_bg), "#e6edf3".to_owned(), "#9198a1", "#3d444d", "#2a2c30", "#4493f8")
    } else {
        ("#ffffff".to_owned(), "#1f2328".to_owned(), "#59636e", "#d1d9e0", "#f6f8fa", "#0969da")
    };
    let mut kinds = String::new();
    for (i, k) in KINDS.iter().enumerate() {
        kinds.push_str(&format!(".k{i}{{color:{}}}", c(crate::ui::theme::syntax(*k, dark))));
    }
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<style>
html{{color-scheme:{scheme}}}
body{{margin:0;background:{bg};color:{text};font-family:-apple-system,"Segoe UI","Noto Sans",Roboto,Helvetica,Arial,sans-serif;font-size:{font_px}px;line-height:1.5;word-wrap:break-word}}
#dex{{max-width:880px;margin:0 auto;padding:24px 32px 48px}}
a{{color:{link};text-decoration:none}} a:hover{{text-decoration:underline}}
h1,h2,h3,h4,h5,h6{{margin:24px 0 16px;font-weight:600;line-height:1.25}}
h1{{font-size:2em;padding-bottom:.3em;border-bottom:1px solid {border}}}
h2{{font-size:1.5em;padding-bottom:.3em;border-bottom:1px solid {border}}}
h3{{font-size:1.25em}} h4{{font-size:1em}} h5{{font-size:.875em}} h6{{font-size:.85em;color:{muted}}}
p,blockquote,ul,ol,dl,table,pre,details{{margin:0 0 16px}}
blockquote{{padding:0 1em;color:{muted};border-left:.25em solid {border};margin-left:0}}
ul,ol{{padding-left:2em}} li+li{{margin-top:.25em}} li>p{{margin:0}}
li:has(> input[type=checkbox]){{list-style:none}}
li > input[type=checkbox]{{margin:0 .4em 0 -1.4em;vertical-align:middle}}
code,pre{{font-family:ui-monospace,"Droid Sans Mono","Cascadia Mono",Menlo,Consolas,monospace;font-size:85%}}
code{{padding:.2em .4em;background:{code_bg};border-radius:6px}}
pre{{padding:16px;overflow:auto;line-height:1.45;background:{code_bg};border-radius:6px}}
pre code{{padding:0;background:none;font-size:100%}}
table{{border-collapse:collapse;display:block;overflow:auto;width:max-content;max-width:100%}}
th,td{{padding:6px 13px;border:1px solid {border}}} th{{font-weight:600}}
tr:nth-child(2n){{background:{code_bg}}}
hr{{height:.25em;padding:0;margin:24px 0;background:{border};border:0}}
img{{max-width:100%}} .dl{{height:0;margin:0;padding:0}}
.footnote-definition{{font-size:.85em;color:{muted}}} .footnote-definition-label{{margin-right:.4em}} .footnote-definition p{{display:inline}}
{kinds}
</style><script>{SCRIPT}</script></head><body><div id="dex">{body}</div></body></html>"#,
        scheme = if dark { "dark" } else { "light" },
    )
}

/// An HTML document with the page script added (for following the editor), as written otherwise.
pub fn html_page(text: &str) -> String {
    let script = format!("<script>{SCRIPT}</script>");
    // After <head> when there is one, else at the start (browsers accept both).
    let end = (0..=text.len().min(8192)).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
    let lower = text[..end].to_ascii_lowercase();
    let head = lower.match_indices("<head").map(|(i, _)| i).find(|i| matches!(lower.as_bytes().get(i + 5), Some(b'>' | b' ' | b'\t' | b'\n' | b'\r')));
    match head.and_then(|i| lower[i..].find('>').map(|e| i + e + 1)) {
        Some(at) => format!("{}{script}{}", &text[..at], &text[at..]),
        None => format!("{script}{text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_to_html() {
        let html = markdown_body("# Hola Mundo!\n\ntexto **b**\n\n- [x] a\n- b\n\n```rust\nfn x() {}\n```\n");
        assert!(html.contains("<div class=\"dl\" data-line=\"0\"></div>\n<h1 id=\"hola-mundo\">Hola Mundo!</h1>"), "{html}");
        assert!(html.contains("data-line=\"2\"></div>\n<p>texto <strong>b</strong></p>"), "{html}");
        assert!(html.contains("<li><span class=\"dl\" data-line=\"4\"></span><input disabled=\"\" type=\"checkbox\" checked=\"\"/>"), "{html}");
        assert!(html.contains("<span class=\"k0\">fn</span>"), "{html}");
    }

    #[test]
    fn script_goes_in_the_head() {
        assert!(html_page("<html><HEAD><title>x</title>").starts_with("<html><HEAD><script>"));
        assert!(html_page("<p>x").starts_with("<script>"));
    }
}

#[cfg(test)]
mod dump {
    /// `DEXNOTAS_WEB_DUMP=in.md:out.html cargo test dump_page`: write the page of a file, to look at.
    #[test]
    fn dump_page() {
        let Ok(spec) = std::env::var("DEXNOTAS_WEB_DUMP") else { return };
        let Some((input, output)) = spec.split_once(':') else { return };
        let Ok(text) = std::fs::read_to_string(input) else { return };
        let page = super::markdown_page(&super::markdown_body(&text), std::env::var("DEXNOTAS_LIGHT").is_err(), 15.0);
        let _ = std::fs::write(output, page);
    }
}
