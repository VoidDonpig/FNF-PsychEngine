//! What is behind the notes: the stage, the three characters and the camera that follows whoever sings.
use crate::atlas::{Frame, Sheet};
use funkin_core::chart::{Chart, ChartEvent, NoteKind};
use funkin_core::content::Paths;
use funkin_core::play::Event;
use macroquad::prelude::*;
use serde_json::Value;
use std::collections::HashMap;

const SING: [&str; 4] = ["singLEFT", "singDOWN", "singUP", "singRIGHT"];
const SCREEN: Vec2 = vec2(1280.0, 720.0);

#[derive(Clone, Copy, PartialEq)]
enum Size {
    Times(f32),
    /// Stretched to this width in pixels.
    Wide(f32),
}

#[derive(Clone, Copy, PartialEq)]
enum Motion {
    /// Shows the first frame.
    Still,
    Loop,
    /// Plays once on every beat.
    Beat,
    /// Alternates on every beat between the first so many frames and the rest (dancers stepping left and right).
    Sway(usize),
    /// Rests on its last frame until the stage plays it through once.
    Once,
}

/// One picture of a stage's backdrop.
#[derive(Clone, Copy)]
struct LayerDef {
    image: &'static str,
    at: Vec2,
    /// How much of the camera's movement the layer follows, per axis: less than 1 looks further away.
    scroll: Vec2,
    size: Size,
    flip: bool,
    /// Animation: frame name prefix ("" for a sheet's frames in file order) and how it plays.
    anim: Option<(&'static str, Motion)>,
    fps: f32,
    /// Drawn over the characters.
    front: bool,
    /// Pixel art keeps its hard edges.
    pixel: bool,
    /// Grows around its middle instead of from its top left corner.
    centered: bool,
}

fn layer(image: &'static str, x: f32, y: f32, scroll_x: f32, scroll_y: f32) -> LayerDef {
    LayerDef { image, at: vec2(x, y), scroll: vec2(scroll_x, scroll_y), size: Size::Times(1.0), flip: false, anim: None, fps: 24.0, front: false, pixel: false, centered: false }
}

/// The backdrop of a stage, back to front. The numbers are those of source/states/stages.
// shortcut: the running soldiers, the crowds' reactions to events, the limo's henchmen and the weekend 1 lighting are not here yet. What `Own` moves is.
fn backdrop(stage: &str) -> Vec<LayerDef> {
    use Motion::*;
    use Size::*;
    let sized = |size: Size, def: LayerDef| LayerDef { size, ..def };
    let moving = |prefix: &'static str, motion: Motion, def: LayerDef| LayerDef { anim: Some((prefix, motion)), ..def };
    let front = |def: LayerDef| LayerDef { front: true, ..def };
    let pixel = |def: LayerDef| LayerDef { pixel: true, ..def };
    // pixel stages draw their 311 pixel wide pictures six times as large
    let wide = 311.0 * 6.0;
    match stage {
        "stage" => vec![
            layer("stageback", -600.0, -200.0, 0.9, 0.9),
            sized(Times(1.1), layer("stagefront", -650.0, 600.0, 0.9, 0.9)),
            sized(Times(1.1), layer("stage_light", -125.0, -100.0, 0.9, 0.9)),
            LayerDef { flip: true, ..sized(Times(1.1), layer("stage_light", 1225.0, -100.0, 0.9, 0.9)) },
            sized(Times(0.9), layer("stagecurtains", -500.0, -300.0, 1.3, 1.3)),
        ],
        // the strike ends on the picture of the quiet house
        "spooky" => vec![moving("halloweem bg lightning strike", Once, layer("halloween_bg", -200.0, -100.0, 1.0, 1.0))],
        "philly" => vec![
            layer("philly/sky", -100.0, 0.0, 0.1, 0.1),
            sized(Times(0.85), layer("philly/city", -10.0, 0.0, 0.3, 0.3)),
            sized(Times(0.85), layer("philly/window", -10.0, 0.0, 0.3, 0.3)),
            layer("philly/behindTrain", -40.0, 50.0, 1.0, 1.0),
            layer("philly/train", 2000.0, 360.0, 1.0, 1.0),
            layer("philly/street", -40.0, 50.0, 1.0, 1.0),
        ],
        "limo" => {
            let mut layers = vec![
                layer("limo/limoSunset", -120.0, -50.0, 0.1, 0.1),
                moving("background limo pink", Loop, layer("limo/bgLimo", -150.0, 480.0, 0.4, 0.4)),
            ];
            layers.extend((0..5).map(|i| moving("bg dancer sketch PINK", Sway(15), layer("limo/limoDancer", 370.0 * i as f32 + 170.0, 80.0, 0.4, 0.4))));
            layers.push(layer("limo/fastCarLol", -12600.0, 160.0, 1.0, 1.0));
            layers.push(moving("Limo stage", Loop, layer("limo/limoDrive", -120.0, 550.0, 1.0, 1.0)));
            layers
        }
        "mall" => vec![
            sized(Times(0.8), layer("christmas/bgWalls", -1000.0, -500.0, 0.2, 0.2)),
            sized(Times(0.85), moving("Upper Crowd Bob", Beat, layer("christmas/upperBop", -240.0, -90.0, 0.33, 0.33))),
            sized(Times(0.9), layer("christmas/bgEscalator", -1100.0, -600.0, 0.3, 0.3)),
            layer("christmas/christmasTree", 370.0, -250.0, 0.4, 0.4),
            moving("Bottom Level Boppers Idle", Beat, layer("christmas/bottomBop", -300.0, 140.0, 0.9, 0.9)),
            layer("christmas/fgSnow", -600.0, 700.0, 1.0, 1.0),
            moving("santa idle in fear", Beat, layer("christmas/santa", -840.0, 150.0, 1.0, 1.0)),
        ],
        "mallEvil" => vec![
            sized(Times(0.8), layer("christmas/evilBG", -400.0, -500.0, 0.2, 0.2)),
            layer("christmas/evilTree", 300.0, -300.0, 0.2, 0.2),
            layer("christmas/evilSnow", -200.0, 700.0, 1.0, 1.0),
        ],
        "school" => vec![
            pixel(sized(Wide(wide), layer("weeb/weebSky", 0.0, 0.0, 0.1, 0.1))),
            pixel(sized(Wide(wide), layer("weeb/weebSchool", -200.0, 0.0, 0.6, 0.9))),
            pixel(sized(Wide(wide), layer("weeb/weebStreet", -200.0, 0.0, 0.95, 0.95))),
            pixel(sized(Wide(wide * 0.8), layer("weeb/weebTreesBack", -30.0, 130.0, 0.9, 0.9))),
            LayerDef { fps: 12.0, ..pixel(sized(Wide(wide * 1.4), moving("", Loop, layer("weeb/weebTrees", -580.0, -800.0, 0.85, 0.85)))) },
            pixel(sized(Wide(wide), moving("PETALS ALL", Loop, layer("weeb/petals", -200.0, -40.0, 0.85, 0.85)))),
            pixel(sized(Times(6.0), moving("BG girls group", Sway(15), layer("weeb/bgFreaks", -100.0, 190.0, 0.9, 0.9)))),
        ],
        "schoolEvil" => vec![LayerDef { centered: true, ..pixel(sized(Times(6.0), moving("background 2", Loop, layer("weeb/animatedEvilSchool", 400.0, 200.0, 0.8, 0.9)))) }],
        "tank" => vec![
            layer("tankSky", -400.0, -400.0, 0.0, 0.0),
            layer("tankClouds", -400.0, 0.0, 0.1, 0.1),
            sized(Times(1.2), layer("tankMountains", -300.0, -20.0, 0.2, 0.2)),
            sized(Times(1.1), layer("tankBuildings", -200.0, 0.0, 0.3, 0.3)),
            sized(Times(1.1), layer("tankRuins", -200.0, 0.0, 0.35, 0.35)),
            moving("SmokeBlurLeft", Loop, layer("smokeLeft", -200.0, -100.0, 0.4, 0.4)),
            moving("SmokeRight", Loop, layer("smokeRight", 1100.0, -100.0, 0.4, 0.4)),
            moving("watchtower gradient color", Beat, layer("tankWatchtower", 100.0, 50.0, 0.5, 0.5)),
            moving("BG tank w lighting", Loop, layer("tankRolling", 0.0, 0.0, 0.5, 0.5)),
            sized(Times(1.15), layer("tankGround", -420.0, -150.0, 1.0, 1.0)),
            front(moving("fg", Beat, layer("tank0", -500.0, 650.0, 1.7, 1.5))),
            front(moving("fg", Beat, layer("tank1", -300.0, 750.0, 2.0, 0.2))),
            front(moving("foreground", Beat, layer("tank2", 450.0, 940.0, 1.5, 1.5))),
            front(moving("fg", Beat, layer("tank4", 1300.0, 900.0, 1.5, 1.5))),
            front(moving("fg", Beat, layer("tank5", 1620.0, 700.0, 1.5, 1.5))),
            front(moving("fg", Beat, layer("tank3", 1300.0, 1200.0, 3.5, 2.5))),
        ],
        "phillyStreets" => vec![
            sized(Times(0.65), layer("phillyStreets/phillySkybox", -650.0, -375.0, 0.1, 0.1)),
            layer("phillyStreets/phillySkyline", -545.0, -273.0, 0.2, 0.2),
            layer("phillyStreets/phillyForegroundCity", 625.0, 94.0, 0.3, 0.3),
            layer("phillyStreets/phillyConstruction", 1800.0, 364.0, 0.7, 1.0),
            layer("phillyStreets/phillyHighwayLights", 284.0, 305.0, 1.0, 1.0),
            layer("phillyStreets/phillyHighway", 139.0, 209.0, 1.0, 1.0),
            layer("phillyStreets/phillySmog", -6.0, 245.0, 0.8, 1.0),
            moving("redtogreen", Still, layer("phillyStreets/phillyTraffic", 1840.0, 608.0, 0.9, 1.0)),
            layer("phillyStreets/phillyForeground", 88.0, 317.0, 1.0, 1.0),
        ],
        _ => Vec::new(),
    }
}

/// A backdrop picture, loaded.
struct Layer {
    def: LayerDef,
    texture: Texture2D,
    /// Its animation's frames; empty for a plain picture.
    frames: Vec<Frame>,
    /// Seconds into the animation.
    time: f32,
    /// Sway: on the second half of its frames.
    swayed: bool,
    tint: Color,
    /// Turned around its middle, in radians.
    angle: f32,
}

/// The train of the philly stage: it waits off screen to the right, and when its sound has played for a while its cars rush past.
struct Train {
    x: f32,
    /// Seconds since its sound began, while it passes.
    passing: Option<f32>,
    cars: i32,
    /// It moves in steps, 24 a second at most.
    tick: f32,
    /// Beats since it last left.
    cooldown: i32,
}

#[derive(PartialEq, Debug)]
enum TrainNow {
    Quiet,
    /// It is rushing past: hair blows.
    Rushing,
    /// It has just gone.
    Gone,
}

impl Train {
    fn new() -> Train {
        Train { x: 2000.0, passing: None, cars: 8, tick: 0.0, cooldown: 0 }
    }

    fn step(&mut self, dt: f32) -> TrainNow {
        let Some(since) = self.passing.as_mut() else { return TrainNow::Quiet };
        *since += dt;
        self.tick += dt;
        if self.tick < 1.0 / 24.0 || *since < 4.7 {
            return TrainNow::Quiet;
        }
        self.tick = 0.0;
        self.x -= 400.0;
        if self.x < -2000.0 && self.cars > 0 {
            // the same picture comes round again as the next car
            self.x = -1150.0;
            self.cars -= 1;
        }
        if self.x < -4000.0 && self.cars <= 0 {
            *self = Train { x: SCREEN.x + 200.0, ..Train::new() };
            return TrainNow::Gone;
        }
        TrainNow::Rushing
    }
}

/// What a stage moves by its own rules, as its class in source/states/stages does.
enum Own {
    Nothing,
    /// Lightning: the beat of the last strike, the beats to wait after it, and the seconds since it.
    Spooky { struck: i64, wait: i64, since: f32 },
    /// The colour of the window lights, and the train.
    Philly { light: usize, train: Train },
    /// The car speeding past: its speed (px/s) and the seconds until it is back at its start.
    Limo { speed: f32, back_in: Option<f32> },
    /// The tank rolling round behind the ground: where on its circle it is (degrees) and how fast it goes.
    Tank { angle: f32, speed: f32 },
}

const WINDOW_LIGHTS: [u32; 5] = [0x31A2FD, 0x31FD8C, 0xFB33F5, 0xFD4531, 0xFBA633];

fn chance(share: f32) -> bool {
    rand::gen_range(0.0, 1.0) < share
}

fn find<'a>(backdrop: &'a mut [Layer], image: &str) -> Option<&'a mut Layer> {
    backdrop.iter_mut().find(|layer| layer.def.image == image)
}

impl Layer {
    fn load(paths: &Paths, def: LayerDef) -> Option<Layer> {
        let image = paths.find(&format!("images/{}.png", def.image))?;
        let (texture, frames) = match def.anim {
            Some((prefix, _)) => {
                let sheet = Sheet::load(&image).map_err(|e| eprintln!("Stage: {e}")).ok()?;
                let frames = sheet.animation(prefix, &[]);
                (sheet.texture, frames)
            }
            None => (Texture2D::from_file_with_format(&std::fs::read(image).ok()?, None), Vec::new()),
        };
        if def.pixel {
            texture.set_filter(FilterMode::Nearest);
        }
        let time = if matches!(def.anim, Some((_, Motion::Once))) { 1e9 } else { 0.0 };
        Some(Layer { def, texture, frames, time, swayed: false, tint: WHITE, angle: 0.0 })
    }

    fn beat(&mut self) {
        match self.def.anim {
            Some((_, Motion::Beat)) => self.time = 0.0,
            Some((_, Motion::Sway(_))) => {
                self.swayed = !self.swayed;
                self.time = 0.0;
            }
            _ => {}
        }
    }

    /// `scroll`: the camera's top left corner in stage coordinates.
    fn draw(&self, scroll: Vec2) {
        let step = (self.time * self.def.fps) as usize;
        let count = self.frames.len();
        let frame = match self.def.anim {
            _ if count == 0 => None,
            Some((_, Motion::Loop)) => Some(step % count),
            Some((_, Motion::Sway(half))) => {
                let (first, len) = if self.swayed { (half.min(count - 1), count.saturating_sub(half).max(1)) } else { (0, half.min(count)) };
                Some(first + step.min(len - 1))
            }
            Some((_, Motion::Beat | Motion::Once)) => Some(step.min(count - 1)),
            _ => Some(0),
        };
        let size = frame.map_or(self.texture.size(), |f| self.frames[f].size);
        let scale = match self.def.size {
            Size::Times(times) => times,
            Size::Wide(width) => width / size.x,
        };
        let mut at = self.def.at + scroll * (Vec2::ONE - self.def.scroll);
        if self.def.centered {
            at -= size * (scale - 1.0) / 2.0;
        }
        if self.tint.a <= 0.0 {
            return;
        }
        // a turned layer is drawn upright with everything turned around its middle
        let gl = unsafe { get_internal_gl() }.quad_gl;
        if self.angle != 0.0 {
            let middle = (at + size * scale / 2.0).extend(0.0);
            gl.push_model_matrix(Mat4::from_translation(middle) * Mat4::from_rotation_z(self.angle) * Mat4::from_translation(-middle));
        }
        match frame {
            Some(f) => crate::atlas::draw_frame(&self.texture, self.frames[f], at.x, at.y, scale, self.def.flip, self.tint),
            None => draw_texture_ex(&self.texture, at.x, at.y, self.tint, DrawTextureParams { dest_size: Some(size * scale), flip_x: self.def.flip, ..Default::default() }),
        }
        if self.angle != 0.0 {
            gl.pop_model_matrix();
        }
    }
}

fn pair(value: &Value) -> Vec2 {
    vec2(value[0].as_f64().unwrap_or(0.0) as f32, value[1].as_f64().unwrap_or(0.0) as f32)
}

fn read_json(paths: &Paths, relative: &str) -> Result<Value, String> {
    let path = paths.find(relative).ok_or(format!("{relative} not found"))?;
    serde_json::from_str(std::fs::read_to_string(&path).map_err(|e| e.to_string())?.trim_start_matches('\u{feff}')).map_err(|e| format!("{}: {e}", path.display()))
}

fn texture(paths: &Paths, relative: &str) -> Option<Texture2D> {
    Some(Texture2D::from_file_with_format(&std::fs::read(paths.find(relative)?).ok()?, None))
}

struct Animation {
    frames: Vec<Frame>,
    fps: f32,
    looped: bool,
    offset: Vec2,
}

pub struct Character {
    sheet: Sheet,
    animations: HashMap<String, Animation>,
    position: Vec2,
    scale: f32,
    flip_x: bool,
    /// Size of the pose the character stands in, for finding its middle.
    size: Vec2,
    /// A character drawn larger or smaller grows around the middle of its first frame, not from its corner
    /// (the Haxe engine's sprites do, and character positions are written for that): this is that shift.
    grow: Vec2,
    camera_offset: Vec2,
    /// Sing poses are held for this many 16th notes after the last note.
    sing_steps: f32,
    current: String,
    /// Seconds into the current animation.
    time: f32,
    /// Seconds since the last note sung.
    since_note: f32,
    /// Sways left and right on every beat instead of bopping on every other one.
    sways: bool,
    swayed_right: bool,
    /// Added to the idle move's name ("-alt"), set by a chart event.
    idle_suffix: String,
    /// A pose a chart asked for is showing: seconds until the idle move may take over (infinite = until the pose ends).
    special: Option<f32>,
    pub icon: Option<Texture2D>,
    pub color: Color,
}

impl Character {
    /// Loads characters/<name>.json and its sheet. `spot` is where the stage puts this character.
    fn load(paths: &Paths, name: &str, spot: Vec2, is_player: bool) -> Result<Character, String> {
        let json = read_json(paths, &format!("characters/{name}.json"))?;
        // shortcut: a character drawn from several sheets ("a,b") uses the first only; texture-atlas (Animate) characters are not read. Extend Sheet for them.
        let image = json["image"].as_str().unwrap_or("").split(',').next().unwrap_or("").trim().to_owned();
        let sheet = Sheet::load(&paths.find(&format!("images/{image}.png")).ok_or(format!("images/{image}.png not found"))?)?;
        let mut animations = HashMap::new();
        for animation in json["animations"].as_array().into_iter().flatten() {
            let indices: Vec<u32> = animation["indices"].as_array().into_iter().flatten().filter_map(|i| i.as_u64()).map(|i| i as u32).collect();
            let frames = sheet.animation(animation["name"].as_str().unwrap_or(""), &indices);
            if !frames.is_empty() {
                let fps = animation["fps"].as_f64().unwrap_or(24.0) as f32;
                animations.insert(animation["anim"].as_str().unwrap_or("").to_owned(), Animation { frames, fps, looped: animation["loop"].as_bool().unwrap_or(false), offset: pair(&animation["offsets"]) });
            }
        }
        let sways = animations.contains_key("danceLeft") && animations.contains_key("danceRight");
        let current = if sways { "danceRight" } else { "idle" }.to_owned();
        let scale = json["scale"].as_f64().unwrap_or(1.0) as f32;
        let frame_size = animations.get(&current).or(animations.values().next()).map_or(Vec2::ZERO, |a| a.frames[0].size);
        let size = frame_size * scale;
        if json["no_antialiasing"].as_bool().unwrap_or(false) {
            sheet.texture.set_filter(FilterMode::Nearest); // pixel art keeps its hard edges
        }
        let rgb = &json["healthbar_colors"];
        let channel = |i: usize| rgb[i].as_u64().unwrap_or(255) as u8;
        let icon_name = json["healthicon"].as_str().unwrap_or("face");
        let icon = texture(paths, &format!("images/icons/icon-{icon_name}.png")).or_else(|| texture(paths, &format!("images/icons/{icon_name}.png"))).or_else(|| texture(paths, "images/icons/icon-face.png"));
        Ok(Character {
            sheet,
            animations,
            position: spot + pair(&json["position"]),
            scale,
            // sheets are drawn facing right; whoever stands on the player's side is turned around
            flip_x: json["flip_x"].as_bool().unwrap_or(false) != is_player,
            size,
            grow: frame_size * (scale - 1.0) / 2.0,
            camera_offset: pair(&json["camera_position"]),
            sing_steps: json["sing_duration"].as_f64().unwrap_or(4.0) as f32,
            current,
            time: 0.0,
            since_note: 0.0,
            sways,
            swayed_right: false,
            idle_suffix: String::new(),
            special: None,
            icon,
            color: Color::from_rgba(channel(0), channel(1), channel(2), 255),
        })
    }

    /// Starts an animation the character has; returns whether it does.
    fn play(&mut self, animation: &str) -> bool {
        let known = self.animations.contains_key(animation);
        if known {
            self.current = animation.to_owned();
            self.time = 0.0;
            self.special = None;
        }
        known
    }

    /// A pose from a chart event or note type; the idle move waits `hold` seconds, or for the pose to end.
    fn pose(&mut self, animation: &str, hold: Option<f32>) {
        if self.play(animation) {
            self.special = Some(hold.unwrap_or(f32::INFINITY));
        }
    }

    /// A one-shot animation has reached its last frame.
    fn finished(&self) -> bool {
        self.animations.get(&self.current).is_none_or(|a| !a.looped && self.time * a.fps >= a.frames.len() as f32)
    }

    fn singing(&self) -> bool {
        self.current.starts_with("sing")
    }

    /// `alt`: with the alternative pose, when the character has one.
    fn sing(&mut self, lane: usize, missed: bool, alt: bool) {
        let pose = format!("{}{}", SING[lane], if missed { "miss" } else { "" });
        if !(alt && self.play(&format!("{pose}-alt"))) {
            self.play(&pose);
        }
        self.since_note = 0.0;
    }

    /// The idle move, restarted on the beat.
    fn dance(&mut self) {
        if self.special.is_some() {
            return;
        }
        let pose = if self.sways {
            self.swayed_right = !self.swayed_right;
            if self.swayed_right { "danceRight" } else { "danceLeft" }
        } else {
            "idle"
        };
        if !self.play(&format!("{pose}{}", self.idle_suffix)) {
            self.play(pose);
        }
    }

    fn middle(&self) -> Vec2 {
        self.position + self.size / 2.0
    }

    fn draw(&self) {
        let Some(animation) = self.animations.get(&self.current) else { return };
        let frame = (self.time * animation.fps) as usize;
        let frame = if animation.looped { frame % animation.frames.len() } else { frame.min(animation.frames.len() - 1) };
        let at = self.position - animation.offset - self.grow;
        self.sheet.draw(animation.frames[frame], at.x, at.y, self.scale, self.flip_x, WHITE);
    }
}

pub struct Scene {
    paths: Paths,
    /// Where the stage puts the opponent, the player and the girlfriend.
    spots: [Vec2; 3],
    /// The player lost: only the fallen character is shown.
    dead: bool,
    backdrop: Vec<Layer>,
    pub opponent: Option<Character>,
    gf: Option<Character>,
    pub player: Option<Character>,
    /// Extra camera offsets per singer from the stage file: opponent, player, girlfriend.
    camera_offsets: [Vec2; 3],
    camera_speed: f32,
    /// Middle of the view, in stage coordinates.
    camera: Vec2,
    target: Vec2,
    default_zoom: f32,
    zoom: f32,
    /// Zoom of the notes and the rest of the HUD (it bumps on the beat too).
    pub hud_zoom: f32,
    /// The beat bumps start once the opponent has sung a note.
    bumping: bool,
    beat: i64,
    /// Scale of the health icons, bumped on the beat.
    pub icon_scale: f32,
    /// The girlfriend moves every this many of her beats (a chart event slows her down).
    gf_speed: i64,
    /// Where a chart event pinned the camera.
    pinned: Option<Vec2>,
    /// Shaking of the stage and of the HUD: strength (share of the screen) and seconds left.
    shake: [(f32, f32); 2],
    /// Characters a chart event will swap in, loaded ahead so that the swap does not stall the song.
    spares: HashMap<String, Character>,
    own: Own,
    /// The stage's own asset folder ("week6"), and whether it wants the pixel look for notes and HUD.
    pub directory: String,
    pub pixel: bool,
    /// The player's choices: whether the camera bumps on the beat, and whether anything may flash.
    pub cam_zooms: bool,
    pub flashing: bool,
    /// Sounds the stage wants played (file and volume); the play screen takes them.
    pub sounds: Vec<(std::path::PathBuf, f32)>,
}

/// Who a chart event means: 0 the opponent, 1 the player, 2 the girlfriend.
fn who(value: &str) -> usize {
    match value.trim().to_lowercase().as_str() {
        "bf" | "boyfriend" | "1" => 1,
        "gf" | "girlfriend" | "2" => 2,
        _ => 0,
    }
}

impl Scene {
    pub fn load(base: &std::path::Path, source: &str, chart: &Chart) -> Result<Scene, String> {
        let stage = read_json(&Paths::new(base, source, ""), &format!("stages/{}.json", chart.stage))?;
        let paths = Paths::new(base, source, stage["directory"].as_str().unwrap_or(""));
        let backdrop = backdrop(&chart.stage).into_iter().filter_map(|def| Layer::load(&paths, def)).collect();
        let character = |name: &str, spot: &str, is_player: bool| Character::load(&paths, name, pair(&stage[spot]), is_player).map_err(|e| eprintln!("Character {name}: {e}")).ok();
        // an opponent that is the girlfriend (the tutorial) stands in her place, and she is not drawn twice
        let opponent_is_gf = chart.opponent.starts_with("gf");
        let opponent = character(&chart.opponent, if opponent_is_gf { "girlfriend" } else { "opponent" }, false);
        let gf = if opponent_is_gf || stage["hide_girlfriend"].as_bool().unwrap_or(false) { None } else { character(&chart.gf, "girlfriend", false) };
        let player = character(&chart.player, "boyfriend", true);
        let default_zoom = stage["defaultZoom"].as_f64().unwrap_or(1.05) as f32;
        let spots = [pair(&stage["opponent"]), pair(&stage["boyfriend"]), pair(&stage["girlfriend"])];
        let mut scene = Scene {
            paths,
            spots,
            dead: false,
            backdrop,
            opponent,
            gf,
            player,
            camera_offsets: [pair(&stage["camera_opponent"]), pair(&stage["camera_boyfriend"]), pair(&stage["camera_girlfriend"])],
            camera_speed: stage["camera_speed"].as_f64().unwrap_or(1.0) as f32,
            camera: Vec2::ZERO,
            target: Vec2::ZERO,
            default_zoom,
            zoom: default_zoom,
            hud_zoom: 1.0,
            bumping: false,
            beat: i64::MIN,
            icon_scale: 1.0,
            gf_speed: 1,
            pinned: None,
            shake: [(0.0, 0.0); 2],
            spares: HashMap::new(),
            own: match chart.stage.as_str() {
                "spooky" => Own::Spooky { struck: 0, wait: 8, since: 1e9 },
                "philly" => Own::Philly { light: 0, train: Train::new() },
                "limo" => Own::Limo { speed: 0.0, back_in: None },
                "tank" => Own::Tank { angle: rand::gen_range(-90.0, 45.0), speed: rand::gen_range(5.0, 7.0) },
                _ => Own::Nothing,
            },
            directory: stage["directory"].as_str().unwrap_or("").to_owned(),
            pixel: stage["stageUI"].as_str().is_some_and(|ui| ui == "pixel" || ui.ends_with("-pixel")) || stage["isPixelStage"].as_bool().unwrap_or(false),
            cam_zooms: true,
            flashing: true,
            sounds: Vec::new(),
        };
        if let Some(window) = find(&mut scene.backdrop, "philly/window") {
            window.tint.a = 0.0; // dark until the first light
        }
        for event in chart.events.iter().filter(|e| e.name == "Change Character") {
            let (role, spot) = scene.role(&event.value1);
            if !scene.spares.contains_key(&event.value2) {
                if let Ok(spare) = Character::load(&scene.paths, &event.value2, spot, role == 1) {
                    scene.spares.insert(event.value2.clone(), spare);
                }
            }
        }
        scene.look(chart, 0.0);
        scene.camera = scene.target;
        Ok(scene)
    }

    /// For "Change Character": which role (0 opponent, 1 player, 2 girlfriend) a value names, and where it stands.
    fn role(&self, value: &str) -> (usize, Vec2) {
        match value.trim().to_lowercase().as_str() {
            "gf" | "girlfriend" | "2" => (2, self.spots[2]),
            "dad" | "opponent" | "1" => (0, self.spots[0]),
            _ => (1, self.spots[1]),
        }
    }

    fn singer(&mut self, who: usize) -> Option<&mut Character> {
        [&mut self.opponent, &mut self.player, &mut self.gf][who].as_mut()
    }

    /// Hud shake offset for this frame, in pixels.
    pub fn hud_shake(&self) -> Vec2 {
        let (strength, left) = self.shake[1];
        if left > 0.0 { vec2(rand::gen_range(-1.0, 1.0), rand::gen_range(-1.0, 1.0)) * strength * SCREEN } else { Vec2::ZERO }
    }

    /// Carries out a chart event that concerns the stage. Unknown ones are ignored.
    pub fn event(&mut self, event: &ChartEvent) {
        let number = |value: &str| value.trim().parse::<f32>().ok();
        match event.name.as_str() {
            "Hey!" => {
                let hold = number(&event.value2).filter(|t| *t > 0.0).unwrap_or(0.6);
                let only = event.value1.trim().to_lowercase();
                if !matches!(only.as_str(), "gf" | "girlfriend" | "1") {
                    self.player.iter_mut().for_each(|c| c.pose("hey", Some(hold)));
                }
                if !matches!(only.as_str(), "bf" | "boyfriend" | "0") {
                    self.gf.iter_mut().for_each(|c| c.pose("cheer", Some(hold)));
                }
            }
            "Set GF Speed" => self.gf_speed = number(&event.value1).map_or(1, |n| (n.round() as i64).max(1)),
            "Add Camera Zoom" => {
                if self.bumping && self.cam_zooms && self.zoom < 1.35 {
                    self.zoom += number(&event.value1).unwrap_or(0.015);
                    self.hud_zoom += number(&event.value2).unwrap_or(0.03);
                }
            }
            "Play Animation" => {
                let who = who(&event.value2);
                self.singer(who).into_iter().for_each(|c| c.pose(&event.value1, None));
            }
            "Camera Follow Pos" => {
                let (x, y) = (number(&event.value1), number(&event.value2));
                self.pinned = (x.is_some() || y.is_some()).then(|| vec2(x.unwrap_or(0.0), y.unwrap_or(0.0)));
            }
            "Alt Idle Animation" => {
                let who = who(&event.value1);
                self.singer(who).into_iter().for_each(|c| c.idle_suffix = event.value2.clone());
            }
            "Screen Shake" => {
                for (camera, value) in [&event.value1, &event.value2].into_iter().enumerate() {
                    let mut parts = value.split(',').map(number);
                    if let (Some(Some(seconds)), Some(Some(strength))) = (parts.next(), parts.next()) {
                        self.shake[camera] = (strength, seconds);
                    }
                }
            }
            "Change Character" => {
                let (role, _) = self.role(&event.value1);
                if let Some(spare) = self.spares.remove(&event.value2) {
                    *[&mut self.opponent, &mut self.player, &mut self.gf][role] = Some(spare);
                }
            }
            _ => {}
        }
    }

    /// Points the camera at whoever the current section belongs to.
    fn look(&mut self, chart: &Chart, position: f64) {
        if let Some(pinned) = self.pinned {
            self.target = pinned;
            return;
        }
        let Some(section) = chart.section_at(position) else { return };
        let target = if section.gf && self.gf.is_some() {
            self.gf.as_ref().map(|c| c.middle() + c.camera_offset + self.camera_offsets[2])
        } else if section.must_hit {
            self.player.as_ref().map(|c| c.middle() + vec2(-100.0 - c.camera_offset.x, -100.0 + c.camera_offset.y) + self.camera_offsets[1])
        } else {
            self.opponent.as_ref().map(|c| c.middle() + vec2(150.0, -100.0) + c.camera_offset + self.camera_offsets[0])
        };
        self.target = target.unwrap_or(self.target);
    }

    /// Advances by `dt` seconds to song position `position` (ms). `held` says which lanes have a hold running, per side.
    pub fn update(&mut self, dt: f32, position: f64, chart: &Chart, events: &[Event], held: [[bool; 4]; 2]) {
        for event in events {
            // the note decides who moves and how; a stray press has no note and is the player's plain miss
            let (lane, note, side, missed) = match *event {
                Event::Hit { lane, note, .. } => (lane, Some(note), 1, false),
                Event::Miss { lane, note } => (lane, note, 1, true),
                Event::OpponentHit { lane, note } => (lane, Some(note), 0, false),
            };
            self.bumping |= side == 0;
            let note = note.map(|index| &chart.notes[index]);
            let kind = note.map_or(NoteKind::Normal, |n| n.kind);
            let alt = kind == NoteKind::Alt || (side == 0 && note.and_then(|n| chart.section_at(n.time)).is_some_and(|s| s.alt));
            match kind {
                NoteKind::NoAnimation => {}
                NoteKind::Hey if !missed => {
                    self.singer(side).into_iter().for_each(|c| c.pose("hey", Some(0.6)));
                    self.gf.iter_mut().for_each(|c| c.pose("cheer", Some(0.6)));
                }
                NoteKind::GfSing if self.gf.is_some() => self.gf.iter_mut().for_each(|c| c.sing(lane, false, alt)),
                _ => self.singer(side).into_iter().for_each(|c| c.sing(lane, missed, alt)),
            }
        }
        let rest_after = chart.step_ms(position) as f32 * 0.0011;
        for (side, character) in [&mut self.opponent, &mut self.player].into_iter().enumerate() {
            if let Some(character) = character {
                if held[side].contains(&true) {
                    character.since_note = 0.0; // still singing the hold
                }
                if character.singing() && character.since_note >= rest_after * character.sing_steps {
                    character.dance();
                }
            }
        }
        for character in [&mut self.opponent, &mut self.gf, &mut self.player].into_iter().flatten() {
            character.time += dt;
            character.since_note += dt;
            // a pose from an event gives way to the idle move when its time is up, or when it has played through
            if let Some(left) = character.special {
                let left = left - dt;
                character.special = Some(left);
                if left <= 0.0 || (left.is_infinite() && character.finished()) {
                    character.special = None;
                    character.dance();
                }
            }
        }
        for shake in &mut self.shake {
            shake.1 = (shake.1 - dt).max(0.0);
        }
        self.backdrop.iter_mut().for_each(|layer| layer.time += dt);

        let beat = chart.beat_at(position).floor() as i64;
        let on_beat = beat != self.beat;
        self.run_own(dt, on_beat.then_some(beat), chart.step_ms(position) as f32 * 4.0 / 1000.0);
        if on_beat {
            self.beat = beat;
            self.icon_scale = 1.2;
            self.backdrop.iter_mut().for_each(Layer::beat);
            if self.bumping && self.cam_zooms && beat % 4 == 0 {
                self.zoom += 0.015;
                self.hud_zoom += 0.03;
            }
            for (who, character) in [&mut self.opponent, &mut self.player, &mut self.gf].into_iter().enumerate() {
                let Some(character) = character else { continue };
                let every = if character.sways { 1 } else { 2 } * if who == 2 { self.gf_speed } else { 1 };
                if !character.singing() && beat % every == 0 {
                    character.dance();
                }
            }
        }
        self.look(chart, position);

        // everything eases back at a fixed rate, whatever the frame rate
        let ease = |rate: f32| 1.0 - (-dt * rate).exp();
        self.camera += (self.target - self.camera) * ease(2.4 * self.camera_speed);
        self.zoom += (self.default_zoom - self.zoom) * ease(3.125);
        self.hud_zoom += (1.0 - self.hud_zoom) * ease(3.125);
        self.icon_scale += (1.0 - self.icon_scale) * ease(9.0);
    }

    /// Moves what the stage moves by its own rules. `beat` is the beat that has just begun, if one has; `beat_s` a beat's length.
    fn run_own(&mut self, dt: f32, beat: Option<i64>, beat_s: f32) {
        let (paths, sounds) = (&self.paths, &mut self.sounds);
        let mut sound = |name: &str, volume: f32| sounds.extend(paths.find(&format!("sounds/{name}.ogg")).map(|path| (path, volume)));
        match &mut self.own {
            Own::Nothing => {}
            Own::Spooky { struck, wait, since } => {
                *since += dt;
                if beat.is_some_and(|beat| beat > *struck + *wait) && chance(0.1) {
                    sound(&format!("thunder_{}", rand::gen_range(1, 3)), 1.0);
                    (*struck, *wait, *since) = (beat.unwrap_or(0), rand::gen_range(8, 25), 0.0);
                    find(&mut self.backdrop, "halloween_bg").into_iter().for_each(|house| house.time = 0.0);
                    for character in [&mut self.opponent, &mut self.gf, &mut self.player].into_iter().flatten() {
                        character.play("scared");
                    }
                    if self.cam_zooms {
                        self.zoom += 0.015;
                        self.hud_zoom += 0.03;
                    }
                }
            }
            Own::Philly { light, train } => {
                if let Some(window) = find(&mut self.backdrop, "philly/window") {
                    // every fourth beat the windows light up in another colour, then dim
                    if beat.is_some_and(|beat| beat % 4 == 0) {
                        *light = (*light + rand::gen_range(1, WINDOW_LIGHTS.len())) % WINDOW_LIGHTS.len();
                        window.tint = Color::from_hex(WINDOW_LIGHTS[*light]);
                    } else {
                        window.tint.a = (window.tint.a - beat_s * dt * 1.5).max(0.0);
                    }
                }
                if let (Some(beat), None) = (beat, train.passing) {
                    train.cooldown += 1;
                    if beat % 8 == 4 && train.cooldown > 8 && chance(0.3) {
                        train.cooldown = rand::gen_range(-4, 1);
                        train.passing = Some(0.0);
                        sound("train_passes", 1.0);
                    }
                }
                match (train.step(dt), &mut self.gf) {
                    // her hair blows for as long as it passes, and falls back when it has gone
                    (TrainNow::Rushing, Some(gf)) if gf.current != "hairBlow" || gf.finished() => gf.pose("hairBlow", None),
                    (TrainNow::Gone, Some(gf)) => {
                        gf.pose("hairFall", None);
                        gf.swayed_right = false;
                    }
                    _ => {}
                }
                find(&mut self.backdrop, "philly/train").into_iter().for_each(|picture| picture.def.at.x = train.x);
            }
            Own::Limo { speed, back_in } => {
                let Some(car) = find(&mut self.backdrop, "limo/fastCarLol") else { return };
                car.def.at.x += *speed * dt;
                match back_in {
                    Some(left) if *left <= dt => {
                        car.def.at = vec2(-12600.0, rand::gen_range(140.0, 250.0));
                        (*speed, *back_in) = (0.0, None);
                    }
                    Some(left) => *left -= dt,
                    None if beat.is_some() && chance(0.1) => {
                        sound(&format!("carPass{}", rand::gen_range(0, 2)), 0.7);
                        (*speed, *back_in) = (rand::gen_range(30600.0, 39600.0), Some(2.0));
                    }
                    None => {}
                }
            }
            Own::Tank { angle, speed } => {
                *angle += dt * *speed;
                if let Some(tank) = find(&mut self.backdrop, "tankRolling") {
                    let around = (*angle + 180.0).to_radians();
                    tank.def.at = vec2(400.0 + 1500.0 * around.cos(), 1300.0 + 1100.0 * around.sin());
                    tank.angle = (*angle - 90.0 + 15.0).to_radians();
                }
            }
        }
    }

    /// Whether the player's character is in its idle move (or absent) rather than singing or posing.
    pub fn player_idle(&self) -> bool {
        self.player.as_ref().is_none_or(|player| ["idle", "danceLeft", "danceRight"].iter().any(|idle| player.current.starts_with(idle)))
    }

    /// The player lost: swaps in the fallen character and turns the camera to it.
    pub fn die(&mut self) {
        self.dead = true;
        // shortcut: "bf-dead", or its pixel self on pixel stages; charts and scripts can name another game over character (gameOverChar). Read it from the chart when one needs it.
        let name = if self.pixel { "bf-pixel-dead" } else { "bf-dead" };
        if let Ok(mut fallen) = Character::load(&self.paths, name, self.spots[1], true) {
            fallen.play("firstDeath");
            self.target = fallen.middle() + fallen.camera_offset;
            self.player = Some(fallen);
        }
    }

    /// Runs the game over scene. Returns true at the moment the fall ends and the waiting loop begins.
    pub fn update_dead(&mut self, dt: f32, confirm: bool) -> bool {
        let ease = 1.0 - (-dt * 2.4).exp();
        self.camera += (self.target - self.camera) * ease;
        self.zoom += (self.default_zoom - self.zoom) * ease;
        let Some(fallen) = &mut self.player else { return confirm };
        fallen.time += dt;
        if confirm {
            fallen.play("deathConfirm");
        } else if fallen.current == "firstDeath" && fallen.finished() {
            fallen.play("deathLoop");
            return true;
        }
        false
    }

    pub fn draw(&self) {
        let (strength, left) = self.shake[0];
        let shake = if left > 0.0 && !self.dead { vec2(rand::gen_range(-1.0, 1.0), rand::gen_range(-1.0, 1.0)) * strength * SCREEN / self.zoom } else { Vec2::ZERO };
        set_camera(&Camera2D { target: self.camera + shake, zoom: vec2(2.0 / SCREEN.x, 2.0 / SCREEN.y) * self.zoom, ..Default::default() });
        if self.dead {
            self.player.iter().for_each(|fallen| fallen.draw());
            return;
        }
        let scroll = self.camera - SCREEN / 2.0;
        self.backdrop.iter().filter(|layer| !layer.def.front).for_each(|layer| layer.draw(scroll));
        for character in [&self.gf, &self.opponent, &self.player].into_iter().flatten() {
            character.draw();
        }
        self.backdrop.iter().filter(|layer| layer.def.front).for_each(|layer| layer.draw(scroll));
        if let Own::Spooky { since, .. } = self.own {
            // the lightning whitens everything: up at once, held, then gone
            let alpha = match since {
                t if t < 0.075 => 0.4 + 0.1 * t / 0.075,
                t if t < 0.15 => 0.5,
                t => 0.5 * (1.0 - (t - 0.15) / 0.25),
            };
            if alpha > 0.0 && self.flashing {
                let view = SCREEN / self.zoom;
                draw_rectangle(self.camera.x - view.x, self.camera.y - view.y, view.x * 2.0, view.y * 2.0, Color::new(1.0, 1.0, 1.0, alpha));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_train_waits_for_its_sound_then_passes_car_by_car() {
        use super::{Train, TrainNow};
        let mut train = Train::new();
        assert_eq!((train.step(1.0), train.x), (TrainNow::Quiet, 2000.0), "nothing moves until it is called");
        train.passing = Some(0.0);
        let frame = 1.0 / 60.0;
        let (mut quiet, mut rushing) = (0.0, 0);
        loop {
            match train.step(frame) {
                TrainNow::Quiet if rushing == 0 => quiet += frame,
                TrainNow::Quiet => {}
                TrainNow::Rushing => rushing += 1,
                TrainNow::Gone => break,
            }
            assert!(quiet + (rushing as f32) < 1000.0, "it never left");
        }
        // 4.7 s of sound first; then 8 cars of 3 steps each after the first approach, and the way out
        assert!((4.65..4.75).contains(&quiet), "{quiet}"); // the frame it first moves in counts as rushing
        assert!((30..45).contains(&rushing), "{rushing}");
        assert_eq!((train.x, train.passing, train.cars), (1480.0, None, 8));
    }

    #[test]
    fn events_name_singers_by_word_or_number() {
        assert_eq!(["dad", "", "0", "BF", " boyfriend ", "1", "gf", "Girlfriend", "2"].map(super::who), [0, 0, 0, 1, 1, 1, 2, 2, 2]);
    }
}
