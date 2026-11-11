//! Lua scripts, as the Haxe engine runs them (psychlua/FunkinLua.hx): each script in a Lua of its own, told about the song
//! through its globals and called back at its moments (onCreate, onBeatHit, goodNoteHit...). What a script asks of the game
//! is queued as a `Request` for the play screen to carry out; what it asks about the game comes from `Shared::props`,
//! which the play screen keeps up to date.
use mlua::{Function, Lua, MultiValue, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What a callback returns to stop what it is called for (an end of song, a pause, a game over...), as in the Haxe engine.
pub const STOP: &str = "##PSYCHLUA_FUNCTIONSTOP";
const CONTINUE: &str = "##PSYCHLUA_FUNCTIONCONTINUE";
const STOP_LUA: &str = "##PSYCHLUA_FUNCTIONSTOPLUA";
const STOP_HSCRIPT: &str = "##PSYCHLUA_FUNCTIONSTOPHSCRIPT";
const STOP_ALL: &str = "##PSYCHLUA_FUNCTIONSTOPALL";

/// Something a script asked of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    Sound { name: String, volume: f32 },
    Music { name: String, volume: f32, looped: bool },
    /// An event as a chart would have it: its name and two values.
    Event { name: String, value1: String, value2: String },
    SetHealth(f64),
    AddHealth(f64),
    /// A character (by the Haxe engine's names: "boyfriend", "dad", "gf") plays an animation, or its idle move.
    Animation { who: String, animation: String },
    Dance { who: String },
    SetProperty { name: String, value: serde_json::Value },
    EndSong,
    RestartSong,
    ExitSong,
}

/// What the scripts of a song share with each other and with the game.
#[derive(Default)]
pub struct Shared {
    pub requests: Vec<Request>,
    /// What scripts may read of the game (getProperty), as the play screen last told it.
    pub props: HashMap<String, serde_json::Value>,
    /// What scripts keep for each other (setVar, getVar).
    vars: HashMap<String, serde_json::Value>,
    /// Lines of debugPrint, for the screen, with when they were printed.
    pub printed: Vec<(String, f64)>,
    /// Timers by tag: seconds left, seconds each, loops left (0: for ever), loops done.
    timers: HashMap<String, (f64, f64, u32, u32)>,
    /// Each mod's own settings, by folder.
    mod_settings: serde_json::Map<String, serde_json::Value>,
    /// The mod the song comes from.
    mod_folder: String,
}

struct Script {
    name: String,
    lua: Lua,
    closed: Rc<RefCell<bool>>,
}

/// The scripts of a song.
pub struct Scripts {
    scripts: Vec<Script>,
    pub shared: Rc<RefCell<Shared>>,
}

/// The scripts to run for a song, in the Haxe engine's order: those of the `scripts/` folders, then the song's own
/// (`data/<song>/`), then the stage's (`stages/<stage>.lua`). `roots` are the folders looked in, the first asked first.
pub fn find(roots: &[PathBuf], song: &str, stage: &str) -> Vec<PathBuf> {
    let in_folders = |folder: &str| -> Vec<PathBuf> {
        // (the game's own folder first, the mod asked first last: the Haxe engine's directoriesWithFile)
        roots.iter().rev().map(|root| root.join(folder)).filter(|dir| dir.is_dir()).flat_map(|dir| {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ending| ending.eq_ignore_ascii_case("lua"))).collect();
            files.sort();
            files
        }).collect()
    };
    let mut found = in_folders("scripts");
    found.extend(in_folders(&format!("data/{song}")));
    found.extend(roots.iter().map(|root| root.join(format!("stages/{stage}.lua"))).find(|path| path.is_file()));
    found
}

/// A Lua value as JSON (what scripts share must outlive the Lua it came from): tables with keys 1, 2... are lists.
fn to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Boolean(flag) => serde_json::json!(flag),
        Value::Integer(number) => serde_json::json!(number),
        Value::Number(number) => serde_json::json!(number),
        Value::String(text) => serde_json::json!(text.to_string_lossy()),
        Value::Table(table) => {
            let length = table.raw_len();
            if length > 0 {
                serde_json::Value::Array((1..=length).map(|index| table.raw_get::<Value>(index).map_or(serde_json::Value::Null, |item| to_json(&item))).collect())
            } else {
                serde_json::Value::Object(table.pairs::<String, Value>().flatten().map(|(key, item)| (key, to_json(&item))).collect())
            }
        }
        _ => serde_json::Value::Null,
    }
}

/// A JSON value as Lua has it.
fn from_json(lua: &Lua, value: &serde_json::Value) -> mlua::Result<Value> {
    Ok(match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(flag) => Value::Boolean(*flag),
        serde_json::Value::Number(number) => number.as_i64().map_or(Value::Number(number.as_f64().unwrap_or(0.0)), Value::Integer),
        serde_json::Value::String(text) => Value::String(lua.create_string(text)?),
        serde_json::Value::Array(items) => Value::Table(lua.create_sequence_from(items.iter().map(|item| from_json(lua, item)).collect::<mlua::Result<Vec<_>>>()?)?),
        serde_json::Value::Object(map) => {
            let table = lua.create_table()?;
            for (key, item) in map {
                table.set(key.as_str(), from_json(lua, item)?)?;
            }
            Value::Table(table)
        }
    })
}

impl Shared {
    /// What the scripts of a mod's song start with: the mods' own settings, and which mod the song is of.
    pub fn new(mod_settings: serde_json::Map<String, serde_json::Value>, mod_folder: &str) -> Shared {
        Shared { mod_settings, mod_folder: mod_folder.to_owned(), ..Default::default() }
    }
}

impl Scripts {
    pub fn none() -> Scripts {
        Scripts { scripts: Vec::new(), shared: Rc::new(RefCell::new(Shared::default())) }
    }

    /// Starts the scripts in `files`, each with the globals in `globals`. One that cannot be read or run is left out
    /// (and said so), as the Haxe engine leaves it out.
    pub fn start(files: &[PathBuf], globals: &[(&str, serde_json::Value)], shared: Shared) -> Scripts {
        let mut scripts = Scripts { scripts: Vec::new(), shared: Rc::new(RefCell::new(shared)) };
        for file in files {
            match scripts.open(file, globals) {
                Ok(script) => scripts.scripts.push(script),
                Err(error) => eprintln!("Lua {}: {error}", file.display()),
            }
        }
        scripts
    }

    fn open(&self, file: &Path, globals: &[(&str, serde_json::Value)]) -> mlua::Result<Script> {
        let code = std::fs::read_to_string(file).map_err(mlua::Error::external)?;
        let lua = Lua::new();
        let closed = Rc::new(RefCell::new(false));
        let name = file.to_string_lossy().replace('\\', "/");
        let g = lua.globals();
        for (key, value) in [("Function_Stop", STOP), ("Function_Continue", CONTINUE), ("Function_StopLua", STOP_LUA), ("Function_StopHScript", STOP_HSCRIPT), ("Function_StopAll", STOP_ALL)] {
            g.set(key, value)?;
        }
        g.set("luaDebugMode", false)?;
        g.set("luaDeprecatedWarnings", true)?;
        g.set("scriptName", name.as_str())?;
        for (key, value) in globals {
            g.set(*key, from_json(&lua, value)?)?;
        }
        self.register(&lua, &closed, &name)?;
        lua.load(&code).set_name(name.as_str()).exec()?;
        Ok(Script { name, lua, closed })
    }

    /// The functions a script can call. Each queues what it asks for, or reads what the game last told.
    fn register(&self, lua: &Lua, closed: &Rc<RefCell<bool>>, name: &str) -> mlua::Result<()> {
        let g = lua.globals();
        let shared = &self.shared;
        macro_rules! function {
            ($name:literal, |$lua:ident, $shared:ident, $args:ident : $ty:ty| $body:expr) => {{
                let $shared = shared.clone();
                g.set($name, lua.create_function(move |$lua, $args: $ty| {
                    let _ = &$lua;
                    $body
                })?)?;
            }};
        }
        macro_rules! request {
            ($name:literal, |$args:ident : $ty:ty| $request:expr) => {
                function!($name, |_lua, shared, $args: $ty| {
                    shared.borrow_mut().requests.push($request);
                    Ok(())
                })
            };
        }
        let script = name.to_owned();
        function!("debugPrint", |_lua, shared, args: MultiValue| {
            let line = args.iter().map(|value| value.to_string().unwrap_or_default()).collect::<Vec<_>>().join(", ");
            eprintln!("{script}: {line}");
            let mut shared = shared.borrow_mut();
            shared.printed.push((line, crate::gfx::get_time()));
            // (only the last few are ever shown)
            let excess = shared.printed.len().saturating_sub(32);
            shared.printed.drain(..excess);
            Ok(())
        });
        let stop = closed.clone();
        function!("close", |_lua, _shared, _args: ()| {
            *stop.borrow_mut() = true;
            Ok(true)
        });
        request!("playSound", |args: (String, Option<f32>)| Request::Sound { name: args.0, volume: args.1.unwrap_or(1.0) });
        request!("playMusic", |args: (String, Option<f32>, Option<bool>)| Request::Music { name: args.0, volume: args.1.unwrap_or(1.0), looped: args.2.unwrap_or(false) });
        request!("triggerEvent", |args: (String, Option<Value>, Option<Value>)| {
            let text = |value: Option<Value>| value.and_then(|value| value.to_string().ok()).unwrap_or_default();
            Request::Event { name: args.0, value1: text(args.1), value2: text(args.2) }
        });
        request!("cameraShake", |args: (String, f64, f64)| {
            // as the Haxe engine's Screen Shake event takes it: "seconds, strength" for the game's camera
            let value = format!("{}, {}", args.2, args.1);
            let (value1, value2) = if args.0.to_lowercase().contains("hud") { (String::new(), value) } else { (value, String::new()) };
            Request::Event { name: "Screen Shake".to_owned(), value1, value2 }
        });
        request!("setHealth", |args: f64| Request::SetHealth(args));
        request!("addHealth", |args: f64| Request::AddHealth(args));
        request!("characterPlayAnim", |args: (String, String, Option<bool>)| Request::Animation { who: args.0, animation: args.1 });
        request!("playAnim", |args: (String, String, Option<bool>)| Request::Animation { who: args.0, animation: args.1 });
        request!("characterDance", |args: String| Request::Dance { who: args });
        request!("endSong", |_args: ()| Request::EndSong);
        request!("restartSong", |_args: Option<bool>| Request::RestartSong);
        request!("exitSong", |_args: Option<bool>| Request::ExitSong);
        request!("setProperty", |args: (String, Value)| Request::SetProperty { name: args.0, value: to_json(&args.1) });
        function!("getProperty", |lua, shared, args: String| from_json(lua, &shared.borrow().props.get(&args).cloned().unwrap_or_default()));
        function!("getHealth", |lua, shared, _args: ()| from_json(lua, &shared.borrow().props.get("health").cloned().unwrap_or_default()));
        function!("getSongPosition", |lua, shared, _args: ()| from_json(lua, &shared.borrow().props.get("songPosition").cloned().unwrap_or_default()));
        function!("setVar", |_lua, shared, args: (String, Value)| {
            shared.borrow_mut().vars.insert(args.0, to_json(&args.1));
            Ok(())
        });
        function!("getVar", |lua, shared, args: String| from_json(lua, &shared.borrow().vars.get(&args).cloned().unwrap_or_default()));
        function!("runTimer", |_lua, shared, args: (String, Option<f64>, Option<u32>)| {
            let seconds = args.1.unwrap_or(1.0);
            shared.borrow_mut().timers.insert(args.0, (seconds, seconds, args.2.unwrap_or(1), 0));
            Ok(())
        });
        function!("cancelTimer", |_lua, shared, args: String| {
            shared.borrow_mut().timers.remove(&args);
            Ok(())
        });
        function!("getModSetting", |lua, shared, args: (String, Option<String>)| {
            let shared = shared.borrow();
            let folder = args.1.filter(|folder| !folder.is_empty()).unwrap_or_else(|| shared.mod_folder.clone());
            let value = &shared.mod_settings.get(&folder).and_then(|saved| saved.get(&args.0)).cloned().unwrap_or_default();
            // a key is kept with both of its kinds; a script asks for the keyboard's
            from_json(lua, value.get("keyboard").unwrap_or(value))
        });
        function!("getRandomInt", |_lua, _shared, args: (Option<i64>, Option<i64>)| {
            let (low, high) = (args.0.unwrap_or(0), args.1.unwrap_or(i32::MAX as i64));
            Ok(crate::gfx::rand::gen_range(low.min(high), low.max(high) + 1))
        });
        function!("getRandomFloat", |_lua, _shared, args: (Option<f64>, Option<f64>)| Ok(crate::gfx::rand::gen_range(args.0.unwrap_or(0.0) as f32, args.1.unwrap_or(1.0) as f32) as f64));
        function!("getRandomBool", |_lua, _shared, args: Option<f64>| Ok(crate::gfx::rand::gen_range(0.0, 100.0) < args.unwrap_or(50.0) as f32));
        function!("stringStartsWith", |_lua, _shared, args: (String, String)| Ok(args.0.starts_with(&args.1)));
        function!("stringEndsWith", |_lua, _shared, args: (String, String)| Ok(args.0.ends_with(&args.1)));
        function!("stringTrim", |_lua, _shared, args: String| Ok(args.trim().to_owned()));
        function!("stringSplit", |_lua, _shared, args: (String, String)| Ok(args.0.split(args.1.as_str()).map(str::to_owned).collect::<Vec<_>>()));
        Ok(())
    }

    /// Calls `name` in every script that has it, with `args`. What the last answered is returned (as the Haxe engine's
    /// callOnLuas), Function_Stop winning over the others. A script whose call fails is said so and goes on.
    pub fn call(&mut self, name: &str, args: impl mlua::IntoLuaMulti + Clone) -> Option<String> {
        let mut answer = None;
        for script in self.scripts.iter().filter(|script| !*script.closed.borrow()) {
            let Ok(Some(function)) = script.lua.globals().get::<Option<Function>>(name) else { continue };
            match function.call::<Value>(args.clone()) {
                Ok(Value::String(text)) => {
                    let text = text.to_str().map(|text| text.to_owned()).unwrap_or_default();
                    if answer.as_deref() != Some(STOP) {
                        answer = Some(text);
                    }
                }
                Ok(_) => {}
                Err(error) => eprintln!("Lua {} ({name}): {error}", script.name),
            }
        }
        self.scripts.retain(|script| !*script.closed.borrow());
        answer
    }

    /// Whether `call` was answered with Function_Stop.
    pub fn stopped(answer: &Option<String>) -> bool {
        matches!(answer.as_deref(), Some(STOP | STOP_LUA | STOP_ALL))
    }

    /// Sets a global in every script.
    pub fn set(&self, name: &str, value: impl mlua::IntoLua + Clone) {
        for script in &self.scripts {
            let _ = script.lua.globals().set(name, value.clone());
        }
    }

    /// Runs the timers on by `dt` seconds, calling onTimerCompleted(tag, loops done, loops left) for each that rings.
    pub fn tick(&mut self, dt: f64) {
        let mut rung = Vec::new();
        {
            let mut shared = self.shared.borrow_mut();
            shared.timers.retain(|tag, (left, each, loops, done)| {
                *left -= dt;
                if *left > 0.0 {
                    return true;
                }
                *done += 1;
                let more = *loops == 0 || *done < *loops;
                rung.push((tag.clone(), *done, if *loops == 0 { 0 } else { *loops - *done }));
                *left += *each;
                more
            });
        }
        for (tag, done, left) in rung {
            self.call("onTimerCompleted", (tag, done, left));
        }
    }

    /// What the scripts asked for since the last time.
    pub fn requests(&self) -> Vec<Request> {
        std::mem::take(&mut self.shared.borrow_mut().requests)
    }

    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(code: &str) -> Scripts {
        let file = std::env::temp_dir().join(format!("funkin-lua-test-{}-{}.lua", std::process::id(), code.len()));
        std::fs::write(&file, code).unwrap();
        let scripts = Scripts::start(&[file.clone()], &[("songName", serde_json::json!("test"))], Shared::default());
        let _ = std::fs::remove_file(file);
        scripts
    }

    #[test]
    fn scripts_are_called_back_and_ask_the_game() {
        let mut scripts = run(r#"
            local beats = 0
            function onBeatHit() beats = beats + 1; if beats == 2 then playSound('confirmMenu', 0.5); setHealth(1.5) end end
            function onEndSong() return Function_Stop end
            function onTimerCompleted(tag, done, left) triggerEvent('Hey!', tag, done) end
            runTimer('wait', 0.5, 2)
        "#);
        scripts.call("onBeatHit", ());
        scripts.call("onBeatHit", ());
        assert_eq!(scripts.requests(), [Request::Sound { name: "confirmMenu".to_owned(), volume: 0.5 }, Request::SetHealth(1.5)]);
        assert!(Scripts::stopped(&scripts.call("onEndSong", ())));
        // a timer of two loops rings twice, then is gone
        scripts.tick(0.6);
        scripts.tick(0.5);
        scripts.tick(0.5);
        assert_eq!(scripts.requests(), [1, 2].map(|done| Request::Event { name: "Hey!".to_owned(), value1: "wait".to_owned(), value2: done.to_string() }));
    }

    #[test]
    fn scripts_share_what_they_keep_and_read_the_game() {
        let mut scripts = run("function onBeatHit() setVar('list', {1, 2, songName}); setProperty('camGame.zoom', getProperty('health') + 1) end");
        scripts.shared.borrow_mut().props.insert("health".to_owned(), serde_json::json!(1.5));
        scripts.call("onBeatHit", ());
        assert_eq!(scripts.requests(), [Request::SetProperty { name: "camGame.zoom".to_owned(), value: serde_json::json!(2.5) }]);
        assert_eq!(scripts.shared.borrow().vars["list"], serde_json::json!([1, 2, "test"]));
    }

    #[test]
    fn a_broken_script_is_left_out_and_a_closed_one_stops() {
        assert!(run("this is not lua").is_empty());
        let mut scripts = run("function onUpdate() close() end");
        assert!(!scripts.is_empty());
        scripts.call("onUpdate", 0.016);
        assert!(scripts.is_empty());
    }
}
