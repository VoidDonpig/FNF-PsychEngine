//! Lua scripts, as the Haxe engine runs them (psychlua/FunkinLua.hx): each script in a Lua of its own, told about the song
//! through its globals and called back at its moments (onCreate, onBeatHit, goodNoteHit...). What a script asks of the game
//! is queued as a `Request` for the play screen to carry out; what it asks about the game comes from `Shared::props`,
//! which the play screen keeps up to date.
use crate::atlas::{Frame, Sheet};
use crate::gfx::*;
use funkin_core::achievements::{exists, score, Op};
use mlua::{Function, Lua, MultiValue, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What a callback returns to stop what it is called for (an end of song, a pause, a game over...), as in the Haxe engine.
pub const STOP: &str = "##PSYCHLUA_FUNCTIONSTOP";
const CONTINUE: &str = "##PSYCHLUA_FUNCTIONCONTINUE";

/// The Haxe engine's shorthands that only call other functions, in Lua: run before each script, which may replace them.
const PRELUDE: &str = r#"
local function who(name)
    name = tostring(name or ''):lower()
    if name == 'dad' or name == 'opponent' then return 'dad' elseif name == 'gf' or name == 'girlfriend' then return 'gf' end
    return 'boyfriend'
end
function getCharacterX(name) return getProperty(who(name) .. '.x') end
function getCharacterY(name) return getProperty(who(name) .. '.y') end
function setCharacterX(name, value) setProperty(who(name) .. '.x', value) end
function setCharacterY(name, value) setProperty(who(name) .. '.y', value) end
-- where something is on the screen through a camera: the game's camera scrolls it by its scroll factor, the others not
local function screenPosition(name, camera, axis)
    local at = getProperty(name .. '.' .. axis)
    if type(at) ~= 'number' then return 0 end
    camera = tostring(camera or 'game'):lower()
    if camera == 'hud' or camera == 'camhud' or camera == 'other' or camera == 'camother' then return at end
    local factor = getProperty(name .. '.scrollFactor.' .. axis)
    if type(factor) ~= 'number' then factor = 1 end
    return at - (getProperty('camGame.scroll.' .. axis) or 0) * factor
end
function getScreenPositionX(name, camera) return screenPosition(name, camera, 'x') end
function getScreenPositionY(name, camera) return screenPosition(name, camera, 'y') end
-- HScript is Haxe code against the Haxe engine's classes: it cannot run here, and each of its functions says so once
do
    for _, name in ipairs({'runHaxeCode', 'runHaxeFunction', 'addHScript', 'removeHScript', 'addHaxeLibrary', 'callOnHScript', 'setOnHScript'}) do
        local told = false
        _G[name] = function()
            if not told then
                told = true
                debugPrint(name .. ': HScript is not supported here')
            end
            return nil
        end
    end
end
function getMidpointX(name) return getProperty(name .. '.midpoint.x') end
function getMidpointY(name) return getProperty(name .. '.midpoint.y') end
function getGraphicMidpointX(name) return getProperty(name .. '.graphicMidpoint.x') or getMidpointX(name) end
function getGraphicMidpointY(name) return getProperty(name .. '.graphicMidpoint.y') or getMidpointY(name) end
function cameraSetTarget(target)
    setProperty('cameraTarget', ({dad = 0, boyfriend = 1, gf = 2})[who(target)])
    return target
end
function getCameraScrollX() return getProperty('camGame.scroll.x') end
function getCameraScrollY() return getProperty('camGame.scroll.y') end
function setCameraScroll(x, y) setProperty('camGame.scroll.x', x); setProperty('camGame.scroll.y', y) end
function addCameraScroll(x, y) setCameraScroll(getCameraScrollX() + (x or 0), getCameraScrollY() + (y or 0)) end
function getCameraFollowX() return getProperty('camFollow.x') end
function getCameraFollowY() return getProperty('camFollow.y') end
function setCameraFollowPoint(x, y) setProperty('camFollow.x', x); setProperty('camFollow.y', y) end
function addCameraFollowPoint(x, y) setCameraFollowPoint(getCameraFollowX() + x, getCameraFollowY() + y) end
function getPropertyLuaSprite(tag, field) return getProperty(tag .. '.' .. field) end
function setPropertyLuaSprite(tag, field, value) setProperty(tag .. '.' .. field, value) end
function luaSpriteMakeGraphic(tag, width, height, color) makeGraphic(tag, width, height, color) end
function scaleLuaSprite(tag, x, y) scaleObject(tag, x, y) end
function setLuaSpriteCamera(tag, camera) return setObjectCamera(tag, camera) end
function setLuaSpriteScrollFactor(tag, x, y) setScrollFactor(tag, x, y) end
function musicFadeIn(duration, from, to) soundFadeIn('', duration, from, to) end
function musicFadeOut(duration, to) soundFadeOut('', duration, to) end
function updateScoreText() end
-- what callMethod and createInstance take for one of the scripts' objects (ReflectionFunctions.instanceArg)
function instanceArg(name, class)
    local text = '##PSYCHLUA_STRINGTOOBJ::' .. tostring(name)
    if class ~= nil then text = text .. '::' .. tostring(class) end
    return text
end
-- the few static methods of the Haxe engine's classes with an answer here, by class (its package left off) and name
-- shortcut: the others are not called; each class not known here says so once
do
    local told = {}
    local methods = {
        ['FlxG.camera.shake'] = function(a) cameraShake('game', a[1] or 0.05, a[2] or 0.5) end,
        ['FlxG.camera.flash'] = function(a) cameraFlash('game', a[1] or 'FFFFFF', a[2] or 1, a[4]) end,
        ['FlxG.camera.fade'] = function(a) cameraFade('game', a[1] or '000000', a[2] or 1, a[5], a[3]) end,
        ['FlxG.random.int'] = function(a) return getRandomInt(a[1], a[2]) end,
        ['FlxG.random.float'] = function(a) return getRandomFloat(a[1], a[2]) end,
        ['FlxG.random.bool'] = function(a) return getRandomBool(a[1]) end,
        ['FlxG.resetState'] = function() restartSong() end,
    }
    function callMethodFromClass(class, method, args)
        local found = methods[(tostring(class or ''):match('([^%.]+)$') or '') .. '.' .. tostring(method)]
        if found then return found(args or {}) end
        if not told[tostring(class)] then
            told[tostring(class)] = true
            debugPrint('callMethodFromClass: ' .. tostring(class) .. ' is not known here')
        end
        return nil
    end
end
function addCharacterToList() end
"#;
const STOP_LUA: &str = "##PSYCHLUA_FUNCTIONSTOPLUA";
const STOP_HSCRIPT: &str = "##PSYCHLUA_FUNCTIONSTOPHSCRIPT";
const STOP_ALL: &str = "##PSYCHLUA_FUNCTIONSTOPALL";

/// Something a script asked of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// A sound of sounds/; with a tag, scripts reach it by that name (another of that name stops).
    Sound { name: String, volume: f32, tag: Option<String>, looped: bool },
    /// Something done to the sound of a tag ("" for the song's music).
    SoundOp { tag: String, op: SoundOp },
    Music { name: String, volume: f32, looped: bool },
    /// An event as a chart would have it: its name and two values.
    Event { name: String, value1: String, value2: String },
    SetHealth(f64),
    AddHealth(f64),
    /// A character (by the Haxe engine's names: "boyfriend", "dad", "gf") plays an animation, or its idle move.
    Animation { who: String, animation: String },
    Dance { who: String },
    SetProperty { name: String, value: serde_json::Value },
    /// A camera flashes a colour and fades back over that many seconds, or (rising) fades to it and stays (cameraFade).
    Flash { camera: Cam, color: u32, seconds: f64, rising: bool },
    /// setHealthBarColors (or, `time`, setTimeBarColors): the bar's left and right colours, those given.
    BarColors { time: bool, left: Option<u32>, right: Option<u32> },
    /// callOnLuas/callOnScripts: a function of every script called, with these values.
    Call { name: String, args: Vec<serde_json::Value> },
    /// setOnLuas/setOnScripts: a global of every script set.
    SetOn { name: String, value: serde_json::Value },
    /// The run's score, misses or hits ("score", "misses", "hits") set, or added to.
    Score { field: &'static str, value: f64, add: bool },
    /// A field of a chart note (by its number) set: ignoreNote, hitCausesMiss, noAnimation, gfNote...
    Note { index: usize, field: String, value: serde_json::Value },
    /// A video (its file): skippable, over black with the game waiting (else over the song, which goes on), looped,
    /// kept from beginning until PlayVideo.
    Video { path: PathBuf, skippable: bool, cover: bool, looped: bool, waiting: bool },
    /// The video that was kept waiting begins (callMethod('videoCutscene.play')).
    PlayVideo,
    /// A dialogue file (DialogueBoxPsych's) and the music to play under it.
    Dialogue { path: PathBuf, music: Option<String> },
    /// The countdown asked for again (onStartCountdown is called again first), as startCountdown() does.
    StartCountdown,
    /// setRatingPercent, setRatingName, setRatingFC: what the rating shows until it is worked out again.
    Rating { percent: Option<f64>, name: Option<String>, fc: Option<String> },
    /// changeDiscordPresence: what Discord shows, until the game changes it again.
    Presence(crate::discord::Presence),
    /// loadSong: another song (by its name, "" for this one) at a difficulty (by its number, -1 for this one).
    LoadSong { name: String, difficulty: i64 },
    EndSong,
    RestartSong,
    ExitSong,
    /// A custom substate opened (openCustomSubstate), carried out the next moment as Flixel opens one; with `pause`
    /// the song and the play screen stand still under it.
    OpenSubstate { name: String, pause: bool },
    CloseSubstate,
    /// Discord's application changed (changeDiscordClientID; empty: the game's own).
    DiscordClient(String),
}

/// What scripts do to a sound they named.
#[derive(Debug, Clone, PartialEq)]
pub enum SoundOp {
    Stop,
    Pause,
    Resume,
    Volume(f32),
    Time(f64),
    Pitch(f64),
    /// Its volume from `from` (or where it is) to `to` over that many seconds.
    Fade { from: Option<f32>, to: f32, seconds: f64 },
    FadeCancel,
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
    /// The song's folder name and the chosen language, where startDialogue looks for files.
    pub song: String,
    pub language: String,
    /// The scripts running, by their names (isRunning, getRunningScripts).
    running: Vec<String>,
    /// The globals scripts start with, and the names of those the game sets as it goes (addLuaScript's script gets both).
    pub globals: Vec<(String, serde_json::Value)>,
    told: Vec<String>,
    /// The pictures' pixels, read back from their files when a script asks for one (getPixelColor).
    pixels: HashMap<PathBuf, Option<Image>>,
    /// The tagged sounds playing ("" the song's music), as the play screen last told: volume, place in ms, pitch.
    pub sounds: HashMap<String, (f32, f64, f64)>,
    /// The achievements there are (the game's and the mods') and a copy of the save, which the achievement functions
    /// work on; what they changed (id, score, unlocked just now) is the game's to keep.
    pub achievements: Option<(Vec<funkin_core::achievements::Listed>, funkin_core::save::Save)>,
    pub achieved: Vec<(String, Option<f64>, bool)>,
    /// What getPropertyFromClass reads that stays as it is through a song, by "Class.field" (the class without its package).
    pub class_props: HashMap<String, serde_json::Value>,
    /// The mods' shaders read so far, by name (None: not found, or refused); whether the player has shaders on; and
    /// those the three characters are drawn through (the opponent, the player, the girlfriend).
    shaders: HashMap<String, Option<Shader>>,
    pub shaders_on: bool,
    pub character_shaders: [Option<Shader>; 3],
    /// And those of the stage file's objects, by their names (the names the stage has, as the play screen told).
    pub object_shaders: HashMap<String, Shader>,
    pub object_names: Vec<String>,
    /// And those the cameras are seen through: the game's, the HUD's, the one over everything.
    pub camera_shaders: [Option<Shader>; 3],
    /// The custom substate open, and the name of the last opened (CustomSubstate.name; none: "unnamed").
    pub substate: Option<Substate>,
    substate_name: Option<String>,
    /// The groups scripts made (createInstance), by name.
    groups: HashMap<String, Group>,
    /// The lines of what is not known here already printed (each is printed once).
    unknown: std::collections::HashSet<String>,
}

/// A custom substate (CustomSubstate.hx): its name, whether it holds the game still, the tags of the sprites put in it
/// in their order, and the timers and tweens it holds still (those running when it opened, as PlayState.openSubState).
#[derive(Clone, Debug, Default)]
pub struct Substate {
    pub name: String,
    pub pause: bool,
    members: Vec<String>,
    held_timers: Vec<String>,
    held_tweens: Vec<String>,
}

/// A group the scripts made (FlxTypedGroup, FlxSpriteGroup): the tags of its sprites, and whether it is added (in front
/// of the characters or behind them).
#[derive(Clone, Debug, Default)]
struct Group {
    members: Vec<String>,
    added: bool,
    front: bool,
}

struct Script {
    name: String,
    lua: Lua,
    closed: Rc<RefCell<bool>>,
    /// Started by another script (addLuaScript), which called its onCreate then.
    added: bool,
}

/// The scripts running; what their functions hold of it (addLuaScript, callScript) does not keep them alive.
type List = Rc<RefCell<Vec<Rc<Script>>>>;
type WeakList = std::rc::Weak<RefCell<Vec<Rc<Script>>>>;

/// Two names of the same script file.
fn same_file(one: &str, other: &str) -> bool {
    one == other || matches!((std::fs::canonicalize(one), std::fs::canonicalize(other)), (Ok(one), Ok(other)) if one == other)
}

/// The scripts of a song.
pub struct Scripts {
    scripts: List,
    pub shared: Rc<RefCell<Shared>>,
}

/// Which of the three characters a name of the Haxe engine's means: 0 the opponent, 1 the player, 2 the girlfriend.
fn character_slot(name: &str) -> Option<usize> {
    ["dad", "boyfriend", "gf", "dadGroup", "boyfriendGroup", "gfGroup"].iter().position(|known| *known == name).map(|at| at % 3)
}

/// Which camera a name means: 0 the game's, 1 the HUD's, 2 the one over everything.
fn camera_slot(name: &str) -> Option<usize> {
    ["camGame", "camHUD", "camOther", "game", "hud", "other"].iter().position(|known| known.eq_ignore_ascii_case(name)).map(|at| at % 3)
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
            "hitHealth" => json!(0.02),
            "missHealth" => json!(0.1),
            "offsetX" | "offsetY" => json!(0),
            "rgbShader.enabled" => json!(true),
            "noteSplashData.disabled" => json!(false),
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
    /// The sheet its frames are on, when that is not the sprite's first (loadMultipleFrames).
    texture: Option<Texture2D>,
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
    /// Drawn adding its light (setBlendMode "add").
    add: bool,
    /// The mod's shader it is drawn through (setSpriteShader).
    shader: Option<Shader>,
    /// The file of its picture (getPixelColor reads it), and the sheets after the first of loadMultipleFrames.
    file: Option<PathBuf>,
    more: Vec<Rc<Sheet>>,
    /// An Adobe Animate atlas it is drawn from instead (makeFlxAnimateSprite), and its animations by name.
    atlas: Option<Rc<crate::animate::Atlas>>,
    symbols: HashMap<String, Symbol>,
}

/// An animation of an atlas sprite (addAnimationBySymbol): a symbol played on its own, all its frames or those numbered,
/// and how far it is moved.
#[derive(Clone)]
struct Symbol {
    name: String,
    numbers: Vec<usize>,
    fps: f64,
    looped: bool,
    at: [f64; 2],
}

/// The frame of a symbol's animation `time` seconds in: of its `numbers` (none: all `length` of the symbol's), started
/// over or held on the last.
fn symbol_frame(numbers: &[usize], length: usize, fps: f64, looped: bool, time: f64) -> usize {
    let count = if numbers.is_empty() { length } else { numbers.len() }.max(1);
    let step = (time * fps) as usize;
    let step = if looped { step % count } else { step.min(count - 1) };
    if numbers.is_empty() { step } else { numbers[step] }
}

impl Sprite {
    fn new(art: Option<Art>, x: f64, y: f64) -> Sprite {
        Sprite { text: None, art, solid: None, x, y, scale: [1.0; 2], scroll: [1.0; 2], offset: [0.0; 2], alpha: 1.0, visible: true, angle: 0.0, flip_x: false, color: 0xFFFFFF, camera: Cam::Game, added: false, front: false, animations: HashMap::new(), playing: None, add: false, shader: None, file: None, more: Vec::new(), atlas: None, symbols: HashMap::new() }
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
        if let (Some((name, time)), Some(atlas)) = (&self.playing, &self.atlas) {
            if let Some(symbol) = self.symbols.get(name) {
                let count = if symbol.numbers.is_empty() { atlas.rig.symbol_length(&symbol.name) } else { symbol.numbers.len() };
                return !symbol.looped && time * symbol.fps >= count as f64;
            }
        }
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
            "midpoint.x" => json!(self.x + size[0] * self.scale[0].abs() / 2.0),
            "midpoint.y" => json!(self.y + size[1] * self.scale[1].abs() / 2.0),
            // (scaled around its middle, the picture's middle stays where it was)
            "graphicMidpoint.x" => json!(self.x - self.offset[0] + size[0] / 2.0),
            "graphicMidpoint.y" => json!(self.y - self.offset[1] + size[1] / 2.0),
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
        blend_add(self.add);
        if let Some(text) = &self.text {
            text.draw(corner, color);
            pop_model_matrix();
            blend_add(false);
            return;
        }
        shade(self.shader.as_ref());
        if let Some(atlas) = &self.atlas {
            match self.playing.as_ref().and_then(|(name, time)| Some((self.symbols.get(name)?, *time))) {
                Some((symbol, time)) => {
                    let frame = symbol_frame(&symbol.numbers, atlas.rig.symbol_length(&symbol.name), symbol.fps, symbol.looped, time);
                    atlas.draw_symbol_as(&symbol.name, frame, corner + vec2(symbol.at[0] as f32, symbol.at[1] as f32), 1.0, self.flip_x, color);
                }
                // (with no animation, the first frame of its main timeline)
                None => atlas.draw(0, corner, 1.0, self.flip_x, color),
            }
        }
        match (&self.solid, self.frame(), &self.art) {
            (Some((_, rgb)), _, _) => draw_rectangle(corner.x, corner.y, size.x, size.y, Color { a: self.alpha as f32, ..Color::from_hex(*rgb & 0xFFFFFF) }),
            (_, Some((frame, moved)), Some(art)) => {
                let other = self.playing.as_ref().and_then(|(name, _)| self.animations.get(name)?.texture.as_ref());
                let texture = match art {
                    _ if other.is_some() => other.unwrap(),
                    Art::Sheet(sheet) => &sheet.texture,
                    Art::Picture(picture) => picture,
                };
                crate::atlas::draw_frame(texture, frame, corner.x - moved[0] as f32, corner.y - moved[1] as f32, 1.0, self.flip_x, color);
            }
            _ => {}
        }
        shade(None);
        pop_model_matrix();
        blend_add(false);
    }

    /// Where it is and how big (Flixel's hitbox: its place, and its frame's size scaled).
    fn bounds(&self) -> Rect {
        let size = self.size();
        Rect::new(self.x as f32, self.y as f32, (size[0] * self.scale[0].abs()) as f32, (size[1] * self.scale[1].abs()) as f32)
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
/// A message of the Haxe engine's luaTrace: on the screen only when the script has luaDebugMode on (a deprecation also
/// wants luaDeprecatedWarnings), always on the error output.
fn trace(lua: &Lua, shared: &mut Shared, line: String, deprecated: bool) {
    let flag = |name: &str| lua.globals().get::<bool>(name).unwrap_or(false);
    if flag("luaDebugMode") && (!deprecated || flag("luaDeprecatedWarnings")) {
        shared.say(line);
    } else {
        eprintln!("Lua: {line}");
    }
}

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

    /// A shader of `shaders/` (`<name>.frag`), read once; one that is missing or cannot be read is said so, once.
    fn shader(&mut self, name: &str) -> Option<Shader> {
        if let Some(known) = self.shaders.get(name) {
            return known.clone();
        }
        // its two parts, either of which may be missing (the usual one is used in its place), as the Haxe engine reads them
        let part = |ending: &str| self.paths.as_ref().and_then(|paths| paths.find(&format!("shaders/{name}.{ending}"))).and_then(|file| std::fs::read_to_string(file).ok());
        let (fragment, vertex) = (part("frag"), part("vert"));
        let read = if fragment.is_none() && vertex.is_none() { Err("its .frag or .vert file was not found".to_owned()) } else { Shader::load(fragment.as_deref(), vertex.as_deref()) };
        if let Err(error) = &read {
            // (on one line: the graphics card's side answers in several)
            let error = error.split_whitespace().collect::<Vec<_>>().join(" ");
            eprintln!("Lua: shader {name}: {error}");
            self.printed.push((format!("Shader {name}: {error}"), crate::gfx::get_time()));
        }
        self.shaders.insert(name.to_owned(), read.as_ref().ok().cloned());
        read.ok()
    }

    /// The shader something is drawn through: a script's sprite by its tag, or a character by the Haxe engine's name.
    fn shader_of(&mut self, object: &str) -> Option<Shader> {
        match (character_slot(object), camera_slot(object)) {
            (Some(slot), _) => self.character_shaders[slot].clone(),
            (_, Some(slot)) => self.camera_shaders[slot].clone(),
            _ if self.object_names.iter().any(|name| name == object) => self.object_shaders.get(object).cloned(),
            _ => self.sprite(object)?.shader.clone(),
        }
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

    /// Where a script may write `path`: under the mods folder (the mod's own folder first, when the path does not start
    /// with "mods/"), never above it.
    fn writable(&self, path: &str) -> Option<PathBuf> {
        let mods = self.paths.as_ref()?.roots().iter().find(|root| root.ends_with("mods"))?.clone();
        let relative = Path::new(path.strip_prefix("mods/").unwrap_or(path));
        let safe = relative.components().all(|part| matches!(part, std::path::Component::Normal(_)));
        let inside = if path.starts_with("mods/") || self.mod_folder.is_empty() { mods } else { mods.join(&self.mod_folder) };
        safe.then(|| inside.join(relative))
    }

    /// The file of a picture of `images/`.
    fn art_file(&self, image: &str) -> Option<PathBuf> {
        self.paths.as_ref()?.find(&format!("images/{image}.png"))
    }

    /// A script's file as the Haxe engine finds it: in the mods' folders (or the game's), else as the path given; ".lua"
    /// added when missing.
    fn find_script(&self, file: &str) -> Option<String> {
        let file = if file.ends_with(".lua") { file.to_owned() } else { format!("{file}.lua") };
        let found = self.paths.as_ref().and_then(|paths| paths.find(&file)).or_else(|| Path::new(&file).is_file().then(|| PathBuf::from(&file)));
        found.map(|path| path.to_string_lossy().replace('\\', "/"))
    }

    fn sprite(&mut self, tag: &str) -> Option<&mut Sprite> {
        self.sprites.iter_mut().find(|(name, _)| name == tag).map(|(_, sprite)| sprite)
    }

    fn has(&self, tag: &str) -> bool {
        self.sprites.iter().any(|(name, _)| name == tag)
    }

    /// A line for the player, on the screen as debugPrint's are (and on the error output).
    fn say(&mut self, line: String) {
        eprintln!("Lua: {line}");
        self.printed.push((line, crate::gfx::get_time()));
    }

    /// `say`, once for each line.
    fn say_once(&mut self, line: String) {
        if self.unknown.insert(line.clone()) {
            self.say(line);
        }
    }
}

impl Scripts {
    pub fn none() -> Scripts {
        Scripts { scripts: Default::default(), shared: Rc::new(RefCell::new(Shared::default())) }
    }

    /// Starts the scripts in `files`, each with the globals in `globals`. One that cannot be read or run is left out
    /// (and said so), as the Haxe engine leaves it out.
    pub fn start(files: &[PathBuf], globals: &[(&str, serde_json::Value)], shared: Shared) -> Scripts {
        let scripts = Scripts { scripts: Default::default(), shared: Rc::new(RefCell::new(shared)) };
        scripts.shared.borrow_mut().globals = globals.iter().map(|(key, value)| (key.to_string(), value.clone())).collect();
        for file in files {
            // (listed first, as the Haxe engine lists a script before it runs)
            scripts.shared.borrow_mut().running.push(file.to_string_lossy().replace('\\', "/"));
            match Scripts::open(&scripts.shared, &Rc::downgrade(&scripts.scripts), file, globals, false) {
                Ok(script) => scripts.scripts.borrow_mut().push(Rc::new(script)),
                Err(error) => eprintln!("Lua {}: {error}", file.display()),
            }
        }
        scripts.shared.borrow_mut().running = scripts.scripts.borrow().iter().filter(|script| !*script.closed.borrow()).map(|script| script.name.clone()).collect();
        scripts
    }

    fn open(shared: &Rc<RefCell<Shared>>, list: &WeakList, file: &Path, globals: &[(&str, serde_json::Value)], added: bool) -> mlua::Result<Script> {
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
        Scripts::register(shared, list, &lua, &closed, &name)?;
        lua.load(PRELUDE).set_name("prelude").exec()?;
        lua.load(&code).set_name(name.as_str()).exec()?;
        Ok(Script { name, lua, closed, added })
    }

    /// The functions a script can call. Each queues what it asks for, or reads what the game last told.
    fn register(shared: &Rc<RefCell<Shared>>, list: &WeakList, lua: &Lua, closed: &Rc<RefCell<bool>>, name: &str) -> mlua::Result<()> {
        let g = lua.globals();
        macro_rules! function {
            ($name:expr, |$lua:ident, $shared:ident, $args:ident : $ty:ty| $body:expr) => {{
                let $shared = shared.clone();
                g.set($name, lua.create_function(move |$lua, $args: $ty| {
                    let _ = &$lua;
                    $body
                })?)?;
            }};
        }
        macro_rules! request {
            ($name:expr, |$args:ident : $ty:ty| $request:expr) => {
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
        request!("playSound", |args: (String, Option<f32>, Option<String>, Option<bool>)| {
            Request::Sound { name: args.0, volume: args.1.unwrap_or(1.0), tag: args.2.filter(|tag| !tag.is_empty()), looped: args.3.unwrap_or(false) }
        });
        // a tag's sound (none: the song's music)
        macro_rules! sound_op {
            ($name:expr, |$args:ident : $ty:ty| ($tag:expr, $op:expr)) => {
                request!($name, |$args: $ty| Request::SoundOp { tag: $tag.unwrap_or_default(), op: $op })
            };
        }
        sound_op!("stopSound", |args: Option<String>| (args, SoundOp::Stop));
        sound_op!("pauseSound", |args: Option<String>| (args, SoundOp::Pause));
        sound_op!("resumeSound", |args: Option<String>| (args, SoundOp::Resume));
        sound_op!("soundFadeCancel", |args: Option<String>| (args, SoundOp::FadeCancel));
        sound_op!("setSoundVolume", |args: (Option<String>, f32)| (args.0, SoundOp::Volume(args.1)));
        sound_op!("setSoundTime", |args: (Option<String>, f64)| (args.0, SoundOp::Time(args.1)));
        sound_op!("setSoundPitch", |args: (Option<String>, f64, Option<bool>)| (args.0, SoundOp::Pitch(args.1)));
        sound_op!("soundFadeIn", |args: (Option<String>, f64, Option<f32>, Option<f32>)| (args.0, SoundOp::Fade { from: Some(args.2.unwrap_or(0.0)), to: args.3.unwrap_or(1.0), seconds: args.1 }));
        sound_op!("soundFadeOut", |args: (Option<String>, f64, Option<f32>)| (args.0, SoundOp::Fade { from: None, to: args.2.unwrap_or(0.0), seconds: args.1 }));
        let heard = |shared: &Rc<RefCell<Shared>>, tag: Option<String>| shared.borrow().sounds.get(&tag.unwrap_or_default()).copied();
        function!("luaSoundExists", |_lua, shared, args: Option<String>| Ok(args.is_some() && heard(&shared, args).is_some()));
        function!("getSoundVolume", |_lua, shared, args: Option<String>| Ok(heard(&shared, args).map_or(0.0, |sound| sound.0)));
        function!("getSoundTime", |_lua, shared, args: Option<String>| Ok(heard(&shared, args).map_or(0.0, |sound| sound.1)));
        function!("getSoundPitch", |_lua, shared, args: Option<String>| Ok(heard(&shared, args).map_or(1.0, |sound| sound.2)));
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
        request!("startCountdown", |_args: ()| Request::StartCountdown);
        function!("startDialogue", |_lua, shared, args: (String, Option<String>)| {
            let mut shared = shared.borrow_mut();
            let found = shared.paths.as_ref().and_then(|paths| crate::speech::find(paths, &shared.song, &args.0, &shared.language));
            let Some(path) = found else {
                shared.printed.push((format!("startDialogue: Dialogue file not found: {}", args.0), crate::gfx::get_time()));
                return Ok(false);
            };
            shared.requests.push(Request::Dialogue { path, music: args.1 });
            Ok(true)
        });
        function!("startVideo", |_lua, shared, args: (String, Option<bool>, Option<bool>, Option<bool>, Option<bool>)| {
            let mut shared = shared.borrow_mut();
            let Some(path) = shared.paths.as_ref().and_then(|paths| paths.video(&args.0)) else {
                shared.printed.push((format!("startVideo: Video file not found: {}", args.0), crate::gfx::get_time()));
                return Ok(false);
            };
            shared.requests.push(Request::Video { path, skippable: args.1.unwrap_or(true), cover: !args.2.unwrap_or(false), looped: args.3.unwrap_or(false), waiting: !args.4.unwrap_or(true) });
            Ok(true)
        });
        // shortcut: the methods scripts call by name here are the few the game has an answer to; the Haxe engine calls any of its own
        function!("callMethod", |_lua, shared, args: (String, Option<Vec<Value>>)| {
            match args.0.as_str() {
                "videoCutscene.play" | "videoCutscene.videoSprite.play" => shared.borrow_mut().requests.push(Request::PlayVideo),
                method => {
                    // a group the scripts made, filled as Haxe scripts fill one: callMethod('group.add', {instanceArg('tag')})
                    let given = args.1.iter().flatten();
                    let tags: Vec<String> = given.clone().filter_map(|value| match value {
                        Value::String(text) => text.to_string_lossy().strip_prefix("##PSYCHLUA_STRINGTOOBJ::").map(|rest| rest.split("::").next().unwrap_or(rest).to_owned()),
                        _ => None,
                    }).collect();
                    let position = given.clone().find_map(|value| match value {
                        Value::Integer(number) => Some(*number),
                        Value::Number(number) => Some(*number as i64),
                        _ => None,
                    });
                    let mut shared = shared.borrow_mut();
                    let (group, call) = method.rsplit_once('.').unwrap_or((method, ""));
                    match (shared.groups.get_mut(group), call, tags.first()) {
                        (Some(group), "add" | "insert", Some(tag)) => {
                            if !group.members.contains(tag) {
                                let at = position.and_then(|at| usize::try_from(at).ok()).map_or(group.members.len(), |at| at.min(group.members.len()));
                                group.members.insert(at, tag.clone());
                            }
                        }
                        (Some(group), "remove", Some(tag)) => group.members.retain(|name| name != tag),
                        _ => eprintln!("Lua: callMethod {method} is not known here"),
                    }
                }
            }
            Ok(())
        });
        // the achievements: scores counted up to a top that unlocks, as Achievements.addLuaCallbacks
        for (name, op, amount) in [("getAchievementScore", Op::Get, 0.0), ("setAchievementScore", Op::Set, 0.0), ("addAchievementScore", Op::Add, 1.0)] {
            function!(name, |_lua, shared, args: (String, Option<f64>)| {
                let mut shared = shared.borrow_mut();
                let shared = &mut *shared;
                let Some((list, save)) = shared.achievements.as_mut().filter(|(list, _)| exists(list, &args.0)) else {
                    shared.printed.push((format!("{name}: Couldnt find achievement: {}", args.0), crate::gfx::get_time()));
                    return Ok(Some(-1.0));
                };
                match score(list, save, &args.0, op, args.1.unwrap_or(amount)) {
                    Ok(now) => {
                        if op != Op::Get {
                            shared.achieved.push((args.0, Some(now.value), now.unlocked_now));
                        }
                        Ok(Some(now.value))
                    }
                    Err(error) => {
                        shared.printed.push((error, crate::gfx::get_time()));
                        Ok(None)
                    }
                }
            });
        }
        function!("unlockAchievement", |_lua, shared, args: String| {
            let mut shared = shared.borrow_mut();
            let shared = &mut *shared;
            let Some((_, save)) = shared.achievements.as_mut().filter(|(list, _)| exists(list, &args)) else {
                shared.printed.push((format!("unlockAchievement: Couldnt find achievement: {args}"), crate::gfx::get_time()));
                return Ok(None);
            };
            if !save.unlock(&args) {
                return Ok(None);
            }
            shared.achieved.push((args.clone(), None, true));
            Ok(Some(args))
        });
        function!("isAchievementUnlocked", |_lua, shared, args: String| {
            let mut shared = shared.borrow_mut();
            let known = shared.achievements.as_ref().filter(|(list, _)| exists(list, &args)).map(|(_, save)| save.unlocked(&args));
            if known.is_none() {
                shared.printed.push((format!("isAchievementUnlocked: Couldnt find achievement: {args}"), crate::gfx::get_time()));
            }
            Ok(known)
        });
        function!("achievementExists", |_lua, shared, args: String| Ok(shared.borrow().achievements.as_ref().is_some_and(|(list, _)| exists(list, &args))));
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
        // shortcut: the classes of the Haxe engine scripts read from are the few here: the options (ClientPrefs.data), the
        // song and the play screen (PlayState, its `instance` being what getProperty reads), the Conductor, FlxG's size,
        // music and camera; setPropertyFromClass reaches the play screen and the game over's names
        function!("getPropertyFromClass", |lua, shared, args: (String, String)| {
            let shared = shared.borrow();
            let class = args.0.rsplit('.').next().unwrap_or("");
            if (class, args.1.as_str()) == ("CustomSubstate", "name") {
                return from_json(lua, &serde_json::json!(shared.substate_name.as_deref().unwrap_or("unnamed")));
            }
            let live = match (class, args.1.as_str()) {
                ("PlayState", field) => field.strip_prefix("instance."),
                ("Conductor", "songPosition") | ("FlxG", "sound.music.time") => Some("songPosition"),
                ("FlxG", "sound.music.length") => Some("songLength"),
                ("FlxG", "camera.zoom") => Some("camGame.zoom"),
                _ => None,
            };
            let found = live.and_then(|name| shared.props.get(name)).or_else(|| shared.class_props.get(&format!("{class}.{}", args.1)));
            if found.is_none() {
                eprintln!("Lua: getPropertyFromClass {}.{} is not known here", args.0, args.1);
            }
            from_json(lua, &found.cloned().unwrap_or_default())
        });
        function!("setPropertyFromClass", |_lua, shared, args: (String, String, Value)| {
            let class = args.0.rsplit('.').next().unwrap_or("").to_owned();
            let name = match (class.as_str(), args.1.strip_prefix("instance.")) {
                ("PlayState", Some(field)) => field.to_owned(),
                ("FlxG", _) if args.1 == "camera.zoom" => "camGame.zoom".to_owned(),
                _ => format!("{class}.{}", args.1),
            };
            shared.borrow_mut().requests.push(Request::SetProperty { name, value: to_json(&args.2) });
            Ok(args.2)
        });
        g.set("getTranslationPhrase", lua.create_function(|_, args: (String, Option<String>, Option<Vec<Value>>)| {
            let values: Vec<String> = args.2.unwrap_or_default().iter().map(|value| value.to_string().unwrap_or_default()).collect();
            Ok(funkin_core::lang::phrase_with(&args.0, args.1.as_deref().unwrap_or(&args.0), &values.iter().map(String::as_str).collect::<Vec<_>>()))
        })?)?;
        g.set("getFileTranslation", lua.create_function(|_, args: String| Ok(funkin_core::lang::phrase(&args, &args)))?)?;
        // where a script's sprite lies among the others: those behind the characters count from 0, then the three
        // characters (gfGroup, dadGroup, boyfriendGroup), then those in front
        // shortcut: a sprite cannot lie between two characters; one put there goes behind all three
        function!("getObjectOrder", |_lua, shared, args: (String, Option<String>)| {
            let shared = shared.borrow();
            let stage = |sprite: &Sprite| sprite.added && sprite.camera == Cam::Game;
            let behind = shared.sprites.iter().filter(|(_, sprite)| stage(sprite) && !sprite.front).count();
            let character = ["gfGroup", "dadGroup", "boyfriendGroup", "gf", "dad", "boyfriend"].iter().position(|name| *name == args.0).map(|at| behind + at % 3);
            let place = |front: bool, tag: &str| shared.sprites.iter().filter(|(_, sprite)| stage(sprite) && sprite.front == front).position(|(name, _)| name == tag);
            Ok(character.or_else(|| place(false, &args.0)).or_else(|| place(true, &args.0).map(|at| behind + 3 + at)).map_or(-1, |at| at as i64))
        });
        function!("setObjectOrder", |_lua, shared, args: (String, i64, Option<String>)| {
            let mut shared = shared.borrow_mut();
            let Some(from) = shared.sprites.iter().position(|(name, _)| *name == args.0) else {
                eprintln!("Lua: setObjectOrder: Object {} doesn't exist!", args.0);
                return Ok(());
            };
            let (name, mut sprite) = shared.sprites.remove(from);
            let stage = |sprite: &Sprite| sprite.added && sprite.camera == Cam::Game;
            let behind: Vec<usize> = shared.sprites.iter().enumerate().filter(|(_, (_, other))| stage(other) && !other.front).map(|(at, _)| at).collect();
            let front: Vec<usize> = shared.sprites.iter().enumerate().filter(|(_, (_, other))| stage(other) && other.front).map(|(at, _)| at).collect();
            let place = args.1.max(0) as usize;
            // its place in the list of all sprites: before the one that has that place now, or after the last of its side
            let count = shared.sprites.len();
            let to = if place < behind.len() + 3 {
                sprite.front = false;
                behind.get(place).copied().unwrap_or(behind.last().map_or(0, |last| last + 1))
            } else {
                sprite.front = true;
                front.get(place - behind.len() - 3).copied().unwrap_or(count)
            };
            shared.sprites.insert(to.min(count), (name, sprite));
            Ok(())
        });
        // mods' shaders (`shaders/<name>.frag`), on the scripts' sprites and the three characters
        // shortcut: on the game's own pictures other than the characters and the stage file's objects (the HUD's, the
        // stages the game draws in code) they are not taken. A character's shader
        // stays on its place when the character changes (the Haxe engine's goes with the old one), and a sprite that
        // is a plain rectangle (makeGraphic) is one dot to its shader (openfl_TextureSize 1 by 1).
        function!("initLuaShader", |_lua, shared, args: (String, Option<Value>)| {
            let mut shared = shared.borrow_mut();
            Ok(shared.shaders_on && shared.shader(&args.0).is_some())
        });
        function!("setSpriteShader", |_lua, shared, args: (String, String)| {
            let mut shared = shared.borrow_mut();
            if !shared.shaders_on {
                return Ok(false);
            }
            let Some(shader) = shared.shader(&args.1) else {
                shared.printed.push((format!("setSpriteShader: Shader {} is missing!", args.1), crate::gfx::get_time()));
                return Ok(false);
            };
            // shortcut: a camera by its name ("camGame", "camHUD", "camOther") takes a shader here; the Haxe engine's
            // scripts put one on a camera through HScript (camGame.setFilters), which cannot run here
            if let Some(slot) = camera_slot(&args.0) {
                shared.camera_shaders[slot] = Some(shader.fresh());
                return Ok(true);
            }
            // (an object of the stage's file, by its name there)
            let of_stage = shared.object_names.contains(&args.0);
            match (character_slot(&args.0), shared.sprite(&args.0)) {
                (Some(slot), _) => shared.character_shaders[slot] = Some(shader.fresh()),
                (None, Some(sprite)) => sprite.shader = Some(shader.fresh()),
                (None, None) if of_stage => drop(shared.object_shaders.insert(args.0.clone(), shader.fresh())),
                (None, None) => return Ok(false),
            }
            Ok(true)
        });
        function!("removeSpriteShader", |_lua, shared, args: String| {
            let mut shared = shared.borrow_mut();
            if let Some(slot) = camera_slot(&args) {
                return Ok(shared.camera_shaders[slot].take().is_some());
            }
            Ok(match (character_slot(&args), shared.sprite(&args)) {
                (Some(slot), _) => shared.character_shaders[slot].take().is_some(),
                (None, Some(sprite)) => sprite.shader.take().is_some(),
                (None, None) => shared.object_shaders.remove(&args).is_some(),
            })
        });
        // a uniform read: one number (or true/false), or all it holds
        for (name, list, flag) in [("getShaderFloat", false, false), ("getShaderInt", false, false), ("getShaderBool", false, true), ("getShaderFloatArray", true, false), ("getShaderIntArray", true, false), ("getShaderBoolArray", true, true)] {
            function!(name, |lua, shared, args: (String, String)| {
                let Some(numbers) = shared.borrow_mut().shader_of(&args.0).and_then(|shader| shader.get(&args.1)) else { return Ok(Value::Nil) };
                let value = |number: f64| if flag { serde_json::json!(number != 0.0) } else { serde_json::json!(number) };
                from_json(lua, &if list { numbers.into_iter().map(value).collect() } else { numbers.first().map_or(serde_json::Value::Null, |number| value(*number)) })
            });
        }
        // and set: a number, true or false, or a list of them
        for name in ["setShaderFloat", "setShaderInt", "setShaderBool", "setShaderFloatArray", "setShaderIntArray", "setShaderBoolArray"] {
            function!(name, |_lua, shared, args: (String, String, Value)| {
                let number = |value: &Value| match value {
                    Value::Boolean(flag) => Some(*flag as u8 as f64),
                    Value::Integer(whole) => Some(*whole as f64),
                    Value::Number(number) => Some(*number),
                    _ => None,
                };
                let numbers: Vec<f64> = match &args.2 {
                    Value::Table(list) => list.sequence_values::<Value>().flatten().filter_map(|value| number(&value)).collect(),
                    value => number(value).into_iter().collect(),
                };
                Ok(shared.borrow_mut().shader_of(&args.0).is_some_and(|shader| shader.set(&args.1, &numbers)))
            });
        }
        function!("setShaderSampler2D", |_lua, shared, args: (String, String, String)| {
            let mut shared = shared.borrow_mut();
            let Some(shader) = shared.shader_of(&args.0) else { return Ok(false) };
            Ok(match shared.art(&args.2, false) {
                Some(Art::Picture(picture)) => shader.set_picture(&args.1, &picture),
                _ => false,
            })
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
        // gamepads, any of them, by Flixel's names for their buttons (the first argument of gamepad*Pressed, the pad's number,
        // is taken as any pad)
        for (name, state) in [("anyGamepadJustPressed", crate::pad::pressed as fn(crate::pad::Button) -> bool), ("anyGamepadPressed", crate::pad::down), ("anyGamepadReleased", crate::pad::released)] {
            g.set(name, lua.create_function(move |_, args: String| Ok(crate::pad::named(&args).is_some_and(state)))?)?;
        }
        for (name, state) in [("gamepadJustPressed", crate::pad::pressed as fn(crate::pad::Button) -> bool), ("gamepadPressed", crate::pad::down), ("gamepadReleased", crate::pad::released)] {
            g.set(name, lua.create_function(move |_, args: (Value, String)| Ok(crate::pad::named(&args.1).is_some_and(state)))?)?;
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
            Request::Flash { camera, color: args.1.as_ref().map_or(0xFFFFFF, color_of), seconds: args.2.unwrap_or(1.0), rising: false }
        });
        // FlxCamera.fade: to the colour, staying (or, fading out, from it)
        request!("cameraFade", |args: (String, Option<Value>, Option<f64>, Option<bool>, Option<bool>)| {
            let camera = match args.0.to_lowercase().as_str() {
                "hud" | "camhud" => Cam::Hud,
                "other" | "camother" => Cam::Other,
                _ => Cam::Game,
            };
            Request::Flash { camera, color: args.1.as_ref().map_or(0x000000, color_of), seconds: args.2.unwrap_or(1.0), rising: !args.4.unwrap_or(false) }
        });
        request!("setHealthBarColors", |args: (Option<Value>, Option<Value>)| Request::BarColors { time: false, left: args.0.as_ref().map(color_of), right: args.1.as_ref().map(color_of) });
        request!("setTimeBarColors", |args: (Option<Value>, Option<Value>)| Request::BarColors { time: true, left: args.0.as_ref().map(color_of), right: args.1.as_ref().map(color_of) });
        // other scripts are reached once this one's call is over (what they answer is not passed back)
        for name in ["callOnLuas", "callOnScripts"] {
            request!(name, |args: (String, Option<Vec<Value>>)| Request::Call { name: args.0, args: args.1.unwrap_or_default().iter().map(to_json).collect() });
        }
        for name in ["setOnLuas", "setOnScripts"] {
            function!(name, |_lua, shared, args: (String, Value)| {
                shared.borrow_mut().requests.push(Request::SetOn { name: args.0, value: to_json(&args.1) });
                Ok(())
            });
        }
        function!("isRunning", |_lua, shared, args: String| Ok(shared.borrow().running.iter().any(|running| running.ends_with(&args) || running.ends_with(&format!("{args}.lua")))));
        function!("getRunningScripts", |_lua, shared, _args: ()| Ok(shared.borrow().running.clone()));
        // another script started now, with the globals the others have, and its onCreate called (as Haxe's new FunkinLua)
        let scripts = list.clone();
        function!("addLuaScript", |lua, shared, args: (String, Option<bool>)| {
            let found = shared.borrow().find_script(&args.0);
            let Some(file) = found else {
                eprintln!("addLuaScript: Script doesn't exist!");
                return Ok(());
            };
            if !args.1.unwrap_or(false) && shared.borrow().running.iter().any(|running| same_file(running, &file)) {
                eprintln!("addLuaScript: The script \"{file}\" is already running!");
                return Ok(());
            }
            let Some(list) = scripts.upgrade() else { return Ok(()) };
            let (mut globals, told) = { let shared = shared.borrow(); (shared.globals.clone(), shared.told.clone()) };
            // (what the game set as it went, as the calling script has it now)
            globals.extend(told.into_iter().filter_map(|key| Some((key.clone(), to_json(&lua.globals().get::<Value>(key.as_str()).ok()?)))));
            let globals: Vec<(&str, serde_json::Value)> = globals.iter().map(|(key, value)| (key.as_str(), value.clone())).collect();
            shared.borrow_mut().running.push(file.clone());
            match Scripts::open(&shared, &scripts, Path::new(&file), &globals, true) {
                Ok(script) => {
                    let script = Rc::new(script);
                    list.borrow_mut().push(script.clone());
                    if let Ok(Some(function)) = script.lua.globals().get::<Option<Function>>("onCreate") {
                        if let Err(error) = function.call::<Value>(()) {
                            eprintln!("Lua {file} (onCreate): {error}");
                        }
                    }
                }
                Err(error) => {
                    eprintln!("Lua {file}: {error}");
                    shared.borrow_mut().running.retain(|running| *running != file);
                }
            }
            Ok(())
        });
        let scripts = list.clone();
        function!("removeLuaScript", |_lua, shared, args: String| {
            let found = shared.borrow().find_script(&args);
            let list = scripts.upgrade().map(|list| list.borrow().clone()).unwrap_or_default();
            let mut any = false;
            for script in list.iter().filter(|script| found.as_ref().is_some_and(|file| same_file(&script.name, file))) {
                *script.closed.borrow_mut() = true;
                shared.borrow_mut().running.retain(|running| *running != script.name);
                any = true;
            }
            if !any {
                eprintln!("removeLuaScript: Script {args} isn't running!");
            }
            Ok(any)
        });
        // a function of another script called, and what it answered (Function_Continue for nothing)
        let scripts = list.clone();
        function!("callScript", |lua, shared, args: (String, String, Option<Vec<Value>>)| {
            let found = shared.borrow().find_script(&args.0);
            let list = scripts.upgrade().map(|list| list.borrow().clone()).unwrap_or_default();
            let Some(script) = list.iter().find(|script| found.as_ref().is_some_and(|file| same_file(&script.name, file))) else { return Ok(Value::Nil) };
            let function = script.lua.globals().get::<Option<Function>>(args.1.as_str()).ok().flatten();
            let Some(function) = function.filter(|_| !*script.closed.borrow()) else { return Ok(Value::String(lua.create_string(CONTINUE)?)) };
            let values: Vec<Value> = args.2.unwrap_or_default().iter().map(|value| from_json(&script.lua, &to_json(value))).collect::<mlua::Result<_>>()?;
            match function.call::<Value>(MultiValue::from_iter(values)) {
                Ok(Value::Nil) => Ok(Value::String(lua.create_string(CONTINUE)?)),
                Ok(answer) => from_json(lua, &to_json(&answer)),
                Err(error) => {
                    eprintln!("Lua {} ({}): {error}", script.name, args.1);
                    Ok(Value::String(lua.create_string(CONTINUE)?))
                }
            }
        });
        // what the rating shows (and its globals) until it is worked out again
        function!("setRatingPercent", |lua, shared, args: f64| {
            lua.globals().set("rating", args)?;
            shared.borrow_mut().requests.push(Request::Rating { percent: Some(args), name: None, fc: None });
            Ok(())
        });
        function!("setRatingName", |lua, shared, args: String| {
            lua.globals().set("ratingName", args.as_str())?;
            shared.borrow_mut().requests.push(Request::Rating { percent: None, name: Some(args), fc: None });
            Ok(())
        });
        function!("setRatingFC", |lua, shared, args: String| {
            lua.globals().set("ratingFC", args.as_str())?;
            shared.borrow_mut().requests.push(Request::Rating { percent: None, name: None, fc: Some(args) });
            Ok(())
        });
        // as Haxe's DiscordClient.changePresence: the end is given in ms after the start (or, without a start, after 1970)
        request!("changeDiscordPresence", |args: (Option<String>, Option<String>, Option<String>, Option<bool>, Option<f64>, Option<String>)| {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |since| since.as_millis() as f64);
            let start = if args.3.unwrap_or(false) { now } else { 0.0 };
            let end = args.4.filter(|end| *end > 0.0).map(|end| ((start + end) / 1000.0) as u64);
            Request::Presence(crate::discord::Presence { large_image: args.5, details: args.0.unwrap_or_else(|| "In the Menus".to_owned()), state: args.1.filter(|state| !state.is_empty()), small_image: args.2.filter(|key| !key.is_empty()), start: (start > 0.0).then_some((start / 1000.0) as u64), end })
        });
        // (none: the game's own application, as Discord.hx)
        request!("changeDiscordClientID", |args: Option<String>| Request::DiscordClient(args.unwrap_or_default()));
        request!("loadSong", |args: (Option<String>, Option<i64>)| Request::LoadSong { name: args.0.unwrap_or_default(), difficulty: args.1.unwrap_or(-1) });
        g.set("mouseReleased", lua.create_function(|_, _args: Option<String>| Ok(is_mouse_released()))?)?;
        g.set("gamepadAnalogX", lua.create_function(|_, args: (Option<u32>, Option<bool>)| Ok(crate::pad::stick(args.1.unwrap_or(true), true)))?)?;
        g.set("gamepadAnalogY", lua.create_function(|_, args: (Option<u32>, Option<bool>)| Ok(crate::pad::stick(args.1.unwrap_or(true), false)))?)?;
        // files a script writes or deletes: inside the game's mods folder only
        function!("saveFile", |_lua, shared, args: (String, String, Option<bool>)| Ok(shared.borrow().writable(&args.0).is_some_and(|path| {
            path.parent().is_some_and(|folder| std::fs::create_dir_all(folder).is_ok()) && std::fs::write(path, args.1).is_ok()
        })));
        function!("deleteFile", |_lua, shared, args: (String, Option<bool>, Option<bool>)| Ok(shared.borrow().writable(&args.0).is_some_and(|path| std::fs::remove_file(path).is_ok())));
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
            let value = strum(&args.0, args.1).and_then(|index| shared.borrow().props.get(&format!("strum{index}.{}", args.2)).cloned()).or_else(|| {
                // (a sprite of a group the scripts made)
                let mut shared = shared.borrow_mut();
                let tag = shared.groups.get(&args.0)?.members.get(args.1)?.clone();
                shared.sprite(&tag)?.get(&args.2)
            });
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
                None => {
                    let mut shared = shared.borrow_mut();
                    let (tag, value) = (shared.groups.get(&args.0).and_then(|group| group.members.get(args.1).cloned()), to_json(&args.3));
                    if !tag.is_some_and(|tag| shared.sprite(&tag).is_some_and(|sprite| sprite.set(&args.2, &value))) {
                        eprintln!("Lua: setPropertyFromGroup {} {} {} is not known here", args.0, args.1, args.2);
                    }
                }
            }
            Ok(())
        });

        // sprites: made, given pictures and animations, added in front of the characters or behind them, and moved about
        let make = |shared: &mut Shared, tag: String, image: Option<String>, x: Option<f64>, y: Option<f64>, animated: bool| {
            let image = image.filter(|image| !image.is_empty());
            let art = image.as_ref().and_then(|image| shared.art(image, animated));
            let mut sprite = Sprite::new(art, x.unwrap_or(0.0), y.unwrap_or(0.0));
            sprite.file = image.and_then(|image| shared.art_file(&image));
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
        // a new picture for a sprite: whole, or cut in a grid of frames for addAnimation (loadGraphic), or a sheet (loadFrames)
        function!("loadGraphic", |_lua, shared, args: (String, String, Option<f64>, Option<f64>)| {
            let mut shared = shared.borrow_mut();
            let art = shared.art(&args.1, false);
            let file = shared.art_file(&args.1);
            let (across, down) = (args.2.unwrap_or(0.0) as f32, args.3.unwrap_or(0.0) as f32);
            let art = match art {
                Some(Art::Picture(picture)) if across > 0.0 || down > 0.0 => {
                    let cell = vec2(if across > 0.0 { across } else { picture.width() }, if down > 0.0 { down } else { picture.height() });
                    let (columns, rows) = ((picture.width() / cell.x) as usize, (picture.height() / cell.y) as usize);
                    let frames = (0..rows * columns).map(|index| {
                        let source = Rect::new((index % columns) as f32 * cell.x, (index / columns) as f32 * cell.y, cell.x, cell.y);
                        (index.to_string(), Frame { source, offset: Vec2::ZERO, size: cell, rotated: false })
                    }).collect();
                    Some(Art::Sheet(Rc::new(Sheet { texture: picture, frames })))
                }
                art => art,
            };
            if let Some(sprite) = shared.sprite(&args.0) {
                (sprite.art, sprite.solid, sprite.playing, sprite.file) = (art, None, None, file);
                sprite.more.clear();
                sprite.animations.clear();
            }
            Ok(())
        });
        function!("loadFrames", |_lua, shared, args: (String, String, Option<String>)| {
            let mut shared = shared.borrow_mut();
            let art = shared.art(&args.1, true);
            let file = shared.art_file(&args.1);
            if let Some(sprite) = shared.sprite(&args.0) {
                (sprite.art, sprite.solid, sprite.playing, sprite.file) = (art, None, None, file);
                sprite.more.clear();
                sprite.animations.clear();
            }
            Ok(())
        });
        // several sheets for one sprite: its animations take their frames from the first sheet that has them
        function!("loadMultipleFrames", |_lua, shared, args: (String, Vec<String>)| {
            let mut shared = shared.borrow_mut();
            let mut sheets: Vec<Rc<Sheet>> = args.1.iter().filter_map(|image| match shared.art(image, true) {
                Some(Art::Sheet(sheet)) => Some(sheet),
                _ => None,
            }).collect();
            let file = args.1.first().and_then(|image| shared.art_file(image));
            if let (Some(sprite), false) = (shared.sprite(&args.0), sheets.is_empty()) {
                let first = sheets.remove(0);
                (sprite.art, sprite.solid, sprite.playing, sprite.file, sprite.more) = (Some(Art::Sheet(first)), None, None, file, sheets);
                sprite.animations.clear();
            }
            Ok(())
        });
        // a pixel of a sprite's picture, as 0xAARRGGBB (a whole number of 32 bits, as Haxe's FlxColor); black without a sprite
        function!("getPixelColor", |_lua, shared, args: (String, i64, i64)| {
            const BLACK: i64 = 0xFF000000u32 as i32 as i64;
            let mut shared = shared.borrow_mut();
            let Some(sprite) = shared.sprite(&args.0) else { return Ok(BLACK) };
            let argb = |a: u8, r: u8, g: u8, b: u8| u32::from_be_bytes([a, r, g, b]) as i32 as i64;
            let inside = |width: f64, height: f64| (0.0..width).contains(&(args.1 as f64)) && (0.0..height).contains(&(args.2 as f64));
            if let Some((size, rgb)) = sprite.solid {
                return Ok(if inside(size[0], size[1]) { (0xFF000000 | (rgb & 0xFFFFFF)) as i32 as i64 } else { 0 });
            }
            // shortcut: a text's pixels are not read; it answers as an object without pixels
            let Some(file) = sprite.file.clone() else { return Ok(BLACK) };
            let image = shared.pixels.entry(file.clone()).or_insert_with(|| std::fs::read(&file).ok().and_then(|bytes| Image::from_file_with_format(&bytes, None).ok()));
            Ok(match image {
                Some(image) if inside(image.width as f64, image.height as f64) => {
                    let at = (args.2 as usize * image.width as usize + args.1 as usize) * 4;
                    argb(image.bytes[at + 3], image.bytes[at], image.bytes[at + 1], image.bytes[at + 2])
                }
                _ => 0,
            })
        });
        function!("setBlendMode", |_lua, shared, args: (String, Option<String>)| {
            if let Some(sprite) = shared.borrow_mut().sprite(&args.0) {
                sprite.add = args.1.is_some_and(|blend| blend.trim().eq_ignore_ascii_case("add"));
            }
            Ok(())
        });
        function!("objectsOverlap", |_lua, shared, args: (String, String)| {
            let mut shared = shared.borrow_mut();
            let first = shared.sprite(&args.0).map(|sprite| sprite.bounds());
            let second = shared.sprite(&args.1).map(|sprite| sprite.bounds());
            Ok(first.zip(second).is_some_and(|(first, second)| first.overlaps(&second)))
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
            let mut frames = match prefix {
                Some(prefix) => sheet.animation(prefix, &numbers),
                None => numbers.iter().filter_map(|&number| sheet.frames.get(number as usize).map(|(_, frame)| *frame)).collect(),
            };
            // (of loadMultipleFrames' sheets, the first that has frames of that name)
            // shortcut: an animation is of one sheet; Haxe's may take frames from several
            let mut texture = None;
            if let (true, Some(prefix)) = (frames.is_empty(), prefix) {
                if let Some((sheet, found)) = sprite.more.iter().map(|sheet| (sheet, sheet.animation(prefix, &numbers))).find(|(_, found)| !found.is_empty()) {
                    (frames, texture) = (found, Some(sheet.texture.clone()));
                }
            }
            let first = sprite.animations.is_empty();
            sprite.animations.insert(name.clone(), Animation { frames, fps, looped, offset: [0.0; 2], texture });
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
        Scripts::register_more(shared, lua, &g, strum, note)?;
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

    /// The custom substate's functions (CustomSubstate.hx), the groups' and instances' (ReflectionFunctions.hx) and the
    /// atlas sprites' (FlxAnimateFunctions.hx).
    fn register_more(shared: &Rc<RefCell<Shared>>, lua: &Lua, g: &mlua::Table, strum: fn(&str, usize) -> Option<usize>, note: fn(&Shared, &str, usize) -> Option<usize>) -> mlua::Result<()> {
        macro_rules! function {
            ($name:expr, |$lua:ident, $shared:ident, $args:ident : $ty:ty| $body:expr) => {{
                let $shared = shared.clone();
                g.set($name, lua.create_function(move |$lua, $args: $ty| {
                    let _ = &$lua;
                    $body
                })?)?;
            }};
        }
        // the custom substate: opened and closed by the game the next moment, as Flixel opens one; until then (and once
        // it is gone) there is none to put sprites in or close
        function!("openCustomSubstate", |_lua, shared, args: (String, Option<bool>)| {
            let mut shared = shared.borrow_mut();
            shared.substate_name = Some(args.0.clone());
            shared.requests.push(Request::OpenSubstate { name: args.0, pause: args.1.unwrap_or(false) });
            Ok(())
        });
        function!("closeCustomSubstate", |_lua, shared, _args: ()| {
            let mut shared = shared.borrow_mut();
            let open = shared.substate.is_some();
            if open {
                shared.requests.push(Request::CloseSubstate);
            }
            Ok(open)
        });
        function!("insertToCustomSubstate", |_lua, shared, args: (String, Option<i64>)| {
            let mut shared = shared.borrow_mut();
            let shared = &mut *shared;
            let (Some(substate), true) = (shared.substate.as_mut(), shared.sprites.iter().any(|(name, _)| *name == args.0)) else { return Ok(false) };
            if !substate.members.contains(&args.0) {
                let at = usize::try_from(args.1.unwrap_or(-1)).map_or(substate.members.len(), |at| at.min(substate.members.len()));
                substate.members.insert(at, args.0);
            }
            Ok(true)
        });

        // groups: the game's (the notes, the strums) and those the scripts made (createInstance)
        const GAME_GROUPS: [&str; 5] = ["notes", "unspawnNotes", "strumLineNotes", "playerStrums", "opponentStrums"];
        let place = |members: &[String], index: Option<i64>| usize::try_from(index.unwrap_or(-1)).map_or(members.len(), |at| at.min(members.len()));
        function!("addToGroup", |lua, shared, args: (String, String, Option<i64>)| {
            let mut shared = shared.borrow_mut();
            if !shared.has(&args.1) {
                trace(lua, &mut shared, format!("addToGroup: Object {} is not valid!", args.1), false);
            } else if let Some(group) = shared.groups.get_mut(&args.0) {
                if !group.members.contains(&args.1) {
                    group.members.insert(place(&group.members, args.2), args.1);
                }
            } else if GAME_GROUPS.contains(&args.0.as_str()) {
                // shortcut: a sprite put in the notes' or the strums' group is drawn on the HUD over them, not among them
                if let Some(sprite) = shared.sprite(&args.1) {
                    (sprite.added, sprite.camera) = (true, Cam::Hud);
                }
            } else {
                trace(lua, &mut shared, format!("addToGroup: Group/Array {} is not valid!", args.0), false);
            }
            Ok(())
        });
        function!("removeFromGroup", |lua, shared, args: (String, Option<i64>, Option<String>, Option<bool>)| {
            let mut shared = shared.borrow_mut();
            let shared = &mut *shared;
            let (index, tag, destroy) = (args.1.unwrap_or(-1), args.2, args.3.unwrap_or(true));
            if let Some(tag) = tag.as_ref().filter(|tag| !shared.has(tag)) {
                trace(lua, shared, format!("removeFromGroup: Object {tag} is not valid!"), false);
                return Ok(());
            }
            if let Some(group) = shared.groups.get_mut(&args.0) {
                let at = match &tag {
                    Some(tag) => group.members.iter().position(|name| name == tag),
                    None => usize::try_from(index).ok().filter(|at| *at < group.members.len()),
                };
                let Some(at) = at else { return Ok(()) };
                let gone = group.members.remove(at);
                if destroy {
                    shared.sprites.retain(|(name, _)| *name != gone);
                }
            } else if GAME_GROUPS.contains(&args.0.as_str()) {
                match tag {
                    Some(tag) if destroy => shared.sprites.retain(|(name, _)| *name != tag),
                    Some(tag) => shared.sprite(&tag).iter_mut().for_each(|sprite| sprite.added = false),
                    // shortcut: a note taken out of its group is hidden and let pass (ignoreNote), a strum hidden
                    None => {
                        let index = usize::try_from(index).unwrap_or(usize::MAX);
                        if let Some(at) = note(shared, &args.0, index) {
                            for (field, value) in [("visible", false), ("ignoreNote", true)] {
                                shared.notes[at].fields.insert(field.to_owned(), serde_json::json!(value));
                                shared.requests.push(Request::Note { index: at, field: field.to_owned(), value: serde_json::json!(value) });
                            }
                        } else if let Some(at) = strum(&args.0, index) {
                            shared.requests.push(Request::SetProperty { name: format!("strum{at}.visible"), value: serde_json::json!(false) });
                        }
                    }
                }
            } else {
                trace(lua, shared, format!("removeFromGroup: Group/Array {} is not valid!", args.0), false);
            }
            Ok(())
        });
        function!("updateHitboxFromGroup", |lua, shared, args: (String, i64)| {
            let mut shared = shared.borrow_mut();
            let member = shared.groups.get(&args.0).and_then(|group| group.members.get(usize::try_from(args.1).ok()?).cloned());
            match (member, args.0.as_str()) {
                (Some(tag), _) => shared.sprite(&tag).iter_mut().for_each(|sprite| sprite.update_hitbox()),
                // shortcut: the notes and the strums keep their hitboxes; unspawnNotes (an array, not a group) says the
                // function is deprecated, as only the Haxe engine's array branch does
                (None, "unspawnNotes") => trace(lua, &mut shared, "updateHitboxFromGroup is deprecated! Use updateHitbox instead.".to_owned(), true),
                (None, group) if GAME_GROUPS.contains(&group) || shared.groups.contains_key(group) => {}
                (None, group) => trace(lua, &mut shared, format!("updateHitboxFromGroup: Group/Array {group} is not valid!"), false),
            }
            Ok(())
        });

        // instances: of the classes scripts build, one of the scripts' sprites, texts or groups
        // shortcut: the classes built here are Flixel's sprite, text and group ones and the engine's sprites; the Haxe
        // engine builds any of its classes
        function!("createInstance", |lua, shared, args: (String, String, Option<Vec<Value>>)| {
            let tag = args.0.trim().replace('.', "");
            let given = args.2.unwrap_or_default();
            let number = |at: usize, or: f64| match given.get(at) {
                Some(Value::Integer(number)) => *number as f64,
                Some(Value::Number(number)) => *number,
                _ => or,
            };
            let word = |at: usize| match given.get(at) {
                Some(Value::String(text)) => Some(text.to_string_lossy()).filter(|text| !text.is_empty()),
                _ => None,
            };
            let mut shared = shared.borrow_mut();
            if shared.has(&tag) || shared.groups.contains_key(&tag) || shared.vars.contains_key(&tag) {
                trace(lua, &mut shared, format!("createInstance: Variable {tag} is already being used and cannot be replaced!"), false);
                return Ok(Some(false));
            }
            // (each by the arguments of its constructor: FlxSprite(x, y, graphic), BGSprite(image, x, y, scrollX, scrollY),
            // FlxText(x, y, fieldWidth, text, size))
            let (image, mut sprite) = match args.1.rsplit('.').next().unwrap_or("") {
                "FlxSprite" | "FlxSkewedSprite" | "ModchartSprite" => (word(2), Sprite::new(None, number(0, 0.0), number(1, 0.0))),
                "BGSprite" => {
                    let mut sprite = Sprite::new(None, number(1, 0.0), number(2, 0.0));
                    sprite.scroll = [number(3, 1.0), number(4, 1.0)];
                    (word(0), sprite)
                }
                "FlxText" => {
                    let mut sprite = Sprite::new(None, number(0, 0.0), number(1, 0.0));
                    sprite.text = Some(Text { words: word(3).unwrap_or_default(), size: number(4, 8.0), width: number(2, 0.0), font: None, font_name: String::new(), align: 0.0, border: (0.0, 0) });
                    (None, sprite)
                }
                "FlxTypedGroup" | "FlxGroup" | "FlxSpriteGroup" | "FlxTypedSpriteGroup" => {
                    shared.groups.insert(tag, Group::default());
                    return Ok(Some(true));
                }
                _ => {
                    shared.say_once(format!("createInstance: {} is not known here", args.1));
                    return Ok(None);
                }
            };
            if let Some(image) = image {
                (sprite.art, sprite.file) = (shared.art(&image, false), shared.art_file(&image));
            }
            shared.sprites.push((tag, sprite));
            Ok(Some(true))
        });
        function!("addInstance", |lua, shared, args: (String, Option<bool>)| {
            let mut shared = shared.borrow_mut();
            let front = args.1.unwrap_or(false);
            if let Some(group) = shared.groups.get_mut(&args.0) {
                (group.added, group.front) = (true, front);
            } else if let Some(sprite) = shared.sprite(&args.0) {
                (sprite.added, sprite.front) = (true, front);
            } else {
                trace(lua, &mut shared, format!("addInstance: Can't add what doesn't exist~ ({})", args.0), false);
            }
            Ok(())
        });

        // Adobe Animate atlas sprites: drawn from a folder of images/ with an Animation.json and its spritemaps
        // shortcut: an atlas sprite has no size here (width and height read 0)
        let atlas = |shared: &Shared, folder: &str| -> Option<Rc<crate::animate::Atlas>> {
            let file = shared.paths.as_ref()?.find(&format!("images/{folder}/Animation.json"));
            match file.ok_or(format!("images/{folder}/Animation.json: not found")).and_then(|file| crate::animate::Atlas::load(file.parent().unwrap_or(Path::new(".")))) {
                Ok(atlas) => Some(Rc::new(atlas)),
                Err(error) => {
                    eprintln!("Lua: {error}");
                    None
                }
            }
        };
        function!("makeFlxAnimateSprite", |_lua, shared, args: (String, Option<f64>, Option<f64>, Option<String>)| {
            let tag = args.0.replace('.', "");
            let mut shared = shared.borrow_mut();
            let mut sprite = Sprite::new(None, args.1.unwrap_or(0.0), args.2.unwrap_or(0.0));
            sprite.atlas = args.3.and_then(|folder| atlas(&shared, &folder));
            shared.sprites.retain(|(name, _)| *name != tag);
            shared.sprites.push((tag, sprite));
            Ok(())
        });
        // shortcut: an atlas is read from its folder's Animation.json and spritemaps; other file names given are not used
        function!("loadAnimateAtlas", |_lua, shared, args: (String, String, Option<String>, Option<String>)| {
            let mut shared = shared.borrow_mut();
            if shared.has(&args.0) {
                let found = atlas(&shared, &args.1);
                shared.sprite(&args.0).iter_mut().for_each(|sprite| sprite.atlas = found.clone());
            }
            Ok(())
        });
        let by_symbol = |shared: &mut Shared, tag: &str, name: String, symbol: &str, numbers: Vec<usize>, more: (Option<f64>, Option<bool>, Option<f64>, Option<f64>)| -> bool {
            let Some(sprite) = shared.sprite(tag) else { return false };
            let Some(atlas) = &sprite.atlas else { return false };
            let symbol = atlas.rig.symbol_named(symbol).unwrap_or(symbol).to_owned();
            sprite.symbols.insert(name.clone(), Symbol { name: symbol, numbers, fps: more.0.unwrap_or(24.0), looped: more.1.unwrap_or(false), at: [more.2.unwrap_or(0.0), more.3.unwrap_or(0.0)] });
            // (the first one added plays at once)
            if sprite.playing.is_none() {
                sprite.playing = Some((name, 0.0));
            }
            true
        };
        function!("addAnimationBySymbol", |_lua, shared, args: (String, String, String, Option<f64>, Option<bool>, Option<f64>, Option<f64>)| {
            Ok(by_symbol(&mut shared.borrow_mut(), &args.0, args.1, &args.2, Vec::new(), (args.3, args.4, args.5, args.6)))
        });
        function!("addAnimationBySymbolIndices", |_lua, shared, args: (String, String, String, Option<Value>, Option<f64>, Option<bool>, Option<f64>, Option<f64>)| {
            let numbers = args.3.as_ref().map_or(vec![0], |value| indices(value).into_iter().map(|number| number as usize).collect());
            Ok(by_symbol(&mut shared.borrow_mut(), &args.0, args.1, &args.2, numbers, (args.4, args.5, args.6, args.7)))
        });
        Ok(())
    }

    /// Calls `name` in every script that has it, with `args`. What the last answered is returned (as the Haxe engine's
    /// callOnLuas), Function_Stop winning over the others. A script whose call fails is said so and goes on.
    pub fn call(&mut self, name: &str, args: impl mlua::IntoLuaMulti + Clone) -> Option<String> {
        let mut answer = None;
        let scripts = self.scripts.borrow().clone();
        // (a script another started had its onCreate then)
        for script in scripts.iter().filter(|script| !*script.closed.borrow() && !(script.added && name == "onCreate")) {
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
        self.scripts.borrow_mut().retain(|script| !*script.closed.borrow());
        answer
    }

    /// Calls `name` in every script that has it; the number the last of them answered with, if any did.
    pub fn call_number(&mut self, name: &str, args: impl mlua::IntoLuaMulti + Clone) -> Option<f64> {
        let mut answer = None;
        let scripts = self.scripts.borrow().clone();
        for script in scripts.iter().filter(|script| !*script.closed.borrow()) {
            let Ok(Some(function)) = script.lua.globals().get::<Option<Function>>(name) else { continue };
            match function.call::<Value>(args.clone()) {
                Ok(Value::Integer(whole)) => answer = Some(whole as f64),
                Ok(Value::Number(number)) => answer = Some(number),
                Ok(_) => {}
                Err(error) => eprintln!("Lua {} ({name}): {error}", script.name),
            }
        }
        answer
    }

    /// Sets a field of a sprite the scripts made, by "tag.field"; false if there is none of that tag or field.
    pub fn set_of_sprite(&self, name: &str, value: &serde_json::Value) -> bool {
        let mut shared = self.shared.borrow_mut();
        name.split_once('.').and_then(|(tag, field)| Some(shared.sprite(tag)?.set(field, value))).unwrap_or(false)
    }

    /// Whether `call` was answered with Function_Stop.
    pub fn stopped(answer: &Option<String>) -> bool {
        matches!(answer.as_deref(), Some(STOP | STOP_LUA | STOP_ALL))
    }

    /// Calls `name` in every script with values given as JSON (callOnLuas).
    pub fn call_json(&mut self, name: &str, args: &[serde_json::Value]) {
        let scripts = self.scripts.borrow().clone();
        for script in scripts.iter().filter(|script| !*script.closed.borrow()) {
            let Ok(Some(function)) = script.lua.globals().get::<Option<Function>>(name) else { continue };
            let values: mlua::Result<Vec<Value>> = args.iter().map(|value| from_json(&script.lua, value)).collect();
            if let Err(error) = values.and_then(|values| function.call::<Value>(mlua::MultiValue::from_iter(values))) {
                eprintln!("Lua {} ({name}): {error}", script.name);
            }
        }
    }

    /// Sets a global given as JSON in every script (setOnLuas).
    pub fn set_json(&self, name: &str, value: &serde_json::Value) {
        self.told(name);
        for script in self.scripts.borrow().iter() {
            if let Ok(value) = from_json(&script.lua, value) {
                let _ = script.lua.globals().set(name, value);
            }
        }
    }

    /// Sets a global in every script.
    pub fn set(&self, name: &str, value: impl mlua::IntoLua + Clone) {
        self.told(name);
        for script in self.scripts.borrow().iter() {
            let _ = script.lua.globals().set(name, value.clone());
        }
    }

    /// Runs the timers on by `dt` seconds, calling onTimerCompleted(tag, loops done, loops left) for each that rings.
    pub fn tick(&mut self, dt: f64) {
        let mut rung = Vec::new();
        // (a custom substate holding the game holds the timers and tweens that ran when it opened)
        // shortcut: the scripts' sprites all go on playing their animations under it
        let held = self.shared.borrow().substate.as_ref().map_or((Vec::new(), Vec::new()), |substate| (substate.held_timers.clone(), substate.held_tweens.clone()));
        {
            let mut shared = self.shared.borrow_mut();
            shared.timers.retain(|tag, (left, each, loops, done)| {
                if held.0.contains(tag) {
                    return true;
                }
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
            for (tag, mut tween) in tweens.into_iter().filter(|(tag, _)| !held.1.contains(tag)) {
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
    /// Those of a group the scripts added are drawn as the group was added; those in the custom substate, by `draw_substate`.
    pub fn draw(&self, camera: Cam, front: bool, scroll: Vec2) {
        let shared = self.shared.borrow();
        let within = |tag: &String| shared.substate.as_ref().is_some_and(|substate| substate.members.contains(tag));
        let shown = |tag: &String, sprite: &Sprite| match shared.groups.values().find(|group| group.added && group.members.contains(tag)) {
            Some(group) => Some(group.front),
            None => sprite.added.then_some(sprite.front),
        };
        for (_, sprite) in shared.sprites.iter().filter(|(tag, sprite)| !within(tag) && sprite.camera == camera && shown(tag, sprite).is_some_and(|in_front| camera != Cam::Game || in_front == front)) {
            sprite.draw(scroll);
        }
    }

    /// Draws the sprites put in the custom substate, in their order, over everything.
    /// shortcut: they are seen through the top camera, whatever camera they were given
    pub fn draw_substate(&self) {
        let shared = self.shared.borrow();
        let Some(substate) = &shared.substate else { return };
        for tag in &substate.members {
            if let Some((_, sprite)) = shared.sprites.iter().find(|(name, _)| name == tag) {
                sprite.draw(Vec2::ZERO);
            }
        }
    }

    /// The custom substate open: its name, and whether it holds the game still.
    pub fn substate(&self) -> Option<(String, bool)> {
        self.shared.borrow().substate.as_ref().map(|substate| (substate.name.clone(), substate.pause))
    }

    /// Opens the custom substate a script asked for (CustomSubstate.create), closing one open first as Flixel does.
    pub fn open_substate(&mut self, name: &str, pause: bool) {
        self.close_substate();
        {
            let mut shared = self.shared.borrow_mut();
            let (held_timers, held_tweens) = if pause { (shared.timers.keys().cloned().collect(), shared.tweens.keys().cloned().collect()) } else { Default::default() };
            shared.substate_name = Some(name.to_owned());
            shared.substate = Some(Substate { name: name.to_owned(), pause, members: Vec::new(), held_timers, held_tweens });
        }
        self.call("onCustomSubstateCreate", name.to_owned());
        self.call("onCustomSubstateCreatePost", name.to_owned());
    }

    /// A frame of the custom substate open, if any.
    pub fn update_substate(&mut self, dt: f32) {
        let Some((name, _)) = self.substate() else { return };
        self.call("onCustomSubstateUpdate", (name.clone(), dt));
        self.call("onCustomSubstateUpdatePost", (name, dt));
    }

    /// Closes the custom substate (its destroy): the sprites put in it go with it. Whether it held the game, if one was open.
    pub fn close_substate(&mut self) -> Option<bool> {
        let (name, _) = self.substate()?;
        self.call("onCustomSubstateDestroy", name);
        let mut shared = self.shared.borrow_mut();
        shared.substate_name = None;
        let substate = shared.substate.take()?;
        shared.sprites.retain(|(tag, _)| !substate.members.contains(tag));
        Some(substate.pause)
    }

    /// What the scripts asked for since the last time.
    pub fn requests(&self) -> Vec<Request> {
        std::mem::take(&mut self.shared.borrow_mut().requests)
    }

    pub fn is_empty(&self) -> bool {
        self.scripts.borrow().is_empty()
    }

    /// A global the game set, which a script started later gets too.
    fn told(&self, name: &str) {
        let mut shared = self.shared.borrow_mut();
        if !shared.told.iter().any(|told| told == name) {
            shared.told.push(name.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn scripts_write_only_inside_the_mods_folder() {
        let base = Path::new("game");
        let mut shared = Shared::new(Default::default(), "", funkin_core::content::Paths::new(base, "", ""));
        assert_eq!(shared.writable("a/b.txt"), Some(base.join("mods").join("a/b.txt")));
        assert_eq!(shared.writable("../x.txt"), None);
        assert_eq!(shared.writable("a/../../x.txt"), None);
        assert_eq!(shared.writable("/etc/x"), None);
        shared.mod_folder = "Mine".to_owned();
        assert_eq!(shared.writable("a.txt"), Some(base.join("mods").join("Mine").join("a.txt")));
        assert_eq!(shared.writable("mods/a.txt"), Some(base.join("mods").join("a.txt")));
    }

    use super::*;

    fn run(code: &str) -> Scripts {
        let file = std::env::temp_dir().join(format!("funkin-lua-test-{}-{}.lua", std::process::id(), code.len()));
        std::fs::write(&file, code).unwrap();
        let scripts = Scripts::start(&[file.clone()], &[("songName", serde_json::json!("test"))], Shared::default());
        let _ = std::fs::remove_file(file);
        scripts
    }

    #[test]
    fn a_custom_substate_is_opened_by_the_game_and_holds_the_timers_running() {
        let mut scripts = run(r#"
            function onCreate()
                makeLuaSprite('box'); runTimer('before', 1)
                openCustomSubstate('menu', true)
                setVar('early', insertToCustomSubstate('box'))
            end
            function onCustomSubstateCreate(name)
                setVar('inside', insertToCustomSubstate('box')); runTimer('within', 1)
                setVar('named', getPropertyFromClass('psychlua.CustomSubstate', 'name'))
            end
            function onCustomSubstateUpdate(name, elapsed) setVar('updated', name) end
            function onCustomSubstateDestroy(name) setVar('gone', name) end
            function onTimerCompleted(tag) setVar('rang', (getVar('rang') or '') .. tag) end
        "#);
        scripts.call("onCreate", ());
        assert_eq!(scripts.requests(), vec![Request::OpenSubstate { name: "menu".to_owned(), pause: true }]);
        scripts.open_substate("menu", true);
        scripts.tick(1.5);
        scripts.update_substate(0.1);
        let var = |scripts: &Scripts, name: &str| scripts.shared.borrow().vars.get(name).cloned().unwrap_or_default();
        assert_eq!([var(&scripts, "early"), var(&scripts, "inside"), var(&scripts, "named"), var(&scripts, "updated"), var(&scripts, "rang")], [serde_json::json!(false), serde_json::json!(true), serde_json::json!("menu"), serde_json::json!("menu"), serde_json::json!("within")]);
        assert_eq!(scripts.close_substate(), Some(true));
        assert!(!scripts.shared.borrow().has("box"));
        scripts.tick(1.0);
        assert_eq!([var(&scripts, "gone"), var(&scripts, "rang")], [serde_json::json!("menu"), serde_json::json!("withinbefore")]);
    }

    #[test]
    fn groups_made_by_scripts_hold_their_sprites() {
        let scripts = run(r#"
            createInstance('crowd', 'flixel.group.FlxTypedGroup')
            createInstance('a.b', 'flixel.FlxSprite', {10, 20})
            setVar('again', createInstance('ab', 'flixel.FlxSprite'))
            createInstance('c', 'flixel.text.FlxText', {0, 0, 100, 'hi', 12})
            addToGroup('crowd', 'ab')
            callMethod('crowd.insert', {0, instanceArg('c')})
            setPropertyFromGroup('crowd', 1, 'x', 5)
            setVar('x', getPropertyFromGroup('crowd', 1, 'x')); setVar('y', getPropertyFromGroup('crowd', 1, 'y'))
            removeFromGroup('crowd', 0)
            removeFromGroup('crowd', nil, 'ab', false)
        "#);
        let shared = scripts.shared.borrow();
        assert_eq!([&shared.vars["again"], &shared.vars["x"], &shared.vars["y"]], [&serde_json::json!(false), &serde_json::json!(5.0), &serde_json::json!(20.0)]);
        assert!(shared.groups["crowd"].members.is_empty() && shared.has("ab") && !shared.has("c"));
    }

    #[test]
    fn a_symbol_plays_its_frames_or_those_numbered() {
        assert_eq!([0.0, 0.5, 1.0, 2.0].map(|time| symbol_frame(&[], 10, 10.0, false, time)), [0, 5, 9, 9]);
        assert_eq!([0.0, 0.25, 0.35].map(|time| symbol_frame(&[4, 2, 7], 10, 10.0, true, time)), [4, 7, 4]);
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
        assert_eq!(scripts.requests(), [Request::Sound { name: "confirmMenu".to_owned(), volume: 0.5, tag: None, looped: false }, Request::SetHealth(1.5)]);
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
    fn sprites_are_put_in_order_among_each_other_and_the_characters() {
        let scripts = run("for _, tag in ipairs({'a', 'b', 'c'}) do makeLuaSprite(tag, nil, 0, 0); addLuaSprite(tag, tag == 'c') end
            before = {getObjectOrder('a'), getObjectOrder('b'), getObjectOrder('gfGroup'), getObjectOrder('boyfriend'), getObjectOrder('c'), getObjectOrder('none')}
            setObjectOrder('a', getObjectOrder('boyfriendGroup') + 1); setObjectOrder('c', 0); setObjectOrder('b', getObjectOrder('dadGroup'))
            setVar('orders', {before, {getObjectOrder('c'), getObjectOrder('b'), getObjectOrder('a')}})");
        // (b, asked between two characters, stays behind them all: the last of those behind)
        assert_eq!(scripts.shared.borrow().vars["orders"], serde_json::json!([[0, 1, 2, 4, 5, -1], [0, 1, 5]]));
        let shared = scripts.shared.borrow();
        assert_eq!(shared.sprites.iter().map(|(name, sprite)| (name.as_str(), sprite.front)).collect::<Vec<_>>(), [("c", false), ("b", false), ("a", true)]);
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

    #[test]
    fn hscript_functions_say_once_they_cannot_run() {
        let mut scripts = run("local said = {}; function debugPrint(line) said[#said + 1] = line end; a = runHaxeCode('x'); runHaxeCode('y'); addHScript('z')
            function result() return tostring(a) .. '|' .. table.concat(said, '|') end");
        assert_eq!(scripts.call("result", ()).as_deref(), Some("nil|runHaxeCode: HScript is not supported here|addHScript: HScript is not supported here"));
    }

    #[test]
    fn a_script_starts_another_calls_it_and_stops_it() {
        let other = std::env::temp_dir().join(format!("funkin-lua-other-{}.lua", std::process::id()));
        std::fs::write(&other, "function onCreate() created = songName end\nfunction double(x) return x * 2 end\nfunction tell() return created end").unwrap();
        let path = other.to_string_lossy().trim_end_matches(".lua").to_owned();
        let mut scripts = run(&format!(r#"
            addLuaScript('{path}'); addLuaScript('{path}')
            local answer, made, count, running = callScript('{path}', 'double', {{21}}), callScript('{path}', 'tell'), #getRunningScripts(), isRunning('{path}')
            local removed, again = removeLuaScript('{path}'), removeLuaScript('{path}')
            function result() return table.concat({{answer, made, count, tostring(running), tostring(removed), tostring(again), tostring(isRunning('{path}'))}}, ',') end
        "#));
        // (the one started by another had its onCreate when it started, not again; as Haxe's, a script removed is found
        // again by removeLuaScript until the scripts are next called)
        scripts.call("onCreate", ());
        assert_eq!(scripts.call("result", ()).as_deref(), Some("42,test,2,true,true,true,false"));
        let _ = std::fs::remove_file(other);
    }

    #[test]
    fn screen_positions_and_pixels_of_sprites() {
        let mut scripts = run(r#"
            makeLuaSprite('a', '', 100, 50); setProperty('a.scrollFactor.x', 0.5)
            makeLuaSprite('b'); makeGraphic('b', 4, 4, 'FF0000')
            function result() return table.concat({getScreenPositionX('a'), getScreenPositionX('a', 'hud'), getScreenPositionY('a'), getPixelColor('b', 1, 1), getPixelColor('b', 9, 1), getPixelColor('nothing', 0, 0)}, ',') end
        "#);
        scripts.shared.borrow_mut().props.extend([("camGame.scroll.x".to_owned(), serde_json::json!(40.0)), ("camGame.scroll.y".to_owned(), serde_json::json!(10.0))]);
        assert_eq!(scripts.call("result", ()).as_deref(), Some("80.0,100.0,40.0,-65536,0,-16777216"));
    }

    #[test]
    fn a_script_sets_the_rating_shown() {
        let scripts = run("setRatingName('Great'); setRatingPercent(0.5); seen = ratingName");
        let requests = scripts.requests();
        assert_eq!(requests, [Request::Rating { percent: None, name: Some("Great".to_owned()), fc: None }, Request::Rating { percent: Some(0.5), name: None, fc: None }]);
    }
}
