//! Colours: the window and the syntax highlighting, dark and light.

use egui::Color32;

use crate::syntax::Kind;

#[derive(Clone, Copy)]
pub struct Palette {
    pub editor_bg: Color32,
    pub text: Color32,
    pub gutter_bg: Color32,
    pub gutter_text: Color32,
    pub gutter_current: Color32,
    pub current_line: Color32,
    pub bar: Color32,
    pub separator: Color32,
    pub find_match: Color32,
    pub find_current: Color32,
    pub error: Color32,
    pub recording: Color32,
}

pub fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            editor_bg: Color32::from_rgb(0x1e, 0x1f, 0x22),
            text: Color32::from_rgb(0xd4, 0xd4, 0xd4),
            gutter_bg: Color32::from_rgb(0x1e, 0x1f, 0x22),
            gutter_text: Color32::from_rgb(0x6e, 0x76, 0x81),
            gutter_current: Color32::from_rgb(0xc8, 0xc8, 0xc8),
            current_line: Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 10),
            bar: Color32::from_rgb(0x2b, 0x2d, 0x30),
            separator: Color32::from_rgb(0x3c, 0x3f, 0x41),
            find_match: Color32::from_rgba_unmultiplied(0xe2, 0xc0, 0x8d, 60),
            find_current: Color32::from_rgba_unmultiplied(0xf0, 0xa0, 0x30, 140),
            error: Color32::from_rgb(0xf4, 0x87, 0x71),
            recording: Color32::from_rgb(0xe5, 0x48, 0x4d),
        }
    } else {
        Palette {
            editor_bg: Color32::from_rgb(0xff, 0xff, 0xff),
            text: Color32::from_rgb(0x1f, 0x1f, 0x1f),
            gutter_bg: Color32::from_rgb(0xf5, 0xf5, 0xf5),
            gutter_text: Color32::from_rgb(0x99, 0x99, 0x99),
            gutter_current: Color32::from_rgb(0x33, 0x33, 0x33),
            current_line: Color32::from_rgba_unmultiplied(0x00, 0x60, 0xc0, 14),
            bar: Color32::from_rgb(0xf0, 0xf0, 0xf0),
            separator: Color32::from_rgb(0xd6, 0xd6, 0xd6),
            find_match: Color32::from_rgba_unmultiplied(0xff, 0xd0, 0x40, 90),
            find_current: Color32::from_rgba_unmultiplied(0xff, 0x90, 0x00, 150),
            error: Color32::from_rgb(0xc0, 0x30, 0x20),
            recording: Color32::from_rgb(0xd0, 0x20, 0x30),
        }
    }
}

/// The colour of a kind of code.
pub fn syntax(kind: Kind, dark: bool) -> Color32 {
    let rgb = if dark {
        match kind {
            Kind::Keyword => 0xc6_78_dd,
            Kind::String => 0x98_c3_79,
            Kind::Comment => 0x7f_84_8e,
            Kind::Number => 0xd1_9a_66,
            Kind::Type => 0xe5_c0_7b,
            Kind::Function => 0x61_af_ef,
            Kind::Constant => 0xd1_9a_66,
            Kind::Property => 0xe0_6c_75,
            Kind::Attribute => 0x56_b6_c2,
        }
    } else {
        match kind {
            Kind::Keyword => 0xa6_26_a4,
            Kind::String => 0x50_a1_4f,
            Kind::Comment => 0x8a_8f_98,
            Kind::Number => 0x98_68_01,
            Kind::Type => 0xc1_84_01,
            Kind::Function => 0x40_78_f2,
            Kind::Constant => 0x98_68_01,
            Kind::Property => 0xe4_56_49,
            Kind::Attribute => 0x01_84_bc,
        }
    };
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}
