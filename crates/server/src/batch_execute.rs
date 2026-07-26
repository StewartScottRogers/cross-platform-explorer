//! Batch **execute** runner (CPE-1084, epic CPE-723): the real-filesystem counterpart to
//! [`batch_media::plan`] and [`batch_transform::apply_ops`]. `plan` only computes *where* each output
//! goes (pure, no I/O); `apply_ops` only transforms bytes→bytes (pure, no I/O). This module is the glue
//! that actually reads each planned input off disk, runs the job's ops over its bytes, and writes the
//! result to the planned output — **skip-on-error, never fatal**, mirroring the `list_dir` /
//! `revert_engine` ethos: one bad file must never abort the whole batch.
//!
//! Non-destructive by construction: `batch_media::plan` already computed collision-safe output paths
//! distinct from every input (when `job.non_destructive`), so this module never overwrites a source file
//! — it only ever reads inputs and writes to the (different) planned outputs.

use std::fs;
use std::path::Path;

use crate::batch_media::{BatchJob, PlannedItem};
use crate::batch_transform;

/// Outcome of running a plan: how many items were written successfully, and which were skipped (with a
/// short human reason each). Skipped items are never fatal to the batch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatchReport {
    pub written: usize,
    pub skipped: Vec<(String /* input */, String /* why */)>,
}

/// Execute a planned batch: for each `PlannedItem`, read its input bytes, apply `job.ops` via
/// [`batch_transform::apply_ops`], and write the result to the item's planned output (creating the
/// output's parent directory as needed). A failing item — unreadable input, a non-image or otherwise
/// rejected transform, or a failed write — is recorded in `skipped` with a short reason and the batch
/// continues; it never aborts the run. Input files are never modified.
pub fn execute_plan(items: &[PlannedItem], job: &BatchJob) -> BatchReport {
    let mut report = BatchReport::default();
    for item in items {
        match execute_one(item, job) {
            Ok(()) => report.written += 1,
            Err(reason) => report.skipped.push((item.input.clone(), reason)),
        }
    }
    report
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
    fn empty_plan_yields_an_empty_report_with_no_panic() {
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        let report = execute_plan(&[], &job);
        assert_eq!(report, BatchReport::default());
    }
}
