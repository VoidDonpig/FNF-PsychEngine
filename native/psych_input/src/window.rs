//! Window chores the game needs from Windows at startup (source/backend/Native.hx).
//!
//! C ABI:
//! - `psych_set_dpi_aware()`        the game draws at the display's real resolution, unscaled by Windows
//! - `psych_fill_window_black()`    paints the game window black, before the first frame is ready

use crate::present::Rect;
use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};

type Handle = *mut c_void;

// ((DPI_AWARENESS_CONTEXT)-4) in windef.h. A pointer in the official bindings, so the tests cannot compare it.
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
pub(crate) const GW_OWNER: u32 = 4;
pub(crate) const BLACK_BRUSH: i32 = 4;

#[link(name = "user32")]
extern "system" {
    fn SetProcessDPIAware() -> i32;
    fn SetProcessDpiAwarenessContext(context: Handle) -> i32;
    fn EnumWindows(callback: unsafe extern "system" fn(Handle, isize) -> i32, param: isize) -> i32;
    fn GetWindowThreadProcessId(window: Handle, process_id: *mut u32) -> u32;
    fn GetWindow(window: Handle, command: u32) -> Handle;
    fn IsWindowVisible(window: Handle) -> i32;
    fn GetDC(window: Handle) -> Handle;
    fn ReleaseDC(window: Handle, dc: Handle) -> i32;
    fn GetClientRect(window: Handle, rect: *mut Rect) -> i32;
    fn FillRect(dc: Handle, rect: *const Rect, brush: Handle) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn GetStockObject(object: i32) -> Handle;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcessId() -> u32;
}

/// The game window, once found.
static GAME_WINDOW: AtomicUsize = AtomicUsize::new(0);

/// Stops at the first visible, unowned top-level window of this process: the game window.
unsafe extern "system" fn find_game_window(window: Handle, _: isize) -> i32 {
    let mut process_id = 0;
    GetWindowThreadProcessId(window, &mut process_id);
    if process_id != GetCurrentProcessId() || !GetWindow(window, GW_OWNER).is_null() || IsWindowVisible(window) == 0 {
        return 1; // keep looking
    }
    GAME_WINDOW.store(window as usize, Ordering::Relaxed);
    0
}

#[no_mangle]
pub unsafe extern "C" fn psych_set_dpi_aware() {
    SetProcessDPIAware();
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 as Handle);
}

#[no_mangle]
pub unsafe extern "C" fn psych_fill_window_black() {
    if GAME_WINDOW.load(Ordering::Relaxed) == 0 {
        EnumWindows(find_game_window, 0);
    }
    let window = GAME_WINDOW.load(Ordering::Relaxed) as Handle;
    if window.is_null() {
        return;
    }
    let dc = GetDC(window);
    let mut rect = Rect { left: 0, top: 0, right: 0, bottom: 0 };
    GetClientRect(window, &mut rect);
    FillRect(dc, &rect, GetStockObject(BLACK_BRUSH));
    ReleaseDC(window, dc);
}

/// Checked by the compiler with `cargo check --tests --target x86_64-pc-windows-msvc`.
#[cfg(test)]
mod layout {
    use super::*;
    use windows::Win32::Graphics::Gdi;
    use windows::Win32::UI::WindowsAndMessaging as wm;

    const _: () = {
        assert!(GW_OWNER == wm::GW_OWNER.0);
        assert!(BLACK_BRUSH == Gdi::BLACK_BRUSH.0);
        assert!(core::mem::size_of::<Rect>() == core::mem::size_of::<windows::Win32::Foundation::RECT>());
    };
}
