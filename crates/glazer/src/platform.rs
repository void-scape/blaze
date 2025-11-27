use alloc::{collections::vec_deque::VecDeque, sync::Arc};
use std::sync::Mutex;

extern crate std;

type SampleBuffer = Arc<Mutex<VecDeque<f32>>>;

#[cfg(feature = "software")]
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
    software::run(
        mem,
        frame_buffer,
        width,
        height,
        handle_input,
        update_and_render,
        reload,
    );
}

#[cfg(feature = "software")]
mod software {
    use super::*;

    use crate::callback::FnPtrs;
    use alloc::collections::vec_deque::VecDeque;
    use alloc::rc::Rc;
    use alloc::sync::Arc;
    use core::marker::PhantomData;
    use core::num::NonZeroU32;
    use cpal::Stream;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use softbuffer::Context;
    #[cfg(not(target_arch = "wasm32"))]
    use std::time::SystemTime;
    use winit::application::ApplicationHandler;
    use winit::dpi::PhysicalSize;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop, OwnedDisplayHandle};
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
                        for (out, input) in
                            data.iter_mut().zip(buffer.drain(0..len.min(buffer_len)))
                        {
                            *out = input;
                        }
                    }
                },
                |err| std::println!("[ERROR] audio thread: {}", err),
                None,
            )
            .unwrap();

        #[cfg(not(target_arch = "wasm32"))]
        stream.play().unwrap();

        let event_loop = EventLoop::new().unwrap();
        let frame_buffer = frame_buffer.as_mut_ptr().cast();

        #[allow(unused_mut)]
        let mut app = App {
            window: None,
            stream,
            gfx: None,
            width,
            height,
            mem,
            frame_buffer,
            fns: FnPtrs::new(handle_input, update_and_render, reload),
            sample_buffer,
            sample_rate,
            channels,
            #[cfg(not(target_arch = "wasm32"))]
            now: SystemTime::now(),
            _pixels: PhantomData::<Pixels>,
        };

        #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
        event_loop.run_app(&mut app).unwrap();

        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        console_error_panic_hook::set_once();
        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        winit::platform::web::EventLoopExtWebSys::spawn_app(event_loop, app);
    }

    struct App<Memory, Pixels> {
        window: Option<Rc<Window>>,
        #[allow(unused)]
        stream: Stream,
        gfx: Option<Gfx>,
        width: usize,
        height: usize,
        mem: Memory,
        frame_buffer: *mut u32,
        fns: FnPtrs,
        sample_buffer: SampleBuffer,
        sample_rate: u32,
        channels: usize,
        #[cfg(not(target_arch = "wasm32"))]
        now: SystemTime,
        _pixels: PhantomData<Pixels>,
    }

    struct Gfx {
        _ctx: Context<OwnedDisplayHandle>,
        surface: softbuffer::Surface<OwnedDisplayHandle, Rc<Window>>,
    }

    impl<Memory, Pixels> ApplicationHandler for App<Memory, Pixels> {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }

            let attributes = WindowAttributes::default();
            #[cfg(target_arch = "wasm32")]
            let attributes =
                winit::platform::web::WindowAttributesExtWebSys::with_append(attributes, true);
            self.window = match event_loop.create_window(attributes) {
                Ok(window) => {
                    let failed = window.request_inner_size(PhysicalSize::new(
                        self.width as u32,
                        self.height as u32,
                    ));
                    assert!(
                        failed.is_none(),
                        "platform does not support resizing the window"
                    );
                    window.request_redraw();
                    Some(Rc::new(window))
                }
                Err(err) => {
                    std::println!("[ERROR] failed to create the window: {err}");
                    event_loop.exit();
                    return;
                }
            };

            let ctx = Context::new(event_loop.owned_display_handle())
                .expect("Failed to create a softbuffer context");
            let mut surface = softbuffer::Surface::new(&ctx, self.window.as_ref().unwrap().clone())
                .expect("Failed to create a softbuffer surface");

            if self.width > 0 && self.height > 0 {
                surface
                    .resize(
                        NonZeroU32::new(self.width as u32).unwrap(),
                        NonZeroU32::new(self.height as u32).unwrap(),
                    )
                    .expect("failed to resize the softbuffer surface");
            }

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
                            .expect("failed to resize the softbuffer surface");
                    }
                    self.window
                        .as_ref()
                        .expect("resize event without a window")
                        .request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    let reloaded = self.fns.reload();

                    let window = self
                        .window
                        .as_ref()
                        .expect("redraw request without a window");
                    window.request_redraw();

                    #[cfg(target_arch = "wasm32")]
                    let delta = 1.0 / 60.0;
                    #[cfg(not(target_arch = "wasm32"))]
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

                    self.fns.update_and_render(crate::PlatformUpdate {
                        memory: &mut self.mem,
                        delta,
                        frame_buffer: unsafe {
                            core::slice::from_raw_parts_mut(
                                self.frame_buffer.cast::<Pixels>(),
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

                        let size = window.inner_size();
                        if let (Some(w), Some(h)) =
                            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                        {
                            gfx.surface
                                .resize(w, h)
                                .expect("failed to resize the softbuffer surface");
                        }

                        let mut buffer = gfx
                            .surface
                            .buffer_mut()
                            .expect("Failed to get the softbuffer buffer");
                        let display = buffer.as_mut_ptr();
                        unsafe {
                            display.copy_from_nonoverlapping(
                                self.frame_buffer,
                                self.width * self.height,
                            );
                        }
                        buffer
                            .present()
                            .expect("Failed to present the softbuffer buffer");
                    }
                }
                event => {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use winit::event::{ElementState, MouseButton};
                        if matches!(
                            event,
                            WindowEvent::MouseInput {
                                state: ElementState::Pressed,
                                button: MouseButton::Left,
                                ..
                            }
                        ) {
                            _ = self.stream.play();
                        }
                    }
                    self.fns.handle_input(crate::PlatformInput {
                        memory: &mut self.mem,
                        input: event,
                    });
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(feature = "opengl")]
pub fn run<Memory>(
    mem: Memory,
    width: usize,
    height: usize,
    handle_input: fn(crate::PlatformInput<Memory>),
    update_and_render: fn(crate::PlatformUpdate<Memory>),
    reload: Option<&str>,
) where
    Memory: 'static,
{
    opengl::run(mem, width, height, handle_input, update_and_render, reload);
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(feature = "opengl")]
mod opengl {
    use super::*;

    use crate::callback::FnPtrs;
    use core::num::NonZeroU32;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use glow::HasContext;
    use glutin::config::{Config, ConfigTemplateBuilder, GetGlConfig};
    use glutin::context::{
        ContextApi, ContextAttributesBuilder, NotCurrentContext, PossiblyCurrentContext,
    };
    use glutin::display::GetGlDisplay;
    use glutin::prelude::{GlDisplay, NotCurrentGlContext, PossiblyCurrentGlContext};
    use glutin::surface::{GlSurface, Surface, SwapInterval, WindowSurface};
    use glutin_winit::{DisplayBuilder, GlWindow};
    use std::time::SystemTime;
    use winit::application::ApplicationHandler;
    use winit::dpi::PhysicalSize;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::raw_window_handle::HasWindowHandle;
    use winit::window::{Window, WindowId};

    pub fn run<Memory>(
        mem: Memory,
        width: usize,
        height: usize,
        handle_input: fn(crate::PlatformInput<Memory>),
        update_and_render: fn(crate::PlatformUpdate<Memory>),
        reload: Option<&str>,
    ) where
        Memory: 'static,
    {
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
                        for (out, input) in
                            data.iter_mut().zip(buffer.drain(0..len.min(buffer_len)))
                        {
                            *out = input;
                        }
                    }
                },
                |err| std::println!("[ERROR] audio thread: {}", err),
                None,
            )
            .unwrap();

        stream.play().unwrap();

        let event_loop = EventLoop::new().unwrap();

        let mut app = &mut opengl::OpenGLApp {
            window: None,
            gl_display: Some(
                DisplayBuilder::new().with_window_attributes(Some(Window::default_attributes())),
            ),
            gl_context: None,
            gl_surface: None,
            gl: None,
            width,
            height,
            mem,
            fns: FnPtrs::new(handle_input, update_and_render, reload),
            sample_buffer,
            sample_rate,
            channels,
            now: SystemTime::now(),
        };
        event_loop.run_app(&mut app).unwrap();
    }

    pub struct OpenGLApp<Memory> {
        window: Option<Window>,
        gl_display: Option<DisplayBuilder>,
        gl_context: Option<PossiblyCurrentContext>,
        gl_surface: Option<Surface<WindowSurface>>,
        gl: Option<glow::Context>,
        width: usize,
        height: usize,
        mem: Memory,
        fns: FnPtrs,
        sample_buffer: SampleBuffer,
        sample_rate: u32,
        channels: usize,
        now: SystemTime,
    }

    impl<Memory> ApplicationHandler for OpenGLApp<Memory> {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let (window, gl_config) = match &self.gl_display {
                // We just created the event loop, so initialize the display, pick the config, and
                // create the context.
                Some(display_builder) => {
                    let template = ConfigTemplateBuilder::new();
                    let (window, gl_config) =
                        match display_builder
                            .clone()
                            .build(event_loop, template, |mut config| config.next().unwrap())
                        {
                            Ok((window, gl_config)) => (window.unwrap(), gl_config),
                            Err(err) => {
                                std::println!("[ERROR] failed to initialize OpenGL: {err}");
                                event_loop.exit();
                                return;
                            }
                        };
                    self.gl_display = None;
                    self.gl_context =
                        Some(create_gl_context(&window, &gl_config).treat_as_possibly_current());

                    let failed = window.request_inner_size(PhysicalSize::new(
                        self.width as u32,
                        self.height as u32,
                    ));
                    assert!(
                        failed.is_none(),
                        "platform does not support resizing the window"
                    );

                    (window, gl_config)
                }
                None => {
                    // Pick the config which we already use for the context.
                    let gl_config = self.gl_context.as_ref().unwrap().config();
                    match glutin_winit::finalize_window(
                        event_loop,
                        Window::default_attributes(),
                        &gl_config,
                    ) {
                        Ok(window) => (window, gl_config),
                        Err(err) => {
                            std::println!("[ERROR] failed to resume the OpenGL context: {err}");
                            event_loop.exit();
                            return;
                        }
                    }
                }
            };

            let attrs = window
                .build_surface_attributes(Default::default())
                .expect("Failed to build surface attributes");
            let gl_surface = unsafe {
                gl_config
                    .display()
                    .create_window_surface(&gl_config, &attrs)
                    .unwrap()
            };

            // The context needs to be current for the Renderer to set up shaders and
            // buffers. It also performs function loading, which needs a current context on
            // WGL.
            let gl_context = self.gl_context.as_ref().unwrap();
            gl_context.make_current(&gl_surface).unwrap();

            let gl = unsafe {
                glow::Context::from_loader_function_cstr(|s| {
                    gl_config.display().get_proc_address(s)
                })
            };

            gl_surface
                .set_swap_interval(gl_context, SwapInterval::DontWait)
                .unwrap();

            self.gl_surface = Some(gl_surface);
            self.gl = Some(gl);
            self.window = Some(window);
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::CloseRequested => {
                    event_loop.exit();
                }
                WindowEvent::Resized(size) if size.width != 0 && size.height != 0 => {
                    // Some platforms like EGL require resizing GL surface to update the size
                    // Notable platforms here are Wayland and macOS, other don't require it
                    // and the function is no-op, but it's wise to resize it for portability
                    // reasons.
                    if let Some(gl_surface) = &self.gl_surface {
                        let gl_context = self.gl_context.as_ref().unwrap();
                        gl_surface.resize(
                            gl_context,
                            NonZeroU32::new(size.width).unwrap(),
                            NonZeroU32::new(size.height).unwrap(),
                        );
                        self.width = size.width as usize;
                        self.height = size.height as usize;
                        unsafe {
                            if let Some(gl) = &mut self.gl {
                                gl.viewport(0, 0, self.width as i32, self.height as i32);
                            }
                        }
                    }
                }
                WindowEvent::RedrawRequested => {
                    let reloaded = self.fns.reload();

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

                    if let Some(gl) = &self.gl {
                        self.fns.update_and_render(crate::PlatformUpdate {
                            memory: &mut self.mem,
                            delta,
                            gl,
                            width: self.width,
                            height: self.height,
                            samples,
                            sample_rate: self.sample_rate,
                            channels: self.channels,
                            reloaded,
                        });
                    }

                    if !samples.is_empty() {
                        let mut sample_buffer = self.sample_buffer.lock().unwrap();
                        sample_buffer.extend(samples.iter());
                    }

                    if let Some(gl_surface) = &self.gl_surface {
                        let gl_context = self.gl_context.as_ref().unwrap();
                        gl_surface.swap_buffers(gl_context).unwrap();
                    }
                }
                event => {
                    self.fns.handle_input(crate::PlatformInput {
                        memory: &mut self.mem,
                        input: event,
                    });
                }
            }
        }
    }

    fn create_gl_context(window: &Window, gl_config: &Config) -> NotCurrentContext {
        let raw_window_handle = window.window_handle().ok().map(|wh| wh.as_raw());
        let context_attributes = ContextAttributesBuilder::new().build(raw_window_handle);

        // Since glutin by default tries to create OpenGL core context, which may not be
        // present we should try gles.
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(raw_window_handle);

        // Reuse the uncurrented context from a suspended() call if it exists, otherwise
        // this is the first time resumed() is called, where the context still
        // has to be created.
        let gl_display = gl_config.display();

        unsafe {
            gl_display
                .create_context(gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    gl_display
                        .create_context(gl_config, &fallback_context_attributes)
                        .expect("failed to create OpenGL context")
                })
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[cfg(feature = "opengl")]
pub fn run<Memory>(
    mem: Memory,
    width: usize,
    height: usize,
    handle_input: fn(crate::PlatformInput<Memory>),
    update_and_render: fn(crate::PlatformUpdate<Memory>),
    reload: Option<&str>,
) where
    Memory: 'static,
{
    opengl_wasm::run(mem, width, height, handle_input, update_and_render, reload);
}

#[cfg(target_arch = "wasm32")]
#[cfg(feature = "opengl")]
mod opengl_wasm {
    use super::*;

    use crate::callback::FnPtrs;
    use crate::winit::platform::web::WindowExtWebSys;
    use cpal::Stream;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use glow::HasContext;
    use wasm_bindgen::JsCast;
    use winit::application::ApplicationHandler;
    use winit::dpi::PhysicalSize;
    use winit::event::WindowEvent;
    use winit::event::{ElementState, MouseButton};
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::window::{Window, WindowAttributes, WindowId};

    pub fn run<Memory>(
        mem: Memory,
        width: usize,
        height: usize,
        handle_input: fn(crate::PlatformInput<Memory>),
        update_and_render: fn(crate::PlatformUpdate<Memory>),
        reload: Option<&str>,
    ) where
        Memory: 'static,
    {
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
                        for (out, input) in
                            data.iter_mut().zip(buffer.drain(0..len.min(buffer_len)))
                        {
                            *out = input;
                        }
                    }
                },
                |err| std::println!("[ERROR] audio thread: {}", err),
                None,
            )
            .unwrap();

        let event_loop = EventLoop::new().unwrap();

        let app = OpenGLApp {
            window: None,
            stream,
            gl: None,
            width,
            height,
            mem,
            fns: FnPtrs::new(handle_input, update_and_render, reload),
            sample_buffer,
            sample_rate,
            channels,
        };
        console_error_panic_hook::set_once();
        winit::platform::web::EventLoopExtWebSys::spawn_app(event_loop, app);
    }

    pub struct OpenGLApp<Memory> {
        window: Option<Window>,
        stream: Stream,
        gl: Option<glow::Context>,
        width: usize,
        height: usize,
        mem: Memory,
        fns: FnPtrs,
        sample_buffer: SampleBuffer,
        sample_rate: u32,
        channels: usize,
    }

    impl<Memory> ApplicationHandler for OpenGLApp<Memory> {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }

            let window = match event_loop
                .create_window(window_attributes(self.width as u32, self.height as u32))
            {
                Ok(window) => window,
                Err(err) => {
                    crate::log!("[ERROR] failed to create the window: {err}");
                    event_loop.exit();
                    return;
                }
            };

            web_sys::window()
                .unwrap()
                .document()
                .unwrap()
                .body()
                .unwrap()
                .append_child(&window.canvas().unwrap())
                .unwrap();
            let webgl2_context = window
                .canvas()
                .unwrap()
                .get_context("webgl2")
                .unwrap()
                .unwrap()
                .dyn_into::<web_sys::WebGl2RenderingContext>()
                .unwrap();
            self.gl = Some(glow::Context::from_webgl2_context(webgl2_context));
            self.window = Some(window);
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::CloseRequested => {
                    event_loop.exit();
                }
                WindowEvent::Resized(size) if size.width != 0 && size.height != 0 => {
                    self.width = size.width as usize;
                    self.height = size.height as usize;

                    let device_pixel_ratio = web_sys::window().unwrap().device_pixel_ratio();
                    let physical_width = (size.width as f64 * device_pixel_ratio) as u32;
                    let physical_height = (size.height as f64 * device_pixel_ratio) as u32;

                    if let Some(window) = &self.window {
                        let canvas = window.canvas().unwrap();
                        canvas.set_width(physical_width);
                        canvas.set_height(physical_height);
                    }
                }
                WindowEvent::RedrawRequested => {
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

                    if let Some(gl) = &self.gl {
                        unsafe {
                            gl.viewport(0, 0, self.width as i32, self.height as i32);
                        }
                        self.fns.update_and_render(crate::PlatformUpdate {
                            memory: &mut self.mem,
                            delta: 1.0 / 60.0,
                            gl,
                            width: self.width,
                            height: self.height,
                            samples,
                            sample_rate: self.sample_rate,
                            channels: self.channels,
                            reloaded: false,
                        });
                    }

                    if !samples.is_empty() {
                        let mut sample_buffer = self.sample_buffer.lock().unwrap();
                        sample_buffer.extend(samples.iter());
                    }
                }
                event => {
                    if matches!(
                        event,
                        WindowEvent::MouseInput {
                            state: ElementState::Pressed,
                            button: MouseButton::Left,
                            ..
                        }
                    ) {
                        _ = self.stream.play();
                    }
                    self.fns.handle_input(crate::PlatformInput {
                        memory: &mut self.mem,
                        input: event,
                    });
                }
            }
        }

        fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    fn window_attributes(width: u32, height: u32) -> WindowAttributes {
        winit::platform::web::WindowAttributesExtWebSys::with_append(
            Window::default_attributes().with_inner_size(PhysicalSize::new(width, height)),
            true,
        )
    }
}
