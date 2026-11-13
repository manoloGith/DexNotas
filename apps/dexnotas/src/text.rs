//! Text files as bytes and as the editor's text: encodings and line endings.
//!
//! The editor works on a `String` with `\n` line endings. Opening a file finds its encoding
//! (a byte order mark, valid UTF-8, the zero bytes of UTF-16 without a mark, or else the legacy
//! "ANSI" code page) and its line ending; saving writes the text back the same way, or the way the
//! user chose.

use serde::{Deserialize, Serialize};

/// How a file's characters are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    /// A single-byte legacy code page ("ANSI").
    Windows1252,
    Iso8859_1,
    Iso8859_15,
    Windows1250,
    Windows1251,
}

impl Encoding {
    /// Every encoding the menus offer, in their order.
    pub const ALL: [Encoding; 8] = [
        Encoding::Utf8,
        Encoding::Utf16Le,
        Encoding::Utf16Be,
        Encoding::Windows1252,
        Encoding::Iso8859_1,
        Encoding::Iso8859_15,
        Encoding::Windows1250,
        Encoding::Windows1251,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf16Le => "UTF-16 LE",
            Encoding::Utf16Be => "UTF-16 BE",
            Encoding::Windows1252 => "ANSI (Windows-1252)",
            Encoding::Iso8859_1 => "ISO-8859-1",
            Encoding::Iso8859_15 => "ISO-8859-15",
            Encoding::Windows1250 => "Windows-1250",
            Encoding::Windows1251 => "Windows-1251",
        }
    }

    /// The short name for the status bar.
    pub fn short(self) -> &'static str {
        match self {
            Encoding::Windows1252 => "ANSI",
            other => other.label(),
        }
    }

    pub fn is_unicode(self) -> bool {
        matches!(self, Encoding::Utf8 | Encoding::Utf16Le | Encoding::Utf16Be)
    }

    fn legacy(self) -> &'static encoding_rs::Encoding {
        match self {
            Encoding::Iso8859_1 => encoding_rs::WINDOWS_1252, // the WHATWG alias of Latin-1
            Encoding::Iso8859_15 => encoding_rs::ISO_8859_15,
            Encoding::Windows1250 => encoding_rs::WINDOWS_1250,
            Encoding::Windows1251 => encoding_rs::WINDOWS_1251,
            _ => encoding_rs::WINDOWS_1252,
        }
    }

    fn bom(self) -> &'static [u8] {
        match self {
            Encoding::Utf8 => &[0xEF, 0xBB, 0xBF],
            Encoding::Utf16Le => &[0xFF, 0xFE],
            Encoding::Utf16Be => &[0xFE, 0xFF],
            _ => &[],
        }
    }
}

/// How lines end in the file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
    Cr,
}

impl LineEnding {
    pub const ALL: [LineEnding; 3] = [LineEnding::CrLf, LineEnding::Lf, LineEnding::Cr];

    pub fn label(self) -> &'static str {
        match self {
            LineEnding::Lf => "Unix (LF)",
            LineEnding::CrLf => "Windows (CR LF)",
            LineEnding::Cr => "Mac (CR)",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            LineEnding::Lf => "LF",
            LineEnding::CrLf => "CRLF",
            LineEnding::Cr => "CR",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
            LineEnding::Cr => "\r",
        }
    }
}

/// How a file is stored: what [`decode`] found and [`encode`] writes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Format {
    pub encoding: Encoding,
    /// Starts with a byte order mark.
    pub bom: bool,
    pub line_ending: LineEnding,
}

impl Default for Format {
    /// A new document: UTF-8 without a mark, and the platform's line ending.
    fn default() -> Self {
        Format {
            encoding: Encoding::Utf8,
            bom: false,
            line_ending: if cfg!(windows) { LineEnding::CrLf } else { LineEnding::Lf },
        }
    }
}

/// A file read as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    /// With `\n` line endings.
    pub text: String,
    pub format: Format,
    /// Some bytes were not valid in the encoding and became U+FFFD.
    pub lossy: bool,
}

/// Read `bytes`, finding their encoding.
pub fn decode(bytes: &[u8]) -> Decoded {
    let (encoding, bom) = detect(bytes);
    decode_as(bytes, encoding, bom)
}

/// Read `bytes` as `encoding` ("Reopen with encoding"). A byte order mark of that encoding is
/// skipped.
pub fn decode_as(bytes: &[u8], encoding: Encoding, bom: bool) -> Decoded {
    let mark = encoding.bom();
    let has_mark = !mark.is_empty() && bytes.starts_with(mark);
    let body = if has_mark { &bytes[mark.len()..] } else { bytes };
    let (raw, lossy) = match encoding {
        Encoding::Utf8 => match std::str::from_utf8(body) {
            Ok(s) => (s.to_owned(), false),
            Err(_) => (String::from_utf8_lossy(body).into_owned(), true),
        },
        Encoding::Utf16Le | Encoding::Utf16Be => utf16_to_string(body, encoding == Encoding::Utf16Le),
        legacy => {
            let (text, errors) = legacy.legacy().decode_without_bom_handling(body);
            (text.into_owned(), errors)
        }
    };
    let line_ending = detect_line_ending(&raw);
    Decoded {
        text: normalize_newlines(&raw),
        format: Format { encoding, bom: bom || has_mark, line_ending },
        lossy,
    }
}

/// `text` (with `\n` line endings) as the bytes of a file stored as `format`. Characters a legacy
/// code page can't hold become `?`; [`fits`] says whether there are any.
pub fn encode(text: &str, format: Format) -> Vec<u8> {
    let text = with_line_ending(text, format.line_ending);
    let mut out = Vec::with_capacity(text.len() + 3);
    if format.bom && format.encoding.is_unicode() {
        out.extend_from_slice(format.encoding.bom());
    }
    match format.encoding {
        Encoding::Utf8 => out.extend_from_slice(text.as_bytes()),
        Encoding::Utf16Le => text.encode_utf16().for_each(|u| out.extend_from_slice(&u.to_le_bytes())),
        Encoding::Utf16Be => text.encode_utf16().for_each(|u| out.extend_from_slice(&u.to_be_bytes())),
        legacy => {
            // encoding_rs writes HTML numeric references for what doesn't fit; a text editor
            // writes a question mark, as Windows' own conversion does.
            let enc = legacy.legacy();
            let mut buf = [0u8; 4];
            for c in text.chars() {
                let (bytes, _, unmappable) = enc.encode(c.encode_utf8(&mut buf));
                if unmappable {
                    out.push(b'?');
                } else {
                    out.extend_from_slice(&bytes);
                }
            }
        }
    }
    out
}

/// [`encode`] for a rope, a piece at a time (no copy of the whole text first).
pub fn encode_rope(rope: &ropey::Rope, format: Format) -> Vec<u8> {
    let mut out = Vec::with_capacity(rope.len_bytes() + 3);
    let first = Format { bom: format.bom, ..format };
    let rest = Format { bom: false, ..format };
    for (i, chunk) in rope.chunks().enumerate() {
        out.extend_from_slice(&encode(chunk, if i == 0 { first } else { rest }));
    }
    if rope.len_bytes() == 0 {
        out = encode("", first);
    }
    out
}

/// Whether every character of `text` can be stored in `encoding`.
pub fn fits(text: &str, encoding: Encoding) -> bool {
    if encoding.is_unicode() {
        return true;
    }
    let (_, _, unmappable) = encoding.legacy().encode(text);
    !unmappable
}

/// The encoding of `bytes` and whether a byte order mark says so.
fn detect(bytes: &[u8]) -> (Encoding, bool) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return (Encoding::Utf8, true);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return (Encoding::Utf16Le, true);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return (Encoding::Utf16Be, true);
    }
    if let Some(encoding) = utf16_without_bom(bytes) {
        return (encoding, false);
    }
    if std::str::from_utf8(bytes).is_ok() {
        return (Encoding::Utf8, false);
    }
    // Not UTF-8: a legacy code page. Windows-1252 is what "ANSI" means for Western languages.
    (Encoding::Windows1252, false)
}

/// UTF-16 without a byte order mark: text in Latin script has a zero in every other byte.
fn utf16_without_bom(bytes: &[u8]) -> Option<Encoding> {
    let sample = &bytes[..bytes.len().min(4096) & !1];
    if sample.len() < 4 {
        return None;
    }
    let pairs = sample.len() / 2;
    let even_zeros = sample.iter().step_by(2).filter(|b| **b == 0).count();
    let odd_zeros = sample.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
    // Most code units of one side zero, almost none of the other.
    if odd_zeros * 10 >= pairs * 6 && even_zeros * 10 <= pairs {
        Some(Encoding::Utf16Le)
    } else if even_zeros * 10 >= pairs * 6 && odd_zeros * 10 <= pairs {
        Some(Encoding::Utf16Be)
    } else {
        None
    }
}

fn utf16_to_string(bytes: &[u8], little_endian: bool) -> (String, bool) {
    let units = bytes.chunks_exact(2).map(|p| {
        if little_endian { u16::from_le_bytes([p[0], p[1]]) } else { u16::from_be_bytes([p[0], p[1]]) }
    });
    let mut lossy = !bytes.len().is_multiple_of(2);
    let mut text = String::with_capacity(bytes.len() / 2);
    for c in char::decode_utf16(units) {
        match c {
            Ok(c) => text.push(c),
            Err(_) => {
                lossy = true;
                text.push(char::REPLACEMENT_CHARACTER);
            }
        }
    }
    if !bytes.len().is_multiple_of(2) {
        text.push(char::REPLACEMENT_CHARACTER);
    }
    (text, lossy)
}

/// The line ending most lines of `text` use (LF when there are none).
pub fn detect_line_ending(text: &str) -> LineEnding {
    let bytes = text.as_bytes();
    let (mut lf, mut crlf, mut cr) = (0usize, 0usize, 0usize);
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                crlf += 1;
                i += 1;
            }
            b'\r' => cr += 1,
            b'\n' => lf += 1,
            _ => {}
        }
        i += 1;
    }
    if crlf >= lf && crlf >= cr && crlf > 0 {
        LineEnding::CrLf
    } else if cr > lf {
        LineEnding::Cr
    } else {
        LineEnding::Lf
    }
}

/// `text` with every line ending (CR LF, CR) made `\n`.
pub fn normalize_newlines(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

fn with_line_ending(text: &str, ending: LineEnding) -> std::borrow::Cow<'_, str> {
    if ending == LineEnding::Lf || !text.contains('\n') {
        std::borrow::Cow::Borrowed(text)
    } else {
        std::borrow::Cow::Owned(text.replace('\n', ending.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(encoding: Encoding, bom: bool, line_ending: LineEnding) -> Format {
        Format { encoding, bom, line_ending }
    }

    #[test]
    fn utf8_with_and_without_bom() {
        let d = decode("año\r\nñu".as_bytes());
        assert_eq!(d.text, "año\nñu");
        assert_eq!(d.format, format(Encoding::Utf8, false, LineEnding::CrLf));
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("hola\n".as_bytes());
        let d = decode(&bytes);
        assert_eq!(d.text, "hola\n");
        assert_eq!(d.format, format(Encoding::Utf8, true, LineEnding::Lf));
        assert_eq!(encode(&d.text, d.format), bytes);
    }

    #[test]
    fn utf16_with_bom_round_trips() {
        for (encoding, little) in [(Encoding::Utf16Le, true), (Encoding::Utf16Be, false)] {
            let f = format(encoding, true, LineEnding::CrLf);
            let bytes = encode("¿Qué?\n€ 😀", f);
            assert_eq!(bytes[..2], if little { [0xFF, 0xFE] } else { [0xFE, 0xFF] });
            let d = decode(&bytes);
            assert_eq!(d.text, "¿Qué?\n€ 😀");
            assert_eq!(d.format, f);
            assert!(!d.lossy);
        }
    }

    #[test]
    fn utf16_without_bom_is_recognised() {
        let f = format(Encoding::Utf16Le, false, LineEnding::Lf);
        let d = decode(&encode("plain ASCII text in UTF-16\n", f));
        assert_eq!(d.format.encoding, Encoding::Utf16Le);
        assert_eq!(d.text, "plain ASCII text in UTF-16\n");
        let f = format(Encoding::Utf16Be, false, LineEnding::Lf);
        assert_eq!(decode(&encode("more text here", f)).format.encoding, Encoding::Utf16Be);
    }

    #[test]
    fn ansi_is_the_fallback_and_round_trips() {
        let bytes = b"Espa\xf1a \x80 caf\xe9";
        let d = decode(bytes);
        assert_eq!(d.format.encoding, Encoding::Windows1252);
        assert_eq!(d.text, "España € café");
        assert_eq!(encode(&d.text, d.format), bytes);
    }

    #[test]
    fn what_ansi_cannot_hold_becomes_a_question_mark() {
        let f = format(Encoding::Windows1252, false, LineEnding::Lf);
        assert!(!fits("日本", Encoding::Windows1252));
        assert!(fits("café", Encoding::Windows1252));
        assert_eq!(encode("a日b", f), b"a?b");
    }

    #[test]
    fn reopen_as_another_encoding() {
        let bytes = "café".as_bytes();
        let d = decode_as(bytes, Encoding::Windows1252, false);
        assert_eq!(d.text, "cafÃ©");
    }

    #[test]
    fn line_endings() {
        assert_eq!(detect_line_ending("a\nb\n"), LineEnding::Lf);
        assert_eq!(detect_line_ending("a\r\nb\r\nc\n"), LineEnding::CrLf);
        assert_eq!(detect_line_ending("a\rb\rc"), LineEnding::Cr);
        assert_eq!(normalize_newlines("a\r\nb\rc\n"), "a\nb\nc\n");
        assert_eq!(encode("a\nb", format(Encoding::Utf8, false, LineEnding::CrLf)), b"a\r\nb");
        assert_eq!(encode("a\nb", format(Encoding::Utf8, false, LineEnding::Cr)), b"a\rb");
    }

    #[test]
    fn rope_encoding_matches() {
        let text = "línea ñ\n".repeat(3000);
        let rope = ropey::Rope::from_str(&text);
        for f in [format(Encoding::Utf8, true, LineEnding::CrLf), format(Encoding::Utf16Be, true, LineEnding::Lf), format(Encoding::Windows1252, false, LineEnding::Cr)] {
            assert_eq!(encode_rope(&rope, f), encode(&text, f));
        }
    }

    #[test]
    fn invalid_bytes_are_reported() {
        let d = decode_as(b"ok \xff", Encoding::Utf8, false);
        assert!(d.lossy);
        assert_eq!(d.text, "ok \u{FFFD}");
    }
}
