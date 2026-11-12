//! Text copied and pasted (the Haxe engine's lime Clipboard): on Windows the system's clipboard, so codes go between the
//! game and other programs; elsewhere one kept inside the game.
// shortcut: no system clipboard off Windows (X11 and Wayland hand it over through the window); add one when asked.

thread_local! {
    static KEPT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// What was last copied.
pub fn get() -> String {
    #[cfg(windows)]
    if let Some(text) = windows::get() {
        return text;
    }
    KEPT.with(|kept| kept.borrow().clone())
}

pub fn set(text: &str) {
    KEPT.with(|kept| *kept.borrow_mut() = text.to_owned());
    #[cfg(windows)]
    windows::set(text);
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
    #[test]
    fn what_is_copied_is_pasted() {
        super::set("1,2,3");
        // (on Windows another program may take the clipboard between the two; there it is the system's)
        if !cfg!(windows) {
            assert_eq!(super::get(), "1,2,3");
        }
    }
}
