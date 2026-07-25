//! SimHash text fingerprinting (CPE-999, epic CPE-997: "Near-duplicate & similar-image detection").
//! [`crate::perceptual`] finds near-duplicate **images** via a perceptual hash + Hamming-distance
//! clustering; this module extends the same clustering core to **text/documents** via Charikar's
//! SimHash, so a caller can group near-identical notes/READMEs/text files the same way it groups
//! near-identical photos — one clustering primitive, two fingerprint sources. Pure std: no new
//! dependencies, no filesystem I/O (a caller reads file bytes/text and feeds `(id, text)` pairs in).
//!
//! ## Algorithm: Charikar SimHash
//!
//! Unlike a cryptographic hash (where a one-character edit scrambles every bit), SimHash is built so
//! that **similar inputs produce hashes with a small Hamming distance** — the same distance metric and
//! clustering ([`crate::perceptual::hamming`], [`crate::perceptual::cluster`]) that already backs image
//! near-duplicate detection:
//!
//! 1. Tokenise the text (lowercase, split on non-alphanumeric runs, drop empties — the same tokenizer
//!    shape as [`crate::embedder`]'s bag-of-words tokenizer, so behaviour is consistent across the two
//!    "turn text into features" modules in this crate).
//! 2. Hash each token to a stable 64-bit fingerprint with FNV-1a (not `std::collections::hash_map::
//!    DefaultHasher`, which is randomly seeded per-process and would make the same document hash
//!    differently between runs — SimHash's whole point is a *stable*, comparable fingerprint).
//! 3. Maintain a 64-slot signed accumulator, one slot per bit position. For every token, for every bit
//!    position `i` of that token's hash: add 1 to slot `i` if the bit is set, subtract 1 if it's clear.
//!    A token that repeats naturally contributes more weight each time it appears (no separate term-
//!    frequency step needed).
//! 4. The final hash's bit `i` is `1` iff accumulator slot `i` ended up `> 0` (ties — an exactly-zero
//!    slot — resolve to `0`).
//!
//! Two documents that share most of their vocabulary end up with most accumulator slots pushed the same
//! direction, so their SimHashes differ in only a handful of bits; two unrelated documents' hashes are
//! close to random relative to each other (~32 bits apart on average for a 64-bit hash). That's exactly
//! the property [`crate::perceptual::cluster`] already exploits for images.

use crate::perceptual::cluster;

/// FNV-1a 64-bit hash — the 64-bit sibling of [`crate::embedder`]'s 32-bit `fnv1a`. Stable (cross-run,
/// cross-platform, unlike `std`'s randomly-seeded `DefaultHasher`), which matters here because SimHash's
/// whole value proposition is a fingerprint that's *comparable* across calls/processes.
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut h: u64 = OFFSET_BASIS;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}

/// Lowercase + split on non-alphanumeric runs into tokens (empties dropped) — same shape as
/// [`crate::embedder`]'s tokenizer, kept as a private copy here since that one isn't `pub`.
fn tokenize(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
}

/// Compute the 64-bit Charikar SimHash of `text`. Deterministic: identical text (up to the tokenizer's
/// case/punctuation normalisation) always produces the same value, and texts sharing most of their
/// vocabulary produce hashes that are close in Hamming distance ([`crate::perceptual::hamming`]).
///
/// Empty text, or text with no alphanumeric tokens (e.g. only punctuation/whitespace), returns `0` —
/// this falls out of the algorithm naturally (an all-zero accumulator has no slot `> 0`), so it isn't a
/// special case in the code, just a documented consequence of it.
pub fn simhash(text: &str) -> u64 {
    let mut acc: [i64; 64] = [0; 64];

    for token in tokenize(text) {
        let h = fnv1a64(token.as_bytes());
        for (i, slot) in acc.iter_mut().enumerate() {
            if (h >> i) & 1 == 1 {
                *slot += 1;
            } else {
                *slot -= 1;
            }
        }
    }

    let mut result: u64 = 0;
    for (i, &slot) in acc.iter().enumerate() {
        if slot > 0 {
            result |= 1u64 << i;
        }
    }
    result
}

/// Group `docs` (an id paired with its full text) into near-duplicate clusters: hashes each doc's text
/// with [`simhash`], then hands the resulting `(id, u64)` pairs to [`crate::perceptual::cluster`] — the
/// same union-find, single-link, 2+-member-groups-only, deterministically-ordered clustering that
/// already backs image near-duplicate detection. `max_distance` is a Hamming-bit threshold (see the
/// module-level "recommended default" note below); it's a caller parameter, not hard-coded, so a future
/// UI can expose it as a sensitivity slider the same way the perceptual image core does.
pub fn near_duplicate_docs(docs: &[(String, String)], max_distance: u32) -> Vec<Vec<String>> {
    let fingerprints: Vec<(String, u64)> =
        docs.iter().map(|(id, text)| (id.clone(), simhash(text))).collect();
    cluster(&fingerprints, max_distance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perceptual::hamming;

    // Realistic multi-sentence fixtures: SimHash's "similar text -> small distance" property is noisy on
    // very short strings (too few tokens for the accumulator to average out), so these paragraphs give
    // it enough vocabulary to hold robustly.
    const PARAGRAPH_A: &str = "The quick brown fox jumps over the lazy dog near the riverbank. \
        It happens every morning just after sunrise, when the grass is still wet with dew. \
        Local farmers say the fox has been doing this for several years without fail.";

    // Same paragraph as A with ONE word changed ("morning" -> "afternoon") and one short sentence
    // appended — a realistic "lightly edited" near-duplicate.
    const PARAGRAPH_A_EDITED: &str = "The quick brown fox jumps over the lazy dog near the riverbank. \
        It happens every afternoon just after sunrise, when the grass is still wet with dew. \
        Local farmers say the fox has been doing this for several years without fail. \
        Nobody quite knows why the fox prefers that particular spot.";

    // A completely unrelated document: different topic, almost no shared vocabulary with PARAGRAPH_A.
    const PARAGRAPH_UNRELATED: &str = "Quarterly revenue increased twelve percent, driven mainly by \
        strong demand in the northern sales region and a new pricing structure for enterprise customers. \
        The finance team expects margins to hold steady into the next fiscal year.";

    #[test]
    fn simhash_is_deterministic() {
        let h1 = simhash(PARAGRAPH_A);
        let h2 = simhash(PARAGRAPH_A);
        assert_eq!(h1, h2, "hashing the same text twice must be identical");
    }

    #[test]
    fn identical_text_has_zero_hamming_distance() {
        let h1 = simhash(PARAGRAPH_A);
        let h2 = simhash(String::from(PARAGRAPH_A).as_str());
        assert_eq!(hamming(h1, h2), 0);
    }

    #[test]
    fn near_identical_text_is_closer_than_unrelated_text() {
        let a = simhash(PARAGRAPH_A);
        let edited = simhash(PARAGRAPH_A_EDITED);
        let unrelated = simhash(PARAGRAPH_UNRELATED);

        let near = hamming(a, edited);
        let far = hamming(a, unrelated);

        // Measured on these fixtures: near=2, far=15 — a real, non-marginal gap. Thresholds below give
        // headroom either side of the measured values rather than pinning them exactly, so the test
        // stays robust to minor future edits to the fixtures without being tautological.
        assert!(near < far, "near pair ({near}) should be closer than the unrelated pair ({far})");
        assert!(near <= 8, "lightly-edited near-duplicate should stay within a small Hamming radius, got {near}");
        assert!(far >= 12, "an unrelated document should land much farther away, got {far}");
        assert!(far - near >= 6, "expected a real gap between near and far distances, got near={near} far={far}");
    }

    #[test]
    fn empty_or_whitespace_only_text_hashes_to_zero() {
        assert_eq!(simhash(""), 0);
        assert_eq!(simhash("   "), 0);
        assert_eq!(simhash("...!!!???"), 0);
        assert_eq!(simhash("  ---  ,,,  "), 0);
    }

    #[test]
    fn near_duplicate_docs_returns_only_the_near_pair() {
        let docs = vec![
            ("doc_b_edited".to_string(), PARAGRAPH_A_EDITED.to_string()),
            ("doc_a_original".to_string(), PARAGRAPH_A.to_string()),
            ("doc_c_unrelated".to_string(), PARAGRAPH_UNRELATED.to_string()),
        ];

        let groups = near_duplicate_docs(&docs, 12);

        assert_eq!(groups.len(), 1, "only the near pair should form a group; the unrelated doc is a dropped singleton");
        assert_eq!(
            groups[0],
            vec!["doc_a_original".to_string(), "doc_b_edited".to_string()],
            "group ids sorted"
        );
    }

    #[test]
    fn near_duplicate_docs_is_input_order_independent() {
        let forward = vec![
            ("a".to_string(), PARAGRAPH_A.to_string()),
            ("b".to_string(), PARAGRAPH_A_EDITED.to_string()),
            ("c".to_string(), PARAGRAPH_UNRELATED.to_string()),
        ];
        let reversed: Vec<(String, String)> = forward.iter().cloned().rev().collect();

        let groups_forward = near_duplicate_docs(&forward, 12);
        let groups_reversed = near_duplicate_docs(&reversed, 12);

        assert_eq!(groups_forward, groups_reversed, "clustering result must not depend on input order");
        assert_eq!(groups_forward.len(), 1);
        assert_eq!(groups_forward[0], vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn near_duplicate_docs_threshold_controls_grouping() {
        // Same three docs, but a threshold too tight to bridge even the near pair -> no groups at all.
        let docs = vec![
            ("a".to_string(), PARAGRAPH_A.to_string()),
            ("b".to_string(), PARAGRAPH_A_EDITED.to_string()),
            ("c".to_string(), PARAGRAPH_UNRELATED.to_string()),
        ];
        let near_dist = hamming(simhash(PARAGRAPH_A), simhash(PARAGRAPH_A_EDITED));

        // Threshold below the actual near-pair distance: nothing groups.
        if near_dist > 0 {
            let groups_too_tight = near_duplicate_docs(&docs, near_dist - 1);
            assert!(groups_too_tight.is_empty(), "threshold tighter than the actual distance should yield no groups");
        }

        // Threshold generous enough to catch everything: all three docs collapse into one group
        // (single-link transitivity through the near pair reaching the unrelated one at a loose enough
        // threshold is expected/documented behaviour of single-link clustering, not a bug).
        let groups_loose = near_duplicate_docs(&docs, 64);
        assert_eq!(groups_loose.len(), 1);
        assert_eq!(groups_loose[0].len(), 3);
    }

    #[test]
    fn near_duplicate_docs_empty_and_singleton_inputs() {
        assert!(near_duplicate_docs(&[], 10).is_empty());
        assert!(near_duplicate_docs(&[("only".to_string(), PARAGRAPH_A.to_string())], 10).is_empty());
    }

    #[test]
    fn fnv1a64_matches_published_test_vectors() {
        // Pin the canonical FNV-1a-64 test vectors (offset basis 0xcbf29ce484222325, prime
        // 0x100000001b3) so an accidental hash/bit-order change fails here, before it silently breaks
        // every persisted SimHash comparison.
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn simhash_stable_hash_guard() {
        // Same "no accidental algorithm change" guard as the FNV vector pin above, but for the full
        // simhash pipeline (tokenize -> per-token fnv1a64 -> 64-slot vote -> pack). Value computed once
        // by running this test and reading the actual output, then hard-coded here.
        assert_eq!(simhash(""), 0);
        assert_eq!(simhash("hello world"), 0x0410_d846_0008_8803);
    }
}
