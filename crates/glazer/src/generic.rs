extern crate std;

use alloc::collections::vec_deque::VecDeque;
use alloc::sync::Arc;
use core::marker::PhantomData;
use core::num::NonZeroU32;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use softbuffer::{Context, Surface};
use std::sync::Mutex;
use std::time::SystemTime;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

pub fn run<Memory, Pixels>(
    mem: Memory,
    frame_buffer: &mut [Pixels],
    width: usize,
    height: usize,
    handle_input: fn(crate::PlatformInput<Memory>),
    update_and_render: fn(crate::PlatformUpdate<Memory, Pixels>),
    reload: Option<&str>,
) where
    Pixels: 'static,
    Memory: 'static + Send,
{
    assert_eq!(core::mem::size_of::<Pixels>(), 4);
    assert!(frame_buffer.len() >= width * height);

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("No output device available")
        .unwrap();

    let config = device.default_output_config().unwrap();
    let channels = config.channels() as usize;
    let sample_rate = config.sample_rate().0;
    let sample_buffer = Arc::new(Mutex::new(VecDeque::new()));

    let stream = device
        .build_output_stream(
            &config.into(),
            {
                let sample_buffer = sample_buffer.clone();
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut buffer = sample_buffer.lock().unwrap();
                    let len = data.len();
                    let buffer_len = buffer.len();
                    for (out, input) in data.iter_mut().zip(buffer.drain(0..len.min(buffer_len))) {
                        *out = input;
                    }
                }
            },
            |err| std::eprintln!("audio thread error: {}", err),
            None,
        )
        .unwrap();

    stream.play().unwrap();

    let event_loop = EventLoop::new().unwrap();
    let frame_buffer = frame_buffer.as_mut_ptr().cast();
    event_loop
        .run_app(&mut App {
            window: None,
            gfx: None,
            width,
            height,
            mem,
            frame_buffer,
            fns: FnPtrs::new(handle_input, update_and_render, reload),
            sample_buffer,
            sample_rate,
            channels,
            now: SystemTime::now(),
            _pixels: PhantomData,
        })
        .unwrap();
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
    panic!("opengl not supported on this platform");
}

type SampleBuffer = Arc<Mutex<VecDeque<f32>>>;

struct App<Memory, Pixels> {
    window: Option<Window>,
    gfx: Option<Gfx>,
    width: usize,
    height: usize,
    mem: Memory,
    frame_buffer: *mut u32,
    fns: FnPtrs<Memory, Pixels>,
    sample_buffer: SampleBuffer,
    sample_rate: u32,
    channels: usize,
    now: SystemTime,
    _pixels: PhantomData<Pixels>,
}

struct Gfx {
    _ctx: Context<&'static Window>,
    surface: Surface<&'static Window, &'static Window>,
}

impl<Memory, Pixels> ApplicationHandler for App<Memory, Pixels> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attributes = WindowAttributes::default();
        self.window = match event_loop.create_window(window_attributes) {
            Ok(window) => {
                let failed = window
                    .request_inner_size(PhysicalSize::new(self.width as u32, self.height as u32));
                assert!(
                    failed.is_none(),
                    "platform does not support resizing the window"
                );
                Some(window)
            }
            Err(err) => {
                std::eprintln!("error creating window: {err}");
                event_loop.exit();
                return;
            }
        };

        let ctx = Context::new(unsafe {
            core::mem::transmute::<&'_ Window, &'static Window>(self.window.as_ref().unwrap())
        })
        .expect("Failed to create a softbuffer context");
        let surface = Surface::new(&ctx, unsafe {
            core::mem::transmute::<&'_ Window, &'static Window>(self.window.as_ref().unwrap())
        })
        .expect("Failed to create a softbuffer surface");
        self.gfx = Some(Gfx { _ctx: ctx, surface });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(gfx) = self.gfx.as_mut()
                    && let (Some(w), Some(h)) =
                        (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    gfx.surface
                        .resize(w, h)
                        .expect("Failed to resize the softbuffer surface");
                }
                self.window
                    .as_ref()
                    .expect("resize event without a window")
                    .request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let reloaded = load_game_dylib(&mut self.fns);

                let window = self
                    .window
                    .as_ref()
                    .expect("redraw request without a window");
                window.request_redraw();

                let delta = {
                    let now = SystemTime::now();
                    let delta = now
                        .duration_since(self.now)
                        .unwrap_or_default()
                        .as_secs_f32();
                    self.now = now;
                    delta
                };

                const HEAD: usize = 1024 * 5;
                let mut stack_sample_buffer = [0.0; HEAD];
                let samples = {
                    let sample_buffer = self.sample_buffer.lock().unwrap();
                    let len = sample_buffer.len();
                    if len < HEAD {
                        stack_sample_buffer.as_mut_slice()
                    } else {
                        &mut []
                    }
                };

                (self.fns.update_and_render)(crate::PlatformUpdate {
                    memory: &mut self.mem,
                    delta,
                    frame_buffer: unsafe {
                        core::slice::from_raw_parts_mut(
                            self.frame_buffer.cast(),
                            self.width * self.height,
                        )
                    },
                    width: self.width,
                    height: self.height,
                    samples,
                    sample_rate: self.sample_rate,
                    channels: self.channels,
                    reloaded,
                });

                if !samples.is_empty() {
                    let mut sample_buffer = self.sample_buffer.lock().unwrap();
                    sample_buffer.extend(samples.iter());
                }

                if let Some(gfx) = self.gfx.as_mut() {
                    // Notify that you're about to draw.
                    window.pre_present_notify();

                    let mut buffer = gfx
                        .surface
                        .buffer_mut()
                        .expect("Failed to get the softbuffer buffer");
                    let display = buffer.as_mut_ptr();
                    unsafe {
                        display
                            .copy_from_nonoverlapping(self.frame_buffer, self.width * self.height);
                    }
                    buffer
                        .present()
                        .expect("Failed to present the softbuffer buffer");
                }
            }
            _ => (),
        }
    }
}

#[inline]
pub fn log(str: &str) {
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

struct FnPtrs<Memory, Pixels> {
    dylib: *mut core::ffi::c_void,
    path: Option<alloc::string::String>,
    loaded: SystemTime,
    handle_input: fn(crate::PlatformInput<Memory>),
    update_and_render: fn(crate::PlatformUpdate<Memory, Pixels>),
}

impl<Memory, Pixels> FnPtrs<Memory, Pixels> {
    pub fn new(
        handle_input: fn(crate::PlatformInput<Memory>),
        update_and_render: fn(crate::PlatformUpdate<Memory, Pixels>),
        path: Option<&str>,
    ) -> Self {
        use alloc::string::ToString;
        Self {
            dylib: core::ptr::null_mut(),
            path: path.map(|inner| inner.to_string()),
            loaded: SystemTime::now(),
            handle_input,
            update_and_render,
        }
    }
}

fn load_game_dylib<Memory, Pixels>(ptrs: &mut FnPtrs<Memory, Pixels>) -> bool {
    use alloc::ffi::CString;

    let Some(path) = ptrs.path.as_deref() else {
        return false;
    };
    let Some(modified) = std::fs::metadata(path).ok().and_then(|meta| {
        meta.modified().ok().and_then(|modified| {
            modified
                .duration_since(ptrs.loaded)
                .is_ok_and(|dur| !dur.is_zero())
                .then_some(modified)
        })
    }) else {
        return false;
    };

    if !ptrs.dylib.is_null() {
        // NOTE: This does nothing on macos.
        debug_assert_eq!(unsafe { libc::dlclose(ptrs.dylib) }, 0);
    }
    ptrs.loaded = modified;

    crate::log!("loading game functions from `{path}`");
    let mut copy = std::path::PathBuf::from(path);
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    copy.pop();
    copy.push(alloc::format!("{}", time.as_millis()));
    std::fs::copy(path, &copy).expect("failed to copy dylib");
    let filename = CString::new(copy.to_str().unwrap()).expect("invalid dylib string");

    let dylib = unsafe { libc::dlopen(filename.as_ptr(), libc::RTLD_LOCAL | libc::RTLD_LAZY) };
    if !dylib.is_null() {
        let symbol = unsafe { libc::dlsym(dylib, c"update_and_render".as_ptr().cast()) };
        if !symbol.is_null() {
            let update_and_render = unsafe {
                std::mem::transmute::<
                    *mut std::ffi::c_void,
                    fn(crate::PlatformUpdate<Memory, Pixels>),
                >(symbol)
            };
            ptrs.update_and_render = update_and_render;

            let symbol = unsafe { libc::dlsym(dylib, c"handle_input".as_ptr().cast()) };
            if !symbol.is_null() {
                let handle_input = unsafe {
                    std::mem::transmute::<*mut std::ffi::c_void, fn(crate::PlatformInput<Memory>)>(
                        symbol,
                    )
                };
                ptrs.handle_input = handle_input;
            } else {
                err("failed to dynamically load symbol `handle_input`");
            }
        } else {
            err("failed to dynamically load symbol `update_and_render`");
        }
    } else {
        err(&alloc::format!("failed to load dylib `{path}`"));
    }

    fn err(msg: &str) {
        let str = unsafe { core::ffi::CStr::from_ptr(libc::dlerror()) };
        crate::log!("ERROR: {}: {}", msg, str.to_str().unwrap());
    }

    true
}
