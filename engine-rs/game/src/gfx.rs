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
pub const GREEN: Color = Color::new(0.0, 0.89, 0.19, 1.0);
pub const YELLOW: Color = Color::new(0.99, 0.98, 0.0, 1.0);
pub const ORANGE: Color = Color::new(1.0, 0.63, 0.0, 1.0);

/// Hue, saturation and lightness of a colour, each 0 to 1.
pub fn rgb_to_hsl(color: Color) -> (f32, f32, f32) {
    let (max, min) = (color.r.max(color.g).max(color.b), color.r.min(color.g).min(color.b));
    let (light, spread) = ((max + min) / 2.0, max - min);
    if spread == 0.0 {
        return (0.0, 0.0, light);
    }
    let saturation = if light < 0.5 { spread / (max + min) } else { spread / (2.0 - max - min) };
    let hue = if max == color.r { (color.g - color.b) / spread } else if max == color.g { 2.0 + (color.b - color.r) / spread } else { 4.0 + (color.r - color.g) / spread };
    ((hue / 6.0).rem_euclid(1.0), saturation, light)
}

pub fn hsl_to_rgb(hue: f32, saturation: f32, light: f32) -> Color {
    let high = if light < 0.5 { light * (1.0 + saturation) } else { light + saturation - light * saturation };
    let low = 2.0 * light - high;
    // each channel is the same ramp, a third of the circle apart
    let channel = |turn: f32| match turn.rem_euclid(1.0) {
        t if t < 1.0 / 6.0 => low + (high - low) * 6.0 * t,
        t if t < 0.5 => high,
        t if t < 2.0 / 3.0 => low + (high - low) * (2.0 / 3.0 - t) * 6.0,
        _ => low,
    };
    Color::new(channel(hue + 1.0 / 3.0), channel(hue), channel(hue - 1.0 / 3.0), 1.0)
}

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
    Left, Right, Up, Down, Space, Enter, Escape, Backspace, Home, End, F11, KpAdd, KpSubtract,
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
        Their::Space => Space, Their::Enter | Their::NumpadEnter => Enter, Their::Escape => Escape, Their::Backspace => Backspace, Their::Home => Home, Their::End => End, Their::F11 => F11,
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

/// A picture in main memory: rows of red, green, blue, alpha bytes, top row first.
pub struct Image {
    pub bytes: Vec<u8>,
    pub width: u16,
    pub height: u16,
}

impl Image {
    /// From the bytes of a PNG file.
    pub fn from_file_with_format(bytes: &[u8], _format: Option<()>) -> Result<Image, String> {
        let mut decoder = png::Decoder::new(bytes);
        // palettes and odd bit depths become plain 8-bit channels
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
        let mut raw = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut raw).map_err(|error| error.to_string())?;
        let pixels = info.width as usize * info.height as usize;
        let bytes: Vec<u8> = match info.color_type {
            png::ColorType::Rgba => {
                raw.truncate(pixels * 4);
                raw
            }
            png::ColorType::Rgb => raw[..pixels * 3].chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
            png::ColorType::GrayscaleAlpha => raw[..pixels * 2].chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
            png::ColorType::Grayscale => raw[..pixels].iter().flat_map(|&p| [p, p, p, 255]).collect(),
            png::ColorType::Indexed => return Err("a palette the decoder left in place".to_owned()),
        };
        if info.width > u16::MAX as u32 || info.height > u16::MAX as u32 {
            return Err("too large".to_owned());
        }
        Ok(Image { bytes, width: info.width as u16, height: info.height as u16 })
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

/// A picture on the graphics card. Copies are the same picture.
#[derive(Clone)]
pub struct Texture2D(Rc<Picture>);

impl Texture2D {
    /// From the bytes of a PNG file. A file that cannot be read becomes one transparent dot.
    pub fn from_file_with_format(bytes: &[u8], format: Option<()>) -> Texture2D {
        let image = Image::from_file_with_format(bytes, format).unwrap_or_else(|error| {
            eprintln!("Picture: {error}");
            Image::gen_image_color(1, 1, Color::new(0.0, 0.0, 0.0, 0.0))
        });
        Texture2D::from_image(&image)
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

    /// No copy of it is held anywhere else.
    pub fn alone(&self) -> bool {
        Rc::strong_count(&self.0) == 1
    }
}

/// Where a letter of one size sits in its font's sheet of letters, and how it is placed on the line.
#[derive(Clone, Copy)]
struct Glyph {
    source: Rect,
    left: f32,
    bottom: f32,
    advance: f32,
}

struct Letters {
    font: fontdue::Font,
    /// Letters drawn so far, by size, all on one sheet that fills up row after row.
    glyphs: RefCell<HashMap<(char, u16), Glyph>>,
    sheet: Texture2D,
    /// Where the next letter goes, and how tall the row it is in has become.
    next: Cell<(u32, u32, u32)>,
}

const SHEET: u32 = 1024;

#[derive(Clone)]
pub struct Font(Rc<Letters>);

pub fn load_ttf_font_from_bytes(bytes: &[u8]) -> Result<Font, String> {
    let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())?;
    let sheet = with(|context| context.picture(SHEET, SHEET, &vec![0; (SHEET * SHEET * 4) as usize]));
    Ok(Font(Rc::new(Letters { font, glyphs: RefCell::default(), sheet, next: Cell::new((1, 1, 0)) })))
}

impl Font {
    pub fn set_filter(&mut self, filter: FilterMode) {
        self.0.sheet.set_filter(filter);
    }

    /// The letter at that size, drawn onto the sheet the first time it is asked for.
    fn glyph(&self, letter: char, size: u16, context: &Context) -> Glyph {
        if let Some(known) = self.0.glyphs.borrow().get(&(letter, size)) {
            return *known;
        }
        let (metrics, coverage) = self.0.font.rasterize(letter, size as f32);
        let (width, height) = (metrics.width as u32, metrics.height as u32);
        let (mut x, mut y, mut row) = self.0.next.get();
        if x + width + 1 > SHEET {
            (x, y, row) = (1, y + row + 1, 0);
        }
        let mut glyph = Glyph { source: Rect::new(x as f32, y as f32, width as f32, height as f32), left: metrics.xmin as f32, bottom: metrics.ymin as f32, advance: metrics.advance_width };
        if y + height + 1 > SHEET {
            // shortcut: a full sheet leaves further letters blank. Start a second sheet if a font ever needs this many sizes.
            glyph.source = Rect::default();
        } else if width > 0 && height > 0 {
            let pixels: Vec<u8> = coverage.iter().flat_map(|&cover| [255, 255, 255, cover]).collect();
            context.write(&self.0.sheet.0.texture, x, y, width, height, &pixels);
            self.0.next.set((x + width + 1, y, row.max(height)));
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
}

@vertex
fn corner(@location(0) position: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>,
          @location(3) red: vec3<f32>, @location(4) green: vec3<f32>, @location(5) blue: vec3<f32>, @location(6) recolored: f32) -> Corner {
    return Corner(vec4<f32>(position, 0.0, 1.0), uv, color, red, green, blue, recolored);
}

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var stretch: sampler;

@fragment
fn pixel(at: Corner) -> @location(0) vec4<f32> {
    let seen = textureSample(picture, stretch, at.uv);
    // notes are stored as three masks, each of which takes one of the lane's colours
    let mixed = vec4<f32>(min(seen.r * at.red + seen.g * at.green + seen.b * at.blue, vec3<f32>(1.0)), seen.a);
    return mix(seen, mixed, at.recolored) * at.color;
}";

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
    picture: (u64, FilterMode),
    end: u32,
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
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    samplers: [wgpu::Sampler; 2],
    pictures: u64,
    white: Option<Texture2D>,
    letters: Option<Font>,
    /// The frame so far.
    vertices: Vec<Vertex>,
    calls: Vec<Call>,
    buffer: Option<(wgpu::Buffer, u64)>,
    background: Color,
    camera: Mat4,
    models: Vec<Mat4>,
    recolor: Option<[Vec3; 3]>,
    down: HashSet<KeyCode>,
    pressed: HashSet<KeyCode>,
    released: HashSet<KeyCode>,
    last_pressed: Option<KeyCode>,
    typed: VecDeque<char>,
    /// Alt+Enter went down since the last frame (it is not an Enter).
    alt_enter: bool,
    focused: bool,
    started: Instant,
    frame_began: Instant,
    frame_time: f32,
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
/// A frame that ran late does not make the following ones hurry.
fn next_due(last: Instant, now: Instant, fps: f64) -> Instant {
    let frame = Duration::from_secs_f64(1.0 / fps.max(1.0));
    (last + frame).max(now.checked_sub(frame).unwrap_or(now))
}

/// Opens the window. Returns what it draws with ("Vulkan, <graphics card>"), and a note when that is not what was asked for.
pub fn open(conf: Conf) -> Result<(String, Option<String>), String> {
    let mut event_loop = winit::event_loop::EventLoop::new().map_err(|error| error.to_string())?;
    // key presses are timed by the game's own raw input thread, and Windows hands raw input to one listener only
    event_loop.listen_device_events(winit::event_loop::DeviceEvents::Never);
    let attributes = winit::window::Window::default_attributes().with_title(conf.title).with_inner_size(winit::dpi::LogicalSize::new(conf.width, conf.height));
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
    let device = wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() };
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
    let attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x3, 4 => Float32x3, 5 => Float32x3, 6 => Float32];
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("corner"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attributes })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("pixel"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let sampler = |filter: wgpu::FilterMode| device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: filter, min_filter: filter, ..Default::default() });
    let samplers = [sampler(wgpu::FilterMode::Linear), sampler(wgpu::FilterMode::Nearest)];

    let info = adapter.get_info();
    let now = Instant::now();
    let mut context = Context {
        event_loop: Some(event_loop), app, window, instance, surface: Some(surface), device, queue, config, format, pipeline, layout, samplers, pictures: 0, white: None, letters: None,
        vertices: Vec::new(), calls: Vec::new(), buffer: None, background: BLACK, camera: Mat4::IDENTITY, models: Vec::new(), recolor: None,
        down: HashSet::new(), pressed: HashSet::new(), released: HashSet::new(), last_pressed: None, typed: VecDeque::new(), alt_enter: false, focused: true,
        started: now, frame_began: now, frame_time: 1.0 / 60.0, due: now, open: true,
    };
    context.white = Some(context.picture(1, 1, &[255; 4]));
    context.fullscreen(conf.fullscreen, false);
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
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.write(&texture, 0, 0, width, height, pixels);
        let view = texture.create_view(&Default::default());
        let group = |sampler: &wgpu::Sampler| {
            let entries = [wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) }];
            self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.layout, entries: &entries })
        };
        let groups = [group(&self.samplers[0]), group(&self.samplers[1])];
        self.pictures += 1;
        Texture2D(Rc::new(Picture { texture, width, height, groups, filter: Cell::new(FilterMode::Linear), id: self.pictures }))
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
        let id = (picture.0.id, filter);
        if self.calls.last().is_none_or(|call| call.picture != id) {
            self.calls.push(Call { group: picture.0.groups[filter as usize].clone(), picture: id, end: self.vertices.len() as u32 });
        }
        let place = self.camera * self.models.last().copied().unwrap_or(Mat4::IDENTITY);
        let [red, green, blue] = self.recolor.unwrap_or_default().map(|part| part.to_array());
        let corner = |index: usize| {
            let position = place.transform_point3(corners[index].extend(0.0));
            Vertex { position: [position.x, position.y], uv: uvs[index].to_array(), color: [color.r, color.g, color.b, color.a], red, green, blue, recolored: self.recolor.is_some() as u8 as f32 }
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
                self.texture(&font.0.sheet, x + pen + glyph.left * scale, top, params.color, &from);
            }
            pen += glyph.advance * scale;
        }
        pen
    }

    /// Sends the frame so far to the graphics card, to be drawn into `target`.
    fn render(&mut self, target: &wgpu::TextureView) {
        let bytes: &[u8] = bytemuck::cast_slice(&self.vertices);
        if self.buffer.as_ref().is_none_or(|(_, room)| *room < bytes.len() as u64) {
            let room = (bytes.len() as u64).next_power_of_two().max(1 << 16);
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: room, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            self.buffer = Some((buffer, room));
        }
        let Some((buffer, _)) = &self.buffer else { return };
        self.queue.write_buffer(buffer, 0, bytes);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let background = wgpu::Color { r: self.background.r as f64, g: self.background.g as f64, b: self.background.b as f64, a: 1.0 };
            let onto = wgpu::RenderPassColorAttachment { view: target, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(background), store: wgpu::StoreOp::Store } };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { color_attachments: &[Some(onto)], ..Default::default() });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, buffer.slice(..));
            let mut from = 0;
            for call in &self.calls {
                pass.set_bind_group(0, &call.group, &[]);
                pass.draw(from..call.end, 0..1);
                from = call.end;
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// The frame so far as a picture in main memory.
    fn capture(&mut self) -> Image {
        let (width, height) = (self.config.width, self.config.height);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
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
        context.models.clear();
        (context.recolor, context.last_pressed) = (None, None);

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
        context.background = color;
    });
}

pub fn set_camera(camera: &Camera2D) {
    // up on screen is up for the graphics card, and down for the game
    with(|context| context.camera = Mat4::from_scale(vec3(camera.zoom.x, -camera.zoom.y, 1.0)) * Mat4::from_translation(vec3(-camera.target.x, -camera.target.y, 0.0)));
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
pub fn recolor(colors: Option<[Vec3; 3]>) {
    with(|context| context.recolor = colors);
}

pub fn draw_texture(picture: &Texture2D, x: f32, y: f32, color: Color) {
    with(|context| context.texture(picture, x, y, color, &DrawTextureParams::default()));
}

pub fn draw_texture_ex(picture: &Texture2D, x: f32, y: f32, color: Color, params: DrawTextureParams) {
    with(|context| context.texture(picture, x, y, color, &params));
}

pub fn draw_rectangle(x: f32, y: f32, width: f32, height: f32, color: Color) {
    with(|context| {
        let (white, was) = (context.white.clone().expect("opened"), context.recolor.take());
        context.texture(&white, x, y, color, &DrawTextureParams { dest_size: Some(vec2(width, height)), ..Default::default() });
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

/// The next letter typed, in the order typed.
pub fn get_char_pressed() -> Option<char> {
    with(|context| context.typed.pop_front())
}

/// Seconds since the window opened.
pub fn get_time() -> f64 {
    with(|context| context.started.elapsed().as_secs_f64())
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
    use super::*;

    #[test]
    fn frames_are_spaced_evenly_and_a_late_one_is_not_made_up_for() {
        let start = Instant::now();
        let ms = Duration::from_millis;
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
        // there and back: hue, saturation and lightness say the same colour
        for hex in [0xC24B99u32, 0x00FFFF, 0x12FA05, 0xF9393F, 0x808080, 0x000000] {
            let (hue, saturation, light) = rgb_to_hsl(Color::from_hex(hex));
            let back = hsl_to_rgb(hue, saturation, light);
            let code = [back.r, back.g, back.b].iter().fold(0, |code, channel| code << 8 | (channel * 255.0).round() as u32);
            assert_eq!(code, hex);
        }
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
    fn a_picture_file_becomes_rows_of_rgba() {
        let mut file = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut file, 2, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.write_header().unwrap().write_image_data(&[255, 0, 0, 0, 0, 255]).unwrap();
        }
        let image = Image::from_file_with_format(&file, None).unwrap();
        assert_eq!((image.width, image.height, image.get_pixel(1, 0)), (2, 1, Color::new(0.0, 0.0, 1.0, 1.0)));
        assert!(Image::from_file_with_format(b"not a picture", None).is_err());
    }
}
