//! Friday Night Funkin' engine in Rust: song list, play screen, results.
//!
//! Run it from a folder that has the game's `assets` (and optionally `mods`), or pass `--base <folder>`.
//! `--bot` lets the game play itself, `--play <text>` starts the first song whose name contains the text.
//! For checking a build without a screen: `--at <seconds>` starts at that point of the song, `--options`
//! opens the options, and `--shot <file.png>` saves the picture after a few frames (or `--after <seconds>`, or
//! once the song has reached `--when <seconds>`) and quits.
mod adjust;
mod animate;
mod atlas;
mod audio;
mod controls;
mod dialogue;
mod discord;
mod fight;
mod gfx;
mod input;
mod menu;
mod options;
mod props;
mod scene;
mod settings;

use funkin_core::content::{songs as find_songs, weeks as find_weeks, Paths, SongEntry};
use funkin_core::achievements::{earned, Run, ROADKILL};
use funkin_core::save::Save;
use funkin_core::lang::{phrase, phrase_with};
use menu::{Go, Menus, Page};
use funkin_core::chart::{Chart, NoteKind};
use funkin_core::play::{Event, NoteState, Play, Rating};
use input::{KeyEvent, Keyboard};
use controls::Ui;
use gfx::*;
use settings::Settings;
use std::path::{Path, PathBuf};
use std::rc::Rc;

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

struct Skin {
    sheet: Texture2D,
    frames: Vec<(String, atlas::Frame)>,
    /// The three colours each lane's notes are made of (body, shine, outline): the sheet stores each note as three masks.
    lanes: std::cell::Cell<[[Vec3; 3]; 4]>,
    font: Option<Font>,
    /// Pictures for sick, good, bad, shit.
    ratings: Vec<Texture2D>,
    /// Pictures of the digits 0 to 9.
    digits: Vec<Texture2D>,
    /// Pictures for ready, set, go.
    countdown: Vec<Texture2D>,
    splashes: Option<atlas::Sheet>,
    /// The splash animation of each lane, as the splash skin names it.
    splash_names: [String; 4],
    /// How much the note pictures are enlarged, and how wide a hold's body and how long its end cap are drawn.
    note_scale: f32,
    hold: Vec2,
    /// How much the rating, combo digit and countdown pictures are enlarged.
    popup_scales: [f32; 3],
    /// The notes are drawn in the colours they were painted in (a chart asked for that).
    plain: bool,
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
    /// The three colours each lane's notes are made of from now on (body, shine, outline).
    fn recolor(&self, lane_colors: &[[u32; 3]; 4]) {
        let rgb = |c: u32| vec3((c >> 16 & 255) as f32, (c >> 8 & 255) as f32, (c & 255) as f32) / 255.0;
        self.lanes.set(lane_colors.map(|colors| colors.map(rgb)));
    }

    /// What a lane's notes are recoloured with.
    fn colors(&self, lane: usize) -> Option<[Vec3; 3]> {
        (!self.plain).then(|| self.lanes.get()[lane])
    }

    /// The note of a lane as it flies, centred on (x, y).
    /// Draws a frame of a lane's notes centred on `at`, at the size that makes the lane's strum `width` wide.
    fn fitted(&self, name: &str, lane: usize, at: Vec2, width: f32, alpha: f32) {
        let source = |name: &str| self.frames.iter().find(|(n, _)| n == name).map(|(_, f)| f.source);
        if let (Some(strum), Some(frame)) = (source(&format!("arrow{}0000", DIRECTIONS[lane].to_uppercase())), source(name)) {
            self.frame(name, at.x, at.y, Some(vec2(frame.w, frame.h) * width / strum.w), Some(lane), alpha);
        }
    }

    /// The pictures named, from a folder; all of them or none (missing pictures only cost their effect).
    fn pictures(dir: &Path, names: &[&str], pixel: bool) -> Vec<Texture2D> {
        let found: Vec<Texture2D> = names.iter().filter_map(|name| atlas::picture(&dir.join(format!("{name}.png")))).collect();
        if pixel {
            found.iter().for_each(|picture| picture.set_filter(FilterMode::Nearest));
        }
        if found.len() == names.len() { found } else { Vec::new() }
    }

    /// The picture of the splashes: the chart's own (`own`, found through `paths`) or the one chosen in the options.
    fn splash_file(base: &Path, own: Option<(&Paths, &str)>, skin: usize) -> PathBuf {
        // "noteSplashes-diamond": the skin's name after the usual file's, as the Haxe engine names them
        let named = settings::splash_skins().get(skin).filter(|_| skin > 0).map(|name| format!("-{}", name.trim().to_lowercase().replace(' ', "-")));
        let chosen = format!("images/noteSplashes/noteSplashes{}.png", named.unwrap_or_default());
        own.and_then(|(paths, name)| paths.find(&format!("images/{name}.png"))).or_else(|| menu_paths(base).find(&chosen)).unwrap_or(base.join("assets/shared").join(chosen))
    }

    /// `skins`: the note skin and the splash skin chosen in the options (the first of each is the usual one).
    /// `own`: where to look for the pictures a chart names for its notes and its splashes instead.
    fn load(base: &Path, skins: (usize, usize), own: Option<(&Paths, &Chart)>) -> Result<Skin, String> {
        // "NOTE_assets-future": the skin's name after the usual file's
        let named = settings::note_skins().get(skins.0).filter(|_| skins.0 > 0).map(|name| format!("-{}", name.trim().to_lowercase().replace(' ', "_")));
        let chosen = format!("images/noteSkins/NOTE_assets{}.png", named.unwrap_or_default());
        let own_notes = own.filter(|(_, chart)| !chart.arrow_skin.is_empty()).and_then(|(paths, chart)| paths.find(&format!("images/{}.png", chart.arrow_skin)));
        // a skin a mod brings is found in the mods
        let notes = own_notes.or_else(|| menu_paths(base).find(&chosen)).unwrap_or(base.join("assets/shared").join(chosen));
        let sheet = atlas::picture(&notes).ok_or(format!("{}: not found", notes.display()))?;
        let frames = atlas::parse(&std::fs::read_to_string(notes.with_extension("xml")).map_err(|e| format!("{}: {e}", notes.with_extension("xml").display()))?);
        let font = std::fs::read(base.join("assets/fonts/vcr.ttf")).ok().and_then(|bytes| load_ttf_font_from_bytes(&bytes).ok());
        let images = base.join("assets/shared/images");
        let digits = Skin::pictures(&images, &["num0", "num1", "num2", "num3", "num4", "num5", "num6", "num7", "num8", "num9"], false);
        let splash = Skin::splash_file(base, own.filter(|(_, chart)| !chart.splash_skin.is_empty()).map(|(paths, chart)| (paths, chart.splash_skin.as_str())), skins.1);
        let splashes = atlas::Sheet::load(&splash).ok();
        // the skin's own file says what each lane's animation is called
        let named: serde_json::Value = std::fs::read_to_string(splash.with_extension("json")).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
        let splash_names = [0, 1, 2, 3].map(|lane| {
            let own = named["animations"].as_object().into_iter().flatten().find(|(_, animation)| animation["noteData"] == lane).and_then(|(_, animation)| animation["prefix"].as_str());
            own.map_or(format!("note splash {} 1", NOTE_NAMES[lane]), str::to_owned)
        });
        Ok(Skin { sheet, frames, lanes: Default::default(), font, ratings: Skin::pictures(&images, &["sick", "good", "bad", "shit"], false), digits, countdown: Skin::pictures(&images, &["ready", "set", "go"], false), splashes, splash_names,
            note_scale: NOTE_SCALE, hold: vec2(50.0, 64.0) * NOTE_SCALE, popup_scales: [0.7, 0.5, 1.0], plain: own.is_some_and(|(_, chart)| chart.no_rgb) })
    }

    /// The look of pixel stages: notes from a grid of small pictures drawn six times as large, with their own colours, ratings and countdown.
    /// `own`: as for `load`; a chart's notes for pixel stages sit under `images/pixelUI/`, the hold pieces in a second picture.
    fn load_pixel(base: &Path, own: Option<(&Paths, &Chart)>) -> Result<Skin, String> {
        let dir = base.join("assets/shared/images/pixelUI");
        let image = |path: PathBuf| -> Result<Image, String> {
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            Image::from_file_with_format(&bytes, None).map_err(|e| format!("{}: {e}", path.display()))
        };
        let own_notes = own.filter(|(_, chart)| !chart.arrow_skin.is_empty()).and_then(|(paths, chart)| {
            Some((paths.find(&format!("images/pixelUI/{}.png", chart.arrow_skin))?, paths.find(&format!("images/pixelUI/{}ENDS.png", chart.arrow_skin))?))
        });
        let (notes, ends) = own_notes.unwrap_or((dir.join("noteSkins/NOTE_assets.png"), dir.join("noteSkins/NOTE_assetsENDS.png")));
        let (notes, ends) = (image(notes)?, image(ends)?);
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
        let splashes = atlas::Sheet::load(&Skin::splash_file(base, own.filter(|(_, chart)| !chart.splash_skin.is_empty()).map(|(paths, chart)| (paths, chart.splash_skin.as_str())), 0)).ok();
        Ok(Skin { sheet, frames, lanes: Default::default(), font, ratings: Skin::pictures(&dir, &["sick-pixel", "good-pixel", "bad-pixel", "shit-pixel"], true), digits,
            countdown: Skin::pictures(&dir, &["ready-pixel", "set-pixel", "date-pixel"], true), splashes, splash_names: NOTE_NAMES.map(|name| format!("note splash {name} 1")), plain: own.is_some_and(|(_, chart)| chart.no_rgb), note_scale: PIXEL_ZOOM, hold: end * PIXEL_ZOOM, popup_scales: [PIXEL_ZOOM * 0.85, PIXEL_ZOOM, PIXEL_ZOOM] })
    }

    /// Draws a sheet frame centred on (x, y), stretched to `size` if given. `lane` recolours it.
    fn frame(&self, name: &str, x: f32, y: f32, size: Option<Vec2>, lane: Option<usize>, alpha: f32) {
        let Some(source) = self.frames.iter().find(|(n, _)| n == name).map(|(_, f)| f.source) else { return };
        let size = size.unwrap_or(vec2(source.w, source.h) * self.note_scale);
        recolor(lane.and_then(|lane| self.colors(lane)));
        draw_texture_ex(&self.sheet, x - size.x / 2.0, y - size.y / 2.0, Color::new(1.0, 1.0, 1.0, alpha), DrawTextureParams { dest_size: Some(size), source: Some(source), ..Default::default() });
        recolor(None);
    }

    fn text(&self, text: &str, x: f32, y: f32, size: u16, color: Color) {
        // a dark copy underneath keeps it readable over notes
        for (dx, dy, color) in [(2.0, 2.0, Color { a: color.a, ..BLACK }), (0.0, 0.0, color)] {
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
    /// The look of the notes when the chart brings its own pictures for them or wants them left unpainted.
    own_skin: Option<Rc<Skin>>,
    base: PathBuf,
    /// The stage plays a scene of its own before the countdown (story mode).
    staged: bool,
    /// The song is over and the stage plays a scene of its own before the next.
    bowing: bool,
    /// The stage's music is playing.
    tune: bool,
    /// Stage and characters; None when they could not be loaded (the notes still work).
    scene: Option<scene::Scene>,
    /// Ms spent on the play screen before the song starts.
    waited: f64,
    /// The song position evened out, and when it was last asked.
    clock: funkin_core::clock::Clock,
    ticked: std::time::Instant,
    started: bool,
    /// The pause menu's selected row, while paused; and the row in its list of difficulties, while that is open.
    paused: Option<usize>,
    /// When the song was paused (program time), and where the pause menu's list has scrolled to.
    paused_at: f64,
    pause_scroll: f32,
    /// Game overs on this song since it was chosen in the menus (the pause menu counts them).
    deaths: u32,
    choosing: Option<usize>,
    /// The difficulties the song can be switched to from the pause menu (none in story mode).
    difficulties: Vec<String>,
    /// Arrows fade in one after the other when the song opens (not in story mode, as in the Haxe engine).
    story: bool,
    /// Week 7's game over: when the taunt that plays over the music ends (seconds since the loss).
    taunt_until: Option<f32>,
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
    /// `seen`: the talk and the stage's own opening scene were seen already (a retry): they do not come again.
    fn open(base: &Path, entry: &SongEntry, songs: &[SongEntry], settings: &Settings, start_at: f64, story: bool, seen: bool) -> Result<Playing, String> {
        let text = std::fs::read_to_string(&entry.chart).map_err(|e| format!("{}: {e}", entry.chart.display()))?;
        let mut chart = funkin_core::chart::parse(&text).map_err(|e| format!("{}: {e}", entry.chart.display()))?;
        // events shared by all difficulties of the song sit in their own file
        if let Some(shared) = std::fs::read_to_string(entry.chart.with_file_name("events.json")).ok().and_then(|text| funkin_core::chart::parse(&text).ok()) {
            chart.events.extend(shared.events);
            chart.events.sort_by(|a, b| a.time.total_cmp(&b.time));
        }
        // a character may name its own voice file ("Voices-<name>.ogg" beside the instrumental)
        let named = |character: &str, usual: &Option<PathBuf>| {
            let file = Paths::new(base, &entry.source, "").find(&format!("characters/{character}.json")).and_then(|file| std::fs::read_to_string(file).ok());
            let own = file.and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok()).and_then(|json| Some(entry.inst.with_file_name(format!("Voices-{}.ogg", json["vocals_file"].as_str().filter(|name| !name.is_empty())?))));
            own.filter(|voice| voice.is_file()).or(usual.clone())
        };
        // (a chart may say it has none)
        let (voices_player, voices_opponent) = if chart.needs_voices { (named(&chart.player, &entry.voices_player), named(&chart.opponent, &entry.voices_opponent)) } else { (None, None) };
        let mut tracks = vec![entry.inst.as_path()];
        tracks.extend(voices_player.as_deref());
        let player_voice = voices_player.as_ref().map(|_| 1);
        tracks.extend(voices_opponent.as_deref());
        let scene = scene::Scene::load(base, &entry.source, &chart, settings.low_quality, settings.shaders, story).map_err(|e| eprintln!("Stage: {e}")).ok();
        // shortcut: --at only moves the silent clock (it exists for screenshots); seek the audio too to make it a practice feature.
        let song = audio::Song::load_at(if start_at > 0.0 { &[] } else { &tracks }, settings.playback_rate);
        // five beats of countdown, as the Haxe engine does
        let lead_in = if start_at > 0.0 { 0.0 } else { 5.0 * 60000.0 / chart.bpm };
        let speed = settings.note_speed(chart.speed) as f32;
        // sounds may sit in the stage's own folder
        let directory = scene.as_ref().map_or(String::new(), |scene| scene.directory.clone());
        let paths = Paths::new(base, &entry.source, &directory);
        let cutscene = if story && start_at <= 0.0 && !seen { dialogue::Dialogue::load(base, &paths, &funkin_core::content::song_path(&entry.title), &chart.stage, &settings.language) } else { None };
        // --bot plays every song by itself without becoming the saved setting
        let settings = &Settings { botplay: settings.botplay || std::env::args().any(|arg| arg == "--bot"), ..settings.clone() };
        let mut play = Play::new(chart, settings.botplay);
        play.skip_to(start_at);
        // shortcut: in story mode the week goes on at the difficulty it began with; let the rest of the week change with it if that is missed
        let difficulties = if story { Vec::new() } else { songs.iter().filter(|other| other.week == entry.week && other.title == entry.title).map(|other| other.difficulty.clone()).collect() };
        atlas::forget_unused();
        let mut scene = scene;
        let staged = story && start_at <= 0.0 && !seen && scene.as_mut().is_some_and(|scene| scene.open_with(&funkin_core::content::song_path(&entry.title), 60.0 / play.chart.bpm as f32));
        Ok(Playing { entry: entry.clone(), play, song, player_voice, own_skin: None, base: base.to_path_buf(), staged, bowing: false, tune: false, paused_at: 0.0, pause_scroll: 0.0, deaths: 0, scene, waited: 0.0, clock: Default::default(), ticked: std::time::Instant::now(), started: false, paused: None, choosing: None, difficulties, story, taunt_until: None, start_at, position: -lead_in, settings: settings.clone(), held_keys: Vec::new(), flash_until: [[f64::MIN; 4]; 2], judgement: None, last_hit: f64::MIN, lanes_used: [false; 4], idled: false, idle_for: 0.0, sang_for: 0.0,
            paths, cutscene, lead_in, counted: i64::MIN, popups: Vec::new(), splashes: Vec::new(), dead: None, next_event: 0, speed, speed_change: None }.with(settings))
    }

    /// Takes the player's settings, at the start and when the options were changed from the pause menu.
    fn with(mut self, settings: &Settings) -> Playing {
        // what the song was opened with stays: the gameplay changers (who plays it, how fast, whether it counts), as in the Haxe
        // engine, whose pause menu cannot reach them
        let kept = &self.settings;
        let settings = &Settings {
            botplay: kept.botplay, playback_rate: kept.playback_rate, practice: kept.practice, instakill: kept.instakill, health_gain: kept.health_gain,
            health_loss: kept.health_loss, scroll_type: kept.scroll_type, scroll_speed: kept.scroll_speed, ..settings.clone()
        };
        self.forget_released();
        let play = &mut self.play;
        (play.ghost_tapping, play.timing, play.one_note_holds) = (settings.ghost_tapping, settings.timing(), settings.one_note_holds);
        (play.health_gain, play.health_loss, play.instakill) = (settings.health_gain, settings.health_loss, settings.instakill);
        if let Some(scene) = &mut self.scene {
            (scene.cam_zooms, scene.flashing) = (settings.cam_zooms, settings.flashing);
        }
        let chart = &self.play.chart;
        if !chart.arrow_skin.is_empty() || !chart.splash_skin.is_empty() || chart.no_rgb {
            let (base, own, skins) = (&self.base, Some((&self.paths, chart)), (settings.note_skin, settings.splash_skin));
            let skin = if self.pixel() { Skin::load_pixel(base, own) } else { Skin::load(base, skins, own) };
            self.own_skin = skin.map_err(|error| eprintln!("The chart's note skin: {error}")).ok().map(Rc::new);
            if let Some(skin) = &self.own_skin {
                skin.recolor(if self.pixel() { &settings.pixel_note_colors } else { &settings.note_colors });
            }
        }
        self.settings = settings.clone();
        self
    }

    fn pause_items(&self) -> Vec<&'static str> {
        let mut items = vec!["Resume", "Restart Song"];
        if self.difficulties.len() > 1 {
            items.push("Change Difficulty");
        }
        items.extend(["Options", "Exit to menu"]);
        items
    }

    /// Whether the stage wants the pixel look for notes and HUD.
    fn pixel(&self) -> bool {
        self.scene.as_ref().is_some_and(|scene| scene.pixel)
    }

    /// The game over: who falls, the sound of the fall, the music after it and at the retry. The chart's own, else the
    /// stage's, as the Haxe engine has them.
    fn death(&self) -> [String; 4] {
        let (fallen, sound, suffix) = self.scene.as_ref().map_or(("bf-dead", "fnf_loss_sfx", ""), |scene| scene.death());
        let usual = [fallen.to_owned(), sound.to_owned(), format!("gameOver{suffix}"), format!("gameOverEnd{suffix}")];
        let mut own = self.play.chart.game_over.clone().into_iter();
        usual.map(|usual| own.next().filter(|own| !own.is_empty()).unwrap_or(usual))
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
        let confirm = confirmed.is_none() && controls::pressed(Ui::Accept);
        let fell = self.scene.as_mut().is_some_and(|scene| scene.update_dead(dt, confirm));
        // without a scene to play the fall, the music starts after a moment
        if (fell || (self.scene.is_none() && since < 1.0 && since + dt >= 1.0)) && confirmed.is_none() {
            // on the tank stage the music stays low while one of Tankman's lines plays over it
            let line = self.paths.find(&format!("sounds/jeffGameover/jeffGameover-{}.ogg", rand::gen_range(1, 26))).filter(|_| self.play.chart.stage == "tank");
            self.sound(&self.death()[2], if line.is_some() { 0.2 } else { 1.0 }, true);
            if let Some(line) = line {
                self.song.play_sound(&line, 1.0, false);
                self.taunt_until = Some(since + audio::seconds(&line).unwrap_or(0.0));
            }
        }
        // and comes back up over four seconds once it is said
        if let Some(until) = self.taunt_until.filter(|until| since > *until && confirmed.is_none()) {
            self.song.set_music_volume((0.2 + 0.8 * (since - until) / 4.0).min(1.0));
        }
        if confirm {
            self.song.stop_sounds();
            self.sound(&self.death()[3], 1.0, false);
        }
        let confirmed = if confirm { Some(0.0) } else { confirmed.map(|t| t + dt) };
        self.dead = Some((since + dt, confirmed));
        if controls::pressed(Ui::Back) {
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
            let (accept, back) = (controls::pressed(Ui::Accept), controls::pressed(Ui::Back));
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
        if self.staged {
            // the stage's own scene: the song waits, the stage goes on
            let dt = get_frame_time().min(0.1);
            if let Some(scene) = &mut self.scene {
                scene.update(dt, self.position, &self.play.chart, &[], [[false; 4]; 2]);
                self.staged = !scene.stunt(dt, controls::down(Ui::Accept));
            }
            self.stage_sounds();
            return Flow::Continue;
        }
        if let Some(row) = self.paused {
            // the pause music creeps up to half loudness, a hundredth a second, as in the Haxe engine
            self.song.set_music_volume((0.01 * (get_time() - self.paused_at)).min(0.5) as f32);
            // the list glides to the chosen row
            let chosen = self.choosing.unwrap_or(row) as f32;
            self.pause_scroll += (chosen - self.pause_scroll) * (1.0 - (-get_frame_time().min(0.1) * 9.6).exp());
            let step = |row: usize, count: usize| (row + controls::pressed(Ui::Down) as usize + (count - 1) * controls::pressed(Ui::Up) as usize) % count;
            if let Some(pick) = self.choosing {
                // the difficulties, then a way back
                let count = self.difficulties.len() + 1;
                let pick = step(pick, count);
                self.choosing = Some(pick);
                if controls::pressed(Ui::Back) || (controls::pressed(Ui::Accept) && pick == count - 1) {
                    self.choosing = None;
                } else if controls::pressed(Ui::Accept) {
                    return Flow::Difficulty(self.difficulties[pick].clone());
                }
                return Flow::Continue;
            }
            let items = self.pause_items();
            let row = step(row, items.len());
            self.paused = Some(row);
            let chosen = if controls::pressed(Ui::Back) { Some("Resume") } else { controls::pressed(Ui::Accept).then_some(items[row]) };
            return match chosen {
                Some("Resume") => {
                    self.paused = None;
                    self.forget_released();
                    self.song.stop_sounds();
                    if self.started {
                        self.song.start();
                    }
                    Flow::Continue
                }
                Some("Restart Song") => Flow::Restart,
                Some("Change Difficulty") => {
                    self.choosing = self.difficulties.iter().position(|name| *name == self.entry.difficulty).or(Some(0));
                    Flow::Continue
                }
                Some("Options") => Flow::Options,
                Some(_) => Flow::Quit,
                None => Flow::Continue,
            };
        }
        // (as the Haxe engine does, the game pauses itself when another window comes to the front)
        if !self.bowing && (controls::pressed(Ui::Pause) || (self.settings.auto_pause && !is_focused())) {
            self.song.pause();
            (self.paused, self.paused_at, self.pause_scroll) = (Some(0), get_time(), 0.0);
            if self.settings.pause_music > 0 {
                // it starts silent and creeps up, as in the Haxe engine
                self.sound(&funkin_core::content::song_path(settings::PAUSE_MUSIC[self.settings.pause_music]), 0.0, true);
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
        // The sound card's clock wobbles by a few ms between callbacks: the notes move by an even one that follows it.
        // The song never runs backwards.
        let elapsed = std::mem::replace(&mut self.ticked, std::time::Instant::now()).elapsed().as_secs_f64() * 1000.0 * rate;
        let now = if self.started { self.clock.advance(elapsed, self.start_at + self.song.position_ms() + self.play.chart.offset) } else { self.waited - lead_in };
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
                    // judged where the song was when the key went down, not where it is this frame; and, if asked, where the
                    // picture on screen was, which is behind the song
                    let lag = if self.settings.display_lag_correction { display_lag() } else { 0.0 };
                    events.extend(self.play.press(lane, judged - (key.age_ms + lag) * self.settings.playback_rate));
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
                    // moved by the player: right and up
                    let [rating_at, combo_at] = [0, 2].map(|i| vec2(self.settings.combo_offset[i] as f32, -self.settings.combo_offset[i + 1] as f32));
                    if let Some(picture) = skin.ratings.get(rating as usize) {
                        let scale = skin.popup_scales[0];
                        let at = rating_at + vec2(left, (HEIGHT - picture.height() * scale) / 2.0 - 60.0);
                        self.popups.push(Popup { texture: picture.clone(), at, scale, motion: (160.0, 550.0), born: judged, stays: crochet });
                    }
                    // the combo in three digits or more, on every hit ("001"), as the Haxe engine shows it
                    if !skin.digits.is_empty() {
                        for (place, digit) in format!("{:03}", self.play.combo).bytes().enumerate() {
                            let picture = &skin.digits[(digit - b'0') as usize];
                            let scale = skin.popup_scales[1];
                            let at = combo_at + vec2(left - 50.0 + 43.0 * place as f32, (HEIGHT - picture.height() * scale) / 2.0 + 80.0);
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

        let length = self.length();
        if let Some(scene) = &mut self.scene {
            let held = self.play.holding(judged);
            // (only the speaker of weekend 1 shows them; working them out takes thousands of sines a frame)
            if scene.shows_levels() {
                scene.levels = self.song.levels();
            }
            (scene.health, scene.combo, scene.practice) = (self.play.health, self.play.combo, self.settings.practice);
            scene.progress = (judged / length) as f32;
            scene.update(dt * rate as f32, judged, &self.play.chart, &events, held);
        }
        self.stage_sounds();
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
        // a reset key that is also a note key only plays notes
        let reset = controls::keys(Ui::Reset).into_iter().flatten().any(|key| is_key_pressed(key) && self.settings.lane_of(key).is_none()) && !self.settings.no_reset && !self.play.botplay && self.started;
        // (once the song is over, as the stage plays its closing scene, there is nothing left to lose)
        if (self.play.health <= 0.0 || reset) && !self.settings.practice && !self.bowing {
            self.dead = Some((0.0, None));
            self.deaths += 1;
            self.song.pause();
            let [fallen, sound, ..] = self.death();
            self.sound(&sound, 1.0, false);
            self.scene.iter_mut().for_each(|scene| scene.die(&fallen));
            return Flow::Continue;
        }
        if self.position < self.length() { Flow::Continue } else { Flow::Finished }
    }

    /// The song is over: whether the stage's own scene before `next` (the folder of the week's next song) still runs.
    fn bows_out(&mut self, next: &str) -> bool {
        let Some(scene) = self.scene.as_mut().filter(|_| self.story) else { return false };
        self.bowing = self.bowing || scene.close_with(next);
        let runs = self.bowing && !scene.stunt(get_frame_time().min(0.1), false);
        self.stage_sounds();
        runs
    }

    /// Plays what the stage asks for: its sounds, and its music as loud as it wants it just now.
    fn stage_sounds(&mut self) {
        let Some(scene) = &mut self.scene else { return };
        if std::mem::take(&mut scene.hush) {
            self.song.stop_sounds();
            self.tune = false;
        }
        for (path, volume) in scene.sounds.drain(..) {
            self.song.play_sound(&path, volume, false);
        }
        match &scene.tune {
            Some((path, volume)) => {
                if !std::mem::replace(&mut self.tune, true) {
                    self.song.play_sound(path, *volume, true);
                }
                self.song.set_music_volume(*volume);
            }
            None if std::mem::take(&mut self.tune) => self.song.stop_music(),
            None => {}
        }
    }

    /// Length of the song in ms.
    fn length(&self) -> f64 {
        let last_note = self.play.chart.notes.last().map_or(0.0, |n| n.time + n.sustain);
        // (a song opened part way in, with --at, has no sound to measure)
        self.song.length_ms().filter(|ms| *ms > 0.0).unwrap_or(last_note + 1500.0)
    }

    /// Keys let go of while the song did not listen (paused, in the options) are let go of now.
    fn forget_released(&mut self) {
        self.held_keys.retain(|key| is_key_down(*key));
        for lane in 0..4 {
            if !self.lane_held(lane) {
                self.play.release(lane);
            }
        }
    }

    fn lane_held(&self, lane: usize) -> bool {
        self.held_keys.iter().any(|&key| self.settings.lane_of(key) == Some(lane))
    }

    /// `letters` writes in the menus' bold letters: words, left and top, opacity.
    fn draw(&self, skin: &Skin, letters: &dyn Fn(&str, f32, f32, f32)) {
        let (settings, position) = (&self.settings, self.judged());
        // the stage through its own moving camera, then notes and the rest of the HUD on top
        let (hud_zoom, hud_shake) = self.scene.as_ref().map_or((1.0, Vec2::ZERO), |scene| {
            scene.draw();
            (scene.hud_zoom, scene.hud_shake())
        });
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0) + hud_shake, zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT) * hud_zoom, ..Default::default() });
        // the game over scene is the fallen player alone, as in the Haxe engine
        if self.dead.is_some() {
            return;
        }
        if let Some(scene) = self.scene.as_ref().filter(|scene| scene.hides_hud()) {
            // shortcut: a bar that fills while Accept is held to skip; the Haxe engine fills a ring.
            draw_rectangle(WIDTH - 200.0, HEIGHT - 80.0, 120.0 * scene.skipping.min(1.0), 10.0, Color::new(1.0, 1.0, 1.0, scene.skipping.min(1.0)));
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
                // outside story mode the arrows drop in one after the other, over a second each
                let arrived = if self.story { 1.0 } else { ((self.waited as f32 / 1000.0 - 0.5 - 0.2 * lane as f32).clamp(0.0, 1.0) - 1.0).powi(2) };
                let (shown, y) = ((1.0 - arrived).sqrt(), strum_y - 10.0 * (1.0 - (1.0 - arrived).sqrt()));
                if hit {
                    skin.frame(&format!("{} confirm0003", DIRECTIONS[lane]), x, y, None, Some(lane), shown);
                } else if held {
                    skin.frame(&format!("{} press0003", DIRECTIONS[lane]), x, y, None, Some(lane), shown);
                } else {
                    skin.frame(&format!("arrow{}0000", DIRECTIONS[lane].to_uppercase()), x, y, None, None, shown);
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
                let frames = sheet.animation(&skin.splash_names[lane], &[]);
                if let Some(&frame) = frames.get(((position - at) * 0.024) as usize) {
                    recolor(skin.colors(lane));
                    // pixel stages show them as coarse as everything else there
                    coarse(if self.pixel() { PIXEL_ZOOM } else { 0.0 });
                    sheet.draw(frame, lane_x(settings, true, lane) - frame.size.x / 2.0, strum_y - frame.size.y / 2.0, 1.0, false, Color::new(1.0, 1.0, 1.0, settings.splash_alpha as f32));
                    (recolor(None), coarse(0.0));
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
            // hidden until the song starts, then in over half a second (the Haxe engine's circOut)
            let shown = ((self.position - self.start_at) / 500.0).clamp(0.0, 1.0) as f32;
            let shown = (1.0 - (shown - 1.0).powi(2)).sqrt();
            let seen = |color: Color| Color { a: color.a * shown, ..color };
            if settings.time_bar < 2 {
                draw_rectangle(WIDTH / 2.0 - 204.0, time_y - 4.0, 408.0, 27.0, seen(BLACK));
                draw_rectangle(WIDTH / 2.0 - 200.0, time_y, 400.0 * (played / length) as f32, 19.0, seen(WHITE));
            }
            if shown > 0.0 {
                skin.centered(&words[settings.time_bar], time_y + 17.0, 22, seen(WHITE));
            }
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

        // "Sick! (95.12%) - FC", as the Haxe engine puts it
        let rating = self.play.accuracy().map_or("?".to_owned(), |a| {
            let (name, combo) = (funkin_core::play::rating_name(a), self.play.full_combo());
            format!("{} ({}%) - {}", phrase(&format!("rating_{name}"), name), (a * 10000.0).floor() / 100.0, phrase(combo, combo))
        });
        if hud {
            // the score swells a little on every hit and settles within a fifth of a second
            let swell = if settings.score_zoom { 1.0 + 0.075 * (1.0 - (position - self.last_hit) / 200.0).clamp(0.0, 1.0) } else { 1.0 };
            // the same letters drawn larger, not letters of another size: those would have to be made mid-song, which garbles the text for a frame
            let (score, misses) = (self.play.score.to_string(), self.play.misses.to_string());
            let line = if settings.instakill {
                phrase_with("score_text_instakill", "Score: {1} | Rating: {2}", &[&score, &rating])
            } else {
                phrase_with("score_text", "Score: {1} | Misses: {2} | Rating: {3}", &[&score, &misses, &rating])
            };
            let x = (WIDTH - measure_text(&line, skin.font.as_ref(), 20, 1.0).width * swell as f32) / 2.0;
            for (shift, color) in [(2.0, BLACK), (0.0, WHITE)] {
                draw_text_ex(&line, x + shift, bar_y + 52.0 + shift, TextParams { font: skin.font.as_ref(), font_size: 20, font_scale: swell as f32, color, ..Default::default() });
            }
        }
        if let Some((text, color, at)) = &self.judgement {
            let age = (position - at) as f32;
            if age < 700.0 {
                let fade = Color { a: (1.0 - (age - 500.0).max(0.0) / 200.0), ..*color };
                skin.text(text, side_x + settings.combo_offset[2] as f32, HEIGHT / 2.0 + 160.0 - settings.combo_offset[3] as f32, 24, fade);
            }
        }
        if self.play.botplay {
            skin.centered(&phrase("Botplay", "Botplay").to_uppercase(), if settings.downscroll { HEIGHT - 190.0 } else { 190.0 }, 32, WHITE);
        }
        skin.text(&format!("{} [{}]", self.entry.title, difficulty_name(&self.entry.difficulty)), 12.0, if settings.downscroll { 24.0 } else { HEIGHT - 12.0 }, 16, WHITE);

        if let Some(talk) = &self.cutscene {
            talk.draw();
        }
        if let Some(row) = self.paused {
            // the Haxe engine's pause menu: the screen darkens, the song's name, difficulty and game overs slide in at the
            // top right one after another, and the choices stand in a slanted list in large letters
            set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
            let since = (get_time() - self.paused_at) as f32;
            let quart = |t: f32| {
                let t = t.clamp(0.0, 1.0);
                if t < 0.5 { 8.0 * t.powi(4) } else { 1.0 - (2.0 - 2.0 * t).powi(4) / 2.0 }
            };
            draw_rectangle(0.0, 0.0, WIDTH, HEIGHT, Color::new(0.0, 0.0, 0.0, 0.6 * quart(since / 0.4)));
            let deaths = phrase_with("blueballed", "Blueballed: {1}", &[&self.deaths.to_string()]);
            let right = |text: &str, top: f32, alpha: f32| {
                let x = WIDTH - 20.0 - measure_text(text, skin.font.as_ref(), 32, 1.0).width;
                draw_text_ex(text, x, top + 26.0, TextParams { font: skin.font.as_ref(), font_size: 32, color: Color::new(1.0, 1.0, 1.0, alpha), ..Default::default() });
            };
            for (index, text) in [self.entry.title.clone(), difficulty_name(&self.entry.difficulty).to_uppercase(), deaths].iter().enumerate() {
                let shown = quart((since - 0.3 - 0.2 * index as f32) / 0.4);
                right(text, 15.0 + 32.0 * index as f32 + 5.0 * shown, shown);
            }
            if settings.practice {
                right(&phrase("Practice Mode", "Practice Mode").to_uppercase(), 116.0, 1.0);
            }
            let (items, row): (Vec<String>, usize) = match self.choosing {
                Some(pick) => (self.difficulties.iter().map(|name| difficulty_name(name)).chain([phrase("pause_back", "BACK")]).collect(), pick),
                None => (self.pause_items().iter().map(|item| phrase(&format!("pause_{item}"), item)).collect(), row),
            };
            for (index, label) in items.iter().enumerate() {
                let from = index as f32 - self.pause_scroll;
                letters(label, 90.0 + from * 20.0, 320.0 + from * 1.3 * 120.0, if index == row { 1.0 } else { 0.6 });
            }
        }
    }
}

/// The Haxe engine's save (`funkin.sol`) and what it holds: one put in the game's folder, else the one the Haxe engine keeps
/// where the system keeps programs' data, under its company's name.
fn haxe_save(base: &Path) -> Option<(PathBuf, serde_json::Value)> {
    fn find(folder: &Path, depth: u32) -> Option<PathBuf> {
        let entries: Vec<PathBuf> = std::fs::read_dir(folder).ok()?.flatten().map(|entry| entry.path()).collect();
        entries.iter().find(|path| path.file_name().is_some_and(|name| name == "funkin.sol")).cloned()
            .or_else(|| entries.iter().filter(|path| path.is_dir() && depth > 0).find_map(|path| find(path, depth - 1)))
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data = [std::env::var_os("APPDATA").map(PathBuf::from), std::env::var_os("XDG_DATA_HOME").map(PathBuf::from), home.as_ref().map(|home| home.join(".local/share")), home.map(|home| home.join("Library/Application Support"))];
    let file = Some(base.join("funkin.sol")).filter(|file| file.is_file()).or_else(|| data.into_iter().flatten().find_map(|folder| find(&folder.join("ShadowMario"), 4)))?;
    let saved = funkin_core::haxe::unserialize(&std::fs::read_to_string(&file).ok()?)?;
    Some((file, saved))
}

/// What Discord shows of a screen, in the Haxe engine's words.
fn presence(screen: &Screen, weeks: &[funkin_core::content::Week]) -> discord::Presence {
    let details = match screen {
        Screen::Menu(Page::Achievements) => "Achievements Menu",
        Screen::Options(..) => "Options Menu",
        Screen::Playing(_) => "",
        _ => "In the Menus",
    };
    let Screen::Playing(playing) = screen else { return discord::Presence { details: details.to_owned(), ..Default::default() } };
    let mode = if playing.story { format!("Story Mode: {}", weeks.get(playing.entry.week).map_or("", |week| week.name.as_str())) } else { "Freeplay".to_owned() };
    let details = match (playing.dead.is_some(), playing.paused.is_some()) {
        (true, _) => format!("Game Over - {mode}"),
        (_, true) => format!("Paused - {mode}"),
        _ => mode,
    };
    // while the song plays, the time it has left
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs());
    let running = playing.started && playing.paused.is_none() && playing.dead.is_none();
    let left = ((playing.length() - playing.position).max(0.0) / 1000.0) as u64;
    discord::Presence {
        details,
        state: Some(format!("{} ({})", playing.entry.title, playing.entry.difficulty)),
        small_image: playing.scene.as_ref().and_then(|scene| scene.opponent.as_ref()).map(|opponent| opponent.icon_name.clone()),
        start: running.then_some(now),
        end: running.then_some(now + left),
    }
}

/// A difficulty as the player's language calls it.
fn difficulty_name(name: &str) -> String {
    phrase(&format!("difficulty_{name}"), name)
}

/// Where the menus look for files: in the first mod switched on, then where every song looks.
fn menu_paths(base: &Path) -> Paths {
    Paths::new(base, &funkin_core::content::top_mod(base), "")
}

/// The words of the language chosen in the options, from its files in the game and the mods.
fn speak(base: &Path, language: &str) {
    let files: Vec<String> = menu_paths(base).find_all(&format!("data/{language}.lang")).iter().filter_map(|file| std::fs::read_to_string(file).ok()).collect();
    funkin_core::lang::choose(funkin_core::lang::Phrases::parse(files.iter().map(String::as_str)));
}

/// What the play screen wants next.
enum Flow {
    Continue,
    Finished,
    Restart,
    /// The same song at another difficulty.
    Difficulty(String),
    /// The options, and then back to the paused song.
    Options,
    Quit,
}

enum Screen {
    Menu(Page),
    /// The options, and the menu they were opened from.
    Options(options::OptionsMenu, Page),
    Playing(Box<Playing>),
    Failed(String),
    /// What a new player is told before anything flashes.
    Warning,
    /// A song is about to be read (the entry, whether with its story talk): a picture to look at meanwhile, shown for a frame first.
    Loading(usize, bool, bool),
}

/// How much memory the game holds, in megabytes, where the system says.
fn memory_mb() -> Option<u64> {
    #[cfg(windows)]
    {
        /// PROCESS_MEMORY_COUNTERS: the size of the structure, then counts; the fourth number is the memory in use now.
        #[repr(C)]
        struct Counters {
            size: u32,
            faults: u32,
            numbers: [usize; 8],
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn K32GetProcessMemoryInfo(process: *mut std::ffi::c_void, counters: *mut Counters, size: u32) -> i32;
        }
        let mut counters = Counters { size: std::mem::size_of::<Counters>() as u32, faults: 0, numbers: [0; 8] };
        let known = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.size) } != 0;
        known.then_some(counters.numbers[1] as u64 >> 20)
    }
    #[cfg(not(windows))]
    {
        // the second number of /proc/self/statm: pages in memory
        let pages: u64 = std::fs::read_to_string("/proc/self/statm").ok()?.split(' ').nth(1)?.parse().ok()?;
        Some(pages * 4096 >> 20)
    }
}

const TITLE: &str = "Friday Night Funkin': Psych Engine (Rust)";

/// The game's files: where `--base` says, else here, else next to the program.
fn base_dir() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == "--base").and_then(|i| args.get(i + 1)).map(PathBuf::from).unwrap_or_else(|| {
        let here = PathBuf::from(".");
        let beside = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        if here.join("assets").is_dir() { here } else { beside.filter(|p| p.join("assets").is_dir()).unwrap_or(here) }
    })
}

fn main() {
    // what went wrong, where a player can find it; then the usual message
    let usual = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let _ = std::fs::write(base_dir().join("funkin-rs-crash.log"), format!("{panic}\n\n{}\n", std::backtrace::Backtrace::force_capture()));
        usual(panic);
    }));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let shot = value("--shot");
    let shot_after = value("--after").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let shot_when = value("--when").and_then(|s| s.parse::<f64>().ok()).map(|seconds| seconds * 1000.0);
    let start_at = value("--at").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) * 1000.0;

    let base = base_dir();
    let new_player = !Settings::kept(&base);
    // the skins to choose from, the mods' among them, before the settings that choose one are read
    let listed = |file: &str| menu_paths(&base).find_all(file).iter().filter_map(|file| std::fs::read_to_string(file).ok()).flat_map(|text| text.lines().map(str::to_owned).collect::<Vec<_>>()).collect::<Vec<_>>();
    settings::set_skins(&listed("images/noteSkins/list.txt"), &listed("images/noteSplashes/list.txt"));
    let mut settings = Settings::load(&base);
    controls::set(settings.ui_keys);
    audio::set_master(settings.volume as f32);
    // the volume as it was when its keys were last used, and when
    let mut volume_shown = f64::MIN;
    let mut muted = false;
    atlas::SMOOTH.store(settings.antialiasing, std::sync::atomic::Ordering::Relaxed);
    // `--opengl`: the way out when the chosen graphics API shows nothing and the options cannot be reached
    let api = if flag("--opengl") { "OpenGL" } else { settings::GRAPHICS_APIS[settings.graphics_api] };
    match open(Conf { title: TITLE, width: WIDTH, height: HEIGHT, fullscreen: settings.fullscreen, api }) {
        Ok((drawn_with, note)) => {
            eprintln!("Drawing with {drawn_with}");
            // why the chosen graphics API is not the one in use, where a player can find it
            let _ = match note {
                Some(note) => std::fs::write(base.join("funkin-rs-graphics.log"), format!("{note}\nDrawing with {drawn_with}\n")),
                None => std::fs::remove_file(base.join("funkin-rs-graphics.log")),
            };
        }
        Err(error) => {
            eprintln!("The game cannot draw: {error}");
            let _ = std::fs::write(base.join("funkin-rs-graphics.log"), format!("The game cannot draw: {error}\n"));
            return;
        }
    }

    let mut weeks = find_weeks(&base);
    let mut songs = find_songs(&weeks);
    // the first time, what the Haxe engine saved is taken in: its scores, finished weeks and achievements
    let first_save = !Save::exists(&base);
    let mut save = Save::load(&base);
    if let Some((file, saved)) = haxe_save(&base).filter(|_| first_save) {
        save.take_in_haxe(&saved);
        save.save(&base);
        // and the mods' own settings, which this game keeps with the others
        if let Some(kept) = saved["modSettings"].as_object().filter(|_| settings.mod_settings.is_empty()) {
            settings.mod_settings = kept.clone();
            settings.save(&base);
        }
        eprintln!("Took in the Haxe engine's save: {}", file.display());
    }
    // the words of the language chosen, before anything is written or any picture found
    let found_languages = |base: &Path| funkin_core::content::languages(menu_paths(base).roots());
    let (mut languages, mut spoken) = (found_languages(&base), settings.language.clone());
    speak(&base, &spoken);
    let mut menus = Menus::new(&base);
    // story mode: the week being played, the songs still to come, and the score and the misses so far
    let mut story: Option<(usize, Vec<usize>, i64, u32)> = None;
    // achievements just unlocked, with the time each was, and whether their chime is still to play
    let mut toasts: Vec<(&'static str, f64)> = Vec::new();
    let mut chime = false;
    let mut greeted = false;
    let mut fullscreen = (settings.fullscreen, false);
    // the song that waits, paused, while its player is in the options
    let mut held: Option<Box<Playing>> = None;
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
    let pixel_skin = Skin::load_pixel(&base, None).map_err(|error| eprintln!("No pixel note skin: {error}")).ok();
    let mut skins = (settings.note_skin, settings.splash_skin);
    let (mut skin, mut screen) = match Skin::load(&base, skins, None) {
        Ok(skin) => (Some(skin), Screen::Menu(first)),
        Err(error) => (None, Screen::Failed(format!("Could not load the note skin.\n{error}\nRun the game from the folder that holds \"assets\", or pass --base <folder>."))),
    };
    skin.iter().for_each(|skin| skin.recolor(&settings.note_colors));
    pixel_skin.iter().for_each(|skin| skin.recolor(&settings.pixel_note_colors));
    if new_player && matches!(screen, Screen::Menu(Page::Title)) {
        screen = Screen::Warning;
    }
    if flag("--options") && matches!(screen, Screen::Menu(_)) {
        screen = Screen::Options(options::OptionsMenu::at(&value("--options").unwrap_or_default()), Page::Main);
    }
    if let (Some(wanted), Screen::Menu(_)) = (value("--play"), &screen) {
        // compared as file names are, so that spaces, hyphens and capitals do not matter
        let wanted = funkin_core::content::song_path(&wanted);
        screen = match songs.iter().find(|s| funkin_core::content::song_path(&format!("{} {}", s.title, s.difficulty)).contains(&wanted)) {
            Some(entry) => Playing::open(&base, entry, &songs, &settings, start_at, false, false).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p))),
            None => Screen::Failed(format!("No song matches \"{wanted}\".")),
        };
    }

    if let (Some(wanted), Screen::Menu(_)) = (value("--story"), &screen) {
        let list = weeks.iter().position(|week| week.file == wanted).and_then(|index| menu::playlist(index, &weeks[index], &songs, "Normal").map(|list| (index, list)));
        screen = match list {
            Some((week, mut list)) => {
                let entry = &songs[list.remove(0)];
                story = Some((week, list, 0, 0));
                Playing::open(&base, entry, &songs, &settings, start_at, true, false).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p)))
            }
            None => Screen::Failed(format!("No week \"{wanted}\" with a Normal difficulty.")),
        };
    }

    // when the screen last changed: the new one is uncovered by a dark edge sliding down. MAX: changed this very frame.
    let mut wiped = f64::MIN;
    // before that the old screen, as it last looked, is covered the same way: its picture and since when. The new screen waits.
    let mut closing: Option<(Texture2D, f64)> = None;
    // when the mouse last moved: the pointer shows over the menus for two seconds after
    let mut pointer_moved = f64::MIN;
    // what Discord was last told: whether it may show anything, and of which screen (it is told again only when this changes)
    let mut told_discord = (false, (0u8, 0usize, false, false, false));
    loop {
        // the change itself may have taken long (a song loading): the wipe starts when the new screen first shows
        if wiped == f64::MAX {
            wiped = get_time();
        }
        // the game is laid out for 1280x720 and stretched to the window
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        clear_background(BLACK);
        // read every frame, also outside songs, so that a song never starts with old key presses queued
        let key_events = keyboard.poll();
        let plain = |text: &str, x: f32, y: f32, size: u16, color: Color| match &skin {
            Some(skin) => skin.text(text, x, y, size, color),
            None => drop(draw_text(text, x, y, size as f32, color)),
        };

        // which screen it is, for noticing a change
        let which = |screen: &Screen| (std::mem::discriminant(screen), match screen {
            Screen::Menu(page) => *page as usize,
            _ => 0,
        });
        let (was, was_loading) = (which(&screen), matches!(screen, Screen::Loading(..)));
        screen = match screen {
            waiting if closing.is_some() => waiting,
            Screen::Menu(page) => match skin.as_ref().map(|skin| menus.run(page, skin, &weeks, &songs, &save, settings.flashing, settings.shaders)) {
                Some(Go::Stay) => Screen::Menu(page),
                Some(Go::Page(page)) => Screen::Menu(page),
                Some(Go::Options) => Screen::Options(Default::default(), Page::Main),
                Some(Go::Changers) => Screen::Options(options::OptionsMenu::at("Gameplay Changers"), page),
                Some(Go::ModSettings(folder, list)) => Screen::Options(options::OptionsMenu::for_mod(&folder, list), page),
                Some(Go::Play(mut list, week)) => {
                    menus.quiet();
                    let first = list.remove(0);
                    story = week.map(|week| (week, list, 0, 0));
                    Screen::Loading(first, story.is_some(), false)
                }
                Some(Go::Reset(week, name, difficulty)) => {
                    match week {
                        menu::Reset::Song => save.reset_song(&name, &difficulty),
                        menu::Reset::Week => save.reset_week(&name, &difficulty),
                        menu::Reset::Achievement => save.forget(&name),
                    }
                    save.save(&base);
                    Screen::Menu(page)
                }
                Some(Go::Egg(word)) => {
                    save.egg = if save.egg == word { String::new() } else { word.to_owned() };
                    save.save(&base);
                    Screen::Menu(page)
                }
                Some(Go::Unlock(id)) => {
                    if save.unlock(id) {
                        save.save(&base);
                        toasts.push((id, get_time()));
                        chime = true;
                    }
                    Screen::Menu(page)
                }
                Some(Go::Quit) | None => break,
            },
            Screen::Options(mut menu, back) => {
                // the notes as the player has coloured them so far
                let note = |name: &str, lane: usize, pixel: bool, at: Vec2, width: f32, alpha: f32| {
                    if let Some(skin) = if pixel { pixel_skin.as_ref() } else { skin.as_ref() } {
                        skin.fitted(name, lane, at, width, alpha);
                    }
                };
                let adjusting = menu.adjusting();
                let left = match &skin {
                    Some(skin) => menu.run(&mut settings, &mut menus, skin, &plain, &note, keyboard.raw, &bindable, &languages, &base),
                    None => true,
                };
                // the screen for the delay brings its own music: everything else is quiet meanwhile
                match (adjusting, menu.adjusting(), &held) {
                    (false, true, None) => menus.quiet(),
                    // shortcut: a paused song's pause music does not come back afterwards; start it again if that is missed.
                    (false, true, Some(held)) => held.song.stop_sounds(),
                    (true, false, None) => menus.music(),
                    _ => {}
                }
                skin.iter().for_each(|skin| skin.recolor(&settings.note_colors));
                pixel_skin.iter().for_each(|skin| skin.recolor(&settings.pixel_note_colors));
                if left {
                    settings.save(&base);
                    // opened from a paused song: back to it, with what was changed
                    match held.take() {
                        Some(playing) => Screen::Playing(Box::new(playing.with(&settings))),
                        None => Screen::Menu(back),
                    }
                } else {
                    Screen::Options(menu, back)
                }
            }
            Screen::Playing(mut playing) => {
                // a song is only ever opened with the skin loaded
                let flow = match &skin {
                    Some(skin) => {
                        let own = playing.own_skin.clone();
                        let skin = own.as_deref().or(pixel_skin.as_ref().filter(|_| playing.pixel())).unwrap_or(skin);
                        let flow = playing.update(&key_events, skin);
                        playing.draw(skin, &|text, x, y, alpha| menus.write(skin, text, true, x, y, 1.0, alpha));
                        flow
                    }
                    None => Flow::Quit,
                };
                // the henchmen swept off the limo, fifty times over all the songs ever played
                for _ in 0..playing.scene.as_mut().map_or(0, |scene| std::mem::take(&mut scene.kills)) {
                    if save.count(ROADKILL.0, ROADKILL.1) {
                        toasts.push((ROADKILL.0, get_time()));
                        chime = true;
                    }
                    save.save(&base);
                }
                // ten seconds of singing without a rest
                if playing.sang_for >= 10.0 && playing.settings.keeps_scores() && save.unlock("oversinging") {
                    save.save(&base);
                    toasts.push(("oversinging", get_time()));
                    chime = true;
                }
                // what the week goes on with, as its folder is called
                let coming = || story.as_ref().and_then(|(_, rest, ..)| rest.first()).map_or(String::new(), |next| funkin_core::content::song_path(&songs[*next].title));
                match flow {
                    Flow::Continue => Screen::Playing(playing),
                    Flow::Finished if playing.bows_out(&coming()) => Screen::Playing(playing),
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
                        let run = Run { song: &entry.title, accuracy: play.accuracy().unwrap_or(0.0), practice: playing.settings.practice, botplay: play.botplay, idled: playing.idled, lowest_graphics: playing.settings.low_quality && !playing.settings.antialiasing && !playing.settings.shaders,
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
                                Playing::open(&base, next, &songs, &settings, 0.0, true, false).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p)))
                            }
                            Some((week, _, score, _)) => {
                                if playing.settings.keeps_scores() {
                                    save.record_week(&weeks[week].file, &entry.difficulty, score + play.score);
                                }
                                save.save(&base);
                                Screen::Menu(Page::Story)
                            }
                            // straight back to freeplay, as in the Haxe engine
                            None => {
                                save.save(&base);
                                Screen::Menu(Page::Freeplay)
                            }
                        }
                    }
                    // a retry goes straight to the song: the talk was heard
                    Flow::Restart => Playing::open(&base, &playing.entry, &songs, &settings, 0.0, story.is_some(), true).map_or_else(Screen::Failed, |mut again| {
                        again.deaths = playing.deaths;
                        Screen::Playing(Box::new(again))
                    }),
                    Flow::Difficulty(name) => match songs.iter().find(|other| other.week == playing.entry.week && other.title == playing.entry.title && other.difficulty == name) {
                        Some(entry) => Playing::open(&base, entry, &songs, &settings, 0.0, false, false).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p))),
                        None => Screen::Playing(playing),
                    },
                    Flow::Options => {
                        // (the note colours start on the notes of the stage's kind)
                        let menu = options::OptionsMenu::pixel(playing.pixel());
                        held = Some(playing);
                        Screen::Options(menu, Page::Main)
                    }
                    Flow::Quit => Screen::Menu(if story.take().is_some() { Page::Story } else { Page::Freeplay }),
                }
            }
            Screen::Loading(entry, talk, shown) => {
                clear_background(Color::from_hex(0xCAFF4D));
                if let Some(picture) = atlas::picture(&base.join("assets/shared/images/funkay.png")) {
                    let size = picture.size() * (HEIGHT / picture.height());
                    draw_texture_ex(&picture, (WIDTH - size.x) / 2.0, 0.0, WHITE, DrawTextureParams { dest_size: Some(size), ..Default::default() });
                }
                plain(&phrase_with("now_loading", "Now Loading{1}", &["..."]), 12.0, HEIGHT - 16.0, 30, WHITE);
                // the reading itself holds this frame up: the picture was put on screen by the one before
                match shown {
                    true => Playing::open(&base, &songs[entry], &songs, &settings, 0.0, talk, false).map_or_else(Screen::Failed, |p| Screen::Playing(Box::new(p))),
                    false => Screen::Loading(entry, talk, true),
                }
            }
            Screen::Warning => {
                let lines = ["Hey, watch out!", "This Mod contains some flashing lights!", "Press ENTER to disable them now or go to Options Menu.", "Press ESCAPE to ignore this message.", "You've been warned!"];
                for (row, line) in lines.iter().enumerate() {
                    if let Some(skin) = &skin {
                        skin.centered(line, 280.0 + row as f32 * 40.0, 32, WHITE);
                    }
                }
                if controls::pressed(Ui::Accept) || controls::pressed(Ui::Back) {
                    settings.flashing = !controls::pressed(Ui::Accept);
                    settings.save(&base);
                    Screen::Menu(Page::Title)
                } else {
                    Screen::Warning
                }
            }
            Screen::Failed(message) => {
                for (row, line) in message.lines().enumerate() {
                    plain(line, 40.0, 120.0 + row as f32 * 30.0, 22, ORANGE);
                }
                plain("Esc: quit", 40.0, 320.0, 20, GRAY);
                if controls::pressed(Ui::Back) { break; }
                Screen::Failed(message)
            }
        };

        // a dark edge slides down over the screen: black with a screen's height of fading edge, `fading` below it or above
        let cover = |edge: f32, fading: f32| {
            for strip in 0..36 {
                draw_rectangle(0.0, edge + fading * (strip as f32 * 20.0 + if fading < 0.0 { 20.0 } else { 0.0 }), WIDTH, 20.0, Color::new(0.0, 0.0, 0.0, 1.0 - (strip as f32 + 0.5) / 36.0));
            }
            draw_rectangle(0.0, if fading < 0.0 { edge } else { edge - 2.0 * HEIGHT - 400.0 }, WIDTH, 2.0 * HEIGHT + 400.0, BLACK);
        };
        if which(&screen) != was {
            if was_loading {
                // (the loading picture went at once: the song behind it is what gets uncovered)
                wiped = f64::MAX;
            } else {
                closing = Some((Texture2D::from_image(&get_screen_data()), get_time()));
            }
        }
        // (a song may have left its own view of things set)
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        if let Some((picture, since)) = &closing {
            let covered = (get_time() - since) as f32 / 0.5;
            draw_texture_ex(picture, 0.0, 0.0, WHITE, DrawTextureParams { dest_size: Some(vec2(WIDTH, HEIGHT)), ..Default::default() });
            cover(-HEIGHT + (2.0 * HEIGHT + 50.0) * covered, 1.0);
            if covered >= 1.0 {
                closing = None;
                // (the loading picture is there at once)
                wiped = if matches!(screen, Screen::Loading(..)) { f64::MIN } else { f64::MAX };
            }
        }
        let uncovered = if wiped == f64::MAX { 0.0 } else { (get_time() - wiped) as f32 / 0.5 };
        // (until the fading edge above the black has left the screen too)
        let edge = -400.0 + (2.0 * HEIGHT + 50.0) * uncovered;
        if edge < 2.0 * HEIGHT {
            cover(edge, -1.0);
        }
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
                Screen::Playing(playing) => playing.sound("confirmMenu", 0.5, false),
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
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        if settings.show_fps {
            plain(&format!("FPS: {}", get_fps()), 10.0, 17.0, 14, WHITE);
            if let Some(megabytes) = memory_mb() {
                plain(&format!("Memory: {megabytes} MB"), 10.0, 31.0, 14, WHITE);
            }
        }
        // the volume keys, at any time but in the options (where keys are being chosen and typed); a key that plays a note only plays notes
        let free = |action: Ui| !matches!(screen, Screen::Options(..)) && controls::keys(action).into_iter().flatten().any(|key| is_key_pressed(key) && settings.lane_of(key).is_none());
        let louder = free(Ui::VolumeUp) as i32 - free(Ui::VolumeDown) as i32;
        if louder != 0 || free(Ui::VolumeMute) {
            muted = louder == 0 && !muted;
            settings.volume = ((settings.volume * 10.0).round() + louder as f64).clamp(0.0, 10.0) / 10.0;
            settings.save(&base);
            volume_shown = get_time();
        }
        // (also when the options changed it)
        audio::set_master(if muted { 0.0 } else { settings.volume as f32 });
        if get_time() - volume_shown < 1.5 {
            // ten bars at the top of the screen, as many lit as the volume has tenths
            draw_rectangle(WIDTH / 2.0 - 80.0, 0.0, 160.0, 46.0, Color::new(0.0, 0.0, 0.0, 0.6));
            plain("VOLUME", WIDTH / 2.0 - 33.0, 40.0, 14, WHITE);
            let lit = if muted { 0 } else { (settings.volume * 10.0).round() as usize };
            for bar in 0..10 {
                let height = 4.0 + bar as f32 * 2.0;
                draw_rectangle(WIDTH / 2.0 - 59.0 + bar as f32 * 12.0, 26.0 - height, 8.0, height, if bar < lit { WHITE } else { Color::new(1.0, 1.0, 1.0, 0.3) });
            }
        }
        let smoothing = atlas::SMOOTH.swap(settings.antialiasing, std::sync::atomic::Ordering::Relaxed) != settings.antialiasing;
        if smoothing {
            // the Anti-Aliasing option changed: what is already loaded is loaded again, to look as chosen
            menus.forget();
        }
        if smoothing || skins != (settings.note_skin, settings.splash_skin) {
            skins = (settings.note_skin, settings.splash_skin);
            match Skin::load(&base, skins, None) {
                Ok(new) => {
                    new.recolor(&settings.note_colors);
                    skin = Some(new);
                }
                Err(error) => eprintln!("Note skin: {error}"),
            }
        }
        if mouse_moved() || is_mouse_pressed() {
            pointer_moved = get_time();
        }
        show_mouse(matches!(screen, Screen::Menu(_) | Screen::Options(..)) && get_time() - pointer_moved < 2.0);
        let showing = match &screen {
            Screen::Playing(playing) => (3, &**playing as *const Playing as usize, playing.paused.is_some(), playing.dead.is_some(), playing.started),
            Screen::Menu(page) => (1, *page as usize, false, false, false),
            Screen::Options(..) => (2, 0, false, false, false),
            _ => (0, 0, false, false, false),
        };
        if (settings.discord_rpc, showing) != told_discord {
            if settings.discord_rpc {
                discord::show(Some(presence(&screen, &weeks)));
            } else if told_discord.0 {
                discord::show(None);
            }
            told_discord = (settings.discord_rpc, showing);
        }
        let reload = std::mem::take(&mut menus.reload);
        if reload {
            weeks = find_weeks(&base);
            songs = find_songs(&weeks);
            languages = found_languages(&base);
        }
        if reload || spoken != settings.language {
            // another language (or other mods, which may bring words of their own): its words, and its pictures where it has any
            spoken = settings.language.clone();
            speak(&base, &spoken);
            menus.forget();
        }

        frames += 1;
        let reached = match (&screen, shot_when) {
            (Screen::Playing(playing), Some(when)) => playing.position >= when,
            _ => true,
        };
        if let (Some(path), true) = (&shot, frames >= 5 && get_time() >= shot_after && reached) {
            get_screen_data().export_png(path);
            break;
        }
        if is_key_pressed(KeyCode::F11) || alt_enter_pressed() {
            settings.fullscreen ^= true;
            settings.save(&base);
        }
        if (settings.fullscreen, settings.exclusive_fullscreen) != fullscreen {
            fullscreen = (settings.fullscreen, settings.exclusive_fullscreen);
            set_fullscreen(fullscreen.0, fullscreen.1);
        }
        if !next_frame(settings.framerate) {
            break;
        }
    }
    close();
}
