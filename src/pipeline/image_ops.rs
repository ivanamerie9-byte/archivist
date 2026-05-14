use std::path::Path;

use anyhow::{Context, Result};
use image::{codecs::jpeg::JpegEncoder, imageops::FilterType, DynamicImage};

pub fn decode(bytes: &[u8]) -> Result<DynamicImage> {
    image::load_from_memory(bytes).context("load_from_memory")
}

pub fn resize_to_width(img: &DynamicImage, target_width: u32) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w <= target_width {
        return img.clone();
    }
    let new_h = ((h as u64 * target_width as u64) / w as u64).max(1) as u32;
    img.resize(target_width, new_h, FilterType::Lanczos3)
}

pub fn write_jpeg(img: &DynamicImage, path: &Path, quality: u8) -> Result<()> {
    let rgb = img.to_rgb8();
    let file = std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    let writer = std::io::BufWriter::new(file);
    let mut enc = JpegEncoder::new_with_quality(writer, quality);
    enc.encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ExtendedColorType::Rgb8,
    )
    .with_context(|| format!("encode jpeg {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn resize_keeps_aspect() {
        let img: DynamicImage = RgbaImage::from_pixel(1000, 500, Rgba([255, 0, 0, 255])).into();
        let r = resize_to_width(&img, 200);
        assert_eq!(r.width(), 200);
        assert_eq!(r.height(), 100);
    }

    #[test]
    fn resize_no_upscale() {
        let img: DynamicImage = RgbaImage::from_pixel(100, 50, Rgba([0, 255, 0, 255])).into();
        let r = resize_to_width(&img, 512);
        assert_eq!(r.width(), 100);
    }
}
