//! The fist fight of weekend 1: what the two fighters do for each note, hit or missed.
//! The tables are those of PicoBlazinHandler and DarnellBlazinHandler in the Haxe engine.

/// One fighter's answer to a note.
#[derive(Debug, PartialEq)]
pub struct Move {
    pub pose: String,
    /// He steps in front of the other for it.
    pub front: bool,
    /// How hard and for how long the screen shakes: share of the screen, seconds.
    pub shake: Option<(f32, f32)>,
}

fn act(pose: &str, front: bool, shake: Option<(f32, f32)>) -> Option<Move> {
    Some(Move { pose: pose.to_owned(), front, shake })
}

const HIT: Option<(f32, f32)> = Some((0.0025, 0.15));
const HARD: Option<(f32, f32)> = Some((0.005, 0.25));

/// What carries over from note to note.
#[derive(Default)]
pub struct Fight {
    /// Each fighter punches with one arm, then the other.
    player_left: bool,
    opponent_left: bool,
    /// The player fumbled the wind-up of his uppercut: his next note is a block, whatever it was.
    fumbled: bool,
}

impl Fight {
    /// The moves of the player's fighter and of the opponent for a note.
    /// `kind` is the note type without "weekend-1-" ("" for a plain note); `missed` also stands for notes on the opponent's side,
    /// `stray` for a press that hit nothing. `now` is the pose each is in.
    // shortcut: the two rare turns the Haxe engine adds when the player is nearly out (a badly timed hit answered by an uppercut, the punch that ends it) are left out.
    pub fn moves(&mut self, kind: &str, missed: bool, stray: bool, now: (&str, &str)) -> (Option<Move>, Option<Move>) {
        let punch = |left: &mut bool, high: bool| {
            *left = !*left;
            act(&format!("punch{}{}", if high { "High" } else { "Low" }, if *left { 1 } else { 2 }), true, None)
        };
        let back = |pose: &str, shake| act(pose, false, shake);
        if stray {
            // he swings at nothing, and the opponent ducks or blocks
            let dodge = crate::gfx::rand::gen_range(0, 2) == 0;
            return (punch(&mut self.player_left, true), if dodge { back("dodge", None) } else { back("block", Some((0.002, 0.1))) });
        }
        let low = kind.contains("low");
        let spin = kind.ends_with("spin");
        let family = ["punch", "block", "dodge", "hit"].into_iter().find(|family| kind.starts_with(family) && (low || spin || kind.contains("high"))).unwrap_or(kind);
        // being hit: spun round, low or high
        let struck = || back(if spin { "hitSpin" } else if low { "hitLow" } else { "hitHigh" }, HIT);
        let taunt = |pose: &str, was: &str, then: &str| if pose == was { back(then, None) } else { back("idle", None) };

        let player = if !missed && std::mem::take(&mut self.fumbled) {
            back("block", Some((0.002, 0.1)))
        } else if missed && matches!(now.1, "uppercut" | "uppercut-hold") {
            back("uppercutHit", HARD)
        } else if missed && self.fumbled {
            back("hitHigh", HIT)
        } else {
            match (family, missed) {
                ("punch", false) => punch(&mut self.player_left, !low),
                ("block", false) => back("block", Some((0.002, 0.1))),
                ("dodge", false) => back("dodge", None),
                ("punch" | "block" | "dodge" | "hit", _) => struck(),
                ("picouppercutprep", false) => act("uppercutPrep", true, None),
                ("picouppercutprep", true) => {
                    self.fumbled = true;
                    punch(&mut self.player_left, true)
                }
                // it only shakes the screen when it lands
                ("picouppercut", missed) => act("uppercut", true, HARD.filter(|_| !missed)),
                ("darnelluppercut", _) => back("uppercutHit", HARD),
                ("fakeout", false) => back("fakeout", None),
                ("fakeout", true) => back("hitHigh", HIT),
                ("taunt", _) => taunt(now.0, "fakeout", "taunt"),
                ("tauntforce", _) => back("taunt", None),
                ("darnelluppercutprep" | "idle" | "reversefakeout", _) => back("idle", None),
                _ => None,
            }
        };
        let opponent = if missed && now.1 == "uppercutPrep" {
            act("uppercut", true, None)
        } else {
            match (family, missed) {
                // the player's punch lands, is blocked or dodged, or spins him round
                ("punch", false) if kind.ends_with("blocked") => back("block", Some((0.002, 0.1))),
                ("punch", false) if kind.ends_with("dodged") => back("dodge", None),
                ("punch", false) => struck(),
                // otherwise he punches: at the guard, at the air, or home
                ("punch" | "block" | "dodge" | "hit", _) => punch(&mut self.opponent_left, !low),
                ("picouppercutprep", false) => None,
                ("picouppercutprep", true) => back("hitHigh", HIT),
                ("picouppercut", false) => back("uppercutHit", None),
                ("picouppercut", true) => back("dodge", None),
                ("darnelluppercutprep", _) => act("uppercutPrep", true, None),
                ("darnelluppercut", _) => act("uppercut", true, None),
                ("idle", _) => back("idle", None),
                ("fakeout", _) => back("cringe", None),
                ("taunt", _) => taunt(now.1, "cringe", "pissed"),
                ("tauntforce", _) => back("pissed", None),
                ("reversefakeout", _) => back("fakeout", None),
                _ => None,
            }
        };
        (player, opponent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poses(moves: (Option<Move>, Option<Move>)) -> (String, String) {
        (moves.0.map_or(String::new(), |m| m.pose), moves.1.map_or(String::new(), |m| m.pose))
    }

    #[test]
    fn each_note_is_an_exchange() {
        let mut fight = Fight::default();
        let idle = ("idle", "idle");
        // punches alternate arms; a blocked one is blocked, a missed one is answered
        assert_eq!(poses(fight.moves("punchhigh", false, false, idle)), ("punchHigh1".to_owned(), "hitHigh".to_owned()));
        assert_eq!(poses(fight.moves("punchlowblocked", false, false, idle)), ("punchLow2".to_owned(), "block".to_owned()));
        assert_eq!(poses(fight.moves("punchlowspin", true, false, idle)), ("hitSpin".to_owned(), "punchLow1".to_owned()));
        assert_eq!(poses(fight.moves("blockhigh", false, false, idle)), ("block".to_owned(), "punchHigh2".to_owned()));
        assert_eq!(poses(fight.moves("dodgelow", true, false, idle)), ("hitLow".to_owned(), "punchLow1".to_owned()));
        // who punches steps in front, and a hit shakes the screen
        let (player, opponent) = fight.moves("hithigh", false, false, idle);
        assert_eq!((player.unwrap(), opponent.unwrap().front), (Move { pose: "hitHigh".to_owned(), front: false, shake: HIT }, true));
        // a plain note moves nobody
        assert_eq!(fight.moves("", false, false, idle), (None, None));
    }

    #[test]
    fn uppercuts_and_taunts_depend_on_what_came_before() {
        let mut fight = Fight::default();
        // a fumbled wind-up: the next note hit is a block instead of the uppercut
        assert_eq!(poses(fight.moves("picouppercutprep", true, false, ("idle", "idle"))), ("punchHigh1".to_owned(), "hitHigh".to_owned()));
        assert_eq!(poses(fight.moves("picouppercut", false, false, ("punchHigh1", "hitHigh"))).0, "block");
        assert_eq!(poses(fight.moves("picouppercut", false, false, ("block", "idle"))), ("uppercut".to_owned(), "uppercutHit".to_owned()));
        // the opponent's uppercut lands on a miss made while he winds up, and then the player takes it
        assert_eq!(poses(fight.moves("blockhigh", true, false, ("idle", "uppercutPrep"))).1, "uppercut");
        assert_eq!(poses(fight.moves("blockhigh", true, false, ("idle", "uppercut"))).0, "uppercutHit");
        // a taunt only after a fake-out; the opponent only fumes after cringing
        assert_eq!(poses(fight.moves("taunt", false, false, ("fakeout", "cringe"))), ("taunt".to_owned(), "pissed".to_owned()));
        assert_eq!(poses(fight.moves("taunt", false, false, ("idle", "idle"))), ("idle".to_owned(), "idle".to_owned()));
    }
}
