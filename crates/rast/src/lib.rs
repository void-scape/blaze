//! Provides simple rendering functionality for primitive types.
//!
//! The primitive types are triangles, lines, and wireframes. Every primitive
//! draw function comes with a `checked` variant. The `checked` draw functions
//! use a depth check to determine if the pixel should be drawn by comparing the
//! current z value to the corresponding z value in `zbuffer`. If the current z
//! is smaller than the value in the `zbuffer`, the pixel is drawn and the `zbuffer`
//! is updated.
//!
//! ## Triangles
//! The `triangle` rasterizer uses [`barycentric_coordinates`] to interpolate
//! vertex data across points on the triangle.
//! - [`rast_triangle`]
//! - [`rast_triangle_checked`]
//! - [`rast_triangle_colored`]
//! - [`rast_triangle_colored_checked`]
//!
//! ## Quads
//! - [`rast_quad`]
//! - [`rast_quad_checked`]
//! - [`rast_quad_colored`]
//! - [`rast_quad_colored_checked`]
//!
//! ## Lines
//! - [`rast_line`]
//! - [`rast_line_checked`]
//!
//! ## Wireframes
//! The `wireframe` draw calls dispatch three `rast_line` calls for each edge
//! of the triangle.
//! - [`rast_triangle_wireframe`]
//! - [`rast_triangle_wireframe_checked`]

#![no_std]
extern crate alloc;

// TODO: There is either a precision issue in the zbuffer or a bug somewhere upstream
// causing z fighting.
//
// TODO: The barycentric interpolation does not account for the vertices z values.
// The interpolation should behave as if it is over a 3d triangle, not on the 2d
// projected triangle.
//
// TODO: function to sort quads?
// let cx = (v1x + v2x + v3x + v4x) as f32 / 4.0;
// let cy = (v1y + v2y + v3y + v4y) as f32 / 4.0;
//
// let mut sorted = [
//     (v1x, v1y, v1z, d1),
//     (v2x, v2y, v2z, d2),
//     (v3x, v3y, v3z, d3),
//     (v4x, v4y, v4z, d4),
// ]
// .map(|(x, y, z, d)| ((x, y, z, d), libm::atan2f(y as f32 - cy, x as f32 - cx)));
// sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
// let [
//     ((v1x, v1y, v1z, d1), a1),
//     ((v2x, v2y, v2z, d2), a2),
//     ((v3x, v3y, v3z, d3), a3),
//     ((v4x, v4y, v4z, d4), a4),
// ] = sorted;
// debug_assert!(a1 <= a2 && a2 <= a3 && a3 <= a4);

use core::marker::PhantomData;
use tint::*;

pub use tint;

pub fn rast_triangle<S: Shader, Pixel: Color>(
    pixels: &mut [Pixel],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v2x: i32,
    v2y: i32,
    v3x: i32,
    v3y: i32,
    d1: S::VertexData,
    d2: S::VertexData,
    d3: S::VertexData,
    shader: S,
) {
    #[rustfmt::skip]
    rast_triangle_inner(
        pixels,
        &mut [],
        width,
        height,
        v1x, v1y, 0.0,
        v2x, v2y, 0.0,
        v3x, v3y, 0.0,
        d1, d2, d3,
        shader,
        false,
    );
}

pub fn rast_triangle_checked<S: Shader, Pixel: Color>(
    pixels: &mut [Pixel],
    zbuffer: &mut [f32],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v1z: f32,
    v2x: i32,
    v2y: i32,
    v2z: f32,
    v3x: i32,
    v3y: i32,
    v3z: f32,
    d1: S::VertexData,
    d2: S::VertexData,
    d3: S::VertexData,
    shader: S,
) {
    #[rustfmt::skip]
    rast_triangle_inner(
        pixels,
        zbuffer,
        width,
        height,
        v1x, v1y, v1z,
        v2x, v2y, v2z,
        v3x, v3y, v3z,
        d1, d2, d3,
        shader,
        true,
    );
}

fn rast_triangle_inner<S: Shader, Pixel: Color>(
    pixels: &mut [Pixel],
    zbuffer: &mut [f32],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v1z: f32,
    v2x: i32,
    v2y: i32,
    v2z: f32,
    v3x: i32,
    v3y: i32,
    v3z: f32,
    d1: S::VertexData,
    d2: S::VertexData,
    d3: S::VertexData,
    mut shader: S,
    depth_check: bool,
) {
    debug_assert_eq!(depth_check, !zbuffer.is_empty());

    let iwidth = width as i32;
    let iheight = height as i32;

    let (v1x, v1y, v1z) = shader.vertex(v1x, v1y, v1z);
    let (v2x, v2y, v2z) = shader.vertex(v2x, v2y, v2z);
    let (v3x, v3y, v3z) = shader.vertex(v3x, v3y, v3z);

    // bounding box clip
    let minx = v1x.min(v2x).min(v3x).max(0);
    let maxx = v1x.max(v2x).max(v3x).min(iwidth);
    let miny = v1y.min(v2y).min(v3y).max(0);
    let maxy = v1y.max(v2y).max(v3y).min(iheight);
    if miny == maxy || minx == maxx || miny >= iheight || minx >= iwidth {
        return;
    }

    for y in miny..maxy {
        for x in minx..maxx {
            if let Some((u1, u2, d)) = barycentric_coordinates(x, y, v1x, v1y, v2x, v2y, v3x, v3y) {
                // Thank you zozin for this strange integer barycentric math:
                // https://github.com/tsoding/olive.c/blob/master/olive.c
                let bcx = u1 as f32 / d as f32;
                let bcy = u2 as f32 / d as f32;
                let bcz = (d - u1 - u2) as f32 / d as f32;
                debug_assert!((bcx + bcy + bcz - 1.0).abs() < 0.00001);

                let index = (y * width as i32 + x) as usize;
                debug_assert!(
                    index < width * height,
                    "w: {width} h: {height} x: {x} y: {y} \
                        minx: {minx} maxx: {maxx} miny: {miny} maxy: {maxy}"
                );
                if depth_check {
                    let z = v1z * bcx + v2z * bcy + v3z * bcz;
                    if zbuffer[index] <= z {
                        continue;
                    }
                    zbuffer[index] = z;
                }
                let vd = shader.interpolate(bcx, bcy, bcz, d1, d2, d3);
                pixels[index] = shader.fragment(vd).into();
            }
        }
    }
}

pub fn barycentric_coordinates(
    px: i32,
    py: i32,
    v1x: i32,
    v1y: i32,
    v2x: i32,
    v2y: i32,
    v3x: i32,
    v3y: i32,
) -> Option<(i32, i32, i32)> {
    // Thank you zozin for this strange integer barycentric math:
    // https://github.com/tsoding/olive.c/blob/master/olive.c

    // https://en.wikipedia.org/wiki/Barycentric_coordinate_system#Edge_approach
    // TODO: subtraction overflow panic
    // TODO: mult overflow panic
    let d = (v1x - v3x) * (v2y - v3y) - (v1y - v3y) * (v2x - v3x);
    if d == 0 {
        return None;
    }

    let u = (px - v3x) * (v2y - v3y) - (py - v3y) * (v2x - v3x);
    let v = (px - v3x) * (v3y - v1y) - (py - v3y) * (v3x - v1x);
    let w = d - u - v;

    let ds = d.signum();
    ((u.signum() == ds || u == 0) && (v.signum() == ds || v == 0) && (w.signum() == ds || w == 0))
        .then_some((u, v, d))
}

pub fn rast_triangle_colored<Pixel: Color>(
    pixels: &mut [Pixel],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v2x: i32,
    v2y: i32,
    v3x: i32,
    v3y: i32,
    color: Srgb,
) {
    #[rustfmt::skip]
    rast_triangle_colored_inner(
        pixels,
        &mut [],
        width,
        height,
        v1x, v1y, 0.0,
        v2x, v2y, 0.0,
        v3x, v3y, 0.0,
        color,
        false,
    );
}

pub fn rast_triangle_colored_checked<Pixel: Color>(
    pixels: &mut [Pixel],
    zbuffer: &mut [f32],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v1z: f32,
    v2x: i32,
    v2y: i32,
    v2z: f32,
    v3x: i32,
    v3y: i32,
    v3z: f32,
    color: Srgb,
) {
    #[rustfmt::skip]
    rast_triangle_colored_inner(
        pixels,
        zbuffer,
        width,
        height,
        v1x, v1y, v1z,
        v2x, v2y, v2z,
        v3x, v3y, v3z,
        color,
        true,
    );
}

fn rast_triangle_colored_inner<Pixel: Color>(
    pixels: &mut [Pixel],
    zbuffer: &mut [f32],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v1z: f32,
    v2x: i32,
    v2y: i32,
    v2z: f32,
    v3x: i32,
    v3y: i32,
    v3z: f32,
    color: Srgb,
    depth_check: bool,
) {
    debug_assert_eq!(depth_check, !zbuffer.is_empty());

    let iwidth = width as i32;
    let iheight = height as i32;

    // bounding box clip
    let minx = v1x.min(v2x).min(v3x).max(0);
    let maxx = v1x.max(v2x).max(v3x).min(iwidth);
    let miny = v1y.min(v2y).min(v3y).max(0);
    let maxy = v1y.max(v2y).max(v3y).min(iheight);
    if miny == maxy || minx == maxx || miny >= iheight || minx >= iwidth {
        return;
    }

    for y in miny..maxy {
        for x in minx..maxx {
            if let Some((u1, u2, d)) = barycentric_coordinates(x, y, v1x, v1y, v2x, v2y, v3x, v3y) {
                let index = (y * width as i32 + x) as usize;
                debug_assert!(
                    index < width * height,
                    "w: {width} h: {height} x: {x} y: {y} \
                        minx: {minx} maxx: {maxx} miny: {miny} maxy: {maxy}"
                );
                if depth_check {
                    // Thank you zozin for this strange integer barycentric math:
                    // https://github.com/tsoding/olive.c/blob/master/olive.c
                    let bcx = u1 as f32 / d as f32;
                    let bcy = u2 as f32 / d as f32;
                    let bcz = (d - u1 - u2) as f32 / d as f32;
                    debug_assert!((bcx + bcy + bcz - 1.0).abs() < 0.00001);

                    let z = v1z * bcx + v2z * bcy + v3z * bcz;
                    if zbuffer[index] <= z {
                        continue;
                    }
                    zbuffer[index] = z;
                }
                pixels[index] = color.into();
            }
        }
    }
}

pub fn rast_quad<S: Shader + Clone, Pixel: Color>(
    pixels: &mut [Pixel],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v2x: i32,
    v2y: i32,
    v3x: i32,
    v3y: i32,
    v4x: i32,
    v4y: i32,
    d1: S::VertexData,
    d2: S::VertexData,
    d3: S::VertexData,
    d4: S::VertexData,
    shader: S,
) {
    #[rustfmt::skip]
    rast_triangle_inner(
        pixels,
        &mut [],
        width,
        height,
        v1x, v1y, 0.0,
        v2x, v2y, 0.0,
        v3x, v3y, 0.0,
        d1, d2, d3,
        shader.clone(),
        false,
    );
    #[rustfmt::skip]
    rast_triangle_inner(
        pixels,
        &mut [],
        width,
        height,
        v1x, v1y, 0.0,
        v3x, v3y, 0.0,
        v4x, v4y, 0.0,
        d1, d3, d4,
        shader,
        false,
    );
}

pub fn rast_quad_checked<S: Shader + Clone, Pixel: Color>(
    pixels: &mut [Pixel],
    zbuffer: &mut [f32],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v1z: f32,
    v2x: i32,
    v2y: i32,
    v2z: f32,
    v3x: i32,
    v3y: i32,
    v3z: f32,
    v4x: i32,
    v4y: i32,
    v4z: f32,
    d1: S::VertexData,
    d2: S::VertexData,
    d3: S::VertexData,
    d4: S::VertexData,
    shader: S,
) {
    #[rustfmt::skip]
    rast_triangle_inner(
        pixels,
        zbuffer,
        width,
        height,
        v1x, v1y, v1z,
        v2x, v2y, v2z,
        v3x, v3y, v3z,
        d1, d2, d3,
        shader.clone(),
        true,
    );
    #[rustfmt::skip]
    rast_triangle_inner(
        pixels,
        zbuffer,
        width,
        height,
        v1x, v1y, v1z,
        v3x, v3y, v3z,
        v4x, v4y, v4z,
        d1, d3, d4,
        shader,
        true,
    );
}

pub fn rast_quad_colored<Pixel: Color>(
    pixels: &mut [Pixel],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v2x: i32,
    v2y: i32,
    v3x: i32,
    v3y: i32,
    v4x: i32,
    v4y: i32,
    color: Srgb,
) {
    #[rustfmt::skip]
    rast_triangle_colored_inner(
        pixels,
        &mut [],
        width,
        height,
        v1x, v1y, 0.0,
        v2x, v2y, 0.0,
        v3x, v3y, 0.0,
        color,
        false,
    );
    #[rustfmt::skip]
    rast_triangle_colored_inner(
        pixels,
        &mut [],
        width,
        height,
        v1x, v1y, 0.0,
        v3x, v3y, 0.0,
        v4x, v4y, 0.0,
        color,
        false,
    );
}

pub fn rast_quad_colored_checked<Pixel: Color>(
    pixels: &mut [Pixel],
    zbuffer: &mut [f32],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v1z: f32,
    v2x: i32,
    v2y: i32,
    v2z: f32,
    v3x: i32,
    v3y: i32,
    v3z: f32,
    v4x: i32,
    v4y: i32,
    v4z: f32,
    color: Srgb,
) {
    #[rustfmt::skip]
    rast_triangle_colored_inner(
        pixels,
        zbuffer,
        width,
        height,
        v1x, v1y, v1z,
        v2x, v2y, v2z,
        v3x, v3y, v3z,
        color,
        true,
    );
    #[rustfmt::skip]
    rast_triangle_colored_inner(
        pixels,
        zbuffer,
        width,
        height,
        v1x, v1y, v1z,
        v3x, v3y, v3z,
        v4x, v4y, v4z,
        color,
        true,
    );
}

pub fn rast_line<Pixel: Copy>(
    pixels: &mut [Pixel],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v1z: f32,
    v2x: i32,
    v2y: i32,
    v2z: f32,
    c: Pixel,
) {
    #[rustfmt::skip]
    rast_line_inner(
        pixels,
        &mut [],
        width,
        height,
        v1x, v1y, v1z,
        v2x, v2y, v2z,
        c,
        false,
    );
}

pub fn rast_line_checked<Pixel: Copy>(
    pixels: &mut [Pixel],
    zbuffer: &mut [f32],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v1z: f32,
    v2x: i32,
    v2y: i32,
    v2z: f32,
    c: Pixel,
) {
    #[rustfmt::skip]
    rast_line_inner(
        pixels,
        zbuffer,
        width,
        height,
        v1x, v1y, v1z,
        v2x, v2y, v2z,
        c,
        true,
    );
}

fn rast_line_inner<Pixel: Copy>(
    pixels: &mut [Pixel],
    zbuffer: &mut [f32],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v1z: f32,
    v2x: i32,
    v2y: i32,
    v2z: f32,
    c: Pixel,
    depth_check: bool,
) {
    let dx = v2x - v1x;
    let dy = v2y - v1y;
    if dx.abs() < 1 && dy.abs() < 1 {
        return;
    }

    let steps = if dx.abs() > dy.abs() {
        dx.abs()
    } else {
        dy.abs()
    };

    let step_x = dx as f32 / steps as f32;
    let step_y = dy as f32 / steps as f32;
    let step_z = (v2z - v1z) / steps as f32;

    for i in 0..=steps {
        let x = v1x as f32 + i as f32 * step_x;
        let y = v1y as f32 + i as f32 * step_y;
        let z = v1z as f32 + i as f32 * step_z;

        let pixel_x = libm::floorf(x) as i32;
        let pixel_y = libm::floorf(y) as i32;

        if pixel_x >= 0 && pixel_x < width as i32 && pixel_y >= 0 && pixel_y < height as i32 {
            let index = (pixel_y as usize) * width + (pixel_x as usize);

            if depth_check {
                if zbuffer[index] <= z {
                    continue;
                }
                zbuffer[index] = z;
            }
            pixels[index] = c;
        }
    }
}

pub fn rast_triangle_wireframe<Pixel: Copy>(
    pixels: &mut [Pixel],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v1z: f32,
    v2x: i32,
    v2y: i32,
    v2z: f32,
    v3x: i32,
    v3y: i32,
    v3z: f32,
    c: Pixel,
) {
    #[rustfmt::skip]
    rast_line(
        pixels,
        width,
        height,
        v1x, v1y, v1z,
        v2x, v2y, v2z,
        c,
    );
    #[rustfmt::skip]
    rast_line(
        pixels,
        width,
        height,
        v1x, v1y, v1z,
        v3x, v3y, v3z,
        c,
    );
    #[rustfmt::skip]
    rast_line(
        pixels,
        width,
        height,
        v2x, v2y, v2z,
        v3x, v3y, v3z,
        c,
    );
}

pub fn rast_triangle_wireframe_checked<Pixel: Copy>(
    pixels: &mut [Pixel],
    zbuffer: &mut [f32],
    width: usize,
    height: usize,
    v1x: i32,
    v1y: i32,
    v1z: f32,
    v2x: i32,
    v2y: i32,
    v2z: f32,
    v3x: i32,
    v3y: i32,
    v3z: f32,
    c: Pixel,
) {
    #[rustfmt::skip]
    rast_line_checked(
        pixels,
        zbuffer,
        width,
        height,
        v1x, v1y, v1z,
        v2x, v2y, v2z,
        c,
    );
    #[rustfmt::skip]
    rast_line_checked(
        pixels,
        zbuffer,
        width,
        height,
        v1x, v1y, v1z,
        v3x, v3y, v3z,
        c,
    );
    #[rustfmt::skip]
    rast_line_checked(
        pixels,
        zbuffer,
        width,
        height,
        v2x, v2y, v2z,
        v3x, v3y, v3z,
        c,
    );
}

pub trait Shader {
    type VertexData: Copy;

    fn interpolate(
        &self,
        bcx: f32,
        bcy: f32,
        bcz: f32,
        d1: Self::VertexData,
        d2: Self::VertexData,
        d3: Self::VertexData,
    ) -> Self::VertexData;

    #[inline]
    fn vertex(&mut self, x: i32, y: i32, z: f32) -> (i32, i32, f32) {
        (x, y, z)
    }

    #[inline]
    fn fragment(&mut self, data: Self::VertexData) -> LinearRgb {
        let _ = data;
        LinearRgb::rgb(1.0, 1.0, 1.0)
    }
}

pub fn barycentric_lerp<T>(bcx: f32, bcy: f32, bcz: f32, d1: T, d2: T, d3: T) -> T
where
    T: core::ops::Add<T, Output = T> + core::ops::Mul<f32, Output = T>,
{
    (d1 * bcx) + (d2 * bcy) + (d3 * bcz)
}

#[derive(Debug, Clone, Copy)]
pub struct FnShader<V, F, D>(V, F, PhantomData<D>);

impl<V, F, D> FnShader<V, F, D> {
    pub fn new(vertex: V, fragment: F) -> Self {
        Self(vertex, fragment, PhantomData)
    }
}

impl<V, F, D> Shader for FnShader<V, F, D>
where
    V: FnMut(i32, i32, f32) -> (i32, i32, f32),
    F: FnMut(D) -> LinearRgb,
    D: Copy + core::ops::Add<D, Output = D> + core::ops::Mul<f32, Output = D>,
{
    type VertexData = D;

    #[inline]
    fn interpolate(
        &self,
        bcx: f32,
        bcy: f32,
        bcz: f32,
        d1: Self::VertexData,
        d2: Self::VertexData,
        d3: Self::VertexData,
    ) -> Self::VertexData {
        barycentric_lerp(bcx, bcy, bcz, d1, d2, d3)
    }

    #[inline]
    fn vertex(&mut self, x: i32, y: i32, z: f32) -> (i32, i32, f32) {
        self.0(x, y, z)
    }

    #[inline]
    fn fragment(&mut self, data: Self::VertexData) -> LinearRgb {
        self.1(data)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ColorShader;

impl Shader for ColorShader {
    type VertexData = LinearRgb;

    #[inline]
    fn interpolate(
        &self,
        bcx: f32,
        bcy: f32,
        bcz: f32,
        d1: Self::VertexData,
        d2: Self::VertexData,
        d3: Self::VertexData,
    ) -> Self::VertexData {
        barycentric_lerp(bcx, bcy, bcz, d1, d2, d3)
    }

    #[inline]
    fn fragment(&mut self, data: Self::VertexData) -> LinearRgb {
        data
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TextureShader<'a, T> {
    pub texture: &'a [T],
    pub width: usize,
    pub height: usize,
    pub sampler: Sampler,
}

#[derive(Debug, Clone, Copy)]
pub enum Sampler {
    Nearest,
    Bilinear,
}

impl<T> Shader for TextureShader<'_, T>
where
    T: Copy + Color,
{
    type VertexData = (f32, f32);

    fn interpolate(
        &self,
        bcx: f32,
        bcy: f32,
        bcz: f32,
        d1: Self::VertexData,
        d2: Self::VertexData,
        d3: Self::VertexData,
    ) -> Self::VertexData {
        let u = barycentric_lerp(bcx, bcy, bcz, d1.0, d2.0, d3.0);
        let v = barycentric_lerp(bcx, bcy, bcz, d1.1, d2.1, d3.1);
        (u, v)
    }

    fn fragment(&mut self, data: Self::VertexData) -> LinearRgb {
        let (u, v) = data;
        match self.sampler {
            Sampler::Nearest => {
                let x = libm::roundf(u * self.width as f32) as usize;
                let y = libm::roundf(v * self.height as f32) as usize;
                let len = self.texture.len().saturating_sub(1);
                self.texture[(y * self.height + x).clamp(0, len)].into()
            }
            Sampler::Bilinear => {
                // https://en.wikipedia.org/wiki/Bilinear_interpolation

                let xf = (u * self.width as f32).max(0.0);
                let yf = (v * self.height as f32).max(0.0);

                let x0 = (xf as usize).min(self.width - 1);
                let x1 = (x0 + 1).min(self.width - 1);
                let y0 = (yf as usize).min(self.height - 1);
                let y1 = (y0 + 1).min(self.height - 1);

                let dx = xf - x0 as f32;
                let dy = yf - y0 as f32;

                let c00: LinearRgb = self.texture[y0 * self.width + x0].into();
                let c10: LinearRgb = self.texture[y0 * self.width + x1].into();
                let c01: LinearRgb = self.texture[y1 * self.width + x0].into();
                let c11: LinearRgb = self.texture[y1 * self.width + x1].into();

                let top = c00 * (1.0 - dx) + c10 * dx;
                let bottom = c01 * (1.0 - dx) + c11 * dx;
                top * (1.0 - dy) + bottom * dy
            }
        }
    }
}

pub mod empty {
    use crate::Shader;

    pub struct EmptyShader;
    impl Shader for EmptyShader {
        type VertexData = EmptyVertexData;
        fn interpolate(
            &self,
            _bcx: f32,
            _bcy: f32,
            _bcz: f32,
            _d1: Self::VertexData,
            _d2: Self::VertexData,
            _d3: Self::VertexData,
        ) -> Self::VertexData {
            EmptyVertexData
        }
    }
    #[derive(Clone, Copy)]
    pub struct EmptyVertexData;
    impl core::ops::Add for EmptyVertexData {
        type Output = Self;
        fn add(self, _: Self) -> Self::Output {
            EmptyVertexData
        }
    }
    impl core::ops::Mul<f32> for EmptyVertexData {
        type Output = Self;
        fn mul(self, _: f32) -> Self::Output {
            EmptyVertexData
        }
    }
}
