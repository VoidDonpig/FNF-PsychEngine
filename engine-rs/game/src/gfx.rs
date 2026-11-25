//! Drawing, the window and the keyboard.
//!
//! Drawn with wgpu, on Vulkan, DirectX 12 or OpenGL as the Graphics API option says; the window and its keys are
//! winit's. The game draws through a handful of functions (pictures, rectangles, text, in the order they are to
//! appear) that keep the names and behaviour of the library it was first written against, macroquad.
//! Everything drawn in a frame is collected and sent to the graphics card in one go when the frame ends.
pub use glam::{vec2, vec3, Affine2, Mat4, Vec2, Vec3, Vec4};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::platform::pump_events::EventLoopExtPumpEvents;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r, g, b, a }
    }

    /// From 0xRRGGBB.
    pub fn from_hex(hex: u32) -> Color {
        Color::from_rgba((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255)
    }

    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0)
    }
}

pub const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);
pub const BLACK: Color = Color::new(0.0, 0.0, 0.0, 1.0);
pub const GRAY: Color = Color::new(0.51, 0.51, 0.51, 1.0);
pub const RED: Color = Color::new(0.90, 0.16, 0.22, 1.0);
pub const ORANGE: Color = Color::new(1.0, 0.63, 0.0, 1.0);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    /// The two share some room (touching edges do not count).
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.x + other.w && other.x < self.x + self.w && self.y < other.y + other.h && other.y < self.y + self.h
    }
}

/// Random numbers; the same sequence after the same `srand`.
pub mod rand {
    use std::sync::atomic::{AtomicU64, Ordering};

    static STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);

    pub fn srand(seed: u64) {
        STATE.store(seed | 1, Ordering::Relaxed);
    }

    /// The next number of the sequence (xorshift).
    fn next() -> u64 {
        let mut x = STATE.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        STATE.store(x, Ordering::Relaxed);
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub trait Ranged: Copy {
        fn between(low: Self, high: Self, roll: f64) -> Self;
    }

    macro_rules! ranged {
        ($($number:ty: $whole:expr),*) => {$(impl Ranged for $number {
            fn between(low: Self, high: Self, roll: f64) -> Self {
                let rolled = low as f64 + (high as f64 - low as f64) * roll;
                // whole numbers stop one short of `high`
                (if $whole { rolled.floor() } else { rolled }) as $number
            }
        })*};
    }
    ranged!(i32: true, i64: true, u8: true, u32: true, u64: true, usize: true, f32: false, f64: false);

    /// A number from `low` up to, but not including, `high`.
    pub fn gen_range<T: Ranged>(low: T, high: T) -> T {
        T::between(low, high, (next() >> 11) as f64 / (1u64 << 53) as f64)
    }
}

/// Keys, named as the settings file names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[rustfmt::skip]
pub enum KeyCode {
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9,
    Kp0, Kp1, Kp2, Kp3, Kp4, Kp5, Kp6, Kp7, Kp8, Kp9,
    Left, Right, Up, Down, Space, Enter, Escape, Backspace, Delete, Home, End, F1, F11, F12, KpAdd, KpSubtract,
    LeftShift, RightShift, LeftControl, RightControl, LeftAlt, RightAlt,
    Comma, Period, Slash, Semicolon, Apostrophe, LeftBracket, RightBracket, Minus, Equal, Backslash,
}

#[rustfmt::skip]
fn key(code: winit::keyboard::KeyCode) -> Option<KeyCode> {
    use winit::keyboard::KeyCode as Their;
    use KeyCode::*;
    Some(match code {
        Their::KeyA => A, Their::KeyB => B, Their::KeyC => C, Their::KeyD => D, Their::KeyE => E, Their::KeyF => F, Their::KeyG => G, Their::KeyH => H, Their::KeyI => I,
        Their::KeyJ => J, Their::KeyK => K, Their::KeyL => L, Their::KeyM => M, Their::KeyN => N, Their::KeyO => O, Their::KeyP => P, Their::KeyQ => Q, Their::KeyR => R,
        Their::KeyS => S, Their::KeyT => T, Their::KeyU => U, Their::KeyV => V, Their::KeyW => W, Their::KeyX => X, Their::KeyY => Y, Their::KeyZ => Z,
        Their::Digit0 => Key0, Their::Digit1 => Key1, Their::Digit2 => Key2, Their::Digit3 => Key3, Their::Digit4 => Key4,
        Their::Digit5 => Key5, Their::Digit6 => Key6, Their::Digit7 => Key7, Their::Digit8 => Key8, Their::Digit9 => Key9,
        Their::Numpad0 => Kp0, Their::Numpad1 => Kp1, Their::Numpad2 => Kp2, Their::Numpad3 => Kp3, Their::Numpad4 => Kp4,
        Their::Numpad5 => Kp5, Their::Numpad6 => Kp6, Their::Numpad7 => Kp7, Their::Numpad8 => Kp8, Their::Numpad9 => Kp9,
        Their::ArrowLeft => Left, Their::ArrowRight => Right, Their::ArrowUp => Up, Their::ArrowDown => Down,
        Their::Space => Space, Their::Enter | Their::NumpadEnter => Enter, Their::Escape => Escape, Their::Backspace => Backspace, Their::Delete => Delete, Their::Home => Home, Their::End => End, Their::F1 => F1, Their::F11 => F11, Their::F12 => F12,
        Their::ShiftLeft => LeftShift, Their::ShiftRight => RightShift, Their::ControlLeft => LeftControl, Their::ControlRight => RightControl,
        Their::AltLeft => LeftAlt, Their::AltRight => RightAlt, Their::NumpadAdd => KpAdd, Their::NumpadSubtract => KpSubtract,
        Their::Comma => Comma, Their::Period => Period, Their::Slash => Slash, Their::Semicolon => Semicolon, Their::Quote => Apostrophe,
        Their::BracketLeft => LeftBracket, Their::BracketRight => RightBracket, Their::Minus => Minus, Their::Equal => Equal, Their::Backslash => Backslash,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterMode {
    /// Smoothed when stretched.
    Linear,
    /// Hard edged.
    Nearest,
}

/// Reads a PNG file a row at a time: `row` gets the picture's width and height, the row's number and its red, green, blue,
/// alpha bytes, top row first.
fn png_rows(bytes: &[u8], mut row: impl FnMut(u32, u32, u32, &[u8])) -> Result<(), String> {
    let mut decoder = png::Decoder::new(bytes);
    // palettes and odd bit depths become plain 8-bit channels
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let (width, height, color) = (reader.info().width, reader.info().height, reader.output_color_type().0);
    if width > u16::MAX as u32 || height > u16::MAX as u32 {
        return Err("too large".to_owned());
    }
    let mut y = 0;
    let mut put = |line: &[u8], rgba: &mut Vec<u8>| {
        rgba.clear();
        match color {
            png::ColorType::Rgba => rgba.extend_from_slice(&line[..width as usize * 4]),
            png::ColorType::Rgb => rgba.extend(line.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255])),
            png::ColorType::GrayscaleAlpha => rgba.extend(line.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]])),
            png::ColorType::Grayscale => rgba.extend(line.iter().flat_map(|&p| [p, p, p, 255])),
            png::ColorType::Indexed => return Err("a palette the decoder left in place".to_owned()),
        }
        row(width, height, y, rgba);
        y += 1;
        Ok(())
    };
    let mut rgba = Vec::with_capacity(width as usize * 4);
    // (rows of an interlaced file come in seven passes: those are put together whole first)
    if reader.info().interlaced {
        let mut raw = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut raw).map_err(|error| error.to_string())?;
        return raw.chunks(info.line_size).take(height as usize).try_for_each(|line| put(line, &mut rgba));
    }
    while let Some(line) = reader.next_row().map_err(|error| error.to_string())? {
        put(line.data(), &mut rgba)?;
    }
    Ok(())
}

/// A picture in main memory: rows of red, green, blue, alpha bytes, top row first.
pub struct Image {
    pub bytes: Vec<u8>,
    pub width: u16,
    pub height: u16,
}

impl Image {
    /// From the bytes of a PNG file.
    pub fn from_file_with_format(bytes: &[u8], _format: Option<()>) -> Result<Image, String> {
        let mut image = Image { bytes: Vec::new(), width: 0, height: 0 };
        png_rows(bytes, |width, height, y, row| {
            if y == 0 {
                image.bytes.reserve_exact(width as usize * height as usize * 4);
                (image.width, image.height) = (width as u16, height as u16);
            }
            image.bytes.extend_from_slice(row);
        })?;
        Ok(image)
    }

    pub fn gen_image_color(width: u16, height: u16, color: Color) -> Image {
        let pixel = [color.r, color.g, color.b, color.a].map(|channel| (channel * 255.0).round() as u8);
        Image { bytes: pixel.repeat(width as usize * height as usize), width, height }
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> Color {
        let at = (y as usize * self.width as usize + x as usize) * 4;
        Color::from_rgba(self.bytes[at], self.bytes[at + 1], self.bytes[at + 2], self.bytes[at + 3])
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        let at = (y as usize * self.width as usize + x as usize) * 4;
        self.bytes[at..at + 4].copy_from_slice(&[color.r, color.g, color.b, color.a].map(|channel| (channel * 255.0).round() as u8));
    }

    pub fn export_png(&self, path: &str) {
        let written = std::fs::File::create(path).map_err(|error| error.to_string()).and_then(|file| {
            let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), self.width as u32, self.height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().and_then(|mut writer| writer.write_image_data(&self.bytes)).map_err(|error| error.to_string())
        });
        if let Err(error) = written {
            eprintln!("{path}: {error}");
        }
    }
}

struct Picture {
    texture: wgpu::Texture,
    width: u32,
    height: u32,
    /// The picture with each way of stretching it: smoothed, hard edged.
    groups: [wgpu::BindGroup; 2],
    filter: Cell<FilterMode>,
    id: u64,
}

/// The mouse as of the last frame: where it is in the window (0 to 1 across and down), its left button, the wheel.
#[derive(Default)]
struct Mouse {
    at: Vec2,
    down: bool,
    pressed: bool,
    released: bool,
    /// The right button is down, and went down this frame.
    right: bool,
    right_pressed: bool,
    /// Notches turned this frame, away from the player positive.
    wheel: f32,
    moved: bool,
    /// The pointer is shown over the window.
    shown: bool,
}

/// A picture on the graphics card. Copies are the same picture.
#[derive(Clone)]
pub struct Texture2D(Rc<Picture>);

impl Texture2D {
    /// From the bytes of a PNG file. A file that cannot be read becomes one transparent dot.
    /// It goes to the card some rows at a time: a large picture (a character's sheet can be 200 MB) is never whole in main memory.
    pub fn from_file_with_format(bytes: &[u8], _format: Option<()>) -> Texture2D {
        let (mut texture, mut band) = (None, Vec::new());
        let read = png_rows(bytes, |width, height, y, row| {
            let texture: &Texture2D = texture.get_or_insert_with(|| with(|context| context.picture(width, height, &[])));
            band.extend_from_slice(row);
            if band.len() >= 8 << 20 || y + 1 == height {
                let rows = (band.len() / row.len()) as u32;
                // (a picture too large for the card became a dot: nothing goes in it)
                if (texture.0.width, texture.0.height) == (width, height) {
                    // (each band sent off at once: the card's copy of it is let go of before the next)
                    with(|context| (context.write(&texture.0.texture, 0, y + 1 - rows, width, rows, &band), context.queue.submit([])));
                }
                band.clear();
            }
        });
        match (read, texture) {
            (Ok(()), Some(texture)) => texture,
            (read, _) => {
                eprintln!("Picture: {}", read.err().unwrap_or_else(|| "no rows".to_owned()));
                Texture2D::from_image(&Image::gen_image_color(1, 1, Color::new(0.0, 0.0, 0.0, 0.0)))
            }
        }
    }

    pub fn from_image(image: &Image) -> Texture2D {
        with(|context| context.picture(image.width as u32, image.height as u32, &image.bytes))
    }

    pub fn width(&self) -> f32 {
        self.0.width as f32
    }

    pub fn height(&self) -> f32 {
        self.0.height as f32
    }

    pub fn size(&self) -> Vec2 {
        vec2(self.width(), self.height())
    }

    pub fn set_filter(&self, filter: FilterMode) {
        self.0.filter.set(filter);
    }

    /// Puts new pixels (RGBA, as many as it has) into the picture, in place: a video's next frame.
    pub fn update(&self, pixels: &[u8]) {
        with(|context| context.write(&self.0.texture, 0, 0, self.0.width, self.0.height, pixels));
    }

    /// No copy of it is held anywhere else.
    pub fn alone(&self) -> bool {
        Rc::strong_count(&self.0) == 1
    }
}

/// Where a letter of one size sits in its font's sheet of letters, and how it is placed on the line.
#[derive(Clone, Copy)]
struct Glyph {
    /// Which of the font's sheets it is on.
    sheet: usize,
    source: Rect,
    left: f32,
    bottom: f32,
    advance: f32,
}

struct Letters {
    font: fontdue::Font,
    /// Letters drawn so far, by size, on sheets that fill up row after row (a new sheet is begun when one is full).
    glyphs: RefCell<HashMap<(char, u16), Glyph>>,
    sheets: RefCell<Vec<Texture2D>>,
    filter: Cell<FilterMode>,
    /// Where the next letter goes, and how tall the row it is in has become.
    next: Cell<(u32, u32, u32)>,
}

const SHEET: u32 = 1024;

/// Where a letter of that size goes on a sheet whose next place is `next` (x, y, height of the row), and the next place after
/// it; None when the sheet has no room left for it.
fn place(next: (u32, u32, u32), width: u32, height: u32) -> Option<((u32, u32), (u32, u32, u32))> {
    let (mut x, mut y, mut row) = next;
    if x + width + 1 > SHEET {
        (x, y, row) = (1, y + row + 1, 0);
    }
    if y + height + 1 > SHEET {
        return None;
    }
    Some(((x, y), (x + width + 1, y, row.max(height))))
}

#[derive(Clone)]
pub struct Font(Rc<Letters>);

pub fn load_ttf_font_from_bytes(bytes: &[u8]) -> Result<Font, String> {
    let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())?;
    let sheet = with(|context| context.picture(SHEET, SHEET, &vec![0; (SHEET * SHEET * 4) as usize]));
    Ok(Font(Rc::new(Letters { font, glyphs: RefCell::default(), sheets: RefCell::new(vec![sheet]), filter: Cell::new(FilterMode::Linear), next: Cell::new((1, 1, 0)) })))
}

impl Font {
    pub fn set_filter(&mut self, filter: FilterMode) {
        self.0.filter.set(filter);
        for sheet in self.0.sheets.borrow().iter() {
            sheet.set_filter(filter);
        }
    }

    /// The letter at that size, drawn onto the sheet the first time it is asked for.
    fn glyph(&self, letter: char, size: u16, context: &mut Context) -> Glyph {
        if let Some(known) = self.0.glyphs.borrow().get(&(letter, size)) {
            return *known;
        }
        let (metrics, coverage) = self.0.font.rasterize(letter, size as f32);
        let (width, height) = (metrics.width as u32, metrics.height as u32);
        let mut glyph = Glyph { sheet: self.0.sheets.borrow().len() - 1, source: Rect::new(0.0, 0.0, width as f32, height as f32), left: metrics.xmin as f32, bottom: metrics.ymin as f32, advance: metrics.advance_width };
        let mut spot = place(self.0.next.get(), width, height);
        if spot.is_none() && width + 2 <= SHEET && height + 2 <= SHEET {
            // the sheet is full: begin another (with the filter the others have)
            let sheet = context.picture(SHEET, SHEET, &vec![0; (SHEET * SHEET * 4) as usize]);
            sheet.set_filter(self.0.filter.get());
            self.0.sheets.borrow_mut().push(sheet);
            glyph.sheet += 1;
            spot = place((1, 1, 0), width, height);
        }
        match spot {
            Some(((x, y), next)) => {
                glyph.source = Rect::new(x as f32, y as f32, width as f32, height as f32);
                if width > 0 && height > 0 {
                    let pixels: Vec<u8> = coverage.iter().flat_map(|&cover| [255, 255, 255, cover]).collect();
                    context.write(&self.0.sheets.borrow()[glyph.sheet].0.texture, x, y, width, height, &pixels);
                    self.0.next.set(next);
                }
            }
            // a letter bigger than a whole sheet stays blank
            None => glyph.source = Rect::default(),
        }
        self.0.glyphs.borrow_mut().insert((letter, size), glyph);
        glyph
    }
}

pub struct TextParams<'a> {
    pub font: Option<&'a Font>,
    pub font_size: u16,
    /// The letters of `font_size` drawn this many times as large.
    pub font_scale: f32,
    pub color: Color,
}

impl Default for TextParams<'_> {
    fn default() -> Self {
        TextParams { font: None, font_size: 20, font_scale: 1.0, color: WHITE }
    }
}

pub struct TextDimensions {
    pub width: f32,
}

#[derive(Clone, Default)]
pub struct DrawTextureParams {
    /// Stretched to this size.
    pub dest_size: Option<Vec2>,
    /// The part of the picture to draw, in its pixels.
    pub source: Option<Rect>,
    /// Turned around its middle, in radians.
    pub rotation: f32,
    pub flip_x: bool,
    pub flip_y: bool,
}

/// What the game's coordinates mean on screen: `target` is drawn in the middle, and one unit is `zoom` of half the screen.
#[derive(Default)]
pub struct Camera2D {
    pub target: Vec2,
    pub zoom: Vec2,
    /// FlxCamera's angle (degrees, about the middle of the view) and x/y (the view moved on screen, in pixels).
    pub turn: f32,
    pub shift: Vec2,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    /// What the picture's red, green and blue each become, when `recolored` is 1.
    red: [f32; 3],
    green: [f32; 3],
    blue: [f32; 3],
    recolored: f32,
    /// Above 0: the picture is shown in squares of this many of its pixels.
    blocks: f32,
    /// What a mod's vertex shader is given (`openfl_Position`, and `openfl_Matrix`: x and y scale and shift): the corner as
    /// the sprite put it (before the camera) and the camera's way to the screen.
    world: [f32; 2],
    view: [f32; 4],
}

const SHADER: &str = "
struct Corner {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) red: vec3<f32>,
    @location(3) green: vec3<f32>,
    @location(4) blue: vec3<f32>,
    @location(5) recolored: f32,
    @location(6) blocks: f32,
}

@vertex
fn corner(@location(0) position: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>,
          @location(3) red: vec3<f32>, @location(4) green: vec3<f32>, @location(5) blue: vec3<f32>, @location(6) recolored: f32,
          @location(7) blocks: f32) -> Corner {
    return Corner(vec4<f32>(position, 0.0, 1.0), uv, color, red, green, blue, recolored, blocks);
}

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var stretch: sampler;

@fragment
fn pixel(at: Corner) -> @location(0) vec4<f32> {
    // coarse: every square shows the pixel at its corner
    let squares = vec2<f32>(textureDimensions(picture)) / max(at.blocks, 1.0);
    let seen = textureSample(picture, stretch, select(at.uv, floor(at.uv * squares) / squares, at.blocks > 0.0));
    // notes are stored as three masks, each of which takes one of the lane's colours
    let mixed = vec4<f32>(min(seen.r * at.red + seen.g * at.green + seen.b * at.blue, vec3<f32>(1.0)), seen.a);
    // the colour swap shader's hue turn (ColorSwap.hx): the hue is in `red.x`, `recolored` is 2
    if at.recolored > 1.5 {
        return vec4<f32>(hsv_to_rgb(rgb_to_hsv(seen.rgb) + vec3<f32>(at.red.x, 0.0, 0.0)), seen.a) * at.color;
    }
    return mix(seen, mixed, at.recolored) * at.color;
}

// these two are ColorSwap.hx's, line for line
fn rgb_to_hsv(c: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4<f32>(c.b, c.g, k.w, k.z), vec4<f32>(c.g, c.b, k.x, k.y), step(c.b, c.g));
    let q = mix(vec4<f32>(p.x, p.y, p.w, c.r), vec4<f32>(c.r, p.y, p.z, p.x), step(p.x, c.r));
    let d = q.x - min(q.w, q.y);
    let e = 1.0e-10;
    return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

fn hsv_to_rgb(c: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(c.xxx + k.xyz) * 6.0 - k.www);
    return c.z * mix(k.xxx, clamp(p - k.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}";

/// The rain of weekend 1, the Haxe engine's RainShader: what was drawn is seen through four layers of falling drops, each
/// bending the picture behind it a little and adding a pale blue.
const RAIN_SHADER: &str = "
struct Rain {
    // the part of the world the camera shows: left, top, right, bottom
    bounds: vec4<f32>,
    time: f32,
    scale: f32,
    intensity: f32,
    unused: f32,
}

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var stretch: sampler;
@group(0) @binding(2) var<uniform> rain: Rain;

struct Across {
    @builtin(position) position: vec4<f32>,
    // (0, 0) the top left of the window, (1, 1) its bottom right
    @location(0) screen: vec2<f32>,
}

@vertex
fn whole(@builtin(vertex_index) index: u32) -> Across {
    // one triangle over the whole window
    let at = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Across(vec4<f32>(at.x * 2.0 - 1.0, 1.0 - at.y * 2.0, 0.0, 1.0), at);
}

fn rand(a: vec2<f32>) -> f32 {
    let wrapped = a - 1000.0 * floor(a / 1000.0);
    return fract(sin(dot(wrapped, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

// how far a point is from the nearest drop of a layer (below 0: inside one)
fn rain_dist(point: vec2<f32>, scale: f32, intensity: f32) -> f32 {
    var p = point * 0.1;
    // sheared, scrolled, stretched downwards
    p.x += p.y * 0.1;
    p.y -= rain.time * 500.0 / scale;
    p.y *= 0.03;
    let ix = floor(p.x);
    p.y += (ix - 2.0 * floor(ix / 2.0)) * 0.5 + (rand(vec2<f32>(ix)) - 0.5) * 0.3;
    let iy = floor(p.y);
    let index = vec2<f32>(ix, iy);
    p -= index;
    p.x += (rand(index.yx) * 2.0 - 1.0) * 0.35;
    let a = abs(p - 0.5);
    let res = max(a.x * 0.8, a.y * 0.5) - 0.1;
    // the harder it rains, the fewer cells stay empty
    let empty = rand(index) < mix(1.0, 0.1, intensity);
    return select(res, 1.0, empty);
}

@fragment
fn pixel(at: Across) -> @location(0) vec4<f32> {
    let size = rain.bounds.zw - rain.bounds.xy;
    var wpos = at.screen * size + rain.bounds.xy;
    var add = vec3<f32>(0.0);
    var rain_sum = 0.0;
    var scales = array<f32, 4>(1.0, 1.8, 2.6, 4.8);
    for (var i = 0; i < 4; i++) {
        let scale = scales[i];
        let r = rain_dist(wpos * scale / rain.scale + 500.0 * f32(i), scale, rain.intensity);
        if r < 0.0 {
            let v = (1.0 - exp(r * 5.0)) / scale * 2.0;
            wpos.x += v * 10.0 * rain.scale;
            wpos.y -= v * 2.0 * rain.scale;
            add += vec3<f32>(0.1, 0.15, 0.2) * v;
            rain_sum += (1.0 - rain_sum) * 0.75;
        }
    }
    var color = textureSampleLevel(picture, stretch, (wpos - rain.bounds.xy) / size, 0.0).rgb;
    color += add;
    color = mix(color, vec3<f32>(0.4, 0.5, 0.8), 0.1 * rain_sum);
    return vec4<f32>(color, 1.0);
}";

/// What draws the frame so far through the rain: made when it first rains.
struct RainPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    /// The picture the frame is drawn to first, as large as the window, and how the rain reads it.
    stage: Option<(wgpu::TextureView, wgpu::BindGroup, (u32, u32))>,
}

/// Where a picture of size `whole` drawn at (x, y) lands: its four corners clockwise from the top left, and the part of the
/// picture (0 to 1) that goes to each.
fn placed(whole: Vec2, x: f32, y: f32, params: &DrawTextureParams) -> ([Vec2; 4], [Vec2; 4]) {
    let source = params.source.unwrap_or(Rect::new(0.0, 0.0, whole.x, whole.y));
    let size = params.dest_size.unwrap_or(vec2(source.w, source.h));
    let middle = vec2(x, y) + size / 2.0;
    let turn = Vec2::from_angle(params.rotation);
    let corners = [vec2(x, y), vec2(x + size.x, y), vec2(x + size.x, y + size.y), vec2(x, y + size.y)].map(|corner| middle + turn.rotate(corner - middle));
    let (mut left, mut right, mut top, mut bottom) = (source.x / whole.x, (source.x + source.w) / whole.x, source.y / whole.y, (source.y + source.h) / whole.y);
    if params.flip_x {
        (left, right) = (right, left);
    }
    if params.flip_y {
        (top, bottom) = (bottom, top);
    }
    (corners, [vec2(left, top), vec2(right, top), vec2(right, bottom), vec2(left, bottom)])
}

/// Consecutive things drawn from one picture, up to vertex `end`.
struct Call {
    group: wgpu::BindGroup,
    /// The picture, how it is stretched, whether it adds its light to what is under it, and the shader it is drawn
    /// through (0: the game's own).
    picture: (u64, FilterMode, bool, u64),
    end: u32,
    shader: Option<Shader>,
}

/// How the corners of what is drawn are handed to the graphics card (`Vertex`).
const ATTRIBUTES: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x3, 4 => Float32x3, 5 => Float32x3, 6 => Float32, 7 => Float32, 8 => Float32x2, 9 => Float32x4];

/// A mod's shader read and made ready: what draws through it, and where its uniforms lie.
struct Program {
    pipeline: wgpu::RenderPipeline,
    adding: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    slots: Vec<crate::shader::Slot>,
    size: usize,
    /// The pictures it asks for besides the sprite's own, by their uniforms' names.
    pictures: Vec<String>,
    defaults: Vec<(String, Vec<f64>)>,
}

/// A mod's shader as one sprite is drawn through it, with its own values for the uniforms. Copies are the same one;
/// `fresh` makes another with the values the file gives.
#[derive(Clone)]
pub struct Shader(Rc<ShaderUse>);

struct ShaderUse {
    id: u64,
    program: Rc<Program>,
    buffer: wgpu::Buffer,
    /// The uniforms' bytes, and whether the graphics card has yet to be given them.
    bytes: RefCell<Vec<u8>>,
    stale: Cell<bool>,
    pictures: RefCell<Vec<Texture2D>>,
    group: RefCell<Option<wgpu::BindGroup>>,
}

impl Shader {
    /// Reads a shader written for OpenFL (a mod's `shaders/<name>.frag` and `.vert`, either of which may be left to
    /// the usual one). The error says what is wrong with it.
    pub fn load(fragment: Option<&str>, vertex: Option<&str>) -> Result<Shader, String> {
        with(|context| {
            let program = context.program(fragment, vertex)?;
            Ok(context.shader_use(program))
        })
    }

    /// The same shader for another sprite, its uniforms as the file gives them.
    pub fn fresh(&self) -> Shader {
        with(|context| context.shader_use(self.0.program.clone()))
    }

    /// Sets a uniform by name (as many numbers as it holds); false when the shader has none of that name.
    pub fn set(&self, name: &str, numbers: &[f64]) -> bool {
        let Some(slot) = self.0.program.slots.iter().find(|slot| slot.name == name) else { return false };
        crate::shader::write(&mut self.0.bytes.borrow_mut(), slot, numbers);
        self.0.stale.set(true);
        true
    }

    pub fn get(&self, name: &str) -> Option<Vec<f64>> {
        let slot = self.0.program.slots.iter().find(|slot| slot.name == name)?;
        Some(crate::shader::read(&self.0.bytes.borrow(), slot))
    }

    /// Gives one of the pictures the shader asks for (a `sampler2D` uniform); false when it has none of that name.
    pub fn set_picture(&self, name: &str, picture: &Texture2D) -> bool {
        let Some(at) = self.0.program.pictures.iter().position(|known| known == name) else { return false };
        self.0.pictures.borrow_mut()[at] = picture.clone();
        *self.0.group.borrow_mut() = None;
        true
    }
}

/// The window and what winit said about it since last asked.
struct App {
    attributes: winit::window::WindowAttributes,
    window: Option<Arc<winit::window::Window>>,
    events: Vec<winit::event::WindowEvent>,
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_none() {
            self.window = event_loop.create_window(self.attributes.clone()).map_err(|error| eprintln!("Window: {error}")).ok().map(Arc::new);
        }
    }

    fn window_event(&mut self, _: &winit::event_loop::ActiveEventLoop, _: winit::window::WindowId, event: winit::event::WindowEvent) {
        self.events.push(event);
    }
}

// Fields are let go of in this order when the game closes: what draws first, the window and its event loop last.
struct Context {
    instance: wgpu::Instance,
    /// None when it was lost and could not be made again yet.
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// The format frames are drawn in: the surface's, without any conversion of colours on the way.
    format: wgpu::TextureFormat,
    /// Drawing over what is there, and adding light to it.
    pipeline: wgpu::RenderPipeline,
    adding: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    samplers: [wgpu::Sampler; 2],
    pictures: u64,
    white: Option<Texture2D>,
    letters: Option<Font>,
    /// The frame so far.
    vertices: Vec<Vertex>,
    calls: Vec<Call>,
    /// The game's own shader (mods' fragment shaders are drawn with its corners), the mod's shader drawn through now, and
    /// how many of those were made.
    module: wgpu::ShaderModule,
    shader: Option<Shader>,
    shaders: u64,
    /// The layers of this frame: stretches of calls (from, to) drawn to pictures of their own, which are then laid over
    /// the frame through a mod's shader (a camera's); where the one being drawn began; and the pictures, kept from frame
    /// to frame (the picture, how it is read, the window's size it was made for).
    layers: Vec<(usize, usize)>,
    layer_open: Option<usize>,
    /// The shader a HUD piece's layer (`shade_piece`) is going to be seen through.
    piece: Option<(usize, Shader)>,
    layer_stages: Vec<(wgpu::TextureView, wgpu::BindGroup, (u32, u32))>,
    /// The next thing drawn starts a call of its own (a layer or the rain's reach ends before it).
    cut: bool,
    /// Rain asked for this frame: the calls so far are seen through it (how many, and the shader's numbers).
    effect: Option<(usize, [f32; 8])>,
    rain: Option<RainPass>,
    buffer: Option<(wgpu::Buffer, u64)>,
    background: Color,
    camera: Mat4,
    models: Vec<Mat4>,
    /// What a mod's shader is told the picture's size is, when it is not the picture's own (a script's makeGraphic rectangle: the
    /// white dot stands for a bitmap of that size).
    shader_size: Option<[f32; 2]>,
    recolor: Option<[Vec3; 3]>,
    /// How much of everything drawn now shows (a whole camera faded, as a script fades the HUD's).
    fade: f32,
    blocks: f32,
    add: bool,
    down: HashSet<KeyCode>,
    pressed: HashSet<KeyCode>,
    released: HashSet<KeyCode>,
    last_pressed: Option<KeyCode>,
    typed: VecDeque<char>,
    /// Alt+Enter went down since the last frame (it is not an Enter).
    alt_enter: bool,
    focused: bool,
    mouse: Mouse,
    started: Instant,
    frame_began: Instant,
    frame_time: f32,
    /// When the last frame was shown, and how far behind the start of its frame a picture is on screen on average (ms).
    shown: Instant,
    display_lag: f64,
    due: Instant,
    open: bool,
    window: Arc<winit::window::Window>,
    app: App,
    event_loop: Option<winit::event_loop::EventLoop<()>>,
}

thread_local! {
    static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) };
}

fn with<R>(run: impl FnOnce(&mut Context) -> R) -> R {
    CONTEXT.with(|context| run(context.borrow_mut().as_mut().expect("the window is not open")))
}

/// The window to open.
pub struct Conf {
    pub title: &'static str,
    pub width: f32,
    pub height: f32,
    pub fullscreen: bool,
    /// The graphics API to draw with: "Vulkan", "DirectX" or "OpenGL". Anything else, or one this machine does not have: whichever there is.
    pub api: &'static str,
}

/// When the next frame may start, for `fps` frames a second at most: `last` is when this one was due.
/// A frame that ran late does not make the following ones hurry. Unlimited is due at once, so nothing waits for it.
fn next_due(last: Instant, now: Instant, fps: f64) -> Instant {
    if fps >= crate::settings::UNLIMITED_FPS {
        return now;
    }
    let frame = Duration::from_secs_f64(1.0 / fps.max(1.0));
    (last + frame).max(now.checked_sub(frame).unwrap_or(now))
}

/// Opens the window. Returns what it draws with ("Vulkan, <graphics card>"), and a note when that is not what was asked for.
pub fn open(conf: Conf) -> Result<(String, Option<String>), String> {
    let mut event_loop = winit::event_loop::EventLoop::new().map_err(|error| error.to_string())?;
    // key presses are timed by the game's own raw input thread, and Windows hands raw input to one listener only
    event_loop.listen_device_events(winit::event_loop::DeviceEvents::Never);
    let mut attributes = winit::window::Window::default_attributes().with_title(conf.title).with_inner_size(winit::dpi::LogicalSize::new(conf.width, conf.height));
    // the game's icon on the window and, on Windows, in the taskbar and Alt+Tab (the .exe's own is in its resources)
    let icon = Image::from_file_with_format(include_bytes!("../../../art/icons/icon64.png"), None).ok()
        .and_then(|image| winit::window::Icon::from_rgba(image.bytes, image.width as u32, image.height as u32).ok());
    #[cfg(windows)]
    {
        use winit::platform::windows::WindowAttributesExtWindows;
        attributes = attributes.with_taskbar_icon(icon.clone());
    }
    attributes = attributes.with_window_icon(icon);
    let mut app = App { attributes, window: None, events: Vec::new() };
    for _ in 0..100 {
        if app.window.is_some() {
            break;
        }
        let _ = event_loop.pump_app_events(Some(Duration::from_millis(10)), &mut app);
    }
    let window = app.window.clone().ok_or("the window could not be opened")?;

    let wanted = match conf.api {
        "Vulkan" => Some(wgpu::Backends::VULKAN),
        "DirectX" => Some(wgpu::Backends::DX12),
        "OpenGL" => Some(wgpu::Backends::GL),
        _ => None,
    };
    // `software`: also when it is the processor that would draw, which every Windows offers for DirectX 12 and which is far too slow
    let connect = |backends: wgpu::Backends, software: bool| -> Result<(wgpu::Instance, wgpu::Surface<'static>, wgpu::Adapter), String> {
        let mut descriptor = wgpu::InstanceDescriptor::new_with_display_handle(Box::new(event_loop.owned_display_handle()));
        descriptor.backends = backends;
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance.create_surface(window.clone()).map_err(|error| error.to_string())?;
        let options = wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: Some(&surface), ..Default::default() };
        let adapter = pollster::block_on(instance.request_adapter(&options)).map_err(|error| error.to_string())?;
        if !software && adapter.get_info().device_type == wgpu::DeviceType::Cpu {
            return Err("no graphics card draws with it, only the processor".to_owned());
        }
        Ok((instance, surface, adapter))
    };
    // the API asked for on a graphics card; else any API on a graphics card; else drawn by the processor
    let (all, mut found, mut why) = (wgpu::Backends::all(), None, None);
    for (backends, software) in [(wanted.unwrap_or(all), false), (all, false), (wanted.unwrap_or(all), true), (all, true)] {
        match connect(backends, software) {
            Ok(connected) => {
                found = Some(connected);
                break;
            }
            Err(error) => drop(why.get_or_insert(error)),
        }
    }
    let Some((instance, surface, adapter)) = found else { return Err(why.unwrap_or_default()) };
    let note = why.filter(|_| wanted.is_some()).map(|why| format!("{} could not be used as asked ({why})", conf.api));
    // pictures are put on the card in small blocks, the large ones each in a block of its own: with wgpu's usual blocks (128
    // to 256 MB, given back only once nothing at all is left in one) a menu picture sharing a block with a song's kept
    // the whole block after the song, and a few hundred megabytes more stayed with each song played
    let memory_hints = wgpu::MemoryHints::Manual { suballocated_device_memory_block_size: 4 << 20..8 << 20 };
    let device = wgpu::DeviceDescriptor { required_limits: adapter.limits(), memory_hints, ..Default::default() };
    let (device, queue) = pollster::block_on(adapter.request_device(&device)).map_err(|error| error.to_string())?;

    let size = window.inner_size();
    let able = surface.get_capabilities(&adapter);
    // colours go to the screen as the pictures have them: a surface that would convert them is written to as if it did not
    // (eight bits a colour where there is such a format, which is also what a screenshot is read back as)
    let plain = [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm];
    let formats = || able.formats.iter();
    let surface_format = *formats().find(|format| plain.contains(format)).or(formats().find(|format| plain.contains(&format.remove_srgb_suffix()))).or(formats().next()).ok_or("the window cannot be drawn to")?;
    let format = surface_format.remove_srgb_suffix();
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: size.width.max(1),
        height: size.height.max(1),
        // frames are shown as soon as they are done: the Framerate option is what limits them
        present_mode: wgpu::PresentMode::AutoNoVsync,
        alpha_mode: able.alpha_modes[0],
        view_formats: if format == surface_format { vec![] } else { vec![format] },
        ..surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).ok_or("the window cannot be drawn to")?
    };
    surface.configure(&device, &config);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], ..Default::default() });
    let pipeline_with = |blend: wgpu::BlendState| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("corner"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRIBUTES })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("pixel"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let pipeline = pipeline_with(wgpu::BlendState::ALPHA_BLENDING);
    // light: what is drawn is added to what is there (Flixel's ADD), as much as it is opaque
    let light = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
    // (it leaves the opacity of what is there as it was: light added to a see-through layer stays light, not a cover)
    let kept = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
    let adding = pipeline_with(wgpu::BlendState { color: light, alpha: kept });
    let sampler = |filter: wgpu::FilterMode| device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: filter, min_filter: filter, ..Default::default() });
    let samplers = [sampler(wgpu::FilterMode::Linear), sampler(wgpu::FilterMode::Nearest)];

    let info = adapter.get_info();
    let now = Instant::now();
    let mut context = Context {
        event_loop: Some(event_loop), app, window, instance, surface: Some(surface), device, queue, config, format, pipeline, adding, layout, samplers, pictures: 0, white: None, letters: None,
        vertices: Vec::new(), calls: Vec::new(), module: shader, shader: None, shaders: 0, layers: Vec::new(), layer_open: None, piece: None, layer_stages: Vec::new(), cut: false, effect: None, rain: None, buffer: None, background: BLACK, camera: Mat4::IDENTITY, models: Vec::new(), shader_size: None, recolor: None, fade: 1.0, blocks: 0.0, add: false,
        down: HashSet::new(), pressed: HashSet::new(), released: HashSet::new(), last_pressed: None, typed: VecDeque::new(), alt_enter: false, focused: true, mouse: Mouse::default(),
        started: now, frame_began: now, frame_time: 1.0 / 60.0, shown: now, display_lag: 0.0, due: now, open: true,
    };
    context.white = Some(context.picture(1, 1, &[255; 4]));
    context.fullscreen(conf.fullscreen, false);
    // the pointer shows only when a menu wants it, as in the Haxe engine
    context.window.set_cursor_visible(false);
    CONTEXT.with(|slot| *slot.borrow_mut() = Some(context));
    // the letters the game falls back on when its own font is not found
    let letters = load_ttf_font_from_bytes(include_bytes!("../../../assets/fonts/vcr.ttf"))?;
    with(|context| context.letters = Some(letters));
    let name = match info.backend {
        wgpu::Backend::Vulkan => "Vulkan",
        wgpu::Backend::Dx12 => "DirectX 12",
        wgpu::Backend::Gl => "OpenGL",
        wgpu::Backend::Metal => "Metal",
        _ => "another graphics API",
    };
    Ok((format!("{name}, {}", info.name), note))
}

impl Context {
    fn picture(&mut self, width: u32, height: u32, pixels: &[u8]) -> Texture2D {
        let most = self.device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > most || height > most {
            eprintln!("A picture of {width} by {height} cannot be drawn by this graphics card");
            return self.picture(1, 1, &[0; 4]);
        }
        let texture = Self::blank(&self.device, width, height, wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        // (none: they are put in afterwards)
        if !pixels.is_empty() {
            self.write(&texture, 0, 0, width, height, pixels);
        }
        // sent off now: the copy of the pixels made for the card is let go of at once, not when the frame is drawn (a song
        // loading many large pictures would otherwise hold a second copy of all of them until then)
        self.queue.submit([]);
        let view = texture.create_view(&Default::default());
        let group = |sampler: &wgpu::Sampler| {
            let entries = [wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) }];
            self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.layout, entries: &entries })
        };
        let groups = [group(&self.samplers[0]), group(&self.samplers[1])];
        self.pictures += 1;
        Texture2D(Rc::new(Picture { texture, width, height, groups, filter: Cell::new(FilterMode::Linear), id: self.pictures }))
    }

    /// A blank picture on the card, of one layer and one size.
    fn blank(device: &wgpu::Device, width: u32, height: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> wgpu::Texture {
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let dimension = wgpu::TextureDimension::D2;
        device.create_texture(&wgpu::TextureDescriptor { label: None, size, mip_level_count: 1, sample_count: 1, dimension, format, usage, view_formats: &[] })
    }

    /// Puts pixels into a part of a picture.
    fn write(&self, texture: &wgpu::Texture, x: u32, y: u32, width: u32, height: u32, pixels: &[u8]) {
        let place = wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All };
        let rows = wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) };
        self.queue.write_texture(place, pixels, rows, wgpu::Extent3d { width, height, depth_or_array_layers: 1 });
    }

    /// A four-cornered piece of a picture: corners clockwise from the top left, with the part of the picture (0 to 1) at each.
    fn quad(&mut self, picture: &Texture2D, corners: [Vec2; 4], uvs: [Vec2; 4], color: Color) {
        let filter = picture.0.filter.get();
        let id = (picture.0.id, filter, self.add, self.shader.as_ref().map_or(0, |shader| shader.0.id));
        // (what is drawn after the rain was asked for, or a layer began or ended, is not part of what came before)
        if std::mem::take(&mut self.cut) || self.calls.last().is_none_or(|call| call.picture != id) {
            self.calls.push(Call { group: picture.0.groups[filter as usize].clone(), picture: id, end: self.vertices.len() as u32, shader: self.shader.clone() });
        }
        // a mod's shader is told how large the picture is (openfl_TextureSize, the first of its uniforms)
        if let Some(shader) = &self.shader {
            let size = self.shader_size.map_or([picture.0.width as f64, picture.0.height as f64], |size| size.map(f64::from));
            if shader.get("openfl_TextureSize").as_deref() != Some(&size) {
                shader.set("openfl_TextureSize", &size);
            }
        }
        let model = self.models.last().copied().unwrap_or(Mat4::IDENTITY);
        let view = [self.camera.x_axis.x, self.camera.w_axis.x, self.camera.y_axis.y, self.camera.w_axis.y];
        let [red, green, blue] = self.recolor.unwrap_or_default().map(|part| part.to_array());
        let corner = |index: usize| {
            let world = model.transform_point3(corners[index].extend(0.0));
            let position = self.camera.transform_point3(world);
            // a hue turn is marked by NaN in `green` (see `recolor_hue`); `red.x` is the hue
            let recolored = if self.recolor.is_some_and(|colors| colors[1].x.is_nan()) { 2.0 } else { self.recolor.is_some() as u8 as f32 };
            Vertex { position: [position.x, position.y], uv: uvs[index].to_array(), color: [color.r, color.g, color.b, color.a * self.fade], red, green, blue, recolored, blocks: self.blocks, world: [world.x, world.y], view }
        };
        self.vertices.extend([0, 1, 2, 0, 2, 3].map(corner));
        if let Some(call) = self.calls.last_mut() {
            call.end = self.vertices.len() as u32;
        }
    }

    fn texture(&mut self, picture: &Texture2D, x: f32, y: f32, color: Color, params: &DrawTextureParams) {
        let (corners, uvs) = placed(picture.size(), x, y, params);
        self.quad(picture, corners, uvs, color);
    }

    /// Draws the text with its baseline at (x, y), or only measures it. Returns how wide it is.
    fn text(&mut self, text: &str, x: f32, y: f32, params: &TextParams, draw: bool) -> f32 {
        let Some(font) = params.font.or(self.letters.as_ref()).cloned() else { return 0.0 };
        let scale = params.font_scale;
        let mut pen = 0.0;
        for letter in text.chars() {
            let glyph = font.glyph(letter, params.font_size, self);
            let (width, height) = (glyph.source.w * scale, glyph.source.h * scale);
            let top = y - height - glyph.bottom * scale;
            if draw && width > 0.0 {
                let from = DrawTextureParams { dest_size: Some(vec2(width, height)), source: Some(glyph.source), ..Default::default() };
                self.texture(&font.0.sheets.borrow()[glyph.sheet], x + pen + glyph.left * scale, top, params.color, &from);
            }
            pen += glyph.advance * scale;
        }
        pen
    }

    /// Sends the frame so far to the graphics card, to be drawn into `target`.
    fn render(&mut self, target: &wgpu::TextureView) {
        let rained = self.effect.filter(|(calls, _)| *calls > 0 && *calls <= self.calls.len());
        if rained.is_some() {
            self.ready_rain();
        }
        // the mods' shaders used this frame are given their uniforms and pictures
        for shader in self.calls.iter().filter_map(|call| call.shader.as_ref()) {
            // (gl_FragCoord counts from the bottom of the target, which is as high as the window)
            let size = [self.config.width as f64, self.config.height as f64];
            if shader.get("funkin_target").is_some_and(|target| target != size) {
                shader.set("funkin_target", &size);
            }
            if shader.0.stale.replace(false) {
                self.queue.write_buffer(&shader.0.buffer, 0, &shader.0.bytes.borrow());
            }
            if shader.0.group.borrow().is_none() {
                let pictures = shader.0.pictures.borrow();
                let views: Vec<wgpu::TextureView> = pictures.iter().map(|picture| picture.0.texture.create_view(&Default::default())).collect();
                let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: shader.0.buffer.as_entire_binding() }];
                for (at, view) in views.iter().enumerate() {
                    entries.push(wgpu::BindGroupEntry { binding: 1 + 2 * at as u32, resource: wgpu::BindingResource::TextureView(view) });
                    entries.push(wgpu::BindGroupEntry { binding: 2 + 2 * at as u32, resource: wgpu::BindingResource::Sampler(&self.samplers[0]) });
                }
                *shader.0.group.borrow_mut() = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &shader.0.program.layout, entries: &entries }));
            }
        }
        let bytes: &[u8] = bytemuck::cast_slice(&self.vertices);
        if self.buffer.as_ref().is_none_or(|(_, room)| *room < bytes.len() as u64) {
            let room = (bytes.len() as u64).next_power_of_two().max(1 << 16);
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: room, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            self.buffer = Some((buffer, room));
        }
        let Some((buffer, _)) = &self.buffer else { return };
        self.queue.write_buffer(buffer, 0, bytes);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let background = wgpu::Color { r: self.background.r as f64, g: self.background.g as f64, b: self.background.b as f64, a: 1.0 };
        fn onto(view: &wgpu::TextureView, clear: wgpu::Color) -> wgpu::RenderPassColorAttachment<'_> {
            wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store } }
        }
        // draws the calls numbered `range`; those of a layer are left out (they go to its own picture) unless it is `inside`
        let draw = |pass: &mut wgpu::RenderPass, range: std::ops::Range<usize>, inside: Option<usize>| {
            pass.set_vertex_buffer(0, buffer.slice(..));
            let mut adding = None;
            for index in range {
                // (a layer within a layer: what is drawn belongs to the innermost, the smallest range holding it)
                let owner = self.layers.iter().enumerate().filter(|(_, (first, end))| (*first..*end).contains(&index)).min_by_key(|(_, (first, end))| end - first).map(|(layer, _)| layer);
                if owner != inside {
                    continue;
                }
                let call = &self.calls[index];
                if let Some(shader) = &call.shader {
                    pass.set_pipeline(if call.picture.2 { &shader.0.program.adding } else { &shader.0.program.pipeline });
                    pass.set_bind_group(1, shader.0.group.borrow().as_ref(), &[]);
                    adding = None;
                } else if adding != Some(call.picture.2) {
                    adding = Some(call.picture.2);
                    pass.set_pipeline(if call.picture.2 { &self.adding } else { &self.pipeline });
                }
                pass.set_bind_group(0, &call.group, &[]);
                pass.draw(if index == 0 { 0 } else { self.calls[index - 1].end }..call.end, 0..1);
            }
        };
        let rain = self.rain.as_ref().and_then(|rain| Some((rain, rain.stage.as_ref()?)));
        // A stretch of the frame drawn to `view` over `clear`. With rain asked for within it, what the rain falls on goes
        // to a picture first; `view` gets that picture seen through the rain, then the rest of the stretch.
        let stretch = |encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, clear: wgpu::Color, range: std::ops::Range<usize>, inside: Option<usize>| {
            // (rain asked for inside a layer is that layer's alone)
            let here = |mark: usize| range.start < mark && mark <= range.end && (inside.is_some() || !self.layers.iter().any(|(first, end)| *first < mark && mark <= *end));
            match rained.filter(|(mark, _)| here(*mark)).zip(rain) {
                Some(((mark, numbers), (rain, (stage, group, _)))) => {
                    self.queue.write_buffer(&rain.uniforms, 0, bytemuck::cast_slice(&numbers));
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { color_attachments: &[Some(onto(stage, background))], ..Default::default() });
                        draw(&mut pass, range.start..mark, inside);
                    }
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { color_attachments: &[Some(onto(view, clear))], ..Default::default() });
                    pass.set_pipeline(&rain.pipeline);
                    pass.set_bind_group(0, group, &[]);
                    pass.draw(0..3, 0..1);
                    draw(&mut pass, mark..range.end, inside);
                }
                None => {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { color_attachments: &[Some(onto(view, clear))], ..Default::default() });
                    draw(&mut pass, range, inside);
                }
            }
        };
        // each layer to its own picture (see-through where nothing is drawn), then the frame, the layers' pictures in it
        for (layer, (first, end)) in self.layers.iter().enumerate() {
            if let Some((view, _, _)) = self.layer_stages.get(layer) {
                stretch(&mut encoder, view, wgpu::Color::TRANSPARENT, *first..*end, Some(layer));
            }
        }
        stretch(&mut encoder, target, background, 0..self.calls.len(), None);
        self.queue.submit([encoder.finish()]);
    }

    /// Reads a mod's fragment shader and makes what draws through it. Whatever the graphics card's side finds wrong with
    /// it is the error (the game goes on without the shader).
    fn program(&mut self, fragment: Option<&str>, vertex: Option<&str>) -> Result<Rc<Program>, String> {
        let crate::shader::Translated { module, vertex, slots, size, pictures, defaults } = crate::shader::translate_pair(fragment, vertex)?;
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let fragment = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)) });
        // (the corners go through the shader's own vertex part when it has one, else through the game's)
        let corners = vertex.map(|module| self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)) }));
        let mut entries = vec![wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }];
        for at in 0..pictures.len() as u32 {
            let picture = wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false };
            entries.push(wgpu::BindGroupLayoutEntry { binding: 1 + 2 * at, visibility: wgpu::ShaderStages::FRAGMENT, ty: picture, count: None });
            entries.push(wgpu::BindGroupLayoutEntry { binding: 2 + 2 * at, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None });
        }
        let layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &entries });
        let pipeline_layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&self.layout), Some(&layout)], ..Default::default() });
        let make = |blend: wgpu::BlendState| self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: corners.as_ref().unwrap_or(&self.module),
                entry_point: Some(if corners.is_some() { "main" } else { "corner" }),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRIBUTES })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &fragment,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                // (what OpenFL's shaders answer has its colours multiplied by its opacity already)
                targets: &[Some(wgpu::ColorTargetState { format: self.format, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let pipeline = make(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        // a sprite that adds its light (setBlendMode 'add'): the colours are multiplied by the opacity already, so they
        // are added as they are; the opacity of what is there stays (as with the game's own `adding`)
        let light = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
        let kept = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
        let adding = make(wgpu::BlendState { color: light, alpha: kept });
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(error.to_string());
        }
        Ok(Rc::new(Program { pipeline, adding, layout, slots, size, pictures, defaults }))
    }

    /// A shader for one sprite: its uniforms as the file gives them, a white dot for each picture it asks for.
    fn shader_use(&mut self, program: Rc<Program>) -> Shader {
        self.shaders += 1;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: program.size.max(16) as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let pictures = self.white.iter().cycle().take(program.pictures.len()).cloned().collect();
        let shader = Shader(Rc::new(ShaderUse { id: self.shaders, bytes: RefCell::new(vec![0; program.size.max(16)]), buffer, stale: Cell::new(true), pictures: RefCell::new(pictures), group: RefCell::new(None), program }));
        for (name, numbers) in &shader.0.program.defaults {
            shader.set(name, numbers);
        }
        shader
    }

    /// Ends the layer begun at call `first`: what was drawn since goes to a picture of its own, and that picture is laid
    /// over the frame, the whole window, through `shader`.
    fn end_layer(&mut self, first: usize, shader: &Shader, fit: bool) {
        if first >= self.calls.len() {
            return;
        }
        let layer = self.layers.len();
        let window = (self.config.width.max(1), self.config.height.max(1));
        // a piece's layer is a picture of the piece alone: as large as everything drawn in it (in clip space `left`, `top`,
        // `right`, `bottom`), so that to the shader the picture's edges are the piece's; a camera's layer is the window
        let (mut left, mut top, mut right, mut bottom) = (-1.0f32, 1.0f32, 1.0f32, -1.0f32);
        let mut size = window;
        if fit {
            let from = if first == 0 { 0 } else { self.calls[first - 1].end as usize };
            let drawn = &mut self.vertices[from..];
            let (mut low, mut high) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
            for vertex in drawn.iter() {
                (low, high) = (low.min(vec2(vertex.position[0], vertex.position[1])), high.max(vec2(vertex.position[0], vertex.position[1])));
            }
            (left, right, bottom, top) = (low.x, high.x, low.y, high.y);
            size = ((((right - left) / 2.0 * window.0 as f32).ceil() as u32).clamp(1, 4096), (((top - bottom) / 2.0 * window.1 as f32).ceil() as u32).clamp(1, 4096));
            // (what was drawn is moved to fill that picture)
            for vertex in drawn {
                vertex.position = [(vertex.position[0] - left) / (right - left) * 2.0 - 1.0, (vertex.position[1] - bottom) / (top - bottom) * 2.0 - 1.0];
                let (kx, ky) = (2.0 / (right - left), 2.0 / (top - bottom));
                vertex.view = [vertex.view[0] * kx, (vertex.view[1] - left) * kx - 1.0, vertex.view[2] * ky, (vertex.view[3] - bottom) * ky - 1.0];
            }
        }
        if self.layer_stages.get(layer).is_none_or(|(_, _, made)| *made != size) {
            let texture = Self::blank(&self.device, size.0, size.1, self.format, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING);
            let view = texture.create_view(&Default::default());
            let entries = [wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.samplers[0]) }];
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.layout, entries: &entries });
            self.layer_stages.truncate(layer);
            self.layer_stages.push((view, group, size));
        }
        self.layers.push((first, self.calls.len()));
        // the picture over the whole window (or the piece's place); its colours are multiplied by its opacity already (told by `recolored`)
        shader.set("openfl_TextureSize", &if fit { [size.0 as f64, size.1 as f64] } else { [crate::WIDTH as f64, crate::HEIGHT as f64] });
        // (to a vertex shader the picture is `pixels` large, from its top left: the window's, or the piece's)
        let pixels = if fit { [size.0 as f32, size.1 as f32] } else { [crate::WIDTH, crate::HEIGHT] };
        let view = [(right - left) / pixels[0], left, -(top - bottom) / pixels[1], top];
        let corner = |x: f32, y: f32| Vertex { position: [left + x * (right - left), top - y * (top - bottom)], uv: [x, y], color: [1.0; 4], red: [0.0; 3], green: [0.0; 3], blue: [0.0; 3], recolored: 1.0, blocks: 0.0, world: [x * pixels[0], y * pixels[1]], view };
        self.vertices.extend([(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|(x, y)| corner(x, y)));
        self.calls.push(Call { group: self.layer_stages[layer].1.clone(), picture: (u64::MAX - layer as u64, FilterMode::Linear, false, shader.0.id), end: self.vertices.len() as u32, shader: Some(shader.clone()) });
        self.cut = true;
    }

    /// Makes what the rain is drawn with, and the picture it rains on (again when the window's size changed).
    fn ready_rain(&mut self) {
        let size = (self.config.width.max(1), self.config.height.max(1));
        if self.rain.is_none() {
            let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(RAIN_SHADER.into()) });
            let layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                    wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                ],
            });
            let pipeline_layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], ..Default::default() });
            let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("whole"), compilation_options: Default::default(), buffers: &[] },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("pixel"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: self.format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            });
            let uniforms = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 32, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            self.rain = Some(RainPass { pipeline, layout, uniforms, stage: None });
        }
        let Some(rain) = &mut self.rain else { return };
        if rain.stage.as_ref().is_none_or(|(_, _, made)| *made != size) {
            let texture = Self::blank(&self.device, size.0, size.1, self.format, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING);
            let view = texture.create_view(&Default::default());
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &rain.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.samplers[0]) },
                    wgpu::BindGroupEntry { binding: 2, resource: rain.uniforms.as_entire_binding() },
                ],
            });
            rain.stage = Some((view, group, size));
        }
    }

    /// The frame so far as a picture in main memory.
    fn capture(&mut self) -> Image {
        let (width, height) = (self.config.width, self.config.height);
        let texture = Self::blank(&self.device, width, height, self.format, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
        self.render(&texture.create_view(&Default::default()));
        // rows in a buffer start at multiples of 256 bytes
        let row = (width * 4).next_multiple_of(256);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: row as u64 * height as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let from = wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All };
        let into = wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(height) } };
        encoder.copy_texture_to_buffer(from, into, wgpu::Extent3d { width, height, depth_or_array_layers: 1 });
        self.queue.submit([encoder.finish()]);
        buffer.map_async(wgpu::MapMode::Read, .., |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let mut bytes = Vec::with_capacity((width * height * 4) as usize);
        if let Ok(rows) = buffer.get_mapped_range(..) {
            let blue_first = matches!(self.format, wgpu::TextureFormat::Bgra8Unorm);
            for line in rows.chunks(row as usize) {
                bytes.extend(line[..(width * 4) as usize].chunks(4).flat_map(|p| if blue_first { [p[2], p[1], p[0], 255] } else { [p[0], p[1], p[2], 255] }));
            }
        }
        Image { bytes, width: width as u16, height: height as u16 }
    }

    fn fullscreen(&self, on: bool, exclusive: bool) {
        use winit::window::Fullscreen;
        // exclusive: the display in the mode it is in now, at the fastest refresh it offers, for the game alone
        let mode = || {
            let monitor = self.window.current_monitor()?;
            monitor.video_modes().filter(|mode| mode.size() == monitor.size()).max_by_key(|mode| mode.refresh_rate_millihertz())
        };
        self.window.set_fullscreen(on.then(|| mode().filter(|_| exclusive).map_or(Fullscreen::Borderless(None), Fullscreen::Exclusive)));
    }

    /// Takes in what happened to the window since the last frame.
    fn listen(&mut self) {
        self.pressed.clear();
        self.released.clear();
        self.typed.clear(); // letters nobody read this frame were not meant for the next screen
        self.alt_enter = false;
        (self.mouse.pressed, self.mouse.released, self.mouse.wheel, self.mouse.moved, self.mouse.right_pressed) = (false, false, 0.0, false, false);
        if let Some(mut event_loop) = self.event_loop.take() {
            if let winit::platform::pump_events::PumpStatus::Exit(_) = event_loop.pump_app_events(Some(Duration::ZERO), &mut self.app) {
                self.open = false;
            }
            self.event_loop = Some(event_loop);
        }
        for event in std::mem::take(&mut self.app.events) {
            use winit::event::{ElementState, WindowEvent};
            match event {
                WindowEvent::CloseRequested => self.open = false,
                WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                    (self.config.width, self.config.height) = (size.width, size.height);
                    if let Some(surface) = &self.surface {
                        surface.configure(&self.device, &self.config);
                    }
                }
                // keys let go of while another window was in front are not heard of
                WindowEvent::Focused(focused) => {
                    self.focused = focused;
                    self.down.clear();
                    self.mouse.down = false;
                    self.mouse.right = false;
                }
                WindowEvent::CursorMoved { position, .. } => {
                    self.mouse.at = vec2(position.x as f32 / self.config.width as f32, position.y as f32 / self.config.height as f32);
                    self.mouse.moved = true;
                }
                WindowEvent::MouseInput { state, button: winit::event::MouseButton::Right, .. } => {
                    self.mouse.right_pressed |= state == ElementState::Pressed && !self.mouse.right;
                    self.mouse.right = state == ElementState::Pressed;
                }
                WindowEvent::MouseInput { state, button: winit::event::MouseButton::Left, .. } => {
                    self.mouse.down = state == ElementState::Pressed;
                    self.mouse.pressed |= self.mouse.down;
                    self.mouse.released |= !self.mouse.down;
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    self.mouse.wheel += match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, lines) => lines,
                        // a touchpad gives pixels: about a notch for every 40 of them
                        winit::event::MouseScrollDelta::PixelDelta(pixels) => pixels.y as f32 / 40.0,
                    };
                }
                WindowEvent::KeyboardInput { event, is_synthetic, .. } => {
                    let code = match event.physical_key {
                        winit::keyboard::PhysicalKey::Code(code) => key(code),
                        _ => None,
                    };
                    // winit also reports the keys that are down when the window comes to the front: held, but not pressed now
                    if event.state == ElementState::Pressed && !is_synthetic {
                        self.typed.extend(event.text.iter().flat_map(|text| text.chars()));
                    }
                    match (code, event.state) {
                        // Alt+Enter switches fullscreen: the menus must not take it for an Enter
                        (Some(KeyCode::Enter), ElementState::Pressed) if !event.repeat && [KeyCode::LeftAlt, KeyCode::RightAlt].iter().any(|alt| self.down.contains(alt)) => self.alt_enter = true,
                        (Some(code), ElementState::Pressed) if !event.repeat => {
                            self.down.insert(code);
                            if !is_synthetic {
                                self.pressed.insert(code);
                                self.last_pressed = Some(code);
                            }
                        }
                        (Some(code), ElementState::Released) => {
                            self.down.remove(&code);
                            self.released.insert(code);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

/// Shows the frame, waits out what is left of its time (`fps` frames a second at most), and takes in what happened
/// to the window meanwhile. False when the window was closed.
pub fn next_frame(fps: f64) -> bool {
    with(|context| {
        use wgpu::CurrentSurfaceTexture::*;
        match context.surface.as_ref().map_or(Lost, wgpu::Surface::get_current_texture) {
            Success(frame) | Suboptimal(frame) => {
                let view = frame.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(context.format), ..Default::default() });
                context.render(&view);
                context.queue.present(frame);
                // from the start of the frame until it is shown, and half the time it then stays up; smoothed, and a long one
                // is a loading stall, not a frame
                let now = Instant::now();
                let lag = (now - context.frame_began).as_secs_f64() * 1000.0 + (now - context.shown).as_secs_f64() * 500.0;
                if lag < 500.0 {
                    context.display_lag += (lag - context.display_lag) * 0.2;
                }
                context.shown = now;
            }
            // the window changed under the surface: fitted again, and this frame is skipped
            Outdated => context.surface.iter().for_each(|surface| surface.configure(&context.device, &context.config)),
            // the surface is gone (the graphics driver started over, say): a new one, once the old one is out of the window's way
            Lost => {
                context.surface = None;
                match context.instance.create_surface(context.window.clone()) {
                    Ok(surface) => {
                        surface.configure(&context.device, &context.config);
                        context.surface = Some(surface);
                    }
                    Err(error) => eprintln!("The window cannot be drawn to: {error}"),
                }
            }
            _ => {}
        }
        context.vertices.clear();
        context.calls.clear();
        context.layers.clear();
        (context.effect, context.layer_open, context.piece, context.cut) = (None, None, None, false);
        context.models.clear();
        (context.recolor, context.blocks, context.add, context.last_pressed, context.fade, context.shader) = (None, 0.0, false, None, 1.0, None);

        // sleeping is coarse: it stops a little short, and the rest is waited out awake
        if let Some(left) = context.due.checked_duration_since(Instant::now()).and_then(|left| left.checked_sub(Duration::from_millis(1))) {
            std::thread::sleep(left);
        }
        while Instant::now() < context.due {
            std::hint::spin_loop();
        }
        let now = Instant::now();
        context.due = next_due(context.due, now, fps);
        (context.frame_time, context.frame_began) = ((now - context.frame_began).as_secs_f32(), now);
        context.listen();
        context.open
    })
}

/// Closes the window and lets go of the graphics card. Called before the program ends: left to the system's own
/// clean-up of a thread, which Windows runs after it has stopped every other thread, the graphics driver can wait for ever.
pub fn close() {
    CONTEXT.with(|slot| drop(slot.borrow_mut().take()));
}

/// Forgets what was drawn this frame and starts over on that colour.
pub fn clear_background(color: Color) {
    with(|context| {
        context.vertices.clear();
        context.calls.clear();
        context.layers.clear();
        (context.effect, context.layer_open, context.piece, context.cut) = (None, None, None, false);
        context.background = color;
    });
}

/// What is drawn from now on is drawn through a mod's shader (None: the game's own way again).
pub fn shade(shader: Option<&Shader>) {
    with(|context| context.shader = shader.cloned());
}

/// Everything drawn this frame so far is seen through the rain of weekend 1: `bounds` is the part of the world the
/// camera shows (left, top, right, bottom), `time` the seconds it has rained, `scale` the size of the drops, `intensity`
/// how hard it rains (0 to 1). What is drawn afterwards lies over it, dry.
pub fn rain(bounds: [f32; 4], time: f32, scale: f32, intensity: f32) {
    with(|context| {
        context.effect = Some((context.calls.len(), [bounds[0], bounds[1], bounds[2], bounds[3], time, scale, intensity, 0.0]));
        context.cut = true;
    });
}

/// What is drawn from now until `layer_end` is a layer: a camera's whole picture, which a mod's shader is then put on.
/// (One at a time: a layer within a layer is part of the outer one.)
pub fn layer_begin() {
    with(|context| {
        if context.layer_open.is_none() {
            (context.layer_open, context.cut) = (Some(context.calls.len()), true);
        }
    });
}

/// Ends the layer: all of it is seen through `shader` (the sprite's picture, to the shader, is the layer's).
pub fn layer_end(shader: &Shader) {
    with(|context| {
        if let Some(first) = context.layer_open.take() {
            context.end_layer(first, shader, false);
        }
    });
}

/// What is drawn from now on, up to the next call, is a HUD piece (the health bar, the score text...) seen through
/// `shader`: drawn into a picture of its own first, as Flixel's bars and texts are pictures. None ends the piece.
/// (Inside a camera's layer, the piece's picture goes into the camera's.)
pub fn shade_piece(shader: Option<&Shader>) {
    with(|context| {
        if let Some((first, shader)) = context.piece.take() {
            context.end_layer(first, &shader, true);
        }
        if let Some(shader) = shader {
            (context.piece, context.cut) = (Some((context.calls.len(), shader.clone())), true);
        }
    });
}

pub fn set_camera(camera: &Camera2D) {
    // up on screen is up for the graphics card, and down for the game
    with(|context| {
        let to_screen = Mat4::from_scale(vec3(camera.zoom.x, -camera.zoom.y, 1.0)) * Mat4::from_translation(vec3(-camera.target.x, -camera.target.y, 0.0));
        context.camera = if camera.turn == 0.0 && camera.shift == Vec2::ZERO { to_screen } else {
            // turned and moved in pixels of the screen (y down), on top of the usual: then to the screen's -1..1 as before
            let (width, height) = (crate::WIDTH, crate::HEIGHT);
            let pixels = Mat4::from_scale(vec3(2.0 / width, -2.0 / height, 1.0));
            pixels * Mat4::from_translation(camera.shift.extend(0.0)) * Mat4::from_rotation_z(camera.turn.to_radians()) * pixels.inverse() * to_screen
        };
    });
}

/// Until `pop_model_matrix`, everything drawn is moved by `model` first.
pub fn push_model_matrix(model: Mat4) {
    with(|context| {
        let under = context.models.last().copied().unwrap_or(Mat4::IDENTITY);
        context.models.push(under * model);
    });
}

pub fn pop_model_matrix() {
    with(|context| {
        context.models.pop();
    });
}

/// From now on pictures are drawn as masks: their red, green and blue each become one of these colours. None: as they are.
/// Pictures drawn from now on are shown in squares of `blocks` of their own pixels (0: as they are).
pub fn coarse(blocks: f32) {
    with(|context| context.blocks = blocks);
}

/// Whether what is drawn from now on adds its light to what is under it (Flixel's ADD blend) instead of covering it.
pub fn blend_add(on: bool) {
    with(|context| context.add = on);
}

/// Everything drawn from now on shows this much (1: as it is), until set again or the frame ends.
pub fn fade(alpha: f32) {
    with(|context| context.fade = alpha);
}

pub fn recolor(colors: Option<[Vec3; 3]>) {
    with(|context| context.recolor = colors);
}

/// What is drawn next has its hue turned by `hue` (of the colour wheel, 1 is all the way round), as the Haxe engine's ColorSwap shader does; None: as it is.
pub fn recolor_hue(hue: Option<f32>) {
    recolor(hue.map(|hue| [vec3(hue, 0.0, 0.0), Vec3::NAN, Vec3::ZERO]));
}

/// The same turn on the CPU, to test the shader's maths against.
#[cfg(test)]
fn hue_turned(rgb: Vec3, hue: f32) -> Vec3 {
    let (r, g, b) = (rgb.x, rgb.y, rgb.z);
    let step = |edge: f32, x: f32| if x < edge { 0.0 } else { 1.0 };
    let k = glam::Vec4::new(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = glam::Vec4::new(b, g, k.w, k.z).lerp(glam::Vec4::new(g, b, k.x, k.y), step(b, g));
    let q = glam::Vec4::new(p.x, p.y, p.w, r).lerp(glam::Vec4::new(r, p.y, p.z, p.x), step(p.x, r));
    let d = q.x - q.w.min(q.y);
    let e = 1.0e-10;
    let (h, s, v) = ((q.z + (q.w - q.y) / (6.0 * d + e)).abs() + hue, d / (q.x + e), q.x);
    let turned = Vec3::splat(h) + vec3(1.0, 2.0 / 3.0, 1.0 / 3.0);
    let p = ((turned - turned.floor()) * 6.0 - Vec3::splat(3.0)).abs();
    v * Vec3::ONE.lerp((p - Vec3::ONE).clamp(Vec3::ZERO, Vec3::ONE), s)
}

pub fn draw_texture(picture: &Texture2D, x: f32, y: f32, color: Color) {
    with(|context| context.texture(picture, x, y, color, &DrawTextureParams::default()));
}

pub fn draw_texture_ex(picture: &Texture2D, x: f32, y: f32, color: Color, params: DrawTextureParams) {
    with(|context| context.texture(picture, x, y, color, &params));
}

pub fn draw_rectangle(x: f32, y: f32, width: f32, height: f32, color: Color) {
    draw_rectangle_turned(x, y, width, height, 0.0, color);
}

/// A rectangle whose shader sees a bitmap of `bitmap` pixels (openfl_TextureSize), the uv still running 0 to 1 across it.
pub fn draw_rectangle_bitmap(x: f32, y: f32, width: f32, height: f32, color: Color, bitmap: [f32; 2]) {
    with(|context| context.shader_size = Some(bitmap));
    draw_rectangle(x, y, width, height, color);
    with(|context| context.shader_size = None);
}

/// The same, turned around its middle by `rotation` radians.
pub fn draw_rectangle_turned(x: f32, y: f32, width: f32, height: f32, rotation: f32, color: Color) {
    with(|context| {
        let (white, was) = (context.white.clone().expect("opened"), context.recolor.take());
        context.texture(&white, x, y, color, &DrawTextureParams { dest_size: Some(vec2(width, height)), rotation, ..Default::default() });
        context.recolor = was;
    });
}

/// Text in the built-in letters, with its baseline at (x, y).
pub fn draw_text(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text_ex(text, x, y, TextParams { font_size: size as u16, color, ..Default::default() });
}

pub fn draw_text_ex(text: &str, x: f32, y: f32, params: TextParams) {
    with(|context| {
        let was = context.recolor.take();
        context.text(text, x, y, &params, true);
        context.recolor = was;
    });
}

pub fn measure_text(text: &str, font: Option<&Font>, font_size: u16, font_scale: f32) -> TextDimensions {
    TextDimensions { width: with(|context| context.text(text, 0.0, 0.0, &TextParams { font, font_size, font_scale, color: WHITE }, false)) }
}

pub fn is_key_down(key: KeyCode) -> bool {
    with(|context| context.down.contains(&key))
}

/// Went down since the last frame.
pub fn is_key_pressed(key: KeyCode) -> bool {
    with(|context| context.pressed.contains(&key))
}

pub fn is_key_released(key: KeyCode) -> bool {
    with(|context| context.released.contains(&key))
}

/// Alt+Enter went down since the last frame.
pub fn alt_enter_pressed() -> bool {
    with(|context| context.alt_enter)
}

/// The game's window is the one in front.
pub fn is_focused() -> bool {
    with(|context| context.focused)
}

pub fn get_last_key_pressed() -> Option<KeyCode> {
    with(|context| context.last_pressed)
}

/// Where the mouse is, in a screen of `size` stretched over the window.
pub fn mouse_position(size: Vec2) -> Vec2 {
    with(|context| context.mouse.at * size)
}

/// The left mouse button: went down this frame, is down.
pub fn is_mouse_pressed() -> bool {
    with(|context| context.mouse.pressed)
}

/// The right button went down this frame.
pub fn is_mouse_right_pressed() -> bool {
    with(|context| context.mouse.right_pressed)
}

pub fn is_mouse_right_down() -> bool {
    with(|context| context.mouse.right)
}

/// Came up this frame.
pub fn is_mouse_released() -> bool {
    with(|context| context.mouse.released)
}

pub fn is_mouse_down() -> bool {
    with(|context| context.mouse.down)
}

/// Notches the wheel turned this frame, away from the player positive.
pub fn mouse_wheel() -> f32 {
    with(|context| context.mouse.wheel)
}

pub fn mouse_moved() -> bool {
    with(|context| context.mouse.moved)
}

/// Shows or hides the pointer over the window.
pub fn show_mouse(shown: bool) {
    with(|context| {
        if context.mouse.shown != shown {
            context.window.set_cursor_visible(shown);
            context.mouse.shown = shown;
        }
    })
}

/// The next letter typed, in the order typed.
pub fn get_char_pressed() -> Option<char> {
    with(|context| context.typed.pop_front())
}

/// Seconds since the window opened.
pub fn get_time() -> f64 {
    with(|context| context.started.elapsed().as_secs_f64())
}

/// How far behind the start of its frame a picture is on screen, on average, in ms.
pub fn display_lag() -> f64 {
    with(|context| context.display_lag)
}

/// How long the last frame took, in seconds.
pub fn get_frame_time() -> f32 {
    with(|context| context.frame_time)
}

pub fn get_fps() -> i32 {
    (1.0 / get_frame_time()) as i32
}

/// How many pictures a second the display the window is on shows, where the system says.
pub fn refresh_rate() -> Option<f64> {
    with(|context| context.window.current_monitor()?.refresh_rate_millihertz().map(|rate| rate as f64 / 1000.0))
}

/// The window's title (a script's Lib.application.window.title).
pub fn set_title(title: &str) {
    with(|context| context.window.set_title(title));
}

/// `exclusive`: the display is taken for the game alone, instead of covered by a window without borders.
pub fn set_fullscreen(on: bool, exclusive: bool) {
    with(|context| context.fullscreen(on, exclusive));
}

/// The frame as drawn so far.
pub fn get_screen_data() -> Image {
    with(Context::capture)
}

#[cfg(test)]
mod tests {
    #[test]
    fn letter_sheet_rows_wrap_and_then_fill_up() {
        use super::{place, SHEET};
        assert_eq!(place((1, 1, 0), 10, 12), Some(((1, 1), (12, 1, 12))));
        // no room left in the row: the next row starts below the tallest letter
        assert_eq!(place((SHEET - 5, 1, 12), 10, 8), Some(((1, 14), (12, 14, 8))));
        // no room left on the sheet
        assert_eq!(place((SHEET - 5, SHEET - 20, 12), 10, 8), None);
        assert_eq!(place((1, SHEET - 5, 0), 4, 8), None);
    }

    use super::*;

    #[test]
    fn frames_are_spaced_evenly_and_a_late_one_is_not_made_up_for() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        assert_eq!(next_due(start, start + ms(3), crate::settings::UNLIMITED_FPS), start + ms(3), "unlimited: due at once, whatever was due before");
        // on time: exactly one frame after the last one was due, whenever within the frame it was asked
        assert_eq!(next_due(start, start + ms(3), 100.0), start + ms(10));
        assert_eq!(next_due(start, start + ms(10), 100.0), start + ms(10));
        // half a second late: the next is due at once, and the one after it a frame later, not all the missed ones
        let late = next_due(start, start + ms(500), 100.0);
        assert_eq!(late, start + ms(490));
        assert_eq!(next_due(late, start + ms(500), 100.0), start + ms(500));
    }

    #[test]
    fn colours_and_random_numbers_behave() {
        assert_eq!(Color::from_hex(0xFF8000), Color::new(1.0, 128.0 / 255.0, 0.0, 1.0));
        rand::srand(7);
        let rolls: Vec<i32> = (0..200).map(|_| rand::gen_range(0, 3)).collect();
        assert!(rolls.iter().all(|roll| (0..3).contains(roll)) && (0..3).all(|face| rolls.contains(&face)));
        assert!((0..200).map(|_| rand::gen_range(-1.0f32, 1.0)).all(|roll| (-1.0..1.0).contains(&roll)));
        rand::srand(7);
        assert_eq!((0..200).map(|_| rand::gen_range(0, 3)).collect::<Vec<i32>>(), rolls);
    }

    #[test]
    fn a_picture_lands_where_asked_stretched_cut_flipped_and_turned() {
        let whole = vec2(100.0, 50.0);
        // as it is: its own size, all of it
        let (corners, uvs) = placed(whole, 10.0, 20.0, &DrawTextureParams::default());
        assert_eq!((corners, uvs), ([vec2(10.0, 20.0), vec2(110.0, 20.0), vec2(110.0, 70.0), vec2(10.0, 70.0)], [vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)]));
        // a part of it, stretched and mirrored: the part's right edge is drawn on the left
        let part = DrawTextureParams { source: Some(Rect::new(50.0, 0.0, 25.0, 25.0)), dest_size: Some(vec2(4.0, 2.0)), flip_x: true, ..Default::default() };
        let (corners, uvs) = placed(whole, 10.0, 20.0, &part);
        assert_eq!((corners[2], uvs[0], uvs[2]), (vec2(14.0, 22.0), vec2(0.75, 0.0), vec2(0.5, 0.5)));
        // a quarter turn clockwise about its middle (12, 21): the top left corner ends up at the top right
        let (corners, _) = placed(whole, 10.0, 20.0, &DrawTextureParams { rotation: std::f32::consts::FRAC_PI_2, ..part });
        assert!(corners[0].distance(vec2(13.0, 19.0)) < 1e-4 && corners[1].distance(vec2(13.0, 23.0)) < 1e-4, "{corners:?}");
    }

    #[test]
    fn the_hue_turn_keeps_brightness_and_strength_as_the_colour_swap_shader_does() {
        let near = |a: Vec3, b: Vec3| (a - b).length() < 1e-3;
        assert!(near(hue_turned(Vec3::X, 1.0 / 3.0), Vec3::Y) && near(hue_turned(Vec3::Y, 1.0 / 3.0), Vec3::Z));
        assert!(near(hue_turned(vec3(0.5, 0.5, 0.5), 0.27), vec3(0.5, 0.5, 0.5)), "grey has no hue");
        // half way round, a muted red goes to its opposite: same strongest and weakest channel, the middle one moved
        assert!(near(hue_turned(vec3(0.8, 0.2, 0.2), 0.5), vec3(0.2, 0.8, 0.8)));
        assert!(near(hue_turned(vec3(0.8, 0.4, 0.2), 0.0), vec3(0.8, 0.4, 0.2)) && near(hue_turned(vec3(0.8, 0.4, 0.2), 1.0), vec3(0.8, 0.4, 0.2)));
        // orange a sixth on: the weakest stays 0.2 and the strongest 0.8 (yellow-ish)
        let moved = hue_turned(vec3(0.8, 0.4, 0.2), 1.0 / 12.0);
        assert!((moved.max_element() - 0.8).abs() < 1e-3 && (moved.min_element() - 0.2).abs() < 1e-3);
    }

    #[test]
    fn a_picture_file_becomes_rows_of_rgba() {
        let mut file = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut file, 2, 2);
            encoder.set_color(png::ColorType::Rgb);
            encoder.write_header().unwrap().write_image_data(&[255, 0, 0, 0, 0, 255, 0, 255, 0, 0, 0, 0]).unwrap();
        }
        let image = Image::from_file_with_format(&file, None).unwrap();
        assert_eq!((image.width, image.height, image.get_pixel(1, 0), image.get_pixel(0, 1)), (2, 2, Color::new(0.0, 0.0, 1.0, 1.0), Color::new(0.0, 1.0, 0.0, 1.0)));
        assert!(Image::from_file_with_format(b"not a picture", None).is_err());
    }
}
