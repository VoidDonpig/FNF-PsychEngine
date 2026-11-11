//! The screens before a song: title, main menu, story mode and freeplay, laid out as in the Haxe engine.
use crate::atlas::{draw_frame, Frame, Sheet};
use crate::audio::Song;
use crate::{Skin, HEIGHT, WIDTH};
use funkin_core::content::{song_path, Paths, SongEntry, Week};
use funkin_core::save::Save;
use macroquad::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Title,
    Main,
    Story,
    Freeplay,
}

/// What a menu wants next.
pub enum Go {
    Stay,
    Page(Page),
    Options,
    Quit,
    /// Songs to play in a row (indices into the song list), and the week they are played for in story mode.
    Play(Vec<usize>, Option<usize>),
}

const UP: [KeyCode; 2] = [KeyCode::Up, KeyCode::W];
const DOWN: [KeyCode; 2] = [KeyCode::Down, KeyCode::S];
const LEFT: [KeyCode; 2] = [KeyCode::Left, KeyCode::A];
const RIGHT: [KeyCode; 2] = [KeyCode::Right, KeyCode::D];
const ACCEPT: [KeyCode; 2] = [KeyCode::Enter, KeyCode::Space];
const BACK: [KeyCode; 2] = [KeyCode::Escape, KeyCode::Backspace];
/// Tempo of the menu music.
// shortcut: the stock title screen (tempo, positions, dance frames); read images/gfDanceTitle.json once a mod's own title screen matters.
const MENU_BPM: f64 = 102.0;

fn pressed(keys: &[KeyCode]) -> bool {
    keys.iter().any(|key| is_key_pressed(*key))
}

fn white(alpha: f32) -> Color {
    Color::new(1.0, 1.0, 1.0, alpha)
}

/// Text with its top (not its baseline) at `top`, as the Haxe engine places it. `right` is the x its end sits at instead.
fn label(skin: &Skin, text: &str, x: f32, top: f32, size: u16, color: Color, right: bool) -> f32 {
    let width = measure_text(text, skin.font.as_ref(), size, 1.0).width;
    let x = if right { x - width } else { x };
    skin.text(text, x, top + size as f32 * 0.8, size, color);
    x
}

/// Where the letters of a line of bold text go (each with its left and top), and the line's width.
/// `size` gives a letter's frame size and its nudge from alphabet.json; letters it does not know are left out.
fn place(text: &str, size: impl Fn(char) -> Option<(Vec2, Vec2)>) -> (Vec<(char, Vec2)>, f32) {
    let (mut placed, mut x, mut spaces) = (Vec::new(), 0.0, 0.0);
    for letter in text.chars().flat_map(char::to_lowercase) {
        if letter == ' ' || letter == '_' {
            spaces += 1.0;
            continue;
        }
        let Some((frame, nudge)) = size(letter) else { continue };
        x += 28.0 * spaces;
        spaces = 0.0;
        // letters stand on a line 70 px down
        placed.push((letter, vec2(x - nudge.x, 70.0 - frame.y - nudge.y)));
        x += frame.x + nudge.x;
    }
    (placed, x)
}

/// The bold, wiggling menu letters.
struct Alphabet {
    texture: Texture2D,
    /// A letter's frames and its nudge.
    letters: HashMap<char, (Vec<Frame>, Vec2)>,
}

impl Alphabet {
    fn load(paths: &Paths) -> Option<Alphabet> {
        let sheet = Sheet::load(&paths.find("images/alphabet.png")?).ok()?;
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(paths.find("images/alphabet.json")?).ok()?).ok()?;
        let letters = json["allowed"].as_str()?.chars().filter_map(|letter| {
            let entry = &json["characters"][letter.to_string()];
            let name = entry["animation"].as_str().map_or(letter.to_string(), str::to_owned);
            let frames = sheet.animation(&format!("{name} bold"), &[]);
            let nudge = vec2(entry["bold"][0].as_f64().unwrap_or(0.0) as f32, entry["bold"][1].as_f64().unwrap_or(0.0) as f32);
            (!frames.is_empty()).then_some((letter, (frames, nudge)))
        }).collect();
        Some(Alphabet { texture: sheet.texture, letters })
    }

    fn size(&self, letter: char) -> Option<(Vec2, Vec2)> {
        self.letters.get(&letter).map(|(frames, nudge)| (frames[0].size, *nudge))
    }
}

/// A character of the story menu, from its `images/menucharacters/<name>.json`.
struct MenuCharacter {
    sheet: Rc<Sheet>,
    idle: Vec<Frame>,
    confirm: Vec<Frame>,
    offset: Vec2,
    scale: f32,
    flip: bool,
}

/// Pictures of the menus, read from disk the first time they are shown.
struct Art {
    base: PathBuf,
    images: HashMap<String, Option<Texture2D>>,
    sheets: HashMap<String, Option<Rc<Sheet>>>,
    characters: HashMap<String, Option<Rc<MenuCharacter>>>,
}

impl Art {
    /// The picture "images/<name>.png" of a mod (`source`) or of the game.
    fn image(&mut self, source: &str, name: &str) -> Option<Texture2D> {
        let base = &self.base;
        self.images.entry(format!("{source}/{name}")).or_insert_with(|| {
            let path = Paths::new(base, source, "").find(&format!("images/{name}.png"))?;
            Some(Texture2D::from_file_with_format(&std::fs::read(path).ok()?, None))
        }).clone()
    }

    fn sheet(&mut self, source: &str, name: &str) -> Option<Rc<Sheet>> {
        let base = &self.base;
        self.sheets.entry(format!("{source}/{name}")).or_insert_with(|| {
            let path = Paths::new(base, source, "").find(&format!("images/{name}.png"))?;
            Sheet::load(&path).map_err(|e| eprintln!("{e}")).ok().map(Rc::new)
        }).clone()
    }

    fn character(&mut self, source: &str, name: &str) -> Option<Rc<MenuCharacter>> {
        let key = format!("{source}/{name}");
        if let Some(known) = self.characters.get(&key) {
            return known.clone();
        }
        let loaded = (|| {
            let path = Paths::new(&self.base, source, "").find(&format!("images/menucharacters/{name}.json"))?;
            let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
            let sheet = self.sheet(source, &format!("menucharacters/{}", json["image"].as_str()?))?;
            let frames = |key: &str| json[key].as_str().filter(|name| !name.is_empty()).map_or(Vec::new(), |name| sheet.animation(name, &[]));
            let number = |value: &serde_json::Value, or: f32| value.as_f64().map_or(or, |n| n as f32);
            // a cheer that is the idle animation again is no cheer
            let confirm = if json["confirm_anim"] == json["idle_anim"] { Vec::new() } else { frames("confirm_anim") };
            Some(Rc::new(MenuCharacter {
                idle: frames("idle_anim"), confirm, offset: vec2(number(&json["position"][0], 0.0), number(&json["position"][1], 0.0)),
                scale: number(&json["scale"], 1.0), flip: json["flipX"].as_bool().unwrap_or(false), sheet: sheet.clone(),
            }))
        })();
        self.characters.insert(key, loaded.clone());
        loaded
    }
}

/// A song of the freeplay list.
struct Row {
    title: String,
    source: String,
    icon: String,
    color: [u8; 3],
    /// Its entries in the song list, one for each difficulty it has a chart for.
    entries: Vec<usize>,
}

pub struct Menus {
    art: Art,
    paths: Paths,
    alphabet: Option<Alphabet>,
    /// Sound output of the menus; none while a song has the sound card.
    audio: Option<Song>,
    /// When the menu music started (program time, s). The title screen moves to it.
    music_at: f64,
    /// Whether the title screen still shows its opening lines, and the two random ones among them.
    intro: bool,
    wacky: [String; 2],
    /// A white flash over the screen: when it began and how long it takes to fade.
    flash: (f64, f64),
    /// When the choice on the current screen was confirmed; the screen changes a moment later.
    chosen: Option<f64>,
    /// Main menu: the row in the middle column, whether the item at the side is selected instead, and how far the background has slid.
    main_row: usize,
    main_side: bool,
    main_slide: f32,
    week: usize,
    week_scroll: f32,
    /// Name of the difficulty last chosen; kept from one song or week to the next where they have it.
    difficulty: String,
    rows: Vec<Row>,
    song: usize,
    song_scroll: f32,
    tint: Vec3,
    /// Score and accuracy as shown: they count up to the real ones.
    shown: (f64, f64),
}

/// The lines of the title screen's opening at a beat, how far down they are shifted, and whether the Newgrounds logo shows.
fn opening(beat: u32, wacky: &[String; 2]) -> (Vec<&str>, f32, bool) {
    match beat {
        2 | 3 => (vec!["Psych Engine by"], 40.0, false),
        4 => (vec!["Psych Engine by", "Shadow Mario", "Riveren"], 40.0, false),
        6 | 7 => (vec!["Not associated", "with"], -40.0, false),
        8 => (vec!["Not associated", "with", "newgrounds"], -40.0, true),
        10 | 11 => (vec![wacky[0].as_str()], 0.0, false),
        12 => (vec![wacky[0].as_str(), wacky[1].as_str()], 0.0, false),
        14..=16 => (["Friday", "Night", "Funkin"][..beat as usize - 13].to_vec(), 0.0, false),
        _ => (Vec::new(), 0.0, false),
    }
}

/// The songs of a week on a difficulty (indices into the song list), if every one of them has its chart.
pub fn playlist(index: usize, week: &Week, songs: &[SongEntry], difficulty: &str) -> Option<Vec<usize>> {
    let entry = |title: &String| songs.iter().position(|song| song.week == index && song.title == *title && song.difficulty == difficulty);
    week.songs.iter().map(|(title, ..)| entry(title)).collect::<Option<Vec<usize>>>().filter(|list| !list.is_empty())
}

/// Which of `list` is chosen: the one named `last`, else "Normal", else the first.
fn difficulty_index<'a>(list: impl Iterator<Item = &'a str> + Clone, last: &str) -> usize {
    list.clone().position(|name| name == last).or_else(|| list.clone().position(|name| name == "Normal")).unwrap_or(0)
}

/// Frame of an animation that plays once at 24 frames a second, `seconds` after it began.
fn once(frames: &[Frame], seconds: f64) -> Option<Frame> {
    frames.get(((seconds.max(0.0) * 24.0) as usize).min(frames.len().saturating_sub(1))).copied()
}

/// Frame of an animation that loops at 24 frames a second.
fn looped(frames: &[Frame]) -> Option<Frame> {
    frames.get((get_time() * 24.0) as usize % frames.len().max(1)).copied()
}

impl Menus {
    pub fn new(base: &Path) -> Menus {
        let paths = Paths::new(base, "", "");
        rand::srand(macroquad::miniquad::date::now() as u64);
        let lines = paths.find("data/introText.txt").and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default();
        let lines: Vec<&str> = lines.lines().filter(|line| line.contains("--")).collect();
        let wacky = lines.get(rand::gen_range(0, lines.len().max(1))).and_then(|line| line.split_once("--")).unwrap_or(("", ""));
        Menus {
            art: Art { base: base.to_path_buf(), images: HashMap::new(), sheets: HashMap::new(), characters: HashMap::new() },
            alphabet: Alphabet::load(&paths), paths, audio: None, music_at: 0.0, intro: true, wacky: [wacky.0.to_owned(), wacky.1.to_owned()], flash: (f64::MIN, 1.0), chosen: None,
            main_row: 0, main_side: false, main_slide: 0.0, week: 0, week_scroll: 0.0, difficulty: "Normal".to_owned(), rows: Vec::new(), song: 0, song_scroll: 0.0, tint: Vec3::ONE, shown: (0.0, 0.0),
        }
    }

    /// Gives up the sound card for a song. The menu music starts again when a menu is next shown.
    pub fn quiet(&mut self) {
        self.audio = None;
        self.chosen = None;
        self.rows.clear(); // a finished week may have unlocked songs
    }

    fn sound(&self, name: &str, volume: f32) {
        if let (Some(audio), Some(path)) = (&self.audio, self.paths.find(&format!("sounds/{name}.ogg"))) {
            audio.play_sound(&path, volume, false);
        }
    }

    /// Width of a line of bold text.
    fn bold_width(&self, skin: &Skin, text: &str) -> f32 {
        match &self.alphabet {
            Some(alphabet) => place(text, |letter| alphabet.size(letter)).1,
            None => measure_text(text, skin.font.as_ref(), 48, 1.0).width,
        }
    }

    /// Bold text with its top left at (x, y), squeezed sideways by `squeeze`.
    fn bold(&self, skin: &Skin, text: &str, x: f32, y: f32, squeeze: f32, alpha: f32) {
        // without the letter sheet the plain font has to do
        let Some(alphabet) = &self.alphabet else { return skin.text(text, x, y + 55.0, 48, white(alpha)) };
        let step = (get_time() * 24.0) as usize;
        for (letter, at) in place(text, |letter| alphabet.size(letter)).0 {
            let frames = &alphabet.letters[&letter].0;
            let frame = frames[step % frames.len()];
            let params = DrawTextureParams { dest_size: Some(vec2(frame.source.w * squeeze, frame.source.h)), source: Some(frame.source), ..Default::default() };
            draw_texture_ex(&alphabet.texture, x + (at.x + frame.offset.x) * squeeze, y + at.y + frame.offset.y, white(alpha), params);
        }
    }

    /// Runs and draws one frame of a menu.
    pub fn run(&mut self, page: Page, skin: &Skin, weeks: &[Week], songs: &[SongEntry], save: &Save) -> Go {
        if self.audio.is_none() {
            let audio = Song::load(&[]);
            if let Some(path) = self.paths.find("music/freakyMenu.ogg") {
                audio.play_sound(&path, 0.7, true);
            }
            self.audio = Some(audio);
            self.music_at = get_time();
        }
        let go = match page {
            Page::Title => self.title(skin),
            Page::Main => self.main_menu(skin),
            Page::Story => self.story(skin, weeks, songs, save),
            Page::Freeplay => self.freeplay(skin, weeks, songs, save),
        };
        let flash = 1.0 - (get_time() - self.flash.0) / self.flash.1;
        if flash > 0.0 {
            draw_rectangle(0.0, 0.0, WIDTH, HEIGHT, white(flash as f32));
        }
        if !matches!(go, Go::Stay) {
            self.chosen = None;
        }
        go
    }

    fn title(&mut self, skin: &Skin) -> Go {
        let now = get_time();
        let beat = (now - self.music_at) * MENU_BPM / 60.0;
        let in_beat = beat.fract() * 60.0 / MENU_BPM;
        if self.intro && (beat >= 17.0 || pressed(&ACCEPT)) {
            self.intro = false;
            self.flash = (now, 2.0);
            return Go::Stay; // the same key press must not also leave the title
        }
        if self.intro {
            clear_background(BLACK);
            let (lines, shift, logo) = opening(beat as u32, &self.wacky);
            for (row, line) in lines.iter().enumerate() {
                self.bold(skin, line, (WIDTH - self.bold_width(skin, line)) / 2.0, row as f32 * 60.0 + 200.0 + shift, 1.0, 1.0);
            }
            if let Some(picture) = self.art.image("", "newgrounds_logo").filter(|_| logo) {
                let size = picture.size() * 0.8;
                draw_texture_ex(&picture, (WIDTH - size.x) / 2.0, HEIGHT * 0.52, WHITE, DrawTextureParams { dest_size: Some(size), ..Default::default() });
            }
            return if pressed(&BACK) { Go::Quit } else { Go::Stay };
        }

        clear_background(BLACK);
        if let Some(sheet) = self.art.sheet("", "gfDanceTitle") {
            // she sways to one side on a beat and to the other on the next
            let side: Vec<u32> = if beat as u32 % 2 == 0 { (15..30).collect() } else { [30].into_iter().chain(0..15).collect() };
            if let Some(frame) = once(&sheet.animation("gfDance", &side), in_beat) {
                sheet.draw(frame, 512.0, 40.0, 1.0, false, WHITE);
            }
        }
        if let Some(sheet) = self.art.sheet("", "logoBumpin") {
            if let Some(frame) = once(&sheet.animation("logo bumpin", &[]), in_beat) {
                sheet.draw(frame, -150.0, -100.0, 1.0, false, WHITE);
            }
        }
        if let Some(sheet) = self.art.sheet("", "titleEnter") {
            // waiting, the words breathe between two blues over two seconds
            let wave = (now % 2.0 - 1.0).abs() as f32;
            let eased = if wave < 0.5 { 2.0 * wave * wave } else { 1.0 - 2.0 * (1.0 - wave) * (1.0 - wave) };
            let (name, color) = match self.chosen {
                Some(_) => ("ENTER PRESSED", WHITE),
                None => ("ENTER IDLE", Color::new(0.2, 0.2 + 0.8 * eased, 0.8 + 0.2 * eased, 0.64 + 0.36 * eased)),
            };
            if let Some(frame) = looped(&sheet.animation(name, &[])) {
                sheet.draw(frame, 100.0, 576.0, 1.0, false, color);
            }
        }
        match self.chosen {
            Some(at) if now - at >= 1.0 => Go::Page(Page::Main),
            Some(_) => Go::Stay,
            None if pressed(&ACCEPT) => {
                self.sound("confirmMenu", 0.7);
                (self.chosen, self.flash) = (Some(now), (now, 1.0));
                Go::Stay
            }
            None if pressed(&BACK) => Go::Quit,
            None => Go::Stay,
        }
    }

    fn main_menu(&mut self, skin: &Skin) -> Go {
        // shortcut: only what exists so far; mods, credits and achievements join the list with their screens.
        const MIDDLE: [&str; 2] = ["story_mode", "freeplay"];
        const SIDE: &str = "options";
        let (now, dt) = (get_time(), get_frame_time().min(0.1));
        let since = self.chosen.map(|at| (now - at) as f32);
        if since.is_none() {
            let step = pressed(&DOWN) as usize + pressed(&UP) as usize * (MIDDLE.len() - 1);
            if step != 0 && !self.main_side {
                self.main_row = (self.main_row + step) % MIDDLE.len();
                self.sound("scrollMenu", 1.0);
            }
            if (pressed(&RIGHT) && !self.main_side) || (pressed(&LEFT) && self.main_side) {
                self.main_side = !self.main_side;
                self.sound("scrollMenu", 1.0);
            }
        }

        // every item: name, top left corner, whether it is the selected one
        let mut items: Vec<(&str, Vec2, bool)> = MIDDLE.iter().enumerate().map(|(row, name)| (*name, vec2(0.0, row as f32 * 140.0 + 90.0 + (4 - MIDDLE.len()) as f32 * 70.0), !self.main_side && row == self.main_row)).collect();
        items.push((SIDE, vec2(WIDTH - 60.0, 490.0), self.main_side));
        let mut drawn = Vec::new();
        for (index, (name, mut at, selected)) in items.into_iter().enumerate() {
            let Some(sheet) = self.art.sheet("", &format!("mainmenu/menu_{name}")) else { continue };
            let idle = sheet.animation(&format!("{name} idle"), &[]);
            let Some(resting) = idle.first().map(|frame| frame.size) else { continue };
            at.x = if index < MIDDLE.len() { (WIDTH - resting.x) / 2.0 } else { at.x - resting.x };
            let frames = if selected { sheet.animation(&format!("{name} selected"), &[]) } else { idle };
            if let Some(frame) = looped(&frames) {
                // the bigger selected picture stays centred on the resting one
                drawn.push((sheet.clone(), frame, at + (resting - frame.size) / 2.0, selected));
                if selected {
                    // the background slides a little towards the selected item
                    let target = (at.y + resting.y / 2.0 - HEIGHT / 2.0) * 0.25;
                    self.main_slide += (target - self.main_slide) * (1.0 - 0.85f32.powf(dt * 60.0));
                }
            }
        }

        // once chosen, the background blinks pink and the chosen item flickers while the others fade away
        let pink = since.is_some_and(|t| t < 1.1 && (t / 0.15) as u32 % 2 == 0);
        if let Some(picture) = self.art.image("", if pink { "menuDesat" } else { "menuBG" }) {
            let size = picture.size() * 1.175;
            let color = if pink { Color::from_hex(0xFD719B) } else { WHITE };
            draw_texture_ex(&picture, (WIDTH - size.x) / 2.0, (HEIGHT - size.y) / 2.0 - self.main_slide, color, DrawTextureParams { dest_size: Some(size), ..Default::default() });
        }
        for (sheet, frame, at, selected) in drawn {
            let alpha = match since {
                Some(t) if selected => ((t / 0.06) as u32 % 2) as f32,
                Some(t) => (1.0 - t / 0.4).clamp(0.0, 1.0).powi(2),
                None => 1.0,
            };
            sheet.draw(frame, at.x, at.y, 1.0, false, white(alpha));
        }
        label(skin, "Psych Engine (Rust)", 12.0, HEIGHT - 44.0, 16, WHITE, false);
        label(skin, "Friday Night Funkin' v0.2.8", 12.0, HEIGHT - 24.0, 16, WHITE, false);

        match since {
            Some(t) if t >= 1.0 => match (self.main_side, self.main_row) {
                (true, _) => Go::Options,
                (false, 0) => Go::Page(Page::Story),
                (false, _) => Go::Page(Page::Freeplay),
            },
            Some(_) => Go::Stay,
            None if pressed(&ACCEPT) => {
                self.sound("confirmMenu", 1.0);
                self.chosen = Some(now);
                Go::Stay
            }
            None if pressed(&BACK) => {
                self.sound("cancelMenu", 1.0);
                Go::Page(Page::Title)
            }
            None => Go::Stay,
        }
    }

    fn story(&mut self, skin: &Skin, weeks: &[Week], songs: &[SongEntry], save: &Save) -> Go {
        let (now, dt) = (get_time(), get_frame_time().min(0.1));
        clear_background(BLACK);
        let shown: Vec<usize> = weeks.iter().enumerate().filter(|(_, week)| !week.hide_story && !(week.hidden_until_unlocked && save.locked(week))).map(|(index, _)| index).collect();
        if shown.is_empty() {
            label(skin, "NO WEEKS ADDED FOR STORY MODE", 40.0, 120.0, 32, WHITE, false);
            return if pressed(&BACK) || pressed(&ACCEPT) { Go::Page(Page::Main) } else { Go::Stay };
        }
        let since = self.chosen.map(|at| (now - at) as f32);
        if since.is_none() {
            let step = pressed(&DOWN) as usize + pressed(&UP) as usize * (shown.len() - 1);
            if step != 0 {
                self.week += step;
                self.sound("scrollMenu", 1.0);
            }
        }
        self.week %= shown.len();
        let week = &weeks[shown[self.week]];
        let locked = save.locked(week);
        let names = || week.difficulties.iter().map(String::as_str);
        let mut difficulty = difficulty_index(names(), &self.difficulty);
        if since.is_none() && !week.difficulties.is_empty() {
            let step = pressed(&RIGHT) as usize + pressed(&LEFT) as usize * (week.difficulties.len() - 1);
            difficulty = (difficulty + step) % week.difficulties.len();
            self.difficulty = week.difficulties[difficulty].clone();
        }
        let difficulty = week.difficulties.get(difficulty).map_or("", String::as_str);
        let playlist = || playlist(shown[self.week], week, songs, difficulty);

        // the week titles roll under the yellow band, the selected one just below it
        let pictures: Vec<Option<Texture2D>> = shown.iter().map(|index| self.art.image(&weeks[*index].source, &format!("storymenu/{}", weeks[*index].file))).collect();
        let heights: Vec<f32> = pictures.iter().map(|picture| picture.as_ref().map_or(89.0, Texture2D::height)).collect();
        let tops: Vec<f32> = heights.iter().scan(0.0, |top, height| Some(std::mem::replace(top, *top + height.max(110.0) + 10.0))).collect();
        self.week_scroll += (tops[self.week] - self.week_scroll) * (1.0 - (-dt * 10.2).exp());
        let ui = self.art.sheet("", "campaign_menu_UI_assets");
        let mut locks = Vec::new();
        for (row, picture) in pictures.iter().enumerate() {
            let y = tops[row] - self.week_scroll + 480.0;
            let name = &weeks[shown[row]].file;
            let width = picture.as_ref().map_or_else(|| measure_text(name, skin.font.as_ref(), 48, 1.0).width, Texture2D::width);
            let x = (WIDTH - width) / 2.0;
            let color = match since {
                Some(t) if row == self.week && (t * 60.0) as u32 % 2 == 0 => Color::from_hex(0x33FFFF),
                _ => white(if row == self.week && !locked { 1.0 } else { 0.6 }),
            };
            match picture {
                Some(picture) => draw_texture(picture, x, y, color),
                None => drop(label(skin, name, x, y + 20.0, 48, color, false)),
            }
            if save.locked(&weeks[shown[row]]) {
                locks.push(vec2(x + width + 10.0, y + heights[row] / 2.0));
            }
        }
        draw_rectangle(0.0, 0.0, WIDTH, 56.0, BLACK);
        if let Some(ui) = &ui {
            let frame = |name: &str| ui.animation(name, &[]).first().copied();
            for at in locks {
                if let Some(lock) = frame("lock") {
                    ui.draw(lock, at.x, at.y - lock.size.y / 2.0, 1.0, false, WHITE);
                }
            }
            if !locked {
                let held = |keys: &[KeyCode]| keys.iter().any(|key| is_key_down(*key));
                for (x, resting, pushed, down) in [(850.0, "arrow left", "arrow push left", held(&LEFT)), (850.0 + 376.0, "arrow right", "arrow push right", held(&RIGHT))] {
                    if let Some(arrow) = frame(if down { pushed } else { resting }) {
                        ui.draw(arrow, x, 462.0, 1.0, false, WHITE);
                    }
                }
            }
        }
        if !locked {
            match self.art.image(&week.source, &format!("menudifficulties/{}", song_path(difficulty))) {
                Some(picture) => draw_texture(&picture, 910.0 + (308.0 - picture.width()) / 3.0, 542.0 - picture.height(), WHITE),
                // a difficulty without its picture is written out
                None => drop(label(skin, &difficulty.to_uppercase(), 930.0, 490.0, 48, WHITE, false)),
            }
        }

        draw_rectangle(0.0, 56.0, WIDTH, 386.0, Color::from_hex(0xF9CF51));
        if let Some(picture) = self.art.image(&week.source, &format!("menubackgrounds/menu_{}", week.background)).filter(|_| !week.background.is_empty()) {
            draw_texture(&picture, 0.0, 56.0, WHITE);
        }
        for (slot, name) in week.characters.iter().enumerate() {
            let Some(character) = self.art.character(&week.source, name).filter(|_| !name.is_empty()) else { continue };
            let frame = match since {
                Some(t) if !character.confirm.is_empty() => once(&character.confirm, t as f64),
                _ => looped(&character.idle),
            };
            if let Some(frame) = frame {
                let at = vec2(WIDTH * 0.25 * (1 + slot) as f32 - 150.0, 70.0) - character.offset;
                draw_frame(&character.sheet.texture, frame, at.x, at.y, character.scale, character.flip, WHITE);
            }
        }
        if let Some(picture) = self.art.image("", "Menu_Tracks") {
            draw_texture(&picture, WIDTH * 0.07 + 100.0 - picture.width() / 2.0, 481.0, WHITE);
        }
        for (row, (title, ..)) in week.songs.iter().enumerate() {
            let title = title.to_uppercase();
            let width = measure_text(&title, skin.font.as_ref(), 32, 1.0).width;
            label(skin, &title, WIDTH * 0.15 - width / 2.0, 541.0 + row as f32 * 32.0, 32, Color::from_hex(0xE55777), false);
        }
        let best = save.week(&week.file, difficulty) as f64;
        self.shown.0 = if (best - self.shown.0).abs() < 10.0 { best } else { best + (self.shown.0 - best) * (-dt as f64 * 30.0).exp() };
        label(skin, &format!("WEEK SCORE: {}", self.shown.0 as i64), 10.0, 10.0, 32, WHITE, false);
        label(skin, &week.story_name.to_uppercase(), WIDTH - 10.0, 10.0, 32, white(0.7), true);

        match since {
            Some(t) if t >= 1.0 => playlist().map_or(Go::Stay, |list| Go::Play(list, Some(shown[self.week]))),
            Some(_) => Go::Stay,
            None if pressed(&ACCEPT) => {
                let ready = !locked && playlist().is_some();
                self.sound(if ready { "confirmMenu" } else { "cancelMenu" }, 1.0);
                self.chosen = ready.then_some(now);
                Go::Stay
            }
            None if pressed(&BACK) => {
                self.sound("cancelMenu", 1.0);
                Go::Page(Page::Main)
            }
            None => Go::Stay,
        }
    }

    fn freeplay(&mut self, skin: &Skin, weeks: &[Week], songs: &[SongEntry], save: &Save) -> Go {
        let dt = get_frame_time().min(0.1);
        if self.rows.is_empty() {
            for (index, week) in weeks.iter().enumerate().filter(|(_, week)| !week.hide_freeplay && !save.locked(week)) {
                for (title, icon, color) in &week.songs {
                    let entries: Vec<usize> = songs.iter().enumerate().filter(|(_, song)| song.week == index && song.title == *title).map(|(entry, _)| entry).collect();
                    if !entries.is_empty() {
                        self.rows.push(Row { title: title.clone(), source: week.source.clone(), icon: icon.clone(), color: *color, entries });
                    }
                }
            }
            self.song = self.song.min(self.rows.len().saturating_sub(1));
            self.song_scroll = self.song as f32;
        }
        if self.rows.is_empty() {
            label(skin, "NO WEEKS ADDED FOR FREEPLAY", 40.0, 120.0, 32, WHITE, false);
            return if pressed(&BACK) || pressed(&ACCEPT) { Go::Page(Page::Main) } else { Go::Stay };
        }
        let step = pressed(&DOWN) as usize + pressed(&UP) as usize * (self.rows.len() - 1);
        if step != 0 {
            self.song = (self.song + step) % self.rows.len();
            self.sound("scrollMenu", 0.4);
        }
        let row = &self.rows[self.song];
        let names = || row.entries.iter().map(|entry| songs[*entry].difficulty.as_str());
        let step = pressed(&RIGHT) as usize + pressed(&LEFT) as usize * (row.entries.len() - 1);
        let difficulty = (difficulty_index(names(), &self.difficulty) + step) % row.entries.len();
        let entry = &songs[row.entries[difficulty]];
        if step != 0 {
            self.difficulty = entry.difficulty.clone();
        }

        // the background takes the colour of the selected song
        let target = Vec3::from(row.color.map(|c| c as f32 / 255.0));
        self.tint += (target - self.tint) * (1.0 - (-dt * 5.0).exp());
        if let Some(picture) = self.art.image("", "menuDesat") {
            draw_texture(&picture, (WIDTH - picture.width()) / 2.0, (HEIGHT - picture.height()) / 2.0, Color::new(self.tint.x, self.tint.y, self.tint.z, 1.0));
        }
        self.song_scroll += (self.song as f32 - self.song_scroll) * (1.0 - (-dt * 9.6).exp());
        let near = |row: f32| (row.round().max(0.0) as usize).min(self.rows.len());
        for index in near(self.song_scroll - 4.0)..near(self.song_scroll + 4.0) {
            let (row, away) = (&self.rows[index], index as f32 - self.song_scroll);
            let (x, y, alpha) = (away * 20.0 + 90.0, away * 156.0 + 320.0, if index == self.song { 1.0 } else { 0.6 });
            let width = self.bold_width(skin, &row.title);
            let squeeze = (980.0 / width).min(1.0);
            self.bold(skin, &row.title, x, y, squeeze, alpha);
            let icon = [format!("icons/{}", row.icon), format!("icons/icon-{}", row.icon), "icons/icon-face".to_owned()].iter().find_map(|name| self.art.image(&row.source, name));
            if let Some(icon) = icon {
                // an icon file holds the normal and the losing face side by side
                let faces = (icon.width() / icon.height()).round().max(1.0);
                let source = Rect::new(0.0, 0.0, icon.width() / faces, icon.height());
                draw_texture_ex(&icon, x + width * squeeze + 12.0, y - 30.0, white(alpha), DrawTextureParams { source: Some(source), ..Default::default() });
            }
        }

        let best = save.song(&entry.title, &entry.difficulty);
        self.shown.0 = if (best.0 as f64 - self.shown.0).abs() <= 10.0 { best.0 as f64 } else { best.0 as f64 + (self.shown.0 - best.0 as f64) * (-dt as f64 * 24.0).exp() };
        self.shown.1 = if (best.1 - self.shown.1).abs() <= 0.0001 { best.1 } else { best.1 + (self.shown.1 - best.1) * (-dt as f64 * 12.0).exp() };
        let score = format!("PERSONAL BEST: {} ({:.2}%)", self.shown.0 as i64, (self.shown.1 * 10000.0).floor() / 100.0);
        let left = WIDTH - 6.0 - measure_text(&score, skin.font.as_ref(), 32, 1.0).width;
        draw_rectangle(left - 6.0, 0.0, WIDTH - left + 6.0, 66.0, Color::new(0.0, 0.0, 0.0, 0.6));
        label(skin, &score, left, 5.0, 32, WHITE, false);
        let name = entry.difficulty.to_uppercase();
        let name = if row.entries.len() > 1 { format!("< {name} >") } else { name };
        label(skin, &name, (left - 6.0 + WIDTH) / 2.0 - measure_text(&name, skin.font.as_ref(), 24, 1.0).width / 2.0, 41.0, 24, WHITE, false);
        draw_rectangle(0.0, HEIGHT - 26.0, WIDTH, 26.0, Color::new(0.0, 0.0, 0.0, 0.6));
        let tip = "Up/Down: song / Left/Right: difficulty / Enter: play / Esc: back";
        label(skin, tip, (WIDTH - measure_text(tip, skin.font.as_ref(), 16, 1.0).width) / 2.0, HEIGHT - 22.0, 16, WHITE, false);

        if pressed(&ACCEPT) {
            Go::Play(vec![row.entries[difficulty]], None)
        } else if pressed(&BACK) {
            self.sound("cancelMenu", 1.0);
            Go::Page(Page::Main)
        } else {
            Go::Stay
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_letters_stand_on_one_line_with_gaps_for_spaces() {
        let size = |letter: char| match letter {
            'a' => Some((vec2(50.0, 68.0), Vec2::ZERO)),
            '\'' => Some((vec2(20.0, 25.0), vec2(0.0, 40.0))),
            _ => None,
        };
        let (placed, width) = place("A a'?", size);
        // capitals use the same letters; the unknown "?" is left out
        assert_eq!(placed, [('a', vec2(0.0, 2.0)), ('a', vec2(78.0, 2.0)), ('\'', vec2(128.0, 5.0))]);
        assert_eq!(width, 148.0);
    }

    #[test]
    fn difficulty_stays_where_the_next_song_has_it() {
        let list = ["Easy", "Normal", "Hard"];
        assert_eq!(difficulty_index(list.iter().copied(), "Hard"), 2);
        assert_eq!(difficulty_index(list.iter().copied(), "Practice"), 1, "falls back to Normal");
        assert_eq!(difficulty_index(["Practice"].iter().copied(), "Hard"), 0);
    }

    #[test]
    fn the_opening_adds_its_lines_beat_by_beat() {
        let wacky = ["a".to_owned(), "b".to_owned()];
        assert_eq!(opening(4, &wacky).0.len(), 3);
        assert_eq!(opening(12, &wacky).0, ["a", "b"]);
        assert_eq!(opening(15, &wacky).0, ["Friday", "Night"]);
        assert!(opening(8, &wacky).2 && opening(9, &wacky).0.is_empty());
    }
}
