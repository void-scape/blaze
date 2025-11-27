#![no_std]
extern crate alloc;

#[cfg(feature = "opengl")]
pub extern crate gl;
pub extern crate winit;

mod callback;
mod platform;

#[cfg(feature = "software")]
#[macro_export]
macro_rules! static_frame_buffer {
    ($width:expr, $height:expr, $type:ty, $init:expr) => {{
        static mut FRAME_BUFFER: [$type; $width * $height] = [$init; $width * $height];
        // ## Safety
        //
        // `FRAME_BUFFER` is locally scoped to this macro invocation. There cannot
        // exist any other mutable references to `FRAME_BUFFER` with safe Rust code.
        unsafe {
            #[allow(static_mut_refs)]
            &mut FRAME_BUFFER
        }
    }};
}

#[cfg(feature = "software")]
pub fn run<Memory, Pixels>(
    memory: Memory,
    frame_buffer: &mut [Pixels],
    width: usize,
    height: usize,
    handle_input: fn(PlatformInput<Memory>),
    update_and_render: fn(PlatformUpdate<Memory, Pixels>),
    shared_lib_path: Option<&str>,
) where
    Pixels: 'static,
    Memory: 'static + Send,
{
    assert!(
        core::mem::size_of::<Pixels>() == 4,
        "`Pixels` must be 4 bytes"
    );
    platform::run(
        memory,
        frame_buffer,
        width,
        height,
        handle_input,
        update_and_render,
        shared_lib_path,
    );
}

#[cfg(feature = "software")]
#[derive(Debug)]
pub struct PlatformUpdate<'a, T, Pixels> {
    // logic
    pub memory: &'a mut T,
    pub delta: f32,

    // graphics
    pub frame_buffer: &'a mut [Pixels],
    pub width: usize,
    pub height: usize,

    // audio
    pub samples: &'a mut [f32],
    pub sample_rate: u32,
    pub channels: usize,

    // debug
    pub reloaded: bool,
}

#[cfg(feature = "opengl")]
pub fn run_opengl<Memory>(
    memory: Memory,
    width: usize,
    height: usize,
    handle_input: fn(PlatformInput<Memory>),
    update_and_render: fn(PlatformUpdate<Memory>),
    initialize_opengl: fn(&dyn Fn(&'static str) -> *const core::ffi::c_void),
    shared_lib_path: Option<&str>,
) where
    Memory: 'static + Send,
{
    platform::run(
        memory,
        width,
        height,
        handle_input,
        update_and_render,
        initialize_opengl,
        shared_lib_path,
    );
}

#[cfg(feature = "opengl")]
#[derive(Debug)]
pub struct PlatformUpdate<'a, T> {
    // logic
    pub memory: &'a mut T,
    pub delta: f32,

    // graphics
    pub width: usize,
    pub height: usize,

    // audio
    pub samples: &'a mut [f32],
    pub sample_rate: u32,
    pub channels: usize,

    // debug
    pub reloaded: bool,
}

#[derive(Debug)]
pub struct PlatformInput<'a, T> {
    pub memory: &'a mut T,
    pub input: winit::event::WindowEvent,
}

// Debug utility

pub use debug::{
    debug_target, debug_time_micros, debug_time_millis, debug_time_nanos, debug_time_secs,
};

pub mod debug {
    extern crate std;

    /// Automatically generate a path to the crate's dynamic library in `target/debug`.
    ///
    /// Returns `None` if `debug_assertions` are disabled.
    pub fn debug_target() -> Option<&'static str> {
        #[cfg(not(debug_assertions))]
        {
            None
        }
        #[cfg(all(debug_assertions, any(target_os = "linux", target_os = "macos")))]
        {
            extern crate std;

            #[cfg(target_os = "linux")]
            let extension = "so";
            #[cfg(target_os = "macos")]
            let extension = "dylib";

            let name = env!("CARGO_CRATE_NAME");
            let path = alloc::format!("target/debug/lib{}.{}", name, extension);
            match std::fs::exists(&path) {
                Ok(_) => Some(std::string::String::leak(path)),
                Err(err) => panic!("failed to load {path}: {err}"),
            }
        }
    }

    #[macro_export]
    macro_rules! log {
        () => {
            $crate::__log("\n")
        };
        ($($arg:tt)*) => {{
            $crate::debug::__log(&alloc::format!($($arg)*));
            $crate::debug::__log("\n")
        }};
    }

    #[inline]
    #[doc(hidden)]
    pub fn __log(str: &str) {
        std::print!("{str}");
    }

    pub fn debug_time_secs<R>(mut f: impl FnMut() -> R) -> (f32, R) {
        let start = std::time::Instant::now();
        let result = f();
        let duration = std::time::Instant::now()
            .duration_since(start)
            .as_secs_f32();
        (duration, result)
    }

    pub fn debug_time_millis<R>(mut f: impl FnMut() -> R) -> (u128, R) {
        let start = std::time::Instant::now();
        let result = f();
        let duration = std::time::Instant::now().duration_since(start).as_millis();
        (duration, result)
    }

    pub fn debug_time_micros<R>(mut f: impl FnMut() -> R) -> (u128, R) {
        let start = std::time::Instant::now();
        let result = f();
        let duration = std::time::Instant::now().duration_since(start).as_micros();
        (duration, result)
    }

    pub fn debug_time_nanos<R>(mut f: impl FnMut() -> R) -> (u128, R) {
        let start = std::time::Instant::now();
        let result = f();
        let duration = std::time::Instant::now().duration_since(start).as_nanos();
        (duration, result)
    }
}
