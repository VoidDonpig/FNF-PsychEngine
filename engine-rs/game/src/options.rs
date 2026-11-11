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
    /// The screen for the note delay and the place of rating and combo, while it is open.
    adjust: Option<Box<crate::adjust::Adjust>>,
    /// The list of languages, while it is open: the row chosen.
    language: Option<usize>,
    /// Where the list of a page has glided to, and when a switch was last flipped (its box plays its tick).
    scroll: f32,
    flipped: f64,
    /// Key binds: the second key of the row is chosen; how long Escape or Backspace has been held while binding.
    alt: bool,
    holding: f32,
}

/// The options' first list, in the Haxe engine's order.
const MAIN: [&str; 7] = ["Note Colors", "Controls", "Adjust Delay and Combo", "Graphics", "Visuals", "Gameplay", "Language"];

/// The pink of the options' background.
const PINK: Vec3 = vec3(0xEA as f32 / 255.0, 0x71 as f32 / 255.0, 0xFD as f32 / 255.0);

/// The blue of the key binds' background.
const BLUE: Vec3 = vec3(0x71 as f32 / 255.0, 0x92 as f32 / 255.0, 0xFD as f32 / 255.0);

/// A row of the key binds' page.
#[derive(Clone, Copy, PartialEq)]
enum Bind {
    Header,
    Gap,
    Lane(usize),
    /// Of `controls::NAMES`.
    Action(usize),
    Defaults,
}

/// The key binds' rows in the Haxe engine's order: each with its name, and its name while it waits for a key.
const BINDS: [(Bind, &str, &str); 24] = {
    use Bind::*;
    [
        (Header, "NOTES", ""), (Lane(0), "Left", "Note Left"), (Lane(1), "Down", "Note Down"), (Lane(2), "Up", "Note Up"), (Lane(3), "Right", "Note Right"), (Gap, "", ""),
        (Header, "UI", ""), (Action(0), "Left", "UI Left"), (Action(1), "Down", "UI Down"), (Action(2), "Up", "UI Up"), (Action(3), "Right", "UI Right"), (Gap, "", ""),
        (Action(7), "Reset", "Reset"), (Action(4), "Accept", "Accept"), (Action(5), "Back", "Back"), (Action(6), "Pause", "Pause"), (Gap, "", ""),
        (Header, "VOLUME", ""), (Action(8), "Mute", "Volume Mute"), (Action(9), "Up", "Volume Up"), (Action(10), "Down", "Volume Down"), (Gap, "", ""), (Gap, "", ""),
        (Defaults, "Reset to Default Keys", ""),
    ]
};

/// A key's name as the Haxe engine shows it ("---" for none).
fn key_name(key: Option<KeyCode>) -> String {
    use KeyCode::*;
    let Some(key) = key else { return "---".to_owned() };
    let short = match key {
        Backspace => "BckSpc",
        LeftControl | RightControl => "Ctrl",
        LeftAlt | RightAlt => "Alt",
        KpAdd => "#+",
        KpSubtract => "#-",
        Semicolon => ";",
        Comma => ",",
        Period => ".",
        LeftBracket => "[",
        RightBracket => "]",
        Apostrophe => "'",
        _ => "",
    };
    if !short.is_empty() {
        return short.to_owned();
    }
    let name = format!("{key:?}");
    // a digit of the numbers' row, or (after a "#") of the keypad
    if let Some(digit) = name.strip_prefix("Key").filter(|digit| digit.len() == 1) {
        return digit.to_owned();
    }
    if let Some(digit) = name.strip_prefix("Kp").filter(|digit| digit.len() == 1) {
        return format!("#{digit}");
    }
    // the others' words spaced out: "LeftShift" reads "Left Shift"
    name.chars().enumerate().flat_map(|(at, letter)| (at > 0 && letter.is_uppercase()).then_some(' ').into_iter().chain([letter])).collect()
}

/// The name a row's keys are saved under, which its translation is found by.
fn key_of(bind: Bind) -> &'static str {
    match bind {
        Bind::Lane(lane) => ["note_left", "note_down", "note_up", "note_right"][lane],
        Bind::Action(action) => NAMES[action].0,
        _ => "",
    }
}

/// Where a list of the Haxe engine's large letters puts a row `from` rows away from the chosen one, starting at `start`.
fn listed(from: f32, start: Vec2) -> Vec2 {
    start + vec2(from * 20.0, from * 1.3 * 120.0)
}

/// The row of `MAIN` that opens page `page`.
fn main_row(page: usize) -> usize {
    MAIN.iter().position(|name| *name == PAGES[page]).unwrap_or(0)
}

/// The tick box of a switch at a row's `at` (the row's corner), `since` seconds after it was last flipped.
fn checkbox(menus: &mut crate::menu::Menus, on: bool, since: f32, at: Vec2, alpha: f32) {
    let Some(sheet) = menus.sheet("checkboxanim") else { return };
    // turning on or off, then still: the animations and the shifts the Haxe engine gives them
    let (turning, turning_by, still, still_by) = if on { ("checkbox anim0", (34.0, 25.0), "checkbox finish", (3.0, 12.0)) } else { ("checkbox anim reverse", (25.0, 28.0), "checkbox0", (0.0, 2.0)) };
    let frames = sheet.animation(turning, &[]);
    let step = (since * 24.0) as usize;
    let (frame, by) = match frames.get(step) {
        Some(frame) => (*frame, turning_by),
        None => match sheet.animation(still, &[]).first() {
            Some(frame) => (*frame, still_by),
            None => return,
        },
    };
    // drawn at nine tenths of its size, about its middle
    let corner = at + vec2(-130.0, 30.0) - vec2(by.0, by.1) + frame.size * 0.05;
    sheet.draw(frame, corner.x, corner.y, 0.9, false, Color::new(1.0, 1.0, 1.0, alpha));
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

/// Which of `languages` (file, name) is chosen `steps` on from the one in `file`. A language that is gone counts as the game's own.
fn next_language(languages: &[(String, String)], file: &str, steps: usize) -> usize {
    let at = |file: &str| languages.iter().position(|(known, _)| known == file);
    (at(file).or(at(funkin_core::lang::ENGLISH.0)).unwrap_or(0) + steps) % languages.len().max(1)
}

impl OptionsMenu {
    /// Opened at the page of that name, if there is one.
    pub fn at(page: &str) -> OptionsMenu {
        let page = PAGES.iter().position(|name| name.eq_ignore_ascii_case(page));
        OptionsMenu { page, direct: page.is_some(), ..Default::default() }
    }

    /// Runs and draws one frame. `text` writes a line with its baseline at (x, y). Returns true when the player leaves the options.
    /// `note` draws a lane's note (of pixel stages or not) centred on (x, y).
    /// Whether the screen with its own music (delay and combo) is open.
    pub fn adjusting(&self) -> bool {
        self.adjust.is_some()
    }

    /// `languages`: the languages there are, as (file, name). `base`: the game's folder. `menus` lends its letters, pictures and
    /// sounds, `skin` the plain font.
    #[allow(clippy::too_many_arguments)]
    pub fn run(&mut self, settings: &mut Settings, menus: &mut crate::menu::Menus, skin: &crate::Skin, text: &dyn Fn(&str, f32, f32, u16, Color), note: &dyn Fn(usize, bool, f32, f32), raw_input: bool, bindable: &[KeyCode], languages: &[(String, String)], base: &std::path::Path) -> bool {
        if let Some(adjust) = &mut self.adjust {
            if adjust.run(settings, text) {
                self.adjust = None;
            }
            return false;
        }
        let dt = get_frame_time().min(0.1);
        let line = |index: usize, selected: usize, content: &str, y: f32| {
            text(&format!("{}{content}", if index == selected { "> " } else { "  " }), 40.0, y, 24, if index == selected { YELLOW } else { WHITE });
        };
        let step = |rows: usize| (pressed(Ui::Down) as usize + pressed(Ui::Up) as usize * (rows - 1)) % rows;
        let back = pressed(Ui::Back);
        // the Haxe engine's look: the pink background, lists in large letters (the key binds and the note colours stay plain lists)
        match self.page {
            Some(page) if page == PAGES.len() - 1 => text(&phrase(&format!("options_{}", PAGES[page]), PAGES[page]), 40.0, 60.0, 36, WHITE),
            Some(0) => menus.backdrop(BLUE, dt),
            _ => menus.backdrop(PINK, dt),
        }
        if self.binding {
        } else if back {
            menus.sound("cancelMenu", 1.0);
        } else if pressed(Ui::Up) || pressed(Ui::Down) {
            menus.sound("scrollMenu", 1.0);
        }
        // a centred list that does not move, the chosen row marked (the first list, and the languages)
        let fixed = |menus: &crate::menu::Menus, rows: &[String], chosen: usize, gap: f32, marked: bool| {
            for (index, name) in rows.iter().enumerate() {
                let (width, alpha) = (menus.width(skin, name, true), if index == chosen { 1.0 } else { 0.6 });
                let at = vec2((crate::WIDTH - width) / 2.0, (HEIGHT - 70.0) / 2.0 + gap * (index as f32 - rows.len() as f32 / 2.0) + 45.0);
                menus.write(skin, name, true, at.x, at.y, 1.0, alpha);
                if marked && index == chosen {
                    menus.write(skin, ">", true, at.x - 63.0, at.y, 1.0, 1.0);
                    menus.write(skin, "<", true, at.x + width + 15.0, at.y, 1.0, 1.0);
                }
            }
        };

        if let Some(row) = self.language {
            let row = (row + step(languages.len().max(1))) % languages.len().max(1);
            self.language = Some(row);
            fixed(menus, &languages.iter().map(|(_, name)| name.clone()).collect::<Vec<_>>(), row, 100.0, false);
            if pressed(Ui::Accept) {
                menus.sound("confirmMenu", 0.6);
                settings.language = languages.get(row).map_or(settings.language.clone(), |(file, _)| file.clone());
            }
            if back {
                (self.language, self.row) = (None, MAIN.len() - 1);
            }
            return false;
        }
        let Some(page) = self.page else {
            self.row = (self.row + step(MAIN.len())) % MAIN.len();
            fixed(menus, &MAIN.map(|name| phrase(&format!("options_{name}"), name)), self.row, 92.0, true);
            if pressed(Ui::Accept) {
                match MAIN[self.row] {
                    "Adjust Delay and Combo" => self.adjust = Some(Box::new(crate::adjust::Adjust::open(base, settings))),
                    "Language" => self.language = Some(next_language(languages, &settings.language, 0)),
                    name => (self.page, self.row, self.scroll) = (PAGES.iter().position(|page| *page == name), 0, 0.0),
                }
            }
            return back;
        };

        if page == 0 {
            return self.controls(settings, menus, skin, text, dt, back, raw_input, bindable);
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
                (self.page, self.row) = (None, main_row(page));
            }
            return back && self.direct;
        }

        // a page of options: its name faint at the top, the options in a slanted list gliding to the chosen one (switches with
        // a tick box before them, the others with their value after), what the chosen one does in a box at the bottom
        let options: Vec<_> = OPTIONS.iter().filter(|option| option.page == page).collect();
        self.row = (self.row + step(options.len())) % options.len();
        self.scroll += (self.row as f32 - self.scroll) * (1.0 - (-dt * 9.6).exp());
        let title = match PAGES[page] {
            "Gameplay Changers" => phrase("gameplay_changers", "Gameplay Changers"),
            name => phrase(&format!("{}_menu", name.to_lowercase()), &format!("{name} Settings")),
        };
        push_model_matrix(Mat4::from_translation(vec3(75.0, 45.0, 0.0)) * Mat4::from_scale(vec3(0.6, 0.6, 1.0)));
        menus.write(skin, &title, true, 0.0, 0.0, 1.0, 0.4);
        pop_model_matrix();
        let since = (get_time() - self.flipped) as f32;
        for (index, option) in options.iter().enumerate() {
            let switch = matches!(option.kind, Kind::Flag(_));
            let at = listed(index as f32 - self.scroll, vec2(if switch { 220.0 } else { 140.0 }, 260.0));
            if !(-200.0..HEIGHT + 100.0).contains(&at.y) {
                continue;
            }
            let (name, alpha) = (phrase(&format!("setting_{}", option.name), option.name), if index == self.row { 1.0 } else { 0.6 });
            menus.write(skin, &name, false, at.x, at.y, 1.0, alpha);
            if let Kind::Flag(get) = option.kind {
                checkbox(menus, *get(settings), if index == self.row { since } else { 60.0 }, at, alpha);
            } else {
                let value = shown(&option.kind, settings);
                // a choice among names may have its names translated too
                let value = if matches!(option.kind, Kind::Choice(..)) { phrase(&format!("setting_{}-{value}", option.name), &value) } else { value };
                menus.write(skin, &value, false, at.x + menus.width(skin, &name, false) + 60.0, at.y, 1.0, alpha);
            }
        }
        let selected = options[self.row];
        let about = phrase(&format!("description_{}", selected.name), selected.about);
        let about = crate::menu::wrap(&about, 1180.0, |words| measure_text(words, skin.font.as_ref(), 32, 1.0).width);
        let top = (HEIGHT - about.len() as f32 * 32.0) / 2.0 + 270.0;
        draw_rectangle(40.0, top - 10.0, 1200.0, about.len() as f32 * 32.0 + 25.0, Color::new(0.0, 0.0, 0.0, 0.6));
        for (row, words) in about.iter().enumerate() {
            let width = measure_text(words, skin.font.as_ref(), 32, 1.0).width;
            skin.text(words, (crate::WIDTH - width) / 2.0, top + 26.0 + row as f32 * 32.0, 32, WHITE);
        }
        if steps != 0.0 {
            change(&selected.kind, settings, steps * fast);
            menus.sound("scrollMenu", 1.0);
        } else if pressed(Ui::Accept) {
            menus.sound("scrollMenu", 1.0);
            match (selected.key, &selected.kind) {
                // the frame rate takes the display's own
                ("framerate", &Kind::Number(get, low, high, ..)) => *get(settings) = refresh_rate().map_or(*get(settings), |rate| rate.round().clamp(low, high)),
                // Enter flips a switch (its box ticks) and steps through a list
                (_, Kind::Flag(_)) => {
                    change(&selected.kind, settings, 0.0);
                    self.flipped = get_time();
                }
                _ => change(&selected.kind, settings, 1.0),
            }
        }
        if back {
            (self.page, self.row) = (None, main_row(page));
        }
        back && self.direct
    }

    /// The key binds' page, as the Haxe engine's: a row for each thing with its two keys, Left/Right choosing which; Enter
    /// waits for a key (Escape or Backspace held half a second: give up, or leave none).
    #[allow(clippy::too_many_arguments)]
    fn controls(&mut self, settings: &mut Settings, menus: &mut crate::menu::Menus, skin: &crate::Skin, text: &dyn Fn(&str, f32, f32, u16, Color), dt: f32, back: bool, raw_input: bool, bindable: &[KeyCode]) -> bool {
        // light squares drifting down and to the right
        let drift = (get_time() * 40.0 % 160.0) as f32;
        for row in -1..5 {
            for column in -1..8 {
                for corner in [0.0, 80.0] {
                    draw_rectangle(column as f32 * 160.0 + drift + corner, row as f32 * 160.0 + drift + corner, 80.0, 80.0, Color::new(1.0, 1.0, 1.0, 0.2));
                }
            }
        }
        if let Some(sign) = menus.picture("controllertype") {
            draw_texture_ex(&sign, 50.0, 40.0, WHITE, DrawTextureParams { source: Some(Rect::new(0.0, 0.0, 82.0, 60.0)), ..Default::default() });
        }
        push_model_matrix(Mat4::from_translation(vec3(60.0 - menus.width(skin, "CTRL", false) * 0.2, 90.0, 0.0)) * Mat4::from_scale(vec3(0.4, 0.4, 1.0)));
        menus.write(skin, "CTRL", false, 0.0, 0.0, 1.0, 1.0);
        pop_model_matrix();

        let rows: Vec<usize> = (0..BINDS.len()).filter(|&id| matches!(BINDS[id].0, Bind::Lane(_) | Bind::Action(_) | Bind::Defaults)).collect();
        if !self.binding {
            self.row = (self.row + (pressed(Ui::Down) as usize + pressed(Ui::Up) as usize * (rows.len() - 1))) % rows.len();
        }
        let chosen = rows[self.row];
        // the list glides to keep three rows above the chosen one and four below, short of its ends
        self.scroll += ((chosen as f32).clamp(3.0, (BINDS.len() - 5) as f32) - self.scroll) * (1.0 - (-dt * 9.6).exp());
        let keys = |bind: Bind| match bind {
            Bind::Lane(lane) => settings.keys[lane],
            Bind::Action(action) => settings.ui_keys[action],
            _ => [None; 2],
        };
        for (id, &(bind, name, _)) in BINDS.iter().enumerate() {
            let y = (id as f32 - self.scroll) * 1.3 * 60.0 + 300.0;
            let alpha = if id == chosen { 1.0 } else { 0.6 };
            if bind == Bind::Gap || !(-100.0..HEIGHT + 20.0).contains(&y) {
                continue;
            }
            let centred = |name: &str, bold: bool, alpha: f32| menus.write(skin, name, bold, (crate::WIDTH - menus.width(skin, name, bold)) / 2.0, y - 55.0, 1.0, alpha);
            match bind {
                Bind::Header => centred(&phrase(&format!("keygroup_{name}"), name), false, 1.0),
                Bind::Defaults => centred(&phrase(name, name), true, alpha),
                _ => {
                    let label = phrase(&format!("key_{}", key_of(bind)), name);
                    menus.write(skin, &label, true, 475.0 - menus.width(skin, &label, true), y, 1.0, alpha);
                    for (slot, key) in keys(bind).into_iter().enumerate() {
                        let x = 550.0 + slot as f32 * 300.0;
                        draw_rectangle(x, y - 6.0, 250.0, 78.0, Color::new(0.0, 0.0, 0.0, 0.4 * alpha));
                        if id == chosen && slot == self.alt as usize {
                            draw_rectangle(x, y - 6.0, 250.0, 78.0, Color::new(1.0, 1.0, 1.0, 0.75));
                        }
                        let key = key_name(key);
                        menus.write(skin, &key, false, x + 10.0, y - 52.0, (230.0 / menus.width(skin, &key, false)).min(1.0), alpha);
                    }
                }
            }
        }
        let timing = if raw_input { "raw input: each press is timed when it happens" } else { "per frame (raw input is not available on this system)" };
        text(&format!("Keyboard timing: {timing}"), 20.0, HEIGHT - 12.0, 16, WHITE);

        let bind = BINDS[chosen].0;
        if !self.binding {
            if pressed(Ui::Left) || pressed(Ui::Right) {
                self.alt ^= true;
                menus.sound("scrollMenu", 1.0);
            }
            if pressed(Ui::Accept) && bind == Bind::Defaults {
                (settings.keys, settings.ui_keys) = (Settings::default().keys, crate::controls::defaults());
                crate::controls::set(settings.ui_keys);
                menus.sound("cancelMenu", 1.0);
            } else if pressed(Ui::Accept) {
                (self.binding, self.holding, self.flipped) = (true, 0.0, get_time());
                menus.sound("scrollMenu", 1.0);
            } else if back {
                (self.page, self.row) = (None, main_row(0));
                return self.direct;
            }
            return false;
        }

        // waiting for a key, over a whitened screen
        let since = (get_time() - self.flipped) as f32;
        draw_rectangle(0.0, 0.0, crate::WIDTH, HEIGHT, Color::new(1.0, 1.0, 1.0, 0.6 * (since / 0.35).min(1.0)));
        let rebinding = funkin_core::lang::phrase_with("controls_rebinding", "Rebinding {1}", &[BINDS[chosen].2]);
        menus.write(skin, &rebinding, false, (crate::WIDTH - menus.width(skin, &rebinding, false)) / 2.0, 160.0, 1.0, 1.0);
        for (line, words) in phrase("controls_rebinding2", "Hold ESC to Cancel\nHold Backspace to Delete").lines().enumerate() {
            menus.write(skin, words, true, (crate::WIDTH - menus.width(skin, words, true)) / 2.0, 340.0 + line as f32 * 85.0, 1.0, 1.0);
        }
        let (escape, backspace) = (is_key_down(KeyCode::Escape), is_key_down(KeyCode::Backspace));
        let mut chosen_key = None;
        if escape || backspace {
            self.holding += dt;
            if self.holding > 0.5 {
                menus.sound("cancelMenu", 1.0);
                self.binding = false;
                // held Backspace leaves the slot empty
                chosen_key = backspace.then_some(None);
            }
        } else {
            // a short press of Escape or Backspace binds it, once let go
            self.holding = 0.0;
            let tapped = [KeyCode::Escape, KeyCode::Backspace].into_iter().find(|&key| is_key_released(key));
            let key = get_last_key_pressed().filter(|key| !matches!(key, KeyCode::Escape | KeyCode::Backspace)).or(tapped);
            let allowed = |key: &KeyCode| if matches!(bind, Bind::Lane(_)) { bindable.contains(key) } else { crate::settings::ui_bindable().contains(key) };
            if let Some(key) = key.filter(allowed) {
                menus.sound("confirmMenu", 1.0);
                chosen_key = Some(Some(key));
            }
        }
        if let Some(key) = chosen_key {
            let slot = self.alt as usize;
            match bind {
                Bind::Lane(lane) => settings.bind(lane, slot, key),
                Bind::Action(action) => {
                    // a key is not bound twice to the same thing
                    let keys = &mut settings.ui_keys[action];
                    if key.is_some() && keys[1 - slot] == key {
                        keys[1 - slot] = None;
                    }
                    keys[slot] = key;
                    crate::controls::set(settings.ui_keys);
                }
                _ => {}
            }
            self.binding = false;
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
        change(option("playback_rate"), &mut settings, 10.0);
        assert_eq!(shown(option("playback_rate"), &mut settings), "1.50X");
        change(option("rating_y"), &mut settings, -2.0);
        assert_eq!((settings.combo_offset, shown(option("rating_y"), &mut settings).as_str()), ([0.0, -10.0, 0.0, 0.0], "-10 px"));
    }

    #[test]
    fn a_language_that_is_gone_counts_as_english() {
        let languages: Vec<(String, String)> = [("de", "Deutsch"), ("en-US", "English (US)"), ("pt-BR", "Português (Brasil)")].map(|(file, name)| (file.to_owned(), name.to_owned())).into();
        assert_eq!([0, 1, 2].map(|steps| next_language(&languages, "pt-BR", steps)), [2, 0, 1]);
        // its mod was switched off: English is shown, and nothing changes until a key is pressed
        assert_eq!((next_language(&languages, "fr", 0), next_language(&languages, "fr", 1)), (1, 2));
        assert_eq!(next_language(&[], "fr", 1), 0);
    }

    #[test]
    fn keys_are_named_as_in_the_haxe_engine() {
        use KeyCode::*;
        let names: Vec<String> = [Some(A), Some(Key7), Some(Kp3), Some(KpAdd), Some(Backspace), Some(LeftShift), Some(Left), None].map(key_name).to_vec();
        assert_eq!(names, ["A", "7", "#3", "#+", "BckSpc", "Left Shift", "Left", "---"]);
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
