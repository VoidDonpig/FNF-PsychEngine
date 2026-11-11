//! Sparrow sprite sheets: a picture plus an .xml that says where each animation frame sits in it.
use macroquad::prelude::*;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Where the frame's pixels are in the sheet.
    pub source: Rect,
    /// Where those pixels sit inside the frame: sheets store frames with their empty border cut off.
    pub offset: Vec2,
    /// Size of the whole frame, border included.
    pub size: Vec2,
    /// Stored in the sheet turned a quarter to save room: `source` is the turned rectangle.
    pub rotated: bool,
}

/// Frames by name, in name order (so an animation's frames are in playing order).
pub fn parse(xml: &str) -> Vec<(String, Frame)> {
    let attr = |tag: &str, name: &str| -> Option<String> {
        let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        Some(tag[start..start + tag[start..].find('"')?].to_owned())
    };
    let mut frames: Vec<(String, Frame)> = xml.split("<SubTexture").skip(1).filter_map(|tag| {
        let number = |name| attr(tag, name)?.parse::<f32>().ok();
        let source = Rect::new(number("x")?, number("y")?, number("width")?, number("height")?);
        let rotated = attr(tag, "rotated").is_some_and(|r| r == "true");
        let upright = if rotated { vec2(source.h, source.w) } else { vec2(source.w, source.h) };
        let offset = vec2(-number("frameX").unwrap_or(0.0), -number("frameY").unwrap_or(0.0));
        let size = vec2(number("frameWidth").unwrap_or(upright.x), number("frameHeight").unwrap_or(upright.y));
        Some((attr(tag, "name")?.replace("&apos;", "'").replace("&amp;", "&"), Frame { source, offset, size, rotated }))
    }).collect();
    frames.sort_by(|a, b| a.0.cmp(&b.0));
    frames
}

/// Frames of the animation whose frame names start with `prefix`, or only those numbered in `indices`.
pub fn animation(frames: &[(String, Frame)], prefix: &str, indices: &[u32]) -> Vec<Frame> {
    let all: Vec<&(String, Frame)> = frames.iter().filter(|(name, _)| name.starts_with(prefix)).collect();
    if indices.is_empty() {
        return all.iter().map(|(_, f)| *f).collect();
    }
    // a frame's number is the digits its name ends with
    let number = |name: &str| name[prefix.len()..].trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok();
    indices.iter().filter_map(|i| all.iter().find(|(name, _)| number(name) == Some(*i)).map(|(_, f)| *f)).collect()
}

/// Frames of a "packer" sheet (a .txt with `name = x y width height` lines), in file order.
pub fn parse_packer(txt: &str) -> Vec<(String, Frame)> {
    txt.lines().filter_map(|line| {
        let (name, numbers) = line.split_once('=')?;
        let n: Vec<f32> = numbers.split_whitespace().filter_map(|v| v.parse().ok()).collect();
        let source = Rect::new(*n.first()?, *n.get(1)?, *n.get(2)?, *n.get(3)?);
        Some((name.trim().to_owned(), Frame { source, offset: Vec2::ZERO, size: vec2(source.w, source.h), rotated: false }))
    }).collect()
}

/// The Anti-Aliasing option: when off, pictures are loaded to keep their hard edges however they are stretched.
pub static SMOOTH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// A picture from the bytes of its file.
pub fn texture(bytes: &[u8]) -> Texture2D {
    let texture = Texture2D::from_file_with_format(bytes, None);
    if !SMOOTH.load(std::sync::atomic::Ordering::Relaxed) {
        texture.set_filter(FilterMode::Nearest);
    }
    texture
}

pub struct Sheet {
    pub texture: Texture2D,
    pub frames: Vec<(String, Frame)>,
}

impl Sheet {
    /// Loads `image` (a .png) and the .xml beside it.
    pub fn load(image: &Path) -> Result<Sheet, String> {
        let read = |path: &Path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
        let texture = texture(&read(image)?);
        let frames = match read(&image.with_extension("xml")) {
            Ok(xml) => parse(&String::from_utf8_lossy(&xml)),
            Err(missing) => parse_packer(&String::from_utf8_lossy(&read(&image.with_extension("txt")).map_err(|_| missing)?)),
        };
        Ok(Sheet { texture, frames })
    }

    pub fn animation(&self, prefix: &str, indices: &[u32]) -> Vec<Frame> {
        animation(&self.frames, prefix, indices)
    }

    /// Draws a frame with the frame's top left corner at (x, y).
    pub fn draw(&self, frame: Frame, x: f32, y: f32, scale: f32, flip_x: bool, color: Color) {
        draw_frame(&self.texture, frame, x, y, scale, flip_x, color);
    }
}

/// Draws a frame of `texture` with the frame's top left corner at (x, y).
pub fn draw_frame(texture: &Texture2D, frame: Frame, x: f32, y: f32, scale: f32, flip_x: bool, color: Color) {
    let stored = vec2(frame.source.w, frame.source.h) * scale;
    let upright = if frame.rotated { vec2(stored.y, stored.x) } else { stored };
    // mirrored, the cut-off border swaps sides
    let inset = if flip_x { frame.size.x * scale - frame.offset.x * scale - upright.x } else { frame.offset.x * scale };
    let middle = vec2(x + inset, y + frame.offset.y * scale) + upright / 2.0;
    // a turned frame is drawn as stored around its middle and turned back; mirroring it sideways is then a flip along its stored height
    let params = if frame.rotated {
        DrawTextureParams { dest_size: Some(stored), source: Some(frame.source), rotation: -std::f32::consts::FRAC_PI_2, flip_y: flip_x, ..Default::default() }
    } else {
        DrawTextureParams { dest_size: Some(stored), source: Some(frame.source), flip_x, ..Default::default() }
    };
    draw_texture_ex(texture, middle.x - stored.x / 2.0, middle.y - stored.y / 2.0, color, params);
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<TextureAtlas imagePath="a.png">
        <SubTexture name="BF idle dance0001" x="10" y="0" width="5" height="5"/>
        <SubTexture name="BF idle dance0000" x="0" y="0" width="5" height="5"/>
        <SubTexture name="BF idle dance0013" x="20" y="0" width="5" height="5"/>
        <SubTexture name="left press0000" x="289" y="395" width="140" height="142" frameX="-3" frameY="-4" frameWidth="146" frameHeight="149"/>
        <SubTexture name="turned0000" x="0" y="0" width="30" height="10" rotated="true"/>
    </TextureAtlas>"#;

    #[test]
    fn reads_frames_with_their_trim() {
        let frames = parse(XML);
        assert_eq!(frames.len(), 5);
        let press = frames.iter().find(|(n, _)| n == "left press0000").unwrap().1;
        assert_eq!(press, Frame { source: Rect::new(289.0, 395.0, 140.0, 142.0), offset: vec2(3.0, 4.0), size: vec2(146.0, 149.0), rotated: false });
        let turned = frames.iter().find(|(n, _)| n == "turned0000").unwrap().1;
        assert!(turned.rotated && turned.size == vec2(10.0, 30.0), "a turned frame's own size is the upright one");
    }

    #[test]
    fn packer_sheets_keep_file_order() {
        let frames = parse_packer("trees_0 = 0 0 512 512\ntrees_1 = 513 0 512 512\n\nbroken line\n");
        assert_eq!(frames.iter().map(|(n, f)| (n.as_str(), f.source.x, f.size.x)).collect::<Vec<_>>(), [("trees_0", 0.0, 512.0), ("trees_1", 513.0, 512.0)]);
    }

    #[test]
    fn animations_play_in_order_or_by_number() {
        let frames = parse(XML);
        let xs = |frames: Vec<Frame>| frames.iter().map(|f| f.source.x).collect::<Vec<_>>();
        assert_eq!(xs(animation(&frames, "BF idle dance", &[])), [0.0, 10.0, 20.0]);
        assert_eq!(xs(animation(&frames, "BF idle dance", &[13, 0, 7])), [20.0, 0.0]);
        assert!(animation(&frames, "nothing", &[]).is_empty());
    }
}
