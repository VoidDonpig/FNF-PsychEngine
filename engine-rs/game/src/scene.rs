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
    /// Shows only while the stage plays it through once.
    Flash,
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
// shortcut: the weekend 1 lighting is not here yet. What `Own` moves is.
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
            // between the dancers' limo and the dancers: what the "Kill Henchmen" event brings (a pole, its lamp, who it leaves behind, what flies off)
            let gore = |prefix: &'static str, motion: Motion, y: f32| moving(prefix, motion, layer("gore/noooooo", -500.0, y, 0.4, 0.4));
            let mut layers = vec![
                layer("limo/limoSunset", -120.0, -50.0, 0.1, 0.1),
                layer("gore/metalPole", -500.0, 220.0, 0.4, 0.4),
                moving("background limo pink", Loop, layer("limo/bgLimo", -150.0, 480.0, 0.4, 0.4)),
                gore("Henchmen on rail", Loop, 90.0),
                gore("henchmen death", Loop, 220.0),
            ];
            layers.extend((0..5).map(|i| moving("bg dancer sketch PINK", Sway(15), layer("limo/limoDancer", 370.0 * i as f32 + 170.0, 80.0, 0.4, 0.4))));
            layers.push(layer("gore/coldHeartKiller", -680.0, 140.0, 0.4, 0.4));
            layers.extend(["hench leg spin PINK", "hench arm spin PINK", "hench head spin PINK", "hench leg spin 2 PINK", "hench arm spin 2 PINK", "hench head spin 2 PINK"].map(|prefix| gore(prefix, Flash, 0.0)));
            layers.extend([0, 1].map(|_| LayerDef { flip: true, ..moving("blood", Flash, layer("gore/stupidBlood", -400.0, -400.0, 0.4, 0.4)) }));
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
        "schoolEvil" => vec![
            LayerDef { centered: true, ..pixel(sized(Times(6.0), moving("background 2", Loop, layer("weeb/animatedEvilSchool", 400.0, 200.0, 0.8, 0.9)))) },
            // the ghouls of the "Trigger BG Ghouls" event
            pixel(sized(Times(6.0), moving("BG freaks glitch instance", Flash, layer("weeb/bgGhouls", -100.0, 190.0, 0.9, 0.9)))),
        ],
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
        // the fight in the rain: a blurred street that stays with the camera, under a sky that drifts and flashes
        "phillyBlazin" => {
            let mut layers: Vec<LayerDef> = (0..3).map(|_| layer("phillyBlazin/skyBlur", -500.0, -120.0, 0.0, 0.0)).collect();
            layers.extend([
                sized(Times(1.75), layer("phillyBlazin/skyBlur", -600.0, -175.0, 0.0, 0.0)),
                sized(Times(1.75), moving("lightning0", Flash, layer("phillyBlazin/lightning", -50.0, -300.0, 0.0, 0.0))),
                sized(Times(1.75), layer("phillyBlazin/streetBlur", -600.0, -175.0, 0.0, 0.0)),
            ]);
            layers
        }
        "phillyStreets" => vec![
            sized(Times(0.65), layer("phillyStreets/phillySkybox", -650.0, -375.0, 0.1, 0.1)),
            layer("phillyStreets/phillySkyline", -545.0, -273.0, 0.2, 0.2),
            layer("phillyStreets/phillyForegroundCity", 625.0, 94.0, 0.3, 0.3),
            layer("phillyStreets/phillyConstruction", 1800.0, 364.0, 0.7, 1.0),
            layer("phillyStreets/phillyHighwayLights", 284.0, 305.0, 1.0, 1.0),
            layer("phillyStreets/phillyHighway", 139.0, 209.0, 1.0, 1.0),
            layer("phillyStreets/phillySmog", -6.0, 245.0, 0.8, 1.0),
            // shortcut: the light maps over the lamps (added brightness) and the rain are left out.
            moving("car1", Still, layer("phillyStreets/phillyCars", 1200.0, 818.0, 0.9, 1.0)),
            LayerDef { flip: true, ..moving("car1", Still, layer("phillyStreets/phillyCars", 1200.0, 818.0, 0.9, 1.0)) },
            moving("redtogreen", Once, layer("phillyStreets/phillyTraffic", 1840.0, 608.0, 0.9, 1.0)),
            layer("phillyStreets/phillyForeground", 88.0, 317.0, 1.0, 1.0),
            front(layer("SpraycanPile", 920.0, 1045.0, 1.0, 1.0)),
        ],
        _ => Vec::new(),
    }
}

/// The pictures of a stage as read from disk, by name and by whether their sheet was read with them.
type Pictures = HashMap<(&'static str, bool), (Texture2D, std::rc::Rc<Vec<(String, Frame)>>)>;

/// A backdrop picture, loaded.
struct Layer {
    def: LayerDef,
    texture: Texture2D,
    /// Its animation's frames; empty for a plain picture.
    frames: Vec<Frame>,
    /// Every frame of its sheet, for changing animation.
    sheet: std::rc::Rc<Vec<(String, Frame)>>,
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
    /// And the pole that takes the dancers off their limo, which then drives off for new ones at `limo_speed`.
    Limo { speed: f32, back_in: Option<f32>, kill: Kill, limo_speed: f32 },
    /// Seconds the crowd at the front keeps cheering.
    Mall { hey: f32 },
    /// Whether the girls in the back have turned sour.
    School { sour: bool },
    /// The fight stage: how far its sky has drifted, the seconds until lightning strikes and since it last did.
    Blazin { drift: f32, strike_in: f32, since: f32 },
    /// The traffic light (red or not, the beat it last changed and the beats until it changes again) and the two cars: one each way.
    Streets { red: bool, changed: i64, every: i64, cars: [Car; 2] },
    /// The tank rolling round behind the ground: where on its circle it is (degrees) and how fast it goes.
    Tank { angle: f32, speed: f32 },
}

/// A colour with its saturation and its brightness scaled, its hue kept.
fn toned(color: Color, saturation: f32, brightness: f32) -> Color {
    let brightest = color.r.max(color.g).max(color.b);
    let tone = |channel: f32| (brightest - (brightest - channel) * saturation) * brightness;
    Color::new(tone(color.r), tone(color.g), tone(color.b), color.a)
}

/// A spark rising from the street while the lights are out.
struct Particle {
    at: Vec2,
    speed: Vec2,
    pull: Vec2,
    scroll: Vec2,
    scale: f32,
    /// Seconds before it starts to fade, and how fast it fades then.
    life: f32,
    decay: f32,
    alpha: f32,
}

/// The lights-out show of the "Philly Glow" event: the city goes black but for its windows, a glow and sparks, all in one colour.
struct Glow {
    gradient: Texture2D,
    spark: Texture2D,
    /// The colour of the show (of `WINDOW_LIGHTS`) while it is on.
    color: Option<usize>,
    /// How high the glow stands over the street: every burst of sparks lifts it, and it sinks again.
    height: f32,
    sparks: Vec<Particle>,
}

/// A soldier of the tank stage running in to be shot from the speakers.
#[derive(Debug, PartialEq)]
struct Soldier {
    /// When he is shot (ms into the song).
    shot_at: f64,
    y: f32,
    /// Runs in from the left.
    right: bool,
    /// How far short of the middle he gets, and how fast he runs (px per ms).
    short: f32,
    speed: f32,
    /// Which of the two ways of falling, and which frame his running starts on.
    fall: usize,
    step: usize,
}

impl Soldier {
    /// Where his top left corner is at song position `position`, and which frame of running (false) or falling (true) shows. None once he has fallen.
    fn at(&self, position: f64, running: usize, falling: usize) -> Option<(Vec2, bool, usize)> {
        let since = position - self.shot_at;
        let run = |ms: f64| if self.right { 0.02 * SCREEN.x - self.short + ms as f32 * self.speed } else { 0.74 * SCREEN.x + self.short - ms as f32 * self.speed };
        if since <= 0.0 {
            let x = run(since);
            let seen = x > -0.5 * SCREEN.x && x < 1.2 * SCREEN.x;
            return seen.then(|| (vec2(x, self.y), false, (self.step + (position.max(0.0) * 0.024) as usize) % running.max(1)));
        }
        // he falls where the shot caught him; the fall of those from the left is drawn further up and left
        let frame = (since * 0.024) as usize;
        let shift = if self.right { vec2(249.5, 152.2) } else { Vec2::ZERO };
        (frame < falling).then(|| (vec2(run(0.0), self.y) - shift, true, frame))
    }
}

/// The soldiers of Stress and their pictures.
struct Soldiers {
    sheet: Sheet,
    running: Vec<Frame>,
    falling: [Vec<Frame>; 2],
    list: Vec<Soldier>,
}

/// The spotlight of the "Dadbattle Spotlight" event, with the smoke that drifts in under it.
struct Spotlight {
    light: Texture2D,
    smoke: Texture2D,
    on: bool,
    /// Who it is on (0 the opponent, 1 the player) and for how many seconds.
    who: usize,
    since: f32,
    /// How thick the smoke is, what it is heading for and how fast.
    fog: f32,
    fog_to: (f32, f32),
    /// The two drifts of smoke: where, how large, how fast.
    drifts: [(Vec2, f32, f32); 2],
}

/// How a car's drive is paced.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ease {
    Even,
    /// Fast at first, coming to rest.
    Slowing,
    /// Pulling away.
    Quickening,
}

/// A car of the weekend 1 street driving along a bend.
#[derive(Debug, PartialEq)]
struct Drive {
    /// Start, the point the bend is pulled towards, end.
    path: [Vec2; 3],
    /// How it is turned at the start and at the end, in degrees.
    turn: (f32, f32),
    seconds: f32,
    /// Seconds it still stands before setting off.
    wait: f32,
    ease: Ease,
    time: f32,
    /// It stops at the red light and stays until the light changes.
    to_the_light: bool,
}

impl Drive {
    /// One of the drives: 0 past the light and away, 1 up to the red light, 2 away from the light when it turns green, 3 the other way.
    /// `variant` (1 to 4) is the car's look; some look slower.
    fn new(kind: usize, variant: usize) -> Drive {
        let o = vec2(306.6, 168.3);
        let range = match (variant, kind) {
            (1, _) => (1.0, 1.7),
            (2, 1) => (0.9, 1.5),
            (2, _) => (0.6, 1.2),
            _ => (1.5, 2.5),
        };
        let seconds = rand::gen_range(range.0, range.1);
        let (away, far) = (vec2(2400.0, 980.0) - o, vec2(3102.0, 1127.0) - o);
        let at_light = vec2(1950.0 - 80.0, 980.0 + 15.0) - o;
        let plain = Drive { path: [Vec2::ZERO; 3], turn: (0.0, 0.0), seconds, wait: 0.0, ease: Ease::Even, time: 0.0, to_the_light: false };
        match kind {
            0 => Drive { path: [vec2(1570.0, 1049.0 - 30.0) - o, away + vec2(0.0, -50.0), far + vec2(0.0, 40.0)], turn: (-8.0, 18.0), ..plain },
            1 => Drive { path: [vec2(1500.0 - 20.0, 1049.0 - 20.0) - o, vec2(1770.0 - 80.0, 994.0 + 10.0) - o, at_light], turn: (-7.0, -5.0), ease: Ease::Slowing, to_the_light: true, ..plain },
            2 => Drive { path: [at_light, away + vec2(0.0, -50.0), far + vec2(0.0, 40.0)], turn: (-5.0, 18.0), seconds: rand::gen_range(1.8, 3.0), wait: rand::gen_range(0.2, 1.2), ease: Ease::Quickening, ..plain },
            _ => Drive { path: [far + vec2(0.0, 60.0), away + vec2(0.0, -30.0), vec2(1570.0, 1049.0 - 10.0) - o], turn: (18.0, -8.0), ..plain },
        }
    }

    /// Where the car is and how it is turned (degrees); and whether it has arrived.
    fn at(&self) -> (Vec2, f32, bool) {
        let done = ((self.time - self.wait) / self.seconds).clamp(0.0, 1.0);
        let t = match self.ease {
            Ease::Even => done,
            Ease::Slowing => 1.0 - (1.0 - done).powi(3),
            Ease::Quickening => 1.0 - (done * std::f32::consts::FRAC_PI_2).cos(),
        };
        let [from, pull, to] = self.path;
        let at = from * (1.0 - t) * (1.0 - t) + pull * 2.0 * (1.0 - t) * t + to * t * t;
        (at, self.turn.0 + (self.turn.1 - self.turn.0) * t, done >= 1.0)
    }
}

/// A car of the weekend 1 street: its drive, whether it stands at the red light, and how far its look is drawn off its spot.
#[derive(Default)]
struct Car {
    drive: Option<Drive>,
    waiting: bool,
    shift: Vec2,
}

#[derive(Clone, Copy, PartialEq)]
enum Kill {
    Wait,
    /// The pole sweeps along the limo.
    Sweep,
    /// The dancers' limo falls back out of sight,
    Leaving,
    /// comes back with new dancers,
    Returning,
    /// and settles in its place.
    Settling,
}

const WINDOW_LIGHTS: [u32; 5] = [0x31A2FD, 0x31FD8C, 0xFB33F5, 0xFD4531, 0xFBA633];

fn chance(share: f32) -> bool {
    rand::gen_range(0.0, 1.0) < share
}

fn find<'a>(backdrop: &'a mut [Layer], image: &str) -> Option<&'a mut Layer> {
    backdrop.iter_mut().find(|layer| layer.def.image == image)
}

impl Layer {
    /// `loaded` keeps the pictures read so far: layers of one picture share it.
    fn load(paths: &Paths, def: LayerDef, loaded: &mut Pictures) -> Option<Layer> {
        let key = (def.image, def.anim.is_some());
        if !loaded.contains_key(&key) {
            let image = paths.find(&format!("images/{}.png", def.image))?;
            let picture = match def.anim {
                Some(_) => {
                    let sheet = Sheet::load(&image).map_err(|e| eprintln!("Stage: {e}")).ok()?;
                    (sheet.texture, std::rc::Rc::new(sheet.frames))
                }
                None => (Texture2D::from_file_with_format(&std::fs::read(image).ok()?, None), Default::default()),
            };
            loaded.insert(key, picture);
        }
        let (texture, sheet) = loaded[&key].clone();
        if def.pixel {
            texture.set_filter(FilterMode::Nearest);
        }
        let frames = def.anim.map_or(Vec::new(), |(prefix, _)| crate::atlas::animation(&sheet, prefix, &[]));
        let time = if matches!(def.anim, Some((_, Motion::Once | Motion::Flash))) { 1e9 } else { 0.0 };
        Some(Layer { def, texture, frames, sheet, time, swayed: false, tint: WHITE, angle: 0.0 })
    }

    /// Changes to another animation of its sheet, from its start.
    fn play(&mut self, prefix: &'static str, motion: Motion) {
        self.frames = crate::atlas::animation(&self.sheet, prefix, &[]);
        self.def.anim = Some((prefix, motion));
        self.time = 0.0;
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
        self.draw_in(scroll, self.tint);
    }

    /// Draws it in another colour than its own.
    fn draw_in(&self, scroll: Vec2, tint: Color) {
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
            Some((_, Motion::Flash)) if step >= count => return,
            Some((_, Motion::Flash)) => Some(step),
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
        if tint.a <= 0.0 {
            return;
        }
        // a turned layer is drawn upright with everything turned around its middle
        let gl = unsafe { get_internal_gl() }.quad_gl;
        if self.angle != 0.0 {
            let middle = (at + size * scale / 2.0).extend(0.0);
            gl.push_model_matrix(Mat4::from_translation(middle) * Mat4::from_rotation_z(self.angle) * Mat4::from_translation(-middle));
        }
        match frame {
            Some(f) => crate::atlas::draw_frame(&self.texture, self.frames[f], at.x, at.y, scale, self.def.flip, tint),
            None => draw_texture_ex(&self.texture, at.x, at.y, tint, DrawTextureParams { dest_size: Some(size * scale), flip_x: self.def.flip, ..Default::default() }),
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
    /// Which of the character's sheets its frames are on.
    sheet: usize,
    /// Its frames; for a character drawn the Animate way one empty frame for each of `steps`.
    frames: Vec<Frame>,
    /// The frames of the Animate timeline it goes through.
    steps: Vec<usize>,
    fps: f32,
    looped: bool,
    offset: Vec2,
}

pub struct Character {
    sheets: Vec<Sheet>,
    /// Drawn the Animate way instead, from a folder of that name.
    rig: Option<crate::animate::Atlas>,
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
    /// The colour of the light on the character (white: as drawn).
    tint: Color,
    /// Has no idle move: whatever it does is done to it from outside.
    skip_dance: bool,
    pub color: Color,
}

impl Character {
    /// Loads characters/<name>.json and its sheet. `spot` is where the stage puts this character.
    fn load(paths: &Paths, name: &str, spot: Vec2, is_player: bool) -> Result<Character, String> {
        let json = read_json(paths, &format!("characters/{name}.json"))?;
        // a folder with an Animation.json in it is a character drawn the Animate way
        let whole = json["image"].as_str().unwrap_or("").trim();
        let rig = match paths.find(&format!("images/{whole}/Animation.json")) {
            Some(file) => Some(crate::animate::Atlas::load(file.parent().unwrap_or(&file))?),
            None => None,
        };
        // otherwise it may be drawn from several sheets ("a, b"): the first is needed, the others bring more animations
        let mut sheets = Vec::new();
        for image in whole.split(',').map(str::trim).filter(|_| rig.is_none()) {
            match paths.find(&format!("images/{image}.png")).ok_or(format!("images/{image}.png not found")).and_then(|path| Sheet::load(&path)) {
                Ok(sheet) => sheets.push(sheet),
                Err(error) if sheets.is_empty() => return Err(error),
                Err(error) => eprintln!("Character {name}: {error}"),
            }
        }
        let mut animations = HashMap::new();
        for animation in json["animations"].as_array().into_iter().flatten() {
            let indices: Vec<u32> = animation["indices"].as_array().into_iter().flatten().filter_map(|i| i.as_u64()).map(|i| i as u32).collect();
            let named = animation["name"].as_str().unwrap_or("");
            let found = match &rig {
                // on a timeline: the frames listed, or those under the label of that name, or all of it
                Some(atlas) => {
                    let (start, length) = atlas.rig.label(named).unwrap_or((0, atlas.rig.length()));
                    let steps: Vec<usize> = if indices.is_empty() { (start..start + length).collect() } else { indices.iter().map(|index| *index as usize).collect() };
                    let empty = Frame { source: Rect::new(0.0, 0.0, 0.0, 0.0), offset: Vec2::ZERO, size: Vec2::ZERO, rotated: false };
                    Some((0, vec![empty; steps.len()], steps)).filter(|(_, _, steps)| !steps.is_empty())
                }
                None => sheets.iter().map(|sheet| sheet.animation(named, &indices)).enumerate().find(|(_, frames)| !frames.is_empty()).map(|(sheet, frames)| (sheet, frames, Vec::new())),
            };
            if let Some((sheet, frames, steps)) = found {
                // a speed of 0 means the timeline's own
                let fps = animation["fps"].as_f64().filter(|fps| *fps > 0.0).map_or(rig.as_ref().map_or(24.0, |atlas| atlas.rig.fps), |fps| fps as f32);
                animations.insert(animation["anim"].as_str().unwrap_or("").to_owned(), Animation { sheet, frames, steps, fps, looped: animation["loop"].as_bool().unwrap_or(false), offset: pair(&animation["offsets"]) });
            }
        }
        let sways = animations.contains_key("danceLeft") && animations.contains_key("danceRight");
        let current = if sways { "danceRight" } else { "idle" }.to_owned();
        let scale = json["scale"].as_f64().unwrap_or(1.0) as f32;
        let frame_size = animations.get(&current).or(animations.values().next()).map_or(Vec2::ZERO, |a| a.frames[0].size);
        let size = frame_size * scale;
        if json["no_antialiasing"].as_bool().unwrap_or(false) {
            sheets.iter().for_each(|sheet| sheet.texture.set_filter(FilterMode::Nearest)); // pixel art keeps its hard edges
        }
        let rgb = &json["healthbar_colors"];
        let channel = |i: usize| rgb[i].as_u64().unwrap_or(255) as u8;
        let icon_name = json["healthicon"].as_str().unwrap_or("face");
        let icon = texture(paths, &format!("images/icons/icon-{icon_name}.png")).or_else(|| texture(paths, &format!("images/icons/{icon_name}.png"))).or_else(|| texture(paths, "images/icons/icon-face.png"));
        Ok(Character {
            sheets,
            rig,
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
            tint: WHITE,
            skip_dance: false,
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
        if self.special.is_some() || self.skip_dance {
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
        match &self.rig {
            Some(atlas) => atlas.draw(animation.steps[frame], at, self.scale, self.flip_x, self.tint),
            None => self.sheets[animation.sheet].draw(animation.frames[frame], at.x, at.y, self.scale, self.flip_x, self.tint),
        }
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
    /// Seconds until the camera, held for a stunt, goes back to following the singers.
    unpin_in: Option<f32>,
    /// The stage's name.
    stage: String,
    /// Shaking of the stage and of the HUD: strength (share of the screen) and seconds left.
    shake: [(f32, f32); 2],
    /// Characters a chart event will swap in, loaded ahead so that the swap does not stall the song.
    spares: HashMap<String, Character>,
    own: Own,
    /// On the weekend 1 stages: the speaker the girlfriend sits on, and the spray can of the songs that have one.
    abot: Option<crate::props::ABot>,
    can: Option<crate::props::Can>,
    /// How loud seven bands of the music are right now; the play screen keeps this up to date.
    pub levels: [f32; 7],
    /// The fist fight of weekend 1, on its stage; and whether the opponent stands in front of the player in it.
    fight: Option<crate::fight::Fight>,
    opponent_front: bool,
    glow: Option<Glow>,
    spotlight: Option<Spotlight>,
    soldiers: Option<Soldiers>,
    /// When the shooter on the speakers fires, and to which side (lane 3: the right); and how many shots are behind.
    shots: Vec<(f64, usize)>,
    shots_done: usize,
    /// Where the song is (ms).
    position: f64,
    /// A flash over the whole stage: its colour (and strength), its age and how long it takes to fade.
    flash: (Color, f32, f32),
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
        let mut loaded = Pictures::new();
        let mut backdrop: Vec<Layer> = backdrop(&chart.stage).into_iter().filter_map(|def| Layer::load(&paths, def, &mut loaded)).collect();
        // what only shows when its moment comes
        for hidden in backdrop.iter_mut().filter(|layer| ["philly/window", "gore/metalPole", "gore/coldHeartKiller"].contains(&layer.def.image) || matches!(layer.def.anim, Some(("Henchmen on rail" | "henchmen death", _)))) {
            hidden.tint.a = 0.0;
        }
        for blood in backdrop.iter_mut().filter(|layer| layer.def.image == "gore/stupidBlood") {
            blood.angle = -57.5f32.to_radians();
        }
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
            unpin_in: None,
            stage: chart.stage.clone(),
            shake: [(0.0, 0.0); 2],
            spares: HashMap::new(),
            own: match chart.stage.as_str() {
                "spooky" => Own::Spooky { struck: 0, wait: 8, since: 1e9 },
                "philly" => Own::Philly { light: 0, train: Train::new() },
                "limo" => Own::Limo { speed: 0.0, back_in: None, kill: Kill::Wait, limo_speed: 0.0 },
                "mall" => Own::Mall { hey: 0.0 },
                "school" => Own::School { sour: false },
                "phillyBlazin" => Own::Blazin { drift: 0.0, strike_in: 3.0, since: 1e9 },
                "phillyStreets" => Own::Streets { red: false, changed: 0, every: 8, cars: Default::default() },
                "tank" => Own::Tank { angle: rand::gen_range(-90.0, 45.0), speed: rand::gen_range(5.0, 7.0) },
                _ => Own::Nothing,
            },
            abot: None,
            can: None,
            levels: [0.0; 7],
            fight: (chart.stage == "phillyBlazin").then(Default::default),
            opponent_front: false,
            glow: None,
            spotlight: None,
            soldiers: None,
            shots: Vec::new(),
            shots_done: 0,
            position: 0.0,
            flash: (WHITE, 1.0, 1.0),
            directory: stage["directory"].as_str().unwrap_or("").to_owned(),
            pixel: stage["stageUI"].as_str().is_some_and(|ui| ui == "pixel" || ui.ends_with("-pixel")) || stage["isPixelStage"].as_bool().unwrap_or(false),
            cam_zooms: true,
            flashing: true,
            sounds: Vec::new(),
        };
        // Pico on the speakers shoots by a chart of his own, and on the tank stage soldiers run into some of his shots
        if let Some(shooter) = scene.gf.as_mut().filter(|_| chart.gf == "pico-speaker") {
            (shooter.skip_dance, shooter.current) = (true, "shoot1".to_owned());
            let cues = scene.paths.find(&format!("data/{}/picospeaker.json", funkin_core::content::song_path(&chart.song))).and_then(|path| std::fs::read_to_string(path).ok());
            scene.shots = cues.and_then(|text| funkin_core::chart::parse(&text).ok()).map_or(Vec::new(), |cues| cues.notes.iter().map(|note| (note.time, note.lane)).collect());
            let sheet = scene.paths.find("images/tankmanKilled1.png").filter(|_| chart.stage == "tank").and_then(|path| Sheet::load(&path).ok());
            if let Some(sheet) = sheet {
                let list = scene.shots.iter().filter(|_| chance(0.16)).map(|&(shot_at, lane)| Soldier {
                    shot_at, y: 200.0 + rand::gen_range(50.0, 100.0), right: lane < 2, short: rand::gen_range(50.0, 200.0), speed: rand::gen_range(0.6, 1.0), fall: rand::gen_range(0, 2), step: rand::gen_range(0, 14),
                }).collect();
                scene.soldiers = Some(Soldiers { running: sheet.animation("tankman running", &[]), falling: [sheet.animation("John Shot 1", &[]), sheet.animation("John Shot 2", &[])], sheet, list });
            }
        }
        if matches!(chart.stage.as_str(), "phillyStreets" | "phillyBlazin") {
            scene.abot = Some(crate::props::ABot::load(&scene.paths, scene.spots[2]));
        }
        if chart.stage == "phillyStreets" && chart.notes.iter().any(|note| note.kind == NoteKind::KickCan) {
            scene.can = crate::props::Can::load(&scene.paths);
        }
        // what only some songs' events use is only loaded for them
        let uses = |name: &str| chart.events.iter().any(|event| event.name == name);
        let picture = |name: &str| texture(&scene.paths, &format!("images/{name}.png"));
        if let (true, Some(gradient), Some(spark)) = (chart.stage == "philly" && uses("Philly Glow"), picture("philly/gradient"), picture("philly/particle")) {
            scene.glow = Some(Glow { gradient, spark, color: None, height: 0.0, sparks: Vec::new() });
        }
        if let (true, Some(light), Some(smoke)) = (chart.stage == "stage" && uses("Dadbattle Spotlight"), picture("spotlight"), picture("smoke")) {
            let drift = |x: f32, direction: f32| (vec2(x, 660.0 + rand::gen_range(-20.0, 20.0)), rand::gen_range(1.1, 1.22), direction * rand::gen_range(15.0, 22.0));
            scene.spotlight = Some(Spotlight { light, smoke, on: false, who: 0, since: 0.0, fog: 0.0, fog_to: (0.0, 1.0), drifts: [drift(-1350.0, 1.0), drift(1750.0, -1.0)] });
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
            "Kill Henchmen" => {
                if let Own::Limo { kill: kill @ Kill::Wait, .. } = &mut self.own {
                    *kill = Kill::Sweep;
                    for layer in self.backdrop.iter_mut() {
                        match (layer.def.image, layer.def.anim) {
                            ("gore/metalPole", _) => (layer.def.at.x, layer.tint.a) = (-400.0, 1.0),
                            ("gore/coldHeartKiller", _) => layer.tint.a = 1.0,
                            ("gore/noooooo", Some((_, Motion::Loop))) => layer.tint.a = 0.0,
                            _ => {}
                        }
                    }
                }
            }
            "BG Freaks Expression" => {
                if let Own::School { sour } = &mut self.own {
                    *sour = !*sour;
                    let prefix = if *sour { "BG fangirls dissuaded" } else { "BG girls group" };
                    find(&mut self.backdrop, "weeb/bgFreaks").into_iter().for_each(|girls| girls.play(prefix, Motion::Sway(15)));
                }
            }
            "Philly Glow" => {
                let Some(glow) = &mut self.glow else { return };
                let lights_out = glow.color.is_some();
                let mut switched = false;
                match number(&event.value1).unwrap_or(0.0).round() as i32 {
                    // sparks, and the glow jumps up
                    2 => {
                        let count = rand::gen_range(8, 13);
                        let apart = 2000.0 / count as f32;
                        for (row, column) in (0..3).flat_map(|row| (0..count).map(move |column| (row, column))) {
                            glow.sparks.push(Particle {
                                at: vec2(-400.0 + apart * column as f32 + rand::gen_range(-apart / 5.0, apart / 5.0), 425.0 + rand::gen_range(0.0, 125.0) + row as f32 * 40.0),
                                speed: vec2(rand::gen_range(-40.0, 40.0), rand::gen_range(-250.0, -175.0)),
                                pull: vec2(rand::gen_range(-10.0, 10.0), 25.0),
                                scroll: vec2(rand::gen_range(0.3, 0.75), rand::gen_range(0.65, 0.75)),
                                scale: rand::gen_range(0.75, 1.0),
                                life: rand::gen_range(0.6, 0.9),
                                // without flashing lights the sparks are fainter and linger
                                decay: rand::gen_range(0.8, 1.0) * if self.flashing { 1.0 } else { 0.5 },
                                alpha: if self.flashing { 1.0 } else { 0.5 },
                            });
                        }
                        glow.height = 400.0;
                    }
                    // lights out, or another colour
                    1 => {
                        let next = (glow.color.unwrap_or(0) + rand::gen_range(1, WINDOW_LIGHTS.len())) % WINDOW_LIGHTS.len();
                        glow.color = Some(next);
                        let color = Color::from_hex(WINDOW_LIGHTS[next]);
                        switched = !lights_out;
                        if lights_out && self.flashing {
                            self.flash = (Color { a: 0.25, ..color }, 0.0, 0.5);
                        }
                        let on_them = toned(color, if self.flashing { 0.75 } else { 0.5 }, 1.0);
                        for character in [&mut self.opponent, &mut self.gf, &mut self.player].into_iter().flatten() {
                            character.tint = on_them;
                        }
                        find(&mut self.backdrop, "philly/street").into_iter().for_each(|street| street.tint = toned(color, 1.0, 0.5));
                    }
                    // lights on again
                    _ if lights_out => {
                        glow.color = None;
                        switched = true;
                        for character in [&mut self.opponent, &mut self.gf, &mut self.player].into_iter().flatten() {
                            character.tint = WHITE;
                        }
                        find(&mut self.backdrop, "philly/street").into_iter().for_each(|street| street.tint = WHITE);
                    }
                    _ => {}
                }
                if switched {
                    self.flash = (Color { a: if self.flashing { 1.0 } else { 0.5 }, ..WHITE }, 0.0, 0.15);
                    if self.cam_zooms {
                        self.zoom += 0.5;
                        self.hud_zoom += 0.1;
                    }
                }
            }
            "Dadbattle Spotlight" => {
                let Some(spot) = &mut self.spotlight else { return };
                match number(&event.value1).unwrap_or(0.0).round() as i32 {
                    // on (1), or over to the opponent (2) or the player (3)
                    aim @ 1..=3 => {
                        if aim == 1 && !spot.on {
                            spot.on = true;
                            self.default_zoom += 0.12;
                        }
                        (spot.who, spot.since, spot.fog_to) = ((aim > 2) as usize, 0.0, (0.7, 0.7 / 1.5));
                    }
                    _ => {
                        if std::mem::take(&mut spot.on) {
                            self.default_zoom -= 0.12;
                        }
                        spot.fog_to = (0.0, 1.0);
                    }
                }
            }
            // shortcut: the two properties the base game's charts set; any other needs the scripting the Haxe engine does this with.
            "Set Property" => match (event.value1.trim(), number(&event.value2)) {
                ("defaultCamZoom", Some(zoom)) => self.default_zoom = zoom,
                ("cameraSpeed", Some(speed)) => self.camera_speed = speed,
                _ => {}
            },
            "Trigger BG Ghouls" => find(&mut self.backdrop, "weeb/bgGhouls").into_iter().for_each(|ghouls| ghouls.time = 0.0),
            "Hey!" => {
                let hold = number(&event.value2).filter(|t| *t > 0.0).unwrap_or(0.6);
                let only = event.value1.trim().to_lowercase();
                // the crowd at the front of the mall cheers along, unless the cheer is the player's alone
                if let (Own::Mall { hey }, false) = (&mut self.own, matches!(only.as_str(), "bf" | "boyfriend" | "0")) {
                    *hey = hold;
                    find(&mut self.backdrop, "christmas/bottomBop").into_iter().for_each(|crowd| crowd.play("Bottom Level Boppers HEY!!", Motion::Once));
                }
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
        self.position = position;
        if let Some(shooter) = self.gf.as_mut().filter(|shooter| shooter.skip_dance) {
            // every cue is a shot to the left (1, 2) or to the right (3, 4)
            while let Some(&(_, lane)) = self.shots.get(self.shots_done).filter(|(time, _)| *time < position) {
                self.shots_done += 1;
                shooter.play(&format!("shoot{}", if lane > 2 { 3 } else { 1 } + rand::gen_range(0, 2)));
            }
            // between shots the last three frames of the shot go round
            if let Some(animation) = shooter.animations.get(&shooter.current).filter(|_| shooter.finished()) {
                shooter.time = animation.frames.len().saturating_sub(3) as f32 / animation.fps;
            }
        }
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
            // in the fist fight nobody sings: every note is an exchange of moves, and the opponent's notes count as the player's misses
            if let Some(fight) = &mut self.fight {
                let name = match kind {
                    NoteKind::Fight(index) => funkin_core::chart::FIGHT_MOVES.get(index as usize).copied().unwrap_or(""),
                    _ => "",
                };
                let now = [&self.player, &self.opponent].map(|c| c.as_ref().map_or(String::new(), |c| c.current.clone()));
                let (mine, his) = fight.moves(name, missed || side == 0, note.is_none(), (&now[0], &now[1]));
                for (who, moved) in [(1, mine), (0, his)] {
                    let Some(moved) = moved else { continue };
                    // going back to idle does not start an idle move over
                    if let Some(fighter) = self.singer(who).filter(|c| !(moved.pose == "idle" && c.current == "idle")) {
                        fighter.play(&moved.pose);
                    }
                    self.opponent_front = (who == 0) == moved.front;
                    if let Some(shake) = moved.shake {
                        self.shake[0] = shake;
                    }
                }
                continue;
            }
            let alt = kind == NoteKind::Alt || (side == 0 && note.and_then(|n| chart.section_at(n.time)).is_some_and(|s| s.alt));
            match kind {
                NoteKind::NoAnimation => {}
                NoteKind::Hey if !missed => {
                    self.singer(side).into_iter().for_each(|c| c.pose("hey", Some(0.6)));
                    self.gf.iter_mut().for_each(|c| c.pose("cheer", Some(0.6)));
                }
                NoteKind::GfSing if self.gf.is_some() => self.gf.iter_mut().for_each(|c| c.sing(lane, false, alt)),
                // weekend 1: the opponent lights a can, kicks it up and knees it over; the camera closes in on him and swings after the can
                NoteKind::LightCan | NoteKind::KickCan | NoteKind::KneeCan if side == 0 => {
                    let (pose, sound) = match kind {
                        NoteKind::LightCan => ("lightCan", "Darnell_Lighter"),
                        NoteKind::KickCan => ("kickCan", "Kick_Can_UP"),
                        _ => ("kneeCan", "Kick_Can_FORWARD"),
                    };
                    self.opponent.iter_mut().for_each(|c| c.pose(pose, None));
                    self.sounds.extend(self.paths.find(&format!("sounds/{sound}.ogg")).map(|path| (path, 1.0)));
                    if kind == NoteKind::KickCan {
                        self.can.iter_mut().for_each(|can| can.kick());
                    }
                    let on_him = self.opponent.as_ref().map(|c| c.middle() + vec2(150.0, -100.0) + c.camera_offset + self.camera_offsets[0]);
                    match (kind, on_him, self.pinned) {
                        (NoteKind::LightCan, Some(on_him), _) => {
                            (self.pinned, self.camera_speed) = (Some(on_him - vec2(100.0, 0.0)), 2.0);
                            self.default_zoom += 0.1;
                        }
                        (NoteKind::KickCan, _, Some(held)) => {
                            (self.pinned, self.camera_speed, self.unpin_in) = (Some(held + vec2(250.0, 0.0)), 1.5, Some(1.1));
                            self.default_zoom -= 0.1;
                        }
                        _ => {}
                    }
                }
                // and the player cocks the gun and shoots the can down, or is hit by it
                // shortcut: the shell flying off the gun and the stage going dark at the shot are not drawn.
                NoteKind::CockGun | NoteKind::FireGun if side == 1 => {
                    if kind == NoteKind::FireGun && !missed {
                        self.can.iter_mut().for_each(|can| can.shoot());
                    }
                    let (pose, sound) = match (kind, missed) {
                        (NoteKind::FireGun, true) => ("shootMISS", "Pico_Bonk".to_owned()),
                        (NoteKind::FireGun, false) => ("shoot", format!("shots/shot{}", rand::gen_range(1, 5))),
                        (_, true) => ("", String::new()),
                        (_, false) => ("cock", "Gun_Prep".to_owned()),
                    };
                    match pose {
                        "" => self.player.iter_mut().for_each(|c| c.sing(lane, true, false)),
                        pose => {
                            self.player.iter_mut().for_each(|c| c.pose(pose, None));
                            self.sounds.extend(self.paths.find(&format!("sounds/{sound}.ogg")).map(|path| (path, 1.0)));
                        }
                    }
                }
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
        self.flash.1 += dt;
        self.can.iter_mut().for_each(|can| can.update(dt));
        if let Some(abot) = &mut self.abot {
            abot.update(dt, self.levels, chart.section_at(position).is_some_and(|section| section.must_hit));
            // it stands in the same light as the girlfriend on it
            abot.tint = self.gf.as_ref().map_or(WHITE, |gf| gf.tint);
        }
        if let Some(left) = &mut self.unpin_in {
            *left -= dt;
            if *left <= 0.0 {
                (self.unpin_in, self.pinned, self.camera_speed) = (None, None, 1.0);
            }
        }
        if let Some(glow) = &mut self.glow {
            glow.height = (glow.height - 1000.0 * dt).max(0.0);
            for spark in &mut glow.sparks {
                spark.life -= dt;
                if spark.life < 0.0 {
                    spark.alpha -= spark.decay * dt;
                }
                spark.at += spark.speed * dt;
                spark.speed += spark.pull * dt;
            }
            glow.sparks.retain(|spark| spark.alpha > 0.0);
        }
        if let Some(spot) = &mut self.spotlight {
            spot.since += dt;
            let (target, rate) = spot.fog_to;
            spot.fog += (target - spot.fog).clamp(-rate * dt, rate * dt);
            for (at, _, speed) in &mut spot.drifts {
                at.x += *speed * dt;
            }
        }

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
            Own::Mall { hey } => {
                if *hey > 0.0 {
                    *hey -= dt;
                    if *hey <= 0.0 {
                        find(&mut self.backdrop, "christmas/bottomBop").into_iter().for_each(|crowd| crowd.play("Bottom Level Boppers Idle", Motion::Beat));
                    }
                }
            }
            Own::School { .. } => {}
            Own::Blazin { drift, strike_in, since } => {
                (*drift, *strike_in, *since) = (*drift + 35.0 * dt, *strike_in - dt, *since + dt);
                let skies: Vec<usize> = self.backdrop.iter().enumerate().filter(|(_, layer)| layer.def.image == "phillyBlazin/skyBlur").map(|(index, _)| index).collect();
                let [first, second, third, glare] = skies[..] else { return };
                // three skies side by side drift past, coming round as each leaves
                let width = self.backdrop[first].texture.width();
                for (place, sky) in [first, second, third].into_iter().enumerate() {
                    self.backdrop[sky].def.at.x = -500.0 - *drift % width + place as f32 * width;
                }
                if *strike_in <= 0.0 {
                    (*strike_in, *since) = (rand::gen_range(7.0, 15.0), 0.0);
                    sound(&format!("lightning/Lightning{}", rand::gen_range(1, 4)), 1.0);
                    if let Some(bolt) = find(&mut self.backdrop, "phillyBlazin/lightning") {
                        // mostly on the left, sometimes on the right
                        (bolt.def.at.x, bolt.time) = (if chance(0.65) { rand::gen_range(-250.0, 280.0) } else { rand::gen_range(780.0, 900.0) }, 0.0);
                    }
                }
                // the sky glares and fades; everyone is a dark shape for a moment
                self.backdrop[glare].tint.a = (0.7 * (1.0 - *since / 1.5)).max(0.0);
                let lit = (*since / 0.3).min(1.0);
                for (who, character) in [&mut self.opponent, &mut self.player, &mut self.gf].into_iter().enumerate() {
                    let usual = if who == 2 { 0x88 } else { 0xDE } as f32;
                    let shade = (0x60 as f32 + (usual - 0x60 as f32) * lit) / 255.0;
                    character.iter_mut().for_each(|c| c.tint = Color::new(shade, shade, shade, 1.0));
                }
            }
            Own::Streets { red, changed, every, cars } => {
                const CARS: [&str; 4] = ["car1", "car2", "car3", "car4"];
                let layers: Vec<usize> = self.backdrop.iter().enumerate().filter(|(_, layer)| layer.def.image == "phillyStreets/phillyCars").map(|(index, _)| index).collect();
                let [Some(&here), Some(&there)] = [layers.first(), layers.get(1)] else { return };
                let b = &mut self.backdrop;
                // a car sets off in one of four looks, some drawn a little off its spot
                let mut set_off = |car: &mut Car, layer: &mut Layer, kind: usize| {
                    let variant = rand::gen_range(1, 5);
                    layer.play(CARS[variant - 1], Motion::Still);
                    (car.drive, car.shift) = (Some(Drive::new(kind, variant)), [Vec2::ZERO, vec2(20.0, -15.0), vec2(30.0, 50.0), vec2(10.0, 60.0)][variant - 1]);
                };
                if let Some(beat) = beat {
                    let due = beat == *changed + *every;
                    if chance(0.1) && !due && cars[0].drive.is_none() && !cars[0].waiting {
                        set_off(&mut cars[0], &mut b[here], *red as usize);
                    }
                    if chance(0.1) && !due && cars[1].drive.is_none() && !*red {
                        set_off(&mut cars[1], &mut b[there], 3);
                    }
                    if due {
                        (*changed, *red) = (beat, !*red);
                        *every = if *red { 20 } else { 30 };
                        find(b, "phillyStreets/phillyTraffic").into_iter().for_each(|light| light.play(if *red { "greentored" } else { "redtogreen" }, Motion::Once));
                    }
                }
                // the car at the light leaves when it is green
                if cars[0].waiting && !*red {
                    (cars[0].waiting, cars[0].drive) = (false, Some(Drive::new(2, 1)));
                }
                for (car, layer) in cars.iter_mut().zip([here, there]) {
                    let Some(drive) = &mut car.drive else { continue };
                    drive.time += dt;
                    let (at, turn, arrived) = drive.at();
                    (b[layer].def.at, b[layer].angle) = (at - car.shift, turn.to_radians());
                    if arrived {
                        car.waiting = drive.to_the_light;
                        car.drive = None;
                    }
                }
            }
            Own::Limo { speed, back_in, kill, limo_speed } => {
                let all = |backdrop: &[Layer], image: &str| backdrop.iter().enumerate().filter(|(_, layer)| layer.def.image == image).map(|(index, _)| index).collect::<Vec<_>>();
                let b = &mut self.backdrop;
                // corpses on the rail and under it, then the flying parts of the first and of the fourth dancer
                let (gore, blood, dancers) = (all(b, "gore/noooooo"), all(b, "gore/stupidBlood"), all(b, "limo/limoDancer"));
                let single = |image: &str| all(b, image).first().copied();
                if let (Some(pole), Some(lamp), Some(limo), true) = (single("gore/metalPole"), single("gore/coldHeartKiller"), single("limo/bgLimo"), gore.len() == 8 && blood.len() == 2 && dancers.len() == 5) {
                    match *kill {
                        Kill::Wait => {}
                        Kill::Sweep => {
                            b[pole].def.at.x += 5000.0 * dt;
                            let x = b[pole].def.at.x - 180.0;
                            (b[lamp].def.at.x, b[gore[0]].def.at.x, b[gore[1]].def.at.x) = (x, x - 50.0, x + 35.0);
                            for (place, &dancer) in dancers.iter().enumerate() {
                                let at = b[dancer].def.at;
                                if at.x >= SCREEN.x * 1.5 || x <= 370.0 * place as f32 + 170.0 {
                                    continue;
                                }
                                match place {
                                    0 | 3 => {
                                        if place == 0 {
                                            sound("dancerdeath", 0.5);
                                        }
                                        // a leg, an arm and the head fly off, and blood
                                        for (part, shift) in [vec2(200.0, 0.0), vec2(160.0, 200.0), vec2(0.0, 50.0)].into_iter().enumerate() {
                                            let flying = &mut b[gore[2 + place + part]];
                                            (flying.def.at, flying.time) = (at + shift, 0.0);
                                        }
                                        let splash = &mut b[blood[place / 3]];
                                        (splash.def.at, splash.time) = (at + vec2(-110.0, 20.0), 0.0);
                                    }
                                    // the second stays on the rail, the third under it
                                    1 | 2 => b[gore[place - 1]].tint.a = 1.0,
                                    _ => {}
                                }
                                b[dancer].def.at.x += SCREEN.x * 2.0;
                            }
                            if b[pole].def.at.x > SCREEN.x * 2.0 {
                                for gone in [pole, lamp, gore[0], gore[1]] {
                                    (b[gone].tint.a, b[gone].def.at.x) = (0.0, -500.0);
                                }
                                (*limo_speed, *kill) = (800.0, Kill::Leaving);
                            }
                        }
                        Kill::Leaving => {
                            *limo_speed -= 4000.0 * dt;
                            b[limo].def.at.x -= *limo_speed * dt;
                            if b[limo].def.at.x > SCREEN.x * 1.5 {
                                (*limo_speed, *kill) = (3000.0, Kill::Returning);
                            }
                        }
                        Kill::Returning | Kill::Settling => {
                            let x = &mut b[limo].def.at.x;
                            if *kill == Kill::Returning {
                                *limo_speed = (*limo_speed - 2000.0 * dt).max(1000.0);
                                *x -= *limo_speed * dt;
                                if *x < -275.0 {
                                    *kill = Kill::Settling;
                                }
                            } else {
                                *x = -150.0 + (*x + 150.0) * (-dt * 9.0).exp();
                                if x.round() == -150.0 {
                                    (*x, *kill) = (-150.0, Kill::Wait);
                                }
                            }
                            // the new dancers ride in on it
                            let limo_x = *x;
                            for (place, &dancer) in dancers.iter().enumerate() {
                                b[dancer].def.at.x = 370.0 * place as f32 + 320.0 + limo_x;
                            }
                        }
                    }
                }
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

    /// Who lies there when the player has lost, the sound of losing, and what is added to the names of the game over music.
    // shortcut: by stage, as the base game's stages set it; charts and scripts can name another game over character (gameOverChar). Read it from the chart when one needs it.
    pub fn death(&self) -> (&'static str, &'static str, &'static str) {
        match self.stage.as_str() {
            _ if self.pixel => ("bf-pixel-dead", "fnf_loss_sfx-pixel", "-pixel"),
            "phillyStreets" => ("pico-dead", "fnf_loss_sfx-pico", "-pico"),
            "phillyBlazin" => ("pico-blazin", "fnf_loss_sfx-pico-gutpunch", "-pico"),
            _ => ("bf-dead", "fnf_loss_sfx", ""),
        }
    }

    /// The player lost: swaps in the fallen character and turns the camera to it.
    pub fn die(&mut self) {
        self.dead = true;
        if let Ok(mut fallen) = Character::load(&self.paths, self.death().0, self.spots[1], true) {
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
        // everything the camera can see, and more
        let view = SCREEN / self.zoom;
        let cover = |color: Color| draw_rectangle(self.camera.x - view.x, self.camera.y - view.y, view.x * 2.0, view.y * 2.0, color);
        for layer in self.backdrop.iter().filter(|layer| !layer.def.front) {
            // with the lights out the street is all that stays in front of the dark, the glow, the sparks and the lit windows
            if let (Some(glow), "philly/street") = (&self.glow, layer.def.image) {
                if let Some(color) = glow.color.map(|index| Color::from_hex(WINDOW_LIGHTS[index])) {
                    cover(BLACK);
                    if glow.height > 0.0 {
                        let at = vec2(-400.0 + scroll.x, 625.0 - glow.height + scroll.y * 0.25);
                        draw_texture_ex(&glow.gradient, at.x, at.y, color, DrawTextureParams { dest_size: Some(vec2(2000.0, glow.height)), ..Default::default() });
                    }
                    for spark in &glow.sparks {
                        // a fading spark shrinks around its middle
                        let size = glow.spark.size() * spark.scale * if spark.life < 0.0 { spark.alpha } else { 1.0 };
                        let at = spark.at + scroll * (Vec2::ONE - spark.scroll) + (glow.spark.size() - size) / 2.0;
                        draw_texture_ex(&glow.spark, at.x, at.y, Color { a: spark.alpha, ..color }, DrawTextureParams { dest_size: Some(size), ..Default::default() });
                    }
                    self.backdrop.iter().filter(|layer| layer.def.image == "philly/window").for_each(|window| window.draw_in(scroll, color));
                }
            }
            // the soldiers run behind the hill's ground
            if let (Some(soldiers), "tankGround") = (&self.soldiers, layer.def.image) {
                for soldier in &soldiers.list {
                    let falling = &soldiers.falling[soldier.fall];
                    if let Some((at, fell, frame)) = soldier.at(self.position, soldiers.running.len(), falling.len()) {
                        let frames = if fell { falling } else { &soldiers.running };
                        soldiers.sheet.draw(frames[frame], at.x, at.y, 0.8, soldier.right, WHITE);
                    }
                }
            }
            layer.draw(scroll);
        }
        if let Some(spot) = &self.spotlight {
            // shortcut: the light and the smoke are laid over the stage; the Haxe engine adds their brightness to it.
            if spot.on {
                cover(Color::new(0.0, 0.0, 0.0, 0.25));
                // the light goes out for a moment whenever it turns to someone
                if let Some(lit) = [&self.opponent, &self.player][spot.who].as_ref().filter(|_| spot.since >= 0.12) {
                    let at = vec2(lit.middle().x - spot.light.width() / 2.0, lit.position.y + lit.size.y - spot.light.height() + 50.0);
                    draw_texture(&spot.light, at.x, at.y, Color::new(1.0, 1.0, 1.0, 0.375));
                }
            }
            for (side, (at, scale, _)) in spot.drifts.iter().enumerate().filter(|_| spot.fog > 0.0) {
                let at = *at + scroll * (Vec2::ONE - vec2(1.2, 1.05));
                let params = DrawTextureParams { dest_size: Some(spot.smoke.size() * *scale), flip_x: side == 1, ..Default::default() };
                draw_texture_ex(&spot.smoke, at.x, at.y, Color::new(1.0, 1.0, 1.0, spot.fog), params);
            }
        }
        self.abot.iter().for_each(|abot| abot.draw());
        // whoever throws the punch stands in front
        let fighters = if self.opponent_front { [&self.gf, &self.player, &self.opponent] } else { [&self.gf, &self.opponent, &self.player] };
        for character in fighters.into_iter().flatten() {
            character.draw();
        }
        self.can.iter().for_each(|can| can.draw());
        self.backdrop.iter().filter(|layer| layer.def.front).for_each(|layer| layer.draw(scroll));
        let (color, age, length) = self.flash;
        if age < length {
            cover(Color { a: color.a * (1.0 - age / length), ..color });
        }
        if let Own::Spooky { since, .. } = self.own {
            // the lightning whitens everything: up at once, held, then gone
            let alpha = match since {
                t if t < 0.075 => 0.4 + 0.1 * t / 0.075,
                t if t < 0.15 => 0.5,
                t => 0.5 * (1.0 - (t - 0.15) / 0.25),
            };
            if alpha > 0.0 && self.flashing {
                cover(Color::new(1.0, 1.0, 1.0, alpha));
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
    fn a_soldier_runs_in_is_shot_and_falls() {
        use super::Soldier;
        let soldier = Soldier { shot_at: 10_000.0, y: 250.0, right: true, short: 100.0, speed: 1.0, fall: 0, step: 3 };
        // from the left: out of sight at first, then running, 14 frames going round
        assert_eq!(soldier.at(5_000.0, 14, 20), None);
        let (at, fell, frame) = soldier.at(9_900.0, 14, 20).unwrap();
        assert_eq!((at.x, at.y, fell, frame), (0.02 * 1280.0 - 100.0 - 100.0, 250.0, false, (3 + 237) % 14));
        // shot where he got to: he falls there (drawn up and left of where he ran), frame by frame, and is gone
        let (at, fell, frame) = soldier.at(10_500.0, 14, 20).unwrap();
        assert_eq!((at.x, at.y, fell, frame), (0.02 * 1280.0 - 100.0 - 249.5, 250.0 - 152.2, true, 12));
        assert_eq!(soldier.at(10_900.0, 14, 20), None);
        // from the right he comes the other way and falls where he stands
        let other = Soldier { right: false, ..soldier };
        assert_eq!(other.at(9_900.0, 14, 20).unwrap().0.x, 0.74 * 1280.0 + 100.0 + 100.0);
        assert_eq!(other.at(10_001.0, 14, 20).unwrap().0, macroquad::prelude::vec2(0.74 * 1280.0 + 100.0, 250.0));
    }

    #[test]
    fn a_car_follows_its_bend_and_stops_at_the_red_light() {
        use super::{Drive, Ease};
        let mut drive = Drive::new(1, 1);
        assert!(drive.to_the_light && drive.ease == Ease::Slowing && (1.0..=1.7).contains(&drive.seconds));
        let (start, turn, arrived) = drive.at();
        assert_eq!((start, turn, arrived), (drive.path[0], -7.0, false));
        drive.time = drive.seconds / 2.0;
        // slowing down, it is past the middle of its way at half time
        assert!(drive.at().0.x > (drive.path[0].x + drive.path[2].x) / 2.0);
        drive.time = drive.seconds;
        assert_eq!(drive.at(), (drive.path[2], -5.0, true));
        // away from the light it first stands a moment
        let mut off = Drive::new(2, 1);
        off.time = off.wait;
        assert_eq!((off.at().0, off.path[0]), (drive.path[2], drive.path[2]));
    }

    #[test]
    fn colours_dim_and_pale_without_changing_hue() {
        use super::toned;
        use macroquad::prelude::Color;
        let red = Color::new(1.0, 0.0, 0.0, 1.0);
        assert_eq!((toned(red, 0.75, 1.0), toned(red, 1.0, 0.5)), (Color::new(1.0, 0.25, 0.25, 1.0), Color::new(0.5, 0.0, 0.0, 1.0)));
        let grey = Color::new(0.4, 0.4, 0.4, 1.0);
        assert_eq!(toned(grey, 0.5, 1.0), grey);
    }

    #[test]
    fn events_name_singers_by_word_or_number() {
        assert_eq!(["dad", "", "0", "BF", " boyfriend ", "1", "gf", "Girlfriend", "2"].map(super::who), [0, 0, 0, 1, 1, 1, 2, 2, 2]);
    }
}
