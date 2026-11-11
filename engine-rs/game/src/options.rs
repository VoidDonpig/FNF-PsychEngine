//! The options menu: key binds, and the pages of `settings::OPTIONS`.
use crate::controls::{pressed, Ui, NAMES};
use crate::settings::{hex, Kind, Settings, LANE_COLORS, OPTIONS, PAGES, PIXEL_LANE_COLORS};
use crate::{DIRECTIONS, HEIGHT};
use crate::gfx::*;
use funkin_core::lang::phrase;

#[derive(Default)]
pub struct OptionsMenu {
    /// The page that is open; none for the list of pages.
    page: Option<usize>,
    row: usize,
    /// The selected key slot waits for a key.
    binding: bool,
    /// Opened straight at a page: leaving the page leaves the options.
    direct: bool,
    /// Note colours: those of pixel stages are shown, and the colour code being typed.
    pixel: bool,
    typing: Option<String>,
}

/// The colour with its hue turned by `degrees`. Greys have none and stay as they are.
fn turned(color: u32, degrees: f32) -> u32 {
    let (hue, saturation, light) = rgb_to_hsl(Color::from_hex(color));
    let Color { r, g, b, .. } = hsl_to_rgb((hue + degrees / 360.0).rem_euclid(1.0), saturation, light);
    [r, g, b].iter().fold(0, |code, channel| code << 8 | (channel * 255.0).round() as u32)
}

/// How an option's value reads in the menu.
fn shown(kind: &Kind, settings: &mut Settings) -> String {
    match *kind {
        Kind::Flag(get) => if *get(settings) { "ON" } else { "off" }.to_owned(),
        Kind::Number(get, .., "%") => format!("{:.0}%", *get(settings) * 100.0),
        Kind::Number(get, low, _, step, unit) => {
            // only what can go below zero shows its sign
            let (decimals, value) = (if step >= 1.0 { 0 } else if step >= 0.1 { 1 } else { 2 }, *get(settings));
            let unit = if unit == "X" { "X".to_owned() } else { format!(" {unit}") };
            if low < 0.0 { format!("{value:+.decimals$}{unit}") } else { format!("{value:.decimals$}{unit}") }.trim_end().to_owned()
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
        let page = PAGES.iter().position(|name| name.eq_ignore_ascii_case(page));
        OptionsMenu { page, direct: page.is_some(), ..Default::default() }
    }

    /// Runs and draws one frame. `text` writes a line with its baseline at (x, y). Returns true when the player leaves the options.
    /// `note` draws a lane's note (of pixel stages or not) centred on (x, y).
    /// `languages`: the languages there are, as (file, name).
    pub fn run(&mut self, settings: &mut Settings, text: &dyn Fn(&str, f32, f32, u16, Color), note: &dyn Fn(usize, bool, f32, f32), raw_input: bool, bindable: &[KeyCode], languages: &[(String, String)]) -> bool {
        let named = |page: usize| phrase(&format!("options_{}", PAGES[page]), PAGES[page]);
        let title = self.page.map_or("Options".to_owned(), |page| format!("Options - {}", named(page)));
        text(&title, 40.0, 60.0, 36, WHITE);
        let line = |index: usize, selected: usize, content: &str, y: f32| {
            text(&format!("{}{content}", if index == selected { "> " } else { "  " }), 40.0, y, 24, if index == selected { YELLOW } else { WHITE });
        };
        let step = |rows: usize| (pressed(Ui::Down) as usize + pressed(Ui::Up) as usize * (rows - 1)) % rows;
        let back = pressed(Ui::Back);

        let Some(page) = self.page else {
            text("Up/Down: choose   Enter: open   Esc: save and back", 40.0, 96.0, 18, GRAY);
            // the pages, and below them the language: it is chosen in place
            self.row = (self.row + step(PAGES.len() + 1)) % (PAGES.len() + 1);
            for index in 0..PAGES.len() {
                line(index, self.row, &named(index), 150.0 + index as f32 * 30.0);
            }
            let spoken = languages.iter().position(|(file, _)| *file == settings.language).unwrap_or(0);
            let language = languages.get(spoken).map_or(settings.language.as_str(), |(_, name)| name);
            line(PAGES.len(), self.row, &format!("{}: {language}", phrase("options_language", "Language")), 160.0 + PAGES.len() as f32 * 30.0);
            let turn = pressed(Ui::Right) as usize + pressed(Ui::Left) as usize * (languages.len().max(1) - 1);
            if self.row < PAGES.len() {
                if pressed(Ui::Accept) {
                    (self.page, self.row) = (Some(self.row), 0);
                }
            } else if let Some((file, _)) = languages.get((spoken + turn + pressed(Ui::Accept) as usize) % languages.len().max(1)) {
                settings.language = file.clone();
            }
            return back;
        };

        if page == 0 {
            // two keys for each of the four lanes, then two for each thing the menus and the volume do
            text("Up/Down: choose   Enter: bind   Backspace: unbind   Esc: back", 40.0, 96.0, 18, GRAY);
            let rows = 8 + NAMES.len() * 2;
            for index in 0..rows {
                let waiting = self.binding && index == self.row;
                let named = |key: Option<KeyCode>| if waiting { "press a key...".to_owned() } else { key.map_or("-".to_owned(), |k| format!("{k:?}")) };
                if index < 8 {
                    let (lane, slot) = (index / 2, index % 2);
                    line(index, self.row, &format!("{} key {}: {}", DIRECTIONS[lane], slot + 1, named(settings.keys[lane][slot])), 150.0 + index as f32 * 30.0 + lane as f32 * 10.0);
                } else {
                    let (action, slot, chosen) = ((index - 8) / 2, index % 2, index == self.row);
                    let label = format!("{}{} key {}: {}", if chosen { "> " } else { "  " }, NAMES[action].1, slot + 1, named(settings.ui_keys[action][slot]));
                    text(&label, 600.0, 150.0 + (index - 8) as f32 * 24.0 + action as f32 * 2.0, 20, if chosen { YELLOW } else { WHITE });
                }
            }
            let timing = if raw_input { "raw input: each press is timed when it happens" } else { "per frame (raw input is not available on this system)" };
            text(&format!("Keyboard timing: {timing}"), 40.0, 118.0, 18, GRAY);
            // the slot the selected row stands for: a lane's, or an action's
            let slot = if self.row < 8 { &mut settings.keys[self.row / 2][self.row % 2] } else { &mut settings.ui_keys[(self.row - 8) / 2][self.row % 2] };
            if self.binding {
                // every key belongs to the binding now, the arrows included
                match get_last_key_pressed() {
                    Some(KeyCode::Escape) => self.binding = false,
                    Some(key) if self.row < 8 && bindable.contains(&key) => {
                        settings.bind(self.row / 2, self.row % 2, Some(key));
                        self.binding = false;
                    }
                    Some(key) if self.row >= 8 && crate::settings::ui_bindable().contains(&key) => {
                        *slot = Some(key);
                        self.binding = false;
                    }
                    _ => {}
                }
                crate::controls::set(settings.ui_keys);
                return false;
            }
            self.row = (self.row + step(rows)) % rows;
            if pressed(Ui::Accept) {
                self.binding = true;
            } else if is_key_pressed(KeyCode::Backspace) {
                *slot = None;
                crate::controls::set(settings.ui_keys);
            } else if back {
                (self.page, self.row) = (None, page);
                return self.direct;
            }
            return false;
        }

        let fast = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) { 10.0 } else { 1.0 };
        let steps = (pressed(Ui::Right) as i32 - pressed(Ui::Left) as i32) as f64;
        if page == PAGES.len() - 1 {
            // three colours for each of the four lanes
            text("Up/Down: choose   Left/Right: turn the hue (Shift: x10)   Enter: type a colour code", 40.0, 96.0, 18, GRAY);
            text("R: back to its usual colour   Ctrl: the notes of pixel stages   Esc: back", 40.0, 118.0, 18, GRAY);
            let (colors, usual) = if self.pixel { (&mut settings.pixel_note_colors, PIXEL_LANE_COLORS) } else { (&mut settings.note_colors, LANE_COLORS) };
            if self.typing.is_none() {
                self.row = (self.row + step(12)) % 12;
            }
            let (lane, part) = (self.row / 3, self.row % 3);
            for index in 0..12 {
                let code = match &self.typing {
                    Some(typed) if index == self.row => format!("#{typed}_"),
                    _ => format!("#{:06X}", colors[index / 3][index % 3]),
                };
                line(index, self.row, &format!("{} {}: {code}", DIRECTIONS[index / 3], ["body", "shine", "outline"][index % 3]), 170.0 + index as f32 * 30.0 + (index / 3) as f32 * 20.0);
                if index % 3 == 1 {
                    note(index / 3, self.pixel, 480.0, 162.0 + index as f32 * 30.0 + (index / 3) as f32 * 20.0);
                }
            }
            text(if self.pixel { "Pixel stages" } else { "Other stages" }, 40.0, HEIGHT - 40.0, 18, GRAY);
            if let Some(typed) = &mut self.typing {
                while let Some(letter) = get_char_pressed() {
                    if letter.is_ascii_hexdigit() && typed.len() < 6 {
                        typed.push(letter.to_ascii_uppercase());
                    }
                }
                if is_key_pressed(KeyCode::Backspace) {
                    typed.pop();
                }
                // the sixth digit completes it
                if let Some(color) = hex(typed) {
                    (colors[lane][part], self.typing) = (color, None);
                } else if is_key_pressed(KeyCode::Escape) {
                    self.typing = None;
                }
                return false;
            }
            if steps != 0.0 {
                colors[lane][part] = turned(colors[lane][part], (steps * fast * 3.0) as f32);
            } else if pressed(Ui::Accept) {
                self.typing = Some(String::new());
            } else if pressed(Ui::Reset) {
                colors[lane][part] = usual[lane][part];
            } else if is_key_pressed(KeyCode::LeftControl) || is_key_pressed(KeyCode::RightControl) {
                self.pixel ^= true;
            }
            if back {
                (self.page, self.row) = (None, page);
            }
            return back && self.direct;
        }

        text("Up/Down: choose   Enter or Left/Right: change (Shift: x10)   Esc: back", 40.0, 96.0, 18, GRAY);
        let options: Vec<_> = OPTIONS.iter().filter(|option| option.page == page).collect();
        self.row = (self.row + step(options.len())) % options.len();
        for (index, option) in options.iter().enumerate() {
            let value = shown(&option.kind, settings);
            // a choice among names may have its names translated too
            let value = if matches!(option.kind, Kind::Choice(..)) { phrase(&format!("setting_{}-{value}", option.name), &value) } else { value };
            line(index, self.row, &format!("{}: {value}", phrase(&format!("setting_{}", option.name), option.name)), 150.0 + index as f32 * 30.0);
        }
        let selected = options[self.row];
        // long explanations run on to further lines (the letters are all 11 wide)
        let about = phrase(&format!("description_{}", selected.name), selected.about);
        let about = crate::menu::wrap(&about, 1200.0, |text| text.chars().count() as f32 * 11.0);
        for (line, words) in about.iter().enumerate() {
            text(words, 40.0, HEIGHT - 40.0 - (about.len() - 1 - line) as f32 * 22.0, 18, GRAY);
        }
        if steps != 0.0 {
            change(&selected.kind, settings, steps * fast);
        } else if pressed(Ui::Accept) {
            match (selected.key, &selected.kind) {
                // the frame rate takes the display's own
                ("framerate", &Kind::Number(get, low, high, ..)) => *get(settings) = refresh_rate().map_or(*get(settings), |rate| rate.round().clamp(low, high)),
                // Enter flips a switch and steps through a list
                _ => change(&selected.kind, settings, if matches!(selected.kind, Kind::Choice(..)) { 1.0 } else { 0.0 }),
            }
        }
        if back {
            (self.page, self.row) = (None, page);
        }
        back && self.direct
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
        change(option("playback_rate"), &mut settings, 10.0);
        assert_eq!(shown(option("playback_rate"), &mut settings), "1.50X");
        change(option("rating_y"), &mut settings, -2.0);
        assert_eq!((settings.combo_offset, shown(option("rating_y"), &mut settings).as_str()), ([0.0, -10.0, 0.0, 0.0], "-10 px"));
    }

    #[test]
    fn a_hue_turns_all_the_way_round() {
        assert_eq!((turned(0xFF0000, 120.0), turned(0xFF0000, -120.0), turned(0x808080, 50.0)), (0x00FF00, 0x0000FF, 0x808080));
        // a third of the way round, three times, and each lane's colour is itself again
        for color in LANE_COLORS.iter().chain(&PIXEL_LANE_COLORS).flatten() {
            assert_eq!(turned(turned(turned(*color, 120.0), 120.0), 120.0), *color);
        }
    }
}
