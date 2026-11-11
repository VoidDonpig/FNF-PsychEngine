//! Psych Engine chart files (`psych_v1` and the older section-relative format).
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// Milliseconds from the start of the song.
    pub time: f64,
    /// 0 = left, 1 = down, 2 = up, 3 = right.
    pub lane: usize,
    /// On the player's side (otherwise the opponent plays it).
    pub player: bool,
    /// Hold length in milliseconds, 0 for a tap.
    pub sustain: f64,
    pub kind: NoteKind,
}

/// What a note does besides being hit. Charts name it by the note type's title.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum NoteKind {
    #[default]
    Normal,
    /// Hitting it counts as a miss, letting it pass does not.
    Hurt,
    /// Sung with the character's alternative poses.
    Alt,
    /// The singer cheers instead of singing.
    Hey,
    /// The girlfriend sings it.
    GfSing,
    /// Nobody moves for it.
    NoAnimation,
    /// Weekend 1: the opponent lights a spray can, kicks it up and knees it over,
    LightCan,
    KickCan,
    KneeCan,
    /// and the player cocks the gun and shoots the can down. Missing the shot hurts.
    CockGun,
    FireGun,
    /// Weekend 1: a move of the fist fight, by its place in `FIGHT_MOVES`.
    Fight(u8),
}

/// The moves of the weekend 1 fist fight: note types named "weekend-1-" and one of these.
pub const FIGHT_MOVES: [&str; 26] = [
    "punchlow", "punchlowblocked", "punchlowdodged", "punchlowspin", "punchhigh", "punchhighblocked", "punchhighdodged", "punchhighspin",
    "blockhigh", "blocklow", "blockspin", "dodgehigh", "dodgelow", "dodgespin", "hithigh", "hitlow", "hitspin",
    "picouppercutprep", "picouppercut", "darnelluppercutprep", "darnelluppercut", "idle", "fakeout", "taunt", "tauntforce", "reversefakeout",
];

/// Something a chart schedules besides notes: a camera zoom, a pose, a character swap.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartEvent {
    /// Milliseconds from the start of the song.
    pub time: f64,
    pub name: String,
    pub value1: String,
    pub value2: String,
}

/// A stretch of the chart (normally one bar) and who the camera looks at during it.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// Milliseconds from the start of the song.
    pub start: f64,
    pub must_hit: bool,
    pub gf: bool,
    /// The opponent sings this section with its alternative poses.
    pub alt: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Chart {
    pub song: String,
    pub bpm: f64,
    /// Scroll speed multiplier.
    pub speed: f64,
    /// Sorted by time.
    pub notes: Vec<Note>,
    /// Sorted by time.
    pub events: Vec<ChartEvent>,
    pub sections: Vec<Section>,
    /// Tempo changes as (time in ms, beat number there, BPM from there on); the first is at 0.
    pub tempo: Vec<(f64, f64, f64)>,
    pub player: String,
    pub opponent: String,
    pub gf: String,
    pub stage: String,
    /// The chart is this many ms ahead of its sound.
    pub offset: f64,
    /// The chart's own note and splash pictures (under `images/`); empty: the player's.
    pub arrow_skin: String,
    pub splash_skin: String,
    /// The notes keep the colours they were painted in.
    pub no_rgb: bool,
}

impl Chart {
    /// Beat number (fractional) at a song position; negative before the song starts.
    pub fn beat_at(&self, ms: f64) -> f64 {
        let &(start, beat, bpm) = self.tempo.iter().rev().find(|t| t.0 <= ms).or(self.tempo.first()).unwrap_or(&(0.0, 0.0, self.bpm.max(1.0)));
        beat + (ms - start) * bpm / 60000.0
    }

    /// Length of a 16th note at a song position, in ms.
    pub fn step_ms(&self, ms: f64) -> f64 {
        let bpm = self.tempo.iter().rev().find(|t| t.0 <= ms).or(self.tempo.first()).map_or(self.bpm, |t| t.2);
        15000.0 / bpm.max(1.0)
    }

    pub fn section_at(&self, ms: f64) -> Option<&Section> {
        self.sections.iter().rev().find(|s| s.start <= ms).or(self.sections.first())
    }
}

/// Note types that old charts store as a number.
const LEGACY_NOTE_TYPES: [&str; 6] = ["", "Alt Animation", "Hey!", "Hurt Note", "GF Sing", "No Animation"];

pub fn parse(text: &str) -> Result<Chart, String> {
    let json: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    // Old charts wrap everything in {"song": {...}}; in new ones "song" is the name.
    let root = if json["song"].is_object() { &json["song"] } else { &json };
    let v1 = root["format"].as_str().is_some_and(|f| f.starts_with("psych_v1"));

    let mut bpm = root["bpm"].as_f64().filter(|b| *b > 0.0).unwrap_or(100.0);
    let (mut time, mut beat) = (0.0, 0.0);
    let mut tempo = vec![(0.0, 0.0, bpm)];
    let mut sections = Vec::new();
    let mut notes = Vec::new();
    // values are stored as text, but old charts have numbers in them
    let text = |value: &Value| value.as_str().map(str::to_owned).unwrap_or_else(|| if value.is_null() { String::new() } else { value.to_string() });
    let mut events = Vec::new();
    for entry in root["events"].as_array().into_iter().flatten() {
        for event in entry[1].as_array().into_iter().flatten() {
            if let (Some(time), Some(name)) = (entry[0].as_f64(), event[0].as_str()) {
                events.push(ChartEvent { time, name: name.to_owned(), value1: text(&event[1]), value2: text(&event[2]) });
            }
        }
    }
    // a song's events.json is a chart with events only
    for section in root["notes"].as_array().into_iter().flatten() {
        let must_hit = section["mustHitSection"].as_bool().unwrap_or(false);
        if let (Some(true), Some(new)) = (section["changeBPM"].as_bool(), section["bpm"].as_f64().filter(|b| *b > 0.0)) {
            if new != bpm {
                bpm = new;
                tempo.push((time, beat, bpm));
            }
        }
        let gf_section = section["gfSection"].as_bool().unwrap_or(false);
        sections.push(Section { start: time, must_hit, gf: gf_section, alt: section["altAnim"].as_bool().unwrap_or(false) });
        let beats = section["sectionBeats"].as_f64().filter(|b| *b > 0.0).unwrap_or(4.0);
        time += beats * 60000.0 / bpm;
        beat += beats;
        for raw in section["sectionNotes"].as_array().into_iter().flatten() {
            let (Some(time), Some(column)) = (raw[0].as_f64(), raw[1].as_f64()) else { continue };
            if column < 0.0 {
                // an event stored as a note (pre-0.7 charts)
                if let Some(name) = raw[2].as_str() {
                    events.push(ChartEvent { time, name: name.to_owned(), value1: text(&raw[3]), value2: text(&raw[4]) });
                }
                continue;
            }
            let column = column as usize;
            // psych_v1: columns 0-3 are the player's. Before: 0-3 belong to whoever the section focuses on.
            let player = if v1 { column < 4 } else { (column < 4) == must_hit };
            let kind = match &raw[3] {
                Value::String(s) => s.as_str(),
                Value::Number(n) => LEGACY_NOTE_TYPES.get(n.as_u64().unwrap_or(0) as usize).copied().unwrap_or(""),
                _ => "",
            };
            notes.push(Note {
                time,
                lane: column % 4,
                player,
                sustain: raw[2].as_f64().filter(|s| s.is_finite()).unwrap_or(0.0).max(0.0),
                kind: match kind {
                    "Hurt Note" => NoteKind::Hurt,
                    "Alt Animation" => NoteKind::Alt,
                    "Hey!" => NoteKind::Hey,
                    "No Animation" => NoteKind::NoAnimation,
                    "GF Sing" => NoteKind::GfSing,
                    "weekend-1-lightcan" => NoteKind::LightCan,
                    "weekend-1-kickcan" => NoteKind::KickCan,
                    "weekend-1-kneecan" => NoteKind::KneeCan,
                    "weekend-1-cockgun" => NoteKind::CockGun,
                    "weekend-1-firegun" => NoteKind::FireGun,
                    kind if kind.strip_prefix("weekend-1-").is_some_and(|fight| FIGHT_MOVES.contains(&fight)) => {
                        NoteKind::Fight(FIGHT_MOVES.iter().position(|fight| kind.strip_prefix("weekend-1-") == Some(*fight)).unwrap_or(0) as u8)
                    }
                    // in a girlfriend section, the notes of the side in focus are hers
                    _ if gf_section && player == must_hit => NoteKind::GfSing,
                    _ => NoteKind::Normal,
                },
            });
        }
    }
    notes.sort_by(|a, b| a.time.total_cmp(&b.time));
    // Charts sometimes stack two notes on the same spot; the game plays one.
    notes.dedup_by(|b, a| a.lane == b.lane && a.player == b.player && (a.time - b.time).abs() < 1e-3);
    events.sort_by(|a, b| a.time.total_cmp(&b.time));

    let name = |keys: &[&str], default: &str| keys.iter().find_map(|k| root[*k].as_str()).filter(|s| !s.is_empty()).unwrap_or(default).to_owned();
    Ok(Chart {
        song: root["song"].as_str().unwrap_or("").to_owned(),
        bpm: tempo[0].2,
        speed: root["speed"].as_f64().unwrap_or(1.0),
        offset: root["offset"].as_f64().unwrap_or(0.0),
        arrow_skin: root["arrowSkin"].as_str().unwrap_or("").to_owned(),
        splash_skin: root["splashSkin"].as_str().unwrap_or("").to_owned(),
        no_rgb: root["disableNoteRGB"].as_bool().unwrap_or(false),
        notes,
        events,
        sections,
        tempo,
        player: name(&["player1"], "bf"),
        opponent: name(&["player2"], "dad"),
        gf: name(&["gfVersion", "player3"], "gf"),
        stage: name(&["stage"], vanilla_stage(&crate::content::song_path(root["song"].as_str().unwrap_or("")))),
    })
}

/// The stage of a base game song whose chart does not name one.
fn vanilla_stage(song: &str) -> &'static str {
    match song {
        "spookeez" | "south" | "monster" => "spooky",
        "pico" | "blammed" | "philly" | "philly-nice" => "philly",
        "milf" | "satin-panties" | "high" => "limo",
        "cocoa" | "eggnog" => "mall",
        "winter-horrorland" => "mallEvil",
        "senpai" | "roses" => "school",
        "thorns" => "schoolEvil",
        "ugh" | "guns" | "stress" => "tank",
        _ => "stage",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn psych_v1_columns_are_absolute() {
        let chart = parse(r#"{"song":"T","bpm":120,"speed":2.5,"format":"psych_v1","arrowSkin":"noteSkins/mine","disableNoteRGB":true,"notes":[
            {"mustHitSection":false,"sectionNotes":[[500,2,0],[0,5,250,"Hurt Note"]]}]}"#).unwrap();
        assert_eq!(chart.speed, 2.5);
        // its own notes, unpainted; the player's splashes
        assert_eq!((chart.arrow_skin.as_str(), chart.splash_skin.as_str(), chart.no_rgb), ("noteSkins/mine", "", true));
        assert_eq!(chart.notes, vec![
            Note { time: 0.0, lane: 1, player: false, sustain: 250.0, kind: NoteKind::Hurt },
            Note { time: 500.0, lane: 2, player: true, sustain: 0.0, kind: NoteKind::Normal },
        ]);
    }

    #[test]
    fn legacy_columns_follow_the_section() {
        let chart = parse(r#"{"song":{"song":"T","bpm":100,"notes":[
            {"mustHitSection":true,"sectionNotes":[[0,0,0],[100,4,0],[200,-1,"Event","",""]]},
            {"mustHitSection":false,"sectionNotes":[[300,0,0],[400,7,0,3]]}]}}"#).unwrap();
        let sides: Vec<(bool, usize, bool)> = chart.notes.iter().map(|n| (n.player, n.lane, n.kind == NoteKind::Hurt)).collect();
        assert_eq!(sides, vec![(true, 0, false), (false, 0, false), (false, 0, false), (true, 3, true)]);
        assert_eq!(chart.events, vec![ChartEvent { time: 200.0, name: "Event".to_owned(), value1: String::new(), value2: String::new() }]);
    }

    #[test]
    fn sections_and_tempo_changes() {
        let chart = parse(r#"{"song":"T","bpm":120,"player2":"mom","format":"psych_v1","notes":[
            {"mustHitSection":true,"sectionNotes":[]},
            {"mustHitSection":false,"changeBPM":true,"bpm":240,"sectionBeats":2,"sectionNotes":[]},
            {"mustHitSection":true,"gfSection":true,"sectionNotes":[]}]}"#).unwrap();
        assert_eq!(chart.sections.iter().map(|s| s.start).collect::<Vec<_>>(), [0.0, 2000.0, 2500.0]);
        assert_eq!((chart.beat_at(1000.0), chart.beat_at(2250.0), chart.beat_at(-500.0)), (2.0, 5.0, -1.0));
        assert_eq!((chart.step_ms(0.0), chart.step_ms(2100.0)), (125.0, 62.5));
        assert_eq!(chart.section_at(2499.0).map(|s| s.must_hit), Some(false));
        assert!(chart.section_at(9999.0).unwrap().gf);
        assert_eq!((chart.opponent.as_str(), chart.player.as_str(), chart.gf.as_str(), chart.stage.as_str()), ("mom", "bf", "gf", "stage"));
    }

    #[test]
    fn events_and_note_kinds() {
        let chart = parse(r#"{"song":"T","format":"psych_v1","events":[[900,[["Add Camera Zoom","0.1",""],["Hey!",1,0.6]]],[100,[["Set GF Speed","2",""]]]],"notes":[
            {"mustHitSection":true,"gfSection":true,"altAnim":true,"sectionNotes":[[0,0,0],[0,4,0],[10,1,0,"Alt Animation"],[20,2,0,"Hey!"],[30,3,0,"No Animation"],[40,5,0,"GF Sing"]]}]}"#).unwrap();
        assert_eq!(chart.events.iter().map(|e| (e.time, e.name.as_str(), e.value1.as_str(), e.value2.as_str())).collect::<Vec<_>>(),
                   [(100.0, "Set GF Speed", "2", ""), (900.0, "Add Camera Zoom", "0.1", ""), (900.0, "Hey!", "1", "0.6")]);
        let kinds: Vec<NoteKind> = chart.notes.iter().map(|n| n.kind).collect();
        // sorted by time: the player's note of the girlfriend section, the opponent's, then the typed ones
        assert_eq!(kinds, [NoteKind::GfSing, NoteKind::Normal, NoteKind::Alt, NoteKind::Hey, NoteKind::NoAnimation, NoteKind::GfSing]);
        assert!(chart.sections[0].alt);
    }

    #[test]
    fn weekend_1_notes_are_stunts_and_fight_moves() {
        let chart = parse(r#"{"song":"T","format":"psych_v1","notes":[{"mustHitSection":true,"sectionNotes":[
            [0,0,0,"weekend-1-firegun"],[10,1,0,"weekend-1-punchhigh"],[20,2,0,"weekend-1-reversefakeout"],[30,3,0,"weekend-1-unheard-of"]]}]}"#).unwrap();
        let kinds: Vec<NoteKind> = chart.notes.iter().map(|n| n.kind).collect();
        assert_eq!(kinds, [NoteKind::FireGun, NoteKind::Fight(4), NoteKind::Fight(25), NoteKind::Normal]);
        assert_eq!(FIGHT_MOVES[4], "punchhigh");
    }

    #[test]
    fn an_events_file_is_a_chart_without_notes() {
        let chart = parse(r#"{"song":{"events":[[4290,[["Hey!","0",""]]]],"format":"psych_v1"}}"#).unwrap();
        assert!(chart.notes.is_empty());
        assert_eq!(chart.events.len(), 1);
        assert!(parse("not json").is_err());
    }

    #[test]
    fn a_chart_without_a_stage_gets_the_songs_usual_one() {
        let stage = |json: &str| parse(json).unwrap().stage;
        assert_eq!(stage(r#"{"song":"Philly Nice","notes":[]}"#), "philly");
        assert_eq!(stage(r#"{"song":"Spookeez","stage":"","notes":[]}"#), "spooky");
        assert_eq!(stage(r#"{"song":"Spookeez","stage":"tank","notes":[]}"#), "tank");
        assert_eq!(stage(r#"{"song":"My Song","notes":[]}"#), "stage");
    }

    #[test]
    fn stacked_notes_collapse() {
        let chart = parse(r#"{"song":"T","format":"psych_v1","notes":[{"sectionNotes":[[0,1,0],[0,1,0],[0,5,0]]}]}"#).unwrap();
        assert_eq!(chart.notes.len(), 2);
    }

    #[test]
    fn reads_a_base_game_chart() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/base_game/shared/data/fresh/fresh-hard.json");
        let chart = parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!((chart.song.as_str(), chart.bpm), ("Fresh", 120.0));
        assert!(chart.notes.len() > 200 && chart.notes.iter().any(|n| n.player) && chart.notes.iter().any(|n| !n.player));
    }
}
