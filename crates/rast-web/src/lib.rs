use rast::tint::*;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Response, Server};

pub fn serve(mut f: impl FnMut(&mut [Srgb], &mut [f32], f32) + Send + Sync + 'static) {
    let render_time = Arc::new(RwLock::new(0.0));
    let double_buffer = Arc::new(RwLock::new(PixelBuffer::new(WIDTH, HEIGHT)));
    thread::spawn({
        let render_time = render_time.clone();
        let double_buffer = double_buffer.clone();
        move || {
            let mut dt = 16.0 / 1000.0;
            loop {
                let start = std::time::Instant::now();
                {
                    let PixelBuffer {
                        pixels,
                        depth_buffer,
                    } = &mut *double_buffer.write().unwrap();
                    pixels.fill(Srgb::rgb(42, 42, 42));
                    depth_buffer.fill(std::f32::MAX);
                    f(pixels.as_mut_slice(), depth_buffer.as_mut_slice(), dt);
                }
                let end = std::time::Instant::now()
                    .duration_since(start)
                    .as_secs_f32();
                *render_time.write().unwrap() = end;
                dt = end;

                // Sleep so that server can read buffer.
                thread::sleep(Duration::from_millis(1));
            }
        }
    });
    server(double_buffer.clone(), render_time.clone());
}

pub const WIDTH: usize = 600;
pub const HEIGHT: usize = 600;

struct PixelBuffer {
    pixels: Vec<Srgb>,
    depth_buffer: Vec<f32>,
}

impl PixelBuffer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            pixels: vec![Srgb::default(); width * height],
            depth_buffer: vec![0.0; width * height],
        }
    }
}

fn server(double_buffer: Arc<RwLock<PixelBuffer>>, render_time: Arc<RwLock<f32>>) {
    let server = Server::http("localhost:3030").unwrap();
    println!("Server running at http://localhost:3030");

    for request in server.incoming_requests() {
        let html = include_str!("index.html");
        let response = match request.url() {
            "/pixels" => {
                let buffers = double_buffer.read().unwrap();
                let pixel_data = unsafe {
                    std::slice::from_raw_parts::<u8>(
                        buffers.pixels.as_ptr() as *const u8,
                        WIDTH * HEIGHT * 4,
                    )
                };

                Response::from_data(pixel_data.to_vec()).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/octet-stream"[..])
                        .unwrap(),
                )
            }
            "/time" => {
                let time = render_time.read().unwrap();
                Response::from_string(format!("{time:.4}")).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/plain"[..]).unwrap(),
                )
            }
            _ => Response::from_string(html)
                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap()),
        };

        let _ = request.respond(response);
    }
}
