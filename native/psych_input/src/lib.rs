//! Keyboard capture for Psych Engine through Windows Raw Input.
//!
//! OpenFL delivers key events once per frame, so at low FPS a press is judged up to a whole
//! frame away from when it happened. This library reads the keyboard on its own thread and
//! stamps every key transition with QueryPerformanceCounter, so the game can judge a press at
//! the moment it happened whatever the frame rate is.
//!
//! C ABI (called from source/backend/RawInput.hx, main thread only):
//! - `psych_rawinput_start() -> int`     1 when the capture thread is running
//! - `psych_rawinput_next() -> int`      next event as `code | down << 16`, or -1 when empty
//! - `psych_rawinput_age_ms() -> double` age of the event `next` returned last
//! - `psych_rawinput_covers(code) -> int` 1 when this key code is delivered by `next`
#![cfg_attr(not(any(test, feature = "std")), no_std)]

use core::sync::atomic::{AtomicI64, AtomicU32, AtomicUsize, Ordering};

#[cfg_attr(not(windows), allow(dead_code))]
const RI_KEY_BREAK: u16 = 1;
#[cfg_attr(not(windows), allow(dead_code))]
const RI_KEY_E0: u16 = 2;

/// Key codes this library delivers. They are Windows virtual keys that equal the Flash/FlxKey
/// code on every keyboard layout. Every other key stays on the OpenFL event path.
// shortcut: punctuation and numpad keys are not covered because their virtual key differs from
// the OpenFL code on some layouts (JIS, AZERTY, NumLock off); map them here to give them raw timing.
fn covered(code: i32) -> bool {
    matches!(code,
        8 | 9 | 13          // backspace, tab, enter
        | 16..=18           // shift, control, alt
        | 32..=40           // space, page up/down, end, home, arrows
        | 45 | 46           // insert, delete
        | 48..=57           // 0-9
        | 65..=90           // A-Z
        | 112..=123)        // F1-F12
}

/// Key code for a raw keyboard event, or `None` when the OpenFL path must handle the key.
#[cfg_attr(not(windows), allow(dead_code))]
fn translate(vkey: u16, make_code: u16, flags: u16) -> Option<u16> {
    if !covered(vkey as i32) {
        return None;
    }
    // Events sent by software can carry no scan code; the virtual key is all there is.
    let extended = flags & RI_KEY_E0 != 0 || make_code == 0;
    match vkey {
        // Without the E0 prefix these are numpad keys with NumLock off: OpenFL reports NUMPAD_x.
        33..=40 | 45 | 46 if !extended => None,
        // With the E0 prefix this is the numpad enter: OpenFL reports NUMPAD_ENTER.
        13 if flags & RI_KEY_E0 != 0 => None,
        _ => Some(vkey),
    }
}

/// Single-producer (capture thread) single-consumer (game thread) ring of key transitions.
struct Queue {
    head: AtomicUsize,
    tail: AtomicUsize,
    keys: [AtomicU32; Queue::CAP],
    ticks: [AtomicI64; Queue::CAP],
}

impl Queue {
    const CAP: usize = 256;

    const fn new() -> Self {
        const KEY: AtomicU32 = AtomicU32::new(0);
        const TICK: AtomicI64 = AtomicI64::new(0);
        Queue {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            keys: [KEY; Queue::CAP],
            ticks: [TICK; Queue::CAP],
        }
    }

    /// Drops the event when the game has not read 256 events: it is not consuming input then.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn push(&self, key: u32, tick: i64) {
        let head = self.head.load(Ordering::Relaxed);
        if head.wrapping_sub(self.tail.load(Ordering::Acquire)) >= Queue::CAP {
            return;
        }
        self.keys[head % Queue::CAP].store(key, Ordering::Relaxed);
        self.ticks[head % Queue::CAP].store(tick, Ordering::Relaxed);
        self.head.store(head.wrapping_add(1), Ordering::Release);
    }

    fn pop(&self) -> Option<(u32, i64)> {
        let tail = self.tail.load(Ordering::Relaxed);
        if tail == self.head.load(Ordering::Acquire) {
            return None;
        }
        let event = (
            self.keys[tail % Queue::CAP].load(Ordering::Relaxed),
            self.ticks[tail % Queue::CAP].load(Ordering::Relaxed),
        );
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(event)
    }
}

static QUEUE: Queue = Queue::new();

const STOPPED: u32 = 0;
#[cfg(windows)]
const STARTING: u32 = 1;
const RUNNING: u32 = 2;
#[cfg(windows)]
const FAILED: u32 = 3;
static STATE: AtomicU32 = AtomicU32::new(STOPPED);

/// QueryPerformanceCounter value of the event `psych_rawinput_next` returned last.
static LAST_TICK: AtomicI64 = AtomicI64::new(0);

#[no_mangle]
pub extern "C" fn psych_rawinput_covers(code: i32) -> i32 {
    (STATE.load(Ordering::Acquire) == RUNNING && covered(code)) as i32
}

#[no_mangle]
pub extern "C" fn psych_rawinput_next() -> i32 {
    match QUEUE.pop() {
        Some((key, tick)) => {
            LAST_TICK.store(tick, Ordering::Relaxed);
            key as i32
        }
        None => -1,
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use core::ffi::c_void;
    use core::ptr::{null, null_mut};

    type Handle = *mut c_void;

    #[repr(C)]
    struct RawInputDevice {
        usage_page: u16,
        usage: u16,
        flags: u32,
        target: Handle,
    }

    #[repr(C)]
    struct RawInputHeader {
        kind: u32,
        size: u32,
        device: Handle,
        wparam: usize,
    }

    #[repr(C)]
    struct RawKeyboard {
        make_code: u16,
        flags: u16,
        reserved: u16,
        vkey: u16,
        message: u32,
        extra: u32,
    }

    /// RAWINPUT for a keyboard, with room to spare so GetRawInputData never finds it too small.
    #[repr(C)]
    struct RawInput {
        header: RawInputHeader,
        keyboard: RawKeyboard,
        spare: [u64; 8],
    }

    #[repr(C)]
    struct Msg {
        hwnd: Handle,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        x: i32,
        y: i32,
    }

    const WM_INPUT: u32 = 0x00FF;
    const RID_INPUT: u32 = 0x1000_0003;
    const RIM_TYPEKEYBOARD: u32 = 1;
    const RIDEV_INPUTSINK: u32 = 0x100;
    const HWND_MESSAGE: Handle = -3isize as Handle;
    const THREAD_PRIORITY_HIGHEST: i32 = 2;

    #[link(name = "user32")]
    extern "system" {
        fn CreateWindowExW(ex_style: u32, class: *const u16, name: *const u16, style: u32,
            x: i32, y: i32, w: i32, h: i32, parent: Handle, menu: Handle, instance: Handle,
            param: *mut c_void) -> Handle;
        fn RegisterRawInputDevices(devices: *const RawInputDevice, count: u32, size: u32) -> i32;
        fn GetRawInputData(input: Handle, command: u32, data: *mut c_void, size: *mut u32,
            header_size: u32) -> u32;
        fn GetMessageW(msg: *mut Msg, hwnd: Handle, min: u32, max: u32) -> i32;
        fn DispatchMessageW(msg: *const Msg) -> isize;
        fn GetForegroundWindow() -> Handle;
        fn GetAsyncKeyState(vkey: i32) -> i16;
        fn GetWindowThreadProcessId(hwnd: Handle, process_id: *mut u32) -> u32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateThread(security: *mut c_void, stack_size: usize,
            start: unsafe extern "system" fn(*mut c_void) -> u32, param: *mut c_void, flags: u32,
            thread_id: *mut u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn GetCurrentThread() -> Handle;
        fn SetThreadPriority(thread: Handle, priority: i32) -> i32;
        fn GetCurrentProcessId() -> u32;
        fn QueryPerformanceCounter(count: *mut i64) -> i32;
        fn QueryPerformanceFrequency(frequency: *mut i64) -> i32;
        fn Sleep(milliseconds: u32);
    }

    fn now() -> i64 {
        let mut count = 0i64;
        unsafe { QueryPerformanceCounter(&mut count) };
        count
    }

    fn game_has_focus() -> bool {
        let mut process_id = 0u32;
        unsafe {
            GetWindowThreadProcessId(GetForegroundWindow(), &mut process_id);
            process_id == GetCurrentProcessId()
        }
    }

    unsafe extern "system" fn capture_thread(_: *mut c_void) -> u32 {
        // A message-only window of the predefined STATIC class receives WM_INPUT.
        let class: [u16; 7] = [b'S' as u16, b'T' as u16, b'A' as u16, b'T' as u16, b'I' as u16, b'C' as u16, 0];
        let window = CreateWindowExW(0, class.as_ptr(), null(), 0, 0, 0, 0, 0, HWND_MESSAGE,
            null_mut(), null_mut(), null_mut());
        // INPUTSINK is what delivers input to a window that never has keyboard focus. Legacy key
        // messages stay on (no RIDEV_NOLEGACY), so the game window's own keyboard handling is untouched.
        let keyboard = RawInputDevice { usage_page: 1, usage: 6, flags: RIDEV_INPUTSINK, target: window };
        if window.is_null()
            || RegisterRawInputDevices(&keyboard, 1, core::mem::size_of::<RawInputDevice>() as u32) == 0
        {
            STATE.store(FAILED, Ordering::Release);
            return 1;
        }
        // The game thread is busiest exactly when timestamps matter most (low FPS).
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        STATE.store(RUNNING, Ordering::Release);

        // Start from the real keyboard state, or a key held right now would count as pressed
        // again on its first auto-repeat.
        let mut down = [false; 256];
        for (vkey, held) in down.iter_mut().enumerate() {
            *held = GetAsyncKeyState(vkey as i32) < 0;
        }
        let mut msg: Msg = core::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if msg.message == WM_INPUT {
                let tick = now();
                let mut input: RawInput = core::mem::zeroed();
                let mut size = core::mem::size_of::<RawInput>() as u32;
                let read = GetRawInputData(msg.lparam as Handle, RID_INPUT,
                    &mut input as *mut RawInput as *mut c_void, &mut size,
                    core::mem::size_of::<RawInputHeader>() as u32);
                if read != u32::MAX && input.header.kind == RIM_TYPEKEYBOARD {
                    let key = input.keyboard;
                    if let Some(code) = translate(key.vkey, key.make_code, key.flags) {
                        let pressed = key.flags & RI_KEY_BREAK == 0;
                        // Track transitions even without focus, or a key released while the game
                        // was in the background would swallow its next press as auto-repeat.
                        if down[code as usize] != pressed {
                            down[code as usize] = pressed;
                            if game_has_focus() {
                                QUEUE.push(code as u32 | (pressed as u32) << 16, tick);
                            }
                        }
                    }
                }
            }
            // DefWindowProc must see WM_INPUT so the system frees the raw input buffer.
            DispatchMessageW(&msg);
        }
        STATE.store(FAILED, Ordering::Release);
        0
    }

    #[no_mangle]
    pub extern "C" fn psych_rawinput_start() -> i32 {
        if STATE.compare_exchange(STOPPED, STARTING, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            let thread = unsafe { CreateThread(null_mut(), 0, capture_thread, null_mut(), 0, null_mut()) };
            if thread.is_null() {
                STATE.store(FAILED, Ordering::Release);
            } else {
                unsafe { CloseHandle(thread) };
            }
        }
        // The thread always leaves STARTING: it reports RUNNING or FAILED right after registering.
        while STATE.load(Ordering::Acquire) == STARTING {
            unsafe { Sleep(0) };
        }
        (STATE.load(Ordering::Acquire) == RUNNING) as i32
    }

    #[no_mangle]
    pub extern "C" fn psych_rawinput_age_ms() -> f64 {
        let mut frequency = 1i64;
        unsafe { QueryPerformanceFrequency(&mut frequency) };
        (now() - LAST_TICK.load(Ordering::Relaxed)) as f64 * 1000.0 / frequency as f64
    }
}

// Rust callers reach these by path; C callers through the exported symbols.
#[cfg(windows)]
pub use win::{psych_rawinput_age_ms, psych_rawinput_start};

#[cfg(windows)]
mod present;
#[cfg(windows)]
mod window;

// Other systems have no capture thread: the game keeps using OpenFL key events.
#[cfg(not(windows))]
#[no_mangle]
pub extern "C" fn psych_rawinput_start() -> i32 {
    0
}

#[cfg(not(windows))]
#[no_mangle]
pub extern "C" fn psych_rawinput_age_ms() -> f64 {
    0.0
}

#[cfg(not(any(test, feature = "std")))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_delivered_code_is_covered() {
        for vkey in 0..=u16::MAX {
            for make_code in [0u16, 0x1C, 0x48] {
                for flags in 0..8u16 {
                    if let Some(code) = translate(vkey, make_code, flags) {
                        assert!(covered(code as i32) && code < 256);
                    }
                }
            }
        }
    }

    #[test]
    fn numpad_keys_stay_on_the_openfl_path() {
        assert_eq!(translate(38, 0x48, RI_KEY_E0), Some(38)); // arrow up
        assert_eq!(translate(38, 0x48, 0), None); // numpad 8, NumLock off
        assert_eq!(translate(38, 0, 0), Some(38)); // arrow up sent by software, no scan code
        assert_eq!(translate(13, 0x1C, 0), Some(13)); // enter
        assert_eq!(translate(13, 0x1C, RI_KEY_E0), None); // numpad enter
        assert_eq!(translate(104, 0x48, 0), None); // numpad 8, NumLock on
        assert_eq!(translate(186, 0x27, 0), None); // punctuation depends on the layout
        assert_eq!(translate(255, 0x2A, RI_KEY_E0), None); // fake shift
        assert_eq!(translate(68, 0x20, RI_KEY_BREAK), Some(68)); // D released
    }

    #[test]
    fn queue_keeps_order_and_drops_when_full() {
        let queue = Queue::new();
        assert_eq!(queue.pop(), None);
        for i in 0..Queue::CAP as u32 + 10 {
            queue.push(i, i as i64 * 2);
        }
        for i in 0..Queue::CAP as u32 {
            assert_eq!(queue.pop(), Some((i, i as i64 * 2)));
        }
        assert_eq!(queue.pop(), None);
        queue.push(7, 70);
        assert_eq!(queue.pop(), Some((7, 70)));
    }

    #[test]
    fn nothing_is_covered_before_the_thread_runs() {
        assert_eq!(psych_rawinput_covers(65), 0);
        assert_eq!(psych_rawinput_next(), -1);
    }
}
