//! The editors' controls, as the Haxe engine's PsychUI draws them (backend/ui): boxes with tabs, buttons, tick
//! boxes, numbers with - and + (PsychUINumericStepper), typed text (PsychUIInputText) and lists that drop down.
//! Immediate: each frame the editor asks for its controls where they stand and hears what was done with them; what
//! they look like is kept and drawn with the rest of the editor.
use crate::gfx::*;

enum Shape {
    Rect(Rect, Color),
    Text(String, Vec2, Color),
}

#[derive(Default)]
pub struct Ui {
    shapes: Vec<Shape>,
    /// Drawn over the rest: an open list.
    over: Vec<Shape>,
    /// This frame's click was taken by a control (or fell on a box): the editor behind does not get it.
    taken: bool,
    /// The text being typed into, and what it holds so far.
    focus: Option<(&'static str, String)>,
    /// What was typed into a text left for another before it was given back.
    pending: Option<(&'static str, String)>,
    /// The list that is open, and how far it is scrolled.
    open: Option<(&'static str, usize)>,
    /// The boxes drawn this frame: clicks on them are not the editor's.
    boxes: Vec<Rect>,
    mouse: Vec2,
    clicked: bool,
}

const TEXT: f32 = 14.0;
const ROWS: usize = 10;

fn inside(rect: Rect, at: Vec2) -> bool {
    (rect.x..rect.x + rect.w).contains(&at.x) && (rect.y..rect.y + rect.h).contains(&at.y)
}

impl Ui {
    /// A new frame: the mouse as it is now.
    pub fn begin(&mut self) {
        self.shapes.clear();
        self.over.clear();
        self.boxes.clear();
        self.mouse = mouse_position(vec2(crate::WIDTH, crate::HEIGHT));
        self.clicked = is_mouse_pressed();
        self.taken = false;
    }

    /// Whether this frame's click went to the controls or fell on their boxes.
    pub fn took_click(&self) -> bool {
        self.taken || (self.clicked && self.boxes.iter().any(|area| inside(*area, self.mouse)))
    }

    /// Text is being typed: keys are not the editor's.
    pub fn typing(&self) -> bool {
        self.focus.is_some()
    }

    /// A click inside `rect` not yet taken by another control; taking it.
    fn click(&mut self, rect: Rect) -> bool {
        let hit = self.clicked && !self.taken && inside(rect, self.mouse);
        self.taken |= hit;
        hit
    }

    fn rect(&mut self, rect: Rect, color: Color) {
        self.shapes.push(Shape::Rect(rect, color));
    }

    pub fn label(&mut self, x: f32, y: f32, words: &str) {
        for (row, line) in words.lines().enumerate() {
            self.shapes.push(Shape::Text(line.to_owned(), vec2(x, y + TEXT + row as f32 * (TEXT + 2.0)), WHITE));
        }
    }

    /// A box (black, faded) with tabs along its top; the tab chosen (a click on another chooses it). Clicks inside
    /// the box are the box's.
    pub fn tabs(&mut self, area: Rect, names: &[&str], chosen: usize) -> usize {
        // each as wide as its name asks, the box's width shared out
        let wanted: Vec<f32> = names.iter().map(|name| name.len() as f32 + 2.0).collect();
        let unit = area.w / wanted.iter().sum::<f32>();
        let mut chosen = chosen;
        let mut left = area.x;
        for (index, name) in names.iter().enumerate() {
            let tab = Rect::new(left, area.y - 20.0, wanted[index] * unit, 20.0);
            left += tab.w;
            if self.click(tab) && index != chosen {
                // (the controls of the tab left are not asked for again: what was typed there, a list open, go)
                (chosen, self.focus, self.pending, self.open) = (index, None, None, None);
            }
            let shade = if index == chosen { 0.8 } else if inside(tab, self.mouse) { 0.5 } else { 0.3 };
            self.rect(tab, Color { a: shade, ..BLACK });
            self.shapes.push(Shape::Text(name.to_string(), vec2(tab.x + 6.0, tab.y + 15.0), WHITE));
        }
        self.rect(area, Color { a: 0.6, ..BLACK });
        self.boxes.extend([area, Rect::new(area.x, area.y - 20.0, area.w, 20.0)]);
        chosen
    }

    pub fn button(&mut self, x: f32, y: f32, width: f32, words: &str) -> bool {
        let area = Rect::new(x, y, width, 20.0);
        let pressed = self.click(area);
        let color = if inside(area, self.mouse) { Color::from_hex(0xFFFFFF) } else { Color::from_hex(0xAAAAAA) };
        self.rect(area, Color { a: if pressed { 1.0 } else { 0.6 }, ..color });
        self.shapes.push(Shape::Text(words.to_owned(), vec2(x + 4.0, y + 15.0), BLACK));
        pressed
    }

    /// A tick box with its words; the new state when clicked.
    pub fn check(&mut self, x: f32, y: f32, words: &str, ticked: bool) -> Option<bool> {
        let area = Rect::new(x, y, 18.0 + 6.0 + words.len() as f32 * 8.0, 18.0);
        let flipped = self.click(area);
        self.rect(Rect::new(x, y, 18.0, 18.0), WHITE);
        if ticked != flipped {
            self.rect(Rect::new(x + 4.0, y + 4.0, 10.0, 10.0), BLACK);
        }
        self.shapes.push(Shape::Text(words.to_owned(), vec2(x + 24.0, y + 14.0), WHITE));
        flipped.then_some(!ticked)
    }

    /// A line of text that can be typed into: a click starts typing, Enter or a click elsewhere keeps what was
    /// typed (given back then), Escape drops it.
    pub fn input(&mut self, id: &'static str, x: f32, y: f32, width: f32, value: &str) -> Option<String> {
        let area = Rect::new(x, y, width, 18.0);
        let mut done = self.pending.take_if(|(pending, _)| *pending == id).map(|(_, text)| text);
        let focused = self.focus.as_ref().is_some_and(|(focus, _)| *focus == id);
        if focused {
            let (_, text) = self.focus.as_mut().expect("focused");
            while let Some(letter) = get_char_pressed() {
                if !letter.is_control() {
                    text.push(letter);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                text.pop();
            }
            if is_key_pressed(KeyCode::Escape) {
                self.focus = None;
            } else if is_key_pressed(KeyCode::Enter) || (self.clicked && !inside(area, self.mouse)) {
                done = self.focus.take().map(|(_, text)| text);
            }
        } else if self.click(area) {
            // (what was typed into another is given back to it when it is next asked for)
            self.pending = self.focus.replace((id, value.to_owned()));
        }
        let shown = self.focus.as_ref().filter(|(focus, _)| *focus == id).map_or(value.to_owned(), |(_, text)| text.clone());
        self.rect(area, BLACK);
        self.rect(Rect::new(x + 1.0, y + 1.0, width - 2.0, 16.0), WHITE);
        let caret = if self.focus.as_ref().is_some_and(|(focus, _)| *focus == id) && get_time() % 1.0 < 0.5 { "|" } else { "" };
        self.shapes.push(Shape::Text(format!("{shown}{caret}"), vec2(x + 3.0, y + 14.0), BLACK));
        done
    }

    /// A number with - and + beside it (by `step`, kept between `min` and `max`), which can be typed in too.
    #[allow(clippy::too_many_arguments)]
    pub fn stepper(&mut self, id: &'static str, x: f32, y: f32, width: f32, value: f64, step: f64, min: f64, max: f64) -> Option<f64> {
        let shown = format!("{}", (value * 1000.0).round() / 1000.0);
        let typed = self.input(id, x, y, width, &shown).and_then(|text| text.trim().parse::<f64>().ok());
        let less = self.button(x + width + 2.0, y - 1.0, 20.0, " -");
        let more = self.button(x + width + 24.0, y - 1.0, 20.0, " +");
        let new = typed.or(less.then(|| value - step)).or(more.then(|| value + step))?;
        Some(new.clamp(min, max))
    }

    /// A list that drops down: what is chosen; a click opens it, the wheel scrolls it, a click on one chooses it.
    pub fn dropdown(&mut self, id: &'static str, x: f32, y: f32, width: f32, options: &[String], chosen: usize) -> Option<usize> {
        let area = Rect::new(x, y, width, 18.0);
        let mut picked = None;
        let is_open = self.open.is_some_and(|(open, _)| open == id);
        if is_open {
            let (_, scroll) = self.open.expect("open");
            let scroll = (scroll as f32 - mouse_wheel()).clamp(0.0, options.len().saturating_sub(ROWS) as f32) as usize;
            self.open = Some((id, scroll));
            for (row, index) in (scroll..options.len()).take(ROWS).enumerate() {
                let line = Rect::new(x, y + 18.0 * (row + 1) as f32, width, 18.0);
                let lit = inside(line, self.mouse);
                if self.click(line) {
                    picked = Some(index);
                }
                self.over.push(Shape::Rect(line, if lit { Color::from_hex(0x0066FF) } else { WHITE }));
                self.over.push(Shape::Text(options[index].clone(), vec2(x + 3.0, line.y + 14.0), if lit { WHITE } else { BLACK }));
            }
            // any click closes it, and goes no further
            if self.clicked {
                self.taken = true;
                self.open = None;
            }
        } else if self.click(area) {
            self.open = Some((id, chosen.saturating_sub(ROWS / 2)));
        }
        self.rect(area, BLACK);
        self.rect(Rect::new(x + 1.0, y + 1.0, width - 2.0, 16.0), WHITE);
        let shown = options.get(chosen).cloned().unwrap_or_default();
        self.shapes.push(Shape::Text(if shown.is_empty() { "(none)".to_owned() } else { shown }, vec2(x + 3.0, y + 14.0), BLACK));
        self.shapes.push(Shape::Text("v".to_owned(), vec2(x + width - 12.0, y + 14.0), BLACK));
        picked
    }

    /// Draws what the controls asked for this frame, the open list last.
    pub fn draw(&self, font: Option<&Font>) {
        for shape in self.shapes.iter().chain(&self.over) {
            match shape {
                Shape::Rect(rect, color) => draw_rectangle(rect.x, rect.y, rect.w, rect.h, *color),
                Shape::Text(words, at, color) => draw_text_ex(words, at.x, at.y, TextParams { font, font_size: TEXT as u16, color: *color, ..Default::default() }),
            }
        }
    }
}
