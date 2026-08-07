//! Small shared filesystem utilities used across the Server's domain logic (CPE-815): epoch-ms time
//! conversion and streaming SHA-256 hashing. Pure and Tauri-free; re-exported into the app so its
//! many call sites resolve unchanged.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Convert a `SystemTime` into epoch milliseconds, if representable.
pub fn to_epoch_ms(t: SystemTime) -> Option<u64> {
    t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// Render Unix-epoch seconds as an RFC 3339 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`) — hand-rolled since
/// this crate carries no `chrono`/`time` dependency. Shared by [`crate::jwt_preview`] (`exp`/`iat`/`nbf`
/// claims, CPE-1418) and [`crate::cert_decode`] (`notBefore`/`notAfter`, CPE-1419) so both humanize
/// timestamps identically instead of each hand-rolling their own copy.
pub fn unix_to_rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Days-since-1970-01-01 -> (year, month, day). Howard Hinnant's `civil_from_days`
/// (<https://howardhinnant.github.io/date_algorithms.html>), valid for the entire representable `i64`
/// range with no overflow (it stays within `i64` arithmetic throughout).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

/// Whether a directory entry is a symlink (without following it). Used to avoid symlink cycles in the
/// recursive walks (CPE-609/611).
pub fn entry_is_symlink(entry: &std::fs::DirEntry) -> bool {
    entry.file_type().map(|t| t.is_symlink()).unwrap_or(false)
}

/// Stream a file through SHA-256 and return the lowercase hex digest. Shared by `hash_file` (CPE-412),
/// the folder checksum baseline (CPE-791), and the backup verifier. 64 KiB chunks — a multi-GB file
/// never loads into memory.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    // Lowercase hex — one dependency fewer than pulling in `hex` for three lines.
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_ms_of_unix_epoch_is_zero() {
        assert_eq!(to_epoch_ms(UNIX_EPOCH), Some(0));
    }

    #[test]
    fn epoch_ms_is_monotonic_for_later_times() {
        use std::time::Duration;
        let later = UNIX_EPOCH + Duration::from_millis(1_500);
        assert_eq!(to_epoch_ms(later), Some(1_500));
    }

    #[test]
    fn unix_to_rfc3339_matches_known_dates() {
        assert_eq!(unix_to_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(unix_to_rfc3339(-1), "1969-12-31T23:59:59Z");
        assert_eq!(unix_to_rfc3339(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn sha256_matches_known_vector() {
        // SHA-256("hello") — a fixed vector so the hex formatting is pinned.
        let dir = std::env::temp_dir().join(format!("cpe-fsutil-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("h.txt");
        std::fs::write(&f, b"hello").unwrap();
        assert_eq!(
            sha256_file(&f).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
