//! The screens before a song: title, main menu, story mode and freeplay, laid out as in the Haxe engine.
use crate::atlas::{draw_frame, Frame, Sheet};
use crate::audio::Song;
use crate::{Skin, HEIGHT, WIDTH};
use funkin_core::content::{mods as find_mods, save_mods, song_path, Paths, SongEntry, Week};
use funkin_core::achievements::LIST as ACHIEVEMENTS;
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
    Mods,
    Credits,
    Achievements,
}

/// What a menu wants next.
pub enum Go {
    Stay,
    Page(Page),
    Options,
    /// The gameplay changers, opened from story mode or freeplay with Ctrl.
    Changers,
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

/// Where the letters of a line of menu text go (each with its left and top), and the line's width.
/// Bold text is all capitals standing on a line 70 px down; plain text keeps its case on a line 110 px down.
/// `size` gives a letter's frame size and its nudge from alphabet.json; letters it does not know are left out.
// shortcut: one line only; plain text does not wrap at 65% of the screen as dialogue will need.
fn place(text: &str, bold: bool, size: impl Fn(char) -> Option<(Vec2, Vec2)>) -> (Vec<(char, Vec2)>, f32) {
    let (mut placed, mut x, mut spaces) = (Vec::new(), 0.0, 0.0);
    let (line, gap) = if bold { (70.0, 0.0) } else { (110.0, 2.0) };
    let letters: Vec<char> = if bold { text.chars().flat_map(char::to_lowercase).collect() } else { text.chars().collect() };
    for letter in letters {
        if letter == ' ' || (bold && letter == '_') {
            spaces += 1.0;
            continue;
        }
        let Some((frame, nudge)) = size(letter) else { continue };
        x += 28.0 * spaces;
        spaces = 0.0;
        placed.push((letter, vec2(x - nudge.x, line - frame.y - nudge.y)));
        x += frame.x + nudge.x + gap;
    }
    (placed, x)
}

/// Text broken into lines no wider than `width`, at spaces.
pub fn wrap(text: &str, width: f32, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let longer = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
            if measure(&longer) > width && !line.is_empty() {
                lines.push(std::mem::replace(&mut line, word.to_owned()));
            } else {
                line = longer;
            }
        }
        lines.push(line);
    }
    lines
}

/// Opens a web page in the player's browser. Links come from mods' files, so only web addresses pass, and no shell sees them.
fn open_link(link: &str) {
    if !(link.starts_with("https://") || link.starts_with("http://")) {
        return;
    }
    #[cfg(windows)]
    let (program, first): (&str, Option<&str>) = ("rundll32", Some("url.dll,FileProtocolHandler"));
    #[cfg(target_os = "macos")]
    let (program, first): (&str, Option<&str>) = ("open", None);
    #[cfg(not(any(windows, target_os = "macos")))]
    let (program, first): (&str, Option<&str>) = ("xdg-open", None);
    if let Err(error) = std::process::Command::new(program).args(first).arg(link).spawn() {
        eprintln!("Could not open {link}: {error}");
    }
}

/// The wiggling menu letters, bold and plain.
struct Alphabet {
    texture: Texture2D,
    /// The frames and the nudge of a letter, bold (true) or plain.
    letters: HashMap<(char, bool), (Vec<Frame>, Vec2)>,
}

impl Alphabet {
    fn load(paths: &Paths) -> Option<Alphabet> {
        let sheet = Sheet::load(&paths.find("images/alphabet.png")?).ok()?;
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(paths.find("images/alphabet.json")?).ok()?).ok()?;
        let mut letters = HashMap::new();
        for letter in json["allowed"].as_str()?.chars() {
            let entry = &json["characters"][letter.to_string()];
            let name = entry["animation"].as_str().map_or(letter.to_string(), str::to_owned);
            let nudge = |style: &str| vec2(entry[style][0].as_f64().unwrap_or(0.0) as f32, entry[style][1].as_f64().unwrap_or(0.0) as f32);
            let mut add = |key: (char, bool), animation: String, style: &str| {
                let frames = sheet.animation(&animation, &[]);
                if !frames.is_empty() {
                    letters.insert(key, (frames, nudge(style)));
                }
            };
            add((letter, true), format!("{name} bold"), "bold");
            if letter.is_alphabetic() {
                add((letter, false), format!("{name} lowercase"), "normal");
                add((letter.to_uppercase().next().unwrap_or(letter), false), format!("{name} uppercase"), "normal");
            } else {
                add((letter, false), format!("{name} normal"), "normal");
            }
        }
        Some(Alphabet { texture: sheet.texture, letters })
    }

    fn size(&self, letter: char, bold: bool) -> Option<(Vec2, Vec2)> {
        self.letters.get(&(letter, bold)).map(|(frames, nudge)| (frames[0].size, *nudge))
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

    /// A mod's own picture, the "pack.png" in its folder.
    fn pack(&mut self, folder: &str) -> Option<Texture2D> {
        let path = self.base.join("mods").join(folder).join("pack.png");
        self.images.entry(format!("{folder}/pack.png")).or_insert_with(|| Some(Texture2D::from_file_with_format(&std::fs::read(path).ok()?, None))).clone()
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

/// A line of the credits: someone to thank, or (without an icon and the rest) a heading or a gap.
struct Credit {
    name: String,
    person: bool,
    icon: String,
    text: String,
    link: String,
    color: Vec3,
    /// The mod whose credits file it is from.
    source: String,
}

/// Name, icon, what for, link and background colour of the credits that are always there. One entry alone is a heading or a gap.
const CREDITS: &[&[&str]] = &[
    &["Psych Engine Team"],
    &["Shadow Mario", "shadowmario", "Main Programmer and Head of Psych Engine", "https://ko-fi.com/shadowmario", "444444"],
    &["Riveren", "riveren", "Main Artist/Animator of Psych Engine", "https://x.com/riverennn", "14967B"],
    &[""],
    &["Former Engine Members"],
    &["bb-panzu", "bb", "Ex-Programmer of Psych Engine", "https://x.com/bbsub3", "3E813A"],
    &[""],
    &["Engine Contributors"],
    &["crowplexus", "crowplexus", "Linux Support, HScript Iris, Input System v3, and Other PRs", "https://twitter.com/IamMorwen", "CFCFCF"],
    &["Kamizeta", "kamizeta", "Creator of Pessy, Psych Engine's mascot.", "https://www.instagram.com/cewweey/", "D21C11"],
    &["MaxNeton", "maxneton", "Loading Screen Easter Egg Artist/Animator.", "https://bsky.app/profile/maxneton.bsky.social", "3C2E4E"],
    &["Keoiki", "keoiki", "Note Splash Animations and Latin Alphabet", "https://x.com/Keoiki_", "D2D2D2"],
    &["SqirraRNG", "sqirra", "Crash Handler and Base code for\nChart Editor's Waveform", "https://x.com/gedehari", "E1843A"],
    &["EliteMasterEric", "mastereric", "Runtime Shaders support and Other PRs", "https://x.com/EliteMasterEric", "FFBD40"],
    &["MAJigsaw77", "majigsaw", ".MP4 Video Loader Library (hxvlc)", "https://x.com/MAJigsaw77", "5F5F5F"],
    &["iFlicky", "flicky", "Composer of Psync and Tea Time\nAnd some sound effects", "https://x.com/flicky_i", "9E29CF"],
    &["KadeDev", "kade", "Fixed some issues on Chart Editor and Other PRs", "https://x.com/kade0912", "64A250"],
    &["superpowers04", "superpowers04", "LUA JIT Fork", "https://x.com/superpowers04", "B957ED"],
    &["CheemsAndFriends", "cheems", "Creator of FlxAnimate", "https://x.com/CheemsnFriendos", "E1E1E1"],
    &[""],
    &["Funkin' Crew"],
    &["ninjamuffin99", "ninjamuffin99", "Programmer of Friday Night Funkin'", "https://x.com/ninja_muffin99", "CF2D2D"],
    &["PhantomArcade", "phantomarcade", "Animator of Friday Night Funkin'", "https://x.com/PhantomArcade3K", "FADC45"],
    &["evilsk8r", "evilsk8r", "Artist of Friday Night Funkin'", "https://x.com/evilsk8r", "5ABD4B"],
    &["kawaisprite", "kawaisprite", "Composer of Friday Night Funkin'", "https://x.com/kawaisprite", "378FC7"],
    &[""],
    &["Psych Engine Discord"],
    &["Join the Psych Ward!", "discord", "", "https://discord.gg/2ka77eMXDv", "5165F6"],
];

/// A credits line from its fields, as the constant above and a mod's `data/credits.txt` (fields split by "::") give them.
fn credit(fields: &[&str], source: &str) -> Credit {
    let field = |index: usize| fields.get(index).map_or(String::new(), |text| text.trim().replace("\\n", "\n"));
    let color = u32::from_str_radix(field(4).trim_start_matches('#').trim_start_matches("0x"), 16).unwrap_or(0xFFFFFF);
    let color = vec3((color >> 16 & 255) as f32, (color >> 8 & 255) as f32, (color & 255) as f32) / 255.0;
    Credit { name: field(0), person: fields.len() > 1, icon: field(1), text: field(2), link: field(3), color, source: source.to_owned() }
}

/// A mod in the mods menu.
struct ModRow {
    folder: String,
    on: bool,
    /// From its pack.json, when it has one.
    name: String,
    text: String,
    color: Vec3,
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
    /// Main menu: the row in the middle column, whether the item at the left (-1) or right (1) is selected instead, and how far the background has slid.
    main_row: usize,
    main_side: i8,
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
    credits: Vec<Credit>,
    credit: usize,
    credit_scroll: f32,
    mods: Vec<ModRow>,
    mod_row: usize,
    mods_changed: bool,
    /// The mods were switched or moved: the weeks and songs have to be found again.
    pub reload: bool,
    /// The player's choice on flashing lights: without them nothing flashes or blinks.
    flashing: bool,
    /// Selected achievement, and how far its list has scrolled.
    achievement: usize,
    achievement_scroll: f32,
    /// An achievement was unlocked while the menus had no sound: its chime is still owed.
    pub chime: bool,
}

/// Where a step of (dx, dy) leads from `selected` in a grid of `count` things, four to a row. Rows wrap around, and so does the list.
fn grid_step(selected: usize, count: usize, dx: i32, dy: i32) -> usize {
    let (row, column, rows) = (selected / 4, selected % 4, count.div_ceil(4));
    if dx != 0 {
        let in_row = (count - row * 4).min(4) as i32;
        return row * 4 + (column as i32 + dx).rem_euclid(in_row) as usize;
    }
    let target = (row as i32 + dy).rem_euclid(rows as i32) as usize * 4 + column;
    // the last row may be short: from above it wraps on to the first row, from below it stops a row short
    if target < count { target } else if dy > 0 { column } else { target - 4 }
}

/// Whether it is Friday evening by the player's own clock.
fn friday_night() -> bool {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct SystemTime { year: u16, month: u16, day_of_week: u16, day: u16, hour: u16, minute: u16, second: u16, milliseconds: u16 }
        #[link(name = "kernel32")]
        extern "system" { fn GetLocalTime(time: *mut SystemTime); }
        let mut now = SystemTime::default();
        // SAFETY: GetLocalTime fills the struct it is given and cannot fail.
        unsafe { GetLocalTime(&mut now) };
        now.day_of_week == 5 && now.hour >= 18
    }
    #[cfg(not(windows))]
    {
        // the fields of C's `struct tm` this needs, and room for the ones platforms add after them
        #[repr(C)]
        struct Tm { sec: i32, min: i32, hour: i32, mday: i32, mon: i32, year: i32, wday: i32, yday: i32, isdst: i32, rest: [u64; 4] }
        extern "C" { fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm; }
        let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs() as i64);
        let mut now = Tm { sec: 0, min: 0, hour: 0, mday: 0, mon: 0, year: 0, wday: 0, yday: 0, isdst: 0, rest: [0; 4] };
        // SAFETY: both pointers are to live values of the sizes localtime_r reads and writes.
        unsafe { localtime_r(&seconds, &mut now) };
        now.wday == 5 && now.hour >= 18
    }
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
            main_row: 0, main_side: 0, main_slide: 0.0, week: 0, week_scroll: 0.0, difficulty: "Normal".to_owned(), rows: Vec::new(), song: 0, song_scroll: 0.0, tint: Vec3::ONE, shown: (0.0, 0.0),
            credits: Vec::new(), credit: 0, credit_scroll: 0.0, mods: Vec::new(), mod_row: 0, mods_changed: false, reload: false, flashing: true, achievement: 0, achievement_scroll: 0.0, chime: false,
        }
    }

    /// Gives up the sound card for a song. The menu music starts again when a menu is next shown.
    pub fn quiet(&mut self) {
        self.audio = None;
        self.chosen = None;
        self.rows.clear(); // a finished week may have unlocked songs
    }

    pub fn sound(&self, name: &str, volume: f32) {
        if let (Some(audio), Some(path)) = (&self.audio, self.paths.find(&format!("sounds/{name}.ogg"))) {
            audio.play_sound(&path, volume, false);
        }
    }

    /// Width of a line of menu letters.
    fn width(&self, skin: &Skin, text: &str, bold: bool) -> f32 {
        match &self.alphabet {
            Some(alphabet) => place(text, bold, |letter| alphabet.size(letter, bold)).1,
            None => measure_text(text, skin.font.as_ref(), 48, 1.0).width,
        }
    }

    /// Menu letters with their top left at (x, y), squeezed sideways by `squeeze`.
    fn write(&self, skin: &Skin, text: &str, bold: bool, x: f32, y: f32, squeeze: f32, alpha: f32) {
        // without the letter sheet the plain font has to do
        let Some(alphabet) = &self.alphabet else { return skin.text(text, x, y + 55.0, 48, white(alpha)) };
        let step = (get_time() * 24.0) as usize;
        for (letter, at) in place(text, bold, |letter| alphabet.size(letter, bold)).0 {
            let frames = &alphabet.letters[&(letter, bold)].0;
            let frame = frames[step % frames.len()];
            let params = DrawTextureParams { dest_size: Some(vec2(frame.source.w * squeeze, frame.source.h)), source: Some(frame.source), ..Default::default() };
            draw_texture_ex(&alphabet.texture, x + (at.x + frame.offset.x) * squeeze, y + at.y + frame.offset.y, white(alpha), params);
        }
    }

    /// The menu background in a colour that follows `target`.
    fn backdrop(&mut self, target: Vec3, dt: f32) {
        self.tint += (target - self.tint) * (1.0 - (-dt * 5.0).exp());
        if let Some(picture) = self.art.image("", "menuDesat") {
            draw_texture(&picture, (WIDTH - picture.width()) / 2.0, (HEIGHT - picture.height()) / 2.0, Color::new(self.tint.x, self.tint.y, self.tint.z, 1.0));
        }
    }

    /// Runs and draws one frame of a menu.
    pub fn run(&mut self, page: Page, skin: &Skin, weeks: &[Week], songs: &[SongEntry], save: &Save, flashing: bool) -> Go {
        self.flashing = flashing;
        if self.audio.is_none() {
            let audio = Song::load(&[]);
            if let Some(path) = self.paths.find("music/freakyMenu.ogg") {
                audio.play_sound(&path, 0.7, true);
            }
            self.audio = Some(audio);
            self.music_at = get_time();
        }
        if std::mem::take(&mut self.chime) {
            self.sound("confirmMenu", 0.5);
        }
        let go = match page {
            Page::Title => self.title(skin),
            Page::Main => self.main_menu(skin),
            Page::Story => self.story(skin, weeks, songs, save),
            Page::Freeplay => self.freeplay(skin, weeks, songs, save),
            Page::Mods => self.mods_menu(skin),
            Page::Credits => self.credits_roll(skin),
            Page::Achievements => self.achievements(skin, save),
        };
        let flash = 1.0 - (get_time() - self.flash.0) / self.flash.1;
        if flash > 0.0 && self.flashing {
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
                self.write(skin, line, true, (WIDTH - self.width(skin, line, true)) / 2.0, row as f32 * 60.0 + 200.0 + shift, 1.0, 1.0);
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
                Some(_) => (if self.flashing { "ENTER PRESSED" } else { "ENTER FREEZE" }, WHITE),
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
        const MIDDLE: [&str; 4] = ["story_mode", "freeplay", "mods", "credits"];
        let (now, dt) = (get_time(), get_frame_time().min(0.1));
        let since = self.chosen.map(|at| (now - at) as f32);
        if since.is_none() {
            let step = pressed(&DOWN) as usize + pressed(&UP) as usize * (MIDDLE.len() - 1);
            if step != 0 && self.main_side == 0 {
                self.main_row = (self.main_row + step) % MIDDLE.len();
                self.sound("scrollMenu", 1.0);
            }
            let side = (self.main_side + pressed(&RIGHT) as i8 - pressed(&LEFT) as i8).clamp(-1, 1);
            if side != self.main_side {
                self.main_side = side;
                self.sound("scrollMenu", 1.0);
            }
        }

        // every item: name, top left corner, whether it is the selected one
        let mut items: Vec<(&str, Vec2, bool)> = MIDDLE.iter().enumerate().map(|(row, name)| (*name, vec2(0.0, row as f32 * 140.0 + 90.0 + (4 - MIDDLE.len()) as f32 * 70.0), self.main_side == 0 && row == self.main_row)).collect();
        // the two at the sides: achievements from the left edge, options up to the right edge
        items.push(("achievements", vec2(60.0, 490.0), self.main_side == -1));
        items.push(("options", vec2(WIDTH - 60.0, 490.0), self.main_side == 1));
        let mut drawn = Vec::new();
        for (index, (name, mut at, selected)) in items.into_iter().enumerate() {
            let Some(sheet) = self.art.sheet("", &format!("mainmenu/menu_{name}")) else { continue };
            let idle = sheet.animation(&format!("{name} idle"), &[]);
            let Some(resting) = idle.first().map(|frame| frame.size) else { continue };
            at.x = if index < MIDDLE.len() { (WIDTH - resting.x) / 2.0 } else if name == "options" { at.x - resting.x } else { at.x };
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
        let pink = self.flashing && since.is_some_and(|t| t < 1.1 && (t / 0.15) as u32 % 2 == 0);
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
                (1, _) => Go::Options,
                (-1, _) => Go::Page(Page::Achievements),
                (_, 0) => Go::Page(Page::Story),
                (_, 1) => Go::Page(Page::Freeplay),
                (_, 2) => Go::Page(Page::Mods),
                (_, _) => Go::Page(Page::Credits),
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
            None if is_key_pressed(KeyCode::LeftControl) || is_key_pressed(KeyCode::RightControl) => Go::Changers,
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
        let chosen = row.entries[difficulty];
        self.backdrop(Vec3::from(row.color.map(|c| c as f32 / 255.0)), dt);
        let row = &self.rows[self.song];
        self.song_scroll += (self.song as f32 - self.song_scroll) * (1.0 - (-dt * 9.6).exp());
        let near = |row: f32| (row.round().max(0.0) as usize).min(self.rows.len());
        for index in near(self.song_scroll - 4.0)..near(self.song_scroll + 4.0) {
            let (row, away) = (&self.rows[index], index as f32 - self.song_scroll);
            let (x, y, alpha) = (away * 20.0 + 90.0, away * 156.0 + 320.0, if index == self.song { 1.0 } else { 0.6 });
            let width = self.width(skin, &row.title, true);
            let squeeze = (980.0 / width).min(1.0);
            self.write(skin, &row.title, true, x, y, squeeze, alpha);
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
        let tip = "Up/Down: song / Left/Right: difficulty / Enter: play / Ctrl: Gameplay Changers / Esc: back";
        label(skin, tip, (WIDTH - measure_text(tip, skin.font.as_ref(), 16, 1.0).width) / 2.0, HEIGHT - 22.0, 16, WHITE, false);

        if is_key_pressed(KeyCode::LeftControl) || is_key_pressed(KeyCode::RightControl) {
            Go::Changers
        } else if pressed(&ACCEPT) {
            Go::Play(vec![chosen], None)
        } else if pressed(&BACK) {
            self.sound("cancelMenu", 1.0);
            Go::Page(Page::Main)
        } else {
            Go::Stay
        }
    }

    /// The achievement earned by opening the main menu on a Friday evening, if it is one.
    pub fn greeting(&self) -> Option<&'static str> {
        friday_night().then_some("friday_night_play")
    }

    /// The picture of an achievement: its own, or the padlock while it is locked.
    fn badge(&mut self, id: &str, unlocked: bool) -> Option<Texture2D> {
        if !unlocked {
            return self.art.image("", "achievements/lockedachievement");
        }
        [format!("achievements/{id}-pixel"), format!("achievements/{id}"), "unknownMod".to_owned()].iter().find_map(|name| self.art.image("", name))
    }

    /// The notice that an achievement was unlocked, `age` seconds after it was. It drops in at the top left, stays, and leaves.
    pub fn popup(&mut self, skin: &Skin, id: &str, age: f32, slot: usize) {
        let Some(achievement) = ACHIEVEMENTS.iter().find(|achievement| achievement.id == id) else { return };
        let shown = (age / 0.4).min((4.0 - age) / 0.4).clamp(0.0, 1.0);
        let (x, y) = (20.0, -130.0 + 150.0 * shown + slot as f32 * 140.0);
        draw_rectangle(x, y, 420.0, 130.0, BLACK);
        if let Some(picture) = self.badge(id, true) {
            draw_texture_ex(&picture, x + 15.0, y + 15.0, WHITE, DrawTextureParams { dest_size: Some(vec2(100.0, 100.0)), ..Default::default() });
        }
        label(skin, achievement.name, x + 130.0, y + 35.0, 16, WHITE, false);
        let measure = |text: &str| measure_text(text, skin.font.as_ref(), 16, 1.0).width;
        for (row, line) in wrap(achievement.text, 270.0, measure).iter().enumerate() {
            label(skin, line, x + 130.0, y + 65.0 + row as f32 * 18.0, 16, WHITE, false);
        }
    }

    fn achievements(&mut self, skin: &Skin, save: &Save) -> Go {
        let dt = get_frame_time().min(0.1);
        if let Some(picture) = self.art.image("", "menuBGBlue") {
            let size = picture.size() * 1.1;
            draw_texture_ex(&picture, (WIDTH - size.x) / 2.0, (HEIGHT - size.y) / 2.0, WHITE, DrawTextureParams { dest_size: Some(size), ..Default::default() });
        }
        // hidden ones stay out of the list until they are unlocked
        let listed: Vec<_> = ACHIEVEMENTS.iter().filter(|achievement| !achievement.hidden || save.unlocked(achievement.id)).collect();
        let (dx, dy) = (pressed(&RIGHT) as i32 - pressed(&LEFT) as i32, pressed(&DOWN) as i32 - pressed(&UP) as i32);
        self.achievement = self.achievement.min(listed.len() - 1);
        if dx != 0 || dy != 0 {
            self.achievement = grid_step(self.achievement, listed.len(), dx, dy);
            self.sound("scrollMenu", 1.0);
        }
        // the list scrolls so that the first row sits near the top and the last one clear of the text below
        let (rows, height) = (listed.len() / 4, (listed.len().div_ceil(4) - 1) as f32 * 180.0 + 150.0);
        let target = if rows > 0 { (self.achievement / 4) as f32 / rows as f32 * (height - HEIGHT / 2.0 - 50.0).max(0.0) - 100.0 } else { -100.0 };
        self.achievement_scroll += (target - self.achievement_scroll) * (1.0 - 0.85f32.powf(dt * 60.0));
        let top = -self.achievement_scroll;
        draw_rectangle((WIDTH - 750.0) / 2.0, top - 30.0, 750.0, height + 60.0, Color::new(0.0, 0.0, 0.0, 0.6));
        for (index, achievement) in listed.iter().enumerate() {
            let at = vec2(295.0 + 180.0 * (index % 4) as f32, top + (index / 4) as f32 * 180.0);
            if let Some(picture) = self.badge(achievement.id, save.unlocked(achievement.id)) {
                let params = DrawTextureParams { dest_size: Some(vec2(150.0, 150.0)), ..Default::default() };
                draw_texture_ex(&picture, at.x, at.y, white(if index == self.achievement { 1.0 } else { 0.6 }), params);
            }
        }
        let selected = listed[self.achievement];
        draw_rectangle(0.0, 570.0, WIDTH, HEIGHT - 570.0, Color::new(0.0, 0.0, 0.0, 0.6));
        let centred = |text: &str, top: f32, size: u16| drop(label(skin, text, (WIDTH - measure_text(text, skin.font.as_ref(), size, 1.0).width) / 2.0, top, size, WHITE, false));
        centred(if save.unlocked(selected.id) { selected.name } else { "???" }, 580.0, 32);
        centred(selected.text, 618.0, 24);
        if pressed(&BACK) {
            self.sound("cancelMenu", 1.0);
            return Go::Page(Page::Main);
        }
        Go::Stay
    }

    fn credits_roll(&mut self, skin: &Skin) -> Go {
        let dt = get_frame_time().min(0.1);
        if self.credits.is_empty() {
            // the credits of the mods that are switched on come first
            for (folder, _) in find_mods(&self.art.base).into_iter().filter(|(_, on)| *on) {
                let Ok(text) = std::fs::read_to_string(self.art.base.join("mods").join(&folder).join("data/credits.txt")) else { continue };
                self.credits.extend(text.lines().map(|line| credit(&line.split("::").collect::<Vec<_>>(), &folder)));
                self.credits.push(credit(&[""], ""));
            }
            self.credits.extend(CREDITS.iter().map(|fields| credit(fields, "")));
            self.credit = self.credits.iter().position(|credit| credit.person).unwrap_or(0);
            self.credit_scroll = self.credit as f32;
        }
        let count = self.credits.len();
        let step = (pressed(&DOWN) as usize + pressed(&UP) as usize * (count - 1)) % count;
        if step != 0 && self.credits.iter().any(|credit| credit.person) {
            // Shift goes three at a time; headings and gaps are stepped over
            let times = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) { 3 } else { 1 };
            self.credit = (self.credit + step * times) % count;
            while !self.credits[self.credit].person {
                self.credit = (self.credit + step) % count;
            }
            self.sound("scrollMenu", 0.4);
        }
        self.backdrop(self.credits[self.credit].color, dt);
        self.credit_scroll += (self.credit as f32 - self.credit_scroll) * (1.0 - (-dt * 9.6).exp());
        let near = |row: f32| (row.round().max(0.0) as usize).min(count);
        for index in near(self.credit_scroll - 4.0)..near(self.credit_scroll + 4.0) {
            let (credit, away) = (&self.credits[index], index as f32 - self.credit_scroll);
            let y = away * 156.0 + 300.0;
            let width = self.width(skin, &credit.name, !credit.person);
            if !credit.person {
                self.write(skin, &credit.name, true, (WIDTH - width) / 2.0, y, 1.0, 1.0);
                continue;
            }
            // names step in from the left as they near the middle row, where the selected one sits centred
            let (aside, centred) = (200.0 - 40.0 * away.abs(), (WIDTH - width) / 2.0 - 70.0);
            let x = aside + (centred - aside) * (1.0 - away.abs()).clamp(0.0, 1.0);
            let alpha = if index == self.credit { 1.0 } else { 0.6 };
            self.write(skin, &credit.name, false, x, y, 1.0, alpha);
            let icon = [format!("credits/{}", credit.icon), format!("credits/{}-pixel", credit.icon), "credits/missing_icon".to_owned()].iter().find_map(|name| self.art.image(&credit.source, name));
            if let Some(icon) = icon {
                draw_texture(&icon, x + width + 10.0, y, white(alpha));
            }
        }
        let credit = &self.credits[self.credit];
        if !credit.text.trim().is_empty() {
            let lines: Vec<&str> = credit.text.lines().collect();
            let height = lines.len() as f32 * 36.0 + 4.0;
            let top = HEIGHT - height - 60.0;
            draw_rectangle(40.0, top - 10.0, 1200.0, height + 25.0, Color::new(0.0, 0.0, 0.0, 0.6));
            for (row, line) in lines.iter().enumerate() {
                label(skin, line, (WIDTH - measure_text(line, skin.font.as_ref(), 32, 1.0).width) / 2.0, top + row as f32 * 36.0, 32, WHITE, false);
            }
        }
        if pressed(&ACCEPT) {
            open_link(&credit.link);
        }
        if pressed(&BACK) {
            self.sound("cancelMenu", 1.0);
            self.credits.clear(); // read again next time: the mods may change
            return Go::Page(Page::Main);
        }
        Go::Stay
    }

    fn mods_menu(&mut self, skin: &Skin) -> Go {
        let dt = get_frame_time().min(0.1);
        let base = self.art.base.clone();
        if self.mods.is_empty() {
            for (folder, on) in find_mods(&base) {
                let pack: serde_json::Value = std::fs::read_to_string(base.join("mods").join(&folder).join("pack.json")).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
                let text = |key: &str, or: &str| pack[key].as_str().filter(|text| !text.trim().is_empty()).unwrap_or(or).to_owned();
                let color = [0, 1, 2].map(|i| pack["color"][i].as_f64().map(|c| c as f32 / 255.0));
                let color = if color.contains(&None) { vec3(0.4, 0.353, 1.0) } else { Vec3::from(color.map(Option::unwrap_or_default)) };
                self.mods.push(ModRow { name: text("name", &folder), text: text("description", "No description provided."), color, folder, on });
            }
            self.mod_row = self.mod_row.min(self.mods.len().saturating_sub(1));
        }
        let panel = Color::new(0.0, 0.0, 0.0, 0.6);
        if self.mods.is_empty() {
            self.backdrop(vec3(0.4, 0.353, 1.0), dt);
            draw_rectangle(40.0, 40.0, 340.0, 440.0, panel);
            label(skin, "No Mods found.", 55.0, 55.0, 16, WHITE, false);
            for (row, line) in ["NO MODS INSTALLED", "PRESS BACK TO EXIT OR INSTALL A MOD"].iter().enumerate() {
                label(skin, line, 400.0 + (860.0 - measure_text(line, skin.font.as_ref(), 32, 1.0).width) / 2.0, 320.0 + row as f32 * 40.0, 32, WHITE, false);
            }
            return if pressed(&BACK) { Go::Page(Page::Main) } else { Go::Stay };
        }

        let count = self.mods.len();
        let step = (pressed(&DOWN) as usize + pressed(&UP) as usize * (count - 1)) % count;
        if step != 0 {
            let next = (self.mod_row + step) % count;
            // with Shift the selected mod moves through the list: mods higher up are looked in first
            if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
                let moved = self.mods.remove(self.mod_row);
                self.mods.insert(next, moved);
                self.mods_changed = true;
            }
            self.mod_row = next;
            self.sound("scrollMenu", 0.6);
        }
        if pressed(&ACCEPT) {
            self.mods[self.mod_row].on ^= true;
            self.mods_changed = true;
            self.sound("scrollMenu", 0.6);
        }

        self.backdrop(self.mods[self.mod_row].color, dt);
        let icon = |art: &mut Art, folder: &str, x: f32, y: f32, size: f32, alpha: f32| {
            if let Some(picture) = art.pack(folder).or_else(|| art.image("", "unknownMod")) {
                // a pack picture may hold several frames of 150 px; the first is shown
                let params = DrawTextureParams { dest_size: Some(vec2(size, size)), source: Some(Rect::new(0.0, 0.0, picture.width().min(150.0), picture.height().min(150.0))), ..Default::default() };
                draw_texture_ex(&picture, x, y, white(alpha), params);
            }
        };
        // the list, with the selected mod on its middle row
        draw_rectangle(40.0, 40.0, 340.0, 440.0, panel);
        for (index, row) in self.mods.iter().enumerate() {
            let slot = index as i64 - self.mod_row as i64 + 2;
            if !(0..5).contains(&slot) {
                continue;
            }
            let (y, alpha) = (45.0 + 86.0 * slot as f32, if row.on { 1.0 } else { 0.4 });
            if index == self.mod_row {
                draw_rectangle(45.0, y, 330.0, 85.0, white(0.25));
            }
            icon(&mut self.art, &row.folder, 50.0, y + 5.0, 75.0, alpha);
            let name: String = row.name.chars().take(22).collect();
            label(skin, &name, 140.0, y + 22.0, 16, white(alpha), false);
            label(skin, if row.on { "ON" } else { "OFF" }, 140.0, y + 46.0, 16, if row.on { GREEN } else { GRAY }, false);
        }
        label(skin, "Enter: on/off", 55.0, 500.0, 16, WHITE, false);
        label(skin, "Shift+Up/Down: move up or down", 55.0, 524.0, 16, WHITE, false);
        label(skin, "Esc: save and go back", 55.0, 548.0, 16, WHITE, false);

        let row = &self.mods[self.mod_row];
        draw_rectangle(400.0, 40.0, 840.0, 180.0, panel);
        icon(&mut self.art, &row.folder, 415.0, 55.0, 150.0, 1.0);
        let width = self.width(skin, &row.name, true);
        self.write(skin, &row.name, true, 580.0, 95.0, (640.0 / width).min(1.0), 1.0);
        draw_rectangle(400.0, 240.0, 840.0, 450.0, panel);
        let measure = |text: &str| measure_text(text, skin.font.as_ref(), 24, 1.0).width;
        for (line, text) in wrap(&row.text, 810.0, measure).iter().take(14).enumerate() {
            label(skin, text, 415.0, 255.0 + line as f32 * 28.0, 24, WHITE, false);
        }
        label(skin, "Mods higher in the list are looked in first. Changes take effect when you go back.", 415.0, 660.0, 16, GRAY, false);

        if pressed(&BACK) {
            self.sound("cancelMenu", 1.0);
            if self.mods_changed {
                let list: Vec<(String, bool)> = self.mods.iter().map(|row| (row.folder.clone(), row.on)).collect();
                if let Err(error) = save_mods(&base, &list) {
                    eprintln!("Could not save modsList.txt: {error}");
                }
                // everything found through the mods is found again
                (self.reload, self.mods_changed) = (true, false);
                self.paths = Paths::new(&base, "", "");
                self.art = Art { base, images: HashMap::new(), sheets: HashMap::new(), characters: HashMap::new() };
                self.rows.clear();
                (self.week, self.song) = (0, 0);
            }
            self.mods.clear();
            return Go::Page(Page::Main);
        }
        Go::Stay
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
        let (placed, width) = place("A a'?", true, size);
        // capitals use the same letters; the unknown "?" is left out
        assert_eq!(placed, [('a', vec2(0.0, 2.0)), ('a', vec2(78.0, 2.0)), ('\'', vec2(128.0, 5.0))]);
        assert_eq!(width, 148.0);
        // plain letters keep their case (no capital A here), stand lower and sit 2 px apart
        let (placed, width) = place("Aa a", false, size);
        assert_eq!(placed, [('a', vec2(0.0, 42.0)), ('a', vec2(80.0, 42.0))]);
        assert_eq!(width, 132.0);
    }

    #[test]
    fn steps_through_a_grid_whose_last_row_is_short() {
        // 14 things: three full rows and a row of two
        assert_eq!([grid_step(3, 14, 1, 0), grid_step(0, 14, -1, 0), grid_step(13, 14, 1, 0), grid_step(12, 14, -1, 0)], [0, 3, 12, 13]);
        assert_eq!([grid_step(1, 14, 0, 1), grid_step(13, 14, 0, 1), grid_step(11, 14, 0, 1)], [5, 1, 3]);
        assert_eq!([grid_step(1, 14, 0, -1), grid_step(3, 14, 0, -1), grid_step(5, 14, 0, -1)], [13, 11, 1]);
        assert_eq!([grid_step(0, 1, 1, 0), grid_step(0, 1, 0, 1), grid_step(0, 1, 0, -1)], [0, 0, 0]);
    }

    #[test]
    fn text_wraps_at_spaces() {
        let lines = wrap("one two three\nfour", 7.0, |text| text.len() as f32);
        assert_eq!(lines, ["one two", "three", "four"]);
    }

    #[test]
    fn credits_lines_are_people_or_headings() {
        let person = credit(&"Someone::icon::Did\\nthings::https://example.com::FF8000".split("::").collect::<Vec<_>>(), "My Mod");
        assert!(person.person && person.text == "Did\nthings" && person.color == vec3(1.0, 128.0 / 255.0, 0.0) && person.source == "My Mod");
        assert!(!credit(&["A heading"], "").person && !credit(&[""], "").person);
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
