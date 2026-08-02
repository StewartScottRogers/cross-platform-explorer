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
//! **ffmpeg resolution:** the injected native-dep dir ([`set_native_dep_dir`]) is tried first, then a
//! bundled copy sitting next to the running executable, then a bare `ffmpeg` resolved via `PATH` as
//! the dev fallback (mirrors `thumb_pdf::resolve_bindings`'s resolution order). If none are present,
//! spawning the process itself fails with a normal `Err` — never a panic — and the caller's existing
//! contract (no thumbnail -> generic type icon) takes over.
//!
//! **Why "next to the executable" alone isn't enough (CPE-1258 fix):** Tauri's `bundle.resources`
//! (where ship-time packaging stages ffmpeg) land in a *different* directory than the running
//! executable on 2 of the 3 shipped platforms — macOS (`AppName.app/Contents/MacOS/<exe>` vs
//! `AppName.app/Contents/Resources/`) and Linux (`usr/bin/<exe>` vs `usr/lib/<exe>/`); only Windows
//! NSIS stages resources next to the exe. This crate is Tauri-free and can't call
//! `app.path().resource_dir()` itself, so the app adapter resolves it and calls
//! [`set_native_dep_dir`] once at startup (mirroring how the sidecar host resolves `resource_dir()`
//! for its own bundled binaries in `src-tauri/src/lib.rs`) — tried before the `current_exe().parent()`
//! guess.
//!
//! **Frame selection:** seeks ~1s into the clip first (`-ss 1`) to skip a likely black lead-in frame;
//! very short or unusual clips where that offset yields no frame fall back to `-ss 0` (the very first
//! frame). Computing an exact 10%-of-duration offset would need `ffprobe` (or ffmpeg's own duration
//! probing) as a second dependency surface — the ticket explicitly prefers keeping this to the ONE
//! ffmpeg binary, so a fixed small offset + a zero-offset retry is used instead.
//!
//! **Output path + cleanup:** ffmpeg writes exactly one PNG frame to a unique path under the OS temp
//! dir (never a fixed/shared name — concurrent thumbnail requests must not collide), which is read back
//! via `image::open`, then deleted — on both the success and the error path, so a failed render never
//! leaks a temp file.
//!
//! **Sizing:** ffmpeg's own `scale` filter does the bulk of the downscale work (cheap: ffmpeg decodes
//! at reduced resolution rather than us decoding a full-size frame into memory), capping width to
//! `max_edge`. Because that alone doesn't correctly cap the longest edge for a portrait (taller-than-wide)
//! clip, a final exact `image`-crate downscale ([`downscale_to_max_edge`]) is applied after decode —
//! never upscales, and lands the longest edge at exactly `max_edge` regardless of orientation.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use image::DynamicImage;

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
    let tmp = unique_temp_png_path();

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

    let _ = fs::remove_file(&tmp); // clean up even on the error path — never leak the temp file

    let img = decoded?;
    Ok(downscale_to_max_edge(img, edge))
}

/// Runs ffmpeg once, seeking to `offset_secs` and writing a single scaled PNG frame to `out`. `Err` on
/// a failed spawn (binary missing/unresolvable), a non-zero exit, or a missing output file — never
/// panics.
fn run_ffmpeg_frame(ffmpeg: &Path, input: &Path, out: &Path, offset_secs: &str, max_edge: u32) -> Result<(), String> {
    // Caps the width to at most max_edge, height computed to preserve aspect (ffmpeg's `-2` rounds to
    // an even value). The comma inside `min(...)` must be backslash-escaped so ffmpeg's filtergraph
    // parser doesn't read it as a filter separator.
    let scale = format!("scale='min({max_edge}\\,iw)':-2");

    let output = Command::new(ffmpeg)
        .arg("-y")
        .arg("-ss")
        .arg(offset_secs)
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
    if !out.exists() {
        return Err("ffmpeg reported success but produced no output file".to_string());
    }
    Ok(())
}

/// The bundle-resource directory injected by the (Tauri-aware) app adapter — see the module doc's
/// "CPE-1258 fix" note. `None` until [`set_native_dep_dir`] is called (dev builds / not yet wired),
/// in which case resolution falls through to the `current_exe().parent()` / `PATH` guesses unchanged.
static NATIVE_DEP_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Sets the directory the bundled `ffmpeg` executable was staged into, so [`resolve_ffmpeg_bin`] can
/// find it there first — call once at app startup with `app.path().resource_dir()` (or wherever the
/// `bundle.resources` ffmpeg entry actually lands). This crate is Tauri-free and can't resolve that
/// path itself, hence the injection seam (mirrors `thumb_pdf::set_native_dep_dir`). Unlike pdfium,
/// ffmpeg is resolved fresh on every call rather than cached, so — unlike the pdfium setter — there's
/// no "must be called before first use" ordering requirement here; still, call it once at startup.
/// A second call is a silent no-op.
pub fn set_native_dep_dir(dir: PathBuf) {
    let _ = NATIVE_DEP_DIR.set(dir);
}

/// Resolves the ffmpeg binary, trying each candidate path in order (see the module doc's resolution-
/// order note), then finally a bare `ffmpeg` resolved via `PATH` as the last-resort dev fallback.
/// Never fails outright — an unresolvable PATH fallback simply fails later at spawn time with a normal
/// `Err`.
fn resolve_ffmpeg_bin() -> PathBuf {
    let exe_name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };

    // (1) The injected native-dep dir — the CPE-1258 fix, and the ONLY correct location on
    // macOS/Linux once the app adapter has set it (real installs).
    if let Some(dir) = NATIVE_DEP_DIR.get() {
        let bundled = dir.join(exe_name);
        if bundled.exists() {
            return bundled;
        }
    }
    // (2) Next to the running executable — already correct on Windows; a dev-build fallback
    // everywhere else (e.g. `cargo run` with a hand-copied binary).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join(exe_name);
            if bundled.exists() {
                return bundled;
            }
        }
    }
    // (3) Bare `ffmpeg`/`ffmpeg.exe` resolved via PATH — the dev fallback (e.g. a system ffmpeg
    // install, or CI's package-manager install for the real-render tests).
    PathBuf::from(exe_name)
}

/// A unique path under the OS temp dir for ffmpeg's single-frame PNG output — never a fixed/shared
/// name, so concurrent thumbnail requests (the pipeline fans these out across `spawn_blocking` threads)
/// can't collide on the same file.
fn unique_temp_png_path() -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    std::env::temp_dir().join(format!("cpe-thumbvideo-{}-{ts}-{n}.png", std::process::id()))
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

/// True if ffmpeg is resolvable and actually runs in this environment (`ffmpeg -version` succeeds).
/// Used to gate the real-render test so it SKIPS (not fails) on a runner without ffmpeg. Test-only.
#[cfg(test)]
fn ffmpeg_available() -> bool {
    Command::new(resolve_ffmpeg_bin())
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-thumbvideo-test-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
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

    /// Real-render smoke test, gated on ffmpeg actually being resolvable+runnable in this environment.
    /// ffmpeg IS installed locally (v8.1.1) at the time this ticket (CPE-1257) was built, so this test
    /// synthesizes a tiny test clip with ffmpeg's `lavfi testsrc` source, then asserts `extract_frame`
    /// returns a real, non-degenerate frame scaled to ~max_edge on its longest side. SKIPS (eprintln +
    /// early return, never a fail) on a runner without ffmpeg, rather than faking a pass.
    #[test]
    fn extract_frame_extracts_a_real_frame_from_a_synthetic_clip_when_ffmpeg_is_available() {
        if !ffmpeg_available() {
            eprintln!(
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
            eprintln!(
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
}
