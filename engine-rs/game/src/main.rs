//! Friday Night Funkin' engine in Rust: song list, play screen, results.
//!
//! Run it from a folder that has the game's `assets` (and optionally `mods`), or pass `--base <folder>`.
//! `--bot` lets the game play itself, `--play <text>` starts the first song whose name contains the text.
//! For checking a build without a screen: `--at <seconds>` starts at that point of the song, `--options`
//! opens the options, and `--shot <file.png>` saves the picture after a few frames (or `--after <seconds>`) and quits.
mod atlas;
mod audio;
mod input;
mod scene;
mod settings;

use funkin_core::content::{discover, Paths, SongEntry};
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
        let images = base.join("assets/shared/images");
        // missing pictures only cost their effect
        let pictures = |names: &[&str]| -> Vec<Texture2D> {
            let found: Vec<Texture2D> = names.iter().filter_map(|name| std::fs::read(images.join(format!("{name}.png"))).ok()).map(|bytes| Texture2D::from_file_with_format(&bytes, None)).collect();
            if found.len() == names.len() { found } else { Vec::new() }
        };
        let digits = pictures(&["num0", "num1", "num2", "num3", "num4", "num5", "num6", "num7", "num8", "num9"]);
        let splashes = atlas::Sheet::load(&images.join("noteSplashes/noteSplashes.png")).ok();
        Ok(Skin { sheet, frames, lanes, font, ratings: pictures(&["sick", "good", "bad", "shit"]), digits, countdown: pictures(&["ready", "set", "go"]), splashes })
    }

    /// Draws a sheet frame centred on (x, y), stretched to `size` if given. `lane` recolours it.
    fn frame(&self, name: &str, x: f32, y: f32, size: Option<Vec2>, lane: Option<usize>, alpha: f32) {
        let Some(source) = self.frames.iter().find(|(n, _)| n == name).map(|(_, f)| f.source) else { return };
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
}

impl Playing {
    fn open(base: &Path, entry: &SongEntry, settings: &Settings, start_at: f64) -> Result<Playing, String> {
        let text = std::fs::read_to_string(&entry.chart).map_err(|e| format!("{}: {e}", entry.chart.display()))?;
        let chart = funkin_core::chart::parse(&text).map_err(|e| format!("{}: {e}", entry.chart.display()))?;
        let mut tracks = vec![entry.inst.as_path()];
        tracks.extend(entry.voices_player.as_deref());
        let player_voice = entry.voices_player.as_ref().map(|_| 1);
        tracks.extend(entry.voices_opponent.as_deref());
        let scene = scene::Scene::load(base, &entry.source, &chart).map_err(|e| eprintln!("Stage: {e}")).ok();
        // shortcut: --at only moves the silent clock (it exists for screenshots); seek the audio too to make it a practice feature.
        let song = audio::Song::load(if start_at > 0.0 { &[] } else { &tracks });
        // five beats of countdown, as the Haxe engine does
        let lead_in = if start_at > 0.0 { 0.0 } else { 5.0 * 60000.0 / chart.bpm };
        let mut play = Play::new(chart, settings.botplay);
        play.ghost_tapping = settings.ghost_tapping;
        play.skip_to(start_at);
        Ok(Playing { entry: entry.clone(), play, song, player_voice, scene, waited: 0.0, started: false, paused: None, start_at, position: -lead_in, settings: settings.clone(), held_keys: Vec::new(), flash_until: [[f64::MIN; 4]; 2], judgement: None,
            paths: Paths::new(base, &entry.source, ""), lead_in, counted: i64::MIN, popups: Vec::new(), splashes: Vec::new(), dead: None })
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
            self.sound("gameOver", 1.0, true);
        }
        if confirm {
            self.song.stop_sounds();
            self.sound("gameOverEnd", 1.0, false);
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
        if let Some(row) = self.paused {
            let row = (row + is_key_pressed(KeyCode::Down) as usize + 2 * is_key_pressed(KeyCode::Up) as usize) % 3;
            self.paused = Some(row);
            let chosen = if is_key_pressed(KeyCode::Escape) { Some(0) } else { is_key_pressed(KeyCode::Enter).then_some(row) };
            return match chosen {
                Some(0) => {
                    self.paused = None;
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
            return Flow::Continue;
        }

        // a frame that took long (loading) does not eat the countdown; the Haxe engine caps frames the same way
        let dt = get_frame_time().min(0.1);
        let lead_in = self.lead_in;
        self.waited += dt as f64 * 1000.0;
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
            self.sound(["intro3", "intro2", "intro1", "introGo"][(beat + 4) as usize], 0.6, false);
            if let Some(picture) = skin.countdown.get((beat + 3) as usize) {
                let at = (vec2(WIDTH, HEIGHT) - picture.size()) / 2.0;
                self.popups.push(Popup { texture: picture.clone(), at, scale: 1.0, motion: (0.0, 0.0), born: self.position, stays: crochet * 0.6 });
            }
        }

        let judged = self.judged();
        let mut events = Vec::new();
        for key in keys {
            let Some(lane) = self.settings.lane_of(key.key) else { continue };
            if key.pressed && !self.held_keys.contains(&key.key) {
                self.held_keys.push(key.key);
                if !self.play.botplay {
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
                Event::Hit { lane, rating, diff_ms } => {
                    self.flash_until[1][lane] = judged + 120.0;
                    let color = if diff_ms < 0.0 { Color::from_hex(0x7FC8FF) } else { Color::from_hex(0xFF9F7F) };
                    self.judgement = Some((format!("{:+.1} ms", diff_ms), color, judged));
                    self.player_voice.iter().for_each(|&track| self.song.set_volume(track, 1.0));
                    if rating == Rating::Sick {
                        self.splashes.push((lane, judged));
                    }
                    // the rating jumps up left of the player's lanes, the combo count under it
                    let left = if self.settings.middlescroll { 60.0 } else { WIDTH * 0.35 - 40.0 };
                    if let Some(picture) = skin.ratings.get(rating as usize) {
                        let at = vec2(left, (HEIGHT - picture.height() * 0.7) / 2.0 - 60.0);
                        self.popups.push(Popup { texture: picture.clone(), at, scale: 0.7, motion: (160.0, 550.0), born: judged, stays: crochet });
                    }
                    if self.play.combo >= 10 && !skin.digits.is_empty() {
                        for (place, digit) in format!("{:03}", self.play.combo).bytes().enumerate() {
                            let picture = &skin.digits[(digit - b'0') as usize];
                            let at = vec2(left - 50.0 + 43.0 * place as f32, (HEIGHT - picture.height() * 0.5) / 2.0 + 80.0);
                            self.popups.push(Popup { texture: picture.clone(), at, scale: 0.5, motion: (150.0, 250.0), born: judged, stays: crochet * 2.0 });
                        }
                    }
                }
                Event::Miss { .. } => {
                    self.judgement = Some(("Miss".to_owned(), Color::from_hex(0xFF4040), judged));
                    self.player_voice.iter().for_each(|&track| self.song.set_volume(track, 0.0));
                    self.sound(["missnote1", "missnote2", "missnote3"][self.play.misses as usize % 3], 0.15, false);
                }
                Event::OpponentHit { lane } => self.flash_until[0][lane] = judged + 120.0,
            }
        }

        if let Some(scene) = &mut self.scene {
            let mut held = [[false; 4]; 2];
            for (note, state) in self.play.chart.notes.iter().zip(&self.play.state).take_while(|(note, _)| note.time <= judged) {
                held[note.player as usize][note.lane] |= *state == NoteState::Held;
            }
            scene.update(dt, judged, &self.play.chart, &events, held);
        }

        self.splashes.retain(|&(_, at)| judged - at < 300.0);
        self.popups.retain(|popup| self.position.max(judged) - popup.born < popup.stays + 200.0);

        if self.play.health <= 0.0 {
            self.dead = Some((0.0, None));
            self.song.pause();
            self.sound("fnf_loss_sfx", 1.0, false);
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
        let hud_zoom = self.scene.as_ref().map_or(1.0, |scene| {
            scene.draw();
            scene.hud_zoom
        });
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT) * hud_zoom, ..Default::default() });
        if self.dead.is_some() {
            skin.centered("Enter: retry    Esc: back to the song list", HEIGHT - 30.0, 18, GRAY);
            return;
        }
        let speed = self.play.chart.speed as f32;
        let strum_y = strum_y(settings);
        // notes come up from below, or with downscroll down from above
        let direction = if settings.downscroll { -1.0 } else { 1.0 };
        let y_at = |time: f64| strum_y + direction * (time - position) as f32 * PIXELS_PER_MS * speed;
        // with middlescroll the opponent still plays, but is not shown
        let shown = |player: bool| player || !settings.middlescroll;

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
                        let cap = (64.0 * NOTE_SCALE).min(bottom - top);
                        let cap_y = tail - direction * cap / 2.0;
                        let size = vec2(50.0 * NOTE_SCALE, bottom - top - cap);
                        skin.frame(&format!("{} hold piece0000", NOTE_NAMES[note.lane]), x, (head + tail - direction * cap) / 2.0, Some(size), Some(note.lane), 0.6 * alpha);
                        skin.frame(&format!("{} hold end0000", NOTE_NAMES[note.lane]), x, cap_y, Some(vec2(50.0 * NOTE_SCALE, cap * direction)), Some(note.lane), 0.6 * alpha);
                    }
                }
                if pass == 1 && matches!(state, NoteState::Pending | NoteState::Missed) && on_screen(y_at(note.time)) {
                    let tint = if note.hurt { None } else { Some(note.lane) }; // hurt notes show as raw masks
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
                    sheet.draw(frame, lane_x(settings, true, lane) - frame.size.x / 2.0, strum_y - frame.size.y / 2.0, 1.0, false, Color::new(1.0, 1.0, 1.0, 0.6));
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
        let left_s = ((length - self.position.clamp(0.0, length)) / 1000.0) as u32;
        draw_rectangle(WIDTH / 2.0 - 204.0, time_y - 4.0, 408.0, 27.0, BLACK);
        draw_rectangle(WIDTH / 2.0 - 200.0, time_y, 400.0 * (self.position / length).clamp(0.0, 1.0) as f32, 19.0, WHITE);
        skin.centered(&format!("{}:{:02}", left_s / 60, left_s % 60), time_y + 17.0, 22, WHITE);

        // health: the player's share grows from the right
        let (bar_x, bar_y, bar_w) = (WIDTH / 2.0 - 300.0, HEIGHT * if settings.downscroll { 0.11 } else { 0.89 }, 600.0);
        // judgements sit left of the player's lanes
        let side_x = if settings.middlescroll { 60.0 } else { WIDTH * 0.35 - 80.0 };
        let player_share = (self.play.health / 2.0) as f32;
        draw_rectangle(bar_x - 4.0, bar_y - 4.0, bar_w + 8.0, 26.0, BLACK);
        let singers = self.scene.as_ref().map(|scene| [scene.opponent.as_ref(), scene.player.as_ref()]).unwrap_or([None, None]);
        let middle = bar_x + bar_w * (1.0 - player_share);
        draw_rectangle(bar_x, bar_y, middle - bar_x, 18.0, singers[0].map_or(Color::from_hex(0xFF0000), |c| c.color));
        draw_rectangle(middle, bar_y, bar_w * player_share, 18.0, singers[1].map_or(Color::from_hex(0x66FF33), |c| c.color));
        // the two singers' icons meet where the bar's colours meet; the losing one shows its second picture
        let icon_size = 150.0 * self.scene.as_ref().map_or(1.0, |scene| scene.icon_scale);
        for (side, singer) in singers.iter().enumerate() {
            let Some(icon) = singer.and_then(|c| c.icon.as_ref()) else { continue };
            let pictures = (icon.width() / icon.height()).round().max(1.0);
            let losing = if side == 1 { player_share < 0.2 } else { player_share > 0.8 };
            let picture = if losing && pictures > 1.0 { 1.0 } else { 0.0 };
            let source = Rect::new(icon.width() / pictures * picture, 0.0, icon.width() / pictures, icon.height());
            let x = if side == 1 { middle - 26.0 } else { middle + 26.0 - icon_size };
            draw_texture_ex(icon, x, bar_y + 9.0 - icon_size / 2.0, WHITE, DrawTextureParams { dest_size: Some(vec2(icon_size, icon_size)), source: Some(source), flip_x: side == 1, ..Default::default() });
        }

        let rating = self.play.accuracy().map_or("?".to_owned(), |a| format!("{:.2}%", a * 100.0));
        skin.centered(&format!("Score: {} | Misses: {} | Rating: {}", self.play.score, self.play.misses, rating), bar_y + 52.0, 20, WHITE);
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

        if let Some(row) = self.paused {
            set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
            draw_rectangle(0.0, 0.0, WIDTH, HEIGHT, Color::new(0.0, 0.0, 0.0, 0.6));
            skin.centered("PAUSED", 220.0, 48, WHITE);
            for (index, label) in ["Resume", "Restart song", "Exit to the song list"].iter().enumerate() {
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
    Menu,
    /// The options: the selected row, and whether it waits for a key to bind.
    Options(usize, bool),
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
    let start_at = value("--at").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) * 1000.0;

    // the game's files: where asked, else here, else next to the program
    let base = value("--base").map(PathBuf::from).unwrap_or_else(|| {
        let here = PathBuf::from(".");
        let beside = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        if here.join("assets").is_dir() { here } else { beside.filter(|p| p.join("assets").is_dir()).unwrap_or(here) }
    });

    let songs = discover(&base);
    let mut settings = Settings::load(&base);
    settings.botplay |= flag("--bot");
    let keyboard = Keyboard::start();
    let bindable: Vec<KeyCode> = settings::bindable().into_iter().map(|(key, _)| key).collect();
    let mut selected = 0usize;
    let mut frames = 0u32;
    let (skin, mut screen) = match Skin::load(&base) {
        Ok(skin) => (Some(skin), Screen::Menu),
        Err(error) => (None, Screen::Failed(format!("Could not load the note skin.\n{error}\nRun the game from the folder that holds \"assets\", or pass --base <folder>."))),
    };
    if flag("--options") && matches!(screen, Screen::Menu) {
        screen = Screen::Options(0, false);
    }
    if let (Some(wanted), Screen::Menu) = (value("--play"), &screen) {
        let wanted = wanted.to_lowercase();
        screen = match songs.iter().find(|s| format!("{} {}", s.title, s.difficulty).to_lowercase().contains(&wanted)) {
            Some(entry) => Playing::open(&base, entry, &settings, start_at).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p))),
            None => Screen::Failed(format!("No song matches \"{wanted}\".")),
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
            Screen::Menu => {
                plain("Friday Night Funkin' - Rust engine", 40.0, 60.0, 36, WHITE);
                plain(&format!("Up/Down: choose   Enter: play   O: options   B: botplay {}   Esc: quit", if settings.botplay { "ON" } else { "off" }), 40.0, 96.0, 18, GRAY);
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
                if is_key_pressed(KeyCode::B) {
                    settings.botplay = !settings.botplay;
                    settings.save(&base);
                }
                if is_key_pressed(KeyCode::Escape) { break; }
                match songs.get(selected) {
                    Some(entry) if is_key_pressed(KeyCode::Enter) => Playing::open(&base, entry, &settings, 0.0).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p))),
                    _ if is_key_pressed(KeyCode::O) => Screen::Options(0, false),
                    _ => Screen::Menu,
                }
            }
            Screen::Options(mut row, mut binding) => {
                const SWITCHES: [&str; 4] = ["Downscroll", "Middlescroll", "Ghost tapping", "Botplay"];
                const ROWS: usize = 13; // 4 switches, the offset, 2 keys for each of 4 lanes
                plain("Options", 40.0, 60.0, 36, WHITE);
                plain("Up/Down: choose   Enter: change   Left/Right: offset (Shift: x10)   Backspace: unbind   Esc: save and back", 40.0, 96.0, 18, GRAY);
                let key_slot = |row: usize| ((row - 5) / 2, (row - 5) % 2);
                for index in 0..ROWS {
                    let text = match index {
                        0..=3 => {
                            let on = [settings.downscroll, settings.middlescroll, settings.ghost_tapping, settings.botplay][index];
                            format!("{}: {}", SWITCHES[index], if on { "ON" } else { "off" })
                        }
                        4 => format!("Note offset: {:+.0} ms", settings.offset_ms),
                        _ => {
                            let (lane, slot) = key_slot(index);
                            let key = if binding && index == row { "press a key...".to_owned() } else { settings.keys[lane][slot].map_or("-".to_owned(), |k| format!("{k:?}")) };
                            format!("{} key {}: {key}", DIRECTIONS[lane], slot + 1)
                        }
                    };
                    let y = 150.0 + index as f32 * 30.0 + if index > 4 { 20.0 } else { 0.0 } + if index > 3 { 10.0 } else { 0.0 };
                    plain(&format!("{}{text}", if index == row { "> " } else { "  " }), 40.0, y, 24, if index == row { YELLOW } else { WHITE });
                }
                let timing = if keyboard.raw { "raw input: each press is timed when it happens" } else { "per frame (raw input is not available on this system)" };
                plain(&format!("Keyboard timing: {timing}"), 40.0, HEIGHT - 40.0, 18, GRAY);

                if binding {
                    // every key belongs to the binding now, the arrows included
                    match get_last_key_pressed() {
                        Some(KeyCode::Escape) => binding = false,
                        Some(key) if bindable.contains(&key) => {
                            let (lane, slot) = key_slot(row);
                            settings.bind(lane, slot, Some(key));
                            binding = false;
                        }
                        _ => {}
                    }
                    Screen::Options(row, binding)
                } else {
                    if is_key_pressed(KeyCode::Down) && row + 1 < ROWS { row += 1; }
                    if is_key_pressed(KeyCode::Up) { row = row.saturating_sub(1); }
                    let step = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) { 10.0 } else { 1.0 };
                    if row == 4 {
                        let change = is_key_pressed(KeyCode::Right) as i32 - is_key_pressed(KeyCode::Left) as i32;
                        settings.offset_ms = (settings.offset_ms + change as f64 * step).clamp(-500.0, 500.0);
                    }
                    if is_key_pressed(KeyCode::Enter) {
                        match row {
                            0 => settings.downscroll = !settings.downscroll,
                            1 => settings.middlescroll = !settings.middlescroll,
                            2 => settings.ghost_tapping = !settings.ghost_tapping,
                            3 => settings.botplay = !settings.botplay,
                            4 => {}
                            _ => binding = true,
                        }
                    }
                    if row > 4 && is_key_pressed(KeyCode::Backspace) {
                        let (lane, slot) = key_slot(row);
                        settings.bind(lane, slot, None);
                    }
                    if is_key_pressed(KeyCode::Escape) {
                        settings.save(&base);
                        Screen::Menu
                    } else {
                        Screen::Options(row, binding)
                    }
                }
            }
            Screen::Playing(mut playing) => {
                // a song is only ever opened with the skin loaded
                let flow = match &skin {
                    Some(skin) => {
                        let flow = playing.update(&key_events, skin);
                        playing.draw(skin);
                        flow
                    }
                    None => Flow::Quit,
                };
                match flow {
                    Flow::Continue => Screen::Playing(playing),
                    Flow::Finished => {
                        let survived = playing.play.health > 0.0;
                        Screen::Results(playing, survived)
                    }
                    Flow::Restart => Playing::open(&base, &playing.entry, &settings, 0.0).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p))),
                    Flow::Quit => Screen::Menu,
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
        if let (Some(path), true) = (&shot, frames >= 5 && get_time() >= shot_after) {
            // the frame is only drawn for real when it ends; push it out now to read it back
            unsafe { get_internal_gl() }.flush();
            get_screen_data().export_png(path);
            break;
        }
        next_frame().await;
    }
}
