//! OCR → semantic ingest glue (CPE-996, epic CPE-980 ↔ epic CPE-976): the thin seam that makes an
//! image-only file semantically searchable.
//!
//! Epic CPE-980 gives us OCR ([`crate::ocr`]: the [`OcrEngine`] trait + `OcrCache`); epic CPE-976 gives us
//! the document-level semantic index ([`crate::semantic_index::SemanticIndex`]). Neither knows about the
//! other. This module is the one-way composition between them: **recognise an image's text, then index that
//! text** — so a scanned page or a screenshot (no extractable text of its own) becomes a first-class hit in
//! semantic search, indexed by the words the OCR engine read out of the pixels.
//!
//! It is deliberately pure glue: no I/O, no new dependencies, no state of its own. The caller owns the
//! `SemanticIndex`, the `OcrEngine`, and (for the cached variant) the `OcrCache`; this just wires
//! `engine.recognize(bytes) → index.upsert_document(doc_id, text)`.

use crate::ocr::{OcrCache, OcrEngine};
use crate::semantic_index::SemanticIndex;

/// OCR `image_bytes` with `engine`, then index the recognised text under `doc_id` in `index`, so an
/// image-only file becomes semantically searchable by what the OCR engine read out of it.
///
/// This is an upsert: re-calling it for the same `doc_id` replaces any prior text for that document
/// (via [`SemanticIndex::upsert_document`]). An image whose OCR yields no words indexes an empty
/// document — tracked but never matched — matching the index's own tokenless-document behaviour.
pub fn index_image(
    index: &mut SemanticIndex,
    engine: &dyn OcrEngine,
    doc_id: &str,
    image_bytes: &[u8],
) {
    // 980 → 976 in two lines: recognise the pixels' text, then hand it to the semantic pipeline.
    let text = engine.recognize(image_bytes);
    index.upsert_document(doc_id, &text);
}

/// Same as [`index_image`], but resolve the OCR text through `cache` so re-indexing the **same image
/// bytes** does not re-run the (expensive) engine. On a cache hit the stored text is reused and the
/// engine is not invoked; on a miss the engine runs and the text is cached (keyed on a content hash of
/// the bytes — see [`OcrCache`]). Either way the recognised text is upserted under `doc_id`.
pub fn index_image_cached(
    index: &mut SemanticIndex,
    engine: &dyn OcrEngine,
    cache: &mut OcrCache,
    doc_id: &str,
    image_bytes: &[u8],
) {
    // The only difference from index_image: the OCR text comes from the content-addressed cache, so
    // identical bytes are recognised once even across many re-index passes.
    let text = cache.recognize_cached(engine, image_bytes);
    index.upsert_document(doc_id, &text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedder::FakeEmbedder;
    use crate::ocr::FakeOcr;
    use crate::semantic_index::SemanticIndex;
    use std::cell::Cell;

    fn fake_index() -> SemanticIndex {
        SemanticIndex::new(Box::new(FakeEmbedder::new(1024)))
    }

    /// Prove the 980 → 976 composition end-to-end: `FakeOcr` reads text-as-bytes back out, `index_image`
    /// feeds it into the semantic index, and a natural-language query retrieves the right image by its
    /// (OCR'd) meaning/keywords — exactly as if the image had extractable text.
    #[test]
    fn index_image_makes_ocrd_text_semantically_searchable() {
        let mut si = fake_index();
        // "Images" whose bytes are their drawn text (FakeOcr returns the bytes as text).
        index_image(&mut si, &FakeOcr::new(), "receipt.png", b"total amount due invoice payment");
        index_image(&mut si, &FakeOcr::new(), "landscape.png", b"mountain sunset ocean beach photo");

        assert_eq!(si.document_count(), 2, "both images indexed as documents");

        // A query about money retrieves the receipt, not the landscape.
        let hits = si.search("invoice payment", 2);
        assert_eq!(hits[0].doc_id, "receipt.png", "money query → receipt image first");
        assert!(
            !hits.iter().any(|h| h.doc_id == "landscape.png"),
            "unrelated image shares no tokens → filtered out"
        );

        // A query about scenery retrieves the landscape.
        let hits = si.search("ocean beach", 2);
        assert_eq!(hits[0].doc_id, "landscape.png", "scenery query → landscape image first");
    }

    /// A call-counting engine so a test can prove `index_image_cached` OCRs identical bytes only once.
    /// `Cell` gives interior mutability behind the `&self` trait method.
    struct CountingOcr {
        calls: Cell<usize>,
    }

    impl OcrEngine for CountingOcr {
        fn recognize(&self, image_bytes: &[u8]) -> String {
            self.calls.set(self.calls.get() + 1);
            String::from_utf8_lossy(image_bytes).into_owned()
        }
    }

    /// The cached variant must not re-OCR identical bytes across re-index passes, yet the document must
    /// still be findable after each pass.
    #[test]
    fn index_image_cached_reuses_ocr_and_stays_findable() {
        let mut si = fake_index();
        let engine = CountingOcr { calls: Cell::new(0) };
        let mut cache = OcrCache::new();
        let bytes = b"quarterly financial report figures";

        index_image_cached(&mut si, &engine, &mut cache, "report.png", bytes);
        // Re-index the exact same image bytes (e.g. a change-driven re-scan): a cache hit, no re-OCR.
        index_image_cached(&mut si, &engine, &mut cache, "report.png", bytes);

        assert_eq!(engine.calls.get(), 1, "identical bytes OCR'd exactly once (cache hit on the 2nd)");
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
        assert_eq!(si.document_count(), 1, "same doc_id upserted, not duplicated");

        // The doc is still semantically findable after the cached re-index.
        let hits = si.search("financial report", 1);
        assert_eq!(hits[0].doc_id, "report.png");
    }
}
