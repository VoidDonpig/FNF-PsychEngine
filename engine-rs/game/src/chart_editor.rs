//! The chart editor (states/editors/ChartingState.hx), its first part: the song's notes on the Haxe engine's grid (the
//! events' column, then the player's four, then the opponent's four), scrolling under a line as the music plays; notes
//! put down and taken off with the mouse at the chosen beat snap, held longer or shorter with Q and E; undo and redo;
//! play-testing the chart as it is (Enter) and saving it over its file, the old one kept beside it as `.bak`. Events
//! are put down in their column; the box at the right has the Haxe editor's Charting, Events, Note, Section and Song
//! tabs, and the File, Edit and View menus at the top left do what the keys and the tabs do (and a few things more).
use crate::gfx::*;
use crate::dialog::{self, Answer};
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

impl Picked {
    /// Which list it is in and where (an event's entries share its place).
    fn place(&self) -> (usize, usize) {
        match *self {
            Picked::Note(section, index) => (section + 1, index),
            Picked::Event(index, _) => (0, index),
        }
    }
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
    /// The notes and events picked (the one last put down, Shift+clicked, boxed in with Ctrl+drag, Ctrl+A).
    selected: Vec<Picked>,
    /// Where a Ctrl+drag began: the mouse's x and the moment under it.
    boxing: Option<(f32, f64)>,
    /// Notes and events copied (Ctrl+C, Ctrl+X), their times from the first one's.
    clipboard: Vec<(bool, Value)>,
    undo: Vec<Value>,
    redo: Vec<Value>,
    /// Changed since it was last saved (or brought back from a play-test), and Escape was pressed once already.
    unsaved: bool,
    leaving: bool,
    said: Option<(String, std::time::Instant)>,
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
    /// The Charting tab: how fast the music plays, whether the wheel steps by the beat snap, how loud the player's and
    /// the opponent's hitsounds and the metronome are, each track's volume and whether it is muted (instrumental,
    /// player's voice, opponent's voice); where the music was last frame, for the sounds on the way.
    rate: f64,
    wheel_snaps: bool,
    /// The Vortex Editor: keys 1 to 8 put down (or take off) notes of their column at the line, on the beat snap.
    vortex: bool,
    hitsounds: [f32; 3],
    volumes: [f32; 3],
    muted: [bool; 3],
    heard: f64,
    /// Which tracks the voices are (None: the song has no such file).
    voice_tracks: [Option<usize>; 2],
    sounds: [Option<PathBuf>; 2],
    /// Autosaves (the Haxe editor's backups/<chart>_<date>.bkp): the game's folder, when the last was made and what it
    /// held (none is made when nothing changed since).
    base: PathBuf,
    autosaved: (std::time::Instant, String),
    /// How often (minutes; 0: never) and how many autosaves are kept (0: all); these and the Charting tab's switches as
    /// last kept in the editor's file (the Haxe editor's chartEditorSave).
    autosave_minutes: f64,
    /// The characters and stages there are, for the Song tab's lists.
    characters: Vec<String>,
    stages: Vec<String>,
    /// The charts of the song's folder (each song at each difficulty), for the Song tab's Open Chart.
    charts: Vec<SongEntry>,
    backup_limit: usize,
    prefs: Value,
    /// The waveform drawn on the grid: of which track (0 none, 1 the instrumental, 2 and 3 the voices), its peaks.
    waveform: (usize, Peaks),
    /// Pasted notes and events following the mouse until a click puts them down.
    floating: Option<Floating>,
    /// The Vortex Editor's strums: how long each column's stays lit (seconds) after a note passed it.
    lit: [f64; 8],
    /// The Edit menu's Lock Events: the events' column cannot be clicked on.
    locked_events: bool,
    /// The View menu's Note Labels: notes of a type show its number in the list.
    labels: bool,
    /// The View menu's Go to...: the time (ms) and section to go to, while its box is open.
    going: Option<(f64, usize)>,
    /// The chart has no file of its own yet (File > New): Save asks where it goes. Its `entry.chart` is a name not
    /// taken in the folder it came from.
    untitled: bool,
    /// The system's file dialog that is up, and what it was asked for; the editor waits for it.
    dialog: Option<(Want, dialog::Pending)>,
    /// With no dialog here: the file's name typed (beside the chart).
    typed: Option<(Want, String)>,
    /// A question about a file chosen (open it over changes not saved, the events found in it).
    prompt: Option<Prompt>,
    /// The View menu's Theme...: the theme, the Custom one's colours (the background, the grid's, the other
    /// sections' grid's), whether its box is open.
    theme: Theme,
    custom_colors: [u32; 5],
    theming: bool,
}

/// What a file is chosen for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Want {
    OpenChart,
    OpenEvents,
    SaveAs,
    SaveEventsAs,
}

impl Want {
    fn title(self) -> &'static str {
        match self {
            Want::OpenChart => "Open Chart",
            Want::OpenEvents => "Open Events",
            Want::SaveAs => "Save Chart As",
            Want::SaveEventsAs => "Save Events As",
        }
    }
}

enum Prompt {
    /// A chart chosen while this one has changes not saved.
    OpenChart(PathBuf),
    /// The events of an events file: in place of the chart's or added to them.
    Events(Vec<Value>),
}

/// The Haxe editor's themes (ChartingTheme): the background's colour, the grid's two, the other sections' grid's two.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Theme {
    Light,
    Dark,
    Default,
    VSlice,
    Custom,
}

/// Each theme, as it is kept and as its button reads.
const THEMES: [(Theme, &str, &str); 5] = [(Theme::Light, "light", "Light"), (Theme::Dark, "dark", "Dark"), (Theme::Default, "default", "Default"), (Theme::VSlice, "vslice", "V-Slice"), (Theme::Custom, "custom", "Custom")];
const DEFAULT_COLORS: [u32; 5] = [0x303030, 0xDFDFDF, 0xBFBFBF, 0x5F5F5F, 0x4A4A4A];

impl Theme {
    fn colors(self, custom: [u32; 5]) -> [u32; 5] {
        match self {
            Theme::Light => [0xA0A0A0, 0xDFDFDF, 0xBFBFBF, 0x5F5F5F, 0x4A4A4A],
            Theme::Dark => [0x222222, 0x3F3F3F, 0x2F2F2F, 0x1F1F1F, 0x111111],
            Theme::Default => DEFAULT_COLORS,
            Theme::VSlice => [0x673AB7, 0xD0D0D0, 0xAFAFAF, 0x595959, 0x464646],
            Theme::Custom => custom,
        }
    }
}

/// Pasted notes and events (the Haxe editor's moving notes).
struct Floating {
    /// What was copied: times from the first one's, columns from the leftmost note's.
    things: Vec<(bool, Value)>,
    /// Where they are: the first one's time, the leftmost note's column (0 to 7).
    at: f64,
    column: usize,
    /// The column the mouse was last over: the notes move by as many columns as it does.
    mouse: usize,
}

impl Floating {
    /// How many columns the notes reach past the leftmost one.
    fn width(&self) -> usize {
        self.things.iter().filter(|(event, _)| !event).filter_map(|(_, note)| note[1].as_f64()).fold(0.0, f64::max) as usize
    }
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
        let voice_tracks = [entry.voices_player.as_ref().map(|_| 1), entry.voices_opponent.as_ref().map(|_| tracks.len() - 1)];
        let song = audio::Song::load(&tracks);
        let length = song.length_ms().or_else(|| audio::seconds(&entry.inst).map(|seconds| seconds as f64 * 1000.0)).unwrap_or(60000.0);
        fill_sections(&mut json, length);
        let paths = Paths::new(base, &entry.source, "");
        let picture = |name: &str| paths.find(&format!("images/{name}.png")).and_then(|path| crate::atlas::picture(&path));
        let icons = [icon_of(&paths, json["player1"].as_str().unwrap_or("bf")), icon_of(&paths, json["player2"].as_str().unwrap_or("dad"))];
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
            entry: entry.clone(), json, spans: Vec::new(), song, playing: false, position: 0.0, length, zoom: 2, quant: 3, selected: Vec::new(), boxing: None, clipboard: Vec::new(),
            undo: Vec::new(), redo: Vec::new(), unsaved, leaving: false, said: None, background: picture("menuDesat"), event_icon: picture("editors/eventIcon"), icons,
            rate: 1.0, wheel_snaps: true, vortex: false, hitsounds: [0.0; 3], volumes: [0.6, 1.0, 1.0], muted: [false; 3], heard: 0.0, voice_tracks,
            sounds: [paths.find("sounds/hitsound.ogg"), paths.find("sounds/Metronome_Tick.ogg")],
            ui: Ui::default(), tab: 5, note_types, note_type: 0, events, abouts, event: 0, values: Default::default(), copied: None,
            base: base.to_path_buf(), autosaved: (std::time::Instant::now(), String::new()), waveform: (0, Peaks::default()),
            autosave_minutes: 2.0, backup_limit: 10, prefs: Value::Null, characters: crate::character_editor::character_names(&paths), stages: crate::stage_editor::stage_names(&paths),
            charts: funkin_core::content::songs(&funkin_core::content::weeks(base)).into_iter().filter(|song| song.source == entry.source).collect(),
            floating: None, lit: [0.0; 8], locked_events: false, labels: true, going: None,
            untitled: !entry.chart.is_file(), dialog: None, typed: None, prompt: None, theme: Theme::Default, custom_colors: DEFAULT_COLORS, theming: false,
        };
        let prefs = funkin_core::files::read(&base.join(PREFS));
        editor.autosave_minutes = prefs["autosave"].as_f64().unwrap_or(2.0).clamp(0.0, 60.0);
        editor.backup_limit = prefs["backups"].as_u64().unwrap_or(10).min(50) as usize;
        editor.vortex = prefs["vortex"].as_bool().unwrap_or(false);
        editor.wheel_snaps = prefs["mouseScrollSnap"].as_bool().unwrap_or(true);
        for (index, volume) in editor.hitsounds.iter_mut().enumerate() {
            *volume = prefs["hitsounds"][index].as_f64().unwrap_or(0.0).clamp(0.0, 1.0) as f32;
        }
        editor.theme = THEMES.iter().find(|(_, name, _)| prefs["theme"].as_str() == Some(*name)).map_or(Theme::Default, |(theme, _, _)| *theme);
        let kept = [&prefs["customBgColor"], &prefs["customGridColors"][0], &prefs["customGridColors"][1], &prefs["customNextGridColors"][0], &prefs["customNextGridColors"][1]];
        for (color, kept) in editor.custom_colors.iter_mut().zip(kept) {
            *color = kept.as_str().and_then(hex).unwrap_or(*color);
        }
        editor.prefs = editor.prefs_json();
        editor.autosaved.1 = editor.text();
        editor.measure();
        editor.set_volumes();
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
        self.said = Some((words.into(), std::time::Instant::now()));
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
        // while the system's file dialog is up the editor only waits for it (still drawn as it was)
        if let Some((want, answer)) = self.dialog.as_ref().map(|(want, pending)| (*want, pending.answer())) {
            let Some(answer) = answer else {
                self.ui.hold();
                return Go::Stay;
            };
            self.dialog = None;
            match answer {
                Answer::Chosen(path) => self.chosen(want, path),
                Answer::Cancelled => {}
                Answer::Failed => {
                    self.fall_back(want);
                    self.say("The file dialog did not work: type the file's name instead.");
                }
            }
        }
        // (keys typed into a box are not the editor's: what was asked before this frame's controls)
        let typing = self.ui.typing();
        self.ui.begin();
        // (the menus first: open, they drop down over the boxes under them)
        if let Some(go) = self.menus() {
            return go;
        }
        self.go_to_box();
        self.typed_box();
        self.prompt_box();
        self.theme_box();
        self.panel();
        let prefs = self.prefs_json();
        if prefs != self.prefs {
            if let Err(error) = funkin_core::files::write(&self.base.join(PREFS), &prefs.to_string()) {
                self.say(format!("Could not keep the editor's settings: {error}"));
            }
            self.prefs = prefs;
        }
        let dt = get_frame_time().min(0.1) as f64;
        if self.autosave_minutes > 0.0 && self.autosaved.0.elapsed().as_secs_f64() >= self.autosave_minutes * 60.0 {
            self.autosave();
        }
        let (ctrl, shift) = (held(KeyCode::LeftControl) || held(KeyCode::RightControl), held(KeyCode::LeftShift) || held(KeyCode::RightShift));
        let current = self.section_at(self.position);
        let span = self.spans.get(current).copied().unwrap_or(Span { start: 0.0, beats: 4.0, bpm: 100.0, rows_before: 0.0 });
        // the Vortex Editor's keys work while the music plays
        let digits = [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4, KeyCode::Key5, KeyCode::Key6, KeyCode::Key7, KeyCode::Key8];
        let columns: Vec<usize> = digits.iter().enumerate().filter(|(_, key)| is_key_pressed(**key)).map(|(column, _)| column).collect();
        if self.vortex && !typing && !ctrl && !columns.is_empty() {
            // on the beat snap, counted from the section's start (the Haxe editor's)
            let snap = span.step_ms() * 16.0 / QUANTS[self.quant] as f64;
            let at = round(span.start + ((self.position - span.start) / snap).floor() * snap);
            self.remember();
            let kind = self.note_types.get(self.note_type).filter(|kind| !kind.is_empty()).cloned();
            let notes = &mut self.json["notes"][current]["sectionNotes"];
            if !notes.is_array() {
                *notes = json!([]);
            }
            for column in columns {
                vortex(notes.as_array_mut().expect("made an array"), at, column, kind.as_deref());
            }
            self.selected.clear();
        }
        if typing {
        } else if self.floating.is_some() && (is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace) || is_mouse_right_down()) {
            // pasted ones not put down yet are dropped
            self.floating = None;
        } else if ctrl {
            if self.floating.is_some() {
                // (not while pasted ones follow the mouse)
            } else if is_key_pressed(KeyCode::S) {
                self.save();
            } else if is_key_pressed(KeyCode::C) || is_key_pressed(KeyCode::X) {
                self.copy();
                if is_key_pressed(KeyCode::X) {
                    self.delete_picked();
                }
            } else if is_key_pressed(KeyCode::V) {
                self.paste();
            } else if is_key_pressed(KeyCode::A) {
                self.pick_all();
            } else if is_key_pressed(KeyCode::Z) {
                self.step_history(true);
            } else if is_key_pressed(KeyCode::Y) {
                self.step_history(false);
            }
        } else if is_key_pressed(KeyCode::Enter) {
            return self.play_test();
        } else if is_key_pressed(KeyCode::Escape) {
            if let Go::Leave = self.leave() {
                return Go::Leave;
            }
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
        } else if held(KeyCode::LeftBracket) || held(KeyCode::RightBracket) {
            // the music faster or slower ([ and ]; Alt puts it back)
            let alt = held(KeyCode::LeftAlt) || held(KeyCode::RightAlt);
            let toward = if held(KeyCode::RightBracket) { 1.0 } else { -1.0 };
            self.set_rate(if alt { 1.0 } else { self.rate + toward * dt });
        } else if mouse_wheel() != 0.0 && !self.wheel_snaps {
            self.wind(self.position - mouse_wheel() as f64 * 60000.0 / span.bpm * if shift { 4.0 } else { 1.0 } * 0.25 / ZOOMS[self.zoom] as f64);
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
            self.delete_picked();
        }
        if self.playing {
            self.position = self.song.position_ms();
            if self.position >= self.length {
                self.wind(self.length);
            }
            self.tick_sounds();
        }
        for lit in &mut self.lit {
            *lit = (*lit - dt).max(0.0);
        }
        if self.playing {
            self.light_strums();
        }
        self.heard = self.position;
        // pasted ones follow the mouse over the grid (their first one on its row, on the beat snap unless Shift is held;
        // their columns as many on as the mouse moved), and the click let go of puts them down
        if let (Some((column, at, _)), true) = (self.under_mouse(!shift), self.floating.is_some()) {
            let floating = self.floating.as_mut().expect("checked");
            let to = column.saturating_sub(1);
            if to != floating.mouse {
                floating.column = follow(floating.column, floating.width(), floating.mouse, to);
                floating.mouse = to;
            }
            floating.at = at;
        }
        if self.floating.is_some() {
            if is_mouse_released() {
                self.put_down();
            }
        // Ctrl+drag boxes notes and events in; a click elsewhere puts down, takes off or picks
        } else if is_mouse_pressed() && !self.ui.took_click() {
            match self.under_mouse(false) {
                Some((_, at, _)) if ctrl => self.boxing = Some((mouse_position(vec2(WIDTH, HEIGHT)).x, at)),
                _ => self.click(shift),
            }
        }
        if let Some((x, at)) = self.boxing.filter(|_| !is_mouse_down()) {
            self.boxing = None;
            self.pick_boxed(self.picking_box(x, at));
        }
        Go::Stay
    }

    /// Play-tests the chart as it is (Enter, File > Playtest).
    fn play_test(&mut self) -> Go {
        self.stop();
        Go::Play(self.text())
    }

    /// Leaves the editor (Escape, File > Exit); with changes not saved, only when asked twice.
    fn leave(&mut self) -> Go {
        if !self.unsaved || self.leaving {
            self.stop();
            return Go::Leave;
        }
        self.leaving = true;
        self.say("Unsaved changes: Escape (or Exit) again to leave, Ctrl+S to save");
        Go::Stay
    }

    /// The File, Edit and View menus (the Haxe editor's upper box); what the editor wants next when an entry leaves it.
    fn menus(&mut self) -> Option<Go> {
        let names = |names: &[&str]| names.iter().map(|name| name.to_string()).collect::<Vec<_>>();
        let file = names(&["New", "Open Chart...", "Open Autosave...", "Open Events...", "Save", "Save As...", "Save Events", "Save Events As...", "Reload Chart", "Playtest (Enter)", "Exit"]);
        let lock = if self.locked_events { "Unlock Events" } else { "Lock Events" };
        let edit = names(&["Undo", "Redo", "Select All", lock, "Autosave Settings...", "Clear All Notes", "Clear All Events"]);
        let labels = if self.labels { "Hide Note Labels" } else { "Show Note Labels" };
        let view = names(&[labels, if self.vortex { "Vortex Editor ON" } else { "Vortex Editor OFF" }, "Waveform...", "Go to...", "Theme..."]);
        match self.ui.menus("menus", 40.0, 40.0, 330.0, &[("File", file), ("Edit", edit), ("View", view)])? {
            (0, 0) if !self.ui.sure("new", 0, self.unsaved) => self.say("Unsaved changes: choose New again to start over"),
            (0, 0) => self.new_chart(),
            // through the system's dialog, else the Song tab's list, opened
            (0, 1) => self.ask(Want::OpenChart),
            (0, 2) if self.autosaves().is_empty() => self.say("No autosave files found."),
            (0, 2) => {
                self.tab = 5;
                self.ui.open_list("autosaves", 0);
            }
            (0, 3) => self.ask(Want::OpenEvents),
            (0, 4) => self.save(),
            (0, 5) => self.ask(Want::SaveAs),
            // (a chart whose events are all in the song's events.json has none of its own: nothing is written over that file)
            (0, 6 | 7) if self.json["events"].as_array().is_none_or(|events| events.is_empty()) => self.say("This chart has no events of its own to save."),
            (0, 6) if self.entry.chart.with_file_name("events.json").exists() && !self.ui.sure("save events", 0, true) => self.say("events.json is there already. Choose it again to write over it."),
            (0, 6) => self.save_events(),
            (0, 7) => self.ask(Want::SaveEventsAs),
            (0, 8) if self.untitled => self.say("This chart has no file to reload yet."),
            (0, 8) if !self.ui.sure("reload chart", 0, self.unsaved) => self.say(crate::ui::UNSAVED),
            (0, 8) => {
                if self.open_chart(&self.entry.clone()) {
                    self.say("Chart reloaded successfully!");
                }
            }
            (0, 9) => return Some(self.play_test()),
            (0, 10) => {
                if let Go::Leave = self.leave() {
                    return Some(Go::Leave);
                }
            }
            (1, 0) => self.step_history(true),
            (1, 1) => self.step_history(false),
            (1, 2) => self.pick_all(),
            (1, 3) => {
                self.locked_events = !self.locked_events;
                if self.locked_events {
                    self.selected.retain(|picked| matches!(picked, Picked::Note(..)));
                }
            }
            // (on the Charting tab)
            (1, 4) => self.tab = 0,
            (1, entry @ (5 | 6)) if !self.ui.sure("clear all", entry, true) => self.say("Delete them all? Choose it again to do so."),
            (1, 5) => {
                self.remember();
                for section in self.json["notes"].as_array_mut().into_iter().flatten() {
                    section["sectionNotes"] = json!([]);
                }
                self.selected.clear();
            }
            (1, 6) => {
                self.remember();
                self.json["events"] = json!([]);
                self.selected.clear();
            }
            (2, 0) => self.labels = !self.labels,
            (2, 1) => self.set_vortex(!self.vortex),
            (2, 2) => {
                self.tab = 0;
                self.ui.open_list("waveform", self.waveform.0);
            }
            (2, 3) => {
                self.close_boxes();
                self.going = Some((self.position, self.section_at(self.position)));
            }
            (2, 4) => {
                self.close_boxes();
                self.theming = true;
            }
            _ => {}
        }
        None
    }

    /// The View menu's Go to... box: a time (in seconds) or a section, and Go To.
    fn go_to_box(&mut self) {
        let Some((mut time, mut section)) = self.going else { return };
        let (x, y) = (40.0, 64.0);
        self.ui.area(Rect::new(x, y, 330.0, 120.0));
        self.ui.label(x + 10.0, y + 4.0, "Go to Time/Section:");
        self.ui.label(x + 10.0, y + 30.0, "Time (in seconds):");
        if let Some(seconds) = self.ui.stepper("go to time", x + 10.0, y + 50.0, 80.0, time.floor() / 1000.0, 1.0, 0.0, (self.length / 1000.0 - 0.01).max(0.0)) {
            time = seconds * 1000.0;
            section = self.section_at(time);
        }
        self.ui.label(x + 170.0, y + 30.0, "Section:");
        if let Some(index) = self.ui.stepper("go to section", x + 170.0, y + 50.0, 50.0, section as f64, 1.0, 0.0, self.spans.len().saturating_sub(1) as f64) {
            section = index.round() as usize;
            time = self.spans.get(section).map_or(0.0, |span| span.start) + 0.000001;
        }
        self.ui.label(x + 10.0, y + 74.0, &format!("{} / {}", clock(time), clock(self.length)));
        self.going = Some((time, section));
        if self.ui.button(x + 170.0, y + 92.0, 70.0, "Go To") {
            self.going = None;
            self.wind(time);
        } else if self.ui.button(x + 250.0, y + 92.0, 70.0, "Cancel") {
            self.going = None;
        }
    }

    /// The Vortex Editor on or off, its strums unlit.
    fn set_vortex(&mut self, on: bool) {
        self.vortex = on;
        self.lit = [0.0; 8];
    }

    /// In the Vortex Editor, the strums of the notes passed since the last frame light up, for as long as the note
    /// is held and no less than a step and a quarter (the Haxe editor's confirm; it is not lit by notes put down).
    fn light_strums(&mut self) {
        if !self.vortex {
            return;
        }
        let step = self.spans.get(self.section_at(self.position)).map_or(150.0, Span::step_ms);
        let passed: Vec<(usize, f64)> = self.sections().iter().flat_map(|section| section["sectionNotes"].as_array().into_iter().flatten())
            .filter_map(|note| Some((note[0].as_f64()?, note[1].as_f64()?, note[2].as_f64().unwrap_or(0.0))))
            .filter(|(time, data, _)| self.heard < *time && *time <= self.position && (0.0..8.0).contains(data))
            .map(|(_, data, length)| (data as usize, (step * 1.25).max(length) / 1000.0 / self.rate)).collect();
        for (column, seconds) in passed {
            self.lit[column] = seconds;
        }
    }

    /// The column and moment under the mouse, when it is over the grid: snapped to the beat snap unless Shift is held.
    /// (The events' column is not, while they are locked.)
    fn under_mouse(&self, snapped: bool) -> Option<(usize, f64, f64)> {
        let at = mouse_position(vec2(WIDTH, HEIGHT));
        let column = ((at.x - LEFT) / GRID).floor();
        if !(0.0..COLUMNS as f32).contains(&column) || (self.locked_events && column == 0.0) {
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
            Some((section, index)) if shift => self.toggle(Picked::Note(section, index)),
            Some((section, index)) => {
                self.remember();
                if let Some(notes) = self.json["notes"][section]["sectionNotes"].as_array_mut() {
                    notes.remove(index);
                }
                self.selected.clear();
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
                self.selected = vec![Picked::Note(section, notes.len() - 1)];
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
                self.toggle(Picked::Event(index, 0));
                self.show_event();
            }
            Some(index) => {
                self.remember();
                if let Some(events) = self.json["events"].as_array_mut() {
                    events.remove(index);
                }
                self.selected.clear();
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
                self.selected = vec![Picked::Event(place, 0)];
            }
        }
    }

    /// The Events tab shows the picked event's entry: its name and values.
    fn show_event(&mut self) {
        let Some(Picked::Event(index, entry)) = self.one() else { return };
        let entry = &self.json["events"][index][1][entry];
        let name = entry[0].as_str().unwrap_or_default();
        if let Some(found) = self.events.iter().position(|known| known == name) {
            self.event = found;
        }
        self.values = [plain(&entry[1]), plain(&entry[2])];
    }

    /// The picked notes held longer (or shorter, never below none).
    fn hold_longer(&mut self, by: f64) {
        if !self.selected.iter().any(|picked| matches!(picked, Picked::Note(..))) {
            return;
        }
        self.remember();
        for picked in self.selected.clone() {
            let Picked::Note(section, index) = picked else { continue };
            let note = &mut self.json["notes"][section]["sectionNotes"][index];
            if note.is_array() {
                let length = note[2].as_f64().unwrap_or(0.0);
                *slot(note, 2, &json!(0)) = json!(round((length + by).max(0.0)));
            }
        }
    }

    /// The one thing picked, when only one is (the Note and Events tabs show it).
    fn one(&self) -> Option<Picked> {
        match self.selected[..] {
            [one] => Some(one),
            _ => None,
        }
    }

    /// Picked, or no longer when it was (Shift+click).
    fn toggle(&mut self, picked: Picked) {
        match self.selected.iter().position(|kept| *kept == picked) {
            Some(place) => drop(self.selected.remove(place)),
            None => self.selected.push(picked),
        }
    }

    /// The picked notes and events taken off.
    fn delete_picked(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.remember();
        // the last ones first, so the places of the others stay right
        let mut picked = std::mem::take(&mut self.selected);
        picked.sort_by(|a, b| b.place().cmp(&a.place()));
        picked.dedup_by_key(|picked| picked.place());
        for picked in picked {
            let (list, index) = match picked {
                Picked::Note(section, index) => (&mut self.json["notes"][section]["sectionNotes"], index),
                Picked::Event(index, _) => (&mut self.json["events"], index),
            };
            if let Some(list) = list.as_array_mut().filter(|list| index < list.len()) {
                list.remove(index);
            }
        }
    }

    /// The picked notes and events copied, their times from the first one's.
    fn copy(&mut self) {
        let mut copied: Vec<(bool, Value)> = self.selected.iter().filter_map(|picked| match *picked {
            Picked::Note(section, index) => Some((false, self.json["notes"][section]["sectionNotes"].get(index)?.clone())),
            Picked::Event(index, _) => Some((true, self.json["events"].get(index)?.clone())),
        }).collect();
        let first = copied.iter().filter_map(|(_, thing)| thing[0].as_f64()).fold(f64::INFINITY, f64::min);
        for (_, thing) in &mut copied {
            thing[0] = json!(round(thing[0].as_f64().unwrap_or(first) - first));
        }
        self.say(format!("Copied {}", copied.len()));
        self.clipboard = copied;
    }

    /// What was copied, to follow the mouse until a click puts it down (as the Haxe editor, from the start of the
    /// section playing and the leftmost column until the mouse is over the grid).
    fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let leftmost = self.clipboard.iter().filter(|(event, _)| !event).filter_map(|(_, note)| note[1].as_f64()).fold(f64::INFINITY, f64::min);
        let leftmost = if leftmost.is_finite() { leftmost.max(0.0) as usize } else { 0 };
        let things = self.clipboard.iter().cloned().map(|(event, mut thing)| {
            if !event {
                thing[1] = json!((thing[1].as_f64().unwrap_or(0.0) as usize).saturating_sub(leftmost));
            }
            (event, thing)
        }).collect();
        let at = self.spans.get(self.section_at(self.position)).map_or(0.0, |span| span.start);
        self.selected.clear();
        // (the mouse is taken to be over the column they were copied from: the Haxe editor's)
        self.floating = Some(Floating { things, at, column: 0, mouse: leftmost });
    }

    /// The pasted notes and events put down where they are, and picked.
    fn put_down(&mut self) {
        let Some(floating) = self.floating.take() else { return };
        self.remember();
        self.selected.clear();
        for (event, mut thing) in floating.things {
            let time = round(thing[0].as_f64().unwrap_or(0.0) + floating.at);
            thing[0] = json!(time);
            if event {
                if !self.json["events"].is_array() {
                    self.json["events"] = json!([]);
                }
                let events = self.json["events"].as_array_mut().expect("made an array");
                let place = events.iter().position(|kept| kept[0].as_f64().is_some_and(|kept| kept > time)).unwrap_or(events.len());
                // (the events after it move one place on)
                for picked in &mut self.selected {
                    if let Picked::Event(index, _) = picked {
                        *index += (*index >= place) as usize;
                    }
                }
                events.insert(place, thing);
                self.selected.push(Picked::Event(place, 0));
            } else {
                thing[1] = json!(thing[1].as_u64().unwrap_or(0) as usize + floating.column);
                let section = self.section_at(time);
                let notes = &mut self.json["notes"][section]["sectionNotes"];
                if !notes.is_array() {
                    *notes = json!([]);
                }
                let notes = notes.as_array_mut().expect("made an array");
                notes.push(thing);
                self.selected.push(Picked::Note(section, notes.len() - 1));
            }
        }
    }

    /// Every note and event of the section playing picked (Ctrl+A).
    fn pick_all(&mut self) {
        let span = self.spans.get(self.section_at(self.position)).copied();
        let (from, until) = span.map_or((0.0, 0.0), |span| (span.start, span.start + span.beats * 60000.0 / span.bpm));
        self.pick_boxed((0.0..COLUMNS as f32, from..until));
    }

    /// The columns and moments a Ctrl+drag from (`x`, `at`) to the mouse covers.
    fn picking_box(&self, x: f32, at: f64) -> (std::ops::Range<f32>, std::ops::Range<f64>) {
        let mouse = mouse_position(vec2(WIDTH, HEIGHT));
        let now = self.ms_of(self.row_of(self.position) + ((mouse.y - HEIGHT / 2.0) / (GRID * ZOOMS[self.zoom])) as f64);
        let column = |x: f32| (x - LEFT) / GRID;
        // (a note is boxed in when its drawn square is: it reaches a cell below its time)
        let top = self.ms_of((self.row_of(at.min(now)) - 1.0 / ZOOMS[self.zoom] as f64).max(0.0));
        (column(x.min(mouse.x)).floor()..column(x.max(mouse.x)).ceil(), top..at.max(now))
    }

    /// The notes and events in some columns between two moments picked.
    fn pick_boxed(&mut self, (columns, times): (std::ops::Range<f32>, std::ops::Range<f64>)) {
        let within = |column: f32, time: Option<f64>| columns.contains(&column) && time.is_some_and(|time| times.contains(&time));
        let notes = self.sections().iter().enumerate().flat_map(|(section, value)| {
            value["sectionNotes"].as_array().into_iter().flatten().enumerate()
                .filter(|(_, note)| note[1].as_f64().is_some_and(|data| within(data as f32 + 1.0, note[0].as_f64())))
                .map(move |(index, _)| Picked::Note(section, index))
        });
        let events = self.json["events"].as_array().into_iter().flatten().enumerate().filter(|(_, event)| within(0.0, event[0].as_f64())).map(|(index, _)| Picked::Event(index, 0));
        self.selected = notes.chain(events).collect();
        if self.selected.len() == 1 {
            self.show_event();
        }
    }

    fn step_history(&mut self, back: bool) {
        // (not while pasted ones follow the mouse)
        if self.floating.is_some() {
            return;
        }
        let (from, to) = if back { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        let Some(chart) = from.pop() else { return };
        to.push(std::mem::replace(&mut self.json, chart));
        (self.unsaved, self.leaving) = (true, false);
        self.selected.clear();
        self.measure();
        self.say(if back { "Undone" } else { "Redone" });
    }

    /// Over the chart's file, the one before kept as `<file>.bak`.
    fn save(&mut self) {
        if self.untitled {
            return self.ask(Want::SaveAs);
        }
        let done = funkin_core::files::replace(&self.entry.chart, &self.text());
        self.unsaved &= done.is_err();
        self.say(done.unwrap_or_else(|error| error));
    }

    /// The chart's events into the song's events.json, beside the chart (the game adds them to every difficulty's), the
    /// one before kept as `.bak`.
    fn save_events(&mut self) {
        let done = funkin_core::files::replace(&self.entry.chart.with_file_name("events.json"), &events_text(&self.json));
        self.say(done.unwrap_or_else(|error| error));
    }

    /// Another chart in this editor (or this one again, as its file holds it); whether it could be read.
    fn open_chart(&mut self, entry: &SongEntry) -> bool {
        let base = self.base.clone();
        match ChartEditor::open(&base, entry, None) {
            Ok(editor) => {
                self.stop();
                *self = editor;
                true
            }
            Err(error) => {
                self.say(error);
                false
            }
        }
    }

    /// File > New: an empty chart (the Haxe editor's openNewChart) over the song's music, with no file of its own yet.
    fn new_chart(&mut self) {
        self.stop();
        let mut json = new_chart();
        fill_sections(&mut json, self.length);
        self.json = json;
        let folder = self.entry.chart.parent().map(Path::to_path_buf).unwrap_or_default();
        self.entry.chart = (1..).map(|count| folder.join(if count == 1 { "untitled.json".to_owned() } else { format!("untitled-{count}.json") }))
            .find(|path| !path.exists()).expect("some name is free");
        let paths = Paths::new(&self.base, &self.entry.source, "");
        self.icons = [icon_of(&paths, "bf"), icon_of(&paths, "dad")];
        (self.untitled, self.unsaved, self.leaving) = (true, false, false);
        self.undo.clear();
        self.redo.clear();
        self.selected.clear();
        self.floating = None;
        self.measure();
        self.wind(0.0);
        self.say("New chart: Save asks where it goes");
    }

    /// Closes the boxes over the grid (one is open at a time).
    fn close_boxes(&mut self) {
        (self.going, self.typed, self.prompt, self.theming) = (None, None, None, false);
    }

    /// The name a file dialog (or the typed box) offers.
    fn offered_name(&self, want: Want) -> String {
        match want {
            Want::SaveAs if self.untitled => format!("{}.json", song_path(self.json["song"].as_str().unwrap_or("Test"))),
            Want::SaveAs | Want::OpenChart => self.entry.chart.file_name().map_or(String::new(), |name| name.to_string_lossy().into_owned()),
            Want::OpenEvents | Want::SaveEventsAs => "events.json".to_owned(),
        }
    }

    /// Asks for a file through the system's dialog, from the chart's folder; with none, the editor's own way.
    fn ask(&mut self, want: Want) {
        self.stop();
        self.close_boxes();
        let folder = self.entry.chart.parent().map(|folder| std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf())).unwrap_or_default();
        let pending = match want {
            Want::OpenChart | Want::OpenEvents => dialog::open(want.title(), &["json"], &folder),
            Want::SaveAs | Want::SaveEventsAs => dialog::save(want.title(), &self.offered_name(want), &folder),
        };
        match pending {
            Some(pending) => {
                self.dialog = Some((want, pending));
                self.say("Choose the file in the window that opened");
            }
            None => self.fall_back(want),
        }
    }

    /// With no file dialog: the Song tab's list of the folder's charts, else a box to type the file's name in.
    fn fall_back(&mut self, want: Want) {
        if want == Want::OpenChart {
            self.tab = 5;
            let open = self.charts.iter().position(|song| song.chart == self.entry.chart).unwrap_or(0);
            self.ui.open_list("open chart", open);
            self.say("No file dialog here: choose the chart in the list");
        } else {
            self.typed = Some((want, self.offered_name(want)));
        }
    }

    /// What is done with the file chosen, as the Haxe editor's File entries.
    fn chosen(&mut self, want: Want, path: PathBuf) {
        let read = || std::fs::read_to_string(&path).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .map(|mut json| if json["song"].is_object() { json["song"].take() } else { json });
        match want {
            Want::OpenChart if read().is_none_or(|song| song.get("song").is_none()) => self.say("Error: File loaded is not a Psych Engine/FNF 0.2.x.x chart."),
            Want::OpenChart if self.unsaved => self.prompt = Some(Prompt::OpenChart(path)),
            Want::OpenChart => self.open_chart_file(&path),
            Want::OpenEvents => {
                let Some(mut file) = read().filter(|file| file.is_object() && file.get("scrollSpeed").is_none()) else {
                    return self.say("Error: File loaded is not a Psych Engine chart/events file.");
                };
                convert(&mut file);
                match file["events"].as_array() {
                    None => self.say("Error: File loaded is not a Psych Engine chart/events file."),
                    Some(events) if events.is_empty() => self.say("Events file loaded is empty."),
                    Some(events) => self.prompt = Some(Prompt::Events(events.clone())),
                }
            }
            Want::SaveAs => match funkin_core::files::replace(&path, &self.text()) {
                Ok(_) => {
                    self.say(format!("Chart saved successfully to: {}", path.display()));
                    self.entry.chart = path;
                    (self.untitled, self.unsaved, self.leaving) = (false, false, false);
                }
                Err(error) => self.say(error),
            },
            Want::SaveEventsAs => match funkin_core::files::replace(&path, &events_text(&self.json)) {
                Ok(_) => self.say(format!("Events saved successfully to: {}", path.display())),
                Err(error) => self.say(error),
            },
        }
    }

    /// A chart file in this editor: as the folder's chart it is, else with this song's music.
    fn open_chart_file(&mut self, path: &Path) {
        let real = std::fs::canonicalize(path).ok();
        let mut entry = self.charts.iter().find(|song| std::fs::canonicalize(&song.chart).ok() == real).cloned().unwrap_or_else(|| self.entry.clone());
        entry.chart = path.to_path_buf();
        if self.open_chart(&entry) {
            self.say(format!("Opened chart \"{}\" successfully!", path.display()));
        }
    }

    /// With no file dialog: the box the file's name is typed in (it goes beside the chart).
    fn typed_box(&mut self) {
        let Some((want, mut name)) = self.typed.take() else { return };
        let (x, y) = (40.0, 64.0);
        self.ui.area(Rect::new(x, y, 330.0, 80.0));
        self.ui.label(x + 10.0, y + 4.0, &format!("{} (a file beside the chart):", want.title()));
        if let Some(typed) = self.ui.input("typed name", x + 10.0, y + 28.0, 310.0, &name) {
            name = typed;
        }
        let verb = if matches!(want, Want::OpenChart | Want::OpenEvents) { "Open" } else { "Save" };
        if self.ui.button(x + 170.0, y + 54.0, 70.0, verb) {
            match beside(&self.entry.chart, &name) {
                Some(path) if verb == "Open" && !path.is_file() => {
                    self.say(format!("There is no {} beside the chart.", name.trim()));
                    self.typed = Some((want, name));
                }
                Some(path) => self.chosen(want, path),
                None => {
                    self.say("Type a file's name only: it goes beside the chart.");
                    self.typed = Some((want, name));
                }
            }
        } else if !self.ui.button(x + 250.0, y + 54.0, 70.0, "Cancel") {
            self.typed = Some((want, name));
        }
    }

    /// The question about a file chosen: open it over changes not saved; the events found in one put in place of the
    /// chart's, or added to them (a step to undo).
    fn prompt_box(&mut self) {
        let Some(prompt) = self.prompt.take() else { return };
        let (x, y) = (40.0, 64.0);
        self.ui.area(Rect::new(x, y, 330.0, 80.0));
        match prompt {
            Prompt::OpenChart(path) => {
                self.ui.label(x + 10.0, y + 4.0, "Warning: Any unsaved progress\nwill be lost.");
                if self.ui.button(x + 170.0, y + 54.0, 70.0, "Open") {
                    self.open_chart_file(&path);
                } else if !self.ui.button(x + 250.0, y + 54.0, 70.0, "Cancel") {
                    self.prompt = Some(Prompt::OpenChart(path));
                }
            }
            Prompt::Events(events) => {
                self.ui.label(x + 10.0, y + 4.0, &format!("Events Found! ({}) Choose an action.", events.len()));
                if self.ui.button(x + 10.0, y + 54.0, 100.0, "Replace All") {
                    self.remember();
                    self.json["events"] = json!(events);
                    self.selected.retain(|picked| matches!(picked, Picked::Note(..)));
                    self.say("Events loaded successfully!");
                } else if self.ui.button(x + 120.0, y + 54.0, 90.0, "Add") {
                    self.remember();
                    if !self.json["events"].is_array() {
                        self.json["events"] = json!([]);
                    }
                    self.json["events"].as_array_mut().expect("made an array").extend(events);
                    self.say("Events added successfully!");
                } else if !self.ui.button(x + 220.0, y + 54.0, 100.0, "Cancel") {
                    self.prompt = Some(Prompt::Events(events));
                }
            }
        }
    }

    /// The View menu's Theme... box: a theme's button, and the Custom one's colours (typed in hexadecimal).
    fn theme_box(&mut self) {
        if !self.theming {
            return;
        }
        let (x, y) = (40.0, 64.0);
        self.ui.area(Rect::new(x, y, 400.0, 150.0));
        self.ui.label(x + 10.0, y + 4.0, "Chart Editor Theme:");
        if self.ui.button(x + 370.0, y + 4.0, 24.0, "X") {
            self.theming = false;
        }
        for (index, (theme, _, words)) in THEMES.iter().enumerate() {
            if self.ui.button(x + 10.0 + index as f32 * 77.0, y + 30.0, 72.0, words) {
                self.theme = *theme;
            }
        }
        self.ui.label(x + 10.0, y + 62.0, "BG Color:");
        self.ui.label(x + 115.0, y + 62.0, "Grid Colors:");
        self.ui.label(x + 220.0, y + 62.0, "Next Grid Colors:");
        const BOXES: [(&str, f32, f32); 5] = [("theme bg", 10.0, 82.0), ("theme grid", 115.0, 82.0), ("theme grid 2", 115.0, 108.0), ("theme next", 220.0, 82.0), ("theme next 2", 220.0, 108.0)];
        for (index, (id, left, top)) in BOXES.into_iter().enumerate() {
            if let Some(color) = self.ui.input(id, x + left, y + top, 80.0, &format!("{:06X}", self.custom_colors[index])).as_deref().and_then(hex) {
                self.custom_colors[index] = color;
                self.theme = Theme::Custom;
            }
        }
    }

    /// The chosen track's loudness down the grid, a line of each screen row: out from the middle stripe by how far
    /// its left and right channels swing (the Haxe editor's waveform, in its blue).
    fn draw_waveform(&self, row_height: f32) {
        if self.waveform.0 == 0 || self.waveform.1.peaks.is_empty() {
            return;
        }
        let (middle, half) = (LEFT + 5.0 * GRID, 4.0 * GRID);
        let at = |y: f32| self.ms_of(self.row_of(self.position) + ((y - HEIGHT / 2.0) / row_height) as f64);
        let top = self.y_of(0.0).max(0.0) as usize;
        for y in top..HEIGHT as usize {
            let [low, high] = self.waveform.1.between(at(y as f32), at(y as f32 + 1.0));
            // (as the Haxe editor: a full swing reaches a quarter of the grid each way and no further)
            let reach = |swing: f32| (swing * 2.0 * half / 1.12).min(half) / 2.0;
            let (left, right) = (reach(low), reach(high));
            if left + right >= 0.5 {
                draw_rectangle(middle - left, y as f32, left + right, 1.0, Color::from_hex(0x0000FF));
            }
        }
    }

    fn prefs_json(&self) -> Value {
        let [bg, grid, grid_2, next, next_2] = self.custom_colors.map(|color| format!("{color:06X}"));
        let theme = THEMES.iter().find(|(theme, _, _)| *theme == self.theme).map_or("default", |(_, name, _)| *name);
        json!({"autosave": self.autosave_minutes, "backups": self.backup_limit, "vortex": self.vortex, "mouseScrollSnap": self.wheel_snaps, "hitsounds": self.hitsounds,
            "theme": theme, "customBgColor": bg, "customGridColors": [grid, grid_2], "customNextGridColors": [next, next_2]})
    }

    fn backups(&self) -> PathBuf {
        self.base.join("backups")
    }

    /// The chart's file name without its ending, which its autosaves start with.
    fn chart_name(&self) -> String {
        self.entry.chart.file_stem().map_or("unknown".to_owned(), |stem| stem.to_string_lossy().into_owned())
    }

    /// The chart into backups/, with the file it is of; the oldest autosaves past the tenth taken off.
    fn autosave(&mut self) {
        let text = self.text();
        self.autosaved.0 = std::time::Instant::now();
        if text == self.autosaved.1 {
            return;
        }
        let [year, month, day, _, hour, minute, second] = crate::menu::local_time();
        let name = format!("{}_{year}-{month:02}-{day:02}_{hour:02}-{minute:02}-{second:02}.bkp", self.chart_name());
        let mut copy = self.json.clone();
        copy["__original_path"] = json!(self.entry.chart.to_string_lossy());
        let folder = self.backups();
        let done = std::fs::create_dir_all(&folder).and_then(|_| funkin_core::files::write(&folder.join(&name), &print(&copy, 0)));
        match done {
            Ok(()) => {
                self.autosaved.1 = text;
                let names: Vec<String> = std::fs::read_dir(&folder).into_iter().flatten().flatten().map(|file| file.file_name().to_string_lossy().into_owned()).collect();
                for old in past_limit(&names, self.backup_limit).into_iter().filter(|_| self.backup_limit > 0) {
                    let _ = std::fs::remove_file(folder.join(old));
                }
                self.say(format!("Autosaved {name}"));
            }
            Err(error) => self.say(format!("Could not autosave: {error}")),
        }
    }

    /// The chart's autosaves, the newest first.
    fn autosaves(&self) -> Vec<String> {
        let start = format!("{}_", self.chart_name());
        let mut names: Vec<String> = std::fs::read_dir(self.backups()).into_iter().flatten().flatten().map(|file| file.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(&start) && name.ends_with(".bkp")).collect();
        names.sort_by(|a, b| b.cmp(a));
        names
    }

    /// An autosave of this chart in its place (a step to undo).
    fn load_autosave(&mut self, name: &str) {
        let read = std::fs::read_to_string(self.backups().join(name)).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok());
        let Some(mut json) = read.filter(|json| json["__original_path"].is_string()) else {
            return self.say("Error: File loaded is not a valid Psych Engine autosave.");
        };
        // (another song's chart may have the same file name: it is not put over this one)
        if json["__original_path"].as_str().map(Path::new) != Some(self.entry.chart.as_path()) {
            return self.say(format!("{name} is an autosave of {}", json["__original_path"].as_str().unwrap_or_default()));
        }
        json.as_object_mut().map(|fields| fields.remove("__original_path"));
        convert(&mut json);
        fill_sections(&mut json, self.length);
        self.remember();
        self.json = json;
        self.selected.clear();
        self.measure();
        self.say(format!("Opened autosave \"{name}\" successfully!"));
    }

    pub fn draw(&self, skin: &Skin) {
        let colors = self.theme.colors(self.custom_colors).map(Color::from_hex);
        clear_background(colors[0]);
        set_camera(&Camera2D { target: vec2(WIDTH / 2.0, HEIGHT / 2.0), zoom: vec2(2.0 / WIDTH, 2.0 / HEIGHT), ..Default::default() });
        if let Some(background) = &self.background {
            draw_texture_ex(background, 0.0, 0.0, colors[0], DrawTextureParams { dest_size: Some(vec2(WIDTH, HEIGHT)), ..Default::default() });
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
            let colors = if index == current { [colors[1], colors[2]] } else { [colors[3], colors[4]] };
            for row in 0..span.rows().ceil() as usize {
                let y = top + row as f32 * row_height;
                if y + row_height < 0.0 || y > HEIGHT {
                    continue;
                }
                for column in 0..COLUMNS {
                    draw_rectangle(LEFT + column as f32 * GRID, y, GRID, row_height.min(bottom - y), colors[(row + column) % 2]);
                }
                // the Vortex Editor's line on each beat
                if self.vortex && row % 4 == 0 {
                    draw_rectangle(LEFT + GRID, y - 1.0, GRID * (COLUMNS - 1) as f32, 2.0, Color::new(0.0, 0.0, 0.0, 0.6));
                }
            }
            // the stripes between the events and each side
            for column in [1.0, 5.0] {
                draw_rectangle(LEFT + column * GRID - 1.0, top.max(0.0), 2.0, bottom.min(HEIGHT) - top.max(0.0), BLACK);
            }
        }
        self.draw_waveform(row_height);
        // the Vortex Editor's strums in the row under the line, faded, lit as notes pass them
        if self.vortex {
            for (column, lit) in self.lit.iter().enumerate() {
                let (lane, at) = (column % 4, vec2(LEFT + (column as f32 + 1.0) * GRID + GRID / 2.0, HEIGHT / 2.0 + GRID / 2.0));
                if *lit > 0.0 {
                    skin.fitted(&format!("{} confirm0003", crate::DIRECTIONS[lane]), lane, at, GRID, 0.4);
                } else {
                    let name = format!("arrow{}0000", crate::DIRECTIONS[lane].to_uppercase());
                    let size = skin.frames.iter().find(|(known, _)| *known == name).map(|(_, frame)| vec2(frame.source.w, frame.source.h));
                    skin.frame(&name, at.x, at.y, size.map(|size| size * GRID / size.x), None, 0.4);
                }
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
                if self.selected.contains(&Picked::Note(index, place)) {
                    draw_rectangle(x - GRID / 2.0, y, GRID, GRID, Color { a: 0.5, ..Color::from_hex(0x00FFFF) });
                }
                skin.frame(&format!("{}0000", NOTE_NAMES[lane]), x, y + GRID / 2.0, Some(vec2(GRID, GRID)), Some(lane), alpha);
                // the note type's number in the list (? for one not in it)
                if let Some(kind) = note[3].as_str().filter(|kind| self.labels && !kind.is_empty()) {
                    let label = self.note_types.iter().position(|known| known == kind).map_or("?".to_owned(), |index| index.to_string());
                    let width = measure_text(&label, skin.font.as_ref(), 16, 1.0).width;
                    for (shift, color) in [(2.0, BLACK), (0.0, WHITE)] {
                        draw_text_ex(&label, x - width / 2.0 + shift, y + GRID / 2.0 + 6.0 + shift, TextParams { font: skin.font.as_ref(), font_size: 16, color, ..Default::default() });
                    }
                }
            }
        }
        // the events: an icon each moment that has some, with how many, and what they are written beside the grid
        for (index, event) in self.json["events"].as_array().into_iter().flatten().enumerate() {
            let Some(time) = event[0].as_f64() else { continue };
            let y = self.y_of(time);
            if !(-GRID..HEIGHT).contains(&y) {
                continue;
            }
            if self.selected.iter().any(|picked| matches!(picked, Picked::Event(picked, _) if *picked == index)) {
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
        if self.locked_events {
            draw_rectangle(LEFT, 0.0, GRID, HEIGHT, Color { a: 0.6, ..BLACK });
        }
        // pasted ones following the mouse
        if let Some(floating) = &self.floating {
            for (event, thing) in &floating.things {
                let y = self.y_of(floating.at + thing[0].as_f64().unwrap_or(0.0));
                let column = if *event { 0 } else { floating.column + thing[1].as_u64().unwrap_or(0) as usize + 1 };
                draw_rectangle(LEFT + column as f32 * GRID, y, GRID, GRID, Color { a: 0.5, ..Color::from_hex(0x00FFFF) });
                if !event {
                    let (lane, x) = ((column - 1) % 4, LEFT + column as f32 * GRID + GRID / 2.0);
                    // (its hold's tail too, as the notes put down have)
                    let end = self.y_of(floating.at + thing[0].as_f64().unwrap_or(0.0) + thing[2].as_f64().unwrap_or(0.0));
                    if end > y + GRID / 2.0 {
                        skin.frame(&format!("{} hold piece0000", NOTE_NAMES[lane]), x, (y + GRID / 2.0 + end) / 2.0, Some(vec2(8.0, end - y - GRID / 2.0)), Some(lane), 1.0);
                    }
                    skin.frame(&format!("{}0000", NOTE_NAMES[lane]), x, y + GRID / 2.0, Some(vec2(GRID, GRID)), Some(lane), 1.0);
                } else if let Some(icon) = &self.event_icon {
                    draw_texture_ex(icon, LEFT + 5.0, y + 5.0, WHITE, DrawTextureParams { dest_size: Some(vec2(30.0, 30.0)), ..Default::default() });
                }
            }
        }
        // where the mouse would put a note
        if let Some((column, from, _)) = self.under_mouse(!(held(KeyCode::LeftShift) || held(KeyCode::RightShift))) {
            draw_rectangle(LEFT + column as f32 * GRID, self.y_of(from), GRID, GRID, Color { a: 0.5, ..WHITE });
        }
        draw_rectangle(LEFT, HEIGHT / 2.0 - 2.0, GRID * COLUMNS as f32, 4.0, WHITE);
        // the Ctrl+drag box
        if let Some((x, at)) = self.boxing {
            let (mouse, top) = (mouse_position(vec2(WIDTH, HEIGHT)), self.y_of(at));
            blend_add(true);
            draw_rectangle(x.min(mouse.x), top.min(mouse.y), (x - mouse.x).abs(), (top - mouse.y).abs(), Color { a: 0.4, ..Color::from_hex(0x00FFFF) });
            blend_add(false);
        }
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

    /// The hitsounds of the notes passed since the last frame (one a side at most) and the metronome's beats.
    fn tick_sounds(&self) {
        if self.position <= self.heard {
            return;
        }
        let passed = |time: f64| self.heard < time && time <= self.position;
        let mut sides = [false; 2];
        for note in self.sections().iter().flat_map(|section| section["sectionNotes"].as_array().into_iter().flatten()) {
            if let (Some(time), Some(data)) = (note[0].as_f64(), note[1].as_f64()) {
                if passed(time) && (0.0..8.0).contains(&data) {
                    sides[(data >= 4.0) as usize] = true;
                }
            }
        }
        let beat = |ms: f64| (self.row_of(ms) / 4.0).floor();
        let ticked = beat(self.position) > beat(self.heard);
        for (sounded, (volume, sound)) in sides.into_iter().chain([ticked]).zip(self.hitsounds.iter().zip([&self.sounds[0], &self.sounds[0], &self.sounds[1]])) {
            if let (true, Some(sound)) = (sounded && *volume > 0.0, sound) {
                self.song.play_sound(sound, *volume, false);
            }
        }
    }

    fn set_rate(&mut self, rate: f64) {
        self.rate = (rate * 100.0).round().clamp(10.0, 500.0) / 100.0;
        self.song.set_speed(self.rate);
    }

    /// Each track as loud as the Charting tab says (muted ones silent).
    fn set_volumes(&self) {
        for (track, (volume, muted)) in [Some(0), self.voice_tracks[0], self.voice_tracks[1]].into_iter().zip(self.volumes.iter().zip(self.muted)) {
            if let Some(track) = track {
                self.song.set_volume(track, if muted { 0.0 } else { *volume });
            }
        }
    }

    fn charting_tab(&mut self, x: f32, y: f32) {
        self.ui.label(x, y, "Any options here won't actually\naffect gameplay!");
        self.ui.label(x, y + 38.0, "Playback Rate:");
        if let Some(rate) = self.ui.stepper("rate", x + 130.0, y + 38.0, 60.0, self.rate, 0.1, 0.1, 5.0) {
            self.set_rate(rate);
        }
        // (the list first: open, it drops down over the controls under it)
        self.ui.label(x + 238.0, y + 38.0, "Waveform:");
        let tracks = ["None", "Instrumental", "Main Vocals", "Opp. Vocals"].map(str::to_owned);
        if let Some(track) = self.ui.dropdown("waveform", x + 238.0, y + 56.0, 102.0, &tracks, self.waveform.0) {
            let path = [None, Some(&self.entry.inst), self.entry.voices_player.as_ref(), self.entry.voices_opponent.as_ref()][track].cloned();
            self.waveform = (track, path.and_then(|path| audio::samples(&path)).map_or_else(Peaks::default, |(samples, rate)| Peaks::of(&samples, rate)));
        }
        if let Some(on) = self.ui.check(x + 258.0, y + 228.0, "Vortex", self.vortex) {
            self.set_vortex(on);
        }
        self.ui.label(x, y + 258.0, "Autosave (min):");
        if let Some(minutes) = self.ui.stepper("autosave minutes", x + 125.0, y + 256.0, 36.0, self.autosave_minutes, 1.0, 0.0, 60.0) {
            self.autosave_minutes = minutes.round();
        }
        self.ui.label(x + 212.0, y + 258.0, "Backups:");
        if let Some(count) = self.ui.stepper("backup limit", x + 274.0, y + 256.0, 30.0, self.backup_limit as f64, 1.0, 0.0, 50.0) {
            self.backup_limit = count.round() as usize;
        }
        if let Some(snaps) = self.ui.check(x, y + 66.0, "Mouse Scroll Snap", self.wheel_snaps) {
            self.wheel_snaps = snaps;
        }
        for (index, words) in ["Hitsound\n(Player):", "Hitsound\n(Opp.):", "Metronome:"].into_iter().enumerate() {
            let x = x + 95.0 * index as f32;
            self.ui.label(x, y + 96.0, words);
            if let Some(volume) = self.ui.stepper(["hit player", "hit opponent", "metronome"][index], x, y + 132.0, 36.0, self.hitsounds[index] as f64, 0.2, 0.0, 1.0) {
                self.hitsounds[index] = volume as f32;
            }
        }
        for (index, words) in ["Inst.\nVolume:", "Main\nVocals:", "Opp.\nVocals:"].into_iter().enumerate() {
            let x = x + 95.0 * index as f32;
            self.ui.label(x, y + 166.0, words);
            let volume = self.ui.stepper(["inst volume", "player volume", "opponent volume"][index], x, y + 202.0, 36.0, self.volumes[index] as f64, 0.1, 0.0, 1.0);
            let muted = self.ui.check(x, y + 228.0, "Mute", self.muted[index]);
            if volume.is_some() || muted.is_some() {
                self.volumes[index] = volume.map_or(self.volumes[index], |volume| volume as f32);
                self.muted[index] = muted.unwrap_or(self.muted[index]);
                self.set_volumes();
            }
        }
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

    /// The box at the right with the Haxe editor's tabs.
    fn panel(&mut self) {
        let area = Rect::new(900.0, 60.0, 360.0, 290.0);
        self.tab = self.ui.tabs(area, &["Charting", "Data", "Events", "Note", "Section", "Song"], self.tab);
        let (x, y) = (area.x + 10.0, area.y + 10.0);
        match self.tab {
            0 => self.charting_tab(x, y),
            1 => self.data_tab(x, y),
            2 => self.events_tab(x, y),
            3 => self.note_tab(x, y),
            4 => self.section_tab(x, y),
            _ => self.song_tab(x, y),
        }
    }

    /// The song's game over character, sounds and music, and its notes' and splashes' looks; a text left empty takes
    /// its field off (the game's own is used).
    fn data_tab(&mut self, x: f32, y: f32) {
        let fields = [
            ("gameOverChar", "Game Over Character:"), ("gameOverSound", "Game Over Death Sound (sounds/):"),
            ("gameOverLoop", "Game Over Loop Music (music/):"), ("gameOverEnd", "Game Over Retry Music (music/):"),
        ];
        for (index, (field, words)) in fields.into_iter().enumerate() {
            let top = y + 40.0 * index as f32;
            self.ui.label(x, top, words);
            if let Some(text) = self.ui.input(field, x, top + 16.0, 150.0, self.json[field].as_str().unwrap_or_default()) {
                self.remember();
                if text.trim().is_empty() {
                    self.json.as_object_mut().map(|fields| fields.remove(field));
                } else {
                    self.json[field] = json!(text);
                }
            }
        }
        if let Some(off) = self.ui.check(x + 180.0, y + 16.0, "Disable Note RGB", self.json["disableNoteRGB"].as_bool().unwrap_or(false)) {
            self.set("disableNoteRGB", json!(off));
        }
        for (index, (field, words)) in [("arrowSkin", "Note Texture:"), ("splashSkin", "Note Splashes Texture:")].into_iter().enumerate() {
            let x = x + 180.0 * index as f32;
            self.ui.label(x, y + 166.0, words);
            if let Some(text) = self.ui.input(field, x, y + 182.0, 150.0, self.json[field].as_str().unwrap_or_default()) {
                self.set(field, if text.trim().is_empty() { Value::Null } else { json!(text.trim()) });
            }
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
        // lists of the characters and stages there are (one the chart names that is not there among them); the upper row
        // first, its lists dropping down over the lower one
        for (index, (field, words)) in [("player1", "Player:"), ("player2", "Opponent:"), ("gfVersion", "Girlfriend:"), ("stage", "Stage:")].into_iter().enumerate() {
            let (x, y) = (x + 150.0 * (index % 2) as f32, y + 96.0 + 44.0 * (index / 2) as f32);
            self.ui.label(x, y, words);
            let now = text(&self.json, field);
            let mut names = if field == "stage" { self.stages.clone() } else { self.characters.clone() };
            if !names.contains(&now) {
                names.insert(0, now.clone());
            }
            let chosen = names.iter().position(|name| *name == now).unwrap_or(0);
            if let Some(picked) = self.ui.dropdown(["song player", "song opponent", "song gf", "song stage"][index], x, y + 18.0, 130.0, &names, chosen) {
                self.set(field, json!(names[picked]));
                if index < 2 {
                    self.icons[index] = icon_of(&Paths::new(&self.base, &self.entry.source, ""), &names[picked]);
                }
            }
        }
        let autosaves = self.autosaves();
        // (shown as "11-12 02:41:20", the year left out to fit)
        let dates: Vec<String> = autosaves.iter().map(|name| name.get(self.chart_name().len() + 6..name.len() - 4).unwrap_or(name).replacen('_', " ", 1).replace('-', ":").replacen(':', "-", 1)).collect();
        self.ui.label(x + 130.0, y + 184.0, "Load Autosave:");
        if let Some(index) = self.ui.dropdown("autosaves", x + 130.0, y + 200.0, 150.0, &dates, usize::MAX) {
            if self.ui.sure("autosaves", index, self.unsaved) { self.load_autosave(&autosaves[index]) } else { self.say(crate::ui::UNSAVED) }
        }
        // another chart of the folder, in this editor (the Haxe editor's File > Open Chart)
        let charts: Vec<String> = self.charts.iter().map(|song| format!("{} ({})", song.title, song.difficulty)).collect();
        let open = self.charts.iter().position(|song| song.chart == self.entry.chart).unwrap_or(usize::MAX);
        self.ui.label(x, y + 228.0, "Open Chart:");
        if let Some(index) = self.ui.dropdown("open chart", x + 90.0, y + 228.0, 190.0, &charts, open) {
            if !self.ui.sure("open chart", index, self.unsaved) {
                self.say(crate::ui::UNSAVED);
            } else {
                self.open_chart(&self.charts[index].clone());
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
            self.selected.clear();
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
        let picked = match self.one() {
            Some(Picked::Note(section, index)) if self.json["notes"][section]["sectionNotes"][index].is_array() => Some((section, index)),
            _ => None,
        };
        let step = self.spans.get(self.section_at(self.position)).map_or(150.0, |span| span.step_ms());
        if let Some((section, index)) = picked {
            let note = self.json["notes"][section]["sectionNotes"][index].clone();
            self.ui.label(x, y, "Sustain length:");
            if let Some(length) = self.ui.stepper("sustain", x, y + 18.0, 80.0, note[2].as_f64().unwrap_or(0.0), step / 2.0, 0.0, step * 128.0) {
                self.remember();
                *slot(&mut self.json["notes"][section]["sectionNotes"][index], 2, &json!(0)) = json!(round((length / (step / 2.0)).round() * step / 2.0));
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
        let picked = match self.one() {
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
                    *slot(&mut self.json["events"][index][1][entry], value + 1, &json!("")) = json!(text);
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
                Some(0) => self.selected = vec![Picked::Event(index, entry.saturating_sub(1))],
                Some(1) => self.selected = vec![Picked::Event(index, (entry + 1).min(count - 1))],
                Some(2) if count > 1 => {
                    self.remember();
                    if let Some(entries) = self.json["events"][index][1].as_array_mut() {
                        entries.remove(entry);
                    }
                    self.selected = vec![Picked::Event(index, entry.saturating_sub(1))];
                }
                Some(3) => {
                    self.remember();
                    let name = self.events.get(self.event).cloned().unwrap_or_default();
                    if let Some(entries) = self.json["events"][index][1].as_array_mut() {
                        entries.push(json!([name, self.values[0], self.values[1]]));
                    }
                    self.selected = vec![Picked::Event(index, count)];
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
        let lines = [
            format!("{} / {}", clock(self.position), clock(self.length)), String::new(),
            format!("Section: {}", self.section_at(self.position)), format!("Beat: {}", (step / 4.0).floor()), format!("Step: {step}"), String::new(),
            format!("Beat Snap: {} / 16", QUANTS[self.quant]), format!("Selected: {}", self.selected.len()),
        ];
        let (x, y) = (1000.0, 390.0);
        draw_rectangle(x, y - 26.0, 110.0, 26.0, Color { a: 0.8, ..BLACK });
        draw_text_ex("Information", x + 8.0, y - 8.0, TextParams { font: skin.font.as_ref(), font_size: 16, color: WHITE, ..Default::default() });
        draw_rectangle(x, y, 220.0, 220.0, Color { a: 0.6, ..BLACK });
        for (row, line) in lines.iter().enumerate() {
            draw_text_ex(line, x + 15.0, y + 30.0 + row as f32 * 18.0, TextParams { font: skin.font.as_ref(), font_size: 16, color: WHITE, ..Default::default() });
        }
        let help = "Space: play  W/S, wheel: move  A/D: section  Click: note  Q/E: hold  Left/Right: snap  Z/X: zoom  Ctrl+Z/Y: undo/redo  Ctrl+S: save  Enter: play  Esc: leave";
        draw_text_ex(help, 10.0, HEIGHT - 10.0, TextParams { font: skin.font.as_ref(), font_size: 14, color: Color { a: 0.8, ..WHITE }, ..Default::default() });
        if let Some((words, when)) = &self.said {
            let alpha = (1.0 - (when.elapsed().as_secs_f64() - 2.0).max(0.0)) as f32;
            if alpha > 0.0 {
                draw_text_ex(words, 25.0, HEIGHT - 32.0, TextParams { font: skin.font.as_ref(), font_size: 20, color: Color { a: alpha, ..WHITE }, ..Default::default() });
            }
        }
    }
}

/// A track's loudest swings each millisecond, to draw its waveform from without going through all its samples.
#[derive(Default)]
struct Peaks {
    /// Each millisecond: how far down and up its left and right channels go, as 0 to 0.5 (the Haxe editor's scale).
    peaks: Vec<[f32; 2]>,
}

impl Peaks {
    fn of(samples: &[i16], rate: u32) -> Peaks {
        let frames = (rate as usize / 1000).max(1);
        let peaks = samples.chunks(frames * 2).map(|chunk| {
            let (mut low, mut high) = ([0i32; 2], [0i32; 2]);
            for frame in chunk.chunks_exact(2) {
                for channel in 0..2 {
                    low[channel] = low[channel].min(frame[channel] as i32);
                    high[channel] = high[channel].max(frame[channel] as i32);
                }
            }
            // the channels' swings added up, down and up, as Haxe's lmin + rmin and lmax + rmax
            [(-low[0] - low[1]) as f32 / 65535.0, (high[0] + high[1]) as f32 / 65535.0]
        }).collect();
        Peaks { peaks }
    }

    /// The loudest swings down and up from `from` to `to` ms.
    fn between(&self, from: f64, to: f64) -> [f32; 2] {
        let (start, end) = (from.max(0.0) as usize, (to.max(0.0) as usize).max(from.max(0.0) as usize + 1));
        self.peaks.get(start..end.min(self.peaks.len())).unwrap_or(&[]).iter().fold([0.0f32; 2], |most, peak| [most[0].max(peak[0]), most[1].max(peak[1])])
    }
}

/// The Vortex Editor's key for a column: a note of it at `at` (ms, within a millisecond) is taken off, else one is put
/// down there (of the note type chosen).
fn vortex(notes: &mut Vec<Value>, at: f64, column: usize, kind: Option<&str>) {
    match notes.iter().position(|note| note[1].as_u64() == Some(column as u64) && note[0].as_f64().is_some_and(|time| (time - at).abs() < 1.0)) {
        Some(place) => {
            notes.remove(place);
        }
        None => notes.push(match kind {
            Some(kind) => json!([at, column, 0, kind]),
            None => json!([at, column, 0]),
        }),
    }
}

/// Where the leftmost of pasted notes `width` columns wide goes from `column` when the mouse moves from column `from`
/// to `to`: as many columns on as the mouse moved, kept on the grid (the Haxe editor's moving notes).
fn follow(column: usize, width: usize, from: usize, to: usize) -> usize {
    (column as i64 + to as i64 - from as i64).clamp(0, 7 - width.min(7) as i64) as usize
}

/// The chart File > New starts (the Haxe editor's openNewChart), before its sections are added.
fn new_chart() -> Value {
    json!({"song": "Test", "notes": [], "events": [], "bpm": 150, "needsVoices": true, "speed": 1, "offset": 0,
        "player1": "bf", "player2": "dad", "gfVersion": "gf", "stage": "stage", "format": "psych_v1"})
}

/// A song's name as its files are named (Paths.formatToSongPath): lower case, spaces as dashes, characters a file
/// name cannot hold left out.
fn song_path(name: &str) -> String {
    name.trim().to_lowercase().replace(' ', "-").chars().filter(|letter| !r#"~&\/;:<>#?*"|"#.contains(*letter)).collect()
}

/// The file named `name` (a name only, no folders) beside `chart`.
fn beside(chart: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    let plain = !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\']);
    plain.then(|| chart.with_file_name(name))
}

/// A colour typed in hexadecimal ("FF00AA", "#ff00aa", "0xFF00AA").
fn hex(text: &str) -> Option<u32> {
    let text = text.trim();
    let digits = text.strip_prefix('#').or_else(|| text.strip_prefix("0x")).unwrap_or(text);
    (digits.len() == 6).then(|| u32::from_str_radix(digits, 16).ok()).flatten()
}

/// The song's events.json holding a chart's events, as the Haxe editor's Save Events writes it.
fn events_text(song: &Value) -> String {
    print(&json!({"events": song["events"].as_array().cloned().unwrap_or_default(), "format": "psych_v1"}), 0)
}

/// A moment as the editor shows it (minutes:seconds).
fn clock(ms: f64) -> String {
    format!("{}:{:05.2}", (ms / 60000.0).floor(), (ms / 1000.0) % 60.0)
}

/// A character's health icon (its file's healthicon; the face when there is none).
fn icon_of(paths: &Paths, character: &str) -> Option<Texture2D> {
    let picture = |name: &str| paths.find(&format!("images/{name}.png")).and_then(|path| crate::atlas::picture(&path));
    let file = paths.find(&format!("characters/{character}.json")).and_then(|file| std::fs::read_to_string(file).ok());
    let name = file.and_then(|text| serde_json::from_str::<Value>(&text).ok()).and_then(|json| json["healthicon"].as_str().map(str::to_owned)).unwrap_or_else(|| "face".to_owned());
    picture(&format!("icons/icon-{name}")).or_else(|| picture(&format!("icons/{name}"))).or_else(|| picture("icons/icon-face"))
}

/// The editor's own settings, in the game's folder.
const PREFS: &str = "funkin-rs-chart-editor.json";

/// The autosaves (`<chart>_<date>_<time>.bkp`) past the `limit` newest, by the date in their names; files named
/// otherwise are left alone.
fn past_limit(names: &[String], limit: usize) -> Vec<&String> {
    let mut dated: Vec<(&str, &String)> = names.iter().filter(|name| name.ends_with(".bkp"))
        .filter_map(|name| { let mut parts = name.rsplitn(3, '_'); let (time, date) = (parts.next()?, parts.next()?); parts.next()?; Some((&name[name.len() - time.len() - date.len() - 1..], name)) })
        .collect();
    dated.sort();
    dated.truncate(dated.len().saturating_sub(limit));
    dated.into_iter().map(|(_, name)| name).collect()
}

/// A value of an event as text (they are mostly text, sometimes numbers).
fn plain(value: &Value) -> String {
    value.as_str().map_or_else(|| if value.is_null() { String::new() } else { value.to_string() }, str::to_owned)
}

/// Where an editor saves the file `relative` (to a root: "characters/bf.json") it read from `found`: over it, unless
/// it belongs elsewhere than the mod folder worked in (the game's, or another mod's), which is left alone; then the
/// file goes into that folder. Working in no mod folder, the file found is saved over, as it is the one in use.
pub(crate) fn save_target(base: &Path, folder: &str, relative: &str, found: Option<PathBuf>) -> PathBuf {
    let own = if folder.is_empty() { base.join("mods") } else { base.join("mods").join(folder) };
    match found {
        Some(path) if folder.is_empty() || path.starts_with(&own) => path,
        _ => own.join(relative),
    }
}

/// A place of a JSON list, made when the list is shorter (filled with `fill`): files written by hand may leave things off.
pub(crate) fn slot<'a>(list: &'a mut Value, index: usize, fill: &Value) -> &'a mut Value {
    if let Some(items) = list.as_array_mut().filter(|items| items.len() <= index) {
        items.resize(index + 1, fill.clone());
    }
    &mut list[index]
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
pub(crate) fn print(value: &Value, depth: usize) -> String {
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
    fn a_new_chart_is_the_haxe_editors() {
        let mut chart = new_chart();
        assert_eq!(print(&chart, 0), "{\n\t\"song\": \"Test\",\n\t\"notes\": [],\n\t\"events\": [],\n\t\"bpm\": 150,\n\t\"needsVoices\": true,\n\t\"speed\": 1,\n\t\"offset\": 0,\n\t\"player1\": \"bf\",\n\t\"player2\": \"dad\",\n\t\"gfVersion\": \"gf\",\n\t\"stage\": \"stage\",\n\t\"format\": \"psych_v1\"\n}");
        fill_sections(&mut chart, 3300.0);
        // four beats at 150 a minute are 1.6 seconds: three sections reach past 3.3
        assert_eq!(chart["notes"].as_array().map(Vec::len), Some(3));
        assert_eq!(chart["notes"][0]["sectionNotes"], json!([]));
        assert_eq!(song_path("My Song?"), "my-song");
    }

    #[test]
    fn typed_names_stay_beside_the_chart_and_colours_are_hexadecimal() {
        let chart = Path::new("/game/mods/m/data/song/song-hard.json");
        assert_eq!(beside(chart, " it's $(x).json "), Some(PathBuf::from("/game/mods/m/data/song/it's $(x).json")));
        assert_eq!(beside(chart, "../events.json"), None);
        assert_eq!(beside(chart, ".."), None);
        assert_eq!(beside(chart, ""), None);
        assert_eq!(hex("#a0a0a0"), Some(0xA0A0A0));
        assert_eq!(hex("0x673AB7"), Some(0x673AB7));
        assert_eq!(hex("12345"), None);
        assert_eq!(hex("GGGGGG"), None);
        assert_eq!(Theme::Custom.colors([1, 2, 3, 4, 5]), [1, 2, 3, 4, 5]);
    }

    #[test]
    fn vortex_keys_put_notes_down_and_take_them_off() {
        let mut notes = vec![json!([1000.0, 2, 0])];
        vortex(&mut notes, 1000.4, 2, None);
        assert!(notes.is_empty());
        vortex(&mut notes, 500.0, 5, Some("Hurt Note"));
        vortex(&mut notes, 500.0, 6, None);
        assert_eq!(notes, vec![json!([500.0, 5, 0, "Hurt Note"]), json!([500.0, 6, 0])]);
    }

    #[test]
    fn waveform_peaks_are_the_loudest_swings_of_each_millisecond() {
        // 2 kHz stereo: two frames a millisecond
        let samples = [100, -50, -200, 300, 0, 0, 32767, -32768];
        let peaks = Peaks::of(&samples, 2000);
        assert_eq!(peaks.peaks.len(), 2);
        assert_eq!(peaks.peaks[0], [250.0 / 65535.0, 400.0 / 65535.0]);
        assert_eq!(peaks.between(0.0, 2.0), [32768.0 / 65535.0, 32767.0 / 65535.0]);
        assert_eq!(peaks.between(5.0, 9.0), [0.0, 0.0]);
    }

    #[test]
    fn the_oldest_autosaves_past_the_limit_go() {
        let names: Vec<String> = ["b-hard_2026-01-02_00-00-00.bkp", "a_2026-01-01_10-00-00.bkp", "a_2026-01-03_00-00-00.bkp", "notes.txt", "plain.bkp"].map(str::to_owned).to_vec();
        assert_eq!(past_limit(&names, 2), vec![&names[1]]);
        assert!(past_limit(&names, 10).is_empty());
    }

    #[test]
    fn a_click_finds_the_note_drawn_under_it() {
        let _alone = crate::audio::ONE_SONG.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
    fn picked_notes_and_events_are_copied_pasted_and_taken_off() {
        let _alone = crate::audio::ONE_SONG.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let text = r#"{"bpm": 120, "format": "psych_v1", "events": [[250, [["Hey!", "BF", ""]]]], "notes": [{"sectionNotes": [[0, 1, 0], [125, 6, 0]], "sectionBeats": 4}]}"#;
        let missing = PathBuf::from("no such folder");
        let entry = SongEntry {
            title: "T".to_owned(), difficulty: "Hard".to_owned(), source: String::new(), week: 0, icon: String::new(), color: [0; 3],
            chart: missing.join("t.json"), inst: missing.join("Inst.ogg"), voices_player: None, voices_opponent: None,
        };
        let mut editor = ChartEditor::open(&missing, &entry, Some(text.to_owned())).unwrap();
        let original = editor.json.clone();
        editor.pick_all();
        assert_eq!(editor.selected.len(), 3);
        editor.copy();
        // pasted, they follow the mouse and are not in the chart until put down
        editor.paste();
        assert_eq!(editor.json, original);
        let floating = editor.floating.as_mut().unwrap();
        assert_eq!((floating.at, floating.column, floating.mouse, floating.width()), (0.0, 0, 1, 5));
        // the mouse half a second in, one column right of where they were copied from: times kept apart as they were
        floating.at = 500.0;
        floating.column = follow(floating.column, floating.width(), floating.mouse, 2);
        editor.put_down();
        assert_eq!(editor.json["notes"][0]["sectionNotes"], json!([[0, 1, 0], [125, 6, 0], [500.0, 1, 0], [625.0, 6, 0]]));
        assert_eq!(editor.json["events"][1], json!([750.0, [["Hey!", "BF", ""]]]));
        // the pasted ones are picked: taking them off leaves the chart as it was
        editor.delete_picked();
        assert_eq!(editor.json, original);
    }

    #[test]
    fn pasted_notes_move_as_many_columns_as_the_mouse_and_stay_on_the_grid() {
        // copied from the opponent's first two columns (4 and 5): they start leftmost, the mouse taken to be over 4
        assert_eq!(follow(0, 1, 4, 6), 2);
        assert_eq!(follow(0, 1, 4, 4), 0);
        // past an edge they stop there, and move back from there with the mouse
        assert_eq!(follow(2, 1, 6, 7), 3);
        assert_eq!(follow(5, 1, 4, 7), 6);
        assert_eq!(follow(6, 1, 7, 6), 5);
        assert_eq!(follow(0, 3, 0, 0), 0);
        assert_eq!(follow(1, 3, 3, 0), 0);
    }

    #[test]
    fn vortex_strums_light_up_as_notes_pass_while_the_music_plays() {
        let _alone = crate::audio::ONE_SONG.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let text = r#"{"bpm": 120, "format": "psych_v1", "notes": [{"sectionNotes": [[0, 1, 0], [125, 1, 0], [150, 6, 1000], [400, 2, 0]], "sectionBeats": 4}]}"#;
        let missing = PathBuf::from("no such folder");
        let entry = SongEntry {
            title: "T".to_owned(), difficulty: "Hard".to_owned(), source: String::new(), week: 0, icon: String::new(), color: [0; 3],
            chart: missing.join("t.json"), inst: missing.join("Inst.ogg"), voices_player: None, voices_opponent: None,
        };
        let mut editor = ChartEditor::open(&missing, &entry, Some(text.to_owned())).unwrap();
        (editor.heard, editor.position) = (0.0, 200.0);
        editor.light_strums();
        assert_eq!(editor.lit, [0.0; 8], "not in the Vortex Editor");
        editor.set_vortex(true);
        editor.light_strums();
        // a step and a quarter (125 ms a step), or as long as the note is held; at half speed, twice as long
        assert_eq!(editor.lit, [0.0, 0.15625, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
        editor.set_rate(0.5);
        editor.light_strums();
        assert_eq!(editor.lit[6], 2.0);
        editor.set_vortex(false);
        assert_eq!(editor.lit, [0.0; 8]);
    }

    #[test]
    fn saved_events_are_read_back_as_the_songs_events() {
        let song = json!({"bpm": 120, "events": [[250, [["Hey!", "BF", ""], ["Play Sound", "a", "1"]]]], "notes": []});
        let text = events_text(&song);
        let read = funkin_core::chart::parse(&text).unwrap();
        assert_eq!(read.events.iter().map(|event| (event.time, event.name.as_str())).collect::<Vec<_>>(), vec![(250.0, "Hey!"), (250.0, "Play Sound")]);
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), json!({"events": [[250, [["Hey!", "BF", ""], ["Play Sound", "a", "1"]]]], "format": "psych_v1"}));
    }

    #[test]
    fn saves_never_go_over_files_of_another_place_than_the_mod_folder() {
        let base = Path::new("game");
        let game_file = base.join("assets/shared/characters/bf.json");
        let own_file = base.join("mods/Mine/characters/bf.json");
        // in a mod folder: its own file is saved over; the game's (or another mod's) stays, a copy goes into the folder
        assert_eq!(save_target(base, "Mine", "characters/bf.json", Some(own_file.clone())), own_file);
        assert_eq!(save_target(base, "Mine", "characters/bf.json", Some(game_file.clone())), own_file);
        assert_eq!(save_target(base, "Mine", "characters/bf.json", Some(base.join("mods/Other/characters/bf.json"))), own_file);
        // in no mod folder the file in use is the one saved over; a new one goes under mods/
        assert_eq!(save_target(base, "", "characters/bf.json", Some(game_file.clone())), game_file);
        assert_eq!(save_target(base, "", "characters/new.json", None), base.join("mods/characters/new.json"));
    }

    #[test]
    fn short_lists_are_filled_before_writing_into_them() {
        let mut note = json!([100, 1]);
        *slot(&mut note, 2, &json!(0)) = json!(250);
        assert_eq!(note, json!([100, 1, 250]));
        let mut entry = json!(["Hey!"]);
        *slot(&mut entry, 2, &json!("")) = json!("0.6");
        assert_eq!(entry, json!(["Hey!", "", "0.6"]));
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
