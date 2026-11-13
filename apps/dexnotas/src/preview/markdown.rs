//! Markdown (CommonMark with GitHub's tables, task lists, strikethrough and footnotes) into the
//! preview's blocks, with `pulldown-cmark`.

use std::iter::Peekable;
use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use super::html::{self, InlineState};
use super::{Align, Block, Inline, Item, LineCounter, Parsed, Style, code_language};

pub fn parse(text: &str) -> Parsed {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM
        | Options::ENABLE_SUPERSCRIPT
        | Options::ENABLE_SUBSCRIPT
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut r = Reader { events: Parser::new_ext(text, options).into_offset_iter().peekable(), links: Vec::new(), inline: InlineState::default() };
    let mut lines = LineCounter::new(text);
    let mut blocks = Vec::new();
    let mut starts = Vec::new();
    let mut pending = Vec::new();
    while let Some((event, range)) = r.events.next() {
        let before = blocks.len();
        let line = lines.line_at(range.start);
        r.event(event, &mut pending, &mut blocks);
        starts.extend(std::iter::repeat_n(line, blocks.len() - before));
    }
    if !pending.is_empty() {
        blocks.push(Block::Paragraph(pending));
        starts.push(lines.line_at(text.len()));
    }
    Parsed { blocks, lines: starts, links: r.links, base: None }
}

struct Reader<'a, I: Iterator<Item = (Event<'a>, Range<usize>)>> {
    events: Peekable<I>,
    links: Vec<String>,
    inline: InlineState,
}

impl<'a, I: Iterator<Item = (Event<'a>, Range<usize>)>> Reader<'a, I> {
    /// One event at block level: a block into `out`, or inline content into `pending` (as in a
    /// tight list's items, whose text has no paragraph).
    fn event(&mut self, event: Event<'a>, pending: &mut Vec<Inline>, out: &mut Vec<Block>) {
        if self.inline_event(&event, pending) {
            return;
        }
        if !pending.is_empty() {
            out.push(Block::Paragraph(std::mem::take(pending)));
        }
        match event {
            Event::Start(Tag::Paragraph) => {
                let inlines = self.inlines(|e| matches!(e, TagEnd::Paragraph));
                if !inlines.is_empty() {
                    out.push(Block::Paragraph(inlines));
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                let inlines = self.inlines(|e| matches!(e, TagEnd::Heading(_)));
                out.push(Block::Heading { level: level as u8, inlines });
            }
            Event::Start(Tag::BlockQuote(_)) => {
                let blocks = self.blocks(|e| matches!(e, TagEnd::BlockQuote(_)));
                out.push(Block::Quote(blocks));
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match &kind {
                    CodeBlockKind::Fenced(info) => code_language(info),
                    CodeBlockKind::Indented => None,
                };
                let mut text = String::new();
                for (event, _) in self.events.by_ref() {
                    match event {
                        Event::Text(t) => text.push_str(&t),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                if text.ends_with('\n') {
                    text.pop();
                }
                out.push(Block::Code { language, text, spans: Vec::new() });
            }
            Event::Start(Tag::List(start)) => {
                let mut items = Vec::new();
                while let Some((event, _)) = self.events.next() {
                    match event {
                        Event::Start(Tag::Item) => {
                            let task = match self.events.peek() {
                                Some((Event::TaskListMarker(done), _)) => {
                                    let done = *done;
                                    self.events.next();
                                    Some(done)
                                }
                                _ => None,
                            };
                            let blocks = self.blocks(|e| matches!(e, TagEnd::Item));
                            items.push(Item { task, blocks });
                        }
                        Event::End(TagEnd::List(_)) => break,
                        _ => {}
                    }
                }
                out.push(Block::List { start, items });
            }
            Event::Start(Tag::Table(aligns)) => {
                let aligns = aligns
                    .iter()
                    .map(|a| match a {
                        Alignment::Center => Align::Center,
                        Alignment::Right => Align::Right,
                        Alignment::Left | Alignment::None => Align::Left,
                    })
                    .collect();
                let mut head = Vec::new();
                let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();
                let mut in_head = false;
                while let Some((event, _)) = self.events.next() {
                    match event {
                        Event::Start(Tag::TableHead) => in_head = true,
                        Event::End(TagEnd::TableHead) => in_head = false,
                        Event::Start(Tag::TableRow) => rows.push(Vec::new()),
                        Event::Start(Tag::TableCell) => {
                            let cell = self.inlines(|e| matches!(e, TagEnd::TableCell));
                            if in_head {
                                head.push(cell);
                            } else if let Some(row) = rows.last_mut() {
                                row.push(cell);
                            }
                        }
                        Event::End(TagEnd::Table) => break,
                        _ => {}
                    }
                }
                out.push(Block::Table { aligns, head, rows });
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                let mut blocks = self.blocks(|e| matches!(e, TagEnd::FootnoteDefinition));
                let mark = Inline::Text { text: format!("[{label}] "), style: Style { small: true, ..Style::default() }, link: None };
                match blocks.first_mut() {
                    Some(Block::Paragraph(inlines)) => inlines.insert(0, mark),
                    _ => blocks.insert(0, Block::Paragraph(vec![mark])),
                }
                out.extend(blocks);
            }
            Event::Start(Tag::HtmlBlock) => {
                let mut source = String::new();
                for (event, _) in self.events.by_ref() {
                    match event {
                        Event::Html(h) | Event::Text(h) => source.push_str(&h),
                        Event::End(TagEnd::HtmlBlock) => break,
                        _ => {}
                    }
                }
                out.extend(html::blocks_of(&source, &mut self.links));
            }
            Event::Start(Tag::MetadataBlock(_)) => {
                // Front matter: shown as code, as GitHub does.
                let mut text = String::new();
                for (event, _) in self.events.by_ref() {
                    match event {
                        Event::Text(t) => text.push_str(&t),
                        Event::End(TagEnd::MetadataBlock(_)) => break,
                        _ => {}
                    }
                }
                let text = text.trim_end().to_owned();
                out.push(Block::Code { language: Some(crate::syntax::LanguageId::Yaml), text, spans: Vec::new() });
            }
            Event::Html(h) => out.extend(html::blocks_of(&h, &mut self.links)),
            Event::Rule => out.push(Block::Rule),
            Event::DisplayMath(m) => out.push(Block::Code { language: None, text: m.to_string(), spans: Vec::new() }),
            _ => {}
        }
    }

    /// Blocks up to the end tag `is_end` matches.
    fn blocks(&mut self, is_end: impl Fn(&TagEnd) -> bool) -> Vec<Block> {
        let mut out = Vec::new();
        let mut pending = Vec::new();
        while let Some((event, _)) = self.events.next() {
            if let Event::End(end) = &event
                && is_end(end)
            {
                break;
            }
            self.event(event, &mut pending, &mut out);
        }
        if !pending.is_empty() {
            out.push(Block::Paragraph(pending));
        }
        out
    }

    /// Inline content up to the end tag `is_end` matches.
    fn inlines(&mut self, is_end: impl Fn(&TagEnd) -> bool) -> Vec<Inline> {
        let mut out = Vec::new();
        while let Some((event, _)) = self.events.next() {
            if let Event::End(end) = &event
                && is_end(end)
            {
                break;
            }
            self.inline_event(&event, &mut out);
        }
        out
    }

    /// Apply an inline event; false if it is not one.
    fn inline_event(&mut self, event: &Event<'a>, out: &mut Vec<Inline>) -> bool {
        match event {
            Event::Text(t) => self.inline.text(out, t),
            Event::Code(t) => {
                let style = Style { code: true, ..self.inline.style() };
                out.push(Inline::Text { text: t.to_string(), style, link: self.inline.link });
            }
            Event::InlineMath(t) => {
                let style = Style { code: true, italic: true, ..self.inline.style() };
                out.push(Inline::Text { text: t.to_string(), style, link: self.inline.link });
            }
            Event::SoftBreak => self.inline.text(out, " "),
            Event::HardBreak => self.inline.text(out, "\n"),
            Event::FootnoteReference(label) => {
                let style = Style { small: true, ..self.inline.style() };
                out.push(Inline::Text { text: format!("[{label}]"), style, link: None });
            }
            Event::InlineHtml(h) => {
                if let Some(token) = html::tag(h) {
                    self.inline.apply(&token, &mut self.links, out);
                }
            }
            Event::Start(Tag::Emphasis) => self.inline.set_italic(true),
            Event::End(TagEnd::Emphasis) => self.inline.set_italic(false),
            Event::Start(Tag::Strong) => self.inline.set_bold(true),
            Event::End(TagEnd::Strong) => self.inline.set_bold(false),
            Event::Start(Tag::Strikethrough) => self.inline.set_strike(true),
            Event::End(TagEnd::Strikethrough) => self.inline.set_strike(false),
            Event::Start(Tag::Superscript | Tag::Subscript) => self.inline.set_small(true),
            Event::End(TagEnd::Superscript | TagEnd::Subscript) => self.inline.set_small(false),
            Event::Start(Tag::Link { dest_url, .. }) => {
                self.links.push(dest_url.to_string());
                self.inline.link = Some(self.links.len() as u32 - 1);
            }
            Event::End(TagEnd::Link) => self.inline.link = None,
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                let mut alt = String::new();
                for (event, _) in self.events.by_ref() {
                    match event {
                        Event::Text(t) | Event::Code(t) => alt.push_str(&t),
                        Event::End(TagEnd::Image) => break,
                        _ => {}
                    }
                }
                if alt.is_empty() {
                    alt = title.to_string();
                }
                out.push(Inline::Image { src: dest_url.to_string(), alt, link: self.inline.link });
            }
            _ => return false,
        }
        true
    }
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
    fn a_document() {
        let p = parse(
            "# Title\n\nSome **bold** and *it* with [a link](https://x.org).\n\n\
             - [x] done\n- [ ] todo\n  1. nested\n\n\
             ```rust\nfn main() {}\n```\n\n\
             | A | B |\n|:-:|--:|\n| 1 | 2 |\n\n> quoted\n\n---\n\n![alt text](img.png)\n\n<p>raw <b>html</b></p>\n",
        );
        let b = &p.blocks;
        assert!(matches!(&b[0], Block::Heading { level: 1, inlines } if text(inlines) == "Title"));
        let Block::Paragraph(inlines) = &b[1] else { panic!() };
        assert_eq!(text(inlines), "Some bold and it with a link.");
        assert!(inlines.iter().any(|i| matches!(i, Inline::Text { text, style, .. } if text == "bold" && style.bold)));
        assert!(inlines.iter().any(|i| matches!(i, Inline::Text { text, link: Some(0), .. } if text == "a link")));
        assert_eq!(p.links[0], "https://x.org");
        let Block::List { items, start: None } = &b[2] else { panic!("{:?}", b[2]) };
        assert_eq!(items[0].task, Some(true));
        assert_eq!(items[1].task, Some(false));
        assert!(matches!(items[1].blocks.last(), Some(Block::List { start: Some(1), .. })));
        assert!(matches!(&b[3], Block::Code { language: Some(crate::syntax::LanguageId::Rust), text, .. } if text == "fn main() {}"));
        let Block::Table { aligns, head, rows } = &b[4] else { panic!() };
        assert_eq!(aligns, &[Align::Center, Align::Right]);
        assert_eq!(text(&head[0]), "A");
        assert_eq!(text(&rows[0][1]), "2");
        assert!(matches!(&b[5], Block::Quote(q) if q.len() == 1));
        assert_eq!(b[6], Block::Rule);
        assert!(matches!(&b[7], Block::Paragraph(i) if matches!(&i[0], Inline::Image { src, alt, .. } if src == "img.png" && alt == "alt text")));
        assert!(matches!(&b[8], Block::Paragraph(i) if text(i) == "raw html"));
        assert_eq!(p.lines[..4], [0, 2, 4, 8]);
    }
}
