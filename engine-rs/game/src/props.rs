//! Things that stand on the weekend 1 stages: the speaker the girlfriend sits on, and the spray can that gets kicked about.
use crate::animate::Atlas;
use crate::atlas::Sheet;
use funkin_core::content::Paths;
use macroquad::prelude::*;

/// Frame of a once-through animation `seconds` in, at 24 frames a second; None when it is over.
fn step(first: usize, last: usize, seconds: f32) -> Option<usize> {
    let frame = first + (seconds.max(0.0) * 24.0) as usize;
    (frame <= last).then_some(frame)
}

/// The speaker with a face: bars that follow the music, eyes that follow whoever sings, and a body that thumps on loud moments.
pub struct ABot {
    at: Vec2,
    back: Option<Texture2D>,
    bars: Option<Sheet>,
    eyes: Option<Atlas>,
    body: Option<Atlas>,
    /// Looking at the player's side, and for how long it has been.
    right: bool,
    looked: f32,
    /// Seconds since the body last thumped.
    thumped: f32,
    /// How high each bar stands, 0 to 5, and the highest of them a frame ago.
    heights: [usize; 7],
    highest: usize,
    /// The light on it (white: as drawn).
    pub tint: Color,
}

impl ABot {
    /// `spot` is where the stage puts the girlfriend.
    pub fn load(paths: &Paths, spot: Vec2) -> ABot {
        let atlas = |name: &str| paths.find(&format!("images/abot/{name}/Animation.json")).and_then(|file| Atlas::load(file.parent()?).map_err(|error| eprintln!("{error}")).ok());
        ABot {
            at: spot + vec2(0.0, 550.0),
            back: paths.find("images/abot/stereoBG.png").and_then(|path| std::fs::read(path).ok()).map(|bytes| crate::atlas::texture(&bytes)),
            bars: paths.find("images/abot/aBotViz.png").and_then(|path| Sheet::load(&path).ok()),
            eyes: atlas("systemEyes"),
            body: atlas("abotSystem"),
            right: true, looked: 1e9, thumped: 1e9, heights: [0; 7], highest: 0, tint: WHITE,
        }
    }

    /// `levels`: how loud seven bands of the music are (0 to 1). `player_sings`: whether the section is the player's.
    pub fn update(&mut self, dt: f32, levels: [f32; 7], player_sings: bool) {
        (self.looked, self.thumped) = (self.looked + dt, self.thumped + dt);
        if player_sings != self.right {
            (self.right, self.looked) = (player_sings, 0.0);
        }
        self.heights = levels.map(|level| ((level * 5.0).round() as usize).min(5));
        let highest = self.heights.into_iter().max().unwrap_or(0);
        // a loud moment that is no quieter than the last frame's makes the body thump, unless it is still in the middle of one
        if highest >= 4 && self.highest <= highest && (highest >= 5 || self.thumped * 24.0 >= 3.0) {
            self.thumped = 0.0;
        }
        self.highest = highest;
    }

    pub fn draw(&self) {
        if let Some(back) = &self.back {
            draw_texture(back, self.at.x + 90.0, self.at.y + 20.0, self.tint);
        }
        if let Some(bars) = &self.bars {
            // each bar sits a little further right and lower or higher than the one before
            const STEPS: [(f32, f32); 7] = [(0.0, 0.0), (59.0, -8.0), (56.0, -3.5), (66.0, -0.4), (54.0, 0.5), (52.0, 4.7), (51.0, 7.0)];
            let mut at = self.at + vec2(140.0, 74.0);
            for (bar, step) in STEPS.iter().enumerate() {
                at += vec2(step.0, step.1);
                // the sheet has the tallest bar first
                if let Some(frame) = bars.animation(&format!("viz{}", bar + 1), &[]).get(5 - self.heights[bar]) {
                    bars.draw(*frame, at.x, at.y, 1.0, false, self.tint);
                }
            }
        }
        draw_rectangle(self.at.x - 30.0, self.at.y + 215.0, 160.0, 60.0, self.tint);
        if let Some(eyes) = &self.eyes {
            // the first half of the timeline turns the eyes left, the second half right
            let (first, last) = if self.right { (18, 35) } else { (0, 17) };
            eyes.draw(step(first, last, self.looked).unwrap_or(last), self.at + vec2(-10.0, 230.0), 1.0, false, self.tint);
        }
        if let Some(body) = &self.body {
            let last = body.rig.length().saturating_sub(1);
            body.draw(step(0, last, self.thumped).unwrap_or(last), self.at + vec2(-65.0, -10.0), 1.0, false, self.tint);
        }
    }
}

/// What the can is doing, and for how many seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Flight {
    /// Kicked up and coming down.
    Up(f32),
    /// Not shot down: it hits the player,
    Hits(f32),
    /// and bursts.
    Bursts(f32),
    /// Shot out of the air.
    Shot(f32),
}

impl Flight {
    /// A moment later. None when the can is gone.
    fn after(self, dt: f32) -> Option<Flight> {
        // how long each part lasts: its frames at 24 a second
        let over = |seconds: f32, frames: usize| seconds * 24.0 >= frames as f32;
        match self {
            Flight::Up(t) if over(t + dt, 19) => Some(Flight::Hits(0.0)),
            Flight::Up(t) => Some(Flight::Up(t + dt)),
            Flight::Hits(t) if over(t + dt, 7) => Some(Flight::Bursts(0.0)),
            Flight::Hits(t) => Some(Flight::Hits(t + dt)),
            Flight::Bursts(t) => Some(Flight::Bursts(t + dt)).filter(|_| !over(t + dt, 14)),
            Flight::Shot(t) => Some(Flight::Shot(t + dt)).filter(|_| !over(t + dt, 17)),
        }
    }
}

/// The spray can of the weekend 1 street: kicked up by the opponent, shot down by the player or bursting on him.
pub struct Can {
    can: Atlas,
    burst: Option<Sheet>,
    flight: Option<Flight>,
}

impl Can {
    const AT: Vec2 = vec2(1450.0, 805.0);

    pub fn load(paths: &Paths) -> Option<Can> {
        let file = paths.find("images/spraycanAtlas/Animation.json")?;
        let can = Atlas::load(file.parent()?).map_err(|error| eprintln!("{error}")).ok()?;
        Some(Can { can, burst: paths.find("images/spraypaintExplosionEZ.png").and_then(|path| Sheet::load(&path).ok()), flight: None })
    }

    pub fn kick(&mut self) {
        self.flight = Some(Flight::Up(0.0));
    }

    /// The player's shot: it only counts while the can is in the air.
    pub fn shoot(&mut self) {
        if matches!(self.flight, Some(Flight::Up(_))) {
            self.flight = Some(Flight::Shot(0.0));
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.flight = self.flight.and_then(|flight| flight.after(dt));
    }

    pub fn draw(&self) {
        // the can's timeline: kicked up (0 to 18), hitting the player (19 to 25), shot (26 to 42)
        let frame = match self.flight {
            Some(Flight::Up(t)) => step(0, 18, t),
            Some(Flight::Hits(t)) => step(19, 25, t),
            Some(Flight::Shot(t)) => step(26, 42, t),
            _ => None,
        };
        if let Some(frame) = frame {
            self.can.draw(frame, Can::AT, 1.0, false, WHITE);
        }
        if let (Some(Flight::Bursts(t)), Some(burst)) = (self.flight, &self.burst) {
            if let Some(frame) = burst.animation("explosion round 1 short0", &[]).get((t * 24.0) as usize) {
                burst.draw(*frame, Can::AT.x - 25.0, Can::AT.y - 450.0, 1.0, false, WHITE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_can_left_alone_comes_down_on_the_player_and_bursts() {
        let mut flight = Some(Flight::Up(0.0));
        let mut seen = Vec::new();
        for _ in 0..200 {
            flight = flight.and_then(|flight| flight.after(1.0 / 60.0));
            let part = flight.map(|flight| std::mem::discriminant(&flight));
            if seen.last() != Some(&part) {
                seen.push(part);
            }
        }
        let name = |flight: Flight| Some(std::mem::discriminant(&flight));
        assert_eq!(seen, [name(Flight::Up(0.0)), name(Flight::Hits(0.0)), name(Flight::Bursts(0.0)), None]);
        // shot down, it just falls away
        assert!(matches!(Flight::Shot(0.0).after(0.5), Some(Flight::Shot(_))));
        assert_eq!(Flight::Shot(0.7).after(0.1), None);
        assert_eq!((step(26, 42, 0.5), step(26, 42, 1.0)), (Some(38), None));
    }
}
