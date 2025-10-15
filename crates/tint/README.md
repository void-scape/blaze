# Tint

*Tint* is a `no_std` library that uses lookup tables for colorspace conversions.

## Example

```rust
let srgb = Srgb::rgb(r, g, b);
let mut linear = srgb.to_linear();
linear *= 0.5;
let srgb = linear.to_srgb();
```

## Motivation

I wrote a [software rasterizer called rast](https://github.com/void-scape/rast) 
that needed to convert between the sRGB and Linear RGB color spaces without crippling 
performance.

## Conversions

The `Srgb` to `LinearRgb` conversion is performed with a loss-less, 256 byte lookup table 
index for each color component (red, green, and blue). Likewise, the `LinearRgb` to `Srgb`
conversion uses a lossy 3KB lookup table.

Round trip conversions from `Srgb` to `LinearRgb` and back to `Srgb` are garaunteed
to have a maximum error of 1 for each color component.

## Performance

Conversions with `Hsv` are not optimized.

Converting between `Srgb` and `LinearRgb` is an order of magnitude faster than alternatives 
that directly compute gamma correction.
