//! Key presses with the time they really happened.
//!
//! The window hands over key events once per frame, so on its own a press is judged up to a
//! frame late. On Windows the raw input thread (native/psych_input) stamps every key change
//! when it happens; other systems and the few keys it leaves alone are timed per frame.
use crate::settings::bindable;
use macroquad::input::{is_key_pressed, is_key_released, KeyCode};

pub struct KeyEvent {
    pub key: KeyCode,
    pub pressed: bool,
    /// How long ago it happened, in ms.
    pub age_ms: f64,
}

pub struct Keyboard {
    keys: Vec<(KeyCode, Option<i32>)>,
    /// Whether the raw input thread is running.
    pub raw: bool,
}

impl Keyboard {
    pub fn start() -> Keyboard {
        Keyboard { keys: bindable(), raw: psych_input::psych_rawinput_start() != 0 }
    }

    fn raw_covers(&self, code: Option<i32>) -> bool {
        code.is_some_and(|code| psych_input::psych_rawinput_covers(code) != 0)
    }

    /// Every change of a bindable key since the last call.
    pub fn poll(&self) -> Vec<KeyEvent> {
        let mut events = Vec::new();
        loop {
            let event = psych_input::psych_rawinput_next();
            if event < 0 {
                break;
            }
            let age_ms = psych_input::psych_rawinput_age_ms();
            if let Some(&(key, _)) = self.keys.iter().find(|(_, code)| *code == Some(event & 0xFFFF)) {
                events.push(KeyEvent { key, pressed: event & 0x10000 != 0, age_ms });
            }
        }
        for &(key, code) in &self.keys {
            if !self.raw_covers(code) {
                for (changed, pressed) in [(is_key_pressed(key), true), (is_key_released(key), false)] {
                    if changed {
                        events.push(KeyEvent { key, pressed, age_ms: 0.0 });
                    }
                }
            }
        }
        events
    }
}
