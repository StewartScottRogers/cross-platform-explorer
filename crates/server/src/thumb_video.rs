//! Video representative-frame thumbnail extraction (CPE-1257, epic CPE-718): grab one frame from a
//! `.mp4`/`.mov`/`.mkv`/… file and decode it to an `image::DynamicImage` at (approximately) `max_edge`
//! pixels on its longest side, so the thumbnail grid shows an actual frame instead of a generic icon.
//! Integrates into the same [`crate::thumb_source::decode_thumb_image`] dispatch the PSD/SVG/font/PDF
//! paths use — see that module for the shared orient+downscale+encode pipeline this plugs into.
//!
//! Feature-gated OFF by default (`video-thumb`; dependency approach decided in the research-library
//! entry `thumbnail-native-deps-pdf-video-2026-08-02.md`, Foreman decide-and-log 2026-08-02): unlike
//! the PDF path (`thumb_pdf`, `pdfium-render` linked in-process), video decoding here **shells out to a
//! bundled `ffmpeg` executable** via [`std::process::Command`] rather than linking a video-decode crate
//! (`ffmpeg-next`/`ffmpeg-sys`/`video-rs`/GStreamer all link ffmpeg in-process, which drags its
//! LGPL/GPL license onto the whole signed binary and is a brutal cross-platform CI/link surface).
//! Keeping ffmpeg a **separate bundled program** invoked as a subprocess is mere aggregation: its
//! license never attaches to our binary. This crate therefore adds **zero new Cargo dependency** for
//! this feature — `[features] video-thumb = []` has no `dep:` at all — so the plain build (feature off)
//! compiles exactly zero video code (the "small when off" rule, mirrored from `thumb_pdf`/`index`).
//!
//! **Never slurps the video into memory.** Unlike the PSD/SVG/PDF arms in `thumb_source`, which run
//! *after* the whole source file has already been read into `bytes`, the video dispatch happens EARLY —
//! before that `fs::read` — because a video can be many gigabytes. This module always operates on the
//! file `Path` directly; ffmpeg reads it itself as a subprocess.
//!
//! **ffmpeg resolution + scratch-dir plumbing now lives in [`crate::ffmpeg_util`]** (CPE-1478 extraction):
//! [`crate::ffmpeg_util::resolve_ffmpeg_bin`] tries the injected native-dep dir first, then a bundled
//! copy sitting next to the running executable, then a bare `ffmpeg` resolved via `PATH` as the dev
//! fallback (mirrors `thumb_pdf::resolve_bindings`'s resolution order). If none are present, spawning
//! the process itself fails with a normal `Err` — never a panic — and the caller's existing contract (no
//! thumbnail -> generic type icon) takes over. See that module's doc for the full CPE-1258
//! ("next to the executable" isn't enough on macOS/Linux) and CPE-1261 (exclusive scratch dir, CWE-377)
//! rationale — this module is now just ffmpeg_util's first caller, with `media_waveform` (CPE-1478) as
//! the second, sharing one `NATIVE_DEP_DIR` injection so the app only wires the bundled ffmpeg's
//! resource dir in once for both. Since CPE-1480 the two also share `ffmpeg_util`'s
//! `reject_unsafe_ffmpeg_input` pre-spawn guard + `-protocol_whitelist file,pipe` (the SSRF/protocol-
//! injection hardening `media_waveform` got first under CPE-1478) so there's one hardened input path,
//! not two independently-maintained copies.
//!
//! **Frame selection:** seeks ~1s into the clip first (`-ss 1`) to skip a likely black lead-in frame;
//! very short or unusual clips where that offset yields no frame fall back to `-ss 0` (the very first
//! frame). Computing an exact 10%-of-duration offset would need `ffprobe` (or ffmpeg's own duration
//! probing) as a second dependency surface — the ticket explicitly prefers keeping this to the ONE
//! ffmpeg binary, so a fixed small offset + a zero-offset retry is used instead.
//!
//! **Output path + cleanup (CPE-1261 hardening, CWE-377):** rather than writing ffmpeg's output PNG
//! directly to a uniquely-named-but-*predictable* path under the shared OS temp dir (which, on a
//! world-writable `/tmp`, an attacker could pre-plant a symlink at — ffmpeg's `-y` follows symlinks and
//! truncates the target), each call first creates a fresh **exclusively-owned scratch directory** via
//! [`create_scratch_dir`] using `std::fs::create_dir`, which fails atomically if the path already
//! exists (a pre-planted dir or symlink included). A successful create therefore proves the directory
//! is brand-new and attacker-uncontrolled; ffmpeg's single PNG frame is written *inside* it. The whole
//! scratch dir is removed with `remove_dir_all` on both the success and every error path, so a failed
//! render never leaks a temp file or directory. Windows/macOS use per-user temp dirs unaffected by this
//! class of attack, but the hardening applies uniformly on every platform.
//!
//! **Sizing:** ffmpeg's own `scale` filter does the bulk of the downscale work (cheap: ffmpeg decodes
//! at reduced resolution rather than us decoding a full-size frame into memory), capping width to
//! `max_edge`. Because that alone doesn't correctly cap the longest edge for a portrait (taller-than-wide)
//! clip, a final exact `image`-crate downscale ([`downscale_to_max_edge`]) is applied after decode —
//! never upscales, and lands the longest edge at exactly `max_edge` regardless of orientation.

use std::fs;
use std::path::Path;
use std::process::Command;

use image::DynamicImage;

use crate::ffmpeg_util::{
    create_scratch_dir, reject_unsafe_ffmpeg_input, resolve_ffmpeg_bin, FFMPEG_PROTOCOL_WHITELIST,
};

/// Extensions this module claims in [`crate::thumb_source::decode_thumb_image`]'s dispatch. Kept here
/// (rather than duplicated in `thumb_source`) so the two stay in lockstep by construction. Matched
/// against the already-lowercased extension `crate::model::extension_of` returns.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "mkv", "webm", "avi", "m4v", "mpg", "mpeg", "wmv", "flv",
];

/// Extracts one representative frame from the video at `path`, decoded to a [`DynamicImage`] whose
/// longest edge is at most `max_edge` pixels (aspect preserved, never upscaled past the source's own
/// size). Never panics: a missing/unresolvable ffmpeg binary, a non-zero ffmpeg exit, a missing output
/// file, or an undecodable frame all return `Err` — the caller's existing "no thumbnail -> generic type
/// icon" fallback handles it. The video file itself is never read into memory by this crate; ffmpeg
/// reads it directly from `path` as a subprocess.
pub fn extract_frame(path: &Path, max_edge: u32) -> Result<DynamicImage, String> {
    extract_frame_with_ffmpeg(path, max_edge, &resolve_ffmpeg_bin())
}

/// The real implementation, taking the ffmpeg binary path as a parameter so tests can point it at a
/// bogus path (to exercise the "ffmpeg missing" `Err` path) without needing to tamper with `PATH` or
/// the current executable's directory.
fn extract_frame_with_ffmpeg(path: &Path, max_edge: u32, ffmpeg: &Path) -> Result<DynamicImage, String> {
    let edge = max_edge.max(1);

    // Defense-in-depth on this IPC-reachable subprocess boundary (CPE-1480, mirroring CPE-1478's
    // `media_waveform` hardening): reject anything that isn't an existing regular local file BEFORE we
    // hand the string to ffmpeg. See `reject_unsafe_ffmpeg_input`'s doc for the full rationale.
    // `-protocol_whitelist` on the spawned command below is the load-bearing guard at the ffmpeg layer;
    // this is the cheaper first line that also yields a clearer error and skips creating a scratch dir
    // for a request that's doomed anyway.
    reject_unsafe_ffmpeg_input(path)?;

    // Exclusively-created scratch dir (CPE-1261, CWE-377 hardening) — see the module doc's "Output
    // path + cleanup" note. If we can't even get a fresh directory, bail out cleanly; there's nothing
    // to clean up yet.
    let scratch_dir = create_scratch_dir("thumbvideo")?;
    let tmp = scratch_dir.join("frame.png");

    // Seek ~1s in first (skips a likely black lead-in frame); fall back to the very first frame if
    // that offset produced nothing (e.g. a clip shorter than 1s, or an unusual/variable-framerate file).
    let mut result = run_ffmpeg_frame(ffmpeg, path, &tmp, "1", edge);
    if result.is_err() {
        let _ = fs::remove_file(&tmp); // clear any partial output before retrying
        result = run_ffmpeg_frame(ffmpeg, path, &tmp, "0", edge);
    }

    let decoded = result.and_then(|_| {
        image::open(&tmp).map_err(|e| format!("failed to decode ffmpeg's extracted frame: {e}"))
    });

    // Clean up the WHOLE scratch dir (not just the PNG) — on both the success and the error path, so a
    // failed render never leaks the temp file or its owning directory.
    let _ = fs::remove_dir_all(&scratch_dir);

    let img = decoded?;
    Ok(downscale_to_max_edge(img, edge))
}

/// Runs ffmpeg once, seeking to `offset_secs` and writing a single scaled PNG frame to `out`. `Err` on
/// a failed spawn (binary missing/unresolvable), a non-zero exit, or a missing output file — never
/// panics.
fn run_ffmpeg_frame(ffmpeg: &Path, input: &Path, out: &Path, offset_secs: &str, max_edge: u32) -> Result<(), String> {
    // Fit the frame INSIDE a `max_edge`×`max_edge` box, preserving aspect and only ever shrinking
    // (`force_original_aspect_ratio=decrease`), so ffmpeg itself caps the *longest* edge — the final
    // Rust `downscale_to_max_edge` then just double-checks it. Each dimension is floored at 2px
    // (`max(2\,…)`): the old `…:-2` height expression rounds a tiny target height down to 0 for a very
    // small `max_edge` (e.g. width clamped to 1 on a 320×240 clip → height 0), which makes ffmpeg's
    // swscaler reject the filter with `dsth out of range [1 - …]` and produce no frame (CPE-1268). A
    // 2px floor is always valid; the exact `max_edge` cap is enforced afterwards in Rust. The commas
    // inside the `min(…)`/`max(…)` expressions must be backslash-escaped so ffmpeg's filtergraph parser
    // doesn't read them as filter separators.
    let scale = format!(
        "scale=w='max(2\\,min({max_edge}\\,iw))':h='max(2\\,min({max_edge}\\,ih))':force_original_aspect_ratio=decrease"
    );

    let output = Command::new(ffmpeg)
        .arg("-y")
        .arg("-ss")
        .arg(offset_secs)
        // Restrict ffmpeg to the file+pipe protocols so a crafted `input` can't make it reach the
        // network (`http:`/`tcp:` → SSRF) or read via a non-file protocol (`concat:`/`subfile:`/`data:`)
        // — belt-and-braces alongside the pre-spawn `reject_unsafe_ffmpeg_input` check in
        // `extract_frame_with_ffmpeg` above (CPE-1480, mirrors `media_waveform`'s CPE-1478 hardening).
        .arg("-protocol_whitelist")
        .arg(FFMPEG_PROTOCOL_WHITELIST)
        .arg("-i")
        .arg(input)
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg(&scale)
        .arg("-f")
        .arg("image2")
        .arg(out)
        .output()
        .map_err(|e| format!("failed to launch ffmpeg ({}): {e}", ffmpeg.display()))?;

    if !output.status.success() {
        return Err(format!(
            "ffmpeg exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    classify_ffmpeg_output(out, out.try_exists())
}

/// Turn the post-run existence check on ffmpeg's output file into this function's verdict (CPE-1696).
///
/// The check was `if !out.exists() { Err("ffmpeg reported success but produced no output file") }`, and
/// `Path::exists()` folds every `stat` failure into `false` — so a permission-denied or otherwise
/// unreadable scratch path was reported as *ffmpeg having produced nothing*, sending the reader after an
/// ffmpeg bug that isn't there. Split out from `run_ffmpeg_frame` so the taxonomy is testable without a
/// real ffmpeg binary or a real permission denial (both are environment-dependent — see
/// `crate::dispatch::classify_path_error`'s rationale).
fn classify_ffmpeg_output(out: &Path, stat: std::io::Result<bool>) -> Result<(), String> {
    match stat {
        Ok(true) => Ok(()),
        Ok(false) => Err("ffmpeg reported success but produced no output file".to_string()),
        // `try_exists` already folds a genuine `NotFound` into `Ok(false)`; be explicit anyway.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err("ffmpeg reported success but produced no output file".to_string())
        }
        Err(e) => Err(format!(
            "ffmpeg reported success but its output file at {} could not be confirmed: {e}",
            out.display()
        )),
    }
}

/// Downscales `img` so its longest edge is at most `max_edge` pixels, preserving aspect ratio and
/// **never upscaling** past the source's own size. ffmpeg's own `-vf scale` already does most of the
/// work, but only caps *width* (see `run_ffmpeg_frame`'s comment), so a portrait (taller-than-wide)
/// clip could still come back with a height past `max_edge`; this final pass makes the longest-edge cap
/// exact regardless of orientation, mirroring `thumb_pdf::render_first_page`'s scale-to-fit math.
fn downscale_to_max_edge(img: DynamicImage, max_edge: u32) -> DynamicImage {
    let (w, h) = (img.width().max(1), img.height().max(1));
    let longest = w.max(h);
    if longest <= max_edge {
        return img; // already within budget — never upscale
    }
    let scale = max_edge as f32 / longest as f32;
    let out_w = ((w as f32) * scale).round().max(1.0) as u32;
    let out_h = ((h as f32) * scale).round().max(1.0) as u32;
    img.resize(out_w, out_h, image::imageops::FilterType::Lanczos3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg_util::{ffmpeg_available, last_scratch_dir_for_test};

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-thumbvideo-test-{tag}"))
    }

    /// Unconditional — no ffmpeg install needed: pointing the resolver at a path that can't possibly
    /// exist must fail cleanly (spawn error), never panic.
    #[test]
    fn extract_frame_with_missing_ffmpeg_binary_errs_without_panicking() {
        let d = scratch("missing-ffmpeg");
        let bogus_input = d.join("clip.mp4");
        fs::write(&bogus_input, b"irrelevant").unwrap();
        let bogus_ffmpeg = d.join("definitely-not-a-real-ffmpeg-binary-xyz123");

        let err = extract_frame_with_ffmpeg(&bogus_input, 64, &bogus_ffmpeg);
        assert!(err.is_err(), "a nonexistent ffmpeg binary must be reported as Err, not panic");

        let _ = fs::remove_dir_all(&d);
    }

    /// Unconditional: garbage (non-video) bytes handed to a REAL ffmpeg resolution must fail cleanly —
    /// if ffmpeg is installed it will refuse to decode the file (non-zero exit -> Err); if ffmpeg is
    /// NOT installed, resolution itself fails at spawn time (also Err). Either way, no panic.
    #[test]
    fn extract_frame_rejects_an_undecodable_file_without_panicking() {
        let d = scratch("garbage");
        let f = d.join("not_a_video.mp4");
        fs::write(&f, b"this is not a video file, just garbage bytes").unwrap();

        let err = extract_frame(&f, 64);
        assert!(err.is_err(), "an undecodable/garbage file must be rejected, not panic");

        let _ = fs::remove_dir_all(&d);
    }

    /// Unconditional: a nonexistent input path must fail cleanly rather than panic, regardless of
    /// whether ffmpeg itself is available.
    #[test]
    fn extract_frame_rejects_a_nonexistent_input_path_without_panicking() {
        let err = extract_frame(Path::new("Z:/definitely/does/not/exist/nope.mp4"), 64);
        assert!(err.is_err(), "a nonexistent input file must be rejected, not panic");
    }

    /// Security (CPE-1480, mirrors `media_waveform`'s CPE-1478 test): an ffmpeg *protocol* string that
    /// isn't a local file — e.g. an `http://` URL (blind SSRF to internal hosts / cloud-metadata) or
    /// `concat:`/`subfile:` (arbitrary local-file read) — must be rejected, never handed to ffmpeg as
    /// input. The pre-spawn `reject_unsafe_ffmpeg_input` guard rejects it here before ffmpeg runs, and
    /// this must hold with NO ffmpeg install needed (the guard fires before any spawn is attempted).
    #[test]
    fn extract_frame_rejects_a_non_file_protocol_string_without_reaching_the_network() {
        for evil in [
            "http://169.254.169.254/latest/meta-data/",
            "concat:/etc/passwd",
            "subfile:,start,0,end,64,,:/etc/passwd",
        ] {
            let err = extract_frame(Path::new(evil), 64);
            assert!(err.is_err(), "a non-file protocol input ({evil}) must be rejected, not opened");
        }
    }

    /// Real-render smoke test, gated on ffmpeg actually being resolvable+runnable in this environment.
    /// ffmpeg IS installed locally (v8.1.1) at the time this ticket (CPE-1257) was built, so this test
    /// synthesizes a tiny test clip with ffmpeg's `lavfi testsrc` source, then asserts `extract_frame`
    /// returns a real, non-degenerate frame scaled to ~max_edge on its longest side. SKIPS (eprintln +
    /// early return, never a fail) on a runner without ffmpeg, rather than faking a pass.
    #[test]
    fn extract_frame_extracts_a_real_frame_from_a_synthetic_clip_when_ffmpeg_is_available() {
        if !ffmpeg_available() {
            crate::skip_notice!(
                "skipping extract_frame real-render test: no ffmpeg binary available in this \
                 environment (expected until CPE-1258 provisions/bundles one for CI/dev)"
            );
            return;
        }

        let d = scratch("real-clip");
        let clip = d.join("synthetic.mp4");
        let ffmpeg = resolve_ffmpeg_bin();

        // 320x240 landscape test pattern, 1 second at 10fps — tiny and fast to synthesize.
        let synth = Command::new(&ffmpeg)
            .args(["-y", "-f", "lavfi", "-i", "testsrc=duration=1:size=320x240:rate=10"])
            .arg(&clip)
            .output()
            .expect("ffmpeg is available; synthesizing the test clip must not fail to spawn");
        assert!(
            synth.status.success(),
            "synthesizing the test clip failed: {}",
            String::from_utf8_lossy(&synth.stderr)
        );

        let img = extract_frame(&clip, 64).expect("ffmpeg is available; extracting a frame must succeed");
        assert!(
            img.width() > 0 && img.height() > 0,
            "extracted frame must be non-degenerate, got {}x{}",
            img.width(),
            img.height()
        );
        let longest = img.width().max(img.height());
        assert_eq!(longest, 64, "longest edge should be scaled to exactly max_edge, got {}x{}", img.width(), img.height());
        assert!(img.width() <= 64 && img.height() <= 64);

        let _ = fs::remove_dir_all(&d);
    }

    /// Unconditional: max_edge=0 must clamp to at least 1px, never divide by zero / attempt a 0x0
    /// scale filter. Gated on ffmpeg availability the same way as the real-render test since it still
    /// needs a real render to observe the clamp.
    #[test]
    fn extract_frame_handles_a_zero_max_edge_without_panicking() {
        if !ffmpeg_available() {
            crate::skip_notice!(
                "skipping extract_frame zero-max-edge test: no ffmpeg binary available in this \
                 environment (expected until CPE-1258 provisions/bundles one for CI/dev)"
            );
            return;
        }

        let d = scratch("zero-edge");
        let clip = d.join("synthetic.mp4");
        let ffmpeg = resolve_ffmpeg_bin();
        let synth = Command::new(&ffmpeg)
            .args(["-y", "-f", "lavfi", "-i", "testsrc=duration=1:size=320x240:rate=10"])
            .arg(&clip)
            .output()
            .expect("ffmpeg is available; synthesizing the test clip must not fail to spawn");
        assert!(synth.status.success());

        let img = extract_frame(&clip, 0).expect("max_edge=0 must still succeed (clamped to 1)");
        assert!(img.width() >= 1 && img.height() >= 1);

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn downscale_to_max_edge_never_upscales_a_small_image() {
        let small = DynamicImage::new_rgb8(20, 10);
        let out = downscale_to_max_edge(small, 64);
        assert_eq!((out.width(), out.height()), (20, 10), "smaller-than-budget image must be untouched");
    }

    #[test]
    fn downscale_to_max_edge_caps_the_longest_edge_for_a_portrait_image() {
        // Simulates ffmpeg's width-only scale leaving a portrait frame's height uncapped: width already
        // <= max_edge, but height is not.
        let portrait = DynamicImage::new_rgb8(50, 200);
        let out = downscale_to_max_edge(portrait, 64);
        assert_eq!(out.height(), 64, "longest edge (height) must be capped to max_edge");
        assert!(out.width() <= 64);
    }

    // --- CPE-1261: exclusive scratch-dir hardening (CWE-377) ---------------------------------------
    // The "concurrent calls to create_scratch_dir never collide" guarantee is now tested once, directly
    // against the shared implementation, in `crate::ffmpeg_util`'s own test module (CPE-1478
    // extraction) — no need to re-prove it per caller here. The tests below stay in this module because
    // they exercise `extract_frame`'s *integration* with the scratch dir (created-then-cleaned-up around
    // a real ffmpeg spawn), which `ffmpeg_util`'s tests don't cover.

    /// Unconditional: pointing `extract_frame_with_ffmpeg` at a bogus ffmpeg binary still creates the
    /// scratch dir first (before the doomed spawn), and the error path must remove it — no leaked temp
    /// directory just because ffmpeg itself couldn't run.
    #[test]
    fn extract_frame_removes_the_scratch_dir_on_the_error_path_too() {
        let d = scratch("no-leak-error");
        let bogus_input = d.join("clip.mp4");
        fs::write(&bogus_input, b"irrelevant").unwrap();
        let bogus_ffmpeg = d.join("definitely-not-a-real-ffmpeg-binary-xyz123");

        let err = extract_frame_with_ffmpeg(&bogus_input, 64, &bogus_ffmpeg);
        assert!(err.is_err(), "a nonexistent ffmpeg binary must still be reported as Err");

        let scratch_dir = last_scratch_dir_for_test()
            .expect("create_scratch_dir must have run (and recorded itself) before the doomed ffmpeg spawn");
        assert!(
            !scratch_dir.exists(),
            "scratch dir must be removed on the error path too, leaked: {}",
            scratch_dir.display()
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// Real-render gated: after a SUCCESSFUL extract, the scratch dir the frame was written into must
    /// be gone — no leaked temp directory on the happy path either. SKIPS (not fails) without ffmpeg.
    #[test]
    fn extract_frame_removes_the_scratch_dir_after_a_successful_extract() {
        if !ffmpeg_available() {
            crate::skip_notice!(
                "skipping extract_frame scratch-dir-no-leak test: no ffmpeg binary available in this \
                 environment"
            );
            return;
        }

        let d = scratch("no-leak-success");
        let clip = d.join("synthetic.mp4");
        let ffmpeg = resolve_ffmpeg_bin();

        let synth = Command::new(&ffmpeg)
            .args(["-y", "-f", "lavfi", "-i", "testsrc=duration=1:size=320x240:rate=10"])
            .arg(&clip)
            .output()
            .expect("ffmpeg is available; synthesizing the test clip must not fail to spawn");
        assert!(synth.status.success(), "synthesizing the test clip failed: {}", String::from_utf8_lossy(&synth.stderr));

        let _img = extract_frame(&clip, 64).expect("ffmpeg is available; extracting a frame must succeed");

        let scratch_dir = last_scratch_dir_for_test()
            .expect("create_scratch_dir must have run (and recorded itself) during the successful extract");
        assert!(
            !scratch_dir.exists(),
            "scratch dir must be removed after a successful extract, leaked: {}",
            scratch_dir.display()
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1696: `if !out.exists()` folded every stat failure into "ffmpeg produced no output file", so an
    /// unreadable scratch path was blamed on an ffmpeg bug that isn't there. Runs on every OS with no
    /// ffmpeg binary and no privilege needed — the taxonomy is the whole behaviour.
    #[test]
    fn cpe_1696_an_unconfirmable_output_file_is_not_blamed_on_ffmpeg_producing_nothing() {
        let out = Path::new("/tmp/cpe1696/frame.png");
        assert!(classify_ffmpeg_output(out, Ok(true)).is_ok(), "a real output file is a success");
        let missing = classify_ffmpeg_output(out, Ok(false)).unwrap_err();
        assert!(
            missing.contains("produced no output file"),
            "a genuine absence must still say ffmpeg produced nothing: {missing}"
        );
        assert!(
            classify_ffmpeg_output(out, Err(std::io::Error::from(std::io::ErrorKind::NotFound)))
                .unwrap_err()
                .contains("produced no output file"),
            "an explicit NotFound is a genuine absence too"
        );
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
            std::io::ErrorKind::TimedOut,
        ] {
            let msg = classify_ffmpeg_output(out, Err(std::io::Error::new(kind, "Access is denied.")))
                .unwrap_err();
            assert!(
                !msg.contains("produced no output file"),
                "{kind:?} must not be reported as ffmpeg having produced nothing: {msg}"
            );
            assert!(
                msg.contains("could not be confirmed") && msg.contains("Access is denied."),
                "{kind:?} must name the real cause: {msg}"
            );
        }
    }
}
