//! Macros: what the user typed and did, recorded and played back.
//!
//! Recording keeps the editor's input (text, keys with their modifiers, paste, copy, cut) and the
//! commands run from the menus and the find bar ([`Command`]). Playing turns the input back into
//! the same egui events and runs the commands in between, in order.

use serde::{Deserialize, Serialize};

use crate::find::Query;

/// A command a macro can repeat: the editing commands of the menus and the find bar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    Upper,
    Lower,
    ToggleComment,
    DuplicateLines,
    DeleteLines,
    SortAscending,
    SortDescending,
    TrimTrailing,
    Indent,
    Outdent,
    SelectAll,
    JsonPretty,
    JsonMinify,
    XmlPretty,
    XmlMinify,
    GoToLine(usize),
    FindNext { query: Query, backwards: bool },
    ReplaceNext { query: Query, replacement: String },
    ReplaceAll { query: Query, replacement: String },
}

/// One recorded step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Step {
    Text(String),
    /// A key pressed in the editor (egui's key name) with its modifiers.
    Key { key: String, ctrl: bool, shift: bool, alt: bool },
    Paste(String),
    Copy,
    Cut,
    Command(Command),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub steps: Vec<Step>,
    /// Ctrl+Alt+1…9 plays it.
    #[serde(default)]
    pub slot: Option<u8>,
}

/// The steps for the editor's input events of one frame (appended to `steps`, joining typed text).
pub fn record_events(steps: &mut Vec<Step>, events: &[egui::Event]) {
    for event in events {
        let step = match event {
            egui::Event::Text(t) => {
                if let Some(Step::Text(last)) = steps.last_mut() {
                    last.push_str(t);
                    continue;
                }
                Step::Text(t.clone())
            }
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                Step::Key {
                    key: key.name().to_owned(),
                    ctrl: modifiers.ctrl || modifiers.command || modifiers.mac_cmd,
                    shift: modifiers.shift,
                    alt: modifiers.alt,
                }
            }
            egui::Event::Paste(t) => Step::Paste(t.clone()),
            egui::Event::Copy => Step::Copy,
            egui::Event::Cut => Step::Cut,
            _ => continue,
        };
        steps.push(step);
    }
}

/// The egui events that replay an input step (`None` for a command).
pub fn events_for(step: &Step) -> Option<Vec<egui::Event>> {
    Some(match step {
        Step::Text(t) => vec![egui::Event::Text(t.clone())],
        Step::Key { key, ctrl, shift, alt } => {
            let key = egui::Key::from_name(key)?;
            let modifiers = egui::Modifiers { alt: *alt, ctrl: *ctrl, shift: *shift, mac_cmd: false, command: *ctrl };
            vec![
                egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers },
                egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers },
            ]
        }
        Step::Paste(t) => vec![egui::Event::Paste(t.clone())],
        Step::Copy => vec![egui::Event::Copy],
        Step::Cut => vec![egui::Event::Cut],
        Step::Command(_) => return None,
    })
}

/// How many times to play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    Times(usize),
    /// Until a run leaves the cursor at the end of the document, or where it was.
    ToEnd,
}

/// The upper bound of runs of [`Repeat::ToEnd`], against a macro that never moves forward.
pub const MAX_RUNS: usize = 100_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_is_joined_and_keys_replay() {
        let mut steps = Vec::new();
        let ctrl = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
        record_events(
            &mut steps,
            &[
                egui::Event::Text("a".into()),
                egui::Event::Text("b".into()),
                egui::Event::Key { key: egui::Key::Home, physical_key: None, pressed: true, repeat: false, modifiers: ctrl },
                egui::Event::Key { key: egui::Key::Home, physical_key: None, pressed: false, repeat: false, modifiers: ctrl },
                egui::Event::Paste("p".into()),
            ],
        );
        assert_eq!(
            steps,
            vec![
                Step::Text("ab".into()),
                Step::Key { key: "Home".into(), ctrl: true, shift: false, alt: false },
                Step::Paste("p".into()),
            ]
        );
        let events = events_for(&steps[1]).unwrap();
        assert!(matches!(events[0], egui::Event::Key { key: egui::Key::Home, pressed: true, modifiers, .. } if modifiers.ctrl));
        let json = serde_json::to_string(&Macro { name: "m".into(), steps, slot: Some(1) }).unwrap();
        let back: Macro = serde_json::from_str(&json).unwrap();
        assert_eq!(back.slot, Some(1));
    }
}
