//! Gamepads, as the Haxe engine takes them: any gamepad's buttons, and its left stick as four more buttons. Read once a
//! frame (`update`); then `down`, `pressed` (this frame) and `released`. On Windows through gilrs (XInput and Windows'
//! own gamepads); elsewhere none are seen.
// shortcut: Windows only; gilrs needs libudev on Linux. Add it there behind the same functions when a player asks.
use std::cell::RefCell;

/// The buttons the Haxe engine's controls name (FlxGamepadInputID), the left stick's directions among them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Button {
    A,
    B,
    X,
    Y,
    Start,
    Back,
    LeftShoulder,
    RightShoulder,
    LeftTrigger,
    RightTrigger,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    StickUp,
    StickDown,
    StickLeft,
    StickRight,
}


#[derive(Default)]
struct State {
    down: Vec<Button>,
    was: Vec<Button>,
    /// The sticks: left x, y, right x, y (-1 to 1, up negative as in Flixel), the first gamepad's.
    sticks: [f32; 4],
    #[cfg(windows)]
    gilrs: Option<gilrs::Gilrs>,
    #[cfg(windows)]
    tried: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The buttons down on any gamepad now.
#[cfg(windows)]
fn read(state: &mut State) -> Vec<Button> {
    use gilrs::{Axis, Button as Their};
    if !std::mem::replace(&mut state.tried, true) {
        state.gilrs = gilrs::Gilrs::new().map_err(|error| eprintln!("Gamepads: {error}")).ok();
    }
    let Some(gilrs) = &mut state.gilrs else { return Vec::new() };
    while gilrs.next_event().is_some() {}
    let mut down = Vec::new();
    state.sticks = gilrs.gamepads().next().map_or([0.0; 4], |(_, pad)| {
        [pad.value(Axis::LeftStickX), -pad.value(Axis::LeftStickY), pad.value(Axis::RightStickX), -pad.value(Axis::RightStickY)]
    });
    for (_, pad) in gilrs.gamepads() {
        let buttons = [
            (Their::South, Button::A), (Their::East, Button::B), (Their::West, Button::X), (Their::North, Button::Y), (Their::Start, Button::Start),
            (Their::Select, Button::Back), (Their::LeftTrigger, Button::LeftShoulder), (Their::RightTrigger, Button::RightShoulder),
            (Their::LeftTrigger2, Button::LeftTrigger), (Their::RightTrigger2, Button::RightTrigger), (Their::DPadUp, Button::DpadUp),
            (Their::DPadDown, Button::DpadDown), (Their::DPadLeft, Button::DpadLeft), (Their::DPadRight, Button::DpadRight),
        ];
        down.extend(buttons.into_iter().filter(|(theirs, _)| pad.is_pressed(*theirs)).map(|(_, ours)| ours));
        // the left stick past half way counts as a button, as Flixel's LEFT_STICK_DIGITAL_* (up is positive in gilrs)
        let (x, y) = (pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
        down.extend([(y > 0.5, Button::StickUp), (y < -0.5, Button::StickDown), (x < -0.5, Button::StickLeft), (x > 0.5, Button::StickRight)].into_iter().filter(|(on, _)| *on).map(|(_, button)| button));
    }
    down
}

#[cfg(not(windows))]
fn read(_state: &mut State) -> Vec<Button> {
    Vec::new()
}

/// Reads the gamepads for this frame.
pub fn update() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let now = read(&mut state);
        state.was = std::mem::replace(&mut state.down, now);
    });
}

pub fn down(button: Button) -> bool {
    STATE.with(|state| state.borrow().down.contains(&button))
}

/// A stick's place along one way (gamepadAnalogX/Y): the left stick or the right, across or down.
pub fn stick(left: bool, across: bool) -> f32 {
    STATE.with(|state| state.borrow().sticks[(!left as usize) * 2 + (!across as usize)])
}

/// Went down this frame.
pub fn pressed(button: Button) -> bool {
    STATE.with(|state| {
        let state = state.borrow();
        state.down.contains(&button) && !state.was.contains(&button)
    })
}

/// Came up this frame.
pub fn released(button: Button) -> bool {
    STATE.with(|state| {
        let state = state.borrow();
        !state.down.contains(&button) && state.was.contains(&button)
    })
}

/// Two buttons for each of the four lanes and then each of the first eight actions of `controls::Ui` (left, down, up,
/// right, accept, back, pause, reset; the volume's and the debug keys have none), in the settings' order.
pub type Binds = [[Option<Button>; 2]; 12];

/// Each row's name in the settings file (the Haxe engine's names).
pub const ROWS: [&str; 12] = ["note_left", "note_down", "note_up", "note_right", "ui_left", "ui_down", "ui_up", "ui_right", "accept", "back", "pause", "reset"];

/// The Haxe engine's buttons (ClientPrefs.gamepadBinds).
pub fn defaults() -> Binds {
    use Button::*;
    [
        [DpadLeft, X], [DpadDown, A], [DpadUp, Y], [DpadRight, B],
        [DpadLeft, StickLeft], [DpadDown, StickDown], [DpadUp, StickUp], [DpadRight, StickRight],
        [A, Start], [B, B], [Start, Start], [Back, Back],
    ].map(|[first, second]| [Some(first), (second != first).then_some(second)])
}

thread_local! {
    static BINDS: std::cell::Cell<Binds> = std::cell::Cell::new(defaults());
}

/// The buttons in use from now on.
pub fn set(binds: Binds) {
    BINDS.with(|kept| kept.set(binds));
}

/// A row's buttons; one left with none keeps its usual ones (no menu can be locked).
fn bound(row: usize) -> [Option<Button>; 2] {
    let bound = BINDS.with(std::cell::Cell::get)[row];
    if bound == [None, None] { defaults()[row] } else { bound }
}

/// The buttons of a lane.
pub fn lane(lane: usize) -> [Option<Button>; 2] {
    bound(lane)
}

/// The buttons of an action of `controls::Ui`, by its place there.
pub fn action(index: usize) -> [Option<Button>; 2] {
    if index < 8 { bound(4 + index) } else { [None; 2] }
}

/// A button that went down this frame on any gamepad (the key binds' page waits for one).
pub fn first_pressed() -> Option<Button> {
    NAMES.iter().map(|(button, _)| *button).find(|&button| pressed(button))
}

/// The buttons by Flixel's names (FlxGamepadInputID), as scripts and the settings file name them.
const NAMES: [(Button, &str); 18] = {
    use Button::*;
    [
        (A, "A"), (B, "B"), (X, "X"), (Y, "Y"), (Start, "START"), (Back, "BACK"), (LeftShoulder, "LEFT_SHOULDER"), (RightShoulder, "RIGHT_SHOULDER"),
        (LeftTrigger, "LEFT_TRIGGER"), (RightTrigger, "RIGHT_TRIGGER"), (DpadUp, "DPAD_UP"), (DpadDown, "DPAD_DOWN"), (DpadLeft, "DPAD_LEFT"),
        (DpadRight, "DPAD_RIGHT"), (StickUp, "LEFT_STICK_DIGITAL_UP"), (StickDown, "LEFT_STICK_DIGITAL_DOWN"), (StickLeft, "LEFT_STICK_DIGITAL_LEFT"),
        (StickRight, "LEFT_STICK_DIGITAL_RIGHT"),
    ]
};

/// A button by Flixel's name for it ("A", "START", "DPAD_LEFT", "LEFT_STICK_DIGITAL_UP"...).
pub fn named(name: &str) -> Option<Button> {
    let name = name.trim().to_uppercase();
    NAMES.iter().find(|(_, own)| *own == name).map(|(button, _)| *button)
}

/// Flixel's name for a button.
pub fn flx_name(button: Button) -> &'static str {
    NAMES.iter().find(|(own, _)| *own == button).map_or("NONE", |(_, name)| name)
}

/// A button as the key binds' page shows it (an Xbox pad's names, "---" for none).
pub fn shown(button: Option<Button>) -> &'static str {
    use Button::*;
    match button {
        None => "---",
        Some(A) => "A",
        Some(B) => "B",
        Some(X) => "X",
        Some(Y) => "Y",
        Some(Start) => "Start",
        Some(Back) => "Back",
        Some(LeftShoulder) => "LB",
        Some(RightShoulder) => "RB",
        Some(LeftTrigger) => "LT",
        Some(RightTrigger) => "RT",
        Some(DpadUp) => "D-Pad Up",
        Some(DpadDown) => "D-Pad Down",
        Some(DpadLeft) => "D-Pad Left",
        Some(DpadRight) => "D-Pad Right",
        Some(StickUp) => "LS Up",
        Some(StickDown) => "LS Down",
        Some(StickLeft) => "LS Left",
        Some(StickRight) => "LS Right",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_without_buttons_keep_their_usual_ones_and_names_go_both_ways() {
        let mut binds = defaults();
        binds[9] = [None, None];
        binds[0] = [Some(Button::LeftTrigger), None];
        set(binds);
        assert_eq!((action(5), lane(0)), ([Some(Button::B), None], [Some(Button::LeftTrigger), None]));
        assert_eq!(action(8), [None, None]);
        set(defaults());
        assert!(NAMES.iter().all(|(button, name)| named(name) == Some(*button) && flx_name(*button) == *name));
        assert_eq!(named(" dpad_left "), Some(Button::DpadLeft));
    }
}
