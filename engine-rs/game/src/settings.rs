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

/// How an option is shown and changed.
pub enum Kind {
    Flag(fn(&mut Settings) -> &mut bool),
    /// A number with its lowest and highest value, its step and its unit ("%" shows 0 to 1 as 0 to 100).
    Number(fn(&mut Settings) -> &mut f64, f64, f64, f64, &'static str),
    /// One of a list of names.
    Choice(fn(&mut Settings) -> &mut usize, &'static [&'static str]),
}

/// An option of the options menu: the page it is on, its name in the settings file, and what the menu says about it.
pub struct Opt {
    pub page: usize,
    pub key: &'static str,
    pub name: &'static str,
    pub about: &'static str,
    pub kind: Kind,
}

/// The pages of the options menu. The first holds the key binds; the others list `OPTIONS`.
pub const PAGES: [&str; 4] = ["Controls", "Gameplay", "Visuals", "Gameplay Changers"];
pub const SCROLL_TYPES: [&str; 2] = ["multiplicative", "constant"];
pub const TIME_BARS: [&str; 4] = ["Time Left", "Time Elapsed", "Song Name", "Disabled"];
pub const PAUSE_MUSIC: [&str; 4] = ["None", "Tea Time", "Breakfast", "Breakfast (Pico)"];

/// Names and texts are the Haxe engine's.
pub const OPTIONS: &[Opt] = &[
    Opt { page: 1, key: "downscroll", name: "Downscroll", about: "If checked, notes go Down instead of Up, simple enough.", kind: Kind::Flag(|s| &mut s.downscroll) },
    Opt { page: 1, key: "middlescroll", name: "Middlescroll", about: "If checked, your notes get centered.", kind: Kind::Flag(|s| &mut s.middlescroll) },
    Opt { page: 1, key: "opponent_strums", name: "Opponent Notes", about: "If unchecked, opponent notes get hidden.", kind: Kind::Flag(|s| &mut s.opponent_strums) },
    Opt { page: 1, key: "ghost_tapping", name: "Ghost Tapping", about: "If checked, you won't get misses from pressing keys while there are no notes able to be hit.", kind: Kind::Flag(|s| &mut s.ghost_tapping) },
    Opt { page: 1, key: "no_reset", name: "Disable Reset Button", about: "If checked, pressing Reset (R) won't do anything.", kind: Kind::Flag(|s| &mut s.no_reset) },
    Opt { page: 1, key: "hitsound_volume", name: "Hitsound Volume", about: "Funny notes does \"Tick!\" when you hit them.", kind: Kind::Number(|s| &mut s.hitsound_volume, 0.0, 1.0, 0.1, "%") },
    Opt { page: 1, key: "rating_offset", name: "Rating Offset", about: "Changes how late/early you have to hit for a \"Sick!\" Higher values mean you have to hit later.", kind: Kind::Number(|s| &mut s.rating_offset, -30.0, 30.0, 1.0, "ms") },
    Opt { page: 1, key: "sick_window", name: "Sick! Hit Window", about: "Changes the amount of time you have for hitting a \"Sick!\" in milliseconds.", kind: Kind::Number(|s| &mut s.sick_window, 15.0, 45.0, 1.0, "ms") },
    Opt { page: 1, key: "good_window", name: "Good Hit Window", about: "Changes the amount of time you have for hitting a \"Good\" in milliseconds.", kind: Kind::Number(|s| &mut s.good_window, 15.0, 90.0, 1.0, "ms") },
    Opt { page: 1, key: "bad_window", name: "Bad Hit Window", about: "Changes the amount of time you have for hitting a \"Bad\" in milliseconds.", kind: Kind::Number(|s| &mut s.bad_window, 15.0, 135.0, 1.0, "ms") },
    Opt { page: 1, key: "safe_frames", name: "Safe Frames", about: "Changes how many frames you have for hitting a note earlier or late.", kind: Kind::Number(|s| &mut s.safe_frames, 2.0, 10.0, 0.1, "") },
    Opt { page: 1, key: "offset_ms", name: "Note Offset", about: "Notes are judged and drawn this much later, for sound that arrives late (Bluetooth).", kind: Kind::Number(|s| &mut s.offset_ms, -500.0, 500.0, 1.0, "ms") },
    Opt { page: 2, key: "splash_alpha", name: "Note Splash Opacity", about: "How much transparent should the Note Splashes be.", kind: Kind::Number(|s| &mut s.splash_alpha, 0.0, 1.0, 0.1, "%") },
    Opt { page: 2, key: "hide_hud", name: "Hide HUD", about: "If checked, hides most HUD elements.", kind: Kind::Flag(|s| &mut s.hide_hud) },
    Opt { page: 2, key: "time_bar", name: "Time Bar", about: "What should the Time Bar display?", kind: Kind::Choice(|s| &mut s.time_bar, &TIME_BARS) },
    Opt { page: 2, key: "flashing", name: "Flashing Lights", about: "Uncheck this if you're sensitive to flashing lights!", kind: Kind::Flag(|s| &mut s.flashing) },
    Opt { page: 2, key: "cam_zooms", name: "Camera Zooms", about: "If unchecked, the camera won't zoom in on a beat hit.", kind: Kind::Flag(|s| &mut s.cam_zooms) },
    Opt { page: 2, key: "score_zoom", name: "Score Text Grow on Hit", about: "If unchecked, disables the Score text growing everytime you hit a note.", kind: Kind::Flag(|s| &mut s.score_zoom) },
    Opt { page: 2, key: "health_bar_alpha", name: "Health Bar Opacity", about: "How much transparent should the health bar and icons be.", kind: Kind::Number(|s| &mut s.health_bar_alpha, 0.0, 1.0, 0.1, "%") },
    Opt { page: 2, key: "show_fps", name: "FPS Counter", about: "If unchecked, hides the FPS Counter.", kind: Kind::Flag(|s| &mut s.show_fps) },
    Opt { page: 2, key: "pause_music", name: "Pause Music", about: "What song do you prefer for the Pause Screen?", kind: Kind::Choice(|s| &mut s.pause_music, &PAUSE_MUSIC) },
    Opt { page: 2, key: "combo_stacking", name: "Combo Stacking", about: "If unchecked, Ratings and Combo won't stack, saving on System Memory and making them easier to read.", kind: Kind::Flag(|s| &mut s.combo_stacking) },
    Opt { page: 2, key: "show_ms", name: "Hit Timing (ms)", about: "If checked, shows how many milliseconds early or late each hit was.", kind: Kind::Flag(|s| &mut s.show_ms) },
    Opt { page: 3, key: "scroll_type", name: "Scroll Type", about: "Multiplicative: the chart's speed times Scroll Speed. Constant: Scroll Speed itself, for every song.", kind: Kind::Choice(|s| &mut s.scroll_type, &SCROLL_TYPES) },
    Opt { page: 3, key: "scroll_speed", name: "Scroll Speed", about: "How fast the notes travel (up to 3X when multiplicative, up to 6 when constant).", kind: Kind::Number(|s| &mut s.scroll_speed, 0.35, 6.0, 0.05, "X") },
    Opt { page: 3, key: "playback_rate", name: "Playback Rate", about: "How fast the song plays. Its pitch follows.", kind: Kind::Number(|s| &mut s.playback_rate, 0.5, 3.0, 0.05, "X") },
    Opt { page: 3, key: "health_gain", name: "Health Gain Multiplier", about: "How much more health a hit gives.", kind: Kind::Number(|s| &mut s.health_gain, 0.0, 5.0, 0.1, "X") },
    Opt { page: 3, key: "health_loss", name: "Health Loss Multiplier", about: "How much more health a miss takes.", kind: Kind::Number(|s| &mut s.health_loss, 0.5, 5.0, 0.1, "X") },
    Opt { page: 3, key: "instakill", name: "Instakill on Miss", about: "One miss ends the song.", kind: Kind::Flag(|s| &mut s.instakill) },
    Opt { page: 3, key: "practice", name: "Practice Mode", about: "You cannot lose. Scores are not kept.", kind: Kind::Flag(|s| &mut s.practice) },
    Opt { page: 3, key: "botplay", name: "Botplay", about: "The game plays the player's notes itself. Scores are not kept.", kind: Kind::Flag(|s| &mut s.botplay) },
];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub downscroll: bool,
    pub middlescroll: bool,
    pub opponent_strums: bool,
    pub ghost_tapping: bool,
    pub no_reset: bool,
    pub botplay: bool,
    /// 0 to 1.
    pub hitsound_volume: f64,
    pub rating_offset: f64,
    pub sick_window: f64,
    pub good_window: f64,
    pub bad_window: f64,
    pub safe_frames: f64,
    /// Notes are judged and drawn this many ms later (for sound that arrives late, such as Bluetooth).
    pub offset_ms: f64,
    pub splash_alpha: f64,
    pub hide_hud: bool,
    /// Index into `TIME_BARS`.
    pub time_bar: usize,
    pub flashing: bool,
    pub cam_zooms: bool,
    pub score_zoom: bool,
    pub health_bar_alpha: f64,
    pub show_fps: bool,
    /// Index into `PAUSE_MUSIC`.
    pub pause_music: usize,
    pub combo_stacking: bool,
    pub show_ms: bool,
    /// Index into `SCROLL_TYPES`.
    pub scroll_type: usize,
    pub scroll_speed: f64,
    pub playback_rate: f64,
    pub health_gain: f64,
    pub health_loss: f64,
    pub instakill: bool,
    pub practice: bool,
    /// Two keys per lane.
    pub keys: [[Option<KeyCode>; 2]; 4],
}

impl Default for Settings {
    fn default() -> Settings {
        use KeyCode::*;
        Settings {
            downscroll: false, middlescroll: false, opponent_strums: true, ghost_tapping: true, no_reset: false, botplay: false,
            hitsound_volume: 0.0, rating_offset: 0.0, sick_window: 45.0, good_window: 90.0, bad_window: 135.0, safe_frames: 10.0, offset_ms: 0.0,
            splash_alpha: 0.6, hide_hud: false, time_bar: 0, flashing: true, cam_zooms: true, score_zoom: true, health_bar_alpha: 1.0, show_fps: true,
            pause_music: 1, combo_stacking: true, show_ms: false,
            scroll_type: 0, scroll_speed: 1.0, playback_rate: 1.0, health_gain: 1.0, health_loss: 1.0, instakill: false, practice: false,
            keys: [[Some(A), Some(Left)], [Some(S), Some(Down)], [Some(W), Some(Up)], [Some(D), Some(Right)]],
        }
    }
}

impl Settings {
    /// Anything missing or unreadable keeps its default.
    pub fn from_json(text: &str) -> Settings {
        let mut settings = Settings::default();
        let Ok(json) = serde_json::from_str::<Value>(text) else { return settings };
        for option in OPTIONS {
            let saved = &json[option.key];
            match option.kind {
                Kind::Flag(get) => *get(&mut settings) = saved.as_bool().unwrap_or(*get(&mut settings)),
                Kind::Number(get, low, high, ..) => *get(&mut settings) = saved.as_f64().unwrap_or(*get(&mut settings)).clamp(low, high),
                Kind::Choice(get, names) => *get(&mut settings) = saved.as_str().and_then(|name| names.iter().position(|n| *n == name)).unwrap_or(*get(&mut settings)),
            }
        }
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
        let mut copy = self.clone();
        let mut json = serde_json::Map::new();
        for option in OPTIONS {
            let value = match option.kind {
                Kind::Flag(get) => json!(*get(&mut copy)),
                Kind::Number(get, ..) => json!(*get(&mut copy)),
                Kind::Choice(get, names) => json!(names[*get(&mut copy)]),
            };
            json.insert(option.key.to_owned(), value);
        }
        let keys: Vec<Vec<Value>> = self.keys.iter().map(|lane| lane.iter().map(|k| k.map_or(Value::Null, |k| json!(format!("{k:?}")))).collect()).collect();
        json.insert("keys".to_owned(), json!(keys));
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

    /// How strictly the song judges, from the windows chosen.
    pub fn timing(&self) -> funkin_core::play::Timing {
        funkin_core::play::Timing { sick: self.sick_window, good: self.good_window, bad: self.bad_window, safe: self.safe_frames / 60.0 * 1000.0, rating_offset: self.rating_offset, rate: self.playback_rate }
    }

    /// How fast the notes of a chart written for `chart_speed` travel.
    pub fn note_speed(&self, chart_speed: f64) -> f64 {
        if self.scroll_type == 1 { self.scroll_speed } else { chart_speed * self.scroll_speed.min(3.0) }
    }

    /// Whether a finished run counts for the high scores.
    pub fn keeps_scores(&self) -> bool {
        !self.botplay && !self.practice
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
        let mut settings = Settings { downscroll: true, offset_ms: -12.0, ghost_tapping: false, time_bar: 2, splash_alpha: 0.3, sick_window: 30.0, ..Default::default() };
        settings.bind(0, 0, Some(KeyCode::Z));
        settings.bind(3, 1, None);
        assert_eq!(Settings::from_json(&settings.to_json()), settings);
        assert_eq!(Settings::from_json("not json"), Settings::default());
        assert_eq!(Settings::from_json(r#"{"downscroll":true,"keys":"?","offset_ms":9999}"#), Settings { downscroll: true, offset_ms: 500.0, ..Default::default() });
    }

    #[test]
    fn every_option_has_its_own_name_in_the_file_and_a_page() {
        let mut keys: Vec<&str> = OPTIONS.iter().map(|option| option.key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), OPTIONS.len());
        assert!(OPTIONS.iter().all(|option| (1..PAGES.len()).contains(&option.page)));
        assert_eq!(Settings::from_json(r#"{"time_bar":"Disabled","pause_music":"Nothing known","sick_window":1}"#), Settings { time_bar: 3, sick_window: 15.0, ..Default::default() });
        assert_eq!(Settings { safe_frames: 6.0, ..Default::default() }.timing().safe, 100.0);
        let fast = Settings { scroll_speed: 4.0, ..Default::default() };
        assert_eq!((fast.note_speed(2.0), Settings { scroll_type: 1, ..fast.clone() }.note_speed(2.0)), (6.0, 4.0), "multiplied up to 3X, or the same for every song");
        assert!(!Settings { practice: true, ..Default::default() }.keeps_scores() && Settings::default().keeps_scores());
    }

    #[test]
    fn a_key_sits_on_one_slot() {
        let mut settings = Settings::default();
        settings.bind(2, 0, Some(KeyCode::A)); // A was the left lane's
        assert_eq!(settings.keys[0], [None, Some(KeyCode::Left)]);
        assert_eq!((settings.lane_of(KeyCode::A), settings.lane_of(KeyCode::W)), (Some(2), None));
    }
}
