//! Perceptual-hash + similarity-clustering core (CPE-998, epic CPE-997: "Near-duplicate & similar-image
//! detection"). [`crate::duplicates`] finds only **byte-identical** files (hash the whole file, exact
//! match); this module finds the complement — images that *look* alike even though their bytes differ
//! (recompression, resize, format conversion, a crop, a colour tweak). Pure over already-decoded bytes:
//! no filesystem I/O here, no new dependencies (reuses the `image` crate already in this crate's
//! `Cargo.toml`). A caller (future ticket) walks a folder, reads each candidate's bytes, and feeds
//! `(id, phash(bytes))` pairs into [`cluster`].
//!
//! ## Algorithm: dHash (difference hash)
//!
//! dHash is chosen over the more common aHash (average hash) because it's robust to brightness/contrast
//! shifts (it compares *adjacent* pixels rather than each pixel to a global average) while staying just
//! as cheap — one small resize, one grayscale pass, no DCT (unlike pHash proper). The recipe:
//!
//! 1. Decode the image and convert to grayscale (colour is discarded — near-duplicate detection here is
//!    about structure/luminance, not colour matching).
//! 2. Resize to a fixed **9×8** pixel grid with a deterministic filter ([`FilterType::Triangle`] — cheap,
//!    stable, doesn't ring like Lanczos on tiny targets). Resizing to an exact, tiny size is what makes the
//!    hash format/resolution-independent: a JPEG re-encode or a resize of the *original* rarely changes
//!    the 9×8 downsample enough to flip a comparison.
//! 3. For each of the 8 rows, compare each pixel to its right-hand neighbour (9 pixels → 8 comparisons per
//!    row × 8 rows = 64 bits): bit = 1 if the left pixel is brighter than the right one.
//! 4. Pack the 64 bits into a `u64`, row-major, MSB-first.
//!
//! Two hashes' visual similarity is then just their Hamming distance ([`hamming`]) — cheap to compute in
//! bulk, no vector math.

use std::io::Cursor;

use image::imageops::FilterType;

/// Hash width/height for the dHash grid: 9 columns (→ 8 horizontal comparisons per row) × 8 rows.
const HASH_W: u32 = 9;
const HASH_H: u32 = 8;

/// Compute a 64-bit dHash (difference hash) for `image_bytes`, or `None` if the bytes aren't a
/// decodable image (never panics — an unreadable/corrupt/unsupported file is just excluded from
/// similarity clustering, same "skip what can't be read" spirit as the rest of this crate).
///
/// Deterministic: the same bytes always produce the same hash, and — because the 9×8 target grid is
/// tiny and the filter is fixed — different encodings of the *same* underlying image (e.g. re-saved at
/// another size or as a different format) produce the same or a very close hash.
pub fn phash(image_bytes: &[u8]) -> Option<u64> {
    let img = image::ImageReader::new(Cursor::new(image_bytes)).with_guessed_format().ok()?.decode().ok()?;

    // Grayscale first, then resize — resizing a single luma channel is what makes the 9×8 samples
    // depend on luminance structure only, matching the "compare brightness" step below.
    let small = img.grayscale().resize_exact(HASH_W, HASH_H, FilterType::Triangle);
    let gray = small.to_luma8();

    let mut bits: u64 = 0;
    for row in 0..HASH_H {
        for col in 0..HASH_W - 1 {
            let left = gray.get_pixel(col, row)[0];
            let right = gray.get_pixel(col + 1, row)[0];
            bits <<= 1;
            if left > right {
                bits |= 1;
            }
        }
    }
    Some(bits)
}

/// Hamming distance between two dHashes — the number of differing bits, i.e. the perceptual distance.
/// `0` means identical hashes; `64` (the max for a `u64`) means every bit differs.
pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Group `items` (an id paired with its [`phash`]) into near-duplicate clusters: any two items whose
/// hashes are within `max_distance` Hamming bits of each other end up in the same group, transitively
/// (single-link clustering — if A is close to B and B is close to C, A/B/C all land in one group even if
/// A and C alone exceed `max_distance`). Implemented with union-find so the transitive closure is O(n²)
/// comparisons + near-linear union/find, no repeated graph traversal.
///
/// Only groups with **2 or more** members are returned — a singleton (nothing within `max_distance`) has
/// no near-duplicate, so it's dropped rather than reported as a group of one. Each group's ids are sorted
/// for a stable, deterministic order, and the groups themselves are sorted by their (already-sorted)
/// first id, so `cluster` is fully deterministic regardless of `items`' input order.
pub fn cluster(items: &[(String, u64)], max_distance: u32) -> Vec<Vec<String>> {
    let n = items.len();
    if n < 2 {
        return Vec::new();
    }

    // Union-find over item indices.
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]); // path compression
        }
        parent[x]
    }
    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    for i in 0..n {
        for j in (i + 1)..n {
            if hamming(items[i].1, items[j].1) <= max_distance {
                union(&mut parent, i, j);
            }
        }
    }

    // Bucket indices by root, then keep only buckets with 2+ members.
    let mut groups: std::collections::HashMap<usize, Vec<String>> = std::collections::HashMap::new();
    for (i, (id, _hash)) in items.iter().enumerate() {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(id.clone());
    }

    let mut result: Vec<Vec<String>> =
        groups.into_values().filter(|g| g.len() >= 2).map(|mut g| { g.sort(); g }).collect();
    result.sort_by(|a, b| a[0].cmp(&b[0]));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};

    /// Encode a solid-colour `w×h` image to bytes in `format` — a real fixture, no external files
    /// (mirrors `image_column.rs`'s test style).
    fn solid(w: u32, h: u32, rgb: [u8; 3], format: ImageFormat) -> Vec<u8> {
        let mut img = RgbImage::new(w, h);
        for px in img.pixels_mut() {
            *px = image::Rgb(rgb);
        }
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), format).unwrap();
        buf
    }

    /// A horizontal gradient (brightness increases left→right) — dHash's canonical "structured" fixture.
    fn horizontal_gradient(w: u32, h: u32, format: ImageFormat) -> Vec<u8> {
        let mut img = RgbImage::new(w, h);
        for (x, _y, px) in img.enumerate_pixels_mut() {
            let v = ((x as f32 / w as f32) * 255.0) as u8;
            *px = image::Rgb([v, v, v]);
        }
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), format).unwrap();
        buf
    }

    /// `n_blocks` solid vertical bands of alternating black/white, each `block_px` wide — a fixture with
    /// sharp, resize-stable horizontal structure (unlike a smooth monotonic gradient, whose adjacent-pixel
    /// comparisons are all the same sign and so all collapse to one repeated bit). `start_black` picks
    /// which colour band 0 is, so two calls with opposite `start_black` produce a fully-inverted dHash —
    /// a clean "maximally different" pair. Width is `n_blocks * block_px`, sized to line up with
    /// [`HASH_W`]'s 9-column resize target so the bands survive the resize as near-distinct columns.
    fn column_bands(n_blocks: u32, block_px: u32, h: u32, start_black: bool, format: ImageFormat) -> Vec<u8> {
        let mut img = RgbImage::new(n_blocks * block_px, h);
        for (x, _y, px) in img.enumerate_pixels_mut() {
            let block = x / block_px;
            let is_black = (block % 2 == 0) == start_black;
            let v = if is_black { 0 } else { 255 };
            *px = image::Rgb([v, v, v]);
        }
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), format).unwrap();
        buf
    }

    #[test]
    fn phash_is_deterministic() {
        let bytes = horizontal_gradient(200, 150, ImageFormat::Png);
        let h1 = phash(&bytes).unwrap();
        let h2 = phash(&bytes).unwrap();
        assert_eq!(h1, h2, "hashing the same bytes twice must be identical");
    }

    #[test]
    fn phash_stable_across_reencoding_the_same_image() {
        // Build one logical image, encode it to PNG bytes twice independently (two separate write_to
        // calls) — a fresh encode of the same pixel data should hash identically.
        let img = horizontal_gradient(200, 150, ImageFormat::Png);
        let h1 = phash(&img).unwrap();

        // Re-decode + re-encode as PNG again (simulates "resaved" bytes with a different byte stream).
        let decoded = image::load_from_memory(&img).unwrap();
        let mut buf2 = Vec::new();
        decoded.write_to(&mut Cursor::new(&mut buf2), ImageFormat::Png).unwrap();
        let h2 = phash(&buf2).unwrap();

        assert_eq!(h1, h2, "re-encoding identical pixel data must produce the same hash");
    }

    #[test]
    fn clearly_different_images_have_large_hamming_distance() {
        // Solid colours have no internal left/right contrast (every dHash bit is 0 either way), so they
        // make a poor "clearly different" fixture despite looking maximally different to a human — use
        // two colour-inverted striped images instead, where dHash actually has bits to compare.
        let bands_a = column_bands(9, 20, 100, true, ImageFormat::Png);
        let bands_b = column_bands(9, 20, 100, false, ImageFormat::Png);
        let ha = phash(&bands_a).unwrap();
        let hb = phash(&bands_b).unwrap();
        let dist = hamming(ha, hb);
        assert!(dist > 20, "inverted striped images should differ substantially, got {dist}");

        // Sanity: solid black vs solid white still decode + hash without panicking (both all-zero bits,
        // i.e. distance 0 — a known dHash blind spot, not a bug in this implementation: dHash only
        // compares *horizontal* neighbours, so any column-invariant image hashes to all zero bits).
        let black = solid(64, 64, [0, 0, 0], ImageFormat::Png);
        let white = solid(64, 64, [255, 255, 255], ImageFormat::Png);
        assert_eq!(phash(&black).unwrap(), phash(&white).unwrap());
    }

    #[test]
    fn near_identical_images_have_small_hamming_distance() {
        // Same gradient, encoded at two different pixel sizes (simulates a resized near-duplicate) —
        // dHash's 9x8 downsample should land on essentially the same bits either way.
        let a = horizontal_gradient(300, 200, ImageFormat::Png);
        let b = horizontal_gradient(150, 100, ImageFormat::Png);
        let ha = phash(&a).unwrap();
        let hb = phash(&b).unwrap();
        let near_dist = hamming(ha, hb);
        assert!(near_dist < 10, "resized copy of the same gradient should be near, got {near_dist}");

        // Cross-check: that near distance is smaller than the clearly-different pair from the sibling test.
        let bands_a = column_bands(9, 20, 100, true, ImageFormat::Png);
        let bands_b = column_bands(9, 20, 100, false, ImageFormat::Png);
        let far_dist = hamming(phash(&bands_a).unwrap(), phash(&bands_b).unwrap());
        assert!(near_dist < far_dist, "near pair ({near_dist}) should be closer than far pair ({far_dist})");
    }

    #[test]
    fn hamming_basics() {
        assert_eq!(hamming(0, 0), 0);
        assert_eq!(hamming(42, 42), 0, "equal hashes are distance 0");
        assert_eq!(hamming(u64::MAX, 0), 64, "fully inverted hashes are max distance");
        // Symmetric.
        assert_eq!(hamming(0b1010, 0b0110), hamming(0b0110, 0b1010));
    }

    #[test]
    fn non_image_bytes_yield_none() {
        assert_eq!(phash(b""), None);
        assert_eq!(phash(b"not an image, just text"), None);
        assert_eq!(phash(&[0x89, b'P', b'N', b'G', 0, 1, 2, 3]), None); // PNG magic, garbage body
    }

    #[test]
    fn cluster_groups_near_pair_and_omits_far_singleton() {
        // Two hashes 1 bit apart (near), one far away (32 bits flipped).
        let a: u64 = 0b0;
        let b: u64 = 0b1; // hamming(a,b) == 1
        let far: u64 = 0xFFFF_FFFF_0000_0000; // hamming(a,far) == 32

        let items = vec![("b_item".to_string(), b), ("a_item".to_string(), a), ("far_item".to_string(), far)];
        let groups = cluster(&items, 5);

        assert_eq!(groups.len(), 1, "only the near pair forms a group; the far item is a dropped singleton");
        assert_eq!(groups[0], vec!["a_item".to_string(), "b_item".to_string()], "group ids sorted");
    }

    #[test]
    fn cluster_identical_hashes_group_together() {
        let items = vec![("x".to_string(), 7u64), ("y".to_string(), 7u64), ("z".to_string(), 7u64)];
        let groups = cluster(&items, 0);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], vec!["x".to_string(), "y".to_string(), "z".to_string()]);
    }

    #[test]
    fn cluster_is_transitive_single_link() {
        // a-b within distance, b-c within distance, a-c NOT within distance directly — still one group.
        let a: u64 = 0b0000_0000;
        let b: u64 = 0b0000_0011; // hamming(a,b) = 2
        let c: u64 = 0b0000_1111; // hamming(b,c) = 2, hamming(a,c) = 4
        let items = vec![("a".to_string(), a), ("b".to_string(), b), ("c".to_string(), c)];
        let groups = cluster(&items, 2);
        assert_eq!(groups.len(), 1, "single-link transitivity should chain a-b-c into one group");
        assert_eq!(groups[0], vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn cluster_no_matches_returns_empty() {
        let items = vec![("solo1".to_string(), 0u64), ("solo2".to_string(), u64::MAX)];
        assert!(cluster(&items, 5).is_empty(), "nothing within max_distance -> no groups");
        assert!(cluster(&[], 5).is_empty());
        assert!(cluster(&[("only".to_string(), 1)], 5).is_empty(), "a single item can't form a group");
    }

    #[test]
    fn cluster_output_order_is_deterministic() {
        let items = vec![
            ("zzz".to_string(), 0u64),
            ("aaa".to_string(), 0u64),
            ("mmm".to_string(), 100u64),
            ("nnn".to_string(), 100u64),
        ];
        let groups = cluster(&items, 0);
        assert_eq!(groups.len(), 2);
        // Groups sorted by their own (sorted) first id: "aaa..." group before "mmm..." group.
        assert_eq!(groups[0], vec!["aaa".to_string(), "zzz".to_string()]);
        assert_eq!(groups[1], vec!["mmm".to_string(), "nnn".to_string()]);
    }
}
