extern crate std;

use std::vec::Vec;

use image::{GenericImageView, Pixel, metadata::Cicp};
use rodio::{Decoder, Source, decoder::DecoderError};

use crate::{Audio, Image};

impl<T> Image<'static, T>
where
    T: From<u32>,
{
    /// Read an image asset from bytes, leaking the memory.
    pub fn from_bytes(bytes: &[u8]) -> image::ImageResult<Self> {
        let mut image = image::load_from_memory(bytes)?;
        image.set_color_space(Cicp::SRGB)?;

        Ok(Self {
            width: image.width() as usize,
            height: image.height() as usize,
            pixels: Vec::leak(
                image
                    .pixels()
                    .map(|p| {
                        let c = p.2.channels();
                        T::from(u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    })
                    .collect::<Vec<_>>(),
            ),
        })
    }
}

impl Audio<'static> {
    /// Read an audio asset from bytes, leaking the memory.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecoderError> {
        let decoder = Decoder::new(std::io::Cursor::new(bytes.to_vec()))?;
        let sample_rate = decoder.sample_rate() as f32;
        let channels = decoder.channels() as usize;
        let samples = Vec::leak(decoder.collect::<Vec<_>>());

        Ok(Self {
            sample_rate,
            channels,
            samples,
        })
    }
}
