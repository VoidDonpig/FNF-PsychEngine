//! Friday Night Funkin' engine in Rust: song list, play screen, results.
//!
//! Run it from a folder that has the game's `assets` (and optionally `mods`), or pass `--base <folder>`.
//! `--bot` lets the game play itself, `--play <text>` starts the first song whose name contains the text.
//! For checking a build without a screen: `--at <seconds>` starts at that point of the song and
//! `--shot <file.png>` saves the picture after a few frames and quits.
mod atlas;
mod audio;

use funkin_core::content::{discover, SongEntry};
use funkin_core::play::{Event, NoteState, Play};
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;
/// Notes and strums are drawn at this share of the sheet's size.
const NOTE_SCALE: f32 = 0.7;
const LANE_WIDTH: f32 = 160.0 * NOTE_SCALE;
const STRUM_Y: f32 = 50.0 + LANE_WIDTH / 2.0;
/// Pixels a note travels per millisecond at scroll speed 1.
const PIXELS_PER_MS: f32 = 0.45;
/// The song starts this long after the play screen opens.
const LEAD_IN_MS: f64 = 2000.0;

const KEYS: [[KeyCode; 2]; 4] = [[KeyCode::A, KeyCode::Left], [KeyCode::S, KeyCode::Down], [KeyCode::W, KeyCode::Up], [KeyCode::D, KeyCode::Right]];
const DIRECTIONS: [&str; 4] = ["left", "down", "up", "right"];
const NOTE_NAMES: [&str; 4] = ["purple", "blue", "green", "red"];
/// The sheet stores each note as three masks; these are the colours they become per lane (body, shine, outline).
const LANE_COLORS: [[u32; 3]; 4] = [[0xC24B99, 0xFFFFFF, 0x3C1F56], [0x00FFFF, 0xFFFFFF, 0x1542B7], [0x12FA05, 0xFFFFFF, 0x0A4447], [0xF9393F, 0xFFFFFF, 0x651038]];

const VERTEX: &str = "#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec2 uv;
varying lowp vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}";
const RECOLOR: &str = "#version 100
precision mediump float;
varying lowp vec2 uv;
varying lowp vec4 color;
uniform sampler2D Texture;
uniform vec3 r;
uniform vec3 g;
uniform vec3 b;
void main() {
    vec4 mask = texture2D(Texture, uv);
    gl_FragColor = vec4(min(mask.r * r + mask.g * g + mask.b * b, vec3(1.0)), mask.a) * color;
}";

struct Skin {
    sheet: Texture2D,
    frames: HashMap<String, Rect>,
    /// One recolouring material per lane.
    lanes: Vec<Material>,
    font: Option<Font>,
}

impl Skin {
    fn load(base: &Path) -> Result<Skin, String> {
        let dir = base.join("assets/shared/images/noteSkins");
        let read = |name: &str| std::fs::read(dir.join(name)).map_err(|e| format!("{}: {e}", dir.join(name).display()));
        let sheet = Texture2D::from_file_with_format(&read("NOTE_assets.png")?, None);
        let frames = atlas::parse(&String::from_utf8_lossy(&read("NOTE_assets.xml")?));
        let rgb = |c: u32| vec3((c >> 16 & 255) as f32, (c >> 8 & 255) as f32, (c & 255) as f32) / 255.0;
        let mut lanes = Vec::new();
        for colors in LANE_COLORS {
            let uniforms = ["r", "g", "b"].iter().map(|name| UniformDesc::new(name, UniformType::Float3)).collect();
            // custom materials do not blend unless told to
            let blend = BlendState::new(Equation::Add, BlendFactor::Value(BlendValue::SourceAlpha), BlendFactor::OneMinusValue(BlendValue::SourceAlpha));
            let params = MaterialParams { uniforms, pipeline_params: PipelineParams { color_blend: Some(blend), ..Default::default() }, ..Default::default() };
            let material = load_material(ShaderSource::Glsl { vertex: VERTEX, fragment: RECOLOR }, params).map_err(|e| e.to_string())?;
            for (name, color) in ["r", "g", "b"].iter().zip(colors) {
                material.set_uniform(name, rgb(color));
            }
            lanes.push(material);
        }
        let font = std::fs::read(base.join("assets/fonts/vcr.ttf")).ok().and_then(|bytes| load_ttf_font_from_bytes(&bytes).ok());
        Ok(Skin { sheet, frames, lanes, font })
    }

    /// Draws a sheet frame centred on (x, y), stretched to `size` if given. `lane` recolours it.
    fn frame(&self, name: &str, x: f32, y: f32, size: Option<Vec2>, lane: Option<usize>, alpha: f32) {
        let Some(&source) = self.frames.get(name) else { return };
        let size = size.unwrap_or(vec2(source.w, source.h) * NOTE_SCALE);
        match lane {
            Some(lane) => gl_use_material(&self.lanes[lane]),
            None => gl_use_default_material(),
        }
        draw_texture_ex(&self.sheet, x - size.x / 2.0, y - size.y / 2.0, Color::new(1.0, 1.0, 1.0, alpha), DrawTextureParams { dest_size: Some(size), source: Some(source), ..Default::default() });
        gl_use_default_material();
    }

    fn text(&self, text: &str, x: f32, y: f32, size: u16, color: Color) {
        // a dark copy underneath keeps it readable over notes
        for (dx, dy, color) in [(2.0, 2.0, BLACK), (0.0, 0.0, color)] {
            draw_text_ex(text, x + dx, y + dy, TextParams { font: self.font.as_ref(), font_size: size, color, ..Default::default() });
        }
    }

    fn centered(&self, text: &str, y: f32, size: u16, color: Color) {
        let width = measure_text(text, self.font.as_ref(), size, 1.0).width;
        self.text(text, (WIDTH - width) / 2.0, y, size, color);
    }
}

fn lane_x(player: bool, lane: usize) -> f32 {
    92.0 + if player { WIDTH / 2.0 } else { 0.0 } + LANE_WIDTH * lane as f32 + LANE_WIDTH / 2.0
}

struct Playing {
    entry: SongEntry,
    play: Play,
    song: audio::Song,
    opened: Instant,
    started: bool,
    /// Where the song starts (`--at`), in ms.
    start_at: f64,
    position: f64,
    /// Until when each strum shows its hit flash, per side (0 = opponent, 1 = player).
    flash_until: [[f64; 4]; 2],
    /// Last judgement to show, and when it happened.
    judgement: Option<(String, Color, f64)>,
}

impl Playing {
    fn open(entry: &SongEntry, botplay: bool, start_at: f64) -> Result<Playing, String> {
        let text = std::fs::read_to_string(&entry.chart).map_err(|e| format!("{}: {e}", entry.chart.display()))?;
        let chart = funkin_core::chart::parse(&text).map_err(|e| format!("{}: {e}", entry.chart.display()))?;
        let mut tracks = vec![entry.inst.as_path()];
        tracks.extend(entry.voices.as_deref());
        // shortcut: --at only moves the silent clock (it exists for screenshots); seek the audio too to make it a practice feature.
        let song = audio::Song::load(if start_at > 0.0 { &[] } else { &tracks });
        let mut play = Play::new(chart, botplay);
        play.skip_to(start_at);
        Ok(Playing { entry: entry.clone(), play, song, opened: Instant::now(), started: false, start_at, position: -LEAD_IN_MS, flash_until: [[0.0; 4]; 2], judgement: None })
    }

    /// Runs one frame of the song. Returns false once it is over.
    fn update(&mut self) -> bool {
        let lead_in = if self.start_at > 0.0 { 0.0 } else { LEAD_IN_MS };
        let waited = self.opened.elapsed().as_secs_f64() * 1000.0;
        if !self.started && waited >= lead_in {
            self.song.start();
            self.started = true;
        }
        // The sound card's clock wobbles by a buffer between callbacks; the song never runs backwards.
        let now = if self.started { self.start_at + self.song.position_ms() } else { waited - lead_in };
        self.position = self.position.max(now);

        let mut events = Vec::new();
        for (lane, keys) in KEYS.iter().enumerate() {
            // shortcut: a key press is timed at the frame that sees it. Feed timestamped events (native/psych_input) in here for frame-rate independent judgement.
            if keys.iter().any(|&key| is_key_pressed(key)) {
                events.extend(self.play.press(lane, self.position));
            }
            if !keys.iter().any(|&key| is_key_down(key)) {
                self.play.release(lane);
            }
        }
        events.extend(self.play.update(self.position));
        for event in events {
            match event {
                Event::Hit { lane, rating, diff_ms } => {
                    self.flash_until[1][lane] = self.position + 120.0;
                    let color = if diff_ms < 0.0 { Color::from_hex(0x7FC8FF) } else { Color::from_hex(0xFF9F7F) };
                    self.judgement = Some((format!("{}  {:+.1} ms", rating.name(), diff_ms), color, self.position));
                }
                Event::Miss { .. } => self.judgement = Some(("Miss".to_owned(), Color::from_hex(0xFF4040), self.position)),
                Event::OpponentHit { lane } => self.flash_until[0][lane] = self.position + 120.0,
            }
        }

        let last_note = self.play.chart.notes.last().map_or(0.0, |n| n.time + n.sustain);
        let end = self.song.length_ms().unwrap_or(last_note + 1500.0);
        self.play.health > 0.0 && self.position < end
    }

    fn draw(&self, skin: &Skin) {
        let speed = self.play.chart.speed as f32;
        let y_at = |time: f64| STRUM_Y + (time - self.position) as f32 * PIXELS_PER_MS * speed;

        for side in 0..2 {
            for lane in 0..4 {
                let (x, hit) = (lane_x(side == 1, lane), self.position < self.flash_until[side][lane]);
                let held = side == 1 && !self.play.botplay && KEYS[lane].iter().any(|&key| is_key_down(key));
                if hit {
                    skin.frame(&format!("{} confirm0003", DIRECTIONS[lane]), x, STRUM_Y, None, Some(lane), 1.0);
                } else if held {
                    skin.frame(&format!("{} press0003", DIRECTIONS[lane]), x, STRUM_Y, None, Some(lane), 1.0);
                } else {
                    skin.frame(&format!("arrow{}0000", DIRECTIONS[lane].to_uppercase()), x, STRUM_Y, None, None, 1.0);
                }
            }
        }

        let visible_until = self.position + (HEIGHT / (PIXELS_PER_MS * speed)) as f64 + 200.0;
        let notes = self.play.chart.notes.iter().zip(&self.play.state).take_while(|(note, _)| note.time < visible_until);
        // holds first so that heads lie on top of them
        for pass in 0..2 {
            for (note, &state) in notes.clone() {
                let x = lane_x(note.player, note.lane);
                let alpha = if state == NoteState::Missed { 0.3 } else { 1.0 };
                if pass == 0 && note.sustain > 0.0 && state != NoteState::Done {
                    let top = if state == NoteState::Held { STRUM_Y } else { y_at(note.time) };
                    let bottom = y_at(note.time + note.sustain);
                    if bottom > top && top < HEIGHT {
                        let size = vec2(50.0 * NOTE_SCALE, bottom - top);
                        skin.frame(&format!("{} hold piece0000", NOTE_NAMES[note.lane]), x, (top + bottom) / 2.0, Some(size), Some(note.lane), 0.6 * alpha);
                    }
                }
                if pass == 1 && matches!(state, NoteState::Pending | NoteState::Missed) && y_at(note.time) > -LANE_WIDTH {
                    let tint = if note.hurt { None } else { Some(note.lane) }; // hurt notes show as raw masks
                    skin.frame(&format!("{}0000", NOTE_NAMES[note.lane]), x, y_at(note.time), None, tint, alpha);
                }
            }
        }

        // health: the player's share grows from the right
        let (bar_x, bar_y, bar_w) = (WIDTH / 2.0 - 300.0, HEIGHT * 0.89, 600.0);
        let player_share = (self.play.health / 2.0) as f32;
        draw_rectangle(bar_x - 4.0, bar_y - 4.0, bar_w + 8.0, 26.0, BLACK);
        draw_rectangle(bar_x, bar_y, bar_w * (1.0 - player_share), 18.0, Color::from_hex(0xFF0000));
        draw_rectangle(bar_x + bar_w * (1.0 - player_share), bar_y, bar_w * player_share, 18.0, Color::from_hex(0x66FF33));

        let rating = self.play.accuracy().map_or("?".to_owned(), |a| format!("{:.2}%", a * 100.0));
        skin.centered(&format!("Score: {} | Misses: {} | Rating: {}", self.play.score, self.play.misses, rating), bar_y + 52.0, 20, WHITE);
        if self.play.combo > 1 {
            skin.text(&format!("{} combo", self.play.combo), WIDTH * 0.35 - 80.0, HEIGHT / 2.0 + 60.0, 28, WHITE);
        }
        if let Some((text, color, at)) = &self.judgement {
            let age = (self.position - at) as f32;
            if age < 700.0 {
                let fade = Color { a: (1.0 - (age - 500.0).max(0.0) / 200.0), ..*color };
                skin.text(text, WIDTH * 0.35 - 80.0, HEIGHT / 2.0 + 20.0, 32, fade);
            }
        }
        if self.play.botplay {
            skin.centered("BOTPLAY", 150.0, 32, WHITE);
        }
        skin.text(&format!("{} [{}]", self.entry.title, self.entry.difficulty), 12.0, HEIGHT - 12.0, 16, WHITE);
    }
}

enum Screen {
    Menu,
    Playing(Box<Playing>),
    /// The finished run, and whether the player survived it.
    Results(Box<Playing>, bool),
    Failed(String),
}

fn window() -> Conf {
    Conf { window_title: "Friday Night Funkin': Psych Engine (Rust)".to_owned(), window_width: WIDTH as i32, window_height: HEIGHT as i32, ..Default::default() }
}

#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let shot = value("--shot");
    let start_at = value("--at").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) * 1000.0;

    // the game's files: where asked, else here, else next to the program
    let base = value("--base").map(PathBuf::from).unwrap_or_else(|| {
        let here = PathBuf::from(".");
        let beside = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        if here.join("assets").is_dir() { here } else { beside.filter(|p| p.join("assets").is_dir()).unwrap_or(here) }
    });

    let songs = discover(&base);
    let mut botplay = flag("--bot");
    let mut selected = 0usize;
    let mut frames = 0u32;
    let (skin, mut screen) = match Skin::load(&base) {
        Ok(skin) => (Some(skin), Screen::Menu),
        Err(error) => (None, Screen::Failed(format!("Could not load the note skin.\n{error}\nRun the game from the folder that holds \"assets\", or pass --base <folder>."))),
    };
    if let (Some(wanted), Screen::Menu) = (value("--play"), &screen) {
        let wanted = wanted.to_lowercase();
        screen = match songs.iter().find(|s| format!("{} {}", s.title, s.difficulty).to_lowercase().contains(&wanted)) {
            Some(entry) => Playing::open(entry, botplay, start_at).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p))),
            None => Screen::Failed(format!("No song matches \"{wanted}\".")),
        };
    }

    loop {
        // the game is laid out for 1280x720 and stretched to the window
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        clear_background(Color::from_hex(0x1B1B26));
        let plain = |text: &str, x: f32, y: f32, size: u16, color: Color| match &skin {
            Some(skin) => skin.text(text, x, y, size, color),
            None => drop(draw_text(text, x, y, size as f32, color)),
        };

        screen = match screen {
            Screen::Menu => {
                plain("Friday Night Funkin' - Rust engine", 40.0, 60.0, 36, WHITE);
                plain(&format!("Up/Down: choose   Enter: play   B: botplay {}   Esc: quit", if botplay { "ON" } else { "off" }), 40.0, 96.0, 18, GRAY);
                if songs.is_empty() {
                    plain(&format!("No songs found under {}", base.display()), 40.0, 160.0, 22, ORANGE);
                }
                let first = selected.saturating_sub(8);
                for (row, (index, song)) in songs.iter().enumerate().skip(first).take(18).enumerate() {
                    let from = if song.source.is_empty() { String::new() } else { format!("   ({})", song.source) };
                    let color = if index == selected { YELLOW } else { WHITE };
                    plain(&format!("{}{} [{}]{from}", if index == selected { "> " } else { "  " }, song.title, song.difficulty), 40.0, 150.0 + row as f32 * 30.0, 24, color);
                }
                if is_key_pressed(KeyCode::Down) && selected + 1 < songs.len() { selected += 1; }
                if is_key_pressed(KeyCode::Up) { selected = selected.saturating_sub(1); }
                if is_key_pressed(KeyCode::B) { botplay = !botplay; }
                if is_key_pressed(KeyCode::Escape) { break; }
                match songs.get(selected) {
                    Some(entry) if is_key_pressed(KeyCode::Enter) => Playing::open(entry, botplay, 0.0).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p))),
                    _ => Screen::Menu,
                }
            }
            Screen::Playing(mut playing) => {
                let alive = playing.update();
                if let Some(skin) = &skin {
                    playing.draw(skin);
                }
                if is_key_pressed(KeyCode::Escape) {
                    Screen::Menu
                } else if alive {
                    Screen::Playing(playing)
                } else {
                    let survived = playing.play.health > 0.0;
                    Screen::Results(playing, survived)
                }
            }
            Screen::Results(playing, survived) => {
                let play = &playing.play;
                plain(if survived { "Song cleared" } else { "Game over" }, 40.0, 120.0, 48, if survived { GREEN } else { RED });
                plain(&format!("{} [{}]", playing.entry.title, playing.entry.difficulty), 40.0, 170.0, 28, WHITE);
                let accuracy = play.accuracy().map_or("-".to_owned(), |a| format!("{:.2}%", a * 100.0));
                plain(&format!("Score {}    Misses {}    Accuracy {}", play.score, play.misses, accuracy), 40.0, 220.0, 28, WHITE);
                plain("Enter: back to the song list", 40.0, 280.0, 20, GRAY);
                if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Escape) { Screen::Menu } else { Screen::Results(playing, survived) }
            }
            Screen::Failed(message) => {
                for (row, line) in message.lines().enumerate() {
                    plain(line, 40.0, 120.0 + row as f32 * 30.0, 22, ORANGE);
                }
                plain("Esc: quit", 40.0, 320.0, 20, GRAY);
                if is_key_pressed(KeyCode::Escape) { break; }
                Screen::Failed(message)
            }
        };

        frames += 1;
        if let (Some(path), true) = (&shot, frames == 5) {
            // the frame is only drawn for real when it ends; push it out now to read it back
            unsafe { get_internal_gl() }.flush();
            get_screen_data().export_png(path);
            break;
        }
        next_frame().await;
    }
}
