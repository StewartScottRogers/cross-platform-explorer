//! Image preview (CPE-099/101/659, epic CPE-615): transcode a format the webview can't render natively
//! (TIFF, PSD) to a PNG `data:` URL, and read image dimensions + basic EXIF for the Properties dialog.
//! Pure-Rust (`image` decoders, `psd` composite, `kamadak-exif`); extracted into the Server (CPE-815).
//! The Tauri `read_image_data_url` / `image_meta` commands dispatch here (the app caps file size first).

use std::fs;
use std::path::Path;

use serde::Serialize;

/// Encode a raw RGBA8 pixel buffer (`width * height * 4` bytes, row-major, no padding) to a PNG
/// `data:image/png;base64,...` URL the `<img>` tag can show. The single shared sink both the
/// pure-Rust preview decoders and the platform-API HEIC/HEIF decoders in the app adapter feed
/// (CPE-1351): they own the FFI; this owns the PNG-encode + base64 wrap, keeping the encoding in
/// one tested place in `cpe-server`. `Err` (never a panic) when `rgba`'s length doesn't match
/// `width * height * 4` or PNG encoding fails.
pub fn encode_rgba_to_png_data_url(width: u32, height: u32, rgba: Vec<u8>) -> Result<String, String> {
    use base64::Engine;
    use std::io::Cursor;

    let buf = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or("RGBA buffer size does not match width * height * 4")?;
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(buf)
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(out.into_inner());
    Ok(format!("data:image/png;base64,{b64}"))
}

/// Decode an image the webview can't render natively (TIFF, PSD) to a PNG `data:` URL the `<img>` tag
/// can show. PSD uses the psd crate's flattened composite; TIFF (and any other image-crate-decodable
/// format routed here) uses the image crate. Errors (rather than hangs) on a corrupt file.
pub fn read_image_data_url(path: &str) -> Result<String, String> {
    use base64::Engine;
    use std::io::Cursor;

    let ext = crate::model::extension_of(Path::new(path));
    if ext == "psd" {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let psd = psd::Psd::from_bytes(&bytes).map_err(|e| e.to_string())?;
        if !crate::thumb_source::psd_within_limits(&psd) {
            return Err(format!(
                "PSD dimensions exceed limit ({}x{})",
                psd.width(),
                psd.height()
            ));
        }
        // The composite is already RGBA8 — route it through the shared PNG sink (CPE-1351).
        encode_rgba_to_png_data_url(psd.width(), psd.height(), psd.rgba())
    } else {
        let img = image::open(path).map_err(|e| e.to_string())?;
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(out.into_inner());
        Ok(format!("data:image/png;base64,{b64}"))
    }
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
    fn encode_rgba_to_png_data_url_round_trips_a_2x2() {
        // A tiny 2x2 RGBA buffer (4 opaque pixels: red, green, blue, white).
        let rgba = vec![
            255, 0, 0, 255, //
            0, 255, 0, 255, //
            0, 0, 255, 255, //
            255, 255, 255, 255,
        ];
        let url = encode_rgba_to_png_data_url(2, 2, rgba).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        // The base64 payload must decode back to a valid 2x2 PNG.
        use base64::Engine;
        let b64 = url.strip_prefix("data:image/png;base64,").unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        assert_eq!((img.width(), img.height()), (2, 2));
        // A buffer whose length doesn't match width*height*4 is an Err, not a panic.
        assert!(encode_rgba_to_png_data_url(2, 2, vec![0u8; 3]).is_err());
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

    /// Build a minimal, valid, **uncompressed** 8BPS PSD by hand — same recipe as
    /// `thumb_source::tests::minimal_psd` (CPE-1086/CPE-1087): a 26-byte file header (RGB, 8-bit, `width`
    /// x `height`), three empty length-prefixed sections, then a raw planar R/G/B image-data section (no
    /// alpha, fully opaque composite). Kept as a local copy per this file's "documented local copy"
    /// precedent rather than reaching into another module's private test helper.
    fn minimal_psd(width: u32, height: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"8BPS");
        b.extend_from_slice(&1u16.to_be_bytes());
        b.extend_from_slice(&[0u8; 6]);
        b.extend_from_slice(&3u16.to_be_bytes());
        b.extend_from_slice(&height.to_be_bytes());
        b.extend_from_slice(&width.to_be_bytes());
        b.extend_from_slice(&8u16.to_be_bytes());
        b.extend_from_slice(&3u16.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u16.to_be_bytes());
        let plane = (width as usize) * (height as usize);
        b.extend(std::iter::repeat(200u8).take(plane));
        b.extend(std::iter::repeat(100u8).take(plane));
        b.extend(std::iter::repeat(50u8).take(plane));
        b
    }

    #[test]
    fn read_image_data_url_transcodes_a_normal_psd_to_png() {
        let d = scratch("psd");
        let f = d.join("a.psd");
        fs::write(&f, minimal_psd(6, 4)).unwrap();
        let url = read_image_data_url(&f.to_string_lossy()).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1087: a PSD declaring a width (25,000) past the shared `thumb_source::psd_within_limits`
    /// guard's `MAX_IMAGE_DIMENSION` (20,000) — but still inside the `psd` crate's own hard-coded 30,000
    /// header cap, so this is a genuinely valid, parseable PSD, not one the crate already rejects on its
    /// own — must be rejected with an `Err` *before* `.rgba()`'s `width * height * 4` composite
    /// allocation. Height is kept at 2 so the fixture's real (uncompressed) pixel data is a trivial
    /// ~150KB rather than gigabytes.
    #[test]
    fn read_image_data_url_rejects_an_overwide_psd() {
        let d = scratch("psdbomb");
        let f = d.join("bomb.psd");
        fs::write(&f, minimal_psd(25_000, 2)).unwrap();
        let err = read_image_data_url(&f.to_string_lossy());
        assert!(err.is_err(), "a PSD wider than MAX_IMAGE_DIMENSION must be rejected, not OOM/panic");
        let _ = fs::remove_dir_all(&d);
    }
}
