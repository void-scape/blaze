pub fn run<Memory, Pixels>(
    _: Memory,
    _: &mut [Pixels],
    _: usize,
    _: usize,
    _: fn(crate::PlatformInput<Memory>),
    _: fn(crate::PlatformUpdate<Memory, Pixels>),
    _: Option<&str>,
) where
    Pixels: 'static,
    Memory: 'static,
{
    panic!("platform not supported");
}

#[cfg(feature = "opengl")]
pub fn run_opengl<Memory>(
    memory: Memory,
    width: usize,
    height: usize,
    handle_input: fn(crate::PlatformInput<Memory>),
    update_and_render: fn(crate::PlatformUpdateGL<Memory>),
    shared_lib_path: Option<&str>,
) where
    Memory: 'static,
{
    panic!("platform not supported");
}

pub fn log(_: &str) {
    panic!("platform not supported");
}

pub fn debug_time_secs<R>(_: impl FnMut() -> R) -> (f32, R) {
    panic!("platform not supported");
}

pub fn debug_time_millis<R>(_: impl FnMut() -> R) -> (u128, R) {
    panic!("platform not supported");
}

pub fn debug_time_micros<R>(_: impl FnMut() -> R) -> (u128, R) {
    panic!("platform not supported");
}

pub fn debug_time_nanos<R>(_: impl FnMut() -> R) -> (u128, R) {
    panic!("platform not supported");
}
