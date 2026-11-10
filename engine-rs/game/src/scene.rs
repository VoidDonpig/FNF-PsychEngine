//! What is behind the notes: the stage, the three characters and the camera that follows whoever sings.
use crate::atlas::{Frame, Sheet};
use funkin_core::chart::Chart;
use funkin_core::content::Paths;
use funkin_core::play::Event;
use macroquad::prelude::*;
use serde_json::Value;
use std::collections::HashMap;

const SING: [&str; 4] = ["singLEFT", "singDOWN", "singUP", "singRIGHT"];
const SCREEN: Vec2 = vec2(1280.0, 720.0);

/// Backdrop pictures of a stage, back to front: image, x, y, scroll factor, scale, mirrored.
// shortcut: only the week 1 stage has its backdrop; the others are still code in source/states/stages. Add them here (or read them from the stage file once it lists its objects).
fn backdrop(stage: &str) -> &'static [(&'static str, f32, f32, f32, f32, bool)] {
    match stage {
        "stage" => &[
            ("stageback", -600.0, -200.0, 0.9, 1.0, false),
            ("stagefront", -650.0, 600.0, 0.9, 1.1, false),
            ("stage_light", -125.0, -100.0, 0.9, 1.1, false),
            ("stage_light", 1225.0, -100.0, 0.9, 1.1, true),
            ("stagecurtains", -500.0, -300.0, 1.3, 0.9, false),
        ],
        _ => &[],
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
        let size = animations.get(&current).or(animations.values().next()).map_or(Vec2::ZERO, |a| a.frames[0].size * scale);
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
            camera_offset: pair(&json["camera_position"]),
            sing_steps: json["sing_duration"].as_f64().unwrap_or(4.0) as f32,
            current,
            time: 0.0,
            since_note: 0.0,
            sways,
            swayed_right: false,
            icon,
            color: Color::from_rgba(channel(0), channel(1), channel(2), 255),
        })
    }

    fn play(&mut self, animation: &str) {
        if self.animations.contains_key(animation) {
            self.current = animation.to_owned();
            self.time = 0.0;
        }
    }

    fn singing(&self) -> bool {
        self.current.starts_with("sing")
    }

    fn sing(&mut self, lane: usize, missed: bool) {
        self.play(&format!("{}{}", SING[lane], if missed { "miss" } else { "" }));
        self.since_note = 0.0;
    }

    /// The idle move, restarted on the beat.
    fn dance(&mut self) {
        if self.sways {
            self.swayed_right = !self.swayed_right;
            self.play(if self.swayed_right { "danceRight" } else { "danceLeft" });
        } else {
            self.play("idle");
        }
    }

    fn middle(&self) -> Vec2 {
        self.position + self.size / 2.0
    }

    fn draw(&self) {
        let Some(animation) = self.animations.get(&self.current) else { return };
        let frame = (self.time * animation.fps) as usize;
        let frame = if animation.looped { frame % animation.frames.len() } else { frame.min(animation.frames.len() - 1) };
        let at = self.position - animation.offset;
        self.sheet.draw(animation.frames[frame], at.x, at.y, self.scale, self.flip_x, WHITE);
    }
}

pub struct Scene {
    backdrop: Vec<(Texture2D, Vec2, f32, f32, bool)>,
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
}

impl Scene {
    pub fn load(base: &std::path::Path, source: &str, chart: &Chart) -> Result<Scene, String> {
        let stage = read_json(&Paths::new(base, source, ""), &format!("stages/{}.json", chart.stage))?;
        let paths = Paths::new(base, source, stage["directory"].as_str().unwrap_or(""));
        let backdrop = backdrop(&chart.stage).iter().filter_map(|&(image, x, y, scroll, scale, flip)| Some((texture(&paths, &format!("images/{image}.png"))?, vec2(x, y), scroll, scale, flip))).collect();
        let character = |name: &str, spot: &str, is_player: bool| Character::load(&paths, name, pair(&stage[spot]), is_player).map_err(|e| eprintln!("Character {name}: {e}")).ok();
        // an opponent that is the girlfriend (the tutorial) stands in her place, and she is not drawn twice
        let opponent_is_gf = chart.opponent.starts_with("gf");
        let opponent = character(&chart.opponent, if opponent_is_gf { "girlfriend" } else { "opponent" }, false);
        let gf = if opponent_is_gf || stage["hide_girlfriend"].as_bool().unwrap_or(false) { None } else { character(&chart.gf, "girlfriend", false) };
        let player = character(&chart.player, "boyfriend", true);
        let default_zoom = stage["defaultZoom"].as_f64().unwrap_or(1.05) as f32;
        let mut scene = Scene {
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
        };
        scene.look(chart, 0.0);
        scene.camera = scene.target;
        Ok(scene)
    }

    /// Points the camera at whoever the current section belongs to.
    fn look(&mut self, chart: &Chart, position: f64) {
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
            match *event {
                Event::Hit { lane, .. } => self.player.iter_mut().for_each(|c| c.sing(lane, false)),
                Event::Miss { lane } => self.player.iter_mut().for_each(|c| c.sing(lane, true)),
                Event::OpponentHit { lane } => {
                    self.opponent.iter_mut().for_each(|c| c.sing(lane, false));
                    self.bumping = true;
                }
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
        }

        let beat = chart.beat_at(position).floor() as i64;
        if beat != self.beat {
            self.beat = beat;
            self.icon_scale = 1.2;
            if self.bumping && beat % 4 == 0 {
                self.zoom += 0.015;
                self.hud_zoom += 0.03;
            }
            for character in [&mut self.opponent, &mut self.gf, &mut self.player].into_iter().flatten() {
                if !character.singing() && (character.sways || beat % 2 == 0) {
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

    pub fn draw(&self) {
        set_camera(&Camera2D { target: self.camera, zoom: vec2(2.0 / SCREEN.x, 2.0 / SCREEN.y) * self.zoom, ..Default::default() });
        let scroll = self.camera - SCREEN / 2.0;
        for (texture, position, factor, scale, flip_x) in &self.backdrop {
            // a scroll factor below 1 makes a layer follow the camera partly, so it looks further away
            let at = *position + scroll * (1.0 - factor);
            draw_texture_ex(texture, at.x, at.y, WHITE, DrawTextureParams { dest_size: Some(texture.size() * *scale), flip_x: *flip_x, ..Default::default() });
        }
        for character in [&self.gf, &self.opponent, &self.player].into_iter().flatten() {
            character.draw();
        }
    }
}
