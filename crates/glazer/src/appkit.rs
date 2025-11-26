#![allow(clippy::type_complexity)]
#![allow(unused_unsafe)]
// OpenGL on macos is deprecated
#![allow(deprecated)]

extern crate std;

use std::boxed::Box;
use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::{NonNull, null_mut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use std::{dbg, format};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate,
    NSApplicationTerminateReply, NSBackingStoreType, NSBitmapImageRep, NSColorSpaceName, NSEvent,
    NSEventModifierFlags, NSImage, NSView, NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_audio_toolbox::{
    AURenderCallbackStruct, AudioComponentDescription, AudioComponentFindNext,
    AudioComponentInstance, AudioComponentInstanceNew, AudioOutputUnitStart, AudioOutputUnitStop,
    AudioUnitInitialize, AudioUnitRenderActionFlags, AudioUnitSetProperty,
    kAudioUnitManufacturer_Apple, kAudioUnitProperty_SetRenderCallback,
    kAudioUnitProperty_StreamFormat, kAudioUnitScope_Global, kAudioUnitScope_Input,
    kAudioUnitSubType_DefaultOutput, kAudioUnitType_Output,
};
use objc2_core_audio_types::{
    AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp, kAudioFormatLinearPCM,
    kLinearPCMFormatFlagIsFloat,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSTimer, ns_string,
};

#[cfg(feature = "opengl")]
use crate::PlatformUpdateGL;
use crate::{Input, KeyCode, KeyModifiers, PlatformInput, PlatformUpdate};

enum PlatformRequest<'a> {
    Update(PlatformState<'a>),
    Input(Input),
}

struct PlatformState<'a> {
    delta: f32,
    //
    frame_buffer: *mut u8,
    width: usize,
    height: usize,
    //
    samples: &'a mut [f32],
    channels: usize,
    sample_rate: u32,
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
    Memory: 'static,
{
    if let Some(shared_lib_path) = shared_lib_path {
        software_renderer::run_dynamic(memory, frame_buffer, width, height, shared_lib_path);
    } else {
        software_renderer::run_static(
            memory,
            frame_buffer,
            width,
            height,
            handle_input,
            update_and_render,
        );
    }
}

mod software_renderer {
    use super::*;

    pub fn run_static<Memory, Pixels>(
        mut memory: Memory,
        frame_buffer: &mut [Pixels],
        width: usize,
        height: usize,
        handle_input: fn(PlatformInput<Memory>),
        update_and_render: fn(PlatformUpdate<Memory, Pixels>),
    ) where
        Pixels: 'static,
        Memory: 'static,
    {
        let pixels_len = frame_buffer.len();
        let update = move |req: PlatformRequest| {
            match req {
                PlatformRequest::Update(state) => {
                    debug_assert!(pixels_len >= state.width * state.height);
                    update_and_render(PlatformUpdate {
                        memory: &mut memory,
                        delta: state.delta,
                        //
                        frame_buffer: unsafe {
                            core::slice::from_raw_parts_mut(
                                state.frame_buffer as *mut _,
                                state.width * state.height,
                            )
                        },
                        width: state.width,
                        height: state.height,
                        //
                        samples: state.samples,
                        sample_rate: state.sample_rate,
                        channels: state.channels,
                        //
                        reloaded: false,
                    })
                }
                PlatformRequest::Input(input) => handle_input(PlatformInput {
                    memory: &mut memory,
                    input,
                }),
            }
        };
        run_app(frame_buffer.as_mut_ptr() as *mut u8, width, height, update);
    }

    pub fn run_dynamic<Memory, Pixels>(
        mut memory: Memory,
        frame_buffer: &mut [Pixels],
        width: usize,
        height: usize,
        shared_lib_path: &str,
    ) where
        Pixels: 'static,
        Memory: 'static,
    {
        use alloc::string::ToString;

        let shared_lib_path = shared_lib_path.to_string();
        let mut functions =
            load_game_dylib::<Memory, Pixels>(&shared_lib_path).expect("failed to load game dylib");
        let mut loaded_instant = std::time::SystemTime::now();
        let mut reloaded = false;

        let pixels_len = frame_buffer.len();
        let update = move |req: PlatformRequest| {
            if let Some(modified) = std::fs::metadata(&shared_lib_path).ok().and_then(|meta| {
                meta.modified().ok().and_then(|modified| {
                    modified
                        .duration_since(loaded_instant)
                        .is_ok_and(|dur| !dur.is_zero())
                        .then_some(modified)
                })
            }) {
                // NOTE: This does nothing, macos never closes dylibs.
                debug_assert_eq!(unsafe { libc::dlclose(functions.dylib) }, 0);
                functions = load_game_dylib::<Memory, Pixels>(&shared_lib_path)
                    .expect("failed to load game dylib");
                loaded_instant = modified;
                reloaded = true;
            }

            match req {
                PlatformRequest::Update(state) => {
                    debug_assert!(pixels_len >= state.width * state.height);
                    (functions.update_and_render)(PlatformUpdate {
                        memory: &mut memory,
                        delta: state.delta,
                        //
                        frame_buffer: unsafe {
                            core::slice::from_raw_parts_mut(
                                state.frame_buffer as *mut _,
                                state.width * state.height,
                            )
                        },
                        width: state.width,
                        height: state.height,
                        //
                        samples: state.samples,
                        sample_rate: state.sample_rate,
                        channels: state.channels,
                        //
                        reloaded,
                    });
                    reloaded = false;
                }
                PlatformRequest::Input(input) => (functions.handle_input)(PlatformInput {
                    memory: &mut memory,
                    input,
                }),
            }
        };
        run_app(frame_buffer.as_mut_ptr() as *mut u8, width, height, update);
    }

    struct LoadedGameFunctions<Memory, Pixels> {
        dylib: *mut c_void,
        handle_input: fn(PlatformInput<Memory>),
        update_and_render: fn(PlatformUpdate<Memory, Pixels>),
    }

    fn load_game_dylib<Memory, Pixels>(path: &str) -> Option<LoadedGameFunctions<Memory, Pixels>> {
        use alloc::ffi::CString;
        use core::ffi::CStr;

        crate::log!("loading game functions from `{path}`");

        let mut copy = std::path::PathBuf::from(path);
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        copy.pop();
        copy.push(format!("{}", time.as_millis()));
        std::fs::copy(path, &copy).expect("failed to copy dylib");

        let filename = CString::new(copy.to_str().unwrap()).expect("invalid dylib string");
        let dylib = unsafe { libc::dlopen(filename.as_ptr(), libc::RTLD_LOCAL | libc::RTLD_LAZY) };
        if !dylib.is_null() {
            let symbol = unsafe { libc::dlsym(dylib, c"update_and_render".as_ptr().cast()) };
            if !symbol.is_null() {
                let update_and_render: fn(PlatformUpdate<Memory, Pixels>) =
                    unsafe { std::mem::transmute(symbol as *const ()) };

                let symbol = unsafe { libc::dlsym(dylib, c"handle_input".as_ptr().cast()) };
                if !symbol.is_null() {
                    let handle_input: fn(PlatformInput<Memory>) =
                        unsafe { std::mem::transmute(symbol as *const ()) };

                    return Some(LoadedGameFunctions {
                        dylib,
                        handle_input,
                        update_and_render,
                    });
                } else {
                    let str = unsafe { CStr::from_ptr(libc::dlerror()) };
                    crate::log!(
                        "ERROR: failed to load dylib symbol `handle_input`: {}",
                        str.to_str().unwrap()
                    );
                }
            } else {
                let str = unsafe { CStr::from_ptr(libc::dlerror()) };
                crate::log!(
                    "ERROR: failed to load dylib symbol `update_and_render`: {}",
                    str.to_str().unwrap()
                );
            }
        } else {
            let str = unsafe { CStr::from_ptr(libc::dlerror()) };
            crate::log!(
                "ERROR: failed to load dylib `{path}`: {}",
                str.to_str().unwrap()
            );
        }

        None
    }

    fn run_app(
        frame_buffer: *mut u8,
        width: usize,
        height: usize,
        update: impl FnMut(PlatformRequest) + 'static,
    ) {
        let app = init_app(update, frame_buffer, width, height);
        init_audio();
        unsafe { app.finishLaunching() };
        app.run();
    }

    fn init_app(
        update: impl FnMut(PlatformRequest) + 'static,
        frame_buffer: *mut u8,
        width: usize,
        height: usize,
    ) -> Retained<NSApplication> {
        unsafe {
            WIDTH = width;
            HEIGHT = height;
        }

        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);

        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(width as f64, height as f64),
                ),
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable,
                // | NSWindowStyleMask::Resizable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe {
            window.setReleasedWhenClosed(false);
        }

        window.setTitle(ns_string!("glazer app"));
        window.center();
        window.makeKeyAndOrderFront(None);
        window.setAcceptsMouseMovedEvents(true);

        let custom_view = GameView::new(mtm, window.clone(), update, frame_buffer);
        window.makeFirstResponder(Some(&custom_view));
        let delegate = Delegate::new(mtm, window.clone(), &custom_view);
        window.setContentView(Some(&*custom_view.into_super()));
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        // Activate the application.
        // Required when launching unbundled (as is done with Cargo).
        #[expect(deprecated)]
        app.activateIgnoringOtherApps(true);
        app
    }

    pub fn update(view: &GameView, ivars: &GameViewIvars) {
        let now = Instant::now();
        let delta = {
            let mut last_time = ivars.last_time.borrow_mut();
            let delta = now.duration_since(*last_time).as_secs_f32();
            *last_time = now;
            delta
        };

        let fps = if delta > 0.0 { 1.0 / delta } else { 0.0 };
        let title = format!("glazer app - {:.2}", fps);
        ivars.window.setTitle(&NSString::from_str(&title));

        let fb = ivars.fb;
        let indices = AUDIO_SAMPLES_INDICES.load(Ordering::Acquire);
        let write_index = (indices >> 32) as usize;
        debug_assert_eq!(write_index % CHANNELS, 0);
        let wrapped_read_index = (indices & u32::MAX as u64) as usize;
        debug_assert_eq!(wrapped_read_index % CHANNELS, 0);

        let samples_to_write = if write_index >= wrapped_read_index {
            (wrapped_read_index + AUDIO_SAMPLES_LEN - write_index - CHANNELS) % AUDIO_SAMPLES_LEN
        } else {
            wrapped_read_index - write_index - CHANNELS
        };

        let mut update = ivars.update.borrow_mut();
        unsafe {
            update(PlatformRequest::Update(PlatformState {
                delta,
                //
                frame_buffer: fb,
                width: WIDTH,
                height: HEIGHT,
                //
                samples: &mut GAME_SAMPLES[..samples_to_write],
                channels: CHANNELS,
                sample_rate: SAMPLE_RATE,
            }));
            view.setNeedsDisplay(true);

            let mut index = write_index;
            for sample in GAME_SAMPLES[..samples_to_write].iter() {
                AUDIO_SAMPLES[index] = *sample;
                index = (index + 1) % AUDIO_SAMPLES_LEN;
            }
        }

        AUDIO_SAMPLES_INDICES
            .fetch_update(Ordering::Release, Ordering::Acquire, |current_indices| {
                let current_read_index = current_indices & u32::MAX as u64;
                let new_write_index = ((write_index + samples_to_write) % AUDIO_SAMPLES_LEN) as u64;
                Some((new_write_index << 32) | current_read_index)
            })
            .unwrap();
    }
}

#[cfg(feature = "opengl")]
pub fn run_opengl<Memory>(
    memory: Memory,
    width: usize,
    height: usize,
    handle_input: fn(PlatformInput<Memory>),
    update_and_render: fn(PlatformUpdateGL<Memory>),
    shared_lib_path: Option<&str>,
) where
    Memory: 'static,
{
    if let Some(shared_lib_path) = shared_lib_path {
        opengl::run_dynamic(memory, width, height, shared_lib_path);
    } else {
        opengl::run_static(memory, width, height, handle_input, update_and_render);
    }
}

#[cfg(feature = "opengl")]
mod opengl {
    use super::*;

    use objc2::ClassType;
    use objc2_app_kit::{
        NSOpenGLPFADepthSize, NSOpenGLPFADoubleBuffer, NSOpenGLPFAOpenGLProfile,
        NSOpenGLPixelFormat, NSOpenGLProfileVersion3_2Core, NSOpenGLView,
    };

    pub fn run_static<Memory>(
        mut memory: Memory,
        width: usize,
        height: usize,
        handle_input: fn(PlatformInput<Memory>),
        update_and_render: fn(PlatformUpdateGL<Memory>),
    ) where
        Memory: 'static,
    {
        let update = move |req: PlatformRequest| {
            match req {
                PlatformRequest::Update(state) => {
                    update_and_render(PlatformUpdateGL {
                        memory: &mut memory,
                        delta: state.delta,
                        //
                        width: state.width,
                        height: state.height,
                        //
                        samples: state.samples,
                        sample_rate: state.sample_rate,
                        channels: state.channels,
                        //
                        reloaded: false,
                    })
                }
                PlatformRequest::Input(input) => handle_input(PlatformInput {
                    memory: &mut memory,
                    input,
                }),
            }
        };
        run_app(width, height, update);
    }

    pub fn run_dynamic<Memory>(
        mut memory: Memory,
        width: usize,
        height: usize,
        shared_lib_path: &str,
    ) where
        Memory: 'static,
    {
        use alloc::string::ToString;

        let shared_lib_path = shared_lib_path.to_string();
        let mut functions =
            load_game_dylib::<Memory>(&shared_lib_path).expect("failed to load game dylib");
        let mut loaded_instant = std::time::SystemTime::now();
        let mut reloaded = false;

        let update = move |req: PlatformRequest| {
            if let Some(modified) = std::fs::metadata(&shared_lib_path).ok().and_then(|meta| {
                meta.modified().ok().and_then(|modified| {
                    modified
                        .duration_since(loaded_instant)
                        .is_ok_and(|dur| !dur.is_zero())
                        .then_some(modified)
                })
            }) {
                // NOTE: This does nothing, macos never closes dylibs.
                debug_assert_eq!(unsafe { libc::dlclose(functions.dylib) }, 0);
                functions =
                    load_game_dylib::<Memory>(&shared_lib_path).expect("failed to load game dylib");
                loaded_instant = modified;
                reloaded = true;
            }

            match req {
                PlatformRequest::Update(state) => {
                    (functions.update_and_render)(PlatformUpdateGL {
                        memory: &mut memory,
                        delta: state.delta,
                        //
                        width: state.width,
                        height: state.height,
                        //
                        samples: state.samples,
                        sample_rate: state.sample_rate,
                        channels: state.channels,
                        //
                        reloaded,
                    });
                    reloaded = false;
                }
                PlatformRequest::Input(input) => (functions.handle_input)(PlatformInput {
                    memory: &mut memory,
                    input,
                }),
            }
        };
        run_app(width, height, update);
    }

    struct LoadedGameFunctions<Memory> {
        dylib: *mut c_void,
        handle_input: fn(PlatformInput<Memory>),
        update_and_render: fn(PlatformUpdateGL<Memory>),
    }

    fn load_game_dylib<Memory>(path: &str) -> Option<LoadedGameFunctions<Memory>> {
        use alloc::ffi::CString;
        use core::ffi::CStr;

        crate::log!("loading game functions from `{path}`");

        let mut copy = std::path::PathBuf::from(path);
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        copy.pop();
        copy.push(format!("{}", time.as_millis()));
        std::fs::copy(path, &copy).expect("failed to copy dylib");

        let filename = CString::new(copy.to_str().unwrap()).expect("invalid dylib string");
        let dylib = unsafe { libc::dlopen(filename.as_ptr(), libc::RTLD_LOCAL | libc::RTLD_LAZY) };
        if !dylib.is_null() {
            let symbol = unsafe { libc::dlsym(dylib, c"update_and_render".as_ptr().cast()) };
            if !symbol.is_null() {
                let update_and_render: fn(PlatformUpdateGL<Memory>) =
                    unsafe { std::mem::transmute(symbol as *const ()) };

                let symbol = unsafe { libc::dlsym(dylib, c"handle_input".as_ptr().cast()) };
                if !symbol.is_null() {
                    let handle_input: fn(PlatformInput<Memory>) =
                        unsafe { std::mem::transmute(symbol as *const ()) };

                    return Some(LoadedGameFunctions {
                        dylib,
                        handle_input,
                        update_and_render,
                    });
                } else {
                    let str = unsafe { CStr::from_ptr(libc::dlerror()) };
                    crate::log!(
                        "ERROR: failed to load dylib symbol `handle_input`: {}",
                        str.to_str().unwrap()
                    );
                }
            } else {
                let str = unsafe { CStr::from_ptr(libc::dlerror()) };
                crate::log!(
                    "ERROR: failed to load dylib symbol `update_and_render`: {}",
                    str.to_str().unwrap()
                );
            }
        } else {
            let str = unsafe { CStr::from_ptr(libc::dlerror()) };
            crate::log!(
                "ERROR: failed to load dylib `{path}`: {}",
                str.to_str().unwrap()
            );
        }

        None
    }

    struct GameOpenGLViewIvars {
        update: RefCell<Box<dyn FnMut(PlatformRequest)>>,
        last_time: RefCell<Instant>,
        window: Retained<NSWindow>,
    }

    define_class!(
        #[unsafe(super = NSOpenGLView)]
        #[thread_kind = MainThreadOnly]
        #[ivars = GameOpenGLViewIvars]
        struct GameOpenGLView;

        unsafe impl NSObjectProtocol for GameOpenGLView {}

        impl GameOpenGLView {
            #[unsafe(method(drawRect:))]
            fn draw_rect(&self, _rect: NSRect) {
                opengl::update(self.ivars());
                self.as_super().openGLContext().unwrap().flushBuffer();
            }

            #[unsafe(method(update:))]
            fn update(&self, _timer: &NSTimer) {
                self.setNeedsDisplay(true);
            }

            #[unsafe(method(acceptsFirstResponder))]
            fn accepts_first_responder(&self) -> bool {
                true
            }

            #[unsafe(method(keyDown:))]
            fn key_down(&self, event: &NSEvent) {
                unsafe {
                let mut update = self.ivars().update.borrow_mut();
                    update(PlatformRequest::Input(Input::Key {
                        code: KEY_CODE_LUT[event.keyCode() as usize],
                        modifiers: KeyModifiers::from(event.modifierFlags()),
                        pressed: true,
                        repeat: event.isARepeat(),
                    }));
                }
            }

            #[unsafe(method(keyUp:))]
            fn key_up(&self, event: &NSEvent) {
                unsafe {
                let mut update = self.ivars().update.borrow_mut();
                    update(PlatformRequest::Input(Input::Key {
                        code: KEY_CODE_LUT[event.keyCode() as usize],
                        modifiers: KeyModifiers::from(event.modifierFlags()),
                        pressed: false,
                        repeat: event.isARepeat(),
                    }));
                }
            }

            #[unsafe(method(mouseMoved:))]
            fn mouse_moved(&self, event: &NSEvent) {
                unsafe {
                let mut update = self.ivars().update.borrow_mut();
                    update(PlatformRequest::Input(Input::MouseMoved {
                        dx: event.deltaX() as f32,
                        dy: event.deltaY() as f32,
                    }));
                }
            }

            #[unsafe(method(flagsChanged:))]
            fn flags_changed(&self, event: &NSEvent) {
                static mut PREVIOUS_MODIFIER_FLAGS: NSEventModifierFlags = NSEventModifierFlags(0);

                    let current_flags = unsafe {event.modifierFlags()};
                    #[allow(static_mut_refs)]
                    let changed = unsafe { current_flags.bits() ^ PREVIOUS_MODIFIER_FLAGS.bits() };
                    unsafe { PREVIOUS_MODIFIER_FLAGS = current_flags; }
                    let pressed = (current_flags.bits() & changed) != 0;
                    let mut update = self.ivars().update.borrow_mut();

                    if changed & NSEventModifierFlags::Shift.bits() != 0 {
                        update(PlatformRequest::Input(Input::Key {
                            // TODO: LeftShift vs RightShift
                            code: KeyCode::LeftShift,
                            modifiers: KeyModifiers::from(current_flags),
                            pressed,
                            repeat: false,
                        }));
                    }

                    if changed & NSEventModifierFlags::Control.bits() != 0 {
                        update(PlatformRequest::Input(Input::Key {
                            // TODO: LeftAlt vs RightAlt,
                            code: KeyCode::LeftControl,
                            modifiers: KeyModifiers::from(current_flags),
                            pressed,
                            repeat: false,
                        }));
                    }

                    if changed & NSEventModifierFlags::Option.bits() != 0 {
                        update(PlatformRequest::Input(Input::Key {
                            // TODO: LeftAlt vs RightAlt,
                            code: KeyCode::LeftAlt,
                            modifiers: KeyModifiers::from(current_flags),
                            pressed,
                            repeat: false,
                        }));
                    }

                    if changed & NSEventModifierFlags::Command.bits() != 0 {
                        // TODO: need command
                    }
            }
        }
    );

    impl GameOpenGLView {
        fn new(
            mtm: MainThreadMarker,
            window: Retained<NSWindow>,
            update: impl FnMut(PlatformRequest) + 'static,
        ) -> Retained<Self> {
            let ivars = GameOpenGLViewIvars {
                update: RefCell::new(Box::new(update)),
                last_time: RefCell::new(Instant::now()),
                window,
            };
            let this = Self::alloc(mtm).set_ivars(ivars);

            unsafe {
                let attrs = [
                    NSOpenGLPFADoubleBuffer,
                    NSOpenGLPFADepthSize,
                    24,
                    NSOpenGLPFAOpenGLProfile,
                    NSOpenGLProfileVersion3_2Core,
                    0,
                ];

                let pixel_format = NSOpenGLPixelFormat::initWithAttributes(
                    NSOpenGLPixelFormat::alloc(),
                    NonNull::from_ref(&attrs).cast(),
                )
                .unwrap();
                let frame = NSRect::ZERO;

                msg_send![super(this), initWithFrame:frame pixelFormat:&*pixel_format]
            }
        }
    }

    fn run_app(width: usize, height: usize, update: impl FnMut(PlatformRequest) + 'static) {
        let app = init_app(update, width, height);
        init_audio();
        unsafe { app.finishLaunching() };
        app.run();
    }

    fn init_app(
        update: impl FnMut(PlatformRequest) + 'static,
        width: usize,
        height: usize,
    ) -> Retained<NSApplication> {
        unsafe {
            WIDTH = width;
            HEIGHT = height;
        }

        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);

        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(width as f64, height as f64),
                ),
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable,
                // | NSWindowStyleMask::Resizable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe {
            window.setReleasedWhenClosed(false);
        }

        window.setTitle(ns_string!("glazer app"));
        window.center();
        window.makeKeyAndOrderFront(None);
        window.setAcceptsMouseMovedEvents(true);

        let custom_view = GameOpenGLView::new(mtm, window.clone(), update);
        gl::load_with(|symbol| {
            use std::ffi::c_void;
            let symbol_cstr = std::ffi::CString::new(symbol).unwrap();
            unsafe {
                let framework = c"/System/Library/Frameworks/OpenGL.framework/OpenGL".as_ptr();
                let handle = libc::dlopen(framework, libc::RTLD_LAZY);
                if handle.is_null() {
                    return std::ptr::null();
                }
                libc::dlsym(handle, symbol_cstr.as_ptr()) as *const c_void
            }
        });

        window.makeFirstResponder(Some(&custom_view));
        let delegate = Delegate::new(mtm, window.clone(), &custom_view);
        window.setContentView(Some(&*custom_view.into_super()));
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        // Activate the application.
        // Required when launching unbundled (as is done with Cargo).
        #[expect(deprecated)]
        app.activateIgnoringOtherApps(true);
        app
    }

    fn update(ivars: &GameOpenGLViewIvars) {
        let now = Instant::now();
        let delta = {
            let mut last_time = ivars.last_time.borrow_mut();
            let delta = now.duration_since(*last_time).as_secs_f32();
            *last_time = now;
            delta
        };

        let fps = if delta > 0.0 { 1.0 / delta } else { 0.0 };
        let title = format!("glazer app - {:.2}", fps);
        ivars.window.setTitle(&NSString::from_str(&title));

        let indices = AUDIO_SAMPLES_INDICES.load(Ordering::Acquire);
        let write_index = (indices >> 32) as usize;
        debug_assert_eq!(write_index % CHANNELS, 0);
        let wrapped_read_index = (indices & u32::MAX as u64) as usize;
        debug_assert_eq!(wrapped_read_index % CHANNELS, 0);

        let samples_to_write = if write_index >= wrapped_read_index {
            (wrapped_read_index + AUDIO_SAMPLES_LEN - write_index - CHANNELS) % AUDIO_SAMPLES_LEN
        } else {
            wrapped_read_index - write_index - CHANNELS
        };

        let mut update = ivars.update.borrow_mut();
        unsafe {
            update(PlatformRequest::Update(PlatformState {
                delta,
                //
                frame_buffer: core::ptr::null_mut(),
                width: WIDTH,
                height: HEIGHT,
                //
                samples: &mut GAME_SAMPLES[..samples_to_write],
                channels: CHANNELS,
                sample_rate: SAMPLE_RATE,
            }));

            let mut index = write_index;
            for sample in GAME_SAMPLES[..samples_to_write].iter() {
                AUDIO_SAMPLES[index] = *sample;
                index = (index + 1) % AUDIO_SAMPLES_LEN;
            }
        }

        AUDIO_SAMPLES_INDICES
            .fetch_update(Ordering::Release, Ordering::Acquire, |current_indices| {
                let current_read_index = current_indices & u32::MAX as u64;
                let new_write_index = ((write_index + samples_to_write) % AUDIO_SAMPLES_LEN) as u64;
                Some((new_write_index << 32) | current_read_index)
            })
            .unwrap();
    }
}

#[derive(Debug, Clone)]
struct AppDelegateIvars {
    #[expect(unused)]
    window: Retained<NSWindow>,
    _timer: Retained<NSTimer>,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = AppDelegateIvars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, notification: &NSNotification) {
            dbg!(notification);
            dbg!(self.ivars());
            start_audio();
            NSApplication::main(MainThreadMarker::from(self));
        }

        #[unsafe(method(applicationShouldTerminate:))]
        unsafe fn application_should_terminate(
            &self,
            _sender: &NSApplication,
        ) -> NSApplicationTerminateReply {
            stop_audio();
            NSApplicationTerminateReply::TerminateNow
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        unsafe fn application_should_terminate_after_last_window_closed(
            &self,
            _sender: &NSApplication,
        ) -> bool {
            true
        }
    }

    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            // Quit the application when the window is closed.
            unsafe {
                NSApplication::sharedApplication(self.mtm()).terminate(None);
            }
        }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker, window: Retained<NSWindow>, view: &AnyObject) -> Retained<Self> {
        let _timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.0,
                view,
                objc2::sel!(update:),
                None,
                true,
            )
        };
        let this = Self::alloc(mtm).set_ivars(AppDelegateIvars { window, _timer });
        unsafe { msg_send![super(this), init] }
    }
}

struct GameViewIvars {
    fb: *mut u8,
    update: RefCell<Box<dyn FnMut(PlatformRequest)>>,
    last_time: RefCell<Instant>,
    window: Retained<NSWindow>,
}

define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = GameViewIvars]
    struct GameView;

    unsafe impl NSObjectProtocol for GameView {}

    impl GameView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, rect: NSRect) {
            let fb = self.ivars().fb;
            let image_rep = unsafe {

                let planes: [*const u8; 1] = [fb];
                NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                    NSBitmapImageRep::alloc(),
                    planes.as_ptr() as *mut _,
                    WIDTH as isize,
                    HEIGHT as isize,
                    8,
                    4,
                    true,
                    false,
                    &NSColorSpaceName::from_str("NSCalibratedRGBColorSpace"),
                    WIDTH as isize * 4,
                    32,
                )
            };

            if let Some(image_rep) = image_rep {
                unsafe {
                    let size = NSSize::new(WIDTH as f64, HEIGHT as f64);
                    let image = NSImage::initWithSize(NSImage::alloc(), size);
                    image.addRepresentation(&image_rep);
                    image.drawInRect(rect);
                }
            }
        }

        #[unsafe(method(update:))]
        fn update(&self, _timer: &NSTimer) {
            software_renderer::update(self, self.ivars());
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            unsafe {
            let mut update = self.ivars().update.borrow_mut();
                update(PlatformRequest::Input(Input::Key {
                    code: KEY_CODE_LUT[event.keyCode() as usize],
                    modifiers: KeyModifiers::from(event.modifierFlags()),
                    pressed: true,
                    repeat: event.isARepeat(),
                }));
            }
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            unsafe {
            let mut update = self.ivars().update.borrow_mut();
                update(PlatformRequest::Input(Input::Key {
                    code: KEY_CODE_LUT[event.keyCode() as usize],
                    modifiers: KeyModifiers::from(event.modifierFlags()),
                    pressed: false,
                    repeat: event.isARepeat(),
                }));
            }
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            unsafe {
            let mut update = self.ivars().update.borrow_mut();
                update(PlatformRequest::Input(Input::MouseMoved {
                    dx: event.deltaX() as f32,
                    dy: event.deltaY() as f32,
                }));
            }
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            static mut PREVIOUS_MODIFIER_FLAGS: NSEventModifierFlags = NSEventModifierFlags(0);

                let current_flags = unsafe {event.modifierFlags()};
                #[allow(static_mut_refs)]
                let changed = unsafe { current_flags.bits() ^ PREVIOUS_MODIFIER_FLAGS.bits() };
                unsafe { PREVIOUS_MODIFIER_FLAGS = current_flags; }
                let pressed = (current_flags.bits() & changed) != 0;
                let mut update = self.ivars().update.borrow_mut();

                if changed & NSEventModifierFlags::Shift.bits() != 0 {
                    update(PlatformRequest::Input(Input::Key {
                        // TODO: LeftShift vs RightShift
                        code: KeyCode::LeftShift,
                        modifiers: KeyModifiers::from(current_flags),
                        pressed,
                        repeat: false,
                    }));
                }

                if changed & NSEventModifierFlags::Control.bits() != 0 {
                    update(PlatformRequest::Input(Input::Key {
                        // TODO: LeftAlt vs RightAlt,
                        code: KeyCode::LeftControl,
                        modifiers: KeyModifiers::from(current_flags),
                        pressed,
                        repeat: false,
                    }));
                }

                if changed & NSEventModifierFlags::Option.bits() != 0 {
                    update(PlatformRequest::Input(Input::Key {
                        // TODO: LeftAlt vs RightAlt,
                        code: KeyCode::LeftAlt,
                        modifiers: KeyModifiers::from(current_flags),
                        pressed,
                        repeat: false,
                    }));
                }

                if changed & NSEventModifierFlags::Command.bits() != 0 {
                    // TODO: need command
                }
        }
    }
);

impl GameView {
    fn new(
        mtm: MainThreadMarker,
        window: Retained<NSWindow>,
        update: impl FnMut(PlatformRequest) + 'static,
        frame_buffer: *mut u8,
    ) -> Retained<Self> {
        let ivars = GameViewIvars {
            fb: frame_buffer,
            update: RefCell::new(Box::new(update)),
            last_time: RefCell::new(Instant::now()),
            window,
        };
        let this = Self::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }
}

static mut AUDIO_UNIT: AudioComponentInstance = null_mut();
const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: usize = 2;

fn start_audio() {
    unsafe {
        let result = AudioOutputUnitStart(AUDIO_UNIT);
        debug_assert_eq!(result, 0);
    }
}

fn stop_audio() {
    unsafe {
        let result = AudioOutputUnitStop(AUDIO_UNIT);
        debug_assert_eq!(result, 0);
    }
}

fn init_audio() {
    use core::ptr::{NonNull, null_mut};

    let mut unit = core::ptr::null_mut();
    let desc = AudioComponentDescription {
        componentType: kAudioUnitType_Output,
        componentSubType: kAudioUnitSubType_DefaultOutput,
        componentManufacturer: kAudioUnitManufacturer_Apple,
        componentFlags: 0,
        componentFlagsMask: 0,
    };

    let stream_desc = AudioStreamBasicDescription {
        mSampleRate: SAMPLE_RATE as f64,
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kLinearPCMFormatFlagIsFloat,
        mBytesPerPacket: 8,
        mFramesPerPacket: 1,
        mBytesPerFrame: 8,
        mChannelsPerFrame: 2,
        mBitsPerChannel: 32,
        mReserved: 0,
    };
    let callback = AURenderCallbackStruct {
        inputProc: Some(audio_callback),
        inputProcRefCon: null_mut(),
    };

    unsafe {
        let component = AudioComponentFindNext(null_mut(), NonNull::from(&desc));
        debug_assert!(!component.is_null());
        let result = AudioComponentInstanceNew(component, NonNull::from(&mut unit));
        debug_assert_eq!(result, 0);
        set_property(unit, kAudioUnitProperty_StreamFormat, &stream_desc);
        set_property(unit, kAudioUnitProperty_SetRenderCallback, &callback);
        let result = AudioUnitInitialize(unit);
        debug_assert_eq!(result, 0);
        AUDIO_UNIT = unit;

        fn set_property<T>(unit: AudioComponentInstance, prop: u32, value: &T) {
            unsafe {
                let result = AudioUnitSetProperty(
                    unit,
                    prop,
                    kAudioUnitScope_Input,
                    kAudioUnitScope_Global,
                    value as *const _ as *const c_void,
                    std::mem::size_of::<T>() as u32,
                );
                debug_assert_eq!(result, 0);
            }
        }
    }
}

static mut WIDTH: usize = 0;
static mut HEIGHT: usize = 0;

const AUDIO_SAMPLES_LEN: usize = 1024 * 4;
static mut AUDIO_SAMPLES: [f32; AUDIO_SAMPLES_LEN] = [0.0; AUDIO_SAMPLES_LEN];
// secondary buffer for the game to write to
static mut GAME_SAMPLES: [f32; AUDIO_SAMPLES_LEN] = [0.0; AUDIO_SAMPLES_LEN];
// write index is packed into top 32 bits, read index in bottom 32 bits
static AUDIO_SAMPLES_INDICES: AtomicU64 = AtomicU64::new(2 << 32);

unsafe extern "C-unwind" fn audio_callback(
    _ref_con: NonNull<c_void>,
    _action_flags: NonNull<AudioUnitRenderActionFlags>,
    _time_stamp: NonNull<AudioTimeStamp>,
    _bus: u32,
    frames: u32,
    data: *mut AudioBufferList,
) -> i32 {
    let frames = frames as usize;
    unsafe {
        let len = (*data).mNumberBuffers as usize;
        debug_assert_eq!(len, 1);

        let len = (*data).mBuffers[0].mDataByteSize as usize / 2;
        let samples = (*data).mBuffers[0].mData as *mut f32;
        let data = core::slice::from_raw_parts_mut(samples, len);
        debug_assert!(len > 0);

        let indices = AUDIO_SAMPLES_INDICES.load(Ordering::Acquire);
        let wrapped_write_index = (indices >> 32) as usize;
        debug_assert_eq!(wrapped_write_index % CHANNELS, 0);
        let read_index = (indices & u32::MAX as u64) as usize;
        debug_assert_eq!(read_index % CHANNELS, 0);

        let available_samples = if wrapped_write_index >= read_index {
            wrapped_write_index - read_index
        } else {
            wrapped_write_index + AUDIO_SAMPLES_LEN - read_index
        };

        let samples_needed = frames * CHANNELS;
        let samples_to_read = available_samples.min(samples_needed);

        let frames_to_read = samples_to_read / CHANNELS;
        let mut index = read_index;
        debug_assert_eq!(CHANNELS, 2);
        for frame in data.chunks_mut(CHANNELS).take(frames_to_read) {
            frame[0] = AUDIO_SAMPLES[index];
            frame[1] = AUDIO_SAMPLES[index + 1];
            index = (index + CHANNELS) % AUDIO_SAMPLES_LEN;
        }

        if frames_to_read < frames {
            // crate::log!("ERROR: audio underrun {} samples", frames - frames_to_read);
            debug_assert_eq!(CHANNELS, 2);
            for i in frames_to_read..frames {
                data[i * CHANNELS] = 0.0;
                data[i * CHANNELS + 1] = 0.0;
            }
        }

        AUDIO_SAMPLES_INDICES
            .fetch_update(Ordering::Release, Ordering::Acquire, |current_indices| {
                let current_write_index = current_indices >> 32;
                let new_read_index = (read_index + samples_to_read) % AUDIO_SAMPLES_LEN;
                Some((current_write_index << 32) | new_read_index as u64)
            })
            .unwrap();
    }
    0
}

impl From<NSEventModifierFlags> for KeyModifiers {
    fn from(value: NSEventModifierFlags) -> Self {
        let mut mods = 0;
        for modifier in value.iter() {
            mods |= match modifier {
                NSEventModifierFlags::CapsLock => KeyModifiers::CAPSLOCK,
                NSEventModifierFlags::Shift => KeyModifiers::SHIFT,
                NSEventModifierFlags::Control => KeyModifiers::CONTROL,
                NSEventModifierFlags::Option => KeyModifiers::OPTION,
                NSEventModifierFlags::Command => KeyModifiers::COMMAND,
                NSEventModifierFlags::NumericPad => KeyModifiers::NUMERIC_PAD,
                NSEventModifierFlags::Help => KeyModifiers::HELP,
                NSEventModifierFlags::Function => KeyModifiers::FUNCTION,
                NSEventModifierFlags::DeviceIndependentFlagsMask => KeyModifiers::CLEAR,
                _ => KeyModifiers::CLEAR,
            }
            .0;
        }
        KeyModifiers(mods)
    }
}

// https://gist.github.com/eegrok/949034
const KEY_CODE_LUT: [KeyCode; 128] = {
    let mut lut = [KeyCode::Unknown; 128];
    lut[0x00] = KeyCode::KeyA;
    lut[0x01] = KeyCode::KeyS;
    lut[0x02] = KeyCode::KeyD;
    lut[0x03] = KeyCode::KeyF;
    lut[0x04] = KeyCode::KeyH;
    lut[0x05] = KeyCode::KeyG;
    lut[0x06] = KeyCode::KeyZ;
    lut[0x07] = KeyCode::KeyX;
    lut[0x08] = KeyCode::KeyC;
    lut[0x09] = KeyCode::KeyV;
    lut[0x0A] = KeyCode::NonUSBackslash;
    lut[0x0B] = KeyCode::KeyB;
    lut[0x0C] = KeyCode::KeyQ;
    lut[0x0D] = KeyCode::KeyW;
    lut[0x0E] = KeyCode::KeyE;
    lut[0x0F] = KeyCode::KeyR;
    lut[0x10] = KeyCode::KeyY;
    lut[0x11] = KeyCode::KeyT;
    lut[0x12] = KeyCode::Num1;
    lut[0x13] = KeyCode::Num2;
    lut[0x14] = KeyCode::Num3;
    lut[0x15] = KeyCode::Num4;
    lut[0x16] = KeyCode::Num6;
    lut[0x17] = KeyCode::Num5;
    lut[0x18] = KeyCode::EqualSign;
    lut[0x19] = KeyCode::Num9;
    lut[0x1A] = KeyCode::Num7;
    lut[0x1B] = KeyCode::Hyphen;
    lut[0x1C] = KeyCode::Num8;
    lut[0x1D] = KeyCode::Num0;
    lut[0x1E] = KeyCode::CloseBracket;
    lut[0x1F] = KeyCode::KeyO;
    lut[0x20] = KeyCode::KeyU;
    lut[0x21] = KeyCode::OpenBracket;
    lut[0x22] = KeyCode::KeyI;
    lut[0x23] = KeyCode::KeyP;
    lut[0x24] = KeyCode::Return;
    lut[0x25] = KeyCode::KeyL;
    lut[0x26] = KeyCode::KeyJ;
    lut[0x27] = KeyCode::Quote;
    lut[0x28] = KeyCode::KeyK;
    lut[0x29] = KeyCode::Semicolon;
    lut[0x2A] = KeyCode::Backslash;
    lut[0x2B] = KeyCode::Comma;
    lut[0x2C] = KeyCode::Slash;
    lut[0x2D] = KeyCode::KeyN;
    lut[0x2E] = KeyCode::KeyM;
    lut[0x2F] = KeyCode::Period;
    lut[0x30] = KeyCode::Tab;
    lut[0x31] = KeyCode::Spacebar;
    lut[0x32] = KeyCode::NonUSPound;
    lut[0x33] = KeyCode::DeleteOrBackspace;
    lut[0x34] = KeyCode::Return;
    lut[0x35] = KeyCode::Escape;
    lut[0x5F] = KeyCode::Separator;
    lut[0x72] = KeyCode::Insert;
    lut[0x73] = KeyCode::Home;
    lut[0x74] = KeyCode::PageUp;
    lut[0x75] = KeyCode::DeleteForward;
    lut[0x77] = KeyCode::End;
    lut[0x79] = KeyCode::PageDown;
    lut[0x7B] = KeyCode::LeftArrow;
    lut[0x7C] = KeyCode::RightArrow;
    lut[0x7D] = KeyCode::DownArrow;
    lut[0x7E] = KeyCode::UpArrow;
    lut
};

// Debug utilities

#[inline]
pub fn log(str: &str) {
    std::print!("{str}");
}

pub fn debug_time_secs<R>(mut f: impl FnMut() -> R) -> (f32, R) {
    extern crate std;
    let start = std::time::Instant::now();
    let result = f();
    let duration = std::time::Instant::now()
        .duration_since(start)
        .as_secs_f32();
    (duration, result)
}

pub fn debug_time_millis<R>(mut f: impl FnMut() -> R) -> (u128, R) {
    extern crate std;
    let start = std::time::Instant::now();
    let result = f();
    let duration = std::time::Instant::now().duration_since(start).as_millis();
    (duration, result)
}

pub fn debug_time_micros<R>(mut f: impl FnMut() -> R) -> (u128, R) {
    extern crate std;
    let start = std::time::Instant::now();
    let result = f();
    let duration = std::time::Instant::now().duration_since(start).as_micros();
    (duration, result)
}

pub fn debug_time_nanos<R>(mut f: impl FnMut() -> R) -> (u128, R) {
    extern crate std;
    let start = std::time::Instant::now();
    let result = f();
    let duration = std::time::Instant::now().duration_since(start).as_nanos();
    (duration, result)
}
