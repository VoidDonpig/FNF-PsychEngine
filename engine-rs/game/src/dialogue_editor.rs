//! The dialogue editor (states/editors/DialogueEditorState.hx): the lines of a dialogue file (the kind scripts start with
//! startDialogue) edited one at a time, each shown as the game shows it (speech.rs); saved over its file, the one
//! before kept as `.bak`.
use crate::gfx::*;
use crate::speech::{Cue, Speech};
use crate::ui::Ui;
use crate::{audio, Skin, WIDTH};
use funkin_core::content::Paths;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub enum Go {
    Stay,
    Leave,
}

/// A new line as the Haxe editor starts one.
fn new_line() -> Value {
    json!({"portrait": "bf", "expression": "talk", "text": "coolswag", "boxState": "normal", "speed": 0.05, "sound": ""})
}

pub struct DialogueEditor {
    base: PathBuf,
    folder: String,
    /// The folder's dialogue files (data/<song>/dialogue*.json), the one edited, what it holds, the line shown.
    files: Vec<PathBuf>,
    path: Option<PathBuf>,
    json: Value,
    line: usize,
    speech: Option<Speech>,
    sound: audio::Song,
    ui: Ui,
    unsaved: bool,
    leaving: bool,
    said: Option<(String, Instant)>,
}

impl DialogueEditor {
    pub fn open(base: &Path, folder: &str) -> DialogueEditor {
        let paths = Paths::new(base, folder, "");
        let mut files: Vec<PathBuf> = paths.roots().iter().filter_map(|root| std::fs::read_dir(root.join("data")).ok()).flatten().flatten()
            .filter_map(|song| std::fs::read_dir(song.path()).ok()).flatten().flatten().map(|file| file.path())
            .filter(|path| path.extension().is_some_and(|ending| ending == "json") && path.file_name().is_some_and(|name| name.to_string_lossy().starts_with("dialogue"))).collect();
        files.sort();
        let mut editor = DialogueEditor {
            base: base.to_path_buf(), folder: folder.to_owned(), files, path: None, json: json!({"dialogue": [new_line()]}), line: 0, speech: None,
            sound: audio::Song::load(&[]), ui: Ui::default(), unsaved: false, leaving: false, said: None,
        };
        editor.show();
        editor
    }

    fn paths(&self) -> Paths {
        Paths::new(&self.base, &self.folder, "")
    }

    fn lines(&self) -> &[Value] {
        self.json["dialogue"].as_array().map_or(&[], Vec::as_slice)
    }

    fn say(&mut self, words: impl Into<String>) {
        self.said = Some((words.into(), Instant::now()));
    }

    /// The line shown again from its start, as the game shows it.
    fn show(&mut self) {
        self.line = self.line.min(self.lines().len().saturating_sub(1));
        let line = self.lines().get(self.line).cloned().unwrap_or_else(new_line);
        match Speech::from_json(&self.paths(), &json!({"dialogue": [line]}), None) {
            Ok(speech) => self.speech = Some(speech),
            Err(error) => {
                self.speech = None;
                self.say(error);
            }
        }
    }

    fn set(&mut self, field: &str, value: Value) {
        self.json["dialogue"][self.line][field] = value;
        (self.unsaved, self.leaving) = (true, false);
        self.show();
    }

    /// The expressions the line's portrait has (images/dialogue/<portrait>.json).
    fn expressions(&self) -> Vec<String> {
        let portrait = self.lines().get(self.line).and_then(|line| line["portrait"].as_str()).unwrap_or("bf");
        let file = self.paths().find(&format!("images/dialogue/{portrait}.json")).and_then(|file| std::fs::read_to_string(file).ok());
        let json = file.and_then(|text| serde_json::from_str::<Value>(&text).ok()).unwrap_or_default();
        json["animations"].as_array().into_iter().flatten().filter_map(|animation| animation["anim"].as_str().map(str::to_owned)).collect()
    }

    pub fn update(&mut self) -> Go {
        let typing = self.ui.typing();
        self.ui.begin();
        self.panel();
        if let Some(speech) = &mut self.speech {
            speech.update(get_frame_time().min(0.1), false, false);
            for cue in std::mem::take(&mut speech.cues) {
                if let Cue::Sound(name) = cue {
                    if let Some(path) = Paths::new(&self.base, &self.folder, "").find(&format!("sounds/{name}.ogg")) {
                        self.sound.play_sound(&path, 1.0, false);
                    }
                }
            }
        }
        if typing {
            return Go::Stay;
        }
        let count = self.lines().len();
        if is_key_pressed(KeyCode::Space) {
            self.show();
        }
        // W/S: the portrait's expressions; A/D: the lines; O takes the line off, P adds one after it
        if is_key_pressed(KeyCode::W) != is_key_pressed(KeyCode::S) {
            let expressions = self.expressions();
            if !expressions.is_empty() {
                let now = self.lines()[self.line]["expression"].as_str().and_then(|name| expressions.iter().position(|known| known == name)).unwrap_or(0);
                let next = (now + if is_key_pressed(KeyCode::S) { 1 } else { expressions.len() - 1 }) % expressions.len();
                self.set("expression", json!(expressions[next]));
            }
        }
        if is_key_pressed(KeyCode::A) != is_key_pressed(KeyCode::D) && count > 0 {
            self.line = (self.line + if is_key_pressed(KeyCode::D) { 1 } else { count - 1 }) % count;
            self.show();
        }
        if is_key_pressed(KeyCode::O) {
            if let Some(lines) = self.json["dialogue"].as_array_mut() {
                lines.remove(self.line.min(lines.len() - 1));
                if lines.is_empty() {
                    lines.push(new_line());
                }
            }
            (self.unsaved, self.leaving) = (true, false);
            self.show();
        } else if is_key_pressed(KeyCode::P) {
            if let Some(lines) = self.json["dialogue"].as_array_mut() {
                lines.insert((self.line + 1).min(lines.len()), new_line());
            }
            self.line += 1;
            (self.unsaved, self.leaving) = (true, false);
            self.show();
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
        let area = Rect::new(WIDTH - 260.0, 30.0, 250.0, 230.0);
        self.ui.tabs(area, &["Dialogue Line"], 0);
        let (x, y) = (area.x + 10.0, area.y + 10.0);
        // (the file list first: open, it drops down over the rest)
        let names: Vec<String> = self.files.iter().map(|path| path.strip_prefix(&self.base).unwrap_or(path).display().to_string()).collect();
        self.ui.label(x, y + 172.0, "Load Dialogue:");
        let chosen = self.files.iter().position(|path| Some(path) == self.path.as_ref()).unwrap_or(0);
        let picked = self.ui.dropdown("dialogue files", x, y + 190.0, 230.0, &names, chosen);
        let line = self.lines().get(self.line).cloned().unwrap_or_else(new_line);
        let text = |field: &str| line[field].as_str().unwrap_or_default().to_owned();
        self.ui.label(x, y, "Character:");
        if let Some(portrait) = self.ui.input("portrait", x, y + 18.0, 80.0, &text("portrait")) {
            self.set("portrait", json!(portrait));
        }
        self.ui.label(x, y + 42.0, "Interval/Speed (ms):");
        if let Some(speed) = self.ui.stepper("speed", x, y + 60.0, 50.0, line["speed"].as_f64().unwrap_or(0.05), 0.005, 0.0, 0.5) {
            self.set("speed", json!((speed * 1000.0).round() / 1000.0));
        }
        if let Some(angry) = self.ui.check(x + 120.0, y + 60.0, "Angry Textbox", line["boxState"].as_str() == Some("angry")) {
            self.set("boxState", json!(if angry { "angry" } else { "normal" }));
        }
        self.ui.label(x, y + 84.0, "Sound file name:");
        if let Some(sound) = self.ui.input("sound", x, y + 102.0, 150.0, &text("sound")) {
            self.set("sound", json!(sound));
        }
        self.ui.label(x, y + 126.0, "Text:");
        if let Some(said) = self.ui.input("text", x, y + 144.0, 230.0, &text("text")) {
            self.set("text", json!(said));
        }
        if self.ui.button(x + 160.0, y + 18.0, 70.0, "Save") {
            self.save();
        }
        if let Some(index) = picked {
            self.load(index);
        }
    }

    fn load(&mut self, index: usize) {
        let Some(path) = self.files.get(index).cloned() else { return };
        match std::fs::read_to_string(&path).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok()).filter(|json| json["dialogue"].is_array()) {
            Some(json) => {
                (self.path, self.json, self.line, self.unsaved, self.leaving) = (Some(path), json, 0, false, false);
                self.show();
            }
            None => self.say(format!("Not a dialogue file: {}", path.display())),
        }
    }

    /// Over its file, the one before kept as `.bak`; a new one into the folder's data/dialogue.json.
    fn save(&mut self) {
        let path = self.path.clone().unwrap_or_else(|| {
            let mods = self.base.join("mods");
            if self.folder.is_empty() { mods.join("data/dialogue.json") } else { mods.join(&self.folder).join("data/dialogue.json") }
        });
        let done = funkin_core::files::replace(&path, &crate::chart_editor::print(&self.json, 0));
        if done.is_ok() {
            (self.path, self.unsaved) = (Some(path), false);
        }
        self.say(done.unwrap_or_else(|error| error));
    }

    pub fn draw(&self, skin: &Skin) {
        clear_background(Color::from_hex(0x808080));
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, crate::HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / crate::HEIGHT), ..Default::default() });
        if let Some(speech) = &self.speech {
            speech.draw();
        }
        skin.text("Press O to remove the current dialogue line, Press P to add another line after the current one.", 10.0, 26.0, 16, WHITE);
        skin.text(&format!("Line: ({} / {}) - Press A or D to scroll", self.line + 1, self.lines().len()), 10.0, 56.0, 24, WHITE);
        let expressions = self.expressions();
        let now = self.lines().get(self.line).and_then(|line| line["expression"].as_str()).unwrap_or_default();
        let words = match expressions.iter().position(|known| known == now) {
            Some(index) => format!("Animation: {now} ({} / {}) - Press W or S to scroll", index + 1, expressions.len()),
            None if expressions.is_empty() => "ERROR! NO ANIMATIONS FOUND".to_owned(),
            None => format!("Animation: {now} (not one of its {}) - Press W or S to scroll", expressions.len()),
        };
        skin.text(&words, 10.0, 86.0, 24, WHITE);
        if let Some((words, when)) = &self.said {
            if when.elapsed().as_secs_f32() < 3.0 {
                skin.text(words, 10.0, 116.0, 20, WHITE);
            }
        }
        self.ui.draw(skin.font.as_ref());
    }
}
