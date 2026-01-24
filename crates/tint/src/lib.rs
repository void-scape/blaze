//! *Tint* is a `no_std` library that uses lookup tables for colorspace conversions.
//!
//! ## Example
//!
//! ```
//! # use tint::*;
//! # let r = 0;
//! # let g = 0;
//! # let b = 0;
//! let srgb = Srgb::from_rgb(r, g, b);
//! let mut linear = srgb.to_linear();
//! linear *= 0.5;
//! let srgb = linear.to_srgb();
//! ```
//!
//! ## Motivation
//!
//! I wrote a [software rasterizer called rast](https://github.com/void-scape/blaze)
//! that needed to convert between the sRGB and Linear RGB color spaces without crippling
//! performance.
//!
//! ## Conversions
//!
//! The `Srgb` to `LinearRgb` conversion is performed with a loss-less, 256 byte lookup table
//! index for each color component (red, green, and blue). Likewise, the `LinearRgb` to `Srgb`
//! conversion uses a lossy 3KB lookup table.
//!
//! Round trip conversions from `Srgb` to `LinearRgb` and back to `Srgb` are garaunteed
//! to have a maximum error of 1 for each color component.
//!
//! ## Performance
//!
//! Conversions with `Hsv` are not optimized.
//!
//! Converting between `Srgb` and `LinearRgb` is an order of magnitude faster than alternatives
//! that directly compute gamma correction.

#![no_std]

/// Representation independent color interface.
///
/// Implemented by all `tint` color primitives.
pub trait Color:
    Copy + From<Srgb> + From<LinearRgb> + From<Hsv> + Into<Srgb> + Into<LinearRgb> + Into<Hsv>
{
    fn to_srgb(self) -> Srgb;
    fn to_sbgr(self) -> Sbgr;
    fn to_linear(self) -> LinearRgb;
    fn to_hsv(self) -> Hsv;

    fn alpha(&self) -> f32;
    fn set_alpha(&mut self, alpha: f32);
}

/// A color within the [Linear sRGB colorspace].
///
/// Linear sRGB is *not* gamma encoded. The red, green, and blue component light
/// intensities *are* linear. Operations that expect to operate linearly over light
/// intensity, such as mixing, must occur in [linear space](LinearRgb).
///
/// [Linear sRGB colorspace]: https://facelessuser.github.io/coloraide/colors/srgb_linear
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LinearRgb {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

impl LinearRgb {
    #[inline]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
            a: a.clamp(0.0, 1.0),
        }
    }

    #[inline]
    pub const fn from_rgb(r: f32, g: f32, b: f32) -> Self {
        Self::new(r, g, b, 1.0)
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    #[inline]
    pub fn r(&self) -> f32 {
        self.r
    }

    #[inline]
    pub fn g(&self) -> f32 {
        self.g
    }

    #[inline]
    pub fn b(&self) -> f32 {
        self.b
    }

    #[inline]
    pub fn a(&self) -> f32 {
        self.a
    }
}

impl Color for LinearRgb {
    #[inline]
    #[track_caller]
    fn to_srgb(self) -> Srgb {
        debug_assert!(0.0 <= self.r && self.r <= 1.0);
        debug_assert!(0.0 <= self.g && self.g <= 1.0);
        debug_assert!(0.0 <= self.b && self.b <= 1.0);
        debug_assert!(0.0 <= self.a && self.a <= 1.0);

        const RANGE: f32 = (LINEAR_TO_SRGB_COMPONENT_LUT_SIZE - 1) as f32;
        Srgb::new(
            LINEAR_TO_SRGB_COMPONENT_LUT[(self.r * RANGE) as usize],
            LINEAR_TO_SRGB_COMPONENT_LUT[(self.g * RANGE) as usize],
            LINEAR_TO_SRGB_COMPONENT_LUT[(self.b * RANGE) as usize],
            (self.a.clamp(0.0, 1.0) * 255.0) as u8,
        )
    }

    #[inline]
    #[track_caller]
    fn to_sbgr(self) -> Sbgr {
        debug_assert!(0.0 <= self.r && self.r <= 1.0);
        debug_assert!(0.0 <= self.g && self.g <= 1.0);
        debug_assert!(0.0 <= self.b && self.b <= 1.0);
        debug_assert!(0.0 <= self.a && self.a <= 1.0);

        const RANGE: f32 = (LINEAR_TO_SRGB_COMPONENT_LUT_SIZE - 1) as f32;
        Sbgr::new(
            LINEAR_TO_SRGB_COMPONENT_LUT[(self.r * RANGE) as usize],
            LINEAR_TO_SRGB_COMPONENT_LUT[(self.g * RANGE) as usize],
            LINEAR_TO_SRGB_COMPONENT_LUT[(self.b * RANGE) as usize],
            (self.a.clamp(0.0, 1.0) * 255.0) as u8,
        )
    }

    #[inline]
    fn to_linear(self) -> LinearRgb {
        self
    }

    fn to_hsv(self) -> Hsv {
        debug_assert!(0.0 <= self.r && self.r <= 1.0);
        debug_assert!(0.0 <= self.g && self.g <= 1.0);
        debug_assert!(0.0 <= self.b && self.b <= 1.0);
        debug_assert!(0.0 <= self.a && self.a <= 1.0);

        // Derivation: https://en.wikipedia.org/wiki/HSL_and_HSV#From_RGB
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        let c = max - min;

        let mut h = if c == 0.0 {
            0.0
        } else if max == self.r {
            (((self.g - self.b) / c) % 6.0) * 60.0 / 360.0
        } else if max == self.g {
            (((self.b - self.r) / c) + 2.0) * 60.0 / 360.0
        } else {
            debug_assert_eq!(max, self.b);
            (((self.r - self.g) / c) + 4.0) * 60.0 / 360.0
        };
        let s = if max == 0.0 { 0.0 } else { c / max };
        let v = max;

        if h < 0.0 {
            h += 1.0;
        }

        debug_assert!((0.0..=1.0).contains(&h));
        debug_assert!((0.0..=1.0).contains(&s));
        debug_assert!((0.0..=1.0).contains(&v));

        Hsv { h, s, v, a: self.a }
    }

    fn alpha(&self) -> f32 {
        self.a
    }

    fn set_alpha(&mut self, alpha: f32) {
        self.a = alpha.clamp(0.0, 1.0);
    }
}

impl From<Srgb> for LinearRgb {
    fn from(value: Srgb) -> Self {
        value.to_linear()
    }
}

impl From<Sbgr> for LinearRgb {
    fn from(value: Sbgr) -> Self {
        value.to_linear()
    }
}

impl From<Hsv> for LinearRgb {
    fn from(value: Hsv) -> Self {
        value.to_linear()
    }
}

macro_rules! channel_wise {
    ($ident:ident, $fn:ident, $op:tt) => {
        impl core::ops::$ident for LinearRgb {
            type Output = Self;
            fn $fn(self, rhs: Self) -> Self::Output {
                Self::new(
                    (self.r $op rhs.r).clamp(0.0, 1.0),
                    (self.g $op rhs.g).clamp(0.0, 1.0),
                    (self.b $op rhs.b).clamp(0.0, 1.0),
                    self.a,
                )
            }
        }

        impl core::ops::$ident<f32> for LinearRgb {
            type Output = Self;
            fn $fn(self, rhs: f32) -> Self::Output {
                Self::new(
                    (self.r $op rhs).clamp(0.0, 1.0),
                    (self.g $op rhs).clamp(0.0, 1.0),
                    (self.b $op rhs).clamp(0.0, 1.0),
                    self.a,
                )
            }
        }
    };
}

channel_wise!(Add, add, +);
channel_wise!(Sub, sub, -);
channel_wise!(Mul, mul, *);
channel_wise!(Div, div, /);

macro_rules! channel_wise_assign {
    ($ident:ident, $fn:ident, $op:tt) => {
        impl core::ops::$ident for LinearRgb {
            fn $fn(&mut self, rhs: Self) {
                self.r = (self.r $op rhs.r).clamp(0.0, 1.0);
                self.g = (self.g $op rhs.g).clamp(0.0, 1.0);
                self.b = (self.b $op rhs.b).clamp(0.0, 1.0);
            }
        }

        impl core::ops::$ident<f32> for LinearRgb {
            fn $fn(&mut self, rhs: f32) {
                self.r = (self.r $op rhs).clamp(0.0, 1.0);
                self.g = (self.g $op rhs).clamp(0.0, 1.0);
                self.b = (self.b $op rhs).clamp(0.0, 1.0);
            }
        }
    };
}

channel_wise_assign!(AddAssign, add_assign, +);
channel_wise_assign!(SubAssign, sub_assign, -);
channel_wise_assign!(MulAssign, mul_assign, *);
channel_wise_assign!(DivAssign, div_assign, /);

macro_rules! rgb_swizzle {
    ($ident:ident, $c1:ident, $c2:ident, $c3:ident) => {
        /// A color within the [sRGB colorspace].
        ///
        /// sRGB is gamma encoded. The red, green, and blue component light intensities
        /// are [non-linear]. Operations that expect to operate linearly over light intensity,
        /// such as mixing, must occur in [linear space](LinearRgb).
        ///
        /// [sRGB colorspace]: https://en.wikipedia.org/wiki/SRGB
        /// [non-linear]: https://en.wikipedia.org/wiki/RGB_color_model#Nonlinearity
        #[repr(C)]
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct $ident {
            $c1: u8,
            $c2: u8,
            $c3: u8,
            a: u8,
        }

        impl $ident {
            #[inline]
            pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
                Self { r, g, b, a }
            }

            #[inline]
            pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
                Self::new(r, g, b, 255)
            }

            #[inline]
            pub const fn to_array(self) -> [u8; 4] {
                [self.$c1, self.$c2, self.$c3, self.a]
            }

            #[inline]
            pub fn r(&self) -> u8 {
                self.r
            }

            #[inline]
            pub fn g(&self) -> u8 {
                self.g
            }

            #[inline]
            pub fn b(&self) -> u8 {
                self.b
            }

            #[inline]
            pub fn a(&self) -> u8 {
                self.a
            }
        }
    };
}

rgb_swizzle!(Srgb, r, g, b);
impl Color for Srgb {
    #[inline]
    fn to_srgb(self) -> Srgb {
        self
    }

    #[inline]
    fn to_sbgr(self) -> Sbgr {
        Sbgr::new(self.r, self.g, self.b, self.a)
    }

    #[inline]
    fn to_linear(self) -> LinearRgb {
        LinearRgb {
            r: SRGB_TO_LINEAR_COMPONENT_LUT[self.r as usize],
            g: SRGB_TO_LINEAR_COMPONENT_LUT[self.g as usize],
            b: SRGB_TO_LINEAR_COMPONENT_LUT[self.b as usize],
            a: self.a as f32 / 255.0,
        }
    }

    fn to_hsv(self) -> Hsv {
        self.to_linear().to_hsv()
    }

    fn alpha(&self) -> f32 {
        self.a as f32 / u8::MAX as f32
    }

    fn set_alpha(&mut self, alpha: f32) {
        self.a = (alpha.clamp(0.0, 1.0) * u8::MAX as f32) as u8;
    }
}

impl From<Sbgr> for Srgb {
    #[track_caller]
    fn from(value: Sbgr) -> Self {
        value.to_srgb()
    }
}

impl From<LinearRgb> for Srgb {
    #[track_caller]
    fn from(value: LinearRgb) -> Self {
        value.to_srgb()
    }
}

impl From<Hsv> for Srgb {
    fn from(value: Hsv) -> Self {
        value.to_srgb()
    }
}

rgb_swizzle!(Sbgr, b, g, r);
impl Color for Sbgr {
    #[inline]
    fn to_srgb(self) -> Srgb {
        Srgb::new(self.r, self.g, self.b, self.a)
    }

    #[inline]
    fn to_sbgr(self) -> Sbgr {
        self
    }

    #[inline]
    fn to_linear(self) -> LinearRgb {
        LinearRgb {
            r: SRGB_TO_LINEAR_COMPONENT_LUT[self.r as usize],
            g: SRGB_TO_LINEAR_COMPONENT_LUT[self.g as usize],
            b: SRGB_TO_LINEAR_COMPONENT_LUT[self.b as usize],
            a: self.a as f32 / 255.0,
        }
    }

    fn to_hsv(self) -> Hsv {
        self.to_linear().to_hsv()
    }

    fn alpha(&self) -> f32 {
        self.a as f32 / u8::MAX as f32
    }

    fn set_alpha(&mut self, alpha: f32) {
        self.a = (alpha.clamp(0.0, 1.0) * u8::MAX as f32) as u8;
    }
}

impl From<Srgb> for Sbgr {
    fn from(value: Srgb) -> Self {
        Self::new(value.r, value.g, value.b, value.a)
    }
}

impl From<LinearRgb> for Sbgr {
    #[track_caller]
    fn from(value: LinearRgb) -> Self {
        Self::from(value.to_srgb())
    }
}

impl From<Hsv> for Sbgr {
    fn from(value: Hsv) -> Self {
        Self::from(value.to_srgb())
    }
}

/// A color within the [HSV colorspace].
///
/// [`Hsv`] is a cylindrical-coordinate representation of red, green, and blue
/// color components. Operations that expect to operate linearly over light intensity,
/// such as mixing, must occur in [linear space](LinearRgb).
///
/// [HSV colorspace]: https://en.wikipedia.org/wiki/HSL_and_HSV
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Hsv {
    h: f32,
    s: f32,
    v: f32,
    a: f32,
}

impl Hsv {
    #[inline]
    pub const fn new(h: f32, s: f32, v: f32, a: f32) -> Self {
        Self {
            h: h.clamp(0.0, 1.0),
            s: s.clamp(0.0, 1.0),
            v: v.clamp(0.0, 1.0),
            a: a.clamp(0.0, 1.0),
        }
    }

    #[inline]
    pub const fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        Self::new(h, s, v, 1.0)
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 4] {
        [self.h, self.s, self.v, self.a]
    }

    #[inline]
    pub fn h(&self) -> f32 {
        self.h
    }

    #[inline]
    pub fn s(&self) -> f32 {
        self.s
    }

    #[inline]
    pub fn v(&self) -> f32 {
        self.v
    }

    #[inline]
    pub fn a(&self) -> f32 {
        self.a
    }
}

impl Color for Hsv {
    fn to_srgb(self) -> Srgb {
        self.to_linear().to_srgb()
    }

    fn to_sbgr(self) -> Sbgr {
        self.to_linear().to_sbgr()
    }

    fn to_linear(self) -> LinearRgb {
        debug_assert!(0.0 <= self.h && self.h <= 1.0);
        debug_assert!(0.0 <= self.s && self.s <= 1.0);
        debug_assert!(0.0 <= self.v && self.v <= 1.0);
        debug_assert!(0.0 <= self.a && self.a <= 1.0);

        // Derivation: https://en.wikipedia.org/wiki/HSL_and_HSV#HSV_to_RGB
        let c = self.v * self.s;
        let hp = self.h * 6.0;
        let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
        let (r, g, b) = match hp {
            0.0..1.0 => (c, x, 0.0),
            1.0..2.0 => (x, c, 0.0),
            2.0..3.0 => (0.0, c, x),
            3.0..4.0 => (0.0, x, c),
            4.0..5.0 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };

        let m = self.v - c;
        debug_assert!(0.0 <= r + m && r + m <= 1.0);
        debug_assert!(0.0 <= g + m && g + m <= 1.0);
        debug_assert!(0.0 <= b + m && b + m <= 1.0);

        LinearRgb {
            r: r + m,
            g: g + m,
            b: b + m,
            a: self.a,
        }
    }

    fn to_hsv(self) -> Hsv {
        self
    }

    fn alpha(&self) -> f32 {
        self.a
    }

    fn set_alpha(&mut self, alpha: f32) {
        self.a = alpha.clamp(0.0, 1.0);
    }
}

impl From<Srgb> for Hsv {
    fn from(value: Srgb) -> Self {
        value.to_hsv()
    }
}

impl From<Sbgr> for Hsv {
    fn from(value: Sbgr) -> Self {
        value.to_hsv()
    }
}

impl From<LinearRgb> for Hsv {
    fn from(value: LinearRgb) -> Self {
        value.to_hsv()
    }
}

/// Lookup table for SRGB to LinearRGB color conversion.
///
/// Converts one SRGB component to a LinearRGB component. Since each SRGB
/// component is a byte, the table is populated with 256 values thereby making
/// this conversion lossless.
///
/// ## Conversion formula
/// ```ignore
/// // Derivation: https://en.wikipedia.org/wiki/SRGB#Transfer_function_(%22gamma%22)
/// # let srgb = 0;
/// let v = srgb as f32 / 255.0;
/// let l = if v <= 0.04045 {
///     v / 12.92
/// } else {
///     ((v + 0.055) / 1.055).pow(2.4)
/// };
/// ```
pub const SRGB_TO_LINEAR_COMPONENT_LUT: [f32; 256] = [
    0.0,
    0.000303527,
    0.000607054,
    0.000910581,
    0.001214108,
    0.001517635,
    0.001821162,
    0.0021246888,
    0.002428216,
    0.002731743,
    0.00303527,
    0.0033465356,
    0.003676507,
    0.004024717,
    0.004391442,
    0.0047769533,
    0.005181517,
    0.0056053917,
    0.0060488326,
    0.006512091,
    0.00699541,
    0.0074990317,
    0.008023192,
    0.008568125,
    0.009134057,
    0.009721218,
    0.010329823,
    0.010960094,
    0.011612245,
    0.012286487,
    0.012983031,
    0.013702081,
    0.014443844,
    0.015208514,
    0.015996292,
    0.016807375,
    0.017641952,
    0.018500218,
    0.019382361,
    0.020288562,
    0.02121901,
    0.022173883,
    0.023153365,
    0.02415763,
    0.025186857,
    0.026241222,
    0.027320892,
    0.028426038,
    0.029556833,
    0.03071344,
    0.03189603,
    0.033104762,
    0.034339808,
    0.035601314,
    0.036889445,
    0.038204364,
    0.039546236,
    0.0409152,
    0.04231141,
    0.043735027,
    0.045186203,
    0.046665084,
    0.048171822,
    0.049706563,
    0.051269468,
    0.052860655,
    0.05448028,
    0.056128494,
    0.057805434,
    0.05951124,
    0.06124607,
    0.06301004,
    0.06480328,
    0.06662595,
    0.06847818,
    0.07036011,
    0.07227186,
    0.07421358,
    0.07618539,
    0.07818743,
    0.08021983,
    0.082282715,
    0.084376216,
    0.086500466,
    0.088655606,
    0.09084173,
    0.09305898,
    0.095307484,
    0.09758736,
    0.09989874,
    0.10224175,
    0.10461649,
    0.10702311,
    0.10946172,
    0.111932434,
    0.11443538,
    0.11697067,
    0.119538434,
    0.1221388,
    0.12477184,
    0.1274377,
    0.13013649,
    0.13286833,
    0.13563335,
    0.13843162,
    0.1412633,
    0.14412849,
    0.14702728,
    0.1499598,
    0.15292616,
    0.15592647,
    0.15896086,
    0.1620294,
    0.16513222,
    0.1682694,
    0.1714411,
    0.17464739,
    0.17788841,
    0.18116423,
    0.18447499,
    0.18782076,
    0.19120167,
    0.19461781,
    0.1980693,
    0.20155624,
    0.2050787,
    0.20863685,
    0.21223073,
    0.21586053,
    0.21952623,
    0.22322798,
    0.22696589,
    0.23074007,
    0.23455065,
    0.23839766,
    0.2422812,
    0.2462014,
    0.25015837,
    0.25415218,
    0.2581829,
    0.26225072,
    0.26635566,
    0.27049786,
    0.27467737,
    0.27889434,
    0.2831488,
    0.2874409,
    0.2917707,
    0.29613832,
    0.30054384,
    0.30498737,
    0.30946895,
    0.31398875,
    0.31854683,
    0.32314324,
    0.32777813,
    0.33245158,
    0.33716366,
    0.34191445,
    0.3467041,
    0.3515327,
    0.35640025,
    0.36130688,
    0.3662527,
    0.37123778,
    0.37626222,
    0.3813261,
    0.38642952,
    0.39157256,
    0.3967553,
    0.40197787,
    0.4072403,
    0.4125427,
    0.41788515,
    0.42326775,
    0.42869055,
    0.4341537,
    0.43965724,
    0.44520125,
    0.45078585,
    0.4564111,
    0.46207705,
    0.46778384,
    0.47353154,
    0.47932023,
    0.48514998,
    0.4910209,
    0.49693304,
    0.5028866,
    0.50888145,
    0.5149178,
    0.5209957,
    0.5271152,
    0.5332765,
    0.5394796,
    0.5457246,
    0.5520115,
    0.5583405,
    0.56471163,
    0.5711249,
    0.5775805,
    0.5840785,
    0.5906189,
    0.5972019,
    0.6038274,
    0.6104956,
    0.61720663,
    0.62396044,
    0.6307572,
    0.63759696,
    0.64447975,
    0.6514057,
    0.65837485,
    0.66538733,
    0.6724432,
    0.67954254,
    0.68668544,
    0.6938719,
    0.701102,
    0.70837593,
    0.71569365,
    0.72305524,
    0.7304609,
    0.73791057,
    0.74540436,
    0.7529423,
    0.76052463,
    0.7681513,
    0.77582234,
    0.7835379,
    0.79129803,
    0.79910284,
    0.80695236,
    0.8148467,
    0.82278585,
    0.83076996,
    0.8387991,
    0.8468733,
    0.8549927,
    0.8631573,
    0.8713672,
    0.87962234,
    0.8879232,
    0.8962694,
    0.90466136,
    0.9130987,
    0.92158204,
    0.9301109,
    0.9386859,
    0.9473066,
    0.9559735,
    0.9646863,
    0.9734455,
    0.9822506,
    0.9911022,
    1.0,
];

/// Size of the [`LinearRgb`] to [`Srgb`] lookup table.
///
/// This is the smallest possible set of conversions that provides a 1-1
/// `sRGB -> Linear -> sRGB` and `sRGB -> HSV -> sRGB` conversion with a maximum
/// error of 1 for each color component.
///
/// Round tripping is an important metric since a simple texture sampler could
/// sample an [`Srgb`] from an image buffer, convert to a [`LinearRgb`] for interpolation,
/// then convert back into an [`Srgb`] to be displayed, written, or saved.
pub const LINEAR_TO_SRGB_COMPONENT_LUT_SIZE: usize = 3000;

/// Lookup table for LinearRGB to SRGB color conversion.
///
/// Converts one LinearRGB component to a SRGB component. Since each LinearRGB
/// component is a floating point number between 0.0 and 1.0, the conversion to
/// a 0 to 255 value is lossy.
///
/// ## Conversion formula
/// ```ignore
/// // Derivation: https://en.wikipedia.org/wiki/SRGB#Transfer_function_(%22gamma%22)
/// # let l = 0.0;
/// let s = if l <= 0.0031308 {
///     (l * 12.92 * 255.0) as u8
/// } else {
///     let srgb = l.clamp(0.0, 1.0).pow(1.0 / 2.4) * 1.055 - 0.055;
///     (srgb * 255.0) as u8
/// };
/// ```
pub const LINEAR_TO_SRGB_COMPONENT_LUT: [u8; LINEAR_TO_SRGB_COMPONENT_LUT_SIZE] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 17, 18, 19, 20, 20, 21, 21, 22,
    23, 23, 24, 24, 25, 26, 26, 27, 27, 28, 28, 29, 29, 30, 30, 30, 31, 31, 32, 32, 33, 33, 34, 34,
    34, 35, 35, 36, 36, 36, 37, 37, 37, 38, 38, 39, 39, 39, 40, 40, 40, 41, 41, 41, 42, 42, 42, 43,
    43, 43, 44, 44, 44, 45, 45, 45, 46, 46, 46, 46, 47, 47, 47, 48, 48, 48, 48, 49, 49, 49, 50, 50,
    50, 50, 51, 51, 51, 51, 52, 52, 52, 53, 53, 53, 53, 54, 54, 54, 54, 55, 55, 55, 55, 56, 56, 56,
    56, 57, 57, 57, 57, 58, 58, 58, 58, 58, 59, 59, 59, 59, 60, 60, 60, 60, 61, 61, 61, 61, 61, 62,
    62, 62, 62, 62, 63, 63, 63, 63, 64, 64, 64, 64, 64, 65, 65, 65, 65, 65, 66, 66, 66, 66, 66, 67,
    67, 67, 67, 67, 68, 68, 68, 68, 68, 69, 69, 69, 69, 69, 70, 70, 70, 70, 70, 70, 71, 71, 71, 71,
    71, 72, 72, 72, 72, 72, 73, 73, 73, 73, 73, 73, 74, 74, 74, 74, 74, 74, 75, 75, 75, 75, 75, 76,
    76, 76, 76, 76, 76, 77, 77, 77, 77, 77, 77, 78, 78, 78, 78, 78, 78, 79, 79, 79, 79, 79, 79, 80,
    80, 80, 80, 80, 80, 81, 81, 81, 81, 81, 81, 81, 82, 82, 82, 82, 82, 82, 83, 83, 83, 83, 83, 83,
    84, 84, 84, 84, 84, 84, 84, 85, 85, 85, 85, 85, 85, 85, 86, 86, 86, 86, 86, 86, 87, 87, 87, 87,
    87, 87, 87, 88, 88, 88, 88, 88, 88, 88, 89, 89, 89, 89, 89, 89, 89, 90, 90, 90, 90, 90, 90, 90,
    91, 91, 91, 91, 91, 91, 91, 91, 92, 92, 92, 92, 92, 92, 92, 93, 93, 93, 93, 93, 93, 93, 94, 94,
    94, 94, 94, 94, 94, 94, 95, 95, 95, 95, 95, 95, 95, 96, 96, 96, 96, 96, 96, 96, 96, 97, 97, 97,
    97, 97, 97, 97, 97, 98, 98, 98, 98, 98, 98, 98, 98, 99, 99, 99, 99, 99, 99, 99, 99, 100, 100,
    100, 100, 100, 100, 100, 100, 101, 101, 101, 101, 101, 101, 101, 101, 102, 102, 102, 102, 102,
    102, 102, 102, 103, 103, 103, 103, 103, 103, 103, 103, 103, 104, 104, 104, 104, 104, 104, 104,
    104, 105, 105, 105, 105, 105, 105, 105, 105, 105, 106, 106, 106, 106, 106, 106, 106, 106, 106,
    107, 107, 107, 107, 107, 107, 107, 107, 108, 108, 108, 108, 108, 108, 108, 108, 108, 109, 109,
    109, 109, 109, 109, 109, 109, 109, 110, 110, 110, 110, 110, 110, 110, 110, 110, 111, 111, 111,
    111, 111, 111, 111, 111, 111, 111, 112, 112, 112, 112, 112, 112, 112, 112, 112, 113, 113, 113,
    113, 113, 113, 113, 113, 113, 114, 114, 114, 114, 114, 114, 114, 114, 114, 114, 115, 115, 115,
    115, 115, 115, 115, 115, 115, 116, 116, 116, 116, 116, 116, 116, 116, 116, 116, 117, 117, 117,
    117, 117, 117, 117, 117, 117, 117, 118, 118, 118, 118, 118, 118, 118, 118, 118, 118, 119, 119,
    119, 119, 119, 119, 119, 119, 119, 119, 120, 120, 120, 120, 120, 120, 120, 120, 120, 120, 121,
    121, 121, 121, 121, 121, 121, 121, 121, 121, 122, 122, 122, 122, 122, 122, 122, 122, 122, 122,
    122, 123, 123, 123, 123, 123, 123, 123, 123, 123, 123, 124, 124, 124, 124, 124, 124, 124, 124,
    124, 124, 124, 125, 125, 125, 125, 125, 125, 125, 125, 125, 125, 126, 126, 126, 126, 126, 126,
    126, 126, 126, 126, 126, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 128, 128, 128,
    128, 128, 128, 128, 128, 128, 128, 128, 129, 129, 129, 129, 129, 129, 129, 129, 129, 129, 129,
    130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 131, 131, 131, 131, 131, 131, 131, 131,
    131, 131, 131, 131, 132, 132, 132, 132, 132, 132, 132, 132, 132, 132, 132, 133, 133, 133, 133,
    133, 133, 133, 133, 133, 133, 133, 133, 134, 134, 134, 134, 134, 134, 134, 134, 134, 134, 134,
    135, 135, 135, 135, 135, 135, 135, 135, 135, 135, 135, 135, 136, 136, 136, 136, 136, 136, 136,
    136, 136, 136, 136, 136, 137, 137, 137, 137, 137, 137, 137, 137, 137, 137, 137, 137, 138, 138,
    138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 139, 139, 139, 139, 139, 139, 139, 139, 139,
    139, 139, 139, 140, 140, 140, 140, 140, 140, 140, 140, 140, 140, 140, 140, 140, 141, 141, 141,
    141, 141, 141, 141, 141, 141, 141, 141, 141, 142, 142, 142, 142, 142, 142, 142, 142, 142, 142,
    142, 142, 142, 143, 143, 143, 143, 143, 143, 143, 143, 143, 143, 143, 143, 144, 144, 144, 144,
    144, 144, 144, 144, 144, 144, 144, 144, 144, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145,
    145, 145, 145, 146, 146, 146, 146, 146, 146, 146, 146, 146, 146, 146, 146, 146, 147, 147, 147,
    147, 147, 147, 147, 147, 147, 147, 147, 147, 147, 148, 148, 148, 148, 148, 148, 148, 148, 148,
    148, 148, 148, 148, 149, 149, 149, 149, 149, 149, 149, 149, 149, 149, 149, 149, 149, 150, 150,
    150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 151, 151, 151, 151, 151, 151, 151,
    151, 151, 151, 151, 151, 151, 152, 152, 152, 152, 152, 152, 152, 152, 152, 152, 152, 152, 152,
    152, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 154, 154, 154, 154,
    154, 154, 154, 154, 154, 154, 154, 154, 154, 154, 155, 155, 155, 155, 155, 155, 155, 155, 155,
    155, 155, 155, 155, 155, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156,
    157, 157, 157, 157, 157, 157, 157, 157, 157, 157, 157, 157, 157, 157, 158, 158, 158, 158, 158,
    158, 158, 158, 158, 158, 158, 158, 158, 158, 158, 159, 159, 159, 159, 159, 159, 159, 159, 159,
    159, 159, 159, 159, 159, 160, 160, 160, 160, 160, 160, 160, 160, 160, 160, 160, 160, 160, 160,
    160, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 162, 162, 162, 162,
    162, 162, 162, 162, 162, 162, 162, 162, 162, 162, 162, 163, 163, 163, 163, 163, 163, 163, 163,
    163, 163, 163, 163, 163, 163, 163, 164, 164, 164, 164, 164, 164, 164, 164, 164, 164, 164, 164,
    164, 164, 164, 165, 165, 165, 165, 165, 165, 165, 165, 165, 165, 165, 165, 165, 165, 165, 166,
    166, 166, 166, 166, 166, 166, 166, 166, 166, 166, 166, 166, 166, 166, 166, 167, 167, 167, 167,
    167, 167, 167, 167, 167, 167, 167, 167, 167, 167, 167, 168, 168, 168, 168, 168, 168, 168, 168,
    168, 168, 168, 168, 168, 168, 168, 168, 169, 169, 169, 169, 169, 169, 169, 169, 169, 169, 169,
    169, 169, 169, 169, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170,
    170, 171, 171, 171, 171, 171, 171, 171, 171, 171, 171, 171, 171, 171, 171, 171, 171, 172, 172,
    172, 172, 172, 172, 172, 172, 172, 172, 172, 172, 172, 172, 172, 172, 173, 173, 173, 173, 173,
    173, 173, 173, 173, 173, 173, 173, 173, 173, 173, 173, 174, 174, 174, 174, 174, 174, 174, 174,
    174, 174, 174, 174, 174, 174, 174, 174, 174, 175, 175, 175, 175, 175, 175, 175, 175, 175, 175,
    175, 175, 175, 175, 175, 175, 176, 176, 176, 176, 176, 176, 176, 176, 176, 176, 176, 176, 176,
    176, 176, 176, 177, 177, 177, 177, 177, 177, 177, 177, 177, 177, 177, 177, 177, 177, 177, 177,
    177, 178, 178, 178, 178, 178, 178, 178, 178, 178, 178, 178, 178, 178, 178, 178, 178, 178, 179,
    179, 179, 179, 179, 179, 179, 179, 179, 179, 179, 179, 179, 179, 179, 179, 179, 180, 180, 180,
    180, 180, 180, 180, 180, 180, 180, 180, 180, 180, 180, 180, 180, 180, 181, 181, 181, 181, 181,
    181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 182, 182, 182, 182, 182, 182, 182,
    182, 182, 182, 182, 182, 182, 182, 182, 182, 182, 183, 183, 183, 183, 183, 183, 183, 183, 183,
    183, 183, 183, 183, 183, 183, 183, 183, 184, 184, 184, 184, 184, 184, 184, 184, 184, 184, 184,
    184, 184, 184, 184, 184, 184, 184, 185, 185, 185, 185, 185, 185, 185, 185, 185, 185, 185, 185,
    185, 185, 185, 185, 185, 185, 186, 186, 186, 186, 186, 186, 186, 186, 186, 186, 186, 186, 186,
    186, 186, 186, 186, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187,
    187, 187, 187, 188, 188, 188, 188, 188, 188, 188, 188, 188, 188, 188, 188, 188, 188, 188, 188,
    188, 188, 189, 189, 189, 189, 189, 189, 189, 189, 189, 189, 189, 189, 189, 189, 189, 189, 189,
    189, 190, 190, 190, 190, 190, 190, 190, 190, 190, 190, 190, 190, 190, 190, 190, 190, 190, 190,
    191, 191, 191, 191, 191, 191, 191, 191, 191, 191, 191, 191, 191, 191, 191, 191, 191, 191, 191,
    192, 192, 192, 192, 192, 192, 192, 192, 192, 192, 192, 192, 192, 192, 192, 192, 192, 192, 193,
    193, 193, 193, 193, 193, 193, 193, 193, 193, 193, 193, 193, 193, 193, 193, 193, 193, 193, 194,
    194, 194, 194, 194, 194, 194, 194, 194, 194, 194, 194, 194, 194, 194, 194, 194, 194, 194, 195,
    195, 195, 195, 195, 195, 195, 195, 195, 195, 195, 195, 195, 195, 195, 195, 195, 195, 195, 196,
    196, 196, 196, 196, 196, 196, 196, 196, 196, 196, 196, 196, 196, 196, 196, 196, 196, 196, 197,
    197, 197, 197, 197, 197, 197, 197, 197, 197, 197, 197, 197, 197, 197, 197, 197, 197, 197, 198,
    198, 198, 198, 198, 198, 198, 198, 198, 198, 198, 198, 198, 198, 198, 198, 198, 198, 198, 199,
    199, 199, 199, 199, 199, 199, 199, 199, 199, 199, 199, 199, 199, 199, 199, 199, 199, 199, 200,
    200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200,
    201, 201, 201, 201, 201, 201, 201, 201, 201, 201, 201, 201, 201, 201, 201, 201, 201, 201, 201,
    202, 202, 202, 202, 202, 202, 202, 202, 202, 202, 202, 202, 202, 202, 202, 202, 202, 202, 202,
    202, 203, 203, 203, 203, 203, 203, 203, 203, 203, 203, 203, 203, 203, 203, 203, 203, 203, 203,
    203, 203, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204,
    204, 204, 204, 205, 205, 205, 205, 205, 205, 205, 205, 205, 205, 205, 205, 205, 205, 205, 205,
    205, 205, 205, 205, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206,
    206, 206, 206, 206, 206, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207,
    207, 207, 207, 207, 207, 207, 207, 208, 208, 208, 208, 208, 208, 208, 208, 208, 208, 208, 208,
    208, 208, 208, 208, 208, 208, 208, 208, 209, 209, 209, 209, 209, 209, 209, 209, 209, 209, 209,
    209, 209, 209, 209, 209, 209, 209, 209, 209, 209, 210, 210, 210, 210, 210, 210, 210, 210, 210,
    210, 210, 210, 210, 210, 210, 210, 210, 210, 210, 210, 210, 211, 211, 211, 211, 211, 211, 211,
    211, 211, 211, 211, 211, 211, 211, 211, 211, 211, 211, 211, 211, 211, 212, 212, 212, 212, 212,
    212, 212, 212, 212, 212, 212, 212, 212, 212, 212, 212, 212, 212, 212, 212, 212, 213, 213, 213,
    213, 213, 213, 213, 213, 213, 213, 213, 213, 213, 213, 213, 213, 213, 213, 213, 213, 213, 214,
    214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214,
    214, 215, 215, 215, 215, 215, 215, 215, 215, 215, 215, 215, 215, 215, 215, 215, 215, 215, 215,
    215, 215, 215, 215, 216, 216, 216, 216, 216, 216, 216, 216, 216, 216, 216, 216, 216, 216, 216,
    216, 216, 216, 216, 216, 216, 217, 217, 217, 217, 217, 217, 217, 217, 217, 217, 217, 217, 217,
    217, 217, 217, 217, 217, 217, 217, 217, 217, 218, 218, 218, 218, 218, 218, 218, 218, 218, 218,
    218, 218, 218, 218, 218, 218, 218, 218, 218, 218, 218, 218, 219, 219, 219, 219, 219, 219, 219,
    219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 220, 220, 220, 220,
    220, 220, 220, 220, 220, 220, 220, 220, 220, 220, 220, 220, 220, 220, 220, 220, 220, 220, 221,
    221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221,
    221, 221, 222, 222, 222, 222, 222, 222, 222, 222, 222, 222, 222, 222, 222, 222, 222, 222, 222,
    222, 222, 222, 222, 222, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223,
    223, 223, 223, 223, 223, 223, 223, 223, 223, 224, 224, 224, 224, 224, 224, 224, 224, 224, 224,
    224, 224, 224, 224, 224, 224, 224, 224, 224, 224, 224, 224, 225, 225, 225, 225, 225, 225, 225,
    225, 225, 225, 225, 225, 225, 225, 225, 225, 225, 225, 225, 225, 225, 225, 225, 226, 226, 226,
    226, 226, 226, 226, 226, 226, 226, 226, 226, 226, 226, 226, 226, 226, 226, 226, 226, 226, 226,
    226, 227, 227, 227, 227, 227, 227, 227, 227, 227, 227, 227, 227, 227, 227, 227, 227, 227, 227,
    227, 227, 227, 227, 227, 228, 228, 228, 228, 228, 228, 228, 228, 228, 228, 228, 228, 228, 228,
    228, 228, 228, 228, 228, 228, 228, 228, 228, 229, 229, 229, 229, 229, 229, 229, 229, 229, 229,
    229, 229, 229, 229, 229, 229, 229, 229, 229, 229, 229, 229, 229, 230, 230, 230, 230, 230, 230,
    230, 230, 230, 230, 230, 230, 230, 230, 230, 230, 230, 230, 230, 230, 230, 230, 230, 230, 231,
    231, 231, 231, 231, 231, 231, 231, 231, 231, 231, 231, 231, 231, 231, 231, 231, 231, 231, 231,
    231, 231, 231, 232, 232, 232, 232, 232, 232, 232, 232, 232, 232, 232, 232, 232, 232, 232, 232,
    232, 232, 232, 232, 232, 232, 232, 232, 233, 233, 233, 233, 233, 233, 233, 233, 233, 233, 233,
    233, 233, 233, 233, 233, 233, 233, 233, 233, 233, 233, 233, 233, 234, 234, 234, 234, 234, 234,
    234, 234, 234, 234, 234, 234, 234, 234, 234, 234, 234, 234, 234, 234, 234, 234, 234, 234, 235,
    235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235,
    235, 235, 235, 235, 236, 236, 236, 236, 236, 236, 236, 236, 236, 236, 236, 236, 236, 236, 236,
    236, 236, 236, 236, 236, 236, 236, 236, 236, 237, 237, 237, 237, 237, 237, 237, 237, 237, 237,
    237, 237, 237, 237, 237, 237, 237, 237, 237, 237, 237, 237, 237, 237, 238, 238, 238, 238, 238,
    238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238,
    238, 239, 239, 239, 239, 239, 239, 239, 239, 239, 239, 239, 239, 239, 239, 239, 239, 239, 239,
    239, 239, 239, 239, 239, 239, 239, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240,
    240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 241, 241, 241, 241, 241, 241, 241,
    241, 241, 241, 241, 241, 241, 241, 241, 241, 241, 241, 241, 241, 241, 241, 241, 241, 241, 242,
    242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242, 242,
    242, 242, 242, 242, 242, 243, 243, 243, 243, 243, 243, 243, 243, 243, 243, 243, 243, 243, 243,
    243, 243, 243, 243, 243, 243, 243, 243, 243, 243, 243, 244, 244, 244, 244, 244, 244, 244, 244,
    244, 244, 244, 244, 244, 244, 244, 244, 244, 244, 244, 244, 244, 244, 244, 244, 244, 244, 245,
    245, 245, 245, 245, 245, 245, 245, 245, 245, 245, 245, 245, 245, 245, 245, 245, 245, 245, 245,
    245, 245, 245, 245, 245, 246, 246, 246, 246, 246, 246, 246, 246, 246, 246, 246, 246, 246, 246,
    246, 246, 246, 246, 246, 246, 246, 246, 246, 246, 246, 246, 247, 247, 247, 247, 247, 247, 247,
    247, 247, 247, 247, 247, 247, 247, 247, 247, 247, 247, 247, 247, 247, 247, 247, 247, 247, 247,
    248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248,
    248, 248, 248, 248, 248, 248, 249, 249, 249, 249, 249, 249, 249, 249, 249, 249, 249, 249, 249,
    249, 249, 249, 249, 249, 249, 249, 249, 249, 249, 249, 249, 249, 250, 250, 250, 250, 250, 250,
    250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250,
    250, 250, 251, 251, 251, 251, 251, 251, 251, 251, 251, 251, 251, 251, 251, 251, 251, 251, 251,
    251, 251, 251, 251, 251, 251, 251, 251, 251, 252, 252, 252, 252, 252, 252, 252, 252, 252, 252,
    252, 252, 252, 252, 252, 252, 252, 252, 252, 252, 252, 252, 252, 252, 252, 252, 253, 253, 253,
    253, 253, 253, 253, 253, 253, 253, 253, 253, 253, 253, 253, 253, 253, 253, 253, 253, 253, 253,
    253, 253, 253, 253, 253, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254,
    254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254,
];

#[cfg(test)]
mod test {
    use super::*;

    extern crate std;
    use std::*;

    #[test]
    fn linear_roundtrip() {
        srgb_cube(|r, g, b| {
            let srgb = Srgb::from_rgb(r, g, b);
            let linear = srgb.to_linear();
            linear.to_srgb()
        });
    }

    #[test]
    fn hsv_roundtrip() {
        srgb_cube(|r, g, b| {
            let srgb = Srgb::from_rgb(r, g, b);
            let hsv = srgb.to_hsv();
            hsv.to_srgb()
        });
    }

    #[track_caller]
    fn srgb_cube(f: impl Fn(u8, u8, u8) -> Srgb) {
        let mut failures = 0;
        let mut total = 0;
        for r in 0u8..=255 {
            for g in 0u8..=255 {
                for b in 0u8..=255 {
                    let back_to_srgb = f(r, g, b);

                    total += 1;
                    if (r as i16 - back_to_srgb.r as i16).abs() > 1
                        || (g as i16 - back_to_srgb.g as i16).abs() > 1
                        || (b as i16 - back_to_srgb.b as i16).abs() > 1
                    {
                        failures += 1;
                    }
                }
            }
        }
        println!("total: {}, failures: {}", total, failures);
        assert_eq!(failures, 0);
    }

    // TODO: Put this into a build script.
    //
    // It is convenient here because we can use std to write a file.
    #[expect(unused)]
    // #[test]
    fn generate_linear_to_srgb_component_lut() {
        let mut str = std::string::String::default();
        str.push_str("pub const LINEAR_TO_SRGB_COMPONENT_LUT: &[u8] = &[\n");
        for i in 0..LINEAR_TO_SRGB_COMPONENT_LUT_SIZE {
            let l = i as f32 / LINEAR_TO_SRGB_COMPONENT_LUT_SIZE as f32;
            let s = if l <= 0.0031308 {
                (l * 12.92 * 255.0) as u8
            } else {
                let srgb = l.clamp(0.0, 1.0).powf(1.0 / 2.4) * 1.055 - 0.055;
                (srgb * 255.0) as u8
            };
            str.push_str(&format!("{s},\n"));
        }
        str.push_str("];");
        std::fs::write("src/linear_to_srgb_lut.rs", str.as_bytes()).unwrap();
    }
}
