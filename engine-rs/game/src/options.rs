//! The options menu: key binds, and the pages of `settings::OPTIONS`.
use crate::controls::{pressed, Ui, NAMES};
use crate::settings::{Kind, Settings, LANE_COLORS, OPTIONS, PAGES, PIXEL_LANE_COLORS};
use crate::{DIRECTIONS, HEIGHT};
use crate::gfx::*;
use funkin_core::lang::phrase;
use funkin_core::mod_settings::{Kind as ModKind, ModOption};
use serde_json::{json, Value};

#[derive(Default)]
pub struct OptionsMenu {
    /// The page that is open; none for the list of pages.
    page: Option<usize>,
    row: usize,
    /// The selected key slot waits for a key.
    binding: bool,
    /// Opened straight at a page: leaving the page leaves the options.
    direct: bool,
    /// Note colours: those of pixel stages are shown; the lane and the part (body, shine, outline) chosen, and whether Left
    /// and Right choose among lanes; the digit of the colour code being typed; what the mouse holds on to, and the colour
    /// when it took hold; the pictures made for the page.
    pixel: bool,
    lane: usize,
    part: usize,
    on_lanes: bool,
    typing: Option<usize>,
    typed_at: f64,
    dragging: Option<Drag>,
    stored: u32,
    made: Option<Made>,
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
    /// How long Left or Right has been held on a page, and the value it has scrolled to (a mod's settings).
    held: f32,
    hold_value: f64,
    /// A mod's own settings, while they are open: its folder and what it describes.
    mod_page: Option<(String, Vec<ModOption>)>,
}

/// How an option shows beside its name: a tick box, or its value.
enum Shown {
    Switch(bool),
    Value(String),
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

/// Light squares drifting down and to the right, over the key binds and the note colours.
fn grid() {
    let drift = (get_time() * 40.0 % 160.0) as f32;
    for row in -1..5 {
        for column in -1..8 {
            for corner in [0.0, 80.0] {
                draw_rectangle(column as f32 * 160.0 + drift + corner, row as f32 * 160.0 + drift + corner, 80.0, 80.0, Color::new(1.0, 1.0, 1.0, 0.2));
            }
        }
    }
}

/// Large letters at `scale`, centred on `top`'s x with their top at its y.
fn small(menus: &crate::menu::Menus, skin: &crate::Skin, text: &str, bold: bool, top: Vec2, scale: f32, color: Color) {
    push_model_matrix(Mat4::from_translation(vec3(top.x - menus.width(skin, text, bold) * scale / 2.0, top.y, 0.0)) * Mat4::from_scale(vec3(scale, scale, 1.0)));
    menus.write_in(skin, text, bold, 0.0, 0.0, 1.0, color);
    pop_model_matrix();
}

/// What the mouse holds on to among the note colours.
#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Wheel,
    Shade,
}

/// The pictures the note colours' page makes for itself: the dot on the wheel, the shade from white to black, and the
/// palette's colours to pick from.
struct Made {
    dot: Texture2D,
    shade: Texture2D,
    palette: Option<Image>,
}

/// A colour's hue (degrees), saturation and brightness, as Flixel reckons them.
fn to_hsv(color: u32) -> (f32, f32, f32) {
    let [r, g, b] = [16, 8, 0].map(|shift| (color >> shift & 0xFF) as f32 / 255.0);
    let (high, low) = (r.max(g).max(b), r.min(g).min(b));
    let range = high - low;
    let hue = if range == 0.0 {
        0.0
    } else if high == r {
        60.0 * ((g - b) / range).rem_euclid(6.0)
    } else if high == g {
        60.0 * ((b - r) / range + 2.0)
    } else {
        60.0 * ((r - g) / range + 4.0)
    };
    (hue, if high == 0.0 { 0.0 } else { range / high }, high)
}

fn from_hsv(hue: f32, saturation: f32, brightness: f32) -> u32 {
    let chroma = brightness * saturation;
    let side = chroma * (1.0 - ((hue / 60.0).rem_euclid(2.0) - 1.0).abs());
    let [r, g, b] = match (hue.rem_euclid(360.0) / 60.0) as u32 {
        0 => [chroma, side, 0.0],
        1 => [side, chroma, 0.0],
        2 => [0.0, chroma, side],
        3 => [0.0, side, chroma],
        4 => [side, 0.0, chroma],
        _ => [chroma, 0.0, side],
    };
    [r, g, b].iter().fold(0, |code, channel| code << 8 | ((channel + brightness - chroma) * 255.0).round() as u32)
}

/// A key kept by its Flixel name, as the key binds' page names keys.
fn flx_shown(name: &str) -> String {
    let known = crate::controls::key_named(name);
    known.map_or_else(|| if name == "NONE" { key_name(None) } else { name.to_owned() }, |key| key_name(Some(key)))
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
        Kind::Choice(get, names) => names()[*get(settings)].to_owned(),
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
            let names = names();
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
    /// Opened from a song's pause menu: the note colours start on the pixel notes when the stage has them.
    pub fn pixel(pixel: bool) -> OptionsMenu {
        OptionsMenu { pixel, ..Default::default() }
    }

    /// Opened at a mod's own settings.
    pub fn for_mod(folder: &str, options: Vec<ModOption>) -> OptionsMenu {
        OptionsMenu { mod_page: Some((folder.to_owned(), options)), direct: true, ..Default::default() }
    }

    /// Opened at the page of that name, if there is one.
    pub fn at(page: &str) -> OptionsMenu {
        let page = PAGES.iter().position(|name| name.eq_ignore_ascii_case(page));
        OptionsMenu { page, direct: page.is_some(), ..Default::default() }
    }

    /// Runs and draws one frame. `text` writes a line with its baseline at (x, y). Returns true when the player leaves the options.
    /// `note` draws a frame of a lane's notes (of pixel stages or not) centred on a point, at the size that makes the strum
    /// as wide as asked, with its alpha.
    /// Whether the screen with its own music (delay and combo) is open.
    pub fn adjusting(&self) -> bool {
        self.adjust.is_some()
    }

    /// `languages`: the languages there are, as (file, name). `base`: the game's folder. `menus` lends its letters, pictures and
    /// sounds, `skin` the plain font.
    #[allow(clippy::too_many_arguments)]
    pub fn run(&mut self, settings: &mut Settings, menus: &mut crate::menu::Menus, skin: &crate::Skin, text: &dyn Fn(&str, f32, f32, u16, Color), note: &dyn Fn(&str, usize, bool, Vec2, f32, f32), raw_input: bool, bindable: &[KeyCode], languages: &[(String, String)], base: &std::path::Path) -> bool {
        if let Some(adjust) = &mut self.adjust {
            if adjust.run(settings, text) {
                self.adjust = None;
            }
            return false;
        }
        let dt = get_frame_time().min(0.1);
        let step = |rows: usize| (pressed(Ui::Down) as usize + pressed(Ui::Up) as usize * (rows - 1)) % rows;
        let back = pressed(Ui::Back);
        // the Haxe engine's look: the pink background (the key binds' blue), lists in large letters
        menus.backdrop(if self.mod_page.is_some() { Vec3::ONE } else if self.page == Some(0) { BLUE } else { PINK }, dt);
        let colors = self.page == Some(PAGES.len() - 1);
        if self.binding {
        } else if back {
            menus.sound("cancelMenu", 1.0);
        } else if (pressed(Ui::Up) || pressed(Ui::Down)) && !colors {
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

        if self.mod_page.is_some() {
            return self.mod_settings(settings, menus, skin, back, dt);
        }
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
                    name => (self.page, self.row, self.scroll, self.alt) = (PAGES.iter().position(|page| *page == name), 0, 0.0, false),
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
            return self.colors(settings, menus, skin, text, note, back);
        }

        // a page of options: its name faint at the top, the options in a slanted list gliding to the chosen one (switches with
        // a tick box before them, the others with their value after), what the chosen one does in a box at the bottom
        let options: Vec<_> = OPTIONS.iter().filter(|option| option.page == page).collect();
        self.row = (self.row + step(options.len())) % options.len();
        let title = match PAGES[page] {
            "Gameplay Changers" => phrase("gameplay_changers", "Gameplay Changers"),
            name => phrase(&format!("{}_menu", name.to_lowercase()), &format!("{name} Settings")),
        };
        let rows: Vec<(String, Shown)> = options.iter().map(|option| {
            let name = phrase(&format!("setting_{}", option.name), option.name);
            let shown = match option.kind {
                Kind::Flag(get) => Shown::Switch(*get(settings)),
                // a choice among names may have its names translated too
                Kind::Choice(..) => {
                    let value = shown(&option.kind, settings);
                    Shown::Value(phrase(&format!("setting_{}-{value}", option.name), &value))
                }
                _ => Shown::Value(shown(&option.kind, settings)),
            };
            (name, shown)
        }).collect();
        let selected = options[self.row];
        self.draw_page(menus, skin, &title, &rows, &phrase(&format!("description_{}", selected.name), selected.about), dt);
        // held, a number keeps going after half a second
        let held = if matches!(selected.kind, Kind::Number(..)) { self.held_steps(dt) } else { 0.0 };
        if steps != 0.0 || held != 0.0 {
            change(&selected.kind, settings, if steps != 0.0 { steps * fast } else { held * fast });
            if steps != 0.0 {
                menus.sound("scrollMenu", 1.0);
            }
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
        } else if pressed(Ui::Reset) {
            // back to how it was at first
            let mut usual = Settings::default();
            match selected.kind {
                Kind::Flag(get) => *get(settings) = *get(&mut usual),
                Kind::Number(get, ..) => *get(settings) = *get(&mut usual),
                Kind::Choice(get, _) => *get(settings) = *get(&mut usual),
            }
            menus.sound("cancelMenu", 1.0);
        }
        if back {
            (self.page, self.row) = (None, main_row(page));
        }
        back && self.direct
    }

    /// Steps Left or Right held down has made this frame: none for half a second, then twenty-five a second.
    // shortcut: one pace for every number; the Haxe engine gives each option its own (most scroll 50 a second). Give
    // OPTIONS a pace of its own if one feels wrong.
    fn held_steps(&mut self, dt: f32) -> f64 {
        let (left, right) = (crate::controls::down(Ui::Left), crate::controls::down(Ui::Right));
        if left == right {
            self.held = 0.0;
            return 0.0;
        }
        let ticks = |held: f32| ((held - 0.5) * 25.0).floor().max(0.0);
        let before = self.held;
        self.held += dt;
        (ticks(self.held) - ticks(before)) as f64 * if right { 1.0 } else { -1.0 }
    }

    /// A page of options as the Haxe engine draws one: its name faint at the top, the options in a slanted list gliding to
    /// the chosen one (switches with a tick box before them, the others with their value after), what the chosen one does
    /// in a box at the bottom.
    fn draw_page(&mut self, menus: &mut crate::menu::Menus, skin: &crate::Skin, title: &str, rows: &[(String, Shown)], about: &str, dt: f32) {
        self.scroll += (self.row as f32 - self.scroll) * (1.0 - (-dt * 9.6).exp());
        push_model_matrix(Mat4::from_translation(vec3(75.0, 45.0, 0.0)) * Mat4::from_scale(vec3(0.6, 0.6, 1.0)));
        menus.write(skin, title, true, 0.0, 0.0, 1.0, 0.4);
        pop_model_matrix();
        let since = (get_time() - self.flipped) as f32;
        for (index, (name, shown)) in rows.iter().enumerate() {
            let at = listed(index as f32 - self.scroll, vec2(if matches!(shown, Shown::Switch(_)) { 220.0 } else { 140.0 }, 260.0));
            if !(-200.0..HEIGHT + 100.0).contains(&at.y) {
                continue;
            }
            let alpha = if index == self.row { 1.0 } else { 0.6 };
            menus.write(skin, name, false, at.x, at.y, 1.0, alpha);
            match shown {
                Shown::Switch(on) => checkbox(menus, *on, if index == self.row { since } else { 60.0 }, at, alpha),
                Shown::Value(value) => menus.write(skin, value, false, at.x + menus.width(skin, name, false) + 60.0, at.y, 1.0, alpha),
            }
        }
        let about = crate::menu::wrap(about, 1180.0, |words| measure_text(words, skin.font.as_ref(), 32, 1.0).width);
        let top = (HEIGHT - about.len() as f32 * 32.0) / 2.0 + 270.0;
        draw_rectangle(40.0, top - 10.0, 1200.0, about.len() as f32 * 32.0 + 25.0, Color::new(0.0, 0.0, 0.0, 0.6));
        for (row, words) in about.iter().enumerate() {
            let width = measure_text(words, skin.font.as_ref(), 32, 1.0).width;
            skin.text(words, (crate::WIDTH - width) / 2.0, top + 26.0 + row as f32 * 32.0, 32, WHITE);
        }
    }

    /// Waits for a key for `what` over a whitened screen, as the Haxe engine does: Escape or Backspace held half a second
    /// gives up or asks for none, a short press of either is taken once let go; only keys `allowed` count. The key (or none)
    /// once there is one; nothing meanwhile, or when given up.
    fn rebind(&mut self, menus: &mut crate::menu::Menus, skin: &crate::Skin, what: &str, dt: f32, allowed: impl Fn(&KeyCode) -> bool) -> Option<Option<KeyCode>> {
        let since = (get_time() - self.flipped) as f32;
        draw_rectangle(0.0, 0.0, crate::WIDTH, HEIGHT, Color::new(1.0, 1.0, 1.0, 0.6 * (since / 0.35).min(1.0)));
        let rebinding = funkin_core::lang::phrase_with("controls_rebinding", "Rebinding {1}", &[what]);
        menus.write(skin, &rebinding, false, (crate::WIDTH - menus.width(skin, &rebinding, false)) / 2.0, 160.0, 1.0, 1.0);
        for (line, words) in phrase("controls_rebinding2", "Hold ESC to Cancel\nHold Backspace to Delete").lines().enumerate() {
            menus.write(skin, words, true, (crate::WIDTH - menus.width(skin, words, true)) / 2.0, 340.0 + line as f32 * 85.0, 1.0, 1.0);
        }
        let (escape, backspace) = (is_key_down(KeyCode::Escape), is_key_down(KeyCode::Backspace));
        if escape || backspace {
            self.holding += dt;
            if self.holding > 0.5 {
                menus.sound("cancelMenu", 1.0);
                self.binding = false;
                return backspace.then_some(None);
            }
            return None;
        }
        self.holding = 0.0;
        let tapped = [KeyCode::Escape, KeyCode::Backspace].into_iter().find(|&key| is_key_released(key));
        let key = get_last_key_pressed().filter(|key| !matches!(key, KeyCode::Escape | KeyCode::Backspace)).or(tapped).filter(allowed)?;
        menus.sound("confirmMenu", 1.0);
        self.binding = false;
        Some(Some(key))
    }

    /// A mod's own settings, as the Haxe engine's page for them: like the other pages, a key among them bound as on the key
    /// binds' page. Kept in the settings by the mod's folder. True when the player leaves.
    fn mod_settings(&mut self, settings: &mut Settings, menus: &mut crate::menu::Menus, skin: &crate::Skin, back: bool, dt: f32) -> bool {
        let Some((folder, options)) = self.mod_page.take() else { return true };
        if options.is_empty() {
            return true;
        }
        let saved = settings.mod_settings.entry(folder.clone()).or_insert_with(|| json!({}));
        if !saved.is_object() {
            *saved = json!({});
        }
        let Value::Object(saved) = saved else { return true };
        if !self.binding {
            let count = options.len();
            self.row = (self.row + pressed(Ui::Down) as usize + pressed(Ui::Up) as usize * (count - 1)) % count;
        }
        let option = &options[self.row];
        let rows: Vec<(String, Shown)> = options.iter().map(|option| {
            let value = option.value(saved);
            let shown = match option.kind {
                ModKind::Bool => Shown::Switch(value.as_bool() == Some(true)),
                ModKind::Key => Shown::Value(flx_shown(value.as_str().unwrap_or("NONE"))),
                ModKind::Text(_) => {
                    let value = option.shown(&value);
                    Shown::Value(phrase(&format!("setting_{}-{value}", option.translation), &value))
                }
                _ => Shown::Value(option.shown(&value)),
            };
            (phrase(&format!("setting_{}", option.translation), &option.name), shown)
        }).collect();
        self.draw_page(menus, skin, "", &rows, &phrase(&format!("description_{}", option.translation), &option.about), dt);

        let value = option.value(saved);
        if self.binding {
            if let Some(key) = self.rebind(menus, skin, &option.name, dt, |key| crate::controls::flx_name(*key).is_some()) {
                option.store(saved, json!(key.and_then(crate::controls::flx_name).unwrap_or_else(|| "NONE".to_owned())));
            }
        } else if back {
            self.binding = false;
            return true;
        } else if pressed(Ui::Accept) && matches!(option.kind, ModKind::Bool | ModKind::Key) {
            menus.sound("scrollMenu", 1.0);
            if option.kind == ModKind::Key {
                (self.binding, self.holding, self.flipped) = (true, 0.0, get_time());
            } else {
                option.store(saved, option.stepped(&value, 0.0));
                self.flipped = get_time();
            }
        } else if pressed(Ui::Reset) {
            option.store(saved, option.default.clone());
            menus.sound("cancelMenu", 1.0);
        } else if !matches!(option.kind, ModKind::Bool | ModKind::Key) {
            let steps = pressed(Ui::Right) as i32 - pressed(Ui::Left) as i32;
            // (only how long it has been held counts here, not its steps)
            self.held_steps(dt);
            if steps != 0 {
                let next = option.stepped(&value, steps as f64);
                self.hold_value = next.as_f64().unwrap_or(0.0);
                option.store(saved, next);
                menus.sound("scrollMenu", 1.0);
            } else if !matches!(option.kind, ModKind::Text(_)) && self.held > 0.5 {
                // held, it runs on at the mod's own pace
                let way = if crate::controls::down(Ui::Right) { 1.0 } else { -1.0 };
                self.hold_value = (self.hold_value + option.scroll * dt as f64 * way).clamp(option.min.unwrap_or(f64::MIN), option.max.unwrap_or(f64::MAX));
                option.store(saved, option.stepped(&json!(self.hold_value), 0.0));
            }
        }
        self.mod_page = Some((folder, options));
        false
    }

    /// The note colours' page, as the Haxe engine's: the three parts of a note above the four lanes' strums and the chosen
    /// lane's note, large; on the right the colour wheel, its brightness, a palette, and the colour's numbers and code. Up and
    /// Down switch between parts and lanes, Left and Right choose, Ctrl shows the notes of pixel stages; the mouse picks
    /// colours and, clicked on the code, types it (Enter does too, here).
    #[allow(clippy::too_many_arguments)]
    fn colors(&mut self, settings: &mut Settings, menus: &mut crate::menu::Menus, skin: &crate::Skin, text: &dyn Fn(&str, f32, f32, u16, Color), note: &dyn Fn(&str, usize, bool, Vec2, f32, f32), back: bool) -> bool {
        if back {
            (self.page, self.row, self.typing, self.dragging) = (None, main_row(PAGES.len() - 1), None, None);
            return self.direct;
        }
        let (mouse, clicked) = (mouse_position(vec2(crate::WIDTH, HEIGHT)), is_mouse_pressed());
        let on = |at: Vec2, size: Vec2| (at.x..at.x + size.x).contains(&mouse.x) && (at.y..at.y + size.y).contains(&mouse.y);
        let toggled = is_key_pressed(KeyCode::LeftControl) || is_key_pressed(KeyCode::RightControl) || (clicked && on(vec2(48.0, 24.0), vec2(68.0, 68.0)));
        if toggled {
            self.pixel ^= true;
            menus.sound("scrollMenu", 0.6);
        }
        let made = self.made.get_or_insert_with(|| {
            let mut dot = Image::gen_image_color(16, 16, Color::new(1.0, 1.0, 1.0, 0.0));
            let mut shade = Image::gen_image_color(1, 360, WHITE);
            for at in 0..16 * 16 {
                if (vec2((at % 16) as f32, (at / 16) as f32) - 7.5).length() <= 8.0 {
                    dot.set_pixel(at % 16, at / 16, WHITE);
                }
            }
            for y in 0..360 {
                let light = 1.0 - y as f32 / 359.0;
                shade.set_pixel(0, y, Color::new(light, light, light, 1.0));
            }
            let palette = menus.find("images/noteColorMenu/palette.png").and_then(|path| Image::from_file_with_format(&std::fs::read(path).ok()?, None).ok());
            Made { dot: Texture2D::from_image(&dot), shade: Texture2D::from_image(&shade), palette }
        });
        let (dot, shade) = (made.dot.clone(), made.shade.clone());
        let (colors, usual) = if self.pixel { (&mut settings.pixel_note_colors, PIXEL_LANE_COLORS) } else { (&mut settings.note_colors, LANE_COLORS) };
        let (lane, part) = (self.lane, self.part);
        let code = format!("{:06X}", colors[lane][part]);
        // the code is written centred at x 980, at six tenths of the letters' size
        let code_left = 980.0 - menus.width(skin, &code, true) * 0.3;
        let digit_end: [f32; 7] = std::array::from_fn(|count| code_left + menus.width(skin, &code[..count], true) * 0.6);

        if let Some(at) = self.typing {
            let typed = std::iter::from_fn(get_char_pressed).find(char::is_ascii_hexdigit);
            let moved = if is_key_pressed(KeyCode::Left) {
                Some(at as i32 - 1)
            } else if is_key_pressed(KeyCode::Right) {
                Some(at as i32 + 1)
            } else if let Some(digit) = typed {
                // the digit replaces the one at the cursor, which moves on to the next
                let mut digits = code.clone().into_bytes();
                digits[at] = digit.to_ascii_uppercase() as u8;
                colors[lane][part] = u32::from_str_radix(std::str::from_utf8(&digits).unwrap_or("0"), 16).unwrap_or(0);
                self.stored = colors[lane][part];
                Some(at as i32 + 1)
            } else {
                None
            };
            if let Some(moved) = moved {
                // past the last digit, the typing is done
                self.typing = (moved <= 5).then_some(moved.max(0) as usize);
                self.typed_at = get_time();
                menus.sound("scrollMenu", 0.6);
            } else if is_key_pressed(KeyCode::Enter) {
                self.typing = None;
            }
        } else {
            let steps = pressed(Ui::Right) as usize + pressed(Ui::Left) as usize * 2;
            if steps > 0 {
                if self.on_lanes {
                    self.lane = (self.lane + steps + steps / 2) % 4;
                } else {
                    self.part = (self.part + steps) % 3;
                }
                menus.sound("scrollMenu", 1.0);
            }
            if pressed(Ui::Up) || pressed(Ui::Down) {
                self.on_lanes ^= true;
            }
            if pressed(Ui::Accept) {
                (self.typing, self.typed_at) = (Some(0), get_time());
            }
        }

        if clicked {
            self.typing = None;
            let part_at = (0..3).find(|&part| on(vec2(230.0 + 100.0 * part as f32, 100.0), vec2(85.0, 85.0)));
            let lane_at = (0..4).find(|&lane| on(vec2(150.0 + 120.0 * lane as f32, 200.0), vec2(102.0, 102.0)));
            if let Some(part) = part_at {
                if part != self.part {
                    (self.part, self.on_lanes) = (part, false);
                    menus.sound("scrollMenu", 0.6);
                }
            } else if let Some(lane) = lane_at {
                if lane != self.lane {
                    (self.lane, self.on_lanes) = (lane, true);
                    menus.sound("scrollMenu", 0.6);
                }
            } else if on(vec2(860.0, 200.0), vec2(360.0, 360.0)) {
                (self.dragging, self.stored) = (Some(Drag::Wheel), colors[lane][part]);
            } else if on(vec2(780.0, 200.0), vec2(60.0, 360.0)) {
                (self.dragging, self.stored) = (Some(Drag::Shade), colors[lane][part]);
            } else if on(vec2(820.0, 580.0), vec2(320.0, 80.0)) {
                // (a mod's palette may be of another size: it is stretched over the same place)
                if let Some(palette) = self.made.as_ref().and_then(|made| made.palette.as_ref()).filter(|palette| palette.width > 0 && palette.height > 0) {
                    let (x, y) = ((mouse.x - 820.0) / 320.0 * palette.width as f32, (mouse.y - 580.0) / 80.0 * palette.height as f32);
                    let picked = palette.get_pixel((x as u32).min(palette.width as u32 - 1), (y as u32).min(palette.height as u32 - 1));
                    colors[lane][part] = [picked.r, picked.g, picked.b].iter().fold(0, |code, channel| code << 8 | (channel * 255.0).round() as u32);
                    menus.sound("scrollMenu", 0.6);
                }
            } else if (20.0..82.0).contains(&mouse.y) && (mouse.x - 1000.0).abs() <= 84.0 {
                // the cursor goes before the digit clicked on
                (self.typing, self.typed_at) = (Some((1..=6).take_while(|&count| digit_end[count] <= mouse.x).count().min(5)), get_time());
            } else {
                self.dragging = None;
            }
        }
        if let Some(drag) = self.dragging {
            if !is_mouse_down() {
                self.dragging = None;
                self.stored = colors[lane][part];
                menus.sound("scrollMenu", 0.6);
            } else if mouse_moved() || clicked {
                let (hue, saturation, brightness) = to_hsv(self.stored);
                let grey = |light: f32| from_hsv(0.0, 0.0, light);
                colors[lane][part] = match drag {
                    Drag::Shade => {
                        let light = 1.0 - ((mouse.y - 200.0) / 360.0).clamp(0.0, 1.0);
                        if brightness == 0.0 { grey(light) } else { from_hsv(hue, saturation, light) }
                    }
                    Drag::Wheel => {
                        // straight up is red, and the further out the stronger
                        let towards = vec2(1040.0, 380.0) - mouse;
                        let hue = (towards.y.atan2(towards.x).to_degrees().trunc() - 90.0).rem_euclid(360.0);
                        let saturation = (towards.length() / 180.0).min(1.0);
                        if saturation == 0.0 { grey(brightness) } else { from_hsv(hue, saturation, brightness) }
                    }
                };
            }
        } else if pressed(Ui::Reset) && self.typing.is_none() {
            // with Shift, the whole note
            if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
                colors[lane] = usual[lane];
            }
            colors[lane][part] = usual[lane][part];
            menus.sound("cancelMenu", 0.6);
        }

        // the left side: the parts, the lanes, the chosen lane's note large
        let (lane, part, color) = (self.lane, self.part, colors[self.lane][self.part]);
        let (box_at, box_size) = if self.on_lanes { (vec2(140.0, 190.0), vec2(480.0, 125.0)) } else { (vec2(215.0, 85.0), vec2(315.0, 115.0)) };
        draw_rectangle(box_at.x, box_at.y, box_size.x, box_size.y, Color::new(0.0, 0.0, 0.0, 0.4));
        for (pixel, picture) in [(false, "noteColorMenu/note"), (true, "noteColorMenu/notePixel")] {
            let Some(parts) = menus.picture(picture) else { continue };
            let cell = if pixel { 17.0 } else { 160.0 };
            parts.set_filter(if pixel { FilterMode::Nearest } else { FilterMode::Linear });
            let frame = |index: usize, at: Vec2, size: f32, alpha: f32| {
                let params = DrawTextureParams { dest_size: Some(vec2(size, size)), source: Some(Rect::new(index as f32 * cell, 0.0, cell, cell)), ..Default::default() };
                draw_texture_ex(&parts, at.x, at.y, Color::new(1.0, 1.0, 1.0, alpha), params);
            };
            if pixel == self.pixel {
                for index in 0..3 {
                    frame(index, vec2(230.0 + 100.0 * index as f32, 100.0), 85.0, if index == part { 1.0 } else { 0.6 });
                }
            } else {
                // the other kind of note, to switch to
                frame(0, vec2(48.0, 24.0), 68.0, 1.0);
            }
        }
        for index in 0..4 {
            let pose = if index == lane { "confirm" } else { "press" };
            note(&format!("{} {pose}0003", DIRECTIONS[index]), index, self.pixel, vec2(201.0 + 120.0 * index as f32, 251.0), 102.0, if index == lane { 1.0 } else { 0.6 });
        }
        note(&format!("{}0000", ["purple", "blue", "green", "red"][lane]), lane, self.pixel, vec2(375.0, 450.0), 250.0, 1.0);

        // the right side: the wheel, the brightness, the palette, the colour's numbers and code
        draw_rectangle(720.0, 0.0, crate::WIDTH - 720.0, HEIGHT, Color::new(0.0, 0.0, 0.0, 0.25));
        draw_rectangle(750.0, 160.0, crate::WIDTH - 780.0, 540.0, Color::new(0.0, 0.0, 0.0, 0.25));
        small(menus, skin, "CTRL", false, vec2(50.0, 86.0), 0.4, WHITE);
        let (_, _, brightness) = to_hsv(color);
        draw_texture_ex(&shade, 780.0, 200.0, WHITE, DrawTextureParams { dest_size: Some(vec2(60.0, 360.0)), ..Default::default() });
        draw_rectangle(770.0, 195.0 + 360.0 * (1.0 - brightness), 80.0, 10.0, WHITE);
        if let Some(palette) = menus.picture("noteColorMenu/palette") {
            palette.set_filter(FilterMode::Nearest);
            draw_texture_ex(&palette, 820.0, 580.0, WHITE, DrawTextureParams { dest_size: Some(vec2(320.0, 80.0)), ..Default::default() });
        }
        if let Some(wheel) = menus.picture("noteColorMenu/colorWheel") {
            draw_texture_ex(&wheel, 860.0, 200.0, Color::new(brightness, brightness, brightness, 1.0), DrawTextureParams { dest_size: Some(vec2(360.0, 360.0)), ..Default::default() });
        }
        // the dot shows the colour held on to while its brightness is being dragged
        let (hue, saturation, light) = to_hsv(if self.dragging == Some(Drag::Shade) { self.stored } else { color });
        let reach = if light == 0.0 { Vec2::ZERO } else { vec2(hue.to_radians().sin(), -hue.to_radians().cos()) * 180.0 * saturation };
        draw_texture(&dot, 1032.0 + reach.x, 372.0 + reach.y, Color::new(1.0, 1.0, 1.0, 0.6));
        for (index, channel) in [16, 8, 0].into_iter().enumerate() {
            small(menus, skin, &(color >> channel & 0xFF).to_string(), true, vec2(880.0 + 100.0 * index as f32, 90.0), 0.6, WHITE);
        }
        small(menus, skin, &format!("{color:06X}"), true, vec2(980.0, 35.0), 0.6, Color::from_hex(color));
        if let Some(at) = self.typing.filter(|_| ((get_time() - self.typed_at) * 2.0) as i64 % 2 == 0) {
            draw_rectangle(digit_end[at] + 5.0, 20.0, 5.0, 62.0, WHITE);
        }
        text(&phrase("note_colors_tip", "Press RESET to Reset the selected Note Part."), 20.0, 674.0, 16, WHITE);
        let shift = phrase("note_colors_shift", "Shift");
        text(&funkin_core::lang::phrase_with("note_colors_hold_tip", "Hold {1} + Press RESET key to fully reset the selected Note.", &[&shift]), 20.0, 698.0, 16, WHITE);
        false
    }

    /// The key binds' page, as the Haxe engine's: a row for each thing with its two keys, Left/Right choosing which; Enter
    /// waits for a key (Escape or Backspace held half a second: give up, or leave none).
    #[allow(clippy::too_many_arguments)]
    fn controls(&mut self, settings: &mut Settings, menus: &mut crate::menu::Menus, skin: &crate::Skin, text: &dyn Fn(&str, f32, f32, u16, Color), dt: f32, back: bool, raw_input: bool, bindable: &[KeyCode]) -> bool {
        grid();
        if let Some(sign) = menus.picture("controllertype") {
            draw_texture_ex(&sign, 50.0, 40.0, WHITE, DrawTextureParams { source: Some(Rect::new(0.0, 0.0, 82.0, 60.0)), ..Default::default() });
        }
        small(menus, skin, "CTRL", false, vec2(60.0, 90.0), 0.4, WHITE);

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

        // waiting for a key
        let allowed = |key: &KeyCode| if matches!(bind, Bind::Lane(_)) { bindable.contains(key) } else { crate::settings::ui_bindable().contains(key) };
        let chosen_key = self.rebind(menus, skin, BINDS[chosen].2, dt, allowed);
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
    fn colours_go_to_hue_saturation_brightness_and_back() {
        assert_eq!(to_hsv(0xFF0000), (0.0, 1.0, 1.0));
        assert_eq!(to_hsv(0x000000), (0.0, 0.0, 0.0));
        for color in [0xC24B99, 0x00FFFF, 0x12FA05, 0x651038, 0x808080, 0xFFFFFF] {
            let (hue, saturation, brightness) = to_hsv(color);
            assert_eq!(from_hsv(hue, saturation, brightness), color, "{color:06X}");
        }
    }

    #[test]
    fn mods_keep_keys_by_their_flixel_names() {
        use KeyCode::*;
        assert_eq!([Key3, Kp0, Space, LeftShift, Equal, Left].map(|key| crate::controls::flx_name(key).unwrap()), ["THREE", "NUMPADZERO", "SPACE", "SHIFT", "PLUS", "LEFT"]);
        assert_eq!((flx_shown("SPACE"), flx_shown("NONE"), flx_shown("THREE")), ("Space".to_owned(), "---".to_owned(), "3".to_owned()));
    }

    #[test]
    fn keys_are_named_as_in_the_haxe_engine() {
        use KeyCode::*;
        let names: Vec<String> = [Some(A), Some(Key7), Some(Kp3), Some(KpAdd), Some(Backspace), Some(LeftShift), Some(Left), None].map(key_name).to_vec();
        assert_eq!(names, ["A", "7", "#3", "#+", "BckSpc", "Left Shift", "Left", "---"]);
    }
}
