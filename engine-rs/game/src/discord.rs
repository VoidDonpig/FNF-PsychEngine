//! Discord's "Playing" box (Rich Presence), filled as the Haxe engine fills it. Discord is talked to over its own local pipe
//! (a named pipe on Windows, a socket elsewhere): a frame is a number for its kind, its length, and JSON.
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::Duration;

/// The Haxe engine's application on Discord: its pictures ("icon", the characters' icons) are uploaded there.
const CLIENT_ID: &str = "863222024192262205";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Presence {
    pub details: String,
    pub state: Option<String>,
    /// The opponent's health icon, shown small.
    pub small_image: Option<String>,
    /// When the song began and when it will end (seconds since 1970), for the time left.
    pub start: Option<u64>,
    pub end: Option<u64>,
}

static SENDER: Mutex<Option<Sender<Option<Presence>>>> = Mutex::new(None);

/// Shows `presence` in Discord, or nothing. Handed to a thread of its own: Discord being slow or away never holds the game up.
pub fn show(presence: Option<Presence>) {
    let mut sender = SENDER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let sender = sender.get_or_insert_with(|| {
        let (sender, receiver) = channel();
        std::thread::spawn(move || keep(receiver, connect));
        sender
    });
    let _ = sender.send(presence);
}

/// A frame of Discord's pipe: its kind (0 hello, 1 a command), its length, its JSON.
fn frame(kind: u32, payload: &Value) -> Vec<u8> {
    let body = payload.to_string();
    let mut frame = kind.to_le_bytes().to_vec();
    frame.extend((body.len() as u32).to_le_bytes());
    frame.extend(body.as_bytes());
    frame
}

fn read_frame(pipe: &mut impl Read) -> Option<(u32, Value)> {
    let mut head = [0; 8];
    pipe.read_exact(&mut head).ok()?;
    let (kind, length) = (u32::from_le_bytes([head[0], head[1], head[2], head[3]]), u32::from_le_bytes([head[4], head[5], head[6], head[7]]));
    let mut body = vec![0; length.min(1 << 20) as usize];
    pipe.read_exact(&mut body).ok()?;
    Some((kind, serde_json::from_slice(&body).unwrap_or_default()))
}

/// The command that shows `presence` (or clears the box).
fn command(presence: &Option<Presence>, nonce: u64) -> Value {
    let activity = presence.as_ref().map(|presence| {
        let mut activity = json!({ "details": presence.details, "assets": { "large_image": "icon", "large_text": "Engine Version: Psych Engine (Rust)" } });
        if let Some(state) = &presence.state {
            activity["state"] = json!(state);
        }
        if let Some(small) = &presence.small_image {
            activity["assets"]["small_image"] = json!(small);
        }
        for (key, time) in [("start", presence.start), ("end", presence.end)] {
            if let Some(time) = time {
                activity["timestamps"][key] = json!(time);
            }
        }
        activity
    });
    json!({ "cmd": "SET_ACTIVITY", "args": { "pid": std::process::id(), "activity": activity }, "nonce": nonce.to_string() })
}

pub trait Pipe: Read + Write + Send {}
impl<T: Read + Write + Send> Pipe for T {}

/// Discord's pipe, when Discord is running.
fn connect() -> Option<Box<dyn Pipe>> {
    #[cfg(windows)]
    for number in 0..10 {
        if let Ok(pipe) = std::fs::OpenOptions::new().read(true).write(true).open(format!(r"\\.\pipe\discord-ipc-{number}")) {
            return Some(Box::new(pipe));
        }
    }
    #[cfg(unix)]
    {
        let folders = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"].iter().filter_map(std::env::var_os).map(std::path::PathBuf::from).chain([std::path::PathBuf::from("/tmp")]);
        for folder in folders {
            // (the Flatpak and Snap builds of Discord keep theirs a folder further in)
            for within in ["", "app/com.discordapp.Discord", "snap.discord"] {
                for number in 0..10 {
                    if let Ok(socket) = std::os::unix::net::UnixStream::connect(folder.join(within).join(format!("discord-ipc-{number}"))) {
                        return Some(Box::new(socket));
                    }
                }
            }
        }
    }
    None
}

/// Says hello on a new pipe; the pipe, once Discord says it is ready.
fn hello(mut pipe: Box<dyn Pipe>) -> Option<Box<dyn Pipe>> {
    pipe.write_all(&frame(0, &json!({ "v": 1, "client_id": CLIENT_ID }))).ok()?;
    matches!(read_frame(&mut pipe), Some((1, _))).then_some(pipe)
}

/// Keeps Discord showing the last presence asked for, trying to reach it again every fifteen seconds while it is away.
fn keep(news: Receiver<Option<Presence>>, connect: fn() -> Option<Box<dyn Pipe>>) {
    let (mut pipe, mut wanted, mut nonce): (Option<Box<dyn Pipe>>, Option<Presence>, u64) = (None, None, 0);
    loop {
        match news.recv_timeout(Duration::from_secs(15)) {
            // only the latest counts
            Ok(presence) => wanted = news.try_iter().last().unwrap_or(presence),
            Err(RecvTimeoutError::Timeout) if pipe.is_some() || wanted.is_none() => continue,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if pipe.is_none() {
            pipe = connect().and_then(hello);
        }
        let Some(open) = &mut pipe else { continue };
        nonce += 1;
        // every command is answered; an answer that does not come means Discord went away
        if open.write_all(&frame(1, &command(&wanted, nonce))).is_err() || read_frame(open).is_none() {
            pipe = None;
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn says_hello_then_shows_the_latest_presence() {
        let path = std::env::temp_dir().join(format!("funkin-discord-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        static PATH: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);
        *PATH.lock().unwrap() = Some(path.clone());
        let (sender, receiver) = channel();
        std::thread::spawn(move || keep(receiver, || Some(Box::new(std::os::unix::net::UnixStream::connect(PATH.lock().unwrap().clone()?).ok()?))));
        let playing = Presence { details: "Freeplay".to_owned(), state: Some("Bopeebo (Hard)".to_owned()), small_image: Some("dad".to_owned()), start: Some(10), end: Some(100) };
        sender.send(Some(playing)).unwrap();
        // Discord, played by the test: it reads the hello, says it is ready, and answers the command
        let (mut discord, _) = listener.accept().unwrap();
        let (kind, hello) = read_frame(&mut discord).unwrap();
        assert_eq!((kind, hello["client_id"].as_str()), (0, Some(CLIENT_ID)));
        discord.write_all(&frame(1, &json!({ "evt": "READY" }))).unwrap();
        let (kind, shown) = read_frame(&mut discord).unwrap();
        discord.write_all(&frame(1, &json!({ "evt": null }))).unwrap();
        let activity = &shown["args"]["activity"];
        assert_eq!((kind, shown["cmd"].as_str(), activity["state"].as_str(), activity["assets"]["small_image"].as_str()), (1, Some("SET_ACTIVITY"), Some("Bopeebo (Hard)"), Some("dad")));
        assert_eq!((activity["timestamps"]["start"].as_u64(), activity["timestamps"]["end"].as_u64()), (Some(10), Some(100)));
        // nothing to show: the box is cleared
        sender.send(None).unwrap();
        let (_, cleared) = read_frame(&mut discord).unwrap();
        assert!(cleared["args"]["activity"].is_null());
        let _ = std::fs::remove_file(&path);
    }
}
