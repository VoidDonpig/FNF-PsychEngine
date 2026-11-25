//! The achievements and what earns them, as the Haxe engine has them.

pub struct Achievement {
    /// What it is saved under; its picture is `images/achievements/<id>.png`.
    pub id: &'static str,
    pub name: &'static str,
    pub text: &'static str,
    /// Not listed until it is unlocked.
    pub hidden: bool,
}

const fn shown(id: &'static str, name: &'static str, text: &'static str) -> Achievement {
    Achievement { id, name, text, hidden: false }
}

pub const LIST: &[Achievement] = &[
    Achievement { id: "friday_night_play", name: "Freaky on a Friday Night", text: "Play on a Friday... Night.", hidden: true },
    shown("week1_nomiss", "She Calls Me Daddy Too", "Beat Week 1 on Hard with no Misses."),
    shown("week2_nomiss", "No More Tricks", "Beat Week 2 on Hard with no Misses."),
    shown("week3_nomiss", "Call Me The Hitman", "Beat Week 3 on Hard with no Misses."),
    shown("week4_nomiss", "Lady Killer", "Beat Week 4 on Hard with no Misses."),
    shown("week5_nomiss", "Missless Christmas", "Beat Week 5 on Hard with no Misses."),
    shown("week6_nomiss", "Highscore!!", "Beat Week 6 on Hard with no Misses."),
    shown("week7_nomiss", "God Effing Damn It!", "Beat Week 7 on Hard with no Misses."),
    shown("weekend1_nomiss", "Just a Friendly Sparring", "Beat Weekend 1 on Hard with no Misses."),
    shown("ur_bad", "What a Funkin' Disaster!", "Complete a Song with a rating lower than 20%."),
    shown("ur_good", "Perfectionist", "Complete a Song with a rating of 100%."),
    shown(ROADKILL.0, "Roadkill Enthusiast", "Watch the Henchmen die 50 times."),
    shown("oversinging", "Oversinging Much...?", "Sing for 10 seconds without going back to Idle."),
    shown("hype", "Hyperactive", "Finish a Song without going back to Idle."),
    shown("two_keys", "Just the Two of Us", "Finish a Song pressing only two keys."),
    shown("toastie", "Toaster Gamer", "Have you tried to run the game on a toaster?"),
    Achievement { id: "pessy_easter_egg", name: "Engine Gal Pal", text: "Teehee, you found me~!", hidden: true },
    Achievement { id: "debugger", name: "Debugger", text: "Beat the \"Test\" Stage from the Chart Editor.", hidden: true },
];

/// An achievement as the menu lists it: the game's own, then the ones mods declare in `data/achievements.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Listed {
    /// What it is saved under (the `save` of a mod's file); its picture is `images/achievements/<id>.png`.
    pub id: String,
    pub name: String,
    pub text: String,
    pub hidden: bool,
    /// What a counted achievement counts up to; under 1 it has no score.
    pub max_score: f64,
    /// How many decimals the menu shows of the score.
    pub max_decimals: u32,
    /// The mod that declared it, whose folder its picture is looked for in first; none for the game's own and `mods/data`.
    pub from: Option<String>,
}

/// The game's achievements, then those of `mods/data/achievements.json`, then of each switched on mod in the player's order.
/// A name that is taken already is skipped; a file that cannot be read is skipped.
pub fn all(base: &std::path::Path) -> Vec<Listed> {
    let mut list: Vec<Listed> = LIST.iter().map(|a| Listed {
        id: a.id.to_owned(), name: a.name.to_owned(), text: a.text.to_owned(), hidden: a.hidden,
        max_score: if a.id == ROADKILL.0 { ROADKILL.1 as f64 } else { 0.0 }, max_decimals: 0, from: None,
    }).collect();
    let mods = base.join("mods");
    read_file(&mods.join("data/achievements.json"), None, &mut list);
    for (name, _) in crate::content::mods(base).into_iter().filter(|(_, on)| *on) {
        read_file(&mods.join(&name).join("data/achievements.json"), Some(&name), &mut list);
    }
    list
}

/// Makes TJSON-style text strict JSON: drops `//` and `/* */` comments and commas before a closing bracket (never inside strings).
fn loosen(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    out.push(chars[i]);
                } else if chars[i] == '"' {
                    break;
                }
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
            continue;
        } else {
            out.push(c);
        }
        i += 1;
    }
    // second pass: remove commas whose next non-space character closes an array or object
    let b: Vec<char> = out.chars().collect();
    let mut res = String::with_capacity(b.len());
    let mut in_str = false;
    let mut k = 0;
    while k < b.len() {
        let c = b[k];
        if in_str {
            res.push(c);
            if c == '\\' && k + 1 < b.len() {
                k += 1;
                res.push(b[k]);
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            res.push(c);
        } else if c == ',' && b[k + 1..].iter().find(|x| !x.is_whitespace()).is_some_and(|x| *x == ']' || *x == '}') {
        } else {
            res.push(c);
        }
        k += 1;
    }
    res
}

fn read_file(path: &std::path::Path, from: Option<&str>, list: &mut Vec<Listed>) {
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let Ok(serde_json::Value::Array(entries)) = serde_json::from_str(&loosen(text.trim())) else {
        eprintln!("Error loading achievements.json of {}", from.unwrap_or("mods"));
        return;
    };
    for entry in entries {
        // an empty entry, or one without a name to save under, is passed over
        let id = match &entry["save"] {
            serde_json::Value::Null => continue,
            // a "save" that is no text stops the rest of the file, as the Haxe engine's error does
            serde_json::Value::String(id) => id.trim(),
            _ => break,
        };
        if id.is_empty() || list.iter().any(|known| known.id == id) {
            continue;
        }
        let text = |key: &str| entry[key].as_str().map(str::to_owned);
        list.push(Listed {
            id: id.to_owned(), name: text("name").unwrap_or_else(|| id.to_owned()), text: text("description").unwrap_or_default(),
            hidden: entry["hidden"] == true, max_score: entry["maxScore"].as_f64().unwrap_or(0.0),
            max_decimals: entry["maxDecimals"].as_u64().unwrap_or(0) as u32, from: from.map(str::to_owned),
        });
    }
}

/// Where the unlocked achievements and the scores are kept (the save).
pub trait Progress {
    fn unlocked(&self, id: &str) -> bool;
    /// True if it was not unlocked before.
    fn unlock(&mut self, id: &str) -> bool;
    fn value(&self, id: &str) -> f64;
    fn set_value(&mut self, id: &str, value: f64);
}

#[derive(Clone, Copy, PartialEq)]
pub enum Op {
    Get,
    Set,
    Add,
}

/// The score after the call, and whether it just unlocked the achievement (the caller shows the popup and saves).
#[derive(Debug, PartialEq)]
pub struct Score {
    pub value: f64,
    pub unlocked_now: bool,
}

/// Reads, sets or adds to the score of an achievement. An unlocked one answers its top score; one that is not there, -1.
/// An achievement without a score is an error. Reaching the top score unlocks it and keeps the score at the top.
pub fn score(list: &[Listed], progress: &mut impl Progress, id: &str, op: Op, amount: f64) -> Result<Score, String> {
    let Some(achievement) = list.iter().find(|a| a.id == id) else { return Ok(Score { value: -1.0, unlocked_now: false }) };
    if achievement.max_score < 1.0 {
        return Err(format!("Achievement has score disabled or is incorrectly configured: {id}"));
    }
    if progress.unlocked(id) {
        return Ok(Score { value: achievement.max_score, unlocked_now: false });
    }
    let value = match op {
        Op::Get => return Ok(Score { value: progress.value(id), unlocked_now: false }),
        Op::Set => amount,
        Op::Add => amount + progress.value(id),
    };
    let reached = value >= achievement.max_score;
    let value = if reached { achievement.max_score } else { value };
    let unlocked_now = reached && progress.unlock(id);
    progress.set_value(id, value);
    Ok(Score { value, unlocked_now })
}

/// Unlocks an achievement: true if it was locked before, an error if there is none of that name.
pub fn unlock(list: &[Listed], progress: &mut impl Progress, id: &str) -> Result<bool, String> {
    if !exists(list, id) {
        return Err(format!("Achievement \"{id}\" does not exists!"));
    }
    Ok(progress.unlock(id))
}

pub fn exists(list: &[Listed], id: &str) -> bool {
    list.iter().any(|a| a.id == id)
}

/// A score cut (not rounded) to `decimals` decimals, as the menu shows it.
pub fn floor_decimal(value: f64, decimals: u32) -> f64 {
    let scale = 10f64.powi(decimals as i32);
    (value * scale).floor() / scale
}

/// A song played to its end.
pub struct Run<'a> {
    /// The song's title as its week names it.
    pub song: &'a str,
    /// 0 to 1.
    pub accuracy: f64,
    pub practice: bool,
    pub botplay: bool,
    /// The player's character went back to its idle move at some point.
    pub idled: bool,
    /// How many of the four lanes the player pressed.
    pub lanes_used: usize,
    /// The week file, when this song ended a week played in story mode on Hard without one miss.
    pub flawless_week: Option<&'a str>,
    /// Every graphics option is at its lowest.
    pub lowest_graphics: bool,
}

/// The achievement that is counted up to, and how far.
pub const ROADKILL: (&str, u32) = ("roadkill_enthusiast", 50);

/// The achievements a finished song earns.
pub fn earned(run: &Run) -> Vec<&'static str> {
    if run.botplay {
        return Vec::new();
    }
    // practice mode only leaves the one for playing badly
    let fair = !run.practice;
    let week = run.flawless_week.map(|week| format!("{week}_nomiss"));
    LIST.iter().map(|achievement| achievement.id).filter(|id| match *id {
        "ur_bad" => run.accuracy < 0.2 && fair,
        "ur_good" => run.accuracy >= 1.0 && fair,
        "hype" => !run.idled && fair,
        "two_keys" => run.lanes_used <= 2 && fair,
        "toastie" => run.lowest_graphics,
        "debugger" => run.song.eq_ignore_ascii_case("test") && fair,
        id => week.as_deref() == Some(id) && fair,
    }).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn loose_json_is_made_strict() {
        let text = "[ // first\n {\"save\": \"a//b,]\", /* x */ \"name\": \"n\",}, \n]";
        let v: serde_json::Value = serde_json::from_str(&super::loosen(text)).unwrap();
        assert_eq!(v[0]["save"], "a//b,]");
        assert_eq!(v[0]["name"], "n");
    }

    use super::*;

    #[test]
    fn a_finished_song_earns_what_it_shows() {
        let run = Run { song: "Fresh", accuracy: 1.0, practice: false, botplay: false, idled: true, lanes_used: 4, flawless_week: None, lowest_graphics: false };
        assert_eq!(earned(&run), ["ur_good"]);
        assert_eq!(earned(&Run { accuracy: 0.1, idled: false, lanes_used: 2, flawless_week: Some("week3"), ..run }), ["week3_nomiss", "ur_bad", "hype", "two_keys"]);
        assert_eq!(earned(&Run { flawless_week: Some("my-mod-week"), accuracy: 0.5, ..run }), [] as [&str; 0], "only weeks that have one");
        assert!(earned(&Run { practice: true, ..run }).is_empty() && earned(&Run { botplay: true, ..run }).is_empty());
        assert_eq!(earned(&Run { practice: true, lowest_graphics: true, ..run }), ["toastie"], "a toaster is one in practice mode too");
    }

    #[derive(Default)]
    struct Kept(std::collections::HashMap<String, f64>, std::collections::HashSet<String>);
    impl Progress for Kept {
        fn unlocked(&self, id: &str) -> bool { self.1.contains(id) }
        fn unlock(&mut self, id: &str) -> bool { self.1.insert(id.to_owned()) }
        fn value(&self, id: &str) -> f64 { self.0.get(id).copied().unwrap_or(0.0) }
        fn set_value(&mut self, id: &str, value: f64) { self.0.insert(id.to_owned(), value); }
    }

    #[test]
    fn mods_bring_achievements_and_scores_count_to_the_top() {
        let base = std::env::temp_dir().join(format!("funkin-ach-{}", std::process::id()));
        let mods = base.join("mods");
        std::fs::create_dir_all(mods.join("a/data")).unwrap();
        std::fs::create_dir_all(mods.join("b/data")).unwrap();
        std::fs::write(mods.join("a/data/achievements.json"), r#"[null, {"name": "No id"}, {"save": " one ", "name": "One", "description": "d", "hidden": true},
            {"save": "ur_good", "name": "Taken"}, {"save": "far", "name": "Far", "maxScore": 3, "maxDecimals": 1}, {"save": 5}, {"save": "never"}]"#).unwrap();
        std::fs::write(mods.join("b/data/achievements.json"), "[{\"save\": \"one\"}, {\"save\": \"two\"},").unwrap(); // broken: skipped
        std::fs::write(base.join("modsList.txt"), "b|1\na|1").unwrap();
        let list = all(&base);
        let ids: Vec<&str> = list[LIST.len()..].iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["one", "far"], "duplicates, entries without a name and the rest of a file after a bad one are left out");
        assert!(list[LIST.len()].hidden && list[LIST.len()].from.as_deref() == Some("a") && list.iter().find(|a| a.id == "ur_good").unwrap().name != "Taken");
        assert_eq!(list.iter().find(|a| a.id == ROADKILL.0).unwrap().max_score, 50.0);
        let _ = std::fs::remove_dir_all(&base);

        let mut kept = Kept::default();
        assert_eq!(score(&list, &mut kept, "far", Op::Add, 1.5), Ok(Score { value: 1.5, unlocked_now: false }));
        assert_eq!(score(&list, &mut kept, "far", Op::Add, 1.0).unwrap().value, 2.5);
        assert_eq!(score(&list, &mut kept, "far", Op::Add, 9.0), Ok(Score { value: 3.0, unlocked_now: true }));
        assert!(kept.unlocked("far") && score(&list, &mut kept, "far", Op::Set, 0.0) == Ok(Score { value: 3.0, unlocked_now: false }));
        assert_eq!(score(&list, &mut kept, "nothing", Op::Get, 0.0).unwrap().value, -1.0);
        assert!(score(&list, &mut kept, "one", Op::Get, 0.0).is_err() && unlock(&list, &mut kept, "nothing").is_err());
        assert_eq!((unlock(&list, &mut kept, "one"), unlock(&list, &mut kept, "one")), (Ok(true), Ok(false)));
        assert_eq!((floor_decimal(2.678, 1), floor_decimal(2.678, 0), floor_decimal(2.678, 2)), (2.6, 2.0, 2.67));
    }
}
