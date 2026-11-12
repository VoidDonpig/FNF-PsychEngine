//! The dialogue portrait editor (states/editors/DialogueCharacterEditorState.hx): a dialogue portrait's file
//! (`images/dialogue/<name>.json`) edited with the game's dialogue showing it; on the Animations tab, the talking and
//! resting frames of an expression drawn see-through over each other, their offsets moved with WASD and the arrow keys;
//! saved over its file, the one before kept as `.bak`.
// shortcut: the camera (J/K/L/I, Q/E zoom) does not move here; the view is the game's own.
use crate::gfx::*;
use crate::speech::Speech;
use crate::ui::Ui;
use crate::{Skin, HEIGHT, WIDTH};
use funkin_core::content::Paths;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;

const TIP_MAIN: &str = "W/S - Change animation    Space - Play the line again    H - Hide the box";
const TIP_OFFSETS: &str = "WASD - Move Looping animation offset (Red)    Arrow Keys - Move Idle/Finished offset (Blue)    Shift: 10x    H - Ghosts";

pub enum Go {
    Stay,
    Leave,
}

pub struct PortraitEditor {
    base: PathBuf,
    folder: String,
    names: Vec<String>,
    name: String,
    json: Value,
    /// The expression shown (its place in the file's list), the tab (Animations, Character), which ghosts show (both,
    /// the talking one, the resting one), whether the box is hidden.
    expression: usize,
    tab: usize,
    ghosts: usize,
    hidden: bool,
    speech: Option<Speech>,
    ui: Ui,
    /// The Animations tab's form: name, looping frames' prefix, resting frames' prefix.
    form: [String; 3],
    unsaved: bool,
    leaving: bool,
    said: Option<(String, Instant)>,
}

impl PortraitEditor {
    pub fn open(base: &Path, folder: &str) -> PortraitEditor {
        let paths = Paths::new(base, folder, "");
        let mut names: Vec<String> = paths.roots().iter().filter_map(|root| std::fs::read_dir(root.join("images/dialogue")).ok()).flatten().flatten()
            .map(|file| file.path()).filter(|path| path.extension().is_some_and(|ending| ending == "json"))
            .filter_map(|path| Some(path.file_stem()?.to_string_lossy().into_owned())).collect();
        names.sort();
        names.dedup();
        let mut editor = PortraitEditor {
            base: base.to_path_buf(), folder: folder.to_owned(), names, name: String::new(), json: Value::Null, expression: 0, tab: 1, ghosts: 0,
            hidden: false, speech: None, ui: Ui::default(), form: Default::default(), unsaved: false, leaving: false, said: None,
        };
        editor.load("bf");
        editor
    }

    fn paths(&self) -> Paths {
        Paths::new(&self.base, &self.folder, "")
    }

    fn load(&mut self, name: &str) {
        let read = self.paths().find(&format!("images/dialogue/{name}.json")).and_then(|file| std::fs::read_to_string(file).ok()).and_then(|text| serde_json::from_str::<Value>(&text).ok());
        let json = read.unwrap_or_else(|| json!({"image": "BF_Dialogue", "dialogue_pos": "left", "scale": 1, "position": [0, 0], "animations": []}));
        (self.name, self.json, self.expression, self.unsaved, self.leaving) = (name.to_owned(), json, 0, false, false);
        self.fill_form();
        self.show();
    }

    fn animations(&self) -> &[Value] {
        self.json["animations"].as_array().map_or(&[], Vec::as_slice)
    }

    fn current(&self) -> String {
        self.animations().get(self.expression).and_then(|animation| animation["anim"].as_str()).unwrap_or_default().to_owned()
    }

    fn fill_form(&mut self) {
        let animation = self.animations().get(self.expression).cloned().unwrap_or_default();
        self.form = ["anim", "loop_name", "idle_name"].map(|field| animation[field].as_str().unwrap_or_default().to_owned());
    }

    fn say(&mut self, words: impl Into<String>) {
        self.said = Some((words.into(), Instant::now()));
    }

    /// The portrait shown again, saying a line in the expression chosen.
    fn show(&mut self) {
        let line = json!({"portrait": self.name, "expression": self.current(), "text": "Lorem ipsum dolor sit amet", "boxState": "normal", "speed": 0.05});
        match Speech::from_json(&self.paths(), &json!({"dialogue": [line]}), None, Some((&self.name, &self.json))) {
            Ok(speech) => self.speech = Some(speech),
            Err(error) => {
                self.speech = None;
                self.say(error);
            }
        }
    }

    fn changed(&mut self) {
        (self.unsaved, self.leaving) = (true, false);
        self.show();
    }

    pub fn update(&mut self) -> Go {
        let typing = self.ui.typing();
        self.ui.begin();
        self.panel();
        if let Some(speech) = &mut self.speech {
            speech.update(get_frame_time().min(0.1), false, false);
            speech.cues.clear();
        }
        if typing {
            return Go::Stay;
        }
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let count = self.animations().len();
        if self.tab == 1 {
            if is_key_pressed(KeyCode::W) != is_key_pressed(KeyCode::S) && count > 0 {
                self.expression = (self.expression + if is_key_pressed(KeyCode::S) { 1 } else { count - 1 }) % count;
                self.fill_form();
                self.show();
            }
            if is_key_pressed(KeyCode::Space) {
                self.show();
            }
        } else if count > 0 {
            // WASD: the talking frames' offset; the arrow keys: the resting frames'
            let step = if shift { 10 } else { 1 };
            for (field, keys) in [("loop_offsets", [KeyCode::A, KeyCode::W, KeyCode::D, KeyCode::S]), ("idle_offsets", [KeyCode::Left, KeyCode::Up, KeyCode::Right, KeyCode::Down])] {
                let moved = [(keys[0], 0, 1), (keys[1], 1, 1), (keys[2], 0, -1), (keys[3], 1, -1)].into_iter().filter(|(key, ..)| is_key_pressed(*key)).fold([0, 0], |mut moved, (_, axis, sign)| {
                    moved[axis] += sign * step;
                    moved
                });
                if moved != [0, 0] {
                    let now = &self.json["animations"][self.expression][field];
                    let offsets = [0, 1].map(|axis| now[axis].as_i64().unwrap_or(0) + moved[axis]);
                    self.json["animations"][self.expression][field] = json!(offsets);
                    self.changed();
                }
            }
        }
        if is_key_pressed(KeyCode::H) {
            if self.tab == 0 { self.ghosts = (self.ghosts + 1) % 3 } else { self.hidden = !self.hidden }
        }
        if is_key_pressed(KeyCode::Escape) {
            if !self.unsaved || self.leaving {
                return Go::Leave;
            }
            self.leaving = true;
            self.say("Unsaved changes: Escape again to leave");
        }
        Go::Stay
    }

    fn panel(&mut self) {
        let types = Rect::new(900.0, HEIGHT - 210.0, 120.0, 160.0);
        self.ui.tabs(types, &["Character Type"], 0);
        let side = match self.json["dialogue_pos"].as_str() { Some("center") => 1, Some("right") => 2, _ => 0 };
        for (place, words) in ["Left", "Center", "Right"].into_iter().enumerate() {
            if self.ui.check(types.x + 10.0, types.y + 20.0 + 40.0 * place as f32, words, side == place).is_some() {
                self.json["dialogue_pos"] = json!(["left", "center", "right"][place]);
                self.changed();
            }
        }
        let area = Rect::new(types.x + types.w + 10.0, HEIGHT - 280.0, 240.0, 250.0);
        self.tab = self.ui.tabs(area, &["Animations", "Character"], self.tab);
        let (x, y) = (area.x + 10.0, area.y + 10.0);
        if self.tab == 0 { self.animations_tab(x, y) } else { self.character_tab(x, y) }
    }

    fn animations_tab(&mut self, x: f32, y: f32) {
        let names: Vec<String> = self.animations().iter().map(|animation| animation["anim"].as_str().unwrap_or_default().to_owned()).collect();
        self.ui.label(x, y, "Animations:");
        if let Some(index) = self.ui.dropdown("portrait animation", x, y + 18.0, 150.0, &names, self.expression) {
            self.expression = index;
            self.fill_form();
            self.show();
        }
        for (index, (id, words)) in [("portrait anim", "Animation name:"), ("loop name", "Loop name on .XML file:"), ("idle name", "Idle/Finished name on .XML file:")].into_iter().enumerate() {
            let top = y + 46.0 + 42.0 * index as f32;
            self.ui.label(x, top, words);
            if let Some(text) = self.ui.input(id, x, top + 18.0, 200.0, &self.form[index]) {
                self.form[index] = text;
            }
        }
        if self.ui.button(x, y + 176.0, 100.0, "Add/Update") {
            let [anim, looping, resting] = self.form.clone();
            if anim.trim().is_empty() {
                return self.say("An animation needs a name");
            }
            if !self.json["animations"].is_array() {
                self.json["animations"] = json!([]);
            }
            let list = self.json["animations"].as_array_mut().expect("made an array");
            let place = list.iter().position(|animation| animation["anim"].as_str() == Some(anim.as_str()));
            let kept = |field: &str| place.map_or(json!([0, 0]), |place| list[place][field].clone());
            let animation = json!({"anim": anim, "loop_name": looping, "loop_offsets": kept("loop_offsets"), "idle_name": resting, "idle_offsets": kept("idle_offsets")});
            match place {
                Some(place) => list[place] = animation,
                None => list.push(animation),
            }
            self.expression = place.unwrap_or(list.len() - 1);
            self.changed();
        }
        if self.ui.button(x + 110.0, y + 176.0, 100.0, "Remove") {
            let anim = self.form[0].clone();
            if let Some(list) = self.json["animations"].as_array_mut() {
                list.retain(|animation| animation["anim"].as_str() != Some(anim.as_str()));
            }
            self.expression = 0;
            self.fill_form();
            self.changed();
        }
    }

    fn character_tab(&mut self, x: f32, y: f32) {
        // (the list first: open, it drops down over the rest)
        self.ui.label(x, y + 160.0, "Load Character:");
        let chosen = self.names.iter().position(|name| *name == self.name).unwrap_or(0);
        let picked = self.ui.dropdown("portraits", x, y + 178.0, 120.0, &self.names, chosen);
        self.ui.label(x, y, "Image file name:");
        if let Some(image) = self.ui.input("portrait image", x, y + 18.0, 120.0, self.json["image"].as_str().unwrap_or_default()) {
            self.json["image"] = json!(image);
            self.changed();
        }
        self.ui.label(x, y + 42.0, "Position Offset:");
        for axis in 0..2 {
            let now = self.json["position"][axis].as_f64().unwrap_or(0.0);
            if let Some(value) = self.ui.stepper(["portrait x", "portrait y"][axis], x + 110.0 * axis as f32, y + 60.0, 50.0, now, 10.0, -2000.0, 2000.0) {
                let mut both = [0, 1].map(|other| self.json["position"][other].as_f64().unwrap_or(0.0));
                both[axis] = value.round();
                self.json["position"] = json!(both);
                self.changed();
            }
        }
        self.ui.label(x, y + 86.0, "Scale:");
        if let Some(scale) = self.ui.stepper("portrait scale", x, y + 104.0, 50.0, self.json["scale"].as_f64().unwrap_or(1.0), 0.05, 0.1, 10.0) {
            self.json["scale"] = json!((scale * 100.0).round() / 100.0);
            self.changed();
        }
        if let Some(rough) = self.ui.check(x + 110.0, y + 104.0, "No Antialiasing", self.json["no_antialiasing"].as_bool().unwrap_or(false)) {
            self.json["no_antialiasing"] = json!(rough);
            self.changed();
        }
        if self.ui.button(x + 130.0, y + 178.0, 90.0, "Save") {
            self.save();
        }
        if let Some(index) = picked {
            let name = self.names[index].clone();
            self.load(&name);
        }
    }

    /// Over its file, the one before kept as `.bak`, or into the folder's images/dialogue/ for a new one.
    fn save(&mut self) {
        let relative = format!("images/dialogue/{}.json", self.name);
        let path = crate::chart_editor::save_target(&self.base, &self.folder, &relative, self.paths().find(&relative));
        let done = funkin_core::files::replace(&path, &crate::chart_editor::print(&self.json, 0));
        self.unsaved &= done.is_err();
        self.say(done.unwrap_or_else(|error| error));
    }

    pub fn draw(&self, skin: &Skin) {
        clear_background(Color::from_hex(0x808080));
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        let current = self.current();
        let animation = self.animations().get(self.expression).cloned().unwrap_or_default();
        match (&self.speech, self.tab) {
            // the two frames over each other (H: one of them alone), as the Haxe editor's ghosts
            (Some(speech), 0) => {
                if self.ghosts != 2 {
                    speech.draw_pose(&self.name, &current, false, if self.ghosts == 1 { 1.0 } else { 0.6 });
                }
                if self.ghosts != 1 {
                    speech.draw_pose(&self.name, &current, true, if self.ghosts == 2 { 1.0 } else { 0.6 });
                }
                let pair = |field: &str| format!("[{}, {}]", animation[field][0].as_i64().unwrap_or(0), animation[field][1].as_i64().unwrap_or(0));
                skin.text(&format!("Loop: {}", pair("loop_offsets")), 10.0, 70.0, 32, Color::from_hex(0xFF0000));
                skin.text(&format!("Idle: {}", pair("idle_offsets")), 10.0, 106.0, 32, Color::from_hex(0x0000FF));
            }
            (Some(speech), _) if !self.hidden => speech.draw(),
            _ => {}
        }
        skin.text(if self.tab == 0 { TIP_OFFSETS } else { TIP_MAIN }, 10.0, 24.0, 16, WHITE);
        if self.tab == 1 {
            let count = self.animations().len();
            let words = if count == 0 { "ERROR! NO ANIMATIONS FOUND".to_owned() } else { format!("Animation: {current} ({} / {count}) - Press W or S to scroll", self.expression + 1) };
            skin.text(&words, 10.0, 46.0, 16, WHITE);
        }
        if let Some((words, when)) = &self.said {
            if when.elapsed().as_secs_f32() < 3.0 {
                skin.text(words, 10.0, 140.0, 20, WHITE);
            }
        }
        self.ui.draw(skin.font.as_ref());
    }
}
