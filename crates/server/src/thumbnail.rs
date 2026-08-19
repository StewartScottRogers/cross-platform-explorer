//! Image thumbnails (CPE-642/644, epic CPE-615): generate a downscaled PNG thumbnail for an image file,
//! served from an mtime-keyed on-disk cache. Pure-Rust `image` decoders; extracted into the Server
//! (CPE-815). The Tauri `thumbnail` command resolves the cache dir via `ServerCtx` and wraps the PNG
//! bytes as a `data:` URL.
//!
//! Source decode (PSD + bomb-guard, plus SVG/font glyph-sheet extractors, CPE-1236) lives in
//! [`crate::thumb_source`]; EXIF-orientation correction in [`crate::thumb_orient`] (CPE-1085/1086, epic
//! CPE-718) — this module owns only the downscale + encode.

use std::fs;
use std::path::Path;

use crate::{thumb_orient, thumb_source};

/// Decode `path` and produce a downscaled PNG thumbnail whose longest edge is at most `max_edge` pixels,
/// preserving aspect ratio. `image::thumbnail` is a fast box filter — good enough for a grid tile.
pub fn make_thumbnail_png(path: &Path, max_edge: u32) -> Result<Vec<u8>, String> {
    let (img, bytes) = thumb_source::decode_thumb_image(path, max_edge)?;
    let img = thumb_orient::orient_for_display(img, &bytes);
    let edge = max_edge.max(1);
    let thumb = img.thumbnail(edge, edge);
    let mut buf = std::io::Cursor::new(Vec::new());
    thumb.write_to(&mut buf, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

/// A cache key for a thumbnail: hex SHA-256 of the path + mtime + edge, so editing the file (mtime
/// changes) or requesting a different size is a cache miss (CPE-644).
fn thumb_cache_key(path: &Path, mtime: u64, max_edge: u32) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(path.to_string_lossy().as_bytes());
    h.update(mtime.to_le_bytes());
    h.update(max_edge.to_le_bytes());
    format!("{:x}.png", h.finalize())
}

/// A file's mtime as whole seconds since the epoch (0 if unavailable).
fn file_mtime_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Keep the thumbnail cache under `cap_bytes` by deleting the oldest files first. Best-effort — cache
/// misses just regenerate.
fn prune_thumb_cache(cache_dir: &Path, cap_bytes: u64) {
    let Ok(rd) = fs::read_dir(cache_dir) else { return };
    let mut files: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> = rd
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((e.path(), m.len(), m.modified().ok()?))
        })
        .collect();
    let total: u64 = files.iter().map(|(_, l, _)| *l).sum();
    if total <= cap_bytes {
        return;
    }
    files.sort_by_key(|(_, _, t)| *t); // oldest first
    let mut to_free = total - cap_bytes;
    for (p, len, _) in files {
        if to_free == 0 {
            break;
        }
        let _ = fs::remove_file(&p);
        to_free = to_free.saturating_sub(len);
    }
}

const THUMB_CACHE_CAP_BYTES: u64 = 128 * 1024 * 1024;

/// Thumbnail PNG bytes for `path`, served from `cache_dir` when present + fresh, else generated, cached,
/// and pruned. Pure over an explicit `cache_dir` so it's testable (CPE-644).
pub fn thumbnail_cached(cache_dir: &Path, path: &Path, max_edge: u32) -> Result<Vec<u8>, String> {
    let file = cache_dir.join(thumb_cache_key(path, file_mtime_secs(path), max_edge));
    if let Ok(bytes) = fs::read(&file) {
        return Ok(bytes);
    }
    let png = make_thumbnail_png(path, max_edge)?;
    if fs::create_dir_all(cache_dir).is_ok() && fs::write(&file, &png).is_ok() {
        prune_thumb_cache(cache_dir, THUMB_CACHE_CAP_BYTES);
    }
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-thumb-{tag}"))
    }

    #[test]
    fn thumbnail_cache_keys_by_path_mtime_and_edge() {
        let p = Path::new("/a/b.png");
        assert_eq!(thumb_cache_key(p, 100, 64), thumb_cache_key(p, 100, 64));
        assert_ne!(thumb_cache_key(p, 100, 64), thumb_cache_key(p, 101, 64)); // mtime
        assert_ne!(thumb_cache_key(p, 100, 64), thumb_cache_key(p, 100, 32)); // edge
        assert_ne!(thumb_cache_key(p, 100, 64), thumb_cache_key(Path::new("/a/c.png"), 100, 64));
        assert!(thumb_cache_key(p, 100, 64).ends_with(".png"));
    }

    #[test]
    fn make_thumbnail_png_downscales_and_preserves_aspect() {
        let d = scratch("thumb");
        // A 100x40 image → longest edge scaled to 32 → 32 x ~13, aspect kept.
        image::RgbImage::from_pixel(100, 40, image::Rgb([10u8, 20, 30]))
            .save(d.join("x.png"))
            .unwrap();
        let png = make_thumbnail_png(&d.join("x.png"), 32).unwrap();
        let out = image::load_from_memory(&png).unwrap();
        assert_eq!(out.width(), 32, "longest edge scaled to max_edge");
        assert!(out.height() <= 32 && out.height() >= 10, "aspect preserved: {}", out.height());
        // A non-image file errors (frontend falls back to a generic icon).
        fs::write(d.join("t.txt"), b"not an image").unwrap();
        assert!(make_thumbnail_png(&d.join("t.txt"), 32).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    /// Build a tiny JPEG carrying an EXIF block with the given `Orientation` value, using raw
    /// APP1/Exif segment bytes prepended to a minimal JPEG — copied from
    /// `thumb_orient::tests::jpeg_with_exif_orientation` (itself copied from `batch_transform`'s fixture
    /// of the same name), the established "documented local copy" pattern for this test fixture.
    fn jpeg_with_exif_orientation(w: u32, h: u32, orientation: u16) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II"); // little-endian
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at offset 8
        tiff.extend_from_slice(&1u16.to_le_bytes()); // 1 entry
        tiff.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation tag
        tiff.extend_from_slice(&3u16.to_le_bytes()); // type SHORT
        tiff.extend_from_slice(&1u32.to_le_bytes()); // count 1
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0, 0]); // pad the 4-byte value field
        tiff.extend_from_slice(&0u32.to_le_bytes()); // no next IFD

        let mut app1 = Vec::new();
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(&tiff);

        let mut jpeg = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(w, h, image::Rgb([5u8, 6, 7]))
            .write_to(&mut jpeg, image::ImageFormat::Jpeg)
            .unwrap();
        let plain = jpeg.into_inner();

        let mut out = Vec::new();
        out.extend_from_slice(&plain[..2]);
        out.push(0xFF);
        out.push(0xE1); // APP1
        let seg_len = (app1.len() + 2) as u16;
        out.extend_from_slice(&seg_len.to_be_bytes());
        out.extend_from_slice(&app1);
        out.extend_from_slice(&plain[2..]);
        out
    }

    /// Minimal, valid, uncompressed 8BPS PSD (RGB, 8-bit, `width` x `height`) — copied from
    /// `thumb_source::tests::minimal_psd` (same "documented local copy" fixture pattern).
    fn minimal_psd(width: u32, height: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"8BPS");
        b.extend_from_slice(&1u16.to_be_bytes());
        b.extend_from_slice(&[0u8; 6]);
        b.extend_from_slice(&3u16.to_be_bytes()); // channels: R, G, B
        b.extend_from_slice(&height.to_be_bytes());
        b.extend_from_slice(&width.to_be_bytes());
        b.extend_from_slice(&8u16.to_be_bytes()); // depth 8
        b.extend_from_slice(&3u16.to_be_bytes()); // color mode RGB
        b.extend_from_slice(&0u32.to_be_bytes()); // color mode data: empty
        b.extend_from_slice(&0u32.to_be_bytes()); // image resources: empty
        b.extend_from_slice(&0u32.to_be_bytes()); // layer and mask: empty
        b.extend_from_slice(&0u16.to_be_bytes()); // compression: raw
        let plane = (width * height) as usize;
        b.extend(std::iter::repeat(200u8).take(plane));
        b.extend(std::iter::repeat(100u8).take(plane));
        b.extend(std::iter::repeat(50u8).take(plane));
        b
    }

    #[test]
    fn make_thumbnail_png_decodes_a_psd_source() {
        let d = scratch("thumbpsd");
        let f = d.join("a.psd");
        fs::write(&f, minimal_psd(20, 10)).unwrap();
        let png = make_thumbnail_png(&f, 8).unwrap();
        let out = image::load_from_memory(&png).unwrap();
        assert_eq!(out.width(), 8, "longest edge scaled to max_edge");
        assert!(out.height() <= 8 && out.height() >= 2, "aspect preserved: {}", out.height());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn make_thumbnail_png_bakes_in_exif_orientation_end_to_end() {
        // A wide (10x4) JPEG tagged orientation=6 ("rotate 90 CW to display correctly") must come out
        // of the thumbnail pipeline as a portrait thumbnail — proves decode + orient + downscale are
        // wired together correctly (CPE-1086).
        let d = scratch("thumborient");
        let f = d.join("wide.jpg");
        fs::write(&f, jpeg_with_exif_orientation(10, 4, 6)).unwrap();
        let png = make_thumbnail_png(&f, 32).unwrap();
        let out = image::load_from_memory(&png).unwrap();
        assert!(out.height() > out.width(), "expected portrait, got {}x{}", out.width(), out.height());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn make_thumbnail_png_rasterizes_an_svg_source_end_to_end() {
        // CPE-1236: proves decode-dispatch -> (no-op orient) -> downscale -> PNG-encode are wired
        // together for the SVG branch, not just `thumb_svg::rasterize_svg` in isolation.
        let d = scratch("thumbsvg");
        let f = d.join("wide.svg");
        fs::write(
            &f,
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40">
                  <rect width="100" height="40" fill="#00ff00"/>
                </svg>"##,
        )
        .unwrap();
        let png = make_thumbnail_png(&f, 32).unwrap();
        assert!(!png.is_empty());
        let out = image::load_from_memory(&png).unwrap();
        assert_eq!(out.width(), 32, "longest edge scaled to max_edge");
        assert!(out.height() <= 32 && out.height() >= 10, "aspect preserved: {}", out.height());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn make_thumbnail_png_falls_back_gracefully_for_a_malformed_svg() {
        let d = scratch("thumbsvgbad");
        let f = d.join("bad.svg");
        fs::write(&f, b"<svg><not valid xml at all").unwrap();
        let err = make_thumbnail_png(&f, 32);
        assert!(err.is_err(), "a malformed SVG must error (frontend falls back to a generic icon), not panic");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn make_thumbnail_png_falls_back_gracefully_for_a_malformed_font() {
        let d = scratch("thumbfontbad");
        let f = d.join("bad.ttf");
        fs::write(&f, b"this is not a font").unwrap();
        let err = make_thumbnail_png(&f, 32);
        assert!(err.is_err(), "malformed font data must error, not panic");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn thumbnail_cached_writes_then_reads_the_cache() {
        let d = scratch("thumbcache");
        image::RgbImage::from_pixel(60, 60, image::Rgb([1u8, 2, 3])).save(d.join("i.png")).unwrap();
        let cache = d.join("cache");
        let first = thumbnail_cached(&cache, &d.join("i.png"), 32).unwrap();
        assert_eq!(fs::read_dir(&cache).unwrap().count(), 1);
        assert_eq!(thumbnail_cached(&cache, &d.join("i.png"), 32).unwrap(), first);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn prune_thumb_cache_drops_oldest_over_cap() {
        let d = scratch("thumbprune");
        // Three 100-byte files; cap 150 → prune the oldest until <= cap.
        for (i, n) in ["a", "b", "c"].iter().enumerate() {
            fs::write(d.join(n), vec![0u8; 100]).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10 * (i as u64 + 1)));
        }
        prune_thumb_cache(&d, 150);
        let remaining: u64 = fs::read_dir(&d).unwrap().flatten().map(|e| e.metadata().unwrap().len()).sum();
        assert!(remaining <= 150, "cache should be pruned under the cap, got {remaining}");
        let _ = fs::remove_dir_all(&d);
    }
}
