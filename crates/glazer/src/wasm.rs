use alloc::boxed::Box;
use alloc::vec::Vec;

use wasm_bindgen::prelude::*;
use web_sys::{
    AudioContext, AudioProcessingEvent, CanvasRenderingContext2d, HtmlCanvasElement, ImageData,
};

use crate::{Audio, platform::PlatformState};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);
}

fn init_canvas() -> HtmlCanvasElement {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();

    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(600);
    canvas.set_height(600);
    document.body().unwrap().append_child(&canvas).unwrap();
    canvas
}

fn init_audio(mut audio: impl FnMut(Audio) + 'static) {
    let audio_context = AudioContext::new().unwrap();
    let processor = audio_context.create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
            2048, 0, 1
        ).unwrap();

    let mut buf = [0.0; 2048];
    let audio_closure = Closure::wrap(Box::new(move |event: AudioProcessingEvent| {
        let output_buffer = event.output_buffer().unwrap();
        let sample_rate = output_buffer.sample_rate();
        audio(Audio {
            samples: &mut buf,
            channels: 1,
            sample_rate,
            delta: 1.0 / 60.0,
        });
        output_buffer.copy_to_channel(&buf, 0).unwrap();
    }) as Box<dyn FnMut(AudioProcessingEvent)>);

    processor.set_onaudioprocess(Some(audio_closure.as_ref().unchecked_ref()));
    processor
        .connect_with_audio_node(&audio_context.destination())
        .unwrap();
    audio_closure.forget();
}

fn game_loop(
    mut update: impl FnMut(PlatformState) + 'static,
    context: CanvasRenderingContext2d,
    mut framebuffer: Vec<u8>,
) {
    let closure = Closure::once_into_js(move || {
        update(PlatformState {
            frame_buffer: framebuffer.as_mut_slice(),
            width: 600,
            height: 600,
            delta: 1.0 / 60.0,
        });
        let image_data = ImageData::new_with_u8_clamped_array_and_sh(
            wasm_bindgen::Clamped(framebuffer.as_slice()),
            600,
            600,
        )
        .unwrap();
        context.put_image_data(&image_data, 0.0, 0.0).unwrap();
        game_loop(update, context, framebuffer);
    });
    web_sys::window()
        .unwrap()
        .request_animation_frame(closure.as_ref().unchecked_ref())
        .unwrap();
}

#[macro_export]
macro_rules! log {
        () => {
            $crate::platform::wasm::log("")
        };
        ($($arg:tt)*) => {{
            $crate::platform::wasm::log(&alloc::format!($($arg)*));
        }};
    }

pub fn run(update: impl FnMut(PlatformState) + 'static, audio: impl FnMut(Audio) + 'static) {
    let canvas = init_canvas();
    let context = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()
        .unwrap();

    let mut framebuffer = Vec::with_capacity(600 * 600 * 4);
    framebuffer.extend((0..600usize * 600 * 4).map(|i| i as u8));
    init_audio(audio);
    game_loop(update, context, framebuffer);
}
