//! The menu character editor (states/editors/MenuCharacterEditorState.hx): a story menu character's file
//! (`images/menucharacters/<name>.json`) edited in the place it stands in, the two others faded; its offset moved with
//! the arrow keys; saved over its file, the one before kept as `.bak`.
use crate::gfx::*;
use crate::menu::Menus;
use crate::ui::Ui;
use crate::{Skin, HEIGHT, WIDTH};
use funkin_core::content::Paths;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The characters standing in the three places while one is edited (the Haxe editor's defaultCharacters).
const STANDING: [&str; 3] = ["dad", "bf", "gf"];
const PLACES: [&str; 3] = ["Opponent", "Boyfriend", "Girlfriend"];

pub enum Go {
    Stay,
    Leave,
}

pub struct MenuCharacterEditor {
    base: PathBuf,
    folder: String,
    /// The characters there are (to load one), the one edited, and what its file holds.
    names: Vec<String>,
    name: String,
    json: Value,
    /// The place it is shown in (Opponent, Boyfriend, Girlfriend), and since when it cheers (Space, as Boyfriend).
    place: usize,
    cheered: Option<Instant>,
    ui: Ui,
    unsaved: bool,
    leaving: bool,
    said: Option<(String, Instant)>,
}

impl MenuCharacterEditor {
    /// Opens on bf (of a mod's folder, or the game's).
    pub fn open(base: &Path, folder: &str) -> MenuCharacterEditor {
        let paths = Paths::new(base, folder, "");
        let mut names: Vec<String> = paths.roots().iter().filter_map(|root| std::fs::read_dir(root.join("images/menucharacters")).ok()).flatten().flatten()
            .map(|file| file.path()).filter(|path| path.extension().is_some_and(|ending| ending == "json"))
            .filter_map(|path| Some(path.file_stem()?.to_string_lossy().into_owned())).collect();
        names.sort();
        names.dedup();
        let mut editor = MenuCharacterEditor {
            base: base.to_path_buf(), folder: folder.to_owned(), names, name: String::new(), json: Value::Null, place: 1, cheered: None, ui: Ui::default(),
            unsaved: false, leaving: false, said: None,
        };
        editor.load("bf");
        editor
    }

    fn paths(&self) -> Paths {
        Paths::new(&self.base, &self.folder, "")
    }

    fn load(&mut self, name: &str) {
        let read = self.paths().find(&format!("images/menucharacters/{name}.json")).and_then(|file| std::fs::read_to_string(file).ok()).and_then(|text| serde_json::from_str::<Value>(&text).ok());
        // (a new one: the Haxe editor's starting file)
        let json = read.unwrap_or_else(|| json!({"image": "Menu_Dad", "scale": 1, "position": [0, 0], "idle_anim": "M Dad Idle", "confirm_anim": "M Dad Idle", "flipX": false, "antialiasing": true}));
        (self.name, self.json, self.unsaved, self.leaving) = (name.to_owned(), json, false, false);
    }

    fn say(&mut self, words: impl Into<String>) {
        self.said = Some((words.into(), Instant::now()));
    }

    fn set(&mut self, field: &str, value: Value) {
        self.json[field] = value;
        (self.unsaved, self.leaving) = (true, false);
    }

    fn position(&self) -> Vec2 {
        vec2(self.json["position"][0].as_f64().unwrap_or(0.0) as f32, self.json["position"][1].as_f64().unwrap_or(0.0) as f32)
    }

    pub fn update(&mut self) -> Go {
        let typing = self.ui.typing();
        self.ui.begin();
        self.panel();
        if typing {
            return Go::Stay;
        }
        let step = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) { 10.0 } else { 1.0 };
        let moved = vec2(is_key_pressed(KeyCode::Left) as i32 as f32 - is_key_pressed(KeyCode::Right) as i32 as f32, is_key_pressed(KeyCode::Up) as i32 as f32 - is_key_pressed(KeyCode::Down) as i32 as f32);
        if moved != Vec2::ZERO {
            let at = self.position() + moved * step;
            self.set("position", json!([at.x as i64, at.y as i64]));
        }
        if is_key_pressed(KeyCode::Space) && self.place == 1 {
            self.cheered = Some(Instant::now());
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
        // load and save, between the two boxes, the list first (it drops down over what is under it)
        let chosen = self.names.iter().position(|name| *name == self.name).unwrap_or(0);
        if let Some(index) = self.ui.dropdown("menu characters", WIDTH / 2.0 - 180.0, 480.0, 150.0, &self.names, chosen) {
            let name = self.names[index].clone();
            self.load(&name);
        }
        if self.ui.button(WIDTH / 2.0 + 10.0, 479.0, 130.0, "Save Character") {
            self.save();
        }
        let types = Rect::new(100.0, HEIGHT - 210.0, 140.0, 160.0);
        self.ui.tabs(types, &["Character Type"], 0);
        for (place, words) in PLACES.iter().enumerate() {
            if self.ui.check(types.x + 10.0, types.y + 20.0 + 40.0 * place as f32, words, self.place == place).is_some() {
                (self.place, self.cheered) = (place, None);
            }
        }
        let area = Rect::new(WIDTH - 340.0, HEIGHT - 245.0, 240.0, 195.0);
        self.ui.tabs(area, &["Character"], 0);
        let (x, y) = (area.x + 10.0, area.y + 10.0);
        let text = |json: &Value, field: &str| json[field].as_str().unwrap_or_default().to_owned();
        for (index, (field, words, width)) in [("image", "Image file name:", 110.0), ("idle_anim", "Idle animation on the .XML:", 150.0), ("confirm_anim", "Confirm animation on the .XML:", 150.0)].into_iter().enumerate() {
            let top = y + 40.0 * index as f32;
            self.ui.label(x, top, words);
            if let Some(value) = self.ui.input(field, x, top + 18.0, width, &text(&self.json, field)) {
                self.set(field, json!(value));
            }
        }
        self.ui.label(x + 130.0, y, "Scale:");
        if let Some(scale) = self.ui.stepper("menu scale", x + 130.0, y + 18.0, 40.0, self.json["scale"].as_f64().unwrap_or(1.0), 0.05, 0.1, 30.0) {
            self.set("scale", json!((scale * 100.0).round() / 100.0));
        }
        if let Some(flip) = self.ui.check(x, y + 124.0, "Flip X", self.json["flipX"].as_bool().unwrap_or(false)) {
            self.set("flipX", json!(flip));
        }
        if let Some(smooth) = self.ui.check(x, y + 150.0, "Antialiasing", self.json["antialiasing"].as_bool().unwrap_or(true)) {
            self.set("antialiasing", json!(smooth));
        }
    }

    /// Over its file, the one before kept as `.bak`, or into the folder's images/menucharacters/ for a new one.
    fn save(&mut self) {
        let relative = format!("images/menucharacters/{}.json", self.name);
        let path = crate::chart_editor::save_target(&self.base, &self.folder, &relative, self.paths().find(&relative));
        let done = funkin_core::files::replace(&path, &crate::chart_editor::print(&self.json, 0));
        self.unsaved &= done.is_err();
        self.say(done.unwrap_or_else(|error| error));
    }

    pub fn draw(&self, skin: &Skin, menus: &mut Menus) {
        clear_background(BLACK);
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        draw_rectangle(0.0, 56.0, WIDTH, 386.0, Color::from_hex(0xF9CF51));
        for (place, name) in STANDING.iter().enumerate().filter(|(place, _)| *place != self.place) {
            menus.menu_character_named(&self.folder, name, place, 0.2);
        }
        let since = self.cheered.map(|when| when.elapsed().as_secs_f32());
        if !menus.menu_character(&self.folder, &self.json, self.place, since) {
            let words = format!("images/menucharacters/{}.png not found", self.json["image"].as_str().unwrap_or_default());
            skin.text(&words, 20.0, 300.0, 24, BLACK);
        }
        let at = self.position();
        skin.text(&format!("[{}, {}]", at.x, at.y), 20.0, 40.0, 32, Color { a: 0.7, ..WHITE });
        let tip = "Arrow Keys - Change Offset (Hold shift for 10x speed)";
        skin.text(tip, (WIDTH - measure_text(tip, skin.font.as_ref(), 16, 1.0).width) / 2.0, 556.0, 16, WHITE);
        let tip = "Space - Play \"Start Press\" animation (Boyfriend Character Type)";
        skin.text(tip, (WIDTH - measure_text(tip, skin.font.as_ref(), 16, 1.0).width) / 2.0, 590.0, 16, WHITE);
        skin.text(&format!("Editing: {}", self.name), 20.0, 462.0, 20, WHITE);
        if let Some((words, when)) = &self.said {
            if when.elapsed().as_secs_f32() < 3.0 {
                skin.text(words, 20.0, 484.0, 20, WHITE);
            }
        }
        self.ui.draw(skin.font.as_ref());
    }
}
