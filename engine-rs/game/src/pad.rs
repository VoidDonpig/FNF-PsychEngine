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

/// The buttons of the four lanes and of the game's actions, as the Haxe engine has them at first.
pub const LANES: [[Button; 2]; 4] = [[Button::DpadLeft, Button::X], [Button::DpadDown, Button::A], [Button::DpadUp, Button::Y], [Button::DpadRight, Button::B]];

/// The buttons of an action of `controls::Ui`, by its place there (left, down, up, right, accept, back, pause, reset, and
/// the volume's, which gamepads do not have).
pub fn action(index: usize) -> &'static [Button] {
    use Button::*;
    match index {
        0 => &[DpadLeft, StickLeft],
        1 => &[DpadDown, StickDown],
        2 => &[DpadUp, StickUp],
        3 => &[DpadRight, StickRight],
        4 => &[A, Start],
        5 => &[B],
        6 => &[Start],
        7 => &[Back],
        _ => &[],
    }
}

/// A button by Flixel's name for it ("A", "START", "DPAD_LEFT", "LEFT_STICK_DIGITAL_UP"...), as scripts name them.
pub fn named(name: &str) -> Option<Button> {
    use Button::*;
    Some(match name.trim().to_uppercase().as_str() {
        "A" => A,
        "B" => B,
        "X" => X,
        "Y" => Y,
        "START" => Start,
        "BACK" => Back,
        "LEFT_SHOULDER" => LeftShoulder,
        "RIGHT_SHOULDER" => RightShoulder,
        "LEFT_TRIGGER" => LeftTrigger,
        "RIGHT_TRIGGER" => RightTrigger,
        "DPAD_UP" => DpadUp,
        "DPAD_DOWN" => DpadDown,
        "DPAD_LEFT" => DpadLeft,
        "DPAD_RIGHT" => DpadRight,
        "LEFT_STICK_DIGITAL_UP" => StickUp,
        "LEFT_STICK_DIGITAL_DOWN" => StickDown,
        "LEFT_STICK_DIGITAL_LEFT" => StickLeft,
        "LEFT_STICK_DIGITAL_RIGHT" => StickRight,
        _ => return None,
    })
}
