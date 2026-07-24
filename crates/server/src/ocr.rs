//! OCR seam + content-addressed text cache (CPE-991, epic CPE-980): the pluggable boundary between
//! "recognise the text in an image" and the rest of the scanned-document pipeline.
//!
//! Mirrors how [`crate::provider`] ships a `FakeProvider` and [`crate::embedder`] ships a `FakeEmbedder`:
//! this defines the [`OcrEngine`] trait plus a dependency-free [`FakeOcr`], so the OCR pipeline can be
//! built and tested **end-to-end today** with **zero engine weight** — no Tesseract, no ML stack, no
//! system libs. A **real** engine (a bundled/local OCR backend) is the deferred call and will implement
//! the same trait behind a feature gate; nothing here pulls in a heavy dependency.
//!
//! [`OcrCache`] is the other half: OCR is expensive, so a given image should be recognised **once**. The
//! cache keys on a content hash of the image bytes (SHA-256 hex, via the `sha2` crate already used by
//! [`crate::fsutil`]) so identical bytes reuse the cached text and the engine is never re-invoked.

use std::collections::HashMap;

/// Recognise the text contained in an image. Object-safe (only `&self` + slice/`String`), so a caller can
/// hold a `Box<dyn OcrEngine>` and swap the [`FakeOcr`] for a real engine later without touching call sites.
pub trait OcrEngine {
    /// Return the recognised text for one image's raw bytes. A real engine decodes the image and runs OCR;
    /// [`FakeOcr`] simulates it. Recognising the same bytes twice yields the same text (deterministic).
    fn recognize(&self, image_bytes: &[u8]) -> String;
}

/// A deterministic, dependency-free OCR engine for tests + local dev: it interprets `image_bytes` as UTF-8
/// **lossily** and returns that string — simulating "the text drawn in this image is X". So a test can feed
/// text-as-bytes (`b"hello"`) and assert the recognised text round-trips (`"hello"`), and non-UTF-8 bytes are
/// handled via the Unicode replacement character rather than panicking.
///
/// It performs no real image decoding or recognition; its value is being deterministic, fast, and
/// dependency-free so the pipeline + cache have a real `OcrEngine` to build against.
#[derive(Debug, Clone, Copy, Default)]
pub struct FakeOcr;

impl FakeOcr {
    /// A fresh fake OCR engine.
    pub fn new() -> Self {
        FakeOcr
    }
}

impl OcrEngine for FakeOcr {
    fn recognize(&self, image_bytes: &[u8]) -> String {
        // Lossy UTF-8: invalid sequences become U+FFFD, so any byte input is handled without panic.
        String::from_utf8_lossy(image_bytes).into_owned()
    }
}

/// A content-addressed cache in front of an [`OcrEngine`], so a given image is OCR'd **once**. Keyed on the
/// SHA-256 hex digest of the image bytes: identical content always maps to the same entry (a cache hit,
/// engine not invoked), and different content maps to a different entry (a miss that invokes the engine and
/// stores the result). Tracks `hits`/`misses` so a test can prove the engine ran only on a miss.
#[derive(Debug, Default)]
pub struct OcrCache {
    /// content-hash (hex) → recognised text.
    entries: HashMap<String, String>,
    /// Number of `recognize_cached` calls served from the cache without invoking the engine.
    hits: u64,
    /// Number of `recognize_cached` calls that missed and invoked the engine.
    misses: u64,
}

impl OcrCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Recognise `image_bytes`, using the cache. On a hit the stored text is returned and `engine` is **not**
    /// invoked; on a miss `engine.recognize` runs, the text is stored under the content hash, and returned.
    pub fn recognize_cached(&mut self, engine: &dyn OcrEngine, image_bytes: &[u8]) -> String {
        let key = content_hash(image_bytes);
        if let Some(text) = self.entries.get(&key) {
            self.hits += 1;
            return text.clone();
        }
        self.misses += 1;
        let text = engine.recognize(image_bytes);
        self.entries.insert(key, text.clone());
        text
    }

    /// Number of distinct images cached (one entry per unique content hash).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Calls served from the cache without invoking the engine.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Calls that missed and invoked the engine.
    pub fn misses(&self) -> u64 {
        self.misses
    }
}

/// SHA-256 hex digest of the image bytes — a stable, collision-resistant content key. Uses `sha2` (already a
/// dependency of this crate, see [`crate::fsutil::sha256_file`]) so no new crate is pulled in.
fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    // Lowercase hex — matching `fsutil`, one dependency fewer than pulling in `hex` for three lines.
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A test engine that counts how many times `recognize` is invoked, so a test can prove a cache hit did
    /// not call through. `Cell` gives interior mutability behind the `&self` trait method.
    struct CountingOcr {
        calls: Cell<usize>,
    }

    impl CountingOcr {
        fn new() -> Self {
            CountingOcr { calls: Cell::new(0) }
        }
    }

    impl OcrEngine for CountingOcr {
        fn recognize(&self, image_bytes: &[u8]) -> String {
            self.calls.set(self.calls.get() + 1);
            String::from_utf8_lossy(image_bytes).into_owned()
        }
    }

    #[test]
    fn fake_ocr_returns_text_for_text_as_bytes() {
        let ocr = FakeOcr::new();
        assert_eq!(ocr.recognize(b"hello world"), "hello world");
        assert_eq!(ocr.recognize(b""), "", "empty bytes yield empty text");
    }

    #[test]
    fn fake_ocr_handles_non_utf8_lossily_without_panic() {
        let ocr = FakeOcr::new();
        // 0xFF is not valid UTF-8; lossy decode replaces it with U+FFFD rather than panicking.
        let out = ocr.recognize(&[0xFF, b'h', b'i']);
        assert!(out.contains('\u{FFFD}'), "invalid byte becomes the replacement char: {out:?}");
        assert!(out.ends_with("hi"));
    }

    #[test]
    fn cache_returns_same_text_on_repeat_without_reinvoking_engine() {
        let engine = CountingOcr::new();
        let mut cache = OcrCache::new();

        let first = cache.recognize_cached(&engine, b"scanned page");
        let second = cache.recognize_cached(&engine, b"scanned page");

        assert_eq!(first, "scanned page");
        assert_eq!(second, first, "same text on a repeat");
        assert_eq!(engine.calls.get(), 1, "engine invoked exactly once for identical content");
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.len(), 1, "one distinct image cached");
    }

    #[test]
    fn different_content_creates_different_entries() {
        let engine = CountingOcr::new();
        let mut cache = OcrCache::new();

        assert_eq!(cache.recognize_cached(&engine, b"page one"), "page one");
        assert_eq!(cache.recognize_cached(&engine, b"page two"), "page two");
        // A repeat of the first is still a hit even after the second was inserted.
        assert_eq!(cache.recognize_cached(&engine, b"page one"), "page one");

        assert_eq!(engine.calls.get(), 2, "engine invoked once per distinct content");
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.misses(), 2);
        assert_eq!(cache.hits(), 1);
    }

    #[test]
    fn empty_bytes_are_cached_like_any_other_content() {
        let engine = CountingOcr::new();
        let mut cache = OcrCache::new();
        assert!(cache.is_empty());

        assert_eq!(cache.recognize_cached(&engine, b""), "");
        assert_eq!(cache.recognize_cached(&engine, b""), "");
        assert_eq!(engine.calls.get(), 1, "empty content is OCR'd once, then cached");
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn content_hash_is_stable_and_distinguishes_content() {
        // Pin SHA-256("") so an accidental swap to a different/non-stable hasher fails here.
        assert_eq!(
            content_hash(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_ne!(content_hash(b"a"), content_hash(b"b"));
        assert_eq!(content_hash(b"same"), content_hash(b"same"));
    }

    #[test]
    fn usable_as_a_trait_object() {
        // The seam must be object-safe so a caller can hold Box<dyn OcrEngine> and swap in a real engine.
        let engine: Box<dyn OcrEngine> = Box::new(FakeOcr::new());
        let mut cache = OcrCache::new();
        assert_eq!(cache.recognize_cached(engine.as_ref(), b"boxed"), "boxed");
    }
}
