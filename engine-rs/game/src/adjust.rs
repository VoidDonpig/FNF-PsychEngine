//! The options' screen for setting the note delay by ear and placing the rating and the combo count by eye.
use crate::audio::Song;
use crate::controls::{down, pressed, Ui};
use crate::gfx::*;
use crate::scene::Scene;
use crate::settings::Settings;
use crate::{HEIGHT, WIDTH};
use funkin_core::chart::{Chart, ChartEvent};
use funkin_core::content::Paths;
use funkin_core::lang::{phrase, phrase_with};
use std::path::{Path, PathBuf};

pub struct Adjust {
    /// Week 1's stage with the two who dance to the beat.
    scene: Option<Scene>,
    chart: Chart,
    music: Option<PathBuf>,
    song: Song,
    /// The samples that are placed: a rating and three digits.
    rating: Option<Texture2D>,
    digits: Vec<Texture2D>,
    /// Which half shows: placing, or the delay.
    placing: bool,
    /// The delay before rounding, and for how long Left or Right has been held (it then runs).
    delay: f64,
    held: f32,
    /// The last beat seen, and when "Beat Hit!" last rose.
    beat: i64,
    hit: f64,
}

/// The delay a frame later: a press moves it by one ms, a key held for over half a second runs it at 300 ms a second.
fn delayed(delay: f64, press: f64, held: f32, turn: f64, dt: f32) -> f64 {
    (delay + press + if held > 0.5 { 300.0 * dt as f64 * turn } else { 0.0 }).clamp(-500.0, 500.0)
}

fn play(music: &Option<PathBuf>) -> Song {
    let mut song = Song::load(&music.iter().map(PathBuf::as_path).collect::<Vec<_>>());
    song.start();
    song
}

impl Adjust {
    pub fn open(base: &Path, settings: &Settings) -> Adjust {
        let paths = Paths::new(base, "", "");
        let picture = |name: &str| paths.find(&format!("images/{name}.png")).and_then(|path| crate::atlas::picture(&path));
        // (the girlfriend as the opponent: she stands in her own place and nobody else is loaded)
        let chart = Chart { stage: "stage".to_owned(), player: "bf".to_owned(), opponent: "gf".to_owned(), bpm: 128.0, tempo: vec![(0.0, 0.0, 128.0)], ..Default::default() };
        let mut scene = Scene::load(base, "", &chart, settings.low_quality).map_err(|error| eprintln!("Stage: {error}")).ok();
        let view = ChartEvent { time: 0.0, name: "Camera Follow Pos".to_owned(), value1: "760".to_owned(), value2: "490".to_owned() };
        scene.iter_mut().for_each(|scene| scene.event(&view));
        let music = paths.find("music/offsetSong.ogg");
        Adjust {
            scene, chart, song: play(&music), music, rating: picture("sick"), digits: ["num1", "num2", "num3"].iter().filter_map(|name| picture(name)).collect(),
            placing: true, delay: settings.offset_ms, held: 0.0, beat: i64::MIN, hit: f64::MIN,
        }
    }

    /// Runs and draws one frame. `text` writes a line with its baseline at (x, y). True when the player leaves.
    pub fn run(&mut self, settings: &mut Settings, text: &dyn Fn(&str, f32, f32, u16, Color)) -> bool {
        let dt = get_frame_time().min(0.1);
        if pressed(Ui::Accept) {
            self.placing ^= true;
        }
        if self.placing {
            // the arrows move the rating and W, A, S, D the digits, ten at a time: right and up count up
            let moved = |more: KeyCode, less: KeyCode| (is_key_pressed(more) as i8 - is_key_pressed(less) as i8) as f64 * 10.0;
            let by = [moved(KeyCode::Right, KeyCode::Left), moved(KeyCode::Up, KeyCode::Down), moved(KeyCode::D, KeyCode::A), moved(KeyCode::W, KeyCode::S)];
            for (index, offset) in settings.combo_offset.iter_mut().enumerate() {
                let reach = if index % 2 == 0 { 640.0 } else { 360.0 };
                *offset = if pressed(Ui::Reset) { 0.0 } else { (*offset + by[index]).clamp(-reach, reach) };
            }
        } else {
            let turn = (down(Ui::Right) as i8 - down(Ui::Left) as i8) as f64;
            self.held = if turn != 0.0 { self.held + dt } else { 0.0 };
            let press = (pressed(Ui::Right) as i8 - pressed(Ui::Left) as i8) as f64;
            self.delay = if pressed(Ui::Reset) { 0.0 } else { delayed(self.delay, press, self.held, turn, dt) };
            settings.offset_ms = self.delay.round();
        }

        // the music goes round, and the beat is counted where the sound is heard: later by the delay
        if self.song.length_ms().is_some_and(|length| self.song.position_ms() >= length) {
            self.song = play(&self.music);
        }
        let position = self.song.position_ms() - settings.offset_ms;
        let beat = self.chart.beat_at(position).floor() as i64;
        if std::mem::replace(&mut self.beat, beat) != beat && beat.rem_euclid(4) == 2 {
            self.hit = get_time();
            let bump = ChartEvent { time: position, name: "Add Camera Zoom".to_owned(), value1: "0.15".to_owned(), value2: "0".to_owned() };
            self.scene.iter_mut().for_each(|scene| scene.event(&bump));
        }
        if let Some(scene) = &mut self.scene {
            scene.update(dt, position, &self.chart, &[], [[false; 4]; 2]);
            scene.draw();
        }
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });

        // (the letters are 0.6 of their size wide)
        let centred = |words: &str, y: f32, size: u16, color: Color| text(words, (WIDTH - words.chars().count() as f32 * size as f32 * 0.6) / 2.0, y, size, color);
        draw_rectangle(0.0, 0.0, WIDTH, 40.0, Color::new(0.0, 0.0, 0.0, 0.6));
        let mode = if self.placing { phrase("combo_offset", "Combo Offset") } else { phrase("note_delay", "Note/Beat Delay") };
        centred(&format!("< {} {} >", mode, phrase("switch_on_accept", "(Press Accept to Switch)")).to_uppercase(), 30.0, 28, WHITE);
        if self.placing {
            // where they pop up in a song
            let [rating_at, digits_at] = [0, 2].map(|index| vec2(settings.combo_offset[index] as f32, -settings.combo_offset[index + 1] as f32));
            let left = WIDTH * 0.35 - 40.0;
            let sample = |picture: &Texture2D, scale: f32, at: Vec2| {
                let size = picture.size() * scale;
                draw_texture_ex(picture, at.x, (HEIGHT - size.y) / 2.0 + at.y, WHITE, DrawTextureParams { dest_size: Some(size), ..Default::default() });
            };
            self.rating.iter().for_each(|picture| sample(picture, 0.7, rating_at + vec2(left, -60.0)));
            for (place, picture) in self.digits.iter().enumerate() {
                sample(picture, 0.5, digits_at + vec2(left - 50.0 + 43.0 * place as f32, 80.0));
            }
            let pair = |index: usize| format!("[{}, {}]", settings.combo_offset[index], settings.combo_offset[index + 1]);
            let lines = [phrase("combo_rating_offset", "Rating Offset:"), pair(0), phrase("combo_numbers_offset", "Numbers Offset:"), pair(2)];
            for (row, line) in lines.iter().enumerate() {
                text(line, 10.0, 68.0 + row as f32 * 30.0 + if row > 1 { 24.0 } else { 0.0 }, 24, WHITE);
            }
        } else {
            // the words rise and fall away on every fourth beat
            let age = (get_time() - self.hit) as f32;
            if age < 1.0 {
                let words = phrase("delay_beat_hit", "Beat Hit!");
                text(&words, 380.0, 320.0 - 150.0 * age + 125.0 * age * age, 40, Color::new(1.0, 1.0, 1.0, (age * std::f32::consts::FRAC_PI_2).cos()));
            }
            centred(&phrase_with("delay_current_offset", "Current offset: {1} ms", &[&settings.offset_ms.to_string()]), 625.0, 32, WHITE);
            // a bar from -500 to 500 ms
            let (wide, filled) = (600.0, ((settings.offset_ms + 500.0) / 1000.0) as f32);
            draw_rectangle((WIDTH - wide) / 2.0 - 4.0, 640.0, wide + 8.0, 19.0, BLACK);
            draw_rectangle((WIDTH - wide) / 2.0, 644.0, wide, 11.0, WHITE);
            draw_rectangle((WIDTH - wide) / 2.0, 644.0, wide * filled, 11.0, Color::from_hex(0x00FF00));
        }
        pressed(Ui::Back)
    }
}

#[cfg(test)]
mod tests {
    use super::delayed;

    #[test]
    fn the_delay_steps_on_a_press_and_runs_while_held() {
        // a press: one ms, whichever way
        assert_eq!((delayed(10.0, 1.0, 0.0, 1.0, 0.016), delayed(10.0, -1.0, 0.0, -1.0, 0.016)), (11.0, 9.0));
        // held for less than half a second: nothing more; after that 300 ms a second
        assert_eq!((delayed(10.0, 0.0, 0.4, 1.0, 0.125), delayed(10.0, 0.0, 0.6, -1.0, 0.125)), (10.0, -27.5));
        // never past half a second either way
        assert_eq!((delayed(499.5, 1.0, 0.0, 1.0, 0.016), delayed(-490.0, 0.0, 2.0, -1.0, 0.125)), (500.0, -500.0));
    }
}
