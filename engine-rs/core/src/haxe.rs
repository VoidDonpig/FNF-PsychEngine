//! Reading what the Haxe engine saved: its save file (`funkin.sol`) is the game's data in Haxe's serialization, as text.
//! Read into JSON: objects and string-keyed maps become objects, arrays and lists arrays.
use serde_json::{json, Map, Value};

struct Reader<'a> {
    text: &'a [u8],
    at: usize,
    /// Strings seen so far, which a later "R" may name again; objects likewise for "r".
    strings: Vec<String>,
    objects: Vec<Value>,
}

impl Reader<'_> {
    fn next(&mut self) -> Option<u8> {
        let byte = *self.text.get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    /// The characters while `keep` holds.
    fn run(&mut self, keep: impl Fn(u8) -> bool) -> &str {
        let from = self.at;
        while self.peek().is_some_and(&keep) {
            self.at += 1;
        }
        std::str::from_utf8(&self.text[from..self.at]).unwrap_or("")
    }

    fn int(&mut self) -> Option<i64> {
        self.run(|byte| byte == b'-' || byte.is_ascii_digit()).parse().ok()
    }

    /// A string after its "y" (or a reference to one after its "R").
    fn string(&mut self) -> Option<String> {
        match self.next()? {
            b'y' => {
                let length = self.int()? as usize;
                (self.next()? == b':').then_some(())?;
                let raw = self.text.get(self.at..self.at + length)?;
                self.at += length;
                let decoded = decode(raw);
                self.strings.push(decoded.clone());
                Some(decoded)
            }
            b'R' => {
                let index = self.int()? as usize;
                self.strings.get(index).cloned()
            }
            _ => None,
        }
    }

    /// Items until `end`, `item` reading each.
    fn until(&mut self, end: u8, mut item: impl FnMut(&mut Self) -> Option<()>) -> Option<()> {
        while self.peek()? != end {
            item(self)?;
        }
        self.at += 1;
        Some(())
    }

    /// Fields (a name, then a value) until "g".
    fn fields(&mut self) -> Option<Map<String, Value>> {
        let mut fields = Map::new();
        self.until(b'g', |reader| {
            let name = reader.string()?;
            fields.insert(name, reader.value()?);
            Some(())
        })?;
        Some(fields)
    }

    fn value(&mut self) -> Option<Value> {
        let kept = |reader: &mut Self, value: Value| {
            reader.objects.push(value.clone());
            Some(value)
        };
        Some(match self.peek()? {
            b'y' | b'R' => return self.string().map(Value::String),
            _ => match self.next()? {
                b'n' => Value::Null,
                b't' => json!(true),
                b'f' => json!(false),
                b'z' => json!(0),
                b'i' => json!(self.int()?),
                b'd' => json!(self.run(|byte| byte.is_ascii_digit() || b"-+.eE".contains(&byte)).parse::<f64>().ok()?),
                // not-a-number and the infinities have no place in JSON
                b'k' | b'm' | b'p' => Value::Null,
                b'o' => {
                    let fields = self.fields()?;
                    return kept(self, Value::Object(fields));
                }
                b'c' => {
                    self.string()?; // the class's name
                    let fields = self.fields()?;
                    return kept(self, Value::Object(fields));
                }
                b'a' | b'l' => {
                    let mut items = Vec::new();
                    self.until(b'h', |reader| {
                        // "u5": five nulls in a row
                        if reader.peek()? == b'u' {
                            reader.at += 1;
                            items.extend(std::iter::repeat_n(Value::Null, reader.int()? as usize));
                        } else {
                            items.push(reader.value()?);
                        }
                        Some(())
                    })?;
                    return kept(self, Value::Array(items));
                }
                b'b' => {
                    let mut map = Map::new();
                    self.until(b'h', |reader| {
                        let key = reader.string()?;
                        map.insert(key, reader.value()?);
                        Some(())
                    })?;
                    return kept(self, Value::Object(map));
                }
                b'q' => {
                    let mut map = Map::new();
                    self.until(b'h', |reader| {
                        (reader.next()? == b':').then_some(())?;
                        let key = reader.int()?.to_string();
                        map.insert(key, reader.value()?);
                        Some(())
                    })?;
                    return kept(self, Value::Object(map));
                }
                // a map keyed by objects: kept as its pairs
                b'M' => {
                    let mut pairs = Vec::new();
                    self.until(b'h', |reader| {
                        pairs.push(json!([reader.value()?, reader.value()?]));
                        Some(())
                    })?;
                    return kept(self, Value::Array(pairs));
                }
                b'r' => {
                    let index = self.int()? as usize;
                    return self.objects.get(index).cloned();
                }
                // an enum's case: its name and arguments
                tag @ (b'w' | b'j') => {
                    self.string()?; // the enum's name
                    // by the case's name, or (after a colon) its number
                    let case = if tag == b'j' {
                        (self.next()? == b':').then_some(())?;
                        json!(self.int()?)
                    } else {
                        json!(self.string()?)
                    };
                    (self.next()? == b':').then_some(())?;
                    let count = self.int()?;
                    let arguments: Option<Vec<Value>> = (0..count).map(|_| self.value()).collect();
                    return kept(self, json!({ "case": case, "arguments": arguments? }));
                }
                b'v' => {
                    // a date: "2024-01-31 12:00:00", or a time in milliseconds
                    if self.text.get(self.at + 4) == Some(&b'-') {
                        let date = std::str::from_utf8(self.text.get(self.at..self.at + 19)?).ok()?.to_owned();
                        self.at += 19;
                        json!(date)
                    } else {
                        json!(self.run(|byte| byte.is_ascii_digit() || b"-+.eE".contains(&byte)).parse::<f64>().ok()?)
                    }
                }
                // bytes: skipped over
                b's' => {
                    let length = self.int()? as usize;
                    (self.next()? == b':').then_some(())?;
                    self.at += length;
                    Value::Null
                }
                _ => return None,
            },
        })
    }
}

/// %XX escapes undone, as Haxe writes strings.
fn decode(raw: &[u8]) -> String {
    let mut bytes = Vec::with_capacity(raw.len());
    let mut at = 0;
    while at < raw.len() {
        match (raw[at], raw.get(at + 1..at + 3).and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok())) {
            (b'%', Some(byte)) => {
                bytes.push(byte);
                at += 3;
            }
            (byte, _) => {
                bytes.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// What a Haxe save holds, or None when it cannot be read (all of it).
pub fn unserialize(text: &str) -> Option<Value> {
    let mut reader = Reader { text: text.trim().as_bytes(), at: 0, strings: Vec::new(), objects: Vec::new() };
    let value = reader.value()?;
    (reader.at == reader.text.len()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_haxe_save() {
        // what the Haxe engine writes: an object holding string maps of scores, an array of names, a counted map
        let text = "oy10:songScoresby12:bopeebo-hardi123450y5:freshi-5hy10:songRatingbR1d0.9512R2zhy20:achievementsUnlockedaR2y8:two_keysu2hy13:weekCompletedby5:week1thy18:psychDevsEasterEggy7:BBPANZUy4:noteq:1i2:2nhg";
        let save = unserialize(text).unwrap();
        assert_eq!(save["songScores"], json!({ "bopeebo-hard": 123450, "fresh": -5 }));
        assert_eq!(save["songRating"], json!({ "bopeebo-hard": 0.9512, "fresh": 0 }));
        // "R2" names the third string seen again; "u2" is two nulls
        assert_eq!(save["achievementsUnlocked"], json!(["fresh", "two_keys", null, null]));
        assert_eq!((save["weekCompleted"]["week1"].as_bool(), save["psychDevsEasterEgg"].as_str()), (Some(true), Some("BBPANZU")));
        assert_eq!(save["note"], json!({ "1": 2, "2": null }));
        // escapes, and what cannot be read
        assert_eq!(unserialize("y12:a%20b%C3%A9c"), Some(json!("a béc")));
        assert_eq!(unserialize("wy4:Kindy3:Two:2i1i2"), Some(json!({ "case": "Two", "arguments": [1, 2] })));
        assert_eq!(unserialize("jy4:Kind:1:0"), Some(json!({ "case": 1, "arguments": [] })));
        assert_eq!(unserialize("oy3:abc"), None);
        assert_eq!(unserialize("not a save"), None);
    }
}
