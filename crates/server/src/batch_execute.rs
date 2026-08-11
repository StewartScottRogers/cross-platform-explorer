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
//! the source file's bytes in place**, same as any other planned write (CPE-1590).
//!
//! **The engine itself refuses that, not just the UI (CPE-1599).** Before touching any bytes,
//! [`execute_plan_walk`] scans `items` for one whose planned output is the same file as its input per
//! [`crate::batch_media::same_file`] — **not** raw string equality (CPE-1613): `plan` lower-cases a `Convert` target's
//! extension, so a planned output can be textually different yet the SAME file on a case-insensitive
//! filesystem, and that must be caught too. If it finds one and `job.confirmed_overwrite` is not set, it
//! returns `Err` and writes nothing at all — a clean, specific refusal, not a panic and not a silent
//! no-op that quietly skips the dangerous files. This closes the gap a purely frontend confirm
//! (`BatchMediaDialog.svelte`'s "Overwrite N files" panel) can't: a devtools call, a future
//! automation/agent feature, or a new UI surface that invokes `batch_media_execute_stream` directly no
//! longer gets an in-place overwrite for free just by setting `non_destructive: false` — it must ALSO
//! carry `confirmed_overwrite: true`, which should only ever be set by that one confirm panel after
//! showing its warning. See [`BatchJob::confirmed_overwrite`]'s doc for the ownership of that promise.
//!
//! **Known, out-of-scope gap: TOCTOU.** [`any_in_place`]'s scan runs once per batch, before any bytes are
//! touched — not once per write. A file that changes identity between the check and its own write (e.g.
//! swapped for a symlink mid-batch) isn't re-checked. Filed separately as CPE-1624; not fixed here.

use std::fs;
use std::path::Path;

use crate::batch_media::{same_file, BatchJob, PlannedItem};
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

/// True when running `items` would overwrite at least one input file's bytes in place — decided by
/// [`same_file`] (CPE-1613), **not** raw string equality: `batch_media::plan` lower-cases a `Convert`
/// target's extension, so a planned `output` can be textually different from `input` yet be the SAME
/// file on a case-insensitive filesystem (Windows, default macOS). Shared by the [`execute_plan_walk`]
/// refusal check and available to any future caller that wants to ask "would this plan be destructive?"
/// without duplicating the comparison.
pub fn any_in_place(items: &[PlannedItem]) -> bool {
    items.iter().any(|it| same_file(&it.input, &it.output))
}

/// Execute a planned batch, calling `flush` with each file's [`OpResult`] as it completes — the shared
/// walker behind both the blocking [`execute_plan`] (test/no-progress path) and the streaming Tauri
/// command (`batch_media_execute_stream`), per the "one walker, both callers" streaming convention. For
/// each `PlannedItem`, reads its input bytes, applies `job.ops` via [`batch_transform::apply_ops`], and
/// writes the result to the item's planned output (creating the output's parent directory as needed). A
/// failing item — unreadable input, a non-image or otherwise rejected transform, or a failed write — is
/// recorded in the returned report and reported to `flush` with a reason; it never aborts the run.
///
/// **Refuses up front** ([`Err`], nothing written, `flush` never called) when any planned `output ==
/// input` and `job.confirmed_overwrite` is `false` — see the module doc (CPE-1590/CPE-1599). Otherwise,
/// input files ARE modified in place wherever the planned `output == input`.
pub fn execute_plan_walk(
    items: &[PlannedItem],
    job: &BatchJob,
    mut flush: impl FnMut(OpResult),
) -> Result<BatchReport, String> {
    if !job.confirmed_overwrite && any_in_place(items) {
        let count = items.iter().filter(|it| same_file(&it.input, &it.output)).count();
        return Err(format!(
            "refusing to run: this plan would overwrite {count} original file{} in place, and \
             `confirmed_overwrite` was not set on the batch job — re-plan with an explicit confirmation \
             or change the job so every output differs from its input",
            if count == 1 { "" } else { "s" }
        ));
    }

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
    Ok(report)
}

/// Execute a planned batch without streaming per-file progress — the cargo-test correctness path.
/// Delegates to [`execute_plan_walk`] with a no-op flush so there is exactly one implementation.
pub fn execute_plan(items: &[PlannedItem], job: &BatchJob) -> Result<BatchReport, String> {
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

        let report = execute_plan(&items, &job).unwrap();

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

        let report = execute_plan(&items, &job).unwrap();

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

        let report = execute_plan(&items, &job).unwrap();
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
        let report = execute_plan(&[], &job).unwrap();
        assert_eq!(report, BatchReport::default());
    }

    #[test]
    fn execute_plan_walk_empty_input_never_panics_and_never_flushes() {
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        let mut flushed = 0usize;
        let report = execute_plan_walk(&[], &job, |_| flushed += 1).unwrap();
        assert_eq!(report, BatchReport::default());
        assert_eq!(flushed, 0);
    }

    // ---- CPE-1599: engine-side refusal of an unconfirmed in-place overwrite ---------------------------

    /// The core defence-in-depth guarantee: a plan whose planned `output == input` for at least one item
    /// is REFUSED — `Err`, nothing written, `flush` never called — when `job.confirmed_overwrite` is
    /// left at its default `false`. This must hold even though `job.non_destructive` was explicitly set
    /// to `false` (the only way `plan()` ever produces an in-place output in the first place): setting
    /// `non_destructive: false` alone is no longer sufficient to get an in-place write out of the engine.
    #[test]
    fn refuses_an_in_place_plan_without_confirmed_overwrite() {
        let d = scratch("refuse-unconfirmed");
        let a = d.join("a.jpg");
        fs::write(&a, png_bytes(10, 10)).unwrap();
        let orig = fs::read(&a).unwrap();

        let mut job = BatchJob::new(vec![MediaOp::Compress { quality: 80 }]); // no rename suffix
        job.non_destructive = false; // the only way plan() can resolve output == input
        assert!(!job.confirmed_overwrite, "confirmed_overwrite must default to false");

        let inputs = vec![a.to_string_lossy().to_string()];
        let items = plan(&job, &inputs);
        assert_eq!(items[0].input, items[0].output, "sanity: this plan IS in-place");

        let mut flushed = 0usize;
        let err = execute_plan_walk(&items, &job, |_| flushed += 1)
            .expect_err("an unconfirmed in-place plan must be refused, not executed");

        assert!(!err.is_empty(), "the refusal must carry a specific reason");
        assert!(err.to_lowercase().contains("confirm"), "refusal reason: {err}");
        assert_eq!(flushed, 0, "flush must never fire on a refused plan");
        // The original file must be COMPLETELY untouched — the refusal is a no-op, not a partial write.
        assert_eq!(fs::read(&a).unwrap(), orig);

        let _ = fs::remove_dir_all(&d);
    }

    /// The flip side: the identical in-place plan proceeds and actually overwrites the input once
    /// `confirmed_overwrite` is explicitly set — proving the flag is load-bearing, not decorative.
    #[test]
    fn an_in_place_plan_proceeds_once_confirmed_overwrite_is_set() {
        let d = scratch("refuse-confirmed");
        let a = d.join("a.png");
        fs::write(&a, png_bytes(12, 12)).unwrap();

        let mut job = BatchJob::new(vec![MediaOp::StripMetadata]); // no rename suffix
        job.non_destructive = false;
        job.confirmed_overwrite = true;

        let inputs = vec![a.to_string_lossy().to_string()];
        let items = plan(&job, &inputs);
        assert_eq!(items[0].input, items[0].output, "sanity: this plan IS in-place");

        let report = execute_plan(&items, &job).expect("a confirmed in-place plan must be allowed to run");
        assert_eq!(report.written, 1);
        assert!(report.skipped.is_empty());
        // The file still exists at the same path (it was overwritten in place, not left alone).
        assert!(a.exists());

        let _ = fs::remove_dir_all(&d);
    }

    /// A non-destructive plan (every output != input) is unaffected by `confirmed_overwrite` either
    /// way — the refusal check only ever fires for a plan that actually contains an in-place item.
    #[test]
    fn a_non_destructive_plan_runs_regardless_of_confirmed_overwrite() {
        for confirmed in [false, true] {
            let d = scratch(if confirmed { "nondestructive-confirmed" } else { "nondestructive-unconfirmed" });
            let a = d.join("a.png");
            fs::write(&a, png_bytes(8, 8)).unwrap();

            let mut job = BatchJob::new(vec![MediaOp::Resize { max_px: 4 }]); // default non_destructive: true
            job.confirmed_overwrite = confirmed;
            assert!(job.non_destructive);

            let inputs = vec![a.to_string_lossy().to_string()];
            let items = plan(&job, &inputs);
            assert_ne!(items[0].input, items[0].output, "sanity: this plan is NOT in-place");

            let report = execute_plan(&items, &job)
                .unwrap_or_else(|e| panic!("a non-destructive plan must never be refused (confirmed={confirmed}): {e}"));
            assert_eq!(report.written, 1);

            let _ = fs::remove_dir_all(&d);
        }
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
        let direct_report = execute_plan(&items_direct, &job).unwrap();

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
        let streamed_report = execute_plan_walk(&items_streamed, &job, |r| streamed_results.push(r)).unwrap();

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

        let report = execute_plan(&items, &job).unwrap();
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

    // ---- CPE-1613: same-file detection, not raw string equality ---------------------------------------

    /// The ticket's exact worked example, end-to-end with a REAL file on disk: `IMG_1.JPG` + Convert→jpg
    /// with "write to new files" OFF. `plan()` lower-cases only the extension, so the planned output is
    /// the textually different `"IMG_1.jpg"` — but `any_in_place`/`execute_plan_walk`'s refusal must
    /// recognise it as the SAME file on a case-insensitive filesystem (Windows, default macOS) and refuse
    /// without `confirmed_overwrite`, leaving the original byte-for-byte untouched. On a case-sensitive
    /// filesystem (Linux) these really are two different possible files, so no refusal is needed there —
    /// per CPE-1613, the check must NOT be unconditionally case-insensitive on Linux.
    #[test]
    fn cpe_1613_worked_example_real_file_in_place_detection_is_platform_gated() {
        let d = scratch("cpe1613-worked-example");
        let input = d.join("IMG_1.JPG");
        {
            let mut buf = Cursor::new(Vec::new());
            image::RgbImage::from_pixel(6, 6, image::Rgb([1u8, 2, 3]))
                .write_to(&mut buf, ImageFormat::Jpeg)
                .unwrap();
            fs::write(&input, buf.into_inner()).unwrap();
        }

        let mut job = BatchJob::new(vec![MediaOp::Convert { to_ext: "jpg".into() }]);
        job.non_destructive = false;
        let items = plan(&job, &[input.to_string_lossy().to_string()]);
        assert_eq!(items[0].output, input.with_file_name("IMG_1.jpg").to_string_lossy());

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            assert!(
                any_in_place(&items),
                "a case-only extension difference must be flagged in-place on {}",
                std::env::consts::OS
            );
            let orig = fs::read(&input).unwrap();
            let err = execute_plan_walk(&items, &job, |_| {})
                .expect_err("must refuse an unconfirmed in-place overwrite even though the strings differ");
            assert!(err.to_lowercase().contains("confirm"), "refusal reason: {err}");
            assert_eq!(fs::read(&input).unwrap(), orig, "a refused plan must never touch the original");

            // Confirmed, it proceeds and genuinely overwrites the original in place.
            job.confirmed_overwrite = true;
            let report = execute_plan(&items, &job).expect("a confirmed in-place plan must be allowed to run");
            assert_eq!(report.written, 1);
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            assert!(
                !any_in_place(&items),
                "a case-only extension difference is a DIFFERENT file on a case-sensitive filesystem"
            );
            let report = execute_plan(&items, &job).expect("a genuinely distinct output needs no refusal");
            assert_eq!(report.written, 1);
        }

        let _ = fs::remove_dir_all(&d);
    }
}
