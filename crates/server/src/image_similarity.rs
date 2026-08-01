//! Near-duplicate **image** scan pipeline (CPE-1200, epic CPE-997). Wires the pure perceptual cores in
//! [`crate::perceptual`] (`phash` dHash, `hamming`, single-link `cluster`) to a recursive folder walk,
//! producing groups of visually-similar images. The structural complement of [`crate::duplicates`]:
//! that finds byte-identical files (hash the whole file, exact match); this finds images that *look*
//! alike even when their bytes differ (recompression, resize, format conversion, a small crop/tweak).
//!
//! The walk mirrors [`crate::duplicates::stream_duplicates`] exactly — same skip-dot/symlinked-dir,
//! file-cap, and skip-on-error discipline — so the two scans behave identically where it matters. Only
//! the per-file work differs: instead of size-bucketing + SHA-256, each candidate image is decoded and
//! reduced to a 64-bit dHash via [`crate::perceptual::phash`], then all hashes are clustered by Hamming
//! distance.
//!
//! ## Streaming ([[prefer-streaming-liveness]])
//!
//! Follows the STREAMING-liveness convention: [`stream_similar_images`] is the shared walker backing
//! both a collect-to-vec [`find_similar_images`] and the Tauri streaming command. Unlike duplicate
//! detection — where each size-collision can be confirmed and flushed independently mid-walk — image
//! clustering is inherently a **whole-set** operation (single-link clustering needs every hash before it
//! can decide the transitive closure). So the walk streams **progress** honestly: the (single) result
//! batch is flushed once, after the full walk + cluster completes. This still frees the UI thread and
//! lets the frontend flip `loading` off on the first (and only) batch, matching the streaming contract's
//! shape even though the payload can't be produced incrementally.
//!
//! ## Threshold (`max_distance`)
//!
//! [`DEFAULT_MAX_DISTANCE`] is **10** bits out of 64. dHash near-duplicates (a re-encode / resize / minor
//! edit of the same image) typically land within ~0–8 Hamming bits of the original, while visually
//! distinct images sit far higher (tens of bits). 10 sits just above that near-duplicate band: high
//! enough to absorb the small bit-flips a JPEG re-encode or a resize introduces (the `perceptual.rs`
//! golden tests show resized copies within <10 bits), low enough that unrelated images don't get pulled
//! into a group. It's the midpoint of the epic's 8–12 bits/64 guidance — a deliberately conservative
//! default favouring precision (few false groupings) over recall, since a missed near-duplicate is less
//! surprising to a user than two unrelated photos wrongly grouped.

use std::path::Path;

use serde::Serialize;

use crate::fsutil::entry_is_symlink;

/// Default perceptual-distance threshold for grouping two images as near-duplicates: the maximum Hamming
/// distance (in bits, out of 64) between their dHashes. See the module docs for the justification. dHash
/// near-duplicates cluster within ~0–8 bits; 10 sits just above that band (mid-point of the epic's 8–12
/// guidance), favouring precision over recall.
pub const DEFAULT_MAX_DISTANCE: u32 = 10;

/// Cap on the number of image candidates hashed in one scan — mirrors `duplicates::DUP_MAX_FILES` so the
/// two scans truncate consistently. Hitting it sets `truncated`.
const SIM_MAX_FILES: u64 = 50_000;

/// Lowercase extensions the `image` crate can decode and that `perceptual::phash` handles — the filter
/// that keeps the walk from trying to decode every file. A non-matching extension is skipped cheaply
/// (no read); a matching file that still fails to decode is skipped by `phash` returning `None`.
const IMAGE_EXTS: &[&str] =
    &["png", "jpg", "jpeg", "jpe", "gif", "bmp", "webp", "tif", "tiff", "ico", "tga", "ppm", "pgm", "pbm"];

fn is_image_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| IMAGE_EXTS.contains(&e.as_str()))
}

/// A set of visually-similar images: every path whose dHash landed in one near-duplicate cluster. Only
/// groups with 2+ members are produced (a singleton has no near-duplicate — dropped, mirroring
/// [`crate::perceptual::cluster`]).
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SimGroup {
    pub paths: Vec<String>,
}

/// The result of a similar-image scan: the near-duplicate groups, how many image candidates were hashed,
/// and whether the file cap truncated the walk. Mirrors [`crate::duplicates::DupResult`].
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SimResult {
    pub groups: Vec<SimGroup>,
    pub files_scanned: u64,
    pub truncated: bool,
}

/// The non-group part of a similar-image scan: images hashed + whether the file cap truncated it.
pub struct SimWalkStats {
    pub files_scanned: u64,
    pub truncated: bool,
}

/// The shared similar-image walk (CPE-1200, streaming-liveness convention). Recursively walks `root`
/// mirroring [`crate::duplicates::stream_duplicates`] — skips dot-dirs and symlinked dirs (cycle-safe),
/// caps candidates at [`SIM_MAX_FILES`] (reporting `truncated`), and skips unreadable entries — but
/// filters to decodable image extensions and reduces each to a [`crate::perceptual::phash`]. After the
/// walk, the collected `(path, hash)` pairs are clustered via [`crate::perceptual::cluster`] at
/// `max_distance` and the resulting groups (2+ members, deterministically ordered) are handed to `flush`
/// in a single batch. `flush` returns a `ControlFlow` so a streaming caller can stop early. A non-folder
/// `root` is an `Err`. Backs both [`find_similar_images`] and the streaming command.
pub fn stream_similar_images(
    root: &str,
    max_distance: u32,
    flush: impl FnMut(Vec<SimGroup>) -> std::ops::ControlFlow<()>,
) -> Result<SimWalkStats, String> {
    stream_similar_images_capped(root, max_distance, SIM_MAX_FILES, flush)
}

/// [`stream_similar_images`] with an injectable file cap — the real entry point passes [`SIM_MAX_FILES`];
/// tests pass a tiny cap to exercise the `truncated` path without writing tens of thousands of files.
fn stream_similar_images_capped(
    root: &str,
    max_distance: u32,
    max_files: u64,
    mut flush: impl FnMut(Vec<SimGroup>) -> std::ops::ControlFlow<()>,
) -> Result<SimWalkStats, String> {
    let root_path = Path::new(root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }

    // Walk: collect (path, dHash) for every decodable image (skip-on-error like `list_dir`).
    let mut items: Vec<(String, u64)> = Vec::new();
    let mut files_scanned = 0u64;
    let mut truncated = false;
    let mut stack = vec![root_path.to_path_buf()];
    'walk: while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Skip dot-dirs and symlinked dirs (avoid cycles, CPE-611).
                if !name.to_string_lossy().starts_with('.') && !entry_is_symlink(&entry) {
                    stack.push(path);
                }
                continue;
            }
            if !meta.is_file() || meta.len() == 0 || !is_image_ext(&path) {
                continue;
            }
            if files_scanned >= max_files {
                truncated = true;
                break 'walk;
            }
            files_scanned += 1;
            // Read + hash; unreadable bytes or an undecodable image are silently skipped.
            if let Ok(bytes) = std::fs::read(&path) {
                if let Some(hash) = crate::perceptual::phash(&bytes) {
                    items.push((path.to_string_lossy().into_owned(), hash));
                }
            }
        }
    }

    // Cluster the whole set (single-link needs every hash before it can decide groups), then flush once.
    let groups: Vec<SimGroup> =
        crate::perceptual::cluster(&items, max_distance).into_iter().map(|paths| SimGroup { paths }).collect();
    if !groups.is_empty() {
        let _ = flush(groups); // single batch — nothing follows, so a Break is a no-op
    }
    Ok(SimWalkStats { files_scanned, truncated })
}

/// Collect-to-vec similar-image finder using [`DEFAULT_MAX_DISTANCE`]. Groups arrive already sorted
/// (deterministic, by first path) from [`crate::perceptual::cluster`]. See [`stream_similar_images`].
pub fn find_similar_images(root: &str) -> Result<SimResult, String> {
    let mut groups: Vec<SimGroup> = Vec::new();
    let stats = stream_similar_images(root, DEFAULT_MAX_DISTANCE, |batch| {
        groups.extend(batch);
        std::ops::ControlFlow::Continue(())
    })?;
    Ok(SimResult { groups, files_scanned: stats.files_scanned, truncated: stats.truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};
    use std::fs;
    use std::io::Cursor;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-simimg-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// A horizontal gradient (brightness increases left→right), encoded at `w×h` — dHash's canonical
    /// "structured" fixture. Two sizes of the same gradient are perceptual near-duplicates (mirrors the
    /// `perceptual.rs` test style).
    fn gradient_png(w: u32, h: u32) -> Vec<u8> {
        let mut img = RgbImage::new(w, h);
        for (x, _y, px) in img.enumerate_pixels_mut() {
            let v = ((x as f32 / w as f32) * 255.0) as u8;
            *px = image::Rgb([v, v, v]);
        }
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png).unwrap();
        buf
    }

    /// Alternating vertical black/white bands — a structurally *distinct* image (its dHash is far from a
    /// gradient's), so it forms no group with the gradients (dropped singleton).
    fn bands_png(n_blocks: u32, block_px: u32, h: u32) -> Vec<u8> {
        let mut img = RgbImage::new(n_blocks * block_px, h);
        for (x, _y, px) in img.enumerate_pixels_mut() {
            let v = if (x / block_px) % 2 == 0 { 0 } else { 255 };
            *px = image::Rgb([v, v, v]);
        }
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png).unwrap();
        buf
    }

    #[test]
    fn groups_reencoded_resized_near_duplicate_and_drops_distinct_singleton() {
        let d = scratch("near");
        fs::create_dir_all(d.join("sub")).unwrap();
        // Two encodings of the same gradient at different sizes → a near-duplicate pair (across subdirs).
        fs::write(d.join("orig.png"), gradient_png(300, 200)).unwrap();
        fs::write(d.join("sub/resized.png"), gradient_png(150, 100)).unwrap();
        // A structurally distinct image → no near-duplicate → dropped singleton.
        fs::write(d.join("other.png"), bands_png(9, 20, 100)).unwrap();

        let r = find_similar_images(&d.to_string_lossy()).unwrap();
        assert_eq!(r.files_scanned, 3, "three image candidates hashed");
        assert!(!r.truncated);
        assert_eq!(r.groups.len(), 1, "exactly one near-duplicate group");
        let g = &r.groups[0];
        assert_eq!(g.paths.len(), 2, "the resized pair groups; the bands image is a dropped singleton");
        let names: Vec<String> = g.paths.iter().map(|p| p.replace('\\', "/")).collect();
        assert!(names.iter().any(|p| p.ends_with("orig.png")));
        assert!(names.iter().any(|p| p.ends_with("sub/resized.png")));
        assert!(!names.iter().any(|p| p.ends_with("other.png")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn walk_skips_non_images_dot_dirs_and_unreadable_entries() {
        let d = scratch("skip");
        // A real near-duplicate pair.
        fs::write(d.join("a.png"), gradient_png(300, 200)).unwrap();
        fs::write(d.join("b.png"), gradient_png(150, 100)).unwrap();
        // Non-image files (wrong extension, and image bytes hidden behind a .txt) → never hashed.
        fs::write(d.join("notes.txt"), b"i am not an image").unwrap();
        fs::write(d.join("readme"), b"no extension").unwrap();
        // A .png that isn't actually decodable → counted as a candidate but skipped by phash.
        fs::write(d.join("corrupt.png"), b"PNG-ish but not really").unwrap();
        // A dot-dir with an image inside → the whole dir is skipped, its image never scanned.
        fs::create_dir_all(d.join(".hidden")).unwrap();
        fs::write(d.join(".hidden/c.png"), gradient_png(300, 200)).unwrap();

        let r = find_similar_images(&d.to_string_lossy()).unwrap();
        // Candidates by extension: a.png, b.png, corrupt.png = 3 (notes.txt/readme filtered by ext;
        // .hidden/c.png never entered). corrupt.png fails to decode so contributes no hash.
        assert_eq!(r.files_scanned, 3, "only .png candidates counted; dot-dir + non-images excluded");
        assert_eq!(r.groups.len(), 1, "the a/b gradient pair, and nothing from the dot-dir");
        assert_eq!(r.groups[0].paths.len(), 2);
        let names: Vec<String> = r.groups[0].paths.iter().map(|p| p.replace('\\', "/")).collect();
        assert!(!names.iter().any(|p| p.contains(".hidden")), "dot-dir contents must be skipped");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn non_folder_root_is_err_and_empty_folder_has_no_groups() {
        let d = scratch("edge");
        fs::write(d.join("only.png"), gradient_png(300, 200)).unwrap();
        // A single image can't form a group.
        let r = find_similar_images(&d.to_string_lossy()).unwrap();
        assert!(r.groups.is_empty(), "one image → no group");
        assert_eq!(r.files_scanned, 1);
        // A non-folder root is an Err (mirrors find_duplicates).
        assert!(find_similar_images(&d.join("only.png").to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn truncated_is_set_when_the_file_cap_is_hit() {
        let d = scratch("cap");
        for i in 0..5 {
            fs::write(d.join(format!("g{i}.png")), gradient_png(300, 200)).unwrap();
        }
        // Under the real cap → not truncated; all five identical gradients cluster into one group.
        let full = find_similar_images(&d.to_string_lossy()).unwrap();
        assert!(!full.truncated, "well under the cap");
        assert_eq!(full.files_scanned, 5);
        assert_eq!(full.groups.len(), 1);
        assert_eq!(full.groups[0].paths.len(), 5);

        // A cap of 2 stops the walk early → truncated set, exactly `max_files` candidates counted.
        let mut groups = Vec::new();
        let stats = stream_similar_images_capped(&d.to_string_lossy(), DEFAULT_MAX_DISTANCE, 2, |batch| {
            groups.extend(batch);
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        assert!(stats.truncated, "hitting the cap must set truncated");
        assert_eq!(stats.files_scanned, 2, "the walk stops the instant the cap is reached");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn output_order_is_deterministic() {
        let d = scratch("order");
        // Two independent near-duplicate groups: gradients (group 1) and band images (group 2).
        fs::write(d.join("grad_a.png"), gradient_png(300, 200)).unwrap();
        fs::write(d.join("grad_b.png"), gradient_png(150, 100)).unwrap();
        fs::write(d.join("band_a.png"), bands_png(9, 20, 100)).unwrap();
        fs::write(d.join("band_b.png"), bands_png(9, 20, 80)).unwrap();

        let r1 = find_similar_images(&d.to_string_lossy()).unwrap();
        let r2 = find_similar_images(&d.to_string_lossy()).unwrap();
        let paths = |r: &SimResult| -> Vec<Vec<String>> {
            r.groups.iter().map(|g| g.paths.iter().map(|p| p.replace('\\', "/")).collect()).collect()
        };
        assert_eq!(paths(&r1), paths(&r2), "two scans of the same tree produce identical, ordered output");
        assert_eq!(r1.groups.len(), 2, "two distinct near-duplicate groups");
        // Groups sorted by their first path: within each, paths are sorted too.
        for g in &r1.groups {
            let mut sorted = g.paths.clone();
            sorted.sort();
            assert_eq!(&g.paths, &sorted, "each group's paths are sorted");
        }
        let _ = fs::remove_dir_all(&d);
    }
}
