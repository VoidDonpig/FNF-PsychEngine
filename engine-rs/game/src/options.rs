//! The options menu: key binds, and the pages of `settings::OPTIONS`.
use crate::settings::{Kind, Settings, OPTIONS, PAGES};
use crate::{DIRECTIONS, HEIGHT};
use macroquad::prelude::*;

#[derive(Default)]
pub struct OptionsMenu {
    /// The page that is open; none for the list of pages.
    page: Option<usize>,
    row: usize,
    /// The selected key slot waits for a key.
    binding: bool,
}

/// How an option's value reads in the menu.
fn shown(kind: &Kind, settings: &mut Settings) -> String {
    match *kind {
        Kind::Flag(get) => if *get(settings) { "ON" } else { "off" }.to_owned(),
        Kind::Number(get, .., "%") => format!("{:.0}%", *get(settings) * 100.0),
        Kind::Number(get, low, _, step, unit) => {
            // only what can go below zero shows its sign
            let (decimals, value) = (if step < 1.0 { 1 } else { 0 }, *get(settings));
            if low < 0.0 { format!("{value:+.decimals$} {unit}") } else { format!("{value:.decimals$} {unit}") }.trim_end().to_owned()
        }
        Kind::Choice(get, names) => names[*get(settings)].to_owned(),
    }
}

/// Changes an option by `steps` (Left/Right, times ten with Shift), or flips it (`steps` 0, Enter).
fn change(kind: &Kind, settings: &mut Settings, steps: f64) {
    match *kind {
        Kind::Flag(get) if steps == 0.0 => *get(settings) ^= true,
        Kind::Flag(_) => {}
        Kind::Number(get, low, high, step, _) => {
            // kept on the step's grid, so that ten times 0.1 is 1 and not 0.9999
            let value = ((*get(settings) + steps * step) / step).round() * step;
            *get(settings) = value.clamp(low, high);
        }
        Kind::Choice(get, names) => {
            let step = if steps < 0.0 { names.len() - 1 } else { 1 };
            *get(settings) = (*get(settings) + step) % names.len();
        }
    }
}

impl OptionsMenu {
    /// Opened at the page of that name, if there is one.
    pub fn at(page: &str) -> OptionsMenu {
        OptionsMenu { page: PAGES.iter().position(|name| name.eq_ignore_ascii_case(page)), ..Default::default() }
    }

    /// Runs and draws one frame. `text` writes a line with its baseline at (x, y). Returns true when the player leaves the options.
    pub fn run(&mut self, settings: &mut Settings, text: &dyn Fn(&str, f32, f32, u16, Color), raw_input: bool, bindable: &[KeyCode]) -> bool {
        let title = self.page.map_or("Options".to_owned(), |page| format!("Options - {}", PAGES[page]));
        text(&title, 40.0, 60.0, 36, WHITE);
        let line = |index: usize, selected: usize, content: &str, y: f32| {
            text(&format!("{}{content}", if index == selected { "> " } else { "  " }), 40.0, y, 24, if index == selected { YELLOW } else { WHITE });
        };
        let step = |rows: usize| (is_key_pressed(KeyCode::Down) as usize + is_key_pressed(KeyCode::Up) as usize * (rows - 1)) % rows;
        let back = is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Backspace);

        let Some(page) = self.page else {
            text("Up/Down: choose   Enter: open   Esc: save and back", 40.0, 96.0, 18, GRAY);
            self.row = (self.row + step(PAGES.len())) % PAGES.len();
            for (index, name) in PAGES.iter().enumerate() {
                line(index, self.row, name, 150.0 + index as f32 * 30.0);
            }
            if is_key_pressed(KeyCode::Enter) {
                (self.page, self.row) = (Some(self.row), 0);
            }
            return back;
        };

        if page == 0 {
            // two keys for each of the four lanes
            text("Up/Down: choose   Enter: bind   Backspace: unbind   Esc: back", 40.0, 96.0, 18, GRAY);
            let slot_of = |row: usize| (row / 2, row % 2);
            for index in 0..8 {
                let (lane, slot) = slot_of(index);
                let key = if self.binding && index == self.row { "press a key...".to_owned() } else { settings.keys[lane][slot].map_or("-".to_owned(), |k| format!("{k:?}")) };
                line(index, self.row, &format!("{} key {}: {key}", DIRECTIONS[lane], slot + 1), 150.0 + index as f32 * 30.0 + lane as f32 * 10.0);
            }
            let timing = if raw_input { "raw input: each press is timed when it happens" } else { "per frame (raw input is not available on this system)" };
            text(&format!("Keyboard timing: {timing}"), 40.0, HEIGHT - 40.0, 18, GRAY);
            let (lane, slot) = slot_of(self.row);
            if self.binding {
                // every key belongs to the binding now, the arrows included
                match get_last_key_pressed() {
                    Some(KeyCode::Escape) => self.binding = false,
                    Some(key) if bindable.contains(&key) => {
                        settings.bind(lane, slot, Some(key));
                        self.binding = false;
                    }
                    _ => {}
                }
                return false;
            }
            self.row = (self.row + step(8)) % 8;
            if is_key_pressed(KeyCode::Enter) {
                self.binding = true;
            } else if is_key_pressed(KeyCode::Backspace) {
                settings.bind(lane, slot, None);
            } else if is_key_pressed(KeyCode::Escape) {
                (self.page, self.row) = (None, page);
            }
            return false;
        }

        text("Up/Down: choose   Enter or Left/Right: change (Shift: x10)   Esc: back", 40.0, 96.0, 18, GRAY);
        let options: Vec<_> = OPTIONS.iter().filter(|option| option.page == page).collect();
        self.row = (self.row + step(options.len())) % options.len();
        for (index, option) in options.iter().enumerate() {
            line(index, self.row, &format!("{}: {}", option.name, shown(&option.kind, settings)), 150.0 + index as f32 * 30.0);
        }
        let selected = options[self.row];
        text(selected.about, 40.0, HEIGHT - 40.0, 18, GRAY);
        let steps = (is_key_pressed(KeyCode::Right) as i32 - is_key_pressed(KeyCode::Left) as i32) as f64;
        let fast = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) { 10.0 } else { 1.0 };
        if steps != 0.0 {
            change(&selected.kind, settings, steps * fast);
        } else if is_key_pressed(KeyCode::Enter) {
            // Enter flips a switch and steps through a list
            change(&selected.kind, settings, if matches!(selected.kind, Kind::Choice(..)) { 1.0 } else { 0.0 });
        }
        if back {
            (self.page, self.row) = (None, page);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_change_within_their_limits() {
        let mut settings = Settings::default();
        let option = |key: &str| &OPTIONS.iter().find(|option| option.key == key).unwrap().kind;
        change(option("downscroll"), &mut settings, 0.0);
        change(option("downscroll"), &mut settings, 1.0); // Left/Right leave a switch alone
        assert!(settings.downscroll);
        for _ in 0..10 {
            change(option("hitsound_volume"), &mut settings, 1.0);
        }
        assert_eq!((settings.hitsound_volume, shown(option("hitsound_volume"), &mut settings).as_str()), (1.0, "100%"));
        change(option("sick_window"), &mut settings, -100.0);
        assert_eq!((settings.sick_window, shown(option("sick_window"), &mut settings).as_str()), (15.0, "15 ms"));
        assert_eq!((shown(option("rating_offset"), &mut settings).as_str(), shown(option("safe_frames"), &mut settings).as_str()), ("+0 ms", "10.0"));
        change(option("time_bar"), &mut settings, -1.0);
        assert_eq!(shown(option("time_bar"), &mut settings), "Disabled");
        change(option("time_bar"), &mut settings, 1.0);
        assert_eq!(settings.time_bar, 0);
    }
}
