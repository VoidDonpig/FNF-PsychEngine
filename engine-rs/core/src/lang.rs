//! Translations: `data/<language>.lang` files give the words of another language for the game's own, by key.
use std::collections::HashMap;
use std::sync::RwLock;

/// The language the game is written in; it has no file.
pub const ENGLISH: (&str, &str) = ("en-US", "English (US)");

#[derive(Debug, Default, PartialEq)]
pub struct Phrases(HashMap<String, String>);

/// A key as the files spell it: lower case, `_` for spaces, without punctuation.
fn key_of(text: &str) -> String {
    text.trim().replace(' ', "_").chars().filter(|c| !"~&\\/;:<>#.,'\"%?!".contains(*c)).collect::<String>().to_lowercase()
}

/// The name a language file gives its language: its first line, when that is no phrase.
pub fn name_of(file: &str) -> Option<&str> {
    file.lines().next().map(str::trim).filter(|line| !line.is_empty() && !line.contains(':'))
}

impl Phrases {
    /// The phrases of language files, lines of `key: "words"`; a later file's take the place of an earlier one's.
    pub fn parse<'a>(files: impl IntoIterator<Item = &'a str>) -> Phrases {
        let mut phrases = HashMap::new();
        for line in files.into_iter().flat_map(|file| file.lines().skip(name_of(file).is_some() as usize)).map(str::trim) {
            let Some((key, value)) = line.split_once(':').filter(|_| !line.starts_with("//")) else { continue };
            if let (Some(from), Some(to)) = (value.find('"'), value.rfind('"')) {
                if from < to {
                    phrases.insert(key.trim().to_lowercase(), value[from + 1..to].replace("\\n", "\n"));
                }
            }
        }
        Phrases(phrases)
    }

    /// The words for `key` (else `default`), with `{1}`, `{2}`... replaced by `values`.
    pub fn get(&self, key: &str, default: &str, values: &[&str]) -> String {
        let words = self.0.get(&key_of(key)).map_or(default, String::as_str);
        values.iter().enumerate().fold(words.to_owned(), |words, (index, value)| words.replace(&format!("{{{}}}", index + 1), value))
    }

    /// The file this language has in place of `relative` ("images/x.png"), when it names one.
    pub fn file(&self, relative: &str) -> Option<String> {
        let (stem, ending) = relative.rsplit_once('.')?;
        Some(format!("{}.{ending}", self.0.get(&stem.to_lowercase())?))
    }
}

/// The language the player chose. Empty: the game's own words.
static CHOSEN: RwLock<Option<Phrases>> = RwLock::new(None);

pub fn choose(phrases: Phrases) {
    *CHOSEN.write().unwrap() = Some(phrases).filter(|phrases| !phrases.0.is_empty());
}

/// `default` in the player's language; `key` names it in the language files.
pub fn phrase(key: &str, default: &str) -> String {
    phrase_with(key, default, &[])
}

pub fn phrase_with(key: &str, default: &str, values: &[&str]) -> String {
    CHOSEN.read().unwrap().as_ref().unwrap_or(&Phrases::default()).get(key, default, values)
}

pub fn file(relative: &str) -> Option<String> {
    CHOSEN.read().unwrap().as_ref()?.file(relative)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrases_are_found_by_key_and_filled_in() {
        let game = "Português (Brasil)\n\n// Gameplay\nscore_text: \"Pontos: {1} | Erros: {2}\"\nbotplay: \"Automático\"\nno_mods_installed: \"NENHUM\\nMOD\"\nimages/Menu_Tracks: \"images/pt-BR/Menu_Tracks\"\nbroken: nothing quoted\n";
        let of_a_mod = "botplay: \"Robô\"\n";
        assert_eq!((name_of(game), name_of(of_a_mod)), (Some("Português (Brasil)"), None));
        let phrases = Phrases::parse([game, of_a_mod]);
        assert_eq!(phrases.0.len(), 4);
        assert_eq!(phrases.get("score_text", "Score: {1} | Misses: {2}", &["350", "2"]), "Pontos: 350 | Erros: 2");
        // keys are written without capitals, spaces or punctuation; the mod's file has the last word
        assert_eq!(phrases.get("Botplay", "Botplay", &[]), "Robô");
        assert_eq!(phrases.get("no_mods_installed", "", &[]), "NENHUM\nMOD");
        // what the language does not have stays as it is
        assert_eq!(phrases.get("week_score", "WEEK SCORE: {1}", &["10"]), "WEEK SCORE: 10");
        assert_eq!(phrases.file("images/Menu_Tracks.png").as_deref(), Some("images/pt-BR/Menu_Tracks.png"));
        assert_eq!(phrases.file("images/other.png"), None);
    }
}
