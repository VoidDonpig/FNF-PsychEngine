//! Things drawn the Animate way: a timeline of layers that place pieces of one picture, through symbols nested in symbols.
//! Read from a folder holding `Animation.json` and `spritemap1.json`/`.png`, in either of the two spellings Animate exports.
use crate::gfx::*;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// How a symbol placed on a timeline plays while its key frame lasts.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Repeat {
    Loop,
    Once,
    /// Shows one frame.
    Still,
}

enum Element {
    /// A piece of the picture, by name.
    Piece(String, Affine2),
    /// Another timeline, starting at its frame `first`.
    Symbol { name: String, at: Affine2, first: usize, repeat: Repeat },
}

/// A stretch of a layer during which the same elements show.
struct Key {
    start: usize,
    length: usize,
    label: Option<String>,
    elements: Vec<Element>,
}

/// Layers of key frames, the front layer first as in the file.
#[derive(Default)]
struct Timeline {
    layers: Vec<Vec<Key>>,
}

impl Timeline {
    fn length(&self) -> usize {
        self.layers.iter().filter_map(|layer| layer.last()).map(|key| key.start + key.length).max().unwrap_or(0)
    }
}

/// The value under a key that Animate spells short or long.
fn get<'a>(value: &'a Value, short: &str, long: &str) -> &'a Value {
    if value[short].is_null() { &value[long] } else { &value[short] }
}

/// A 2D placement out of the 4x4 matrix the file gives, as a list or as named cells.
fn placement(matrix: &Value) -> Affine2 {
    let cell = |index: usize, name: &str| if matrix.is_array() { &matrix[index] } else { &matrix[name] }.as_f64().unwrap_or(if index % 5 == 0 { 1.0 } else { 0.0 }) as f32;
    Affine2::from_cols_array(&[cell(0, "m00"), cell(1, "m01"), cell(4, "m10"), cell(5, "m11"), cell(12, "m30"), cell(13, "m31")])
}

fn timeline(value: &Value) -> Timeline {
    let layers = get(value, "L", "LAYERS").as_array().into_iter().flatten().map(|layer| {
        get(layer, "FR", "Frames").as_array().into_iter().flatten().map(|key| Key {
            start: get(key, "I", "index").as_u64().unwrap_or(0) as usize,
            length: get(key, "DU", "duration").as_u64().unwrap_or(1) as usize,
            label: get(key, "N", "name").as_str().map(str::to_owned),
            elements: get(key, "E", "elements").as_array().into_iter().flatten().filter_map(|element| {
                let (piece, symbol) = (get(element, "ASI", "ATLAS_SPRITE_instance"), get(element, "SI", "SYMBOL_Instance"));
                if piece.is_object() {
                    return Some(Element::Piece(get(piece, "N", "name").as_str()?.to_owned(), placement(get(piece, "M3D", "Matrix3D"))));
                }
                let first = get(symbol, "FF", "firstFrame").as_u64();
                // a symbol with no word on how it plays (a movie clip) shows its first frame
                let repeat = match get(symbol, "LP", "loop").as_str() {
                    Some("LP" | "loop") => Repeat::Loop,
                    Some("PO" | "playonce") => Repeat::Once,
                    _ => Repeat::Still,
                };
                Some(Element::Symbol { name: get(symbol, "SN", "SYMBOL_name").as_str()?.to_owned(), at: placement(get(symbol, "M3D", "Matrix3D")), first: first.unwrap_or(0) as usize, repeat })
            }).collect(),
        }).collect()
    }).collect();
    Timeline { layers }
}

/// The moving parts of an atlas, without its picture.
pub struct Rig {
    main: Timeline,
    symbols: HashMap<String, Timeline>,
    /// Frames a second.
    pub fps: f32,
}

impl Rig {
    pub fn parse(text: &str) -> Option<Rig> {
        let json: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
        let animation = get(&json, "AN", "ANIMATION");
        let symbols = get(get(&json, "SD", "SYMBOL_DICTIONARY"), "S", "Symbols").as_array().into_iter().flatten();
        let symbols = symbols.filter_map(|symbol| Some((get(symbol, "SN", "SYMBOL_name").as_str()?.to_owned(), timeline(get(symbol, "TL", "TIMELINE"))))).collect();
        let main = timeline(get(animation, "TL", "TIMELINE"));
        let fps = get(get(&json, "MD", "metadata"), "FRT", "framerate").as_f64().unwrap_or(24.0) as f32;
        (main.length() > 0).then_some(Rig { main, symbols, fps })
    }

    /// Frames of the main timeline.
    pub fn length(&self) -> usize {
        self.main.length()
    }

    /// The frames of the main timeline that carry a label: where they start and how many there are.
    pub fn label(&self, name: &str) -> Option<(usize, usize)> {
        self.main.layers.iter().flatten().find(|key| key.label.as_deref() == Some(name)).map(|key| (key.start, key.length))
    }

    /// The pieces that make up a frame of the main timeline, back to front, each with where it goes.
    pub fn pieces(&self, frame: usize) -> Vec<(&str, Affine2)> {
        let mut pieces = Vec::new();
        self.collect(&self.main, frame, Affine2::IDENTITY, 0, &mut pieces);
        pieces
    }

    /// The pieces of a frame of one symbol played on its own (its last frame once it is through), as `pieces` gives them.
    pub fn pieces_of(&self, symbol: &str, frame: usize) -> Vec<(&str, Affine2)> {
        let mut pieces = Vec::new();
        if let Some(timeline) = self.symbols.get(symbol) {
            self.collect(timeline, frame.min(timeline.length().saturating_sub(1)), Affine2::IDENTITY, 0, &mut pieces);
        }
        pieces
    }

    fn collect<'a>(&'a self, timeline: &'a Timeline, frame: usize, at: Affine2, depth: usize, pieces: &mut Vec<(&'a str, Affine2)>) {
        // a symbol that holds itself would never end
        if depth > 16 {
            return;
        }
        for layer in timeline.layers.iter().rev() {
            let Some(key) = layer.iter().find(|key| (key.start..key.start + key.length).contains(&frame)) else { continue };
            for element in &key.elements {
                match element {
                    Element::Piece(name, placed) => pieces.push((name, at * *placed)),
                    Element::Symbol { name, at: placed, first, repeat } => {
                        let Some(symbol) = self.symbols.get(name) else { continue };
                        let (length, played) = (symbol.length().max(1), frame - key.start);
                        let inner = match repeat {
                            Repeat::Loop => (first + played) % length,
                            Repeat::Once => (first + played).min(length - 1),
                            Repeat::Still => (*first).min(length - 1),
                        };
                        self.collect(symbol, inner, at * *placed, depth + 1, pieces);
                    }
                }
            }
        }
    }
}

/// A rig with its picture.
pub struct Atlas {
    pub rig: Rig,
    texture: Texture2D,
    /// Where each piece is in the picture.
    sprites: HashMap<String, Rect>,
}

impl Atlas {
    /// Loads the atlas in folder `dir`.
    pub fn load(dir: &Path) -> Result<Atlas, String> {
        let read = |name: &str| std::fs::read(dir.join(name)).map_err(|e| format!("{}: {e}", dir.join(name).display()));
        let rig = Rig::parse(&String::from_utf8_lossy(&read("Animation.json")?)).ok_or(format!("{}: no animation in it", dir.join("Animation.json").display()))?;
        let map: Value = serde_json::from_str(String::from_utf8_lossy(&read("spritemap1.json")?).trim_start_matches('\u{feff}')).map_err(|e| format!("{}: {e}", dir.join("spritemap1.json").display()))?;
        // shortcut: one picture per atlas and pieces stored upright, as every atlas of the game is. Read spritemap2 onwards and turn `rotated` pieces when a mod needs it.
        let sprites = map["ATLAS"]["SPRITES"].as_array().into_iter().flatten().filter_map(|sprite| {
            let sprite = &sprite["SPRITE"];
            let number = |key: &str| sprite[key].as_f64().map(|n| n as f32);
            Some((sprite["name"].as_str()?.to_owned(), Rect::new(number("x")?, number("y")?, number("w")?, number("h")?)))
        }).collect();
        Ok(Atlas { rig, texture: crate::atlas::picture(&dir.join("spritemap1.png")).ok_or(format!("{}: not found", dir.join("spritemap1.png").display()))?, sprites })
    }

    /// Draws a frame with the timeline's origin at `at`.
    pub fn draw(&self, frame: usize, at: Vec2, scale: f32, flip_x: bool, color: Color) {
        self.put(self.rig.pieces(frame.min(self.rig.length().saturating_sub(1))), at, scale, flip_x, color);
    }

    /// Draws a frame of one symbol played on its own, with the symbol's origin at `at`.
    pub fn draw_symbol(&self, symbol: &str, frame: usize, at: Vec2) {
        self.put(self.rig.pieces_of(symbol, frame), at, 1.0, false, WHITE);
    }

    fn put(&self, pieces: Vec<(&str, Affine2)>, at: Vec2, scale: f32, flip_x: bool, color: Color) {
        // mirrored, it is turned over around the origin
        let whole = Affine2::from_translation(at) * Affine2::from_scale(vec2(if flip_x { -scale } else { scale }, scale));
        for (name, placed) in pieces {
            let Some(source) = self.sprites.get(name) else { continue };
            let Affine2 { matrix2: turn, translation: shift } = whole * placed;
            push_model_matrix(Mat4::from_cols(turn.x_axis.extend(0.0).extend(0.0), turn.y_axis.extend(0.0).extend(0.0), Vec4::Z, shift.extend(0.0).extend(1.0)));
            draw_texture_ex(&self.texture, 0.0, 0.0, color, DrawTextureParams { source: Some(*source), ..Default::default() });
            pop_model_matrix();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // an arm that swings over two frames, placed once looping and once held on its second frame; the second key is labelled
    const SHORT: &str = r#"{"AN": {"SN": "all", "TL": {"L": [
        {"LN": "labels", "FR": [{"I": 0, "DU": 2}, {"N": "Wave", "I": 2, "DU": 3}]},
        {"LN": "front", "FR": [{"I": 0, "DU": 5, "E": [{"SI": {"SN": "arm", "ST": "G", "FF": 0, "LP": "LP", "M3D": [1,0,0,0, 0,1,0,0, 0,0,1,0, 10,20,0,1]}}]}]},
        {"LN": "back", "FR": [{"I": 0, "DU": 5, "E": [{"SI": {"SN": "arm", "ST": "G", "FF": 1, "LP": "SF", "M3D": [2,0,0,0, 0,2,0,0, 0,0,1,0, 0,0,0,1]}}]}]}
    ]}}, "SD": {"S": [{"SN": "arm", "TL": {"L": [{"FR": [
        {"I": 0, "DU": 1, "E": [{"ASI": {"N": "0000", "M3D": [1,0,0,0, 0,1,0,0, 0,0,1,0, 1,2,0,1]}}]},
        {"I": 1, "DU": 1, "E": [{"ASI": {"N": "0001", "M3D": [1,0,0,0, 0,1,0,0, 0,0,1,0, 3,4,0,1]}}]}
    ]}]}}]}, "MD": {"FRT": 12.0}}"#;

    #[test]
    fn a_frame_is_its_layers_pieces_back_to_front_through_nested_symbols() {
        let rig = Rig::parse(SHORT).unwrap();
        assert_eq!((rig.length(), rig.fps, rig.label("Wave"), rig.label("Nothing")), (5, 12.0, Some((2, 3)), None));
        let at = |pieces: Vec<(&str, Affine2)>| pieces.into_iter().map(|(name, placed)| (name.to_owned(), placed.translation, placed.matrix2.x_axis.x)).collect::<Vec<_>>();
        // the back layer first: the held arm twice as large, then the looping arm moved by its placement
        assert_eq!(at(rig.pieces(0)), [("0001".to_owned(), vec2(6.0, 8.0), 2.0), ("0000".to_owned(), vec2(11.0, 22.0), 1.0)]);
        assert_eq!(at(rig.pieces(1)), [("0001".to_owned(), vec2(6.0, 8.0), 2.0), ("0001".to_owned(), vec2(13.0, 24.0), 1.0)]);
        // a symbol played on its own: its frames where it puts them, the last one once it is through, nothing for a name unknown
        assert_eq!(at(rig.pieces_of("arm", 0)), [("0000".to_owned(), vec2(1.0, 2.0), 1.0)]);
        assert_eq!((at(rig.pieces_of("arm", 9)), rig.pieces_of("leg", 0).len()), (vec![("0001".to_owned(), vec2(3.0, 4.0), 1.0)], 0));
        assert_eq!(at(rig.pieces(2))[1].0, "0000", "the loop comes round");
    }

    #[test]
    fn the_long_spelling_reads_the_same() {
        let long = r#"{"ANIMATION": {"SYMBOL_name": "all", "TIMELINE": {"LAYERS": [{"Layer_name": "only", "Frames": [{"index": 0, "duration": 2, "name": "Blink", "elements": [
            {"SYMBOL_Instance": {"SYMBOL_name": "eye", "symbolType": "graphic", "firstFrame": 0, "loop": "playonce", "Matrix3D": {"m00": 1, "m01": 0, "m10": 0, "m11": 1, "m30": 5, "m31": 6}}}]}]}]}},
            "SYMBOL_DICTIONARY": {"Symbols": [{"SYMBOL_name": "eye", "TIMELINE": {"LAYERS": [{"Frames": [{"index": 0, "duration": 1, "elements": [
            {"ATLAS_SPRITE_instance": {"name": "0007", "Matrix3D": {"m00": 1, "m01": 0, "m10": 0, "m11": 1, "m30": 0, "m31": 0}}}]}]}]}}]},
            "metadata": {"framerate": 24.0}}"#;
        let rig = Rig::parse(long).unwrap();
        assert_eq!(rig.label("Blink"), Some((0, 2)));
        // played once, the one-frame eye stays on its frame
        assert_eq!(rig.pieces(1).iter().map(|(name, placed)| (*name, placed.translation)).collect::<Vec<_>>(), [("0007", vec2(5.0, 6.0))]);
        assert!(Rig::parse("{}").is_none());
    }
}
