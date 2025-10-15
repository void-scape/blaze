use glam::*;
use rast::tint::*;
use rast::*;
use rast_web::{HEIGHT, WIDTH, serve};

fn main() {
    let nlight_bytes = include_bytes!("../../assets/nlights.bin");
    let nlights = unsafe {
        std::slice::from_raw_parts(nlight_bytes.as_ptr() as *const Srgb, nlight_bytes.len() / 4)
    };
    let shader = TextureShader {
        texture: nlights,
        width: 400,
        height: 400,
        sampler: rast::Sampler::Bilinear,
    };

    let mut angle = 0.0;
    serve(move |pixel_buffer, _, dt| {
        angle += dt;

        let scale = Vec2::new(WIDTH as f32 / 3.0, HEIGHT as f32 / 3.0);
        let offset = Vec2::new(WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0);

        let v1 = transform_vertex(Vec3::new(-0.5, 0.7, 0.0), angle, scale, offset);
        let v2 = transform_vertex(Vec3::new(-0.5, -0.7, 0.0), angle, scale, offset);
        let v3 = transform_vertex(Vec3::new(0.5, 0.7, 0.0), angle, scale, offset);
        rast::rast_triangle(
            pixel_buffer,
            WIDTH,
            HEIGHT,
            v1.x as i32,
            v1.y as i32,
            v2.x as i32,
            v2.y as i32,
            v3.x as i32,
            v3.y as i32,
            (0.0, 1.0),
            (0.0, 0.0),
            (1.0, 1.0),
            shader,
        );

        let v1 = transform_vertex(Vec3::new(0.5, -0.7, 0.0), angle, scale, offset);
        let v2 = transform_vertex(Vec3::new(-0.5, -0.7, 0.0), angle, scale, offset);
        let v3 = transform_vertex(Vec3::new(0.5, 0.7, 0.0), angle, scale, offset);
        rast::rast_triangle(
            pixel_buffer,
            WIDTH,
            HEIGHT,
            v1.x as i32,
            v1.y as i32,
            v2.x as i32,
            v2.y as i32,
            v3.x as i32,
            v3.y as i32,
            (1.0, 0.0),
            (0.0, 0.0),
            (1.0, 1.0),
            shader,
        );
    });
}

fn transform_vertex(v: Vec3, angle: f32, scale: Vec2, offset: Vec2) -> Vec2 {
    let rotated = Quat::from_rotation_z(angle).mul_vec3(v);
    Vec2::new(
        rotated.x * scale.x + offset.x,
        rotated.y * scale.y + offset.y,
    )
}
