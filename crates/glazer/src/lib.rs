#![no_std]
extern crate alloc;

#[cfg(feature = "opengl")]
pub extern crate gl;

mod reloading;

#[cfg(all(target_os = "macos", not(feature = "generic")))]
mod appkit;
#[cfg(all(target_os = "macos", not(feature = "generic")))]
use appkit as platform;

#[cfg(feature = "generic")]
mod generic;
#[cfg(feature = "generic")]
use generic as platform;

#[cfg(all(not(target_os = "macos"), not(feature = "generic")))]
mod unsupported;
#[cfg(all(not(target_os = "macos"), not(feature = "generic")))]
use unsupported as platform;

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
#[cfg_attr(feature = "generic", cfg(feature = "glutin"))]
pub fn run_opengl<Memory>(
    memory: Memory,
    width: usize,
    height: usize,
    handle_input: fn(PlatformInput<Memory>),
    update_and_render: fn(PlatformUpdateGL<Memory>),
    initialize_opengl: fn(&dyn Fn(&'static str) -> *const core::ffi::c_void),
    shared_lib_path: Option<&str>,
) where
    Memory: 'static + Send,
{
    platform::run_opengl(
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
#[cfg_attr(feature = "generic", cfg(feature = "glutin"))]
#[derive(Debug)]
pub struct PlatformUpdateGL<'a, T> {
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
    pub input: Input,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    Key {
        code: KeyCode,
        modifiers: KeyModifiers,
        pressed: bool,
        repeat: bool,
    },
    MouseMoved {
        dx: f32,
        dy: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,

    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,

    Backslash,
    CloseBracket,
    Comma,
    EqualSign,
    Hyphen,
    NonUSBackslash,
    NonUSPound,
    OpenBracket,
    Period,
    Quote,
    Semicolon,
    Separator,
    Slash,
    Spacebar,

    CapsLock,
    LeftAlt,
    LeftControl,
    LeftShift,
    LockingCapsLock,
    LockingNumLock,
    LockingScrollLock,
    RightAlt,
    RightControl,
    RightShift,
    ScrollLock,

    LeftArrow,
    RightArrow,
    UpArrow,
    DownArrow,
    PageUp,
    PageDown,
    Home,
    End,
    DeleteForward,
    DeleteOrBackspace,
    Escape,
    Insert,
    Return,
    Tab,

    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyModifiers(pub u8);

impl KeyModifiers {
    pub const CLEAR: Self = Self(0);
    pub const CAPSLOCK: Self = Self(1);
    pub const SHIFT: Self = Self(1 << 1);
    pub const CONTROL: Self = Self(1 << 2);
    pub const OPTION: Self = Self(1 << 3);
    pub const COMMAND: Self = Self(1 << 4);
    pub const NUMERIC_PAD: Self = Self(1 << 5);
    pub const HELP: Self = Self(1 << 6);
    pub const FUNCTION: Self = Self(1 << 7);
}

impl core::ops::BitOr for KeyModifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitAnd for KeyModifiers {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

// Debug utility

pub use platform::{debug_time_micros, debug_time_millis, debug_time_nanos, debug_time_secs};

#[macro_export]
macro_rules! log {
    () => {
        $crate::__log("\n")
    };
    ($($arg:tt)*) => {{
        $crate::__log(&alloc::format!($($arg)*));
        $crate::__log("\n")
    }};
}

#[inline]
#[doc(hidden)]
pub fn __log(str: &str) {
    platform::log(str);
}

/// Automatically generate a path to the crate's dynamic library in `target/debug`.
///
/// Returns `None` if `debug_assertions` are disabled.
pub fn debug_target() -> Option<&'static str> {
    #[cfg(not(debug_assertions))]
    {
        None
    }

    #[cfg(debug_assertions)]
    {
        extern crate std;
        extern crate alloc;

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
