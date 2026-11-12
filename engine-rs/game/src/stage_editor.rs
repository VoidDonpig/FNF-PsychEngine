//! The stage editor (states/editors/StageEditorState.hx): a stage file (`stages/<name>.json`) shown as the game builds it
//! from its objects, with the three characters where it puts them; its objects listed, picked (in the list or with a click)
//! and moved (arrow keys, right mouse button), their settings and the stage's in the Haxe editor's boxes; saved over its
//! file, the one before kept as `.bak`.
// shortcut: no animated sprites are made here (the Haxe editor's animation box), nor are images picked from a file dialog;
// add them when asked.
use crate::gfx::*;
use crate::scene::Scene;
use crate::ui::Ui;
use crate::{Skin, HEIGHT, WIDTH};
use funkin_core::chart::Chart;
use funkin_core::content::Paths;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub enum Go {
    Stay,
    Leave,
}

/// The characters' entries in a stage's objects, and their places in its file (its keys for where they stand).
const CHARACTERS: [(&str, &str); 3] = [("dad", "opponent"), ("boyfriend", "boyfriend"), ("gf", "girlfriend")];

/// A stage the Haxe editor starts a new one as.
fn new_stage() -> Value {
    json!({
        "directory": "", "defaultZoom": 0.9, "isPixelStage": false, "stageUI": "normal",
        "boyfriend": [770, 100], "girlfriend": [400, 130], "opponent": [100, 100], "hide_girlfriend": false,
        "camera_boyfriend": [0, 0], "camera_opponent": [0, 0], "camera_girlfriend": [0, 0], "camera_speed": 1,
        "objects": [{"type": "gf"}, {"type": "dad"}, {"type": "boyfriend"}],
        "_editorMeta": {"dad": "dad", "gf": "gf", "boyfriend": "bf"},
    })
}

pub struct StageEditor {
    base: PathBuf,
    folder: String,
    /// The stages there are, the one edited (its file name) and what its file holds; the scene built from it.
    names: Vec<String>,
    name: String,
    json: Value,
    scene: Option<Scene>,
    /// The object picked (its place in the file's objects).
    picked: Option<usize>,
    /// The view: its middle (stage coordinates) and zoom; where the mouse was last frame (moving with the right button).
    camera: Vec2,
    zoom: f32,
    mouse: Vec2,
    /// The first of the sprite list's rows shown.
    scroll: usize,
    boxes: bool,
    help: bool,
    ui: Ui,
    tab: usize,
    unsaved: bool,
    leaving: bool,
    said: Option<(String, Instant)>,
}

impl StageEditor {
    /// Opens on a mod folder's first stage, else "stage", else the first there is.
    pub fn open(base: &Path, folder: &str) -> StageEditor {
        let paths = Paths::new(base, folder, "");
        let mut names: Vec<String> = paths.roots().iter().filter_map(|root| std::fs::read_dir(root.join("stages")).ok()).flatten().flatten()
            .map(|file| file.path()).filter(|path| path.extension().is_some_and(|ending| ending == "json"))
            .filter_map(|path| Some(path.file_stem()?.to_string_lossy().into_owned())).collect();
        names.sort();
        names.dedup();
        // in a mod folder, its own first stage
        let own = std::fs::read_dir(base.join("mods").join(folder).join("stages")).into_iter().flatten().flatten()
            .filter_map(|file| file.path().file_stem().map(|stem| stem.to_string_lossy().into_owned())).filter(|name| !folder.is_empty() && names.contains(name)).min();
        let first = own.or_else(|| names.iter().find(|name| *name == "stage").or(names.first()).cloned()).unwrap_or_else(|| "stage".to_owned());
        let mut editor = StageEditor {
            base: base.to_path_buf(), folder: folder.to_owned(), names, name: String::new(), json: Value::Null, scene: None, picked: None,
            camera: Vec2::ZERO, zoom: 1.0, mouse: Vec2::ZERO, scroll: 0, boxes: true, help: false, ui: Ui::default(), tab: 1,
            unsaved: false, leaving: false, said: None,
        };
        editor.load(&first);
        editor
    }

    fn paths(&self) -> Paths {
        Paths::new(&self.base, &self.folder, "")
    }

    fn say(&mut self, words: impl Into<String>) {
        self.said = Some((words.into(), Instant::now()));
    }

    fn changed(&mut self) {
        (self.unsaved, self.leaving) = (true, false);
    }

    fn load(&mut self, name: &str) {
        let read = self.paths().find(&format!("stages/{name}.json")).and_then(|file| std::fs::read_to_string(file).ok())
            .and_then(|text| serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).ok());
        let mut json = read.unwrap_or_else(new_stage);
        // a file without objects (the game's own stages draw theirs in code) gets the three characters to place
        if !json["objects"].is_array() {
            json["objects"] = json!([{"type": "gf"}, {"type": "dad"}, {"type": "boyfriend"}]);
        }
        (self.name, self.json, self.picked, self.scroll, self.unsaved, self.leaving) = (name.to_owned(), json, None, 0, false, false);
        self.rebuild();
        self.zoom = self.json["defaultZoom"].as_f64().unwrap_or(0.9) as f32;
        // looking at the player, as the Haxe editor opens
        self.camera = self.scene.as_ref().and_then(|scene| scene.character_bounds()[1]).map_or(vec2(WIDTH, HEIGHT) / 2.0, |(_, look)| look);
    }

    /// The scene built again: its characters (the stage's `_editorMeta` names them) and asset folder from the file.
    fn rebuild(&mut self) {
        let meta = |key: &str, usual: &str| self.json["_editorMeta"][key].as_str().filter(|name| !name.is_empty()).unwrap_or(usual).to_owned();
        let chart = Chart { stage: self.name.clone(), opponent: meta("dad", "dad"), gf: meta("gf", "gf"), player: meta("boyfriend", "bf"), ..Default::default() };
        match Scene::load(&self.base, &self.folder, &chart, false, false, false) {
            Ok(mut scene) => {
                scene.restage(&self.json);
                self.scene = Some(scene);
            }
            Err(error) => self.say(format!("Could not show it: {error}")),
        }
    }

    /// The stage drawn again from its file as it now is (the characters kept).
    fn restage(&mut self) {
        if let Some(scene) = &mut self.scene {
            scene.restage(&self.json);
        }
    }

    fn objects(&self) -> &[Value] {
        self.json["objects"].as_array().map_or(&[], Vec::as_slice)
    }

    /// Which character an object is (0 the opponent, 1 the player, 2 the girlfriend), if it is one.
    fn character(object: &Value) -> Option<usize> {
        match object["type"].as_str()? {
            "dad" | "dadGroup" => Some(0),
            "boyfriend" | "boyfriendGroup" => Some(1),
            "gf" | "gfGroup" => Some(2),
            _ => None,
        }
    }

    /// How an object is called in the list.
    fn title(object: &Value) -> String {
        match StageEditor::character(object) {
            Some(who) => ["- Dad -", "- Boyfriend -", "- Girlfriend -"][who].to_owned(),
            None => object["name"].as_str().map_or_else(|| object["type"].as_str().unwrap_or("?").to_owned(), str::to_owned),
        }
    }

    /// Where the picked object is: a character's place in the stage, else its own.
    fn place(&self, index: usize) -> Vec2 {
        let object = &self.objects()[index];
        let pair = |value: &Value| vec2(value[0].as_f64().unwrap_or(0.0) as f32, value[1].as_f64().unwrap_or(0.0) as f32);
        match StageEditor::character(object) {
            Some(who) => pair(&self.json[CHARACTERS[who].1]),
            None => vec2(object["x"].as_f64().unwrap_or(0.0) as f32, object["y"].as_f64().unwrap_or(0.0) as f32),
        }
    }

    fn set_place(&mut self, index: usize, at: Vec2) {
        let round = |value: f32| (value * 100.0).round() as f64 / 100.0;
        match StageEditor::character(&self.objects()[index]) {
            Some(who) => self.json[CHARACTERS[who].1] = json!([round(at.x), round(at.y)]),
            None => {
                self.json["objects"][index]["x"] = json!(round(at.x));
                self.json["objects"][index]["y"] = json!(round(at.y));
            }
        }
        self.changed();
        self.restage();
    }

    /// The stage coordinates under a point of the screen.
    fn world(&self, screen: Vec2) -> Vec2 {
        self.camera + (screen - vec2(WIDTH, HEIGHT) / 2.0) / self.zoom
    }

    pub fn update(&mut self) -> Go {
        let typing = self.ui.typing();
        self.ui.begin();
        self.panels();
        let dt = get_frame_time().min(0.1);
        let mouse = mouse_position(vec2(WIDTH, HEIGHT));
        let moved_mouse = mouse - std::mem::replace(&mut self.mouse, mouse);
        if let Some(scene) = &mut self.scene {
            scene.view(self.camera, self.zoom, dt);
        }
        if typing {
            return Go::Stay;
        }
        let (shift, ctrl) = (is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift), is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl));
        if ctrl && is_key_pressed(KeyCode::S) {
            self.save();
        }
        // the view: J/K/L/I move it, E/Q zoom, R back to the stage's zoom (Shift: four times as fast, Ctrl: a quarter)
        let speed = if shift { 4.0 } else if ctrl { 0.25 } else { 1.0 };
        let held = |key| is_key_down(key) as i32 as f32;
        self.camera += vec2(held(KeyCode::L) - held(KeyCode::J), held(KeyCode::K) - held(KeyCode::I)) * 500.0 * speed * dt / self.zoom;
        self.zoom = (self.zoom * (1.0 + (held(KeyCode::E) - held(KeyCode::Q)) * speed * dt)).clamp(0.05, 3.0);
        if is_key_pressed(KeyCode::R) && !ctrl {
            self.zoom = self.json["defaultZoom"].as_f64().unwrap_or(0.9) as f32;
        }
        if is_key_pressed(KeyCode::F12) {
            self.boxes = !self.boxes;
        }
        if is_key_pressed(KeyCode::F1) {
            self.help = !self.help;
        }
        // a click on the stage picks what is drawn on top there
        if is_mouse_pressed() && !self.ui.took_click() {
            let at = self.world(mouse);
            self.picked = self.under(at).or(self.picked);
        }
        if let Some(index) = self.picked.filter(|index| *index < self.objects().len()) {
            // the arrow keys move it (Shift: four times as far, Ctrl: by a pixel); the right mouse button drags it
            let step = if shift { 40.0 } else if ctrl { 1.0 } else { 10.0 };
            let pressed = |key| is_key_pressed(key) as i32 as f32;
            let by = vec2(pressed(KeyCode::Right) - pressed(KeyCode::Left), pressed(KeyCode::Down) - pressed(KeyCode::Up)) * step;
            let dragged = if is_mouse_right_down() && !self.ui.took_click() { moved_mouse / self.zoom } else { Vec2::ZERO };
            if by + dragged != Vec2::ZERO {
                let at = self.place(index) + by + dragged;
                self.set_place(index, at);
            }
            if is_key_pressed(KeyCode::Delete) {
                self.delete();
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

    /// The object drawn on top at a point of the stage.
    fn under(&self, at: Vec2) -> Option<usize> {
        let scene = self.scene.as_ref()?;
        let inside = |rect: &Rect| at.x >= rect.x && at.x <= rect.x + rect.w && at.y >= rect.y && at.y <= rect.y + rect.h;
        let characters = scene.character_bounds();
        // the list is drawn in its order: the last one there is the one on top
        (0..self.objects().len()).rev().find(|index| match StageEditor::character(&self.objects()[*index]) {
            Some(who) => characters[who].is_some_and(|(rect, _)| inside(&rect)),
            None => scene.object_bounds().iter().any(|(source, rect)| source == index && inside(rect)),
        })
    }

    fn delete(&mut self) {
        let Some(index) = self.picked else { return };
        if StageEditor::character(&self.objects()[index]).is_some() {
            return self.say("The characters stay: hide the girlfriend in the Data tab");
        }
        if let Some(objects) = self.json["objects"].as_array_mut() {
            objects.remove(index);
        }
        self.picked = None;
        self.changed();
        self.restage();
    }

    /// A new object in the middle of the view, after the one picked (or on top of all).
    fn add(&mut self, kind: &str) {
        let names: Vec<String> = self.objects().iter().filter_map(|object| object["name"].as_str().map(str::to_owned)).collect();
        let name = (1..).map(|number| format!("{kind}{number}")).find(|name| !names.contains(name)).unwrap_or_default();
        let at = self.camera;
        let object = match kind {
            "square" => json!({"type": "square", "name": name, "x": at.x.round(), "y": at.y.round(), "scale": [100, 100], "scroll": [1, 1], "color": "FFFFFF", "alpha": 1, "angle": 0, "filters": 3}),
            _ => json!({"type": "sprite", "name": name, "image": "", "x": at.x.round(), "y": at.y.round(), "scale": [1, 1], "scroll": [1, 1], "color": "FFFFFF", "alpha": 1, "angle": 0, "antialiasing": true, "flipX": false, "flipY": false, "filters": 3}),
        };
        let place = self.picked.map_or(self.objects().len(), |index| index + 1);
        if let Some(objects) = self.json["objects"].as_array_mut() {
            objects.insert(place.min(objects.len()), object);
        }
        self.picked = Some(place);
        self.tab = 2;
        self.changed();
        self.restage();
    }

    /// The picked object a place up or down the list (drawn later or earlier).
    fn shift(&mut self, later: bool) {
        let Some(index) = self.picked else { return };
        let other = if later { index + 1 } else { index.wrapping_sub(1) };
        if let Some(objects) = self.json["objects"].as_array_mut().filter(|objects| other < objects.len()) {
            objects.swap(index, other);
            self.picked = Some(other);
            self.changed();
            self.restage();
        }
    }

    fn panels(&mut self) {
        // the sprite list at the left
        let list = Rect::new(25.0, 60.0, 250.0, 240.0);
        self.ui.tabs(list, &["Sprite List"], 0);
        let titles: Vec<String> = self.objects().iter().map(StageEditor::title).collect();
        const ROWS: usize = 9;
        self.scroll = self.scroll.min(titles.len().saturating_sub(ROWS));
        for (row, index) in (self.scroll..titles.len()).take(ROWS).enumerate() {
            let words = if Some(index) == self.picked { format!("> {}", titles[index]) } else { titles[index].clone() };
            if self.ui.button(list.x + 10.0, list.y + 8.0 + 20.0 * row as f32, 180.0, &words) {
                self.picked = Some(index);
            }
        }
        for (row, (words, go)) in [("^", -1i32), ("v", 1)].into_iter().enumerate() {
            if self.ui.button(list.x + 200.0, list.y + 8.0 + 20.0 * row as f32, 40.0, words) {
                self.scroll = (self.scroll as i32 + go * 3).max(0) as usize;
            }
        }
        let low = list.y + 196.0;
        if self.ui.button(list.x + 200.0, list.y + 58.0, 40.0, "Up") {
            self.shift(false);
        }
        if self.ui.button(list.x + 200.0, list.y + 78.0, 40.0, "Down") {
            self.shift(true);
        }
        if self.ui.button(list.x + 10.0, low, 70.0, "Sprite") {
            self.add("sprite");
        }
        if self.ui.button(list.x + 85.0, low, 70.0, "Square") {
            self.add("square");
        }
        if self.ui.button(list.x + 160.0, low, 80.0, "Delete") {
            self.delete();
        }

        // the stage: which, and saving
        let stage = Rect::new(WIDTH - 305.0, 30.0, 280.0, 80.0);
        self.ui.tabs(stage, &["Stage"], 0);
        let (x, y) = (stage.x + 10.0, stage.y + 8.0);
        let chosen = self.names.iter().position(|name| *name == self.name).unwrap_or(usize::MAX);
        let picked_stage = self.ui.dropdown("stages", x, y, 140.0, &self.names, chosen);
        self.ui.label(x, y + 26.0, "File name:");
        if let Some(name) = self.ui.input("stage name", x + 80.0, y + 26.0, 150.0, &self.name.clone()) {
            let name = name.trim().to_owned();
            if !name.is_empty() {
                self.name = name;
                self.changed();
            }
        }
        if self.ui.button(x + 150.0, y, 80.0, "Save") {
            self.save();
        }
        if let Some(index) = picked_stage {
            let name = self.names[index].clone();
            if self.ui.sure("stages", index, self.unsaved) { self.load(&name) } else { self.say(crate::ui::UNSAVED) }
        }

        let area = Rect::new(WIDTH - 305.0, 140.0, 280.0, 400.0);
        self.tab = self.ui.tabs(area, &["Meta", "Data", "Object"], self.tab);
        let (x, y) = (area.x + 10.0, area.y + 10.0);
        match self.tab {
            0 => self.meta_tab(x, y),
            1 => self.data_tab(x, y),
            _ => self.object_tab(x, y),
        }
    }

    /// The characters shown on the stage while it is edited (kept in the file's `_editorMeta`, as the Haxe editor does).
    fn meta_tab(&mut self, x: f32, y: f32) {
        for (index, (key, words, usual)) in [("boyfriend", "Boyfriend:", "bf"), ("gf", "Girlfriend:", "gf"), ("dad", "Opponent:", "dad")].into_iter().enumerate() {
            let top = y + 44.0 * index as f32;
            self.ui.label(x, top, words);
            let now = self.json["_editorMeta"][key].as_str().unwrap_or(usual).to_owned();
            if let Some(name) = self.ui.input(["meta bf", "meta gf", "meta dad"][index], x, top + 18.0, 150.0, &now) {
                if !self.json["_editorMeta"].is_object() {
                    self.json["_editorMeta"] = json!({});
                }
                self.json["_editorMeta"][key] = json!(name.trim());
                self.changed();
                self.rebuild();
            }
        }
        self.ui.label(x, y + 140.0, "UI Style (stageUI):");
        let ui = self.json["stageUI"].as_str().unwrap_or("normal").to_owned();
        if let Some(style) = self.ui.input("stage ui", x, y + 158.0, 150.0, &ui) {
            self.json["stageUI"] = json!(style.trim());
            self.json["isPixelStage"] = json!(style.trim() == "pixel");
            self.changed();
        }
    }

    fn data_tab(&mut self, x: f32, y: f32) {
        self.ui.label(x, y, "Asset folder (directory):");
        let directory = self.json["directory"].as_str().unwrap_or_default().to_owned();
        if let Some(directory) = self.ui.input("stage directory", x, y + 18.0, 150.0, &directory) {
            self.json["directory"] = json!(directory.trim());
            self.changed();
            self.rebuild();
        }
        if let Some(hidden) = self.ui.check(x, y + 46.0, "Hide Girlfriend?", self.json["hide_girlfriend"].as_bool().unwrap_or(false)) {
            self.json["hide_girlfriend"] = json!(hidden);
            self.changed();
            self.restage();
        }
        self.ui.label(x, y + 76.0, "Camera Offsets:");
        for (index, (key, words)) in [("camera_opponent", "Opponent:"), ("camera_girlfriend", "Girlfriend:"), ("camera_boyfriend", "Boyfriend:")].into_iter().enumerate() {
            let top = y + 96.0 + 42.0 * index as f32;
            self.ui.label(x, top, words);
            for axis in 0..2 {
                let id = [["cam dad x", "cam dad y"], ["cam gf x", "cam gf y"], ["cam bf x", "cam bf y"]][index][axis];
                let value = self.json[key][axis].as_f64().unwrap_or(0.0);
                if let Some(value) = self.ui.stepper(id, x + 120.0 * axis as f32, top + 18.0, 50.0, value, 50.0, -10000.0, 10000.0) {
                    let mut both = [self.json[key][0].as_f64().unwrap_or(0.0), self.json[key][1].as_f64().unwrap_or(0.0)];
                    both[axis] = value.round();
                    self.json[key] = json!(both);
                    self.changed();
                    self.restage();
                }
            }
        }
        self.ui.label(x, y + 228.0, "Zoom:");
        if let Some(zoom) = self.ui.stepper("stage zoom", x, y + 246.0, 50.0, self.json["defaultZoom"].as_f64().unwrap_or(0.9), 0.05, 0.05, 3.0) {
            self.json["defaultZoom"] = json!((zoom * 100.0).round() / 100.0);
            self.zoom = zoom as f32;
            self.changed();
            self.restage();
        }
        self.ui.label(x + 110.0, y + 228.0, "Camera Speed:");
        if let Some(speed) = self.ui.stepper("camera speed", x + 110.0, y + 246.0, 50.0, self.json["camera_speed"].as_f64().unwrap_or(1.0), 0.1, 0.0, 10.0) {
            self.json["camera_speed"] = json!((speed * 100.0).round() / 100.0);
            self.changed();
        }
    }

    /// The picked object's settings (a character's: where it stands).
    fn object_tab(&mut self, x: f32, y: f32) {
        let Some(index) = self.picked.filter(|index| *index < self.objects().len()) else {
            return self.ui.label(x, y, "Pick something in the list,\nor click it on the stage.");
        };
        let object = self.objects()[index].clone();
        let at = self.place(index);
        self.ui.label(x, y, "X/Y:");
        for axis in 0..2 {
            let value = if axis == 0 { at.x } else { at.y } as f64;
            if let Some(value) = self.ui.stepper(["object x", "object y"][axis], x + 120.0 * axis as f32, y + 18.0, 50.0, value, 10.0, -100000.0, 100000.0) {
                let mut at = at;
                if axis == 0 { at.x = value as f32 } else { at.y = value as f32 }
                self.set_place(index, at);
            }
        }
        if StageEditor::character(&object).is_some() {
            return;
        }
        let set = |editor: &mut StageEditor, field: &str, value: Value| {
            editor.json["objects"][index][field] = value;
            editor.changed();
            editor.restage();
        };
        self.ui.label(x, y + 44.0, "Name:");
        if let Some(name) = self.ui.input("object name", x, y + 62.0, 110.0, object["name"].as_str().unwrap_or_default()) {
            set(self, "name", json!(name.trim()));
        }
        if object["type"] != "square" {
            self.ui.label(x + 120.0, y + 44.0, "Image:");
            if let Some(image) = self.ui.input("object image", x + 120.0, y + 62.0, 110.0, object["image"].as_str().unwrap_or_default()) {
                set(self, "image", json!(image.trim()));
            }
        }
        for (row, (field, words, step, low, high, usual)) in [("scroll", "Scroll X/Y:", 0.05, -10.0, 10.0, 1.0), ("scale", "Scale X/Y:", if object["type"] == "square" { 10.0 } else { 0.05 }, 0.0, 10000.0, 1.0)].into_iter().enumerate() {
            let top = y + 88.0 + 44.0 * row as f32;
            self.ui.label(x, top, words);
            for axis in 0..2 {
                let id = [["scroll x", "scroll y"], ["scale x", "scale y"]][row][axis];
                let value = object[field][axis].as_f64().unwrap_or(usual);
                if let Some(value) = self.ui.stepper(id, x + 120.0 * axis as f32, top + 18.0, 50.0, value, step, low, high) {
                    let mut both = [object[field][0].as_f64().unwrap_or(usual), object[field][1].as_f64().unwrap_or(usual)];
                    both[axis] = (value * 100.0).round() / 100.0;
                    set(self, field, json!(both));
                }
            }
        }
        self.ui.label(x, y + 176.0, "Opacity:");
        if let Some(alpha) = self.ui.stepper("object alpha", x, y + 194.0, 50.0, object["alpha"].as_f64().unwrap_or(1.0), 0.1, 0.0, 1.0) {
            set(self, "alpha", json!((alpha * 100.0).round() / 100.0));
        }
        self.ui.label(x + 120.0, y + 176.0, "Angle:");
        if let Some(angle) = self.ui.stepper("object angle", x + 120.0, y + 194.0, 50.0, object["angle"].as_f64().unwrap_or(0.0), 5.0, -360.0, 360.0) {
            set(self, "angle", json!(angle));
        }
        self.ui.label(x, y + 220.0, "Color:");
        if let Some(color) = self.ui.input("object color", x, y + 238.0, 80.0, object["color"].as_str().unwrap_or("FFFFFF")) {
            set(self, "color", json!(color.trim()));
        }
        if object["type"] != "square" {
            for (column, (field, words, usual)) in [("flipX", "Flip X", false), ("flipY", "Flip Y", false)].into_iter().enumerate() {
                if let Some(on) = self.ui.check(x + 120.0 * column as f32, y + 266.0, words, object[field].as_bool().unwrap_or(usual)) {
                    set(self, field, json!(on));
                }
            }
            if let Some(on) = self.ui.check(x, y + 290.0, "Antialiasing", object["antialiasing"].as_bool().unwrap_or(true)) {
                set(self, "antialiasing", json!(on));
            }
        }
        // 1: shown on low quality, 2: shown otherwise
        let filters = object["filters"].as_u64().unwrap_or(3);
        for (column, (bit, words)) in [(1u64, "Low Quality"), (2, "High Quality")].into_iter().enumerate() {
            if let Some(on) = self.ui.check(x + 120.0 * column as f32, y + 316.0, words, filters & bit != 0) {
                set(self, "filters", json!(if on { filters | bit } else { filters & !bit }));
            }
        }
    }

    /// Over its file, the one before kept as `.bak`, or into the folder's stages/ for a new one.
    fn save(&mut self) {
        let relative = format!("stages/{}.json", self.name);
        let path = crate::chart_editor::save_target(&self.base, &self.folder, &relative, self.paths().find(&relative));
        let done = funkin_core::files::replace(&path, &crate::chart_editor::print(&self.json, 0));
        if done.is_ok() && !self.names.contains(&self.name) {
            self.names.push(self.name.clone());
            self.names.sort();
        }
        self.unsaved &= done.is_err();
        self.say(done.unwrap_or_else(|error| error));
    }

    pub fn draw(&self, skin: &Skin) {
        clear_background(Color::from_hex(0x101010));
        if let Some(scene) = &self.scene {
            scene.draw(&|_, _| {});
            // the picked object's box, and where the camera looks at each character in a song
            if self.boxes {
                let thin = 2.0 / self.zoom;
                let outline = |rect: Rect, color: Color| {
                    draw_rectangle(rect.x, rect.y, rect.w, thin, color);
                    draw_rectangle(rect.x, rect.y + rect.h - thin, rect.w, thin, color);
                    draw_rectangle(rect.x, rect.y, thin, rect.h, color);
                    draw_rectangle(rect.x + rect.w - thin, rect.y, thin, rect.h, color);
                };
                let characters = scene.character_bounds();
                if let Some(index) = self.picked.filter(|index| *index < self.objects().len()) {
                    let lime = Color::new(0.0, 1.0, 0.0, 0.8);
                    match StageEditor::character(&self.objects()[index]) {
                        Some(who) => characters[who].iter().for_each(|(rect, _)| outline(*rect, lime)),
                        None => scene.object_bounds().iter().filter(|(source, _)| *source == index).for_each(|(_, rect)| outline(*rect, lime)),
                    }
                }
                for (_, look) in characters.iter().flatten() {
                    draw_rectangle(look.x - 15.0 / self.zoom, look.y - thin / 2.0, 30.0 / self.zoom, thin, WHITE);
                    draw_rectangle(look.x - thin / 2.0, look.y - 15.0 / self.zoom, thin, 30.0 / self.zoom, WHITE);
                }
            }
        }
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        let words = format!("Stage: {}{}   Zoom: {:.2}   Camera: [{:.0}, {:.0}]", self.name, if self.unsaved { " *" } else { "" }, self.zoom, self.camera.x, self.camera.y);
        skin.text(&words, 25.0, HEIGHT - 12.0, 16, WHITE);
        skin.text("Press F1 for Help", WIDTH - 180.0, HEIGHT - 12.0, 16, WHITE);
        if let Some((words, when)) = &self.said {
            if when.elapsed().as_secs_f32() < 3.0 {
                skin.text(words, 25.0, HEIGHT - 36.0, 20, WHITE);
            }
        }
        self.ui.draw(skin.font.as_ref());
        if self.help {
            draw_rectangle(0.0, 0.0, WIDTH, HEIGHT, Color::new(0.0, 0.0, 0.0, 0.6));
            let lines = [
                "E/Q - Camera Zoom In/Out", "J/K/L/I - Move Camera", "R - Reset Camera Zoom", "Arrow Keys/Mouse & Right Click - Move Object",
                "Click - Pick what is drawn on top there", "Delete - Remove the picked object", "F12 - Toggle Selection Rectangle",
                "Hold Shift - Move Objects and Camera 4x faster", "Hold Control - Move Objects pixel-by-pixel and Camera 4x slower", "Ctrl+S - Save",
            ];
            for (row, line) in lines.iter().enumerate() {
                let width = measure_text(line, skin.font.as_ref(), 16, 1.0).width;
                skin.text(line, (WIDTH - width) / 2.0, HEIGHT / 2.0 + (row as f32 - lines.len() as f32 / 2.0) * 32.0 + 16.0, 16, WHITE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects_are_called_by_name_or_role_and_characters_found() {
        assert_eq!(StageEditor::title(&json!({"type": "boyfriend"})), "- Boyfriend -");
        assert_eq!(StageEditor::title(&json!({"type": "sprite", "name": "back"})), "back");
        assert_eq!(StageEditor::title(&json!({"type": "square"})), "square");
        assert_eq!(StageEditor::character(&json!({"type": "gfGroup"})), Some(2));
        assert_eq!(StageEditor::character(&json!({"type": "sprite"})), None);
    }
}
