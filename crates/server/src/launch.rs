//! Launch-option resolution from CLI args (CPE-1043). Pure helpers over the raw argument strings; the app
//! adapter supplies the actual filesystem check, so this stays filesystem-free and unit-testable.

use std::path::{Path, PathBuf};

/// Resolve the `--open <dir>` launch argument to a directory to open on startup.
///
/// `raw` is the raw argument value (`None` when the flag wasn't passed). `is_dir` reports whether a path is
/// an existing directory — injected so this function does no I/O and can be tested deterministically.
/// Returns the trimmed path **only** when it names an existing directory; an empty/whitespace value, a
/// missing flag, or a path that isn't a directory yields `None` (the caller falls back to normal startup).
pub fn resolve_open_dir(raw: Option<&str>, is_dir: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    is_dir(&path).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_or_blank_yields_none() {
        assert_eq!(resolve_open_dir(None, |_| true), None);
        assert_eq!(resolve_open_dir(Some(""), |_| true), None);
        assert_eq!(resolve_open_dir(Some("   "), |_| true), None);
    }

    #[test]
    fn an_existing_directory_is_returned_trimmed() {
        let got = resolve_open_dir(Some("  /some/dir  "), |p| p == Path::new("/some/dir"));
        assert_eq!(got, Some(PathBuf::from("/some/dir")));
    }

    #[test]
    fn a_non_directory_yields_none() {
        // A path that exists as a file, or doesn't exist at all — the predicate says "not a directory".
        assert_eq!(resolve_open_dir(Some("/not/a/dir"), |_| false), None);
    }

    #[test]
    fn the_predicate_receives_the_exact_trimmed_path() {
        let seen = std::cell::RefCell::new(None);
        let _ = resolve_open_dir(Some(" C:/data "), |p| {
            *seen.borrow_mut() = Some(p.to_path_buf());
            true
        });
        assert_eq!(seen.into_inner(), Some(PathBuf::from("C:/data")));
    }
}
