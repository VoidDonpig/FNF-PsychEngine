//! One run of a song: which notes were hit, how well, and what that did to score and health.
use crate::chart::{Chart, NoteKind};

/// How strictly hits are judged, in milliseconds. The defaults are the engine's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    /// A hit is this rating up to so far from its note.
    pub sick: f64,
    pub good: f64,
    pub bad: f64,
    /// A note can be hit this long before or after its time (10 frames at 60 FPS).
    pub safe: f64,
    /// Hits are rated as if they came this much earlier (for players who press late throughout).
    pub rating_offset: f64,
    /// How fast the song plays (1 = as written). Hits are rated by real time, so a faster song is no stricter.
    pub rate: f64,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing { sick: 45.0, good: 90.0, bad: 135.0, safe: 10.0 / 60.0 * 1000.0, rating_offset: 0.0, rate: 1.0 }
    }
}
const HIT_HEALTH: f64 = 0.02;
const MISS_HEALTH: f64 = 0.1;
/// What a piece of a hurt hold takes when it is held through (the Haxe engine's missHealth for those).
const HURT_PIECE_HEALTH: f64 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rating { Sick, Good, Bad, Shit }

impl Rating {
    pub fn of(diff_ms: f64, timing: &Timing) -> Rating {
        match ((diff_ms - timing.rating_offset) / timing.rate).abs() {
            d if d <= timing.sick => Rating::Sick,
            d if d <= timing.good => Rating::Good,
            d if d <= timing.bad => Rating::Bad,
            _ => Rating::Shit,
        }
    }
    pub fn score(self) -> i64 { [350, 200, 100, 50][self as usize] }
    /// Share of a perfect hit this rating is worth in the accuracy.
    pub fn weight(self) -> f64 { [1.0, 0.67, 0.34, 0.0][self as usize] }
    pub fn name(self) -> &'static str { ["Sick!!", "Good", "Bad", "Shit"][self as usize] }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteState { Pending, Held, Done, Missed }

/// One 16th of a hold. The Haxe engine cuts every hold into such pieces (see `Chart::hold_pieces`), and each is hit by
/// the key being down from its time until the safe zone after it, or missed. For a piece `Done` means hit, and `Missed`
/// that it was missed, lost with the rest of its hold, or let pass for nothing (a hold set aside, a hurt one not touched).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub time: f64,
    pub state: NoteState,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// The player hit chart note `note`, `diff_ms` after (+) or before (-) its time.
    Hit { lane: usize, rating: Rating, diff_ms: f64, note: usize },
    /// A note got away, or (without `note`) a press hit nothing; `piece`: what got away was a piece of the note's hold;
    /// `combo`: the combo it broke.
    Miss { lane: usize, note: Option<usize>, piece: bool, combo: u32 },
    OpponentHit { lane: usize, note: usize },
    /// A piece of chart note `note`'s hold was hit, by the player (or botplay) or by the opponent. Pieces are not rated.
    PieceHit { lane: usize, note: usize, player: bool },
}

pub struct Play {
    pub chart: Chart,
    /// One entry per chart note: its head's state. A hold whose head was hit is `Held` until all its pieces are over.
    pub state: Vec<NoteState>,
    /// Notes set aside by scripts (ignoreNote): not played by the bot or the opponent, not missed when let go.
    pub ignored: Vec<bool>,
    pub held: [bool; 4],
    pub botplay: bool,
    /// Off: a press with nothing to hit counts as a miss.
    pub ghost_tapping: bool,
    pub timing: Timing,
    /// Multipliers on the health a hit gives and a miss takes.
    pub health_gain: f64,
    pub health_loss: f64,
    /// One miss empties the health bar.
    pub instakill: bool,
    /// A hold is one note (the Haxe engine's "Sustains as One Note"): its pieces can be hit only after its head, give no
    /// health, and the first one missed loses the rest as one miss. Off, every piece is hit or missed for itself, gives
    /// health when hit, and can be caught even when the head was missed.
    pub one_note_holds: bool,
    /// How many notes got each rating: sick, good, bad, shit.
    pub ratings: [u32; 4],
    pub score: i64,
    pub misses: u32,
    pub combo: u32,
    /// 0 (dead) to 2.
    pub health: f64,
    rated: u32,
    weight: f64,
    /// Every note before this one is finished.
    first_open: usize,
    /// Every hold's pieces, note after note: note `i`'s are `pieces[piece_start[i]..piece_start[i + 1]]`.
    pieces: Vec<Piece>,
    piece_start: Vec<usize>,
    /// When each lane's key last went down: a key held hits the pieces whose safe zone it has been down in.
    pressed_at: [f64; 4],
}

/// What the Haxe engine calls an accuracy (0 to 1).
pub fn rating_name(accuracy: f64) -> &'static str {
    const NAMES: [(&str, f64); 9] = [("You Suck!", 0.2), ("Shit", 0.4), ("Bad", 0.5), ("Bruh", 0.6), ("Meh", 0.69), ("Nice", 0.7), ("Good", 0.8), ("Great", 0.9), ("Sick!", 1.0)];
    NAMES.iter().find(|(_, below)| accuracy < *below).map_or("Perfect!!", |(name, _)| name)
}

impl Play {
    pub fn new(chart: Chart, botplay: bool) -> Play {
        let state = vec![NoteState::Pending; chart.notes.len()];
        let (mut pieces, mut piece_start) = (Vec::new(), vec![0]);
        for note in &chart.notes {
            pieces.extend(chart.hold_pieces(note).into_iter().map(|time| Piece { time, state: NoteState::Pending }));
            piece_start.push(pieces.len());
        }
        Play { ignored: vec![false; state.len()], chart, state, held: [false; 4], botplay, ghost_tapping: true, timing: Timing::default(), health_gain: 1.0, health_loss: 1.0, instakill: false, one_note_holds: true, ratings: [0; 4], score: 0, misses: 0, combo: 0, health: 1.0, rated: 0, weight: 0.0, first_open: 0, pieces, piece_start, pressed_at: [f64::MIN; 4] }
    }

    /// Accuracy from 0 to 1, once anything was rated.
    /// How clean the run is so far: only sicks, goods at worst, any hit, under ten misses, or just cleared.
    pub fn full_combo(&self) -> &'static str {
        match (self.misses, self.ratings) {
            (0, [_, _, bad, shit]) if bad + shit > 0 => "FC",
            (0, [_, good, ..]) if good > 0 => "GFC",
            (0, [sick, ..]) if sick > 0 => "SFC",
            (0, _) => "",
            (1..=9, _) => "SDCB",
            _ => "Clear",
        }
    }

    pub fn accuracy(&self) -> Option<f64> {
        (self.rated > 0).then(|| self.weight / self.rated as f64)
    }

    pub fn finished(&self) -> bool {
        self.first_open == self.state.len()
    }

    /// The first note still in play (every one before it is done or missed).
    pub fn first_open(&self) -> usize {
        self.first_open
    }

    /// The pieces of chart note `note`'s hold (none for a tap).
    pub fn pieces(&self, note: usize) -> &[Piece] {
        &self.pieces[self.piece_start[note]..self.piece_start[note + 1]]
    }

    fn pieces_mut(&mut self, note: usize) -> &mut [Piece] {
        &mut self.pieces[self.piece_start[note]..self.piece_start[note + 1]]
    }

    /// A note is over when its head and every piece of its hold are.
    fn over(&self, note: usize) -> bool {
        matches!(self.state[note], NoteState::Done | NoteState::Missed) && self.pieces(note).iter().all(|piece| piece.state != NoteState::Pending)
    }

    /// What is left of a hold is lost (or let pass for nothing): none of it can be hit any more.
    fn lose_pieces(&mut self, note: usize) {
        for piece in self.pieces_mut(note).iter_mut().filter(|piece| piece.state == NoteState::Pending) {
            piece.state = NoteState::Missed;
        }
    }

    /// Goes on from `pos` (ms), back or forward: every note before it is out of play and counts for nothing, every note
    /// from it on is to be played (again).
    pub fn skip_to(&mut self, pos: f64) {
        for i in 0..self.state.len() {
            let before = self.chart.notes[i].time < pos;
            self.state[i] = if before { NoteState::Done } else { NoteState::Pending };
            for piece in self.pieces_mut(i) {
                piece.state = if before { NoteState::Missed } else { NoteState::Pending };
            }
        }
        self.held = [false; 4];
        self.first_open = (0..self.state.len()).find(|&i| !self.over(i)).unwrap_or(self.state.len());
    }

    /// A key went down at song position `pos` (ms).
    pub fn press(&mut self, lane: usize, pos: f64) -> Option<Event> {
        self.held[lane] = true;
        self.pressed_at[lane] = pos;
        let found = (self.first_open..self.chart.notes.len())
            .take_while(|&i| self.chart.notes[i].time <= pos + self.timing.safe)
            .find(|&i| {
                let note = &self.chart.notes[i];
                note.player && note.lane == lane && self.state[i] == NoteState::Pending && (note.time - pos).abs() <= self.timing.safe
            });
        let Some(index) = found else {
            if self.ghost_tapping || self.botplay {
                return None;
            }
            return Some(self.miss(lane, None, false));
        };
        let (time, hurt, hold) = (self.chart.notes[index].time, self.chart.notes[index].kind == NoteKind::Hurt, !self.pieces(index).is_empty());
        if hurt {
            self.state[index] = NoteState::Done;
            if self.one_note_holds && hold {
                // The Haxe engine's miss of a head with a tail takes the tail and returns before it counts anything
                // (noteMissCommon), so a hurt hold played as one note costs nothing when pressed.
                self.lose_pieces(index);
                return None;
            }
            return Some(self.miss(lane, Some(index), false));
        }
        self.state[index] = if hold { NoteState::Held } else { NoteState::Done };
        Some(self.hit(lane, pos - time, index))
    }

    /// A key went up at song position `pos` (ms). It was down until then, so it still hits the pieces that came before,
    /// however long ago the last frame was (the Haxe engine's keyReleaseTime).
    pub fn release(&mut self, lane: usize, pos: f64) -> Vec<Event> {
        let mut events = Vec::new();
        if self.held[lane] && !self.botplay {
            for i in self.first_open..self.chart.notes.len() {
                if self.chart.notes[i].time > pos {
                    break;
                }
                if self.chart.notes[i].player && self.chart.notes[i].lane == lane && self.ignored.get(i) != Some(&true) {
                    self.judge_pieces(i, pos, &mut events);
                }
            }
        }
        self.held[lane] = false;
        events
    }

    /// The player's pieces of note `i` up to `pos`: hit where the key was down in their safe zone, missed where that
    /// zone has passed without it. A piece can be hit from its own time on, never before (earlyHitMult = 0); as one
    /// note, only once its head was hit.
    fn judge_pieces(&mut self, i: usize, pos: f64, events: &mut Vec<Event>) {
        if self.one_note_holds && self.state[i] != NoteState::Held {
            return;
        }
        let (lane, hurt) = (self.chart.notes[i].lane, self.chart.notes[i].kind == NoteKind::Hurt);
        for k in self.piece_start[i]..self.piece_start[i + 1] {
            let Piece { time, state } = self.pieces[k];
            if state != NoteState::Pending {
                continue;
            }
            if time > pos {
                break;
            }
            if self.held[lane] && self.pressed_at[lane] < time + self.timing.safe {
                self.pieces[k].state = NoteState::Done;
                if hurt {
                    // every piece of a hurt hold held through hurts, as a miss of its own
                    events.push(self.miss(lane, Some(i), true));
                } else {
                    // no score, combo or rating: only health, and that only when pieces are notes of their own
                    if !self.one_note_holds {
                        self.health = (self.health + HIT_HEALTH * self.health_gain).min(2.0);
                    }
                    events.push(Event::PieceHit { lane, note: i, player: true });
                }
            } else if pos > time + self.timing.safe {
                self.pieces[k].state = NoteState::Missed;
                if hurt {
                    continue;
                }
                events.push(self.miss(lane, Some(i), true));
                if self.one_note_holds {
                    // the rest of the hold goes with it, in the same miss
                    self.lose_pieces(i);
                    break;
                }
            } else {
                break; // in its safe zone with the key up: it may still go down
            }
        }
    }

    /// Advances to song position `pos` (ms): opponent and botplay hits, notes that got away, holds' pieces.
    pub fn update(&mut self, pos: f64) -> Vec<Event> {
        let mut events = Vec::new();
        for i in self.first_open..self.chart.notes.len() {
            let note = self.chart.notes[i].clone();
            if note.time > pos + self.timing.safe {
                break;
            }
            let auto = !note.player || self.botplay;
            let hurt = note.kind == NoteKind::Hurt;
            let hold = !self.pieces(i).is_empty();
            // a note a script set aside (ignoreNote): nobody plays it for you, and letting it go costs nothing
            let ignored = self.ignored.get(i) == Some(&true);
            match self.state[i] {
                NoteState::Pending if auto && note.time <= pos => {
                    if (note.player && hurt) || ignored {
                        self.state[i] = NoteState::Done;
                        self.lose_pieces(i);
                        continue;
                    }
                    self.state[i] = if hold { NoteState::Held } else { NoteState::Done };
                    // Botplay is rated at the note's own time, not at the frame that noticed it.
                    events.push(if note.player { self.hit(note.lane, 0.0, i) } else { Event::OpponentHit { lane: note.lane, note: i } });
                }
                NoteState::Pending if !auto && pos > note.time + self.timing.safe => {
                    if hurt || ignored {
                        self.state[i] = NoteState::Done;
                        // a hurt hold's pieces can still hurt one by one, unless it is played as one note
                        if ignored || self.one_note_holds {
                            self.lose_pieces(i);
                        }
                    } else {
                        self.state[i] = NoteState::Missed;
                        // As one note its tail goes with it, in one miss of a plain note's damage: the Haxe engine
                        // multiplies the damage by the tail's length, but then returns before taking it (noteMissCommon),
                        // and the miss it counts is its first piece's, which gets away in the same frame.
                        if self.one_note_holds {
                            self.lose_pieces(i);
                        }
                        events.push(self.miss(note.lane, Some(i), false));
                    }
                }
                _ => {}
            }
            if !hold {
                continue;
            }
            if auto {
                // botplay and the opponent hit every piece at its time, once the head is hit
                if self.state[i] == NoteState::Held {
                    for k in self.piece_start[i]..self.piece_start[i + 1] {
                        if self.pieces[k].state == NoteState::Pending && self.pieces[k].time <= pos {
                            self.pieces[k].state = NoteState::Done;
                            if note.player && !self.one_note_holds {
                                self.health = (self.health + HIT_HEALTH * self.health_gain).min(2.0);
                            }
                            events.push(Event::PieceHit { lane: note.lane, note: i, player: note.player });
                        }
                    }
                }
            } else if !ignored {
                self.judge_pieces(i, pos, &mut events);
            }
            if self.state[i] == NoteState::Held && self.pieces(i).iter().all(|piece| piece.state != NoteState::Pending) {
                self.state[i] = NoteState::Done;
            }
        }
        while self.first_open < self.state.len() && self.over(self.first_open) {
            self.first_open += 1;
        }
        events
    }

    fn hit(&mut self, lane: usize, diff_ms: f64, note: usize) -> Event {
        let rating = Rating::of(diff_ms, &self.timing);
        self.ratings[rating as usize] += 1;
        self.score += rating.score();
        self.combo += 1;
        self.rated += 1;
        self.weight += rating.weight();
        self.health = (self.health + HIT_HEALTH * self.health_gain).min(2.0);
        Event::Hit { lane, rating, diff_ms, note }
    }

    /// `piece`: what got away was a piece of the note's hold, not its head.
    fn miss(&mut self, lane: usize, note: Option<usize>, piece: bool) -> Event {
        // every miss costs ten points, as in the Haxe engine
        self.score -= 10;
        self.misses += 1;
        let combo = std::mem::take(&mut self.combo);
        self.rated += 1;
        let kind = note.map(|index| self.chart.notes[index].kind);
        // a stray press costs half of what a missed note does, a hurt hold's piece more
        self.health -= self.health_loss * match note {
            None => MISS_HEALTH * 0.5,
            Some(_) if piece && kind == Some(NoteKind::Hurt) => HURT_PIECE_HEALTH,
            Some(_) => MISS_HEALTH,
        };
        // the can that was not shot down goes off in the player's face
        if !piece && kind == Some(NoteKind::FireGun) {
            self.health -= 0.4;
        }
        if self.health < 1e-9 || self.instakill {
            self.health = 0.0; // ten misses from full must empty the bar, whatever the rounding says
        }
        Event::Miss { lane, note, piece, combo }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::Note;

    const SAFE_MS: f64 = 10.0 / 60.0 * 1000.0;

    fn play(notes: &[(f64, usize, bool, f64)], botplay: bool) -> Play {
        let notes = notes.iter().map(|&(time, lane, player, sustain)| Note { time, lane, player, sustain, kind: NoteKind::Normal }).collect();
        Play::new(Chart { bpm: 120.0, speed: 1.0, notes, ..Default::default() }, botplay)
    }

    /// One hold of a beat at 120 BPM from 1000 ms: four pieces, at 1000, 1125, 1250 and 1375.
    fn one_beat_hold(one_note: bool) -> Play {
        let mut p = play(&[(1000.0, 0, true, 500.0)], false);
        p.one_note_holds = one_note;
        p
    }

    fn states(p: &Play, note: usize) -> Vec<NoteState> {
        p.pieces(note).iter().map(|piece| piece.state).collect()
    }

    #[test]
    fn a_run_gets_its_name_and_its_grade() {
        assert_eq!([0.0, 0.19, 0.2, 0.69, 0.7, 0.95, 1.0].map(rating_name), ["You Suck!", "You Suck!", "Shit", "Nice", "Good", "Sick!", "Perfect!!"]);
        let mut p = play(&[(0.0, 0, true, 0.0), (500.0, 1, true, 0.0), (1000.0, 2, true, 0.0), (5000.0, 3, true, 0.0)], false);
        assert_eq!(p.full_combo(), "");
        p.press(0, 0.0);
        assert_eq!((p.full_combo(), p.ratings), ("SFC", [1, 0, 0, 0]));
        p.press(1, 500.0 + 60.0);
        assert_eq!(p.full_combo(), "GFC");
        p.press(2, 1000.0 + 120.0);
        assert_eq!(p.full_combo(), "FC");
        p.update(6000.0);
        assert_eq!((p.full_combo(), p.misses), ("SDCB", 1));
        p.misses = 10;
        assert_eq!(p.full_combo(), "Clear");
    }

    #[test]
    fn a_hold_is_cut_into_16ths_at_the_tempo_where_it_starts() {
        let note = |time, sustain| Note { time, lane: 0, player: true, sustain, kind: NoteKind::Normal };
        // 120 BPM (16ths of 125 ms) up to 2000 ms, 240 BPM (62.5 ms) from there
        let chart = Chart { bpm: 120.0, tempo: vec![(0.0, 0.0, 120.0), (2000.0, 4.0, 240.0)], ..Default::default() };
        assert_eq!(chart.hold_pieces(&note(1000.0, 500.0)), [1000.0, 1125.0, 1250.0, 1375.0]);
        // rounded: 4.48 sixteenths are four pieces, 4.56 five
        assert_eq!([560.0, 570.0].map(|sustain| chart.hold_pieces(&note(1000.0, sustain)).len()), [4, 5]);
        assert_eq!(chart.hold_pieces(&note(2100.0, 250.0)), [2100.0, 2162.5, 2225.0, 2287.5]);
        // under half a 16th there is no hold at all: the note plays as a tap
        assert!(chart.hold_pieces(&note(1000.0, 60.0)).is_empty() && chart.hold_pieces(&note(1000.0, f64::NAN)).is_empty());
        let mut p = play(&[(1000.0, 0, true, 60.0)], false);
        p.press(0, 1000.0);
        assert_eq!((p.state[0], p.finished()), (NoteState::Done, false));
        p.update(1000.0);
        assert!(p.finished());
    }

    #[test]
    fn letting_go_and_pressing_again_within_the_safe_zone_keeps_the_hold() {
        for one_note in [true, false] {
            let mut p = one_beat_hold(one_note);
            p.press(0, 1000.0);
            p.update(1010.0);
            assert!(p.release(0, 1020.0).is_empty());
            // the piece at 1125 can be hit until 1125 + 166 ms: pressed again at 1250, it is
            assert!(p.update(1240.0).is_empty());
            p.press(0, 1250.0);
            assert_eq!(p.update(1260.0), [Event::PieceHit { lane: 0, note: 0, player: true }; 2]);
            p.update(1400.0);
            assert_eq!((p.misses, p.finished(), states(&p, 0)), (0, true, vec![NoteState::Done; 4]), "one note: {one_note}");
        }
    }

    #[test]
    fn a_key_let_go_between_two_frames_was_down_for_the_pieces_before() {
        let mut p = one_beat_hold(true);
        p.press(0, 1000.0);
        p.update(1100.0);
        // the last frame saw the song at 1100; the key went up at 1130, after the piece at 1125
        assert_eq!(p.release(0, 1130.0), [Event::PieceHit { lane: 0, note: 0, player: true }]);
        assert!(p.update(1300.0).is_empty(), "the piece at 1250 can still be pressed for");
        assert_eq!(p.update(1420.0), [Event::Miss { lane: 0, note: Some(0), piece: true, combo: 1 }], "and was not");
    }

    #[test]
    fn as_one_note_a_missed_head_is_one_miss_and_takes_its_tail() {
        let mut p = one_beat_hold(true);
        assert_eq!(p.update(1200.0), [Event::Miss { lane: 0, note: Some(0), piece: false, combo: 0 }]);
        assert_eq!(states(&p, 0), [NoteState::Missed; 4]);
        // nothing of it can be caught any more, and it costs no more than a tap
        assert_eq!(p.press(0, 1200.0), None);
        assert!(p.update(1600.0).is_empty());
        assert_eq!((p.misses, p.finished()), (1, true));
        assert!((p.health - 0.9).abs() < 1e-9);
    }

    #[test]
    fn as_one_note_a_piece_missed_mid_hold_is_one_miss_and_the_rest_is_gone() {
        let mut p = one_beat_hold(true);
        p.press(0, 1000.0);
        p.update(1100.0);
        p.release(0, 1110.0);
        // the piece at 1125 got away at 1291: one miss, and the pieces after it go with it
        assert_eq!(p.update(1300.0), [Event::Miss { lane: 0, note: Some(0), piece: true, combo: 1 }]);
        assert_eq!(states(&p, 0), [NoteState::Done, NoteState::Missed, NoteState::Missed, NoteState::Missed]);
        p.press(0, 1310.0);
        assert!(p.update(1600.0).is_empty());
        assert_eq!((p.misses, p.finished()), (1, true));
        // the head's health, no piece's, and one miss
        assert!((p.health - (1.0 + 0.02 - 0.1)).abs() < 1e-9);
    }

    #[test]
    fn piece_by_piece_every_piece_gives_health_or_is_a_miss_of_its_own() {
        let mut p = one_beat_hold(false);
        p.press(0, 1000.0);
        p.update(1000.0);
        assert!((p.health - 1.04).abs() < 1e-9, "the head and the first piece");
        p.release(0, 1010.0);
        // the pieces at 1125 and 1250 get away, each a miss
        assert_eq!(p.update(1500.0).len(), 2);
        // the one at 1375 is caught by pressing again in its safe zone
        assert_eq!(p.press(0, 1500.0), None);
        assert_eq!(p.update(1510.0), [Event::PieceHit { lane: 0, note: 0, player: true }]);
        assert_eq!((p.misses, p.finished()), (2, true));
        assert!((p.health - (1.04 - 0.2 + 0.02)).abs() < 1e-9);
    }

    #[test]
    fn piece_by_piece_a_hold_can_be_caught_after_its_head_was_missed() {
        let mut p = one_beat_hold(false);
        // the head, and the first piece with it, got away
        assert_eq!(p.update(1200.0).len(), 2);
        p.press(0, 1200.0);
        p.update(1600.0);
        assert_eq!(states(&p, 0), [NoteState::Missed, NoteState::Done, NoteState::Done, NoteState::Done]);
        assert_eq!((p.state[0], p.misses, p.finished()), (NoteState::Missed, 2, true));
    }

    #[test]
    fn hits_on_pieces_leave_score_combo_and_accuracy_alone() {
        for (one_note, health) in [(true, 1.02), (false, 1.02 + 4.0 * 0.02)] {
            let mut p = one_beat_hold(one_note);
            p.press(0, 1000.0);
            assert_eq!(p.update(2000.0).len(), 4);
            assert_eq!((p.score, p.combo, p.ratings, p.accuracy(), p.full_combo()), (350, 1, [1, 0, 0, 0], Some(1.0), "SFC"));
            assert!((p.health - health).abs() < 1e-9, "one note: {one_note}");
        }
    }

    #[test]
    fn botplay_and_the_opponent_hold_through_with_no_miss() {
        for one_note in [true, false] {
            let mut p = play(&[(1000.0, 0, false, 1000.0), (1000.0, 2, true, 1000.0), (2500.0, 3, true, 0.0)], true);
            p.one_note_holds = one_note;
            let mut events = Vec::new();
            for frame in 0..200 {
                events.extend(p.update(frame as f64 * 16.0));
            }
            let pieces = |player| events.iter().filter(|e| matches!(e, Event::PieceHit { player: p, .. } if *p == player)).count();
            assert_eq!((pieces(false), pieces(true), p.misses, p.finished(), p.combo), (8, 8, 0, true, 2));
        }
    }

    #[test]
    fn windows() {
        let of = |diff| Rating::of(diff, &Timing::default());
        assert_eq!([of(-45.0), of(45.1), of(90.0), of(135.0), of(-140.0)], [Rating::Sick, Rating::Good, Rating::Good, Rating::Bad, Rating::Shit]);
        // narrower windows, and a player who always presses 20 ms late
        // at double speed 60 ms of song pass in 30 ms of the player's time
        assert_eq!(Rating::of(60.0, &Timing { rate: 2.0, ..Timing::default() }), Rating::Sick);
        let strict = Timing { sick: 20.0, rating_offset: 20.0, ..Timing::default() };
        assert_eq!([Rating::of(40.0, &strict), Rating::of(0.0, &strict), Rating::of(41.0, &strict)], [Rating::Sick, Rating::Sick, Rating::Good]);
    }

    #[test]
    fn press_takes_the_earliest_note_in_reach() {
        let mut p = play(&[(1000.0, 0, true, 0.0), (1100.0, 0, true, 0.0), (1000.0, 1, true, 0.0)], false);
        assert_eq!(p.press(0, 1030.0), Some(Event::Hit { lane: 0, rating: Rating::Sick, diff_ms: 30.0, note: 0 }));
        assert_eq!(p.press(0, 1030.0), Some(Event::Hit { lane: 0, rating: Rating::Good, diff_ms: -70.0, note: 1 }));
        assert_eq!(p.press(0, 1030.0), None, "nothing left on this lane: a ghost tap");
        assert_eq!((p.score, p.combo, p.misses), (550, 2, 0));
        assert_eq!(p.accuracy(), Some((1.0 + 0.67) / 2.0));
    }

    #[test]
    fn out_of_reach_is_a_ghost_tap_then_a_miss() {
        let mut p = play(&[(1000.0, 2, true, 0.0)], false);
        assert_eq!(p.press(2, 1000.0 - SAFE_MS - 1.0), None);
        assert_eq!(p.update(1000.0 + SAFE_MS), vec![]);
        assert_eq!(p.update(1000.0 + SAFE_MS + 1.0), vec![Event::Miss { lane: 2, note: Some(0), piece: false, combo: 0 }]);
        // a note that got away costs ten points, as a stray press does
        assert_eq!((p.misses, p.combo, p.accuracy(), p.finished(), p.score), (1, 0, Some(0.0), true, -10));
        assert!(p.health < 1.0);
    }

    #[test]
    fn dropping_a_hold_is_one_miss_and_holding_it_is_none() {
        // a hold of two beats at 120 BPM: eight pieces, the last at 875
        let mut held = play(&[(0.0, 3, true, 1000.0)], false);
        held.press(3, 0.0);
        assert_eq!(held.update(500.0).len(), 5);
        held.release(3, 950.0); // after the last piece
        assert!(held.update(1100.0).is_empty());
        assert!(held.finished() && held.misses == 0);

        let mut dropped = play(&[(0.0, 3, true, 1000.0)], false);
        dropped.press(3, 0.0);
        dropped.release(3, 0.0);
        assert_eq!(dropped.update(300.0), vec![Event::Miss { lane: 3, note: Some(0), piece: true, combo: 1 }]);
        assert_eq!(dropped.update(600.0), vec![], "only once");

        // let go in the hold's last 16th, it was held through: its last piece came before
        for (let_go, misses) in [(880.0, 0), (860.0, 1)] {
            let mut hold = play(&[(0.0, 3, true, 1000.0)], false);
            hold.press(3, 0.0);
            hold.update(let_go - 10.0);
            hold.release(3, let_go);
            hold.update(let_go);
            hold.update(1100.0);
            assert_eq!(hold.misses, misses, "let go at {let_go}");
        }
    }

    #[test]
    fn a_note_set_aside_is_neither_played_for_you_nor_missed() {
        let mut p = play(&[(100.0, 0, false, 0.0), (200.0, 1, true, 500.0), (300.0, 2, true, 0.0)], false);
        (p.ignored[0], p.ignored[1]) = (true, true);
        // the opponent leaves it, and the player lets it go for nothing, hold and all; a note not set aside is missed
        assert_eq!(p.update(1000.0), vec![Event::Miss { lane: 2, note: Some(2), piece: false, combo: 0 }]);
        assert_eq!((p.misses, p.finished()), (1, true));
    }

    #[test]
    fn botplay_and_opponent_play_themselves() {
        // the player's hold is 2.4 sixteenths long: two pieces, at 200 and 325
        let mut p = play(&[(100.0, 0, false, 0.0), (200.0, 1, true, 300.0)], true);
        assert_eq!(p.update(99.0), vec![]);
        assert_eq!(p.update(250.0), vec![Event::OpponentHit { lane: 0, note: 0 }, Event::Hit { lane: 1, rating: Rating::Sick, diff_ms: 0.0, note: 1 }, Event::PieceHit { lane: 1, note: 1, player: true }]);
        assert!(!p.finished());
        p.update(500.0);
        assert!(p.finished() && p.accuracy() == Some(1.0) && p.score == 350);
    }

    #[test]
    fn stray_presses_cost_only_without_ghost_tapping() {
        let mut p = play(&[(5000.0, 0, true, 0.0)], false);
        assert_eq!(p.press(0, 100.0), None);
        p.ghost_tapping = false;
        assert_eq!(p.press(0, 100.0), Some(Event::Miss { lane: 0, note: None, piece: false, combo: 0 }));
        assert_eq!((p.misses, p.score), (1, -10));
        assert!((p.health - 0.95).abs() < 1e-9);
    }

    #[test]
    fn health_changers_scale_hits_and_misses_and_instakill_ends_it() {
        let mut p = play(&[(1000.0, 0, true, 0.0), (2000.0, 1, true, 0.0), (3000.0, 2, true, 0.0)], false);
        (p.health_gain, p.health_loss) = (2.0, 3.0);
        p.press(0, 1000.0);
        assert!((p.health - 1.04).abs() < 1e-9);
        p.update(2500.0);
        assert!((p.health - 0.74).abs() < 1e-9);
        p.instakill = true;
        p.update(3500.0);
        assert_eq!(p.health, 0.0);
    }

    #[test]
    fn a_missed_shot_at_the_can_costs_much_more() {
        let mut p = play(&[(1000.0, 0, true, 0.0)], false);
        p.chart.notes[0].kind = NoteKind::FireGun;
        p.update(1500.0);
        assert!((p.health - 0.5).abs() < 1e-9);
    }

    #[test]
    fn ten_misses_from_full_health_empty_it() {
        let notes: Vec<(f64, usize, bool, f64)> = (0..10).map(|i| (i as f64 * 10.0, 0, true, 0.0)).collect();
        let mut p = play(&notes, false);
        assert_eq!(p.update(1000.0).len(), 10);
        assert_eq!(p.health, 0.0);
    }

    #[test]
    fn skipping_ahead_costs_nothing() {
        let mut p = play(&[(100.0, 0, true, 2000.0), (200.0, 1, true, 0.0), (5000.0, 2, true, 0.0)], false);
        p.skip_to(1000.0);
        assert_eq!(p.update(1000.0), vec![]);
        assert_eq!((p.misses, p.health, p.accuracy(), p.finished()), (0, 1.0, None, false));
        assert!(p.press(2, 5000.0).is_some());
        p.update(5000.0);
        assert!(p.finished());
        // and back: the notes from there on are to be played again
        p.skip_to(150.0);
        assert_eq!((p.first_open(), p.finished()), (1, false));
        assert!(p.press(1, 200.0).is_some());
    }

    #[test]
    fn hurt_notes_hurt_only_when_hit() {
        let hurt = |time, sustain| Note { time, lane: 0, player: true, sustain, kind: NoteKind::Hurt };
        let mut p = Play::new(Chart { bpm: 120.0, speed: 1.0, notes: vec![hurt(0.0, 0.0), hurt(1000.0, 0.0)], ..Default::default() }, false);
        assert_eq!(p.press(0, 10.0), Some(Event::Miss { lane: 0, note: Some(0), piece: false, combo: 0 }));
        assert_eq!(p.update(2000.0), vec![]);
        assert!(p.finished() && p.misses == 1);
        // a hurt hold, piece by piece: pressed, its head hurts, and so does each piece held through (more than a tap)
        let mut p = Play::new(Chart { bpm: 120.0, speed: 1.0, notes: vec![hurt(0.0, 250.0)], ..Default::default() }, false);
        p.one_note_holds = false;
        p.press(0, 0.0);
        p.update(500.0);
        assert_eq!(p.misses, 3);
        assert!((p.health - (1.0 - 0.1 - 2.0 * 0.25)).abs() < 1e-9);
        // and let pass, it costs nothing
        let mut p = Play::new(Chart { bpm: 120.0, speed: 1.0, notes: vec![hurt(0.0, 250.0)], ..Default::default() }, false);
        assert!(p.update(500.0).is_empty() && p.finished());
    }
}
