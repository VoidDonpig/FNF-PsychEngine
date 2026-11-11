//! Shows the game's frames through a Vulkan swap chain (the "Vulkan" choice of the Graphics API option).
//!
//! The game keeps drawing with OpenGL. When a frame is done its picture is read back, handed to Vulkan
//! and presented in a window of Vulkan's own: a child of the game window that covers it and lets all
//! input through. Graphics drivers do not let Vulkan present to a window OpenGL already draws to.
// shortcut: the picture goes through main memory (glReadPixels), a few ms a frame at 1080p. Share the
// memory between the two (GL_EXT_memory_object) if Vulkan is to be as fast as the other two.
use ash::vk;
use macroquad::miniquad::gl;
use std::ffi::c_void;

fn failed(what: &str) -> impl Fn(vk::Result) -> String + '_ {
    move |error| format!("{what}: {error}")
}

#[cfg(windows)]
mod native {
    use ash::vk;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};

    type Handle = *mut c_void;

    #[link(name = "opengl32")]
    extern "system" {
        fn wglGetCurrentDC() -> Handle;
    }

    #[link(name = "user32")]
    extern "system" {
        fn WindowFromDC(dc: Handle) -> Handle;
        fn GetClientRect(window: Handle, rect: *mut [i32; 4]) -> i32;
        fn CreateWindowExW(ex_style: u32, class: *const u16, name: *const u16, style: u32, x: i32, y: i32, width: i32, height: i32, parent: Handle, menu: Handle, instance: Handle, param: Handle) -> Handle;
        fn DestroyWindow(window: Handle) -> i32;
        fn SetWindowPos(window: Handle, after: Handle, x: i32, y: i32, width: i32, height: i32, flags: u32) -> i32;
        fn ShowWindow(window: Handle, command: i32) -> i32;
        fn GetWindowLongW(window: Handle, index: i32) -> i32;
        fn SetWindowLongW(window: Handle, index: i32, value: i32) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> Handle;
    }

    const WS_CHILD: u32 = 0x4000_0000;
    const WS_DISABLED: u32 = 0x0800_0000;
    const WS_CLIPCHILDREN: i32 = 0x0200_0000;
    const GWL_STYLE: i32 = -16;
    const SWP_NOZORDER_NOACTIVATE: u32 = 4 | 0x10;
    const SW_SHOWNA: i32 = 8;

    pub const SURFACE: &std::ffi::CStr = ash::khr::win32_surface::NAME;

    pub struct Child {
        parent: Handle,
        window: Handle,
        size: (u32, u32),
    }

    impl Child {
        /// A hidden child of the window the current OpenGL context draws to.
        pub unsafe fn new(_title: &str) -> Result<Child, String> {
            let parent = WindowFromDC(wglGetCurrentDC());
            if parent.is_null() {
                return Err("no game window".to_owned());
            }
            // a disabled window takes no input: clicks go to the game window under it
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let window = CreateWindowExW(0, class.as_ptr(), null(), WS_CHILD | WS_DISABLED, 0, 0, 1, 1, parent, null_mut(), GetModuleHandleW(null()), null_mut());
            if window.is_null() {
                return Err("could not make the window to present in".to_owned());
            }
            Ok(Child { parent, window, size: (1, 1) })
        }

        pub unsafe fn surface(&self, entry: &ash::Entry, instance: &ash::Instance) -> Result<vk::SurfaceKHR, vk::Result> {
            let info = vk::Win32SurfaceCreateInfoKHR::default().hinstance(GetModuleHandleW(null()) as isize).hwnd(self.window as isize);
            ash::khr::win32_surface::Instance::new(entry, instance).create_win32_surface(&info, None)
        }

        /// Keeps it over all of the game window. Returns that size.
        pub unsafe fn fit(&mut self) -> (u32, u32) {
            let mut rect = [0; 4];
            GetClientRect(self.parent, &mut rect);
            let size = (rect[2].max(0) as u32, rect[3].max(0) as u32);
            if size != self.size && size.0 > 0 && size.1 > 0 {
                SetWindowPos(self.window, null_mut(), 0, 0, size.0 as i32, size.1 as i32, SWP_NOZORDER_NOACTIVATE);
                self.size = size;
            }
            // OpenGL's own picture stays out from under it (the style is lost when the window goes fullscreen or back)
            let style = GetWindowLongW(self.parent, GWL_STYLE);
            if style & WS_CLIPCHILDREN == 0 {
                SetWindowLongW(self.parent, GWL_STYLE, style | WS_CLIPCHILDREN);
            }
            size
        }

        pub unsafe fn show(&self) {
            ShowWindow(self.window, SW_SHOWNA);
        }
    }

    impl Drop for Child {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.window) };
        }
    }
}

#[cfg(not(windows))]
mod native {
    use ash::vk;
    use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void, CStr};
    use std::ptr::null_mut;

    extern "C" {
        fn dlopen(name: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(library: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    type Display = *mut c_void;
    type Window = c_ulong;
    type Geometry = unsafe extern "C" fn(Display, Window, *mut Window, *mut c_int, *mut c_int, *mut c_uint, *mut c_uint, *mut c_uint, *mut c_uint) -> c_int;
    type Tree = unsafe extern "C" fn(Display, Window, *mut Window, *mut Window, *mut *mut Window, *mut c_uint) -> c_int;
    type Name = unsafe extern "C" fn(Display, Window, *mut *mut c_char) -> c_int;
    type Free = unsafe extern "C" fn(*mut c_void) -> c_int;
    type OnWindow = unsafe extern "C" fn(Display, Window) -> c_int;

    pub const SURFACE: &CStr = ash::khr::xlib_surface::NAME;

    /// A function of libX11, which is only there on machines with X.
    unsafe fn function<F: Copy>(library: *mut c_void, name: &CStr) -> Result<F, String> {
        let found = dlsym(library, name.as_ptr());
        if found.is_null() {
            return Err(format!("libX11 has no {name:?}"));
        }
        Ok(std::mem::transmute_copy(&found))
    }

    pub struct Child {
        display: Display,
        parent: Window,
        window: Window,
        size: (u32, u32),
        geometry: Geometry,
        resize: unsafe extern "C" fn(Display, Window, c_uint, c_uint) -> c_int,
        map: OnWindow,
        destroy: OnWindow,
        flush: unsafe extern "C" fn(Display) -> c_int,
        close: unsafe extern "C" fn(Display) -> c_int,
    }

    /// The window called `title` among those under `window`, a few levels down (window managers wrap windows in frames).
    // shortcut: of two games running at once, the first found. Compare _NET_WM_PID if that ever matters.
    unsafe fn find(display: Display, window: Window, title: &CStr, depth: u32, tree: Tree, name: Name, free: Free) -> Option<Window> {
        let mut text = null_mut();
        if name(display, window, &mut text) != 0 && !text.is_null() {
            let same = CStr::from_ptr(text) == title;
            free(text.cast());
            if same {
                return Some(window);
            }
        }
        let (mut root, mut parent, mut children, mut count) = (0, 0, null_mut(), 0);
        if depth == 0 || tree(display, window, &mut root, &mut parent, &mut children, &mut count) == 0 || children.is_null() {
            return None;
        }
        let found = std::slice::from_raw_parts(children, count as usize).iter().find_map(|child| find(display, *child, title, depth - 1, tree, name, free));
        free(children.cast());
        found
    }

    impl Child {
        /// A hidden child of the game's window, found by its title.
        pub unsafe fn new(title: &str) -> Result<Child, String> {
            let library = dlopen(c"libX11.so.6".as_ptr(), 2);
            if library.is_null() {
                return Err("no libX11".to_owned());
            }
            let open: unsafe extern "C" fn(*const c_char) -> Display = function(library, c"XOpenDisplay")?;
            let root: unsafe extern "C" fn(Display) -> Window = function(library, c"XDefaultRootWindow")?;
            let create: unsafe extern "C" fn(Display, Window, c_int, c_int, c_uint, c_uint, c_uint, c_ulong, c_ulong) -> Window = function(library, c"XCreateSimpleWindow")?;
            let (tree, name, free): (Tree, Name, Free) = (function(library, c"XQueryTree")?, function(library, c"XFetchName")?, function(library, c"XFree")?);
            let mut child = Child {
                display: null_mut(), parent: 0, window: 0, size: (1, 1),
                geometry: function(library, c"XGetGeometry")?, resize: function(library, c"XResizeWindow")?, map: function(library, c"XMapWindow")?,
                destroy: function(library, c"XDestroyWindow")?, flush: function(library, c"XFlush")?, close: function(library, c"XCloseDisplay")?,
            };
            // a connection of its own: the game's is not handed out
            child.display = open(std::ptr::null());
            if child.display.is_null() {
                return Err("no X display".to_owned());
            }
            let title = std::ffi::CString::new(title).map_err(|error| error.to_string())?;
            child.parent = find(child.display, root(child.display), &title, 3, tree, name, free).ok_or("the game window was not found")?;
            // no events are asked of it, so they all reach the game window
            child.window = create(child.display, child.parent, 0, 0, 1, 1, 0, 0, 0);
            Ok(child)
        }

        pub unsafe fn surface(&self, entry: &ash::Entry, instance: &ash::Instance) -> Result<vk::SurfaceKHR, vk::Result> {
            let info = vk::XlibSurfaceCreateInfoKHR::default().dpy(self.display).window(self.window);
            ash::khr::xlib_surface::Instance::new(entry, instance).create_xlib_surface(&info, None)
        }

        /// Keeps it over all of the game window. Returns that size.
        pub unsafe fn fit(&mut self) -> (u32, u32) {
            let (mut root, mut x, mut y, mut width, mut height, mut border, mut depth) = (0, 0, 0, 0, 0, 0, 0);
            if (self.geometry)(self.display, self.parent, &mut root, &mut x, &mut y, &mut width, &mut height, &mut border, &mut depth) == 0 {
                return (0, 0);
            }
            if (width, height) != self.size && width > 0 && height > 0 {
                (self.resize)(self.display, self.window, width, height);
                (self.flush)(self.display);
                self.size = (width, height);
            }
            (width, height)
        }

        pub unsafe fn show(&self) {
            (self.map)(self.display, self.window);
            (self.flush)(self.display);
        }
    }

    impl Drop for Child {
        fn drop(&mut self) {
            unsafe {
                if !self.display.is_null() {
                    if self.window != 0 {
                        (self.destroy)(self.display, self.window);
                    }
                    (self.close)(self.display);
                }
            }
        }
    }
}

/// What depends on the size of the window: the swap chain, and the frame on its way into it.
#[derive(Default)]
struct Targets {
    swapchain: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    extent: vk::Extent2D,
    /// The frame as OpenGL hands it over (bottom row first), in memory both sides can reach.
    staging: vk::Buffer,
    staging_memory: vk::DeviceMemory,
    mapped: *mut c_void,
    /// The same frame as a picture, to be copied into the swap chain the right way up.
    frame: vk::Image,
    frame_memory: vk::DeviceMemory,
    size: (u32, u32),
}

/// What does not depend on a graphics card: the window to present in, Vulkan itself, and the surface that joins them.
struct Base {
    child: native::Child,
    entry: ash::Entry,
    instance: ash::Instance,
    surfaces: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
}

impl Drop for Base {
    fn drop(&mut self) {
        unsafe {
            self.surfaces.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}

pub struct Vulkan {
    physical: vk::PhysicalDevice,
    device: ash::Device,
    swapchains: ash::khr::swapchain::Device,
    queue: vk::Queue,
    pool: vk::CommandPool,
    commands: vk::CommandBuffer,
    /// Signalled when the last frame's copies are done.
    done: vk::Fence,
    acquired: vk::Semaphore,
    copied: vk::Semaphore,
    targets: Targets,
    /// The swap chain no longer fits the window.
    stale: bool,
    shown: bool,
    /// Last: the device goes before the surface and the window it presents to.
    base: Base,
}

impl Vulkan {
    /// Everything up to the swap chain. `title` is the game window's, for finding it where the system does not say which it is.
    /// The OpenGL context must be current. Nothing shows until the first frame is presented.
    pub fn new(title: &str) -> Result<Vulkan, String> {
        unsafe {
            let entry = ash::Entry::load().map_err(|error| format!("no Vulkan on this system: {error}"))?;
            let child = native::Child::new(title)?;
            let extensions = [ash::khr::surface::NAME.as_ptr(), native::SURFACE.as_ptr()];
            let application = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_0);
            let instance = entry.create_instance(&vk::InstanceCreateInfo::default().application_info(&application).enabled_extension_names(&extensions), None).map_err(failed("instance"))?;
            // from here on whatever was made is destroyed when a later step fails
            let mut base = Base { surfaces: ash::khr::surface::Instance::new(&entry, &instance), surface: vk::SurfaceKHR::null(), child, entry, instance };
            base.surface = base.child.surface(&base.entry, &base.instance).map_err(failed("surface"))?;

            // a graphics card, and a queue of it, that can present to the window
            let mut found = None;
            for physical in base.instance.enumerate_physical_devices().map_err(failed("devices"))? {
                let families = base.instance.get_physical_device_queue_family_properties(physical);
                let fits = |(index, family): (usize, &vk::QueueFamilyProperties)| {
                    family.queue_flags.contains(vk::QueueFlags::GRAPHICS) && base.surfaces.get_physical_device_surface_support(physical, index as u32, base.surface).unwrap_or(false)
                };
                if let Some(index) = families.iter().enumerate().position(fits) {
                    found = Some((physical, index as u32));
                    break;
                }
            }
            let (physical, family) = found.ok_or("no graphics card can present to the window")?;
            let (priorities, extensions) = ([1.0], [ash::khr::swapchain::NAME.as_ptr()]);
            let queues = [vk::DeviceQueueCreateInfo::default().queue_family_index(family).queue_priorities(&priorities)];
            let device = base.instance.create_device(physical, &vk::DeviceCreateInfo::default().queue_create_infos(&queues).enabled_extension_names(&extensions), None).map_err(failed("device"))?;
            let mut vulkan = Vulkan {
                physical, swapchains: ash::khr::swapchain::Device::new(&base.instance, &device), queue: device.get_device_queue(family, 0), device, base,
                pool: vk::CommandPool::null(), commands: vk::CommandBuffer::null(), done: vk::Fence::null(), acquired: vk::Semaphore::null(), copied: vk::Semaphore::null(),
                targets: Targets::default(), stale: false, shown: false,
            };
            let pool = vk::CommandPoolCreateInfo::default().queue_family_index(family).flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
            vulkan.pool = vulkan.device.create_command_pool(&pool, None).map_err(failed("command pool"))?;
            let commands = vk::CommandBufferAllocateInfo::default().command_pool(vulkan.pool).level(vk::CommandBufferLevel::PRIMARY).command_buffer_count(1);
            vulkan.commands = vulkan.device.allocate_command_buffers(&commands).map_err(failed("command buffer"))?[0];
            vulkan.done = vulkan.device.create_fence(&vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED), None).map_err(failed("fence"))?;
            vulkan.acquired = vulkan.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None).map_err(failed("semaphore"))?;
            vulkan.copied = vulkan.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None).map_err(failed("semaphore"))?;
            let (width, height) = vulkan.base.child.fit();
            vulkan.build(width, height)?;
            Ok(vulkan)
        }
    }

    /// Memory for a buffer or picture, of a kind that has all of `wanted` (or, for want of one, any kind that fits).
    unsafe fn allocate(&self, needs: vk::MemoryRequirements, wanted: vk::MemoryPropertyFlags, strict: bool) -> Result<vk::DeviceMemory, String> {
        let kinds = self.base.instance.get_physical_device_memory_properties(self.physical);
        let fits = |index: &u32| needs.memory_type_bits & (1 << index) != 0;
        let has = |index: &u32| kinds.memory_types[*index as usize].property_flags.contains(wanted);
        let kind = (0..kinds.memory_type_count).filter(fits).find(has).or_else(|| (0..kinds.memory_type_count).find(fits).filter(|_| !strict)).ok_or("no memory of the kind needed")?;
        self.device.allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(needs.size).memory_type_index(kind), None).map_err(failed("memory"))
    }

    unsafe fn free_frame(&mut self) {
        self.device.destroy_buffer(self.targets.staging, None);
        self.device.free_memory(self.targets.staging_memory, None);
        self.device.destroy_image(self.targets.frame, None);
        self.device.free_memory(self.targets.frame_memory, None);
        (self.targets.staging, self.targets.staging_memory, self.targets.frame, self.targets.frame_memory) = Default::default();
        self.targets.mapped = std::ptr::null_mut();
    }

    /// The swap chain and the frame's memory, for a window of that size.
    unsafe fn build(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.device.device_wait_idle().map_err(failed("waiting"))?;
        self.free_frame();
        self.targets.size = (width, height);
        self.stale = false;

        let limits = self.base.surfaces.get_physical_device_surface_capabilities(self.physical, self.base.surface).map_err(failed("surface limits"))?;
        let formats = self.base.surfaces.get_physical_device_surface_formats(self.physical, self.base.surface).map_err(failed("surface formats"))?;
        // colours as they are, with no conversion on the way; a surface that only takes sRGB gets the frame declared sRGB as well
        let plain = [vk::Format::B8G8R8A8_UNORM, vk::Format::R8G8B8A8_UNORM];
        let format = *formats.iter().find(|format| plain.contains(&format.format)).or(formats.first()).ok_or("the surface takes no format")?;
        let srgb = [vk::Format::B8G8R8A8_SRGB, vk::Format::R8G8B8A8_SRGB].contains(&format.format);
        let modes = self.base.surfaces.get_physical_device_surface_present_modes(self.physical, self.base.surface).map_err(failed("present modes"))?;
        // shown at once, without waiting for the display, like the other two ways
        let mode = [vk::PresentModeKHR::IMMEDIATE, vk::PresentModeKHR::MAILBOX].into_iter().find(|mode| modes.contains(mode)).unwrap_or(vk::PresentModeKHR::FIFO);
        if !limits.supported_usage_flags.contains(vk::ImageUsageFlags::TRANSFER_DST) {
            return Err("the surface cannot be copied to".to_owned());
        }
        let extent = if limits.current_extent.width != u32::MAX {
            limits.current_extent
        } else {
            vk::Extent2D { width: width.clamp(limits.min_image_extent.width, limits.max_image_extent.width), height: height.clamp(limits.min_image_extent.height, limits.max_image_extent.height) }
        };
        let old = self.targets.swapchain;
        (self.targets.swapchain, self.targets.extent) = (vk::SwapchainKHR::null(), extent);
        self.targets.images.clear();
        if extent.width > 0 && extent.height > 0 {
            let count = if limits.max_image_count > 0 { (limits.min_image_count + 1).min(limits.max_image_count) } else { limits.min_image_count + 1 };
            let opaque = if limits.supported_composite_alpha.contains(vk::CompositeAlphaFlagsKHR::OPAQUE) { vk::CompositeAlphaFlagsKHR::OPAQUE } else { vk::CompositeAlphaFlagsKHR::INHERIT };
            let info = vk::SwapchainCreateInfoKHR::default()
                .surface(self.base.surface).min_image_count(count).image_format(format.format).image_color_space(format.color_space).image_extent(extent).image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::COLOR_ATTACHMENT).image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(limits.current_transform).composite_alpha(opaque).present_mode(mode).clipped(true).old_swapchain(old);
            let made = self.swapchains.create_swapchain(&info, None);
            self.swapchains.destroy_swapchain(old, None);
            self.targets.swapchain = made.map_err(failed("swap chain"))?;
            self.targets.images = self.swapchains.get_swapchain_images(self.targets.swapchain).map_err(failed("swap chain pictures"))?;
        } else {
            self.swapchains.destroy_swapchain(old, None);
        }
        if width == 0 || height == 0 {
            return Ok(());
        }

        let buffer = vk::BufferCreateInfo::default().size(width as u64 * height as u64 * 4).usage(vk::BufferUsageFlags::TRANSFER_SRC).sharing_mode(vk::SharingMode::EXCLUSIVE);
        self.targets.staging = self.device.create_buffer(&buffer, None).map_err(failed("frame buffer"))?;
        let needs = self.device.get_buffer_memory_requirements(self.targets.staging);
        self.targets.staging_memory = self.allocate(needs, vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT, true)?;
        self.device.bind_buffer_memory(self.targets.staging, self.targets.staging_memory, 0).map_err(failed("frame buffer memory"))?;
        self.targets.mapped = self.device.map_memory(self.targets.staging_memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty()).map_err(failed("reaching the frame's memory"))?;

        let picture = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D).format(if srgb { vk::Format::R8G8B8A8_SRGB } else { vk::Format::R8G8B8A8_UNORM })
            .extent(vk::Extent3D { width, height, depth: 1 }).mip_levels(1).array_layers(1).samples(vk::SampleCountFlags::TYPE_1).tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC).sharing_mode(vk::SharingMode::EXCLUSIVE).initial_layout(vk::ImageLayout::UNDEFINED);
        self.targets.frame = self.device.create_image(&picture, None).map_err(failed("frame picture"))?;
        let needs = self.device.get_image_memory_requirements(self.targets.frame);
        self.targets.frame_memory = self.allocate(needs, vk::MemoryPropertyFlags::DEVICE_LOCAL, false)?;
        self.device.bind_image_memory(self.targets.frame, self.targets.frame_memory, 0).map_err(failed("frame picture memory"))
    }

    /// Shows the frame OpenGL has just finished. An error means this way of presenting is over: drop it.
    pub fn present(&mut self) -> Result<(), String> {
        unsafe {
            let (width, height) = self.base.child.fit();
            if width == 0 || height == 0 {
                return Ok(()); // minimized
            }
            if (width, height) != self.targets.size || self.stale {
                self.build(width, height)?;
            }
            if self.targets.swapchain == vk::SwapchainKHR::null() {
                return Ok(());
            }
            // the last frame's copy out of the shared memory is over before this one is written into it
            self.device.wait_for_fences(&[self.done], true, u64::MAX).map_err(failed("waiting"))?;
            let mut bound = 0;
            gl::glGetIntegerv(gl::GL_FRAMEBUFFER_BINDING, &mut bound);
            gl::glBindFramebuffer(gl::GL_FRAMEBUFFER, 0);
            gl::glReadPixels(0, 0, width as i32, height as i32, gl::GL_RGBA, gl::GL_UNSIGNED_BYTE, self.targets.mapped);
            gl::glBindFramebuffer(gl::GL_FRAMEBUFFER, bound as u32);

            let index = match self.swapchains.acquire_next_image(self.targets.swapchain, u64::MAX, self.acquired, vk::Fence::null()) {
                Ok((index, _)) => index,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.stale = true;
                    return Ok(());
                }
                Err(error) => return Err(format!("next picture: {error}")),
            };
            let (commands, frame, screen) = (self.commands, self.targets.frame, self.targets.images[index as usize]);
            self.device.reset_command_buffer(commands, vk::CommandBufferResetFlags::empty()).map_err(failed("commands"))?;
            self.device.begin_command_buffer(commands, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)).map_err(failed("commands"))?;
            let whole = vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 };
            let layer = vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 };
            // a picture is told what is done with it next
            let turn = |image: vk::Image, from: vk::ImageLayout, to: vk::ImageLayout, before: vk::AccessFlags, after: vk::AccessFlags| {
                let change = vk::ImageMemoryBarrier::default()
                    .old_layout(from).new_layout(to).src_access_mask(before).dst_access_mask(after)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED).dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED).image(image).subresource_range(whole);
                let then = if to == vk::ImageLayout::PRESENT_SRC_KHR { vk::PipelineStageFlags::BOTTOM_OF_PIPE } else { vk::PipelineStageFlags::TRANSFER };
                self.device.cmd_pipeline_barrier(commands, vk::PipelineStageFlags::TRANSFER, then, vk::DependencyFlags::empty(), &[], &[], &[change]);
            };
            use vk::{AccessFlags as Access, ImageLayout as Layout};
            turn(frame, Layout::UNDEFINED, Layout::TRANSFER_DST_OPTIMAL, Access::empty(), Access::TRANSFER_WRITE);
            let copy = vk::BufferImageCopy::default().image_subresource(layer).image_extent(vk::Extent3D { width, height, depth: 1 });
            self.device.cmd_copy_buffer_to_image(commands, self.targets.staging, frame, Layout::TRANSFER_DST_OPTIMAL, &[copy]);
            turn(frame, Layout::TRANSFER_DST_OPTIMAL, Layout::TRANSFER_SRC_OPTIMAL, Access::TRANSFER_WRITE, Access::TRANSFER_READ);
            turn(screen, Layout::UNDEFINED, Layout::TRANSFER_DST_OPTIMAL, Access::empty(), Access::TRANSFER_WRITE);
            // OpenGL's first row is the bottom one, Vulkan's the top one: copied upside down
            let corner = |x: u32, y: u32, z: i32| vk::Offset3D { x: x as i32, y: y as i32, z };
            let flip = vk::ImageBlit::default()
                .src_subresource(layer).src_offsets([corner(0, height, 0), corner(width, 0, 1)])
                .dst_subresource(layer).dst_offsets([corner(0, 0, 0), corner(self.targets.extent.width, self.targets.extent.height, 1)]);
            self.device.cmd_blit_image(commands, frame, Layout::TRANSFER_SRC_OPTIMAL, screen, Layout::TRANSFER_DST_OPTIMAL, &[flip], vk::Filter::NEAREST);
            turn(screen, Layout::TRANSFER_DST_OPTIMAL, Layout::PRESENT_SRC_KHR, Access::TRANSFER_WRITE, Access::empty());
            self.device.end_command_buffer(commands).map_err(failed("commands"))?;

            self.device.reset_fences(&[self.done]).map_err(failed("fence"))?;
            let (wait, stage, run, signal) = ([self.acquired], [vk::PipelineStageFlags::TRANSFER], [commands], [self.copied]);
            let work = vk::SubmitInfo::default().wait_semaphores(&wait).wait_dst_stage_mask(&stage).command_buffers(&run).signal_semaphores(&signal);
            self.device.queue_submit(self.queue, &[work], self.done).map_err(failed("copying the frame"))?;
            let (chains, indices) = ([self.targets.swapchain], [index]);
            match self.swapchains.queue_present(self.queue, &vk::PresentInfoKHR::default().wait_semaphores(&signal).swapchains(&chains).image_indices(&indices)) {
                Ok(unfit) => self.stale |= unfit,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.stale = true,
                Err(error) => return Err(format!("presenting: {error}")),
            }
            if !self.shown {
                // only now is there something to show: until here the game window showed OpenGL's own picture
                self.base.child.show();
                self.shown = true;
            }
            Ok(())
        }
    }
}

impl Drop for Vulkan {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.free_frame();
            self.swapchains.destroy_swapchain(self.targets.swapchain, None);
            self.device.destroy_semaphore(self.acquired, None);
            self.device.destroy_semaphore(self.copied, None);
            self.device.destroy_fence(self.done, None);
            self.device.destroy_command_pool(self.pool, None);
            self.device.destroy_device(None);
        }
    }
}
