//! Finds the songs of the base game and of every mod, through their week files.
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct SongEntry {
    pub title: String,
    pub difficulty: String,
    /// Mod folder name, or "" for the base game.
    pub source: String,
    pub chart: PathBuf,
    pub inst: PathBuf,
    /// Present when the song has a vocal track.
    pub voices: Option<PathBuf>,
}

/// Folder and file name a song title maps to.
pub fn song_path(name: &str) -> String {
    name.chars()
        .filter(|c| !".,'\"%?!".contains(*c))
        .map(|c| if "~&;:<>#".contains(c) || c.is_whitespace() { '-' } else { c })
        .collect::<String>()
        .trim()
        .to_lowercase()
}

/// Every playable song under `base` (the folder holding `assets` and `mods`), in week order.
pub fn discover(base: &Path) -> Vec<SongEntry> {
    // (name, weeks, data, songs)
    let mut roots: Vec<(String, PathBuf, PathBuf, PathBuf)> = Vec::new();
    let assets = base.join("assets");
    roots.push((String::new(), assets.join("shared/weeks"), assets.join("shared/data"), assets.join("songs")));
    // The repository keeps the base game's songs apart; a built game has them merged into the line above.
    let base_game = assets.join("base_game");
    roots.push((String::new(), base_game.join("shared/weeks"), base_game.join("shared/data"), base_game.join("songs")));
    for dir in sorted(&base.join("mods")) {
        if dir.is_dir() {
            let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
            roots.push((name, dir.join("weeks"), dir.join("data"), dir.join("songs")));
        }
    }

    let mut found = Vec::new();
    for (source, weeks, data, songs) in roots {
        for week_file in sorted(&weeks).into_iter().filter(|p| p.extension().is_some_and(|e| e == "json")) {
            let Some(week) = std::fs::read_to_string(&week_file).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { continue };
            let difficulties = match week["difficulties"].as_str().map(str::trim) {
                Some(list) if !list.is_empty() => list.split(',').map(|d| d.trim().to_owned()).collect(),
                _ => vec!["Easy".to_owned(), "Normal".to_owned(), "Hard".to_owned()],
            };
            for title in week["songs"].as_array().into_iter().flatten().filter_map(|s| s[0].as_str()) {
                let folder = song_path(title);
                let inst = songs.join(&folder).join("Inst.ogg");
                for difficulty in &difficulties {
                    let suffix = match song_path(difficulty).as_str() { "normal" => String::new(), d => format!("-{d}") };
                    let chart = data.join(&folder).join(format!("{folder}{suffix}.json"));
                    if chart.is_file() && inst.is_file() {
                        let voices = Some(songs.join(&folder).join("Voices.ogg")).filter(|p| p.is_file());
                        found.push(SongEntry { title: title.to_owned(), difficulty: difficulty.clone(), source: source.clone(), chart, inst: inst.clone(), voices });
                    }
                }
            }
        }
    }
    found
}

fn sorted(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    entries.sort();
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_paths() {
        assert_eq!(song_path("KINGDOM COME"), "kingdom-come");
        assert_eq!(song_path("Dad Battle"), "dad-battle");
        assert_eq!(song_path("M.I.L.F"), "milf");
        assert_eq!(song_path("Don't Stop!"), "dont-stop");
    }

    #[test]
    fn finds_base_game_and_mod_songs() {
        let repo = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        let songs = discover(repo);
        let fresh: Vec<&str> = songs.iter().filter(|s| s.title == "Fresh").map(|s| s.difficulty.as_str()).collect();
        assert_eq!(fresh, ["Easy", "Normal", "Hard"]);
        assert!(songs.iter().all(|s| s.chart.is_file() && s.inst.is_file()));

        // a mod with one song on a custom difficulty
        let dir = std::env::temp_dir().join(format!("funkin_core_content_{}", std::process::id()));
        let m = dir.join("mods/My Mod");
        for sub in ["weeks", "data/my-song", "songs/my-song"] { std::fs::create_dir_all(m.join(sub)).unwrap(); }
        std::fs::write(m.join("weeks/w.json"), r#"{"songs":[["My Song","dad",[0,0,0]]],"difficulties":"Practice"}"#).unwrap();
        std::fs::write(m.join("data/my-song/my-song-practice.json"), "{}").unwrap();
        std::fs::write(m.join("songs/my-song/Inst.ogg"), "").unwrap();
        let found = discover(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].title.as_str(), found[0].difficulty.as_str(), found[0].source.as_str(), &found[0].voices), ("My Song", "Practice", "My Mod", &None));
    }
}
