//! Lua scripts, as the Haxe engine runs them (psychlua/FunkinLua.hx): each script in a Lua of its own, told about the song
//! through its globals and called back at its moments (onCreate, onBeatHit, goodNoteHit...). What a script asks of the game
//! is queued as a `Request` for the play screen to carry out; what it asks about the game comes from `Shared::props`,
//! which the play screen keeps up to date.
use crate::atlas::{Frame, Sheet};
use crate::gfx::*;
use mlua::{Function, Lua, MultiValue, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What a callback returns to stop what it is called for (an end of song, a pause, a game over...), as in the Haxe engine.
pub const STOP: &str = "##PSYCHLUA_FUNCTIONSTOP";
const CONTINUE: &str = "##PSYCHLUA_FUNCTIONCONTINUE";
const STOP_LUA: &str = "##PSYCHLUA_FUNCTIONSTOPLUA";
const STOP_HSCRIPT: &str = "##PSYCHLUA_FUNCTIONSTOPHSCRIPT";
const STOP_ALL: &str = "##PSYCHLUA_FUNCTIONSTOPALL";

/// Something a script asked of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    Sound { name: String, volume: f32 },
    Music { name: String, volume: f32, looped: bool },
    /// An event as a chart would have it: its name and two values.
    Event { name: String, value1: String, value2: String },
    SetHealth(f64),
    AddHealth(f64),
    /// A character (by the Haxe engine's names: "boyfriend", "dad", "gf") plays an animation, or its idle move.
    Animation { who: String, animation: String },
    Dance { who: String },
    SetProperty { name: String, value: serde_json::Value },
    /// A camera flashes a colour and fades back over that many seconds.
    Flash { camera: Cam, color: u32, seconds: f64 },
    /// The run's score, misses or hits ("score", "misses", "hits") set, or added to.
    Score { field: &'static str, value: f64, add: bool },
    /// A field of a chart note (by its number) set: ignoreNote, hitCausesMiss, noAnimation, gfNote...
    Note { index: usize, field: String, value: serde_json::Value },
    EndSong,
    RestartSong,
    ExitSong,
}

/// What the scripts of a song share with each other and with the game.
#[derive(Default)]
pub struct Shared {
    pub requests: Vec<Request>,
    /// What scripts may read of the game (getProperty), as the play screen last told it.
    pub props: HashMap<String, serde_json::Value>,
    /// What scripts keep for each other (setVar, getVar).
    vars: HashMap<String, serde_json::Value>,
    /// Lines of debugPrint, for the screen, with when they were printed.
    pub printed: Vec<(String, f64)>,
    /// Timers by tag: seconds left, seconds each, loops left (0: for ever), loops done.
    timers: HashMap<String, (f64, f64, u32, u32)>,
    /// Sprites the scripts made (makeLuaSprite...), in the order they were made; tweens running, by tag.
    sprites: Vec<(String, Sprite)>,
    tweens: HashMap<String, Tween>,
    /// Where the scripts' pictures are found, and those loaded so far.
    paths: Option<funkin_core::content::Paths>,
    art: HashMap<String, Option<Art>>,
    fonts: HashMap<String, Option<Font>>,
    /// The chart's notes as scripts see them, and which of them are on their way now (the `notes` group): the first
    /// and how many.
    pub notes: Vec<NoteView>,
    pub notes_window: (usize, usize),
    /// The keys of the four lanes, which keyPressed("left") and the like ask about.
    pub note_keys: [[Option<KeyCode>; 2]; 4],
    /// Where the scripts' own saves are kept, and those open (by name).
    pub save_dir: PathBuf,
    saves: HashMap<String, serde_json::Map<String, serde_json::Value>>,
    /// Each mod's own settings, by folder.
    mod_settings: serde_json::Map<String, serde_json::Value>,
    /// The mod the song comes from.
    mod_folder: String,
}

struct Script {
    name: String,
    lua: Lua,
    closed: Rc<RefCell<bool>>,
}

/// The scripts of a song.
pub struct Scripts {
    scripts: Vec<Script>,
    pub shared: Rc<RefCell<Shared>>,
}

/// The scripts to run for a song, in the Haxe engine's order: those of the `scripts/` folders, the song's own
/// (`data/<song>/`), the stage's (`stages/<stage>.lua`), then one for each note type (`custom_notetypes/<type>.lua`) and
/// each event (`custom_events/<name>.lua`) the chart has. `roots` are the folders looked in, the first asked first.
pub fn find(roots: &[PathBuf], song: &str, stage: &str, note_types: &[String], events: &[&str]) -> Vec<PathBuf> {
    let in_folders = |folder: &str| -> Vec<PathBuf> {
        // (the game's own folder first, the mod asked first last: the Haxe engine's directoriesWithFile)
        roots.iter().rev().map(|root| root.join(folder)).filter(|dir| dir.is_dir()).flat_map(|dir| {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ending| ending.eq_ignore_ascii_case("lua"))).collect();
            files.sort();
            files
        }).collect()
    };
    let mut found = in_folders("scripts");
    found.extend(in_folders(&format!("data/{song}")));
    let named = |relative: String| roots.iter().map(|root| root.join(&relative)).find(|path| path.is_file());
    found.extend(named(format!("stages/{stage}.lua")));
    // (each once, a note type and an event of one name each their own)
    let (mut types_seen, mut events_seen) = (std::collections::HashSet::new(), std::collections::HashSet::new());
    found.extend(note_types.iter().filter(|kind| types_seen.insert(kind.as_str())).filter_map(|kind| named(format!("custom_notetypes/{kind}.lua"))));
    found.extend(events.iter().filter(|name| events_seen.insert(**name)).filter_map(|name| named(format!("custom_events/{name}.lua"))));
    found
}

/// A Lua value as JSON (what scripts share must outlive the Lua it came from): tables with keys 1, 2... are lists.
fn to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Boolean(flag) => serde_json::json!(flag),
        Value::Integer(number) => serde_json::json!(number),
        Value::Number(number) => serde_json::json!(number),
        Value::String(text) => serde_json::json!(text.to_string_lossy()),
        Value::Table(table) => {
            let length = table.raw_len();
            if length > 0 {
                serde_json::Value::Array((1..=length).map(|index| table.raw_get::<Value>(index).map_or(serde_json::Value::Null, |item| to_json(&item))).collect())
            } else {
                serde_json::Value::Object(table.pairs::<String, Value>().flatten().map(|(key, item)| (key, to_json(&item))).collect())
            }
        }
        _ => serde_json::Value::Null,
    }
}

/// A JSON value as Lua has it.
fn from_json(lua: &Lua, value: &serde_json::Value) -> mlua::Result<Value> {
    Ok(match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(flag) => Value::Boolean(*flag),
        serde_json::Value::Number(number) => number.as_i64().map_or(Value::Number(number.as_f64().unwrap_or(0.0)), Value::Integer),
        serde_json::Value::String(text) => Value::String(lua.create_string(text)?),
        serde_json::Value::Array(items) => Value::Table(lua.create_sequence_from(items.iter().map(|item| from_json(lua, item)).collect::<mlua::Result<Vec<_>>>()?)?),
        serde_json::Value::Object(map) => {
            let table = lua.create_table()?;
            for (key, item) in map {
                table.set(key.as_str(), from_json(lua, item)?)?;
            }
            Value::Table(table)
        }
    })
}

/// A chart note as scripts see it (the Haxe engine's Note): what the chart says, how its run has gone, and what
/// scripts set on it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoteView {
    pub time: f64,
    pub lane: usize,
    pub player: bool,
    pub kind: String,
    pub sustain: f64,
    /// 0 waiting, 1 held, 2 done, 3 missed.
    pub state: u8,
    pub fields: HashMap<String, serde_json::Value>,
}

impl NoteView {
    fn get(&self, field: &str, position: f64) -> serde_json::Value {
        use serde_json::json;
        if let Some(value) = self.fields.get(field) {
            return value.clone();
        }
        match field {
            "strumTime" => json!(self.time),
            "noteData" => json!(self.lane),
            "mustPress" => json!(self.player),
            "noteType" => json!(self.kind),
            "isSustainNote" => json!(false),
            "sustainLength" => json!(self.sustain),
            "wasGoodHit" => json!(self.player && matches!(self.state, 1 | 2)),
            "hitByOpponent" => json!(!self.player && matches!(self.state, 1 | 2)),
            "tooLate" => json!(self.state == 3),
            "canBeHit" => json!(self.state == 0 && (self.time - position).abs() < 166.0),
            "ignoreNote" | "hitCausesMiss" | "noAnimation" | "noMissAnimation" | "gfNote" | "lowPriority" => json!(self.kind == "Hurt Note" && field == "hitCausesMiss"),
            "visible" => json!(true),
            "multAlpha" => json!(1.0),
            "texture" | "animSuffix" => json!(""),
            _ => serde_json::Value::Null,
        }
    }
}

/// A picture a script loaded: alone, or a sheet of frames.
#[derive(Clone)]
enum Art {
    Picture(Texture2D),
    Sheet(Rc<Sheet>),
}

/// Which camera a script's sprite is seen through, as setObjectCamera names them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Cam {
    #[default]
    Game,
    Hud,
    Other,
}

/// An animation of a sprite: its frames, how fast, whether it starts over, and how far it is moved.
#[derive(Clone)]
struct Animation {
    frames: Vec<Frame>,
    fps: f64,
    looped: bool,
    offset: [f64; 2],
}

/// What a text of a script says, and how (Flixel's FlxText, as makeLuaText sets it up).
#[derive(Clone)]
struct Text {
    words: String,
    size: f64,
    /// The width it wraps at and aligns within (0: as wide as its longest line).
    width: f64,
    font: Option<Font>,
    font_name: String,
    /// 0 left, 0.5 centred, 1 right.
    align: f32,
    border: (f64, u32),
}

impl Text {
    /// Its lines, wrapped at its width, each with how wide it is.
    fn lines(&self) -> Vec<(String, f32)> {
        let measure = |line: &str| measure_text(line, self.font.as_ref(), self.size.max(1.0) as u16, 1.0).width;
        let lines = if self.width > 0.0 { crate::menu::wrap(&self.words, self.width as f32, measure) } else { self.words.lines().map(str::to_owned).collect() };
        lines.into_iter().map(|line| {
            let width = measure(&line);
            (line, width)
        }).collect()
    }

    fn size(&self) -> [f64; 2] {
        let lines = self.lines();
        let widest = lines.iter().map(|(_, width)| *width as f64).fold(0.0, f64::max);
        [if self.width > 0.0 { self.width } else { widest + 4.0 }, lines.len().max(1) as f64 * self.size * 1.15 + 4.0]
    }

    fn draw(&self, corner: Vec2, color: Color) {
        let (size, width) = (self.size.max(1.0) as u16, self.size()[0] as f32);
        let (border, border_color) = (self.border.0 as f32, Color { a: color.a, ..Color::from_hex(self.border.1 & 0xFFFFFF) });
        for (row, (line, line_width)) in self.lines().iter().enumerate() {
            let at = corner + vec2(2.0 + (width - 4.0 - line_width) * self.align, 2.0 + (row as f32 + 0.85) * self.size as f32 * 1.15);
            let text = |shift: Vec2, color: Color| draw_text_ex(line, at.x + shift.x, at.y + shift.y, TextParams { font: self.font.as_ref(), font_size: size, color, ..Default::default() });
            // the outline: the words drawn around themselves in the border's colour first
            if border > 0.0 {
                for shift in [vec2(-1.0, -1.0), vec2(0.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 0.0), vec2(1.0, 0.0), vec2(-1.0, 1.0), vec2(0.0, 1.0), vec2(1.0, 1.0)] {
                    text(shift * border, border_color);
                }
            }
            text(Vec2::ZERO, color);
        }
    }
}

/// A sprite a script made, with what Flixel gives every sprite.
#[derive(Clone)]
struct Sprite {
    /// A text instead of a picture (makeLuaText).
    text: Option<Text>,
    art: Option<Art>,
    /// makeGraphic: a plain rectangle of that size and colour.
    solid: Option<([f64; 2], u32)>,
    x: f64,
    y: f64,
    scale: [f64; 2],
    scroll: [f64; 2],
    /// Where the picture is drawn from against x and y (updateHitbox sets it to keep a scaled sprite's corner in place).
    offset: [f64; 2],
    alpha: f64,
    visible: bool,
    angle: f64,
    flip_x: bool,
    color: u32,
    camera: Cam,
    /// Added (addLuaSprite), and in front of the characters or behind them.
    added: bool,
    front: bool,
    animations: HashMap<String, Animation>,
    /// The animation playing and how long it has played.
    playing: Option<(String, f64)>,
}

impl Sprite {
    fn new(art: Option<Art>, x: f64, y: f64) -> Sprite {
        Sprite { text: None, art, solid: None, x, y, scale: [1.0; 2], scroll: [1.0; 2], offset: [0.0; 2], alpha: 1.0, visible: true, angle: 0.0, flip_x: false, color: 0xFFFFFF, camera: Cam::Game, added: false, front: false, animations: HashMap::new(), playing: None }
    }

    /// The frame shown now: of the animation playing, else the first of the sheet, else the whole picture.
    fn frame(&self) -> Option<(Frame, [f64; 2])> {
        if let Some((name, time)) = &self.playing {
            if let Some(animation) = self.animations.get(name).filter(|animation| !animation.frames.is_empty()) {
                let step = (time * animation.fps) as usize;
                let step = if animation.looped { step % animation.frames.len() } else { step.min(animation.frames.len() - 1) };
                return Some((animation.frames[step], animation.offset));
            }
        }
        match self.art.as_ref()? {
            Art::Sheet(sheet) => sheet.frames.first().map(|(_, frame)| (*frame, [0.0; 2])),
            Art::Picture(picture) => Some((Frame { source: Rect::new(0.0, 0.0, picture.width(), picture.height()), offset: Vec2::ZERO, size: picture.size(), rotated: false }, [0.0; 2])),
        }
    }

    /// The size of its frame (or rectangle), unscaled.
    fn size(&self) -> [f64; 2] {
        if let Some(text) = &self.text {
            return text.size();
        }
        match (self.solid, self.frame()) {
            (Some((size, _)), _) => size,
            (_, Some((frame, _))) => [frame.size.x as f64, frame.size.y as f64],
            _ => [0.0; 2],
        }
    }

    /// Whether the animation playing has played through.
    fn finished(&self) -> bool {
        self.playing.as_ref().is_none_or(|(name, time)| self.animations.get(name).is_none_or(|animation| !animation.looped && time * animation.fps >= animation.frames.len() as f64))
    }

    /// Flixel's updateHitbox: its corner stays at x and y however it is scaled.
    fn update_hitbox(&mut self) {
        let size = self.size();
        self.offset = [0, 1].map(|i| -0.5 * (size[i] * self.scale[i].abs() - size[i]));
    }

    /// A field by the name scripts give it (getProperty "tag.x"...).
    fn get(&self, field: &str) -> Option<serde_json::Value> {
        use serde_json::json;
        let size = self.size();
        Some(match field {
            "x" => json!(self.x),
            "y" => json!(self.y),
            "alpha" => json!(self.alpha),
            "visible" => json!(self.visible),
            "angle" => json!(self.angle),
            "flipX" => json!(self.flip_x),
            "color" => json!(self.color),
            "scale.x" => json!(self.scale[0]),
            "scale.y" => json!(self.scale[1]),
            "scrollFactor.x" => json!(self.scroll[0]),
            "scrollFactor.y" => json!(self.scroll[1]),
            "offset.x" => json!(self.offset[0]),
            "offset.y" => json!(self.offset[1]),
            "width" => json!(size[0] * self.scale[0].abs()),
            "height" => json!(size[1] * self.scale[1].abs()),
            "frameWidth" => json!(size[0]),
            "frameHeight" => json!(size[1]),
            "animation.curAnim.name" | "animation.name" => json!(self.playing.as_ref().map_or("", |(name, _)| name)),
            "animation.curAnim.finished" | "animation.finished" => json!(self.finished()),
            "text" => json!(self.text.as_ref()?.words),
            "size" => json!(self.text.as_ref()?.size),
            _ => return None,
        })
    }

    /// Sets a field `get` reads; false for one it does not know.
    fn set(&mut self, field: &str, value: &serde_json::Value) -> bool {
        if let (Some(text), "text") = (&mut self.text, field) {
            text.words = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
            return true;
        }
        let number = value.as_f64().or_else(|| value.as_bool().map(|flag| flag as u8 as f64));
        let Some(number) = number else { return false };
        match field {
            "x" => self.x = number,
            "y" => self.y = number,
            "alpha" => self.alpha = number,
            "visible" => self.visible = number != 0.0,
            "angle" => self.angle = number,
            "flipX" => self.flip_x = number != 0.0,
            "color" => self.color = number as u32 & 0xFFFFFF,
            "scale.x" => self.scale[0] = number,
            "scale.y" => self.scale[1] = number,
            "scrollFactor.x" => self.scroll[0] = number,
            "scrollFactor.y" => self.scroll[1] = number,
            "offset.x" => self.offset[0] = number,
            "offset.y" => self.offset[1] = number,
            "size" if self.text.is_some() => self.text.iter_mut().for_each(|text| text.size = number),
            _ => return false,
        }
        true
    }

    /// Draws it, `scroll` being where the camera it is seen through looks (zero for the HUD's).
    fn draw(&self, scroll: Vec2) {
        if !self.visible || self.alpha <= 0.0 {
            return;
        }
        let size = self.size();
        let (size, scale) = (vec2(size[0] as f32, size[1] as f32), vec2(self.scale[0] as f32, self.scale[1] as f32));
        // a scroll factor under 1 keeps it further back: the camera moves it less
        let corner = vec2((self.x - self.offset[0]) as f32, (self.y - self.offset[1]) as f32) + scroll * (Vec2::ONE - vec2(self.scroll[0] as f32, self.scroll[1] as f32));
        let middle = (corner + size / 2.0).extend(0.0);
        // scaled and turned about its middle, as Flixel does
        push_model_matrix(Mat4::from_translation(middle) * Mat4::from_rotation_z((self.angle as f32).to_radians()) * Mat4::from_scale(scale.extend(1.0)) * Mat4::from_translation(-middle));
        let tint = Color::from_hex(self.color);
        let color = Color { a: self.alpha as f32, ..tint };
        if let Some(text) = &self.text {
            text.draw(corner, color);
            pop_model_matrix();
            return;
        }
        match (&self.solid, self.frame(), &self.art) {
            (Some((_, rgb)), _, _) => draw_rectangle(corner.x, corner.y, size.x, size.y, Color { a: self.alpha as f32, ..Color::from_hex(*rgb & 0xFFFFFF) }),
            (_, Some((frame, moved)), Some(art)) => {
                let texture = match art {
                    Art::Sheet(sheet) => &sheet.texture,
                    Art::Picture(picture) => picture,
                };
                crate::atlas::draw_frame(texture, frame, corner.x - moved[0] as f32, corner.y - moved[1] as f32, 1.0, self.flip_x, color);
            }
            _ => {}
        }
        pop_model_matrix();
    }
}

/// What a tween moves: a field of a sprite, or of the game (by getProperty's name for it).
#[derive(Clone)]
struct Tween {
    object: String,
    field: String,
    from: Option<f64>,
    to: f64,
    /// For a colour: the colours it goes between, mixed channel by channel.
    colors: Option<(u32, u32)>,
    seconds: f64,
    time: f64,
    ease: fn(f64) -> f64,
}

/// Flixel's easing functions, by the names scripts give them (any case); linear for a name not known.
pub fn ease(name: &str) -> fn(f64) -> f64 {
    use std::f64::consts::PI;
    fn bounce_out(t: f64) -> f64 {
        let (n, d) = (7.5625, 2.75);
        if t < 1.0 / d { n * t * t } else if t < 2.0 / d { let t = t - 1.5 / d; n * t * t + 0.75 } else if t < 2.5 / d { let t = t - 2.25 / d; n * t * t + 0.9375 } else { let t = t - 2.625 / d; n * t * t + 0.984375 }
    }
    match name.to_lowercase().as_str() {
        "quadin" => |t| t * t,
        "quadout" => |t| -t * (t - 2.0),
        "quadinout" => |t| if t < 0.5 { 2.0 * t * t } else { 1.0 - 2.0 * (1.0 - t) * (1.0 - t) },
        "cubein" => |t| t * t * t,
        "cubeout" => |t| 1.0 + (t - 1.0).powi(3),
        "cubeinout" => |t| if t < 0.5 { 4.0 * t * t * t } else { 1.0 + 4.0 * (t - 1.0).powi(3) },
        "quartin" => |t| t.powi(4),
        "quartout" => |t| 1.0 - (t - 1.0).powi(4),
        "quartinout" => |t| if t < 0.5 { 8.0 * t.powi(4) } else { 1.0 - 8.0 * (t - 1.0).powi(4) },
        "quintin" => |t| t.powi(5),
        "quintout" => |t| 1.0 + (t - 1.0).powi(5),
        "quintinout" => |t| if t < 0.5 { 16.0 * t.powi(5) } else { 1.0 + 16.0 * (t - 1.0).powi(5) },
        "sinein" => |t| 1.0 - (PI / 2.0 * t).cos(),
        "sineout" => |t| (PI / 2.0 * t).sin(),
        "sineinout" => |t| -((PI * t).cos() - 1.0) / 2.0,
        "circin" => |t| -((1.0 - t * t).max(0.0).sqrt() - 1.0),
        "circout" => |t| (1.0 - (t - 1.0).powi(2)).max(0.0).sqrt(),
        "circinout" => |t| if t < 0.5 { (1.0 - (1.0 - 4.0 * t * t).max(0.0).sqrt()) / 2.0 } else { ((1.0 - (2.0 * t - 2.0).powi(2)).max(0.0).sqrt() + 1.0) / 2.0 },
        "expoin" => |t| if t == 0.0 { 0.0 } else { 2f64.powf(10.0 * (t - 1.0)) },
        "expoout" => |t| if t == 1.0 { 1.0 } else { 1.0 - 2f64.powf(-10.0 * t) },
        "expoinout" => |t| if t == 0.0 || t == 1.0 { t } else if t < 0.5 { 2f64.powf(20.0 * t - 10.0) / 2.0 } else { (2.0 - 2f64.powf(-20.0 * t + 10.0)) / 2.0 },
        "backin" => |t| t * t * (2.70158 * t - 1.70158),
        "backout" => |t| 1.0 - (1.0 - t) * (1.0 - t) * (-2.70158 * (1.0 - t) + 1.70158),
        "backinout" => |t| {
            let c = 1.70158 * 1.525;
            if t < 0.5 { (2.0 * t).powi(2) * ((c + 1.0) * 2.0 * t - c) / 2.0 } else { ((2.0 * t - 2.0).powi(2) * ((c + 1.0) * (2.0 * t - 2.0) + c) + 2.0) / 2.0 }
        },
        "bouncein" => |t| 1.0 - bounce_out(1.0 - t),
        "bounceout" => bounce_out,
        "bounceinout" => |t| if t < 0.5 { (1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0 } else { (1.0 + bounce_out(2.0 * t - 1.0)) / 2.0 },
        "elasticin" => |t| if t == 0.0 || t == 1.0 { t } else { -(2f64.powf(10.0 * (t - 1.0)) * ((t - 1.1) * 2.0 * PI / 0.4).sin()) },
        "elasticout" => |t| if t == 0.0 || t == 1.0 { t } else { 2f64.powf(-10.0 * t) * ((t - 0.1) * 2.0 * PI / 0.4).sin() + 1.0 },
        "elasticinout" => |t| {
            let t = t - 0.5;
            if t < 0.0 { -0.5 * 2f64.powf(10.0 * t) * ((t - 0.1) * 2.0 * PI / 0.4).sin() } else { 2f64.powf(-10.0 * t) * ((t - 0.1) * 2.0 * PI / 0.4).sin() * 0.5 + 1.0 }
        },
        "smoothstepin" => |t| 2.0 * (t / 2.0) * (t / 2.0) * (3.0 - t),
        "smoothstepout" => |t| { let t = t / 2.0 + 0.5; 2.0 * t * t * (3.0 - 2.0 * t) - 1.0 },
        "smoothstepinout" => |t| t * t * (3.0 - 2.0 * t),
        "smootherstepin" => |t| { let t = t / 2.0; 2.0 * t * t * t * (t * (t * 6.0 - 15.0) + 10.0) },
        "smootherstepout" => |t| { let t = t / 2.0 + 0.5; 2.0 * t * t * t * (t * (t * 6.0 - 15.0) + 10.0) - 1.0 },
        "smootherstepinout" => |t| t * t * t * (t * (t * 6.0 - 15.0) + 10.0),
        _ => |t| t,
    }
}

/// A colour as scripts give one ("FF0000", "0xFFFF0000", "#ff0000", or a number).
fn color_of(value: &Value) -> u32 {
    match value {
        Value::Integer(number) => *number as u32 & 0xFFFFFF,
        Value::Number(number) => *number as u32 & 0xFFFFFF,
        Value::String(text) => {
            let text = text.to_string_lossy();
            let digits = text.trim().trim_start_matches('#').trim_start_matches("0x");
            // eight digits carry the opacity first
            let digits = if digits.len() == 8 { &digits[2..] } else { digits };
            u32::from_str_radix(digits, 16).unwrap_or(0xFFFFFF)
        }
        _ => 0xFFFFFF,
    }
}

/// Frame numbers as scripts list them: a table, or a string "0,1,2".
fn indices(value: &Value) -> Vec<u32> {
    match value {
        Value::Table(table) => table.clone().sequence_values::<u32>().flatten().collect(),
        Value::String(text) => text.to_string_lossy().split(',').filter_map(|number| number.trim().parse().ok()).collect(),
        Value::Integer(number) => vec![*number as u32],
        _ => Vec::new(),
    }
}

impl Shared {
    /// What the scripts of a mod's song start with: the mods' own settings, and which mod the song is of.
    pub fn new(mod_settings: serde_json::Map<String, serde_json::Value>, mod_folder: &str, paths: funkin_core::content::Paths) -> Shared {
        Shared { mod_settings, mod_folder: mod_folder.to_owned(), paths: Some(paths), ..Default::default() }
    }

    /// A picture of `images/`, alone or as a sheet of frames, loaded once.
    fn art(&mut self, image: &str, animated: bool) -> Option<Art> {
        let key = format!("{image}|{animated}");
        if let Some(art) = self.art.get(&key) {
            return art.clone();
        }
        let path = self.paths.as_ref()?.find(&format!("images/{image}.png"));
        let art = path.and_then(|path| if animated { Sheet::load(&path).ok().map(|sheet| Art::Sheet(Rc::new(sheet))) } else { crate::atlas::picture(&path).map(Art::Picture) });
        if art.is_none() {
            eprintln!("Lua: images/{image}.png not found");
        }
        self.art.insert(key, art.clone());
        art
    }

    /// A font of `fonts/`, loaded once: the song's mod's, the shared ones, or the game's own.
    fn font(&mut self, name: &str) -> Option<Font> {
        if let Some(font) = self.fonts.get(name) {
            return font.clone();
        }
        let roots = self.paths.as_ref().map_or(Vec::new(), |paths| paths.roots().to_vec());
        // (the game's fonts sit in assets/fonts, beside the shared folder)
        let file = roots.iter().flat_map(|root| [root.join("fonts").join(name), root.join("..").join("fonts").join(name)]).find(|file| file.is_file());
        let font = file.and_then(|file| std::fs::read(file).ok()).and_then(|bytes| load_ttf_font_from_bytes(&bytes).ok());
        self.fonts.insert(name.to_owned(), font.clone());
        font
    }

    fn sprite(&mut self, tag: &str) -> Option<&mut Sprite> {
        self.sprites.iter_mut().find(|(name, _)| name == tag).map(|(_, sprite)| sprite)
    }
}

impl Scripts {
    pub fn none() -> Scripts {
        Scripts { scripts: Vec::new(), shared: Rc::new(RefCell::new(Shared::default())) }
    }

    /// Starts the scripts in `files`, each with the globals in `globals`. One that cannot be read or run is left out
    /// (and said so), as the Haxe engine leaves it out.
    pub fn start(files: &[PathBuf], globals: &[(&str, serde_json::Value)], shared: Shared) -> Scripts {
        let mut scripts = Scripts { scripts: Vec::new(), shared: Rc::new(RefCell::new(shared)) };
        for file in files {
            match scripts.open(file, globals) {
                Ok(script) => scripts.scripts.push(script),
                Err(error) => eprintln!("Lua {}: {error}", file.display()),
            }
        }
        scripts
    }

    fn open(&self, file: &Path, globals: &[(&str, serde_json::Value)]) -> mlua::Result<Script> {
        let code = std::fs::read_to_string(file).map_err(mlua::Error::external)?;
        let lua = Lua::new();
        let closed = Rc::new(RefCell::new(false));
        let name = file.to_string_lossy().replace('\\', "/");
        let g = lua.globals();
        for (key, value) in [("Function_Stop", STOP), ("Function_Continue", CONTINUE), ("Function_StopLua", STOP_LUA), ("Function_StopHScript", STOP_HSCRIPT), ("Function_StopAll", STOP_ALL)] {
            g.set(key, value)?;
        }
        g.set("luaDebugMode", false)?;
        g.set("luaDeprecatedWarnings", true)?;
        g.set("scriptName", name.as_str())?;
        for (key, value) in globals {
            g.set(*key, from_json(&lua, value)?)?;
        }
        self.register(&lua, &closed, &name)?;
        lua.load(&code).set_name(name.as_str()).exec()?;
        Ok(Script { name, lua, closed })
    }

    /// The functions a script can call. Each queues what it asks for, or reads what the game last told.
    fn register(&self, lua: &Lua, closed: &Rc<RefCell<bool>>, name: &str) -> mlua::Result<()> {
        let g = lua.globals();
        let shared = &self.shared;
        macro_rules! function {
            ($name:literal, |$lua:ident, $shared:ident, $args:ident : $ty:ty| $body:expr) => {{
                let $shared = shared.clone();
                g.set($name, lua.create_function(move |$lua, $args: $ty| {
                    let _ = &$lua;
                    $body
                })?)?;
            }};
        }
        macro_rules! request {
            ($name:literal, |$args:ident : $ty:ty| $request:expr) => {
                function!($name, |_lua, shared, $args: $ty| {
                    shared.borrow_mut().requests.push($request);
                    Ok(())
                })
            };
        }
        let script = name.to_owned();
        function!("debugPrint", |_lua, shared, args: MultiValue| {
            let line = args.iter().map(|value| value.to_string().unwrap_or_default()).collect::<Vec<_>>().join(", ");
            eprintln!("{script}: {line}");
            let mut shared = shared.borrow_mut();
            shared.printed.push((line, crate::gfx::get_time()));
            // (only the last few are ever shown)
            let excess = shared.printed.len().saturating_sub(32);
            shared.printed.drain(..excess);
            Ok(())
        });
        let stop = closed.clone();
        function!("close", |_lua, _shared, _args: ()| {
            *stop.borrow_mut() = true;
            Ok(true)
        });
        request!("playSound", |args: (String, Option<f32>)| Request::Sound { name: args.0, volume: args.1.unwrap_or(1.0) });
        request!("playMusic", |args: (String, Option<f32>, Option<bool>)| Request::Music { name: args.0, volume: args.1.unwrap_or(1.0), looped: args.2.unwrap_or(false) });
        request!("triggerEvent", |args: (String, Option<Value>, Option<Value>)| {
            let text = |value: Option<Value>| value.and_then(|value| value.to_string().ok()).unwrap_or_default();
            Request::Event { name: args.0, value1: text(args.1), value2: text(args.2) }
        });
        request!("cameraShake", |args: (String, f64, f64)| {
            // as the Haxe engine's Screen Shake event takes it: "seconds, strength" for the game's camera
            let value = format!("{}, {}", args.2, args.1);
            let (value1, value2) = if args.0.to_lowercase().contains("hud") { (String::new(), value) } else { (value, String::new()) };
            Request::Event { name: "Screen Shake".to_owned(), value1, value2 }
        });
        request!("setHealth", |args: f64| Request::SetHealth(args));
        request!("addHealth", |args: f64| Request::AddHealth(args));
        request!("characterPlayAnim", |args: (String, String, Option<bool>)| Request::Animation { who: args.0, animation: args.1 });
        // a sprite of the scripts by its tag, else a character
        for name in ["playAnim", "objectPlayAnimation", "luaSpritePlayAnimation"] {
            let shared = shared.clone();
            g.set(name, lua.create_function(move |_, args: (String, String, Option<bool>)| {
                let mut shared = shared.borrow_mut();
                match shared.sprite(&args.0) {
                    Some(sprite) => {
                        let same = sprite.playing.as_ref().is_some_and(|(playing, _)| *playing == args.1);
                        if args.2.unwrap_or(false) || !same || sprite.finished() {
                            sprite.playing = Some((args.1, 0.0));
                        }
                    }
                    None => shared.requests.push(Request::Animation { who: args.0, animation: args.1 }),
                }
                Ok(())
            })?)?;
        }
        request!("characterDance", |args: String| Request::Dance { who: args });
        request!("endSong", |_args: ()| Request::EndSong);
        request!("restartSong", |_args: Option<bool>| Request::RestartSong);
        request!("exitSong", |_args: Option<bool>| Request::ExitSong);
        function!("setProperty", |_lua, shared, args: (String, Value)| {
            let (value, mut shared) = (to_json(&args.1), shared.borrow_mut());
            let set = args.0.split_once('.').and_then(|(tag, field)| Some(shared.sprite(tag)?.set(field, &value))).unwrap_or(false);
            if !set {
                shared.requests.push(Request::SetProperty { name: args.0, value });
            }
            Ok(())
        });
        function!("getProperty", |lua, shared, args: String| {
            let mut shared = shared.borrow_mut();
            let of_sprite = args.split_once('.').and_then(|(tag, field)| shared.sprite(tag)?.get(field));
            from_json(lua, &of_sprite.or_else(|| shared.props.get(&args).cloned()).unwrap_or_default())
        });
        function!("getHealth", |lua, shared, _args: ()| from_json(lua, &shared.borrow().props.get("health").cloned().unwrap_or_default()));
        function!("getSongPosition", |lua, shared, _args: ()| from_json(lua, &shared.borrow().props.get("songPosition").cloned().unwrap_or_default()));
        function!("setVar", |_lua, shared, args: (String, Value)| {
            shared.borrow_mut().vars.insert(args.0, to_json(&args.1));
            Ok(())
        });
        function!("getVar", |lua, shared, args: String| from_json(lua, &shared.borrow().vars.get(&args).cloned().unwrap_or_default()));
        function!("runTimer", |_lua, shared, args: (String, Option<f64>, Option<u32>)| {
            let seconds = args.1.unwrap_or(1.0);
            shared.borrow_mut().timers.insert(args.0, (seconds, seconds, args.2.unwrap_or(1), 0));
            Ok(())
        });
        function!("cancelTimer", |_lua, shared, args: String| {
            shared.borrow_mut().timers.remove(&args);
            Ok(())
        });
        function!("getModSetting", |lua, shared, args: (String, Option<String>)| {
            let shared = shared.borrow();
            let folder = args.1.filter(|folder| !folder.is_empty()).unwrap_or_else(|| shared.mod_folder.clone());
            let value = &shared.mod_settings.get(&folder).and_then(|saved| saved.get(&args.0)).cloned().unwrap_or_default();
            // a key is kept with both of its kinds; a script asks for the keyboard's
            from_json(lua, value.get("keyboard").unwrap_or(value))
        });
        function!("getRandomInt", |_lua, _shared, args: (Option<i64>, Option<i64>)| {
            let (low, high) = (args.0.unwrap_or(0), args.1.unwrap_or(i32::MAX as i64));
            Ok(crate::gfx::rand::gen_range(low.min(high), low.max(high) + 1))
        });
        function!("getRandomFloat", |_lua, _shared, args: (Option<f64>, Option<f64>)| Ok(crate::gfx::rand::gen_range(args.0.unwrap_or(0.0) as f32, args.1.unwrap_or(1.0) as f32) as f64));
        function!("getRandomBool", |_lua, _shared, args: Option<f64>| Ok(crate::gfx::rand::gen_range(0.0, 100.0) < args.unwrap_or(50.0) as f32));
        function!("stringStartsWith", |_lua, _shared, args: (String, String)| Ok(args.0.starts_with(&args.1)));
        function!("stringEndsWith", |_lua, _shared, args: (String, String)| Ok(args.0.ends_with(&args.1)));
        function!("stringTrim", |_lua, _shared, args: String| Ok(args.trim().to_owned()));
        function!("stringSplit", |_lua, _shared, args: (String, String)| Ok(args.0.split(args.1.as_str()).map(str::to_owned).collect::<Vec<_>>()));

        // the player's keys: the game's actions by Psych's names ("left" is a lane's key), or any key by Flixel's name
        let asked = |shared: &Shared, name: &str, state: fn(KeyCode) -> bool| -> bool {
            let lane = ["left", "down", "up", "right"].iter().position(|lane| *lane == name);
            let action = match name {
                "accept" => Some(crate::controls::Ui::Accept),
                "back" => Some(crate::controls::Ui::Back),
                "pause" => Some(crate::controls::Ui::Pause),
                "reset" => Some(crate::controls::Ui::Reset),
                "ui_left" => Some(crate::controls::Ui::Left),
                "ui_down" => Some(crate::controls::Ui::Down),
                "ui_up" => Some(crate::controls::Ui::Up),
                "ui_right" => Some(crate::controls::Ui::Right),
                _ => None,
            };
            let keys: Vec<KeyCode> = match (lane, action) {
                (Some(lane), _) => shared.note_keys[lane].into_iter().flatten().collect(),
                (_, Some(action)) => crate::controls::keys(action).into_iter().flatten().collect(),
                _ if name == "space" => vec![KeyCode::Space],
                _ => Vec::new(),
            };
            keys.into_iter().any(state)
        };
        for (name, state) in [("keyJustPressed", is_key_pressed as fn(KeyCode) -> bool), ("keyPressed", is_key_down), ("keyReleased", is_key_released)] {
            let shared = shared.clone();
            g.set(name, lua.create_function(move |_, args: String| Ok(asked(&shared.borrow(), &args.to_lowercase(), state)))?)?;
        }
        for (name, state) in [("keyboardJustPressed", is_key_pressed as fn(KeyCode) -> bool), ("keyboardPressed", is_key_down), ("keyboardReleased", is_key_released)] {
            g.set(name, lua.create_function(move |_, args: String| Ok(crate::controls::key_named(&args).is_some_and(state)))?)?;
        }
        g.set("mouseClicked", lua.create_function(|_, _args: Option<String>| Ok(is_mouse_pressed()))?)?;
        g.set("mousePressed", lua.create_function(|_, _args: Option<String>| Ok(is_mouse_down()))?)?;
        g.set("getMouseX", lua.create_function(|_, _args: Option<String>| Ok(mouse_position(vec2(1280.0, 720.0)).x as f64))?)?;
        g.set("getMouseY", lua.create_function(|_, _args: Option<String>| Ok(mouse_position(vec2(1280.0, 720.0)).y as f64))?)?;

        // the camera's flash, and the run's numbers
        request!("cameraFlash", |args: (String, Option<Value>, Option<f64>, Option<bool>)| {
            let camera = match args.0.to_lowercase().as_str() {
                "hud" | "camhud" => Cam::Hud,
                "other" | "camother" => Cam::Other,
                _ => Cam::Game,
            };
            Request::Flash { camera, color: args.1.as_ref().map_or(0xFFFFFF, color_of), seconds: args.2.unwrap_or(1.0) }
        });
        for (name, field, add) in [("setScore", "score", false), ("addScore", "score", true), ("setMisses", "misses", false), ("addMisses", "misses", true), ("setHits", "hits", false), ("addHits", "hits", true)] {
            let shared = shared.clone();
            g.set(name, lua.create_function(move |_, args: Option<f64>| {
                shared.borrow_mut().requests.push(Request::Score { field, value: args.unwrap_or(if add { 1.0 } else { 0.0 }), add });
                Ok(())
            })?)?;
        }

        // files of the game and the mods, as the song's own are found
        function!("getTextFromFile", |_lua, shared, args: (String, Option<bool>)| {
            let file = shared.borrow().paths.as_ref().and_then(|paths| paths.find(&args.0));
            Ok(file.and_then(|file| std::fs::read_to_string(file).ok()))
        });
        function!("checkFileExists", |_lua, shared, args: (String, Option<bool>)| Ok(Path::new(&args.0).is_file() || shared.borrow().paths.as_ref().and_then(|paths| paths.find(&args.0)).is_some()));
        function!("directoryFileList", |_lua, shared, args: String| {
            let roots: Vec<PathBuf> = shared.borrow().paths.as_ref().map_or(Vec::new(), |paths| paths.roots().to_vec());
            let mut names: Vec<String> = roots.iter().map(|root| root.join(&args)).chain([PathBuf::from(&args)]).filter(|dir| dir.is_dir())
                .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten().map(|entry| entry.file_name().to_string_lossy().into_owned())).collect();
            names.sort();
            names.dedup();
            Ok(names)
        });
        // colours, as numbers with their opacity first (Flixel's)
        for name in ["getColorFromHex", "getColorFromString", "getColorFromName", "FlxColor"] {
            g.set(name, lua.create_function(|_, args: Value| Ok(0xFF00_0000u32 as i64 | color_of(&args) as i64))?)?;
        }
        // (pictures and sounds load when first wanted)
        for name in ["precacheImage", "precacheSound", "precacheMusic"] {
            g.set(name, lua.create_function(|_, _args: MultiValue| Ok(()))?)?;
        }
        // the scripts' own saves: JSON files by name, kept where the game keeps its own
        function!("initSaveData", |_lua, shared, args: (String, Option<String>)| {
            let mut shared = shared.borrow_mut();
            let file = shared.save_dir.join(format!("{}.json", args.0));
            let kept = std::fs::read_to_string(file).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
            shared.saves.entry(args.0).or_insert(kept);
            Ok(())
        });
        function!("setDataFromSave", |_lua, shared, args: (String, String, Value)| {
            shared.borrow_mut().saves.entry(args.0).or_default().insert(args.1, to_json(&args.2));
            Ok(())
        });
        function!("getDataFromSave", |lua, shared, args: (String, String, Option<Value>)| {
            match shared.borrow().saves.get(&args.0).and_then(|save| save.get(&args.1)) {
                Some(value) => from_json(lua, value),
                None => Ok(args.2.unwrap_or(Value::Nil)),
            }
        });
        function!("flushSaveData", |_lua, shared, args: String| {
            let shared = shared.borrow();
            if let Some(save) = shared.saves.get(&args) {
                let _ = std::fs::create_dir_all(&shared.save_dir);
                if let Err(error) = funkin_core::files::write(&shared.save_dir.join(format!("{args}.json")), &serde_json::Value::Object(save.clone()).to_string()) {
                    eprintln!("Lua save {args}: {error}");
                }
            }
            Ok(())
        });
        function!("eraseSaveData", |_lua, shared, args: String| {
            let mut shared = shared.borrow_mut();
            shared.saves.remove(&args);
            let _ = std::fs::remove_file(shared.save_dir.join(format!("{args}.json")));
            Ok(())
        });

        // the strums, by their group and number (strumLineNotes: the opponent's 0-3, the player's 4-7)
        let strum = |group: &str, index: usize| -> Option<usize> {
            match group {
                "strumLineNotes" if index < 8 => Some(index),
                "opponentStrums" if index < 4 => Some(index),
                "playerStrums" if index < 4 => Some(index + 4),
                _ => None,
            }
        };
        // the notes: all of the chart's (unspawnNotes), or those on their way now (notes), by their number
        let note = |shared: &Shared, group: &str, index: usize| -> Option<usize> {
            match group {
                "unspawnNotes" => Some(index).filter(|index| *index < shared.notes.len()),
                "notes" => Some(shared.notes_window.0 + index).filter(|_| index < shared.notes_window.1),
                _ => None,
            }
        };
        function!("getPropertyFromGroup", |lua, shared, args: (String, usize, String)| {
            let shared_now = shared.borrow();
            if let Some(index) = note(&shared_now, &args.0, args.1) {
                let position = shared_now.props.get("songPosition").and_then(|value| value.as_f64()).unwrap_or(0.0);
                return from_json(lua, &shared_now.notes[index].get(&args.2, position));
            }
            drop(shared_now);
            let value = strum(&args.0, args.1).and_then(|index| shared.borrow().props.get(&format!("strum{index}.{}", args.2)).cloned());
            if value.is_none() {
                eprintln!("Lua: getPropertyFromGroup {} {} {} is not known here", args.0, args.1, args.2);
            }
            from_json(lua, &value.unwrap_or_default())
        });
        function!("setPropertyFromGroup", |_lua, shared, args: (String, usize, String, Value)| {
            let mut shared_now = shared.borrow_mut();
            if let Some(index) = note(&shared_now, &args.0, args.1) {
                let value = to_json(&args.3);
                shared_now.notes[index].fields.insert(args.2.clone(), value.clone());
                shared_now.requests.push(Request::Note { index, field: args.2, value });
                return Ok(());
            }
            drop(shared_now);
            match strum(&args.0, args.1) {
                Some(index) => shared.borrow_mut().requests.push(Request::SetProperty { name: format!("strum{index}.{}", args.2), value: to_json(&args.3) }),
                None => eprintln!("Lua: setPropertyFromGroup {} {} {} is not known here", args.0, args.1, args.2),
            }
            Ok(())
        });

        // sprites: made, given pictures and animations, added in front of the characters or behind them, and moved about
        let make = |shared: &mut Shared, tag: String, image: Option<String>, x: Option<f64>, y: Option<f64>, animated: bool| {
            let art = image.filter(|image| !image.is_empty()).and_then(|image| shared.art(&image, animated));
            let sprite = Sprite::new(art, x.unwrap_or(0.0), y.unwrap_or(0.0));
            shared.sprites.retain(|(name, _)| *name != tag);
            shared.sprites.push((tag, sprite));
        };
        function!("makeLuaSprite", |_lua, shared, args: (String, Option<String>, Option<f64>, Option<f64>)| {
            make(&mut shared.borrow_mut(), args.0, args.1, args.2, args.3, false);
            Ok(())
        });
        function!("makeAnimatedLuaSprite", |_lua, shared, args: (String, Option<String>, Option<f64>, Option<f64>, Option<String>)| {
            make(&mut shared.borrow_mut(), args.0, args.1, args.2, args.3, true);
            Ok(())
        });
        function!("makeGraphic", |_lua, shared, args: (String, Option<f64>, Option<f64>, Option<Value>)| {
            if let Some(sprite) = shared.borrow_mut().sprite(&args.0) {
                sprite.solid = Some(([args.1.unwrap_or(256.0), args.2.unwrap_or(256.0)], args.3.as_ref().map_or(0xFFFFFF, color_of)));
            }
            Ok(())
        });
        function!("addLuaSprite", |_lua, shared, args: (String, Option<bool>)| {
            if let Some(sprite) = shared.borrow_mut().sprite(&args.0) {
                (sprite.added, sprite.front) = (true, args.1.unwrap_or(false));
            }
            Ok(())
        });
        function!("removeLuaSprite", |_lua, shared, args: (String, Option<bool>)| {
            let mut shared = shared.borrow_mut();
            if args.1.unwrap_or(true) {
                shared.sprites.retain(|(name, _)| *name != args.0);
            } else if let Some(sprite) = shared.sprite(&args.0) {
                sprite.added = false;
            }
            Ok(())
        });
        function!("luaSpriteExists", |_lua, shared, args: String| Ok(shared.borrow_mut().sprite(&args).is_some()));
        // an animation by the frames whose names begin so (all, or those numbered), or by frame numbers of the sheet
        let animate = |shared: &mut Shared, tag: &str, name: String, prefix: Option<&str>, numbers: Vec<u32>, fps: f64, looped: bool| {
            let Some(sprite) = shared.sprite(tag) else { return };
            let Some(Art::Sheet(sheet)) = &sprite.art else { return };
            let frames = match prefix {
                Some(prefix) => sheet.animation(prefix, &numbers),
                None => numbers.iter().filter_map(|&number| sheet.frames.get(number as usize).map(|(_, frame)| *frame)).collect(),
            };
            let first = sprite.animations.is_empty();
            sprite.animations.insert(name.clone(), Animation { frames, fps, looped, offset: [0.0; 2] });
            // the first one added plays at once, as Psych's do
            if first {
                sprite.playing = Some((name, 0.0));
            }
        };
        function!("addAnimationByPrefix", |_lua, shared, args: (String, String, String, Option<f64>, Option<bool>)| {
            animate(&mut shared.borrow_mut(), &args.0, args.1, Some(&args.2), Vec::new(), args.3.unwrap_or(24.0), args.4.unwrap_or(true));
            Ok(())
        });
        function!("luaSpriteAddAnimationByPrefix", |_lua, shared, args: (String, String, String, Option<f64>, Option<bool>)| {
            animate(&mut shared.borrow_mut(), &args.0, args.1, Some(&args.2), Vec::new(), args.3.unwrap_or(24.0), args.4.unwrap_or(true));
            Ok(())
        });
        for (name, looped) in [("addAnimationByIndices", false), ("luaSpriteAddAnimationByIndices", false), ("addAnimationByIndicesLoop", true)] {
            let shared = shared.clone();
            g.set(name, lua.create_function(move |_, args: (String, String, String, Value, Option<f64>, Option<bool>)| {
                animate(&mut shared.borrow_mut(), &args.0, args.1, Some(&args.2), indices(&args.3), args.4.unwrap_or(24.0), args.5.unwrap_or(looped));
                Ok(())
            })?)?;
        }
        function!("addAnimation", |_lua, shared, args: (String, String, Value, Option<f64>, Option<bool>)| {
            animate(&mut shared.borrow_mut(), &args.0, args.1, None, indices(&args.2), args.3.unwrap_or(24.0), args.4.unwrap_or(true));
            Ok(())
        });
        function!("addOffset", |_lua, shared, args: (String, String, Option<f64>, Option<f64>)| {
            let mut shared = shared.borrow_mut();
            match shared.sprite(&args.0).and_then(|sprite| sprite.animations.get_mut(&args.1)) {
                Some(animation) => animation.offset = [args.2.unwrap_or(0.0), args.3.unwrap_or(0.0)],
                None => shared.requests.push(Request::SetProperty { name: format!("{}.offset.{}", args.0, args.1), value: serde_json::json!([args.2, args.3]) }),
            }
            Ok(())
        });
        function!("setObjectCamera", |_lua, shared, args: (String, Option<String>)| {
            if let Some(sprite) = shared.borrow_mut().sprite(&args.0) {
                sprite.camera = match args.1.unwrap_or_default().to_lowercase().as_str() {
                    "hud" | "camhud" => Cam::Hud,
                    "other" | "camother" => Cam::Other,
                    _ => Cam::Game,
                };
            }
            Ok(())
        });
        function!("scaleObject", |_lua, shared, args: (String, f64, Option<f64>, Option<bool>)| {
            if let Some(sprite) = shared.borrow_mut().sprite(&args.0) {
                sprite.scale = [args.1, args.2.unwrap_or(args.1)];
                if args.3.unwrap_or(true) {
                    sprite.update_hitbox();
                }
            }
            Ok(())
        });
        function!("setGraphicSize", |_lua, shared, args: (String, f64, Option<f64>, Option<bool>)| {
            if let Some(sprite) = shared.borrow_mut().sprite(&args.0) {
                let size = sprite.size();
                let x = if size[0] > 0.0 { args.1 / size[0] } else { 1.0 };
                // a height of 0 keeps the picture's shape
                let y = args.2.filter(|height| *height > 0.0 && size[1] > 0.0).map_or(x, |height| height / size[1]);
                sprite.scale = [x, if args.1 <= 0.0 { y } else { y }];
                if args.3.unwrap_or(true) {
                    sprite.update_hitbox();
                }
            }
            Ok(())
        });
        function!("updateHitbox", |_lua, shared, args: String| {
            shared.borrow_mut().sprite(&args).into_iter().for_each(Sprite::update_hitbox);
            Ok(())
        });
        function!("setScrollFactor", |_lua, shared, args: (String, f64, Option<f64>)| {
            if let Some(sprite) = shared.borrow_mut().sprite(&args.0) {
                sprite.scroll = [args.1, args.2.unwrap_or(args.1)];
            }
            Ok(())
        });
        function!("screenCenter", |_lua, shared, args: (String, Option<String>)| {
            if let Some(sprite) = shared.borrow_mut().sprite(&args.0) {
                let (size, axes) = (sprite.size(), args.1.unwrap_or_else(|| "xy".to_owned()).to_lowercase());
                if axes.contains('x') {
                    sprite.x = (1280.0 - size[0] * sprite.scale[0].abs()) / 2.0;
                }
                if axes.contains('y') {
                    sprite.y = (720.0 - size[1] * sprite.scale[1].abs()) / 2.0;
                }
            }
            Ok(())
        });

        // texts: sprites that say something, on the HUD, as makeLuaText sets them up
        function!("makeLuaText", |_lua, shared, args: (String, Option<String>, Option<f64>, Option<f64>, Option<f64>)| {
            let mut shared = shared.borrow_mut();
            let font = shared.font("vcr.ttf");
            let mut sprite = Sprite::new(None, args.3.unwrap_or(0.0), args.4.unwrap_or(0.0));
            sprite.text = Some(Text { words: args.1.unwrap_or_default(), size: 16.0, width: args.2.unwrap_or(0.0), font, font_name: "vcr.ttf".to_owned(), align: 0.5, border: (2.0, 0) });
            (sprite.camera, sprite.scroll) = (Cam::Hud, [0.0; 2]);
            shared.sprites.retain(|(name, _)| *name != args.0);
            shared.sprites.push((args.0, sprite));
            Ok(())
        });
        let with_text = |shared: &Rc<RefCell<Shared>>, tag: &str, change: &mut dyn FnMut(&mut Text)| -> bool {
            match shared.borrow_mut().sprite(tag).and_then(|sprite| sprite.text.as_mut()) {
                Some(text) => {
                    change(text);
                    true
                }
                None => false,
            }
        };
        function!("setTextString", |_lua, shared, args: (String, String)| Ok(with_text(&shared, &args.0, &mut |text| text.words = args.1.clone())));
        function!("setTextSize", |_lua, shared, args: (String, f64)| Ok(with_text(&shared, &args.0, &mut |text| text.size = args.1)));
        function!("setTextWidth", |_lua, shared, args: (String, f64)| Ok(with_text(&shared, &args.0, &mut |text| text.width = args.1)));
        function!("setTextAlignment", |_lua, shared, args: (String, Option<String>)| {
            let align = match args.1.unwrap_or_default().to_lowercase().as_str() {
                "center" | "centre" => 0.5,
                "right" => 1.0,
                _ => 0.0,
            };
            Ok(with_text(&shared, &args.0, &mut |text| text.align = align))
        });
        function!("setTextBorder", |_lua, shared, args: (String, f64, Option<Value>, Option<String>)| {
            let none = args.3.is_some_and(|style| style.eq_ignore_ascii_case("none"));
            let color = args.2.as_ref().map_or(0, color_of);
            Ok(with_text(&shared, &args.0, &mut |text| text.border = (if none { 0.0 } else { args.1 }, color)))
        });
        function!("setTextColor", |_lua, shared, args: (String, Value)| {
            Ok(match shared.borrow_mut().sprite(&args.0).filter(|sprite| sprite.text.is_some()) {
                Some(sprite) => {
                    sprite.color = color_of(&args.1);
                    true
                }
                None => false,
            })
        });
        function!("setTextFont", |_lua, shared, args: (String, String)| {
            let font = shared.borrow_mut().font(&args.1);
            Ok(with_text(&shared, &args.0, &mut |text| (text.font, text.font_name) = (font.clone().or(text.font.clone()), args.1.clone())))
        });
        // (the plain font has no italics, and a text's height and auto size follow its words here)
        for name in ["setTextItalic", "setTextHeight", "setTextAutoSize"] {
            g.set(name, lua.create_function(|_, _args: MultiValue| Ok(true))?)?;
        }
        let read = |shared: &Rc<RefCell<Shared>>, tag: &str, field: fn(&Text) -> serde_json::Value| shared.borrow_mut().sprite(tag).and_then(|sprite| sprite.text.as_ref()).map(field);
        function!("getTextString", |lua, shared, args: String| from_json(lua, &read(&shared, &args, |text| serde_json::json!(text.words)).unwrap_or_default()));
        function!("getTextSize", |lua, shared, args: String| from_json(lua, &read(&shared, &args, |text| serde_json::json!(text.size)).unwrap_or_default()));
        function!("getTextFont", |lua, shared, args: String| from_json(lua, &read(&shared, &args, |text| serde_json::json!(text.font_name)).unwrap_or_default()));
        function!("getTextWidth", |lua, shared, args: String| from_json(lua, &read(&shared, &args, |text| serde_json::json!(text.size()[0])).unwrap_or_default()));
        function!("addLuaText", |_lua, shared, args: String| {
            if let Some(sprite) = shared.borrow_mut().sprite(&args) {
                sprite.added = true;
            }
            Ok(())
        });
        function!("removeLuaText", |_lua, shared, args: (String, Option<bool>)| {
            let mut shared = shared.borrow_mut();
            if args.1.unwrap_or(true) {
                shared.sprites.retain(|(name, _)| *name != args.0);
            } else if let Some(sprite) = shared.sprite(&args.0) {
                sprite.added = false;
            }
            Ok(())
        });
        function!("luaTextExists", |_lua, shared, args: String| Ok(shared.borrow_mut().sprite(&args).is_some_and(|sprite| sprite.text.is_some())));

        // tweens: of a sprite's field, or of the game's (the camera's zoom, a character's place...)
        let tween = |shared: &mut Shared, tag: String, object: String, field: &str, to: f64, seconds: f64, ease_name: Option<String>, colors: Option<(u32, u32)>| {
            let tween = Tween { object, field: field.to_owned(), from: None, to, colors, seconds: seconds.max(0.0), time: 0.0, ease: ease(ease_name.as_deref().unwrap_or("linear")) };
            shared.tweens.insert(tag, tween);
        };
        for (name, field) in [("doTweenX", "x"), ("doTweenY", "y"), ("doTweenAlpha", "alpha"), ("doTweenAngle", "angle")] {
            let shared = shared.clone();
            g.set(name, lua.create_function(move |_, args: (String, String, f64, f64, Option<String>)| {
                tween(&mut shared.borrow_mut(), args.0, args.1, field, args.2, args.3, args.4, None);
                Ok(())
            })?)?;
        }
        // a strum (0-7, as strumLineNotes numbers them) moved by a tween
        for (name, field) in [("noteTweenX", "x"), ("noteTweenY", "y"), ("noteTweenAlpha", "alpha"), ("noteTweenAngle", "angle"), ("noteTweenDirection", "direction")] {
            let shared = shared.clone();
            g.set(name, lua.create_function(move |_, args: (String, usize, f64, f64, Option<String>)| {
                tween(&mut shared.borrow_mut(), args.0, format!("strum{}", args.1 % 8), field, args.2, args.3, args.4, None);
                Ok(())
            })?)?;
        }
        function!("doTweenZoom", |_lua, shared, args: (String, String, f64, f64, Option<String>)| {
            let camera = if args.1.to_lowercase().contains("hud") { "camHUD" } else { "camGame" };
            tween(&mut shared.borrow_mut(), args.0, camera.to_owned(), "zoom", args.2, args.3, args.4, None);
            Ok(())
        });
        function!("doTweenColor", |_lua, shared, args: (String, String, Value, f64, Option<String>)| {
            let mut shared = shared.borrow_mut();
            let from = shared.sprite(&args.1).map_or(0xFFFFFF, |sprite| sprite.color);
            tween(&mut shared, args.0, args.1, "color", 1.0, args.3, args.4, Some((from, color_of(&args.2))));
            Ok(())
        });
        function!("startTween", |_lua, shared, args: (String, String, mlua::Table, f64, Option<mlua::Table>)| {
            let ease_name = args.4.and_then(|options| options.get::<Option<String>>("ease").ok().flatten());
            let mut shared = shared.borrow_mut();
            // several fields: one tween each, the first under the tag (its end is the one told of)
            for (index, (field, to)) in args.2.pairs::<String, f64>().flatten().enumerate() {
                let tag = if index == 0 { args.0.clone() } else { format!("{}\u{0}{field}", args.0) };
                tween(&mut shared, tag, args.1.clone(), &field, to, args.3, ease_name.clone(), None);
            }
            Ok(())
        });
        function!("cancelTween", |_lua, shared, args: String| {
            shared.borrow_mut().tweens.retain(|tag, _| *tag != args && !tag.starts_with(&format!("{args}\u{0}")));
            Ok(())
        });
        Ok(())
    }

    /// Calls `name` in every script that has it, with `args`. What the last answered is returned (as the Haxe engine's
    /// callOnLuas), Function_Stop winning over the others. A script whose call fails is said so and goes on.
    pub fn call(&mut self, name: &str, args: impl mlua::IntoLuaMulti + Clone) -> Option<String> {
        let mut answer = None;
        for script in self.scripts.iter().filter(|script| !*script.closed.borrow()) {
            let Ok(Some(function)) = script.lua.globals().get::<Option<Function>>(name) else { continue };
            match function.call::<Value>(args.clone()) {
                Ok(Value::String(text)) => {
                    let text = text.to_str().map(|text| text.to_owned()).unwrap_or_default();
                    if answer.as_deref() != Some(STOP) {
                        answer = Some(text);
                    }
                }
                Ok(_) => {}
                Err(error) => eprintln!("Lua {} ({name}): {error}", script.name),
            }
        }
        self.scripts.retain(|script| !*script.closed.borrow());
        answer
    }

    /// Whether `call` was answered with Function_Stop.
    pub fn stopped(answer: &Option<String>) -> bool {
        matches!(answer.as_deref(), Some(STOP | STOP_LUA | STOP_ALL))
    }

    /// Sets a global in every script.
    pub fn set(&self, name: &str, value: impl mlua::IntoLua + Clone) {
        for script in &self.scripts {
            let _ = script.lua.globals().set(name, value.clone());
        }
    }

    /// Runs the timers on by `dt` seconds, calling onTimerCompleted(tag, loops done, loops left) for each that rings.
    pub fn tick(&mut self, dt: f64) {
        let mut rung = Vec::new();
        {
            let mut shared = self.shared.borrow_mut();
            shared.timers.retain(|tag, (left, each, loops, done)| {
                *left -= dt;
                if *left > 0.0 {
                    return true;
                }
                *done += 1;
                let more = *loops == 0 || *done < *loops;
                rung.push((tag.clone(), *done, if *loops == 0 { 0 } else { *loops - *done }));
                *left += *each;
                more
            });
        }
        for (tag, done, left) in rung {
            self.call("onTimerCompleted", (tag, done, left));
        }
        let mut finished = Vec::new();
        {
            let mut shared = self.shared.borrow_mut();
            for (_, sprite) in &mut shared.sprites {
                if let Some((_, time)) = &mut sprite.playing {
                    *time += dt;
                }
            }
            let tweens: Vec<(String, Tween)> = shared.tweens.iter().map(|(tag, tween)| (tag.clone(), tween.clone())).collect();
            for (tag, mut tween) in tweens {
                let name = format!("{}.{}", tween.object, tween.field);
                let from = match tween.from {
                    Some(from) => from,
                    None => {
                        let now = shared.sprite(&tween.object).and_then(|sprite| sprite.get(&tween.field)).or_else(|| shared.props.get(&name).cloned());
                        now.and_then(|now| now.as_f64()).unwrap_or(0.0)
                    }
                };
                tween.from = Some(from);
                tween.time += dt;
                let part = if tween.seconds > 0.0 { (tween.time / tween.seconds).min(1.0) } else { 1.0 };
                let eased = (tween.ease)(part);
                let value = match tween.colors {
                    // a colour goes channel by channel
                    Some((a, b)) => serde_json::json!([16, 8, 0].iter().fold(0u32, |color, shift| {
                        let (a, b) = ((a >> shift & 0xFF) as f64, (b >> shift & 0xFF) as f64);
                        color | (((a + (b - a) * eased).round().clamp(0.0, 255.0) as u32) << shift)
                    })),
                    None => serde_json::json!(from + (tween.to - from) * eased),
                };
                let on_sprite = shared.sprite(&tween.object).is_some_and(|sprite| sprite.set(&tween.field, &value));
                if !on_sprite {
                    shared.requests.push(Request::SetProperty { name, value });
                }
                if part >= 1.0 {
                    shared.tweens.remove(&tag);
                    if !tag.contains('\u{0}') {
                        finished.push((tag, tween.object));
                    }
                } else {
                    shared.tweens.insert(tag, tween);
                }
            }
        }
        for (tag, object) in finished {
            self.call("onTweenCompleted", (tag, object));
        }
    }

    /// Draws the scripts' sprites seen through `camera`; for the game's, those `front` of the characters or behind them.
    /// `scroll`: where that camera looks from (its corner), zero for the HUD's.
    pub fn draw(&self, camera: Cam, front: bool, scroll: Vec2) {
        let shared = self.shared.borrow();
        for (_, sprite) in shared.sprites.iter().filter(|(_, sprite)| sprite.added && sprite.camera == camera && (camera != Cam::Game || sprite.front == front)) {
            sprite.draw(scroll);
        }
    }

    /// What the scripts asked for since the last time.
    pub fn requests(&self) -> Vec<Request> {
        std::mem::take(&mut self.shared.borrow_mut().requests)
    }

    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(code: &str) -> Scripts {
        let file = std::env::temp_dir().join(format!("funkin-lua-test-{}-{}.lua", std::process::id(), code.len()));
        std::fs::write(&file, code).unwrap();
        let scripts = Scripts::start(&[file.clone()], &[("songName", serde_json::json!("test"))], Shared::default());
        let _ = std::fs::remove_file(file);
        scripts
    }

    #[test]
    fn scripts_are_called_back_and_ask_the_game() {
        let mut scripts = run(r#"
            local beats = 0
            function onBeatHit() beats = beats + 1; if beats == 2 then playSound('confirmMenu', 0.5); setHealth(1.5) end end
            function onEndSong() return Function_Stop end
            function onTimerCompleted(tag, done, left) triggerEvent('Hey!', tag, done) end
            runTimer('wait', 0.5, 2)
        "#);
        scripts.call("onBeatHit", ());
        scripts.call("onBeatHit", ());
        assert_eq!(scripts.requests(), [Request::Sound { name: "confirmMenu".to_owned(), volume: 0.5 }, Request::SetHealth(1.5)]);
        assert!(Scripts::stopped(&scripts.call("onEndSong", ())));
        // a timer of two loops rings twice, then is gone
        scripts.tick(0.6);
        scripts.tick(0.5);
        scripts.tick(0.5);
        assert_eq!(scripts.requests(), [1, 2].map(|done| Request::Event { name: "Hey!".to_owned(), value1: "wait".to_owned(), value2: done.to_string() }));
    }

    #[test]
    fn scripts_share_what_they_keep_and_read_the_game() {
        let mut scripts = run("function onBeatHit() setVar('list', {1, 2, songName}); setProperty('camGame.zoom', getProperty('health') + 1) end");
        scripts.shared.borrow_mut().props.insert("health".to_owned(), serde_json::json!(1.5));
        scripts.call("onBeatHit", ());
        assert_eq!(scripts.requests(), [Request::SetProperty { name: "camGame.zoom".to_owned(), value: serde_json::json!(2.5) }]);
        assert_eq!(scripts.shared.borrow().vars["list"], serde_json::json!([1, 2, "test"]));
    }

    #[test]
    fn eases_and_colours_read_as_scripts_write_them() {
        for name in ["linear", "quadInOut", "CUBEOUT", "sineIn", "bounceOut", "backInOut", "elasticOut", "expoIn", "circInOut", "smoothStepInOut", "nonsense"] {
            let ease = ease(name);
            assert!(ease(0.0).abs() < 1e-6 && (ease(1.0) - 1.0).abs() < 1e-6, "{name}");
        }
        assert!((ease("quadIn")(0.5) - 0.25).abs() < 1e-9 && (ease("nonsense")(0.3) - 0.3).abs() < 1e-9);
        let lua = Lua::new();
        let text = |code: &str| Value::String(lua.create_string(code).unwrap());
        assert_eq!([text("FF3366"), text("0xFF00FF00"), text("#0000ff"), Value::Integer(0x123456)].map(|value| color_of(&value)), [0xFF3366, 0x00FF00, 0x0000FF, 0x123456]);
        assert_eq!(indices(&text("0, 2,4")), [0, 2, 4]);
    }

    #[test]
    fn scripts_are_found_where_the_haxe_engine_looks() {
        let root = std::env::temp_dir().join(format!("funkin-lua-find-{}", std::process::id()));
        let (game, top) = (root.join("assets"), root.join("mods/top"));
        for (folder, file) in [(&game, "scripts/b.lua"), (&game, "scripts/a.lua"), (&top, "scripts/c.lua"), (&top, "data/fresh/song.lua"), (&game, "stages/stage.lua"),
                               (&top, "stages/stage.lua"), (&top, "custom_notetypes/Bomb.lua"), (&game, "custom_events/Flash.lua"), (&top, "scripts/notes.txt")] {
            std::fs::create_dir_all(folder.join(file).parent().unwrap()).unwrap();
            std::fs::write(folder.join(file), "").unwrap();
        }
        let found = find(&[top.clone(), game.clone()], "fresh", "stage", &["Bomb".to_owned(), "Bomb".to_owned(), "Unknown".to_owned()], &["Flash", "Hey!"]);
        let names: Vec<String> = found.iter().map(|path| path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/")).collect();
        // the game's scripts folder first, then the mod's; the song's; the stage's from the mod first asked; a script for each type and event
        assert_eq!(names, ["assets/scripts/a.lua", "assets/scripts/b.lua", "mods/top/scripts/c.lua", "mods/top/data/fresh/song.lua", "mods/top/stages/stage.lua", "mods/top/custom_notetypes/Bomb.lua", "assets/custom_events/Flash.lua"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_broken_script_is_left_out_and_a_closed_one_stops() {
        assert!(run("this is not lua").is_empty());
        let mut scripts = run("function onUpdate() close() end");
        assert!(!scripts.is_empty());
        scripts.call("onUpdate", 0.016);
        assert!(scripts.is_empty());
    }
}
