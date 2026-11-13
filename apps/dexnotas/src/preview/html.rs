//! HTML into the preview's blocks: a forgiving tokenizer (unclosed `<p>` and `<li>`, stray
//! tags, entities) and a converter that keeps the content and drops what only a browser uses
//! (CSS, scripts, forms).

use std::iter::Peekable;
use std::vec::IntoIter;

use super::{Align, Block, Inline, Item, LineCounter, Parsed, Style, code_language};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    Open { name: String, attrs: Vec<(String, String)> },
    Close(String),
    Text(String),
}

impl Token {
    fn attr(&self, key: &str) -> Option<&str> {
        match self {
            Token::Open { attrs, .. } => attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()),
            _ => None,
        }
    }
}

/// Elements whose content is raw text (no tags inside).
const RAW: &[&str] = &["script", "style", "textarea", "title", "xmp"];
/// Elements that are skipped with their content.
const SKIP: &[&str] = &["head", "script", "style", "template", "title", "svg", "math", "iframe", "object", "canvas", "select", "textarea", "noscript", "button"];
/// Elements that are only containers: their content is shown, they add nothing.
const CONTAINERS: &[&str] = &[
    "html", "body", "div", "section", "article", "header", "footer", "main", "nav", "aside", "figure", "address", "center", "form",
    "fieldset", "details", "dl", "dd", "hgroup", "picture", "legend", "tbody", "thead", "tfoot", "caption",
];
/// Elements that start a block (they end a paragraph).
const BLOCKS: &[&str] = &[
    "p", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "li", "blockquote", "pre", "hr", "table", "tr", "td", "th", "summary", "dt",
    "figcaption",
];

fn is_block(name: &str) -> bool {
    BLOCKS.contains(&name) || CONTAINERS.contains(&name) || SKIP.contains(&name)
}

/// The tokens of `src`, each with its byte offset.
pub fn tokens(src: &str) -> Vec<(Token, usize)> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut text_start = 0;
    let flush = |out: &mut Vec<(Token, usize)>, from: usize, to: usize| {
        if from < to {
            out.push((Token::Text(decode_entities(&src[from..to])), from));
        }
    };
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &src[i..];
        if rest.starts_with("<!--") {
            flush(&mut out, text_start, i);
            i = rest.find("-->").map_or(b.len(), |e| i + e + 3);
            text_start = i;
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            flush(&mut out, text_start, i);
            i = rest.find('>').map_or(b.len(), |e| i + e + 1);
            text_start = i;
            continue;
        }
        let close = rest.starts_with("</");
        let name_start = i + if close { 2 } else { 1 };
        let name_len = b[name_start..].iter().take_while(|c| c.is_ascii_alphanumeric() || **c == b'-' || **c == b':').count();
        if name_len == 0 || !b[name_start].is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        flush(&mut out, text_start, i);
        let at = i;
        let name = src[name_start..name_start + name_len].to_ascii_lowercase();
        let mut j = name_start + name_len;
        if close {
            j = src[j..].find('>').map_or(b.len(), |e| j + e + 1);
            out.push((Token::Close(name), at));
            i = j;
            text_start = i;
            continue;
        }
        // Attributes.
        let mut attrs = Vec::new();
        let mut self_closed = false;
        loop {
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= b.len() {
                break;
            }
            if b[j] == b'>' {
                j += 1;
                break;
            }
            if b[j] == b'/' {
                j += 1;
                if b.get(j) == Some(&b'>') {
                    self_closed = true;
                    j += 1;
                    break;
                }
                continue;
            }
            let key_start = j;
            while j < b.len() && !b[j].is_ascii_whitespace() && !matches!(b[j], b'=' | b'>' | b'/') {
                j += 1;
            }
            let key = src[key_start..j].to_ascii_lowercase();
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let mut value = String::new();
            if b.get(j) == Some(&b'=') {
                j += 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if let Some(&q) = b.get(j)
                    && (q == b'"' || q == b'\'')
                {
                    let end = src[j + 1..].find(q as char).map_or(b.len(), |e| j + 1 + e);
                    value = decode_entities(&src[j + 1..end]);
                    j = (end + 1).min(b.len());
                } else {
                    let s = j;
                    while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' {
                        j += 1;
                    }
                    value = decode_entities(&src[s..j]);
                }
            }
            if !key.is_empty() {
                attrs.push((key, value));
            }
        }
        i = j;
        text_start = i;
        let raw = RAW.contains(&name.as_str()) && !self_closed;
        out.push((Token::Open { name: name.clone(), attrs }, at));
        if self_closed && !is_void(&name) {
            out.push((Token::Close(name.clone()), at));
        }
        if raw {
            // Up to the closing tag, whatever is inside.
            let lower = src[i..].to_ascii_lowercase();
            let end = lower.find(&format!("</{name}")).map_or(b.len(), |e| i + e);
            if end > i {
                out.push((Token::Text(src[i..end].to_owned()), i));
            }
            i = end;
            text_start = i;
        }
    }
    flush(&mut out, text_start, b.len());
    out
}

fn is_void(name: &str) -> bool {
    matches!(name, "br" | "hr" | "img" | "input" | "meta" | "link" | "wbr" | "source" | "col" | "area" | "base" | "embed" | "track")
}

/// One tag, as found inside Markdown.
pub fn tag(s: &str) -> Option<Token> {
    let mut t = tokens(s.trim());
    if t.len() == 1 || (t.len() == 2 && matches!(t[1].0, Token::Close(_))) { Some(t.swap_remove(0).0) } else { None }
}

/// Replace `&amp;`, `&#233;`, `&eacute;`… with their characters.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        let end = rest[1..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '#')).map(|e| e + 1);
        let decoded = end.filter(|e| rest[*e..].starts_with(';')).and_then(|e| entity(&rest[1..e]).map(|c| (c, e + 1)));
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let n = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(n);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "hellip" => '…',
        "mdash" => '—',
        "ndash" => '–',
        "laquo" => '«',
        "raquo" => '»',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "bull" => '•',
        "middot" => '·',
        "euro" => '€',
        "pound" => '£',
        "yen" => '¥',
        "cent" => '¢',
        "deg" => '°',
        "times" => '×',
        "divide" => '÷',
        "plusmn" => '±',
        "iexcl" => '¡',
        "iquest" => '¿',
        "ordf" => 'ª',
        "ordm" => 'º',
        "sect" => '§',
        "para" => '¶',
        "larr" => '←',
        "rarr" => '→',
        "uarr" => '↑',
        "darr" => '↓',
        "aacute" => 'á',
        "eacute" => 'é',
        "iacute" => 'í',
        "oacute" => 'ó',
        "uacute" => 'ú',
        "Aacute" => 'Á',
        "Eacute" => 'É',
        "Iacute" => 'Í',
        "Oacute" => 'Ó',
        "Uacute" => 'Ú',
        "ntilde" => 'ñ',
        "Ntilde" => 'Ñ',
        "uuml" => 'ü',
        "Uuml" => 'Ü',
        "ouml" => 'ö',
        "Ouml" => 'Ö',
        "auml" => 'ä',
        "Auml" => 'Ä',
        "euml" => 'ë',
        "iuml" => 'ï',
        "agrave" => 'à',
        "egrave" => 'è',
        "igrave" => 'ì',
        "ograve" => 'ò',
        "ugrave" => 'ù',
        "acirc" => 'â',
        "ecirc" => 'ê',
        "icirc" => 'î',
        "ocirc" => 'ô',
        "ucirc" => 'û',
        "ccedil" => 'ç',
        "Ccedil" => 'Ç',
        "szlig" => 'ß',
        _ => return None,
    })
}

/// The inline style in effect, as tags open and close.
#[derive(Default)]
pub(crate) struct InlineState {
    bold: u16,
    italic: u16,
    code: u16,
    strike: u16,
    underline: u16,
    small: u16,
    mark: u16,
    pub link: Option<u32>,
}

fn step(n: &mut u16, open: bool) {
    *n = if open { n.saturating_add(1) } else { n.saturating_sub(1) };
}

impl InlineState {
    pub fn style(&self) -> Style {
        Style {
            bold: self.bold > 0,
            italic: self.italic > 0,
            code: self.code > 0,
            strike: self.strike > 0,
            underline: self.underline > 0,
            small: self.small > 0,
            mark: self.mark > 0,
        }
    }

    pub fn set_bold(&mut self, open: bool) {
        step(&mut self.bold, open);
    }

    pub fn set_italic(&mut self, open: bool) {
        step(&mut self.italic, open);
    }

    pub fn set_strike(&mut self, open: bool) {
        step(&mut self.strike, open);
    }

    pub fn set_small(&mut self, open: bool) {
        step(&mut self.small, open);
    }

    /// Apply an inline tag; false if it is not one.
    pub fn apply(&mut self, token: &Token, links: &mut Vec<String>, out: &mut Vec<Inline>) -> bool {
        let (name, open) = match token {
            Token::Open { name, .. } => (name.as_str(), true),
            Token::Close(name) => (name.as_str(), false),
            Token::Text(_) => return false,
        };
        match name {
            "b" | "strong" => step(&mut self.bold, open),
            "i" | "em" | "cite" | "var" | "dfn" => step(&mut self.italic, open),
            "code" | "kbd" | "samp" | "tt" => step(&mut self.code, open),
            "s" | "del" | "strike" => step(&mut self.strike, open),
            "u" | "ins" => step(&mut self.underline, open),
            "sup" | "sub" | "small" => step(&mut self.small, open),
            "mark" => step(&mut self.mark, open),
            "a" => {
                if open {
                    if let Some(href) = token.attr("href") {
                        links.push(href.to_owned());
                        self.link = Some(links.len() as u32 - 1);
                    }
                } else {
                    self.link = None;
                }
            }
            "br" => {
                if open {
                    self.text(out, "\n");
                }
            }
            "img" => {
                if open {
                    let src = token.attr("src").unwrap_or("").to_owned();
                    let alt = token.attr("alt").or_else(|| token.attr("title")).unwrap_or("").to_owned();
                    out.push(Inline::Image { src, alt, link: self.link });
                }
            }
            "input" => {
                if open && token.attr("type").is_some_and(|t| t.eq_ignore_ascii_case("checkbox")) {
                    self.text(out, if token.attr("checked").is_some() { "☑ " } else { "☐ " });
                }
            }
            "span" | "font" | "abbr" | "label" | "time" | "q" | "bdi" | "bdo" | "data" | "wbr" | "nobr" | "big" => {}
            _ => return false,
        }
        true
    }

    pub fn text(&self, out: &mut Vec<Inline>, text: &str) {
        let style = self.style();
        // Join with the previous piece when it looks the same.
        if let Some(Inline::Text { text: last, style: s, link }) = out.last_mut()
            && *s == style
            && *link == self.link
        {
            last.push_str(text);
            return;
        }
        out.push(Inline::Text { text: text.to_owned(), style, link: self.link });
    }
}

struct Converter {
    tokens: Peekable<IntoIter<(Token, usize)>>,
    links: Vec<String>,
    inline: InlineState,
    /// The byte offset each top-level block starts at.
    starts: Vec<usize>,
}

pub fn parse(src: &str) -> Parsed {
    let mut c = Converter { tokens: tokens(src).into_iter().peekable(), links: Vec::new(), inline: InlineState::default(), starts: Vec::new() };
    let mut blocks = Vec::new();
    while c.tokens.peek().is_some() {
        if !c.next_blocks(&[], &mut blocks) {
            // A stray closing tag at the top: skip it.
            c.tokens.next();
        }
    }
    let mut lines = LineCounter::new(src);
    let mut starts: Vec<usize> = c.starts.iter().map(|at| lines.line_at(*at)).collect();
    starts.resize(blocks.len(), starts.last().copied().unwrap_or(0));
    Parsed { blocks, lines: starts, links: c.links, base: None }
}

/// Parse the blocks of a Markdown file's HTML block.
pub fn blocks_of(src: &str, links: &mut Vec<String>) -> Vec<Block> {
    let mut c = Converter { tokens: tokens(src).into_iter().peekable(), links: std::mem::take(links), inline: InlineState::default(), starts: Vec::new() };
    let mut blocks = Vec::new();
    while c.tokens.peek().is_some() {
        if !c.next_blocks(&[], &mut blocks) {
            c.tokens.next();
        }
    }
    *links = c.links;
    blocks
}

impl Converter {
    /// The blocks inside an element whose open ancestors are `open` (the innermost last), up to
    /// its end.
    fn blocks(&mut self, open: &[&str]) -> Vec<Block> {
        let mut out = Vec::new();
        while self.next_blocks(open, &mut out) {}
        out
    }

    /// Read the next block(s) into `out`; false at the end of the element (or of the text).
    fn next_blocks(&mut self, open: &[&str], out: &mut Vec<Block>) -> bool {
        let Some(&(_, at)) = self.tokens.peek() else { return false };
        let before = out.len();
        let container = matches!(self.tokens.peek(), Some((Token::Open { name, .. }, _)) if CONTAINERS.contains(&name.as_str()));
        let more = self.next_blocks_inner(open, out);
        // Blocks at the top (inside only containers) remember where they start; a container's
        // blocks did that themselves.
        if !container && open.iter().all(|o| CONTAINERS.contains(o)) {
            self.starts.extend(std::iter::repeat_n(at, out.len().saturating_sub(before)));
        }
        more
    }

    fn next_blocks_inner(&mut self, open: &[&str], out: &mut Vec<Block>) -> bool {
        let Some((token, _)) = self.tokens.peek() else { return false };
        let inner = open.last().copied();
        match token.clone() {
            Token::Close(name) => {
                if inner == Some(name.as_str()) {
                    self.tokens.next();
                    return false;
                }
                if open.contains(&name.as_str()) {
                    // An ancestor ends: so does this element.
                    return false;
                }
                // Stray or inline: let the paragraph reader deal with it (or skip it).
                if is_block(&name) {
                    self.tokens.next();
                } else {
                    self.paragraph(open, out);
                }
                true
            }
            Token::Text(text) => {
                if text.trim().is_empty() {
                    self.tokens.next();
                } else {
                    self.paragraph(open, out);
                }
                true
            }
            Token::Open { name, attrs } => {
                let name = name.as_str();
                // A new item ends the previous one; a new row or cell, the previous one.
                if (name == "li" && inner == Some("li"))
                    || (matches!(name, "td" | "th") && matches!(inner, Some("td" | "th")))
                    || (name == "tr" && matches!(inner, Some("tr" | "td" | "th")))
                    || (matches!(name, "dt" | "dd") && matches!(inner, Some("dt" | "dd")))
                {
                    return false;
                }
                if SKIP.contains(&name) {
                    self.tokens.next();
                    self.skip(name);
                    return true;
                }
                let mut nested: Vec<&str> = open.to_vec();
                nested.push(name);
                match name {
                    _ if CONTAINERS.contains(&name) || name == "li" && inner != Some("ul") && inner != Some("ol") => {
                        self.tokens.next();
                        out.extend(self.blocks(&nested));
                    }
                    "p" | "summary" | "dt" | "figcaption" => {
                        self.tokens.next();
                        let bold = name == "summary" || name == "dt";
                        if bold {
                            self.inline.set_bold(true);
                        }
                        let inlines = self.inlines(&nested);
                        if bold {
                            self.inline.set_bold(false);
                        }
                        push_paragraph(out, inlines);
                    }
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        self.tokens.next();
                        let level = name.as_bytes()[1] - b'0';
                        let inlines = self.inlines(&nested);
                        if has_content(&inlines) {
                            out.push(Block::Heading { level, inlines: trim(inlines) });
                        }
                    }
                    "ul" | "ol" => {
                        self.tokens.next();
                        let start = (name == "ol").then(|| attrs.iter().find(|(k, _)| k == "start").and_then(|(_, v)| v.trim().parse().ok()).unwrap_or(1));
                        out.push(Block::List { start, items: self.items(&nested) });
                    }
                    "blockquote" => {
                        self.tokens.next();
                        out.push(Block::Quote(self.blocks(&nested)));
                    }
                    "pre" => {
                        self.tokens.next();
                        out.push(self.pre(&attrs));
                    }
                    "hr" => {
                        self.tokens.next();
                        out.push(Block::Rule);
                    }
                    "table" => {
                        self.tokens.next();
                        out.push(self.table());
                    }
                    "tr" | "td" | "th" => {
                        // Outside a table: their content.
                        self.tokens.next();
                        out.extend(self.blocks(&nested));
                    }
                    _ => self.paragraph(open, out),
                }
                true
            }
        }
    }

    /// Skip an element and everything in it.
    fn skip(&mut self, name: &str) {
        let mut depth = 1;
        for (token, _) in self.tokens.by_ref() {
            match token {
                Token::Open { name: n, .. } if n == name && !is_void(name) => depth += 1,
                Token::Close(n) if n == name => {
                    depth -= 1;
                    if depth == 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    /// Text and inline elements up to the next block, into a paragraph.
    fn paragraph(&mut self, open: &[&str], out: &mut Vec<Block>) {
        let inlines = self.inlines(open);
        if inlines.is_empty() {
            // Nothing could be read: don't loop on it.
            self.tokens.next();
        }
        push_paragraph(out, inlines);
    }

    /// Inline content up to a block or the end of the innermost of `open` (which is consumed
    /// when it is a paragraph-like element).
    fn inlines(&mut self, open: &[&str]) -> Vec<Inline> {
        let mut out = Vec::new();
        let inner = open.last().copied();
        let ends_here = matches!(inner, Some("p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "summary" | "dt" | "figcaption"));
        while let Some((token, _)) = self.tokens.peek() {
            match token {
                Token::Text(text) => {
                    let collapsed = collapse(text, ends_with_space(&out));
                    self.inline.text(&mut out, &collapsed);
                    self.tokens.next();
                }
                Token::Close(name) => {
                    if ends_here && inner == Some(name.as_str()) {
                        self.tokens.next();
                        break;
                    }
                    if is_block(name) || open.contains(&name.as_str()) {
                        break;
                    }
                    let token = token.clone();
                    self.inline.apply(&token, &mut self.links, &mut out);
                    self.tokens.next();
                }
                Token::Open { name, .. } => {
                    if is_block(name) {
                        break;
                    }
                    // Unknown inline elements add nothing; their content shows.
                    let token = token.clone();
                    self.tokens.next();
                    self.inline.apply(&token, &mut self.links, &mut out);
                }
            }
        }
        trim(out)
    }

    fn items(&mut self, open: &[&str]) -> Vec<Item> {
        let mut items = Vec::new();
        loop {
            match self.tokens.peek() {
                None => break,
                Some((Token::Close(name), _)) => {
                    let name = name.clone();
                    if open.last() == Some(&name.as_str()) {
                        self.tokens.next();
                        break;
                    }
                    if open.contains(&name.as_str()) {
                        break;
                    }
                    self.tokens.next();
                }
                Some((Token::Open { name, .. }, _)) if name == "li" => {
                    self.tokens.next();
                    let mut nested = open.to_vec();
                    nested.push("li");
                    let mut blocks = self.blocks(&nested);
                    let task = take_checkbox(&mut blocks);
                    items.push(Item { task, blocks });
                }
                Some((Token::Text(t), _)) if t.trim().is_empty() => {
                    self.tokens.next();
                }
                Some(_) => {
                    // Content outside an item: an item of its own.
                    let mut nested = open.to_vec();
                    nested.push("li");
                    let mut blocks = Vec::new();
                    if !self.next_blocks(&nested, &mut blocks) && blocks.is_empty() {
                        self.tokens.next();
                    }
                    if !blocks.is_empty() {
                        items.push(Item { task: None, blocks });
                    }
                }
            }
        }
        items
    }

    fn pre(&mut self, attrs: &[(String, String)]) -> Block {
        let mut text = String::new();
        let mut language = attrs.iter().find(|(k, _)| k == "class").and_then(|(_, v)| v.split_whitespace().find_map(code_language));
        for (token, _) in self.tokens.by_ref() {
            match token {
                Token::Text(t) => text.push_str(&t),
                Token::Close(n) if n == "pre" => break,
                Token::Open { ref name, .. } if name == "br" => text.push('\n'),
                Token::Open { ref name, .. } if name == "code" && language.is_none() => {
                    language = token.attr("class").and_then(|c| c.split_whitespace().find_map(code_language));
                }
                _ => {}
            }
        }
        // A newline right after <pre> is not content.
        let text = text.strip_prefix("\r\n").or_else(|| text.strip_prefix('\n')).unwrap_or(&text);
        let text = text.trim_end_matches(['\n', '\r']).to_owned();
        Block::Code { language, text, spans: Vec::new() }
    }

    fn table(&mut self) -> Block {
        let mut head: Vec<Vec<Inline>> = Vec::new();
        let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();
        let mut aligns: Vec<Align> = Vec::new();
        let mut row: Option<(Vec<Vec<Inline>>, bool)> = None;
        let finish = |row: &mut Option<(Vec<Vec<Inline>>, bool)>, head: &mut Vec<Vec<Inline>>, rows: &mut Vec<Vec<Vec<Inline>>>| {
            if let Some((cells, all_th)) = row.take()
                && !cells.is_empty()
            {
                if all_th && head.is_empty() && rows.is_empty() {
                    *head = cells;
                } else {
                    rows.push(cells);
                }
            }
        };
        while let Some((token, _)) = self.tokens.peek() {
            match token.clone() {
                Token::Close(name) if name == "table" => {
                    self.tokens.next();
                    break;
                }
                Token::Open { name, .. } if name == "table" => {
                    // A table in a table: its text goes in the cell (not supported as a grid).
                    self.tokens.next();
                    self.skip("table");
                }
                Token::Open { name, .. } if name == "tr" => {
                    self.tokens.next();
                    finish(&mut row, &mut head, &mut rows);
                    row = Some((Vec::new(), true));
                }
                Token::Close(name) if name == "tr" => {
                    self.tokens.next();
                    finish(&mut row, &mut head, &mut rows);
                }
                Token::Open { name, attrs } if name == "td" || name == "th" => {
                    self.tokens.next();
                    let cells = row.get_or_insert_with(|| (Vec::new(), true));
                    let column = cells.0.len();
                    if aligns.len() <= column {
                        let align = attrs
                            .iter()
                            .find(|(k, _)| k == "align" || k == "style")
                            .map(|(_, v)| v.to_ascii_lowercase())
                            .map_or(Align::Left, |v| if v.contains("center") { Align::Center } else if v.contains("right") { Align::Right } else { Align::Left });
                        aligns.push(align);
                    }
                    if name == "td" {
                        cells.1 = false;
                    }
                    let open = ["table", "tr", name.as_str()];
                    let mut blocks = Vec::new();
                    while self.next_blocks(&open, &mut blocks) {}
                    let mut inlines = Vec::new();
                    for b in blocks {
                        flatten(b, &mut inlines);
                    }
                    if let Some((cells, _)) = row.as_mut() {
                        cells.push(inlines);
                    }
                }
                Token::Open { name, .. } if name == "caption" => {
                    self.tokens.next();
                    self.skip("caption");
                }
                _ => {
                    self.tokens.next();
                }
            }
        }
        finish(&mut row, &mut head, &mut rows);
        let columns = head.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
        aligns.resize(columns, Align::Left);
        Block::Table { aligns, head, rows }
    }
}

/// The text of a block as inline pieces (for a table cell).
fn flatten(block: Block, out: &mut Vec<Inline>) {
    let sep = |out: &mut Vec<Inline>| {
        if !out.is_empty() {
            out.push(Inline::Text { text: "\n".into(), style: Style::default(), link: None });
        }
    };
    match block {
        Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
            sep(out);
            out.extend(inlines);
        }
        Block::Code { text, .. } => {
            sep(out);
            out.push(Inline::Text { text, style: Style { code: true, ..Style::default() }, link: None });
        }
        Block::Quote(blocks) => blocks.into_iter().for_each(|b| flatten(b, out)),
        Block::List { items, .. } => {
            for item in items {
                sep(out);
                out.push(Inline::Text { text: "• ".into(), style: Style::default(), link: None });
                let mut first = true;
                for b in item.blocks {
                    if first {
                        if let Block::Paragraph(inlines) = b {
                            out.extend(inlines);
                        } else {
                            flatten(b, out);
                        }
                        first = false;
                    } else {
                        flatten(b, out);
                    }
                }
            }
        }
        Block::Rule | Block::Table { .. } => {}
    }
}

/// An item that starts with a checkbox is a task.
fn take_checkbox(blocks: &mut [Block]) -> Option<bool> {
    let Some(Block::Paragraph(inlines)) = blocks.first_mut() else { return None };
    let Some(Inline::Text { text, .. }) = inlines.first_mut() else { return None };
    for (mark, done) in [("☐ ", false), ("☑ ", true)] {
        if let Some(rest) = text.strip_prefix(mark) {
            *text = rest.to_owned();
            return Some(done);
        }
    }
    None
}

fn push_paragraph(out: &mut Vec<Block>, inlines: Vec<Inline>) {
    if has_content(&inlines) {
        out.push(Block::Paragraph(inlines));
    }
}

fn has_content(inlines: &[Inline]) -> bool {
    inlines.iter().any(|i| match i {
        Inline::Text { text, .. } => !text.trim().is_empty(),
        Inline::Image { .. } => true,
    })
}

fn ends_with_space(out: &[Inline]) -> bool {
    match out.last() {
        None => true,
        Some(Inline::Text { text, .. }) => text.ends_with([' ', '\n']),
        Some(Inline::Image { .. }) => false,
    }
}

/// HTML's white space: runs become one space (none at the start of a line).
fn collapse(text: &str, after_space: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = after_space;
    for c in text.chars() {
        if c.is_whitespace() && c != '\u{a0}' {
            if !space {
                out.push(' ');
                space = true;
            }
        } else {
            out.push(c);
            space = false;
        }
    }
    out
}

/// Without white space at the start and the end.
fn trim(mut inlines: Vec<Inline>) -> Vec<Inline> {
    if let Some(Inline::Text { text, .. }) = inlines.first_mut() {
        *text = text.trim_start_matches([' ', '\n']).to_owned();
    }
    if let Some(Inline::Text { text, .. }) = inlines.last_mut() {
        let trimmed = text.trim_end_matches([' ', '\n']).len();
        text.truncate(trimmed);
    }
    inlines.retain(|i| !matches!(i, Inline::Text { text, .. } if text.is_empty()));
    inlines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(inlines: &[Inline]) -> String {
        inlines
            .iter()
            .map(|i| match i {
                Inline::Text { text, .. } => text.clone(),
                Inline::Image { alt, .. } => format!("[{alt}]"),
            })
            .collect()
    }

    #[test]
    fn tokens_and_entities() {
        let t = tokens("<p class=\"a\">x &amp; y&eacute;<br/></P><!-- no --><script>if (a<b) x()</script>");
        assert_eq!(t[0].0, Token::Open { name: "p".into(), attrs: vec![("class".into(), "a".into())] });
        assert_eq!(t[1].0, Token::Text("x & yé".into()));
        assert_eq!(t[2].0, Token::Open { name: "br".into(), attrs: vec![] });
        assert_eq!(t[3].0, Token::Close("p".into()));
        assert_eq!(t[5].0, Token::Text("if (a<b) x()".into()));
    }

    #[test]
    fn a_page() {
        let p = parse(
            "<!DOCTYPE html><html><head><title>T</title><style>p{}</style></head><body>\n\
             <h1>Title</h1>\n<p>One <b>bold</b>\n   word<p>Two</p>\n<ul><li>a<li>b <i>c</i></ul>\n\
             <pre><code class=\"language-rust\">fn x() {}\n</code></pre>\n\
             <table><tr><th>A<th>B<tr><td>1<td>2</table><hr>loose text</body></html>",
        );
        let b = &p.blocks;
        assert!(matches!(&b[0], Block::Heading { level: 1, inlines } if text(inlines) == "Title"));
        assert!(matches!(&b[1], Block::Paragraph(i) if text(i) == "One bold word"));
        assert!(matches!(&b[2], Block::Paragraph(i) if text(i) == "Two"));
        let Block::List { items, start: None } = &b[3] else { panic!("{:?}", b[3]) };
        assert_eq!(items.len(), 2);
        assert!(matches!(&b[4], Block::Code { language: Some(crate::syntax::LanguageId::Rust), text, .. } if text == "fn x() {}"));
        let Block::Table { head, rows, .. } = &b[5] else { panic!() };
        assert_eq!(text(&head[1]), "B");
        assert_eq!(text(&rows[0][0]), "1");
        assert_eq!(b[6], Block::Rule);
        assert!(matches!(&b[7], Block::Paragraph(i) if text(i) == "loose text"));
        assert_eq!(p.lines[0], 1);
    }
}
