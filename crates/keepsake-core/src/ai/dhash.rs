//! Difference hash: 64 bits that stay the same through re-encoding and
//! resizing, so exact and near-exact copies are found without the model.
//!
//! The image is shrunk to 9×8 grey pixels; each bit says whether a pixel is
//! brighter than its right-hand neighbour.

use fast_image_resize::{images::Image as FirImage, PixelType, ResizeOptions, Resizer};
use image::DynamicImage;

use super::AiError;

/// Hashes this far apart (in bits) or closer count as the same picture.
pub const NEAR: u32 = 6;

pub fn dhash(img: &DynamicImage) -> Result<u64, AiError> {
    let grey = img.to_luma8();
    let (w, h) = grey.dimensions();
    if w == 0 || h == 0 {
        return Err(AiError::Image("empty image".into()));
    }
    let src = FirImage::from_vec_u8(w, h, grey.into_raw(), PixelType::U8)
        .map_err(|e| AiError::Image(e.to_string()))?;
    let mut dst = FirImage::new(9, 8, PixelType::U8);
    Resizer::new()
        .resize(&src, &mut dst, &ResizeOptions::new())
        .map_err(|e| AiError::Image(e.to_string()))?;
    let px = dst.buffer();
    let mut bits = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            let left = px[y * 9 + x];
            let right = px[y * 9 + x + 1];
            bits = (bits << 1) | u64::from(left > right);
        }
    }
    Ok(bits)
}

/// Bits that differ.
pub fn distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn scene(w: u32, h: u32, shift: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
            let xs = (x * 640 / w + shift) as f32;
            let ys = (y * 480 / h) as f32;
            let v = ((xs / 40.0).sin() * 60.0 + (ys / 25.0).cos() * 60.0 + 128.0) as u8;
            Rgb([v, v.wrapping_add(30), 255 - v])
        }))
    }

    fn jpeg_round_trip(img: &DynamicImage) -> DynamicImage {
        let bytes = crate::render::still::encode_jpeg(img).unwrap();
        image::load_from_memory(&bytes).unwrap()
    }

    #[test]
    fn same_picture_resized_and_reencoded_matches() {
        let a = scene(640, 480, 0);
        let b = jpeg_round_trip(&scene(320, 240, 0));
        assert!(distance(dhash(&a).unwrap(), dhash(&b).unwrap()) <= NEAR);
    }

    #[test]
    fn different_pictures_differ() {
        let a = scene(640, 480, 0);
        let b = DynamicImage::ImageRgb8(RgbImage::from_fn(640, 480, |x, y| {
            let v = if (x / 80 + y / 60) % 2 == 0 { 20 } else { 230 };
            Rgb([v, v, v])
        }));
        assert!(distance(dhash(&a).unwrap(), dhash(&b).unwrap()) > NEAR * 2);
    }

    #[test]
    fn distance_counts_bits() {
        assert_eq!(distance(0, 0), 0);
        assert_eq!(distance(0b1011, 0b0001), 2);
        assert_eq!(distance(u64::MAX, 0), 64);
    }
}
