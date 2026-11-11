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
// shortcut: one fixed release tolerance; the Haxe engine frees the tail one step (tempo dependent) early.
const HOLD_RELEASE_MS: f64 = 100.0;
const HIT_HEALTH: f64 = 0.02;
const MISS_HEALTH: f64 = 0.1;

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// The player hit chart note `note`, `diff_ms` after (+) or before (-) its time.
    Hit { lane: usize, rating: Rating, diff_ms: f64, note: usize },
    /// A note got away or was dropped, or (without `note`) a press hit nothing.
    Miss { lane: usize, note: Option<usize> },
    OpponentHit { lane: usize, note: usize },
}

pub struct Play {
    pub chart: Chart,
    /// One entry per chart note.
    pub state: Vec<NoteState>,
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
    pub score: i64,
    pub misses: u32,
    pub combo: u32,
    /// 0 (dead) to 2.
    pub health: f64,
    rated: u32,
    weight: f64,
    /// Every note before this one is finished.
    first_open: usize,
}

impl Play {
    pub fn new(chart: Chart, botplay: bool) -> Play {
        let state = vec![NoteState::Pending; chart.notes.len()];
        Play { chart, state, held: [false; 4], botplay, ghost_tapping: true, timing: Timing::default(), health_gain: 1.0, health_loss: 1.0, instakill: false, score: 0, misses: 0, combo: 0, health: 1.0, rated: 0, weight: 0.0, first_open: 0 }
    }

    /// Accuracy from 0 to 1, once anything was rated.
    pub fn accuracy(&self) -> Option<f64> {
        (self.rated > 0).then(|| self.weight / self.rated as f64)
    }

    pub fn finished(&self) -> bool {
        self.first_open == self.state.len()
    }

    /// Starts the run at `pos` (ms): every note before it is out of play and counts for nothing.
    pub fn skip_to(&mut self, pos: f64) {
        for (note, state) in self.chart.notes.iter().zip(&mut self.state) {
            if note.time < pos {
                *state = NoteState::Done;
            }
        }
        self.first_open = self.state.iter().position(|s| *s != NoteState::Done).unwrap_or(self.state.len());
    }

    /// A key went down at song position `pos` (ms).
    pub fn press(&mut self, lane: usize, pos: f64) -> Option<Event> {
        self.held[lane] = true;
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
            self.score -= 10;
            return Some(self.miss(lane, None));
        };
        let (time, sustain, hurt) = { let n = &self.chart.notes[index]; (n.time, n.sustain, n.kind == NoteKind::Hurt) };
        if hurt {
            self.state[index] = NoteState::Done;
            return Some(self.miss(lane, Some(index)));
        }
        self.state[index] = if sustain > 0.0 { NoteState::Held } else { NoteState::Done };
        Some(self.hit(lane, pos - time, index))
    }

    pub fn release(&mut self, lane: usize) {
        self.held[lane] = false;
    }

    /// Advances to song position `pos` (ms): opponent and botplay hits, notes that got away, dropped holds.
    pub fn update(&mut self, pos: f64) -> Vec<Event> {
        let mut events = Vec::new();
        for i in self.first_open..self.chart.notes.len() {
            let note = self.chart.notes[i].clone();
            if note.time > pos + self.timing.safe {
                break;
            }
            let auto = !note.player || self.botplay;
            let hurt = note.kind == NoteKind::Hurt;
            match self.state[i] {
                NoteState::Pending if auto && note.time <= pos => {
                    if note.player && hurt {
                        self.state[i] = NoteState::Done;
                        continue;
                    }
                    self.state[i] = if note.sustain > 0.0 { NoteState::Held } else { NoteState::Done };
                    // Botplay is rated at the note's own time, not at the frame that noticed it.
                    events.push(if note.player { self.hit(note.lane, 0.0, i) } else { Event::OpponentHit { lane: note.lane, note: i } });
                }
                NoteState::Pending if !auto && pos > note.time + self.timing.safe => {
                    self.state[i] = if hurt { NoteState::Done } else { NoteState::Missed };
                    if !hurt {
                        events.push(self.miss(note.lane, Some(i)));
                    }
                }
                NoteState::Held => {
                    let end = note.time + note.sustain;
                    if pos >= end || (!auto && !self.held[note.lane] && pos >= end - HOLD_RELEASE_MS) {
                        self.state[i] = NoteState::Done;
                    } else if !auto && !self.held[note.lane] {
                        // Letting go early loses the rest of the hold as one miss.
                        self.state[i] = NoteState::Missed;
                        events.push(self.miss(note.lane, Some(i)));
                    }
                }
                _ => {}
            }
        }
        while self.first_open < self.state.len() && matches!(self.state[self.first_open], NoteState::Done | NoteState::Missed) {
            self.first_open += 1;
        }
        events
    }

    fn hit(&mut self, lane: usize, diff_ms: f64, note: usize) -> Event {
        let rating = Rating::of(diff_ms, &self.timing);
        self.score += rating.score();
        self.combo += 1;
        self.rated += 1;
        self.weight += rating.weight();
        self.health = (self.health + HIT_HEALTH * self.health_gain).min(2.0);
        Event::Hit { lane, rating, diff_ms, note }
    }

    fn miss(&mut self, lane: usize, note: Option<usize>) -> Event {
        self.misses += 1;
        self.combo = 0;
        self.rated += 1;
        // a stray press costs half of what a missed note does
        self.health -= MISS_HEALTH * self.health_loss * if note.is_some() { 1.0 } else { 0.5 };
        // the can that was not shot down goes off in the player's face
        if note.is_some_and(|index| self.chart.notes[index].kind == NoteKind::FireGun) {
            self.health -= 0.4;
        }
        if self.health < 1e-9 || self.instakill {
            self.health = 0.0; // ten misses from full must empty the bar, whatever the rounding says
        }
        Event::Miss { lane, note }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::Note;

    fn play(notes: &[(f64, usize, bool, f64)], botplay: bool) -> Play {
        let notes = notes.iter().map(|&(time, lane, player, sustain)| Note { time, lane, player, sustain, kind: NoteKind::Normal }).collect();
        Play::new(Chart { bpm: 120.0, speed: 1.0, notes, ..Default::default() }, botplay)
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
        const SAFE_MS: f64 = 10.0 / 60.0 * 1000.0;
        assert_eq!(p.press(2, 1000.0 - SAFE_MS - 1.0), None);
        assert_eq!(p.update(1000.0 + SAFE_MS), vec![]);
        assert_eq!(p.update(1000.0 + SAFE_MS + 1.0), vec![Event::Miss { lane: 2, note: Some(0) }]);
        assert_eq!((p.misses, p.combo, p.accuracy(), p.finished()), (1, 0, Some(0.0), true));
        assert!(p.health < 1.0);
    }

    #[test]
    fn dropping_a_hold_is_one_miss_and_holding_it_is_none() {
        let mut held = play(&[(0.0, 3, true, 1000.0)], false);
        held.press(3, 0.0);
        assert_eq!(held.update(500.0), vec![]);
        held.release(3); // inside the release tolerance at the end
        assert_eq!(held.update(950.0), vec![]);
        assert!(held.finished() && held.misses == 0);

        let mut dropped = play(&[(0.0, 3, true, 1000.0)], false);
        dropped.press(3, 0.0);
        dropped.release(3);
        assert_eq!(dropped.update(300.0), vec![Event::Miss { lane: 3, note: Some(0) }]);
        assert_eq!(dropped.update(600.0), vec![], "only once");
    }

    #[test]
    fn botplay_and_opponent_play_themselves() {
        let mut p = play(&[(100.0, 0, false, 0.0), (200.0, 1, true, 300.0)], true);
        assert_eq!(p.update(99.0), vec![]);
        assert_eq!(p.update(250.0), vec![Event::OpponentHit { lane: 0, note: 0 }, Event::Hit { lane: 1, rating: Rating::Sick, diff_ms: 0.0, note: 1 }]);
        assert!(!p.finished());
        p.update(500.0);
        assert!(p.finished() && p.accuracy() == Some(1.0) && p.score == 350);
    }

    #[test]
    fn stray_presses_cost_only_without_ghost_tapping() {
        let mut p = play(&[(5000.0, 0, true, 0.0)], false);
        assert_eq!(p.press(0, 100.0), None);
        p.ghost_tapping = false;
        assert_eq!(p.press(0, 100.0), Some(Event::Miss { lane: 0, note: None }));
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
        let mut p = play(&[(100.0, 0, true, 0.0), (200.0, 1, true, 0.0), (5000.0, 2, true, 0.0)], false);
        p.skip_to(1000.0);
        assert_eq!(p.update(1000.0), vec![]);
        assert_eq!((p.misses, p.health, p.accuracy(), p.finished()), (0, 1.0, None, false));
        assert!(p.press(2, 5000.0).is_some());
        p.update(5000.0);
        assert!(p.finished());
    }

    #[test]
    fn hurt_notes_hurt_only_when_hit() {
        let hurt = |time| Note { time, lane: 0, player: true, sustain: 0.0, kind: NoteKind::Hurt };
        let mut p = Play::new(Chart { bpm: 120.0, speed: 1.0, notes: vec![hurt(0.0), hurt(1000.0)], ..Default::default() }, false);
        assert_eq!(p.press(0, 10.0), Some(Event::Miss { lane: 0, note: Some(0) }));
        assert_eq!(p.update(2000.0), vec![]);
        assert!(p.finished() && p.misses == 1);
    }
}
