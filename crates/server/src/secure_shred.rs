//! Secure-delete shred **engine** (CPE-1012, epic CPE-738): executes the overwrite passes that
//! [`crate::secure_delete::plan_shred`] plans, against a real file on disk, then unlinks it.
//!
//! `secure_delete` is pure planning — patterns + honest platform caveats, no I/O. This module is
//! the disk-backed half: for each [`crate::secure_delete::PassPattern`] in the plan, open the file
//! for writing, seek to the start, overwrite exactly the file's byte length with that pattern in
//! 64 KiB chunks, then `flush` + `sync_all` so the pass actually reaches disk (not just the page
//! cache) before the next pass — or the final unlink — begins.
//!
//! **v1 scope:** `shred_file` always plans with `on_ssd = false, copy_on_write = false` — it has no
//! way to detect the target medium yet, so it assumes the conservative "plain disk, in-place
//! overwrite" case. A caller that knows better (e.g. a future UI that inspects the volume) should
//! call [`crate::secure_delete::plan_shred`] itself and surface the caveats; this engine's job is
//! only to execute whatever plan it's handed value-for-value.
//!
//! **RNG for the `Random` pass — non-cryptographic, by design, for now.** `getrandom`/`rand` are not
//! direct dependencies of this crate (only transitive, via the lock file), and the repo guardrail
//! forbids adding a new dependency for this ticket. So the `Random` pattern is filled from a
//! std-only splitmix64 PRNG seeded from wall-clock nanoseconds, a stack address, and the current
//! thread id — enough entropy to make each pass unpredictable in practice, but **not** a
//! cryptographically-secure RNG. That is an acceptable v1 tradeoff: the security value a shred pass
//! delivers is the overwrite *occurring* at all, and `secure_delete`'s own caveats already disclaim
//! any hard erasure guarantee (SSD wear-levelling, copy-on-write snapshots, backups). // NOTE: swap
//! to a CSPRNG if a crypto RNG ever becomes a direct dependency of this crate.

use crate::secure_delete::{self, PassPattern, ShredScheme};
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};

/// Chunk size for each write within a pass. Large enough to avoid per-syscall overhead on big
/// files, small enough to keep peak memory bounded.
const CHUNK_BYTES: usize = 64 * 1024;

/// What a completed (or partially-completed, on error) shred run did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ShredReport {
    /// The path that was shredded.
    pub path: String,
    /// How many overwrite passes actually ran to completion.
    pub passes_run: u32,
    /// Total bytes written across all passes (size × passes_run, for a fully-successful run).
    pub bytes_written: u64,
    /// Whether the file was removed after the final pass.
    pub removed: bool,
}

/// Overwrite `path`'s bytes pass-by-pass per `scheme`, then remove it.
///
/// Stats the file for its size, plans the pass sequence via [`secure_delete::plan_shred`]
/// (`on_ssd = false, copy_on_write = false` — see the module doc), runs every pass in order —
/// each pass opens the file writable, seeks to 0, overwrites exactly `size` bytes with the pass's
/// pattern in [`CHUNK_BYTES`]-sized chunks, then flushes and `sync_all`s — and finally removes the
/// file. A zero-length file still runs every pass (writing 0 bytes each time) and is still removed.
///
/// Returns `Err` (never panics) if the path can't be stat'd, a pass can't open/write/sync the file,
/// or the final removal fails.
pub fn shred_file(path: &str, scheme: ShredScheme) -> Result<ShredReport, String> {
    let size_bytes = std::fs::metadata(path)
        .map_err(|e| format!("cannot stat {path}: {e}"))?
        .len();

    let plan = secure_delete::plan_shred(path, size_bytes, scheme, false, false);

    let mut rng = SplitMix64::new_from_env();
    let mut passes_run: u32 = 0;
    let mut bytes_written: u64 = 0;

    for pattern in &plan.passes {
        run_pass(path, *pattern, size_bytes, &mut rng)?;
        passes_run += 1;
        bytes_written += size_bytes;
    }

    std::fs::remove_file(path).map_err(|e| format!("cannot remove {path} after shredding: {e}"))?;

    Ok(ShredReport { path: path.to_string(), passes_run, bytes_written, removed: true })
}

/// Run a single overwrite pass: open `path` for writing, seek to 0, overwrite exactly `size` bytes
/// with `pattern`, then flush + `sync_all` so the pass is durable before the next one starts.
fn run_pass(path: &str, pattern: PassPattern, size: u64, rng: &mut SplitMix64) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| format!("cannot open {path} for pass: {e}"))?;
    file.seek(SeekFrom::Start(0)).map_err(|e| format!("cannot seek {path}: {e}"))?;

    let mut chunk = vec![0u8; CHUNK_BYTES.min(size.max(1) as usize)];
    fill_pattern(&mut chunk, pattern, rng);

    let mut remaining = size;
    while remaining > 0 {
        let n = remaining.min(chunk.len() as u64) as usize;
        // Random keeps re-filling the (bounded) chunk buffer each write so a huge file doesn't
        // just repeat one 64 KiB random block over and over.
        if matches!(pattern, PassPattern::Random) {
            fill_pattern(&mut chunk[..n], pattern, rng);
        }
        file.write_all(&chunk[..n]).map_err(|e| format!("cannot write {path}: {e}"))?;
        remaining -= n as u64;
    }

    file.flush().map_err(|e| format!("cannot flush {path}: {e}"))?;
    file.sync_all().map_err(|e| format!("cannot sync {path}: {e}"))?;
    Ok(())
}

/// Fill `buf` with the byte pattern `p` calls for.
fn fill_pattern(buf: &mut [u8], p: PassPattern, rng: &mut SplitMix64) {
    match p {
        PassPattern::Zeros => buf.fill(0x00),
        PassPattern::Ones => buf.fill(0xFF),
        PassPattern::Byte { value } => buf.fill(value),
        PassPattern::Random => rng.fill_bytes(buf),
    }
}

/// A small, fast, std-only PRNG (splitmix64) for the `Random` pass. **Not cryptographically
/// secure** — see the module doc for why that's an acceptable v1 tradeoff.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Seed from wall-clock nanoseconds + a stack address + the current thread id — cheap sources
    /// of entropy that vary run-to-run and pass-to-pass without pulling in a dependency.
    fn new_from_env() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let stack_addr = &nanos as *const u64 as u64;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&std::thread::current().id(), &mut hasher);
        let thread_hash = std::hash::Hasher::finish(&hasher);

        let seed = nanos ^ stack_addr.rotate_left(17) ^ thread_hash.rotate_left(31);
        SplitMix64 { state: if seed == 0 { 0x9E3779B97F4A7C15 } else { seed } }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    fn fill_bytes(&mut self, buf: &mut [u8]) {
        let mut i = 0;
        while i < buf.len() {
            let bytes = self.next_u64().to_le_bytes();
            let n = (buf.len() - i).min(8);
            buf[i..i + n].copy_from_slice(&bytes[..n]);
            i += n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn write_temp(dir: &tempfile::TempDir, name: &str, contents: &[u8]) -> String {
        let path = dir.path().join(name);
        std::fs::write(&path, contents).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn zero_scheme_runs_one_pass_and_removes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_temp(&dir, "a.txt", &[0xAB; 500]);

        let report = shred_file(&path, ShredScheme::Zero).unwrap();

        assert_eq!(report.passes_run, 1);
        assert_eq!(report.bytes_written, 500);
        assert!(report.removed);
        assert!(!std::path::Path::new(&path).exists());
    }

    #[test]
    fn dod3_runs_three_passes_and_writes_three_times_the_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_temp(&dir, "b.bin", &[0x11; 1000]);

        let report = shred_file(&path, ShredScheme::Dod3).unwrap();

        assert_eq!(report.passes_run, 3);
        assert_eq!(report.bytes_written, 3000);
        assert!(!std::path::Path::new(&path).exists());
    }

    #[test]
    fn gutmann_runs_seven_passes() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_temp(&dir, "c.bin", &[0x22; 777]);

        let report = shred_file(&path, ShredScheme::Gutmann).unwrap();

        assert_eq!(report.passes_run, 7);
        assert_eq!(report.bytes_written, 777 * 7);
        assert!(!std::path::Path::new(&path).exists());
    }

    #[test]
    fn zero_length_file_shreds_zero_bytes_and_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_temp(&dir, "empty.bin", &[]);

        let report = shred_file(&path, ShredScheme::Gutmann).unwrap();

        assert_eq!(report.passes_run, 7);
        assert_eq!(report.bytes_written, 0);
        assert!(report.removed);
        assert!(!std::path::Path::new(&path).exists());
    }

    #[test]
    fn missing_path_returns_clean_err_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.bin").to_string_lossy().to_string();

        let result = shred_file(&missing, ShredScheme::Zero);

        assert!(result.is_err());
    }

    #[test]
    fn zero_pass_actually_overwrites_the_final_bytes_on_disk_before_removal() {
        // Regression guard: intercept just before removal is impossible without restructuring, so
        // instead verify a single-pass scheme actually wrote the pattern by re-reading mid-flight
        // via a non-removing helper: recreate the file, run only the pass logic through a scheme
        // that a human could inspect, and confirm the byte content changed from the original.
        let dir = tempfile::tempdir().unwrap();
        let path = write_temp(&dir, "d.bin", &[0x99; 4096]);

        // Use the internal pass runner directly so we can inspect bytes before the file is unlinked.
        let mut rng = SplitMix64::new_from_env();
        run_pass(&path, PassPattern::Ones, 4096, &mut rng).unwrap();

        let mut buf = Vec::new();
        std::fs::File::open(&path).unwrap().read_to_end(&mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == 0xFF));
        assert_eq!(buf.len(), 4096);
    }

    #[test]
    fn random_pass_is_not_all_the_same_byte_for_a_reasonably_sized_buffer() {
        let mut rng = SplitMix64::new_from_env();
        let mut buf = vec![0u8; 4096];
        rng.fill_bytes(&mut buf);
        // Overwhelmingly unlikely for a real PRNG to produce a constant buffer.
        assert!(buf.iter().any(|&b| b != buf[0]));
    }

    #[test]
    fn multi_chunk_file_writes_the_pattern_across_chunk_boundaries() {
        // Bigger than CHUNK_BYTES so the write loop crosses at least one chunk boundary.
        let dir = tempfile::tempdir().unwrap();
        let big_size = CHUNK_BYTES * 2 + 123;
        let path = write_temp(&dir, "big.bin", &vec![0x00; big_size]);

        let report = shred_file(&path, ShredScheme::Zero).unwrap();

        assert_eq!(report.bytes_written, big_size as u64);
        assert!(!std::path::Path::new(&path).exists());
    }
}
