//! Image thumbnails (CPE-642/644, epic CPE-615): generate a downscaled PNG thumbnail for an image file,
//! served from an mtime-keyed on-disk cache. Pure-Rust `image` decoders; extracted into the Server
//! (CPE-815). The Tauri `thumbnail` command resolves the cache dir via `ServerCtx` and wraps the PNG
//! bytes as a `data:` URL.

use std::fs;
use std::path::Path;

/// Decode `path` and produce a downscaled PNG thumbnail whose longest edge is at most `max_edge` pixels,
/// preserving aspect ratio. `image::thumbnail` is a fast box filter — good enough for a grid tile.
pub fn make_thumbnail_png(path: &Path, max_edge: u32) -> Result<Vec<u8>, String> {
    let img = image::open(path).map_err(|e| e.to_string())?;
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

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-thumb-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
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
