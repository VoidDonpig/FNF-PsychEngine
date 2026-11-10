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
        // shortcut: frames stored rotated (rotated="true", a few late sheets) are drawn unrotated. Turn them in draw() when a character that uses them is added.
        let offset = vec2(-number("frameX").unwrap_or(0.0), -number("frameY").unwrap_or(0.0));
        let size = vec2(number("frameWidth").unwrap_or(source.w), number("frameHeight").unwrap_or(source.h));
        Some((attr(tag, "name")?.replace("&apos;", "'").replace("&amp;", "&"), Frame { source, offset, size }))
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

pub struct Sheet {
    pub texture: Texture2D,
    pub frames: Vec<(String, Frame)>,
}

impl Sheet {
    /// Loads `image` (a .png) and the .xml beside it.
    pub fn load(image: &Path) -> Result<Sheet, String> {
        let read = |path: &Path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
        let texture = Texture2D::from_file_with_format(&read(image)?, None);
        Ok(Sheet { texture, frames: parse(&String::from_utf8_lossy(&read(&image.with_extension("xml"))?)) })
    }

    pub fn animation(&self, prefix: &str, indices: &[u32]) -> Vec<Frame> {
        animation(&self.frames, prefix, indices)
    }

    /// Draws a frame with the frame's top left corner at (x, y).
    pub fn draw(&self, frame: Frame, x: f32, y: f32, scale: f32, flip_x: bool, color: Color) {
        // mirrored, the cut-off border swaps sides
        let inset = if flip_x { frame.size.x - frame.offset.x - frame.source.w } else { frame.offset.x };
        let dest = vec2(frame.source.w, frame.source.h) * scale;
        draw_texture_ex(&self.texture, x + inset * scale, y + frame.offset.y * scale, color, DrawTextureParams { dest_size: Some(dest), source: Some(frame.source), flip_x, ..Default::default() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<TextureAtlas imagePath="a.png">
        <SubTexture name="BF idle dance0001" x="10" y="0" width="5" height="5"/>
        <SubTexture name="BF idle dance0000" x="0" y="0" width="5" height="5"/>
        <SubTexture name="BF idle dance0013" x="20" y="0" width="5" height="5"/>
        <SubTexture name="left press0000" x="289" y="395" width="140" height="142" frameX="-3" frameY="-4" frameWidth="146" frameHeight="149"/>
    </TextureAtlas>"#;

    #[test]
    fn reads_frames_with_their_trim() {
        let frames = parse(XML);
        assert_eq!(frames.len(), 4);
        let press = frames.iter().find(|(n, _)| n == "left press0000").unwrap().1;
        assert_eq!(press, Frame { source: Rect::new(289.0, 395.0, 140.0, 142.0), offset: vec2(3.0, 4.0), size: vec2(146.0, 149.0) });
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
