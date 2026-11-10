//! What the player chose in the options, kept in a file next to the game.
use macroquad::input::KeyCode;
use serde_json::{json, Value};
use std::path::Path;

const FILE: &str = "funkin-rs-settings.json";

/// Keys that can be bound, with the Windows key code raw input reports them under (where it does).
pub fn bindable() -> Vec<(KeyCode, Option<i32>)> {
    use KeyCode::*;
    let letters = [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z];
    let digits = [Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9];
    let mut keys: Vec<(KeyCode, Option<i32>)> = letters.iter().zip(65..).chain(digits.iter().zip(48..)).map(|(&k, code)| (k, Some(code))).collect();
    keys.extend([(Left, Some(37)), (Up, Some(38)), (Right, Some(39)), (Down, Some(40)), (Space, Some(32))]);
    // Their code differs between keyboard layouts, so raw input leaves them alone: these are timed per frame.
    keys.extend([Comma, Period, Slash, Semicolon, Apostrophe, LeftBracket, RightBracket, Minus, Equal, Backslash, Kp0, Kp1, Kp2, Kp3, Kp4, Kp5, Kp6, Kp7, Kp8, Kp9].map(|k| (k, None)));
    keys
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub downscroll: bool,
    pub middlescroll: bool,
    pub ghost_tapping: bool,
    pub botplay: bool,
    /// Notes are judged and drawn this many ms later (for sound that arrives late, such as Bluetooth).
    pub offset_ms: f64,
    /// Two keys per lane.
    pub keys: [[Option<KeyCode>; 2]; 4],
}

impl Default for Settings {
    fn default() -> Settings {
        use KeyCode::*;
        Settings {
            downscroll: false,
            middlescroll: false,
            ghost_tapping: true,
            botplay: false,
            offset_ms: 0.0,
            keys: [[Some(A), Some(Left)], [Some(S), Some(Down)], [Some(W), Some(Up)], [Some(D), Some(Right)]],
        }
    }
}

impl Settings {
    /// Anything missing or unreadable keeps its default.
    pub fn from_json(text: &str) -> Settings {
        let mut settings = Settings::default();
        let Ok(json) = serde_json::from_str::<Value>(text) else { return settings };
        for (name, flag) in [("downscroll", &mut settings.downscroll), ("middlescroll", &mut settings.middlescroll), ("ghost_tapping", &mut settings.ghost_tapping), ("botplay", &mut settings.botplay)] {
            *flag = json[name].as_bool().unwrap_or(*flag);
        }
        settings.offset_ms = json["offset_ms"].as_f64().unwrap_or(0.0).clamp(-500.0, 500.0);
        if json["keys"].is_array() {
            let known = bindable();
            for (lane, slots) in settings.keys.iter_mut().enumerate() {
                for (slot, key) in slots.iter_mut().enumerate() {
                    let name = json["keys"][lane][slot].as_str().unwrap_or("");
                    *key = known.iter().map(|&(k, _)| k).find(|k| format!("{k:?}") == name);
                }
            }
        }
        settings
    }

    pub fn to_json(&self) -> String {
        let keys: Vec<Vec<Value>> = self.keys.iter().map(|lane| lane.iter().map(|k| k.map_or(Value::Null, |k| json!(format!("{k:?}")))).collect()).collect();
        let json = json!({ "downscroll": self.downscroll, "middlescroll": self.middlescroll, "ghost_tapping": self.ghost_tapping, "botplay": self.botplay, "offset_ms": self.offset_ms, "keys": keys });
        serde_json::to_string_pretty(&json).unwrap_or_default()
    }

    pub fn load(base: &Path) -> Settings {
        Settings::from_json(&std::fs::read_to_string(base.join(FILE)).unwrap_or_default())
    }

    pub fn save(&self, base: &Path) {
        if let Err(error) = std::fs::write(base.join(FILE), self.to_json()) {
            eprintln!("Could not save the settings: {error}");
        }
    }

    /// Puts `key` on a slot; a key sits on one slot only.
    pub fn bind(&mut self, lane: usize, slot: usize, key: Option<KeyCode>) {
        if key.is_some() {
            self.keys.iter_mut().flatten().filter(|k| **k == key).for_each(|k| *k = None);
        }
        self.keys[lane][slot] = key;
    }

    pub fn lane_of(&self, key: KeyCode) -> Option<usize> {
        self.keys.iter().position(|lane| lane.contains(&Some(key)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survives_a_round_trip_and_bad_files() {
        let mut settings = Settings { downscroll: true, offset_ms: -12.0, ghost_tapping: false, ..Default::default() };
        settings.bind(0, 0, Some(KeyCode::Z));
        settings.bind(3, 1, None);
        assert_eq!(Settings::from_json(&settings.to_json()), settings);
        assert_eq!(Settings::from_json("not json"), Settings::default());
        assert_eq!(Settings::from_json(r#"{"downscroll":true,"keys":"?","offset_ms":9999}"#), Settings { downscroll: true, offset_ms: 500.0, ..Default::default() });
    }

    #[test]
    fn a_key_sits_on_one_slot() {
        let mut settings = Settings::default();
        settings.bind(2, 0, Some(KeyCode::A)); // A was the left lane's
        assert_eq!(settings.keys[0], [None, Some(KeyCode::Left)]);
        assert_eq!((settings.lane_of(KeyCode::A), settings.lane_of(KeyCode::W)), (Some(2), None));
    }
}
