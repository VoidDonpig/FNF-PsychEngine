//! The chart editor's V-Slice conversion (the Haxe editor's VSlice.hx): a Psych chart made a V-Slice chart and its
//! metadata, and V-Slice's chart and metadata made a Psych chart for each difficulty and an events file. Only JSON in
//! and JSON out: the editor's File menu reads and writes the files.
use serde_json::{json, Map, Value};

pub const METADATA_VERSION: &str = "2.2.3";
pub const CHART_VERSION: &str = "2.0.0";
const ENGINE: &str = "Psych Engine v1.0.4";

/// The stages Psych and V-Slice name differently (Psych's name, V-Slice's).
const STAGES: [(&str, &str); 6] = [("stage", "mainStage"), ("spooky", "spookyMansion"), ("philly", "phillyTrain"), ("limo", "limoRide"), ("mall", "mallXmas"), ("tank", "tankmanBattlefield")];

/// What a V-Slice chart and metadata make: each difficulty's Psych chart (in the metadata's order) and the events,
/// when there are any but the camera's.
pub struct Psych {
    pub difficulties: Vec<(String, Value)>,
    pub events: Option<Value>,
}

/// A number as Haxe writes a Float: a whole one without its fraction.
fn number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 1e15 { json!(value as i64) } else { json!(value) }
}

/// A value as Haxe's Std.string gives it.
fn string_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(_) => number(value.as_f64().unwrap_or(0.0)).to_string(),
        other => other.to_string(),
    }
}

/// A beat's length (ms) at `bpm` (Conductor.calculateCrochet).
fn crochet(bpm: f64) -> f64 {
    60.0 / bpm * 1000.0
}

fn time_of(value: &Value) -> f64 {
    value["t"].as_f64().unwrap_or(0.0)
}

fn sort_by_time(list: &mut [Value]) {
    list.sort_by(|a, b| time_of(a).total_cmp(&time_of(b)));
}

/// A Psych chart (a psych_v1 one: the song's own fields) as V-Slice's chart and metadata (VSlice.export), its notes
/// under each of `difficulties` (names as in file names); `pixel`: its stage is a pixel one (the note style).
pub fn export(song: &Value, difficulties: &[String], pixel: bool) -> (Value, Value) {
    let mut events = Vec::new();
    for event in song["events"].as_array().into_iter().flatten() {
        for little in event[1].as_array().into_iter().flatten() {
            events.push(json!({"t": event[0], "e": little[0], "v": {"value1": little[1], "value2": little[2]}}));
        }
    }
    let mut notes = Vec::new();
    let generated_by = format!("{ENGINE} - Chart Editor V-Slice Exporter");
    let mut bpm = song["bpm"].as_f64().filter(|bpm| *bpm > 0.0).unwrap_or(100.0);
    let mut time_changes = vec![json!({"t": 0, "bpm": number(bpm)})];
    let mut time = 0.0;
    let mut last_must_hit = false;
    for section in song["notes"].as_array().into_iter().flatten() {
        for note in section["sectionNotes"].as_array().into_iter().flatten() {
            let mut vslice = json!({"t": note[0], "d": note[1]});
            if note[2].as_f64().is_some_and(|length| length > 0.0) {
                vslice["l"] = note[2].clone();
            }
            if note[3].as_str().is_some_and(|kind| !kind.is_empty()) {
                vslice["k"] = note[3].clone();
            }
            notes.push(vslice);
        }
        // camera events in place of "Must hit section"'s camera focus
        if section["changeBPM"].as_bool().unwrap_or(false) {
            bpm = section["bpm"].as_f64().filter(|bpm| *bpm > 0.0).unwrap_or(bpm);
            time_changes.push(json!({"t": number(time), "bpm": number(bpm)}));
        }
        let must_hit = section["mustHitSection"].as_bool().unwrap_or(false);
        if last_must_hit != must_hit {
            events.push(json!({"t": number(time), "e": "FocusCamera", "v": {"char": if must_hit { 0 } else { 1 }}}));
            last_must_hit = must_hit;
        }
        let rows = (4.0 * section["sectionBeats"].as_f64().unwrap_or(4.0)).round();
        time += crochet(bpm) * (rows / 4.0);
    }
    sort_by_time(&mut events);
    sort_by_time(&mut notes);

    // (Psych charts have no composer: one kept in the chart by a mod or by the importer)
    let text = |key: &str| song.get(key).map(string_of);
    let artist = text("artist").or_else(|| text("composer")).unwrap_or_else(|| "Unknown".to_owned());
    // shortcut: VSlice.hx puts a chart's "charter" in the artist's place (a slip there); here it is the charter
    let charter = text("charter").unwrap_or_else(|| "Unknown".to_owned());

    // (every difficulty, or the song may not show up in V-Slice's Freeplay)
    // shortcut: VSlice.hx means to list Difficulty.list but its list is shadowed and stays null; the names given are listed here
    let mut speeds = Map::new();
    let mut by_difficulty = Map::new();
    for name in difficulties {
        speeds.insert(name.clone(), song["speed"].clone());
        by_difficulty.insert(name.clone(), Value::Array(notes.clone()));
    }
    let chart = json!({"scrollSpeed": speeds, "events": events, "notes": by_difficulty, "generatedBy": generated_by, "version": CHART_VERSION});

    let stage = song["stage"].as_str().map(|stage| STAGES.iter().find(|(psych, _)| *psych == stage).map_or(stage, |(_, vslice)| *vslice));
    let girlfriend = if song["gfVersion"].is_null() { json!("") } else { song["gfVersion"].clone() };
    let metadata = json!({
        "songName": song["song"], "artist": artist, "charter": charter,
        "playData": {
            "difficulties": difficulties,
            "characters": {"player": song["player1"], "girlfriend": girlfriend, "opponent": song["player2"]},
            "noteStyle": if pixel { "pixel" } else { "funkin" },
            "stage": stage,
        },
        "timeFormat": "ms", "timeChanges": time_changes, "generatedBy": generated_by, "version": METADATA_VERSION,
    });
    (chart, metadata)
}

/// Whether `chart` is a V-Slice chart file (as the Haxe editor checks it).
pub fn is_chart(chart: &Value) -> bool {
    chart.is_object() && !chart["version"].is_null() && !chart["notes"].is_null() && !chart["scrollSpeed"].is_null()
}

/// Whether `metadata` is a V-Slice metadata file (as the Haxe editor checks it).
pub fn is_metadata(metadata: &Value) -> bool {
    metadata.is_object() && !metadata["version"].is_null() && metadata["playData"].is_object() && !metadata["songName"].is_null()
        && metadata["playData"]["difficulties"].is_array() && metadata["timeChanges"].as_array().is_some_and(|changes| !changes.is_empty())
}

/// V-Slice's chart and metadata as Psych charts and events (VSlice.convertToPsych).
pub fn to_psych(chart: &Value, metadata: &Value) -> Psych {
    let mut changes = metadata["timeChanges"].as_array().cloned().unwrap_or_default();
    sort_by_time(&mut changes);
    let song_bpm = changes.first().and_then(|change| change["bpm"].as_f64()).filter(|bpm| *bpm > 0.0).unwrap_or(100.0);
    let changes: Vec<(f64, f64)> = changes.iter().skip(1).map(|change| (time_of(change), change["bpm"].as_f64().filter(|bpm| *bpm > 0.0).unwrap_or(song_bpm))).collect();
    let bpm_at = |time: f64| changes.iter().take_while(|(at, _)| time >= *at).last().map_or(song_bpm, |(_, bpm)| *bpm);

    let stage = metadata["playData"]["stage"].as_str().map(|stage| STAGES.iter().find(|(_, vslice)| *vslice == stage).map_or(stage, |(psych, _)| *psych));
    let difficulties: Vec<String> = metadata["playData"]["difficulties"].as_array().into_iter().flatten().filter_map(|name| name.as_str().map(str::to_owned)).collect();
    let mut last_note_time: f64 = 0.0;
    let mut notes_of = Vec::new();
    for name in &difficulties {
        let mut notes = chart["notes"][name].as_array().cloned().unwrap_or_default();
        sort_by_time(&mut notes);
        if let Some(last) = notes.last() {
            last_note_time = last_note_time.max(time_of(last));
        }
        notes_of.push(notes);
    }

    // which side each section's camera is on, from the FocusCamera events
    let mut events = chart["events"].as_array().cloned().unwrap_or_default();
    sort_by_time(&mut events);
    let is_focus = |event: &Value| event["e"] == "FocusCamera" && (event["v"].as_f64().is_some_and(|v| v == 0.0 || v == 1.0) || !event["v"]["char"].is_null());
    let (focus, rest): (Vec<Value>, Vec<Value>) = events.into_iter().partition(is_focus);
    let mut must_hits = Vec::new();
    if let Some(last) = focus.last() {
        // (as VSlice.hx: sections only until the last FocusCamera's time, so its own section keeps the side before it)
        let (end, mut time, mut next, mut must_hit) = (time_of(last), 0.0, 0, false);
        while time < end {
            for (index, event) in focus.iter().enumerate().skip(next) {
                if time + 1.0 < time_of(event) {
                    next = index;
                    break;
                }
                let value = &event["v"];
                must_hit = string_of(if value["char"].is_null() { value } else { &value["char"] }) == "0";
            }
            must_hits.push(must_hit);
            time += crochet(bpm_at(time)) * 4.0;
        }
    }
    if must_hits.is_empty() {
        must_hits.push(false);
    }

    // sections enough to reach the last note of every difficulty
    let (mut sections, mut starts) = (Vec::new(), Vec::new());
    let (mut last_bpm, mut time) = (song_bpm, 0.0);
    while time < last_note_time {
        let bpm = bpm_at(time);
        starts.push(time);
        time += crochet(bpm) * 4.0;
        let mut section = json!({"sectionNotes": [], "sectionBeats": 4, "mustHitSection": must_hits[sections.len().min(must_hits.len() - 1)]});
        if last_bpm != bpm {
            section["changeBPM"] = json!(true);
            section["bpm"] = number(bpm);
            last_bpm = bpm;
        }
        sections.push(section);
    }

    let characters = &metadata["playData"]["characters"];
    let mut charts = Vec::new();
    for (name, notes) in difficulties.iter().zip(notes_of) {
        let speeds = &chart["scrollSpeed"];
        let speed = speeds.get(name).unwrap_or(&speeds["default"]).clone();
        let mut data = sections.clone();
        let mut at = 0;
        for note in notes {
            while at + 1 < starts.len() && starts[at + 1] <= time_of(&note) {
                at += 1;
            }
            let mut psych = vec![note["t"].clone(), note["d"].clone(), if note["l"].is_null() { json!(0) } else { note["l"].clone() }];
            if let Some(kind) = note["k"].as_str().filter(|kind| !kind.is_empty() && *kind != "normal") {
                psych.push(json!(kind));
            }
            if let Some(section) = data.get_mut(at) {
                section["sectionNotes"].as_array_mut().expect("made an array").push(json!(psych));
            }
        }
        charts.push((name.clone(), json!({
            "song": metadata["songName"], "notes": data, "events": [], "bpm": number(song_bpm),
            // (V-Slice has nothing saying whether there are voices: it looks for them)
            "needsVoices": true, "speed": speed, "offset": 0,
            "player1": characters["player"], "player2": characters["opponent"], "gfVersion": characters["girlfriend"], "stage": stage,
            "format": "psych_v1_convert", "artist": metadata["artist"], "charter": metadata["charter"],
            "generatedBy": format!("{ENGINE} - Chart Editor V-Slice Importer"),
        })));
    }

    // the other events, each a Psych event of its own with two values
    let events = (!rest.is_empty()).then(|| {
        let mut file: Vec<Value> = rest.iter().map(|event| {
            let mut fields: Vec<Value> = Vec::new();
            match &event["v"] {
                Value::Null => {}
                // shortcut: an object's first two fields in the file's order (Haxe's Reflect.fields has no set order)
                Value::Object(map) => fields.extend(map.values().take(2).map(|value| json!(string_of(value)))),
                Value::String(text) => fields.push(json!(text)),
                Value::Array(items) => fields.extend(items.iter().take(2).map(|value| json!(string_of(value)))),
                other => fields.push(json!(string_of(other))),
            }
            while fields.len() < 2 {
                fields.push(json!(""));
            }
            fields.insert(0, event["e"].clone());
            json!([event["t"], [fields]])
        }).collect();
        file.sort_by(|a, b| a[0].as_f64().unwrap_or(0.0).total_cmp(&b[0].as_f64().unwrap_or(0.0)));
        json!({"events": file, "format": "psych_v1_convert"})
    });
    Psych { difficulties: charts, events }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Notes on both sides, a hold, a typed note, a BPM change, the camera turning between sections and one event.
    fn psych_chart() -> Value {
        json!({
            "song": "Test Song", "bpm": 100, "speed": 2.5, "player1": "bf", "player2": "dad", "gfVersion": "gf", "stage": "stage",
            "format": "psych_v1", "artist": "Kawai Sprite",
            "events": [[1200, [["Hey!", "BF", "0.6"]]]],
            "notes": [
                {"sectionNotes": [[0, 0, 0], [600, 5, 300]], "sectionBeats": 4, "mustHitSection": true, "changeBPM": false, "bpm": 100},
                {"sectionNotes": [[2400, 4, 0, "Hurt Note"]], "sectionBeats": 4, "mustHitSection": false, "changeBPM": true, "bpm": 150},
                {"sectionNotes": [[4400, 6, 0], [4000, 1, 0]], "sectionBeats": 4, "mustHitSection": false, "changeBPM": false, "bpm": 150},
            ],
        })
    }

    fn names() -> Vec<String> {
        vec!["easy".to_owned(), "normal".to_owned()]
    }

    #[test]
    fn a_psych_chart_exports_as_vslice_hx_does() {
        let (chart, metadata) = export(&psych_chart(), &names(), false);
        let notes = json!([{"t": 0, "d": 0}, {"t": 600, "d": 5, "l": 300}, {"t": 2400, "d": 4, "k": "Hurt Note"}, {"t": 4000, "d": 1}, {"t": 4400, "d": 6}]);
        let by = "Psych Engine v1.0.4 - Chart Editor V-Slice Exporter";
        assert_eq!(chart, json!({
            "scrollSpeed": {"easy": 2.5, "normal": 2.5},
            "events": [
                {"t": 0, "e": "FocusCamera", "v": {"char": 0}},
                {"t": 1200, "e": "Hey!", "v": {"value1": "BF", "value2": "0.6"}},
                {"t": 2400, "e": "FocusCamera", "v": {"char": 1}},
            ],
            "notes": {"easy": notes, "normal": notes}, "generatedBy": by, "version": "2.0.0",
        }));
        assert_eq!(metadata, json!({
            "songName": "Test Song", "artist": "Kawai Sprite", "charter": "Unknown",
            "playData": {"difficulties": ["easy", "normal"], "characters": {"player": "bf", "girlfriend": "gf", "opponent": "dad"}, "noteStyle": "funkin", "stage": "mainStage"},
            "timeFormat": "ms", "timeChanges": [{"t": 0, "bpm": 100}, {"t": 2400, "bpm": 150}], "generatedBy": by, "version": "2.2.3",
        }));
        assert!(is_chart(&chart) && is_metadata(&metadata) && !is_chart(&metadata) && !is_metadata(&chart));
        assert_eq!(export(&psych_chart(), &names(), true).1["playData"]["noteStyle"], "pixel");
    }

    #[test]
    fn an_exported_chart_imports_back_to_its_notes_bpms_and_events() {
        let (chart, metadata) = export(&psych_chart(), &names(), false);
        let psych = to_psych(&chart, &metadata);
        assert_eq!(psych.difficulties.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>(), names());
        let notes = |song: &Value| song["notes"].as_array().unwrap().iter().flat_map(|section| section["sectionNotes"].as_array().unwrap().clone()).collect::<Vec<_>>();
        let changes = |song: &Value| song["notes"].as_array().unwrap().iter().enumerate()
            .filter(|(_, section)| section["changeBPM"] == true).map(|(index, section)| (index, section["bpm"].clone())).collect::<Vec<_>>();
        let mut original = psych_chart();
        original["notes"][2]["sectionNotes"] = json!([[4000, 1, 0], [4400, 6, 0]]);
        for (_, song) in &psych.difficulties {
            assert_eq!(notes(song), notes(&original));
            assert_eq!(changes(song), changes(&original));
            assert_eq!(song["notes"].as_array().unwrap().len(), 3);
            assert_eq!((&song["bpm"], &song["speed"], &song["stage"], &song["player2"]), (&json!(100), &json!(2.5), &json!("stage"), &json!("dad")));
            assert_eq!((&song["artist"], &song["format"]), (&json!("Kawai Sprite"), &json!("psych_v1_convert")));
            // as VSlice.hx: the camera turns to the opponent with the last FocusCamera, whose section keeps the side before it
            let sides: Vec<&Value> = song["notes"].as_array().unwrap().iter().map(|section| &section["mustHitSection"]).collect();
            assert_eq!(sides, [&json!(true); 3]);
        }
        assert_eq!(psych.events, Some(json!({"events": [[1200, [["Hey!", "BF", "0.6"]]]], "format": "psych_v1_convert"})));
        // and exported again, the same notes and time changes
        let (again, again_metadata) = export(&psych.difficulties[1].1, &names(), false);
        assert_eq!(again["notes"], chart["notes"]);
        assert_eq!(again_metadata["timeChanges"], metadata["timeChanges"]);
    }

    #[test]
    fn camera_events_set_the_sides_and_other_values_become_two_texts() {
        let chart = json!({"version": "2.0.0", "scrollSpeed": {"default": 1.8}, "notes": {"hard": [{"t": 7000, "d": 2, "k": "normal"}]},
            "events": [{"t": 0, "e": "FocusCamera", "v": 0}, {"t": 2400, "e": "FocusCamera", "v": {"char": 1}}, {"t": 4800, "e": "FocusCamera", "v": {"char": 0}},
                {"t": 100, "e": "PlayAnimation", "v": {"target": "bf", "anim": "hey", "force": true}}, {"t": 50, "e": "ZoomCamera", "v": [1.5, 4, "x"]}, {"t": 10, "e": "Flash", "v": 3}]});
        let metadata = json!({"version": "2.2.3", "songName": "Some Song", "playData": {"difficulties": ["hard"], "stage": "limoRide", "characters": {"player": "bf", "opponent": "mom", "girlfriend": "gf"}},
            "timeChanges": [{"t": 0, "bpm": 100}]});
        let psych = to_psych(&chart, &metadata);
        let song = &psych.difficulties[0].1;
        let sides: Vec<bool> = song["notes"].as_array().unwrap().iter().map(|section| section["mustHitSection"].as_bool().unwrap()).collect();
        assert_eq!(sides, [true, false, false]);
        assert_eq!((&song["speed"], &song["stage"], &song["notes"][2]["sectionNotes"]), (&json!(1.8), &json!("limo"), &json!([[7000, 2, 0]])));
        assert_eq!(psych.events.unwrap()["events"], json!([[10, [["Flash", "3", ""]]], [50, [["ZoomCamera", "1.5", "4"]]], [100, [["PlayAnimation", "bf", "hey"]]]]));
    }
}
