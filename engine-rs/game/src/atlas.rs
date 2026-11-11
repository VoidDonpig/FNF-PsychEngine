//! Sparrow sprite sheets: a picture plus an .xml that says where each animation frame sits in it.
use crate::gfx::*;
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
    let frames: Vec<(String, Frame)> = xml.split("<SubTexture").skip(1).filter_map(|tag| {
        let number = |name| attr(tag, name)?.parse::<f32>().ok();
        let source = Rect::new(number("x")?, number("y")?, number("width")?, number("height")?);
        let rotated = attr(tag, "rotated").is_some_and(|r| r == "true");
        let upright = if rotated { vec2(source.h, source.w) } else { vec2(source.w, source.h) };
        let offset = vec2(-number("frameX").unwrap_or(0.0), -number("frameY").unwrap_or(0.0));
        let size = vec2(number("frameWidth").unwrap_or(upright.x), number("frameHeight").unwrap_or(upright.y));
        Some((attr(tag, "name")?.replace("&apos;", "'").replace("&amp;", "&"), Frame { source, offset, size, rotated }))
    }).collect();
    // (kept in the file's order: the first frame gives a character its size, as in Flixel)
    frames
}

/// What frames are put in order by within an animation: the name, then its number, so that "walk 10" comes after "walk 9".
fn order(name: &str) -> (&str, u32) {
    let stem = name.trim_end_matches(|c: char| c.is_ascii_digit());
    (stem, name[stem.len()..].parse::<u32>().unwrap_or(0))
}

/// Frames of the animation whose frame names start with `prefix`, or only those numbered in `indices`.
pub fn animation(frames: &[(String, Frame)], prefix: &str, indices: &[u32]) -> Vec<Frame> {
    let mut all: Vec<&(String, Frame)> = frames.iter().filter(|(name, _)| name.starts_with(prefix)).collect();
    all.sort_by(|a, b| order(&a.0).cmp(&order(&b.0)));
    if indices.is_empty() {
        return all.iter().map(|(_, f)| *f).collect();
    }
    // a frame's number is what follows the prefix at once ("GF Dancing Beat0003", not "GF Dancing Beat Hair blowing0003")
    let number = |name: &str| {
        let rest = name[prefix.len()..].trim_start();
        rest[..rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len()].parse::<u32>().ok()
    };
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

/// Frames of a sheet described in JSON (what Aseprite and TexturePacker write), in the file's order.
pub fn parse_json(json: &str) -> Vec<(String, Frame)> {
    let json: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    // the frames are a list with a "filename" each, or a table by file name
    let listed = json["frames"].as_array().into_iter().flatten().filter_map(|frame| Some((frame["filename"].as_str()?, frame)));
    let tabled = json["frames"].as_object().into_iter().flatten().map(|(name, frame)| (name.as_str(), frame));
    let frames: Vec<(String, Frame)> = listed.chain(tabled).filter_map(|(name, frame)| {
        let number = |value: &serde_json::Value| value.as_f64().map(|n| n as f32);
        let (rect, rotated) = (&frame["frame"], frame["rotated"].as_bool().unwrap_or(false));
        // a turned frame's width and height are given as they were before turning
        let (w, h) = (number(&rect["w"])?, number(&rect["h"])?);
        let source = if rotated { Rect::new(number(&rect["x"])?, number(&rect["y"])?, h, w) } else { Rect::new(number(&rect["x"])?, number(&rect["y"])?, w, h) };
        let offset = vec2(number(&frame["spriteSourceSize"]["x"]).unwrap_or(0.0), number(&frame["spriteSourceSize"]["y"]).unwrap_or(0.0));
        let size = vec2(number(&frame["sourceSize"]["w"]).unwrap_or(w), number(&frame["sourceSize"]["h"]).unwrap_or(h));
        // "walk 10.aseprite": the name is what stands before the file ending
        let name = [".aseprite", ".ase", ".png"].iter().find_map(|ending| name.strip_suffix(ending)).unwrap_or(name);
        Some((name.to_owned(), Frame { source, offset, size, rotated }))
    }).collect();
    frames
}

/// The Anti-Aliasing option: when off, pictures are loaded to keep their hard edges however they are stretched.
pub static SMOOTH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

thread_local! {
    /// The pictures read so far, by file: a song played again, or tried again, does not read its pictures again.
    static PICTURES: std::cell::RefCell<std::collections::HashMap<std::path::PathBuf, Texture2D>> = Default::default();
}

/// The picture in a file. None when there is no such file.
pub fn picture(path: &Path) -> Option<Texture2D> {
    let kept = PICTURES.with(|pictures| pictures.borrow().get(path).cloned());
    let texture = match kept {
        Some(texture) => texture,
        None => {
            let texture = Texture2D::from_file_with_format(&std::fs::read(path).ok()?, None);
            PICTURES.with(|pictures| pictures.borrow_mut().insert(path.to_owned(), texture.clone()));
            texture
        }
    };
    texture.set_filter(if SMOOTH.load(std::sync::atomic::Ordering::Relaxed) { FilterMode::Linear } else { FilterMode::Nearest });
    Some(texture)
}

/// Lets go of the pictures nothing shows any more. Called when a song has been loaded: what only the song before used goes.
pub fn forget_unused() {
    PICTURES.with(|pictures| pictures.borrow_mut().retain(|_, texture| !texture.alone()));
}

pub struct Sheet {
    pub texture: Texture2D,
    pub frames: Vec<(String, Frame)>,
}

impl Sheet {
    /// Loads `image` (a .png) and the .xml beside it.
    pub fn load(image: &Path) -> Result<Sheet, String> {
        let read = |path: &Path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
        let texture = picture(image).ok_or_else(|| format!("{}: not found", image.display()))?;
        // the frames are described beside the picture in one of three ways
        let text = |ending: &str| read(&image.with_extension(ending)).map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        let frames = match text("xml") {
            Ok(xml) => parse(&xml),
            Err(missing) => match text("json").map(|json| parse_json(&json)) {
                // (a JSON file beside a picture may be about something else)
                Ok(frames) if !frames.is_empty() => frames,
                _ => parse_packer(&text("txt").map_err(|_| missing)?),
            },
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
    fn reads_sheets_described_in_json() {
        // a list, as Aseprite writes it: numbers without leading zeros, and a trimmed frame
        let frames = parse_json(r#"{"frames": [
            {"filename": "walk 10.aseprite", "frame": {"x": 20, "y": 0, "w": 8, "h": 9}},
            {"filename": "walk 2.aseprite", "frame": {"x": 10, "y": 0, "w": 8, "h": 9}, "spriteSourceSize": {"x": 1, "y": 2, "w": 8, "h": 9}, "sourceSize": {"w": 16, "h": 16}},
            {"filename": "idle 0.aseprite", "frame": {"x": 0, "y": 0, "w": 8, "h": 9}}]}"#);
        assert_eq!(frames.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["walk 10", "walk 2", "idle 0"]);
        // an animation plays by number: 2 before 10
        assert_eq!(animation(&frames, "walk", &[]).iter().map(|f| f.source.x).collect::<Vec<_>>(), [10.0, 20.0]);
        assert_eq!(frames[1].1, Frame { source: Rect::new(10.0, 0.0, 8.0, 9.0), offset: vec2(1.0, 2.0), size: vec2(16.0, 16.0), rotated: false });
        assert_eq!(animation(&frames, "walk", &[10]).len(), 1);
        // a table, as TexturePacker writes it; a turned frame lies on its side in the picture
        let frames = parse_json(r#"{"frames": {"spin0001.png": {"frame": {"x": 0, "y": 0, "w": 30, "h": 10}, "rotated": true}}}"#);
        assert_eq!(frames, vec![("spin0001".to_owned(), Frame { source: Rect::new(0.0, 0.0, 10.0, 30.0), offset: Vec2::ZERO, size: vec2(30.0, 10.0), rotated: true })]);
    }

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
        // by number, a longer name that starts the same is another animation: the girlfriend's dance is not her hair landing
        let frames = parse(r#"<SubTexture name="GF Dancing Beat Hair Landing0001" x="50" y="0" width="5" height="5"/>
            <SubTexture name="GF Dancing Beat0001" x="60" y="0" width="5" height="5"/>"#);
        assert_eq!(xs(animation(&frames, "GF Dancing Beat", &[1])), [60.0]);
    }
}
