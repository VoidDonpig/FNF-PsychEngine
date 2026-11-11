//! A mod's own settings, from its `data/settings.json`, as the Haxe engine's mod settings page reads, shows and changes
//! them. Their values are kept per mod, by each setting's `save` name; a key's as `{"keyboard": .., "gamepad": ..}`.
use serde_json::{json, Map, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Bool,
    Int,
    Float,
    /// 0 to 1, shown as 0 to 100.
    Percent,
    /// One of these names.
    Text(Vec<String>),
    /// A key, by its name in the Haxe engine (`"SPACE"`, `"A"`).
    Key,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModOption {
    pub name: String,
    pub about: String,
    /// What the value is saved under, and what translations know it by.
    pub save: String,
    pub translation: String,
    pub kind: Kind,
    pub default: Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// How much a press changes it, and how much a second of holding does.
    pub step: f64,
    pub scroll: f64,
    pub decimals: i32,
    /// How it is shown: `%v` the value, `%d` the default.
    pub format: String,
}

/// The settings a mod describes. Those without a `save` name are left out.
pub fn parse(text: &str) -> Vec<ModOption> {
    let list: Value = serde_json::from_str(text).unwrap_or_default();
    list.as_array().into_iter().flatten().filter_map(|option| {
        let save = option["save"].as_str()?.to_owned();
        let text = |key: &str| option[key].as_str().map(str::to_owned);
        let number = |key: &str| option[key].as_f64();
        let kind = match option["type"].as_str().unwrap_or("").trim().to_lowercase().as_str() {
            "int" | "integer" => Kind::Int,
            "float" | "fl" => Kind::Float,
            "percent" => Kind::Percent,
            "string" | "str" => Kind::Text(option["options"].as_array().into_iter().flatten().filter_map(|name| name.as_str().map(str::to_owned)).collect()),
            "keybind" | "key" => Kind::Key,
            _ => Kind::Bool,
        };
        let percent = kind == Kind::Percent;
        let default = match (&kind, &option["value"]) {
            (Kind::Key, _) => json!(text("keyboard").unwrap_or_else(|| "NONE".to_owned())),
            (_, Value::Null) => match &kind {
                Kind::Bool => json!(false),
                Kind::Percent => json!(1),
                Kind::Text(names) => json!(names.first().cloned().unwrap_or_default()),
                _ => json!(0),
            },
            (_, value) => value.clone(),
        };
        let name = text("name").unwrap_or_else(|| save.clone());
        Some(ModOption {
            translation: text("translation_key").unwrap_or_else(|| name.clone()),
            about: text("description").unwrap_or_else(|| "No description provided.".to_owned()),
            min: number("min").or(percent.then_some(0.0)),
            max: number("max").or(percent.then_some(1.0)),
            step: number("step").unwrap_or(if percent { 0.01 } else { 1.0 }),
            scroll: number("scroll").unwrap_or(if percent { 0.5 } else { 50.0 }),
            decimals: number("decimals").map_or(if percent { 2 } else { 1 }, |decimals| decimals as i32),
            format: text("format").unwrap_or_else(|| if percent { "%v%" } else { "%v" }.to_owned()),
            name,
            save,
            kind,
            default,
        })
    }).collect()
}

/// A number as Haxe writes one: no decimals when it has none.
fn written(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 { format!("{}", number as i64) } else { format!("{number}") }
}

fn rounded(number: f64, decimals: i32) -> f64 {
    let scale = 10f64.powi(decimals.max(0));
    (number * scale).round() / scale
}

impl ModOption {
    /// Its value in a mod's saved settings, or its default.
    pub fn value(&self, saved: &Map<String, Value>) -> Value {
        match (saved.get(&self.save), &self.kind) {
            (Some(keys), Kind::Key) => keys["keyboard"].as_str().map_or(self.default.clone(), |key| json!(key)),
            (Some(value), _) if !value.is_null() => value.clone(),
            _ => self.default.clone(),
        }
    }

    /// Keeps `value` in a mod's saved settings.
    pub fn store(&self, saved: &mut Map<String, Value>, value: Value) {
        let value = match self.kind {
            Kind::Key => json!({ "keyboard": value, "gamepad": saved.get(&self.save).and_then(|keys| keys["gamepad"].as_str()).unwrap_or("NONE") }),
            _ => value,
        };
        saved.insert(self.save.clone(), value);
    }

    /// How a value reads beside its name (switches show a tick box instead).
    pub fn shown(&self, value: &Value) -> String {
        let number = |value: &Value| value.as_f64().map_or_else(|| value.as_str().unwrap_or("").to_owned(), |number| written(if self.kind == Kind::Percent { rounded(number * 100.0, 4) } else { number }));
        match self.kind {
            Kind::Key | Kind::Text(_) => value.as_str().unwrap_or("").to_owned(),
            _ => self.format.replace("%v", &number(value)).replace("%d", &number(&self.default)),
        }
    }

    /// The value `steps` presses on (Left negative); a switch flips, a name goes round the list.
    pub fn stepped(&self, value: &Value, steps: f64) -> Value {
        let number = value.as_f64().unwrap_or(0.0) + self.step * steps;
        let number = number.max(self.min.unwrap_or(f64::MIN)).min(self.max.unwrap_or(f64::MAX));
        match &self.kind {
            Kind::Bool => json!(value.as_bool() != Some(true)),
            Kind::Int => json!(number.round() as i64),
            Kind::Float | Kind::Percent => json!(rounded(number, self.decimals)),
            Kind::Text(names) if !names.is_empty() => {
                let at = names.iter().position(|name| Some(name.as_str()) == value.as_str()).unwrap_or(0) as i64;
                json!(names[(at + steps.signum() as i64).rem_euclid(names.len() as i64) as usize])
            }
            _ => value.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_shows_and_changes_a_mods_settings() {
        let options = parse(r#"[
            {"name": "Flashy", "save": "flashy", "type": "bool", "value": true},
            {"name": "Lives", "save": "lives", "type": "int", "min": 1, "max": 5, "value": 3, "format": "%v (default %d)"},
            {"save": "volume", "type": "percent"},
            {"name": "Mode", "save": "mode", "type": "string", "options": ["Easy", "Hard", "Mean"]},
            {"name": "Dodge", "save": "dodge", "type": "keybind", "keyboard": "SPACE"},
            {"name": "No save name", "type": "bool"}
        ]"#);
        assert_eq!(options.iter().map(|option| option.name.as_str()).collect::<Vec<_>>(), ["Flashy", "Lives", "volume", "Mode", "Dodge"]);
        let mut saved = Map::new();
        let (lives, volume, mode, dodge) = (&options[1], &options[2], &options[3], &options[4]);
        // what is not saved yet is its default
        assert_eq!((options[0].value(&saved), lives.value(&saved), volume.value(&saved), mode.value(&saved)), (json!(true), json!(3), json!(1), json!("Easy")));
        assert_eq!((lives.shown(&json!(3)), volume.shown(&json!(0.35)), dodge.shown(&dodge.value(&saved))), ("3 (default 3)".to_owned(), "35%".to_owned(), "SPACE".to_owned()));
        // kept within their limits, rounded to their decimals, and round the list
        assert_eq!((lives.stepped(&json!(5), 1.0), lives.stepped(&json!(1), -10.0)), (json!(5), json!(1)));
        assert_eq!((volume.stepped(&json!(1), 1.0), volume.stepped(&json!(0.5), -1.0)), (json!(1.0), json!(0.49)));
        assert_eq!((mode.stepped(&json!("Easy"), -1.0), mode.stepped(&json!("Mean"), 1.0)), (json!("Mean"), json!("Easy")));
        assert_eq!(options[0].stepped(&json!(true), 0.0), json!(false));
        // a key is kept as the Haxe engine keeps it
        dodge.store(&mut saved, json!("A"));
        assert_eq!((saved["dodge"].clone(), dodge.value(&saved)), (json!({ "keyboard": "A", "gamepad": "NONE" }), json!("A")));
        assert!(parse("not json").is_empty());
    }
}
