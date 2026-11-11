//! Friday Night Funkin' engine in Rust: song list, play screen, results.
//!
//! Run it from a folder that has the game's `assets` (and optionally `mods`), or pass `--base <folder>`.
//! `--bot` lets the game play itself, `--play <text>` starts the first song whose name contains the text.
//! For checking a build without a screen: `--at <seconds>` starts at that point of the song, `--options`
//! opens the options, and `--shot <file.png>` saves the picture after a few frames (or `--after <seconds>`, or
//! once the song has reached `--when <seconds>`) and quits.
mod animate;
mod atlas;
mod audio;
mod dialogue;
mod fight;
mod input;
mod menu;
mod options;
mod props;
mod scene;
mod settings;

use funkin_core::content::{songs as find_songs, weeks as find_weeks, Paths, SongEntry};
use funkin_core::achievements::{earned, Run};
use funkin_core::save::Save;
use menu::{Go, Menus, Page};
use funkin_core::chart::NoteKind;
use funkin_core::play::{Event, NoteState, Play, Rating};
use input::{KeyEvent, Keyboard};
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;
use settings::Settings;
use std::path::{Path, PathBuf};

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;
/// Notes and strums are drawn at this share of the sheet's size.
const NOTE_SCALE: f32 = 0.7;
const LANE_WIDTH: f32 = 160.0 * NOTE_SCALE;
/// Pixels a note travels per millisecond at scroll speed 1.
const PIXELS_PER_MS: f32 = 0.45;

const DIRECTIONS: [&str; 4] = ["left", "down", "up", "right"];
const NOTE_NAMES: [&str; 4] = ["purple", "blue", "green", "red"];
/// Pixel stages draw their small pictures this many times as large.
const PIXEL_ZOOM: f32 = 6.0;
const PIXEL_LANE_COLORS: [[u32; 3]; 4] = [[0xE276FF, 0xFFF9FF, 0x60008D], [0x3DCAFF, 0xF4FFFF, 0x003060], [0x71E300, 0xF6FFE6, 0x003100], [0xFF884E, 0xFFFAF5, 0x6C0000]];
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
    frames: Vec<(String, atlas::Frame)>,
    /// One recolouring material per lane.
    lanes: Vec<Material>,
    font: Option<Font>,
    /// Pictures for sick, good, bad, shit.
    ratings: Vec<Texture2D>,
    /// Pictures of the digits 0 to 9.
    digits: Vec<Texture2D>,
    /// Pictures for ready, set, go.
    countdown: Vec<Texture2D>,
    splashes: Option<atlas::Sheet>,
    /// How much the note pictures are enlarged, and how wide a hold's body and how long its end cap are drawn.
    note_scale: f32,
    hold: Vec2,
    /// How much the rating, combo digit and countdown pictures are enlarged.
    popup_scales: [f32; 3],
}

/// A picture that pops up, jumps and fades: a rating, a combo digit, a countdown step.
struct Popup {
    texture: Texture2D,
    /// Top left corner when it appears.
    at: Vec2,
    scale: f32,
    /// Upward kick (px/s) and gravity (px/s2).
    motion: (f32, f32),
    /// Song position (ms) it appeared at, and how long it stays before fading out.
    born: f64,
    stays: f64,
}

impl Skin {
    /// One recolouring material per lane, for the three colours each lane's notes are made of.
    fn lanes(lane_colors: [[u32; 3]; 4]) -> Result<Vec<Material>, String> {
        let rgb = |c: u32| vec3((c >> 16 & 255) as f32, (c >> 8 & 255) as f32, (c & 255) as f32) / 255.0;
        let mut lanes = Vec::new();
        for colors in lane_colors {
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
        Ok(lanes)
    }

    /// The pictures named, from a folder; all of them or none (missing pictures only cost their effect).
    fn pictures(dir: &Path, names: &[&str], pixel: bool) -> Vec<Texture2D> {
        let found: Vec<Texture2D> = names.iter().filter_map(|name| std::fs::read(dir.join(format!("{name}.png"))).ok()).map(|bytes| Texture2D::from_file_with_format(&bytes, None)).collect();
        if pixel {
            found.iter().for_each(|picture| picture.set_filter(FilterMode::Nearest));
        }
        if found.len() == names.len() { found } else { Vec::new() }
    }

    fn load(base: &Path) -> Result<Skin, String> {
        let dir = base.join("assets/shared/images/noteSkins");
        let read = |name: &str| std::fs::read(dir.join(name)).map_err(|e| format!("{}: {e}", dir.join(name).display()));
        let sheet = Texture2D::from_file_with_format(&read("NOTE_assets.png")?, None);
        let frames = atlas::parse(&String::from_utf8_lossy(&read("NOTE_assets.xml")?));
        let font = std::fs::read(base.join("assets/fonts/vcr.ttf")).ok().and_then(|bytes| load_ttf_font_from_bytes(&bytes).ok());
        let images = base.join("assets/shared/images");
        let digits = Skin::pictures(&images, &["num0", "num1", "num2", "num3", "num4", "num5", "num6", "num7", "num8", "num9"], false);
        let splashes = atlas::Sheet::load(&images.join("noteSplashes/noteSplashes.png")).ok();
        Ok(Skin { sheet, frames, lanes: Skin::lanes(LANE_COLORS)?, font, ratings: Skin::pictures(&images, &["sick", "good", "bad", "shit"], false), digits, countdown: Skin::pictures(&images, &["ready", "set", "go"], false), splashes,
            note_scale: NOTE_SCALE, hold: vec2(50.0, 64.0) * NOTE_SCALE, popup_scales: [0.7, 0.5, 1.0] })
    }

    /// The look of pixel stages: notes from a grid of small pictures drawn six times as large, with their own colours, ratings and countdown.
    fn load_pixel(base: &Path) -> Result<Skin, String> {
        let dir = base.join("assets/shared/images/pixelUI");
        let image = |name: &str| -> Result<Image, String> {
            let path = dir.join(name);
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            Image::from_file_with_format(&bytes, None).map_err(|e| format!("{}: {e}", path.display()))
        };
        let (notes, ends) = (image("noteSkins/NOTE_assets.png")?, image("noteSkins/NOTE_assetsENDS.png")?);
        // one picture for both: the notes, and the hold pieces under them
        let mut both = Image::gen_image_color(notes.width.max(ends.width), notes.height + ends.height, Color::new(0.0, 0.0, 0.0, 0.0));
        for (source, top) in [(&notes, 0), (&ends, notes.height as u32)] {
            for y in 0..source.height as u32 {
                for x in 0..source.width as u32 {
                    both.set_pixel(x, y + top, source.get_pixel(x, y));
                }
            }
        }
        let sheet = Texture2D::from_image(&both);
        sheet.set_filter(FilterMode::Nearest);
        // notes: four lanes across; the strum, the note, two frames of the pressed strum and two of the hit one down.
        // hold pieces: the body above its end.
        let (note, end) = (vec2(notes.width as f32 / 4.0, notes.height as f32 / 5.0), vec2(ends.width as f32 / 4.0, ends.height as f32 / 2.0));
        let cell = |at: Vec2, size: Vec2| atlas::Frame { source: Rect::new(at.x, at.y, size.x, size.y), offset: Vec2::ZERO, size, rotated: false };
        let mut frames = Vec::new();
        for lane in 0..4 {
            let (column, piece) = (lane as f32 * note.x, vec2(lane as f32 * end.x, notes.height as f32));
            frames.push((format!("arrow{}0000", DIRECTIONS[lane].to_uppercase()), cell(vec2(column, 0.0), note)));
            frames.push((format!("{}0000", NOTE_NAMES[lane]), cell(vec2(column, note.y), note)));
            frames.push((format!("{} press0003", DIRECTIONS[lane]), cell(vec2(column, note.y * 2.0), note)));
            frames.push((format!("{} confirm0003", DIRECTIONS[lane]), cell(vec2(column, note.y * 4.0), note)));
            frames.push((format!("{} hold piece0000", NOTE_NAMES[lane]), cell(piece, end)));
            frames.push((format!("{} hold end0000", NOTE_NAMES[lane]), cell(piece + vec2(0.0, end.y), end)));
        }
        let font = std::fs::read(base.join("assets/fonts/vcr.ttf")).ok().and_then(|bytes| load_ttf_font_from_bytes(&bytes).ok());
        let digits = Skin::pictures(&dir, &["num0-pixel", "num1-pixel", "num2-pixel", "num3-pixel", "num4-pixel", "num5-pixel", "num6-pixel", "num7-pixel", "num8-pixel", "num9-pixel"], true);
        // shortcut: the usual splashes; the Haxe engine coarsens them with a shader on pixel stages.
        let splashes = atlas::Sheet::load(&base.join("assets/shared/images/noteSplashes/noteSplashes.png")).ok();
        Ok(Skin { sheet, frames, lanes: Skin::lanes(PIXEL_LANE_COLORS)?, font, ratings: Skin::pictures(&dir, &["sick-pixel", "good-pixel", "bad-pixel", "shit-pixel"], true), digits,
            countdown: Skin::pictures(&dir, &["ready-pixel", "set-pixel", "date-pixel"], true), splashes, note_scale: PIXEL_ZOOM, hold: end * PIXEL_ZOOM, popup_scales: [PIXEL_ZOOM * 0.85, PIXEL_ZOOM, PIXEL_ZOOM] })
    }

    /// Draws a sheet frame centred on (x, y), stretched to `size` if given. `lane` recolours it.
    fn frame(&self, name: &str, x: f32, y: f32, size: Option<Vec2>, lane: Option<usize>, alpha: f32) {
        let Some(source) = self.frames.iter().find(|(n, _)| n == name).map(|(_, f)| f.source) else { return };
        let size = size.unwrap_or(vec2(source.w, source.h) * self.note_scale);
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

/// Centre of a strum. With middlescroll the player's lanes sit in the middle of the screen.
fn lane_x(settings: &Settings, player: bool, lane: usize) -> f32 {
    let left = if settings.middlescroll { (WIDTH - 4.0 * LANE_WIDTH) / 2.0 } else { 92.0 + if player { WIDTH / 2.0 } else { 0.0 } };
    left + LANE_WIDTH * lane as f32 + LANE_WIDTH / 2.0
}

fn strum_y(settings: &Settings) -> f32 {
    if settings.downscroll { HEIGHT - 150.0 + LANE_WIDTH / 2.0 } else { 50.0 + LANE_WIDTH / 2.0 }
}

struct Playing {
    entry: SongEntry,
    play: Play,
    song: audio::Song,
    /// Which of the song's tracks is the player's voice (it drops out on a miss).
    player_voice: Option<usize>,
    /// Stage and characters; None when they could not be loaded (the notes still work).
    scene: Option<scene::Scene>,
    /// Ms spent on the play screen before the song starts.
    waited: f64,
    started: bool,
    /// The pause menu's selected row, while paused.
    paused: Option<usize>,
    /// Where the song starts (`--at`), in ms.
    start_at: f64,
    position: f64,
    /// The options as they were when the song started.
    settings: Settings,
    /// Bound keys that are down right now.
    held_keys: Vec<KeyCode>,
    /// Until when each strum shows its hit flash, per side (0 = opponent, 1 = player).
    flash_until: [[f64; 4]; 2],
    /// Last judgement to show, and when it happened.
    judgement: Option<(String, Color, f64)>,
    /// The talk before the song, while it lasts.
    cutscene: Option<dialogue::Dialogue>,
    /// When the player last hit a note: the score text swells for a moment.
    last_hit: f64,
    /// For the achievements: the lanes the player pressed, whether the player's character ever went back to its
    /// idle move (and for how long it has been there), and for how long it has been singing without a rest.
    lanes_used: [bool; 4],
    idled: bool,
    idle_for: f32,
    sang_for: f32,
    /// Where sounds and other shared files are found for this song.
    paths: Paths,
    /// Ms between opening the play screen and the song's first beat: the countdown.
    lead_in: f64,
    /// The last beat the countdown announced.
    counted: i64,
    popups: Vec<Popup>,
    /// Lanes that just got a "sick" hit, and when.
    splashes: Vec<(usize, f64)>,
    /// Seconds since the player lost, and since they confirmed the retry.
    dead: Option<(f32, Option<f32>)>,
    /// The next chart event to carry out.
    next_event: usize,
    /// Scroll speed right now, and the change a chart event is easing it through: from, to, start (ms), length (ms).
    speed: f32,
    speed_change: Option<(f32, f32, f64, f64)>,
}

impl Playing {
    /// `story`: played in story mode, where some songs open with a talk.
    fn open(base: &Path, entry: &SongEntry, settings: &Settings, start_at: f64, story: bool) -> Result<Playing, String> {
        let text = std::fs::read_to_string(&entry.chart).map_err(|e| format!("{}: {e}", entry.chart.display()))?;
        let mut chart = funkin_core::chart::parse(&text).map_err(|e| format!("{}: {e}", entry.chart.display()))?;
        // events shared by all difficulties of the song sit in their own file
        if let Some(shared) = std::fs::read_to_string(entry.chart.with_file_name("events.json")).ok().and_then(|text| funkin_core::chart::parse(&text).ok()) {
            chart.events.extend(shared.events);
            chart.events.sort_by(|a, b| a.time.total_cmp(&b.time));
        }
        let mut tracks = vec![entry.inst.as_path()];
        tracks.extend(entry.voices_player.as_deref());
        let player_voice = entry.voices_player.as_ref().map(|_| 1);
        tracks.extend(entry.voices_opponent.as_deref());
        let mut scene = scene::Scene::load(base, &entry.source, &chart).map_err(|e| eprintln!("Stage: {e}")).ok();
        if let Some(scene) = &mut scene {
            (scene.cam_zooms, scene.flashing) = (settings.cam_zooms, settings.flashing);
        }
        // shortcut: --at only moves the silent clock (it exists for screenshots); seek the audio too to make it a practice feature.
        let song = audio::Song::load_at(if start_at > 0.0 { &[] } else { &tracks }, settings.playback_rate);
        // five beats of countdown, as the Haxe engine does
        let lead_in = if start_at > 0.0 { 0.0 } else { 5.0 * 60000.0 / chart.bpm };
        let speed = settings.note_speed(chart.speed) as f32;
        // sounds may sit in the stage's own folder
        let directory = scene.as_ref().map_or(String::new(), |scene| scene.directory.clone());
        let paths = Paths::new(base, &entry.source, &directory);
        let cutscene = if story && start_at <= 0.0 { dialogue::Dialogue::load(base, &paths, &funkin_core::content::song_path(&entry.title), &chart.stage) } else { None };
        let mut play = Play::new(chart, settings.botplay);
        play.ghost_tapping = settings.ghost_tapping;
        play.timing = settings.timing();
        (play.health_gain, play.health_loss, play.instakill) = (settings.health_gain, settings.health_loss, settings.instakill);
        play.skip_to(start_at);
        Ok(Playing { entry: entry.clone(), play, song, player_voice, scene, waited: 0.0, started: false, paused: None, start_at, position: -lead_in, settings: settings.clone(), held_keys: Vec::new(), flash_until: [[f64::MIN; 4]; 2], judgement: None, last_hit: f64::MIN, lanes_used: [false; 4], idled: false, idle_for: 0.0, sang_for: 0.0,
            paths, cutscene, lead_in, counted: i64::MIN, popups: Vec::new(), splashes: Vec::new(), dead: None, next_event: 0, speed, speed_change: None })
    }

    /// Whether the stage wants the pixel look for notes and HUD.
    fn pixel(&self) -> bool {
        self.scene.as_ref().is_some_and(|scene| scene.pixel)
    }

    /// What the stage adds to the names of the game over sounds.
    fn death_suffix(&self) -> &'static str {
        self.scene.as_ref().map_or("", |scene| scene.death().2)
    }

    /// What pixel stages add to the names of their countdown sounds.
    fn ui_suffix(&self) -> &'static str {
        if self.pixel() { "-pixel" } else { "" }
    }

    /// Song position the notes are judged and drawn at.
    fn judged(&self) -> f64 {
        self.position - self.settings.offset_ms
    }

    fn sound(&self, name: &str, volume: f32, looped: bool) {
        let folder = if looped { "music" } else { "sounds" };
        if let Some(path) = self.paths.find(&format!("{folder}/{name}.ogg")) {
            self.song.play_sound(&path, volume, looped);
        }
    }

    /// The game over scene: the fall, then waiting for retry (Enter) or giving up (Esc).
    fn update_dead(&mut self, since: f32, confirmed: Option<f32>) -> Flow {
        let dt = get_frame_time().min(0.1);
        let confirm = confirmed.is_none() && is_key_pressed(KeyCode::Enter);
        let fell = self.scene.as_mut().is_some_and(|scene| scene.update_dead(dt, confirm));
        // without a scene to play the fall, the music starts after a moment
        if (fell || (self.scene.is_none() && since < 1.0 && since + dt >= 1.0)) && confirmed.is_none() {
            self.sound(&format!("gameOver{}", self.death_suffix()), 1.0, true);
        }
        if confirm {
            self.song.stop_sounds();
            self.sound(&format!("gameOverEnd{}", self.death_suffix()), 1.0, false);
        }
        let confirmed = if confirm { Some(0.0) } else { confirmed.map(|t| t + dt) };
        self.dead = Some((since + dt, confirmed));
        if is_key_pressed(KeyCode::Escape) {
            self.song.stop_sounds();
            Flow::Quit
        } else if confirmed.is_some_and(|t| t > 2.5) {
            Flow::Restart
        } else {
            Flow::Continue
        }
    }

    /// Runs one frame of the song with the key changes since the last one.
    fn update(&mut self, keys: &[KeyEvent], skin: &Skin) -> Flow {
        if let Some((since, confirmed)) = self.dead {
            return self.update_dead(since, confirmed);
        }
        if let Some(talk) = &mut self.cutscene {
            let accept = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space);
            let back = is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Backspace);
            let (cues, over) = talk.update(get_frame_time().min(0.1), accept, back);
            if over {
                self.cutscene = None;
            }
            for cue in cues {
                match cue {
                    dialogue::Cue::Sound(name, volume) => self.sound(name, volume, false),
                    dialogue::Cue::Music(name) => self.sound(name, 0.8, true),
                    // shortcut: the music stops at once; the Haxe engine fades it out over a second and a half.
                    dialogue::Cue::Quiet => self.song.stop_sounds(),
                }
            }
            return Flow::Continue;
        }
        if let Some(row) = self.paused {
            let row = (row + is_key_pressed(KeyCode::Down) as usize + 2 * is_key_pressed(KeyCode::Up) as usize) % 3;
            self.paused = Some(row);
            let chosen = if is_key_pressed(KeyCode::Escape) { Some(0) } else { is_key_pressed(KeyCode::Enter).then_some(row) };
            return match chosen {
                Some(0) => {
                    self.paused = None;
                    self.song.stop_sounds();
                    if self.started {
                        self.song.start();
                    }
                    Flow::Continue
                }
                Some(1) => Flow::Restart,
                Some(_) => Flow::Quit,
                None => Flow::Continue,
            };
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Escape) {
            self.song.pause();
            self.paused = Some(0);
            if self.settings.pause_music > 0 {
                self.sound(&funkin_core::content::song_path(settings::PAUSE_MUSIC[self.settings.pause_music]), 0.5, true);
            }
            return Flow::Continue;
        }

        // a frame that took long (loading) does not eat the countdown; the Haxe engine caps frames the same way
        let dt = get_frame_time().min(0.1);
        let lead_in = self.lead_in;
        // everything follows the song's speed: the countdown, the stage, the notes
        let rate = self.settings.playback_rate;
        self.waited += dt as f64 * 1000.0 * rate;
        if !self.started && self.waited >= lead_in {
            self.song.start();
            self.started = true;
        }
        // The sound card's clock wobbles by a buffer between callbacks; the song never runs backwards.
        let now = if self.started { self.start_at + self.song.position_ms() } else { self.waited - lead_in };
        self.position = self.position.max(now);

        // the countdown: a sound on each of the four beats before the song, a picture on the last three
        let crochet = 60000.0 / self.play.chart.bpm;
        let beat = self.play.chart.beat_at(self.position).floor() as i64;
        if beat != self.counted && (-4..0).contains(&beat) {
            self.counted = beat;
            self.sound(&format!("{}{}", ["intro3", "intro2", "intro1", "introGo"][(beat + 4) as usize], self.ui_suffix()), 0.6, false);
            if let Some(picture) = skin.countdown.get((beat + 3) as usize) {
                let scale = skin.popup_scales[2];
                let at = (vec2(WIDTH, HEIGHT) - picture.size() * scale) / 2.0;
                self.popups.push(Popup { texture: picture.clone(), at, scale, motion: (0.0, 0.0), born: self.position, stays: crochet * 0.6 });
            }
        }

        let judged = self.judged();
        while let Some(event) = self.play.chart.events.get(self.next_event).filter(|e| e.time <= judged).cloned() {
            self.next_event += 1;
            let number = |value: &str| value.trim().parse::<f32>().ok();
            match event.name.as_str() {
                // a constant scroll speed is the player's choice against exactly this
                "Change Scroll Speed" if self.settings.scroll_type == 1 => {}
                "Change Scroll Speed" => {
                    let to = self.settings.note_speed(self.play.chart.speed) as f32 * number(&event.value1).unwrap_or(1.0);
                    self.speed_change = Some((self.speed, to, event.time, number(&event.value2).unwrap_or(0.0) as f64 * 1000.0));
                }
                "Play Sound" => self.sound(&event.value1, number(&event.value2).unwrap_or(1.0), false),
                _ => self.scene.iter_mut().for_each(|scene| scene.event(&event)),
            }
        }
        if let Some((from, to, start, length)) = self.speed_change {
            let done = if length > 0.0 { ((judged - start) / length).clamp(0.0, 1.0) as f32 } else { 1.0 };
            self.speed = from + (to - from) * done;
            if done >= 1.0 {
                self.speed_change = None;
            }
        }
        let mut events = Vec::new();
        for key in keys {
            let Some(lane) = self.settings.lane_of(key.key) else { continue };
            if key.pressed && !self.held_keys.contains(&key.key) {
                self.held_keys.push(key.key);
                if !self.play.botplay {
                    self.lanes_used[lane] = true;
                    // judged where the song was when the key went down, not where it is this frame
                    events.extend(self.play.press(lane, judged - key.age_ms));
                }
            } else if !key.pressed {
                self.held_keys.retain(|held| *held != key.key);
                if !self.lane_held(lane) {
                    self.play.release(lane);
                }
            }
        }
        events.extend(self.play.update(judged));
        for &event in &events {
            match event {
                Event::Hit { lane, rating, diff_ms, .. } => {
                    self.flash_until[1][lane] = judged + 120.0;
                    let color = if diff_ms < 0.0 { Color::from_hex(0x7FC8FF) } else { Color::from_hex(0xFF9F7F) };
                    if self.settings.show_ms {
                        self.judgement = Some((format!("{:+.1} ms", diff_ms), color, judged));
                    }
                    self.last_hit = judged;
                    self.player_voice.iter().for_each(|&track| self.song.set_volume(track, 1.0));
                    if self.settings.hitsound_volume > 0.0 && !self.play.botplay {
                        self.sound("hitsound", self.settings.hitsound_volume as f32, false);
                    }
                    if rating == Rating::Sick {
                        self.splashes.push((lane, judged));
                    }
                    if self.settings.hide_hud {
                        continue;
                    }
                    if !self.settings.combo_stacking {
                        self.popups.clear(); // one rating at a time
                    }
                    // the rating jumps up left of the player's lanes, the combo count under it
                    let left = if self.settings.middlescroll { 60.0 } else { WIDTH * 0.35 - 40.0 };
                    if let Some(picture) = skin.ratings.get(rating as usize) {
                        let scale = skin.popup_scales[0];
                        let at = vec2(left, (HEIGHT - picture.height() * scale) / 2.0 - 60.0);
                        self.popups.push(Popup { texture: picture.clone(), at, scale, motion: (160.0, 550.0), born: judged, stays: crochet });
                    }
                    if self.play.combo >= 10 && !skin.digits.is_empty() {
                        for (place, digit) in format!("{:03}", self.play.combo).bytes().enumerate() {
                            let picture = &skin.digits[(digit - b'0') as usize];
                            let scale = skin.popup_scales[1];
                            let at = vec2(left - 50.0 + 43.0 * place as f32, (HEIGHT - picture.height() * scale) / 2.0 + 80.0);
                            self.popups.push(Popup { texture: picture.clone(), at, scale, motion: (150.0, 250.0), born: judged, stays: crochet * 2.0 });
                        }
                    }
                }
                Event::Miss { .. } => {
                    if self.settings.show_ms {
                        self.judgement = Some(("Miss".to_owned(), Color::from_hex(0xFF4040), judged));
                    }
                    self.player_voice.iter().for_each(|&track| self.song.set_volume(track, 0.0));
                    self.sound(["missnote1", "missnote2", "missnote3"][self.play.misses as usize % 3], 0.15, false);
                }
                Event::OpponentHit { lane, .. } => self.flash_until[0][lane] = judged + 120.0,
            }
        }

        if let Some(scene) = &mut self.scene {
            let mut held = [[false; 4]; 2];
            for (note, state) in self.play.chart.notes.iter().zip(&self.play.state).take_while(|(note, _)| note.time <= judged) {
                held[note.player as usize][note.lane] |= *state == NoteState::Held;
            }
            scene.levels = self.song.levels();
            scene.update(dt * rate as f32, judged, &self.play.chart, &events, held);
            for (path, volume) in scene.sounds.drain(..) {
                self.song.play_sound(&path, volume, false);
            }
        }
        if self.started {
            // a moment in the idle move is forgiven, as the Haxe engine forgives it
            if self.scene.as_ref().is_none_or(|scene| scene.player_idle()) {
                self.idle_for += dt;
                self.idled |= self.idle_for >= 0.15;
                self.sang_for = 0.0;
            } else {
                self.idle_for = 0.0;
                self.sang_for += dt;
            }
        }

        self.splashes.retain(|&(_, at)| judged - at < 300.0);
        self.popups.retain(|popup| self.position.max(judged) - popup.born < popup.stays + 200.0);

        // the reset key gives up the run, unless a lane uses that key
        let reset = is_key_pressed(KeyCode::R) && !self.settings.no_reset && self.settings.lane_of(KeyCode::R).is_none() && !self.play.botplay && self.started;
        if (self.play.health <= 0.0 || reset) && !self.settings.practice {
            self.dead = Some((0.0, None));
            self.song.pause();
            self.sound(self.scene.as_ref().map_or("fnf_loss_sfx", |scene| scene.death().1), 1.0, false);
            self.scene.iter_mut().for_each(|scene| scene.die());
            return Flow::Continue;
        }
        if self.position < self.length() { Flow::Continue } else { Flow::Finished }
    }

    /// Length of the song in ms.
    fn length(&self) -> f64 {
        let last_note = self.play.chart.notes.last().map_or(0.0, |n| n.time + n.sustain);
        self.song.length_ms().unwrap_or(last_note + 1500.0)
    }

    fn lane_held(&self, lane: usize) -> bool {
        self.held_keys.iter().any(|&key| self.settings.lane_of(key) == Some(lane))
    }

    fn draw(&self, skin: &Skin) {
        let (settings, position) = (&self.settings, self.judged());
        // the stage through its own moving camera, then notes and the rest of the HUD on top
        let (hud_zoom, hud_shake) = self.scene.as_ref().map_or((1.0, Vec2::ZERO), |scene| {
            scene.draw();
            (scene.hud_zoom, scene.hud_shake())
        });
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0) + hud_shake, zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT) * hud_zoom, ..Default::default() });
        if self.dead.is_some() {
            skin.centered("Enter: retry    Esc: back to the song list", HEIGHT - 30.0, 18, GRAY);
            return;
        }
        // a faster song does not make the notes travel faster: they are spread out to match
        let speed = self.speed / settings.playback_rate as f32;
        let strum_y = strum_y(settings);
        // notes come up from below, or with downscroll down from above
        let direction = if settings.downscroll { -1.0 } else { 1.0 };
        let y_at = |time: f64| strum_y + direction * (time - position) as f32 * PIXELS_PER_MS * speed;
        // with middlescroll, or when the player wants them gone, the opponent still plays but is not shown
        let shown = |player: bool| player || (!settings.middlescroll && settings.opponent_strums);

        for side in (0..2).filter(|&side| shown(side == 1)) {
            for lane in 0..4 {
                let (x, hit) = (lane_x(settings, side == 1, lane), position < self.flash_until[side][lane]);
                let held = side == 1 && !self.play.botplay && self.lane_held(lane);
                if hit {
                    skin.frame(&format!("{} confirm0003", DIRECTIONS[lane]), x, strum_y, None, Some(lane), 1.0);
                } else if held {
                    skin.frame(&format!("{} press0003", DIRECTIONS[lane]), x, strum_y, None, Some(lane), 1.0);
                } else {
                    skin.frame(&format!("arrow{}0000", DIRECTIONS[lane].to_uppercase()), x, strum_y, None, None, 1.0);
                }
            }
        }

        let visible_until = position + (HEIGHT / (PIXELS_PER_MS * speed)) as f64 + 200.0;
        let on_screen = |y: f32| y > -LANE_WIDTH && y < HEIGHT + LANE_WIDTH;
        let notes = self.play.chart.notes.iter().zip(&self.play.state).take_while(|(note, _)| note.time < visible_until).filter(|(note, _)| shown(note.player));
        // holds first so that heads lie on top of them
        for pass in 0..2 {
            for (note, &state) in notes.clone() {
                let x = lane_x(settings, note.player, note.lane);
                let alpha = if state == NoteState::Missed { 0.3 } else { 1.0 };
                if pass == 0 && note.sustain > 0.0 && state != NoteState::Done {
                    let head = if state == NoteState::Held { strum_y } else { y_at(note.time) };
                    let tail = y_at(note.time + note.sustain);
                    let (top, bottom) = (head.min(tail), head.max(tail));
                    // a held hold ends at the strum: nothing left once its tail has passed it
                    if (tail - head) * direction > 0.0 && bottom > 0.0 && top < HEIGHT {
                        // the last stretch is the rounded end cap (turned over when notes come from above)
                        let cap = skin.hold.y.min(bottom - top);
                        let cap_y = tail - direction * cap / 2.0;
                        let size = vec2(skin.hold.x, bottom - top - cap);
                        skin.frame(&format!("{} hold piece0000", NOTE_NAMES[note.lane]), x, (head + tail - direction * cap) / 2.0, Some(size), Some(note.lane), 0.6 * alpha);
                        skin.frame(&format!("{} hold end0000", NOTE_NAMES[note.lane]), x, cap_y, Some(vec2(skin.hold.x, cap * direction)), Some(note.lane), 0.6 * alpha);
                    }
                }
                if pass == 1 && matches!(state, NoteState::Pending | NoteState::Missed) && on_screen(y_at(note.time)) {
                    let tint = if note.kind == NoteKind::Hurt { None } else { Some(note.lane) }; // hurt notes show as raw masks
                    skin.frame(&format!("{}0000", NOTE_NAMES[note.lane]), x, y_at(note.time), None, tint, alpha);
                }
            }
        }

        // a burst on the strum for every "sick" hit
        if let Some(sheet) = &skin.splashes {
            for &(lane, at) in &self.splashes {
                let frames = sheet.animation(&format!("note splash {} 1", NOTE_NAMES[lane]), &[]);
                if let Some(&frame) = frames.get(((position - at) * 0.024) as usize) {
                    gl_use_material(&skin.lanes[lane]);
                    sheet.draw(frame, lane_x(settings, true, lane) - frame.size.x / 2.0, strum_y - frame.size.y / 2.0, 1.0, false, Color::new(1.0, 1.0, 1.0, settings.splash_alpha as f32));
                    gl_use_default_material();
                }
            }
        }
        for popup in &self.popups {
            let age = ((self.position.max(position) - popup.born) / 1000.0) as f32;
            let fade = 1.0 - ((age * 1000.0 - popup.stays as f32) / 200.0).clamp(0.0, 1.0);
            let y = popup.at.y - popup.motion.0 * age + popup.motion.1 * age * age / 2.0;
            draw_texture_ex(&popup.texture, popup.at.x, y, Color::new(1.0, 1.0, 1.0, fade), DrawTextureParams { dest_size: Some(popup.texture.size() * popup.scale), ..Default::default() });
        }

        // time left, over a bar that fills as the song goes
        let (length, time_y) = (self.length(), if settings.downscroll { HEIGHT - 44.0 } else { 19.0 });
        let hud = !settings.hide_hud;
        if hud && settings.time_bar < 3 {
            let played = self.position.clamp(0.0, length);
            let clock = |ms: f64| format!("{}:{:02}", ms as u32 / 60000, ms as u32 / 1000 % 60);
            let words = [clock(length - played), clock(played), self.entry.title.clone()];
            // the song's name stands alone; the times sit on a bar that fills as the song goes
            if settings.time_bar < 2 {
                draw_rectangle(WIDTH / 2.0 - 204.0, time_y - 4.0, 408.0, 27.0, BLACK);
                draw_rectangle(WIDTH / 2.0 - 200.0, time_y, 400.0 * (played / length) as f32, 19.0, WHITE);
            }
            skin.centered(&words[settings.time_bar], time_y + 17.0, 22, WHITE);
        }

        // health: the player's share grows from the right
        let (bar_x, bar_y, bar_w) = (WIDTH / 2.0 - 300.0, HEIGHT * if settings.downscroll { 0.11 } else { 0.89 }, 600.0);
        // judgements sit left of the player's lanes
        let side_x = if settings.middlescroll { 60.0 } else { WIDTH * 0.35 - 80.0 };
        let player_share = (self.play.health / 2.0) as f32;
        let faded = |color: Color| Color { a: if hud { settings.health_bar_alpha as f32 } else { 0.0 }, ..color };
        draw_rectangle(bar_x - 4.0, bar_y - 4.0, bar_w + 8.0, 26.0, faded(BLACK));
        let singers = self.scene.as_ref().map(|scene| [scene.opponent.as_ref(), scene.player.as_ref()]).unwrap_or([None, None]);
        let middle = bar_x + bar_w * (1.0 - player_share);
        draw_rectangle(bar_x, bar_y, middle - bar_x, 18.0, faded(singers[0].map_or(Color::from_hex(0xFF0000), |c| c.color)));
        draw_rectangle(middle, bar_y, bar_w * player_share, 18.0, faded(singers[1].map_or(Color::from_hex(0x66FF33), |c| c.color)));
        // the two singers' icons meet where the bar's colours meet; the losing one shows its second picture
        let icon_size = 150.0 * self.scene.as_ref().map_or(1.0, |scene| scene.icon_scale);
        for (side, singer) in singers.iter().enumerate() {
            let Some(icon) = singer.and_then(|c| c.icon.as_ref()) else { continue };
            let pictures = (icon.width() / icon.height()).round().max(1.0);
            let losing = if side == 1 { player_share < 0.2 } else { player_share > 0.8 };
            let picture = if losing && pictures > 1.0 { 1.0 } else { 0.0 };
            let source = Rect::new(icon.width() / pictures * picture, 0.0, icon.width() / pictures, icon.height());
            let x = if side == 1 { middle - 26.0 } else { middle + 26.0 - icon_size };
            draw_texture_ex(icon, x, bar_y + 9.0 - icon_size / 2.0, faded(WHITE), DrawTextureParams { dest_size: Some(vec2(icon_size, icon_size)), source: Some(source), flip_x: side == 1, ..Default::default() });
        }

        let rating = self.play.accuracy().map_or("?".to_owned(), |a| format!("{:.2}%", a * 100.0));
        if hud {
            // the score swells a little on every hit and settles within a fifth of a second
            let swell = if settings.score_zoom { 1.0 + 0.075 * (1.0 - (position - self.last_hit) / 200.0).clamp(0.0, 1.0) } else { 1.0 };
            // the same letters drawn larger, not letters of another size: those would have to be made mid-song, which garbles the text for a frame
            let line = format!("Score: {} | Misses: {} | Rating: {}", self.play.score, self.play.misses, rating);
            let x = (WIDTH - measure_text(&line, skin.font.as_ref(), 20, 1.0).width * swell as f32) / 2.0;
            for (shift, color) in [(2.0, BLACK), (0.0, WHITE)] {
                draw_text_ex(&line, x + shift, bar_y + 52.0 + shift, TextParams { font: skin.font.as_ref(), font_size: 20, font_scale: swell as f32, color, ..Default::default() });
            }
        }
        if let Some((text, color, at)) = &self.judgement {
            let age = (position - at) as f32;
            if age < 700.0 {
                let fade = Color { a: (1.0 - (age - 500.0).max(0.0) / 200.0), ..*color };
                skin.text(text, side_x, HEIGHT / 2.0 + 160.0, 24, fade);
            }
        }
        if self.play.botplay {
            skin.centered("BOTPLAY", if settings.downscroll { HEIGHT - 190.0 } else { 190.0 }, 32, WHITE);
        }
        skin.text(&format!("{} [{}]", self.entry.title, self.entry.difficulty), 12.0, if settings.downscroll { 24.0 } else { HEIGHT - 12.0 }, 16, WHITE);

        if let Some(talk) = &self.cutscene {
            talk.draw();
        }
        if let Some(row) = self.paused {
            set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
            draw_rectangle(0.0, 0.0, WIDTH, HEIGHT, Color::new(0.0, 0.0, 0.0, 0.6));
            skin.centered("PAUSED", 220.0, 48, WHITE);
            for (index, label) in ["Resume", "Restart song", "Exit to menu"].iter().enumerate() {
                skin.centered(label, 320.0 + index as f32 * 50.0, 32, if index == row { YELLOW } else { WHITE });
            }
        }
    }
}

/// What the play screen wants next.
enum Flow {
    Continue,
    Finished,
    Restart,
    Quit,
}

enum Screen {
    Menu(Page),
    /// The options, and the menu they were opened from.
    Options(options::OptionsMenu, Page),
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
    let shot_after = value("--after").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let shot_when = value("--when").and_then(|s| s.parse::<f64>().ok()).map(|seconds| seconds * 1000.0);
    let start_at = value("--at").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) * 1000.0;

    // the game's files: where asked, else here, else next to the program
    let base = value("--base").map(PathBuf::from).unwrap_or_else(|| {
        let here = PathBuf::from(".");
        let beside = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        if here.join("assets").is_dir() { here } else { beside.filter(|p| p.join("assets").is_dir()).unwrap_or(here) }
    });

    let mut weeks = find_weeks(&base);
    let mut songs = find_songs(&weeks);
    let mut save = Save::load(&base);
    let mut menus = Menus::new(&base);
    // story mode: the week being played, the songs still to come, and the score and the misses so far
    let mut story: Option<(usize, Vec<usize>, i64, u32)> = None;
    // achievements just unlocked, with the time each was, and whether their chime is still to play
    let mut toasts: Vec<(&'static str, f64)> = Vec::new();
    let mut chime = false;
    let mut greeted = false;
    let mut settings = Settings::load(&base);
    settings.botplay |= flag("--bot");
    let keyboard = Keyboard::start();
    let bindable: Vec<KeyCode> = settings::bindable().into_iter().map(|(key, _)| key).collect();
    let mut frames = 0u32;
    let first = match value("--menu").as_deref() {
        Some("main") => Page::Main,
        Some("story") => Page::Story,
        Some("freeplay") => Page::Freeplay,
        Some("mods") => Page::Mods,
        Some("credits") => Page::Credits,
        Some("achievements") => Page::Achievements,
        _ => Page::Title,
    };
    let pixel_skin = Skin::load_pixel(&base).map_err(|error| eprintln!("No pixel note skin: {error}")).ok();
    let (skin, mut screen) = match Skin::load(&base) {
        Ok(skin) => (Some(skin), Screen::Menu(first)),
        Err(error) => (None, Screen::Failed(format!("Could not load the note skin.\n{error}\nRun the game from the folder that holds \"assets\", or pass --base <folder>."))),
    };
    if flag("--options") && matches!(screen, Screen::Menu(_)) {
        screen = Screen::Options(options::OptionsMenu::at(&value("--options").unwrap_or_default()), Page::Main);
    }
    if let (Some(wanted), Screen::Menu(_)) = (value("--play"), &screen) {
        // compared as file names are, so that spaces, hyphens and capitals do not matter
        let wanted = funkin_core::content::song_path(&wanted);
        screen = match songs.iter().find(|s| funkin_core::content::song_path(&format!("{} {}", s.title, s.difficulty)).contains(&wanted)) {
            Some(entry) => Playing::open(&base, entry, &settings, start_at, false).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p))),
            None => Screen::Failed(format!("No song matches \"{wanted}\".")),
        };
    }

    if let (Some(wanted), Screen::Menu(_)) = (value("--story"), &screen) {
        let list = weeks.iter().position(|week| week.file == wanted).and_then(|index| menu::playlist(index, &weeks[index], &songs, "Normal").map(|list| (index, list)));
        screen = match list {
            Some((week, mut list)) => {
                let entry = &songs[list.remove(0)];
                story = Some((week, list, 0, 0));
                Playing::open(&base, entry, &settings, start_at, true).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p)))
            }
            None => Screen::Failed(format!("No week \"{wanted}\" with a Normal difficulty.")),
        };
    }

    loop {
        // the game is laid out for 1280x720 and stretched to the window
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        clear_background(Color::from_hex(0x1B1B26));
        // read every frame, also outside songs, so that a song never starts with old key presses queued
        let key_events = keyboard.poll();
        let plain = |text: &str, x: f32, y: f32, size: u16, color: Color| match &skin {
            Some(skin) => skin.text(text, x, y, size, color),
            None => drop(draw_text(text, x, y, size as f32, color)),
        };

        screen = match screen {
            Screen::Menu(page) => match skin.as_ref().map(|skin| menus.run(page, skin, &weeks, &songs, &save, settings.flashing)) {
                Some(Go::Stay) => Screen::Menu(page),
                Some(Go::Page(page)) => Screen::Menu(page),
                Some(Go::Options) => Screen::Options(Default::default(), Page::Main),
                Some(Go::Changers) => Screen::Options(options::OptionsMenu::at("Gameplay Changers"), page),
                Some(Go::Play(mut list, week)) => {
                    menus.quiet();
                    let entry = &songs[list.remove(0)];
                    story = week.map(|week| (week, list, 0, 0));
                    Playing::open(&base, entry, &settings, 0.0, story.is_some()).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p)))
                }
                Some(Go::Quit) | None => break,
            },
            Screen::Options(mut menu, back) => {
                if menu.run(&mut settings, &plain, keyboard.raw, &bindable) {
                    settings.save(&base);
                    Screen::Menu(back)
                } else {
                    Screen::Options(menu, back)
                }
            }
            Screen::Playing(mut playing) => {
                // a song is only ever opened with the skin loaded
                let flow = match &skin {
                    Some(skin) => {
                        let skin = pixel_skin.as_ref().filter(|_| playing.pixel()).unwrap_or(skin);
                        let flow = playing.update(&key_events, skin);
                        playing.draw(skin);
                        flow
                    }
                    None => Flow::Quit,
                };
                // ten seconds of singing without a rest
                if playing.sang_for >= 10.0 && playing.settings.keeps_scores() && save.unlock("oversinging") {
                    save.save(&base);
                    toasts.push(("oversinging", get_time()));
                    chime = true;
                }
                match flow {
                    Flow::Continue => Screen::Playing(playing),
                    Flow::Finished => {
                        let (play, entry) = (&playing.play, &playing.entry);
                        // a run the game played itself is not the player's score
                        if playing.settings.keeps_scores() {
                            save.record_song(&entry.title, &entry.difficulty, play.score, play.accuracy().unwrap_or(0.0));
                        }
                        let flawless_week = match &story {
                            Some((week, rest, _, misses)) if rest.is_empty() && misses + play.misses == 0 && entry.difficulty.eq_ignore_ascii_case("hard") => Some(weeks[*week].file.as_str()),
                            _ => None,
                        };
                        let run = Run { song: &entry.title, accuracy: play.accuracy().unwrap_or(0.0), practice: playing.settings.practice, botplay: play.botplay, idled: playing.idled,
                            lanes_used: playing.lanes_used.iter().filter(|used| **used).count(), flawless_week };
                        for id in earned(&run) {
                            if save.unlock(id) {
                                toasts.push((id, get_time()));
                                chime = true;
                            }
                        }
                        match story.take() {
                            // story mode goes straight on to the next song of the week
                            Some((week, mut rest, score, misses)) if !rest.is_empty() => {
                                save.save(&base);
                                let next = &songs[rest.remove(0)];
                                story = Some((week, rest, score + play.score, misses + play.misses));
                                Playing::open(&base, next, &settings, start_at, true).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p)))
                            }
                            Some((week, _, score, _)) => {
                                if playing.settings.keeps_scores() {
                                    save.record_week(&weeks[week].file, &entry.difficulty, score + play.score);
                                }
                                save.save(&base);
                                Screen::Menu(Page::Story)
                            }
                            None => {
                                save.save(&base);
                                let survived = play.health > 0.0;
                                Screen::Results(playing, survived)
                            }
                        }
                    }
                    // a retry goes straight to the song: the talk was heard
                    Flow::Restart => Playing::open(&base, &playing.entry, &settings, 0.0, false).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p))),
                    Flow::Quit => Screen::Menu(if story.take().is_some() { Page::Story } else { Page::Freeplay }),
                }
            }
            Screen::Results(playing, survived) => {
                let play = &playing.play;
                plain(if survived { "Song cleared" } else { "Game over" }, 40.0, 120.0, 48, if survived { GREEN } else { RED });
                plain(&format!("{} [{}]", playing.entry.title, playing.entry.difficulty), 40.0, 170.0, 28, WHITE);
                let accuracy = play.accuracy().map_or("-".to_owned(), |a| format!("{:.2}%", a * 100.0));
                plain(&format!("Score {}    Misses {}    Accuracy {}", play.score, play.misses, accuracy), 40.0, 220.0, 28, WHITE);
                plain("Enter: back to freeplay", 40.0, 280.0, 20, GRAY);
                if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Escape) { Screen::Menu(Page::Freeplay) } else { Screen::Results(playing, survived) }
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

        // the main menu greets whoever plays on a Friday night
        if matches!(screen, Screen::Menu(Page::Main)) && !std::mem::replace(&mut greeted, true) {
            if let Some(id) = menus.greeting().filter(|id| save.unlock(id)) {
                save.save(&base);
                toasts.push((id, get_time()));
                chime = true;
            }
        }
        if std::mem::take(&mut chime) {
            match &screen {
                Screen::Playing(playing) | Screen::Results(playing, _) => playing.sound("confirmMenu", 0.5, false),
                _ => menus.chime = true,
            }
        }
        toasts.retain(|(_, since)| get_time() - since < 4.0);
        if let (Some(skin), false) = (&skin, toasts.is_empty()) {
            set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
            for (slot, (id, since)) in toasts.iter().enumerate() {
                menus.popup(skin, id, (get_time() - since) as f32, slot);
            }
        }
        if settings.show_fps {
            set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
            plain(&format!("FPS: {}", get_fps()), 10.0, 17.0, 14, WHITE);
        }
        if std::mem::take(&mut menus.reload) {
            weeks = find_weeks(&base);
            songs = find_songs(&weeks);
        }

        frames += 1;
        let reached = match (&screen, shot_when) {
            (Screen::Playing(playing), Some(when)) => playing.position >= when,
            _ => true,
        };
        if let (Some(path), true) = (&shot, frames >= 5 && get_time() >= shot_after && reached) {
            // the frame is only drawn for real when it ends; push it out now to read it back
            unsafe { get_internal_gl() }.flush();
            get_screen_data().export_png(path);
            break;
        }
        next_frame().await;
    }
}
