//! One run of a song: which notes were hit, how well, and what that did to score and health.
use crate::chart::Chart;

/// Timing windows in milliseconds (the engine's defaults).
pub const SICK_MS: f64 = 45.0;
pub const GOOD_MS: f64 = 90.0;
pub const BAD_MS: f64 = 135.0;
/// A note can be hit this long before or after its time (10 frames at 60 FPS).
pub const SAFE_MS: f64 = 10.0 / 60.0 * 1000.0;
// shortcut: one fixed release tolerance; the Haxe engine frees the tail one step (tempo dependent) early.
const HOLD_RELEASE_MS: f64 = 100.0;
const HIT_HEALTH: f64 = 0.02;
const MISS_HEALTH: f64 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rating { Sick, Good, Bad, Shit }

impl Rating {
    pub fn of(diff_ms: f64) -> Rating {
        match diff_ms.abs() {
            d if d <= SICK_MS => Rating::Sick,
            d if d <= GOOD_MS => Rating::Good,
            d if d <= BAD_MS => Rating::Bad,
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
    /// The player hit a note `diff_ms` after (+) or before (-) its time.
    Hit { lane: usize, rating: Rating, diff_ms: f64 },
    Miss { lane: usize },
    OpponentHit { lane: usize },
}

pub struct Play {
    pub chart: Chart,
    /// One entry per chart note.
    pub state: Vec<NoteState>,
    pub held: [bool; 4],
    pub botplay: bool,
    /// Off: a press with nothing to hit counts as a miss.
    pub ghost_tapping: bool,
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
        Play { chart, state, held: [false; 4], botplay, ghost_tapping: true, score: 0, misses: 0, combo: 0, health: 1.0, rated: 0, weight: 0.0, first_open: 0 }
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
            .take_while(|&i| self.chart.notes[i].time <= pos + SAFE_MS)
            .find(|&i| {
                let note = &self.chart.notes[i];
                note.player && note.lane == lane && self.state[i] == NoteState::Pending && (note.time - pos).abs() <= SAFE_MS
            });
        let Some(index) = found else {
            if self.ghost_tapping || self.botplay {
                return None;
            }
            self.score -= 10;
            self.health += MISS_HEALTH / 2.0; // a stray press costs half of what a missed note does
            return Some(self.miss(lane));
        };
        let (time, sustain, hurt) = { let n = &self.chart.notes[index]; (n.time, n.sustain, n.hurt) };
        if hurt {
            self.state[index] = NoteState::Done;
            return Some(self.miss(lane));
        }
        self.state[index] = if sustain > 0.0 { NoteState::Held } else { NoteState::Done };
        Some(self.hit(lane, pos - time))
    }

    pub fn release(&mut self, lane: usize) {
        self.held[lane] = false;
    }

    /// Advances to song position `pos` (ms): opponent and botplay hits, notes that got away, dropped holds.
    pub fn update(&mut self, pos: f64) -> Vec<Event> {
        let mut events = Vec::new();
        for i in self.first_open..self.chart.notes.len() {
            let note = self.chart.notes[i].clone();
            if note.time > pos + SAFE_MS {
                break;
            }
            let auto = !note.player || self.botplay;
            match self.state[i] {
                NoteState::Pending if auto && note.time <= pos => {
                    if note.player && note.hurt {
                        self.state[i] = NoteState::Done;
                        continue;
                    }
                    self.state[i] = if note.sustain > 0.0 { NoteState::Held } else { NoteState::Done };
                    // Botplay is rated at the note's own time, not at the frame that noticed it.
                    events.push(if note.player { self.hit(note.lane, 0.0) } else { Event::OpponentHit { lane: note.lane } });
                }
                NoteState::Pending if !auto && pos > note.time + SAFE_MS => {
                    self.state[i] = if note.hurt { NoteState::Done } else { NoteState::Missed };
                    if !note.hurt {
                        events.push(self.miss(note.lane));
                    }
                }
                NoteState::Held => {
                    let end = note.time + note.sustain;
                    if pos >= end || (!auto && !self.held[note.lane] && pos >= end - HOLD_RELEASE_MS) {
                        self.state[i] = NoteState::Done;
                    } else if !auto && !self.held[note.lane] {
                        // Letting go early loses the rest of the hold as one miss.
                        self.state[i] = NoteState::Missed;
                        events.push(self.miss(note.lane));
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

    fn hit(&mut self, lane: usize, diff_ms: f64) -> Event {
        let rating = Rating::of(diff_ms);
        self.score += rating.score();
        self.combo += 1;
        self.rated += 1;
        self.weight += rating.weight();
        self.health = (self.health + HIT_HEALTH).min(2.0);
        Event::Hit { lane, rating, diff_ms }
    }

    fn miss(&mut self, lane: usize) -> Event {
        self.misses += 1;
        self.combo = 0;
        self.rated += 1;
        self.health = (self.health - MISS_HEALTH).max(0.0);
        Event::Miss { lane }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::Note;

    fn play(notes: &[(f64, usize, bool, f64)], botplay: bool) -> Play {
        let notes = notes.iter().map(|&(time, lane, player, sustain)| Note { time, lane, player, sustain, hurt: false }).collect();
        Play::new(Chart { song: String::new(), bpm: 120.0, speed: 1.0, notes }, botplay)
    }

    #[test]
    fn windows() {
        assert_eq!([Rating::of(-45.0), Rating::of(45.1), Rating::of(90.0), Rating::of(135.0), Rating::of(-140.0)],
                   [Rating::Sick, Rating::Good, Rating::Good, Rating::Bad, Rating::Shit]);
    }

    #[test]
    fn press_takes_the_earliest_note_in_reach() {
        let mut p = play(&[(1000.0, 0, true, 0.0), (1100.0, 0, true, 0.0), (1000.0, 1, true, 0.0)], false);
        assert_eq!(p.press(0, 1030.0), Some(Event::Hit { lane: 0, rating: Rating::Sick, diff_ms: 30.0 }));
        assert_eq!(p.press(0, 1030.0), Some(Event::Hit { lane: 0, rating: Rating::Good, diff_ms: -70.0 }));
        assert_eq!(p.press(0, 1030.0), None, "nothing left on this lane: a ghost tap");
        assert_eq!((p.score, p.combo, p.misses), (550, 2, 0));
        assert_eq!(p.accuracy(), Some((1.0 + 0.67) / 2.0));
    }

    #[test]
    fn out_of_reach_is_a_ghost_tap_then_a_miss() {
        let mut p = play(&[(1000.0, 2, true, 0.0)], false);
        assert_eq!(p.press(2, 1000.0 - SAFE_MS - 1.0), None);
        assert_eq!(p.update(1000.0 + SAFE_MS), vec![]);
        assert_eq!(p.update(1000.0 + SAFE_MS + 1.0), vec![Event::Miss { lane: 2 }]);
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
        assert_eq!(dropped.update(300.0), vec![Event::Miss { lane: 3 }]);
        assert_eq!(dropped.update(600.0), vec![], "only once");
    }

    #[test]
    fn botplay_and_opponent_play_themselves() {
        let mut p = play(&[(100.0, 0, false, 0.0), (200.0, 1, true, 300.0)], true);
        assert_eq!(p.update(99.0), vec![]);
        assert_eq!(p.update(250.0), vec![Event::OpponentHit { lane: 0 }, Event::Hit { lane: 1, rating: Rating::Sick, diff_ms: 0.0 }]);
        assert!(!p.finished());
        p.update(500.0);
        assert!(p.finished() && p.accuracy() == Some(1.0) && p.score == 350);
    }

    #[test]
    fn stray_presses_cost_only_without_ghost_tapping() {
        let mut p = play(&[(5000.0, 0, true, 0.0)], false);
        assert_eq!(p.press(0, 100.0), None);
        p.ghost_tapping = false;
        assert_eq!(p.press(0, 100.0), Some(Event::Miss { lane: 0 }));
        assert_eq!((p.misses, p.score), (1, -10));
        assert!((p.health - 0.95).abs() < 1e-9);
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
        let hurt = |time| Note { time, lane: 0, player: true, sustain: 0.0, hurt: true };
        let mut p = Play::new(Chart { song: String::new(), bpm: 120.0, speed: 1.0, notes: vec![hurt(0.0), hurt(1000.0)] }, false);
        assert_eq!(p.press(0, 10.0), Some(Event::Miss { lane: 0 }));
        assert_eq!(p.update(2000.0), vec![]);
        assert!(p.finished() && p.misses == 1);
    }
}
