//! Presents the game through a DXGI flip-model swap chain instead of OpenGL's SwapBuffers.
//!
//! An OpenGL window is presented by copying it into the desktop compositor ("Composed: Copy" in
//! PresentMon). A flip-model swap chain can be put on screen directly by Windows ("Hardware:
//! Independent Flip") when it covers the monitor, which removes the compositor's frame of latency.
//!
//! The game keeps rendering with OpenGL. Each frame its back buffer is copied into a Direct3D 11
//! texture shared through WGL_NV_DX_interop2 (every current NVIDIA, AMD and Intel driver has it),
//! and that texture is presented by DXGI with tearing allowed, so there is still no vsync.
//!
//! The OpenGL context is also moved off the game window, onto a hidden window of the same size.
//! Graphics drivers treat an OpenGL window that covers the screen as exclusive fullscreen and
//! put its own (never swapped, so black) buffers on screen, over the swap chain.
//!
//! C ABI (called from source/backend/FlipPresent.hx on the render thread, GL context current):
//! - `psych_present(exclusive) -> int`  1 when the frame was presented here (the caller must skip
//!   SwapBuffers), 0 when this path is unavailable and the caller must present as usual.
//!   `exclusive` is 1 while the game is fullscreen and wants the display for itself.
//! - `psych_present_error() -> int`  which step failed last (see `fail`), 0 for none
//! - `psych_present_hresult() -> int`  the Windows error code of that step, 0 when it has none

use core::ffi::c_void;
use core::ptr::{addr_of_mut, null, null_mut};
use core::sync::atomic::{AtomicI32, Ordering};

type Handle = *mut c_void;
type ComPtr = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Guid(pub u32, pub u16, pub u16, pub [u8; 8]);

pub(crate) const IID_IDXGI_FACTORY2: Guid = Guid(0x50c83a1c, 0xe072, 0x4c48, [0x87, 0xb0, 0x36, 0x30, 0xfa, 0x36, 0xa6, 0xd0]);
pub(crate) const IID_IDXGI_DEVICE1: Guid = Guid(0x77db970f, 0x6276, 0x48ba, [0xba, 0x28, 0x07, 0x01, 0x43, 0xb4, 0x39, 0x2c]);
pub(crate) const IID_ID3D11_TEXTURE2D: Guid = Guid(0x6f15aaf2, 0xd208, 0x4e89, [0x9a, 0xb4, 0x48, 0x95, 0x35, 0xd3, 0x4f, 0x9c]);

// Positions of the methods used in each COM interface's function table.
// The tests at the bottom check every one against the `windows` crate.
pub(crate) const UNKNOWN_QUERY_INTERFACE: usize = 0;
pub(crate) const UNKNOWN_RELEASE: usize = 2;
pub(crate) const OBJECT_GET_PARENT: usize = 6;
pub(crate) const DXGI_DEVICE_GET_ADAPTER: usize = 7;
pub(crate) const FACTORY_MAKE_WINDOW_ASSOCIATION: usize = 8;
pub(crate) const FACTORY2_CREATE_SWAP_CHAIN_FOR_HWND: usize = 15;
pub(crate) const SWAP_CHAIN_PRESENT: usize = 8;
pub(crate) const SWAP_CHAIN_GET_BUFFER: usize = 9;
pub(crate) const SWAP_CHAIN_SET_FULLSCREEN_STATE: usize = 10;
pub(crate) const SWAP_CHAIN_GET_FULLSCREEN_STATE: usize = 11;
pub(crate) const SWAP_CHAIN_RESIZE_BUFFERS: usize = 13;
pub(crate) const DEVICE_CREATE_TEXTURE2D: usize = 5;
pub(crate) const CONTEXT_COPY_RESOURCE: usize = 47;
pub(crate) const DXGI_DEVICE1_SET_MAXIMUM_FRAME_LATENCY: usize = 12;

#[repr(C)]
pub(crate) struct SwapChainDesc1 {
    width: u32,
    height: u32,
    format: u32,
    stereo: i32,
    sample_count: u32,
    sample_quality: u32,
    buffer_usage: u32,
    buffer_count: u32,
    scaling: u32,
    swap_effect: u32,
    alpha_mode: u32,
    flags: u32,
}

#[repr(C)]
pub(crate) struct Texture2DDesc {
    width: u32,
    height: u32,
    mip_levels: u32,
    array_size: u32,
    format: u32,
    sample_count: u32,
    sample_quality: u32,
    usage: u32,
    bind_flags: u32,
    cpu_access_flags: u32,
    misc_flags: u32,
}

#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
pub(crate) struct WindowClass {
    size: u32,
    style: u32,
    procedure: unsafe extern "system" fn(Handle, u32, usize, isize) -> isize,
    class_extra: i32,
    window_extra: i32,
    instance: Handle,
    icon: Handle,
    cursor: Handle,
    background: Handle,
    menu_name: *const u16,
    class_name: *const u16,
    small_icon: Handle,
}

/// PIXELFORMATDESCRIPTOR, only ever filled in by Windows and handed back to it.
pub(crate) type PixelFormat = [u8; 40];

pub(crate) const CS_OWNDC: u32 = 0x20;
pub(crate) const WS_POPUP: u32 = 0x8000_0000;
pub(crate) const SWP_NOMOVE: u32 = 2;
pub(crate) const SWP_NOZORDER: u32 = 4;
pub(crate) const SWP_NOACTIVATE: u32 = 0x10;
pub(crate) const SWP_FRAMECHANGED: u32 = 0x20;
pub(crate) const GWL_STYLE: i32 = -16;
pub(crate) const GWL_EXSTYLE: i32 = -20;

pub(crate) const DXGI_FORMAT_B8G8R8A8_UNORM: u32 = 87;
pub(crate) const DXGI_USAGE_RENDER_TARGET_OUTPUT: u32 = 0x20;
pub(crate) const DXGI_SWAP_EFFECT_FLIP_DISCARD: u32 = 4;
pub(crate) const DXGI_ALPHA_MODE_IGNORE: u32 = 3;
pub(crate) const DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING: u32 = 2048;
pub(crate) const DXGI_PRESENT_ALLOW_TEARING: u32 = 0x200;
pub(crate) const DXGI_MWA_NO_WINDOW_CHANGES: u32 = 1;
pub(crate) const DXGI_MWA_NO_ALT_ENTER: u32 = 2;
pub(crate) const D3D_DRIVER_TYPE_HARDWARE: u32 = 1;
pub(crate) const D3D11_SDK_VERSION: u32 = 7;
pub(crate) const D3D11_BIND_RENDER_TARGET: u32 = 0x20;

const WGL_ACCESS_WRITE_DISCARD_NV: u32 = 2;
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_SCISSOR_TEST: u32 = 0x0C11;
const GL_NEAREST: u32 = 0x2600;
const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
const GL_SAMPLE_BUFFERS: u32 = 0x80A8;
const GL_DRAW_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
const GL_READ_FRAMEBUFFER: u32 = 0x8CA8;
const GL_DRAW_FRAMEBUFFER: u32 = 0x8CA9;
const GL_READ_FRAMEBUFFER_BINDING: u32 = 0x8CAA;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;

#[link(name = "opengl32")]
extern "system" {
    fn wglGetCurrentDC() -> Handle;
    fn wglGetCurrentContext() -> Handle;
    fn wglMakeCurrent(dc: Handle, context: Handle) -> i32;
    fn wglGetProcAddress(name: *const u8) -> *const c_void;
    fn glGetIntegerv(name: u32, value: *mut i32);
    fn glIsEnabled(capability: u32) -> u8;
    fn glEnable(capability: u32);
    fn glDisable(capability: u32);
    fn glGenTextures(count: i32, names: *mut u32);
    fn glDeleteTextures(count: i32, names: *const u32);
}

#[link(name = "user32")]
extern "system" {
    fn WindowFromDC(dc: Handle) -> Handle;
    fn GetClientRect(window: Handle, rect: *mut Rect) -> i32;
    fn RegisterClassExW(class: *const WindowClass) -> u16;
    fn CreateWindowExW(ex_style: u32, class: *const u16, name: *const u16, style: u32, x: i32, y: i32,
        width: i32, height: i32, parent: Handle, menu: Handle, instance: Handle, param: *mut c_void) -> Handle;
    fn DestroyWindow(window: Handle) -> i32;
    fn DefWindowProcW(window: Handle, message: u32, wparam: usize, lparam: isize) -> isize;
    fn SetWindowPos(window: Handle, after: Handle, x: i32, y: i32, width: i32, height: i32, flags: u32) -> i32;
    fn GetDC(window: Handle) -> Handle;
    fn GetForegroundWindow() -> Handle;
    fn GetWindowRect(window: Handle, rect: *mut Rect) -> i32;
    fn GetWindowLongW(window: Handle, index: i32) -> i32;
    fn SetWindowLongW(window: Handle, index: i32, value: i32) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn GetPixelFormat(dc: Handle) -> i32;
    fn DescribePixelFormat(dc: Handle, format: i32, size: u32, descriptor: *mut PixelFormat) -> i32;
    fn SetPixelFormat(dc: Handle, format: i32, descriptor: *const PixelFormat) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Handle;
}

/// "PsychGL" as a wide string: the class of the hidden window OpenGL renders to.
const HIDDEN_CLASS: [u16; 8] = [0x50, 0x73, 0x79, 0x63, 0x68, 0x47, 0x4C, 0];

#[link(name = "d3d11")]
extern "system" {
    fn D3D11CreateDevice(adapter: ComPtr, driver_type: u32, software: Handle, flags: u32,
        feature_levels: *const u32, feature_level_count: u32, sdk_version: u32, device: *mut ComPtr,
        feature_level: *mut u32, context: *mut ComPtr) -> i32;
}

static ERROR_STEP: AtomicI32 = AtomicI32::new(0);
static ERROR_HRESULT: AtomicI32 = AtomicI32::new(0);

/// Remembers which step failed, for the log the game writes. Always answers false.
/// Steps: 1 driver lacks a function, 2 no window or multisampled, 3 Direct3D device, 4 sharing with
/// OpenGL, 5 shared texture, 6 copy of the back buffer, 7 DXGI factory, 8 swap chain, 9 resize,
/// 10 back buffer of the swap chain, 11 present.
fn fail(step: i32, hresult: i32) -> bool {
    ERROR_STEP.store(step, Ordering::Relaxed);
    ERROR_HRESULT.store(hresult, Ordering::Relaxed);
    false
}

#[no_mangle]
pub extern "C" fn psych_present_error() -> i32 {
    ERROR_STEP.load(Ordering::Relaxed)
}

#[no_mangle]
pub extern "C" fn psych_present_hresult() -> i32 {
    ERROR_HRESULT.load(Ordering::Relaxed)
}

/// OpenGL and WGL functions that opengl32.dll does not export: the driver hands them out.
#[derive(Clone, Copy)]
struct Gl {
    gen_framebuffers: unsafe extern "system" fn(i32, *mut u32),
    delete_framebuffers: unsafe extern "system" fn(i32, *const u32),
    bind_framebuffer: unsafe extern "system" fn(u32, u32),
    framebuffer_texture_2d: unsafe extern "system" fn(u32, u32, u32, u32, i32),
    check_framebuffer_status: unsafe extern "system" fn(u32) -> u32,
    blit_framebuffer: unsafe extern "system" fn(i32, i32, i32, i32, i32, i32, i32, i32, u32, u32),
    dx_open_device: unsafe extern "system" fn(ComPtr) -> Handle,
    dx_close_device: unsafe extern "system" fn(Handle) -> i32,
    dx_register_object: unsafe extern "system" fn(Handle, ComPtr, u32, u32, u32) -> Handle,
    dx_unregister_object: unsafe extern "system" fn(Handle, Handle) -> i32,
    dx_lock_objects: unsafe extern "system" fn(Handle, i32, *mut Handle) -> i32,
    dx_unlock_objects: unsafe extern "system" fn(Handle, i32, *mut Handle) -> i32,
}

impl Gl {
    unsafe fn load() -> Option<Gl> {
        unsafe fn get<F>(name: &[u8]) -> Option<F> {
            let address = wglGetProcAddress(name.as_ptr());
            // Some drivers answer 1, 2, 3 or -1 instead of null for a function they do not have.
            if (address as isize) <= 3 {
                None
            } else {
                Some(core::mem::transmute_copy(&address))
            }
        }
        Some(Gl {
            gen_framebuffers: get(b"glGenFramebuffers\0")?,
            delete_framebuffers: get(b"glDeleteFramebuffers\0")?,
            bind_framebuffer: get(b"glBindFramebuffer\0")?,
            framebuffer_texture_2d: get(b"glFramebufferTexture2D\0")?,
            check_framebuffer_status: get(b"glCheckFramebufferStatus\0")?,
            blit_framebuffer: get(b"glBlitFramebuffer\0")?,
            dx_open_device: get(b"wglDXOpenDeviceNV\0")?,
            dx_close_device: get(b"wglDXCloseDeviceNV\0")?,
            dx_register_object: get(b"wglDXRegisterObjectNV\0")?,
            dx_unregister_object: get(b"wglDXUnregisterObjectNV\0")?,
            dx_lock_objects: get(b"wglDXLockObjectsNV\0")?,
            dx_unlock_objects: get(b"wglDXUnlockObjectsNV\0")?,
        })
    }
}

/// Entry `index` of a COM object's function table, as a function of type `F`.
unsafe fn method<F>(object: ComPtr, index: usize) -> F {
    let table = *(object as *const *const *const c_void);
    core::mem::transmute_copy(&*table.add(index))
}

unsafe fn release(object: ComPtr) {
    if !object.is_null() {
        let release: unsafe extern "system" fn(ComPtr) -> u32 = method(object, UNKNOWN_RELEASE);
        release(object);
    }
}

struct Presenter {
    gl: Gl,
    window: Handle,
    /// The game window's device context and the OpenGL context lime made current on it.
    window_dc: Handle,
    gl_context: Handle,
    /// Hidden window the OpenGL context is moved to, null while it is still on the game window.
    hidden_window: Handle,
    device: ComPtr,
    context: ComPtr,
    swap_chain: ComPtr,
    /// Direct3D texture the game's picture is copied into; `gl_texture` is the same memory seen from OpenGL.
    texture: ComPtr,
    gl_texture: u32,
    framebuffer: u32,
    interop_device: Handle,
    interop_texture: Handle,
    width: i32,
    height: i32,
    tearing: bool,
    /// Whether the swap chain holds the display exclusively right now.
    exclusive: bool,
    /// Set when `exclusive` changed and the buffers have not been resized since. DXGI shows
    /// nothing until they are, even when the size is the same.
    stale_buffers: bool,
}

impl Presenter {
    unsafe fn client_size(window: Handle) -> (i32, i32) {
        let mut rect = Rect { left: 0, top: 0, right: 0, bottom: 0 };
        GetClientRect(window, &mut rect);
        (rect.right - rect.left, rect.bottom - rect.top)
    }

    /// Everything that can fail fails here, before the first frame is presented: after it, the
    /// window belongs to the swap chain and SwapBuffers no longer reaches the screen.
    unsafe fn create() -> Option<Presenter> {
        let Some(gl) = Gl::load() else {
            fail(1, 0);
            return None;
        };
        let window_dc = wglGetCurrentDC();
        let window = WindowFromDC(window_dc);
        let (width, height) = Self::client_size(window);
        // A multisampled back buffer cannot be copied upside down in one blit.
        let mut multisampled = 0;
        glGetIntegerv(GL_SAMPLE_BUFFERS, &mut multisampled);
        if window.is_null() || width <= 0 || height <= 0 || multisampled != 0 {
            fail(2, 0);
            return None;
        }

        let mut presenter = Presenter {
            gl, window, window_dc, gl_context: wglGetCurrentContext(), hidden_window: null_mut(), device: null_mut(), context: null_mut(), swap_chain: null_mut(), texture: null_mut(),
            gl_texture: 0, framebuffer: 0, interop_device: null_mut(), interop_texture: null_mut(),
            width, height, tearing: true, exclusive: false, stale_buffers: false,
        };
        if presenter.build() {
            Some(presenter)
        } else {
            presenter.destroy();
            None
        }
    }

    unsafe fn build(&mut self) -> bool {
        let created = D3D11CreateDevice(null_mut(), D3D_DRIVER_TYPE_HARDWARE, null_mut(), 0, null(), 0,
            D3D11_SDK_VERSION, &mut self.device, null_mut(), &mut self.context);
        if created < 0 {
            return fail(3, created);
        }

        // Fails when OpenGL and Direct3D run on different graphics cards (some laptops).
        self.interop_device = (self.gl.dx_open_device)(self.device);
        if self.interop_device.is_null() {
            return fail(4, 0);
        }

        // Prove the shared texture works before the swap chain exists: the swap chain is the step
        // that cannot be undone.
        glGenTextures(1, &mut self.gl_texture);
        (self.gl.gen_framebuffers)(1, &mut self.framebuffer);
        if !(self.create_texture() && self.copy_back_buffer()) {
            return false;
        }

        // The swap chain must come from the factory the device belongs to. On any other factory,
        // MakeWindowAssociation below has no effect and DXGI answers Alt+Enter with its own
        // exclusive fullscreen, on top of lime's.
        let query: unsafe extern "system" fn(ComPtr, *const Guid, *mut ComPtr) -> i32 = method(self.device, UNKNOWN_QUERY_INTERFACE);
        let mut dxgi_device: ComPtr = null_mut();
        let mut adapter: ComPtr = null_mut();
        let mut factory: ComPtr = null_mut();
        let mut found = query(self.device, &IID_IDXGI_DEVICE1, &mut dxgi_device);
        if found >= 0 {
            // Never queue more than one frame: a queued frame is a frame of latency.
            let set_latency: unsafe extern "system" fn(ComPtr, u32) -> i32 = method(dxgi_device, DXGI_DEVICE1_SET_MAXIMUM_FRAME_LATENCY);
            set_latency(dxgi_device, 1);

            let get_adapter: unsafe extern "system" fn(ComPtr, *mut ComPtr) -> i32 = method(dxgi_device, DXGI_DEVICE_GET_ADAPTER);
            found = get_adapter(dxgi_device, &mut adapter);
        }
        if found >= 0 {
            let get_parent: unsafe extern "system" fn(ComPtr, *const Guid, *mut ComPtr) -> i32 = method(adapter, OBJECT_GET_PARENT);
            found = get_parent(adapter, &IID_IDXGI_FACTORY2, &mut factory);
        }
        release(adapter);
        release(dxgi_device);
        if found < 0 {
            return fail(7, found);
        }
        let create_swap_chain: unsafe extern "system" fn(ComPtr, ComPtr, Handle, *const SwapChainDesc1,
            *const c_void, ComPtr, *mut ComPtr) -> i32 = method(factory, FACTORY2_CREATE_SWAP_CHAIN_FOR_HWND);
        let mut desc = SwapChainDesc1 {
            width: self.width as u32,
            height: self.height as u32,
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            stereo: 0,
            sample_count: 1,
            sample_quality: 0,
            buffer_usage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            buffer_count: 2,
            scaling: 0, // stretch
            swap_effect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            alpha_mode: DXGI_ALPHA_MODE_IGNORE,
            flags: DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING,
        };
        let mut result = create_swap_chain(factory, self.device, self.window, &desc, null(), null_mut(), &mut self.swap_chain);
        if result < 0 {
            // Windows 10 before 1511 has the flip model without tearing: frames are then dropped, not torn.
            self.tearing = false;
            desc.flags = 0;
            result = create_swap_chain(factory, self.device, self.window, &desc, null(), null_mut(), &mut self.swap_chain);
        }
        if result >= 0 {
            // DXGI must leave Alt+Enter and the window style alone: lime handles fullscreen.
            let associate: unsafe extern "system" fn(ComPtr, Handle, u32) -> i32 = method(factory, FACTORY_MAKE_WINDOW_ASSOCIATION);
            associate(factory, self.window, DXGI_MWA_NO_WINDOW_CHANGES | DXGI_MWA_NO_ALT_ENTER);
        }
        release(factory);
        if result < 0 {
            return fail(8, result);
        }
        true
    }

    /// The shared texture, at the current size.
    unsafe fn create_texture(&mut self) -> bool {
        let desc = Texture2DDesc {
            width: self.width as u32,
            height: self.height as u32,
            mip_levels: 1,
            array_size: 1,
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            sample_count: 1,
            sample_quality: 0,
            usage: 0, // default
            bind_flags: D3D11_BIND_RENDER_TARGET,
            cpu_access_flags: 0,
            misc_flags: 0,
        };
        let create_texture: unsafe extern "system" fn(ComPtr, *const Texture2DDesc, *const c_void, *mut ComPtr) -> i32 =
            method(self.device, DEVICE_CREATE_TEXTURE2D);
        let created = create_texture(self.device, &desc, null(), &mut self.texture);
        if created < 0 {
            return fail(5, created);
        }
        self.interop_texture = (self.gl.dx_register_object)(self.interop_device, self.texture, self.gl_texture,
            GL_TEXTURE_2D, WGL_ACCESS_WRITE_DISCARD_NV);
        !self.interop_texture.is_null() || fail(5, 0)
    }

    unsafe fn destroy_texture(&mut self) {
        if !self.interop_texture.is_null() {
            (self.gl.dx_unregister_object)(self.interop_device, self.interop_texture);
            self.interop_texture = null_mut();
        }
        release(self.texture);
        self.texture = null_mut();
    }

    /// Copies OpenGL's back buffer into the shared texture, leaving OpenGL's state as it was.
    unsafe fn copy_back_buffer(&mut self) -> bool {
        if (self.gl.dx_lock_objects)(self.interop_device, 1, &mut self.interop_texture) == 0 {
            return fail(6, 0);
        }
        let (mut read, mut draw) = (0, 0);
        glGetIntegerv(GL_READ_FRAMEBUFFER_BINDING, &mut read);
        glGetIntegerv(GL_DRAW_FRAMEBUFFER_BINDING, &mut draw);
        let scissor = glIsEnabled(GL_SCISSOR_TEST) != 0;
        if scissor {
            glDisable(GL_SCISSOR_TEST); // it would clip the copy
        }

        (self.gl.bind_framebuffer)(GL_READ_FRAMEBUFFER, 0);
        (self.gl.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, self.framebuffer);
        (self.gl.framebuffer_texture_2d)(GL_DRAW_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, self.gl_texture, 0);
        let complete = (self.gl.check_framebuffer_status)(GL_DRAW_FRAMEBUFFER) == GL_FRAMEBUFFER_COMPLETE;
        if complete {
            // OpenGL's first row is the bottom one, Direct3D's is the top one: copy upside down.
            (self.gl.blit_framebuffer)(0, 0, self.width, self.height, 0, self.height, self.width, 0,
                GL_COLOR_BUFFER_BIT, GL_NEAREST);
        }

        (self.gl.bind_framebuffer)(GL_READ_FRAMEBUFFER, read as u32);
        (self.gl.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, draw as u32);
        if scissor {
            glEnable(GL_SCISSOR_TEST);
        }
        ((self.gl.dx_unlock_objects)(self.interop_device, 1, &mut self.interop_texture) != 0 && complete) || fail(6, 0)
    }

    /// Moves the OpenGL context to a hidden window with the game window's size and pixel format.
    /// From then on OpenGL's back buffer is that window's, and the game window is only DXGI's.
    /// Without it everything still works, except fullscreen on drivers that take the screen over.
    unsafe fn hide_gl_context(&mut self) {
        let instance = GetModuleHandleW(null());
        let class = WindowClass {
            size: core::mem::size_of::<WindowClass>() as u32,
            style: CS_OWNDC, // OpenGL keeps the device context for as long as the window lives
            procedure: DefWindowProcW,
            class_extra: 0,
            window_extra: 0,
            instance,
            icon: null_mut(),
            cursor: null_mut(),
            background: null_mut(),
            menu_name: null(),
            class_name: HIDDEN_CLASS.as_ptr(),
            small_icon: null_mut(),
        };
        RegisterClassExW(&class); // fails harmlessly when a rebuilt presenter registers it again

        let hidden = CreateWindowExW(0, HIDDEN_CLASS.as_ptr(), null(), WS_POPUP, 0, 0, self.width, self.height,
            null_mut(), null_mut(), instance, null_mut());
        if hidden.is_null() {
            return;
        }
        let hidden_dc = GetDC(hidden);
        let format = GetPixelFormat(self.window_dc);
        let mut descriptor: PixelFormat = [0; 40];
        if DescribePixelFormat(self.window_dc, format, 40, &mut descriptor) != 0
            && SetPixelFormat(hidden_dc, format, &descriptor) != 0
            && wglMakeCurrent(hidden_dc, self.gl_context) != 0
        {
            self.hidden_window = hidden;
        } else {
            wglMakeCurrent(self.window_dc, self.gl_context);
            DestroyWindow(hidden);
        }
    }

    unsafe fn resize(&mut self, width: i32, height: i32) -> bool {
        self.destroy_texture();
        self.width = width;
        self.height = height;
        if !self.hidden_window.is_null() {
            SetWindowPos(self.hidden_window, null_mut(), 0, 0, width, height, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        let resize_buffers: unsafe extern "system" fn(ComPtr, u32, u32, u32, u32, u32) -> i32 =
            method(self.swap_chain, SWAP_CHAIN_RESIZE_BUFFERS);
        let flags = if self.tearing { DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING } else { 0 };
        // 0 buffers and format 0 keep the ones the swap chain has.
        let resized = resize_buffers(self.swap_chain, 0, width as u32, height as u32, 0, flags);
        if resized < 0 {
            return fail(9, resized);
        }
        self.stale_buffers = false;
        self.create_texture()
    }

    unsafe fn is_exclusive(&self) -> bool {
        let get_state: unsafe extern "system" fn(ComPtr, *mut i32, *mut ComPtr) -> i32 =
            method(self.swap_chain, SWAP_CHAIN_GET_FULLSCREEN_STATE);
        let mut exclusive = 0;
        get_state(self.swap_chain, &mut exclusive, null_mut()) >= 0 && exclusive != 0
    }

    /// Takes or gives back exclusive use of the display. Windows refuses while another window is in
    /// front or a mode change is running; the next frame asks again.
    unsafe fn set_exclusive(&mut self, exclusive: bool) {
        let set_state: unsafe extern "system" fn(ComPtr, i32, ComPtr) -> i32 =
            method(self.swap_chain, SWAP_CHAIN_SET_FULLSCREEN_STATE);
        if exclusive {
            set_state(self.swap_chain, 1, null_mut());
            return;
        }
        // On the way out DXGI puts the window back the way it found it, which is lime's borderless
        // fullscreen. By now lime has made it a normal window again: keep what lime set.
        let style = GetWindowLongW(self.window, GWL_STYLE);
        let ex_style = GetWindowLongW(self.window, GWL_EXSTYLE);
        let mut rect = Rect { left: 0, top: 0, right: 0, bottom: 0 };
        GetWindowRect(self.window, &mut rect);
        set_state(self.swap_chain, 0, null_mut());
        SetWindowLongW(self.window, GWL_STYLE, style);
        SetWindowLongW(self.window, GWL_EXSTYLE, ex_style);
        SetWindowPos(self.window, null_mut(), rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    }

    unsafe fn present(&mut self, exclusive: bool) -> bool {
        let (width, height) = Self::client_size(self.window);
        let minimized = width <= 0 || height <= 0;

        // Windows ends exclusive use by itself, and minimizes the window, when another window comes
        // to the front. Only ask for it while the game is the one in front and on screen.
        let exclusive = exclusive && !minimized && GetForegroundWindow() == self.window;
        // Nothing to ask DXGI while exclusive use is neither wanted nor held: the usual case
        if exclusive || self.exclusive {
            if exclusive != self.is_exclusive() {
                self.set_exclusive(exclusive);
            }
            // Read back what it really is: it also changes without being asked, and a request can be refused
            let now_exclusive = self.is_exclusive();
            if now_exclusive != self.exclusive {
                self.exclusive = now_exclusive;
                self.stale_buffers = true;
            }
        }

        // The change above can resize the window
        let (width, height) = Self::client_size(self.window);
        if minimized || width <= 0 || height <= 0 {
            return true; // nothing to show; stale_buffers keeps the resize owed for when there is
        }
        if ((width, height) != (self.width, self.height) || self.stale_buffers) && !self.resize(width, height) {
            return false;
        }
        if !self.copy_back_buffer() {
            return false;
        }

        let get_buffer: unsafe extern "system" fn(ComPtr, u32, *const Guid, *mut ComPtr) -> i32 =
            method(self.swap_chain, SWAP_CHAIN_GET_BUFFER);
        let mut back_buffer: ComPtr = null_mut();
        let got = get_buffer(self.swap_chain, 0, &IID_ID3D11_TEXTURE2D, &mut back_buffer);
        if got < 0 {
            return fail(10, got);
        }
        let copy_resource: unsafe extern "system" fn(ComPtr, ComPtr, ComPtr) = method(self.context, CONTEXT_COPY_RESOURCE);
        copy_resource(self.context, back_buffer, self.texture);
        release(back_buffer);

        let present: unsafe extern "system" fn(ComPtr, u32, u32) -> i32 = method(self.swap_chain, SWAP_CHAIN_PRESENT);
        // The tearing flag is only valid in a window; with exclusive use, interval 0 tears by itself.
        let flags = if self.tearing && !self.exclusive { DXGI_PRESENT_ALLOW_TEARING } else { 0 };
        let presented = present(self.swap_chain, 0, flags);
        presented >= 0 || fail(11, presented)
    }

    unsafe fn destroy(&mut self) {
        if !self.hidden_window.is_null() {
            wglMakeCurrent(self.window_dc, self.gl_context);
            DestroyWindow(self.hidden_window);
            self.hidden_window = null_mut();
        }
        self.destroy_texture();
        if self.framebuffer != 0 {
            (self.gl.delete_framebuffers)(1, &self.framebuffer);
        }
        if self.gl_texture != 0 {
            glDeleteTextures(1, &self.gl_texture);
        }
        if !self.interop_device.is_null() {
            (self.gl.dx_close_device)(self.interop_device);
        }
        if !self.swap_chain.is_null() && self.is_exclusive() {
            self.set_exclusive(false); // a swap chain must not be released while it holds the display
        }
        release(self.swap_chain);
        release(self.context);
        release(self.device);
    }
}

enum State {
    Untried,
    Ready(Presenter),
    Unavailable,
}

// Only the render thread touches these.
static mut STATE: State = State::Untried;
/// Set when a frame failed while exclusive: exclusive use is then left alone, or the swap chain
/// would be rebuilt on every frame.
static mut EXCLUSIVE_FAILED: bool = false;

#[no_mangle]
pub unsafe extern "C" fn psych_present(exclusive: i32) -> i32 {
    let state = &mut *addr_of_mut!(STATE);
    let exclusive = exclusive != 0 && !EXCLUSIVE_FAILED;
    if let State::Untried = state {
        *state = match Presenter::create() {
            Some(presenter) => State::Ready(presenter),
            None => State::Unavailable,
        };
    }
    if let State::Ready(presenter) = state {
        if presenter.present(exclusive) {
            // After the first frame: that one was rendered into the game window's back buffer.
            if presenter.hidden_window.is_null() {
                presenter.hide_gl_context();
            }
            return 1;
        }
        // The window shows nothing but the swap chain once it has presented, so giving up here
        // would leave a frozen picture. Start over once (lost device, failed resize) before that.
        if exclusive || presenter.exclusive {
            EXCLUSIVE_FAILED = true;
        }
        presenter.destroy();
        *state = State::Unavailable;
        if let Some(mut rebuilt) = Presenter::create() {
            if rebuilt.present(false) {
                *state = State::Ready(rebuilt);
                return 1;
            }
            rebuilt.destroy();
        }
    }
    0
}

/// Compiled with `cargo check --tests --target x86_64-pc-windows-msvc`: every assertion is checked
/// by the compiler, nothing has to run.
#[cfg(test)]
mod layout {
    use super::*;
    use core::mem::{offset_of, size_of};
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE as DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11 as d3d11;
    use windows::Win32::Graphics::Dxgi as dxgi;
    use windows::Win32::UI::WindowsAndMessaging as wm;

    const POINTER: usize = size_of::<usize>();

    const fn same(guid: Guid, official: windows::core::GUID) -> bool {
        let mut bytes = 0;
        while bytes < 8 {
            if guid.3[bytes] != official.data4[bytes] {
                return false;
            }
            bytes += 1;
        }
        guid.0 == official.data1 && guid.1 == official.data2 && guid.2 == official.data3
    }

    const _: () = {
        assert!(offset_of!(windows::core::IUnknown_Vtbl, QueryInterface) / POINTER == UNKNOWN_QUERY_INTERFACE);
        assert!(offset_of!(windows::core::IUnknown_Vtbl, Release) / POINTER == UNKNOWN_RELEASE);
        assert!(offset_of!(dxgi::IDXGIObject_Vtbl, GetParent) / POINTER == OBJECT_GET_PARENT);
        assert!(offset_of!(dxgi::IDXGIDevice_Vtbl, GetAdapter) / POINTER == DXGI_DEVICE_GET_ADAPTER);
        assert!(offset_of!(dxgi::IDXGIFactory_Vtbl, MakeWindowAssociation) / POINTER == FACTORY_MAKE_WINDOW_ASSOCIATION);
        assert!(offset_of!(dxgi::IDXGIFactory2_Vtbl, CreateSwapChainForHwnd) / POINTER == FACTORY2_CREATE_SWAP_CHAIN_FOR_HWND);
        assert!(offset_of!(dxgi::IDXGISwapChain_Vtbl, Present) / POINTER == SWAP_CHAIN_PRESENT);
        assert!(offset_of!(dxgi::IDXGISwapChain_Vtbl, GetBuffer) / POINTER == SWAP_CHAIN_GET_BUFFER);
        assert!(offset_of!(dxgi::IDXGISwapChain_Vtbl, SetFullscreenState) / POINTER == SWAP_CHAIN_SET_FULLSCREEN_STATE);
        assert!(offset_of!(dxgi::IDXGISwapChain_Vtbl, GetFullscreenState) / POINTER == SWAP_CHAIN_GET_FULLSCREEN_STATE);
        assert!(offset_of!(dxgi::IDXGISwapChain_Vtbl, ResizeBuffers) / POINTER == SWAP_CHAIN_RESIZE_BUFFERS);
        assert!(offset_of!(d3d11::ID3D11Device_Vtbl, CreateTexture2D) / POINTER == DEVICE_CREATE_TEXTURE2D);
        assert!(offset_of!(d3d11::ID3D11DeviceContext_Vtbl, CopyResource) / POINTER == CONTEXT_COPY_RESOURCE);
        assert!(offset_of!(dxgi::IDXGIDevice1_Vtbl, SetMaximumFrameLatency) / POINTER == DXGI_DEVICE1_SET_MAXIMUM_FRAME_LATENCY);

        assert!(size_of::<SwapChainDesc1>() == size_of::<dxgi::DXGI_SWAP_CHAIN_DESC1>());
        assert!(offset_of!(SwapChainDesc1, swap_effect) == offset_of!(dxgi::DXGI_SWAP_CHAIN_DESC1, SwapEffect));
        assert!(offset_of!(SwapChainDesc1, flags) == offset_of!(dxgi::DXGI_SWAP_CHAIN_DESC1, Flags));
        assert!(size_of::<Texture2DDesc>() == size_of::<d3d11::D3D11_TEXTURE2D_DESC>());
        assert!(size_of::<WindowClass>() == size_of::<wm::WNDCLASSEXW>());
        assert!(offset_of!(WindowClass, class_name) == offset_of!(wm::WNDCLASSEXW, lpszClassName));
        assert!(size_of::<PixelFormat>() == size_of::<windows::Win32::Graphics::OpenGL::PIXELFORMATDESCRIPTOR>());
        assert!(CS_OWNDC == wm::CS_OWNDC.0);
        assert!(WS_POPUP == wm::WS_POPUP.0);
        assert!(SWP_NOMOVE == wm::SWP_NOMOVE.0 && SWP_NOZORDER == wm::SWP_NOZORDER.0 && SWP_NOACTIVATE == wm::SWP_NOACTIVATE.0);
        assert!(SWP_FRAMECHANGED == wm::SWP_FRAMECHANGED.0);
        assert!(GWL_STYLE == wm::GWL_STYLE.0 && GWL_EXSTYLE == wm::GWL_EXSTYLE.0);
        assert!(offset_of!(Texture2DDesc, bind_flags) == offset_of!(d3d11::D3D11_TEXTURE2D_DESC, BindFlags));

        assert!(same(IID_IDXGI_FACTORY2, dxgi::IDXGIFactory2::IID));
        assert!(same(IID_IDXGI_DEVICE1, dxgi::IDXGIDevice1::IID));
        assert!(same(IID_ID3D11_TEXTURE2D, d3d11::ID3D11Texture2D::IID));

        assert!(DXGI_FORMAT_B8G8R8A8_UNORM == dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM.0 as u32);
        assert!(DXGI_USAGE_RENDER_TARGET_OUTPUT == dxgi::DXGI_USAGE_RENDER_TARGET_OUTPUT.0);
        assert!(DXGI_SWAP_EFFECT_FLIP_DISCARD == dxgi::DXGI_SWAP_EFFECT_FLIP_DISCARD.0 as u32);
        assert!(DXGI_ALPHA_MODE_IGNORE == dxgi::Common::DXGI_ALPHA_MODE_IGNORE.0 as u32);
        assert!(DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING == dxgi::DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING.0 as u32);
        assert!(DXGI_PRESENT_ALLOW_TEARING == dxgi::DXGI_PRESENT_ALLOW_TEARING.0);
        assert!(DXGI_MWA_NO_WINDOW_CHANGES == dxgi::DXGI_MWA_NO_WINDOW_CHANGES.0);
        assert!(DXGI_MWA_NO_ALT_ENTER == dxgi::DXGI_MWA_NO_ALT_ENTER.0);
        assert!(D3D_DRIVER_TYPE_HARDWARE == DRIVER_TYPE_HARDWARE.0 as u32);
        assert!(D3D11_SDK_VERSION == d3d11::D3D11_SDK_VERSION);
        assert!(D3D11_BIND_RENDER_TARGET == d3d11::D3D11_BIND_RENDER_TARGET.0 as u32);
    };
}
