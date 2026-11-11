//! What the player chose in the options, kept in a file next to the game.
use crate::gfx::KeyCode;
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

/// Keys a menu action can be given: those of the notes, and the ones only menus use.
pub fn ui_bindable() -> Vec<KeyCode> {
    use KeyCode::*;
    bindable().into_iter().map(|(key, _)| key).chain([Enter, Backspace, Escape, KpAdd, KpSubtract]).collect()
}

/// How an option is shown and changed.
pub enum Kind {
    Flag(fn(&mut Settings) -> &mut bool),
    /// A number with its lowest and highest value, its step and its unit ("%" shows 0 to 1 as 0 to 100).
    Number(fn(&mut Settings) -> &mut f64, f64, f64, f64, &'static str),
    /// One of a list of names.
    /// The names to choose from (a list mods can add to).
    Choice(fn(&mut Settings) -> &mut usize, fn() -> &'static [&'static str]),
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
/// The last holds the note colours.
pub const PAGES: [&str; 6] = ["Controls", "Gameplay", "Visuals", "Gameplay Changers", "Graphics", "Note Colors"];
/// The colours each lane's notes are made of (body, shine, outline) until the player chooses others, and those of pixel stages.
pub const LANE_COLORS: [[u32; 3]; 4] = [[0xC24B99, 0xFFFFFF, 0x3C1F56], [0x00FFFF, 0xFFFFFF, 0x1542B7], [0x12FA05, 0xFFFFFF, 0x0A4447], [0xF9393F, 0xFFFFFF, 0x651038]];
pub const PIXEL_LANE_COLORS: [[u32; 3]; 4] = [[0xE276FF, 0xFFF9FF, 0x60008D], [0x3DCAFF, 0xF4FFFF, 0x003060], [0x71E300, 0xF6FFE6, 0x003100], [0xFF884E, 0xFFFAF5, 0x6C0000]];
pub const SCROLL_TYPES: [&str; 2] = ["multiplicative", "constant"];
pub const TIME_BARS: [&str; 4] = ["Time Left", "Time Elapsed", "Song Name", "Disabled"];
/// What the game draws with (see gfx.rs). DirectX is DirectX 12.
pub const GRAPHICS_APIS: [&str; 3] = ["OpenGL", "DirectX", "Vulkan"];
/// The note skins and splash skins to choose from: the usual one first, then those the game's and the mods'
/// `images/noteSkins/list.txt` and `images/noteSplashes/list.txt` name (set once at start with `set_skins`).
static SKINS: std::sync::OnceLock<(Vec<&'static str>, Vec<&'static str>)> = std::sync::OnceLock::new();
const BASE_SKINS: ([&str; 3], [&str; 5]) = (["Default", "Future", "Chip"], ["Psych", "Diamond", "Electric", "Sparkles", "Vanilla"]);

pub fn note_skins() -> &'static [&'static str] {
    SKINS.get().map_or(&BASE_SKINS.0, |skins| &skins.0)
}

pub fn splash_skins() -> &'static [&'static str] {
    SKINS.get().map_or(&BASE_SKINS.1, |skins| &skins.1)
}

/// The skins listed (the lists' lines, in order, each once), after the usual ones. Kept for the whole run.
pub fn set_skins(notes: &[String], splashes: &[String]) {
    let list = |usual: &'static str, listed: &[String]| {
        let mut names = vec![usual];
        for name in listed.iter().map(|name| name.trim()).filter(|name| !name.is_empty()) {
            if !names.contains(&name) {
                names.push(Box::leak(name.to_owned().into_boxed_str()));
            }
        }
        names
    };
    let _ = SKINS.set((list(BASE_SKINS.0[0], notes), list(BASE_SKINS.1[0], splashes)));
}
pub const PAUSE_MUSIC: [&str; 4] = ["None", "Tea Time", "Breakfast", "Breakfast (Pico)"];

/// Names and texts are the Haxe engine's.
pub const OPTIONS: &[Opt] = &[
    Opt { page: 1, key: "downscroll", name: "Downscroll", about: "If checked, notes go Down instead of Up, simple enough.", kind: Kind::Flag(|s| &mut s.downscroll) },
    Opt { page: 1, key: "middlescroll", name: "Middlescroll", about: "If checked, your notes get centered.", kind: Kind::Flag(|s| &mut s.middlescroll) },
    Opt { page: 1, key: "opponent_strums", name: "Opponent Notes", about: "If unchecked, opponent notes get hidden.", kind: Kind::Flag(|s| &mut s.opponent_strums) },
    Opt { page: 1, key: "ghost_tapping", name: "Ghost Tapping", about: "If checked, you won't get misses from pressing keys while there are no notes able to be hit.", kind: Kind::Flag(|s| &mut s.ghost_tapping) },
    Opt { page: 1, key: "auto_pause", name: "Auto Pause", about: "If checked, the game automatically pauses if the screen isn't on focus.", kind: Kind::Flag(|s| &mut s.auto_pause) },
    Opt { page: 1, key: "no_reset", name: "Disable Reset Button", about: "If checked, pressing Reset (R) won't do anything.", kind: Kind::Flag(|s| &mut s.no_reset) },
    Opt { page: 1, key: "one_note_holds", name: "Sustains as One Note", about: "If checked, Hold Notes can't be pressed if you miss, and count as a single Hit/Miss. Uncheck this if you prefer the old Input System.", kind: Kind::Flag(|s| &mut s.one_note_holds) },
    Opt { page: 1, key: "volume", name: "Volume", about: "How loud everything is. The volume keys change it at any time.", kind: Kind::Number(|s| &mut s.volume, 0.0, 1.0, 0.1, "%") },
    Opt { page: 1, key: "hitsound_volume", name: "Hitsound Volume", about: "Funny notes does \"Tick!\" when you hit them.", kind: Kind::Number(|s| &mut s.hitsound_volume, 0.0, 1.0, 0.1, "%") },
    Opt { page: 1, key: "rating_offset", name: "Rating Offset", about: "Changes how late/early you have to hit for a \"Sick!\" Higher values mean you have to hit later.", kind: Kind::Number(|s| &mut s.rating_offset, -30.0, 30.0, 1.0, "ms") },
    Opt { page: 1, key: "sick_window", name: "Sick! Hit Window", about: "Changes the amount of time you have for hitting a \"Sick!\" in milliseconds.", kind: Kind::Number(|s| &mut s.sick_window, 15.0, 45.0, 1.0, "ms") },
    Opt { page: 1, key: "good_window", name: "Good Hit Window", about: "Changes the amount of time you have for hitting a \"Good\" in milliseconds.", kind: Kind::Number(|s| &mut s.good_window, 15.0, 90.0, 1.0, "ms") },
    Opt { page: 1, key: "bad_window", name: "Bad Hit Window", about: "Changes the amount of time you have for hitting a \"Bad\" in milliseconds.", kind: Kind::Number(|s| &mut s.bad_window, 15.0, 135.0, 1.0, "ms") },
    Opt { page: 1, key: "safe_frames", name: "Safe Frames", about: "Changes how many frames you have for hitting a note earlier or late.", kind: Kind::Number(|s| &mut s.safe_frames, 2.0, 10.0, 0.1, "") },
    Opt { page: 1, key: "display_lag_correction", name: "Display Lag Correction", about: "If checked, your timing is judged against the picture on screen:\nthe time each frame takes to be drawn and shown is taken off it.", kind: Kind::Flag(|s| &mut s.display_lag_correction) },
    Opt { page: 1, key: "offset_ms", name: "Note Offset", about: "Notes are judged and drawn this much later, for sound that arrives late (Bluetooth).", kind: Kind::Number(|s| &mut s.offset_ms, -500.0, 500.0, 1.0, "ms") },
    Opt { page: 2, key: "note_skin", name: "Note Skins", about: "Select your prefered Note skin.", kind: Kind::Choice(|s| &mut s.note_skin, note_skins) },
    Opt { page: 2, key: "splash_skin", name: "Note Splashes", about: "Select your prefered Note Splash variation.", kind: Kind::Choice(|s| &mut s.splash_skin, splash_skins) },
    Opt { page: 2, key: "splash_alpha", name: "Note Splash Opacity", about: "How much transparent should the Note Splashes be.", kind: Kind::Number(|s| &mut s.splash_alpha, 0.0, 1.0, 0.1, "%") },
    Opt { page: 2, key: "hide_hud", name: "Hide HUD", about: "If checked, hides most HUD elements.", kind: Kind::Flag(|s| &mut s.hide_hud) },
    Opt { page: 2, key: "time_bar", name: "Time Bar", about: "What should the Time Bar display?", kind: Kind::Choice(|s| &mut s.time_bar, || &TIME_BARS) },
    Opt { page: 2, key: "flashing", name: "Flashing Lights", about: "Uncheck this if you're sensitive to flashing lights!", kind: Kind::Flag(|s| &mut s.flashing) },
    Opt { page: 2, key: "cam_zooms", name: "Camera Zooms", about: "If unchecked, the camera won't zoom in on a beat hit.", kind: Kind::Flag(|s| &mut s.cam_zooms) },
    Opt { page: 2, key: "score_zoom", name: "Score Text Grow on Hit", about: "If unchecked, disables the Score text growing everytime you hit a note.", kind: Kind::Flag(|s| &mut s.score_zoom) },
    Opt { page: 2, key: "health_bar_alpha", name: "Health Bar Opacity", about: "How much transparent should the health bar and icons be.", kind: Kind::Number(|s| &mut s.health_bar_alpha, 0.0, 1.0, 0.1, "%") },
    Opt { page: 2, key: "show_fps", name: "FPS Counter", about: "If unchecked, hides the FPS Counter.", kind: Kind::Flag(|s| &mut s.show_fps) },
    Opt { page: 2, key: "discord_rpc", name: "Discord Rich Presence", about: "Uncheck this to prevent accidental leaks, it will hide the Application from your \"Playing\" box on Discord", kind: Kind::Flag(|s| &mut s.discord_rpc) },
    Opt { page: 2, key: "pause_music", name: "Pause Music", about: "What song do you prefer for the Pause Screen?", kind: Kind::Choice(|s| &mut s.pause_music, || &PAUSE_MUSIC) },
    Opt { page: 2, key: "combo_stacking", name: "Combo Stacking", about: "If unchecked, Ratings and Combo won't stack, saving on System Memory and making them easier to read.", kind: Kind::Flag(|s| &mut s.combo_stacking) },
    Opt { page: 2, key: "show_ms", name: "Hit Timing (ms)", about: "If checked, shows how many milliseconds early or late each hit was.", kind: Kind::Flag(|s| &mut s.show_ms) },
    Opt { page: 2, key: "rating_x", name: "Rating Position X", about: "Moves the rating that pops up on a hit to the right.", kind: Kind::Number(|s| &mut s.combo_offset[0], -640.0, 640.0, 5.0, "px") },
    Opt { page: 2, key: "rating_y", name: "Rating Position Y", about: "Moves the rating that pops up on a hit up.", kind: Kind::Number(|s| &mut s.combo_offset[1], -360.0, 360.0, 5.0, "px") },
    Opt { page: 2, key: "combo_x", name: "Combo Position X", about: "Moves the combo count and the hit timing to the right.", kind: Kind::Number(|s| &mut s.combo_offset[2], -640.0, 640.0, 5.0, "px") },
    Opt { page: 2, key: "combo_y", name: "Combo Position Y", about: "Moves the combo count and the hit timing up.", kind: Kind::Number(|s| &mut s.combo_offset[3], -360.0, 360.0, 5.0, "px") },
    Opt { page: 3, key: "scroll_type", name: "Scroll Type", about: "Multiplicative: the chart's speed times Scroll Speed. Constant: Scroll Speed itself, for every song.", kind: Kind::Choice(|s| &mut s.scroll_type, || &SCROLL_TYPES) },
    Opt { page: 3, key: "scroll_speed", name: "Scroll Speed", about: "How fast the notes travel (up to 3X when multiplicative, up to 6 when constant).", kind: Kind::Number(|s| &mut s.scroll_speed, 0.35, 6.0, 0.05, "X") },
    Opt { page: 3, key: "playback_rate", name: "Playback Rate", about: "How fast the song plays. Its pitch follows.", kind: Kind::Number(|s| &mut s.playback_rate, 0.5, 3.0, 0.05, "X") },
    Opt { page: 3, key: "health_gain", name: "Health Gain Multiplier", about: "How much more health a hit gives.", kind: Kind::Number(|s| &mut s.health_gain, 0.0, 5.0, 0.1, "X") },
    Opt { page: 3, key: "health_loss", name: "Health Loss Multiplier", about: "How much more health a miss takes.", kind: Kind::Number(|s| &mut s.health_loss, 0.5, 5.0, 0.1, "X") },
    Opt { page: 3, key: "instakill", name: "Instakill on Miss", about: "One miss ends the song.", kind: Kind::Flag(|s| &mut s.instakill) },
    Opt { page: 3, key: "practice", name: "Practice Mode", about: "You cannot lose. Scores are not kept.", kind: Kind::Flag(|s| &mut s.practice) },
    Opt { page: 3, key: "botplay", name: "Botplay", about: "The game plays the player's notes itself. Scores are not kept.", kind: Kind::Flag(|s| &mut s.botplay) },
    Opt { page: 4, key: "low_quality", name: "Low Quality", about: "If checked, disables some background details, decreases loading times and improves performance.", kind: Kind::Flag(|s| &mut s.low_quality) },
    Opt { page: 4, key: "antialiasing", name: "Anti-Aliasing", about: "If unchecked, disables anti-aliasing, increases performance at the cost of sharper visuals.", kind: Kind::Flag(|s| &mut s.antialiasing) },
    Opt { page: 4, key: "shaders", name: "Shaders", about: "If unchecked, disables shaders.\nIt's used for some visual effects, and also CPU intensive for weaker PCs.", kind: Kind::Flag(|s| &mut s.shaders) },
    Opt { page: 4, key: "framerate", name: "Framerate", about: "Pretty self explanatory, isn't it? Enter sets it to what the display shows.", kind: Kind::Number(|s| &mut s.framerate, 60.0, 1000.0, 1.0, "FPS") },
    Opt { page: 4, key: "graphics_api", name: "Graphics API", about: "What the game is drawn with: DirectX (12, Windows only), Vulkan or OpenGL. One this machine does not have is replaced by one it has. Restart the game to apply.", kind: Kind::Choice(|s| &mut s.graphics_api, || &GRAPHICS_APIS) },
    Opt { page: 4, key: "fullscreen", name: "Fullscreen", about: "The game takes the whole screen. F11 switches it at any time.", kind: Kind::Flag(|s| &mut s.fullscreen) },
    Opt { page: 4, key: "exclusive_fullscreen", name: "Exclusive Fullscreen", about: "If checked, fullscreen takes the whole display for the game alone. If unchecked, fullscreen is a borderless window.", kind: Kind::Flag(|s| &mut s.exclusive_fullscreen) },
];

/// How many pictures a second the display shows, where the system says.
fn refresh_rate() -> f64 {
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        extern "system" {
            fn GetDC(window: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
            fn ReleaseDC(window: *mut std::ffi::c_void, dc: *mut std::ffi::c_void) -> i32;
        }
        #[link(name = "gdi32")]
        extern "system" {
            fn GetDeviceCaps(dc: *mut std::ffi::c_void, index: i32) -> i32;
        }
        const VREFRESH: i32 = 116;
        // the whole screen's device context
        let rate = unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let rate = GetDeviceCaps(screen, VREFRESH);
            ReleaseDC(std::ptr::null_mut(), screen);
            rate
        };
        if rate > 1 {
            return rate as f64;
        }
    }
    60.0
}

/// A colour from its six hex digits, with or without a leading "#".
pub fn hex(text: &str) -> Option<u32> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    u32::from_str_radix(digits, 16).ok().filter(|_| digits.len() == 6 && digits.bytes().all(|digit| digit.is_ascii_hexdigit()))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub downscroll: bool,
    pub middlescroll: bool,
    pub opponent_strums: bool,
    pub ghost_tapping: bool,
    pub no_reset: bool,
    pub one_note_holds: bool,
    pub auto_pause: bool,
    /// 0 to 1.
    pub volume: f64,
    pub botplay: bool,
    /// 0 to 1.
    pub hitsound_volume: f64,
    pub rating_offset: f64,
    pub sick_window: f64,
    pub good_window: f64,
    pub bad_window: f64,
    pub safe_frames: f64,
    /// Presses are judged against the picture on screen, which is behind the song by the time frames take to be shown.
    pub display_lag_correction: bool,
    /// Notes are judged and drawn this many ms later (for sound that arrives late, such as Bluetooth).
    pub offset_ms: f64,
    /// Indexes into `note_skins()` and `splash_skins()`.
    pub note_skin: usize,
    pub splash_skin: usize,
    pub splash_alpha: f64,
    pub hide_hud: bool,
    /// Index into `TIME_BARS`.
    pub time_bar: usize,
    pub flashing: bool,
    pub cam_zooms: bool,
    pub score_zoom: bool,
    pub health_bar_alpha: f64,
    pub show_fps: bool,
    /// What is being played shows in Discord.
    pub discord_rpc: bool,
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
    /// How far the rating (x, y) and the combo count (x, y) pop up from their usual place, in pixels: right and up.
    pub combo_offset: [f64; 4],
    pub low_quality: bool,
    pub antialiasing: bool,
    /// What the Haxe engine draws with shaders: the rain of weekend 1, the title screen's colour shift.
    pub shaders: bool,
    /// Frames a second at most.
    pub framerate: f64,
    /// Index into `GRAPHICS_APIS`. Read when the game starts. Until chosen: DirectX on Windows, Vulkan elsewhere.
    pub graphics_api: usize,
    pub fullscreen: bool,
    pub exclusive_fullscreen: bool,
    /// Body, shine and outline of each lane's notes, and those of pixel stages.
    pub note_colors: [[u32; 3]; 4],
    pub pixel_note_colors: [[u32; 3]; 4],
    /// Two keys per lane.
    pub keys: [[Option<KeyCode>; 2]; 4],
    /// The keys of the menus and of the volume.
    pub ui_keys: crate::controls::Binds,
    /// The language file chosen ("pt-BR"); the game's own words are "en-US".
    pub language: String,
    /// Each mod's own settings (its `data/settings.json`), by folder: the values by their save names.
    pub mod_settings: serde_json::Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Settings {
        use KeyCode::*;
        Settings {
            downscroll: false, middlescroll: false, opponent_strums: true, ghost_tapping: true, no_reset: false, one_note_holds: true, auto_pause: true, volume: 1.0, botplay: false,
            hitsound_volume: 0.0, rating_offset: 0.0, sick_window: 45.0, good_window: 90.0, bad_window: 135.0, safe_frames: 10.0, display_lag_correction: false, offset_ms: 0.0,
            note_skin: 0, splash_skin: 0, splash_alpha: 0.6, hide_hud: false, time_bar: 0, flashing: true, cam_zooms: true, score_zoom: true, health_bar_alpha: 1.0, show_fps: true, discord_rpc: true,
            pause_music: 1, combo_stacking: true, show_ms: false,
            scroll_type: 0, scroll_speed: 1.0, playback_rate: 1.0, health_gain: 1.0, health_loss: 1.0, instakill: false, practice: false,
            combo_offset: [0.0; 4], low_quality: false, antialiasing: true, shaders: true, framerate: refresh_rate().clamp(60.0, 1000.0), graphics_api: if cfg!(windows) { 1 } else { 2 }, fullscreen: false, exclusive_fullscreen: false, note_colors: LANE_COLORS, pixel_note_colors: PIXEL_LANE_COLORS,
            keys: [[Some(A), Some(Left)], [Some(S), Some(Down)], [Some(W), Some(Up)], [Some(D), Some(Right)]],
            ui_keys: crate::controls::defaults(), language: funkin_core::lang::ENGLISH.0.to_owned(), mod_settings: serde_json::Map::new(),
        }
    }
}

impl Settings {
    /// Anything missing or unreadable keeps its default.
    #[cfg(test)]
    fn from_json(text: &str) -> Settings {
        Settings::from_value(serde_json::from_str(text).unwrap_or_default())
    }

    fn from_value(json: Value) -> Settings {
        let mut settings = Settings::default();
        for option in OPTIONS {
            let saved = &json[option.key];
            match option.kind {
                Kind::Flag(get) => *get(&mut settings) = saved.as_bool().unwrap_or(*get(&mut settings)),
                Kind::Number(get, low, high, ..) => *get(&mut settings) = saved.as_f64().unwrap_or(*get(&mut settings)).clamp(low, high),
                Kind::Choice(get, names) => *get(&mut settings) = saved.as_str().and_then(|name| names().iter().position(|n| *n == name)).unwrap_or(*get(&mut settings)),
            }
        }
        if let Some(language) = json["language"].as_str() {
            settings.language = language.to_owned();
        }
        settings.mod_settings = json["mod_settings"].as_object().cloned().unwrap_or_default();
        for ((name, _), slots) in crate::controls::NAMES.iter().zip(settings.ui_keys.iter_mut()) {
            if json["ui_keys"][name].is_array() {
                *slots = [0, 1].map(|slot| ui_bindable().into_iter().find(|key| Some(format!("{key:?}").as_str()) == json["ui_keys"][name][slot].as_str()));
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
        for (key, colors) in [("note_colors", &mut settings.note_colors), ("pixel_note_colors", &mut settings.pixel_note_colors)] {
            for (lane, parts) in colors.iter_mut().enumerate() {
                for (part, color) in parts.iter_mut().enumerate() {
                    *color = json[key][lane][part].as_str().and_then(hex).unwrap_or(*color);
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
                Kind::Choice(get, names) => json!(names()[*get(&mut copy)]),
            };
            json.insert(option.key.to_owned(), value);
        }
        let keys: Vec<Vec<Value>> = self.keys.iter().map(|lane| lane.iter().map(|k| k.map_or(Value::Null, |k| json!(format!("{k:?}")))).collect()).collect();
        json.insert("keys".to_owned(), json!(keys));
        let ui_keys: serde_json::Map<String, Value> = crate::controls::NAMES.iter().zip(self.ui_keys).map(|((name, _), slots)| ((*name).to_owned(), json!(slots.map(|key| key.map(|key| format!("{key:?}")))))).collect();
        json.insert("ui_keys".to_owned(), Value::Object(ui_keys));
        json.insert("language".to_owned(), json!(self.language));
        json.insert("mod_settings".to_owned(), Value::Object(self.mod_settings.clone()));
        for (key, colors) in [("note_colors", &self.note_colors), ("pixel_note_colors", &self.pixel_note_colors)] {
            json.insert(key.to_owned(), json!(colors.map(|parts| parts.map(|color| format!("{color:06X}")))));
        }
        serde_json::to_string_pretty(&json).unwrap_or_default()
    }

    /// Whether the game has been started in this folder before.
    pub fn kept(base: &Path) -> bool {
        base.join(FILE).exists()
    }

    pub fn load(base: &Path) -> Settings {
        Settings::from_value(funkin_core::files::read(&base.join(FILE)))
    }

    pub fn save(&self, base: &Path) {
        if let Err(error) = funkin_core::files::write(&base.join(FILE), &self.to_json()) {
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
        let mut settings = Settings { downscroll: true, offset_ms: -12.0, ghost_tapping: false, time_bar: 2, splash_alpha: 0.3, sick_window: 30.0, combo_offset: [5.0, -20.0, 0.0, 300.0], antialiasing: false, ..Default::default() };
        settings.note_colors[2] = [0x000001, 0xABCDEF, 0];
        settings.bind(0, 0, Some(KeyCode::Z));
        settings.bind(3, 1, None);
        assert_eq!(Settings::from_json(&settings.to_json()), settings);
        assert_eq!(Settings::from_json("not json"), Settings::default());
        assert_eq!(Settings::from_json(r#"{"downscroll":true,"keys":"?","offset_ms":9999,"note_colors":[["red","+12345","12345"]]}"#), Settings { downscroll: true, offset_ms: 500.0, ..Default::default() });
        assert_eq!((hex("#c24b99"), hex("C24B99"), hex("C24B9"), hex("C24B99F")), (Some(0xC24B99), Some(0xC24B99), None, None));
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
