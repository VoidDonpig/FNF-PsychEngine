//! The talk before the week 6 songs in story mode: a pixel text box, the speakers' portraits and typed-out lines.
use crate::atlas::{Frame, Sheet};
use crate::menu::wrap;
use crate::{HEIGHT, PIXEL_ZOOM, WIDTH};
use funkin_core::content::Paths;
use crate::gfx::*;
use std::path::Path;

/// Which of the three songs it is before: each has its own box, music and manners.
#[derive(Clone, Copy, PartialEq)]
enum Style {
    Senpai,
    Roses,
    Thorns,
}

/// Something the play screen is to sound for the dialogue.
#[derive(Debug, PartialEq)]
pub enum Cue {
    Sound(&'static str, f32),
    /// Looped, until `Quiet`.
    Music(&'static str),
    Quiet,
}

/// How long "Senpai_Dies" plays: the scene before the Thorns dialogue lasts as long as its sound.
const SENPAI_DIES_S: f32 = 7.33;
const SECONDS_PER_LETTER: f32 = 0.04;
/// Everything of the dialogue is drawn this much larger.
const ZOOM: f32 = PIXEL_ZOOM * 0.9;

/// Where the dialogue is: what was said, what is being typed, who is showing.
struct Talk {
    style: Style,
    /// Who speaks (true for the player) and what.
    lines: Vec<(bool, String)>,
    line: usize,
    /// Seconds since the cutscene began.
    time: f32,
    /// How long the box takes to open.
    opening_s: f32,
    /// Seconds into the current line, once the box has opened.
    typed: Option<f32>,
    /// Seconds since the dialogue was finished or skipped; it fades out and ends.
    ending: Option<f32>,
    /// The portrait showing (true for the player's) and for how long.
    portrait: Option<(bool, f32)>,
}

pub struct Dialogue {
    talk: Talk,
    text_box: Sheet,
    /// The frames the box opens with; it rests on `rest`.
    opening: Vec<Frame>,
    rest: usize,
    portraits: [Option<Sheet>; 2],
    hand: Option<Texture2D>,
    face: Option<Texture2D>,
    crazy: Option<Sheet>,
    font: Option<Font>,
}

/// The lines of a dialogue file: ":dad:text" or ":bf:text" each.
fn parse(text: &str) -> Vec<(bool, String)> {
    text.lines().filter_map(|line| {
        let mut parts = line.trim().splitn(3, ':');
        let (_, speaker, said) = (parts.next()?, parts.next()?, parts.next()?);
        Some((speaker == "bf", said.trim().to_owned()))
    }).collect()
}

impl Dialogue {
    /// The dialogue of a song, if it is one of the three that have one and its file is there.
    /// `language`: the one chosen, whose own file of the lines is taken when the song has one.
    pub fn load(base: &Path, paths: &Paths, song: &str, stage: &str, language: &str) -> Option<Dialogue> {
        let style = match song {
            "senpai" => Style::Senpai,
            "roses" => Style::Roses,
            "thorns" => Style::Thorns,
            _ => return None,
        };
        if !stage.starts_with("school") {
            return None;
        }
        let file = paths.find(&format!("data/{song}/{song}Dialogue_{language}.txt")).or_else(|| paths.find(&format!("data/{song}/{song}Dialogue.txt")))?;
        let lines = parse(&std::fs::read_to_string(file).ok()?);
        let sheet = |name: &str| {
            let sheet = Sheet::load(&paths.find(&format!("images/weeb/{name}.png"))?).ok()?;
            sheet.texture.set_filter(FilterMode::Nearest);
            Some(sheet)
        };
        let picture = |name: &str| {
            let picture = crate::atlas::picture(&paths.find(&format!("images/weeb/{name}.png"))?)?;
            picture.set_filter(FilterMode::Nearest);
            Some(picture)
        };
        let (file, prefix, rest) = match style {
            Style::Senpai => ("pixelUI/dialogueBox-pixel", "Text Box Appear", 4),
            Style::Roses => ("pixelUI/dialogueBox-senpaiMad", "SENPAI ANGRY IMPACT SPEECH", 4),
            Style::Thorns => ("pixelUI/dialogueBox-evil", "Spirit Textbox spawn", 11),
        };
        let text_box = sheet(file)?;
        let opening = text_box.animation(prefix, &[]);
        let mut font = std::fs::read(base.join("assets/fonts/pixel-latin.ttf")).ok().and_then(|bytes| load_ttf_font_from_bytes(&bytes).ok());
        if let Some(font) = &mut font {
            font.set_filter(FilterMode::Nearest);
        }
        (!lines.is_empty() && !opening.is_empty()).then(|| Dialogue {
            talk: Talk { style, lines, line: 0, time: 0.0, opening_s: opening.len() as f32 / 24.0, typed: None, ending: None, portrait: None },
            rest: rest.min(opening.len() - 1), opening, text_box,
            portraits: [sheet("senpaiPortrait"), sheet("bfPortrait")], hand: picture("pixelUI/hand_textbox"),
            face: picture("spiritFaceForward").filter(|_| style == Style::Thorns), crazy: sheet("senpaiCrazy").filter(|_| style == Style::Thorns), font,
        })
    }

    pub fn update(&mut self, dt: f32, accept: bool, back: bool) -> (Vec<Cue>, bool) {
        self.talk.update(dt, accept, back)
    }
}

impl Talk {
    /// How long the scene before the box takes: the screen clearing up, or Senpai's end.
    fn prelude(&self) -> f32 {
        if self.style == Style::Thorns { 4.2 + SENPAI_DIES_S } else { 2.1 }
    }

    /// The current line, and how many of its letters are out.
    fn said(&self) -> (&str, usize) {
        let text = self.lines[self.line].1.as_str();
        (text, self.typed.map_or(0, |typed| ((typed / SECONDS_PER_LETTER) as usize).min(text.chars().count())))
    }

    fn start_line(&mut self) {
        self.typed = Some(0.0);
        let player = self.lines[self.line].0;
        if self.portrait.map(|(side, _)| side) != Some(player) {
            self.portrait = Some((player, 0.0));
        }
    }

    /// Advances by `dt` seconds with the player's presses. Returns what to sound, and whether the dialogue is over.
    fn update(&mut self, dt: f32, accept: bool, back: bool) -> (Vec<Cue>, bool) {
        let mut cues = Vec::new();
        let before = self.time;
        self.time += dt;
        let crossed = |at: f32| before <= at && self.time > at;
        if crossed(0.0) {
            match self.style {
                Style::Senpai => cues.push(Cue::Music("Lunchbox")),
                Style::Roses => cues.extend([Cue::Sound("ANGRY_TEXT_BOX", 1.0), Cue::Sound("ANGRY", 1.0)]),
                Style::Thorns => cues.push(Cue::Music("LunchboxScary")),
            }
        }
        if self.style == Style::Thorns && crossed(4.2) {
            cues.push(Cue::Sound("Senpai_Dies", 1.0));
        }
        if let Some(ending) = &mut self.ending {
            *ending += dt;
            let over = *ending >= 1.5;
            if over {
                cues.push(Cue::Quiet);
            }
            return (cues, over);
        }
        // the box opens, then the first line starts by itself
        let opened = self.prelude() + self.opening_s;
        if self.typed.is_none() && self.time >= opened {
            self.start_line();
            return (cues, false);
        }
        if let Some((_, shown)) = &mut self.portrait {
            *shown += dt;
        }
        let Some(typed) = self.typed else { return (cues, false) };
        let (letters, out) = (self.said().0.chars().count(), self.said().1);
        self.typed = Some(typed + dt);
        if self.said().1 > out {
            cues.push(Cue::Sound("pixelText", 0.6));
        }
        if back || (accept && out == letters && self.line + 1 == self.lines.len()) {
            self.ending = Some(0.0);
            self.typed = Some(f32::MAX);
            cues.push(Cue::Sound("clickText", 0.8));
        } else if accept && out == letters {
            self.line += 1;
            self.start_line();
            cues.push(Cue::Sound("clickText", 0.8));
        } else if accept {
            // the rest of the line at once
            self.typed = Some(f32::MAX);
            cues.push(Cue::Sound("clickText", 0.8));
        }
        (cues, false)
    }

}

impl Dialogue {
    pub fn draw(&self) {
        let talk = &self.talk;
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        let cover = |color: Color, alpha: f32| draw_rectangle(0.0, 0.0, WIDTH, HEIGHT, Color { a: alpha.clamp(0.0, 1.0), ..color });
        // things here change in steps, like the old game's timers
        let steps = |seconds: f32, every: f32| (seconds / every).floor().max(0.0);
        let (time, prelude) = (talk.time, talk.prelude());
        if time < prelude {
            match talk.style {
                // the screen clears up from black
                Style::Senpai => cover(BLACK, 1.0 - 0.15 * steps(time, 0.3)),
                Style::Roses => {}
                Style::Thorns => {
                    // Senpai fades in on red, bursts, and the screen goes white
                    cover(Color::from_hex(0xFF1B31), 1.0);
                    if let Some(crazy) = self.crazy.as_ref().filter(|_| time >= 2.1) {
                        let frames = crazy.animation("Senpai Pre Explosion", &[]);
                        if let Some(frame) = frames.get((((time - 4.2).max(0.0) * 24.0) as usize).min(frames.len().saturating_sub(1))) {
                            let size = frame.size * PIXEL_ZOOM;
                            let alpha = (0.15 * steps(time - 2.1, 0.3)).min(1.0);
                            crazy.draw(*frame, (WIDTH - size.x) / 2.0 + 300.0, (HEIGHT - size.y) / 2.0, PIXEL_ZOOM, false, Color::new(1.0, 1.0, 1.0, alpha));
                        }
                    }
                    cover(WHITE, (time - 4.2 - 3.2) / 1.6);
                }
            }
            return;
        }
        // once finished, everything fades away in five steps
        let fade = 1.0 - 0.2 * talk.ending.map_or(0.0, |ending| steps(ending, 0.2));
        let tint = Color::new(1.0, 1.0, 1.0, fade.max(0.0));
        cover(Color::from_hex(0xB3DFD8), (0.14 * steps(time - prelude, 0.83)).min(0.7) * fade);
        if let Some(face) = &self.face {
            // enlarged around its middle
            let grown = face.size() * PIXEL_ZOOM;
            let at = vec2(320.0, 170.0) - (grown - face.size()) / 2.0;
            draw_texture_ex(face, at.x, at.y, WHITE, DrawTextureParams { dest_size: Some(grown), ..Default::default() });
        }
        if let (Some((player, shown)), None) = (talk.portrait, talk.ending) {
            // only the first song shows who the opponent is; the player's portrait always shows
            let (sheet, name) = if player { (&self.portraits[1], "Boyfriend portrait enter") } else { (&self.portraits[0], "Senpai Portrait Enter") };
            if let Some(sheet) = sheet.as_ref().filter(|_| player || talk.style == Style::Senpai) {
                let frames = sheet.animation(name, &[]);
                if let Some(frame) = frames.get(((shown * 24.0) as usize).min(frames.len().saturating_sub(1))) {
                    let x = if player { 0.0 } else { (WIDTH - frame.size.x * ZOOM) / 2.0 };
                    sheet.draw(*frame, x, 40.0, ZOOM, false, WHITE);
                }
            }
        }
        let frame = self.opening[(((time - prelude) * 24.0) as usize).min(self.rest)];
        self.text_box.draw(frame, (WIDTH - frame.size.x * ZOOM) / 2.0, 45.0, ZOOM, false, tint);

        let (text, out) = talk.said();
        let measure = |text: &str| measure_text(text, self.font.as_ref(), 32, 1.0).width;
        let mut left = out;
        for (row, line) in wrap(text, WIDTH * 0.6, measure).iter().enumerate() {
            let part: String = line.chars().take(left).collect();
            left = left.saturating_sub(line.chars().count() + 1);
            let (x, y) = (240.0, 528.0 + row as f32 * 38.0);
            let ink = |color: u32| Color { a: fade.max(0.0), ..Color::from_hex(color) };
            // dark letters with a rosy shadow; the spirit writes in plain white
            let layers = if talk.style == Style::Thorns { vec![(0.0, 0xFFFFFF)] } else { vec![(2.0, 0xD89494), (0.0, 0x3F2021)] };
            for (shift, color) in layers {
                draw_text_ex(&part, x + shift, y + shift, TextParams { font: self.font.as_ref(), font_size: 32, color: ink(color), ..Default::default() });
            }
        }
        if let Some(hand) = self.hand.as_ref().filter(|_| out == text.chars().count() && talk.typed.is_some()) {
            draw_texture_ex(hand, 1042.0, 590.0, tint, DrawTextureParams { dest_size: Some(hand.size() * ZOOM), ..Default::default() });
        }
        if talk.ending.is_none() {
            let hint = &funkin_core::lang::phrase("dialogue_skip", "Press BACK to Skip");
            draw_text(hint, WIDTH - 20.0 - measure_text(hint, None, 16, 1.0).width, HEIGHT - 16.0, 16.0, WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_type_out_and_the_player_moves_them_on() {
        let lines = vec![(false, "Hello".to_owned()), (true, "Beep".to_owned())];
        let mut talk = Talk { style: Style::Roses, lines, line: 0, time: 0.0, opening_s: 0.5, typed: None, ending: None, portrait: None };
        assert_eq!(talk.update(0.1, false, false), (vec![Cue::Sound("ANGRY_TEXT_BOX", 1.0), Cue::Sound("ANGRY", 1.0)], false));
        // nothing is said, and no key does anything, until the box has opened (2.1 s of waiting and half a second of box)
        assert_eq!(talk.update(2.0, true, true), (vec![], false));
        talk.update(0.6, false, false);
        assert_eq!((talk.said(), talk.portrait.map(|(player, _)| player)), (("Hello", 0), Some(false)));
        assert_eq!(talk.update(0.09, false, false), (vec![Cue::Sound("pixelText", 0.6)], false));
        assert_eq!(talk.said().1, 2);
        // Accept finishes the line at once, then moves on to the next speaker
        talk.update(0.01, true, false);
        assert_eq!(talk.said(), ("Hello", 5));
        talk.update(0.01, true, false);
        assert_eq!((talk.said(), talk.portrait.map(|(player, _)| player)), (("Beep", 0), Some(true)));
        // on the last line it ends the talk, which takes a moment to fade
        talk.update(1.0, false, false);
        assert_eq!(talk.update(0.01, true, false), (vec![Cue::Sound("clickText", 0.8)], false));
        assert_eq!(talk.update(1.0, false, false), (vec![], false));
        assert_eq!(talk.update(0.6, false, false), (vec![Cue::Quiet], true));
    }

    #[test]
    fn back_skips_the_rest() {
        let mut talk = Talk { style: Style::Senpai, lines: vec![(false, "One".to_owned()), (false, "Two".to_owned())], line: 0, time: 3.0, opening_s: 0.2, typed: None, ending: None, portrait: None };
        talk.update(0.01, false, false);
        assert_eq!(talk.update(0.01, false, true), (vec![Cue::Sound("clickText", 0.8)], false));
        assert!(talk.ending.is_some() && talk.line == 0);
    }

    #[test]
    fn reads_who_says_what() {
        assert_eq!(parse(":dad:Ah, a new fair maiden!\n:bf:Beep: bo bop\n\nnot a line\n"), [(false, "Ah, a new fair maiden!".to_owned()), (true, "Beep: bo bop".to_owned())]);
    }
}
