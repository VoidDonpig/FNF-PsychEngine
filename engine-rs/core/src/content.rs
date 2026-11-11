//! Finds the weeks and songs of the base game and of every mod, through their week files.
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct SongEntry {
    pub title: String,
    pub difficulty: String,
    /// Mod folder name, or "" for the base game.
    pub source: String,
    /// Index of its week in the list it was found through.
    pub week: usize,
    /// Health icon and colour it has in freeplay.
    pub icon: String,
    pub color: [u8; 3],
    pub chart: PathBuf,
    pub inst: PathBuf,
    /// The player's vocals; a song with one vocal file for both singers has it here.
    pub voices_player: Option<PathBuf>,
    pub voices_opponent: Option<PathBuf>,
}

/// Where a song's files are looked for: its mod first, then files shared by all mods, then the game.
pub struct Paths {
    roots: Vec<PathBuf>,
}

impl Paths {
    /// `directory` is the stage's own asset folder ("week1"), or "".
    pub fn new(base: &Path, source: &str, directory: &str) -> Paths {
        let assets = base.join("assets");
        let mut roots = Vec::new();
        if !source.is_empty() {
            roots.push(base.join("mods").join(source));
        }
        // mods that say they run globally lend their files to every song
        roots.extend(mods(base).into_iter().filter(|(name, on)| *on && name != source && runs_globally(base, name)).map(|(name, _)| base.join("mods").join(name)));
        // "secrets" holds the title screen's hidden dancers, "translations" what other languages bring
        roots.extend([base.join("mods"), assets.join("shared"), assets.join("base_game/shared"), assets.join("secrets"), assets.join("translations/shared")]);
        if !directory.is_empty() {
            roots.extend([assets.join(directory), assets.join("base_game").join(directory)]);
        }
        Paths { roots }
    }

    /// First existing file for a path such as "images/characters/BOYFRIEND.png".
    /// The file of this name, from the first folder that has it; the chosen language's own file in its place, when it has one.
    pub fn find(&self, relative: &str) -> Option<PathBuf> {
        let within = |relative: &str| self.roots.iter().map(|root| root.join(relative)).find(|path| path.is_file());
        crate::lang::file(relative).and_then(|own| within(&own)).or_else(|| within(relative))
    }

    /// Every file of this name, the one `find` gives last.
    pub fn find_all(&self, relative: &str) -> Vec<PathBuf> {
        self.roots.iter().rev().map(|root| root.join(relative)).filter(|path| path.is_file()).collect()
    }

    /// The folders looked in, the first to be asked first.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }
}

/// The languages there are files for under `roots` (`data/<file>.lang`), as (file, name), in the order of their names.
pub fn languages(roots: &[PathBuf]) -> Vec<(String, String)> {
    let mut found = vec![(crate::lang::ENGLISH.0.to_owned(), crate::lang::ENGLISH.1.to_owned())];
    for path in roots.iter().flat_map(|root| sorted(&root.join("data"))).filter(|path| path.extension().is_some_and(|ending| ending == "lang")) {
        let Some(file) = path.file_stem().and_then(|file| file.to_str()) else { continue };
        // a file without a name of its own leaves the naming to another file of that language
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let name = crate::lang::name_of(&text).unwrap_or(file).to_owned();
        match found.iter_mut().find(|(known, _)| known == file) {
            Some((known, shown)) if shown == known => *shown = name,
            Some(_) => {}
            None => found.push((file.to_owned(), name)),
        }
    }
    found.sort_by_key(|(_, name)| name.to_lowercase());
    found
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

/// Folders under `mods` that hold loose files for every mod, not a mod of their own.
const NOT_MODS: [&str; 15] = ["characters", "custom_events", "custom_notetypes", "data", "songs", "music", "sounds", "shaders", "videos", "images", "stages", "weeks", "fonts", "scripts", "achievements"];

/// The mods under `base` in the player's order, each with whether it is switched on.
/// `modsList.txt` keeps the order and the switches; folders it does not know yet come last, switched on.
pub fn mods(base: &Path) -> Vec<(String, bool)> {
    let is_mod = |name: &str| !name.is_empty() && !NOT_MODS.contains(&name.to_lowercase().as_str()) && base.join("mods").join(name).is_dir();
    let mut list: Vec<(String, bool)> = Vec::new();
    let listed = std::fs::read_to_string(base.join("modsList.txt")).unwrap_or_default();
    let listed = listed.lines().map(|line| line.trim().split_once('|').map_or((line.trim(), false), |(name, on)| (name, on == "1")));
    let folders = sorted(&base.join("mods"));
    let found = folders.iter().filter_map(|dir| Some((dir.file_name()?.to_str()?, true)));
    for (name, on) in listed.chain(found) {
        if is_mod(name) && !list.iter().any(|(known, _)| known == name) {
            list.push((name.to_owned(), on));
        }
    }
    list
}

pub fn save_mods(base: &Path, list: &[(String, bool)]) -> std::io::Result<()> {
    let lines: Vec<String> = list.iter().map(|(name, on)| format!("{name}|{}", *on as u8)).collect();
    std::fs::write(base.join("modsList.txt"), lines.join("\n"))
}

fn runs_globally(base: &Path, name: &str) -> bool {
    let pack = std::fs::read_to_string(base.join("mods").join(name).join("pack.json")).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok());
    pack.is_some_and(|pack| pack["runsGlobally"].as_bool().unwrap_or(false))
}

/// What a difficulty adds to a song's chart name and to the names scores are kept under.
pub fn suffix(difficulty: &str) -> String {
    match song_path(difficulty).as_str() { "normal" => String::new(), d => format!("-{d}") }
}

/// A week file: a row of the story menu and the songs it brings to freeplay.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Week {
    /// File name without ".json"; scores and unlocks are kept under it.
    pub file: String,
    /// Mod folder name, or "" for the base game.
    pub source: String,
    pub story_name: String,
    /// What the week is called ("Week 1").
    pub name: String,
    /// The week that has to be finished first, unless `start_unlocked`.
    pub before: String,
    pub start_unlocked: bool,
    pub hidden_until_unlocked: bool,
    pub hide_story: bool,
    pub hide_freeplay: bool,
    /// The three characters of the story menu ("" for none).
    pub characters: [String; 3],
    pub background: String,
    pub difficulties: Vec<String>,
    /// Title, health icon and menu colour of each song.
    pub songs: Vec<(String, String, [u8; 3])>,
    /// Folders its charts and its songs' audio are in.
    pub data: PathBuf,
    pub audio: PathBuf,
}

/// Every week under `base` (the folder holding `assets` and `mods`): the game's, then each mod's.
pub fn weeks(base: &Path) -> Vec<Week> {
    // (name, weeks, data, songs)
    let mut roots: Vec<(String, PathBuf, PathBuf, PathBuf)> = Vec::new();
    let assets = base.join("assets");
    roots.push((String::new(), assets.join("shared/weeks"), assets.join("shared/data"), assets.join("songs")));
    // The repository keeps the base game's songs apart; a built game has them merged into the line above.
    let base_game = assets.join("base_game");
    roots.push((String::new(), base_game.join("shared/weeks"), base_game.join("shared/data"), base_game.join("songs")));
    // loose files straight under "mods", then the mods that are switched on, in the player's order
    let loose = base.join("mods");
    roots.push((String::new(), loose.join("weeks"), loose.join("data"), loose.join("songs")));
    for (name, _) in mods(base).into_iter().filter(|(_, on)| *on) {
        let dir = loose.join(&name);
        roots.push((name, dir.join("weeks"), dir.join("data"), dir.join("songs")));
    }

    // weekList.txt names the weeks that come first, in its order; the game's list also covers the base_game folder
    let listed = |dir: &Path| std::fs::read_to_string(dir.join("weekList.txt")).unwrap_or_default().lines().map(|l| l.trim().to_owned()).collect::<Vec<_>>();
    let game_order = [listed(&roots[0].1), listed(&roots[1].1)].concat();
    let mut found = Vec::new();
    for (source, dir, data, audio) in roots {
        let order = if source.is_empty() { game_order.clone() } else { listed(&dir) };
        let name = |path: &Path| path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let mut files: Vec<PathBuf> = sorted(&dir).into_iter().filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
        files.sort_by_key(|path| order.iter().position(|listed| *listed == name(path)).unwrap_or(usize::MAX)); // stable: the rest stay alphabetical
        for path in files {
            let Some(week) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { continue };
            let text = |key: &str| week[key].as_str().unwrap_or_default().to_owned();
            let difficulties = match week["difficulties"].as_str().map(str::trim) {
                Some(list) if !list.is_empty() => list.split(',').map(|d| d.trim().to_owned()).collect(),
                _ => vec!["Easy".to_owned(), "Normal".to_owned(), "Hard".to_owned()],
            };
            let songs = week["songs"].as_array().into_iter().flatten().filter_map(|song| {
                let color = [0, 1, 2].map(|i| song[2][i].as_u64().map(|c| c.min(255) as u8));
                let color = if color.contains(&None) { [146, 113, 253] } else { color.map(Option::unwrap_or_default) };
                Some((song[0].as_str()?.to_owned(), song[1].as_str().unwrap_or("face").to_owned(), color))
            }).collect();
            found.push(Week {
                file: name(&path), source: source.clone(), story_name: text("storyName"), name: text("weekName"), before: text("weekBefore"),
                start_unlocked: week["startUnlocked"].as_bool().unwrap_or(true), hidden_until_unlocked: week["hiddenUntilUnlocked"].as_bool().unwrap_or(false),
                hide_story: week["hideStoryMode"].as_bool().unwrap_or(false), hide_freeplay: week["hideFreeplay"].as_bool().unwrap_or(false),
                characters: [0, 1, 2].map(|i| week["weekCharacters"][i].as_str().unwrap_or_default().to_owned()), background: text("weekBackground"),
                difficulties, songs, data: data.clone(), audio: audio.clone(),
            });
        }
    }
    found
}

/// Every playable song of `weeks`, in their order: one entry for each difficulty that has a chart.
pub fn songs(weeks: &[Week]) -> Vec<SongEntry> {
    let mut found = Vec::new();
    for (index, week) in weeks.iter().enumerate() {
        for (title, icon, color) in &week.songs {
            let folder = song_path(title);
            let inst = week.audio.join(&folder).join("Inst.ogg");
            for difficulty in &week.difficulties {
                let chart = week.data.join(&folder).join(format!("{folder}{}.json", suffix(difficulty)));
                if chart.is_file() && inst.is_file() {
                    let track = |name: &str| Some(week.audio.join(&folder).join(name)).filter(|p| p.is_file());
                    let voices_player = track("Voices-Player.ogg").or_else(|| track("Voices.ogg"));
                    found.push(SongEntry { title: title.clone(), difficulty: difficulty.clone(), source: week.source.clone(), week: index, icon: icon.clone(), color: *color, chart, inst: inst.clone(), voices_player, voices_opponent: track("Voices-Opponent.ogg") });
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
    fn languages_are_listed_by_name_with_the_games_own() {
        let dir = std::env::temp_dir().join(format!("funkin_core_languages_{}", std::process::id()));
        let (game, of_a_mod) = (dir.join("shared"), dir.join("mod"));
        for (folder, file, text) in [(&game, "pt-BR.lang", "Português (Brasil)\nbotplay: \"Automático\"\n"), (&of_a_mod, "pt-BR.lang", "botplay: \"Robô\"\n"), (&of_a_mod, "de.lang", "Deutsch\n")] {
            std::fs::create_dir_all(folder.join("data")).unwrap();
            std::fs::write(folder.join("data").join(file), text).unwrap();
        }
        let found = languages(&[of_a_mod, game, dir.join("missing")]);
        let _ = std::fs::remove_dir_all(&dir);
        // the mod's file has no name of its own: the game's file of that language names it
        let names: Vec<(&str, &str)> = found.iter().map(|(file, name)| (file.as_str(), name.as_str())).collect();
        assert_eq!(names, [("de", "Deutsch"), ("en-US", "English (US)"), ("pt-BR", "Português (Brasil)")]);
    }

    #[test]
    fn song_paths() {
        assert_eq!(song_path("KINGDOM COME"), "kingdom-come");
        assert_eq!(song_path("Dad Battle"), "dad-battle");
        assert_eq!(song_path("M.I.L.F"), "milf");
        assert_eq!(song_path("Don't Stop!"), "dont-stop");
    }

    #[test]
    fn paths_prefer_the_mod_and_reach_the_stage_folder() {
        let repo = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        let paths = Paths::new(repo, "", "week1");
        assert!(paths.find("images/characters/BOYFRIEND.png").unwrap().ends_with("assets/shared/images/characters/BOYFRIEND.png"));
        assert!(paths.find("images/stageback.png").unwrap().ends_with("assets/base_game/week1/images/stageback.png"));
        assert!(paths.find("characters/dad.json").is_some() && paths.find("images/nothing.png").is_none());
        assert!(Paths::new(repo, "", "").find("images/stageback.png").is_none());
    }

    #[test]
    fn finds_base_game_and_mod_songs() {
        let repo = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        let all = weeks(repo);
        assert_eq!(all.iter().map(|w| w.file.as_str()).collect::<Vec<_>>(), ["tutorial", "week1", "week2", "week3", "week4", "week5", "week6", "week7", "weekend1"]);
        assert_eq!((all[1].story_name.as_str(), all[1].before.as_str(), all[1].characters[0].as_str(), all[1].background.as_str()), ("Daddy Dearest", "tutorial", "dad", "stage"));
        let songs = songs(&all);
        assert_eq!((songs[0].icon.as_str(), songs[0].color, songs[0].week), ("gf", [165, 0, 77], 0));
        let fresh: Vec<&str> = songs.iter().filter(|s| s.title == "Fresh").map(|s| s.difficulty.as_str()).collect();
        assert_eq!(fresh, ["Easy", "Normal", "Hard"]);
        assert!(songs.iter().all(|s| s.chart.is_file() && s.inst.is_file()));

        // a mod with one song on a custom difficulty
        let dir = std::env::temp_dir().join(format!("funkin_core_content_{}", std::process::id()));
        let m = dir.join("mods/My Mod");
        for sub in ["weeks", "data/my-song", "songs/my-song"] { std::fs::create_dir_all(m.join(sub)).unwrap(); }
        std::fs::write(m.join("weeks/w.json"), r#"{"songs":[["My Song","dad",[0,0,0]]],"difficulties":"Practice"}"#).unwrap();
        std::fs::write(m.join("weeks/a.json"), r#"{"songs":[], "startUnlocked": false, "weekBefore": "w"}"#).unwrap();
        std::fs::write(m.join("weeks/weekList.txt"), "w\n").unwrap();
        std::fs::write(m.join("data/my-song/my-song-practice.json"), "{}").unwrap();
        std::fs::write(m.join("songs/my-song/Inst.ogg"), "").unwrap();
        let listed = weeks(&dir);
        let found = super::songs(&listed);

        // the mod list: known mods in their saved order, new folders after them, asset folders are no mods
        for folder in ["mods/A Mod", "mods/images", "mods/Global/images"] { std::fs::create_dir_all(dir.join(folder)).unwrap(); }
        std::fs::write(dir.join("mods/Global/pack.json"), r#"{"runsGlobally": true}"#).unwrap();
        std::fs::write(dir.join("mods/Global/images/shared.png"), "").unwrap();
        std::fs::write(dir.join("modsList.txt"), "My Mod|0\nGone|1\nGlobal|1").unwrap();
        let order = mods(&dir);
        let off = weeks(&dir);
        let lent = Paths::new(&dir, "A Mod", "").find("images/shared.png").is_some();
        save_mods(&dir, &order).unwrap();
        let saved = std::fs::read_to_string(dir.join("modsList.txt")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(order, [("My Mod".to_owned(), false), ("Global".to_owned(), true), ("A Mod".to_owned(), true)]);
        assert!(off.is_empty(), "a mod switched off brings no weeks");
        assert!(lent, "a global mod's files are found from another mod");
        assert_eq!(saved, "My Mod|0\nGlobal|1\nA Mod|1");

        assert_eq!(listed.iter().map(|w| w.file.as_str()).collect::<Vec<_>>(), ["w", "a"], "weekList.txt goes before the alphabet");
        assert!(!listed[1].start_unlocked && listed[0].start_unlocked);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].title.as_str(), found[0].difficulty.as_str(), found[0].source.as_str(), &found[0].voices_player), ("My Song", "Practice", "My Mod", &None));
    }
}
