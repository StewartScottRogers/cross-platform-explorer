//! Audio-waveform peak extraction (CPE-1478, epic CPE-720): turn an audio file into a downsampled peak
//! array — a fixed bucket count of `(min, max)` samples regardless of source length — for a later GUI
//! ticket to render as a scrub-bar waveform (CPE-1431). See the parent ticket for the full rationale;
//! this module reuses the same license-clean pattern [`crate::thumb_video`] established (CPE-1257): shell
//! out to a **separately bundled `ffmpeg` executable** as a subprocess, never linked in-process — the
//! shared resolve/scratch-dir plumbing now lives in [`crate::ffmpeg_util`] (this module is its second
//! caller). Feature-gated OFF by default (`waveform`) so the plain build compiles zero waveform code and
//! adds **zero** new Cargo dependencies even when the feature is on.
//!
//! **Decode:** `ffmpeg -i <path> -f f32le -ac 1 -ar 8000 pipe:1` — mono 32-bit float PCM at a
//! deliberately low [`SAMPLE_RATE`] (a waveform strip only needs the min/max *envelope* shape, not audio
//! fidelity). Every format ffmpeg can demux/decode is covered (mp3/wav/ogg/flac/m4a/aac/opus/…), unlike
//! a WAV-only pure-Rust parser.
//!
//! **CRITICAL — bounded read (CPE-1478):** unlike [`crate::thumb_video`], which only ever reads back a
//! small PNG file, this module pipes a genuinely unbounded amount of data — a long or crafted audio file
//! makes ffmpeg emit gigabytes of raw PCM. [`extract_waveform_peaks`] therefore **never** uses
//! `Command::output()`/`Child::wait_with_output()` (both buffer the child's entire stdout before
//! returning — an OOM DoS on a hostile or merely very long input). Instead [`read_capped`] wraps the
//! child's stdout pipe in `Read::take(MAX_PCM_BYTES + 1)` (the `+1`/truncate-back-down idiom
//! [`crate::doc_text::read_entry_capped`] already established in this crate, so an exact-cap-sized
//! stream is distinguishable from a truncated one) and reads AT MOST `MAX_PCM_BYTES + 1` bytes no matter
//! how much more the process would otherwise try to write. If the cap was hit, the child is killed
//! immediately afterward — otherwise it would block forever trying to write more PCM into a pipe nobody
//! is draining on the other end. `read_capped` is a small, `Read`-generic helper specifically so this
//! bounding logic is unit-testable directly against an in-memory buffer (see the tests module) without
//! needing a real ffmpeg subprocess to actually emit gigabytes of data.
//!
//! **Bucketing:** the (possibly cap-truncated) PCM buffer is parsed into `f32` samples and split into
//! exactly `buckets` contiguous, near-equal-length spans (`i * total / buckets` integer arithmetic, done
//! in `u128` to avoid overflow) — the min and max sample in each span becomes that bucket's `(min, max)`
//! pair, in ascending time order. A request for more buckets than available samples degrades gracefully
//! (an empty span repeats the previous bucket's value, or `(0.0, 0.0)` for the very first) rather than
//! panicking on an out-of-range slice.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use crate::ffmpeg_util::resolve_ffmpeg_bin;

/// Mono PCM sample rate (Hz) ffmpeg is asked to resample to before piping data back. Chosen low
/// deliberately: a waveform strip only ever needs the min/max *envelope* shape, not audio fidelity, and
/// a lower rate means more real-world audio duration fits under [`MAX_PCM_BYTES`] before the bounded
/// read truncates it. 8 kHz keeps a comfortable margin above what any sane bucket-count/duration
/// combination needs to resolve a visually meaningful envelope.
const SAMPLE_RATE: u32 = 8_000;

/// Hard cap on PCM bytes read from ffmpeg's stdout pipe — the CRITICAL bounded-read requirement (see the
/// module doc). At [`SAMPLE_RATE`] mono `f32le` (4 bytes/sample), 64 MiB is ~35 minutes of decoded
/// audio: generous headroom for any real audio file (music tracks / voice memos / podcasts are almost
/// always well under that), while bounding worst-case memory to one fixed, small budget regardless of a
/// long/crafted source file's declared or actual duration. Mirrors the `MAX_SOURCE_FILE_BYTES` (128 MiB,
/// `thumb_source`) / `MAX_SFNT_BYTES` (64 MiB, `thumb_font`) style caps already established elsewhere in
/// this crate.
const MAX_PCM_BYTES: u64 = 64 * 1024 * 1024;

/// Hard cap on bytes drained from ffmpeg's stderr pipe. `-loglevel error` keeps ffmpeg's own diagnostics
/// small, but stderr still needs *some* bound and must be drained concurrently with stdout (see
/// [`extract_waveform_peaks_with_ffmpeg`]) — an undrained full pipe would make ffmpeg block trying to
/// write to it, deadlocking the whole call.
const MAX_STDERR_BYTES: u64 = 64 * 1024;

/// Extracts a downsampled peak array from the audio at `path`: exactly `buckets` `(min, max)` pairs in
/// ascending time order, regardless of the source's length (`buckets` is clamped to at least 1). Never
/// panics: a missing/unresolvable ffmpeg binary, a non-zero ffmpeg exit, a nonexistent/empty/undecodable
/// input, or a crafted file that would otherwise make ffmpeg emit unbounded PCM all return `Err` — see
/// the module doc's "CRITICAL — bounded read" note for how the last case is actually bounded rather than
/// merely hoped to be rare. The audio file itself is never read into memory by this crate directly;
/// ffmpeg reads it as a subprocess.
pub fn extract_waveform_peaks(path: &Path, buckets: usize) -> Result<Vec<(f32, f32)>, String> {
    extract_waveform_peaks_with_ffmpeg(path, buckets, &resolve_ffmpeg_bin())
}

/// The real implementation, taking the ffmpeg binary path as a parameter so tests can point it at a
/// bogus path (to exercise the "ffmpeg missing" `Err` path) without needing to tamper with `PATH` or the
/// current executable's directory — mirrors `thumb_video::extract_frame_with_ffmpeg`'s test seam.
fn extract_waveform_peaks_with_ffmpeg(
    path: &Path,
    buckets: usize,
    ffmpeg: &Path,
) -> Result<Vec<(f32, f32)>, String> {
    let buckets = buckets.max(1);

    let mut child: Child = Command::new(ffmpeg)
        .arg("-nostdin") // never let ffmpeg read our stdin (there isn't one) or block waiting on it
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(path)
        .arg("-vn") // audio only — don't waste effort decoding a video stream, if any
        .arg("-f")
        .arg("f32le")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg(SAMPLE_RATE.to_string())
        .arg("pipe:1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to launch ffmpeg ({}): {e}", ffmpeg.display()))?;

    let stdout = child.stdout.take().ok_or_else(|| "ffmpeg produced no stdout pipe".to_string())?;
    let stderr = child.stderr.take().ok_or_else(|| "ffmpeg produced no stderr pipe".to_string())?;

    // Drain stderr on its own thread, bounded, concurrently with the bounded stdout read below. ffmpeg
    // can write diagnostics to stderr at any point; if nobody drains it while its pipe buffer fills,
    // ffmpeg blocks writing to IT too, deadlocking the whole call regardless of the stdout cap.
    let stderr_handle = std::thread::spawn(move || {
        read_capped(stderr, MAX_STDERR_BYTES)
            .map(|(buf, _truncated)| String::from_utf8_lossy(&buf).into_owned())
            .unwrap_or_default()
    });

    // CRITICAL bounded read (CPE-1478) — see the module doc. Never `Command::output()`.
    let (pcm, truncated) = read_capped(stdout, MAX_PCM_BYTES)
        .map_err(|e| format!("failed to read ffmpeg's PCM output: {e}"))?;

    // If the cap was hit, ffmpeg may still be trying to write more PCM into a pipe nobody is reading
    // from any more and would otherwise block forever — kill it rather than waiting for a natural exit.
    // If the whole stream fit under the cap, ffmpeg already hit EOF on its own and `wait()` reaps it
    // normally below.
    if truncated {
        let _ = child.kill();
    }
    let status = child.wait();
    let stderr_text = stderr_handle.join().unwrap_or_default();

    // Only enforce a clean exit status when the WHOLE stream was consumed — a truncated read means WE
    // killed the process ourselves, so a non-zero/"killed" status there is expected, not a real failure
    // (the truncated PCM we already captured is still valid audio data, just capped).
    if !truncated {
        match status {
            Ok(st) if st.success() => {}
            Ok(st) => return Err(format!("ffmpeg exited with {st}: {stderr_text}")),
            Err(e) => return Err(format!("failed to wait on ffmpeg: {e}")),
        }
    }

    if pcm.len() < 4 {
        return Err(if stderr_text.trim().is_empty() {
            "ffmpeg produced no decodable audio data (empty, nonexistent, or undecodable file)".to_string()
        } else {
            format!("ffmpeg produced no decodable audio data: {stderr_text}")
        });
    }

    Ok(bucket_pcm_f32le(&pcm, buckets))
}

/// Reads at most `cap + 1` bytes from `r` into memory and reports whether the input had more than `cap`
/// bytes available (i.e. was truncated). Generic over [`Read`] specifically so the bounded-read logic
/// that guards [`extract_waveform_peaks`] against an OOM DoS is unit-testable directly against an
/// in-memory buffer, without needing a real ffmpeg subprocess to actually emit a huge amount of data.
///
/// Reads one byte *past* `cap` rather than stopping exactly at it (mirrors
/// [`crate::doc_text::read_entry_capped`]'s cap-plus-one idiom): with an exact `take(cap)`, an input
/// whose real size is precisely `cap` bytes is indistinguishable from one that got cut off there — both
/// read exactly `cap` bytes. Reading the extra byte disambiguates them (only an input with *more* than
/// `cap` bytes ever fills the `cap + 1` buffer), and the buffer is truncated back to `cap` before
/// returning either way, so memory stays bounded at `cap + 1` bytes — one byte of headroom, never an
/// unbounded read.
fn read_capped<R: Read>(r: R, cap: u64) -> Result<(Vec<u8>, bool), String> {
    let mut buf = Vec::new();
    let mut limited = r.take(cap + 1);
    limited.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let truncated = buf.len() as u64 > cap;
    if truncated {
        buf.truncate(cap as usize);
    }
    Ok((buf, truncated))
}

/// Splits `pcm` (raw little-endian `f32` mono samples) into exactly `buckets` contiguous, near-equal
/// spans and returns each span's `(min, max)` sample as one bucket, in ascending time order. Any trailing
/// bytes that don't form a whole 4-byte sample (possible when [`read_capped`] truncated mid-sample) are
/// silently dropped. `buckets` is assumed already clamped to at least 1 by the caller. Never panics: a
/// span with zero samples in it (more buckets requested than samples available) repeats the previous
/// bucket's value, or falls back to `(0.0, 0.0)` for the very first bucket.
fn bucket_pcm_f32le(pcm: &[u8], buckets: usize) -> Vec<(f32, f32)> {
    let samples: Vec<f32> = pcm.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    let total = samples.len() as u128;
    let buckets_u = buckets as u128;

    let mut out = Vec::with_capacity(buckets);
    for i in 0..buckets {
        let start = (i as u128 * total / buckets_u) as usize;
        let end = ((i as u128 + 1) * total / buckets_u) as usize;

        if start >= end || start >= samples.len() {
            out.push(out.last().copied().unwrap_or((0.0, 0.0)));
            continue;
        }

        let span = &samples[start..end.min(samples.len())];
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        for &s in span {
            // Guard against NaN/±Inf a crafted or malformed PCM stream could reinterpret bytes into —
            // ordinary comparisons against NaN are always false, which would otherwise leave lo/hi stuck
            // at their sentinel values instead of a plausible in-range number.
            let s = if s.is_finite() { s } else { 0.0 };
            if s < lo {
                lo = s;
            }
            if s > hi {
                hi = s;
            }
        }
        out.push((lo, hi));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg_util::ffmpeg_available;
    use std::fs;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-waveform-test-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    // --- CPE-1478 CRITICAL: bounded read ------------------------------------------------------------

    /// Unconditional, no subprocess involved: a huge (well past the cap) in-memory input must be read
    /// as AT MOST `cap + 1` bytes, never buffered in full — this is the actual proof the bounded-read
    /// requirement holds, independent of whether a real ffmpeg happens to be installed on this runner.
    #[test]
    fn read_capped_never_buffers_more_than_the_cap_plus_one_byte_on_a_huge_input() {
        let huge = vec![0x7Fu8; 10 * 1024 * 1024]; // 10 MiB — far past a small test cap
        let cap = 1024u64;

        let (buf, truncated) = read_capped(Cursor::new(huge), cap).expect("read must not fail");

        assert!(truncated, "an input larger than the cap must be reported as truncated");
        assert_eq!(buf.len(), cap as usize, "buffer must be truncated back down to exactly the cap");
    }

    /// Unconditional: an input strictly under the cap is returned whole and NOT flagged truncated.
    #[test]
    fn read_capped_returns_the_whole_input_when_under_the_cap() {
        let small = b"just a few bytes of pcm-shaped data".to_vec();
        let (buf, truncated) = read_capped(Cursor::new(small.clone()), 4096).expect("read must not fail");
        assert_eq!(buf, small);
        assert!(!truncated);
    }

    /// Unconditional: an input of EXACTLY `cap` bytes must not be misreported as truncated — this is
    /// exactly the cap-plus-one disambiguation `read_capped`'s doc explains (mirrors
    /// `doc_text::read_entry_capped`'s own test of the same edge case).
    #[test]
    fn read_capped_does_not_misreport_an_input_of_exactly_the_cap_as_truncated() {
        let exact = vec![0xABu8; 256];
        let (buf, truncated) = read_capped(Cursor::new(exact), 256).expect("read must not fail");
        assert_eq!(buf.len(), 256);
        assert!(!truncated, "an input of exactly the cap must not be reported as truncated");
    }

    // --- Bucketing -----------------------------------------------------------------------------------

    #[test]
    fn bucket_pcm_f32le_returns_exactly_the_requested_bucket_count() {
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32 / 1000.0) * 2.0 - 1.0).collect();
        let pcm: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();

        let out = bucket_pcm_f32le(&pcm, 37);
        assert_eq!(out.len(), 37);
    }

    #[test]
    fn bucket_pcm_f32le_reports_the_true_min_and_max_within_each_span() {
        // A single bucket over a ramp from -1.0 to +1.0 must report exactly that min/max.
        let samples: Vec<f32> = (0..=100).map(|i| (i as f32 / 100.0) * 2.0 - 1.0).collect();
        let pcm: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();

        let out = bucket_pcm_f32le(&pcm, 1);
        assert_eq!(out.len(), 1);
        let (lo, hi) = out[0];
        assert!((lo - (-1.0)).abs() < 1e-6, "min should be ~-1.0, got {lo}");
        assert!((hi - 1.0).abs() < 1e-6, "max should be ~1.0, got {hi}");
    }

    #[test]
    fn bucket_pcm_f32le_never_panics_when_more_buckets_are_requested_than_samples() {
        let samples: Vec<f32> = vec![0.5, -0.5, 0.25];
        let pcm: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();

        let out = bucket_pcm_f32le(&pcm, 20);
        assert_eq!(out.len(), 20, "must still return exactly the requested bucket count");
    }

    #[test]
    fn bucket_pcm_f32le_handles_empty_pcm_without_panicking() {
        let out = bucket_pcm_f32le(&[], 10);
        assert_eq!(out.len(), 10);
        assert!(out.iter().all(|&(lo, hi)| lo == 0.0 && hi == 0.0));
    }

    #[test]
    fn bucket_pcm_f32le_drops_a_trailing_partial_sample_without_panicking() {
        let samples: Vec<f32> = vec![0.1, 0.2, 0.3];
        let mut pcm: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        pcm.extend_from_slice(&[0xFF, 0xFF]); // 2 trailing bytes — not a whole f32
        let out = bucket_pcm_f32le(&pcm, 3);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn bucket_pcm_f32le_guards_against_nan_and_infinite_reinterpreted_bytes() {
        // f32 bit pattern for NaN and +Infinity, reinterpreted from raw bytes (simulates a malformed /
        // adversarial PCM stream) — must not poison min/max with NaN or leave them at the sentinel
        // +-Infinity used internally.
        let mut pcm = Vec::new();
        pcm.extend_from_slice(&f32::NAN.to_le_bytes());
        pcm.extend_from_slice(&f32::INFINITY.to_le_bytes());
        pcm.extend_from_slice(&f32::NEG_INFINITY.to_le_bytes());

        let out = bucket_pcm_f32le(&pcm, 1);
        assert_eq!(out.len(), 1);
        let (lo, hi) = out[0];
        assert!(lo.is_finite(), "min must be finite, got {lo}");
        assert!(hi.is_finite(), "max must be finite, got {hi}");
    }

    // --- Unconditional error paths (no ffmpeg install required) --------------------------------------

    /// Pointing the resolver at a path that can't possibly exist must fail cleanly (spawn error), never
    /// panic.
    #[test]
    fn extract_waveform_peaks_with_missing_ffmpeg_binary_errs_without_panicking() {
        let d = scratch("missing-ffmpeg");
        let bogus_input = d.join("clip.mp3");
        fs::write(&bogus_input, b"irrelevant").unwrap();
        let bogus_ffmpeg = d.join("definitely-not-a-real-ffmpeg-binary-xyz123");

        let err = extract_waveform_peaks_with_ffmpeg(&bogus_input, 10, &bogus_ffmpeg);
        assert!(err.is_err(), "a nonexistent ffmpeg binary must be reported as Err, not panic");

        let _ = fs::remove_dir_all(&d);
    }

    /// Garbage (non-audio) bytes handed to a REAL ffmpeg resolution must fail cleanly — if ffmpeg is
    /// installed it refuses to decode the file (non-zero exit -> Err); if ffmpeg is NOT installed,
    /// resolution itself fails at spawn time (also Err). Either way, no panic.
    #[test]
    fn extract_waveform_peaks_rejects_an_undecodable_file_without_panicking() {
        let d = scratch("garbage");
        let f = d.join("not_audio.mp3");
        fs::write(&f, b"this is not an audio file, just garbage bytes").unwrap();

        let err = extract_waveform_peaks(&f, 10);
        assert!(err.is_err(), "an undecodable/garbage file must be rejected, not panic");

        let _ = fs::remove_dir_all(&d);
    }

    /// A nonexistent input path must fail cleanly rather than panic, regardless of whether ffmpeg itself
    /// is available.
    #[test]
    fn extract_waveform_peaks_rejects_a_nonexistent_input_path_without_panicking() {
        let err = extract_waveform_peaks(Path::new("Z:/definitely/does/not/exist/nope.mp3"), 10);
        assert!(err.is_err(), "a nonexistent input file must be rejected, not panic");
    }

    /// A 0-byte file must fail cleanly rather than panic.
    #[test]
    fn extract_waveform_peaks_rejects_an_empty_file_without_panicking() {
        let d = scratch("empty");
        let f = d.join("empty.mp3");
        fs::write(&f, b"").unwrap();

        let err = extract_waveform_peaks(&f, 10);
        assert!(err.is_err(), "a 0-byte file must be rejected, not panic");

        let _ = fs::remove_dir_all(&d);
    }

    /// `buckets == 0` must clamp to at least 1 rather than dividing by zero.
    #[test]
    fn extract_waveform_peaks_clamps_a_zero_bucket_request_without_panicking() {
        let d = scratch("zero-buckets");
        let f = d.join("not_audio.mp3");
        fs::write(&f, b"garbage").unwrap();
        // Still expected to error (garbage input), but must not panic on the zero-buckets divide.
        let _ = extract_waveform_peaks(&f, 0);
        let _ = fs::remove_dir_all(&d);
    }

    // --- ffmpeg-gated real-render tests (SKIP, not fail, without ffmpeg) -----------------------------

    fn synth_sine(ffmpeg: &Path, out: &Path, duration_secs: u32) {
        let synth = Command::new(ffmpeg)
            .args(["-y", "-f", "lavfi"])
            .arg("-i")
            .arg(format!("sine=frequency=440:duration={duration_secs}"))
            .arg(out)
            .output()
            .expect("ffmpeg is available; synthesizing the test fixture must not fail to spawn");
        assert!(
            synth.status.success(),
            "synthesizing the sine fixture failed: {}",
            String::from_utf8_lossy(&synth.stderr)
        );
    }

    #[test]
    fn extract_waveform_peaks_extracts_a_real_non_degenerate_envelope_when_ffmpeg_is_available() {
        if !ffmpeg_available() {
            eprintln!(
                "skipping extract_waveform_peaks real-render test: no ffmpeg binary available in this \
                 environment"
            );
            return;
        }

        let d = scratch("real-sine");
        let clip = d.join("sine.wav");
        let ffmpeg = resolve_ffmpeg_bin();
        synth_sine(&ffmpeg, &clip, 1);

        let peaks = extract_waveform_peaks(&clip, 64).expect("ffmpeg is available; extraction must succeed");
        assert_eq!(peaks.len(), 64, "must return exactly the requested bucket count");

        // A 440 Hz sine at full ffmpeg-default amplitude must swing well away from zero somewhere in the
        // envelope — a degenerate all-(0,0) result would mean decoding silently produced nothing.
        let max_abs = peaks.iter().fold(0.0f32, |acc, &(lo, hi)| acc.max(lo.abs()).max(hi.abs()));
        assert!(max_abs > 0.1, "sine envelope must be non-degenerate, got max abs amplitude {max_abs}");
        // Sanity range: a normalized f32 PCM sample should never be wildly outside [-1, 1].
        assert!(max_abs <= 2.0, "sine envelope amplitude implausibly large: {max_abs}");

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_waveform_peaks_downsamples_a_longer_source_to_the_same_bucket_count() {
        if !ffmpeg_available() {
            eprintln!(
                "skipping extract_waveform_peaks downsample test: no ffmpeg binary available in this \
                 environment"
            );
            return;
        }

        let d = scratch("downsample");
        let ffmpeg = resolve_ffmpeg_bin();

        let short_clip = d.join("short.wav");
        synth_sine(&ffmpeg, &short_clip, 1);
        let long_clip = d.join("long.wav");
        synth_sine(&ffmpeg, &long_clip, 4);

        let short_peaks = extract_waveform_peaks(&short_clip, 40).expect("short clip extraction must succeed");
        let long_peaks = extract_waveform_peaks(&long_clip, 40).expect("long clip extraction must succeed");

        assert_eq!(short_peaks.len(), 40);
        assert_eq!(long_peaks.len(), 40, "a 4x longer source must downsample to the SAME bucket count");

        let _ = fs::remove_dir_all(&d);
    }
}
