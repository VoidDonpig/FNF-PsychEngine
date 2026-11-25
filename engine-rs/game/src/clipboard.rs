//! Text copied and pasted (the Haxe engine's lime Clipboard): the system's clipboard, so codes go between the game and
//! other programs: on Windows directly, on macOS and Linux through the system's own tools (pbcopy and pbpaste; wl-copy and
//! wl-paste, xclip or xsel). With none of them at hand a copy kept inside the game is used.

thread_local! {
    static KEPT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    /// When the game last copied something.
    static COPIED: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

/// What was last copied.
pub fn get() -> String {
    #[cfg(windows)]
    if let Some(text) = windows::get() {
        return text;
    }
    // (a tool takes a moment to own the clipboard: what the game copied a moment ago is what it pastes)
    #[cfg(not(windows))]
    if COPIED.get().is_none_or(|at| at.elapsed() > std::time::Duration::from_secs(1)) {
        if let Some(text) = tools::get() {
            return text;
        }
    }
    KEPT.with(|kept| kept.borrow().clone())
}

pub fn set(text: &str) {
    KEPT.with(|kept| *kept.borrow_mut() = text.to_owned());
    COPIED.set(Some(std::time::Instant::now()));
    #[cfg(windows)]
    windows::set(text);
    #[cfg(not(windows))]
    tools::set(text);
}

/// The command lines (program and arguments) that copy and paste, best first, for a system and whether Wayland is in use.
#[cfg(not(windows))]
fn tools_for(macos: bool, wayland: bool) -> Vec<([&'static str; 3], [&'static str; 3])> {
    // (copy, paste), unused arguments left empty
    if macos {
        vec![(["pbcopy", "", ""], ["pbpaste", "", ""])]
    } else {
        let mut tools = Vec::new();
        if wayland {
            tools.push((["wl-copy", "", ""], ["wl-paste", "--no-newline", ""]));
        }
        tools.push((["xclip", "-selection", "clipboard"], ["xclip", "-selection", "clipboard"]));
        tools.push((["xsel", "--clipboard", ""], ["xsel", "--clipboard", ""]));
        tools
    }
}

/// The system's tools, found and run without ever waiting long for them.
#[cfg(not(windows))]
mod tools {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const WAIT: Duration = Duration::from_millis(300);

    fn tools() -> Vec<([&'static str; 3], [&'static str; 3])> {
        super::tools_for(cfg!(target_os = "macos"), std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty()))
    }

    fn command(line: &[&str; 3], paste: bool) -> Command {
        let mut command = Command::new(line[0]);
        command.args(line[1..].iter().filter(|argument| !argument.is_empty()));
        // xclip and xsel read with -o; the others have their own paste program
        if paste && matches!(line[0], "xclip" | "xsel") {
            command.arg("-o");
        } else if !paste && line[0] == "xsel" {
            command.arg("-i");
        }
        command.stderr(Stdio::null());
        command
    }

    pub fn get() -> Option<String> {
        for (_, line) in tools() {
            let Ok(mut child) = command(&line, true).stdin(Stdio::null()).stdout(Stdio::piped()).spawn() else { continue };
            let mut out = child.stdout.take()?;
            // read on a helper thread, so a tool that hangs costs at most the wait
            let (send, receive) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = send.send(out.read_to_end(&mut bytes).ok().map(|_| bytes));
            });
            let bytes = receive.recv_timeout(WAIT).ok().flatten();
            let ok = bytes.is_some() && wait(&mut child);
            if !ok {
                let _ = child.kill();
                let _ = child.wait();
            }
            if let Some(bytes) = bytes.filter(|_| ok) {
                return Some(String::from_utf8_lossy(&bytes).into_owned());
            }
        }
        None
    }

    /// Whether the tool ended well within the wait.
    fn wait(child: &mut std::process::Child) -> bool {
        let start = Instant::now();
        while start.elapsed() < WAIT {
            match child.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) => std::thread::sleep(Duration::from_millis(2)),
                Err(_) => return false,
            }
        }
        false
    }

    /// How long a tool has to fail before it counts as serving the selection (wl-copy, xclip and xsel stay running to do so).
    const GRACE: Duration = Duration::from_millis(250);

    /// Starts the tool and gives it the text; whether it took it (it ended well, or is still running after the grace time).
    /// A tool still running is reaped by a helper thread, so it does not stay a zombie once it ends.
    fn serve(mut command: Command, text: &str) -> bool {
        let Ok(mut child) = command.stdin(Stdio::piped()).stdout(Stdio::null()).spawn() else { return false };
        let Some(mut stdin) = child.stdin.take() else { return false };
        // a tool that is gone already makes this fail; its exit status says the rest
        let _ = stdin.write_all(text.as_bytes());
        drop(stdin);
        let start = Instant::now();
        while start.elapsed() < GRACE {
            match child.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) => std::thread::sleep(Duration::from_millis(2)),
                Err(_) => return false,
            }
        }
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        true
    }

    pub fn set(text: &str) {
        // on a helper thread: a failing tool is waited for a moment before the next is tried, and the frame must not wait
        let text = text.to_owned();
        std::thread::spawn(move || {
            for (line, _) in tools() {
                if serve(command(&line, false), &text) {
                    return;
                }
            }
        });
    }

    #[cfg(test)]
    mod tests {
        fn sh(script: &str) -> std::process::Command {
            let mut command = std::process::Command::new("sh");
            command.args(["-c", script]);
            command
        }

        #[test]
        fn a_tool_that_starts_and_fails_is_not_the_one() {
            assert!(!super::serve(sh("cat >/dev/null; exit 1"), "x"));
            assert!(!super::serve(std::process::Command::new("no-such-clipboard-tool"), "x"));
            assert!(super::serve(sh("cat >/dev/null"), "x"));
            // one that keeps running (as it does to serve the selection) took it
            assert!(super::serve(sh("cat >/dev/null; sleep 1"), "x"));
        }
    }
}

#[cfg(windows)]
mod windows {
    const UNICODE_TEXT: u32 = 13;
    const MOVEABLE: u32 = 2;
    #[link(name = "user32")]
    extern "system" {
        fn OpenClipboard(window: isize) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn GetClipboardData(format: u32) -> isize;
        fn SetClipboardData(format: u32, memory: isize) -> isize;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
        fn GlobalFree(memory: isize) -> isize;
        fn GlobalLock(memory: isize) -> *mut u16;
        fn GlobalUnlock(memory: isize) -> i32;
    }

    pub fn get() -> Option<String> {
        // SAFETY: the clipboard is opened and closed around its use; the text it holds is read up to its terminating zero
        // while its memory is locked.
        unsafe {
            if OpenClipboard(0) == 0 {
                return None;
            }
            let memory = GetClipboardData(UNICODE_TEXT);
            let text = (memory != 0).then(|| {
                let start = GlobalLock(memory);
                if start.is_null() {
                    return String::new();
                }
                let length = (0..).take_while(|&at| *start.add(at) != 0).count();
                let text = String::from_utf16_lossy(std::slice::from_raw_parts(start, length));
                GlobalUnlock(memory);
                text
            });
            CloseClipboard();
            text
        }
    }

    pub fn set(text: &str) {
        let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
        // SAFETY: the memory is sized for the text and its zero; once handed over the clipboard owns it, else it is freed.
        unsafe {
            if OpenClipboard(0) == 0 {
                return;
            }
            EmptyClipboard();
            let memory = GlobalAlloc(MOVEABLE, wide.len() * 2);
            if memory != 0 {
                let start = GlobalLock(memory);
                if !start.is_null() {
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), start, wide.len());
                    GlobalUnlock(memory);
                    if SetClipboardData(UNICODE_TEXT, memory) == 0 {
                        GlobalFree(memory);
                    }
                } else {
                    GlobalFree(memory);
                }
            }
            CloseClipboard();
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    #[test]
    fn the_tool_depends_on_the_system() {
        let first = |macos, wayland| super::tools_for(macos, wayland).iter().map(|(copy, _)| copy[0]).collect::<Vec<_>>();
        assert_eq!(first(true, false), ["pbcopy"]);
        assert_eq!(first(false, true), ["wl-copy", "xclip", "xsel"]);
        assert_eq!(first(false, false), ["xclip", "xsel"]);
    }

    #[test]
    fn what_is_copied_is_pasted() {
        super::set("1,2,3");
        // (on Windows another program may take the clipboard between the two; there it is the system's)
        if !cfg!(windows) {
            assert_eq!(super::get(), "1,2,3");
        }
    }
}
