//! How a finished frame reaches the screen: the "Graphics API" option, and the frame rate limit.
//!
//! The game always draws with OpenGL. The option picks who shows the drawn frame:
//! - OpenGL: the usual way, by swapping the window's buffers.
//! - DirectX (Windows): a DXGI flip-model swap chain (native/psych_input/src/present.rs), which Windows can put on
//!   screen without its compositor, and which can take the display for the game alone (exclusive fullscreen).
//! - Vulkan: a Vulkan swap chain (vulkan.rs).
//! One that fails says why in `funkin-rs-present.log` and leaves the frames to OpenGL from then on.
use crate::vulkan::Vulkan;
use std::path::Path;
use std::time::{Duration, Instant};

/// The window's own swap of OpenGL's buffers, which the window library does after every frame, is made to do
/// nothing while DXGI shows the frames: both at once fight over the window.
#[cfg(windows)]
mod swap {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    type Handle = *mut c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> Handle;
        fn GetModuleHandleA(name: *const u8) -> Handle;
        fn GetProcAddress(module: Handle, name: *const u8) -> *const c_void;
        fn VirtualProtect(address: *mut c_void, size: usize, protection: u32, old: *mut u32) -> i32;
    }

    /// While set, swapping does nothing and says it went well.
    pub static SKIP: AtomicBool = AtomicBool::new(false);
    static ORIGINAL: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn swap_buffers(dc: Handle) -> i32 {
        if SKIP.load(Ordering::Relaxed) {
            return 1;
        }
        let original: unsafe extern "system" fn(Handle) -> i32 = std::mem::transmute(ORIGINAL.load(Ordering::Relaxed));
        original(dc)
    }

    /// Puts `swap_buffers` in the place of gdi32's SwapBuffers for every call this program makes: its entry in
    /// the table of imported functions is rewritten. False when the entry is not found (then nothing changed).
    pub unsafe fn hook() -> bool {
        if ORIGINAL.load(Ordering::Relaxed) != 0 {
            return true;
        }
        let target = GetProcAddress(GetModuleHandleA(b"gdi32.dll\0".as_ptr()), b"SwapBuffers\0".as_ptr()) as usize;
        let base = GetModuleHandleW(std::ptr::null()) as *const u8;
        if target == 0 || base.is_null() {
            return false;
        }
        // the program file's headers: "PE" header, 20 bytes of file header, then the optional header (64-bit kind only)
        let optional = base.add(*(base.add(0x3C) as *const i32) as usize + 24);
        if (optional as *const u16).read_unaligned() != 0x20B {
            return false;
        }
        let imports = (optional.add(120) as *const u32).read_unaligned() as usize;
        if imports == 0 {
            return false;
        }
        // one descriptor of five numbers per library, the last of them where its functions' addresses are kept
        let mut library = base.add(imports) as *const [u32; 5];
        while (*library)[3] != 0 {
            let mut slot = base.add((*library)[4] as usize) as *mut usize;
            while *slot != 0 {
                if *slot == target {
                    const PAGE_READWRITE: u32 = 4;
                    let mut old = 0;
                    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut old) == 0 {
                        return false;
                    }
                    ORIGINAL.store(target, Ordering::Relaxed);
                    *slot = swap_buffers as usize;
                    VirtualProtect(slot.cast(), 8, old, &mut old);
                    return true;
                }
                slot = slot.add(1);
            }
            library = library.add(1);
        }
        false
    }

    #[cfg(test)]
    mod tests {
        #[link(name = "gdi32")]
        extern "system" {
            fn SwapBuffers(dc: *mut std::ffi::c_void) -> i32;
        }

        #[test]
        fn the_programs_own_swaps_can_be_switched_off() {
            unsafe {
                // no window: the real one fails
                assert_eq!(SwapBuffers(std::ptr::null_mut()), 0);
                assert!(super::hook() && super::hook());
                assert_eq!(SwapBuffers(std::ptr::null_mut()), 0, "still the real one underneath");
                super::SKIP.store(true, std::sync::atomic::Ordering::Relaxed);
                assert_eq!(SwapBuffers(std::ptr::null_mut()), 1);
                super::SKIP.store(false, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}

/// When the next frame may start, for `fps` frames a second at most: `last` is when this one was due.
/// A frame that ran late does not make the following ones hurry.
fn next_due(last: Instant, now: Instant, fps: f64) -> Instant {
    let frame = Duration::from_secs_f64(1.0 / fps.max(1.0));
    (last + frame).max(now.checked_sub(frame).unwrap_or(now))
}

pub struct Presenter {
    /// Index into `settings::GRAPHICS_APIS`; back to 0 when the chosen one fails.
    api: usize,
    vulkan: Option<Vulkan>,
    title: &'static str,
    due: Instant,
}

impl Presenter {
    /// `title`: the game window's.
    pub fn new(api: usize, title: &'static str) -> Presenter {
        Presenter { api, vulkan: None, title, due: Instant::now() }
    }

    fn give_up(&mut self, base: &Path, why: &str) {
        let name = crate::settings::GRAPHICS_APIS[self.api];
        eprintln!("{name} presentation is off: {why}");
        let _ = std::fs::write(base.join("funkin-rs-present.log"), format!("{name} presentation is off: {why}\n"));
        (self.api, self.vulkan) = (0, None);
    }

    /// Call when a frame is drawn: shows it the chosen way, then waits out what is left of the frame's time.
    /// `exclusive`: the game is fullscreen and wants the display for itself.
    pub fn frame(&mut self, exclusive: bool, fps: f64, base: &Path) {
        if self.api != 0 {
            // what was drawn so far is only a list of things to draw until the frame ends: have it drawn now
            unsafe { macroquad::prelude::get_internal_gl() }.flush();
        }
        match self.api {
            1 => {
                if let Err(why) = directx(exclusive) {
                    self.give_up(base, &why);
                }
            }
            2 => {
                if self.vulkan.is_none() {
                    match Vulkan::new(self.title) {
                        Ok(vulkan) => self.vulkan = Some(vulkan),
                        Err(why) => self.give_up(base, &why),
                    }
                }
                if let Some(Err(why)) = self.vulkan.as_mut().map(Vulkan::present) {
                    self.give_up(base, &why);
                }
            }
            _ => {}
        }
        let _ = exclusive;

        // sleeping is coarse: it stops a little short, and the rest is waited out awake
        let now = Instant::now();
        if let Some(left) = self.due.checked_duration_since(now).and_then(|left| left.checked_sub(Duration::from_millis(1))) {
            std::thread::sleep(left);
        }
        while Instant::now() < self.due {
            std::hint::spin_loop();
        }
        self.due = next_due(self.due, Instant::now(), fps);
    }
}

/// Shows the frame through DXGI and keeps the window's own swap from showing it as well.
#[cfg(windows)]
fn directx(exclusive: bool) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    unsafe {
        if !swap::hook() {
            return Err("the window's own way of showing frames could not be switched off".to_owned());
        }
        let shown = psych_input::psych_present(exclusive as i32) != 0;
        swap::SKIP.store(shown, Ordering::Relaxed);
        if shown {
            Ok(())
        } else {
            // the steps are listed in native/psych_input/src/present.rs
            Err(format!("step {}, error 0x{:08X}", psych_input::psych_present_error(), psych_input::psych_present_hresult()))
        }
    }
}

#[cfg(not(windows))]
fn directx(_exclusive: bool) -> Result<(), String> {
    Err("only on Windows".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_spaced_evenly_and_a_late_one_is_not_made_up_for() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        // on time: exactly one frame after the last one was due, whenever within the frame it was asked
        assert_eq!(next_due(start, start + ms(3), 100.0), start + ms(10));
        assert_eq!(next_due(start, start + ms(10), 100.0), start + ms(10));
        // half a second late: the next is due at once, and the one after it a frame later, not all the missed ones
        let late = next_due(start, start + ms(500), 100.0);
        assert_eq!(late, start + ms(490));
        assert_eq!(next_due(late, start + ms(500), 100.0), start + ms(500));
    }
}
