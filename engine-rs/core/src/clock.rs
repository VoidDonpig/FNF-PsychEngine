//! The song position the notes move by.

/// Further off than this (ms), the position is not eased toward what is heard but put there: a pause, a seek, a stall.
const JUMP: f64 = 100.0;
/// How long (ms) it takes to take up most of a difference between the even position and the heard one.
const EASE: f64 = 250.0;

/// Where the song is, running as evenly as time itself and eased toward what is heard.
/// A sound card says where it is in steps a few milliseconds wide; notes that follow that directly stop and jump.
#[derive(Default)]
pub struct Clock {
    position: f64,
}

impl Clock {
    /// `elapsed`: how much of the song (ms) went by since the last call, by the computer's clock.
    /// `heard`: where the sound card says the song is now.
    pub fn advance(&mut self, elapsed: f64, heard: f64) -> f64 {
        let even = self.position + elapsed;
        let off = heard - even;
        self.position = if off.abs() > JUMP { heard } else { even + off * (elapsed / EASE).min(1.0) };
        self.position
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_runs_evenly_through_a_jittery_sound_card_and_follows_real_changes() {
        let mut clock = Clock::default();
        // starting somewhere in the song: taken at once
        assert_eq!(clock.advance(2.0, 20_000.0), 20_000.0);
        // 1000 frames of 2 ms, heard in steps of 10 ms with up to 3 ms of error either way
        let (mut last, mut worst_step, mut worst_offset) = (20_000.0, 0.0f64, 0.0f64);
        for frame in 1..=1000 {
            let truth = 20_000.0 + frame as f64 * 2.0;
            let heard = (truth / 10.0).floor() * 10.0 + 5.0 + [3.0, -2.0, 0.5, -3.0, 1.0][frame % 5];
            let now = clock.advance(2.0, heard);
            (worst_step, worst_offset) = (worst_step.max((now - last - 2.0).abs()), worst_offset.max((now - truth).abs()));
            last = now;
        }
        assert!(worst_step < 0.1, "every frame moves the notes by a frame's worth: {worst_step}");
        assert!(worst_offset < 5.0, "and stays with the song: {worst_offset}");
        // the sound runs 1% slow: followed, without ever drifting far
        let mut heard = last;
        for _ in 0..5000 {
            heard += 1.98;
            last = clock.advance(2.0, heard);
        }
        assert!((last - heard).abs() < 6.0, "{}", last - heard);
        // the sound stalled for half a second: back with it at once
        assert_eq!(clock.advance(2.0, heard - 500.0), heard - 500.0);
    }
}
