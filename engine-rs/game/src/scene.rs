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
        let at = self.position - animation.offset;
        self.sheet.draw(animation.frames[frame], at.x, at.y, self.scale, self.flip_x, WHITE);
    }
}

pub struct Scene {
    paths: Paths,
    /// Where the stage puts the opponent, the player and the girlfriend.
    spots: [Vec2; 3],
    /// The player lost: only the fallen character is shown.
    dead: bool,
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
    /// The girlfriend moves every this many of her beats (a chart event slows her down).
    gf_speed: i64,
    /// Where a chart event pinned the camera.
    pinned: Option<Vec2>,
    /// Shaking of the stage and of the HUD: strength (share of the screen) and seconds left.
    shake: [(f32, f32); 2],
    /// Characters a chart event will swap in, loaded ahead so that the swap does not stall the song.
    spares: HashMap<String, Character>,
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
        let backdrop = backdrop(&chart.stage).iter().filter_map(|&(image, x, y, scroll, scale, flip)| Some((texture(&paths, &format!("images/{image}.png"))?, vec2(x, y), scroll, scale, flip))).collect();
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
        };
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
                if self.bumping && self.zoom < 1.35 {
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

        let beat = chart.beat_at(position).floor() as i64;
        if beat != self.beat {
            self.beat = beat;
            self.icon_scale = 1.2;
            if self.bumping && beat % 4 == 0 {
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

    /// The player lost: swaps in the fallen character and turns the camera to it.
    pub fn die(&mut self) {
        self.dead = true;
        // shortcut: always "bf-dead"; charts and scripts can name another game over character (gameOverChar). Read it from the chart when one needs it.
        if let Ok(mut fallen) = Character::load(&self.paths, "bf-dead", self.spots[1], true) {
            fallen.play("firstDeath");
            self.target = fallen.middle();
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

#[cfg(test)]
mod tests {
    #[test]
    fn events_name_singers_by_word_or_number() {
        assert_eq!(["dad", "", "0", "BF", " boyfriend ", "1", "gf", "Girlfriend", "2"].map(super::who), [0, 0, 0, 1, 1, 1, 2, 2, 2]);
    }
}
