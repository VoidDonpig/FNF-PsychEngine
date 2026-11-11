//! The files the game keeps (scores, settings): written so that neither a crash nor a power cut half way leaves a broken
//! one, and one that cannot be read is put aside instead of being written over.
use serde_json::Value;
use std::io::Write;
use std::path::Path;

/// Writes `text` beside `path`, onto the disk, then moves it over.
pub fn write(path: &Path, text: &str) -> std::io::Result<()> {
    let part = path.with_extension("part");
    let mut file = std::fs::File::create(&part)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&part, path)
}

/// The JSON kept at `path`, or null when there is none. One that cannot be read is moved to `<name>.bad.json`, so that
/// what is saved next does not take its place.
pub fn read(path: &Path) -> Value {
    let Ok(bytes) = std::fs::read(path) else { return Value::Null };
    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        let aside = path.with_extension("bad.json");
        let moved = std::fs::rename(path, &aside);
        eprintln!("{} cannot be read ({error}); {}", path.display(), if moved.is_ok() { format!("it was moved to {}", aside.display()) } else { "it is left as it is".to_owned() });
        Value::Null
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broken_file_is_put_aside_not_written_over() {
        let folder = std::env::temp_dir().join(format!("funkin-files-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("kept.json");
        assert_eq!(read(&path), Value::Null);
        write(&path, r#"{"a": 1}"#).unwrap();
        assert_eq!(read(&path)["a"], 1);
        std::fs::write(&path, r#"{"a": 1,"#).unwrap();
        assert_eq!(read(&path), Value::Null);
        assert!(!path.exists() && folder.join("kept.bad.json").exists());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
