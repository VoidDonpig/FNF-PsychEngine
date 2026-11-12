//! The note splash editor (states/editors/NoteSplashEditorState.hx): a splash skin's file
//! (`images/noteSplashes/<name>.json`) edited, its animations played over four strums; offsets moved with the arrow keys
//! (Shift: ten times), Ctrl+C/V/R copy, paste and reset them; saved over its file, the one before kept as `.bak`.
// shortcut: the Shader tab (the colours a splash's RGB shader replaces), Template and Convert TXT are not here.
use crate::atlas::Sheet;
use crate::gfx::*;
use crate::ui::Ui;
use crate::{Skin, DIRECTIONS, HEIGHT, WIDTH};
use funkin_core::content::Paths;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub enum Go {
    Stay,
    Leave,
}

pub struct SplashEditor {
    base: PathBuf,
    folder: String,
    /// The skins there are (their file names), the one edited, what its file holds, its sheet.
    names: Vec<String>,
    name: String,
    json: Value,
    sheet: Option<Sheet>,
    /// The animation chosen (by name), the form of the Animation tab (name, prefix, indices, note data, fps from, to).
    chosen: String,
    form: (String, String, String, f64, f64, f64),
    /// A splash playing: its animation, on which lane, since when.
    playing: Option<(String, usize, Instant)>,
    copied: [f64; 2],
    ui: Ui,
    unsaved: bool,
    leaving: bool,
    said: Option<(String, Instant)>,
}

/// Where a lane's strum stands in the editor.
fn strum_at(lane: usize) -> Vec2 {
    vec2(WIDTH / 2.0 + (lane as f32 - 1.5) * 112.0, HEIGHT / 2.0 - 60.0)
}

impl SplashEditor {
    pub fn open(base: &Path, folder: &str) -> SplashEditor {
        let paths = Paths::new(base, folder, "");
        let mut names: Vec<String> = paths.roots().iter().filter_map(|root| std::fs::read_dir(root.join("images/noteSplashes")).ok()).flatten().flatten()
            // (a skin with only an old .txt config is listed too: Convert TXT makes its file)
            .map(|file| file.path()).filter(|path| path.extension().is_some_and(|ending| ending == "json" || ending == "txt") && path.file_stem().is_some_and(|stem| stem != "list"))
            .filter_map(|path| Some(path.file_stem()?.to_string_lossy().into_owned())).collect();
        names.sort();
        names.dedup();
        let first = names.iter().find(|name| *name == "noteSplashes").or(names.first()).cloned().unwrap_or_else(|| "noteSplashes".to_owned());
        let mut editor = SplashEditor {
            base: base.to_path_buf(), folder: folder.to_owned(), names, name: String::new(), json: Value::Null, sheet: None, chosen: String::new(),
            form: Default::default(), playing: None, copied: [0.0; 2], ui: Ui::default(), unsaved: false, leaving: false, said: None,
        };
        editor.load(&first);
        editor
    }

    fn paths(&self) -> Paths {
        Paths::new(&self.base, &self.folder, "")
    }

    fn load(&mut self, name: &str) {
        let paths = self.paths();
        let read = paths.find(&format!("images/noteSplashes/{name}.json")).and_then(|file| std::fs::read_to_string(file).ok()).and_then(|text| serde_json::from_str::<Value>(&text).ok());
        let json = read.unwrap_or_else(blank);
        self.sheet = paths.find(&format!("images/noteSplashes/{name}.png")).and_then(|path| Sheet::load(&path).ok());
        (self.name, self.json, self.unsaved, self.leaving, self.playing) = (name.to_owned(), json, false, false, None);
        let first = self.animation_names().into_iter().next().unwrap_or_default();
        self.choose(&first);
    }

    fn animation_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.json["animations"].as_object().map_or(Vec::new(), |animations| animations.keys().cloned().collect());
        names.sort();
        names
    }

    fn choose(&mut self, name: &str) {
        let animation = self.json["animations"][name].clone();
        let indices = animation["indices"].as_array().map_or(String::new(), |list| list.iter().map(Value::to_string).collect::<Vec<_>>().join(","));
        self.chosen = name.to_owned();
        self.form = (
            name.to_owned(), animation["prefix"].as_str().unwrap_or_default().to_owned(), indices, animation["noteData"].as_f64().unwrap_or(0.0),
            animation["fps"][0].as_f64().unwrap_or(22.0), animation["fps"][1].as_f64().unwrap_or(26.0),
        );
    }

    fn say(&mut self, words: impl Into<String>) {
        self.said = Some((words.into(), Instant::now()));
    }

    fn changed(&mut self) {
        (self.unsaved, self.leaving) = (true, false);
    }

    fn offsets(&self) -> [f64; 2] {
        let offsets = &self.json["animations"][&self.chosen]["offsets"];
        [offsets[0].as_f64().unwrap_or(0.0), offsets[1].as_f64().unwrap_or(0.0)]
    }

    /// The chosen animation splashes on the lane its note data says.
    fn splash(&mut self) {
        let lane = self.json["animations"][&self.chosen]["noteData"].as_u64().unwrap_or(0) as usize % 4;
        self.playing = Some((self.chosen.clone(), lane, Instant::now()));
    }

    pub fn update(&mut self) -> Go {
        let typing = self.ui.typing();
        self.ui.begin();
        self.panel();
        if typing {
            return Go::Stay;
        }
        let known = self.json["animations"][&self.chosen].is_object();
        if known {
            let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
            let step = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) { 10.0 } else { 1.0 };
            let mut offsets = self.offsets();
            let before = offsets;
            if ctrl && is_key_pressed(KeyCode::C) {
                self.copied = offsets;
            } else if ctrl && is_key_pressed(KeyCode::V) {
                offsets = self.copied;
            } else if ctrl && is_key_pressed(KeyCode::R) {
                offsets = [0.0; 2];
            }
            offsets[0] += (is_key_pressed(KeyCode::Left) as i32 - is_key_pressed(KeyCode::Right) as i32) as f64 * step;
            offsets[1] += (is_key_pressed(KeyCode::Up) as i32 - is_key_pressed(KeyCode::Down) as i32) as f64 * step;
            if offsets != before {
                self.json["animations"][&self.chosen]["offsets"] = json!(offsets);
                self.changed();
            }
            if offsets != before || is_key_pressed(KeyCode::Space) {
                self.splash();
            }
        }
        // a click on a strum splashes the chosen animation there
        if is_mouse_pressed() && !self.ui.took_click() {
            let mouse = mouse_position(vec2(WIDTH, HEIGHT));
            if let Some(lane) = (0..4).find(|lane| mouse.distance(strum_at(*lane)) < 56.0) {
                self.playing = Some((self.chosen.clone(), lane, Instant::now()));
            }
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
        let area = Rect::new(WIDTH - 330.0, HEIGHT - 280.0, 300.0, 240.0);
        self.ui.tabs(area, &["Animation"], 0);
        let (x, y) = (area.x + 10.0, area.y + 10.0);
        // (the lists first: open, they drop down over the rest)
        let names = self.animation_names();
        self.ui.label(x, y + 150.0, "Animations:");
        let chosen = names.iter().position(|name| *name == self.chosen).unwrap_or(0);
        if let Some(index) = self.ui.dropdown("splash animations", x, y + 168.0, 130.0, &names, chosen) {
            let name = names[index].clone();
            self.choose(&name);
            self.splash();
        }
        let properties = Rect::new(30.0, HEIGHT - 200.0, 280.0, 170.0);
        let skins = self.names.clone();
        let skin = skins.iter().position(|name| *name == self.name).unwrap_or(0);
        let picked_skin = self.ui.dropdown("splash skins", properties.x + 10.0, properties.y + 28.0, 200.0, &skins, skin);
        let form = &mut self.form;
        for (index, (id, words)) in [("splash anim", "Animation Name:"), ("splash prefix", "Animation Prefix:")].into_iter().enumerate() {
            let top = y + 46.0 * index as f32;
            self.ui.label(x, top, words);
            let value = if index == 0 { &mut form.0 } else { &mut form.1 };
            if let Some(text) = self.ui.input(id, x, top + 18.0, 130.0, value) {
                *value = text;
            }
        }
        self.ui.label(x + 150.0, y, "Note Data:");
        if let Some(data) = self.ui.stepper("splash data", x + 150.0, y + 18.0, 40.0, form.3, 1.0, 0.0, 999.0) {
            form.3 = data;
        }
        self.ui.label(x + 150.0, y + 46.0, "Indices (OPTIONAL):");
        if let Some(text) = self.ui.input("splash indices", x + 150.0, y + 64.0, 130.0, &form.2) {
            form.2 = text;
        }
        for (index, words) in ["Minimum FPS:", "Maximum FPS:"].into_iter().enumerate() {
            let x = x + 150.0 * index as f32;
            self.ui.label(x, y + 92.0, words);
            let value = if index == 0 { &mut form.4 } else { &mut form.5 };
            if let Some(fps) = self.ui.stepper(["splash min fps", "splash max fps"][index], x, y + 110.0, 40.0, *value, 1.0, 1.0, 120.0) {
                *value = fps;
            }
        }
        if self.ui.button(x + 150.0, y + 167.0, 60.0, if self.json["animations"][&self.form.0].is_object() { "Update" } else { "Add" }) {
            self.add_animation();
        }
        if self.ui.button(x + 220.0, y + 167.0, 60.0, "Remove") {
            let name = self.form.0.clone();
            if let Some(animations) = self.json["animations"].as_object_mut() {
                if animations.remove(&name).is_some() {
                    self.changed();
                    let first = self.animation_names().into_iter().next().unwrap_or_default();
                    self.choose(&first);
                }
            }
        }
        self.ui.tabs(properties, &["Properties"], 0);
        self.ui.label(properties.x + 10.0, properties.y + 10.0, "Image:");
        self.ui.label(properties.x + 10.0, properties.y + 52.0, "Scale:");
        if let Some(scale) = self.ui.stepper("splash scale", properties.x + 10.0, properties.y + 70.0, 50.0, self.json["scale"].as_f64().unwrap_or(1.0), 0.1, 0.0, 4.0) {
            self.json["scale"] = json!((scale * 100.0).round() / 100.0);
            self.changed();
        }
        for (index, (field, words)) in [("allowRGB", "Allow RGB"), ("allowPixel", "Allow Pixel")].into_iter().enumerate() {
            if let Some(on) = self.ui.check(properties.x + 10.0 + 130.0 * index as f32, properties.y + 100.0, words, self.json[field].as_bool().unwrap_or(true)) {
                self.json[field] = json!(on);
                self.changed();
            }
        }
        if self.ui.button(properties.x + 10.0, properties.y + 132.0, 70.0, "Save") {
            self.save();
        }
        if self.ui.button(properties.x + 88.0, properties.y + 132.0, 80.0, "Template") {
            self.replace_with(blank());
        }
        if self.ui.button(properties.x + 176.0, properties.y + 132.0, 100.0, "Convert TXT") {
            let relative = format!("images/noteSplashes/{}.txt", self.name);
            match self.paths().find(&relative).and_then(|file| std::fs::read_to_string(file).ok()) {
                Some(text) => self.replace_with(from_txt(&text)),
                None => self.say(format!("No {relative} to convert")),
            }
        }
        if let Some(index) = picked_skin {
            let name = skins[index].clone();
            if self.ui.sure("splash skins", index, self.unsaved) { self.load(&name) } else { self.say(crate::ui::UNSAVED) }
        }
    }

    /// The skin's config in place of the one edited (Save writes it).
    fn replace_with(&mut self, json: Value) {
        self.json = json;
        self.changed();
        let first = self.animation_names().into_iter().next().unwrap_or_default();
        self.choose(&first);
    }

    fn add_animation(&mut self) {
        let (name, prefix, indices, data, from, to) = self.form.clone();
        if name.trim().is_empty() {
            return self.say("An animation needs a name");
        }
        if !self.json["animations"].is_object() {
            self.json["animations"] = Value::Object(Map::new());
        }
        let offsets = self.json["animations"][&name]["offsets"].as_array().cloned().map_or(json!([0, 0]), Value::Array);
        let indices: Vec<i64> = indices.split(',').filter_map(|index| index.trim().parse().ok()).collect();
        self.json["animations"][&name] = json!({"prefix": prefix, "fps": [from, to], "offsets": offsets, "indices": indices, "name": name, "noteData": data as i64});
        self.changed();
        self.choose(&name);
        self.splash();
    }

    /// Over its file, the one before kept as `.bak`, or into the folder's images/noteSplashes/ for a new one.
    fn save(&mut self) {
        let relative = format!("images/noteSplashes/{}.json", self.name);
        let path = crate::chart_editor::save_target(&self.base, &self.folder, &relative, self.paths().find(&relative));
        let done = funkin_core::files::replace(&path, &crate::chart_editor::print(&self.json, 0));
        self.unsaved &= done.is_err();
        self.say(done.unwrap_or_else(|error| error));
    }

    pub fn draw(&self, skin: &Skin) {
        clear_background(Color::from_hex(0x222222));
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        for lane in 0..4 {
            let at = strum_at(lane);
            skin.frame(&format!("arrow{}0000", DIRECTIONS[lane].to_uppercase()), at.x, at.y, None, None, 1.0);
        }
        // the splash where Psych's NoteSplash puts it: the strum's corner less 0.95 and 1 note widths, less (10, 10) and its
        // offset, scaled around its middle; at the middle of its speed range
        if let (Some((name, lane, since)), Some(sheet)) = (&self.playing, &self.sheet) {
            let animation = &self.json["animations"][name];
            let indices: Vec<u32> = animation["indices"].as_array().into_iter().flatten().filter_map(|index| index.as_u64()).map(|index| index as u32).collect();
            let frames = sheet.animation(animation["prefix"].as_str().unwrap_or_default(), &indices);
            let fps = (animation["fps"][0].as_f64().unwrap_or(24.0) + animation["fps"][1].as_f64().unwrap_or(24.0)) / 2.0;
            let step = (since.elapsed().as_secs_f64() * fps) as usize;
            if let Some(frame) = frames.get(step) {
                let scale = self.json["scale"].as_f64().unwrap_or(1.0) as f32;
                let offset = vec2(animation["offsets"][0].as_f64().unwrap_or(0.0) as f32, animation["offsets"][1].as_f64().unwrap_or(0.0) as f32);
                const NOTE: f32 = 160.0 * 0.7;
                let corner = strum_at(*lane) - vec2(NOTE, NOTE) / 2.0;
                let at = corner - vec2(NOTE * 0.95, NOTE) - (vec2(10.0, 10.0) + offset) + frame.size * (1.0 - scale) / 2.0;
                sheet.draw(*frame, at.x, at.y, scale, false, WHITE);
            }
        }
        let offsets = self.offsets();
        let lines = [
            format!("Copied Offsets: [{}, {}]", self.copied[0], self.copied[1]),
            if self.chosen.is_empty() { "Current Animation: NONE".to_owned() } else { format!("Current Animation: {} ([{}, {}])", self.chosen, offsets[0], offsets[1]) },
            "Arrow Keys - Change Offset (Shift: 10x)   Ctrl+C/V/R - Copy, Paste, Reset   Space or a click on a strum - Splash".to_owned(),
        ];
        for (row, line) in lines.iter().enumerate() {
            skin.text(line, 10.0, 26.0 + 22.0 * row as f32, 18, WHITE);
        }
        if self.sheet.is_none() {
            skin.text(&format!("images/noteSplashes/{}.png not found", self.name), 10.0, 100.0, 18, Color::from_hex(0xFF5555));
        }
        if let Some((words, when)) = &self.said {
            if when.elapsed().as_secs_f32() < 3.0 {
                skin.text(words, 10.0, 124.0, 20, WHITE);
            }
        }
        self.ui.draw(skin.font.as_ref());
    }
}

/// A splash config with no animations (NoteSplash.createConfig).
fn blank() -> Value {
    json!({"animations": {}, "scale": 1, "allowRGB": true, "allowPixel": true, "rgb": null})
}

/// An old splash config (.txt: the animations' prefix, then "<min fps> <max fps>", then an offset "x y" per line) as
/// the JSON one (NoteSplashEditorState.parseTxt): an animation per lane and offset line, "purple", "blue2"...
fn from_txt(text: &str) -> Value {
    let mut config = blank();
    let lines: Vec<&str> = text.split('\n').collect();
    if text.trim().is_empty() || lines.len() < 2 || lines[0].trim().is_empty() {
        return config;
    }
    let prefix = lines[0].trim_end();
    let numbers: Vec<&str> = lines[1].split_whitespace().collect();
    let fps = [(0, 22), (1, 26)].map(|(at, usual)| numbers.get(at).and_then(|number| number.parse::<i64>().ok()).unwrap_or(usual));
    let mut offsets: Vec<[f64; 2]> = lines.iter().skip(2).map(|line| line.trim()).filter(|line| !line.is_empty())
        .map(|line| { let mut parts = line.split(' ').map(|part| part.parse::<f64>().ok().filter(|value| !value.is_nan()).unwrap_or(0.0)); [parts.next().unwrap_or(0.0), parts.next().unwrap_or(0.0)] })
        .collect();
    if offsets.is_empty() {
        offsets.push([0.0, 0.0]);
    }
    let (mut index, mut round) = (0, 1);
    loop {
        for color in ["purple", "blue", "green", "red"] {
            let name = if round <= 1 { color.to_owned() } else { format!("{color}{round}") };
            config["animations"][&name] = json!({"name": name, "noteData": index, "prefix": format!("{prefix} {color} {round}"), "indices": [], "offsets": offsets[index % offsets.len()], "fps": fps});
            index += 1;
        }
        if index >= offsets.len() {
            return config;
        }
        round += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_txt_configs_become_an_animation_per_lane_and_offset() {
        let config = from_txt("note splash\n24 28\n1 2\n\n3 x\n4 4\n5 5\n6 6\n");
        let animations = config["animations"].as_object().unwrap();
        // five offsets: two rounds of four lanes, the offsets taken in turn and round again
        assert_eq!(animations.len(), 8);
        assert_eq!(config["animations"]["purple"], json!({"name": "purple", "noteData": 0, "prefix": "note splash purple 1", "indices": [], "offsets": [1.0, 2.0], "fps": [24, 28]}));
        assert_eq!(config["animations"]["blue"]["offsets"], json!([3.0, 0.0]));
        assert_eq!(config["animations"]["red2"]["prefix"], json!("note splash red 2"));
        assert_eq!(config["animations"]["blue2"]["offsets"], json!([1.0, 2.0]));
        assert_eq!(from_txt("only a name"), blank());
    }
}
