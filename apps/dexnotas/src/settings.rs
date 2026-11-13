//! The user's choices, kept in `settings.json`.

use serde::{Deserialize, Serialize};

use crate::i18n::Language;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    pub dark: bool,
    pub word_wrap: bool,
    pub line_numbers: bool,
    pub highlight_line: bool,
    /// The editor's font size, in points.
    pub font_size: f32,
    /// Tab inserts spaces instead of a tab character.
    pub indent_spaces: bool,
    /// Spaces per indent (also the indent of formatted JSON and XML).
    pub indent_width: usize,
    /// Enter keeps the indent of the line above.
    pub auto_indent: bool,
    /// Also write documents that have a file to it, a moment after each change.
    pub autosave_files: bool,
    /// The editor's share of the window when the preview is beside it (or below it).
    pub preview_split: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: Language::Auto,
            dark: true,
            word_wrap: false,
            line_numbers: true,
            highlight_line: true,
            font_size: 14.0,
            indent_spaces: true,
            indent_width: 4,
            auto_indent: true,
            autosave_files: false,
            preview_split: 0.5,
        }
    }
}

impl Settings {
    pub const MIN_FONT: f32 = 8.0;
    pub const MAX_FONT: f32 = 40.0;

    pub fn indent_unit(&self) -> String {
        if self.indent_spaces { " ".repeat(self.indent_width.clamp(1, 8)) } else { "\t".to_owned() }
    }
}
