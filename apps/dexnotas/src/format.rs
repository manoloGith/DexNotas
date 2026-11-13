//! Formatting and checking structured text: JSON, XML (and HTML, SVG), YAML, TOML.

use std::fmt;

/// Why a document is not valid, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// 1-based, when known.
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(l), Some(c)) => write!(f, "{l}:{c}: {}", self.message),
            (Some(l), None) => write!(f, "{l}: {}", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

fn json_problem(e: serde_json::Error) -> Problem {
    Problem { line: Some(e.line()), column: Some(e.column()), message: strip_position(&e.to_string()) }
}

/// serde's messages end with " at line L column C"; the position is shown separately.
fn strip_position(message: &str) -> String {
    match message.rfind(" at line ") {
        Some(i) => message[..i].to_owned(),
        None => message.to_owned(),
    }
}

/// JSON indented by `indent` spaces, in the original key order, numbers as written.
pub fn json_pretty(text: &str, indent: usize) -> Result<String, Problem> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(json_problem)?;
    let spaces = vec![b' '; indent];
    let formatter = serde_json::ser::PrettyFormatter::with_indent(&spaces);
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    serde::Serialize::serialize(&value, &mut ser).map_err(json_problem)?;
    let mut s = String::from_utf8(out).unwrap_or_default();
    if text.ends_with('\n') {
        s.push('\n');
    }
    Ok(s)
}

/// JSON on one line, without spaces.
pub fn json_minify(text: &str) -> Result<String, Problem> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(json_problem)?;
    serde_json::to_string(&value).map_err(json_problem)
}

pub fn json_check(text: &str) -> Result<(), Problem> {
    serde_json::from_str::<serde::de::IgnoredAny>(text).map(|_| ()).map_err(json_problem)
}

/// XML (or HTML, SVG) with one element per line, indented by `indent` spaces; `indent` 0 puts
/// everything on one line. Text content is trimmed. HTML's unclosed tags (`<br>`) are accepted.
pub fn xml_reformat(text: &str, indent: usize) -> Result<String, Problem> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(text);
    let config = reader.config_mut();
    config.trim_text(true);
    config.check_end_names = false;
    let mut writer = if indent > 0 {
        quick_xml::Writer::new_with_indent(Vec::new(), b' ', indent)
    } else {
        quick_xml::Writer::new(Vec::new())
    };
    loop {
        let event = reader.read_event().map_err(|e| {
            let at = reader.error_position() as usize;
            let (line, column) = crate::find::line_col(text, at);
            Problem { line: Some(line), column: Some(column), message: e.to_string() }
        })?;
        if matches!(event, Event::Eof) {
            break;
        }
        writer
            .write_event(event)
            .map_err(|e| Problem { line: None, column: None, message: e.to_string() })?;
    }
    let mut out = String::from_utf8(writer.into_inner())
        .map_err(|e| Problem { line: None, column: None, message: e.to_string() })?;
    if indent > 0 && text.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

pub fn xml_check(text: &str) -> Result<(), Problem> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(text);
    let mut depth = 0usize;
    loop {
        let event = reader.read_event().map_err(|e| {
            let (line, column) = crate::find::line_col(text, reader.error_position() as usize);
            Problem { line: Some(line), column: Some(column), message: e.to_string() }
        })?;
        match event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Eof => break,
            _ => {}
        }
    }
    if depth > 0 {
        return Err(Problem { line: None, column: None, message: format!("{depth} element(s) not closed") });
    }
    Ok(())
}

pub fn yaml_check(text: &str) -> Result<(), Problem> {
    for document in serde_yaml_ng::Deserializer::from_str(text) {
        let result: Result<serde_yaml_ng::Value, _> = serde::Deserialize::deserialize(document);
        if let Err(e) = result {
            let location = e.location();
            return Err(Problem {
                line: location.as_ref().map(|l| l.line()),
                column: location.as_ref().map(|l| l.column()),
                message: strip_position(&e.to_string()),
            });
        }
    }
    Ok(())
}

pub fn toml_check(text: &str) -> Result<(), Problem> {
    match text.parse::<toml::Table>() {
        Ok(_) => Ok(()),
        Err(e) => {
            let (line, column) = match e.span() {
                Some(span) => {
                    let (l, c) = crate::find::line_col(text, span.start);
                    (Some(l), Some(c))
                }
                None => (None, None),
            };
            let message = e.message().to_owned();
            Err(Problem { line, column, message })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_keeps_order_and_numbers() {
        let pretty = json_pretty("{\"z\":1,\"a\":[1.50,2e3,123456789012345678901234567890]}", 2).unwrap();
        assert_eq!(pretty, "{\n  \"z\": 1,\n  \"a\": [\n    1.50,\n    2e+3,\n    123456789012345678901234567890\n  ]\n}");
        assert_eq!(json_minify(&pretty).unwrap(), "{\"z\":1,\"a\":[1.50,2e+3,123456789012345678901234567890]}");
        assert_eq!(json_pretty("[1]", 4).unwrap(), "[\n    1\n]");
    }

    #[test]
    fn json_errors_have_a_position() {
        let e = json_check("{\n  \"a\": 1,\n  \"b\" 2\n}").unwrap_err();
        assert_eq!(e.line, Some(3));
        assert!(e.message.contains("expected"), "{e}");
    }

    #[test]
    fn xml_reformat_and_minify() {
        let pretty = xml_reformat("<a><b x=\"1\">t</b><c/></a>", 2).unwrap();
        assert_eq!(pretty, "<a>\n  <b x=\"1\">t</b>\n  <c/>\n</a>");
        assert_eq!(xml_reformat(&pretty, 0).unwrap(), "<a><b x=\"1\">t</b><c/></a>");
        assert!(xml_reformat("<p>a<br>b</p>", 2).is_ok());
    }

    #[test]
    fn xml_check_finds_mismatches() {
        assert!(xml_check("<a><b></b></a>").is_ok());
        assert!(xml_check("<a><b></a>").is_err());
        assert!(xml_check("<a>").is_err());
    }

    #[test]
    fn yaml_and_toml() {
        assert!(yaml_check("a: 1\nb: [1, 2]\n").is_ok());
        let e = yaml_check("a: 1\nb: [1, 2\n").unwrap_err();
        assert!(e.line.is_some());
        assert!(toml_check("a = 1\n[t]\nb = \"x\"\n").is_ok());
        let e = toml_check("a = 1\nb = \n").unwrap_err();
        assert_eq!(e.line, Some(2));
    }
}
