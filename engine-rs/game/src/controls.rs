//! The keys of the menus and of the sound volume, which the player can change like the note keys.
use crate::gfx::{is_key_down, is_key_pressed, KeyCode};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ui {
    Left,
    Down,
    Up,
    Right,
    Accept,
    Back,
    Pause,
    Reset,
    VolumeMute,
    VolumeUp,
    VolumeDown,
    /// The debug keys: the chart editor from a song (and the editors from the main menu), the character editor.
    Debug1,
    Debug2,
}

/// Two keys for each action, in the order of `Ui`.
pub type Binds = [[Option<KeyCode>; 2]; 13];

/// Each action's name in the settings file (the Haxe engine's) and in the options.
pub const NAMES: [(&str, &str); 13] = [
    ("ui_left", "Left"), ("ui_down", "Down"), ("ui_up", "Up"), ("ui_right", "Right"), ("accept", "Accept"), ("back", "Back"), ("pause", "Pause"), ("reset", "Reset"),
    ("volume_mute", "Volume Mute"), ("volume_up", "Volume Up"), ("volume_down", "Volume Down"), ("debug_1", "Debug Key #1"), ("debug_2", "Debug Key #2"),
];

/// The Haxe engine's keys.
pub fn defaults() -> Binds {
    use KeyCode::*;
    [[A, Left], [S, Down], [W, Up], [D, Right], [Space, Enter], [Backspace, Escape], [Enter, Escape], [R, R], [Key0, Key0], [KpAdd, Equal], [KpSubtract, Minus], [Key7, Key7], [Key8, Key8]].map(|keys| keys.map(Some))
}

thread_local! {
    static BINDS: Cell<Binds> = Cell::new(defaults());
}

/// The keys in use from now on.
pub fn set(binds: Binds) {
    BINDS.with(|kept| kept.set(binds));
}

/// The keys of an action. One left with no key at all keeps its usual ones: no menu can be locked.
pub fn keys(action: Ui) -> [Option<KeyCode>; 2] {
    let bound = BINDS.with(Cell::get)[action as usize];
    if bound == [None, None] { defaults()[action as usize] } else { bound }
}

/// Pressed this frame: by one of its keys, or one of its gamepad buttons.
pub fn pressed(action: Ui) -> bool {
    keys(action).into_iter().flatten().any(is_key_pressed) || crate::pad::action(action as usize).into_iter().flatten().any(crate::pad::pressed)
}

pub fn down(action: Ui) -> bool {
    keys(action).into_iter().flatten().any(is_key_down) || crate::pad::action(action as usize).into_iter().flatten().any(crate::pad::down)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_without_keys_keeps_its_usual_ones() {
        let mut binds = defaults();
        binds[Ui::Back as usize] = [None, None];
        binds[Ui::Accept as usize] = [Some(KeyCode::Z), None];
        set(binds);
        assert_eq!(keys(Ui::Back), [Some(KeyCode::Backspace), Some(KeyCode::Escape)]);
        assert_eq!(keys(Ui::Accept), [Some(KeyCode::Z), None]);
        assert_eq!(NAMES.len(), defaults().len());
        set(defaults());
    }
}

/// A key's name in the Haxe engine (Flixel's), which mods' settings keep keys by; none for a key it has no name for.
pub fn flx_name(key: KeyCode) -> Option<String> {
    use KeyCode::*;
    const DIGITS: [&str; 10] = ["ZERO", "ONE", "TWO", "THREE", "FOUR", "FIVE", "SIX", "SEVEN", "EIGHT", "NINE"];
    let name = format!("{key:?}");
    Some(match key {
        Key0 | Key1 | Key2 | Key3 | Key4 | Key5 | Key6 | Key7 | Key8 | Key9 => DIGITS[name[3..].parse::<usize>().ok()?].to_owned(),
        Kp0 | Kp1 | Kp2 | Kp3 | Kp4 | Kp5 | Kp6 | Kp7 | Kp8 | Kp9 => format!("NUMPAD{}", DIGITS[name[2..].parse::<usize>().ok()?]),
        KpAdd => "NUMPADPLUS".to_owned(),
        KpSubtract => "NUMPADMINUS".to_owned(),
        LeftShift | RightShift => "SHIFT".to_owned(),
        LeftControl | RightControl => "CONTROL".to_owned(),
        LeftAlt | RightAlt => "ALT".to_owned(),
        Apostrophe => "QUOTE".to_owned(),
        LeftBracket => "LBRACKET".to_owned(),
        RightBracket => "RBRACKET".to_owned(),
        Equal => "PLUS".to_owned(),
        _ => name.to_uppercase(),
    })
}

/// The key of that Flixel name (scripts and mods' settings name keys so), when the game knows it.
pub fn key_named(name: &str) -> Option<KeyCode> {
    use KeyCode::*;
    let name = name.trim().to_uppercase();
    crate::settings::ui_bindable().into_iter().chain([LeftShift, LeftControl, LeftAlt, Home, End, F11]).find(|&key| flx_name(key).as_deref() == Some(name.as_str()))
}
