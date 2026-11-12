//! The chart editor (states/editors/ChartingState.hx), its first part: the song's notes on the Haxe engine's grid (the
//! events' column, then the player's four, then the opponent's four), scrolling under a line as the music plays; notes
//! put down and taken off with the mouse at the chosen beat snap, held longer or shorter with Q and E; undo and redo;
//! play-testing the chart as it is (Enter) and saving it over its file, the old one kept beside it as `.bak`.
// shortcut: the File/Edit/View menus and the Charting, Data, Events, Note, Section and Song tabs are not here yet;
// events are shown but not placed, and Escape leaves (the Haxe editor's File > Exit). Add them with a UI kit of their own.
use crate::gfx::*;
use crate::{audio, Skin, HEIGHT, NOTE_NAMES, WIDTH};
use funkin_core::content::{Paths, SongEntry};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const GRID: f32 = 40.0;
/// The events' column, then four of the player's, then four of the opponent's.
const COLUMNS: usize = 9;
const LEFT: f32 = (WIDTH - GRID * COLUMNS as f32) / 2.0;
const QUANTS: [u32; 11] = [4, 8, 12, 16, 20, 24, 32, 48, 64, 96, 192];
const ZOOMS: [f32; 11] = [0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0];

/// What the editor wants next.
pub enum Go {
    Stay,
    /// Play the chart as it is now (its text), from the start.
    Play(String),
    Leave,
}

/// Where a section starts (ms), how many beats it has, at what tempo, and how many grid rows come before it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    start: f64,
    beats: f64,
    bpm: f64,
    rows_before: f64,
}

impl Span {
    fn step_ms(&self) -> f64 {
        15000.0 / self.bpm
    }

    fn rows(&self) -> f64 {
        self.beats * 4.0
    }
}

pub struct ChartEditor {
    pub entry: SongEntry,
    /// The chart file as read, the song's fields in `song()`; edits go straight into it, so what the game does not know
    /// of is kept.
    json: Value,
    spans: Vec<Span>,
    song: audio::Song,
    playing: bool,
    position: f64,
    length: f64,
    zoom: usize,
    quant: usize,
    /// The note last put down or picked (section, place in its notes).
    selected: Option<(usize, usize)>,
    undo: Vec<Value>,
    redo: Vec<Value>,
    /// Changed since it was last saved (or brought back from a play-test), and Escape was pressed once already.
    unsaved: bool,
    leaving: bool,
    said: Option<(String, f64)>,
    background: Option<Texture2D>,
    event_icon: Option<Texture2D>,
    icons: [Option<Texture2D>; 2],
}

impl ChartEditor {
    /// The chart of `entry`; `text` is the chart as last edited (back from a play-test), else its file is read.
    pub fn open(base: &Path, entry: &SongEntry, text: Option<String>) -> Result<ChartEditor, String> {
        // (back from a play-test, the chart may hold changes not saved)
        let unsaved = text.is_some();
        let text = match text {
            Some(text) => text,
            None => std::fs::read_to_string(&entry.chart).map_err(|e| format!("{}: {e}", entry.chart.display()))?,
        };
        let mut json: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", entry.chart.display()))?;
        // the song's own fields, not its "song" wrapper (Psych 1.0 saves them so)
        if json["song"].is_object() {
            json = json["song"].take();
        }
        convert(&mut json);
        let mut tracks = vec![entry.inst.as_path()];
        tracks.extend(entry.voices_player.as_deref());
        tracks.extend(entry.voices_opponent.as_deref());
        let song = audio::Song::load(&tracks);
        let length = song.length_ms().or_else(|| audio::seconds(&entry.inst).map(|seconds| seconds as f64 * 1000.0)).unwrap_or(60000.0);
        fill_sections(&mut json, length);
        let paths = Paths::new(base, &entry.source, "");
        let picture = |name: &str| paths.find(&format!("images/{name}.png")).and_then(|path| crate::atlas::picture(&path));
        let icon = |character: &str| {
            let file = paths.find(&format!("characters/{character}.json")).and_then(|file| std::fs::read_to_string(file).ok());
            let name = file.and_then(|text| serde_json::from_str::<Value>(&text).ok()).and_then(|json| json["healthicon"].as_str().map(str::to_owned)).unwrap_or_else(|| "face".to_owned());
            picture(&format!("icons/icon-{name}")).or_else(|| picture(&format!("icons/{name}"))).or_else(|| picture("icons/icon-face"))
        };
        let icons = [icon(json["player1"].as_str().unwrap_or("bf")), icon(json["player2"].as_str().unwrap_or("dad"))];
        let mut editor = ChartEditor {
            entry: entry.clone(), json, spans: Vec::new(), song, playing: false, position: 0.0, length, zoom: 2, quant: 3, selected: None,
            undo: Vec::new(), redo: Vec::new(), unsaved, leaving: false, said: None, background: picture("menuDesat"), event_icon: picture("editors/eventIcon"), icons,
        };
        editor.measure();
        Ok(editor)
    }

    /// The chart as it would be saved.
    pub fn text(&self) -> String {
        print(&self.json, 0)
    }

    fn sections(&self) -> &[Value] {
        self.json["notes"].as_array().map_or(&[], Vec::as_slice)
    }

    /// Where each section starts, from the tempo and its beats (a section may change the tempo).
    fn measure(&mut self) {
        let mut bpm = self.json["bpm"].as_f64().filter(|bpm| *bpm > 0.0).unwrap_or(100.0);
        let (mut start, mut rows_before) = (0.0, 0.0);
        self.spans = self.sections().iter().map(|section| {
            if section["changeBPM"].as_bool().unwrap_or(false) {
                bpm = section["bpm"].as_f64().filter(|bpm| *bpm > 0.0).unwrap_or(bpm);
            }
            let beats = section["sectionBeats"].as_f64().filter(|beats| *beats > 0.0).unwrap_or(4.0);
            let span = Span { start, beats, bpm, rows_before };
            start += beats * 60000.0 / bpm;
            rows_before += beats * 4.0;
            span
        }).collect();
    }

    /// The section a moment falls in.
    fn section_at(&self, ms: f64) -> usize {
        self.spans.iter().rposition(|span| span.start <= ms).unwrap_or(0)
    }

    /// A moment as a grid row (rows are sixteenths, counted from the song's start).
    fn row_of(&self, ms: f64) -> f64 {
        let Some(span) = self.spans.get(self.section_at(ms)) else { return 0.0 };
        span.rows_before + (ms - span.start) / span.step_ms()
    }

    fn ms_of(&self, row: f64) -> f64 {
        let index = self.spans.iter().rposition(|span| span.rows_before <= row).unwrap_or(0);
        self.spans.get(index).map_or(0.0, |span| span.start + (row - span.rows_before) * span.step_ms())
    }

    /// Where a moment is on screen: the line across the middle is now.
    fn y_of(&self, ms: f64) -> f32 {
        HEIGHT / 2.0 + ((self.row_of(ms) - self.row_of(self.position)) as f32) * GRID * ZOOMS[self.zoom]
    }

    fn say(&mut self, words: impl Into<String>) {
        self.said = Some((words.into(), get_time()));
    }

    /// Keeps the chart as it is, to come back to with undo.
    fn remember(&mut self) {
        (self.unsaved, self.leaving) = (true, false);
        self.undo.push(self.json["notes"].clone());
        // (the last hundred changes)
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn stop(&mut self) {
        if self.playing {
            self.song.pause();
            self.playing = false;
        }
    }

    fn wind(&mut self, ms: f64) {
        self.stop();
        self.position = ms.clamp(0.0, (self.length - 1.0).max(0.0));
        self.song.seek(self.position);
    }

    pub fn update(&mut self) -> Go {
        let dt = get_frame_time().min(0.1) as f64;
        let (ctrl, shift) = (held(KeyCode::LeftControl) || held(KeyCode::RightControl), held(KeyCode::LeftShift) || held(KeyCode::RightShift));
        let current = self.section_at(self.position);
        let span = self.spans.get(current).copied().unwrap_or(Span { start: 0.0, beats: 4.0, bpm: 100.0, rows_before: 0.0 });
        if ctrl {
            if is_key_pressed(KeyCode::S) {
                self.save();
            } else if is_key_pressed(KeyCode::Z) {
                self.step_history(true);
            } else if is_key_pressed(KeyCode::Y) {
                self.step_history(false);
            }
        } else if is_key_pressed(KeyCode::Enter) {
            self.stop();
            return Go::Play(self.text());
        } else if is_key_pressed(KeyCode::Escape) {
            // changes not saved: a second Escape is needed
            if !self.unsaved || self.leaving {
                self.stop();
                return Go::Leave;
            }
            self.leaving = true;
            self.say("Unsaved changes: Escape again to leave, Ctrl+S to save");
        } else if is_key_pressed(KeyCode::A) != is_key_pressed(KeyCode::D) {
            let by = if shift { 4 } else { 1 };
            let to = if is_key_pressed(KeyCode::A) { current.saturating_sub(by) } else { (current + by).min(self.spans.len().saturating_sub(1)) };
            if let Some(span) = self.spans.get(to) {
                self.wind(span.start);
            }
        } else if is_key_pressed(KeyCode::Home) {
            self.wind(0.0);
        } else if is_key_pressed(KeyCode::End) {
            self.wind(self.length);
        } else if is_key_pressed(KeyCode::R) {
            self.wind(if shift { 0.0 } else { span.start });
        } else if is_key_down(KeyCode::W) != is_key_down(KeyCode::S) {
            // a beat and a half a second, four times that with Shift, slower zoomed in
            let toward = if is_key_down(KeyCode::W) { -1.0 } else { 1.0 };
            self.wind(self.position + toward * 60000.0 / span.bpm * if shift { 4.0 } else { 1.0 } * 1.5 * dt / ZOOMS[self.zoom] as f64);
        } else if mouse_wheel() != 0.0 {
            // the wheel steps by the beat snap, rounded onto it (the Haxe editor's Mouse Scroll Snap)
            let snap = 16.0 / QUANTS[self.quant] as f64 / ZOOMS[self.zoom] as f64;
            let row = ((self.row_of(self.position) - mouse_wheel() as f64 * snap * if shift { 4.0 } else { 1.0 }) / snap).round() * snap;
            let ms = self.ms_of(row.max(0.0));
            self.wind(if ms > 0.0 { ms + 0.000001 } else { 0.0 });
        } else if is_key_pressed(KeyCode::Space) {
            if self.playing {
                self.stop();
            } else {
                self.song.seek(self.position);
                self.song.start();
                self.playing = true;
            }
        } else if is_key_pressed(KeyCode::Left) != is_key_pressed(KeyCode::Right) {
            self.quant = if is_key_pressed(KeyCode::Left) { self.quant.saturating_sub(1) } else { (self.quant + 1).min(QUANTS.len() - 1) };
        } else if is_key_pressed(KeyCode::Z) != is_key_pressed(KeyCode::X) {
            self.zoom = if is_key_pressed(KeyCode::Z) { self.zoom.saturating_sub(1) } else { (self.zoom + 1).min(ZOOMS.len() - 1) };
            self.say(format!("Zoom: {}x", ZOOMS[self.zoom]));
        } else if is_key_pressed(KeyCode::Q) != is_key_pressed(KeyCode::E) {
            let by = span.step_ms() / 2.0 * if shift { 4.0 } else { 1.0 } * if is_key_pressed(KeyCode::Q) { -1.0 } else { 1.0 };
            self.hold_longer(by);
        } else if is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace) {
            if let Some((section, index)) = self.selected.take() {
                self.remember();
                if let Some(notes) = self.json["notes"][section]["sectionNotes"].as_array_mut().filter(|notes| index < notes.len()) {
                    notes.remove(index);
                }
            }
        }
        if self.playing {
            self.position = self.song.position_ms();
            if self.position >= self.length {
                self.wind(self.length);
            }
        }
        if is_mouse_pressed() {
            self.click(shift);
        }
        Go::Stay
    }

    /// The column and moment under the mouse, when it is over the grid: snapped to the beat snap unless Shift is held.
    fn under_mouse(&self, snapped: bool) -> Option<(usize, f64, f64)> {
        let at = mouse_position(vec2(WIDTH, HEIGHT));
        let column = ((at.x - LEFT) / GRID).floor();
        if !(0.0..COLUMNS as f32).contains(&column) {
            return None;
        }
        let row = self.row_of(self.position) + ((at.y - HEIGHT / 2.0) / (GRID * ZOOMS[self.zoom])) as f64;
        let snap = 16.0 / QUANTS[self.quant] as f64;
        let row = if snapped { (row / snap).floor() * snap } else { row };
        (row >= 0.0).then(|| (column as usize, self.ms_of(row), self.ms_of(row + if snapped { snap } else { 0.0 })))
    }

    /// The note drawn under a point of a column (section, place in its notes): the Haxe editor picks by the note's box.
    fn note_at(&self, column: usize, y: f32) -> Option<(usize, usize)> {
        let data = column.checked_sub(1)? as f64;
        self.sections().iter().enumerate().find_map(|(section, value)| {
            value["sectionNotes"].as_array()?.iter().position(|note| {
                let top = note[0].as_f64().map_or(f32::MAX, |time| self.y_of(time));
                note[1].as_f64() == Some(data) && (top..top + GRID).contains(&y)
            }).map(|index| (section, index))
        })
    }

    /// A click on the grid: the note under it is taken off (picked with Shift), else one is put down at the beat
    /// snap (with Shift, right where the mouse is).
    fn click(&mut self, shift: bool) {
        let Some((column, snapped, _)) = self.under_mouse(true) else { return };
        if column == 0 {
            return self.say("Events are not placed here yet");
        }
        let found = self.note_at(column, mouse_position(vec2(WIDTH, HEIGHT)).y);
        let from = if shift { self.under_mouse(false).map_or(snapped, |(_, free, _)| free) } else { snapped };
        match found {
            Some(note) if shift => self.selected = Some(note),
            Some((section, index)) => {
                self.remember();
                if let Some(notes) = self.json["notes"][section]["sectionNotes"].as_array_mut() {
                    notes.remove(index);
                }
                self.selected = None;
            }
            None => {
                self.remember();
                let section = self.section_at(from);
                let notes = &mut self.json["notes"][section]["sectionNotes"];
                if !notes.is_array() {
                    *notes = json!([]);
                }
                let notes = notes.as_array_mut().expect("made an array");
                notes.push(json!([round(from), column - 1, 0]));
                self.selected = Some((section, notes.len() - 1));
            }
        }
    }

    /// The picked note held longer (or shorter, never below none).
    fn hold_longer(&mut self, by: f64) {
        let Some((section, index)) = self.selected else { return };
        self.remember();
        let note = &mut self.json["notes"][section]["sectionNotes"][index];
        if note.is_array() {
            note[2] = json!(round((note[2].as_f64().unwrap_or(0.0) + by).max(0.0)));
        }
    }

    fn step_history(&mut self, back: bool) {
        let (from, to) = if back { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        let Some(notes) = from.pop() else { return };
        to.push(std::mem::replace(&mut self.json["notes"], notes));
        (self.unsaved, self.leaving) = (true, false);
        self.selected = None;
        self.measure();
        self.say(if back { "Undone" } else { "Redone" });
    }

    /// Over the chart's file, the one before kept as `<file>.bak`.
    fn save(&mut self) {
        let file = &self.entry.chart;
        let backup = PathBuf::from(format!("{}.bak", file.display()));
        let kept = !file.is_file() || std::fs::copy(file, &backup).is_ok();
        let written = kept && funkin_core::files::write(file, &self.text()).is_ok();
        self.unsaved &= !written;
        self.say(match (kept, written) {
            (true, true) => format!("Saved {}", file.file_name().map_or(String::new(), |name| name.to_string_lossy().into_owned())),
            (false, _) => format!("Could not keep a copy of {}; not saved", file.display()),
            (true, false) => format!("Could not save {}", file.display()),
        });
    }

    pub fn draw(&self, skin: &Skin) {
        clear_background(Color::from_hex(0x303030));
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        if let Some(background) = &self.background {
            draw_texture_ex(background, 0.0, 0.0, Color::from_hex(0x303030), DrawTextureParams { dest_size: Some(vec2(WIDTH, HEIGHT)), ..Default::default() });
        }
        let row_height = GRID * ZOOMS[self.zoom];
        let current = self.section_at(self.position);
        // the sections in sight: the one playing light, the others dark; each square a sixteenth
        for (index, span) in self.spans.iter().enumerate() {
            let top = self.y_of(span.start);
            let bottom = top + span.rows() as f32 * row_height;
            if bottom < 0.0 || top > HEIGHT {
                continue;
            }
            let colors = if index == current { [0xDFDFDF, 0xBFBFBF] } else { [0x5F5F5F, 0x4A4A4A] }.map(Color::from_hex);
            for row in 0..span.rows().ceil() as usize {
                let y = top + row as f32 * row_height;
                if y + row_height < 0.0 || y > HEIGHT {
                    continue;
                }
                for column in 0..COLUMNS {
                    draw_rectangle(LEFT + column as f32 * GRID, y, GRID, row_height.min(bottom - y), colors[(row + column) % 2]);
                }
            }
            // the stripes between the events and each side
            for column in [1.0, 5.0] {
                draw_rectangle(LEFT + column * GRID - 1.0, top.max(0.0), 2.0, bottom.min(HEIGHT) - top.max(0.0), BLACK);
            }
        }
        for (index, section) in self.sections().iter().enumerate() {
            for (place, note) in section["sectionNotes"].as_array().into_iter().flatten().enumerate() {
                let (Some(time), Some(data)) = (note[0].as_f64(), note[1].as_f64()) else { continue };
                if !(0.0..8.0).contains(&data) {
                    continue;
                }
                let (lane, y) = (data as usize % 4, self.y_of(time));
                let end = self.y_of(time + note[2].as_f64().unwrap_or(0.0));
                if end < -GRID || y > HEIGHT + GRID {
                    continue;
                }
                let x = LEFT + (data as f32 + 1.0) * GRID + GRID / 2.0;
                let alpha = if time >= self.position { 1.0 } else { 0.6 };
                if end > y + GRID / 2.0 {
                    skin.frame(&format!("{} hold piece0000", NOTE_NAMES[lane]), x, (y + GRID / 2.0 + end) / 2.0, Some(vec2(8.0, end - y - GRID / 2.0)), Some(lane), alpha);
                }
                if self.selected == Some((index, place)) {
                    draw_rectangle(x - GRID / 2.0, y, GRID, GRID, Color { a: 0.5, ..Color::from_hex(0x00FFFF) });
                }
                skin.frame(&format!("{}0000", NOTE_NAMES[lane]), x, y + GRID / 2.0, Some(vec2(GRID, GRID)), Some(lane), alpha);
            }
        }
        // the events: an icon each moment that has some, with how many
        for event in self.json["events"].as_array().into_iter().flatten() {
            let Some(time) = event[0].as_f64() else { continue };
            let y = self.y_of(time);
            if !(-GRID..HEIGHT).contains(&y) {
                continue;
            }
            if let Some(icon) = &self.event_icon {
                draw_texture_ex(icon, LEFT + 5.0, y + 5.0, WHITE, DrawTextureParams { dest_size: Some(vec2(30.0, 30.0)), ..Default::default() });
            }
            let count = event[1].as_array().map_or(0, Vec::len);
            if count > 1 {
                draw_text(&count.to_string(), LEFT + 26.0, y + 36.0, 16.0, WHITE);
            }
        }
        // where the mouse would put a note
        if let Some((column, from, _)) = self.under_mouse(!(held(KeyCode::LeftShift) || held(KeyCode::RightShift))) {
            draw_rectangle(LEFT + column as f32 * GRID, self.y_of(from), GRID, GRID, Color { a: 0.5, ..WHITE });
        }
        draw_rectangle(LEFT, HEIGHT / 2.0 - 2.0, GRID * COLUMNS as f32, 4.0, WHITE);
        // the two sides' icons over their columns (the player's first, as in the Haxe editor)
        for (side, icon) in self.icons.iter().enumerate() {
            let Some(icon) = icon else { continue };
            let pictures = (icon.width() / icon.height()).round().max(1.0);
            let size = vec2(icon.width() / pictures, icon.height()) * 0.3;
            let x = LEFT + GRID + side as f32 * GRID * 4.0 + GRID * 2.0 - size.x / 2.0;
            let source = Rect::new(0.0, 0.0, icon.width() / pictures, icon.height());
            draw_texture_ex(icon, x, 50.0, Color { a: 0.6, ..WHITE }, DrawTextureParams { dest_size: Some(size), source: Some(source), ..Default::default() });
        }
        self.draw_info(skin);
    }

    fn draw_info(&self, skin: &Skin) {
        let span = self.spans.get(self.section_at(self.position)).copied();
        let step = span.map_or(0.0, |span| span.rows_before + (self.position - span.start) / span.step_ms()).floor();
        let time = |ms: f64| format!("{}:{:05.2}", (ms / 60000.0).floor(), (ms / 1000.0) % 60.0);
        let lines = [
            format!("{} / {}", time(self.position), time(self.length)), String::new(),
            format!("Section: {}", self.section_at(self.position)), format!("Beat: {}", (step / 4.0).floor()), format!("Step: {step}"), String::new(),
            format!("Beat Snap: {} / 16", QUANTS[self.quant]), format!("Selected: {}", self.selected.is_some() as u8),
        ];
        let (x, y) = (1000.0, 360.0);
        draw_rectangle(x, y - 26.0, 110.0, 26.0, Color { a: 0.8, ..BLACK });
        draw_text_ex("Information", x + 8.0, y - 8.0, TextParams { font: skin.font.as_ref(), font_size: 16, color: WHITE, ..Default::default() });
        draw_rectangle(x, y, 220.0, 220.0, Color { a: 0.6, ..BLACK });
        for (row, line) in lines.iter().enumerate() {
            draw_text_ex(line, x + 15.0, y + 30.0 + row as f32 * 18.0, TextParams { font: skin.font.as_ref(), font_size: 16, color: WHITE, ..Default::default() });
        }
        let help = "Space: play  W/S, wheel: move  A/D: section  Click: note  Q/E: hold  Left/Right: snap  Z/X: zoom  Ctrl+Z/Y: undo/redo  Ctrl+S: save  Enter: play  Esc: leave";
        draw_text_ex(help, 10.0, HEIGHT - 10.0, TextParams { font: skin.font.as_ref(), font_size: 14, color: Color { a: 0.8, ..WHITE }, ..Default::default() });
        if let Some((words, when)) = &self.said {
            let alpha = (1.0 - (get_time() - when - 2.0).max(0.0)) as f32;
            if alpha > 0.0 {
                draw_text_ex(words, 25.0, HEIGHT - 32.0, TextParams { font: skin.font.as_ref(), font_size: 20, color: Color { a: alpha, ..WHITE }, ..Default::default() });
            }
        }
    }
}

/// A key down, or pressed this frame (a modifier pressed and let go of within one frame still counts).
fn held(key: KeyCode) -> bool {
    is_key_down(key) || is_key_pressed(key)
}

/// Times kept to the thousandth, as the Haxe editor's.
fn round(ms: f64) -> f64 {
    (ms * 1000.0).round() / 1000.0
}

/// An older chart made a Psych 1.0 one, as Song.convert does: each note's column made absolute (0-3 the player's,
/// 4-7 the opponent's), events kept among the notes moved to the events, the format named.
fn convert(song: &mut Value) {
    if song["format"].as_str().is_some_and(|format| format.starts_with("psych_v1")) {
        return;
    }
    let mut events = Vec::new();
    for section in song["notes"].as_array_mut().into_iter().flatten() {
        let must_hit = section["mustHitSection"].as_bool().unwrap_or(false);
        if section["sectionBeats"].as_f64().is_none() {
            section["sectionBeats"] = json!(section["lengthInSteps"].as_f64().map_or(4.0, |steps| steps / 4.0));
        }
        if let Some(notes) = section["sectionNotes"].as_array_mut() {
            notes.retain_mut(|note| {
                let data = note[1].as_f64().unwrap_or(0.0);
                if data < 0.0 {
                    events.push(json!([note[0], [[note[2], note[3], note[4]]]]));
                    return false;
                }
                let player = if data < 4.0 { must_hit } else { !must_hit };
                note[1] = json!(data as i64 % 4 + if player { 0 } else { 4 });
                true
            });
        }
    }
    if !song["events"].is_array() {
        song["events"] = json!([]);
    }
    if let Some(kept) = song["events"].as_array_mut() {
        kept.extend(events);
    }
    song["format"] = json!("psych_v1");
}

/// Sections enough for the whole song (new ones empty, four beats, on the player's side), as the Haxe editor adds them.
fn fill_sections(song: &mut Value, length: f64) {
    if !song["notes"].is_array() {
        song["notes"] = json!([]);
    }
    let mut bpm = song["bpm"].as_f64().filter(|bpm| *bpm > 0.0).unwrap_or(100.0);
    let mut end = 0.0;
    for section in song["notes"].as_array().expect("made an array") {
        if section["changeBPM"].as_bool().unwrap_or(false) {
            bpm = section["bpm"].as_f64().filter(|bpm| *bpm > 0.0).unwrap_or(bpm);
        }
        end += section["sectionBeats"].as_f64().filter(|beats| *beats > 0.0).unwrap_or(4.0) * 60000.0 / bpm;
    }
    let notes = song["notes"].as_array_mut().expect("made an array");
    while end < length {
        notes.push(json!({"sectionNotes": [], "sectionBeats": 4, "mustHitSection": true, "gfSection": false, "altAnim": false, "changeBPM": false, "bpm": bpm}));
        end += 4.0 * 60000.0 / bpm;
    }
}

/// JSON as the Haxe editor writes charts (PsychJsonPrinter): objects a key a line; a list a thing a line, a thing
/// without objects in it (a note, an event) on its one line.
fn print(value: &Value, depth: usize) -> String {
    let indent = |depth: usize| "\t".repeat(depth);
    let plain = |value: &Value| !value.is_object() && value.as_array().is_none_or(|items| items.iter().all(|item| !item.is_object() && !item.is_array()));
    let flat = |value: &Value| value.as_array().is_some_and(|items| items.iter().all(|item| !item.is_object() && item.as_array().is_none_or(|inner| inner.iter().all(|v| !v.is_object()))));
    match value {
        Value::Object(map) if !map.is_empty() => {
            let fields: Vec<String> = map.iter().map(|(key, value)| format!("{}{}: {}", indent(depth + 1), Value::String(key.clone()), print(value, depth + 1))).collect();
            format!("{{\n{}\n{}}}", fields.join(",\n"), indent(depth))
        }
        Value::Array(items) if !plain(value) => {
            let items: Vec<String> = items.iter().map(|item| format!("{}{}", indent(depth + 1), if flat(item) { item.to_string() } else { print(item, depth + 1) })).collect();
            format!("[\n{}\n{}]", items.join(",\n"), indent(depth))
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_finds_the_note_drawn_under_it() {
        // 120 beats a minute: a sixteenth is 125 ms, 40 px at the usual zoom, now at the middle line (360)
        let text = r#"{"bpm": 120, "format": "psych_v1", "notes": [{"sectionNotes": [[0, 1, 0], [125, 1, 0], [125, 2, 0]], "sectionBeats": 4}]}"#;
        let missing = PathBuf::from("no such folder");
        let entry = SongEntry {
            title: "T".to_owned(), difficulty: "Hard".to_owned(), source: String::new(), week: 0, icon: String::new(), color: [0; 3],
            chart: missing.join("t.json"), inst: missing.join("Inst.ogg"), voices_player: None, voices_opponent: None,
        };
        let editor = ChartEditor::open(&missing, &entry, Some(text.to_owned())).unwrap();
        // a sixteenth apart in one column: each is found by its own box, even where one beat snap holds both
        assert_eq!(editor.note_at(2, 370.0), Some((0, 0)));
        assert_eq!(editor.note_at(2, 410.0), Some((0, 1)));
        assert_eq!(editor.note_at(3, 410.0), Some((0, 2)));
        assert_eq!([editor.note_at(2, 450.0), editor.note_at(0, 370.0)], [None, None]);
        assert!(editor.unsaved, "back from a play-test, it may hold changes");
    }

    #[test]
    fn old_charts_get_absolute_columns_and_their_events_moved() {
        let mut song = json!({"bpm": 120, "notes": [
            {"mustHitSection": false, "sectionNotes": [[0, 1, 0], [100, 5, 0], [200, -1, "Hey!", "", ""]], "lengthInSteps": 16},
            {"mustHitSection": true, "sectionNotes": [[2000, 2, 0]], "sectionBeats": 4}
        ]});
        convert(&mut song);
        assert_eq!(song["format"], "psych_v1");
        assert_eq!(song["notes"][0]["sectionNotes"], json!([[0, 5, 0], [100, 1, 0]]));
        assert_eq!(song["notes"][1]["sectionNotes"], json!([[2000, 2, 0]]));
        assert_eq!(song["notes"][0]["sectionBeats"], json!(4.0));
        assert_eq!(song["events"], json!([[200, [["Hey!", "", ""]]]]));
    }

    #[test]
    fn sections_fill_the_song_and_charts_print_back_alike() {
        let mut song = json!({"bpm": 120, "format": "psych_v1", "notes": [{"sectionNotes": [[0, 1, 0]], "sectionBeats": 4}]});
        // two seconds a section at 120: five seconds take three
        fill_sections(&mut song, 5000.0);
        assert_eq!(song["notes"].as_array().map(Vec::len), Some(3));
        let text = print(&song, 0);
        assert!(text.contains("[0,1,0]"), "a note on one line: {text}");
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), song);
    }
}
