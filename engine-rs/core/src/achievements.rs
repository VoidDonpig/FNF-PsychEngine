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

// shortcut: "Roadkill Enthusiast" (it counts the limo's henchmen) and "Engine Gal Pal" (the title screen's easter egg) join with those features.
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
    shown("oversinging", "Oversinging Much...?", "Sing for 10 seconds without going back to Idle."),
    shown("hype", "Hyperactive", "Finish a Song without going back to Idle."),
    shown("two_keys", "Just the Two of Us", "Finish a Song pressing only two keys."),
    shown("toastie", "Toaster Gamer", "Have you tried to run the game on a toaster?"),
    Achievement { id: "debugger", name: "Debugger", text: "Beat the \"Test\" Stage from the Chart Editor.", hidden: true },
];

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
}
