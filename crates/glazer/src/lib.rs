#![no_std]
extern crate alloc;

#[cfg(feature = "opengl")]
pub extern crate glow;
#[cfg(feature = "software")]
pub extern crate tint;
pub extern crate winit;

mod callback;
mod platform;
mod time;

pub fn run<Memory>(
    memory: Memory,
    width: usize,
    height: usize,
    handle_input: fn(PlatformInput<Memory>),
    update_and_render: fn(PlatformUpdate<Memory>),
    shared_lib_path: Option<&str>,
) -> !
where
    Memory: 'static + Send,
{
    platform::run(
        memory,
        width,
        height,
        handle_input,
        update_and_render,
        shared_lib_path,
    );
    // NOTE: Some platforms never return and this communicates that clearly.
    extern crate std;
    std::process::exit(0);
}

pub struct PlatformUpdate<'a, T> {
    // logic
    pub memory: &'a mut T,
    pub delta: f32,

    // graphics
    #[cfg(feature = "opengl")]
    pub gl: &'a glow::Context,
    #[cfg(feature = "software")]
    pub frame_buffer: &'a mut [tint::Srgb],
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
        #[cfg(all(debug_assertions, any(target_os = "linux", target_os = "macos")))]
        {
            extern crate std;

            #[cfg(target_os = "linux")]
            let extension = "so";
            #[cfg(target_os = "macos")]
            let extension = "dylib";

            let name = env!("CARGO_CRATE_NAME");
            let path = alloc::format!("target/debug/lib{}.{}", name, extension);
            return match std::fs::exists(&path) {
                Ok(_) => Some(std::string::String::leak(path)),
                Err(err) => panic!("failed to load {path}: {err}"),
            };
        }
        #[allow(unused)]
        None
    }

    #[macro_export]
    macro_rules! log {
        () => {
            $crate::__log("\n")
        };
        ($($arg:tt)*) => {{
            extern crate alloc;
            #[cfg(not(target_arch = "wasm32"))]
            {
                $crate::debug::__log(&alloc::format!($($arg)*));
                $crate::debug::__log("\n")
            }
            #[cfg(target_arch = "wasm32")]
            {
                $crate::debug::__log(&alloc::format!($($arg)*));
            }
        }};
    }

    #[inline]
    #[doc(hidden)]
    pub fn __log(str: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        std::print!("{str}");
        #[cfg(target_arch = "wasm32")]
        web_sys::console::info_1(&str.into());
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
