//! Mods' shaders (`shaders/<name>.frag`, written for OpenFL in the GLSL of OpenGL ES 2), turned into what the
//! graphics card is given here: the text rewritten for today's GLSL, read, and its uniforms found.
use wgpu::naga;

/// What a uniform holds: so many numbers (1 to 4), whole numbers, or one that is true or false.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Float(usize),
    Int(usize),
    Bool,
}

/// A uniform scripts may set: its name, kind and place in the uniforms' bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub name: String,
    pub kind: Kind,
    pub offset: usize,
}

/// A shader read: ready for the graphics card, with its uniforms, how many bytes they take, the pictures it asks for
/// besides the sprite's own (its `sampler2D` uniforms, in the order of their bindings), and the values the file gives.
pub struct Translated {
    pub module: naga::Module,
    pub slots: Vec<Slot>,
    pub size: usize,
    pub pictures: Vec<String>,
    pub defaults: Vec<(String, Vec<f64>)>,
}

/// What OpenFL and Flixel put where a shader says `#pragma header`, as this engine has it: the sprite's picture
/// (`bitmap`), its size, the place in it, and the sprite's colour and opacity. Pictures come out as OpenFL's do, their
/// colours already multiplied by their opacity, and so does what the shader answers.
const HEADER: &str = "#define bitmap sampler2D(funkin_bitmap_t, funkin_bitmap_s)
#define gl_FragColor funkin_FragColor
#define openfl_Alphav funkin_color.a
#define openfl_ColorMultiplierv funkin_color
#define openfl_ColorOffsetv vec4(0.0)
#define openfl_HasColorTransform false
#define hasTransform true
#define hasColorTransform false
vec4 funkin_pre(vec4 c) { return vec4(c.rgb * c.a, c.a); }
vec4 funkin_tinted(vec4 c) { return vec4(c.rgb * funkin_color.rgb, 1.0) * (c.a * funkin_color.a); }
#define texture2D(funkin_t, funkin_c) funkin_pre(textureLod(funkin_t, funkin_c, 0.0))
#define flixel_texture2D(funkin_t, funkin_c) funkin_tinted(textureLod(funkin_t, funkin_c, 0.0))
";

const TYPES: [&str; 12] = ["float", "int", "bool", "vec2", "vec3", "vec4", "ivec2", "ivec3", "ivec4", "mat2", "mat3", "mat4"];

/// Rewrites a fragment shader written for OpenFL: its uniforms gathered into one block (and its pictures given
/// bindings), the header's names defined, and the values it gives its globals outside any function (which only some
/// drivers take) given at the start of `main` instead.
// shortcut: arrays of uniforms, `varying`s of its own and vertex shaders are not taken; such a shader is refused with
// the reason. `gl_FragCoord` counts from the top of the window here (from the bottom in OpenGL), and a picture read
// with plain `texture(...)` (not `texture2D`, `flixel_texture2D` or the usual `#define texture flixel_texture2D`)
// comes without its colours multiplied by its opacity.
pub fn rewrite(source: &str) -> Result<(String, Vec<(String, Vec<f64>)>), String> {
    let source = without_comments(source);
    let (mut block, mut pictures, mut flags, mut defaults) = (String::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut body, mut hoisted, mut depth, mut entry) = (String::new(), String::new(), 0usize, "main".to_owned());
    // statement by statement: up to each `;`, `{` or `}` outside brackets, and each line that starts with `#`
    let mut rest = source.as_str();
    while !rest.is_empty() {
        let trimmed = rest.trim_start();
        let lead = &rest[..rest.len() - trimmed.len()];
        if trimmed.starts_with('#') {
            // (to the end of its line, or of the last line it goes on to with a `\`)
            let mut length = 0;
            for line in trimmed.split_inclusive('\n') {
                length += line.len();
                if !line.trim_end().ends_with('\\') {
                    break;
                }
            }
            let line = trimmed[..length].trim_end();
            let words: Vec<&str> = line[1..].split_whitespace().collect();
            match words.as_slice() {
                ["version", ..] | ["pragma", "header", ..] | ["extension", ..] => {}
                // (the boilerplate of ported shaders: `#define mainImage main`)
                ["define", name, "main"] => {
                    entry = (*name).to_owned();
                    body.push_str(&format!("{lead}{line}"));
                }
                _ => body.push_str(&format!("{lead}{line}")),
            }
            rest = &trimmed[line.len()..];
            continue;
        }
        let end = trimmed.find([';', '{', '}']).map_or(trimmed.len(), |at| at + 1);
        let statement = &trimmed[..end];
        rest = &trimmed[end..];
        let words: Vec<&str> = statement.trim_end_matches([';', '{', '}']).split_whitespace().collect();
        let closing = statement.ends_with('}');
        match words.as_slice() {
            ["precision", ..] if depth == 0 => {}
            ["uniform", declared @ ..] if depth == 0 && statement.ends_with(';') => {
                // (a precision before the type says nothing here)
                let declared = if matches!(declared.first(), Some(&"lowp" | &"mediump" | &"highp")) { &declared[1..] } else { declared };
                let Some((kind, names)) = declared.split_first() else { return Err(format!("a uniform without a type: {statement}")) };
                for declared in split_outside_brackets(&names.join(" ")) {
                    let (name, value) = declared.split_once('=').map_or((declared.trim(), None), |(name, value)| (name.trim(), Some(value.trim())));
                    if name.contains('[') {
                        return Err(format!("arrays of uniforms are not taken here: {name}"));
                    }
                    match *kind {
                        // (the header's own are there already)
                        _ if matches!(name, "bitmap" | "openfl_TextureSize" | "hasTransform" | "hasColorTransform" | "openfl_HasColorTransform") => {}
                        "sampler2D" => pictures.push(name.to_owned()),
                        "bool" => {
                            block.push_str(&format!("    int funkin_b_{name};\n"));
                            flags.push(name.to_owned());
                        }
                        kind if TYPES.contains(&kind) => block.push_str(&format!("    {kind} {name};\n")),
                        kind => return Err(format!("a uniform of a kind not taken here: {kind} {name}")),
                    }
                    // the value the file gives it: true or false, or the numbers in it
                    let numbers = match value {
                        Some("true") => vec![1.0],
                        Some(value) => value.split_once('(').map_or(value, |(_, inside)| inside).split(|letter: char| !(letter.is_ascii_digit() || matches!(letter, '.' | '-' | 'e'))).filter_map(|number| number.parse::<f64>().ok()).collect(),
                        None => Vec::new(),
                    };
                    if !numbers.is_empty() {
                        defaults.push((name.to_owned(), numbers));
                    }
                }
            }
            // what the header declares, said again by a shader without `#pragma header`
            ["varying", .., name] if depth == 0 && statement.ends_with(';') => {
                if !matches!(*name, "openfl_TextureCoordv" | "openfl_Alphav" | "openfl_ColorMultiplierv" | "openfl_ColorOffsetv") {
                    return Err(format!("a varying of the shader's own is not taken here: {name}"));
                }
            }
            // a global given a value outside any function (one name, not a constant): declared here, given its value as
            // `main` begins
            [kind, ..] if depth == 0 && statement.ends_with(';') && TYPES.contains(kind) && global_with_value(statement).is_some() => {
                let (name, value) = global_with_value(statement).unwrap_or_default();
                body.push_str(&format!("{lead}{kind} {name};"));
                hoisted.push_str(&format!(" {name} = {value};"));
            }
            _ => {
                body.push_str(lead);
                body.push_str(statement);
                // the function the picture is made in begins: the globals get their values first
                let starts_entry = depth == 0 && statement.ends_with('{') && words.first() == Some(&"void") && words.get(1).is_some_and(|name| name.split('(').next() == Some(entry.as_str()));
                if starts_entry {
                    body.push_str("\u{1}");
                }
            }
        }
        if statement.ends_with('{') {
            depth += 1;
        } else if closing {
            depth = depth.saturating_sub(1);
        }
    }
    if !hoisted.is_empty() && !body.contains('\u{1}') {
        return Err(format!("globals are given values outside any function, and there is no `void {entry}()` to give them in"));
    }
    let body = body.replace('\u{1}', &hoisted);
    let mut text = String::from("#version 450\nlayout(location = 0) in vec2 openfl_TextureCoordv;\nlayout(location = 1) in vec4 funkin_color;\nlayout(location = 0) out vec4 funkin_FragColor;\n");
    text.push_str("layout(set = 0, binding = 0) uniform texture2D funkin_bitmap_t;\nlayout(set = 0, binding = 1) uniform sampler funkin_bitmap_s;\n");
    for (at, name) in pictures.iter().enumerate() {
        text.push_str(&format!("layout(set = 1, binding = {}) uniform texture2D funkin_t_{name};\nlayout(set = 1, binding = {}) uniform sampler funkin_s_{name};\n", 1 + 2 * at, 2 + 2 * at));
    }
    text.push_str(&format!("layout(set = 1, binding = 0) uniform Funkin {{\n    vec2 openfl_TextureSize;\n{block}}};\n"));
    for name in &pictures {
        text.push_str(&format!("#define {name} sampler2D(funkin_t_{name}, funkin_s_{name})\n"));
    }
    for name in &flags {
        text.push_str(&format!("#define {name} (funkin_b_{name} != 0)\n"));
    }
    text.push_str(HEADER);
    text.push_str(&body);
    text.push('\n');
    defaults.extend(pictures.into_iter().map(|name| (format!("\u{1}{name}"), Vec::new())));
    Ok((text, defaults))
}

/// The name and the value of a statement `<type> <name> = <value>;` that declares one global; none for another shape.
fn global_with_value(statement: &str) -> Option<(&str, &str)> {
    let declared = statement.trim_end_matches(';').split_once(char::is_whitespace)?.1;
    let (name, value) = declared.split_once('=')?;
    let name = name.trim();
    let plain = !name.is_empty() && name.chars().all(|letter| letter.is_ascii_alphanumeric() || letter == '_');
    (plain && split_outside_brackets(value).len() == 1).then_some((name, value.trim()))
}

/// The parts of a list between its commas, those inside brackets not counted (`a = vec2(1.0, 2.0), b`).
fn split_outside_brackets(list: &str) -> Vec<&str> {
    let (mut parts, mut depth, mut from) = (Vec::new(), 0i32, 0);
    for (at, letter) in list.char_indices() {
        match letter {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&list[from..at]);
                from = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&list[from..]);
    parts
}

/// The text without its comments (a uniform commented out is none).
fn without_comments(source: &str) -> String {
    let mut clean = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find("/") {
        clean.push_str(&rest[..at]);
        rest = &rest[at..];
        if rest.starts_with("//") {
            rest = &rest[rest.find('\n').unwrap_or(rest.len())..];
        } else if rest.starts_with("/*") {
            rest = &rest[rest.find("*/").map_or(rest.len(), |end| end + 2)..];
            clean.push(' ');
        } else {
            clean.push('/');
            rest = &rest[1..];
        }
    }
    clean.push_str(rest);
    clean
}

/// Reads a fragment shader written for OpenFL: rewritten, checked, its uniforms listed. An error says what the shader
/// has that is not taken here, or where it is wrong.
pub fn translate(source: &str) -> Result<Translated, String> {
    let (text, mut defaults) = rewrite(source)?;
    // (a mod's file must not end the game: should the reader give up on one, that is an error like any other)
    let parsed = std::panic::catch_unwind(|| naga::front::glsl::Frontend::default().parse(&naga::front::glsl::Options::from(naga::ShaderStage::Fragment), &text));
    let module = parsed.map_err(|_| "the shader could not be read".to_owned())?.map_err(|errors| {
        // with the line of the rewritten text each error is at, and that line
        errors.errors.iter().map(|error| {
            let line = error.meta.location(&text).line_number as usize;
            format!("{} (\"{}\")", error.kind, text.lines().nth(line.saturating_sub(1)).unwrap_or("").trim())
        }).collect::<Vec<_>>().join("; ")
    })?;
    let block = module.types.iter().find(|(_, kind)| kind.name.as_deref() == Some("Funkin")).map(|(_, kind)| &kind.inner);
    let Some(naga::TypeInner::Struct { members, span }) = block else { return Err("the shader's uniforms were not found".to_owned()) };
    let slots = members.iter().filter_map(|member| {
        let name = member.name.clone()?;
        let kind = match module.types[member.ty].inner {
            naga::TypeInner::Scalar(scalar) if scalar.kind == naga::ScalarKind::Float => Kind::Float(1),
            naga::TypeInner::Scalar(_) if name.starts_with("funkin_b_") => Kind::Bool,
            naga::TypeInner::Scalar(_) => Kind::Int(1),
            naga::TypeInner::Vector { size, scalar } if scalar.kind == naga::ScalarKind::Float => Kind::Float(size as usize),
            naga::TypeInner::Vector { size, .. } => Kind::Int(size as usize),
            naga::TypeInner::Matrix { columns, rows, .. } if columns as usize == 2 && rows as usize == 2 => Kind::Float(4),
            // shortcut: larger matrices have their columns padded; scripts cannot set them yet
            _ => return None,
        };
        Some(Slot { name: name.trim_start_matches("funkin_b_").to_owned(), kind, offset: member.offset as usize })
    }).collect();
    let pictures = defaults.iter().filter_map(|(name, _)| name.strip_prefix('\u{1}').map(str::to_owned)).collect();
    defaults.retain(|(name, _)| !name.starts_with('\u{1}'));
    let size = (*span as usize).next_multiple_of(16);
    Ok(Translated { module, slots, size, pictures, defaults })
}

/// Writes `numbers` where a uniform of `kind` lies in the uniforms' bytes (as many as it holds; the rest stay).
pub fn write(bytes: &mut [u8], slot: &Slot, numbers: &[f64]) {
    let count = match slot.kind { Kind::Float(count) | Kind::Int(count) => count, Kind::Bool => 1 };
    for (at, number) in numbers.iter().take(count).enumerate() {
        let four = match slot.kind {
            Kind::Float(_) => (*number as f32).to_le_bytes(),
            Kind::Int(_) => (*number as i32).to_le_bytes(),
            Kind::Bool => ((*number != 0.0) as i32).to_le_bytes(),
        };
        if let Some(place) = bytes.get_mut(slot.offset + at * 4..slot.offset + at * 4 + 4) {
            place.copy_from_slice(&four);
        }
    }
}

/// The numbers a uniform holds.
pub fn read(bytes: &[u8], slot: &Slot) -> Vec<f64> {
    let count = match slot.kind { Kind::Float(count) | Kind::Int(count) => count, Kind::Bool => 1 };
    (0..count).filter_map(|at| {
        let four: [u8; 4] = bytes.get(slot.offset + at * 4..slot.offset + at * 4 + 4)?.try_into().ok()?;
        Some(match slot.kind { Kind::Float(_) => f32::from_le_bytes(four) as f64, _ => i32::from_le_bytes(four) as f64 })
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shader_written_for_openfl_is_read_with_its_uniforms() {
        // the shape most ported shaders have: globals given values outside functions, `texture` and `mainImage` renamed
        let ported = "#pragma header
            vec2 uv = openfl_TextureCoordv.xy;
            vec2 iResolution = openfl_TextureSize;
            uniform float iTime; // seconds
            uniform vec3 tint = vec3(1.0, 0.5, 0.25);
            uniform bool on;
            /* uniform float gone; */
            uniform sampler2D noise;
            #define iChannel0 bitmap
            #define texture flixel_texture2D
            #define fragColor gl_FragColor
            #define mainImage main
            #ifdef GL_ES
            precision mediump float;
            #endif
            float wave(float y) { return sin(y * 10.0 + iTime) / iResolution.x; }
            void mainImage() {
                vec4 color = texture(iChannel0, uv + vec2(wave(uv.y), 0.0));
                if (on) color.rgb *= tint;
                fragColor = color + texture2D(noise, uv) * 0.1;
            }";
        let read = translate(ported).unwrap_or_else(|error| panic!("{error}"));
        let slot = |name: &str| read.slots.iter().find(|slot| slot.name == name).map(|slot| (slot.kind, slot.offset));
        assert_eq!((slot("openfl_TextureSize"), slot("iTime"), slot("tint"), slot("on")), (Some((Kind::Float(2), 0)), Some((Kind::Float(1), 8)), Some((Kind::Float(3), 16)), Some((Kind::Bool, 28))));
        assert_eq!((slot("gone"), read.size, read.pictures.as_slice()), (None, 32, &["noise".to_owned()][..]));
        assert_eq!(read.defaults, [("tint".to_owned(), vec![1.0, 0.5, 0.25])]);
        // the plain shape, and numbers put in and taken out again
        let plain = translate("#pragma header\nuniform float amount;\nvoid main() { gl_FragColor = flixel_texture2D(bitmap, openfl_TextureCoordv) * amount; }").unwrap();
        let mut bytes = vec![0; plain.size];
        write(&mut bytes, &plain.slots[1], &[0.75, 9.0]);
        assert_eq!((plain.slots[1].name.as_str(), super::read(&bytes, &plain.slots[1])), ("amount", vec![0.75]));
        // other shapes mods come in: no spaces, a line going on, no header, the function of a ported one called by main
        for shape in [
            "#pragma header\nvec2 uv=openfl_TextureCoordv.xy;\nvoid main() { gl_FragColor = flixel_texture2D(bitmap, uv); }",
            "#pragma header\n#define LONG(a, b) \\\n    ((a) + \\\n     (b))\nprecision highp float;\nuniform float a, b;\nvoid main() { for (int i = 0; i < 4; i++) { gl_FragColor += flixel_texture2D(bitmap, openfl_TextureCoordv) * LONG(a, b); } }",
            "varying vec2 openfl_TextureCoordv;\nuniform sampler2D bitmap;\nuniform vec2 openfl_TextureSize;\nvoid main() { float x = 1; gl_FragColor = texture2D(bitmap, openfl_TextureCoordv) * x; }",
            "#pragma header\nuniform float iTime;\nvoid mainImage(out vec4 fragColor, in vec2 fragCoord) { fragColor = flixel_texture2D(bitmap, fragCoord / openfl_TextureSize) * iTime; }\nvoid main() { mainImage(gl_FragColor, openfl_TextureCoordv * openfl_TextureSize); }",
        ] {
            translate(shape).unwrap_or_else(|error| panic!("{error}\n{shape}"));
        }
        assert!(translate("#pragma header\nvec2 uv = openfl_TextureCoordv;\nvoid other() {}").is_err_and(|error| error.contains("no `void main()`")));
        assert!(translate("varying vec2 mine;\nvoid main() {}").is_err_and(|error| error.contains("varying")));
        // what is not taken, or wrong, is said
        assert!(translate("uniform float list[4];\nvoid main() {}").is_err_and(|error| error.contains("arrays")));
        assert!(translate("#pragma header\nvoid main() { gl_FragColor = nothing; }").is_err_and(|error| error.contains("nothing")));
    }
}
