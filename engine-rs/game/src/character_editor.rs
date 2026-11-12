//! The character editor (states/editors/CharacterEditorState.hx): a character on week 1's stage, beside the silhouettes
//! of where the opponent and the player stand, its animations played and stepped through frame by frame, their offsets
//! moved with the arrow keys or the mouse, and its file's settings in the Haxe editor's boxes; saved over its file, the
//! one before kept as `.bak`.
// shortcut: Load Template, the ghost's highlight and the icon's colour picker are not here; add them when asked.
use crate::gfx::*;
use crate::scene::Character;
use crate::ui::Ui;
use crate::{Skin, HEIGHT, WIDTH};
use funkin_core::content::Paths;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Instant;

/// Where the opponent and the player stand (the Haxe editor's dadPosition and bfPosition).
const OPPONENT_AT: Vec2 = vec2(100.0, 100.0);
const PLAYER_AT: Vec2 = vec2(770.0, 100.0);

const HELP: [&str; 19] = [
    "CAMERA", "E/Q - Camera Zoom In/Out", "J/K/L/I - Move Camera", "R - Reset Camera Zoom", "", "CHARACTER", "Ctrl + R - Reset Current Offset",
    "Ctrl + C - Copy Current Offset", "Ctrl + V - Paste Copied Offset on Current Animation", "Ctrl + Z - Undo Last Paste or Reset",
    "W/S - Previous/Next Animation", "Space - Replay Animation", "Arrow Keys/Mouse & Right Click - Move Offset", "A/D - Frame Advance (Back/Forward)", "",
    "OTHER", "F12 - Toggle Silhouettes", "Hold Shift - Move Offsets 10x faster and Camera 4x faster", "Hold Control - Move camera 4x slower",
];

/// What the editor wants next.
pub enum Go {
    Stay,
    Leave,
}

pub struct CharacterEditor {
    paths: Paths,
    /// The mod folder it works in ("": the game's).
    folder: String,
    name: String,
    /// The character's file as read; edits go into it, so what the game does not know of is kept.
    json: Value,
    character: Option<Character>,
    player: bool,
    /// Which animation (its place in the file's list), how far into it, whether it is held still (A/D).
    animation: usize,
    time: f32,
    still: bool,
    /// The camera: where its middle is, how near.
    camera: Vec2,
    zoom: f32,
    /// Arrow keys held: for how long, and how much of the next step is due; A/D held, for how long.
    held: (f32, f32),
    framing: f32,
    copied: Vec2,
    undo_offset: Option<Vec2>,
    /// A copy of a pose drawn behind (animation, time, how see-through).
    ghost: Option<(String, f32)>,
    ghost_alpha: f32,
    silhouettes: bool,
    help: bool,
    unsaved: bool,
    leaving: bool,
    said: Option<(String, Instant)>,
    mouse: Vec2,
    ui: Ui,
    tabs: [usize; 2],
    characters: Vec<String>,
    /// The Animations tab's form: the animation's name, its prefix (or symbol), indices, speed, looped.
    form: (String, String, String, f64, bool),
    pictures: [Option<Texture2D>; 4],
}

/// Whether a character of that name is the opponent's kind (the Haxe editor's guess, for files that do not say).
fn opponent_like(name: &str) -> bool {
    (name != "bf" && !name.starts_with("bf-") && !name.ends_with("-player") && !name.ends_with("-playable") && !name.ends_with("-dead"))
        || name.ends_with("-opponent") || name.starts_with("gf-") || name.ends_with("-gf") || name == "gf"
}

fn pair(value: &Value) -> Vec2 {
    vec2(value[0].as_f64().unwrap_or(0.0) as f32, value[1].as_f64().unwrap_or(0.0) as f32)
}

impl CharacterEditor {
    /// The character `name` of a mod's folder (or the game's), as its file is.
    pub fn open(base: &Path, folder: &str, name: &str) -> CharacterEditor {
        let paths = Paths::new(base, folder, "week1");
        let picture = |name: &str| paths.find(&format!("images/{name}.png")).and_then(|path| crate::atlas::picture(&path));
        let pictures = [picture("stageback"), picture("stagefront"), picture("editors/silhouetteDad"), picture("editors/silhouetteBF")];
        // the characters there are: the lists of them, and the files
        let mut characters: Vec<String> = paths.find_all("data/characterList.txt").iter().filter_map(|file| std::fs::read_to_string(file).ok())
            .flat_map(|text| text.lines().map(|line| line.trim().to_owned()).collect::<Vec<_>>()).filter(|line| !line.is_empty()).collect();
        for root in paths.roots() {
            for file in std::fs::read_dir(root.join("characters")).into_iter().flatten().flatten() {
                let path = file.path();
                if let (Some("json"), Some(stem)) = (path.extension().and_then(|ending| ending.to_str()), path.file_stem()) {
                    characters.push(stem.to_string_lossy().into_owned());
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        characters.retain(|name| seen.insert(name.clone()));
        let mut editor = CharacterEditor {
            paths, folder: folder.to_owned(), name: String::new(), json: Value::Null, character: None, player: false, animation: 0, time: 0.0, still: false,
            camera: Vec2::ZERO, zoom: 1.0, held: (0.0, 0.0), framing: 0.0, copied: Vec2::ZERO, undo_offset: None, ghost: None, ghost_alpha: 0.6, silhouettes: true,
            help: false, unsaved: false, leaving: false, said: None, mouse: Vec2::ZERO, ui: Ui::default(), tabs: [1, 0], characters,
            form: (String::new(), String::new(), String::new(), 24.0, false), pictures,
        };
        editor.load(name);
        editor
    }

    /// Reads a character's file (a new one, from the game's bf, when there is none) and shows it.
    fn load(&mut self, name: &str) {
        let read = |name: &str| self.paths.find(&format!("characters/{name}.json")).and_then(|file| std::fs::read_to_string(file).ok()).and_then(|text| serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).ok());
        let (json, new) = match read(name) {
            Some(json) => (json, false),
            None => (read("bf").unwrap_or_else(|| json!({"animations": [], "image": "characters/BOYFRIEND", "scale": 1})), true),
        };
        self.player = json["_editor_isPlayer"].as_bool().unwrap_or(!opponent_like(name));
        (self.name, self.json, self.animation, self.time, self.still, self.unsaved, self.leaving) = (name.to_owned(), json, 0, 0.0, false, new, false);
        self.rebuild();
        self.show_form();
        self.snap_camera();
        if new {
            self.say(format!("characters/{name}.json is new: it starts as bf"));
        }
    }

    /// The character built again from its file as it now is.
    fn rebuild(&mut self) {
        let spot = if self.player { PLAYER_AT } else { OPPONENT_AT };
        match Character::from_json(&self.paths, &self.name, &self.json, spot, self.player) {
            Ok(character) => self.character = Some(character),
            Err(error) => self.say(format!("Could not show it: {error}")),
        }
    }

    fn animations(&self) -> &[Value] {
        self.json["animations"].as_array().map_or(&[], Vec::as_slice)
    }

    fn current(&self) -> Option<String> {
        self.animations().get(self.animation).and_then(|animation| animation["anim"].as_str()).map(str::to_owned)
    }

    /// Where the camera looks at the character (as in a song), the cross drawn there.
    fn camera_point(&self) -> Vec2 {
        let middle = self.character.as_ref().map_or(Vec2::ZERO, Character::middle);
        let offset = pair(&self.json["camera_position"]);
        if self.player { middle + vec2(-100.0 - offset.x, -100.0 + offset.y) } else { middle + vec2(150.0 + offset.x, -100.0 + offset.y) }
    }

    fn snap_camera(&mut self) {
        self.camera = self.camera_point() + vec2(20.0, 20.0);
    }

    fn say(&mut self, words: impl Into<String>) {
        self.said = Some((words.into(), Instant::now()));
    }

    /// Something of the file changed: to be saved.
    fn changed(&mut self) {
        (self.unsaved, self.leaving) = (true, false);
    }

    /// The Animations tab's form filled from the animation shown.
    fn show_form(&mut self) {
        let Some(animation) = self.animations().get(self.animation).cloned() else { return };
        let indices = animation["indices"].as_array().map_or(String::new(), |list| list.iter().map(|index| index.to_string()).collect::<Vec<_>>().join(","));
        self.form = (
            animation["anim"].as_str().unwrap_or_default().to_owned(), animation["name"].as_str().unwrap_or_default().to_owned(), indices,
            animation["fps"].as_f64().unwrap_or(24.0), animation["loop"].as_bool().unwrap_or(false),
        );
    }

    fn offset(&self) -> Vec2 {
        self.animations().get(self.animation).map_or(Vec2::ZERO, |animation| pair(&animation["offsets"]))
    }

    fn set_offset(&mut self, offset: Vec2) {
        let offset = offset.round();
        let Some(name) = self.current() else { return };
        self.json["animations"][self.animation]["offsets"] = json!([offset.x as i64, offset.y as i64]);
        if let Some(character) = &mut self.character {
            character.set_offset(&name, offset);
        }
        self.changed();
    }

    pub fn update(&mut self) -> Go {
        let typing = self.ui.typing();
        self.ui.begin();
        self.panels();
        let dt = get_frame_time().min(0.1);
        let held = |key: KeyCode| is_key_down(key) || is_key_pressed(key);
        let (shift, ctrl) = (held(KeyCode::LeftShift) || held(KeyCode::RightShift), held(KeyCode::LeftControl) || held(KeyCode::RightControl));
        let mouse = mouse_position(vec2(WIDTH, HEIGHT));
        let moved = mouse - std::mem::replace(&mut self.mouse, mouse);
        if !typing {
            // the camera: J/K/L/I, zoom with E/Q, R back to normal
            let speed = 500.0 * dt * if shift { 4.0 } else { 1.0 } * if ctrl { 0.25 } else { 1.0 } / self.zoom;
            self.camera += vec2(held(KeyCode::L) as i32 as f32 - held(KeyCode::J) as i32 as f32, held(KeyCode::K) as i32 as f32 - held(KeyCode::I) as i32 as f32) * speed;
            if is_key_pressed(KeyCode::R) && !ctrl {
                self.zoom = 1.0;
            } else if held(KeyCode::E) != held(KeyCode::Q) {
                let toward = if held(KeyCode::E) { 1.0 } else { -1.0 };
                self.zoom = (self.zoom + toward * dt * self.zoom * if shift { 4.0 } else { 1.0 } * if ctrl { 0.25 } else { 1.0 }).clamp(0.1, 3.0);
            }
            let count = self.animations().len();
            if count > 1 && is_key_pressed(KeyCode::W) != is_key_pressed(KeyCode::S) {
                self.animation = (self.animation + if is_key_pressed(KeyCode::W) { count - 1 } else { 1 }) % count;
                (self.time, self.still, self.undo_offset) = (0.0, false, None);
                self.show_form();
            }
            self.move_offset(dt, shift, ctrl, moved);
            if is_key_pressed(KeyCode::Space) {
                (self.time, self.still) = (0.0, false);
            }
            // A/D: a frame back or on (held, ten a second), the animation held still
            let frames = self.current().and_then(|name| self.character.as_ref()?.frames_of(&name));
            if let Some((length, fps)) = frames.filter(|(length, _)| *length > 0) {
                self.framing = if held(KeyCode::A) || held(KeyCode::D) { self.framing + dt } else { 0.0 };
                let step = if is_key_pressed(KeyCode::A) { -1 } else if is_key_pressed(KeyCode::D) { 1 } else if self.framing > 0.5 && (self.framing * 10.0) as i64 != ((self.framing - dt) * 10.0) as i64 {
                    if held(KeyCode::A) { -1 } else if held(KeyCode::D) { 1 } else { 0 }
                } else {
                    0
                };
                if step != 0 {
                    let frame = ((self.time * fps).round() as i64 + step * if shift { 4 } else { 1 }).rem_euclid(length as i64);
                    (self.time, self.still) = (frame as f32 / fps, true);
                }
            }
            if is_key_pressed(KeyCode::F12) {
                self.silhouettes = !self.silhouettes;
            }
            if is_key_pressed(KeyCode::F1) || (self.help && is_key_pressed(KeyCode::Escape)) {
                self.help = !self.help;
            } else if is_key_pressed(KeyCode::Escape) {
                if !self.unsaved || self.leaving {
                    return Go::Leave;
                }
                self.leaving = true;
                self.say("Unsaved changes: Escape again to leave");
            }
        }
        if !self.still {
            self.time += dt;
        }
        if let (Some(name), Some(character)) = (self.current(), &mut self.character) {
            character.show(&name, self.time);
        }
        Go::Stay
    }

    /// The arrow keys (held: on after a while, 60 steps a second), the mouse dragged with its right button, and
    /// Ctrl+C/V/R/Z move the offset of the animation shown.
    fn move_offset(&mut self, dt: f32, shift: bool, ctrl: bool, moved: Vec2) {
        let step = if shift { 10.0 } else { 1.0 };
        let keys = |check: fn(KeyCode) -> bool| vec2(check(KeyCode::Left) as i32 as f32 - check(KeyCode::Right) as i32 as f32, check(KeyCode::Up) as i32 as f32 - check(KeyCode::Down) as i32 as f32);
        let mut offset = self.offset();
        let before = offset;
        offset += keys(is_key_pressed) * step;
        let down = keys(is_key_down);
        if down != Vec2::ZERO {
            self.held.0 += dt;
            if self.held.0 > 0.6 {
                self.held.1 += dt;
                while self.held.1 > 1.0 / 60.0 {
                    offset += down * step;
                    self.held.1 -= 1.0 / 60.0;
                }
            }
        } else {
            self.held = (0.0, 0.0);
        }
        if is_mouse_right_down() && !self.ui.took_click() {
            offset -= moved / self.zoom;
        }
        if ctrl && is_key_pressed(KeyCode::C) {
            self.copied = offset;
        } else if ctrl && is_key_pressed(KeyCode::V) {
            self.undo_offset = Some(offset);
            offset = self.copied;
        } else if ctrl && is_key_pressed(KeyCode::R) {
            self.undo_offset = Some(offset);
            offset = Vec2::ZERO;
        } else if ctrl && is_key_pressed(KeyCode::Z) {
            offset = self.undo_offset.unwrap_or(offset);
        }
        if offset != before {
            self.set_offset(offset);
        }
    }

    /// The two boxes at the right: Ghost and Settings; Animations and Character.
    fn panels(&mut self) {
        let small = Rect::new(WIDTH - 275.0, 45.0, 250.0, 110.0);
        self.tabs[0] = self.ui.tabs(small, &["Ghost", "Settings"], self.tabs[0]);
        let (x, y) = (small.x + 10.0, small.y + 10.0);
        if self.tabs[0] == 0 {
            if self.ui.button(x, y, 110.0, if self.ghost.is_some() { "Hide Ghost" } else { "Make Ghost" }) {
                self.ghost = match self.ghost {
                    Some(_) => None,
                    None => self.current().map(|name| (name, self.time)),
                };
            }
            self.ui.label(x, y + 30.0, "Opacity:");
            if let Some(alpha) = self.ui.stepper("ghost alpha", x + 80.0, y + 32.0, 50.0, self.ghost_alpha as f64, 0.1, 0.0, 1.0) {
                self.ghost_alpha = alpha as f32;
            }
        } else {
            // (the list first: open, it drops down over the rest and takes its clicks before they do)
            let chosen = self.characters.iter().position(|name| *name == self.name).unwrap_or(0);
            if let Some(index) = self.ui.dropdown("character", x, y, 120.0, &self.characters, chosen) {
                let name = self.characters[index].clone();
                self.load(&name);
            }
            if let Some(player) = self.ui.check(x, y + 30.0, "Playable Character", self.player) {
                self.player = player;
                self.changed();
                self.rebuild();
            }
            if self.ui.button(x + 140.0, y, 90.0, "Reload Char") {
                let name = self.name.clone();
                self.load(&name);
            }
        }
        let big = Rect::new(WIDTH - 375.0, 195.0, 350.0, 290.0);
        self.big_box(big);
    }

    fn big_box(&mut self, area: Rect) {
        self.tabs[1] = self.ui.tabs(area, &["Animations", "Character"], self.tabs[1]);
        let (x, y) = (area.x + 10.0, area.y + 10.0);
        if self.tabs[1] == 0 { self.animations_tab(x, y) } else { self.character_tab(x, y) }
    }

    fn animations_tab(&mut self, x: f32, y: f32) {
        // (the list first: open, it drops down over the form and takes its clicks before it does)
        let names: Vec<String> = self.animations().iter().map(|animation| animation["anim"].as_str().unwrap_or_default().to_owned()).collect();
        self.ui.label(x, y, "Animations:");
        if let Some(index) = self.ui.dropdown("animation", x, y + 18.0, 150.0, &names, self.animation) {
            (self.animation, self.time, self.still) = (index, 0.0, false);
            self.show_form();
        }
        let form = &mut self.form;
        self.ui.label(x, y + 46.0, "Animation name:");
        if let Some(text) = self.ui.input("anim", x, y + 64.0, 150.0, &form.0) {
            form.0 = text;
        }
        self.ui.label(x + 170.0, y + 46.0, "Framerate:");
        if let Some(fps) = self.ui.stepper("fps", x + 170.0, y + 64.0, 50.0, form.3, 1.0, 0.0, 240.0) {
            form.3 = fps;
        }
        self.ui.label(x, y + 92.0, "Animation Symbol Name/Prefix:");
        if let Some(text) = self.ui.input("prefix", x, y + 110.0, 200.0, &form.1) {
            form.1 = text;
        }
        if let Some(looped) = self.ui.check(x + 210.0, y + 110.0, "Loop", form.4) {
            form.4 = looped;
        }
        self.ui.label(x, y + 138.0, "ADVANCED - Animation Indices:");
        if let Some(text) = self.ui.input("indices", x, y + 156.0, 320.0, &form.2) {
            form.2 = text;
        }
        if self.ui.button(x + 50.0, y + 196.0, 110.0, "Add/Update") {
            self.add_animation();
        }
        if self.ui.button(x + 170.0, y + 196.0, 110.0, "Remove") {
            self.remove_animation();
        }
    }

    /// The form's animation added, or put in the place of the one of its name (keeping its offset).
    fn add_animation(&mut self) {
        let (anim, prefix, indices, fps, looped) = self.form.clone();
        if anim.trim().is_empty() {
            return self.say("An animation needs a name");
        }
        let indices: Vec<i64> = indices.split(',').filter_map(|index| index.trim().parse().ok()).collect();
        if !self.json["animations"].is_array() {
            self.json["animations"] = json!([]);
        }
        let list = self.json["animations"].as_array_mut().expect("made an array");
        let place = list.iter().position(|animation| animation["anim"].as_str() == Some(anim.as_str()));
        let offsets = place.map_or(json!([0, 0]), |place| list[place]["offsets"].clone());
        let animation = json!({"offsets": offsets, "loop": looped, "fps": fps, "anim": anim, "indices": indices, "name": prefix});
        match place {
            Some(place) => list[place] = animation,
            None => list.push(animation),
        }
        self.animation = place.unwrap_or(list.len() - 1);
        self.time = 0.0;
        self.changed();
        self.rebuild();
        if self.current().and_then(|name| self.character.as_ref()?.frames_of(&name)).is_none() {
            self.say(format!("No frames of \"{prefix}\" in the sheet"));
        }
    }

    fn remove_animation(&mut self) {
        let anim = self.form.0.clone();
        let Some(list) = self.json["animations"].as_array_mut() else { return };
        let before = list.len();
        list.retain(|animation| animation["anim"].as_str() != Some(anim.as_str()));
        if list.len() != before {
            self.animation = 0;
            self.changed();
            self.rebuild();
            self.show_form();
        }
    }

    fn character_tab(&mut self, x: f32, y: f32) {
        let text = |json: &Value, field: &str| json[field].as_str().unwrap_or_default().to_owned();
        self.ui.label(x, y, "Image file name:");
        if let Some(image) = self.ui.input("image", x, y + 18.0, 200.0, &text(&self.json, "image")) {
            self.json["image"] = json!(image);
            self.changed();
            self.rebuild();
        }
        if self.ui.button(x + 210.0, y + 17.0, 110.0, "Reload Image") {
            self.rebuild();
        }
        for (index, (field, words)) in [("healthicon", "Health icon name:"), ("vocals_file", "Vocals File Postfix:")].into_iter().enumerate() {
            let x = x + 165.0 * index as f32;
            self.ui.label(x, y + 44.0, words);
            if let Some(value) = self.ui.input(field, x, y + 62.0, 150.0, &text(&self.json, field)) {
                self.json[field] = json!(value);
                self.changed();
                self.rebuild();
            }
        }
        let number = |json: &Value, field: &str, usual: f64| json[field].as_f64().unwrap_or(usual);
        self.ui.label(x, y + 88.0, "Sing Anim. length:");
        if let Some(value) = self.ui.stepper("sing duration", x, y + 106.0, 50.0, number(&self.json, "sing_duration", 4.0), 0.1, 0.0, 999.0) {
            self.set("sing_duration", json!((value * 10.0).round() / 10.0));
        }
        self.ui.label(x + 175.0, y + 88.0, "Scale:");
        if let Some(value) = self.ui.stepper("scale", x + 175.0, y + 106.0, 50.0, number(&self.json, "scale", 1.0), 0.1, 0.05, 10.0) {
            self.set("scale", json!((value * 100.0).round() / 100.0));
        }
        for (index, (field, words)) in [("flip_x", "Flip X"), ("no_antialiasing", "No Antialiasing")].into_iter().enumerate() {
            if let Some(on) = self.ui.check(x + 165.0 * index as f32, y + 132.0, words, self.json[field].as_bool().unwrap_or(false)) {
                self.set(field, json!(on));
            }
        }
        for (index, (field, words)) in [("position", "Character X/Y:"), ("camera_position", "Camera X/Y:")].into_iter().enumerate() {
            let (x, y) = (x + 165.0 * index as f32, y + 158.0);
            self.ui.label(x, y, words);
            let now = pair(&self.json[field]);
            for (axis, value) in [now.x, now.y].into_iter().enumerate() {
                let id = [["position x", "position y"], ["camera x", "camera y"]][index][axis];
                if let Some(value) = self.ui.stepper(id, x, y + 18.0 + 26.0 * axis as f32, 50.0, value as f64, 10.0, -9000.0, 9000.0) {
                    let mut both = [now.x as i64, now.y as i64];
                    both[axis] = value.round() as i64;
                    self.set(field, json!(both));
                }
            }
        }
        self.ui.label(x, y + 218.0, "Health Bar R/G/B:");
        let colors = self.json["healthbar_colors"].as_array().cloned().unwrap_or_else(|| vec![json!(161), json!(161), json!(161)]);
        for channel in 0..3 {
            let value = colors.get(channel).and_then(Value::as_f64).unwrap_or(161.0);
            if let Some(value) = self.ui.stepper(["red", "green", "blue"][channel], x + 105.0 * channel as f32, y + 236.0, 40.0, value, 20.0, 0.0, 255.0) {
                let mut colors = colors.clone();
                colors.resize(3, json!(161));
                colors[channel] = json!(value.round() as i64);
                self.set("healthbar_colors", json!(colors));
            }
        }
        if self.ui.button(x + 210.0, y + 44.0 + 230.0, 110.0, "Save Character") {
            self.save();
        }
    }

    fn set(&mut self, field: &str, value: Value) {
        self.json[field] = value;
        self.changed();
        self.rebuild();
    }

    /// Over the character's file (a new one in the folder's characters/), the one before kept as `.bak`.
    fn save(&mut self) {
        self.json["_editor_isPlayer"] = json!(self.player);
        let relative = format!("characters/{}.json", self.name);
        let base = self.paths.roots().iter().find(|root| root.ends_with("mods")).and_then(|mods| mods.parent()).map(Path::to_path_buf).unwrap_or_default();
        let file = crate::chart_editor::save_target(&base, &self.folder, &relative, self.paths.find(&relative));
        let done = funkin_core::files::replace(&file, &crate::chart_editor::print(&self.json, 0));
        self.unsaved &= done.is_err();
        self.say(done.unwrap_or_else(|error| error));
    }

    pub fn draw(&self, skin: &Skin) {
        clear_background(Color::from_hex(0x666666));
        // the stage, through the editor's camera (its back a little slower, as the stage's)
        set_camera(&Camera2D { target: self.camera, zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT) * self.zoom, ..Default::default() });
        let scroll = self.camera - vec2(WIDTH, HEIGHT) / 2.0;
        let behind = |picture: &Option<Texture2D>, at: Vec2, scale: f32| {
            if let Some(picture) = picture {
                let at = at + scroll * 0.1;
                draw_texture_ex(picture, at.x, at.y, WHITE, DrawTextureParams { dest_size: Some(picture.size() * scale), ..Default::default() });
            }
        };
        behind(&self.pictures[0], vec2(-600.0, -200.0), 1.0);
        behind(&self.pictures[1], vec2(-650.0, 600.0), 1.1);
        if self.silhouettes {
            for (picture, at) in [(&self.pictures[2], OPPONENT_AT + vec2(4.0, -1.0)), (&self.pictures[3], PLAYER_AT + vec2(6.0, 348.0))] {
                if let Some(picture) = picture {
                    draw_texture(picture, at.x, at.y, Color { a: 0.25, ..WHITE });
                }
            }
        }
        if let Some(character) = &self.character {
            if let Some((pose, time)) = &self.ghost {
                character.draw_faded(pose, *time, self.ghost_alpha);
            }
            character.draw();
        }
        // the cross where the camera looks at it in a song
        let point = self.camera_point() + vec2(20.0, 20.0);
        draw_rectangle(point.x - 20.0, point.y - 2.0, 40.0, 4.0, WHITE);
        draw_rectangle(point.x - 2.0, point.y - 20.0, 4.0, 40.0, WHITE);

        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        let text = |words: &str, x: f32, y: f32, color: Color| skin.text(words, x, y, 16, color);
        let centered = |words: &str, y: f32, color: Color| text(words, (WIDTH - measure_text(words, skin.font.as_ref(), 16, 1.0).width) / 2.0, y, color);
        for (index, animation) in self.animations().iter().enumerate() {
            let words = format!("{}: [{}, {}]", animation["anim"].as_str().unwrap_or_default(), pair(&animation["offsets"]).x, pair(&animation["offsets"]).y);
            text(&words, 10.0, 48.0 + 18.0 * index as f32, if index == self.animation { Color::from_hex(0xFFFF00) } else { WHITE });
        }
        centered(&format!("Zoom: {}x", (self.zoom * 100.0).round() / 100.0), 66.0, WHITE);
        match self.current().and_then(|name| self.character.as_ref()?.frames_of(&name)) {
            Some((length, fps)) if length > 0 => centered(&format!("Frames: ( {} / {} )", ((self.time * fps) as usize).min(length - 1), length - 1), 91.0, WHITE),
            _ => centered("ERROR: No Animation Found", 91.0, Color::from_hex(0xFF0000)),
        }
        // the health bar in the character's colour, and its icon
        let color = self.character.as_ref().map_or(GRAY, |character| character.color);
        draw_rectangle(26.0, HEIGHT - 79.0, 608.0, 26.0, BLACK);
        draw_rectangle(30.0, HEIGHT - 75.0, 600.0, 18.0, color);
        if let Some(icon) = self.character.as_ref().and_then(|character| character.icon.as_ref()) {
            let faces = (icon.width() / icon.height()).round().max(1.0);
            let source = Rect::new(0.0, 0.0, icon.width() / faces, icon.height());
            draw_texture_ex(icon, 0.0, HEIGHT - 150.0, WHITE, DrawTextureParams { source: Some(source), ..Default::default() });
        }
        text("Press F1 for Help", WIDTH - 160.0, HEIGHT - 10.0, WHITE);
        if let Some((words, when)) = &self.said {
            let alpha = (1.0 - (when.elapsed().as_secs_f32() - 2.0).max(0.0)).min(1.0);
            if alpha > 0.0 {
                text(words, 10.0, HEIGHT - 170.0, Color { a: alpha, ..WHITE });
            }
        }
        self.ui.draw(skin.font.as_ref());
        if self.help {
            draw_rectangle(0.0, 0.0, WIDTH, HEIGHT, Color { a: 0.6, ..BLACK });
            for (index, line) in HELP.iter().enumerate() {
                centered(line, HEIGHT / 2.0 + (index as f32 - HELP.len() as f32 / 2.0) * 32.0 + 16.0, WHITE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_over_the_file_it_read_or_into_the_mods_folder() {
        let base = std::env::temp_dir().join(format!("funkin-character-editor-{}", std::process::id()));
        let own = base.join("mods/Mine/characters/mine.json");
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        std::fs::write(&own, r#"{"image": "nowhere", "scale": 1, "animations": [{"anim": "idle", "name": "idle0", "offsets": [0, 0]}]}"#).unwrap();
        let mut editor = CharacterEditor::open(&base, "Mine", "mine");
        assert!(!editor.player, "not a player's name, and the file does not say");
        editor.form = ("singUP".to_owned(), "up0".to_owned(), "1, 2".to_owned(), 30.0, false);
        editor.add_animation();
        editor.set_offset(vec2(-3.4, 7.0));
        editor.save();
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&own).unwrap()).unwrap();
        assert_eq!(saved["animations"][1], json!({"offsets": [-3, 7], "loop": false, "fps": 30.0, "anim": "singUP", "indices": [1, 2], "name": "up0"}));
        assert_eq!(saved["_editor_isPlayer"], json!(false));
        assert!(own.with_extension("json.bak").is_file());
        // a character with no file yet goes into the folder's characters/
        let mut fresh = CharacterEditor::open(&base, "Mine", "newcomer");
        fresh.save();
        assert!(base.join("mods/Mine/characters/newcomer.json").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }
}
