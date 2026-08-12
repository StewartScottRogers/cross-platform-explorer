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
//! **Broadened beyond self-overwrite (CPE-1623).** The refusal used to fire only for `output == input`
//! (per item, comparing a planned item against its OWN input). A planned output that instead resolves
//! onto some OTHER real file on disk — one never submitted as any of this batch's own inputs — is just as
//! destructive to overwrite unconfirmed: an ordinary rename landing on an unrelated pre-existing file's
//! name, or the directory-traversal case a security audit demonstrated. [`any_in_place`] catches both; see
//! its own doc. **This alone was NOT the "second, independent layer" an earlier cut of this doc claimed
//! it was** — see the containment paragraph immediately below for why, and what actually closes that gap.
//!
//! **The engine — not just `batch_media::plan()` — is the actual containment enforcement point (IPC-bypass
//! follow-up, PR #828).** `PlannedItem` is a plain public struct: `Serialize`/`Deserialize`, no invariants
//! of its own, nothing that enforces "this came from `plan()`". Before this fix, EVERY containment check
//! (a Rename template can't walk `output` outside `input`'s own folder) lived entirely inside
//! `batch_media::validate()`/`plan()` — and `batch_media_execute_stream`, the Tauri command backing this
//! module, took `items: Vec<PlannedItem>` straight off the IPC wire. [`is_foreign_overwrite`]'s only
//! question was "does something already exist at this output?" — never "does this output stay inside the
//! input's own folder?" — so a caller that hand-built a `PlannedItem` (skipping `plan()` entirely: a
//! devtools call, a compromised webview, a future automation surface) pointing `output` at a **path
//! nothing occupied yet** sailed straight through, `confirmed_overwrite` or not: `is_foreign_overwrite`
//! returns `false` the instant `Path::is_file()` is false, and [`execute_one`] then does an unconditional
//! `fs::write`. Demonstrated: hand-build a `PlannedItem` with `output` pointing outside `input`'s
//! directory, skip `plan()`, call [`execute_plan`] → `Ok(BatchReport { written: 1, .. })`, a file created
//! at the arbitrary path — and, with a real file already sitting there plus caller-supplied
//! `confirmed_overwrite: true`, its bytes replaced. [`execute_plan_walk`] now re-derives containment
//! itself, per item, before ANY byte is read or written — using
//! [`crate::batch_media::classify_output_containment`],
//! the identical check `plan()` uses (not a fresh, potentially-drifting reimplementation) — and refuses the
//! whole batch, nothing written, if any item's output would leave its own input's folder. This runs
//! **regardless of `confirmed_overwrite`**: that flag only ever authorises overwriting the user's OWN
//! input in place (or a foreign file the user's own plan happened to land on) — it was never meant to, and
//! must not, license writing to a folder the user never selected. The engine is now the actual enforcement
//! point end-to-end, not just this one IPC command's two former layers.
//!
//! **Known, out-of-scope gap: TOCTOU.** [`any_in_place`]'s scan (and the new containment re-check) run
//! once per batch, before any bytes are touched — not once per write. A file that changes identity between
//! the check and its own write (e.g. swapped for a symlink mid-batch) isn't re-checked. Filed separately
//! as CPE-1624; not fixed here.

use std::fs;
use std::path::Path;

use crate::batch_media::{
    classify_output_containment, same_file, BatchJob, Containment, ParentCache, PlannedItem,
};
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

/// True when `item`'s planned write would replace bytes belonging to a file that was never submitted as
/// one of `items`' own inputs — the original CPE-1613 in-place case (`item.output` is [`same_file`] as
/// `item.input`), OR (CPE-1623) `item.output` resolves onto some OTHER real, pre-existing file that isn't
/// any input in this batch. The second check is gated behind a plain `Path::is_file()` stat — cheap, and
/// false for the overwhelmingly common case (a freshly-computed non-destructive name that doesn't exist
/// yet) — so only a genuine collision pays for the `O(n)` [`same_file`] scan over the batch's own inputs.
fn is_foreign_overwrite(item: &PlannedItem, items: &[PlannedItem]) -> bool {
    if same_file(&item.input, &item.output) {
        return true;
    }
    if !Path::new(&item.output).is_file() {
        return false; // nothing sits there yet — not an overwrite of anything
    }
    // A real file already occupies the output path. It's only a refusable overwrite if it's not one of
    // THIS batch's own inputs — a batch is always allowed to write into paths it explicitly selected.
    !items.iter().any(|other| same_file(&other.input, &item.output))
}

/// True when running `items` would overwrite at least one file's bytes that isn't explicitly part of this
/// batch's own input set — decided by [`is_foreign_overwrite`] (CPE-1613/CPE-1623), **not** raw string
/// equality: `batch_media::plan` lower-cases a `Convert` target's extension, so a planned `output` can be
/// textually different from `input` yet be the SAME file on a case-insensitive filesystem (Windows,
/// default macOS). Shared by the [`execute_plan_walk`] refusal check and available to any future caller
/// that wants to ask "would this plan be destructive?" without duplicating the comparison.
pub fn any_in_place(items: &[PlannedItem]) -> bool {
    items.iter().any(|it| is_foreign_overwrite(it, items))
}

/// Execute a planned batch, calling `flush` with each file's [`OpResult`] as it completes — the shared
/// walker behind both the blocking [`execute_plan`] (test/no-progress path) and the streaming Tauri
/// command (`batch_media_execute_stream`), per the "one walker, both callers" streaming convention. For
/// each `PlannedItem`, reads its input bytes, applies `job.ops` via [`batch_transform::apply_ops`], and
/// writes the result to the item's planned output (creating the output's parent directory as needed). A
/// failing item — unreadable input, a non-image or otherwise rejected transform, or a failed write — is
/// recorded in the returned report and reported to `flush` with a reason; it never aborts the run.
///
/// **Refuses up front** ([`Err`], nothing written, `flush` never called) in two independent ways, checked
/// in this order:
///
/// 1. **Containment (IPC-bypass follow-up, unconditional — `confirmed_overwrite` has no effect on this
///    one).** Any item whose `output` would leave its own `input`'s directory, per
///    [`crate::batch_media::classify_output_containment`] — the same check `batch_media::plan()` itself
///    uses, re-derived here because a `PlannedItem` reaching this fn may never have gone through `plan()`
///    at all (see the module doc for the demonstrated bypass). This can't be waived by any flag: it isn't
///    asking "did the user consent to an overwrite?", it's asking "is this even a place the batch was
///    allowed to touch?". Reports its two refusal reasons **separately** (CPE-1642): an output whose
///    identity couldn't be established has not been shown to leave the folder, and saying otherwise would
///    tell the user something false about their own files.
/// 2. **Foreign overwrite (CPE-1590/CPE-1599/CPE-1623).** Any planned item that [`is_foreign_overwrite`]
///    — its output is the same file as its own input, OR resolves onto some other real file this batch
///    never selected — when `job.confirmed_overwrite` is `false`. Otherwise, those files ARE overwritten
///    wherever the plan says to.
pub fn execute_plan_walk(
    items: &[PlannedItem],
    job: &BatchJob,
    mut flush: impl FnMut(OpResult),
) -> Result<BatchReport, String> {
    let mut parent_cache = ParentCache::new();
    // CPE-1642: count the two refusal reasons SEPARATELY. An output whose identity could not be resolved
    // has not been shown to leave the folder — reporting it as one that "would land outside" tells the
    // user something factually untrue about their own files.
    let mut escaping = 0usize;
    let mut unverifiable: Vec<&'static str> = Vec::new();
    for it in items {
        match classify_output_containment(&it.input, &it.output, &mut parent_cache) {
            Containment::Inside => {}
            Containment::Escapes => escaping += 1,
            Containment::Unverifiable(why) => unverifiable.push(why),
        }
    }
    if escaping > 0 || !unverifiable.is_empty() {
        let mut reasons: Vec<String> = Vec::new();
        if escaping > 0 {
            reasons.push(format!(
                "{escaping} planned output{} would land outside its own input's folder",
                if escaping == 1 { "" } else { "s" }
            ));
        }
        if !unverifiable.is_empty() {
            let why = unverifiable[0];
            reasons.push(format!(
                "{} planned output{} couldn't be verified to stay inside its own input's folder ({why})",
                unverifiable.len(),
                if unverifiable.len() == 1 { "" } else { "s" }
            ));
        }
        return Err(format!(
            "refusing to run: {} — this can happen when a PlannedItem is supplied without going through \
             batch_media::plan() (which normally refuses this itself); nothing was written, and this \
             cannot be overridden by confirmed_overwrite, which only ever authorises overwriting a file \
             inside the selected folder",
            reasons.join(", and ")
        ));
    }

    if !job.confirmed_overwrite {
        let count = items.iter().filter(|it| is_foreign_overwrite(it, items)).count();
        if count > 0 {
            return Err(format!(
                "refusing to run: this plan would overwrite {count} file{} not explicitly part of this \
                 batch (either the same file as its own input, or an existing file this batch never \
                 selected), and `confirmed_overwrite` was not set on the batch job — re-plan with an \
                 explicit confirmation or change the job so every output stays clear of existing files",
                if count == 1 { "" } else { "s" }
            ));
        }
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
        let items = plan(&job, &inputs).unwrap();
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
        let items = plan(&job, std::slice::from_ref(&missing)).unwrap();

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
        let items = plan(&job, &inputs).unwrap();

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
        let items = plan(&job, &inputs).unwrap();
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
        let items = plan(&job, &inputs).unwrap();
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
            let items = plan(&job, &inputs).unwrap();
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
        let items_direct = plan(&job, &inputs).unwrap();
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
        let items_streamed = plan(&job, &inputs2).unwrap();

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
        let items = plan(&job, &inputs).unwrap();
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
        let items = plan(&job, &[input.to_string_lossy().to_string()]).unwrap();
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

    // ---- CPE-1623: broadened destructive-overwrite refusal beyond the batch's own input set -----------

    /// Fix #3 from the ticket: an output that resolves onto an EXISTING file this batch never selected as
    /// one of its own inputs must be treated exactly like an in-place self-overwrite (CPE-1599) — refused
    /// without `confirmed_overwrite`, allowed to proceed once it's set. Overwrite mode (`non_destructive:
    /// false`) is used here because that's the case `plan()` itself does no disambiguation for — see
    /// `batch_media.rs`'s `cpe_1623_non_destructive_mode_steps_around_a_real_pre_existing_unrelated_file`
    /// for the non-destructive-mode side of the fix, where `plan()` just picks a different name instead.
    #[test]
    fn cpe_1623_output_landing_on_a_foreign_existing_file_is_refused_then_allowed_once_confirmed() {
        let d = scratch("cpe1623-foreign-overwrite");
        let input = d.join("photo.png");
        fs::write(&input, png_bytes(10, 10)).unwrap();
        let foreign = d.join("vacation.png"); // real file, NOT one of this batch's inputs
        let foreign_original = png_bytes(4, 4);
        fs::write(&foreign, &foreign_original).unwrap();

        let mut job = BatchJob::new(vec![MediaOp::Rename { template: "vacation".into() }]);
        job.non_destructive = false; // explicit "write to this literal location" mode — no disambiguation
        let items = plan(&job, &[input.to_string_lossy().to_string()]).unwrap();
        assert_eq!(
            items[0].output,
            foreign.to_string_lossy(),
            "sanity: the rename lands exactly on the foreign file"
        );
        assert!(!same_file(&items[0].input, &items[0].output), "sanity: this is NOT the self-overwrite case");

        assert!(any_in_place(&items), "an output landing on a real foreign file must be flagged");
        let err = execute_plan_walk(&items, &job, |_| {})
            .expect_err("must refuse an unconfirmed overwrite of a file outside this batch");
        assert!(err.to_lowercase().contains("confirm"), "refusal reason: {err}");
        assert_eq!(
            fs::read(&foreign).unwrap(),
            foreign_original,
            "the foreign file must be untouched by a refused plan"
        );

        // Confirmed, it proceeds and genuinely overwrites the foreign file.
        job.confirmed_overwrite = true;
        let report = execute_plan(&items, &job).expect("a confirmed overwrite must be allowed to run");
        assert_eq!(report.written, 1);
        assert_ne!(fs::read(&foreign).unwrap(), foreign_original, "the confirmed write actually replaced the bytes");

        let _ = fs::remove_dir_all(&d);
    }

    /// The flip side, proving no new false alarms: a plan whose output does NOT already exist on disk is
    /// never flagged, confirmed_overwrite or not — mirrors the pre-CPE-1623
    /// `a_non_destructive_plan_runs_regardless_of_confirmed_overwrite` coverage above, but for overwrite
    /// mode specifically (where `plan()` does no disambiguation at all).
    #[test]
    fn cpe_1623_overwrite_mode_with_a_genuinely_new_output_name_needs_no_confirmation() {
        let d = scratch("cpe1623-no-false-alarm");
        let input = d.join("photo.png");
        fs::write(&input, png_bytes(10, 10)).unwrap();

        let mut job = BatchJob::new(vec![MediaOp::Rename { template: "brand-new-name".into() }]);
        job.non_destructive = false;
        let items = plan(&job, &[input.to_string_lossy().to_string()]).unwrap();

        assert!(!any_in_place(&items), "a genuinely new output name must not be flagged");
        let report = execute_plan(&items, &job).expect("no refusal expected for a non-colliding rename");
        assert_eq!(report.written, 1);

        let _ = fs::remove_dir_all(&d);
    }

    // ---- IPC-bypass follow-up: execute_plan_walk re-derives containment itself ------------------------
    //
    // The security-audit finding this closes: `PlannedItem` is a plain public struct with zero invariants
    // of its own — `Serialize`/`Deserialize`, nothing enforcing "this came from `plan()`". Every
    // containment guarantee lived in `batch_media::plan()`/`validate()`, but `batch_media_execute_stream`
    // (the Tauri command) took `items: Vec<PlannedItem>` straight off the wire, and this module's only
    // gate (`is_foreign_overwrite`) asked "does something already exist at this output?" — never "does
    // this stay inside the input's own folder?". A caller that skips `plan()` entirely (devtools, a
    // compromised webview, a future automation surface) could hand-build a `PlannedItem` pointing `output`
    // at ANY path the process can write, and a brand-new file at that path sailed straight through with no
    // refusal, `confirmed_overwrite` or not. These tests hand-build exactly such an item WITHOUT ever
    // calling `plan()`, proving `execute_plan` now refuses it — verified by reading bytes off disk, not
    // trusting the `Result`.

    /// The core PoC turned into a permanent regression test: a hand-built `PlannedItem` whose `output`
    /// resolves outside its `input`'s own directory must be refused — nothing written at all, proven by
    /// asserting the target path never came into existence.
    #[test]
    fn ipc_bypass_hand_built_escaping_planned_item_is_refused_and_writes_nothing() {
        let d = scratch("ipc-bypass-escape");
        let workdir = d.join("a").join("traversal_workdir");
        let victim_dir = d.join("cpe1613_traversal_victim");
        fs::create_dir_all(&workdir).unwrap();
        fs::create_dir_all(&victim_dir).unwrap();
        let input = workdir.join("innocuous.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();
        let victim_target = victim_dir.join("important.jpg"); // does NOT exist yet — the attack scenario

        // Hand-built PlannedItem: NEVER goes through batch_media::plan(), so none of plan()'s own
        // containment checks ever run — this simulates a caller invoking the execute IPC surface directly.
        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: victim_target.to_string_lossy().to_string(),
            summary: "hand-built, bypassing plan()".into(),
        }];
        let job = BatchJob::new(vec![MediaOp::StripMetadata]);

        let err = execute_plan(&items, &job)
            .expect_err("an output escaping the input's own directory must be refused even without plan()");
        assert!(err.to_lowercase().contains("folder"), "refusal reason: {err}");

        // Byte-level proof, not a trust-the-return-value check: the target must never have been created.
        assert!(!victim_target.exists(), "the escaping write must never have happened");

        let _ = fs::remove_dir_all(&d);
    }

    /// The same hand-built escape, but with `confirmed_overwrite: true` — confirmation must not buy an
    /// escape. It only ever authorises overwriting the user's OWN input in place (CPE-1599/1613); it was
    /// never meant to, and must not, license writing to an arbitrary path outside the selected folder.
    #[test]
    fn ipc_bypass_hand_built_escaping_planned_item_is_refused_even_with_confirmed_overwrite() {
        let d = scratch("ipc-bypass-escape-confirmed");
        let workdir = d.join("a").join("traversal_workdir");
        let victim_dir = d.join("cpe1613_traversal_victim");
        fs::create_dir_all(&workdir).unwrap();
        fs::create_dir_all(&victim_dir).unwrap();
        let input = workdir.join("innocuous.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();
        let victim_target = victim_dir.join("important.jpg");
        let victim_original = b"VICTIM ORIGINAL CONTENT - must not be touched".to_vec();
        fs::write(&victim_target, &victim_original).unwrap(); // a REAL pre-existing file this time

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: victim_target.to_string_lossy().to_string(),
            summary: "hand-built, bypassing plan()".into(),
        }];
        let mut job = BatchJob::new(vec![MediaOp::StripMetadata]);
        job.confirmed_overwrite = true; // the caller-supplied "confirmation" from the finding

        let err = execute_plan(&items, &job).expect_err(
            "confirmed_overwrite must not authorise escaping the input's own folder — only an in-place \
             overwrite of the user's own input",
        );
        assert!(err.to_lowercase().contains("folder"), "refusal reason: {err}");

        // Byte-for-byte proof the victim's real bytes are untouched.
        assert_eq!(
            fs::read(&victim_target).unwrap(),
            victim_original,
            "confirmed_overwrite must never buy an escape — the victim file must be byte-for-byte untouched"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// No new false refusals: an ordinary plan produced BY `plan()` itself (output correctly contained)
    /// must still execute normally through the new pre-write containment re-check.
    #[test]
    fn ipc_bypass_containment_recheck_does_not_disturb_an_ordinary_contained_plan() {
        let d = scratch("ipc-bypass-no-false-alarm");
        let a = d.join("a.png");
        let b = d.join("b.png");
        fs::write(&a, png_bytes(20, 10)).unwrap();
        fs::write(&b, png_bytes(15, 15)).unwrap();

        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 8 }]);
        let inputs = vec![a.to_string_lossy().to_string(), b.to_string_lossy().to_string()];
        let items = plan(&job, &inputs).unwrap();

        let report = execute_plan(&items, &job)
            .unwrap_or_else(|e| panic!("an ordinary contained plan must not be refused: {e}"));
        assert_eq!(report.written, 2);

        let _ = fs::remove_dir_all(&d);
    }

    // ---- Link-as-final-component: the directory-text check alone isn't enough (reviewer, PR #828 -------
    // ---- attempt 3) ---------------------------------------------------------------------------------
    //
    // `output_escapes_input_dir`'s `out_dir == dir` fast path used to return `false` the instant the two
    // directories' TEXT matched — never asking what `output`'s final path component actually IS on disk. A
    // link whose *name* sits inside the input's own directory can alias data physically outside it. Both
    // shapes below were demonstrated on the real filesystem (see the module's link-as-final-component doc
    // paragraph); these are the permanent regression tests, proven by reading bytes off disk, not by
    // trusting a `Result`. **Negative control:** both failed against the pre-fix branch HEAD (verified
    // manually before landing this fix — `escapes == false` for the hard-link case, and the dangling-symlink
    // case sailed through `is_foreign_overwrite` with `confirmed_overwrite` still at its default `false`).

    /// **Hard link (needs no privilege on any platform, any Windows account, same volume).** A hand-built
    /// `PlannedItem` whose `output` is a hard link to a file OUTSIDE the selected folder, even though
    /// `output`'s own directory text is textually identical to `input`'s — the exact shape that fooled the
    /// old fast path. Run WITH `confirmed_overwrite: true` deliberately: this is the finding that falsified
    /// the earlier claim that flag can never license an out-of-folder write.
    #[test]
    fn link_as_final_component_hard_link_alias_is_refused_even_with_confirmed_overwrite() {
        let d = scratch("link-final-hardlink");
        let selected = d.join("selected");
        let outside = d.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("important.jpg");
        let victim_original = b"VICTIM ORIGINAL CONTENT - must not be touched".to_vec();
        fs::write(&victim, &victim_original).unwrap();

        // link.jpg's NAME lives inside `selected` (so its directory TEXT matches `input`'s), but its DATA
        // is the same file as `victim`, which lives OUTSIDE `selected`.
        let link = selected.join("link.jpg");
        crate::links::create_hard_link(&victim.to_string_lossy(), &link.to_string_lossy())
            .expect("hard link creation needs no elevation on any platform");

        let input = selected.join("photo.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: link.to_string_lossy().to_string(),
            summary: "hand-built, aliasing a hard link".into(),
        }];
        let mut job = BatchJob::new(vec![MediaOp::StripMetadata]);
        job.confirmed_overwrite = true; // the finding's own demonstrated bypass

        let err = execute_plan(&items, &job).expect_err(
            "an output whose directory text matches the input's own, but whose data is hard-linked to a \
             file outside it, must be refused — even with confirmed_overwrite",
        );
        assert!(!err.is_empty(), "the refusal must carry a specific reason");

        // Byte-level proof, not a trust-the-Result check: read the victim's ACTUAL bytes back off disk.
        assert_eq!(
            fs::read(&victim).unwrap(),
            victim_original,
            "the hard-linked victim file must be byte-for-byte untouched"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// **Dangling symlink (needs zero batch-job flags).** `Path::is_file()` on a dangling symlink is
    /// `false`, so `is_foreign_overwrite` sees "nothing there yet" and `confirmed_overwrite` never needs to
    /// be set — this must still be refused by the containment re-check alone. Symlink creation needs
    /// Developer Mode / elevation on Windows; this test skips cleanly (not fails) when that's unavailable,
    /// matching `links.rs`'s own test pattern — this dev machine has Developer Mode enabled, CI may not.
    #[test]
    fn link_as_final_component_dangling_symlink_is_refused_with_no_confirmation_needed() {
        let d = scratch("link-final-dangling-symlink");
        let selected = d.join("selected");
        let outside = d.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("newly-planted.jpg"); // deliberately does NOT exist yet
        let link = selected.join("link.jpg");
        if crate::links::create_symlink(&victim.to_string_lossy(), &link.to_string_lossy()).is_err() {
            eprintln!(
                "skipping dangling-symlink containment test: could not create a symlink in this \
                 environment (Windows needs Developer Mode or elevation) — same skip pattern links.rs uses"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        assert!(!victim.exists(), "sanity: the symlink's target must not exist yet — the dangling case");

        let input = selected.join("photo.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: link.to_string_lossy().to_string(),
            summary: "hand-built, aliasing a dangling symlink".into(),
        }];
        let job = BatchJob::new(vec![MediaOp::StripMetadata]); // confirmed_overwrite left at its default false
        assert!(!job.confirmed_overwrite, "sanity: no confirmation given — this must be refused regardless");

        let err = execute_plan(&items, &job).expect_err(
            "a dangling symlink whose name sits inside the selected folder but whose stored target names a \
             path outside it must be refused — no batch-job flag should even be necessary",
        );
        assert!(!err.is_empty(), "the refusal must carry a specific reason");

        // Byte-level proof: the victim path must never have come into existence at all.
        assert!(!victim.exists(), "the escaping write through a dangling symlink must never have happened");

        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1642: identity resolution, end-to-end and byte-proven ----------------------------------
    //
    // The two escapes CPE-1623's shape-matching left open, reproduced here as full `execute_plan` runs and
    // proven by reading the victim's ACTUAL bytes back off disk. **Negative control:** both were run
    // against the pre-fix code first — the chain case returned `Ok(BatchReport { written: 1 })` with the
    // outside victim's bytes changed, and the contended hard-link case likewise; see the ticket's work log.

    /// **Finding A — symlink CHAIN.** `linkA → linkB` (relative, same folder) → `outside/important.jpg`.
    /// Reading one hop saw `linkB` sitting textually inside the selected folder and waved it through.
    /// Run with `confirmed_overwrite: true`, exactly as demonstrated.
    #[test]
    fn cpe_1642_symlink_chain_alias_is_refused_and_the_victim_bytes_are_untouched() {
        let d = scratch("cpe1642-chain");
        let selected = d.join("selected");
        let outside = d.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("important.jpg");
        let victim_original = b"VICTIM ORIGINAL CONTENT - must survive a two-hop chain".to_vec();
        fs::write(&victim, &victim_original).unwrap();

        let link_b = selected.join("linkB.jpg");
        let link_a = selected.join("linkA.jpg");
        if crate::links::create_symlink(&victim.to_string_lossy(), &link_b.to_string_lossy()).is_err() {
            eprintln!(
                "SKIPPING cpe_1642_symlink_chain_alias_is_refused: could not create a symlink here \
                 (Windows needs Developer Mode or elevation) — this test verified NOTHING"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        if crate::links::create_symlink("linkB.jpg", &link_a.to_string_lossy()).is_err() {
            eprintln!("SKIPPING cpe_1642_symlink_chain_alias_is_refused: second hop could not be created");
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let input = selected.join("photo.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: link_a.to_string_lossy().to_string(),
            summary: "hand-built, aliasing a two-hop symlink chain".into(),
        }];
        let mut job = BatchJob::new(vec![MediaOp::StripMetadata]);
        job.confirmed_overwrite = true;

        let err = execute_plan(&items, &job).expect_err(
            "a symlink CHAIN whose far end lands outside the selected folder must be refused",
        );
        assert!(!err.is_empty(), "the refusal must carry a specific reason");
        assert_eq!(
            fs::read(&victim).unwrap(),
            victim_original,
            "the chain's outside victim must be byte-for-byte untouched"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// **Finding B — a contended hard link used to fail OPEN.** Same hard-link alias the CPE-1623 test
    /// above covers, but with an ordinary unprivileged process holding an exclusive handle on it, which
    /// made the old `GENERIC_READ` link-count read fail and default to "one link, nothing to see".
    #[cfg(windows)]
    #[test]
    fn cpe_1642_contended_hard_link_alias_is_refused_and_the_victim_bytes_are_untouched() {
        use std::os::windows::fs::OpenOptionsExt;

        let d = scratch("cpe1642-contended");
        let selected = d.join("selected");
        let outside = d.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("important.jpg");
        let victim_original = b"VICTIM ORIGINAL CONTENT - must survive a contended read".to_vec();
        fs::write(&victim, &victim_original).unwrap();
        let link = selected.join("link.jpg");
        crate::links::create_hard_link(&victim.to_string_lossy(), &link.to_string_lossy())
            .expect("hard link creation needs no elevation on any platform");

        let input = selected.join("photo.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();

        // Any concurrent holder — another process, an AV scanner, a second thread of the same batch.
        let hold = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&link)
            .expect("an exclusive handle needs no privilege");

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: link.to_string_lossy().to_string(),
            summary: "hand-built, aliasing a contended hard link".into(),
        }];
        let mut job = BatchJob::new(vec![MediaOp::StripMetadata]);
        job.confirmed_overwrite = true;

        let err = execute_plan(&items, &job).expect_err(
            "a hard-linked output must stay refused while the file is held exclusively — a failed read \
             must never be reported as \"not linked\"",
        );
        assert!(!err.is_empty(), "the refusal must carry a specific reason");
        drop(hold);
        assert_eq!(
            fs::read(&victim).unwrap(),
            victim_original,
            "the hard-linked victim must be byte-for-byte untouched"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// **REV-G2 (reviewer, PR #840 round 1) — the probe and the WRITER must address the same files.**
    /// Every write in this crate goes through `std::fs`, which applies Windows' `\\?\` verbatim prefix and
    /// therefore reaches paths past `MAX_PATH`. A raw `CreateFileW` does not. When the identity probe was
    /// a raw Win32 call on the unprefixed path, an over-`MAX_PATH` output failed to open with
    /// `ERROR_PATH_NOT_FOUND`, which classified as "nothing is there" — so a planted symlink in a deep
    /// folder was judged contained and the outside victim's bytes really were replaced, a case the
    /// pre-CPE-1642 code refused correctly. Byte-proven end to end.
    ///
    /// Windows-only: `MAX_PATH` is the Windows limit, and Linux/macOS `symlink_metadata` has no such
    /// truncation, so there is nothing to regress there.
    #[cfg(windows)]
    #[test]
    fn cpe_1642_over_max_path_symlink_alias_is_refused_and_the_victim_bytes_are_untouched() {
        let d = scratch("cpe1642-longpath");
        // Pad the selected folder past MAX_PATH (260). `create_dir_all` gets there because std prefixes.
        let mut deep = d.clone();
        while deep.to_string_lossy().chars().count() < 300 {
            deep = deep.join("padpadpadpadpadpadpadpadpadpadpadpadpadpad");
        }
        let selected = deep.join("selected");
        let outside = deep.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();
        assert!(
            selected.to_string_lossy().chars().count() > 260,
            "sanity: the selected folder must sit past MAX_PATH or this test proves nothing (len {})",
            selected.to_string_lossy().chars().count()
        );

        let victim = outside.join("important.jpg");
        let victim_original = b"VICTIM ORIGINAL CONTENT - must survive a >MAX_PATH probe".to_vec();
        fs::write(&victim, &victim_original).unwrap();

        // A RELATIVE target, resolved against the link's own parent exactly as the OS does — which means
        // the probe has to cope with a `..` segment inside an over-MAX_PATH path too.
        let link_a = selected.join("linkA.jpg");
        if crate::links::create_symlink("..\\outside\\important.jpg", &link_a.to_string_lossy()).is_err() {
            eprintln!(
                "SKIPPING cpe_1642_over_max_path_symlink_alias: could not create a symlink here \
                 (Windows needs Developer Mode or elevation) — this test verified NOTHING"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let input = selected.join("photo.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();

        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: link_a.to_string_lossy().to_string(),
            summary: "hand-built, aliasing a symlink inside an over-MAX_PATH folder".into(),
        }];
        let mut job = BatchJob::new(vec![MediaOp::StripMetadata]);
        job.confirmed_overwrite = true;

        let outcome = execute_plan(&items, &job);
        // Byte-level proof FIRST, so a wrong-reason failure can't be mistaken for the right one.
        assert_eq!(
            fs::read(&victim).unwrap(),
            victim_original,
            "the victim behind an over-MAX_PATH symlink must be byte-for-byte untouched"
        );
        let err = outcome.expect_err(
            "a symlink alias inside an over-MAX_PATH folder must be refused — the probe must reach every \
             path the writer reaches",
        );
        assert!(!err.is_empty(), "the refusal must carry a specific reason");

        let _ = fs::remove_dir_all(&d);
    }

    /// **Refusal messages must state what is actually true (CPE-1642).** An output whose identity could
    /// not be established has NOT been shown to leave the folder — saying "would land outside its own
    /// input's folder" about it tells the user something false about their own files. A symlink cycle is
    /// the deterministic unverifiable case: nothing escaped, the chain simply has no real end.
    #[test]
    fn cpe_1642_unverifiable_output_is_refused_without_claiming_it_left_the_folder() {
        let d = scratch("cpe1642-message");
        let selected = d.join("selected");
        fs::create_dir_all(&selected).unwrap();
        let link_a = selected.join("linkA.jpg");
        let link_b = selected.join("linkB.jpg");
        if crate::links::create_symlink("linkB.jpg", &link_a.to_string_lossy()).is_err()
            || crate::links::create_symlink("linkA.jpg", &link_b.to_string_lossy()).is_err()
        {
            eprintln!(
                "SKIPPING cpe_1642_unverifiable_output_message: could not create a symlink here \
                 (Windows needs Developer Mode or elevation) — this test verified NOTHING"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let input = selected.join("photo.jpg");
        fs::write(&input, png_bytes(8, 8)).unwrap();
        let items = vec![PlannedItem {
            input: input.to_string_lossy().to_string(),
            output: link_a.to_string_lossy().to_string(),
            summary: "hand-built, aliasing a cyclic symlink".into(),
        }];

        let err = execute_plan(&items, &BatchJob::new(vec![MediaOp::StripMetadata]))
            .expect_err("a cyclic link chain must be refused, not followed forever");
        assert!(
            err.contains("couldn't be verified"),
            "the refusal must say the output could not be VERIFIED: {err}"
        );
        assert!(
            !err.contains("would land outside"),
            "an unverifiable output must NOT be reported as a proven escape: {err}"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// Perf regression guard for `execute_plan_walk`'s OWN copy of the containment check (mirrors
    /// `batch_media::cpe_1623_containment_check_for_a_directory_changing_rename_stays_bounded`, which only
    /// ever covered `plan()`'s copy — reviewer finding, PR #828 attempt 3's "ALSO" follow-up: there was no
    /// equivalent guard on the execute side, so a regression there wouldn't be caught by CI). A Rename
    /// template of `"./{stem}"` changes `out_dir` TEXTUALLY (adds `"./"`) but resolves to the exact same
    /// real directory, so `execute_plan_walk`'s pre-write containment re-check genuinely runs `path_key`'s
    /// full resolution for every item (the fast path is skipped), not the near-zero-cost common case.
    #[test]
    fn cpe_1623_execute_plan_walk_containment_recheck_stays_bounded() {
        let d = scratch("cpe1623-execute-containment-perf-guard");
        let n = 300usize;
        let inputs: Vec<String> = (0..n)
            .map(|i| {
                let p = d.join(format!("photo{i:04}.jpg"));
                fs::write(&p, png_bytes(4, 4)).unwrap();
                p.to_string_lossy().to_string()
            })
            .collect();

        // "./{stem}-renamed" changes `out_dir` TEXTUALLY (adds "./", resolving to the same real directory)
        // AND genuinely renames the file, so every output is both non-in-place (no `confirmed_overwrite`
        // needed) and non-colliding — isolates the containment re-check's own cost from `is_foreign_overwrite`.
        let job = BatchJob::new(vec![MediaOp::Rename { template: "./{stem}-renamed".into() }]);
        let items = plan(&job, &inputs).unwrap();
        assert_eq!(items.len(), n);

        // Only count canonicalize calls made by execute_plan_walk itself, not plan()'s own (already
        // separately guarded) resolution above.
        crate::batch_media::reset_canonicalize_call_count();
        let report = execute_plan(&items, &job)
            .unwrap_or_else(|e| panic!("a same-directory rename must not be refused: {e}"));
        let calls = crate::batch_media::canonicalize_call_count();
        assert_eq!(report.written, n);

        // Same generous linear bound as plan()'s own guard — O(n) allows a small constant number of
        // canonicalize calls per file (one path_key resolution per item for each of out_dir/dir, both
        // memoized after the first). O(n²) would blow far past this even at n=300.
        assert!(
            calls <= n * 10,
            "expected O(n) canonicalize calls for execute_plan_walk's containment re-check (bound {}), got \
             {calls} for n={n} files — it may have regressed to a per-item uncached resolution",
            n * 10
        );

        let _ = fs::remove_dir_all(&d);
    }
}
