use std::io::BufReader;

use glam::*;
use rast::tint::*;
use rast::*;
use rast_web::{HEIGHT, WIDTH, serve};

fn main() {
    let utah_teapot = read_utah_teapot();
    let mut angle = 0.0;

    serve(move |pixel_buffer, depth_buffer, dt| {
        fn display(mut v: Vec3) -> Vec3 {
            if v.z < 0.0 {
                v.z = -v.z;
            }

            if v.z < f32::EPSILON {
                v.z += f32::EPSILON;
            }

            let proj = Vec2::new(v.x / v.z, v.y / v.z);
            Vec3::new(
                (proj.x + 1.0) / 2.0 * WIDTH as f32,
                (1.0 - (proj.y + 1.0) / 2.0) * HEIGHT as f32,
                v.z,
            )
        }

        let offset = Vec3::new(0.0, -1.5, 4.5);
        angle = (angle + dt) % core::f32::consts::TAU;
        for slice in utah_teapot.chunks(3) {
            let quat = Quat::from_rotation_y(angle);
            let v1 = quat.mul_vec3(slice[0]);
            let v2 = quat.mul_vec3(slice[1]);
            let v3 = quat.mul_vec3(slice[2]);

            let v1 = display(v1 + offset);
            let v2 = display(v2 + offset);
            let v3 = display(v3 + offset);

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
                LinearRgb::from_rgb(1.0, 0.0, 0.0),
                LinearRgb::from_rgb(0.0, 1.0, 0.0),
                LinearRgb::from_rgb(0.0, 0.0, 1.0),
                ColorShader,
            );
        }
    });
}

fn read_utah_teapot() -> Vec<Vec3> {
    let (model, _) = tobj::load_obj_buf(
        &mut BufReader::new(include_bytes!("../../assets/utah-teapot.obj").as_slice()),
        &tobj::GPU_LOAD_OPTIONS,
        |_| tobj::MTLLoadResult::Ok(Default::default()),
    )
    .unwrap();

    let mut output = Vec::new();
    for i in model[0].mesh.indices.iter() {
        let i = *i as usize;
        let pos = Vec3::new(
            model[0].mesh.positions[i * 3],
            model[0].mesh.positions[i * 3 + 1],
            model[0].mesh.positions[i * 3 + 2],
        );
        output.push(pos);
    }

    output
}
