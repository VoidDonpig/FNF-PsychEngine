//! The Haxe engine's own dialogue (cutscenes/DialogueBoxPsych.hx), as scripts start it: a speech bubble, the speakers'
//! portraits sliding in from their side (`images/dialogue/<name>.json`), and lines typed out in the plain menu letters.
//! Its lines come from a JSON file: portrait, expression, text, boxState ("normal" or "angry"), speed, sound.
use crate::atlas::{Frame, Sheet};
use crate::gfx::*;
use crate::menu::{place, Alphabet};
use funkin_core::content::Paths;
use serde_json::Value;
use std::path::Path;

/// Something the play screen is to do for the dialogue.
#[derive(Debug, PartialEq)]
pub enum Cue {
    Sound(String),
    /// The music to play under it, rising from silence over two seconds.
    Music(String),
    /// The music fades away.
    Quiet,
    /// Scripts hear of it: onNextDialogue, onSkipDialogue (with how many lines came after the first).
    Next(usize),
    Skip(usize),
}

struct Line {
    portrait: String,
    expression: String,
    text: String,
    angry: bool,
    speed: f32,
    sound: String,
}

/// An expression of a portrait: its talking frames (played once, again and again while the line is typed) and its
/// resting ones, each with its offset.
struct Expression {
    talking: Vec<Frame>,
    resting: Vec<Frame>,
    offsets: [Vec2; 2],
}

struct Portrait {
    name: String,
    sheet: Sheet,
    expressions: Vec<(String, Expression)>,
    side: Side,
    scale: f32,
    /// Flixel's origin: the middle of the sheet's first frame, which the portrait is scaled around.
    origin: Vec2,
    at: Vec2,
    /// Where it stands once in (its x, or for the middle one its y).
    resting_at: f32,
    alpha: f32,
    /// The expression showing, resting (true) or talking, since when (seconds), at how many frames a second.
    showing: (usize, bool, f32, f32),
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Left,
    Middle,
    Right,
}

/// How far portraits wait off their side, and how fast they slide (DialogueBoxPsych's offsetPos and scrollSpeed).
const AWAY: f32 = 600.0;
const SLIDE: f32 = 4000.0;
const TEXT_AT: Vec2 = vec2(175.0, 460.0);
const TEXT_SCALE: f32 = 0.7;

pub struct Speech {
    lines: Vec<Line>,
    /// The next line to say.
    next: usize,
    bubble: Sheet,
    /// The bubble's animation (DialogueBoxPsych's names), its frames, since when, and whether it plays backwards (closing).
    bubble_shown: Option<(String, Vec<Frame>, f32, bool)>,
    flip: bool,
    portraits: Vec<Portrait>,
    speaking: Option<usize>,
    alphabet: Alphabet,
    /// What is being typed: its letters, how many show, seconds toward the next, done.
    letters: Vec<(char, Vec2)>,
    rows: usize,
    shown: usize,
    waited: f32,
    typed: bool,
    white: f32,
    ended: bool,
    /// The first frame is let pass, as the Haxe engine's (it loads pictures on it).
    first: bool,
    /// Lines said after the first, for the scripts.
    count: usize,
    /// Seconds since it began, while music plays under it.
    music: Option<f32>,
    pub cues: Vec<Cue>,
}

impl Speech {
    /// The dialogue in `file`, with `music` under it (a name in music/, or none). Fails without lines, a bubble or letters.
    pub fn load(paths: &Paths, file: &Path, music: Option<&str>) -> Result<Speech, String> {
        let json: Value = serde_json::from_str(&std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?).map_err(|e| format!("{}: {e}", file.display()))?;
        Speech::from_json(paths, &json, music, None).map_err(|error| format!("{}: {error}", file.display()))
    }

    /// The dialogue a file's JSON holds (the dialogue editor's, as it is edited); `own`: a portrait's file as the
    /// portrait editor has it, in the place of the one of that name.
    pub fn from_json(paths: &Paths, json: &Value, music: Option<&str>, own: Option<(&str, &Value)>) -> Result<Speech, String> {
        let text = |line: &Value, key: &str| line[key].as_str().unwrap_or_default().to_owned();
        let lines: Vec<Line> = json["dialogue"].as_array().into_iter().flatten().filter(|line| line.is_object()).map(|line| Line {
            portrait: text(line, "portrait"),
            expression: text(line, "expression"),
            text: Some(text(line, "text")).filter(|said| !said.is_empty()).unwrap_or_else(|| " ".to_owned()),
            angry: line["boxState"].as_str() == Some("angry"),
            speed: line["speed"].as_f64().filter(|speed| speed.is_finite()).unwrap_or(0.05) as f32,
            sound: Some(text(line, "sound")).filter(|sound| !sound.trim().is_empty()).unwrap_or_else(|| "dialogue".to_owned()),
        }).collect();
        if lines.is_empty() {
            return Err("badly formatted, no lines".to_owned());
        }
        let bubble = Sheet::load(&paths.find("images/speech_bubble.png").ok_or("images/speech_bubble.png: not found")?)?;
        let alphabet = Alphabet::load(paths).ok_or("images/alphabet: not found")?;
        let mut portraits = Vec::new();
        for line in &lines {
            if !portraits.iter().any(|portrait: &Portrait| portrait.name == line.portrait) {
                portraits.push(match own {
                    Some((name, json)) if name == line.portrait => Portrait::from_json(paths, name, json)?,
                    _ => Portrait::load(paths, &line.portrait)?,
                });
            }
        }
        let mut speech = Speech {
            lines, next: 0, bubble, bubble_shown: None, flip: false, portraits, speaking: None, alphabet, letters: Vec::new(), rows: 0, shown: 0,
            waited: 0.0, typed: false, white: 0.0, ended: false, first: true, count: 0, music: music.filter(|name| !name.is_empty()).map(|_| 0.0), cues: music.filter(|name| !name.is_empty()).map(|name| Cue::Music(name.to_owned())).into_iter().collect(),
        };
        speech.say_next();
        Ok(speech)
    }

    fn play_bubble(&mut self, name: &str) {
        let prefix = match name {
            "normal" => "speech bubble normal",
            "normalOpen" => "Speech Bubble Normal Open",
            "angry" => "AHH speech bubble",
            "angryOpen" => "speech bubble loud open",
            "center-normal" => "speech bubble middle",
            "center-normalOpen" => "Speech Bubble Middle Open",
            "center-angry" => "AHH Speech Bubble middle",
            _ => "speech bubble Middle loud open",
        };
        self.bubble_shown = Some((name.to_owned(), self.bubble.animation(prefix, &[]), 0.0, false)).filter(|shown| !shown.1.is_empty());
    }

    /// startNextDialog: the bubble opens or changes, the speaker's portrait takes the expression, the line starts typing.
    fn say_next(&mut self) {
        let line = &self.lines[self.next];
        let speaker = self.portraits.iter().position(|portrait| portrait.name == line.portrait).unwrap_or(0);
        let middle = if self.portraits[speaker].side == Side::Middle { "center-" } else { "" };
        let kind = if line.angry { "angry" } else { "normal" };
        let (text, speed, expression) = (line.text.clone(), line.speed, line.expression.clone());
        let resting = self.bubble_shown.as_ref().map(|(name, ..)| name.trim_end_matches("Open").to_owned());
        if Some(speaker) != self.speaking {
            self.play_bubble(&format!("{middle}{kind}Open"));
            self.flip = self.portraits[speaker].side == Side::Left;
        } else if resting.as_deref() != Some(&format!("{middle}{kind}")) {
            self.play_bubble(&format!("{middle}{kind}"));
        }
        self.speaking = Some(speaker);
        (self.letters, self.rows) = self.lay_out(&text);
        (self.shown, self.waited, self.typed) = (0, 0.0, false);
        // the talking frames go faster for a fast line: 24 a second at the usual speed, between 12 and 48
        let rate = (24.0 - (speed - 0.05) / 5.0 * 480.0).clamp(12.0, 48.0);
        self.portraits[speaker].show(&expression, false, rate);
        self.next += 1;
    }

    fn lay_out(&self, text: &str) -> (Vec<(char, Vec2)>, usize) {
        let (letters, _) = place(text, false, WIDTH_WRAP / TEXT_SCALE, |letter| self.alphabet.size(letter, false));
        let rows = letters.iter().map(|(_, at)| (at.y / 85.0).floor() as usize + 1).max().unwrap_or(0);
        (letters, rows)
    }

    /// Goes on; `accept` and `back` were pressed this frame. True once it has closed.
    pub fn update(&mut self, dt: f32, accept: bool, back: bool) -> bool {
        if std::mem::take(&mut self.first) {
            return false;
        }
        if let Some(time) = &mut self.music {
            *time += dt;
        }
        let line = &self.lines[self.next - 1];
        let (speed, sound) = (line.speed, line.sound.clone());
        if !self.ended {
            self.white = (self.white + 0.5 * dt).min(0.5);
            if accept || back {
                if !self.typed && !back {
                    self.shown = self.letters.len();
                    self.typed = true;
                    self.cues.push(Cue::Sound(sound.clone()));
                    self.cues.push(Cue::Skip(self.count));
                } else if back || self.next >= self.lines.len() {
                    self.ended = true;
                    // the bubble closes: its opening played backwards
                    if let Some((name, ..)) = &self.bubble_shown {
                        let opening = format!("{}Open", name.trim_end_matches("Open"));
                        self.play_bubble(&opening);
                        if let Some(shown) = &mut self.bubble_shown {
                            shown.3 = true;
                        }
                    }
                    if self.music.take().is_some() {
                        self.cues.push(Cue::Quiet);
                    }
                } else {
                    self.say_next();
                    self.count += 1;
                    self.cues.push(Cue::Next(self.count));
                }
                self.cues.push(Cue::Sound("dialogueClose".to_owned()));
            } else if !self.typed {
                // TypedAlphabet: a letter at most each frame, one every `speed` seconds, the sound with it (every other
                // letter for a fast line)
                self.waited += dt;
                if self.waited >= speed {
                    self.waited = 0.0;
                    if speed > 0.025 || self.shown % 2 == 1 {
                        self.cues.push(Cue::Sound(sound));
                    }
                    self.shown += 1;
                    self.typed = self.shown >= self.letters.len();
                }
            }
            // done typing, the speaker rests once its talking frames are through; while typing, they start over
            if let Some(speaker) = self.speaking {
                let portrait = &mut self.portraits[speaker];
                if portrait.finished() {
                    let (expression, rate) = (portrait.showing.0, portrait.showing.3);
                    portrait.showing = (expression, self.typed, 0.0, if self.typed { 24.0 } else { rate });
                }
            }
            if let Some((name, frames, time, false)) = &self.bubble_shown {
                if name.ends_with("Open") && *time * 24.0 >= frames.len() as f32 {
                    let resting = name.trim_end_matches("Open").to_owned();
                    self.play_bubble(&resting);
                }
            }
            for (index, portrait) in self.portraits.iter_mut().enumerate() {
                let coming = Some(index) == self.speaking;
                portrait.slide(dt, coming);
                portrait.alpha = (portrait.alpha + if coming { 3.0 } else { -3.0 } * dt).clamp(0.0, 1.0);
            }
        } else {
            self.white -= 0.5 * dt;
            for portrait in &mut self.portraits {
                let away = SLIDE * dt * match portrait.side {
                    Side::Left => vec2(-1.0, 0.0),
                    Side::Middle => vec2(0.0, 1.0),
                    Side::Right => vec2(1.0, 0.0),
                };
                portrait.at += away;
                portrait.alpha = (portrait.alpha - 10.0 * dt).max(0.0);
            }
        }
        if let Some((_, frames, time, back)) = &mut self.bubble_shown {
            *time += dt;
            if *back && *time * 24.0 >= frames.len() as f32 {
                self.bubble_shown = None;
            }
        }
        for portrait in &mut self.portraits {
            portrait.showing.2 += dt;
        }
        self.ended && self.bubble_shown.is_none() && self.white <= 0.0
    }

    /// How loud its music is to be now: up from silence over two seconds.
    pub fn music_volume(&self) -> Option<f32> {
        self.music.map(|time| (time / 2.0).min(1.0))
    }

    /// For the portrait editor: a portrait's expression drawn where it rests, its talking (`resting` false) or resting
    /// frames' first, see-through as asked.
    pub fn draw_pose(&self, portrait: &str, expression: &str, resting: bool, alpha: f32) {
        let Some(portrait) = self.portraits.iter().find(|own| own.name == portrait) else { return };
        let Some((_, shown)) = portrait.expressions.iter().find(|(name, _)| name == expression) else { return };
        let frames = if resting || shown.talking.is_empty() { &shown.resting } else { &shown.talking };
        let Some(frame) = frames.first() else { return };
        let at = match portrait.side {
            Side::Middle => vec2(portrait.at.x, portrait.resting_at),
            _ => vec2(portrait.resting_at, portrait.at.y),
        };
        flixel(&portrait.sheet.texture, *frame, at, shown.offsets[resting as usize], portrait.origin, portrait.scale, false, Color { a: alpha, ..WHITE });
    }

    /// Whether "Press BACK to Skip" shows.
    pub fn skippable(&self) -> bool {
        !self.ended
    }

    /// Draws it, in the HUD's camera.
    pub fn draw(&self) {
        if self.white > 0.0 {
            draw_rectangle(-500.0, -500.0, 2560.0, 1440.0, Color { a: self.white, ..WHITE });
        }
        for portrait in &self.portraits {
            portrait.draw();
        }
        if let Some((name, frames, time, back)) = &self.bubble_shown {
            let step = ((time * 24.0) as usize).min(frames.len() - 1);
            let looped = !name.ends_with("Open");
            let frame = frames[if looped { (time * 24.0) as usize % frames.len() } else if *back { frames.len() - 1 - step } else { step }];
            // updateBoxOffsets: an offset by the kind of bubble, ten lower unless mirrored
            let offset = if name.starts_with("angry") { vec2(50.0, 65.0) } else if name.starts_with("center-angry") { vec2(50.0, 30.0) } else { vec2(10.0, 0.0) } + vec2(0.0, if self.flip { 0.0 } else { 10.0 });
            flixel(&self.bubble.texture, frame, vec2(70.0, 370.0), offset, frame.size / 2.0, 0.9, self.flip, WHITE);
        }
        if self.ended {
            return;
        }
        // the letters, a long line (three rows or more) a little higher
        let at = TEXT_AT - vec2(0.0, if self.rows > 2 { 24.0 } else { 0.0 });
        let step = (get_time() * 24.0) as usize;
        for &(letter, place) in self.letters.iter().take(self.shown) {
            let Some((frames, _)) = self.alphabet.letters.get(&(letter, false)) else { continue };
            let corner = at + place * TEXT_SCALE;
            crate::atlas::draw_frame(&self.alphabet.texture, frames[step % frames.len()], corner.x, corner.y, TEXT_SCALE, false, WHITE);
        }
    }
}

/// Where plain letters wrap at a space: 65% of the screen's width.
const WIDTH_WRAP: f32 = crate::WIDTH * 0.65;

impl Portrait {
    /// `images/dialogue/<name>.json` of the mods or the game (bf's when there is none) and its sheet.
    fn load(paths: &Paths, name: &str) -> Result<Portrait, String> {
        let file = paths.find(&format!("images/dialogue/{name}.json")).or_else(|| paths.find("images/dialogue/bf.json")).ok_or("images/dialogue/bf.json: not found")?;
        let json: Value = serde_json::from_str(&std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?).map_err(|e| format!("{}: {e}", file.display()))?;
        Portrait::from_json(paths, name, &json)
    }

    fn from_json(paths: &Paths, name: &str, json: &Value) -> Result<Portrait, String> {
        let image = json["image"].as_str().unwrap_or_default();
        let sheet = Sheet::load(&paths.find(&format!("images/dialogue/{image}.png")).ok_or_else(|| format!("images/dialogue/{image}.png: not found"))?)?;
        let pair = |value: &Value| vec2(value[0].as_f64().unwrap_or(0.0) as f32, value[1].as_f64().unwrap_or(0.0) as f32);
        let expressions = json["animations"].as_array().into_iter().flatten().map(|animation| {
            let (talking, resting) = (animation["loop_name"].as_str().unwrap_or_default(), animation["idle_name"].as_str().unwrap_or_default());
            // (no talking frames of its own: it only ever rests)
            let talking = if talking.is_empty() || talking == resting { Vec::new() } else { sheet.animation(talking, &[]) };
            let expression = Expression { talking, resting: sheet.animation(resting, &[]), offsets: [pair(&animation["loop_offsets"]), pair(&animation["idle_offsets"])] };
            (animation["anim"].as_str().unwrap_or_default().to_owned(), expression)
        }).collect();
        let first = sheet.frames.first().map_or(Vec2::ZERO, |(_, frame)| frame.size);
        let scale = 0.7 * json["scale"].as_f64().unwrap_or(1.0) as f32;
        let (side, width) = (json["dialogue_pos"].as_str().unwrap_or("left"), first.x * scale);
        let moved = pair(&json["position"]);
        // DialogueBoxPsych.spawnCharacters: left at -60, right 100 from the right edge, each waiting 600 further out;
        // the middle one in the middle, waiting below the screen
        let (side, at, resting_at) = match side {
            "center" => (Side::Middle, vec2((crate::WIDTH - width) / 2.0, crate::HEIGHT + 50.0) + moved, 60.0 + moved.y),
            "right" => (Side::Right, vec2(crate::WIDTH - width - 100.0 + AWAY, 60.0) + moved, crate::WIDTH - width - 100.0 + moved.x),
            _ => (Side::Left, vec2(-60.0 - AWAY, 60.0) + moved, -60.0 + moved.x),
        };
        Ok(Portrait { name: name.to_owned(), sheet, expressions, side, scale, origin: first / 2.0, at, resting_at, alpha: 0.0, showing: (0, true, 0.0, 24.0) })
    }

    /// playAnim: an expression by name (one at random when it has none of that name), talking unless `resting`.
    fn show(&mut self, name: &str, resting: bool, rate: f32) {
        let Some(index) = self.expressions.iter().position(|(own, _)| own == name).or_else(|| (!self.expressions.is_empty()).then(|| (get_time() * 1000.0) as usize % self.expressions.len())) else { return };
        let resting = resting || self.expressions[index].1.talking.is_empty();
        self.showing = (index, resting, 0.0, if resting { 24.0 } else { rate });
    }

    fn frames(&self) -> Option<(&Expression, &[Frame])> {
        let (expression, resting, ..) = self.showing;
        let expression = &self.expressions.get(expression)?.1;
        Some((expression, if resting { &expression.resting } else { &expression.talking }))
    }

    /// Its talking frames are through (resting ones loop and never are).
    fn finished(&self) -> bool {
        let (_, resting, time, rate) = self.showing;
        !resting && self.frames().is_some_and(|(_, frames)| time * rate >= frames.len() as f32)
    }

    fn slide(&mut self, dt: f32, coming: bool) {
        let (place, toward, limit) = match self.side {
            Side::Left => (&mut self.at.x, if coming { 1.0 } else { -1.0 }, self.resting_at - if coming { 0.0 } else { AWAY }),
            Side::Right => (&mut self.at.x, if coming { -1.0 } else { 1.0 }, self.resting_at + if coming { 0.0 } else { AWAY }),
            Side::Middle => (&mut self.at.y, if coming { -1.0 } else { 1.0 }, self.resting_at + if coming { 0.0 } else { crate::HEIGHT }),
        };
        *place = slid(*place, toward * SLIDE * dt, limit);
    }

    fn draw(&self) {
        let Some((expression, frames)) = self.frames().filter(|(_, frames)| !frames.is_empty() && self.alpha > 0.0) else { return };
        let (_, resting, time, rate) = self.showing;
        let step = (time * rate) as usize;
        let frame = frames[if resting { step % frames.len() } else { step.min(frames.len() - 1) }];
        flixel(&self.sheet.texture, frame, self.at, expression.offsets[resting as usize], self.origin, self.scale, false, Color { a: self.alpha, ..WHITE });
    }
}

/// A place moved by `step`, stopping at `limit` (where it is going).
fn slid(place: f32, step: f32, limit: f32) -> f32 {
    let moved = place + step;
    if (moved - limit) * step.signum() > 0.0 { limit } else { moved }
}

/// A frame drawn as Flixel draws a sprite at `at` with `offset`, scaled by `scale` around `origin`.
#[allow(clippy::too_many_arguments)]
fn flixel(texture: &Texture2D, frame: Frame, at: Vec2, offset: Vec2, origin: Vec2, scale: f32, flip: bool, color: Color) {
    let corner = at - offset + origin * (1.0 - scale);
    crate::atlas::draw_frame(texture, frame, corner.x, corner.y, scale, flip, color);
}

/// The dialogue file a script names (startDialogue): `data/<song>/<name>_<language>.json`, else `data/<song>/<name>.json`.
pub fn find(paths: &Paths, song: &str, name: &str, language: &str) -> Option<std::path::PathBuf> {
    paths.find(&format!("data/{song}/{name}_{language}.json")).or_else(|| paths.find(&format!("data/{song}/{name}.json")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn portraits_slide_to_their_place_and_no_further() {
        // in from the left at 4000 a second, a tenth of a second at a time, to rest at -60
        assert_eq!(super::slid(-660.0, 400.0, -60.0), -260.0);
        assert_eq!(super::slid(-260.0, 400.0, -60.0), -60.0);
        // back out, no further than where it waited
        assert_eq!(super::slid(-60.0, -4000.0, -660.0), -660.0);
    }
}
