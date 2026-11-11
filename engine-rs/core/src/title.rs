//! How the title screen is laid out: the stock one, a mod's own (`images/gfDanceTitle.json`), and the hidden ones typed in.
use serde_json::Value;

/// The words that, typed on the title screen, swap its dancer (and typed again, swap her back).
pub const EGGS: [&str; 4] = ["SHADOW", "RIVEREN", "BBPANZU", "PESSY"];

#[derive(Debug, Clone, PartialEq)]
pub struct Title {
    pub logo: (f32, f32),
    pub enter: (f32, f32),
    pub dancer: (f32, f32),
    pub bpm: f64,
    /// The dancer's picture and the animation in it.
    pub image: &'static str,
    pub animation: String,
    /// Frames of the sway to each side. With `idle` the whole animation plays every other beat instead.
    pub left: Vec<u32>,
    pub right: Vec<u32>,
    pub idle: bool,
    /// A picture behind everything, when there is one.
    pub background: String,
    /// The sound that greets a dancer who was just typed in.
    pub jingle: &'static str,
}

impl Title {
    /// The layout a `gfDanceTitle.json` describes (none or a broken one: the stock layout) with the dancer of `egg`.
    pub fn new(json: &str, egg: &str) -> Title {
        let json: Value = serde_json::from_str(json).unwrap_or_default();
        let at = |x: &str, y: &str, or: (f32, f32)| (json[x].as_f64().map_or(or.0, |n| n as f32), json[y].as_f64().map_or(or.1, |n| n as f32));
        let frames = |key: &str, or: Vec<u32>| {
            let listed: Vec<u32> = json[key].as_array().into_iter().flatten().filter_map(|frame| Some(frame.as_u64()? as u32)).collect();
            if listed.is_empty() { or } else { listed }
        };
        let mut title = Title {
            logo: at("titlex", "titley", (-150.0, -100.0)),
            enter: at("startx", "starty", (100.0, 576.0)),
            dancer: at("gfx", "gfy", (512.0, 40.0)),
            bpm: json["bpm"].as_f64().filter(|bpm| *bpm > 0.0).unwrap_or(102.0),
            image: "gfDanceTitle",
            animation: json["animation"].as_str().filter(|name| !name.is_empty()).unwrap_or("gfDance").to_owned(),
            left: frames("dance_left", (15..30).collect()),
            right: frames("dance_right", [30].into_iter().chain(0..15).collect()),
            idle: json["idle"].as_bool().unwrap_or(false),
            background: json["backgroundSprite"].as_str().unwrap_or("").trim().to_owned(),
            jingle: "",
        };
        // each hidden dancer: picture, animation, how far it stands from the usual place, sound, and its sways (none: it only bumps)
        let (image, animation, shift, jingle, sways): (_, _, _, _, Option<(Vec<u32>, Vec<u32>)>) = match egg {
            "SHADOW" => ("ShadowBump", "Shadow Title Bump", (210.0, 40.0), "JingleShadow", None),
            "RIVEREN" => ("ZRiverBump", "River Title Bump", (180.0, 40.0), "JingleRiver", None),
            "BBPANZU" => ("BBBump", "BB Title Bump", (45.0, 100.0), "JingleBB", Some(((14..28).collect(), [27].into_iter().chain(0..14).collect()))),
            "PESSY" => ("PessyBump", "Pessy Title Bump", (165.0, 60.0), "JinglePessy", Some(([29].into_iter().chain(0..15).collect(), (15..29).collect()))),
            _ => return title,
        };
        (title.image, title.animation, title.jingle) = (image, animation.to_owned(), jingle);
        title.dancer = (title.dancer.0 + shift.0, title.dancer.1 + shift.1);
        match sways {
            Some((left, right)) => (title.left, title.right) = (left, right),
            None => title.idle = true,
        }
        title
    }
}

/// Adds a key to the letters typed on the title screen (the last 32 are kept) and names the word they now spell, emptying them.
pub fn typed(letters: &mut String, key: char) -> Option<&'static str> {
    if !key.is_ascii_alphabetic() {
        return None;
    }
    letters.push(key.to_ascii_uppercase());
    if letters.len() >= 32 {
        letters.remove(0);
    }
    let word = EGGS.into_iter().find(|word| letters.contains(word))?;
    letters.clear();
    Some(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mods_title_file_moves_things_and_a_typed_word_swaps_the_dancer() {
        let stock = Title::new("", "");
        assert_eq!((stock.dancer, stock.bpm, stock.image, stock.idle), ((512.0, 40.0), 102.0, "gfDanceTitle", false));
        assert_eq!((stock.left[0], stock.right[0], stock.right.len()), (15, 30, 16));

        let own = Title::new(r#"{"titlex": 10, "titley": 20, "gfx": 300, "gfy": 0, "bpm": 150, "animation": "bop", "dance_left": [1, 2], "idle": true, "backgroundSprite": " sky "}"#, "");
        assert_eq!((own.logo, own.dancer, own.enter, own.bpm), ((10.0, 20.0), (300.0, 0.0), (100.0, 576.0), 150.0));
        assert_eq!((own.animation.as_str(), own.left.as_slice(), own.right.len(), own.idle, own.background.as_str()), ("bop", &[1, 2][..], 16, true, "sky"));

        // the hidden dancers stand off the place the file gives; two of them only bump
        let shadow = Title::new(r#"{"gfx": 300, "gfy": 0}"#, "SHADOW");
        assert_eq!((shadow.dancer, shadow.image, shadow.idle, shadow.jingle), ((510.0, 40.0), "ShadowBump", true, "JingleShadow"));
        let pessy = Title::new("", "PESSY");
        assert_eq!((pessy.left.len(), pessy.right.len(), pessy.idle), (16, 14, false));
    }

    #[test]
    fn letters_spell_a_word_among_other_keys() {
        let mut letters = String::new();
        let spelled: Vec<_> = "xx sha1dow".chars().filter_map(|key| typed(&mut letters, key)).collect();
        assert_eq!((spelled, letters.as_str()), (vec!["SHADOW"], ""));
        // only the last 32 letters are kept
        for _ in 0..100 {
            assert_eq!(typed(&mut letters, 'p'), None);
        }
        assert_eq!(letters.len(), 31);
        assert_eq!("essy".chars().filter_map(|key| typed(&mut letters, key)).last(), Some("PESSY"));
    }
}
