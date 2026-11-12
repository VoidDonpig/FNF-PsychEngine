//! The chart editor (states/editors/ChartingState.hx), its first part: the song's notes on the Haxe engine's grid (the
//! events' column, then the player's four, then the opponent's four), scrolling under a line as the music plays; notes
//! put down and taken off with the mouse at the chosen beat snap, held longer or shorter with Q and E; undo and redo;
//! play-testing the chart as it is (Enter) and saving it over its file, the old one kept beside it as `.bak`. Events
//! are put down in their column; the box at the right has the Haxe editor's Song, Section, Note and Events tabs.
// shortcut: the File/Edit/View menus and the Charting and Data tabs are not here, characters and stages are typed
// rather than chosen from lists, and Escape leaves (the Haxe editor's File > Exit).
use crate::gfx::*;
use crate::{audio, ui::Ui, Skin, HEIGHT, NOTE_NAMES, WIDTH};
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

/// The Haxe editor's events (ChartingState.defaultEvents), with what their values mean.
const EVENTS: [(&str, &str); 17] = [
    ("", "Nothing. Yep, that's right."),
    ("Dadbattle Spotlight", "Used in Dad Battle,\nValue 1: 0/1 = ON/OFF,\n2 = Target Dad\n3 = Target BF"),
    ("Hey!", "Plays the \"Hey!\" animation from Bopeebo,\nValue 1: BF = Only Boyfriend, GF = Only Girlfriend,\nSomething else = Both.\nValue 2: Custom animation duration,\nleave it blank for 0.6s"),
    ("Set GF Speed", "Sets GF head bopping speed,\nValue 1: 1 = Normal speed,\n2 = 1/2 speed, 4 = 1/4 speed etc.\nUsed on Fresh during the beatbox parts.\n\nWarning: Value must be integer!"),
    ("Philly Glow", "Exclusive to Week 3\nValue 1: 0/1/2 = OFF/ON/Reset Gradient\n \nNo, i won't add it to other weeks."),
    ("Kill Henchmen", "For Mom's songs, don't use this please, i love them :("),
    ("Add Camera Zoom", "Used on MILF on that one \"hard\" part\nValue 1: Camera zoom add (Default: 0.015)\nValue 2: UI zoom add (Default: 0.03)\nLeave the values blank if you want to use Default."),
    ("BG Freaks Expression", "Should be used only in \"school\" Stage!"),
    ("Trigger BG Ghouls", "Should be used only in \"schoolEvil\" Stage!"),
    ("Play Animation", "Plays an animation on a Character,\nonce the animation is completed,\nthe animation changes to Idle\n\nValue 1: Animation to play.\nValue 2: Character (Dad, BF, GF)"),
    ("Camera Follow Pos", "Value 1: X\nValue 2: Y\n\nThe camera won't change the follow point\nafter using this, for getting it back\nto normal, leave both values blank."),
    ("Alt Idle Animation", "Sets a specified postfix after the idle animation name.\nYou can use this to trigger 'idle-alt' if you set\nValue 2 to -alt\n\nValue 1: Character to set (Dad, BF or GF)\nValue 2: New postfix (Leave it blank to disable)"),
    ("Screen Shake", "Value 1: Camera shake\nValue 2: HUD shake\n\nEvery value works as the following example: \"1, 0.05\".\nThe first number (1) is the duration.\nThe second number (0.05) is the intensity."),
    ("Change Character", "Value 1: Character to change (Dad, BF, GF)\nValue 2: New character's name"),
    ("Change Scroll Speed", "Value 1: Scroll Speed Multiplier (1 is default)\nValue 2: Time it takes to change fully in seconds."),
    ("Set Property", "Value 1: Variable name\nValue 2: New value"),
    ("Play Sound", "Value 1: Sound file name\nValue 2: Volume (Default: 1), ranges from 0 to 1"),
];

/// The Haxe engine's note types (Note.defaultNoteTypes).
const NOTE_TYPES: [&str; 6] = ["", "Alt Animation", "Hey!", "Hurt Note", "GF Sing", "No Animation"];

/// What is picked: a note (section, place in its notes) or an event (place in the events, which of its entries).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Picked {
    Note(usize, usize),
    Event(usize, usize),
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
    /// The note or event last put down or picked.
    selected: Option<Picked>,
    undo: Vec<Value>,
    redo: Vec<Value>,
    /// Changed since it was last saved (or brought back from a play-test), and Escape was pressed once already.
    unsaved: bool,
    leaving: bool,
    said: Option<(String, f64)>,
    background: Option<Texture2D>,
    event_icon: Option<Texture2D>,
    icons: [Option<Texture2D>; 2],
    ui: Ui,
    tab: usize,
    /// The note type and event new ones get (and the picked one is set to), as chosen in their tabs; the event's values.
    note_types: Vec<String>,
    note_type: usize,
    events: Vec<String>,
    abouts: Vec<String>,
    event: usize,
    values: [String; 2],
    /// A section's notes copied (their times from its start).
    copied: Option<Vec<Value>>,
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
        // the types and events the mods bring (custom_notetypes/, custom_events/), and those the chart already uses
        let named = |folder: &str, endings: &[&str]| {
            let mut names: Vec<String> = paths.roots().iter().filter_map(|root| std::fs::read_dir(root.join(folder)).ok()).flatten().flatten()
                .filter_map(|file| { let path = file.path(); endings.iter().any(|ending| path.extension().is_some_and(|own| own == *ending)).then(|| path) })
                .filter_map(|path| path.file_stem().map(|stem| stem.to_string_lossy().into_owned())).collect();
            names.sort();
            names.dedup();
            names
        };
        let mut note_types: Vec<String> = NOTE_TYPES.iter().map(|name| name.to_string()).collect();
        let used = json["notes"].as_array().into_iter().flatten().flat_map(|section| section["sectionNotes"].as_array().into_iter().flatten()).filter_map(|note| note[3].as_str().map(str::to_owned));
        for name in named("custom_notetypes", &["lua", "hx"]).into_iter().chain(used) {
            if !note_types.contains(&name) {
                note_types.push(name);
            }
        }
        let (mut events, mut abouts): (Vec<String>, Vec<String>) = EVENTS.iter().map(|(name, about)| (name.to_string(), about.to_string())).unzip();
        for name in named("custom_events", &["txt"]) {
            if !events.contains(&name) {
                abouts.push(paths.find(&format!("custom_events/{name}.txt")).and_then(|file| std::fs::read_to_string(file).ok()).unwrap_or_default());
                events.push(name);
            }
        }
        let mut editor = ChartEditor {
            entry: entry.clone(), json, spans: Vec::new(), song, playing: false, position: 0.0, length, zoom: 2, quant: 3, selected: None,
            undo: Vec::new(), redo: Vec::new(), unsaved, leaving: false, said: None, background: picture("menuDesat"), event_icon: picture("editors/eventIcon"), icons,
            ui: Ui::default(), tab: 3, note_types, note_type: 0, events, abouts, event: 0, values: Default::default(), copied: None,
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
        self.undo.push(self.json.clone());
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
        // (keys typed into a box are not the editor's: what was asked before this frame's controls)
        let typing = self.ui.typing();
        self.ui.begin();
        self.panel();
        let dt = get_frame_time().min(0.1) as f64;
        let (ctrl, shift) = (held(KeyCode::LeftControl) || held(KeyCode::RightControl), held(KeyCode::LeftShift) || held(KeyCode::RightShift));
        let current = self.section_at(self.position);
        let span = self.spans.get(current).copied().unwrap_or(Span { start: 0.0, beats: 4.0, bpm: 100.0, rows_before: 0.0 });
        if typing {
        } else if ctrl {
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
            if let Some(picked) = self.selected.take() {
                self.remember();
                let (list, index) = match picked {
                    Picked::Note(section, index) => (&mut self.json["notes"][section]["sectionNotes"], index),
                    Picked::Event(index, _) => (&mut self.json["events"], index),
                };
                if let Some(list) = list.as_array_mut().filter(|list| index < list.len()) {
                    list.remove(index);
                }
            }
        }
        if self.playing {
            self.position = self.song.position_ms();
            if self.position >= self.length {
                self.wind(self.length);
            }
        }
        if is_mouse_pressed() && !self.ui.took_click() {
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
        let from = if shift { self.under_mouse(false).map_or(snapped, |(_, free, _)| free) } else { snapped };
        let y = mouse_position(vec2(WIDTH, HEIGHT)).y;
        if column == 0 {
            return self.click_event(y, from, shift);
        }
        match self.note_at(column, y) {
            Some((section, index)) if shift => self.selected = Some(Picked::Note(section, index)),
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
                let mut note = json!([round(from), column - 1, 0]);
                // (with the note type chosen in the Note tab)
                if let Some(kind) = self.note_types.get(self.note_type).filter(|kind| !kind.is_empty()) {
                    note.as_array_mut().expect("an array").push(json!(kind));
                }
                notes.push(note);
                self.selected = Some(Picked::Note(section, notes.len() - 1));
            }
        }
    }

    /// The event drawn under a point of the events' column (its place in the events).
    fn event_at(&self, y: f32) -> Option<usize> {
        self.json["events"].as_array()?.iter().position(|event| {
            let top = event[0].as_f64().map_or(f32::MAX, |time| self.y_of(time));
            (top..top + GRID).contains(&y)
        })
    }

    /// A click on the events' column: the event under it is taken off (picked with Shift, its values shown in the
    /// Events tab), else one is put down with the event and values chosen there.
    fn click_event(&mut self, y: f32, at: f64, shift: bool) {
        match self.event_at(y) {
            Some(index) if shift => {
                self.selected = Some(Picked::Event(index, 0));
                self.show_event();
            }
            Some(index) => {
                self.remember();
                if let Some(events) = self.json["events"].as_array_mut() {
                    events.remove(index);
                }
                self.selected = None;
            }
            None => {
                self.remember();
                if !self.json["events"].is_array() {
                    self.json["events"] = json!([]);
                }
                let name = self.events.get(self.event).cloned().unwrap_or_default();
                let events = self.json["events"].as_array_mut().expect("made an array");
                // (kept in time order)
                let place = events.iter().position(|event| event[0].as_f64().is_some_and(|time| time > at)).unwrap_or(events.len());
                events.insert(place, json!([round(at), [[name, self.values[0], self.values[1]]]]));
                self.selected = Some(Picked::Event(place, 0));
            }
        }
    }

    /// The Events tab shows the picked event's entry: its name and values.
    fn show_event(&mut self) {
        let Some(Picked::Event(index, entry)) = self.selected else { return };
        let entry = &self.json["events"][index][1][entry];
        let name = entry[0].as_str().unwrap_or_default();
        if let Some(found) = self.events.iter().position(|known| known == name) {
            self.event = found;
        }
        self.values = [plain(&entry[1]), plain(&entry[2])];
    }

    /// The picked note held longer (or shorter, never below none).
    fn hold_longer(&mut self, by: f64) {
        let Some(Picked::Note(section, index)) = self.selected else { return };
        self.remember();
        let note = &mut self.json["notes"][section]["sectionNotes"][index];
        if note.is_array() {
            note[2] = json!(round((note[2].as_f64().unwrap_or(0.0) + by).max(0.0)));
        }
    }

    fn step_history(&mut self, back: bool) {
        let (from, to) = if back { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        let Some(chart) = from.pop() else { return };
        to.push(std::mem::replace(&mut self.json, chart));
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
                if self.selected == Some(Picked::Note(index, place)) {
                    draw_rectangle(x - GRID / 2.0, y, GRID, GRID, Color { a: 0.5, ..Color::from_hex(0x00FFFF) });
                }
                skin.frame(&format!("{}0000", NOTE_NAMES[lane]), x, y + GRID / 2.0, Some(vec2(GRID, GRID)), Some(lane), alpha);
            }
        }
        // the events: an icon each moment that has some, with how many, and what they are written beside the grid
        for (index, event) in self.json["events"].as_array().into_iter().flatten().enumerate() {
            let Some(time) = event[0].as_f64() else { continue };
            let y = self.y_of(time);
            if !(-GRID..HEIGHT).contains(&y) {
                continue;
            }
            if matches!(self.selected, Some(Picked::Event(picked, _)) if picked == index) {
                draw_rectangle(LEFT, y, GRID, GRID, Color { a: 0.5, ..Color::from_hex(0x00FFFF) });
            }
            let entries = event[1].as_array().map_or(&[][..], Vec::as_slice);
            let words = match entries {
                [one] => format!("{} - {}, {}", one[0].as_str().unwrap_or_default(), plain(&one[1]), plain(&one[2])),
                many => format!("{} Events: {}", many.len(), many.iter().map(|entry| entry[0].as_str().unwrap_or_default()).collect::<Vec<_>>().join(", ")),
            };
            let width = measure_text(&words, skin.font.as_ref(), 14, 1.0).width;
            draw_text_ex(&words, LEFT - width - 6.0, y + GRID / 2.0 + 5.0, TextParams { font: skin.font.as_ref(), font_size: 14, color: WHITE, ..Default::default() });
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
        self.ui.draw(skin.font.as_ref());
    }

    /// Sets a field of the song (a step to undo), and measures the sections again (the tempo may have changed).
    fn set(&mut self, field: &str, value: Value) {
        self.remember();
        self.json[field] = value;
        self.measure();
    }

    fn set_section(&mut self, section: usize, field: &str, value: Value) {
        self.remember();
        self.json["notes"][section][field] = value;
        self.measure();
    }

    /// The box at the right with the Haxe editor's Song, Section, Note and Events tabs.
    fn panel(&mut self) {
        let area = Rect::new(920.0, 60.0, 300.0, 260.0);
        self.tab = self.ui.tabs(area, &["Events", "Note", "Section", "Song"], self.tab);
        let (x, y) = (area.x + 10.0, area.y + 10.0);
        match self.tab {
            0 => self.events_tab(x, y),
            1 => self.note_tab(x, y),
            2 => self.section_tab(x, y),
            _ => self.song_tab(x, y),
        }
    }

    fn song_tab(&mut self, x: f32, y: f32) {
        let text = |json: &Value, field: &str| json[field].as_str().unwrap_or_default().to_owned();
        self.ui.label(x, y, "Song Name:");
        if let Some(name) = self.ui.input("song", x, y + 18.0, 140.0, &text(&self.json, "song")) {
            self.set("song", json!(name));
        }
        if let Some(voices) = self.ui.check(x + 150.0, y + 18.0, "Allow Vocals", self.json["needsVoices"].as_bool().unwrap_or(true)) {
            self.set("needsVoices", json!(voices));
        }
        self.ui.label(x, y + 48.0, "BPM:");
        let bpm = self.json["bpm"].as_f64().unwrap_or(100.0);
        if let Some(bpm) = self.ui.stepper("bpm", x, y + 66.0, 70.0, bpm, 1.0, 1.0, 999.0) {
            self.set("bpm", json!(bpm));
        }
        self.ui.label(x + 150.0, y + 48.0, "Scroll Speed:");
        let speed = self.json["speed"].as_f64().unwrap_or(1.0);
        if let Some(speed) = self.ui.stepper("speed", x + 150.0, y + 66.0, 70.0, speed, 0.1, 0.1, 10.0) {
            self.set("speed", json!((speed * 100.0).round() / 100.0));
        }
        for (index, (field, words)) in [("player1", "Player:"), ("player2", "Opponent:"), ("gfVersion", "Girlfriend:"), ("stage", "Stage:")].into_iter().enumerate() {
            let (x, y) = (x + 150.0 * (index % 2) as f32, y + 96.0 + 44.0 * (index / 2) as f32);
            self.ui.label(x, y, words);
            if let Some(value) = self.ui.input(field, x, y + 18.0, 130.0, &text(&self.json, field)) {
                self.set(field, json!(value));
            }
        }
        if self.ui.button(x, y + 200.0, 120.0, "Save (Ctrl+S)") {
            self.save();
        }
    }

    fn section_tab(&mut self, x: f32, y: f32) {
        let section = self.section_at(self.position);
        let flag = |json: &Value, field: &str| json["notes"][section][field].as_bool().unwrap_or(false);
        self.ui.label(x, y, &format!("Section {section}"));
        for (index, (field, words)) in [("mustHitSection", "Must Hit Section"), ("gfSection", "GF Section"), ("altAnim", "Alt Animation")].into_iter().enumerate() {
            if let Some(on) = self.ui.check(x, y + 22.0 + 24.0 * index as f32, words, flag(&self.json, field)) {
                self.set_section(section, field, json!(on));
            }
        }
        if let Some(on) = self.ui.check(x, y + 96.0, "Change BPM", flag(&self.json, "changeBPM")) {
            self.set_section(section, "changeBPM", json!(on));
        }
        let span = self.spans.get(section).copied();
        let bpm = self.json["notes"][section]["bpm"].as_f64().or(span.map(|span| span.bpm)).unwrap_or(100.0);
        if let Some(bpm) = self.ui.stepper("section bpm", x + 150.0, y + 96.0, 60.0, bpm, 1.0, 1.0, 999.0) {
            self.set_section(section, "bpm", json!(bpm));
        }
        self.ui.label(x, y + 124.0, "Beats per Section:");
        if let Some(beats) = self.ui.stepper("beats", x + 150.0, y + 126.0, 60.0, span.map_or(4.0, |span| span.beats), 1.0, 1.0, 16.0) {
            self.set_section(section, "sectionBeats", json!(beats));
        }
        let start = span.map_or(0.0, |span| span.start);
        if self.ui.button(x, y + 160.0, 130.0, "Copy Section") {
            let notes = self.json["notes"][section]["sectionNotes"].as_array().cloned().unwrap_or_default();
            self.copied = Some(notes.into_iter().map(|mut note| {
                note[0] = json!(round(note[0].as_f64().unwrap_or(0.0) - start));
                note
            }).collect());
        }
        if self.ui.button(x + 150.0, y + 160.0, 130.0, "Paste Section") {
            if let Some(copied) = self.copied.clone() {
                self.remember();
                let notes = &mut self.json["notes"][section]["sectionNotes"];
                if !notes.is_array() {
                    *notes = json!([]);
                }
                notes.as_array_mut().expect("made an array").extend(copied.into_iter().map(|mut note| {
                    note[0] = json!(round(note[0].as_f64().unwrap_or(0.0) + start));
                    note
                }));
            }
        }
        if self.ui.button(x, y + 186.0, 130.0, "Clear Notes") {
            self.selected = None;
            self.set_section(section, "sectionNotes", json!([]));
        }
        // the player's notes become the opponent's and back
        if self.ui.button(x + 150.0, y + 186.0, 130.0, "Swap Section") {
            self.remember();
            for note in self.json["notes"][section]["sectionNotes"].as_array_mut().into_iter().flatten() {
                if let Some(data) = note[1].as_i64().filter(|data| (0..8).contains(data)) {
                    note[1] = json!((data + 4) % 8);
                }
            }
        }
    }

    fn note_tab(&mut self, x: f32, y: f32) {
        let picked = match self.selected {
            Some(Picked::Note(section, index)) if self.json["notes"][section]["sectionNotes"][index].is_array() => Some((section, index)),
            _ => None,
        };
        let step = self.spans.get(self.section_at(self.position)).map_or(150.0, |span| span.step_ms());
        if let Some((section, index)) = picked {
            let note = self.json["notes"][section]["sectionNotes"][index].clone();
            self.ui.label(x, y, "Sustain length:");
            if let Some(length) = self.ui.stepper("sustain", x, y + 18.0, 80.0, note[2].as_f64().unwrap_or(0.0), step / 2.0, 0.0, step * 128.0) {
                self.remember();
                self.json["notes"][section]["sectionNotes"][index][2] = json!(round((length / (step / 2.0)).round() * step / 2.0));
            }
            self.ui.label(x, y + 46.0, "Strum time (in ms):");
            if let Some(time) = self.ui.input("strum time", x, y + 64.0, 120.0, &plain(&note[0])).and_then(|text| text.trim().parse::<f64>().ok()) {
                self.remember();
                self.json["notes"][section]["sectionNotes"][index][0] = json!(round(time.max(0.0)));
            }
        } else {
            self.ui.label(x, y, "No note picked (Shift+click one).");
        }
        self.ui.label(x, y + 92.0, "Note type:");
        if let Some(kind) = self.ui.dropdown("note type", x, y + 110.0, 200.0, &self.note_types, self.note_type) {
            self.note_type = kind;
            if let Some((section, index)) = picked {
                self.remember();
                let note = self.json["notes"][section]["sectionNotes"][index].as_array_mut().expect("checked");
                note.truncate(3);
                if !self.note_types[kind].is_empty() {
                    note.push(json!(self.note_types[kind]));
                }
            }
        }
    }

    fn events_tab(&mut self, x: f32, y: f32) {
        let picked = match self.selected {
            Some(Picked::Event(index, entry)) if self.json["events"][index][1][entry].is_array() => Some((index, entry)),
            _ => None,
        };
        self.ui.label(x, y, "Event:");
        // (the list first: open, it drops down over the values and takes its clicks before they do)
        if let Some(event) = self.ui.dropdown("event", x, y + 18.0, 140.0, &self.events, self.event) {
            self.event = event;
            if let Some((index, entry)) = picked {
                self.remember();
                self.json["events"][index][1][entry][0] = json!(self.events[event]);
            }
        }
        let description = self.abouts.get(self.event).cloned().unwrap_or_default();
        self.ui.label(x, y + 42.0, &description);
        for (value, words) in [(0, "Value 1:"), (1, "Value 2:")] {
            let top = y + 160.0 + 40.0 * value as f32;
            self.ui.label(x, top, words);
            if let Some(text) = self.ui.input(["value 1", "value 2"][value], x, top + 16.0, 200.0, &self.values[value]) {
                self.values[value] = text.clone();
                if let Some((index, entry)) = picked {
                    self.remember();
                    self.json["events"][index][1][entry][value + 1] = json!(text);
                }
            }
        }
        if let Some((index, entry)) = picked {
            let count = self.json["events"][index][1].as_array().map_or(1, Vec::len);
            self.ui.label(x + 150.0, y, &format!("Entry {} / {count}", entry + 1));
            // another entry of the same moment: before, after, taken off, added
            let mut go = None;
            for (place, words) in ["<", ">", "-", "+"].into_iter().enumerate() {
                if self.ui.button(x + 150.0 + 30.0 * place as f32, y + 18.0, 26.0, &format!(" {words}")) {
                    go = Some(place);
                }
            }
            match go {
                Some(0) => self.selected = Some(Picked::Event(index, entry.saturating_sub(1))),
                Some(1) => self.selected = Some(Picked::Event(index, (entry + 1).min(count - 1))),
                Some(2) if count > 1 => {
                    self.remember();
                    if let Some(entries) = self.json["events"][index][1].as_array_mut() {
                        entries.remove(entry);
                    }
                    self.selected = Some(Picked::Event(index, entry.saturating_sub(1)));
                }
                Some(3) => {
                    self.remember();
                    let name = self.events.get(self.event).cloned().unwrap_or_default();
                    if let Some(entries) = self.json["events"][index][1].as_array_mut() {
                        entries.push(json!([name, self.values[0], self.values[1]]));
                    }
                    self.selected = Some(Picked::Event(index, count));
                }
                _ => {}
            }
            if go.is_some() {
                self.show_event();
            }
        }
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

/// A value of an event as text (they are mostly text, sometimes numbers).
fn plain(value: &Value) -> String {
    value.as_str().map_or_else(|| if value.is_null() { String::new() } else { value.to_string() }, str::to_owned)
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
