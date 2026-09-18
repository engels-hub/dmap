//! The keys of the DM window, and the ones the DM put in their place.
//!
//! Every control a key press starts lives here with its default key. A
//! screen asks this module whether a press means a control, and never
//! names a key itself, so a key the DM changes in Settings changes
//! everywhere at once. Controls that ride on a drag, such as `Ctrl` for a
//! free move, are not here: they are part of a gesture, not a press.
//! Issue #36.

// Rust guideline compliant 2026-02-21

use std::collections::BTreeMap;
use std::fmt;

use crate::text;

/// A control the DM starts with one press of a key.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Takes the last change to the scene back.
    Undo,
    /// Writes the last change the DM took back again.
    Redo,
    /// Steps the DM camera in.
    ZoomIn,
    /// Steps the DM camera out.
    ZoomOut,
    /// Back to the zoom a new project opens with.
    ZoomReset,
    /// Puts the whole TV box on the DM screen.
    FrameBox,
    /// Holds what the TV shows, and lets it go. Issue #76.
    Freeze,
    /// Opens this list of keys.
    Shortcuts,
    /// Turns what the DM holds a quarter.
    Turn,
    /// Flips what the DM holds left to right.
    FlipAcross,
    /// Flips what the DM holds top to bottom.
    FlipDown,
    /// Grows what the DM holds a tenth.
    Grow,
    /// Shrinks what the DM holds a tenth.
    Shrink,
    /// Moves what the DM holds one step up the stack.
    Raise,
    /// Moves what the DM holds one step down the stack.
    Lower,
    /// Takes what the DM holds out of the scene.
    Delete,
    /// Moves the TV box one cell left.
    BoxLeft,
    /// Moves the TV box one cell right.
    BoxRight,
    /// Moves the TV box one cell up.
    BoxUp,
    /// Moves the TV box one cell down.
    BoxDown,
}

/// Every control, in the order the list shows them.
pub const ACTIONS: [Action; 20] = [
    Action::Undo,
    Action::Redo,
    Action::ZoomIn,
    Action::ZoomOut,
    Action::ZoomReset,
    Action::FrameBox,
    Action::Freeze,
    Action::Shortcuts,
    Action::Turn,
    Action::FlipAcross,
    Action::FlipDown,
    Action::Grow,
    Action::Shrink,
    Action::Raise,
    Action::Lower,
    Action::Delete,
    Action::BoxLeft,
    Action::BoxRight,
    Action::BoxUp,
    Action::BoxDown,
];

/// The part of the window a control works in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Every view.
    Every,
    /// The Select view.
    Select,
    /// The Draw view.
    Draw,
    /// The Table view.
    Table,
}

impl Group {
    /// The name the list gives the group.
    pub fn label(self) -> &'static str {
        match self {
            Self::Every => text::keys_group_every(),
            Self::Select => text::keys_group_select(),
            Self::Draw => text::keys_group_draw(),
            Self::Table => text::keys_group_table(),
        }
    }
}

impl Action {
    /// What the control does, as the list says it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Undo => text::keys_undo(),
            Self::Redo => text::keys_redo(),
            Self::ZoomIn => text::keys_zoom_in(),
            Self::ZoomOut => text::keys_zoom_out(),
            Self::ZoomReset => text::keys_zoom_reset(),
            Self::FrameBox => text::keys_frame_box(),
            Self::Freeze => text::keys_freeze(),
            Self::Shortcuts => text::keys_shortcuts(),
            Self::Turn => text::keys_turn(),
            Self::FlipAcross => text::keys_flip_across(),
            Self::FlipDown => text::keys_flip_down(),
            Self::Grow => text::keys_grow(),
            Self::Shrink => text::keys_shrink(),
            Self::Raise => text::keys_raise(),
            Self::Lower => text::keys_lower(),
            Self::Delete => text::keys_delete(),
            Self::BoxLeft => text::keys_box_left(),
            Self::BoxRight => text::keys_box_right(),
            Self::BoxUp => text::keys_box_up(),
            Self::BoxDown => text::keys_box_down(),
        }
    }

    /// The part of the window the control works in.
    pub fn group(self) -> Group {
        match self {
            Self::Undo
            | Self::Redo
            | Self::ZoomIn
            | Self::ZoomOut
            | Self::ZoomReset
            | Self::FrameBox
            | Self::Freeze
            | Self::Shortcuts => Group::Every,
            Self::Turn
            | Self::FlipAcross
            | Self::FlipDown
            | Self::Grow
            | Self::Shrink
            | Self::Raise
            | Self::Lower
            | Self::Delete => Group::Select,
            Self::BoxLeft | Self::BoxRight | Self::BoxUp | Self::BoxDown => Group::Table,
        }
    }

    /// The keys a first run gives the control.
    ///
    /// Redo and Delete answer to two keys each, for a DM who learned them
    /// in another program.
    fn defaults(self) -> Vec<Chord> {
        use egui::Key;
        match self {
            Self::Undo => vec![Chord::ctrl(Key::Z)],
            Self::Redo => vec![
                Chord {
                    shift: true,
                    ..Chord::ctrl(Key::Z)
                },
                Chord::ctrl(Key::Y),
            ],
            Self::ZoomIn => vec![Chord::ctrl(Key::Plus)],
            Self::ZoomOut => vec![Chord::ctrl(Key::Minus)],
            Self::ZoomReset => vec![Chord::ctrl(Key::Num0)],
            Self::FrameBox => vec![Chord::plain(Key::T)],
            // `P` stands for the `pause` glyph of the toolbar entry. `F`
            // would stand for freeze, and a flip holds that key already.
            Self::Freeze => vec![Chord::plain(Key::P)],
            Self::Shortcuts => vec![Chord::plain(Key::Questionmark)],
            Self::Turn => vec![Chord::plain(Key::R)],
            Self::FlipAcross => vec![Chord::plain(Key::F)],
            Self::FlipDown => vec![Chord {
                shift: true,
                ..Chord::plain(Key::F)
            }],
            Self::Grow => vec![Chord::plain(Key::Plus)],
            Self::Shrink => vec![Chord::plain(Key::Minus)],
            Self::Raise => vec![Chord::plain(Key::PageUp)],
            Self::Lower => vec![Chord::plain(Key::PageDown)],
            Self::Delete => vec![Chord::plain(Key::Delete), Chord::plain(Key::Backspace)],
            Self::BoxLeft => vec![Chord::plain(Key::ArrowLeft)],
            Self::BoxRight => vec![Chord::plain(Key::ArrowRight)],
            Self::BoxUp => vec![Chord::plain(Key::ArrowUp)],
            Self::BoxDown => vec![Chord::plain(Key::ArrowDown)],
        }
    }

    /// Whether a key held down repeats the control.
    ///
    /// The arrows walk the box across the canvas. Every other control does
    /// one thing for one press: one turn, one flip, one step of the stack.
    fn repeats(self) -> bool {
        matches!(
            self,
            Self::BoxLeft | Self::BoxRight | Self::BoxUp | Self::BoxDown
        )
    }
}

/// A key with the modifiers held with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    /// The key itself.
    pub key: egui::Key,
    /// `Ctrl`, or `Cmd` on a Mac.
    pub ctrl: bool,
    /// `Shift`.
    pub shift: bool,
    /// `Alt`.
    pub alt: bool,
}

impl Chord {
    /// The key alone.
    const fn plain(key: egui::Key) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    /// The key with `Ctrl`.
    const fn ctrl(key: egui::Key) -> Self {
        Self {
            ctrl: true,
            ..Self::plain(key)
        }
    }

    /// The chord a press of `key` with `modifiers` makes.
    ///
    /// `+` sits on the numpad and on the `=` key of the main row. egui
    /// names the one `Plus` and the other `Equals`, and both keep the job
    /// of `+`. A symbol that takes `Shift` to type, such as `?`, answers
    /// with `Shift` or without it, because the DM thinks of the symbol and
    /// not of the hand that makes it.
    pub fn of(key: egui::Key, modifiers: egui::Modifiers) -> Self {
        let key = if key == egui::Key::Equals {
            egui::Key::Plus
        } else {
            key
        };
        Self {
            key,
            ctrl: modifiers.command,
            shift: modifiers.shift && !shifted_symbol(key),
            alt: modifiers.alt,
        }
    }

    /// Whether a press of `key` with `modifiers` makes this chord.
    fn made_by(self, key: egui::Key, modifiers: egui::Modifiers) -> bool {
        Self::of(key, modifiers) == self
    }

    /// The chord as the config file holds it, such as `Ctrl+Shift+Z`.
    fn stored(self) -> String {
        let mut out = String::new();
        for (on, name) in [
            (self.ctrl, "Ctrl"),
            (self.shift, "Shift"),
            (self.alt, "Alt"),
        ] {
            if on {
                out.push_str(name);
                out.push('+');
            }
        }
        out.push_str(self.key.name());
        out
    }

    /// Reads a chord the config file holds, or `None` for a key egui
    /// does not know.
    fn parse(stored: &str) -> Option<Self> {
        let mut parts: Vec<&str> = stored.split('+').collect();
        let key = egui::Key::from_name(parts.pop()?)?;
        let mut chord = Self::plain(key);
        for part in parts {
            match part {
                "Ctrl" => chord.ctrl = true,
                "Shift" => chord.shift = true,
                "Alt" => chord.alt = true,
                _ => return None,
            }
        }
        Some(Self::of(
            chord.key,
            egui::Modifiers {
                alt: chord.alt,
                shift: chord.shift,
                command: chord.ctrl,
                ctrl: chord.ctrl,
                ..egui::Modifiers::NONE
            },
        ))
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.ctrl, "Ctrl"),
            (self.shift, "Shift"),
            (self.alt, "Alt"),
        ] {
            if on {
                write!(f, "{name} + ")?;
            }
        }
        f.write_str(key_name(self.key))
    }
}

/// How the list names a key.
pub fn key_name(key: egui::Key) -> &'static str {
    match key {
        egui::Key::PageUp => "Page Up",
        egui::Key::PageDown => "Page Down",
        egui::Key::Backspace => "Backspace",
        egui::Key::Delete => "Delete",
        other => other.symbol_or_name(),
    }
}

/// Whether the key names a symbol that takes `Shift` to type.
fn shifted_symbol(key: egui::Key) -> bool {
    use egui::Key;
    matches!(
        key,
        Key::Plus
            | Key::Questionmark
            | Key::Colon
            | Key::Pipe
            | Key::Exclamationmark
            | Key::OpenCurlyBracket
            | Key::CloseCurlyBracket
    )
}

/// Why the program refused a key for a control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    /// Another control holds the key.
    By(Action),
    /// The key belongs to a gesture or a dialog, and the DM cannot move it.
    Fixed(&'static str),
}

impl Taken {
    /// The name of whatever holds the key.
    pub fn holder(self) -> &'static str {
        match self {
            Self::By(action) => action.label(),
            Self::Fixed(label) => label,
        }
    }
}

/// The keys of every control: the defaults, and the ones the DM chose.
///
/// The config file holds only the controls the DM changed, so a new
/// control a later version brings arrives with its default key.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(
    from = "BTreeMap<Action, Vec<String>>",
    into = "BTreeMap<Action, Vec<String>>"
)]
pub struct Keys {
    chosen: BTreeMap<Action, Vec<Chord>>,
}

impl From<BTreeMap<Action, Vec<String>>> for Keys {
    /// Reads the keys the config file holds.
    ///
    /// A key this version cannot read drops out, so a file from another
    /// version costs the DM one key and not every setting of the file.
    fn from(stored: BTreeMap<Action, Vec<String>>) -> Self {
        let chosen = stored
            .into_iter()
            .map(|(action, chords)| {
                let chords: Vec<Chord> = chords.iter().filter_map(|c| Chord::parse(c)).collect();
                (action, chords)
            })
            .filter(|(_, chords)| !chords.is_empty())
            .collect();
        Self { chosen }
    }
}

impl From<Keys> for BTreeMap<Action, Vec<String>> {
    fn from(keys: Keys) -> Self {
        keys.chosen
            .into_iter()
            .map(|(action, chords)| (action, chords.iter().map(|c| c.stored()).collect()))
            .collect()
    }
}

impl Keys {
    /// The keys that start `action` now.
    pub fn of(&self, action: Action) -> Vec<Chord> {
        self.chosen
            .get(&action)
            .cloned()
            .unwrap_or_else(|| action.defaults())
    }

    /// Whether the DM changed no key.
    pub fn is_default(&self) -> bool {
        self.chosen.is_empty()
    }

    /// Brings back the default key of every control.
    pub fn reset(&mut self) {
        self.chosen.clear();
    }

    /// Puts `chord` on `action`, in place of the keys it had.
    ///
    /// # Errors
    ///
    /// Returns what holds the key when another control holds it, or when
    /// it belongs to a gesture or a dialog. The keys stay as they were.
    pub fn set(&mut self, action: Action, chord: Chord) -> Result<(), Taken> {
        if let Some(label) = fixed(chord) {
            return Err(Taken::Fixed(label));
        }
        if let Some(other) = ACTIONS
            .into_iter()
            .find(|other| *other != action && self.of(*other).contains(&chord))
        {
            return Err(Taken::By(other));
        }
        if action.defaults() == [chord] {
            self.chosen.remove(&action);
        } else {
            self.chosen.insert(action, vec![chord]);
        }
        Ok(())
    }

    /// The control among `among` that a press of `key` starts, if any.
    pub fn action_of(
        &self,
        key: egui::Key,
        modifiers: egui::Modifiers,
        repeat: bool,
        among: &[Action],
    ) -> Option<Action> {
        among.iter().copied().find(|action| {
            (!repeat || action.repeats())
                && self
                    .of(*action)
                    .iter()
                    .any(|chord| chord.made_by(key, modifiers))
        })
    }

    /// Takes every press of `action` out of this frame, and counts them.
    ///
    /// A press this takes reaches nothing else, so a text field never sees
    /// the `Z` of an undo.
    pub fn take(&self, input: &mut egui::InputState, action: Action) -> usize {
        let before = input.events.len();
        input.events.retain(|event| {
            let egui::Event::Key {
                key,
                pressed: true,
                repeat,
                modifiers,
                ..
            } = event
            else {
                return true;
            };
            self.action_of(*key, *modifiers, *repeat, &[action])
                .is_none()
        });
        before - input.events.len()
    }

    /// Whether `action` was pressed this frame. Takes the press.
    pub fn pressed(&self, input: &mut egui::InputState, action: Action) -> bool {
        self.take(input, action) > 0
    }
}

/// The name of the fixed control that holds `chord`, if one does.
///
/// `Escape` gives up a measure and closes a dialog, and `Space` with a
/// drag pans the canvas. `Enter` and `Tab` belong to the text fields. A
/// control on any of them would fight what the key does already.
fn fixed(chord: Chord) -> Option<&'static str> {
    match chord.key {
        egui::Key::Escape => Some(text::keys_fixed_escape()),
        egui::Key::Space => Some(text::keys_fixed_pan()),
        egui::Key::Enter | egui::Key::Tab => Some(text::keys_fixed_field()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(shift: bool, ctrl: bool) -> egui::Modifiers {
        egui::Modifiers {
            shift,
            ctrl,
            command: ctrl,
            ..egui::Modifiers::NONE
        }
    }

    #[test]
    fn both_plus_keys_grow() {
        let keys = Keys::default();
        let numpad = keys.action_of(egui::Key::Plus, egui::Modifiers::NONE, false, &ACTIONS);
        let main_row = keys.action_of(egui::Key::Equals, egui::Modifiers::NONE, false, &ACTIONS);
        let shifted = keys.action_of(egui::Key::Plus, held(true, false), false, &ACTIONS);
        assert_eq!(numpad, Some(Action::Grow));
        assert_eq!(main_row, Some(Action::Grow));
        assert_eq!(shifted, Some(Action::Grow));
    }

    #[test]
    fn shift_turns_one_flip_into_the_other() {
        let keys = Keys::default();
        let across = keys.action_of(egui::Key::F, egui::Modifiers::NONE, false, &ACTIONS);
        let down = keys.action_of(egui::Key::F, held(true, false), false, &ACTIONS);
        assert_eq!(across, Some(Action::FlipAcross));
        assert_eq!(down, Some(Action::FlipDown));
    }

    #[test]
    fn redo_and_undo_do_not_share_a_press() {
        let keys = Keys::default();
        let undo = held(false, true);
        let redo = held(true, true);
        assert_eq!(
            keys.action_of(egui::Key::Z, undo, false, &ACTIONS),
            Some(Action::Undo)
        );
        assert_eq!(
            keys.action_of(egui::Key::Z, redo, false, &ACTIONS),
            Some(Action::Redo)
        );
    }

    #[test]
    fn a_held_key_repeats_the_arrows_alone() {
        let keys = Keys::default();
        let none = egui::Modifiers::NONE;
        assert_eq!(keys.action_of(egui::Key::R, none, true, &ACTIONS), None);
        assert_eq!(
            keys.action_of(egui::Key::ArrowLeft, none, true, &ACTIONS),
            Some(Action::BoxLeft)
        );
    }

    #[test]
    fn a_key_another_control_holds_is_refused() {
        let mut keys = Keys::default();
        let taken = keys.set(Action::Freeze, Chord::plain(egui::Key::R));
        assert_eq!(taken, Err(Taken::By(Action::Turn)));
        assert_eq!(keys.of(Action::Freeze), vec![Chord::plain(egui::Key::P)]);
    }

    #[test]
    fn a_fixed_key_is_refused() {
        let mut keys = Keys::default();
        let taken = keys.set(Action::Freeze, Chord::plain(egui::Key::Space));
        assert!(matches!(taken, Err(Taken::Fixed(_))));
    }

    #[test]
    fn a_new_key_replaces_the_old_one() {
        let mut keys = Keys::default();
        assert_eq!(keys.set(Action::Freeze, Chord::plain(egui::Key::B)), Ok(()));
        let none = egui::Modifiers::NONE;
        assert_eq!(
            keys.action_of(egui::Key::B, none, false, &ACTIONS),
            Some(Action::Freeze)
        );
        assert_eq!(keys.action_of(egui::Key::P, none, false, &ACTIONS), None);
        keys.reset();
        assert!(keys.is_default());
        assert_eq!(
            keys.action_of(egui::Key::P, none, false, &ACTIONS),
            Some(Action::Freeze)
        );
    }

    #[test]
    fn the_default_key_again_leaves_nothing_in_the_file() {
        let mut keys = Keys::default();
        keys.set(Action::Turn, Chord::plain(egui::Key::B)).unwrap();
        keys.set(Action::Turn, Chord::plain(egui::Key::R)).unwrap();
        assert!(keys.is_default());
    }

    #[test]
    fn the_config_file_keeps_a_chosen_key() {
        let mut keys = Keys::default();
        let chord = Chord {
            shift: true,
            ..Chord::ctrl(egui::Key::K)
        };
        keys.set(Action::Freeze, chord).unwrap();
        let json = serde_json::to_string(&keys).unwrap();
        assert_eq!(json, r#"{"freeze":["Ctrl+Shift+K"]}"#);
        let back: Keys = serde_json::from_str(&json).unwrap();
        assert_eq!(back, keys);
    }

    #[test]
    fn a_key_this_version_cannot_read_drops_out() {
        let back: Keys = serde_json::from_str(r#"{"freeze":["Hyper+Q"],"turn":["B"]}"#).unwrap();
        assert_eq!(back.of(Action::Freeze), vec![Chord::plain(egui::Key::P)]);
        assert_eq!(back.of(Action::Turn), vec![Chord::plain(egui::Key::B)]);
    }

    #[test]
    fn the_list_names_a_chord_as_the_dm_reads_it() {
        let chord = Chord {
            shift: true,
            ..Chord::ctrl(egui::Key::Z)
        };
        assert_eq!(chord.to_string(), "Ctrl + Shift + Z");
    }
}
