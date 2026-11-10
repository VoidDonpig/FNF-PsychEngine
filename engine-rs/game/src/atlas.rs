//! Sparrow texture atlases (the .xml next to a sprite sheet).
use macroquad::math::Rect;
use std::collections::HashMap;

/// Frame name -> where it sits in the sheet.
// shortcut: the trim offsets (frameX/frameY) are ignored, frames are drawn centred. Read them once animations need exact registration.
pub fn parse(xml: &str) -> HashMap<String, Rect> {
    let attr = |tag: &str, name: &str| -> Option<String> {
        let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        Some(tag[start..start + tag[start..].find('"')?].to_owned())
    };
    xml.split("<SubTexture").skip(1).filter_map(|tag| {
        let number = |name| attr(tag, name)?.parse::<f32>().ok();
        Some((attr(tag, "name")?, Rect::new(number("x")?, number("y")?, number("width")?, number("height")?)))
    }).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_frames() {
        let frames = super::parse(r#"<TextureAtlas imagePath="a.png">
            <SubTexture name="purple0000" x="795" y="234" width="154" height="157"/>
            <SubTexture name="left press0000" x="289" y="395" width="140" height="142" frameX="-3" frameY="-3" frameWidth="146" frameHeight="149"/>
        </TextureAtlas>"#);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames["left press0000"], macroquad::math::Rect::new(289.0, 395.0, 140.0, 142.0));
    }
}
