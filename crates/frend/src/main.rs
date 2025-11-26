use frend::rasterize_glyph;

fn main() {
    let my_text = "ab";
    let font_size = 48.0;
    let sampling = frend::FontSampling::AntiAliased;

    let my_font = include_bytes!("../Hack-Regular.ttf");
    // Stack allocated font data for further parsing
    let ttf = frend::TtfData::new(my_font.as_slice());

    let width = 400;
    let height = 400;
    let mut bitmap = vec![0; width * height];

    let codepoint_start = my_text.chars().min().unwrap() as u16;
    let codepoint_count = 2;

    let mut x = 1f32;
    let mut y = 51f32;
    let mut ymax = 51f32;

    struct GlyphData {
        x1: usize,
        y1: usize,
        x2: usize,
        y2: usize,
        xadvance: f32,
        xoffset: f32,
        yoffset: f32,
    }

    let mut bitmap_glyphs = Vec::with_capacity(codepoint_count as usize);
    for codepoint in codepoint_start..codepoint_start + codepoint_count {
        let (glyph, metrics) = ttf.codepoint_glyph_and_metrics(codepoint);

        let glyph_width = (metrics.xmax - metrics.xmin) * font_size;
        let glyph_height = (metrics.ymax - metrics.ymin) * font_size;

        if x + glyph_width + 1.0 >= width as f32 {
            y = ymax;
            x = 1.0;
        }
        if y + glyph_height + 1.0 >= height as f32 {
            panic!("too many glyphs!");
        }

        rasterize_glyph(
            &ttf,
            &mut bitmap,
            width,
            height,
            x as usize,
            y as usize,
            font_size,
            sampling,
            &glyph,
        );

        bitmap_glyphs.push(GlyphData {
            x1: x as usize,
            y1: y as usize,
            x2: (x + glyph_width) as usize,
            y2: (y + glyph_height) as usize,
            xadvance: metrics.advance_width * font_size,
            xoffset: metrics.xmin * font_size,
            yoffset: metrics.ymin * font_size,
        });

        x += glyph_width + 1.0;
        ymax = ymax.max(y + glyph_height + 1.0);
    }

    let mut pixels = vec![0; width * height];
    let py = 40.0;
    let mut px = 0.0;
    for codepoint in my_text.chars() {
        let index = (codepoint as u16 - codepoint_start) as usize;
        let bglyph = &bitmap_glyphs[index];

        for (y, by) in (bglyph.y1..bglyph.y2).enumerate() {
            for (x, bx) in (bglyph.x1..bglyph.x2).enumerate() {
                let py = py as usize + y + bglyph.yoffset as usize;
                let px = x + px as usize + bglyph.xoffset as usize;
                let pindex = py * width + px;
                let bindex = by * width + bx;
                pixels[pindex] = bitmap[bindex];
            }
        }
        px += bglyph.xadvance;
    }

    let mut pixels = pixels
        .chunks(width)
        // .rev()
        .flat_map(|scanline| {
            scanline
                .iter()
                .map(|b| u32::from_le_bytes([*b, *b, *b, 255]))
        })
        .collect::<Vec<_>>();

    glazer::run((), &mut pixels, width, height, |_| {}, |_| {}, None);
}
