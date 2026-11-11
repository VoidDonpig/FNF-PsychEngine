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
}

/// Two keys for each action, in the order of `Ui`.
pub type Binds = [[Option<KeyCode>; 2]; 11];

/// Each action's name in the settings file (the Haxe engine's) and in the options.
pub const NAMES: [(&str, &str); 11] = [
    ("ui_left", "Left"), ("ui_down", "Down"), ("ui_up", "Up"), ("ui_right", "Right"), ("accept", "Accept"), ("back", "Back"), ("pause", "Pause"), ("reset", "Reset"),
    ("volume_mute", "Volume Mute"), ("volume_up", "Volume Up"), ("volume_down", "Volume Down"),
];

/// The Haxe engine's keys.
pub fn defaults() -> Binds {
    use KeyCode::*;
    [[A, Left], [S, Down], [W, Up], [D, Right], [Space, Enter], [Backspace, Escape], [Enter, Escape], [R, R], [Key0, Key0], [KpAdd, Equal], [KpSubtract, Minus]].map(|keys| keys.map(Some))
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

pub fn pressed(action: Ui) -> bool {
    keys(action).into_iter().flatten().any(is_key_pressed)
}

pub fn down(action: Ui) -> bool {
    keys(action).into_iter().flatten().any(is_key_down)
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
