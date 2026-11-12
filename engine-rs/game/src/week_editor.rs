//! The week editor (states/editors/WeekEditorState.hx and its freeplay page): a week's file edited in the Haxe
//! editor's box while the story menu shows it as it is; saved over its file, the one before kept as `.bak`.
// shortcut: the freeplay page's own view (the song list in its colours) is not drawn; the story menu's view stays.
use crate::gfx::*;
use crate::ui::Ui;
use crate::{Skin, HEIGHT, WIDTH};
use funkin_core::content::{parse_week, Week};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// What the editor wants next.
pub enum Go {
    Stay,
    Leave,
}

pub struct WeekEditor {
    base: PathBuf,
    folder: String,
    /// The folder's weeks there are (to load one), and the one being edited: its file, its name, what it holds.
    weeks: Vec<Week>,
    path: Option<PathBuf>,
    file: String,
    json: Value,
    ui: Ui,
    tab: usize,
    /// The song chosen in the Freeplay tab.
    song: usize,
    unsaved: bool,
    leaving: bool,
    /// A week was saved: the game's lists of weeks and songs are to be read again.
    pub saved: bool,
    said: Option<(String, Instant)>,
}

/// A new week as the Haxe editor starts one (WeekData.createWeekFile).
fn new_week() -> Value {
    json!({
        "songs": [["Bopeebo", "face", [146, 113, 253]], ["Fresh", "face", [146, 113, 253]], ["Dad Battle", "face", [146, 113, 253]]],
        "weekCharacters": ["dad", "bf", "gf"], "weekBackground": "stage", "weekBefore": "tutorial", "storyName": "Your New Week",
        "weekName": "Custom Week", "startUnlocked": true, "hiddenUntilUnlocked": false, "hideStoryMode": false, "hideFreeplay": false, "difficulties": ""
    })
}

impl WeekEditor {
    /// The first week of a mod's folder ("": the game's), or a new one.
    pub fn open(base: &Path, folder: &str) -> WeekEditor {
        let weeks: Vec<Week> = funkin_core::content::weeks(base).into_iter().filter(|week| week.source == folder).collect();
        let mut editor = WeekEditor {
            base: base.to_path_buf(), folder: folder.to_owned(), weeks, path: None, file: "week1".to_owned(), json: new_week(), ui: Ui::default(), tab: 2, song: 0,
            unsaved: false, leaving: false, saved: false, said: None,
        };
        if !editor.weeks.is_empty() {
            editor.load(0);
        }
        editor
    }

    fn load(&mut self, index: usize) {
        let Some(week) = self.weeks.get(index) else { return };
        match std::fs::read_to_string(&week.path).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok()) {
            Some(json) => (self.path, self.file, self.json, self.song, self.unsaved, self.leaving) = (Some(week.path.clone()), week.file.clone(), json, 0, false, false),
            None => self.say(format!("Could not read {}", week.path.display())),
        }
    }

    /// The week as the game would read it, for the story menu's view of it.
    pub fn week(&self) -> Week {
        parse_week(&self.file, &self.folder, &self.json)
    }

    fn say(&mut self, words: impl Into<String>) {
        self.said = Some((words.into(), Instant::now()));
    }

    fn set(&mut self, field: &str, value: Value) {
        self.json[field] = value;
        (self.unsaved, self.leaving) = (true, false);
    }

    pub fn update(&mut self) -> Go {
        let typing = self.ui.typing();
        self.ui.begin();
        self.panel();
        if !typing && is_key_pressed(KeyCode::Escape) {
            if !self.unsaved || self.leaving {
                return Go::Leave;
            }
            self.leaving = true;
            self.say("Unsaved changes: Escape again to leave");
        }
        Go::Stay
    }

    fn panel(&mut self) {
        // (the week list first: open, it drops down over the buttons under it)
        let names: Vec<String> = self.weeks.iter().map(|week| week.file.clone()).collect();
        let x = WIDTH - 250.0;
        self.ui.label(x, 262.0, "Load Week:");
        let chosen = self.weeks.iter().position(|week| Some(&week.path) == self.path.as_ref()).unwrap_or(0);
        if let Some(index) = self.ui.dropdown("weeks", x, 280.0, 230.0, &names, chosen) {
            self.load(index);
        }
        if self.ui.button(x, 308.0, 110.0, "Save Week") {
            self.save();
        }
        if self.ui.button(x + 120.0, 308.0, 110.0, "New Week") {
            (self.path, self.file, self.json, self.song, self.unsaved) = (None, "week1".to_owned(), new_week(), 0, true);
        }
        let area = Rect::new(WIDTH - 250.0, HEIGHT - 355.0, 250.0, 355.0);
        self.tab = self.ui.tabs(area, &["Freeplay", "Other", "Week"], self.tab);
        let (x, y) = (area.x + 10.0, area.y + 10.0);
        match self.tab {
            0 => self.freeplay_tab(x, y),
            1 => self.other_tab(x, y),
            _ => self.week_tab(x, y),
        }
    }

    fn week_tab(&mut self, x: f32, y: f32) {
        let text = |json: &Value, field: &str| json[field].as_str().unwrap_or_default().to_owned();
        self.ui.label(x, y, "Songs:");
        let songs = self.json["songs"].as_array().map_or(String::new(), |songs| songs.iter().filter_map(|song| song[0].as_str()).collect::<Vec<_>>().join(", "));
        if let Some(list) = self.ui.input("songs", x, y + 18.0, 230.0, &songs) {
            // as the Haxe editor: the songs by their place, new ones with the face icon and freeplay's purple
            let names: Vec<String> = list.split(',').map(|name| name.trim().to_owned()).filter(|name| !name.is_empty()).collect();
            let mut kept = self.json["songs"].as_array().cloned().unwrap_or_default();
            kept.truncate(names.len());
            for (index, name) in names.into_iter().enumerate() {
                match kept.get_mut(index) {
                    Some(song) => song[0] = json!(name),
                    None => kept.push(json!([name, "face", [146, 113, 253]])),
                }
            }
            self.set("songs", json!(kept));
        }
        self.ui.label(x, y + 42.0, "Characters:");
        for slot in 0..3 {
            let now = self.json["weekCharacters"][slot].as_str().unwrap_or_default().to_owned();
            if let Some(name) = self.ui.input(["opponent", "boyfriend", "girlfriend"][slot], x + 77.0 * slot as f32, y + 60.0, 72.0, &now) {
                let mut characters: Vec<Value> = (0..3).map(|other| self.json["weekCharacters"][other].clone()).map(|value| if value.is_null() { json!("") } else { value }).collect();
                characters[slot] = json!(name);
                self.set("weekCharacters", json!(characters));
            }
        }
        for (index, (field, words, width)) in [("weekBackground", "Background Asset:", 120.0), ("storyName", "Display Name:", 200.0), ("weekName", "Week Name (for Reset Score Menu):", 150.0)].into_iter().enumerate() {
            let top = y + 84.0 + 42.0 * index as f32;
            self.ui.label(x, top, words);
            if let Some(value) = self.ui.input(field, x, top + 18.0, width, &text(&self.json, field)) {
                self.set(field, json!(value));
            }
        }
        self.ui.label(x, y + 210.0, "Week File:");
        if let Some(file) = self.ui.input("week file", x, y + 228.0, 100.0, &self.file).map(|file| file.trim().to_owned()).filter(|file| !file.is_empty()) {
            self.file = file;
            (self.unsaved, self.leaving) = (true, false);
        }
        if let Some(hidden) = self.ui.check(x, y + 262.0, "Hide Week from Story Mode?", self.json["hideStoryMode"].as_bool().unwrap_or(false)) {
            self.set("hideStoryMode", json!(hidden));
        }
    }

    fn other_tab(&mut self, x: f32, y: f32) {
        let locked = !self.json["startUnlocked"].as_bool().unwrap_or(true);
        if let Some(locked) = self.ui.check(x, y + 10.0, "Week starts Locked", locked) {
            self.set("startUnlocked", json!(!locked));
        }
        if let Some(hidden) = self.ui.check(x, y + 35.0, "Hidden until Unlocked", self.json["hiddenUntilUnlocked"].as_bool().unwrap_or(false)) {
            self.set("hiddenUntilUnlocked", json!(hidden));
        }
        self.ui.label(x, y + 65.0, "Week File name of the Week you\nhave to finish for Unlocking:");
        if let Some(before) = self.ui.input("week before", x, y + 100.0, 100.0, self.json["weekBefore"].as_str().unwrap_or_default()) {
            self.set("weekBefore", json!(before));
        }
        self.ui.label(x, y + 130.0, "Difficulties:");
        if let Some(list) = self.ui.input("difficulties", x, y + 148.0, 200.0, self.json["difficulties"].as_str().unwrap_or_default()) {
            self.set("difficulties", json!(list));
        }
        self.ui.label(x, y + 172.0, "Default difficulties are\n\"Easy, Normal, Hard\"\nwithout quotes.");
    }

    fn freeplay_tab(&mut self, x: f32, y: f32) {
        let names: Vec<String> = self.json["songs"].as_array().into_iter().flatten().map(|song| song[0].as_str().unwrap_or_default().to_owned()).collect();
        self.song = self.song.min(names.len().saturating_sub(1));
        let song = self.json["songs"][self.song].clone();
        // (the song list first: open, it drops down over the rest)
        self.ui.label(x, y, "Song:");
        if let Some(index) = self.ui.dropdown("song", x, y + 18.0, 200.0, &names, self.song) {
            self.song = index;
        }
        if names.is_empty() {
            return;
        }
        self.ui.label(x, y + 46.0, "Selected background Color R/G/B:");
        for channel in 0..3 {
            let value = song[2][channel].as_f64().unwrap_or(255.0);
            if let Some(value) = self.ui.stepper(["song red", "song green", "song blue"][channel], x + 77.0 * channel as f32, y + 64.0, 30.0, value, 20.0, 0.0, 255.0) {
                let mut color: Vec<Value> = (0..3).map(|other| json!(song[2][other].as_f64().unwrap_or(255.0) as i64)).collect();
                color[channel] = json!(value.round() as i64);
                *crate::chart_editor::slot(&mut self.json["songs"][self.song], 2, &json!("face")) = json!(color);
                (self.unsaved, self.leaving) = (true, false);
            }
        }
        self.ui.label(x, y + 92.0, "Selected icon:");
        if let Some(icon) = self.ui.input("song icon", x, y + 110.0, 100.0, song[1].as_str().unwrap_or_default()) {
            *crate::chart_editor::slot(&mut self.json["songs"][self.song], 1, &json!("face")) = json!(icon);
            (self.unsaved, self.leaving) = (true, false);
        }
        self.ui.label(x + 120.0, y + 92.0, "Color:");
        if let Some(hidden) = self.ui.check(x, y + 144.0, "Hide Week from Freeplay?", self.json["hideFreeplay"].as_bool().unwrap_or(false)) {
            self.set("hideFreeplay", json!(hidden));
        }
    }

    /// Over the week's file (renamed as the Week File field says), the one before kept as `.bak`; a new week goes into
    /// the folder's weeks/.
    fn save(&mut self) {
        let folder = match &self.path {
            Some(path) => path.parent().map(Path::to_path_buf).unwrap_or_default(),
            None if self.folder.is_empty() => self.base.join("mods").join("weeks"),
            None => self.base.join("mods").join(&self.folder).join("weeks"),
        };
        let path = folder.join(format!("{}.json", self.file));
        if path.is_file() && Some(&path) != self.path.as_ref() {
            return self.say(format!("{}.json is another week: choose another Week File name", self.file));
        }
        let done = funkin_core::files::replace(&path, &crate::chart_editor::print(&self.json, 0));
        if done.is_ok() {
            (self.path, self.unsaved, self.saved) = (Some(path), false, true);
            self.weeks = funkin_core::content::weeks(&self.base).into_iter().filter(|week| week.source == self.folder).collect();
        }
        self.say(done.unwrap_or_else(|error| error));
    }

    /// The editor over the story menu's view of the week (drawn before it): the week's picture where the list would
    /// show it, the black bar, the box.
    pub fn draw(&self, skin: &Skin, picture: Option<&Texture2D>) {
        draw_rectangle(0.0, 0.0, WIDTH, 56.0, BLACK);
        match picture {
            Some(picture) => draw_texture(picture, (WIDTH - picture.width()) / 2.0, 472.0 + picture.height(), WHITE),
            None => {
                let words = format!("MISSING FILE: images/storymenu/{}.png", self.file);
                skin.text(&words, (WIDTH - measure_text(&words, skin.font.as_ref(), 24, 1.0).width) / 2.0, 520.0, 24, WHITE);
            }
        }
        // the freeplay colour of the song chosen in the Freeplay tab
        if self.tab == 0 {
            let song = &self.json["songs"][self.song];
            let color = Color::from_rgba(song[2][0].as_u64().unwrap_or(255) as u8, song[2][1].as_u64().unwrap_or(255) as u8, song[2][2].as_u64().unwrap_or(255) as u8, 255);
            draw_rectangle(WIDTH - 110.0, HEIGHT - 355.0 + 116.0, 60.0, 26.0, color);
        }
        if let Some((words, when)) = &self.said {
            if when.elapsed().as_secs_f32() < 3.0 {
                skin.text(words, 10.0, 45.0, 20, WHITE);
            }
        }
        self.ui.draw(skin.font.as_ref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_week_is_saved_into_the_folder_and_read_back_as_the_game_reads_weeks() {
        let base = std::env::temp_dir().join(format!("funkin-week-editor-{}", std::process::id()));
        std::fs::create_dir_all(base.join("mods/Mine")).unwrap();
        let mut editor = WeekEditor::open(&base, "Mine");
        assert!(editor.path.is_none(), "the folder has no weeks yet");
        editor.file = "mine".to_owned();
        editor.set("storyName", json!("Mine Week"));
        editor.save();
        let path = base.join("mods/Mine/weeks/mine.json");
        assert!(path.is_file() && !editor.unsaved && editor.saved);
        let read = funkin_core::content::weeks(&base).into_iter().find(|week| week.file == "mine").expect("found as a week");
        assert_eq!((read.story_name.as_str(), read.songs.len(), read.source.as_str()), ("Mine Week", 3, "Mine"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
