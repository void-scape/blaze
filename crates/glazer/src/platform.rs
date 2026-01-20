use crate::callback::FnPtrs;
use crate::time::Time;
use alloc::rc::Rc;
#[cfg(feature = "audio")]
use alloc::{collections::vec_deque::VecDeque, sync::Arc};
use core::num::NonZeroU32;
#[cfg(feature = "audio")]
use cpal::{
    Stream,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
#[cfg(feature = "opengl")]
#[cfg(not(target_arch = "wasm32"))]
use glutin::{
    config::{Config, GetGlConfig},
    context::{NotCurrentContext, PossiblyCurrentContext},
    surface::{Surface, WindowSurface},
};
#[cfg(feature = "opengl")]
#[cfg(not(target_arch = "wasm32"))]
use glutin_winit::DisplayBuilder;
#[cfg(feature = "software")]
use softbuffer::Context;
#[cfg(feature = "audio")]
use std::sync::Mutex;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
#[cfg(feature = "software")]
use winit::event_loop::OwnedDisplayHandle;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

extern crate std;

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
    #[cfg(feature = "audio")]
    let audio = {
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

        Audio {
            stream,
            sample_buffer,
            sample_rate,
            channels,
        }
    };

    #[allow(unused_mut)]
    let mut app = App {
        window: None,
        #[cfg(feature = "opengl")]
        #[cfg(not(target_arch = "wasm32"))]
        opengl_display_builder: Some(
            DisplayBuilder::new()
                .with_window_attributes(Some(window_attributes(width as u32, height as u32))),
        ),
        gfx: None,
        #[cfg(feature = "audio")]
        audio,
        //
        width,
        height,
        mem,
        now: Time::now(),
        fns: FnPtrs::new(handle_input, update_and_render, reload),
    };

    let event_loop = EventLoop::new().unwrap();
    #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
    event_loop.run_app(&mut app).unwrap();
    #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
    {
        console_error_panic_hook::set_once();
        winit::platform::web::EventLoopExtWebSys::spawn_app(event_loop, app);
    }
}

struct App<Memory> {
    window: Option<Rc<Window>>,
    #[cfg(feature = "opengl")]
    #[cfg(not(target_arch = "wasm32"))]
    opengl_display_builder: Option<DisplayBuilder>,
    gfx: Option<Gfx>,
    #[cfg(feature = "audio")]
    audio: Audio,
    //
    width: usize,
    height: usize,
    mem: Memory,
    now: Time,
    fns: FnPtrs,
}

#[cfg(feature = "audio")]
type SampleBuffer = Arc<Mutex<VecDeque<f32>>>;

#[cfg(feature = "audio")]
struct Audio {
    #[allow(unused)]
    stream: Stream,
    sample_buffer: SampleBuffer,
    sample_rate: u32,
    channels: usize,
}

#[cfg(feature = "opengl")]
#[cfg(not(target_arch = "wasm32"))]
struct Gfx {
    context: PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
    gl: glow::Context,
}

#[cfg(feature = "opengl")]
#[cfg(target_arch = "wasm32")]
type Gfx = glow::Context;

#[cfg(feature = "software")]
struct Gfx {
    _ctx: Context<OwnedDisplayHandle>,
    surface: softbuffer::Surface<OwnedDisplayHandle, Rc<Window>>,
}

impl<Memory> ApplicationHandler for App<Memory> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(feature = "opengl")]
        #[cfg(not(target_arch = "wasm32"))]
        {
            use glutin::prelude::{NotCurrentGlContext, PossiblyCurrentGlContext};
            use glutin::surface::GlSurface;
            use glutin::{display::GetGlDisplay, prelude::GlDisplay};
            use glutin_winit::GlWindow;

            let (window, gl_config) = match &self.opengl_display_builder {
                // We just created the event loop, so initialize the display, pick the config, and
                // create the context.
                Some(display_builder) => {
                    use glutin::config::ConfigTemplateBuilder;

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

                    (window, gl_config)
                }
                None => {
                    // Pick the config which we already use for the context.
                    let gl_config = self.gfx.as_ref().unwrap().context.config();
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
            let gl_context = create_gl_context(&window, &gl_config).treat_as_possibly_current();
            gl_context.make_current(&gl_surface).unwrap();
            _ = gl_surface.set_swap_interval(&gl_context, glutin::surface::SwapInterval::DontWait);

            let gl = unsafe {
                glow::Context::from_loader_function_cstr(|s| {
                    gl_config.display().get_proc_address(s)
                })
            };

            self.gfx = Some(Gfx {
                context: gl_context,
                surface: gl_surface,
                gl,
            });
            self.window = Some(Rc::new(window));
        }

        ////

        #[cfg(feature = "opengl")]
        #[cfg(target_arch = "wasm32")]
        {
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

            use crate::winit::platform::web::WindowExtWebSys;
            use web_sys::wasm_bindgen::JsCast;

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
            self.gfx = Some(glow::Context::from_webgl2_context(webgl2_context));
            self.window = Some(Rc::new(window));
        }

        ////

        #[cfg(feature = "software")]
        {
            if self.window.is_some() {
                return;
            }

            self.window = match event_loop
                .create_window(window_attributes(self.width as u32, self.height as u32))
            {
                Ok(window) => Some(Rc::new(window)),
                Err(err) => {
                    std::println!("[ERROR] failed to create the window: {err}");
                    event_loop.exit();
                    return;
                }
            };

            let ctx = Context::new(event_loop.owned_display_handle())
                .expect("failed to create the frame buffer");
            let surface = softbuffer::Surface::new(&ctx, self.window.as_ref().unwrap().clone())
                .expect("failed to create the frame buffer");
            self.gfx = Some(Gfx { _ctx: ctx, surface });
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(gfx) = self.gfx.as_mut()
                    && let (Some(w), Some(h)) =
                        (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    self.width = size.width as usize;
                    self.height = size.height as usize;

                    #[cfg(feature = "software")]
                    gfx.surface
                        .resize(w, h)
                        .expect("failed to resize the frame buffer");

                    // Some platforms like EGL require resizing GL surface to update the size
                    // Notable platforms here are Wayland and macOS, other don't require it
                    // and the function is no-op, but it's wise to resize it for portability
                    // reasons.
                    #[cfg(feature = "opengl")]
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        use crate::glow::HasContext;
                        use glutin::prelude::GlSurface;

                        gfx.surface.resize(&gfx.context, w, h);
                        unsafe {
                            gfx.gl.viewport(0, 0, self.width as i32, self.height as i32);
                        }
                    }

                    #[cfg(feature = "opengl")]
                    #[cfg(target_arch = "wasm32")]
                    {
                        use crate::glow::HasContext;
                        use crate::winit::platform::web::WindowExtWebSys;

                        let device_pixel_ratio = web_sys::window().unwrap().device_pixel_ratio();
                        let physical_width = (size.width as f64 * device_pixel_ratio) as u32;
                        let physical_height = (size.height as f64 * device_pixel_ratio) as u32;

                        if let Some(window) = &self.window {
                            let canvas = window.canvas().unwrap();
                            canvas.set_width(physical_width);
                            canvas.set_height(physical_height);
                            _ = w;
                            _ = h;
                            unsafe {
                                gfx.viewport(0, 0, self.width as i32, self.height as i32);
                            }
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let (Some(window), Some(gfx)) = (&self.window, &mut self.gfx) else {
                    return;
                };

                #[cfg(feature = "audio")]
                const HEAD: usize = 1024 * 5;
                #[cfg(feature = "audio")]
                let mut stack_sample_buffer = [0.0; HEAD];
                #[cfg(feature = "audio")]
                let samples = {
                    let sample_buffer = self.audio.sample_buffer.lock().unwrap();
                    let len = sample_buffer.len();
                    if len < HEAD {
                        stack_sample_buffer.as_mut_slice()
                    } else {
                        &mut []
                    }
                };

                #[cfg(feature = "software")]
                let mut frame_buffer = {
                    let buffer = gfx
                        .surface
                        .buffer_mut()
                        .expect("failed to get the frame buffer");
                    debug_assert_eq!(buffer.len(), self.width * self.height);
                    buffer
                };

                let reloaded = self.fns.reload();

                let delta = {
                    let now = Time::now();
                    let delta = now.elapsed_secs(self.now);
                    self.now = now;
                    delta
                };

                self.fns.update_and_render(crate::PlatformUpdate {
                    memory: &mut self.mem,
                    delta,
                    //
                    window: &window,
                    event_loop,
                    //
                    #[cfg(feature = "opengl")]
                    #[cfg(not(target_arch = "wasm32"))]
                    gl: &gfx.gl,
                    #[cfg(feature = "opengl")]
                    #[cfg(target_arch = "wasm32")]
                    gl: gfx,
                    #[cfg(feature = "software")]
                    frame_buffer: unsafe {
                        core::slice::from_raw_parts_mut(
                            frame_buffer.as_mut_ptr().cast(),
                            frame_buffer.len(),
                        )
                    },
                    width: self.width,
                    height: self.height,
                    //
                    #[cfg(feature = "audio")]
                    samples,
                    #[cfg(feature = "audio")]
                    sample_rate: self.audio.sample_rate,
                    #[cfg(feature = "audio")]
                    channels: self.audio.channels,
                    //
                    reloaded,
                });

                #[cfg(feature = "audio")]
                if !samples.is_empty() {
                    let mut sample_buffer = self.audio.sample_buffer.lock().unwrap();
                    sample_buffer.extend(samples.iter());
                }

                window.pre_present_notify();
                #[cfg(feature = "opengl")]
                #[cfg(not(target_arch = "wasm32"))]
                {
                    use glutin::prelude::GlSurface;
                    gfx.surface
                        .swap_buffers(&gfx.context)
                        .expect("failed to present the frame buffer");
                }
                #[cfg(feature = "software")]
                frame_buffer
                    .present()
                    .expect("failed to present the frame buffer");
                window.request_redraw();
            }
            _ => {}
        }

        // NOTE: Don't start stream until the user interacts with the page.
        #[cfg(feature = "audio")]
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
                _ = self.audio.stream.play();
            }
        }

        #[allow(unused)]
        let Some(gfx) = &mut self.gfx else {
            return;
        };

        let Some(window) = &self.window else {
            return;
        };

        self.fns.handle_input(crate::PlatformInput {
            memory: &mut self.mem,
            window: &window,
            #[cfg(feature = "opengl")]
            #[cfg(not(target_arch = "wasm32"))]
            gl: &gfx.gl,
            #[cfg(feature = "opengl")]
            #[cfg(target_arch = "wasm32")]
            gl: &gfx,
            input: crate::Input::Window(event),
        });
    }

    fn device_event(
        &mut self,
        _: &ActiveEventLoop,
        _: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        #[allow(unused)]
        let (Some(window), Some(gfx)) = (&self.window, &mut self.gfx) else {
            return;
        };

        self.fns.handle_input(crate::PlatformInput {
            memory: &mut self.mem,
            window: &window,
            #[cfg(feature = "opengl")]
            #[cfg(not(target_arch = "wasm32"))]
            gl: &gfx.gl,
            #[cfg(feature = "opengl")]
            #[cfg(target_arch = "wasm32")]
            gl: &gfx,
            input: crate::Input::Device(event),
        });
    }
}

fn window_attributes(width: u32, height: u32) -> WindowAttributes {
    let attributes = Window::default_attributes().with_inner_size(PhysicalSize::new(width, height));
    #[cfg(target_arch = "wasm32")]
    let attributes = winit::platform::web::WindowAttributesExtWebSys::with_append(attributes, true);
    attributes
}

#[cfg(feature = "opengl")]
#[cfg(not(target_arch = "wasm32"))]
fn create_gl_context(window: &Window, gl_config: &Config) -> NotCurrentContext {
    use glutin::prelude::GlDisplay;
    use glutin::{
        context::{ContextApi, ContextAttributesBuilder},
        display::GetGlDisplay,
    };
    use winit::raw_window_handle::HasWindowHandle;

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
