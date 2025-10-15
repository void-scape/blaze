#![no_std]

use core::num::NonZero;

#[cfg(feature = "preprocess")]
pub mod preprocess;

/// Uncompressed image with sRGB pixel data `T`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Image<'a, T> {
    pub width: usize,
    pub height: usize,
    pub pixels: &'a [T],
}

impl<'a, T> Pack<'a> for Image<'a, T>
where
    T: Copy + Into<u32>,
{
    fn pack(&self, bytes: &mut [u8], align: NonZero<usize>) {
        assert!(align.get().is_multiple_of(2) || align.get() == 1);
        assert_eq!(self.pixels.len(), self.width * self.height);
        assert_eq!(core::mem::size_of::<T>(), 4);
        assert!(bytes.len() >= self.bytes(align));
        assert!(self.width < u32::MAX as usize);
        assert!(self.height < u32::MAX as usize);

        (self.width as u32).pack(bytes, align);
        let height_offset = 0u32.bytes(align);
        (self.height as u32).pack(&mut bytes[height_offset..], align);

        let mut pixel_offset = height_offset * 2;
        for pixel in self.pixels.iter() {
            let pixel: u32 = (*pixel).into();
            pixel.pack(&mut bytes[pixel_offset..], align);
            pixel_offset += 4;
        }
    }

    fn unpack(bytes: &[u8], align: NonZero<usize>) -> Self {
        assert!(align.get().is_multiple_of(2) || align.get() == 1);
        let height_offset = 0u32.bytes(align);
        let pixel_offset = height_offset * 2;
        let width = u32::unpack(bytes, align) as usize;
        let height = u32::unpack(&bytes[height_offset..], align) as usize;
        Self {
            width,
            height,
            // TODO: this assumes the packing machine matches the endianness of the
            // consuming machine, this is BAD
            pixels: unsafe {
                core::slice::from_raw_parts(
                    bytes[pixel_offset..].as_ptr() as *const T,
                    width * height,
                )
            },
        }
    }

    fn bytes(&self, align: NonZero<usize>) -> usize {
        let pixels_len = self.pixels.len() * 4;
        let pad = (align.get() - (pixels_len % align.get())) % align.get();
        0u32.bytes(align) + 0u32.bytes(align) + pixels_len + pad
    }
}

/// Uncompressed audio with `f32` sample data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Audio<'a> {
    pub sample_rate: f32,
    pub channels: usize,
    pub samples: &'a [f32],
}

impl<'a> Pack<'a> for Audio<'a> {
    fn pack(&self, bytes: &mut [u8], align: NonZero<usize>) {
        assert!(align.get().is_multiple_of(2) || align.get() == 1);
        assert!(bytes.len() >= self.bytes(align));
        assert!(self.channels < u32::MAX as usize);

        self.sample_rate.pack(bytes, align);
        let channels_offset = 0f32.bytes(align);
        (self.channels as u32).pack(&mut bytes[channels_offset..], align);
        let len_offset = channels_offset + 0u32.bytes(align);
        (self.samples.len() as u64).pack(&mut bytes[len_offset..], align);

        let mut sample_offset = len_offset + 0u64.bytes(align);
        for sample in self.samples.iter() {
            sample.pack(&mut bytes[sample_offset..], align);
            sample_offset += 4;
        }
    }

    fn unpack(bytes: &[u8], align: NonZero<usize>) -> Self {
        assert!(align.get().is_multiple_of(2) || align.get() == 1);
        let channels_offset = 0f32.bytes(align);
        let len_offset = channels_offset + 0u32.bytes(align);
        let samples_offset = len_offset + 0u64.bytes(align);

        let sample_rate = f32::unpack(bytes, align);
        let channels = u32::unpack(&bytes[channels_offset..], align) as usize;
        let len = u64::unpack(&bytes[len_offset..], align) as usize;

        Self {
            sample_rate,
            channels,
            samples: unsafe {
                core::slice::from_raw_parts(bytes[samples_offset..].as_ptr() as *const f32, len)
            },
        }
    }

    fn bytes(&self, align: NonZero<usize>) -> usize {
        let samples_len = self.samples.len() * 4;
        let pad = (align.get() - (samples_len % align.get())) % align.get();
        0f32.bytes(align) + 0u32.bytes(align) + 0u64.bytes(align) + samples_len + pad
    }
}

pub use packit_macro::Pack;
pub trait Pack<'a> {
    fn pack(&self, bytes: &mut [u8], align: NonZero<usize>);
    fn unpack(bytes: &'a [u8], align: NonZero<usize>) -> Self;
    fn bytes(&self, align: NonZero<usize>) -> usize;
}

impl Pack<'_> for bool {
    fn pack(&self, bytes: &mut [u8], align: NonZero<usize>) {
        assert!(!bytes.is_empty());
        u8::pack(&(*self as u8), bytes, align);
    }

    fn unpack(bytes: &[u8], align: NonZero<usize>) -> Self {
        debug_assert!(!bytes.is_empty());
        let result = u8::unpack(bytes, align);
        debug_assert!(result <= 1);
        result == 1
    }

    fn bytes(&self, align: NonZero<usize>) -> usize {
        align.get()
    }
}

macro_rules! impl_prim {
    ($prim:ident) => {
        impl Pack<'_> for $prim {
            fn pack(&self, bytes: &mut [u8], _: NonZero<usize>) {
                let byte_len = core::mem::size_of::<Self>();
                assert!(bytes.len() >= byte_len);
                bytes[..byte_len].copy_from_slice(&self.to_le_bytes());
            }

            fn unpack(bytes: &[u8], _: NonZero<usize>) -> Self {
                let byte_len = core::mem::size_of::<Self>();
                debug_assert!(bytes.len() >= byte_len);
                unsafe { Self::from_le((bytes.as_ptr() as *const Self).read_unaligned()) }
            }

            fn bytes(&self, align: NonZero<usize>) -> usize {
                let byte_len = core::mem::size_of::<Self>();
                let pad = (align.get() - (byte_len % align.get())) % align.get();
                byte_len + pad
            }
        }
    };
}

impl_prim!(u8);
impl_prim!(u16);
impl_prim!(u32);
impl_prim!(u64);
impl_prim!(u128);

impl_prim!(i8);
impl_prim!(i16);
impl_prim!(i32);
impl_prim!(i64);
impl_prim!(i128);

macro_rules! impl_float {
    ($float:ident) => {
        impl Pack<'_> for $float {
            fn pack(&self, bytes: &mut [u8], _: NonZero<usize>) {
                let byte_len = core::mem::size_of::<Self>();
                assert!(bytes.len() >= byte_len);
                bytes[..byte_len].copy_from_slice(&self.to_le_bytes());
            }

            fn unpack(bytes: &[u8], _: NonZero<usize>) -> Self {
                let byte_len = core::mem::size_of::<Self>();
                debug_assert!(bytes.len() >= byte_len);
                unsafe { (bytes.as_ptr() as *const Self).read_unaligned() }
            }

            fn bytes(&self, align: NonZero<usize>) -> usize {
                let byte_len = core::mem::size_of::<Self>();
                let pad = (align.get() - (byte_len % align.get())) % align.get();
                byte_len + pad
            }
        }
    };
}

impl_float!(f32);
impl_float!(f64);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn byte_len() {
        let align = NonZero::new(2).unwrap();
        assert_eq!(0u8.bytes(align), 2);
        assert_eq!(0u16.bytes(align), 2);
        assert_eq!(0u32.bytes(align), 4);
        assert_eq!(0u64.bytes(align), 8);
        assert_eq!(0u128.bytes(align), 16);

        let align = NonZero::new(4).unwrap();
        assert_eq!(0u8.bytes(align), 4);
        assert_eq!(0u16.bytes(align), 4);
        assert_eq!(0u32.bytes(align), 4);
        assert_eq!(0u64.bytes(align), 8);
        assert_eq!(0u128.bytes(align), 16);

        let align = NonZero::new(1).unwrap();
        assert_eq!(0u8.bytes(align), 1);
        assert_eq!(0u16.bytes(align), 2);
        assert_eq!(0u32.bytes(align), 4);
        assert_eq!(0u64.bytes(align), 8);
        assert_eq!(0u128.bytes(align), 16);
    }

    #[test]
    fn pack_unpack() {
        #[derive(Debug, PartialEq, Pack)]
        struct Data {
            f1: bool,
            f2: u32,
            f3: u16,
        }

        #[derive(Debug, PartialEq, Pack)]
        struct NestedData {
            f1: bool,
            data1: Data,
            f2: u32,
            data2: Data,
            f3: u16,
        }

        let mut bytes = [0; 1_000];
        let nested_data = NestedData {
            f1: false,
            data1: Data {
                f1: true,
                f2: 69,
                f3: 420,
            },
            f2: 42,
            data2: Data {
                f1: false,
                f2: 12,
                f3: 99,
            },
            f3: 69,
        };
        let data = Data {
            f1: true,
            f2: 69,
            f3: 420,
        };

        for i in 1..=8 {
            let align = NonZero::new(i).unwrap();

            // intentionally keep garbage in here
            data.pack(&mut bytes, align);
            let unpacked_data = Data::unpack(&bytes, align);
            assert_eq!(unpacked_data, data);

            nested_data.pack(&mut bytes, align);
            let unpacked_data = NestedData::unpack(&bytes, align);
            assert_eq!(unpacked_data, nested_data);
        }

        for i in 1..=8 {
            let mut bytes = [0; 1_000];
            let align = NonZero::new(i).unwrap();

            // intentionally *don't* keep garbage in here
            data.pack(&mut bytes, align);
            let unpacked_data = Data::unpack(&bytes, align);
            assert_eq!(unpacked_data, data);

            let mut bytes = [0; 1_000];
            nested_data.pack(&mut bytes, align);
            let unpacked_data = NestedData::unpack(&bytes, align);
            assert_eq!(unpacked_data, nested_data);
        }
    }

    #[test]
    fn image() {
        let data: [u32; 34 * 35] = core::array::from_fn(|i| i as u32);
        let image = Image {
            width: 34,
            height: 35,
            pixels: &data,
        };

        for i in [1, 4, 8].into_iter() {
            let mut packed = [0; 5_000];
            let align = NonZero::new(i).unwrap();
            image.pack(&mut packed, align);
            let unpacked_image = Image::unpack(&packed, align);
            assert_eq!(unpacked_image, image);
        }
    }

    #[test]
    fn audio() {
        let data: [f32; 69] = core::array::from_fn(|i| i as f32);
        let audio = Audio {
            sample_rate: 44_100.0,
            channels: 2,
            samples: &data,
        };

        for i in [1, 4, 8].into_iter() {
            let mut packed = [0; 5_000];
            let align = NonZero::new(i).unwrap();
            audio.pack(&mut packed, align);
            let unpacked_audio = Audio::unpack(&packed, align);
            assert_eq!(unpacked_audio, audio);
        }
    }
}
