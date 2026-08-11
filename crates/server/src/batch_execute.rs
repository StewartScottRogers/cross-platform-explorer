//! Batch **execute** runner (CPE-1084, epic CPE-723): the real-filesystem counterpart to
//! [`batch_media::plan`] and [`batch_transform::apply_ops`]. `plan` only computes *where* each output
//! goes (pure, no I/O); `apply_ops` only transforms bytes→bytes (pure, no I/O). This module is the glue
//! that actually reads each planned input off disk, runs the job's ops over its bytes, and writes the
//! result to the planned output — **skip-on-error, never fatal**, mirroring the `list_dir` /
//! `revert_engine` ethos: one bad file must never abort the whole batch.
//!
//! Non-destructive **only when `job.non_destructive` is true**: `batch_media::plan` then guarantees
//! every output path differs from its input, so this module only ever reads an input and writes to a
//! *different* planned output. When the caller sets `job.non_destructive = false`, `plan` drops that
//! guarantee — for an op combo with no dedicated output-renaming suffix (a lone Compress, Strip
//! metadata, or Watermark) the planned output can equal the input, and this module then **overwrites
//! the source file's bytes in place**, same as any other planned write (CPE-1590). This module itself
//! has no concept of "confirm the user actually wants that" — by the time a plan reaches here, gating
//! an in-place overwrite behind an explicit confirmation is the caller's job (the batch-media dialog,
//! `BatchMediaDialog.svelte`); see its CPE-1590 doc comment for the guard.

use std::fs;
use std::path::Path;

use crate::batch_media::{BatchJob, PlannedItem};
use crate::batch_transform;
use crate::model::OpResult;

/// Outcome of running a plan: how many items were written successfully, and which were skipped (with a
/// short human reason each). Skipped items are never fatal to the batch.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BatchReport {
    pub written: usize,
    pub skipped: Vec<(String /* input */, String /* why */)>,
}

/// Execute a planned batch, calling `flush` with each file's [`OpResult`] as it completes — the shared
/// walker behind both the blocking [`execute_plan`] (test/no-progress path) and the streaming Tauri
/// command (`batch_media_execute_stream`), per the "one walker, both callers" streaming convention. For
/// each `PlannedItem`, reads its input bytes, applies `job.ops` via [`batch_transform::apply_ops`], and
/// writes the result to the item's planned output (creating the output's parent directory as needed). A
/// failing item — unreadable input, a non-image or otherwise rejected transform, or a failed write — is
/// recorded in the returned report and reported to `flush` with a reason; it never aborts the run.
/// **Input files are modified in place whenever the planned `output == input`** — see the module doc
/// (CPE-1590) for when `plan` allows that (`job.non_destructive == false`).
pub fn execute_plan_walk(items: &[PlannedItem], job: &BatchJob, mut flush: impl FnMut(OpResult)) -> BatchReport {
    let mut report = BatchReport::default();
    for item in items {
        match execute_one(item, job) {
            Ok(()) => {
                report.written += 1;
                flush(OpResult::ok(Path::new(&item.output)));
            }
            Err(reason) => {
                flush(OpResult::err(Path::new(&item.input), &reason));
                report.skipped.push((item.input.clone(), reason));
            }
        }
    }
    report
}

/// Execute a planned batch without streaming per-file progress — the cargo-test correctness path.
/// Delegates to [`execute_plan_walk`] with a no-op flush so there is exactly one implementation.
pub fn execute_plan(items: &[PlannedItem], job: &BatchJob) -> BatchReport {
    execute_plan_walk(items, job, |_| {})
}

fn execute_one(item: &PlannedItem, job: &BatchJob) -> Result<(), String> {
    let input_bytes = fs::read(&item.input).map_err(|e| format!("could not read input: {e}"))?;
    let output_bytes = batch_transform::apply_ops(&input_bytes, &job.ops)?;

    if let Some(parent) = Path::new(&item.output).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("could not create output dir: {e}"))?;
        }
    }
    fs::write(&item.output, output_bytes).map_err(|e| format!("could not write output: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch_media::{plan, MediaOp};
    use image::ImageFormat;
    use std::io::Cursor;
    use std::path::PathBuf;

    /// Process-unique scratch dir (mirrors `snapshot_capture`/`revert_engine`'s test pattern): a
    /// `std::env::temp_dir()` subdir keyed by tag + pid + an atomic counter, so parallel test threads and
    /// parallel CI runs never collide, and no OS-permission trickery is needed for the Windows leg.
    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-batchexec-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::RgbImage::from_pixel(w, h, image::Rgb([10u8, 20, 30]))
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn executes_a_resize_and_convert_plan_on_real_files() {
        let d = scratch("ok");
        let a = d.join("a.png");
        let b = d.join("b.png");
        let t = d.join("c.txt");
        fs::write(&a, png_bytes(64, 32)).unwrap();
        fs::write(&b, png_bytes(50, 50)).unwrap();
        fs::write(&t, b"not an image").unwrap();

        let orig_a = fs::read(&a).unwrap();
        let orig_b = fs::read(&b).unwrap();
        let orig_t = fs::read(&t).unwrap();

        let job = BatchJob::new(vec![
            MediaOp::Resize { max_px: 16 },
            MediaOp::Convert { to_ext: "jpg".into() },
        ]);
        let inputs = vec![
            a.to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
            t.to_string_lossy().to_string(),
        ];
        let items = plan(&job, &inputs);
        assert_eq!(items.len(), 3);

        let report = execute_plan(&items, &job);

        assert_eq!(report.written, 2, "the two PNGs should succeed");
        assert_eq!(report.skipped.len(), 1, "the .txt should be skipped, not fatal");
        assert_eq!(report.skipped[0].0, t.to_string_lossy().to_string());

        // Both image outputs exist, are JPEGs, and respect the resize cap.
        for item in items.iter().take(2) {
            let out_bytes = fs::read(&item.output)
                .unwrap_or_else(|e| panic!("expected output at {}: {e}", item.output));
            assert_eq!(image::guess_format(&out_bytes).unwrap(), ImageFormat::Jpeg);
            let decoded = image::load_from_memory(&out_bytes).unwrap();
            assert!(decoded.width() <= 16 && decoded.height() <= 16);
        }

        // The rejected .txt item's planned output must NOT have been created.
        assert!(!Path::new(&items[2].output).exists());

        // Non-destructive: every input is byte-for-byte unchanged.
        assert_eq!(fs::read(&a).unwrap(), orig_a);
        assert_eq!(fs::read(&b).unwrap(), orig_b);
        assert_eq!(fs::read(&t).unwrap(), orig_t);

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_input_is_skipped_with_a_reason_not_fatal() {
        let d = scratch("missing");
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        let missing = d.join("nope.png").to_string_lossy().to_string();
        let items = plan(&job, std::slice::from_ref(&missing));

        let report = execute_plan(&items, &job);

        assert_eq!(report.written, 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, missing);
        assert!(!report.skipped[0].1.is_empty());

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_watermark_overlay_is_skipped_with_a_reason_not_fatal() {
        // The INPUT file is real and valid; only the watermark's overlay path is missing. This must
        // still be a per-file skip (with a reason), not a fatal error for the whole batch.
        let d = scratch("watermark-missing-overlay");
        let a = d.join("a.png");
        fs::write(&a, png_bytes(20, 20)).unwrap();

        let job = BatchJob::new(vec![MediaOp::Watermark {
            image: d.join("nope-logo.png").to_string_lossy().to_string(),
            position: crate::batch_media::Corner::BottomRight,
            opacity: 50,
        }]);
        let inputs = vec![a.to_string_lossy().to_string()];
        let items = plan(&job, &inputs);

        let report = execute_plan(&items, &job);
        assert_eq!(report.written, 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, a.to_string_lossy().to_string());
        assert!(!report.skipped[0].1.is_empty(), "the skip must carry a reason");
        // The valid input itself must be untouched.
        assert_eq!(fs::read(&a).unwrap(), png_bytes(20, 20));

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn empty_plan_yields_an_empty_report_with_no_panic() {
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        let report = execute_plan(&[], &job);
        assert_eq!(report, BatchReport::default());
    }

    #[test]
    fn execute_plan_walk_empty_input_never_panics_and_never_flushes() {
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        let mut flushed = 0usize;
        let report = execute_plan_walk(&[], &job, |_| flushed += 1);
        assert_eq!(report, BatchReport::default());
        assert_eq!(flushed, 0);
    }

    /// Streamed outcomes (`execute_plan_walk` + a collecting `flush`) must match a direct `execute_plan`
    /// run byte-for-byte: same written count, same skipped set — proving the streaming command and the
    /// blocking command run the identical per-file logic (no drift between the two callers).
    #[test]
    fn streamed_walk_matches_direct_execute_plan_same_written_and_skips() {
        let d = scratch("streamed-vs-direct");
        let a = d.join("a.png");
        let b = d.join("b.png");
        let t = d.join("c.txt");
        fs::write(&a, png_bytes(40, 20)).unwrap();
        fs::write(&b, png_bytes(30, 30)).unwrap();
        fs::write(&t, b"not an image").unwrap();

        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }, MediaOp::StripMetadata]);
        let inputs = vec![
            a.to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
            t.to_string_lossy().to_string(),
        ];
        let items_direct = plan(&job, &inputs);
        let direct_report = execute_plan(&items_direct, &job);

        // Re-plan into a sibling scratch dir so the streamed run doesn't clash with the direct run's
        // already-written outputs (both plans are non-destructive but share the same source dir).
        let d2 = scratch("streamed-vs-direct-2");
        let a2 = d2.join("a.png");
        let b2 = d2.join("b.png");
        let t2 = d2.join("c.txt");
        fs::write(&a2, png_bytes(40, 20)).unwrap();
        fs::write(&b2, png_bytes(30, 30)).unwrap();
        fs::write(&t2, b"not an image").unwrap();
        let inputs2 = vec![
            a2.to_string_lossy().to_string(),
            b2.to_string_lossy().to_string(),
            t2.to_string_lossy().to_string(),
        ];
        let items_streamed = plan(&job, &inputs2);

        let mut streamed_results: Vec<OpResult> = Vec::new();
        let streamed_report = execute_plan_walk(&items_streamed, &job, |r| streamed_results.push(r));

        assert_eq!(streamed_report.written, direct_report.written);
        assert_eq!(streamed_report.skipped.len(), direct_report.skipped.len());
        assert_eq!(streamed_report.written, 2, "the two PNGs should succeed");
        assert_eq!(streamed_report.skipped.len(), 1, "the .txt should be skipped, not fatal");

        // One OpResult flushed per planned item, and the ok/err split matches the report.
        assert_eq!(streamed_results.len(), items_streamed.len());
        let ok_count = streamed_results.iter().filter(|r| r.ok).count();
        let err_count = streamed_results.iter().filter(|r| !r.ok).count();
        assert_eq!(ok_count, streamed_report.written);
        assert_eq!(err_count, streamed_report.skipped.len());
        for r in streamed_results.iter().filter(|r| !r.ok) {
            assert!(!r.error.is_empty(), "a skipped OpResult must carry a reason");
        }

        let _ = fs::remove_dir_all(&d);
        let _ = fs::remove_dir_all(&d2);
    }

    /// QA-Architect burndown (CPE-1115 follow-up): pin the exact scenario the user hit by hand on 0.57.36 —
    /// a file with a valid image *extension* (and even a valid JPEG SOI marker) but **no decodable image
    /// data** must be SKIPPED-with-a-reason (never fatal), while the valid images in the same batch still
    /// produce outputs. The pre-existing skip test uses a `.txt` (obviously not an image); this covers the
    /// subtler "looks like an image, isn't" case that made "2 selected → 1 output" look like a lost-file bug.
    #[test]
    fn a_real_looking_but_undecodable_image_is_skipped_while_valid_files_still_succeed() {
        let d = scratch("undecodable");
        let good_png = d.join("pixel.png");
        let good_jpg = d.join("real.jpg");
        let bad_jpg = d.join("photo.jpg"); // valid .jpg ext + JPEG SOI/EOI, but no frame/scan → undecodable
        fs::write(&good_png, png_bytes(4, 4)).unwrap();
        {
            let mut buf = Cursor::new(Vec::new());
            image::RgbImage::from_pixel(8, 8, image::Rgb([120u8, 130, 140]))
                .write_to(&mut buf, ImageFormat::Jpeg)
                .unwrap();
            fs::write(&good_jpg, buf.into_inner()).unwrap();
        }
        fs::write(&bad_jpg, [0xFFu8, 0xD8, 0xFF, 0xD9]).unwrap(); // SOI + EOI, zero image content

        let job = BatchJob::new(vec![MediaOp::Compress { quality: 80 }]);
        let inputs = vec![
            good_png.to_string_lossy().to_string(),
            good_jpg.to_string_lossy().to_string(),
            bad_jpg.to_string_lossy().to_string(),
        ];
        let items = plan(&job, &inputs);
        assert_eq!(items.len(), 3, "one planned item per input");

        let report = execute_plan(&items, &job);
        assert_eq!(report.written, 2, "the two valid images are written");
        assert_eq!(report.skipped.len(), 1, "the undecodable jpg is skipped, not fatal to the batch");
        assert_eq!(report.skipped[0].0, bad_jpg.to_string_lossy().to_string());
        assert!(!report.skipped[0].1.is_empty(), "the skip must carry a human reason");

        // Both valid outputs exist and decode; the skipped input produced NO output file.
        for item in items.iter().take(2) {
            let bytes = fs::read(&item.output).unwrap_or_else(|e| panic!("expected output {}: {e}", item.output));
            assert!(image::load_from_memory(&bytes).is_ok(), "valid output should decode");
        }
        assert!(!Path::new(&items[2].output).exists(), "no output for the skipped file");

        let _ = fs::remove_dir_all(&d);
    }
}
