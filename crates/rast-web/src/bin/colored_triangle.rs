use rast::tint::*;
use rast::*;
use rast_web::{HEIGHT, WIDTH, serve};

fn main() {
    serve(colored_triangle);
}

fn colored_triangle(pixels: &mut [Srgb], _: &mut [f32], _: f32) {
    rast::rast_triangle(
        pixels,
        WIDTH,
        HEIGHT,
        WIDTH as i32 / 3,
        HEIGHT as i32 / 3 * 2,
        WIDTH as i32 / 2,
        HEIGHT as i32 / 3,
        WIDTH as i32 / 3 * 2,
        HEIGHT as i32 / 3 * 2,
        LinearRgb::rgb(1.0, 0.0, 0.0),
        LinearRgb::rgb(0.0, 1.0, 0.0),
        LinearRgb::rgb(0.0, 0.0, 1.0),
        ColorShader,
    );
}
