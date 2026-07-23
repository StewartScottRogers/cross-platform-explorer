//! Image preview (CPE-099/101/659, epic CPE-615): transcode a format the webview can't render natively
//! (TIFF, PSD) to a PNG `data:` URL, and read image dimensions + basic EXIF for the Properties dialog.
//! Pure-Rust (`image` decoders, `psd` composite, `kamadak-exif`); extracted into the Server (CPE-815).
//! The Tauri `read_image_data_url` / `image_meta` commands dispatch here (the app caps file size first).

use std::fs;
use std::path::Path;

use serde::Serialize;

/// Decode an image the webview can't render natively (TIFF, PSD) to a PNG `data:` URL the `<img>` tag
/// can show. PSD uses the psd crate's flattened composite; TIFF (and any other image-crate-decodable
/// format routed here) uses the image crate. Errors (rather than hangs) on a corrupt file.
pub fn read_image_data_url(path: &str) -> Result<String, String> {
    use base64::Engine;
    use std::io::Cursor;

    let ext = crate::model::extension_of(Path::new(path));
    let png: Vec<u8> = if ext == "psd" {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let psd = psd::Psd::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let rgba = psd.rgba();
        let buf = image::RgbaImage::from_raw(psd.width(), psd.height(), rgba)
            .ok_or("PSD pixel buffer size mismatch")?;
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buf)
            .write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        out.into_inner()
    } else {
        let img = image::open(path).map_err(|e| e.to_string())?;
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        out.into_inner()
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    Ok(format!("data:image/png;base64,{b64}"))
}

/// Image dimensions + basic EXIF for the Properties dialog. Best-effort: every field is optional and a
/// non-image / EXIF-less file yields an all-`None` struct rather than an error.
#[derive(Serialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ImageMeta {
    width: Option<u32>,
    height: Option<u32>,
    camera: Option<String>,
    lens: Option<String>,
    taken: Option<String>,
    iso: Option<String>,
    aperture: Option<String>,
    exposure: Option<String>,
    focal_length: Option<String>,
}

fn read_exif(path: &str) -> Result<exif::Exif, exif::Error> {
    let file = fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(&file);
    exif::Reader::new().read_from_container(&mut reader)
}

/// Read image dimensions (cheaply from the header) + basic EXIF fields.
pub fn image_meta(path: &str) -> Result<ImageMeta, String> {
    use exif::{In, Tag};
    let mut meta = ImageMeta::default();

    if let Ok((w, h)) = image::image_dimensions(path) {
        meta.width = Some(w);
        meta.height = Some(h);
    }

    if let Ok(exif) = read_exif(path) {
        // A human-readable value for a tag, with unit (e.g. "f/2.8", "1/200 s", "50 mm"), trimmed of the
        // quotes kamadak wraps ASCII strings in; `None` when the tag is absent or empty.
        let field = |tag: Tag| {
            exif.get_field(tag, In::PRIMARY)
                .map(|f| f.display_value().with_unit(&exif).to_string())
                .map(|s| s.trim().trim_matches('"').trim().to_string())
                .filter(|s| !s.is_empty())
        };

        // Model usually already includes the make ("NIKON D750"); don't duplicate it.
        meta.camera = match (field(Tag::Make), field(Tag::Model)) {
            (Some(mk), Some(md)) => Some(if md.starts_with(&mk) { md } else { format!("{mk} {md}") }),
            (mk, md) => mk.or(md),
        };
        meta.lens = field(Tag::LensModel);
        meta.taken = field(Tag::DateTimeOriginal);
        meta.iso = field(Tag::PhotographicSensitivity);
        meta.aperture = field(Tag::FNumber);
        meta.exposure = field(Tag::ExposureTime);
        meta.focal_length = field(Tag::FocalLength);

        // JPEGs the `image` crate couldn't size still carry pixel dimensions in EXIF.
        if meta.width.is_none() {
            meta.width = exif.get_field(Tag::PixelXDimension, In::PRIMARY).and_then(|f| f.value.get_uint(0));
            meta.height = exif.get_field(Tag::PixelYDimension, In::PRIMARY).and_then(|f| f.value.get_uint(0));
        }
    }

    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-imgprev-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn read_image_data_url_transcodes_tiff_to_png() {
        let d = scratch("tiff");
        let f = d.join("a.tiff");
        image::RgbImage::from_pixel(8, 4, image::Rgb([9u8, 8, 7]))
            .save_with_format(&f, image::ImageFormat::Tiff)
            .unwrap();
        let url = read_image_data_url(&f.to_string_lossy()).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        // A corrupt file errors, not panics.
        fs::write(d.join("bad.tiff"), b"nope").unwrap();
        assert!(read_image_data_url(&d.join("bad.tiff").to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn image_meta_reports_dimensions() {
        let d = scratch("meta");
        let f = d.join("a.png");
        image::RgbImage::from_pixel(24, 12, image::Rgb([1u8, 2, 3])).save(&f).unwrap();
        let m = image_meta(&f.to_string_lossy()).unwrap();
        assert_eq!((m.width, m.height), (Some(24), Some(12)));
        // A non-image yields an all-None struct, never an error.
        fs::write(d.join("t.txt"), b"not an image").unwrap();
        let none = image_meta(&d.join("t.txt").to_string_lossy()).unwrap();
        assert!(none.width.is_none() && none.camera.is_none());
        let _ = fs::remove_dir_all(&d);
    }
}
