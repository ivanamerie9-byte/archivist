use anyhow::{Context, Result};
use image::{imageops::FilterType, DynamicImage, RgbaImage};

const ICO_SIZES: &[u32] = &[256, 48, 32, 16];

/// Reproduce the Pillow recipe from the legacy `cover_tool.html` Python output:
/// - Make a 256×256 fully transparent canvas
/// - Resize the source to fit inside 256×256 preserving aspect (LANCZOS)
/// - Paste it centred
/// - Save as multi-size ICO with sizes [(256,256),(48,48),(32,32),(16,16)]
pub fn build_multi_size_ico(src: &DynamicImage) -> Result<Vec<u8>> {
    let canvas = build_canvas(src);
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for &size in ICO_SIZES {
        let resized = if size == 256 {
            canvas.clone()
        } else {
            DynamicImage::from(canvas.clone())
                .resize_exact(size, size, FilterType::Lanczos3)
                .to_rgba8()
        };
        let icon = ico::IconImage::from_rgba_data(size, size, resized.into_raw());
        let entry = ico::IconDirEntry::encode(&icon).context("encode ico entry")?;
        dir.add_entry(entry);
    }
    let mut out = Vec::with_capacity(64 * 1024);
    dir.write(&mut out).context("write ico container")?;
    Ok(out)
}

fn build_canvas(src: &DynamicImage) -> RgbaImage {
    let mut canvas = RgbaImage::from_pixel(256, 256, image::Rgba([0, 0, 0, 0]));

    // Fit source into 256×256 preserving aspect ratio.
    let thumb = src.resize(256, 256, FilterType::Lanczos3).to_rgba8();
    let off_x = ((256 - thumb.width() as i32) / 2).max(0) as i64;
    let off_y = ((256 - thumb.height() as i32) / 2).max(0) as i64;
    image::imageops::overlay(&mut canvas, &thumb, off_x, off_y);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn produces_four_entries_with_expected_sizes() {
        let src: DynamicImage = RgbaImage::from_pixel(1024, 1536, Rgba([200, 100, 50, 255])).into();
        let bytes = build_multi_size_ico(&src).unwrap();
        let parsed = ico::IconDir::read(std::io::Cursor::new(&bytes)).expect("ICO must round-trip");
        let mut sizes: Vec<u32> = parsed.entries().iter().map(|e| e.width()).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, vec![16, 32, 48, 256]);
    }

    #[test]
    fn canvas_is_square_256() {
        let src: DynamicImage = RgbaImage::from_pixel(500, 750, Rgba([0, 0, 255, 255])).into();
        let c = build_canvas(&src);
        assert_eq!(c.width(), 256);
        assert_eq!(c.height(), 256);
    }
}
