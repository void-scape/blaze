use criterion::{Criterion, criterion_group, criterion_main};
use glam::*;
use rast::tint::*;
use rast::*;
use std::hint::black_box;

const WIDTH: usize = 800;
const HEIGHT: usize = 800;

fn verts() -> (Vec3, Vec3, Vec3) {
    let v1 = Vec3::new(
        black_box(WIDTH as f32 / 3.0),
        black_box(HEIGHT as f32 / 3.0 * 2.0),
        0.0,
    );
    let v2 = Vec3::new(
        black_box(WIDTH as f32 / 2.0),
        black_box(HEIGHT as f32 / 3.0),
        0.0,
    );
    let v3 = Vec3::new(
        black_box(WIDTH as f32 / 3.0 * 2.0),
        black_box(HEIGHT as f32 / 3.0 * 2.0),
        0.0,
    );
    (v1, v2, v3)
}

fn triangle(pixel_buffer: &mut [Srgb]) {
    let (v1, v2, v3) = verts();
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
        Srgb::rgb(255, 255, 255).into(),
        Srgb::rgb(255, 255, 255).into(),
        Srgb::rgb(255, 255, 255).into(),
        ColorShader,
    );
}

fn triangle_rgb(pixel_buffer: &mut [Srgb]) {
    let (v1, v2, v3) = verts();

    let c1 = LinearRgb::rgb(black_box(1.0), black_box(0.0), black_box(0.0));
    let c2 = LinearRgb::rgb(black_box(0.0), black_box(1.0), black_box(0.0));
    let c3 = LinearRgb::rgb(black_box(0.0), black_box(0.0), black_box(1.0));

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
        black_box(c1),
        black_box(c2),
        black_box(c3),
        black_box(ColorShader),
    );
}

fn triangle_texture(pixel_buffer: &mut [Srgb], texture: TextureShader<Srgb>) {
    let (v1, v2, v3) = verts();

    let uv1 = (black_box(0.0), black_box(1.0));
    let uv2 = (black_box(0.0), black_box(0.0));
    let uv3 = (black_box(1.0), black_box(1.0));

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
        black_box(uv1),
        black_box(uv2),
        black_box(uv3),
        black_box(texture),
    );
}

fn triangle_rgb_checked(pixel_buffer: &mut [Srgb], depth_buffer: &mut [f32]) {
    let (v1, v2, v3) = verts();

    let c1 = LinearRgb::rgb(black_box(1.0), black_box(0.0), black_box(0.0));
    let c2 = LinearRgb::rgb(black_box(0.0), black_box(1.0), black_box(0.0));
    let c3 = LinearRgb::rgb(black_box(0.0), black_box(0.0), black_box(1.0));

    rast::rast_triangle_checked(
        pixel_buffer,
        depth_buffer,
        WIDTH,
        HEIGHT,
        v1.x as i32,
        v1.y as i32,
        v1.z,
        v2.x as i32,
        v2.y as i32,
        v2.z,
        v3.x as i32,
        v3.y as i32,
        v3.z,
        black_box(c1),
        black_box(c2),
        black_box(c3),
        black_box(ColorShader),
    );
}

fn bench_fn(c: &mut Criterion, name: &str, f: impl Fn(&mut [Srgb])) {
    let mut buf = vec![Srgb::default(); WIDTH * HEIGHT];
    c.bench_function(name, |b| {
        b.iter(|| f(black_box(&mut buf)));
    });
}

fn bench_fn_checked(c: &mut Criterion, name: &str, f: impl Fn(&mut [Srgb], &mut [f32])) {
    let mut buf = vec![Srgb::default(); WIDTH * HEIGHT];
    c.bench_function(name, |b| {
        b.iter_batched(
            || vec![1.0; WIDTH * HEIGHT],
            |mut depth| f(black_box(&mut buf), black_box(&mut depth)),
            criterion::BatchSize::LargeInput,
        );
    });
}

fn criterion_benchmark(c: &mut Criterion) {
    bench_fn(c, "triangle", triangle);
    bench_fn(c, "triangle_rgb", triangle_rgb);
    bench_fn_checked(c, "triangle_rgb_checked", triangle_rgb_checked);

    let texture = TextureShader {
        texture: &std::array::from_fn::<_, 10_000, _>(|i| {
            Srgb::rgb(
                (i % 255) as u8,
                ((i + 1) % 255) as u8,
                ((i + 2) % 255) as u8,
            )
        }),
        width: 100,
        height: 100,
        sampler: rast::Sampler::Nearest,
    };
    let mut buf = vec![Srgb::default(); WIDTH * HEIGHT];
    c.bench_function("triangle_texture", |b| {
        b.iter(|| triangle_texture(black_box(&mut buf), black_box(texture)));
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
