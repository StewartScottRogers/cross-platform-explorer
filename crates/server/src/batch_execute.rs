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
//! **TOCTOU: both guards are re-asked immediately before each write (CPE-1624).** They used to run once
//! per batch, before any bytes were touched — so for item N they were an answer about a filesystem that no
//! longer existed. Measured: `same_file(link.jpg -> A, A)` is `true`, and after re-pointing the symlink at
//! `B` a repeat check correctly returns `false`; nothing re-validated at write time, and the window grows
//! with batch size and with slow per-file ops (watermark, compress). [`execute_plan_walk`] now calls
//! [`write_time_guard`] before every single `execute_one`, re-deriving containment **and** the
//! foreign-overwrite question per item. A late detection **skips that item with a reported reason** rather
//! than writing it (and rather than aborting a batch that has already written files) — the up-front scan
//! keeps its all-or-nothing refusal for a plan that arrives wrong.

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
    let mut rejected: Vec<&'static str> = Vec::new();
    let mut unverifiable: Vec<&'static str> = Vec::new();
    for it in items {
        match classify_output_containment(&it.input, &it.output, &mut parent_cache) {
            Containment::Inside => {}
            Containment::Escapes => escaping += 1,
            Containment::Refused(why) => rejected.push(why),
            Containment::Unverifiable(why) => unverifiable.push(why),
        }
    }
    if escaping > 0 || !rejected.is_empty() || !unverifiable.is_empty() {
        let mut reasons: Vec<String> = Vec::new();
        if escaping > 0 {
            reasons.push(format!(
                "{escaping} planned output{} would land outside its own input's folder",
                if escaping == 1 { "" } else { "s" }
            ));
        }
        if !rejected.is_empty() {
            reasons.push(format!(
                "{} planned output{} {}",
                rejected.len(),
                if rejected.len() == 1 { "" } else { "s" },
                rejected[0]
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
        // CPE-1624 finding A: re-ask both questions immediately before THIS item's write, not once for
        // the whole batch (see [`write_time_guard`]).
        if let Err(reason) = write_time_guard(item, items, job, &mut parent_cache) {
            flush(OpResult::err(Path::new(&item.input), &reason));
            report.skipped.push((item.input.clone(), reason));
            continue;
        }
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

/// **The write-time re-check (CPE-1624 finding A).** Both guards above run once, before the loop — so
/// every one of them is a *stale* answer by the time item N is actually written. Measured on a real
/// filesystem: `same_file(link.jpg -> A, A)` is `true`, and after swapping the symlink's target to `B` a
/// repeat check correctly returns `false` — i.e. the pre-batch answer does not describe what the write
/// will hit. The window between the up-front scan and a given item's write grows with batch size and with
/// slow per-file operations (watermark, compress), and closing it costs one extra identity probe per item.
///
/// Both questions are re-asked, because both can change underneath the batch:
/// 1. **Containment** — a name inside the folder can be turned into a link out of it (or a link's target
///    re-pointed) after the scan.
/// 2. **Foreign overwrite** — a file that did not exist at scan time can appear at a planned output
///    between the scan and the write, at which point writing it is exactly the unconfirmed clobber
///    `confirmed_overwrite` exists to prevent.
///
/// **A late detection skips the ITEM, it does not fail the batch.** The up-front scan keeps its
/// all-or-nothing refusal: a plan that is already wrong when it arrives should be rejected whole, before a
/// single byte moves. But once the batch is running, files have been written; aborting mid-way would leave
/// a half-applied batch with no report, and the module's whole ethos is skip-on-error with a reported
/// reason (`list_dir`/`revert_engine`). The item is recorded in `BatchReport::skipped` with a reason
/// naming the late change, and the surviving items still run.
///
/// **Cost, measured against a control.** One [`classify_output_containment`] (which re-probes `output`'s
/// identity — [`ParentCache`] deliberately memoizes only *directory* facts, so this is a genuine
/// re-resolution, not a replay) plus [`is_foreign_overwrite`]'s `same_file` + one `Path::is_file()` stat.
/// The deterministic measure is the `canonicalize` count, and it is an exact 2× by construction — the
/// same two questions, asked a second time: **5 calls per item before this fix, 10 after**, on the
/// worst-case shape `cpe_1623_execute_plan_walk_containment_recheck_stays_bounded` builds (every fast
/// path defeated). In wall-clock terms, on 1000 tiny 16×16 PNGs in a release build, the whole walk
/// measured 536–569 µs/item with the guard against 543–563 µs/item without it — i.e. **inside
/// run-to-run noise**, because even a trivial decode/encode dwarfs a handful of attribute reads. On real
/// photographs the ratio is far smaller still. The guard measured in isolation, cold, is ~85–112 µs/item.
fn write_time_guard(
    item: &PlannedItem,
    items: &[PlannedItem],
    job: &BatchJob,
    parent_cache: &mut ParentCache,
) -> Result<(), String> {
    match classify_output_containment(&item.input, &item.output, parent_cache) {
        Containment::Inside => {}
        Containment::Escapes => {
            return Err(format!(
                "skipped at write time: \"{}\" no longer stays inside this file's own folder — it \
                 changed after the batch's up-front check (a link re-pointed, or a name replaced). \
                 Nothing was written for this file",
                item.output
            ));
        }
        Containment::Refused(why) => {
            return Err(format!(
                "skipped at write time: \"{}\" {why}. Nothing was written for this file",
                item.output
            ));
        }
        Containment::Unverifiable(why) => {
            return Err(format!(
                "skipped at write time: couldn't verify that \"{}\" still stays inside this file's own \
                 folder — {why}. Nothing was written for this file; this is a refusal to guess, not a \
                 detected escape",
                item.output
            ));
        }
    }

    if !job.confirmed_overwrite && is_foreign_overwrite(item, items) {
        return Err(format!(
            "skipped at write time: \"{}\" would now overwrite a file this batch never selected — it \
             appeared after the batch's up-front check, and `confirmed_overwrite` was not set. Nothing \
             was written for this file",
            item.output
        ));
    }
    Ok(())
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

        // O(n) allows a small constant number of canonicalize calls per file (one `path_key` resolution
        // per item for each of out_dir/dir, both memoized after the first). O(n²) would blow far past
        // this even at n=300.
        //
        // **Raised from 10 to 14 by CPE-1624**, deliberately and by exactly the amount the fix costs:
        // the write-time re-check asks the same two questions a second time, immediately before each
        // write, so the per-item count doubles by design. Measured on this worst-case shape (the `"./"`
        // template defeats every fast path): **5 calls/item before, 10 after** — an exact 2×, not a
        // creeping regression. The headroom above 10 is for platform variation in how many tiers
        // `path_key` has to fall through, not for a future doubling; a regression to per-item uncached
        // resolution would still be orders of magnitude past it.
        assert!(
            calls <= n * 14,
            "expected O(n) canonicalize calls for execute_plan_walk's containment re-check (bound {}), got \
             {calls} for n={n} files — it may have regressed to a per-item uncached resolution",
            n * 14
        );
        println!("execute_plan_walk canonicalize calls for n={n}: {calls} ({} per item)", calls / n);

        let _ = fs::remove_dir_all(&d);
    }

    /// Manual-only measurement of the CPE-1624 write-time re-check's per-item overhead — the ticket asks
    /// for a stated number, and this is where it comes from. `#[ignore]`d like the other timing tests
    /// (wall-clock assertions are flaky in CI); run with
    /// `cargo test --release -- --ignored --nocapture cpe_1624_write_time_recheck_overhead`.
    ///
    /// Times the guard itself in isolation as well as the whole walk, so the number is not swamped by the
    /// image transform each item also pays. The **control** is the same run with the guard neutralised
    /// (see the PR's guard-neutralisation table); this test prints both the total and the per-item cost so
    /// the two runs are directly comparable.
    #[test]
    #[ignore]
    fn cpe_1624_write_time_recheck_overhead() {
        let d = scratch("cpe1624-overhead");
        let n = 1000usize;
        let inputs: Vec<String> = (0..n)
            .map(|i| {
                let p = d.join(format!("photo{i:04}.png"));
                fs::write(&p, png_bytes(16, 16)).unwrap();
                p.to_string_lossy().to_string()
            })
            .collect();
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 8 }]);
        let items = plan(&job, &inputs).unwrap();

        // The guard alone, over one shared cache — exactly how execute_plan_walk calls it.
        let mut cache = ParentCache::new();
        let start = std::time::Instant::now();
        for item in &items {
            write_time_guard(item, &items, &job, &mut cache).unwrap();
        }
        let guard_only = start.elapsed();

        let start = std::time::Instant::now();
        let report = execute_plan(&items, &job).unwrap();
        let whole_walk = start.elapsed();
        assert_eq!(report.written, n);

        println!(
            "CPE-1624 write-time re-check over n={n}: guard alone {guard_only:?} ({:?}/item); whole \
             execute_plan_walk {whole_walk:?} ({:?}/item)",
            guard_only / n as u32,
            whole_walk / n as u32
        );

        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1624 finding A: the guards are re-asked immediately before each write -------------------

    /// **The TOCTOU, demonstrated without needing any link privilege.** Both guards used to run once,
    /// before the loop; a file that appears at a planned output *after* that scan but before its own
    /// item's write was therefore clobbered with no confirmation, because the only question ever asked
    /// about it was asked while it did not exist.
    ///
    /// The `flush` callback is the seam that makes the race deterministic: it fires after item 0 is
    /// written and before item 1 is, which is exactly the window. **Red on base `main`:** `written == 2`
    /// and the foreign file's bytes are replaced by PNG data. Green here: item 1 is skipped with a
    /// reason, the foreign file is byte-for-byte intact, and item 0 still succeeds (a late detection
    /// skips the ITEM, it does not abort a batch that has already written files).
    #[test]
    fn cpe_1624_a_file_appearing_at_a_planned_output_mid_batch_is_skipped_not_clobbered() {
        let d = scratch("cpe1624-toctou-appearing-file");
        let a = d.join("a.png");
        let b = d.join("b.png");
        fs::write(&a, png_bytes(32, 32)).unwrap();
        fs::write(&b, png_bytes(32, 32)).unwrap();

        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        let inputs = vec![a.to_string_lossy().to_string(), b.to_string_lossy().to_string()];
        let items = plan(&job, &inputs).unwrap();
        assert_eq!(items.len(), 2);
        let victim_path = items[1].output.clone();
        assert!(!Path::new(&victim_path).exists(), "the second output must be free when the batch starts");

        // Plant a stranger's file at item 1's planned output the instant item 0 finishes — after the
        // up-front scan said "nothing is there", and before item 1's own write.
        let victim_bytes = b"a stranger's file, planted mid-batch".to_vec();
        let planted = std::cell::Cell::new(false);
        let report = execute_plan_walk(&items, &job, |_| {
            if !planted.get() {
                planted.set(true);
                fs::write(&victim_path, &victim_bytes).unwrap();
            }
        })
        .expect("a late collision must not fail the whole batch — the first file was already written");

        assert!(planted.get(), "the test never planted the file, so it verified nothing");
        assert_eq!(report.written, 1, "only the first item may be written");
        assert_eq!(report.skipped.len(), 1, "the second item must be skipped, not written");
        assert_eq!(report.skipped[0].0, items[1].input);
        assert!(
            report.skipped[0].1.contains("write time"),
            "the skip reason must say this was caught at write time: {}",
            report.skipped[0].1
        );
        assert_eq!(
            fs::read(&victim_path).unwrap(),
            victim_bytes,
            "the planted file's bytes must be untouched — this is the clobber the re-check prevents"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// The ticket's own worded scenario: a link whose target is swapped **mid-batch**, so a check made
    /// before the swap is stale for what actually gets written afterwards. Same `flush` seam. Skips
    /// (rather than aborts) and leaves the outside victim's bytes untouched.
    ///
    /// Skipped with a visible message where symlinks can't be created (Windows without Developer Mode),
    /// exactly like this module's other link tests — never a silent pass.
    #[test]
    fn cpe_1624_a_link_repointed_mid_batch_is_caught_at_write_time() {
        let d = scratch("cpe1624-toctou-link-swap");
        let selected = d.join("selected");
        let outside = d.join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let a = selected.join("a.png");
        let b = selected.join("b.png");
        fs::write(&a, png_bytes(32, 32)).unwrap();
        fs::write(&b, png_bytes(32, 32)).unwrap();
        let inside_target = selected.join("inside-target.png");
        fs::write(&inside_target, png_bytes(8, 8)).unwrap();
        let victim = outside.join("important.png");
        let victim_bytes = b"the outside victim's original bytes".to_vec();
        fs::write(&victim, &victim_bytes).unwrap();

        // b's planned output is a symlink that, at scan time, points INSIDE the selected folder — so the
        // up-front containment check correctly says "contained".
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        let items = plan(&job, &[a.to_string_lossy().to_string(), b.to_string_lossy().to_string()]).unwrap();
        let link_path = PathBuf::from(&items[1].output);
        if crate::links::create_symlink(&inside_target.to_string_lossy(), &link_path.to_string_lossy())
            .is_err()
        {
            eprintln!(
                "SKIPPING cpe_1624_a_link_repointed_mid_batch_is_caught_at_write_time: could not create \
                 a symlink here (Windows needs Developer Mode or elevation) — this test verified NOTHING"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        // The batch is allowed to write onto a name it planned, so confirm the overwrite — this isolates
        // the containment re-check from the foreign-overwrite guard.
        let mut job = job;
        job.confirmed_overwrite = true;
        let swapped = std::cell::Cell::new(false);
        let report = execute_plan_walk(&items, &job, |_| {
            if !swapped.get() {
                swapped.set(true);
                fs::remove_file(&link_path).unwrap();
                crate::links::create_symlink(&victim.to_string_lossy(), &link_path.to_string_lossy())
                    .unwrap();
            }
        })
        .expect("a mid-batch swap must skip the item, not fail the whole batch");

        assert!(swapped.get(), "the test never swapped the link, so it verified nothing");
        assert_eq!(report.written, 1, "only the first item may be written");
        assert_eq!(report.skipped.len(), 1);
        assert!(
            report.skipped[0].1.contains("write time"),
            "the skip reason must say this was caught at write time: {}",
            report.skipped[0].1
        );
        assert_eq!(
            fs::read(&victim).unwrap(),
            victim_bytes,
            "the outside victim's bytes must be untouched — this is the write the re-check prevents"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// The negative control for both tests above: with nothing changing underneath it, an ordinary batch
    /// must run to completion. A re-check that refused everything would pass the two tests above and be
    /// useless.
    #[test]
    fn cpe_1624_the_write_time_recheck_does_not_disturb_an_ordinary_batch() {
        let d = scratch("cpe1624-negative-control");
        let inputs: Vec<String> = (0..8)
            .map(|i| {
                let p = d.join(format!("photo{i}.png"));
                fs::write(&p, png_bytes(24, 24)).unwrap();
                p.to_string_lossy().to_string()
            })
            .collect();
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 8 }]);
        let items = plan(&job, &inputs).unwrap();

        let report = execute_plan(&items, &job).expect("an ordinary batch must not be refused");
        assert_eq!(report.written, 8, "every item must still be written: {:?}", report.skipped);
        assert!(report.skipped.is_empty(), "no false alarms: {:?}", report.skipped);

        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1624 finding B, end to end through the real engine entry point.** The audit's PoC:
    /// `plan()` computed `…\workdir\C:foo.png` — same directory, contained — and `execute_plan` returned
    /// `Ok(written: 1)`, with 120 bytes of transformed PNG readable at the `foo.png` stream of the
    /// unrelated file `C`, whose visible size and content never changed. `Path::is_file()` on a
    /// never-before-existing stream returns `false`, so no confirmation was ever required.
    ///
    /// Hand-built `PlannedItem` on purpose: the template-level colon rule cannot see this path, which is
    /// the whole reason the refusal had to move to the shared engine boundary. **Red on base `main`:**
    /// `Ok(written: 1)` and readable bytes at the stream. Windows-only — an alternate data stream is an
    /// NTFS concept, and off Windows this same path is an ordinary (legal) filename.
    #[cfg(windows)]
    #[test]
    fn cpe_1624_a_hand_built_alternate_data_stream_output_is_refused_and_writes_nothing() {
        let d = scratch("cpe1624-ads-engine-boundary");
        let photo = d.join("photo.png");
        fs::write(&photo, png_bytes(32, 32)).unwrap();
        // The unrelated, never-selected file whose hidden stream the write would land on.
        let host = d.join("C");
        let host_bytes = b"an unrelated file the user can see".to_vec();
        fs::write(&host, &host_bytes).unwrap();

        let stream_path = format!("{}:foo.png", host.to_string_lossy());
        let items = vec![PlannedItem {
            input: photo.to_string_lossy().to_string(),
            output: stream_path.clone(),
            summary: "hand-built, targeting an NTFS alternate data stream".into(),
        }];

        for confirmed in [false, true] {
            let mut job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
            job.confirmed_overwrite = confirmed;
            let err = execute_plan(&items, &job).expect_err(
                "an alternate-data-stream output must be refused — confirmed_overwrite authorises \
                 overwriting a FILE, never writing hidden bytes onto an unrelated one",
            );
            assert!(
                err.contains("alternate data stream"),
                "the refusal must name the real reason: {err}"
            );
            assert!(
                !err.contains("would land outside"),
                "an ADS output does NOT leave the folder — saying so would be untrue: {err}"
            );
        }

        assert!(fs::read(&stream_path).is_err(), "no bytes may exist at the stream path");
        assert_eq!(fs::read(&host).unwrap(), host_bytes, "the host file must be untouched");

        let _ = fs::remove_dir_all(&d);
    }

    /// **Pins the one direction in which CPE-1624's ADS-aware identity could *relax* a rule.** Making
    /// `X.png:evil` and `X.png` compare as the same file is conservative everywhere except
    /// [`is_foreign_overwrite`]'s "the output is one of this batch's OWN inputs, so it's permitted" arm —
    /// where fusing them would license the hidden write with no confirmation at all. It is unreachable
    /// because the containment refusal is co-gated with the stripping and runs first; this test is what
    /// keeps that true if either gate is ever moved. See `strip_stream_suffix`'s reach audit.
    #[cfg(windows)]
    #[test]
    fn cpe_1624_an_alternate_stream_of_the_batchs_own_input_is_still_refused() {
        let d = scratch("cpe1624-ads-of-own-input");
        let photo = d.join("photo.png");
        fs::write(&photo, png_bytes(32, 32)).unwrap();
        let photo_bytes = fs::read(&photo).unwrap();

        // The stream's host file IS this batch's own (only) input — the case the "permitted" arm covers.
        let items = vec![PlannedItem {
            input: photo.to_string_lossy().to_string(),
            output: format!("{}:evil.png", photo.to_string_lossy()),
            summary: "hand-built, ADS of the batch's own input".into(),
        }];
        let err = execute_plan(&items, &BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]))
            .expect_err("an ADS output must be refused even when its host is one of the batch's inputs");
        assert!(err.contains("alternate data stream"), "refusal reason: {err}");
        assert!(fs::read(&items[0].output).is_err(), "no bytes may exist at the stream path");
        assert_eq!(fs::read(&photo).unwrap(), photo_bytes, "the input must be untouched");

        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1652 finding B through the real engine entry point: past the census cap the verdict degrades
    /// to `Unverifiable`, and `execute_plan_walk` refuses — **fail closed, not fail slow, and never
    /// "allowed because we stopped looking"**. The uncapped run in the same test is the positive control.
    #[test]
    fn cpe_1652_a_census_past_the_cap_refuses_the_write_rather_than_allowing_it() {
        let d = scratch("cpe1652-census-cap-engine");
        let photo = d.join("photo.png");
        fs::write(&photo, png_bytes(32, 32)).unwrap();
        let a = d.join("shared.png");
        fs::write(&a, png_bytes(8, 8)).unwrap();
        let b = d.join("shared-2.png");
        if fs::hard_link(&a, &b).is_err() {
            eprintln!(
                "SKIPPING cpe_1652_a_census_past_the_cap_refuses_the_write: no hard-link support here \
                 — this test verified NOTHING"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        for i in 0..6 {
            fs::write(d.join(format!("filler{i}.png")), b"filler").unwrap();
        }

        let items = vec![PlannedItem {
            input: photo.to_string_lossy().to_string(),
            output: b.to_string_lossy().to_string(),
            summary: "writes onto a multiply-linked name inside the folder".into(),
        }];
        let mut job = BatchJob::new(vec![MediaOp::Resize { max_px: 16 }]);
        job.confirmed_overwrite = true; // the overwrite itself is authorised; only the census is at issue

        // Positive control: uncapped, every name is accounted for inside the folder, so this is allowed.
        crate::batch_media::set_census_cap_for_test(None);
        let ok = execute_plan(&items, &job);
        assert!(ok.is_ok(), "an uncapped census must still allow a wholly-inside hard link: {ok:?}");

        // Capped below the folder's entry count: refuse rather than guess.
        crate::batch_media::set_census_cap_for_test(Some(1));
        let err = execute_plan(&items, &job);
        crate::batch_media::set_census_cap_for_test(None);
        let err = err.expect_err("past the cap the engine must refuse, not write");
        assert!(
            err.contains("too many entries"),
            "the refusal must say the folder was too big to account for, not blame a lock: {err}"
        );

        let _ = fs::remove_dir_all(&d);
    }
}
