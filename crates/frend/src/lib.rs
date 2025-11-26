//! Minimal truetype font parser and rasterizer library.
//!
//! ## Render to bitmap
//! ```
//! let my_text = "Hello, world!";
//! let font_size = 24.0;
//! let sampling = frend::FontSampling::AntiAliased;
//!
//! let my_font = include_bytes!("../Hack-Regular.ttf");
//! // Stack allocated font data for further parsing
//! let ttf = frend::TtfData::new(my_font.as_slice());
//!
//! // Allocate bitmap for rasterization
//! let (width, height, stride) = ttf.minimum_bitmap_dimensions(my_text, font_size);
//! let mut bitmap = vec![0; width * height];
//!
//! let mut x = 0.0;
//! let y = 0;
//!
//! for codepoint in my_text.chars() {
//!     // Parse glyph data (heap allocation)
//!     let glyph = ttf.codepoint_glyph(codepoint as u16);
//!
//!     // Rasterize `glyph` into the bitmap
//!     frend::rasterize_glyph(
//!         &ttf,
//!         &mut bitmap,
//!         width,
//!         height,
//!         x as usize,
//!         y,
//!         font_size,
//!         sampling,
//!         &glyph,
//!     );
//!
//!     // Increment `x` by the stride
//!     x += stride * font_size;
//! }
//! ```
//!
//! ## Correcting the y-axis
//!
//! [`rasterize_glyph`] uses conventional screen space texture coordinates where
//! the top-left is (0.0, 0.0) and the bottom-right is (1.0, 1.0). Therefore, you
//! may want to correct the y-axis depending on how the bitmap will be used.
//!
//! ```
//! # extern crate alloc;
//! # use alloc::vec::Vec;
//! # let bitmap = [10];
//! # let width = 5;
//! let corrected = bitmap
//!     .chunks(width)
//!     .rev()
//!     .flat_map(|scanline| scanline)
//!     .collect::<Vec<_>>();
//! ```

#![allow(clippy::too_many_arguments)]
extern crate alloc;

/// Relevent truetype font data for extracting [`Glyph`] and shape metrics.
///
/// Constructing [`TtfData`] performs no allocation.
pub struct TtfData<'a> {
    pub head: Head,
    cmap: CMap<'a>,
    loca: Loca<'a>,
    glyf: Glyf<'a>,
    pub hhea: Hhea,
    hmtx: Hmtx<'a>,
}

impl<'a> TtfData<'a> {
    pub fn new(ttf_bytes: &'a [u8]) -> Self {
        new_ttf_data(ttf_bytes)
    }

    pub fn font_size_for_line_height(&self, line_height: f32) -> f32 {
        line_height / (self.hhea.ascent - self.hhea.descent) as f32 * self.head.units_per_em as f32
    }

    pub fn codepoint_glyph(&self, codepoint: u16) -> Glyph {
        codepoint_glyph(self, codepoint)
    }

    pub fn codepoint_metrics(&self, codepoint: u16) -> GlyphMetrics {
        codepoint_metrics(self, codepoint)
    }

    pub fn codepoint_glyph_and_metrics(&self, codepoint: u16) -> (Glyph, GlyphMetrics) {
        codepoint_glyph_and_metrics(self, codepoint)
    }

    pub fn vertical_metrics(&self) -> VerticalMetrics {
        vertical_metrics(self)
    }
}

/// Sampling algorithm used during [`rasterize_glyph`].
#[derive(Clone, Copy)]
pub enum FontSampling {
    /// Nearest sampling.
    BiLevel,
    /// Super sampling.
    AntiAliased,
}

/// Normalized vertical measurement metrics for a true type font.
#[derive(Debug, Clone, Copy)]
pub struct VerticalMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}

impl VerticalMetrics {
    /// Compute the line height in pixels given `font_size`.
    pub fn line_height(&self, font_size: f32) -> f32 {
        (self.ascent - self.descent + self.line_gap) * font_size
    }
}

/// Normalized measurement metrics for [`Glyph`] in pixels.
#[derive(Debug, Clone, Copy)]
pub struct GlyphMetrics {
    pub advance_width: f32,
    pub xmin: f32,
    pub ymin: f32,
    pub xmax: f32,
    pub ymax: f32,
}

/// Allocation of glyph contours for a codepoint.
#[derive(Debug, Clone)]
pub struct Glyph {
    points: Vec<(i16, i16, bool)>,
    end_pts_of_contours: Vec<usize>,
    min: (i16, i16),
    max: (i16, i16),
}

/// Rasterize `glyph` into `pixels` with a `font_size`, offset by `x` and `y`.
///
/// `x`, `y`, and `font_size` are measured in pixels.
// TODO: reuse allocations
pub fn rasterize_glyph<Pixel: From<u8>>(
    data: &TtfData,
    pixels: &mut [Pixel],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    font_size: f32,
    sampling: FontSampling,
    glyph: &Glyph,
) {
    assert!(pixels.len() >= width * height);
    if x >= width || y >= height {
        return;
    }

    let (xmin_glyph, ymin_glyph) = glyph.min;
    let (xmax_glyph, ymax_glyph) = glyph.max;

    let units_per_em = data.head.units_per_em as f32;
    let xmin = xmin_glyph as f32 * font_size / units_per_em;
    let ymin = ymin_glyph as f32 * font_size / units_per_em;
    let xmax = xmax_glyph as f32 * font_size / units_per_em;
    let ymax = ymax_glyph as f32 * font_size / units_per_em;

    let xmin_px = (xmin.floor() as i32 + x as i32).max(0);
    let ymin_px = (y as i32).max(0);
    let xmax_px = (xmax.floor() as i32 + x as i32).min((width - 1) as i32);
    let glyph_height_px = ymax - ymin;
    let ymax_px = ((y as f32 + glyph_height_px).floor() as i32).min((height - 1) as i32);

    assert!(xmin_px <= xmax_px);
    assert!(ymin_px <= ymax_px);

    let edges = build_edges(glyph);
    let font_size_inv = units_per_em / font_size;
    let baseline_y_px = y as f32 + ymax;

    for py in ymin_px..=ymax_px {
        let sy = (baseline_y_px - py as f32) * font_size_inv;
        for px in xmin_px..=xmax_px {
            let index = py as usize * width + px as usize;
            if index < pixels.len() {
                let sx = (px - x as i32) as f32 * font_size_inv;
                match sampling {
                    FontSampling::BiLevel => {
                        let s = point_in_glyph(&edges, sx, sy) as u8 * 255;
                        pixels[index] = Pixel::from(s);
                    }
                    FontSampling::AntiAliased => {
                        let mut accum = 0;
                        let offset = 0.25 * font_size_inv;
                        for suby in 0..4 {
                            for subx in 0..4 {
                                let sample_x = sx + (subx as f32 + 0.5) * offset;
                                let sample_y = sy - (suby as f32 + 0.5) * offset;
                                accum += point_in_glyph(&edges, sample_x, sample_y) as usize;
                            }
                        }
                        pixels[index] = Pixel::from(((accum * 255) / 16) as u8);
                    }
                }
            }
        }
    }
}

fn new_ttf_data<'a>(ttf_bytes: &'a [u8]) -> TtfData<'a> {
    let offset_subtable = OffsetSubtable::read_from_slice(ttf_bytes);
    assert!(offset_subtable.scalar_type == 0x00010000 || offset_subtable.scalar_type == 0x74727565);

    // table directory
    const MAX_TABLES: usize = 45;
    let mut tables = [TableEntry::default(); MAX_TABLES];
    assert!(offset_subtable.num_tables as usize <= MAX_TABLES);

    let input = &mut &ttf_bytes[core::mem::size_of::<OffsetSubtable>()..];
    for table in tables.iter_mut().take(offset_subtable.num_tables as usize) {
        let entry = cast_be_slice::<TableEntry, { core::mem::size_of::<TableEntry>() }>(input);

        *input = &input[core::mem::size_of::<TableEntry>()..];
        *table = entry;
    }

    let head = Head::new(ttf_bytes, &tables);
    let cmap = CMap::new(ttf_bytes, &tables);
    let loca = Loca::new(ttf_bytes, &tables);
    let glyf = Glyf::new(ttf_bytes, &tables);
    let hhea = Hhea::new(ttf_bytes, &tables);
    let hmtx = Hmtx::new(ttf_bytes, &tables, hhea.num_of_long_hor_metrics as usize);

    TtfData {
        head,
        cmap,
        loca,
        glyf,
        hhea,
        hmtx,
    }
}

// #[derive(Debug)]
// pub struct BitmapDimensions {}
//
// fn minimum_bitmap_dimensions(
//     data: &TtfData,
//     codepoints: &str,
//     font_size: f32,
// ) -> (usize, usize, f32, f32) {
//     let mut xmin = 0f32;
//     let mut xmax = 0f32;
//     let mut ymin = 0f32;
//     let mut ymax = 0f32;
//     let mut count = 0;
//     for codepoint in codepoints.chars() {
//         let metrics = data.codepoint_metrics(codepoint as u16);
//         xmin = xmin.min(metrics.xmin);
//         xmax = xmax.max(metrics.xmax);
//         ymin = ymin.min(metrics.ymin);
//         ymax = ymax.max(metrics.ymax);
//         count += 1;
//     }
//     println!("{xmin}");
//     println!("{xmax}");
//     println!("{ymin}");
//     println!("{ymax}");
//     let stride = (xmax - xmin) * font_size;
//     let width = (stride * count as f32).ceil() as usize;
//     let height = ((ymax - ymin) * font_size).ceil() as usize;
//
//     (width, height, stride)
// }

fn codepoint_glyph(data: &TtfData, codepoint: u16) -> Glyph {
    let (_, glyph_offset, glyph_offset_end) = codepoint_to_glyph_data(data, codepoint);
    data.glyf
        .glyph_offset_to_glyph(glyph_offset, glyph_offset_end)
}

fn codepoint_metrics(data: &TtfData, codepoint: u16) -> GlyphMetrics {
    let (glyph_index, glyph_offset, glyph_offset_end) = codepoint_to_glyph_data(data, codepoint);
    codepoint_metrics_from_glyph_data(data, glyph_index, glyph_offset, glyph_offset_end)
}

fn codepoint_metrics_from_glyph_data(
    data: &TtfData,
    glyph_index: u16,
    glyph_offset: usize,
    glyph_offset_end: usize,
) -> GlyphMetrics {
    let horizontal_metrics = data.hmtx.horizontal_metric(glyph_index);
    let ((xmin, ymin), (xmax, ymax)) = data
        .glyf
        .glyph_offset_to_min_max(glyph_offset, glyph_offset_end);
    let units_per_em = data.head.units_per_em as f32;

    GlyphMetrics {
        advance_width: horizontal_metrics.advance_width as f32 / units_per_em,
        xmin: xmin as f32 / units_per_em,
        ymin: ymin as f32 / units_per_em,
        xmax: xmax as f32 / units_per_em,
        ymax: ymax as f32 / units_per_em,
    }
}

fn codepoint_glyph_and_metrics(data: &TtfData, codepoint: u16) -> (Glyph, GlyphMetrics) {
    let (glyph_index, glyph_offset, glyph_offset_end) = codepoint_to_glyph_data(data, codepoint);
    let glyph = data
        .glyf
        .glyph_offset_to_glyph(glyph_offset, glyph_offset_end);
    let metrics =
        codepoint_metrics_from_glyph_data(data, glyph_index, glyph_offset, glyph_offset_end);

    (glyph, metrics)
}

fn codepoint_to_glyph_data(data: &TtfData, codepoint: u16) -> (u16, usize, usize) {
    let glyph_index = data.cmap.codepoint_to_glyph_index(codepoint);
    let glyph_offset = data.loca.glyph_index_to_glyph_offset(glyph_index);
    let glyph_offset_end = data.loca.glyph_index_to_glyph_offset(glyph_index + 1);
    (glyph_index, glyph_offset, glyph_offset_end)
}

// TODO: OS/2 metrics when?
fn vertical_metrics(data: &TtfData) -> VerticalMetrics {
    let units_per_em = data.head.units_per_em as f32;
    VerticalMetrics {
        ascent: data.hhea.ascent as f32 / units_per_em,
        descent: data.hhea.descent as f32 / units_per_em,
        line_gap: data.hhea.line_gap as f32 / units_per_em,
    }
}

enum Edge {
    Line {
        p1: (f32, f32),
        p2: (f32, f32),
    },
    Curve {
        p1: (f32, f32),
        c: (f32, f32),
        p2: (f32, f32),
    },
}

fn build_edges(glyph: &Glyph) -> Vec<Edge> {
    let mut edges = Vec::new();
    let mut start = 0;
    for end in glyph.end_pts_of_contours.iter().copied() {
        let points = &glyph.points[start..=end];
        start = end + 1;

        // if points starts with an off curve point, then this needs to be
        // considered in `normalized_points`
        assert!(points[0].2);

        let mut normalized_points: Vec<(f32, f32, bool)> = Vec::new();
        for p2 in points.iter() {
            if let Some(p1) = normalized_points.last()
                && !p1.2
                && !p2.2
            {
                let mx = (p1.0 + p2.0 as f32) / 2.0;
                let my = (p1.1 + p2.1 as f32) / 2.0;
                normalized_points.push((mx, my, true));
            }
            normalized_points.push((p2.0 as f32, p2.1 as f32, p2.2));
        }

        let mut i = 0;
        while i < normalized_points.len() {
            let p1 = normalized_points[i];
            let p2 = normalized_points[(i + 1) % normalized_points.len()];

            if p2.2 {
                assert!(p1.2);
                assert!(p2.2);
                edges.push(Edge::Line {
                    p1: (p1.0, p1.1),
                    p2: (p2.0, p2.1),
                });
                i += 1;
            } else {
                let p3 = normalized_points[(i + 2) % normalized_points.len()];
                assert!(p1.2);
                assert!(!p2.2);
                assert!(p3.2);
                edges.push(Edge::Curve {
                    p1: (p1.0, p1.1),
                    c: (p2.0, p2.1),
                    p2: (p3.0, p3.1),
                });
                i += 2;
            }
        }
    }
    edges
}

fn point_in_glyph(edges: &[Edge], px: f32, py: f32) -> bool {
    let mut winding = 0;
    for edge in edges.iter() {
        let (p1, p2) = match edge {
            Edge::Line { p1, p2 } => (p1, p2),
            Edge::Curve { p1, c, p2 } => {
                let p1x = p1.0;
                let p1y = p1.1;
                let cx = c.0;
                let cy = c.1;
                let p2x = p2.0;
                let p2y = p2.1;

                if let Some((u1, u2, d)) =
                    barycentric_coordinates(px, py, p1x, p1y, cx, cy, p2x, p2y)
                {
                    let u = u1 / d;
                    let v = u2 / d;
                    let w = (d - u1 - u2) / d;
                    assert!((u + v + w - 1.0).abs() < 0.00001);

                    let crossz = (p1x - cx) * (p2y - cy) - (p1y - cy) * (p2x - cx);
                    let u = v * 0.5 + w;
                    let v = w;
                    let in_curve = if crossz < 0.0 { v - u * u } else { u * u - v };

                    if in_curve < 0.0 {
                        winding += 1;
                    } else {
                        winding -= 1;
                    }
                }

                (p1, p2)
            }
        };

        let p1x = p1.0;
        let p1y = p1.1;
        let p2x = p2.0;
        let p2y = p2.1;

        if p1y <= py && p2y > py {
            // Crossing from top to bottom
            let dir = (p1x - px) * (p2y - py) - (p2x - px) * (p1y - py);
            if dir > 0.0 {
                winding -= 1;
            }
        } else if p1y > py && p2y <= py {
            // Crossing from bottom to top
            let dir = (p1x - px) * (p2y - py) - (p2x - px) * (p1y - py);
            if dir < 0.0 {
                winding += 1;
            }
        }
    }

    winding > 0
}

#[allow(clippy::too_many_arguments)]
fn barycentric_coordinates(
    px: f32,
    py: f32,
    v1x: f32,
    v1y: f32,
    v2x: f32,
    v2y: f32,
    v3x: f32,
    v3y: f32,
) -> Option<(f32, f32, f32)> {
    // Thank you zozin for this strange integer barycentric math:
    // https://github.com/tsoding/olive.c/blob/master/olive.c

    let d = (v1x - v3x) * (v2y - v3y) - (v1y - v3y) * (v2x - v3x);
    if d.abs() < f32::EPSILON {
        return None;
    }

    let u = (px - v3x) * (v2y - v3y) - (py - v3y) * (v2x - v3x);
    let v = (px - v3x) * (v3y - v1y) - (py - v3y) * (v3x - v1x);
    let w = d - u - v;

    let ds = d.signum();
    ((u.signum() == ds || u.abs() < f32::EPSILON)
        && (v.signum() == ds || v.abs() < f32::EPSILON)
        && (w.signum() == ds || w.abs() < f32::EPSILON))
        .then_some((u, v, d))
}

fix_endianness! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct Fixed {
        pub integer: i16,
        pub fract: i16,
    }
}

fix_endianness! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct OffsetSubtable {
        scalar_type: u32,
        num_tables: u16,
        search_range: u16,
        entry_selector: u16,
        range_shift: u16,
    }
}

fix_endianness! {
    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    pub struct TableEntry {
        tag: u32,
        check_sum: u32,
        offset: u32,
        length: u32,
    }
}

fix_endianness! {
    #[repr(C)]
    #[repr(packed(2))]
    #[derive(Debug, Clone, Copy)]
    pub struct Head {
        pub version: Fixed,
        pub font_revision: Fixed,
        check_sum_adjustment: u32,
        magic_number: u32,
        flats: u16,
        pub units_per_em: u16,
        pub created: u64,
        pub modified: u64,
        pub xmin: i16,
        pub ymin: i16,
        pub xmax: i16,
        pub ymax: i16,
        pub mac_style: u16,
        pub lowest_rec_ppem: u16,
        font_direction_hint: i16,
        index_to_loc_format: i16,
        glyph_data_format: i16,
    }
}

impl Head {
    const HEAD: u32 = tag!(b'h', b'e', b'a', b'd');

    fn new(ttf_bytes: &[u8], tables: &[TableEntry]) -> Self {
        let head = read_table::<Head>(ttf_bytes, tables, Self::HEAD);
        let magic_number = head.magic_number;
        let loc_format = head.index_to_loc_format;
        assert_eq!(magic_number, 0x5F0F3CF5);
        assert_eq!(loc_format, 1);
        head
    }
}

fix_endianness! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct CMapIndex {
        version: u16,
        number_subtables: u16,
    }
}

fix_endianness! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct CMapEncodingSubtable {
        platform_id: u16,
        platform_specific_id: u16,
        offset: u32,
    }
}

fix_endianness! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct CMapFormat4 {
        format: u16,
        length: u16,
        language: u16,
        seg_count_x2: u16,
        search_range: u16,
        entry_selector: u16,
        range_shift: u16,
    }
}

fix_endianness! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct GlyphDesc {
        number_of_contours: i16,
        xmin: i16,
        ymin: i16,
        xmax: i16,
        ymax: i16,
    }
}

pub struct CMap<'a> {
    end_code: &'a [u8],
    start_code: &'a [u8],
    id_delta: &'a [u8],
    id_range_offset: &'a [u8],
    glyph_index_array: &'a [u8],
}

impl<'a> CMap<'a> {
    const CMAP: u32 = tag!(b'c', b'm', b'a', b'p');

    fn new(ttf_bytes: &'a [u8], tables: &[TableEntry]) -> Self {
        let cmap_index = read_table::<CMapIndex>(ttf_bytes, tables, Self::CMAP);
        let version = cmap_index.version;
        assert_eq!(version, 0);

        let cmap_table_base = table_entry(tables, Self::CMAP).offset as usize;
        let subtables_offset = cmap_table_base + core::mem::size_of::<CMapIndex>();
        let input = &mut &ttf_bytes[subtables_offset..];
        let mut cmap_format_offset = 0;

        for _ in 0..cmap_index.number_subtables {
            let subtable = CMapEncodingSubtable::read_from_slice(input);
            *input = &input[core::mem::size_of::<CMapEncodingSubtable>()..];

            const UNICODE: u16 = 0;
            const MICROSOFT: u16 = 3;
            const VERSION_1_1: u16 = 1;
            const BMP_ONLY: u16 = 3;

            if (subtable.platform_id == UNICODE && subtable.platform_specific_id == BMP_ONLY)
                || (subtable.platform_id == MICROSOFT
                    && subtable.platform_specific_id == VERSION_1_1)
            {
                cmap_format_offset = cmap_table_base + subtable.offset as usize;
                break;
            }
        }

        if cmap_format_offset == 0 {
            panic!("failed to find unicode bmp ttf cmap format");
        }

        // Now read the format 4 table at the correct offset
        let cmap = CMapFormat4::read_from_slice(&ttf_bytes[cmap_format_offset..]);
        let format = cmap.format;
        assert_eq!(format, 4);

        let seg_count = cmap.seg_count_x2 as usize;
        assert!(seg_count.is_multiple_of(2));
        let start = cmap_format_offset + core::mem::size_of::<CMapFormat4>();
        let end = start + cmap.length as usize;
        let cmap_bytes = &mut &ttf_bytes[start..end];

        let end_code = &cmap_bytes[..seg_count];
        assert!(end_code.last().is_some_and(|v| *v == 0xFF));
        // reserve pad
        *cmap_bytes = &cmap_bytes[seg_count + 2..];

        let start_code = &cmap_bytes[..seg_count];
        *cmap_bytes = &cmap_bytes[seg_count..];

        let id_delta = &cmap_bytes[..seg_count];
        *cmap_bytes = &cmap_bytes[seg_count..];

        let id_range_offset = &cmap_bytes[..seg_count];
        *cmap_bytes = &cmap_bytes[seg_count..];

        let glyph_index_array = cmap_bytes;

        Self {
            end_code,
            start_code,
            id_delta,
            id_range_offset,
            glyph_index_array,
        }
    }

    fn codepoint_to_glyph_index(&self, c: u16) -> u16 {
        let mut i = 0;
        let end_code_len = self.end_code.len() / 2;
        while i < end_code_len {
            if self.end_code(i) >= c {
                break;
            }
            i += 1
        }

        assert!(i < end_code_len);
        let byte_offset_from_id_offset = self.id_range_offset(i);
        if byte_offset_from_id_offset == 0 {
            self.id_delta(i).wrapping_add(c)
        } else {
            let id_range_offset_len = self.id_range_offset.len() / 2;
            let offs_from_loc =
                (byte_offset_from_id_offset / 2).wrapping_add(c.wrapping_sub(self.start_code(i)));
            let dist_to_end = id_range_offset_len - i;
            let glyph_index_index = offs_from_loc as usize - dist_to_end;
            self.glyph_index_array(glyph_index_index)
                .wrapping_add(self.id_delta(i))
        }
    }

    fn end_code(&self, index: usize) -> u16 {
        self.u16_at_index(self.end_code, index)
    }

    fn start_code(&self, index: usize) -> u16 {
        self.u16_at_index(self.start_code, index)
    }

    fn id_delta(&self, index: usize) -> u16 {
        self.u16_at_index(self.id_delta, index)
    }

    fn id_range_offset(&self, index: usize) -> u16 {
        self.u16_at_index(self.id_range_offset, index)
    }

    fn glyph_index_array(&self, index: usize) -> u16 {
        self.u16_at_index(self.glyph_index_array, index)
    }

    fn u16_at_index(&self, be_bytes: &[u8], index: usize) -> u16 {
        let i = index * 2;
        u16::from_be_bytes([be_bytes[i], be_bytes[i + 1]])
    }
}

pub struct Loca<'a> {
    loca_table: &'a [u8],
}

impl<'a> Loca<'a> {
    const LOCA: u32 = tag!(b'l', b'o', b'c', b'a');

    fn new(ttf_bytes: &'a [u8], tables: &[TableEntry]) -> Self {
        let table = table_entry(tables, Self::LOCA);
        let offset = table.offset as usize;
        let length = table.length as usize;

        Self {
            loca_table: &ttf_bytes[offset..offset + length],
        }
    }

    fn glyph_index_to_glyph_offset(&self, glyph_index: u16) -> usize {
        let index = glyph_index as usize * 4;
        cast_be_slice::<u32, 4>(&self.loca_table[index..]) as usize
    }
}

pub struct Glyf<'a> {
    glyf_table: &'a [u8],
}

impl<'a> Glyf<'a> {
    const GLYF: u32 = tag!(b'g', b'l', b'y', b'f');

    fn new(ttf_bytes: &'a [u8], tables: &[TableEntry]) -> Self {
        let table = table_entry(tables, Self::GLYF);
        let offset = table.offset as usize;
        let length = table.length as usize;

        Self {
            glyf_table: &ttf_bytes[offset..offset + length],
        }
    }

    fn glyph_offset_to_desc(&self, offset: usize, len: usize) -> Option<GlyphDesc> {
        let start = offset;
        let desc_slice = &self.glyf_table[start..len];
        if desc_slice.is_empty() {
            return None;
        }

        let desc = GlyphDesc::read_from_slice(desc_slice);
        assert!(
            desc.number_of_contours >= 0,
            "compound glyphs not supported"
        );
        Some(desc)
    }

    fn glyph_offset_to_min_max(&self, offset: usize, len: usize) -> ((i16, i16), (i16, i16)) {
        self.glyph_offset_to_desc(offset, len)
            .map(|d| ((d.xmin, d.ymin), (d.xmax, d.ymax)))
            .unwrap_or(((0, 0), (0, 0)))
    }

    fn glyph_offset_to_glyph(&self, offset: usize, len: usize) -> Glyph {
        let start = offset;
        let desc = match self.glyph_offset_to_desc(offset, len) {
            Some(desc) => desc,
            None => {
                return Glyph {
                    points: Vec::new(),
                    end_pts_of_contours: Vec::new(),
                    min: (0, 0),
                    max: (0, 0),
                };
            }
        };

        let end_pts_start = start + core::mem::size_of::<GlyphDesc>();
        let end_pts_end = end_pts_start + desc.number_of_contours as usize * 2;
        let end_pts_of_contours = self.glyf_table[end_pts_start..end_pts_end]
            .chunks(2)
            .map(|slice| u16::from_be_bytes([slice[0], slice[1]]))
            .collect::<Vec<_>>();

        let instr_len = u16::from_be_bytes([
            self.glyf_table[end_pts_end],
            self.glyf_table[end_pts_end + 1],
        ]);

        let num_points = end_pts_of_contours[end_pts_of_contours.len() - 1] as usize + 1;

        let mut flags = Vec::with_capacity(num_points);
        let mut flag_index = end_pts_end + 2 + instr_len as usize;
        while flags.len() < num_points {
            let flag = self.glyf_table[flag_index];
            assert_eq!(flag & 0b10000000u8, 0);
            flags.push(flag);
            flag_index += 1;
            // repeat
            if (flag & 0b1000) > 0 {
                let repeat = self.glyf_table[flag_index];
                flag_index += 1;
                flags.extend((0..repeat).map(|_| flag));
            }
        }

        let mut x_index = flag_index;
        let mut xpoints = vec![0; num_points];
        for (i, flag) in flags.iter().enumerate() {
            let x_is_byte = (flag & 0b010) > 0;
            let x_extra = (flag & 0b010000) > 0;

            match (x_is_byte, x_extra) {
                (false, false) => {
                    // x is i16 delta
                    let x = i16::from_be_bytes([
                        self.glyf_table[x_index],
                        self.glyf_table[x_index + 1],
                    ]);
                    x_index += 2;
                    xpoints[i] = x;
                }
                (false, true) => {
                    // x is i16 repeat
                    xpoints[i] = 0;
                }
                (true, false) => {
                    // x is i8 delta
                    let x = self.glyf_table[x_index];
                    x_index += 1;
                    xpoints[i] = -(x as i16);
                }
                (true, true) => {
                    // x is u8 delta
                    let x = self.glyf_table[x_index];
                    x_index += 1;
                    xpoints[i] = x as i16;
                }
            }
        }

        let mut y_index = x_index;
        let mut ypoints = vec![0; num_points];
        for (i, flag) in flags.iter().enumerate() {
            let y_is_byte = (flag & 0b100) > 0;
            let y_eytra = (flag & 0b100000) > 0;

            match (y_is_byte, y_eytra) {
                (false, false) => {
                    // y is i16 delta
                    let y = i16::from_be_bytes([
                        self.glyf_table[y_index],
                        self.glyf_table[y_index + 1],
                    ]);
                    y_index += 2;
                    ypoints[i] = y;
                }
                (false, true) => {
                    // y is i16 repeat
                    ypoints[i] = 0;
                }
                (true, false) => {
                    // y is i8 delta
                    let y = self.glyf_table[y_index];
                    y_index += 1;
                    ypoints[i] = -(y as i16);
                }
                (true, true) => {
                    // y is u8 delta
                    let y = self.glyf_table[y_index];
                    y_index += 1;
                    ypoints[i] = y as i16;
                }
            }
        }
        let mut accumx = 0;
        let mut accumy = 0;
        Glyph {
            points: xpoints
                .iter()
                .zip(ypoints.iter())
                .zip(flags.iter())
                .map(|((x, y), f)| {
                    accumx += *x;
                    accumy += *y;
                    (accumx, accumy, (f & 0b1) > 0)
                })
                .collect(),
            end_pts_of_contours: end_pts_of_contours.iter().map(|e| *e as usize).collect(),
            min: (desc.xmin, desc.ymin),
            max: (desc.xmax, desc.ymax),
        }
    }
}

fix_endianness! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct Hhea {
        pub version: Fixed,
        pub ascent: i16,
        pub descent: i16,
        pub line_gap: i16,
        pub advance_width_max: u16,
        pub min_left_side_bearing: i16,
        pub min_right_side_breaing: i16,
        pub xmax_extent: i16,
        caret_slope_rise: i16,
        caret_slope_run: i16,
        caret_offset: i16,
        //
        res0: i16,
        res1: i16,
        res2: i16,
        res3: i16,
        //
        metric_data_format: i16,
        num_of_long_hor_metrics: u16,
    }
}

impl Hhea {
    const HHEA: u32 = tag!(b'h', b'h', b'e', b'a');

    fn new(ttf_bytes: &[u8], tables: &[TableEntry]) -> Self {
        let hhea = read_table::<Self>(ttf_bytes, tables, Self::HHEA);
        assert_eq!(hhea.version.integer, 1);
        assert_eq!(hhea.res0, 0);
        assert_eq!(hhea.res1, 0);
        assert_eq!(hhea.res2, 0);
        assert_eq!(hhea.res3, 0);
        assert_eq!(hhea.metric_data_format, 0);
        hhea
    }
}

#[derive(Debug, Clone)]
pub struct Hmtx<'a> {
    horizontal_metrics: &'a [u8],
    num_of_long_hor_metrics: usize,
}

impl<'a> Hmtx<'a> {
    const HMTX: u32 = tag!(b'h', b'm', b't', b'x');

    fn new(ttf_bytes: &'a [u8], tables: &[TableEntry], num_of_long_hor_metrics: usize) -> Self {
        let hmtx_entry = table_entry(tables, Self::HMTX);
        let horizontal_metrics = &ttf_bytes
            [hmtx_entry.offset as usize..(hmtx_entry.offset + hmtx_entry.length) as usize];

        Self {
            horizontal_metrics,
            num_of_long_hor_metrics,
        }
    }

    fn horizontal_metric(&self, glyph_index: u16) -> HorizontalMetric {
        let glyph_index = glyph_index as usize;
        let metric_size = core::mem::size_of::<HorizontalMetric>();
        let offset = glyph_index * metric_size;
        assert!(glyph_index < self.num_of_long_hor_metrics);
        cast_be_slice::<HorizontalMetric, { core::mem::size_of::<HorizontalMetric>() }>(
            &self.horizontal_metrics[offset..offset + metric_size],
        )

        // TODO: https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6hmtx.html
        // if glyph_index < self.num_of_long_hor_metrics {
        //     cast_be_slice::<HorizontalMetric, { core::mem::size_of::<HorizontalMetric>() }>(
        //         &self.horizontal_metrics[offset..offset + metric_size],
        //     )
        // } else {
        //     HorizontalMetric {
        //         advance_width: 1000,
        //         left_side_bearing: 0,
        //     }
        // }
    }
}

fix_endianness! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct HorizontalMetric {
        advance_width: u16,
        left_side_bearing: i16,
    }
}

trait ReadFromSlice {
    fn read_from_slice(slice: &[u8]) -> Self;
}

trait FixEndianness {
    fn fix_endianness(self) -> Self;
}

#[macro_export]
macro_rules! fix_endianness {
    {
        $(#[$attrs:meta])*
        // TODO: visibility
        pub struct $ident:ident {
            $($vis:vis $field:ident: $field_ty:tt,)*
        }
    } => {
        $(#[$attrs])*
        pub struct $ident {
            $($vis $field: $field_ty,)*
        }

        impl ReadFromSlice for $ident {
            fn read_from_slice(slice: &[u8]) -> Self {
                cast_be_slice::<Self, { core::mem::size_of::<Self>() }>(slice)
            }
        }

        impl FixEndianness for $ident {
            fn fix_endianness(self) -> Self {
                Self {
                    $($field: self.$field.fix_endianness(),)*
                }
            }
        }
    };
}

#[macro_export]
macro_rules! fix_endianness_prim {
    ($ident:ident) => {
        impl FixEndianness for $ident {
            fn fix_endianness(self) -> Self {
                Self::from_be_bytes(self.to_le_bytes())
            }
        }
    };
}

fix_endianness_prim!(i16);
fix_endianness_prim!(u16);
fix_endianness_prim!(u32);
fix_endianness_prim!(u64);

#[macro_export]
macro_rules! tag {
    ($($letter:literal),*) => {
        u32::from_be_bytes([$($letter),*])
    };
}

fn read_table<T: Copy + FixEndianness + ReadFromSlice>(
    bytes: &[u8],
    tables: &[TableEntry],
    tag: u32,
) -> T {
    let entry = tables
        .iter()
        .find(|entry| entry.tag == tag)
        .copied()
        .unwrap();
    let table = &bytes[entry.offset as usize..(entry.offset + entry.length) as usize];
    T::read_from_slice(table)
}

fn table_entry(tables: &[TableEntry], tag: u32) -> TableEntry {
    tables
        .iter()
        .find(|entry| entry.tag == tag)
        .copied()
        .unwrap()
}

fn cast_be_slice<T: Copy + FixEndianness, const SIZE: usize>(slice: &[u8]) -> T {
    let mut slice_be_bytes: [u8; SIZE] = [0; SIZE];
    slice_be_bytes.copy_from_slice(&slice[..SIZE]);
    let ptr = slice_be_bytes.as_ptr() as *const T;
    unsafe { *ptr }.fix_endianness()
}

// TODO: support open type
// fix_endianness! {
//     #[repr(C)]
//     #[derive(Debug, Clone, Copy)]
//     pub struct GsubHeader {
//         major_version: u16,
//         minor_version: u16,
//         script_list_offset: u16,
//         feature_list_offset: u16,
//         lookup_list_offset: u16,
//     }
// }
//
// impl GsubHeader {
//     const GSUB: u32 = tag!(b'G', b'S', b'U', b'B');
//
//     fn new(ttf_bytes: &[u8], tables: &[TableEntry]) -> Self {
//         let header = read_table::<Self>(ttf_bytes, tables, Self::GSUB);
//         assert_eq!(header.major_version, 1);
//         assert!(header.minor_version == 0 || header.minor_version == 1);
//         header
//     }
// }
//
// #[derive(Debug, Clone, Copy)]
// pub struct Gsub<'a> {
//     header: GsubHeader,
//     lookup_count: u16,
//     offsets: &'a [u8],
//     lookup_list_offset: usize,
//     ttf_bytes: &'a [u8],
// }
//
// impl<'a> Gsub<'a> {
//     fn new(ttf_bytes: &'a [u8], tables: &[TableEntry]) -> Self {
//         let header = GsubHeader::new(ttf_bytes, tables);
//         let header_offset = table_entry(tables, GsubHeader::GSUB).offset as usize;
//         let lookup_offset = header_offset + header.lookup_list_offset as usize;
//         let lookup_count =
//             u16::from_be_bytes([ttf_bytes[lookup_offset], ttf_bytes[lookup_offset + 1]]);
//         let offsets = lookup_offset + 2;
//         let offsets_size = lookup_count as usize * core::mem::size_of::<u16>();
//
//         Self {
//             header,
//             lookup_count,
//             offsets: &ttf_bytes[offsets..offsets + offsets_size],
//             lookup_list_offset: lookup_offset,
//             ttf_bytes,
//         }
//     }
//
//     fn lookup_subtables(&self) {
//         for i in 0..self.lookup_count as usize {
//             let offset_offset = i * 2;
//             let lookup_table_offset = self.lookup_list_offset
//                 + cast_be_slice::<u16, 2>(&self.offsets[offset_offset..]) as usize;
//             let lookup_header =
//                 LookupHeader::read_from_slice(&self.ttf_bytes[lookup_table_offset..]);
//
//             // single lookup
//             if lookup_header.lookup_type != 1 {
//                 continue;
//             }
//
//             let subtable_start_offset = lookup_table_offset + core::mem::size_of::<LookupHeader>();
//             for i in 0..lookup_header.sub_table_count as usize {
//                 let offset_pos = subtable_start_offset + i * 2;
//
//                 let subtable_offset = lookup_table_offset
//                     + cast_be_slice::<u16, 2>(&self.ttf_bytes[offset_pos..]) as usize;
//                 let format = cast_be_slice::<u16, 2>(&self.ttf_bytes[subtable_offset..]);
//
//                 // single substitution format 1
//                 if format == 1 {
//                     let coverage_offset =
//                         cast_be_slice::<u16, 2>(&self.ttf_bytes[subtable_offset + 2..]);
//                     let delta = cast_be_slice::<i16, 2>(&self.ttf_bytes[subtable_offset + 4..]);
//
//                     println!("subtable_offset: {subtable_offset}");
//                     println!("  coverage_offset: {coverage_offset}");
//                     println!("  delta: {delta}");
//
//                     let coverage_absolute_offset = coverage_offset as usize + subtable_offset;
//                     let coverage_format =
//                         cast_be_slice::<u16, 2>(&self.ttf_bytes[coverage_absolute_offset..]);
//                     println!("    coverage_format: {coverage_format}");
//
//                     match coverage_format {
//                         1 => {
//                             let glyph_count = cast_be_slice::<u16, 2>(
//                                 &self.ttf_bytes[coverage_absolute_offset + 2..],
//                             );
//                             println!("    glyph_count: {glyph_count}");
//                         }
//                         2 => {
//                             let range_count = cast_be_slice::<u16, 2>(
//                                 &self.ttf_bytes[coverage_absolute_offset + 2..],
//                             );
//                             println!("    range_count: {range_count}");
//                         }
//                         format => panic!("invalid coverage format {format}"),
//                     }
//                 }
//             }
//         }
//     }
// }
//
// fix_endianness! {
//     #[repr(C)]
//     #[derive(Debug, Clone, Copy)]
//     pub struct LookupHeader {
//         lookup_type: u16,
//         lookup_flag: u16,
//         sub_table_count: u16,
//     }
// }
