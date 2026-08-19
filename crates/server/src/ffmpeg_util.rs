//! Shared bundled-`ffmpeg`-subprocess plumbing (CPE-1478, epic CPE-720): the resolve/scratch-dir/
//! availability helpers originally written for [`crate::thumb_video`] (CPE-1257/1258/1261), factored
//! out here so a second ffmpeg-shelling module ([`crate::media_waveform`], CPE-1478) doesn't copy-paste
//! them a third time. Both callers keep the same license-clean "separately bundled program invoked as a
//! subprocess" approach described in `thumb_video`'s module doc — this module never links ffmpeg
//! in-process and adds **zero** new Cargo dependencies.
//!
//! Behaviour is unchanged from the pre-extraction `thumb_video.rs` copies: same resolution order
//! (injected native-dep dir → next to the running executable → bare `ffmpeg`/`ffmpeg.exe` on `PATH`),
//! same exclusive-scratch-dir creation (CWE-377 hardening, CPE-1261), same test-only availability probe.
//! [`create_scratch_dir`] takes a caller-supplied `tag` so each caller's temp-dir names stay
//! distinguishable (`thumb_video` keeps its historical `"thumbvideo"` tag; [`crate::media_waveform`]
//! uses `"waveform"`), but the exclusivity/retry/cleanup mechanics are identical either way.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// The bundle-resource directory injected by the (Tauri-aware) app adapter. `None` until
/// [`set_native_dep_dir`] is called (dev builds / not yet wired), in which case resolution falls
/// through to the `current_exe().parent()` / `PATH` guesses unchanged.
static NATIVE_DEP_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Sets the directory the bundled `ffmpeg` executable was staged into, so [`resolve_ffmpeg_bin`] can
/// find it there first — call once at app startup with `app.path().resource_dir()` (or wherever the
/// `bundle.resources` ffmpeg entry actually lands). This crate is Tauri-free and can't resolve that
/// path itself, hence the injection seam. ffmpeg is resolved fresh on every call rather than cached, so
/// there's no "must be called before first use" ordering requirement here; still, call it once at
/// startup. A second call is a silent no-op.
pub fn set_native_dep_dir(dir: PathBuf) {
    let _ = NATIVE_DEP_DIR.set(dir);
}

/// Resolves the ffmpeg binary, trying each candidate path in order — (1) the injected native-dep dir
/// (the CPE-1258 fix, and the ONLY correct location on macOS/Linux once the app adapter has set it —
/// real installs), (2) next to the running executable (already correct on Windows; a dev-build fallback
/// elsewhere), then finally (3) a bare `ffmpeg`/`ffmpeg.exe` resolved via `PATH` as the last-resort dev
/// fallback. Never fails outright — an unresolvable PATH fallback simply fails later at spawn time with
/// a normal `Err`.
pub fn resolve_ffmpeg_bin() -> PathBuf {
    let exe_name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };

    if let Some(dir) = NATIVE_DEP_DIR.get() {
        let bundled = dir.join(exe_name);
        if bundled.exists() {
            return bundled;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join(exe_name);
            if bundled.exists() {
                return bundled;
            }
        }
    }
    PathBuf::from(exe_name)
}

/// The `-protocol_whitelist` value every ffmpeg-shelling module passes before `-i`: restricts ffmpeg to
/// the `file` protocol for its input plus `pipe` (needed by [`crate::media_waveform`]'s `pipe:1` output;
/// harmless for [`crate::thumb_video`], which never uses it). See [`reject_unsafe_ffmpeg_input`]'s doc
/// for the full rationale — this is the load-bearing guard at the ffmpeg layer itself.
pub const FFMPEG_PROTOCOL_WHITELIST: &str = "file,pipe";

/// Defense-in-depth guard (CPE-1478, then shared with `thumb_video` in CPE-1480) for every ffmpeg-
/// shelling module's IPC-reachable subprocess boundary: rejects any `path` that isn't an existing regular
/// local file BEFORE it's handed to ffmpeg as `-i` input. Without this, a `path` of `http://…` (blind SSRF
/// to internal hosts / cloud-metadata), `concat:…` / `subfile:…` (arbitrary local-file read), or `data:…`
/// is a valid ffmpeg *protocol* string, not a filename — ffmpeg would happily open it. Pair with
/// `-protocol_whitelist` (see [`FFMPEG_PROTOCOL_WHITELIST`]) on the spawned command, which is the
/// load-bearing guard at the ffmpeg layer; this is the cheaper first line that also yields a clearer
/// error for a genuinely-missing file.
pub fn reject_unsafe_ffmpeg_input(path: &Path) -> Result<(), String> {
    if std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
        Ok(())
    } else {
        Err(format!("not a readable regular file: {}", path.display()))
    }
}

/// Creates a fresh, **exclusively-owned** scratch directory under the OS temp dir for one ffmpeg
/// subprocess invocation, tagged with the caller-supplied `tag` (e.g. `"thumbvideo"`, `"waveform"") so
/// each caller's temp dirs stay distinguishable on disk, and returns its path.
///
/// Security rationale (CPE-1261, CWE-377): building a unique-but-*predictable* filename and letting
/// ffmpeg write straight to it (following `-y`) lets an attacker who guessed the name ahead of time
/// pre-plant a symlink there; `-y` truncates-in-place and follows symlinks, so ffmpeg would clobber
/// whatever the symlink pointed at. `std::fs::create_dir` closes that window atomically: it fails with
/// `AlreadyExists` if *anything* — file, directory, or symlink — already sits at that path, so a
/// successful return means this call, and only this call, owns a brand-new directory nothing could have
/// been pre-planted inside (it didn't exist a moment ago). Writing ffmpeg's output *inside* that
/// directory (rather than trying to harden the filename itself) means the exclusivity guarantee covers
/// the output too, with no dependency on high-entropy randomness or a new crate.
///
/// Concurrency: the pid+nanos+monotonic-counter name is already effectively unique per call, so
/// `create_dir` is expected to succeed on the first attempt. The bounded retry below is
/// belt-and-suspenders for the vanishingly unlikely case of a name collision — not required for
/// correctness, since each attempt still gets its own atomically-exclusive name.
///
/// Never panics: after `MAX_ATTEMPTS` failed creates, returns `Err` rather than looping forever or
/// falling back to a non-exclusive path.
pub fn create_scratch_dir(tag: &str) -> Result<PathBuf, String> {
    const MAX_ATTEMPTS: u32 = 8;
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let mut last_err: Option<std::io::Error> = None;
    for _ in 0..MAX_ATTEMPTS {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("cpe-{tag}-{}-{ts}-{n}", std::process::id()));

        match fs::create_dir(&dir) {
            Ok(()) => {
                #[cfg(test)]
                record_scratch_dir_for_test(dir.clone());
                return Ok(dir);
            }
            Err(e) => last_err = Some(e),
        }
    }

    Err(format!(
        "failed to create an exclusive scratch dir under the OS temp dir after {MAX_ATTEMPTS} attempts: {}",
        last_err.map(|e| e.to_string()).unwrap_or_else(|| "unknown error".to_string())
    ))
}

// Test-only side channel: the most recent scratch dir `create_scratch_dir` created **on the calling
// thread**. Thread-local (not a shared global) because cargo test runs each `#[test]` fn on its own
// thread by default, so this lets a leak-check test recover exactly which directory to check, without
// racing other tests that concurrently create their own scratch dirs. Shared across every caller module
// (`thumb_video`, `media_waveform`, …) since each test only ever cares about its own thread's calls.
#[cfg(test)]
thread_local! {
    static LAST_SCRATCH_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn record_scratch_dir_for_test(dir: PathBuf) {
    LAST_SCRATCH_DIR.with(|c| *c.borrow_mut() = Some(dir));
}

#[cfg(test)]
pub fn last_scratch_dir_for_test() -> Option<PathBuf> {
    LAST_SCRATCH_DIR.with(|c| c.borrow().clone())
}

/// True if ffmpeg is resolvable and actually runs in this environment (`ffmpeg -version` succeeds).
/// Used to gate real-render tests so they SKIP (not fail) on a runner without ffmpeg. Test-only, shared
/// by every ffmpeg-shelling module's test suite.
#[cfg(test)]
pub fn ffmpeg_available() -> bool {
    std::process::Command::new(resolve_ffmpeg_bin())
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-ffmpegutil-test-{tag}"))
    }

    /// Unconditional (no ffmpeg needed): each concurrent [`create_scratch_dir`] call must succeed and
    /// get a distinct, exclusively-owned directory.
    #[test]
    fn create_scratch_dir_never_collides_across_concurrent_calls() {
        use std::thread;

        let handles: Vec<_> = (0..16).map(|_| thread::spawn(|| create_scratch_dir("test"))).collect();

        let mut dirs: Vec<PathBuf> = handles
            .into_iter()
            .map(|h| h.join().expect("thread must not panic").expect("scratch dir creation must succeed"))
            .collect();

        let total = dirs.len();
        dirs.sort();
        dirs.dedup();
        assert_eq!(dirs.len(), total, "every concurrent call must get a distinct, non-colliding scratch dir");

        for d in &dirs {
            assert!(d.is_dir(), "each returned path must actually be a created directory: {}", d.display());
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn create_scratch_dir_tags_the_directory_name() {
        let d = create_scratch_dir("mytag").expect("scratch dir creation must succeed");
        assert!(
            d.file_name().unwrap().to_string_lossy().contains("mytag"),
            "scratch dir name should carry the caller's tag: {}",
            d.display()
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// Unconditional: pointing the resolver where nothing exists still returns a path (never panics);
    /// resolution failure surfaces later at spawn time as a normal `Err`.
    #[test]
    fn resolve_ffmpeg_bin_never_panics_without_a_bundled_binary() {
        let _ = resolve_ffmpeg_bin();
    }

    /// Security (CPE-1478/1480): an ffmpeg *protocol* string that isn't a local file — e.g. an `http://`
    /// URL (blind SSRF) or `concat:`/`subfile:` (arbitrary read) — must be rejected by the shared guard,
    /// shielding every caller (`thumb_video`, `media_waveform`) from having to reimplement this check.
    #[test]
    fn reject_unsafe_ffmpeg_input_rejects_non_file_protocol_strings() {
        for evil in [
            "http://169.254.169.254/latest/meta-data/",
            "concat:/etc/passwd",
            "subfile:,start,0,end,64,,:/etc/passwd",
        ] {
            assert!(
                reject_unsafe_ffmpeg_input(Path::new(evil)).is_err(),
                "a non-file protocol input ({evil}) must be rejected"
            );
        }
    }

    #[test]
    fn reject_unsafe_ffmpeg_input_rejects_a_nonexistent_path() {
        assert!(reject_unsafe_ffmpeg_input(Path::new("Z:/definitely/does/not/exist/nope.mp4")).is_err());
    }

    #[test]
    fn reject_unsafe_ffmpeg_input_accepts_an_existing_regular_file() {
        let d = scratch("reject-guard");
        let f = d.join("real.mp4");
        fs::write(&f, b"irrelevant").unwrap();
        assert!(reject_unsafe_ffmpeg_input(&f).is_ok());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn reject_unsafe_ffmpeg_input_rejects_a_directory() {
        let d = scratch("reject-guard-dir");
        assert!(
            reject_unsafe_ffmpeg_input(&d).is_err(),
            "a directory is not a regular file and must be rejected"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn set_native_dep_dir_is_a_silent_no_op_on_a_second_call() {
        let d = scratch("native-dep");
        set_native_dep_dir(d.to_path_buf());
        set_native_dep_dir(Path::new("Z:/somewhere/else").to_path_buf());
        // Doesn't panic and doesn't change already-set state (OnceLock semantics); nothing else to
        // assert without a bundled binary present.
        let _ = fs::remove_dir_all(&d);
    }
}
