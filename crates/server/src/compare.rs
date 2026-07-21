//! File comparison (CPE-418, epic CPE-722). Pure and Tauri-free (CPE-815); the Tauri `files_identical`
//! command dispatches here.

use std::fs;
use std::path::Path;

/// Whether two files have identical content. Different sizes short-circuit to `false`; otherwise the
/// bytes are streamed and compared with an early exit on the first difference — cheaper and
/// collision-free versus hashing both. A directory or unreadable path is an `Err`, never a panic.
pub fn files_identical(a: &str, b: &str) -> Result<bool, String> {
    use std::io::Read;
    let (pa, pb) = (Path::new(a), Path::new(b));
    let (ma, mb) = (
        fs::metadata(pa).map_err(|e| format!("{a}: {e}"))?,
        fs::metadata(pb).map_err(|e| format!("{b}: {e}"))?,
    );
    if ma.is_dir() || mb.is_dir() {
        return Err("folders can't be compared".into());
    }
    if ma.len() != mb.len() {
        return Ok(false); // different size ⇒ different content, no need to read
    }
    let mut fa = fs::File::open(pa).map_err(|e| format!("{a}: {e}"))?;
    let mut fb = fs::File::open(pb).map_err(|e| format!("{b}: {e}"))?;
    let (mut ba, mut bb) = ([0u8; 64 * 1024], [0u8; 64 * 1024]);
    loop {
        let na = fa.read(&mut ba).map_err(|e| format!("{a}: {e}"))?;
        // Same length overall, so read the same count from b (loop until filled or EOF).
        let mut nb = 0;
        while nb < na {
            let r = fb.read(&mut bb[nb..na]).map_err(|e| format!("{b}: {e}"))?;
            if r == 0 {
                break;
            }
            nb += r;
        }
        if na != nb || ba[..na] != bb[..nb] {
            return Ok(false);
        }
        if na == 0 {
            return Ok(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_identical_compares_content_and_short_circuits_on_size() {
        let d = std::env::temp_dir().join(format!("cpe-compare-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let p = |n: &str| d.join(n).to_string_lossy().to_string();
        fs::write(d.join("a"), b"hello world").unwrap();
        fs::write(d.join("b"), b"hello world").unwrap(); // identical
        fs::write(d.join("c"), b"hello worlD").unwrap(); // same size, differing byte
        fs::write(d.join("e"), b"hello").unwrap(); // different size
        assert_eq!(files_identical(&p("a"), &p("b")), Ok(true));
        assert_eq!(files_identical(&p("a"), &p("c")), Ok(false));
        assert_eq!(files_identical(&p("a"), &p("e")), Ok(false));
        // Two empty files are identical.
        fs::write(d.join("z1"), b"").unwrap();
        fs::write(d.join("z2"), b"").unwrap();
        assert_eq!(files_identical(&p("z1"), &p("z2")), Ok(true));
        // A folder or a missing path is an error.
        assert!(files_identical(&p("a"), &d.to_string_lossy()).is_err());
        assert!(files_identical(&p("a"), &p("nope")).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
