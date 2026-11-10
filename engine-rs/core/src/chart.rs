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
    /// "Hurt Note": hitting it counts as a miss, letting it pass does not.
    pub hurt: bool,
}

#[derive(Clone, Debug)]
pub struct Chart {
    pub song: String,
    pub bpm: f64,
    /// Scroll speed multiplier.
    pub speed: f64,
    /// Sorted by time.
    pub notes: Vec<Note>,
}

/// Note types that old charts store as a number.
const LEGACY_NOTE_TYPES: [&str; 6] = ["", "Alt Animation", "Hey!", "Hurt Note", "GF Sing", "No Animation"];

pub fn parse(text: &str) -> Result<Chart, String> {
    let json: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    // Old charts wrap everything in {"song": {...}}; in new ones "song" is the name.
    let root = if json["song"].is_object() { &json["song"] } else { &json };
    let v1 = root["format"].as_str().is_some_and(|f| f.starts_with("psych_v1"));

    let mut notes = Vec::new();
    for section in root["notes"].as_array().ok_or("chart has no \"notes\" list")? {
        let must_hit = section["mustHitSection"].as_bool().unwrap_or(false);
        for raw in section["sectionNotes"].as_array().into_iter().flatten() {
            let (Some(time), Some(column)) = (raw[0].as_f64(), raw[1].as_f64()) else { continue };
            if column < 0.0 {
                continue; // an event stored as a note (pre-0.7 charts)
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
                hurt: kind == "Hurt Note",
            });
        }
    }
    notes.sort_by(|a, b| a.time.total_cmp(&b.time));
    // Charts sometimes stack two notes on the same spot; the game plays one.
    notes.dedup_by(|b, a| a.lane == b.lane && a.player == b.player && (a.time - b.time).abs() < 1e-3);

    Ok(Chart {
        song: root["song"].as_str().unwrap_or("").to_owned(),
        bpm: root["bpm"].as_f64().unwrap_or(100.0),
        speed: root["speed"].as_f64().unwrap_or(1.0),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn psych_v1_columns_are_absolute() {
        let chart = parse(r#"{"song":"T","bpm":120,"speed":2.5,"format":"psych_v1","notes":[
            {"mustHitSection":false,"sectionNotes":[[500,2,0],[0,5,250,"Hurt Note"]]}]}"#).unwrap();
        assert_eq!(chart.speed, 2.5);
        assert_eq!(chart.notes, vec![
            Note { time: 0.0, lane: 1, player: false, sustain: 250.0, hurt: true },
            Note { time: 500.0, lane: 2, player: true, sustain: 0.0, hurt: false },
        ]);
    }

    #[test]
    fn legacy_columns_follow_the_section() {
        let chart = parse(r#"{"song":{"song":"T","bpm":100,"notes":[
            {"mustHitSection":true,"sectionNotes":[[0,0,0],[100,4,0],[200,-1,"Event","",""]]},
            {"mustHitSection":false,"sectionNotes":[[300,0,0],[400,7,0,3]]}]}}"#).unwrap();
        let sides: Vec<(bool, usize, bool)> = chart.notes.iter().map(|n| (n.player, n.lane, n.hurt)).collect();
        assert_eq!(sides, vec![(true, 0, false), (false, 0, false), (false, 0, false), (true, 3, true)]);
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
