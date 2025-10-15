use crate::{PlatformInput, PlatformUpdate};

pub fn run<Memory, Pixels>(
    _memory: Memory,
    _frame_buffer: &mut [Pixels],
    _width: usize,
    _height: usize,
    _handle_input: fn(PlatformInput<Memory>),
    _update_and_render: fn(PlatformUpdate<Memory, Pixels>),
    _shared_lib_path: Option<&str>,
) where
    Pixels: 'static,
    Memory: 'static,
{
    panic!("platform not supported");
}

pub fn log(str: &str) {
    panic!("platform not supported");
}

pub fn debug_time_secs<R>(f: impl FnMut() -> R) -> (f32, R) {
    panic!("platform not supported");
}

pub fn debug_time_millis<R>(f: impl FnMut() -> R) -> (u128, R) {
    panic!("platform not supported");
}

pub fn debug_time_micros<R>(f: impl FnMut() -> R) -> (u128, R) {
    panic!("platform not supported");
}

pub fn debug_time_nanos<R>(f: impl FnMut() -> R) -> (u128, R) {
    panic!("platform not supported");
}
