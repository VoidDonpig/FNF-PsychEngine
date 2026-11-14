//! The game ahead of other programs for the processor, so that a busy computer (a recording being encoded, say) does not
//! make its frames and its sound late: the game above normal, its sound thread above everything (the sound card's
//! library, cpal 0.15, means to do that on Windows but hands Windows a thread number where a handle is wanted, so it
//! never happens).

/// The game's process and the calling thread (the one that draws) above normal. On Linux it asks for a nice of -10, which
/// only works where the user may.
pub fn raise_game() {
    #[cfg(windows)]
    // SAFETY: the current process's and thread's own pseudo handles, which need no closing.
    unsafe {
        windows::SetPriorityClass(windows::GetCurrentProcess(), windows::ABOVE_NORMAL_PRIORITY_CLASS);
        windows::SetThreadPriority(windows::GetCurrentThread(), windows::THREAD_PRIORITY_ABOVE_NORMAL);
    }
    #[cfg(target_os = "linux")]
    {
        extern "C" {
            fn setpriority(which: i32, who: u32, priority: i32) -> i32;
        }
        // SAFETY: setpriority takes plain numbers; on Linux it sets the calling thread, whose threads after it inherit it.
        unsafe { setpriority(0, 0, -10) };
    }
}

/// The calling thread (the sound card's) ahead of every other.
pub fn raise_sound_thread() {
    #[cfg(windows)]
    // SAFETY: the current thread's own pseudo handle.
    unsafe {
        windows::SetThreadPriority(windows::GetCurrentThread(), windows::THREAD_PRIORITY_TIME_CRITICAL);
    }
}

#[cfg(windows)]
mod windows {
    pub const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x8000;
    pub const THREAD_PRIORITY_ABOVE_NORMAL: i32 = 1;
    pub const THREAD_PRIORITY_TIME_CRITICAL: i32 = 15;
    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetCurrentProcess() -> isize;
        pub fn GetCurrentThread() -> isize;
        pub fn SetPriorityClass(process: isize, class: u32) -> i32;
        pub fn SetThreadPriority(thread: isize, priority: i32) -> i32;
    }
}
