//! What the player keeps between runs: best scores and the weeks finished.
use crate::content::{song_path, suffix, Week};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, PartialEq)]
pub struct Save {
    /// Best score of a song on a difficulty, with the accuracy (0 to 1) of that run.
    songs: BTreeMap<String, (i64, f64)>,
    /// Best total of a week on a difficulty.
    weeks: BTreeMap<String, i64>,
    /// Week files finished in story mode.
    completed: BTreeSet<String>,
    /// Achievements unlocked, by id.
    achievements: BTreeSet<String>,
    /// How far the achievements that are counted up to have got.
    counts: BTreeMap<String, u32>,
    /// The word last typed on the title screen, while its dancer stays.
    pub egg: String,
}

fn key(name: &str, difficulty: &str) -> String {
    song_path(name) + &suffix(difficulty)
}

fn file(base: &Path) -> PathBuf {
    base.join("funkin-rs-save.json")
}

impl Save {
    pub fn song(&self, title: &str, difficulty: &str) -> (i64, f64) {
        self.songs.get(&key(title, difficulty)).copied().unwrap_or_default()
    }

    pub fn week(&self, week: &str, difficulty: &str) -> i64 {
        self.weeks.get(&key(week, difficulty)).copied().unwrap_or_default()
    }

    /// Keeps the run if it beats the best score so far; the accuracy kept is the one of the best run.
    pub fn record_song(&mut self, title: &str, difficulty: &str, score: i64, accuracy: f64) {
        let best = self.songs.entry(key(title, difficulty)).or_insert((score, accuracy));
        if score > best.0 {
            *best = (score, accuracy);
        }
    }

    /// A week played to its end: it counts as finished, and its total is kept if it is the best.
    pub fn record_week(&mut self, week: &str, difficulty: &str, score: i64) {
        self.completed.insert(week.to_owned());
        let best = self.weeks.entry(key(week, difficulty)).or_insert(score);
        *best = score.max(*best);
    }

    /// Forgets the best score of a song on a difficulty.
    pub fn reset_song(&mut self, title: &str, difficulty: &str) {
        self.songs.remove(&key(title, difficulty));
    }

    /// Forgets the best total of a week on a difficulty. The week stays finished.
    pub fn reset_week(&mut self, week: &str, difficulty: &str) {
        self.weeks.remove(&key(week, difficulty));
    }

    /// Unlocks an achievement; true if it was not unlocked before.
    pub fn unlock(&mut self, achievement: &str) -> bool {
        self.achievements.insert(achievement.to_owned())
    }

    /// Locks an achievement again, and starts its count over.
    pub fn forget(&mut self, achievement: &str) {
        self.achievements.remove(achievement);
        self.counts.remove(achievement);
    }

    /// One more toward an achievement that is counted up to `goal`. True when this one reached it.
    pub fn count(&mut self, achievement: &str, goal: u32) -> bool {
        let count = self.counts.entry(achievement.to_owned()).or_default();
        *count = (*count + 1).min(goal);
        *count == goal && self.unlock(achievement)
    }

    pub fn counted(&self, achievement: &str) -> u32 {
        self.counts.get(achievement).copied().unwrap_or(0)
    }

    pub fn unlocked(&self, achievement: &str) -> bool {
        self.achievements.contains(achievement)
    }

    /// A week stays locked until the week before it is finished.
    pub fn locked(&self, week: &Week) -> bool {
        !week.start_unlocked && !week.before.is_empty() && !self.completed.contains(&week.before)
    }

    pub fn to_json(&self) -> String {
        let songs: BTreeMap<&String, Value> = self.songs.iter().map(|(name, (score, accuracy))| (name, json!({ "score": score, "accuracy": accuracy }))).collect();
        serde_json::to_string_pretty(&json!({ "songs": songs, "weeks": self.weeks, "completed": self.completed, "achievements": self.achievements, "counts": self.counts, "egg": self.egg })).unwrap_or_default()
    }

    /// Anything missing or unreadable is left empty.
    pub fn from_json(text: &str) -> Save {
        let value: Value = serde_json::from_str(text).unwrap_or_default();
        let entries = |key: &str| value[key].as_object().cloned().unwrap_or_default().into_iter();
        let names = |key: &str| value[key].as_array().into_iter().flatten().filter_map(|name| name.as_str().map(str::to_owned)).collect();
        Save {
            songs: entries("songs").map(|(name, best)| (name, (best["score"].as_i64().unwrap_or(0), best["accuracy"].as_f64().unwrap_or(0.0)))).collect(),
            weeks: entries("weeks").filter_map(|(name, score)| Some((name, score.as_i64()?))).collect(),
            completed: names("completed"),
            achievements: names("achievements"),
            counts: entries("counts").filter_map(|(name, count)| Some((name, count.as_u64()? as u32))).collect(),
            egg: value["egg"].as_str().unwrap_or("").to_owned(),
        }
    }

    pub fn load(base: &Path) -> Save {
        Save::from_json(&std::fs::read_to_string(file(base)).unwrap_or_default())
    }

    /// Written beside the old file and moved over it, so a crash half way cannot leave a broken save.
    pub fn save(&self, base: &Path) {
        let part = file(base).with_extension("json.part");
        if let Err(error) = std::fs::write(&part, self.to_json()).and_then(|_| std::fs::rename(&part, file(base))) {
            eprintln!("Could not save the scores: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_best_and_unlocks_the_next_week() {
        let mut save = Save::default();
        save.record_song("Dad Battle", "Hard", 1000, 0.9);
        save.record_song("Dad Battle", "Hard", 800, 1.0);
        save.record_song("Dad Battle", "Normal", 500, 0.5);
        assert_eq!(save.song("Dad Battle", "Hard"), (1000, 0.9), "a lower score does not replace the best, however accurate");
        assert_eq!((save.song("dad-battle", "normal"), save.song("Fresh", "Hard")), ((500, 0.5), (0, 0.0)));

        let next = Week { before: "week1".to_owned(), start_unlocked: false, ..Default::default() };
        assert!(save.locked(&next));
        assert!(!save.locked(&Week { start_unlocked: true, ..next.clone() }) && !save.locked(&Week { before: String::new(), ..next.clone() }));
        save.record_week("week1", "Hard", 3000);
        save.record_week("week1", "Hard", 2000);
        assert!(!save.locked(&next));
        assert_eq!((save.week("week1", "Hard"), save.week("week1", "Easy")), (3000, 0));

        save.reset_song("Dad Battle", "Hard");
        save.reset_week("week1", "Hard");
        assert_eq!((save.song("Dad Battle", "Hard"), save.song("Dad Battle", "Normal"), save.week("week1", "Hard")), ((0, 0.0), (500, 0.5), 0));
        assert!(!save.locked(&next), "resetting a score does not lock anything again");
        assert!(save.unlock("ur_good") && !save.unlock("ur_good") && save.unlocked("ur_good") && !save.unlocked("hype"));
        save.unlock("hype");
        save.forget("hype");
        // counted up to: unlocked by the count that reaches the goal, and only by that one
        assert_eq!([save.count("roadkill", 3), save.count("roadkill", 3), save.count("roadkill", 3), save.count("roadkill", 3)], [false, false, true, false]);
        assert!(save.unlocked("roadkill") && save.counted("roadkill") == 3 && save.counted("hype") == 0);
        assert!(save.unlocked("ur_good") && !save.unlocked("hype"));
        assert_eq!(Save::from_json(&save.to_json()), save);
        assert_eq!(Save::from_json("not json"), Save::default());
    }
}
