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

/// A uniform scripts may set: its name, kind and place in the uniforms' bytes. A list (`float name[4]`) or a matrix
/// (a list of its columns) has `count` of that kind, each `stride` bytes after the last.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub name: String,
    pub kind: Kind,
    pub offset: usize,
    pub count: usize,
    pub stride: usize,
}

/// A shader read: ready for the graphics card, with its uniforms, how many bytes they take, the pictures it asks for
/// besides the sprite's own (its `sampler2D` uniforms, in the order of their bindings), and the values the file gives.
pub struct Translated {
    pub module: naga::Module,
    /// The shader's own vertex part, when it has one.
    pub vertex: Option<naga::Module>,
    pub slots: Vec<Slot>,
    pub size: usize,
    pub pictures: Vec<String>,
    pub defaults: Vec<(String, Vec<f64>)>,
}

/// What OpenFL and Flixel put where a shader says `#pragma header`, as this engine has it: the sprite's picture
/// (`bitmap`), its size, the place in it, and the sprite's colour and opacity. Pictures come out as OpenFL's do, their
/// colours already multiplied by their opacity, and so does what the shader answers.
const HEADER: &str = "vec4 funkin_flipped(vec4 c) { return vec4(c.x, funkin_target.y - c.y, c.z, c.w); }
#define gl_FragCoord funkin_flipped(gl_FragCoord)
#define bitmap sampler2D(funkin_bitmap_t, funkin_bitmap_s)
#define gl_FragColor funkin_FragColor
#define openfl_Alphav funkin_color.a
#define openfl_ColorMultiplierv funkin_color
#define openfl_ColorOffsetv vec4(0.0)
#define openfl_HasColorTransform false
#define hasTransform true
#define hasColorTransform false
vec4 funkin_pre(vec4 c) { return vec4(c.rgb * mix(c.a, 1.0, funkin_ready), c.a); }
vec4 funkin_tinted(vec4 c) { return vec4(c.rgb * funkin_color.rgb * mix(c.a, 1.0, funkin_ready), c.a) * funkin_color.a; }
#define texture2D(funkin_t, funkin_c) funkin_pre(textureLod(funkin_t, funkin_c, 0.0))
#define flixel_texture2D(funkin_t, funkin_c) funkin_tinted(textureLod(funkin_t, funkin_c, 0.0))
#define texture(funkin_t, funkin_c) funkin_pre(textureLod(funkin_t, funkin_c, 0.0))
";

const TYPES: [&str; 12] = ["float", "int", "bool", "vec2", "vec3", "vec4", "ivec2", "ivec3", "ivec4", "mat2", "mat3", "mat4"];

/// `texture(sampler, uv, bias)`, also `texture2D` and `flixel_texture2D`, as GLSL and OpenFL take it: the bias (a shift of the
/// mip level) is left out, as every read here is of the picture itself (the macros of the header take two arguments).
fn without_bias(source: &str) -> String {
    let (mut out, mut rest) = (String::new(), source);
    while let Some(at) = ["texture(", "texture2D(", "flixel_texture2D("].iter().filter_map(|name| rest.find(name).map(|at| (at, name.len()))).min() {
        let (start, name_len) = at;
        let (name, after) = (&rest[start..start + name_len], &rest[start + name_len..]);
        // (a longer name ending in these, like `mytexture(`, is not the call)
        if rest[..start].chars().next_back().is_some_and(|before| before.is_alphanumeric() || before == '_') {
            out.push_str(&rest[..start + name_len]);
            rest = after;
            continue;
        }
        out.push_str(&rest[..start]);
        // the arguments: up to the bracket that closes this one, split at commas outside brackets
        let (mut depth, mut parts, mut from, mut close) = (0, Vec::new(), 0, None);
        for (index, letter) in after.char_indices() {
            match letter {
                '(' | '[' => depth += 1,
                ')' | ']' if depth == 0 => { parts.push(&after[from..index]); close = Some(index); break }
                ')' | ']' => depth -= 1,
                ',' if depth == 0 => { parts.push(&after[from..index]); from = index + 1 }
                _ => {}
            }
        }
        let Some(close) = close else { out.push_str(&rest[start..]); return out };
        out.push_str(name);
        out.push_str(&parts.iter().take(2).map(|part| without_bias(part)).collect::<Vec<_>>().join(","));
        out.push(')');
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Takes apart one part of a shader written for OpenFL (its fragment shader, or `vertex`): its uniforms, pictures and
/// varyings set aside, and the values it gives its globals outside any function (which only some drivers take) given
/// at the start of `main` instead.
fn gather(source: &str, vertex: bool) -> Result<Gathered, String> {
    let source = without_bias(&without_comments(source));
    let (mut block, mut pictures, mut defaults, mut varyings) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
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
                // what OpenFL's vertex shader does where one says `#pragma body`
                ["pragma", "body", ..] => body.push_str(&format!("{lead}{VERTEX_BODY}")),
                // (the boilerplate of ported shaders: `#define mainImage main`)
                // (a shader's own `#define texture ...` takes the place of the header's)
                ["define", name, ..] if *name == "texture" || name.starts_with("texture(") => body.push_str(&format!("{lead}#undef texture\n{line}")),
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
                    // a list: so many of them, as a number or as a `#define` of the file gives it
                    let (name, length) = match name.split_once('[') {
                        Some((name, length)) => {
                            let length = length.trim_end_matches(']').trim();
                            let defined = source.lines().find_map(|line| line.trim().strip_prefix("#define")?.trim().strip_prefix(length)?.trim().parse::<usize>().ok());
                            let Some(length) = length.parse::<usize>().ok().or(defined) else { return Err(format!("how long the list {name} is cannot be told: {length}")) };
                            (name.trim(), format!("[{length}]"))
                        }
                        None => (name, String::new()),
                    };
                    match *kind {
                        // (the header's own are there already)
                        _ if matches!(name, "bitmap" | "openfl_TextureSize" | "funkin_target" | "hasTransform" | "hasColorTransform" | "openfl_HasColorTransform") => {}
                        _ if vertex && matches!(name, "openfl_Matrix") => {}
                        "sampler2D" if length.is_empty() && !vertex => pictures.push(name.to_owned()),
                        "bool" if length.is_empty() => {
                            block.push((name.to_owned(), format!("    int funkin_b_{name};\n")));
                        }
                        // (a matrix of two by two has no place of its own among the uniforms: its four numbers do)
                        "mat2" if length.is_empty() => {
                            block.push((name.to_owned(), format!("    vec4 funkin_m_{name};\n")));
                        }
                        kind if TYPES.contains(&kind) && !matches!(kind, "bool" | "mat2") => block.push((name.to_owned(), format!("    {kind} {name}{length};\n"))),
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
            // what the header declares, said again by a shader without `#pragma header`; a varying of the shader's own
            // goes from its vertex shader to its fragment shader
            ["varying", .., kind, name] if depth == 0 && statement.ends_with(';') => {
                if !matches!(*name, "openfl_TextureCoordv" | "openfl_Alphav" | "openfl_ColorMultiplierv" | "openfl_ColorOffsetv") {
                    if !TYPES.contains(kind) {
                        return Err(format!("a varying of a kind not taken here: {kind} {name}"));
                    }
                    varyings.push(((*kind).to_owned(), (*name).to_owned()));
                }
            }
            ["attribute", .., name] if depth == 0 && vertex && statement.ends_with(';') => {
                if !matches!(*name, "openfl_Position" | "openfl_TextureCoord" | "openfl_Alpha" | "openfl_ColorMultiplier" | "openfl_ColorOffset") {
                    return Err(format!("an attribute of the shader's own is not taken here: {name}"));
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
    // (a vertex shader's `main` begins by handing on what every corner carries)
    if vertex {
        hoisted.insert_str(0, VERTEX_START);
    }
    if !hoisted.is_empty() && !body.contains('\u{1}') {
        return Err(format!("there is no `void {entry}()`{}", if vertex { "" } else { " to give the globals their values in" }));
    }
    Ok(Gathered { block, pictures, defaults, varyings, body: body.replace('\u{1}', &hoisted) })
}

/// What a shader's text holds, taken apart: its uniforms (name, and how the block declares it), the pictures it asks
/// for, the values the file gives, its own varyings (kind and name), and the rest of its text.
struct Gathered {
    block: Vec<(String, String)>,
    pictures: Vec<String>,
    defaults: Vec<(String, Vec<f64>)>,
    varyings: Vec<(String, String)>,
    body: String,
}

/// What a vertex shader's `main` starts with, and what stands for OpenFL's `#pragma body`: the corner as the game made
/// it (its place on screen already worked out).
const VERTEX_START: &str = " funkin_color = funkin_vcolor; funkin_ready = funkin_vready;";
const VERTEX_BODY: &str = "openfl_TextureCoordv = openfl_TextureCoord; gl_Position = vec4(funkin_position, 0.0, 1.0);";

/// What stands where a vertex shader says `#pragma header`: the corner where the sprite put it, in the world's pixels
/// (before the camera), and the matrix of the camera's way to the screen, so `openfl_Matrix * openfl_Position` is where
/// the corner is.
// As in Flixel, `openfl_Position` has the camera's scroll taken off (the scroll is where the view's left/top edge, -1/+1 on
// screen, lies in the world: worked back from `funkin_view`), so `openfl_Matrix` only scales and moves the view's corner to -1/+1.
const VERTEX_HEADER: &str = "#define openfl_Position vec4(funkin_world - vec2((-1.0 - funkin_view.y) / funkin_view.x, (1.0 - funkin_view.w) / funkin_view.z), 0.0, 1.0)
#define openfl_Matrix mat4(funkin_view.x, 0.0, 0.0, 0.0, 0.0, funkin_view.z, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 1.0, 0.0, 1.0)
#define openfl_Alpha funkin_vcolor.a
#define openfl_ColorMultiplier funkin_vcolor
#define openfl_ColorOffset vec4(0.0)
#define openfl_Alphav funkin_color.a
#define openfl_ColorMultiplierv funkin_color
#define openfl_HasColorTransform false
vec4 openfl_ColorOffsetv;
";

/// The text of one of a shader's two parts for today's GLSL: what goes in and out, the uniforms both parts share (in
/// one block, in one order), the header's names, then the shader's own text.
fn text_of(vertex: bool, block: &[(String, String)], varyings: &[(String, String)], part: &Gathered) -> String {
    let mut text = String::from("#version 450\n");
    text.push_str(if vertex {
        "layout(location = 0) in vec2 funkin_position;\nlayout(location = 1) in vec2 openfl_TextureCoord;\nlayout(location = 2) in vec4 funkin_vcolor;\nlayout(location = 6) in float funkin_vready;\nlayout(location = 8) in vec2 funkin_world;\nlayout(location = 9) in vec4 funkin_view;\nlayout(location = 0) out vec2 openfl_TextureCoordv;\nlayout(location = 1) out vec4 funkin_color;\nlayout(location = 5) out float funkin_ready;\n"
    } else {
        "layout(location = 0) in vec2 openfl_TextureCoordv;\nlayout(location = 1) in vec4 funkin_color;\nlayout(location = 5) in float funkin_ready;\nlayout(location = 0) out vec4 funkin_FragColor;\nlayout(set = 0, binding = 0) uniform texture2D funkin_bitmap_t;\nlayout(set = 0, binding = 1) uniform sampler funkin_bitmap_s;\n"
    });
    // (the game's own corners hand on seven things: the shader's own varyings come after them)
    for (at, (kind, name)) in varyings.iter().enumerate() {
        text.push_str(&format!("layout(location = {}) {} {kind} {name};\n", 7 + at, if vertex { "out" } else { "in" }));
    }
    for (at, name) in part.pictures.iter().enumerate() {
        text.push_str(&format!("layout(set = 1, binding = {}) uniform texture2D funkin_t_{name};\nlayout(set = 1, binding = {}) uniform sampler funkin_s_{name};\n", 1 + 2 * at, 2 + 2 * at));
    }
    let members: String = block.iter().map(|(_, line)| line.as_str()).collect();
    text.push_str(&format!("layout(set = 1, binding = 0) uniform Funkin {{\n    vec2 openfl_TextureSize;\n    vec2 funkin_target;\n{members}}};\n"));
    for name in &part.pictures {
        text.push_str(&format!("#define {name} sampler2D(funkin_t_{name}, funkin_s_{name})\n"));
    }
    // (the uniforms that are true or false, and the small matrices, by the names the shader knows them by)
    for (name, line) in block {
        if line.contains(&format!("funkin_b_{name};")) {
            text.push_str(&format!("#define {name} (funkin_b_{name} != 0)\n"));
        } else if line.contains(&format!("funkin_m_{name};")) {
            text.push_str(&format!("#define {name} mat2(funkin_m_{name}.xy, funkin_m_{name}.zw)\n"));
        }
    }
    text.push_str(if vertex { VERTEX_HEADER } else { HEADER });
    text.push_str(&part.body);
    text.push('\n');
    text
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
#[cfg(test)]
pub fn translate(source: &str) -> Result<Translated, String> {
    translate_pair(Some(source), None)
}

/// Reads a shader's two parts, either of which may be left to the usual one: the fragment shader (`.frag`) and the
/// vertex shader (`.vert`).
pub fn translate_pair(fragment: Option<&str>, vertex: Option<&str>) -> Result<Translated, String> {
    const PLAIN: &str = "#pragma header\nvoid main() { gl_FragColor = flixel_texture2D(bitmap, openfl_TextureCoordv); }";
    let fragment = gather(fragment.unwrap_or(PLAIN), false)?;
    let vertex = vertex.map(|source| gather(source, true)).transpose()?;
    // both parts share the uniforms (the fragment shader's first, then those only the vertex shader has) and the
    // shader's own varyings, which go from the one to the other in the order of their names
    let (mut block, mut varyings, mut defaults) = (fragment.block.clone(), fragment.varyings.clone(), fragment.defaults.clone());
    if let Some(part) = &vertex {
        block.extend(part.block.iter().filter(|(name, _)| !fragment.block.iter().any(|(known, _)| known == name)).cloned());
        varyings.extend(part.varyings.iter().cloned());
        defaults.extend(part.defaults.iter().cloned());
    }
    varyings.sort();
    varyings.dedup();
    if vertex.is_none() && !varyings.is_empty() {
        return Err(format!("the varying {} needs the shader's vertex part (a .vert file)", varyings[0].1));
    }
    let read = |text: String, stage: naga::ShaderStage| -> Result<naga::Module, String> {
        // (a mod's file must not end the game: should the reader give up on one, that is an error like any other)
        let parsed = std::panic::catch_unwind(|| naga::front::glsl::Frontend::default().parse(&naga::front::glsl::Options::from(stage), &text));
        let module = parsed.map_err(|_| "the shader could not be read".to_owned())?.map_err(|errors| {
            // with the line of the rewritten text each error is at, and that line
            errors.errors.iter().map(|error| {
                let line = error.meta.location(&text).line_number as usize;
                format!("{} (\"{}\")", error.kind, text.lines().nth(line.saturating_sub(1)).unwrap_or("").trim())
            }).collect::<Vec<_>>().join("; ")
        })?;
        // (checked as the graphics card's side will check it, so that what is wrong is said here)
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).map_err(|error| error.to_string())?;
        Ok(module)
    };
    let module = read(text_of(false, &block, &varyings, &fragment), naga::ShaderStage::Fragment)?;
    let corners = vertex.as_ref().map(|part| read(text_of(true, &block, &varyings, part), naga::ShaderStage::Vertex)).transpose()?;
    let block = module.types.iter().find(|(_, kind)| kind.name.as_deref() == Some("Funkin")).map(|(_, kind)| &kind.inner);
    let Some(naga::TypeInner::Struct { members, span }) = block else { return Err("the shader's uniforms were not found".to_owned()) };
    let slots = members.iter().filter_map(|member| {
        let name = member.name.clone()?;
        let plain = |inner: &naga::TypeInner| match *inner {
            naga::TypeInner::Scalar(scalar) if scalar.kind == naga::ScalarKind::Float => Some(Kind::Float(1)),
            naga::TypeInner::Scalar(_) if name.starts_with("funkin_b_") => Some(Kind::Bool),
            naga::TypeInner::Scalar(_) => Some(Kind::Int(1)),
            naga::TypeInner::Vector { size, scalar } if scalar.kind == naga::ScalarKind::Float => Some(Kind::Float(size as usize)),
            naga::TypeInner::Vector { size, .. } => Some(Kind::Int(size as usize)),
            _ => None,
        };
        let (kind, count, stride) = match module.types[member.ty].inner {
            // a list of numbers or vectors: each in a place of its own, sixteen bytes on
            naga::TypeInner::Array { base, size: naga::ArraySize::Constant(count), stride } => (plain(&module.types[base].inner)?, count.get() as usize, stride as usize),
            // a matrix: its columns, one after the other
            naga::TypeInner::Matrix { columns, rows, .. } => (Kind::Float(rows as usize), columns as usize, 16),
            ref inner => (plain(inner)?, 1, 0),
        };
        Some(Slot { name: name.trim_start_matches("funkin_b_").trim_start_matches("funkin_m_").to_owned(), kind, offset: member.offset as usize, count, stride })
    }).collect();
    let size = (*span as usize).next_multiple_of(16);
    Ok(Translated { module, vertex: corners, slots, size, pictures: fragment.pictures, defaults })
}

/// Where the numbers of a uniform lie in the uniforms' bytes, in the order scripts give them (a list's or a matrix's
/// one element after the other).
fn places(slot: &Slot) -> impl Iterator<Item = usize> + '_ {
    let each = match slot.kind { Kind::Float(each) | Kind::Int(each) => each, Kind::Bool => 1 };
    (0..slot.count).flat_map(move |element| (0..each).map(move |part| slot.offset + element * slot.stride + part * 4))
}

/// Writes `numbers` where a uniform lies in the uniforms' bytes (as many as it holds; the rest stay).
pub fn write(bytes: &mut [u8], slot: &Slot, numbers: &[f64]) {
    for (at, number) in places(slot).zip(numbers) {
        let four = match slot.kind {
            Kind::Float(_) => (*number as f32).to_le_bytes(),
            Kind::Int(_) => (*number as i32).to_le_bytes(),
            Kind::Bool => ((*number != 0.0) as i32).to_le_bytes(),
        };
        if let Some(place) = bytes.get_mut(at..at + 4) {
            place.copy_from_slice(&four);
        }
    }
}

/// The numbers a uniform holds.
pub fn read(bytes: &[u8], slot: &Slot) -> Vec<f64> {
    places(slot).filter_map(|at| {
        let four: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
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
        assert_eq!((slot("openfl_TextureSize"), slot("iTime"), slot("tint"), slot("on")), (Some((Kind::Float(2), 0)), Some((Kind::Float(1), 16)), Some((Kind::Float(3), 32)), Some((Kind::Bool, 44))));
        assert_eq!((slot("gone"), read.size, read.pictures.as_slice()), (None, 48, &["noise".to_owned()][..]));
        assert_eq!(read.defaults, [("tint".to_owned(), vec![1.0, 0.5, 0.25])]);
        // the plain shape, and numbers put in and taken out again
        let plain = translate("#pragma header\nuniform float amount;\nvoid main() { gl_FragColor = flixel_texture2D(bitmap, openfl_TextureCoordv) * amount; }").unwrap();
        let mut bytes = vec![0; plain.size];
        write(&mut bytes, &plain.slots[2], &[0.75, 9.0]);
        assert_eq!((plain.slots[2].name.as_str(), super::read(&bytes, &plain.slots[2])), ("amount", vec![0.75]));
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
        // gl_FragCoord counts from the bottom of the target, as in OpenGL: the translation flips y with the target's height
        let coord = translate("#pragma header\nvoid main() { gl_FragColor = vec4(gl_FragCoord.x / 100.0, gl_FragCoord.y / 100.0, 0.0, 1.0); }").unwrap_or_else(|error| panic!("{error}"));
        assert!(text_of(false, &[], &[], &gather("#pragma header\nvoid main() { gl_FragColor = vec4(gl_FragCoord.y); }", false).unwrap()).contains("funkin_target.y - c.y"));
        assert!(coord.slots.iter().any(|slot| slot.name == "funkin_target" && slot.offset == 8));
        // a vertex shader that moves each corner by 100 pixels through openfl_Matrix and openfl_Position
        let moved = translate_pair(None, Some("#pragma header\nvoid main() {\n#pragma body\ngl_Position = openfl_Matrix * (openfl_Position + vec4(100.0, 0.0, 0.0, 0.0));\n}")).unwrap_or_else(|error| panic!("{error}"));
        assert!(moved.vertex.is_some());
        // the three-argument form: the bias is left out (also nested, and in the other spellings)
        assert_eq!(without_bias("a = texture(bitmap, uv, -1.0) + texture2D(t, f(uv, 2.0), 0.5) + texture(t, uv) + mytexture(a, b, c);"), "a = texture(bitmap, uv) + texture2D(t, f(uv, 2.0)) + texture(t, uv) + mytexture(a, b, c);");
        translate("#pragma header\nvoid main() { gl_FragColor = texture(bitmap, openfl_TextureCoordv, -0.5); }").unwrap_or_else(|error| panic!("{error}"));
        // the scroll comes off the position and the matrix puts the view's corner at (-1, 1): same screen place as the built-in path
        assert!(VERTEX_HEADER.contains("(-1.0 - funkin_view.y) / funkin_view.x") && VERTEX_HEADER.contains("-1.0, 1.0, 0.0, 1.0)"));
        let (view, world) = ([0.002f32, -0.5, -0.004, 1.0], [300.0f32, 100.0]);
        let scroll = [(-1.0 - view[1]) / view[0], (1.0 - view[3]) / view[2]];
        let screen = [view[0] * (world[0] - scroll[0]) - 1.0, view[2] * (world[1] - scroll[1]) + 1.0];
        assert!((screen[0] - (view[0] * world[0] + view[1])).abs() < 1e-5 && (screen[1] - (view[2] * world[1] + view[3])).abs() < 1e-5);
        // lists and matrices: every element in its own place
        let lists = translate("#pragma header\n#define COUNT 3\nuniform float amounts[COUNT];\nuniform vec2 points[2];\nuniform mat2 turn;\nuniform mat3 warp;\nvoid main() { gl_FragColor = flixel_texture2D(bitmap, turn * openfl_TextureCoordv + points[1]) * amounts[2] * warp[2][2]; }").unwrap_or_else(|error| panic!("{error}"));
        let slot = |name: &str| lists.slots.iter().find(|slot| slot.name == name).cloned().unwrap();
        assert_eq!((slot("amounts").count, slot("amounts").stride, slot("points").kind, slot("turn").kind, slot("warp").count), (3, 16, Kind::Float(2), Kind::Float(4), 3));
        let mut bytes = vec![0; lists.size];
        write(&mut bytes, &slot("amounts"), &[1.0, 2.0, 3.0]);
        write(&mut bytes, &slot("warp"), &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5, 0.25, 1.0]);
        assert_eq!((super::read(&bytes, &slot("amounts")), super::read(&bytes, &slot("warp"))[6..].to_vec()), (vec![1.0, 2.0, 3.0], vec![0.5, 0.25, 1.0]));
        // a shader with a vertex part of its own, and a varying from the one to the other
        let pair = translate_pair(
            Some("#pragma header\nvarying float wobble;\nuniform float iTime;\nvoid main() { gl_FragColor = flixel_texture2D(bitmap, openfl_TextureCoordv) * wobble * iTime; }"),
            Some("#pragma header\nvarying float wobble;\nuniform float amount;\nvoid main() {\n#pragma body\nwobble = sin(openfl_TextureCoord.y * amount);\ngl_Position.x += wobble * 0.1;\n}"),
        ).unwrap_or_else(|error| panic!("{error}"));
        assert!(pair.vertex.is_some() && ["iTime", "amount"].iter().all(|name| pair.slots.iter().any(|slot| slot.name == *name)));
        assert!(translate("#pragma header\nvarying float wobble;\nvoid main() { gl_FragColor = vec4(wobble); }").is_err_and(|error| error.contains(".vert")));
        // a plain `texture(...)` read is premultiplied like texture2D (what OpenFL's bitmaps hold), and a shader's own
        // `#define texture flixel_texture2D` still takes its place
        for shape in ["#pragma header\nvoid main() { gl_FragColor = texture(bitmap, openfl_TextureCoordv); }", "#pragma header\n#define texture flixel_texture2D\nvoid main() { gl_FragColor = texture(bitmap, openfl_TextureCoordv); }"] {
            translate(shape).unwrap_or_else(|error| panic!("{error}\n{shape}"));
        }
        assert!(text_of(false, &[], &[], &gather("#pragma header\n#define texture flixel_texture2D\nvoid main() {}", false).unwrap()).contains("#undef texture\n#define texture flixel_texture2D"));
        // what is not taken, or wrong, is said
        assert!(translate("uniform float list[SOME];\nvoid main() {}").is_err_and(|error| error.contains("how long")));
        assert!(translate("#pragma header\nvoid main() { gl_FragColor = nothing; }").is_err_and(|error| error.contains("nothing")));
    }
}
