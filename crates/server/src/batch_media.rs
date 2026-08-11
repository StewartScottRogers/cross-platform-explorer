//! Batch media operation planner (CPE-940, epic CPE-723): given a set of **non-destructive-by-default**
//! media transforms (resize / convert / rotate / flip / rename / strip-metadata) and a selection of input
//! files, compute the concrete per-file **output path** each will be written to — **collision-safe** — plus
//! a short human summary of the ops applied. No image work here (byte transforms live in
//! `batch_transform`) — but, as of CPE-1613/CPE-1623, `plan()` is **not** filesystem-free either: it
//! canonicalizes paths to decide "same file?" and stats candidate outputs to decide "already occupied?".
//! The transform engine (`batch_execute`) executes the returned plan.
//!
//! **`plan()` is non-destructive only within the folder the caller selected — not an absolute claim
//! (CPE-1623).** Before this fix, an independent security audit demonstrated that a `Rename` template
//! containing `..`/path separators (`"..\\..\\elsewhere\\name"`) made `plan()` compute an output
//! CANONICALIZING OUTSIDE the input's own directory, with no error and no refusal, and `execute_plan`
//! then silently overwrote an unrelated file there — even with `non_destructive: true`, the mode this
//! module's own former wording called safe. `plan()` now refuses the whole batch ([`Err`]) when a
//! computed output would leave the input's own directory (`validate()` also rejects the template up
//! front, so this is the engine's own backstop, not just the one UI's field-level check) — see `plan()`'s
//! own doc for the containment check (shared with `validate()`'s field-level echo AND
//! `batch_execute::execute_plan_walk`'s pre-write re-check, via [`output_escapes_input_dir`]) and the
//! broadened real-filesystem collision guard that closes the rest of the gap. `validate()` sanitises
//! **both** `Rename.template` and `Convert.to_ext` — an earlier cut of this fix checked only the former,
//! which `plan()`'s own backstop still caught (never exploitable) but left no early field-level warning
//! for a malicious/malformed Convert extension, and mis-blamed "rename template" in the error either way.
//!
//! **`plan()`/`validate()` do NOT reach the IPC boundary (follow-up to CPE-1623).** The containment
//! checks above only ever ran inside this module — but `batch_media_execute_stream`'s Tauri command took
//! `items: Vec<PlannedItem>` straight from the caller, and `PlannedItem` is a plain public struct with no
//! invariants of its own (`Serialize`/`Deserialize`, nothing enforcing "came from `plan()`"). A caller
//! that skips `plan()` entirely — devtools, a compromised webview, a future automation surface — could
//! hand-build a `PlannedItem` pointing `output` at any absolute path the process can write, and
//! `batch_execute::execute_plan_walk`'s only gate (`is_foreign_overwrite`) asked "does something already
//! exist there?", never "does this stay inside the input's folder?" — so a **new** file at an arbitrary
//! location sailed straight through, `confirmed_overwrite` or not. `execute_plan_walk` now re-derives
//! containment itself, per item, before any byte is read or written, using the identical
//! [`output_escapes_input_dir`] check `plan()` uses — see that module's doc for the refusal. The engine is
//! now the actual enforcement point end-to-end, not just this one module's own two entry points.
//!
//! **`..` is a traversal risk only as a whole path segment, not any occurrence (UAT follow-up).** The
//! first cut of the template/extension check rejected `..` as a bare substring, which also rejected
//! ordinary filenames like `"shot..final"` or a version stamp `"v1..2"` that can never leave the input's
//! directory (no separator anywhere ⇒ nothing to walk through). See
//! [`template_escapes_directory`]'s doc for the corrected rule.
//!
//! **Unicode look-alike separators (audited, resolved — not path separators here).** A rename template
//! containing U+2215 (DIVISION SLASH), U+FF0F (FULLWIDTH SOLIDUS), or U+FF3C (FULLWIDTH REVERSE SOLIDUS)
//! is **accepted** by `validate()`/`template_escapes_directory`: those are distinct Unicode scalars from
//! ASCII `/`/`\`, so the `char`-based `contains` check correctly does not match them, and neither
//! `split`/`join` (which only ever split on literal `/`/`\`) nor the OS treat them as directory
//! separators — a template containing one produces an output that is still a single path component inside
//! the input's own directory, confirmed by a real-file test in this module (`cpe_1623_unicode_lookalike_...`).
//! They are ordinary, legal filename characters on NTFS/APFS/ext4 (none of the 9 characters NTFS reserves
//! — `< > : " / \ | ? *` — matches any of the three), so accepting them is correct, not a gap.
//!
//! **Two structural bypasses of the directory-identity check itself (reviewer, PR #828 attempt 2).**
//! [`split`] is a plain filename splitter, not a full path parser, and the containment comparison used to
//! trust it too far: (A) a **bare-filename input** (`split` finds no directory component, so `dir == ""`)
//! let a Windows drive-relative template like `"C:foo"` through, because the computed `out_dir` was
//! *also* textually empty — two empty strings compare equal, no escape detected, even though `C:foo.jpg`
//! resolves against drive `C:`'s own current directory, not the folder the user picked; (B) an
//! **extensionless input** plus a template that's literally `".."` produced an `output` whose FINAL path
//! component is a bare dot-segment, which `split` hands back as an ordinary "stem" — `out_dir == dir`
//! skips the check entirely even though the output denotes a directory, not a file. Both are now guarded
//! explicitly in [`output_escapes_input_dir`] itself (not just `validate()`'s template-level `:` rejection
//! above, which only covers the one production call path) — see that fn's doc for the exact conditions.
//!
//! **Link-as-final-component defeats the directory-identity comparison itself (reviewer, PR #828 attempt
//! 3).** `out_dir == dir` (the fast path immediately above, taken when the two directories are textually
//! identical) trusts the path STRING, never what `output`'s final component actually IS on disk. A link
//! whose *name* sits inside the input's own directory can still alias data physically outside it: (1) a
//! **hard link** — `fs::hard_link(outside/important.jpg, selected/link.jpg)`, then a `PlannedItem` whose
//! `output` is `selected/link.jpg` — passed with `escapes = false` purely because the directory text
//! matched, and `execute_plan_walk` then happily wrote through it, mutating `outside/important.jpg`'s
//! actual bytes; demonstrated on Windows even with `confirmed_overwrite: true`, falsifying the earlier
//! claim that flag can't license an out-of-folder write. (2) A **dangling symlink** —
//! `create_symlink(target=outside/newly-planted.jpg /* doesn't exist yet */, link=selected/link.jpg)` —
//! is worse: `Path::is_file()` on a dangling symlink is `false`, so [`crate::batch_execute`]'s
//! `is_foreign_overwrite` sees "nothing there" and `confirmed_overwrite` never even needs to be set. Both
//! shapes are reachable off the IPC bypass AND (a rename template producing the stem `link`) an ordinary
//! UI-driven batch, since `plan()`'s own non-destructive collision check uses the same `Path::is_file()`
//! blind spot. [`output_escapes_input_dir`] now resolves the final component before trusting `out_dir ==
//! dir` — see [`link_alias_escapes`] for exactly what each link shape does and does not close, in
//! particular why a hard link's OTHER name is fundamentally unobservable without a disproportionate
//! full-volume walk, and why that fails closed instead of guessing.
//!
//! **Same-file detection (CPE-1613).** "Does output overwrite input?" must NOT be decided by raw string
//! equality: `plan()` itself lower-cases a `Convert` target's extension, so `IMG_1.JPG` + Convert→jpg
//! yields the string `"IMG_1.jpg"` — different text, but the SAME on-disk file on a case-insensitive
//! filesystem (Windows, default macOS). [`same_file`] is the one shared definition of "same file" used
//! by BOTH (1) this module's non-destructive "output must differ from input" guarantee + collision set
//! below, and (2) [`crate::batch_execute::any_in_place`]'s `confirmed_overwrite` refusal check — fixing
//! one and not the other just moves the hole, per the ticket.
//!
//! **Collision-set performance (CPE-1613 follow-up).** An earlier version of this fix kept the collision
//! set as a `Vec<String>` scanned pairwise with `same_file` — O(n) per item, O(n²) per batch in one
//! folder, each non-trivial `same_file` call issuing 1-2 *uncached* `std::fs::canonicalize` syscalls. On
//! 2000 files in one directory that measured ~718s in a release build. [`path_key`] replaces the pairwise
//! scan: it maps a path to a normalized identity key (canonicalized parent + case-folded final
//! component, memoizing the parent canonicalization in a `HashMap` since a batch overwhelmingly shares
//! one parent directory), and `plan()`'s collision set is a `HashSet<PathKey>` — O(1) lookup, O(n)
//! overall. [`same_file`] is now defined in terms of the same `path_key`, so there is still exactly one
//! notion of "same file" backing both call sites.

/// One media transform in a batch. Order matters (ops apply left-to-right).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum MediaOp {
    /// Downscale so the longest side is at most `max_px` (never upscales — engine's job).
    Resize { max_px: u32 },
    /// Re-encode to a different container/format (changes the output extension).
    Convert { to_ext: String },
    /// Rotate clockwise; only 90 / 180 / 270 are valid.
    Rotate { degrees: u16 },
    /// Mirror horizontally (`true`) or vertically (`false`).
    Flip { horizontal: bool },
    /// Rename the stem from a template — tokens `{stem}` `{n}` (1-based index) `{ext}`.
    Rename { template: String },
    /// Drop all embedded metadata (EXIF/IPTC/XMP).
    StripMetadata,
    /// Re-encode at a target quality (1-100) to shrink file size. Affects JPEG targets only;
    /// formats without a lossy quality knob (png/gif/bmp/tif, and this crate's lossless-only WebP
    /// encoder) accept it as a graceful no-op.
    Compress { quality: u8 },
    /// Alpha-composite an overlay image (logo/stamp) onto each image at a corner + opacity.
    /// **Optional by construction**: an empty `image` path means "no watermark" — the op then
    /// contributes nothing (no summary line, no bytes touched). `opacity` is 0-100.
    Watermark {
        image: String,
        #[serde(default)]
        position: Corner,
        opacity: u8,
    },
}

/// Where a [`MediaOp::Watermark`] overlay is anchored on the base image. Default `BottomRight`
/// matches the common "small logo in the corner" placement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
    Center,
}

impl Corner {
    /// A short lowercase-snake token for plan summaries — mirrors the serde wire form so the
    /// preview text and the JSON payload agree on how a corner reads.
    pub fn as_str(self) -> &'static str {
        match self {
            Corner::TopLeft => "top_left",
            Corner::TopRight => "top_right",
            Corner::BottomLeft => "bottom_left",
            Corner::BottomRight => "bottom_right",
            Corner::Center => "center",
        }
    }
}

/// A batch job: the ordered ops + whether to write to new files (default) or overwrite in place.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BatchJob {
    pub ops: Vec<MediaOp>,
    /// When true (the default/safe mode) outputs never overwrite an input — a suffix is added so the
    /// output name differs, and same-target collisions are disambiguated.
    pub non_destructive: bool,
    /// **Defence in depth (CPE-1599).** Explicit "yes, I understand this overwrites originals in place"
    /// flag, checked by [`crate::batch_execute::execute_plan_walk`] before it will run a plan containing
    /// any item whose planned `output == input`. Defaults to `false` via [`BatchJob::new`] — a caller
    /// must deliberately opt in. This is **not** meant to be flipped anywhere in the codebase except the
    /// batch-media confirm panel (`BatchMediaDialog.svelte`'s "Overwrite N files" button, after the user
    /// has read the danger-styled confirmation) once it has actually shown that confirmation; that is a
    /// frontend-side promise this field cannot itself enforce, but the engine no longer trusts the
    /// caller's word for it either way — `non_destructive: false` alone is no longer sufficient to make
    /// `execute_plan_walk` touch an input file in place. See the module's `batch_execute` doc for the
    /// refusal this guards.
    #[serde(default)]
    pub confirmed_overwrite: bool,
}

impl BatchJob {
    pub fn new(ops: Vec<MediaOp>) -> Self {
        Self { ops, non_destructive: true, confirmed_overwrite: false }
    }
}

/// One planned output: where `input` will be written and a one-line summary of what happens to it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlannedItem {
    pub input: String,
    pub output: String,
    pub summary: String,
}

/// True when `template` could move a [`MediaOp::Rename`]'s (or, as of the Convert-extension fix below,
/// a [`MediaOp::Convert`]'s `to_ext`) computed output into a different directory than its input, or off
/// the volume the input's own directory names entirely: a path separator (`/` or `\`) anywhere, a `:`
/// anywhere, or a **whole-segment** `..` traversal (CPE-1623; narrowed by a UAT follow-up — see below).
/// The template only ever substitutes into the file's STEM (or, for Convert, the extension) — see
/// `plan()`'s `Rename`/`Convert` arms, which run the result straight through [`join`] alongside the
/// input's own unchanged directory — so it has no legitimate reason to name a directory, let alone walk
/// out of one, at all.
///
/// A literal separator (or `:`) is checked as a plain substring deliberately (not "is this a whole path
/// segment"): `template.replace("{stem}", ...)` means the attacker doesn't need it to occupy a whole
/// segment on its own (see the ticket's worked example, `"..\\..\\cpe1613_traversal_victim\\important"`,
/// which has no `{stem}`/`{n}`/`{ext}` token at all and is used completely literally) — and a filename
/// can never legitimately contain a raw `/`, `\`, or `:` at all on any mainstream filesystem (all three
/// are reserved on NTFS; `:` doubles as the drive-letter separator on Windows), so there is no
/// false-positive risk in flagging any of them anywhere they appear.
///
/// **`:` (reviewer finding, PR #828 attempt 2).** A template like `"C:foo"` contains none of the three
/// original characters this fn checked — only `/`, `\`, `..` were rejected — so it passed straight
/// through both here and [`output_escapes_input_dir`]'s directory-identity comparison for a
/// **bare-filename input** (no directory component at all, so both `dir` and the computed `out_dir` are
/// textually empty and compare equal): a classic Windows drive-relative reference, which resolves against
/// drive `C:`'s *current directory* at write time — not the folder the user picked. Rejecting `:` outright
/// closes it at this field-level layer; [`output_escapes_input_dir`] carries a second, independent guard
/// for the same case (see its doc) so a caller that skips this check entirely (bypassing `validate()`)
/// is still caught.
///
/// **`..` is different (UAT follow-up to CPE-1623).** The very first cut of this check flagged `..`
/// as a **substring** — `template.contains("..")` — which rejected perfectly ordinary filenames like
/// `"shot..final"` or a version stamp `"v1..2"` that contain the two characters but can never walk
/// anywhere: with no separator present at all, the whole template is exactly ONE path segment, so `..`
/// is only a traversal risk when it occupies that entire segment. Once any separator/`:` has already
/// failed the check above and returned `true`, there's nothing further to decide here — so by the time
/// this line runs, `template` is guaranteed to contain no `/`, `\`, or `:`, and "is `..` a whole segment"
/// reduces to "is the (trimmed) template exactly `..`". This stays exactly as strict for every case the
/// module's own tests already pinned (`".."`, `"../evil"`, `"..\\evil"`, `"a/../b"` all still contain a
/// separator and are still rejected above) while accepting the two the auditor's own worked examples name.
fn template_escapes_directory(template: &str) -> bool {
    if template.contains('/') || template.contains('\\') || template.contains(':') {
        return true;
    }
    template.trim() == ".."
}

/// Reject a job that can't be executed: no ops, a bad rotation angle, an empty convert extension, an
/// empty rename template, or (CPE-1623) a rename template OR convert extension that could walk the
/// output outside the folder the user picked.
pub fn validate(job: &BatchJob) -> Result<(), String> {
    if job.ops.is_empty() {
        return Err("a batch job needs at least one operation".into());
    }
    for op in &job.ops {
        match op {
            MediaOp::Rotate { degrees } if !matches!(degrees, 90 | 180 | 270) => {
                return Err(format!("rotate must be 90, 180 or 270 degrees (got {degrees})"));
            }
            MediaOp::Convert { to_ext } if to_ext.trim().is_empty() => {
                return Err("convert needs a target extension".into());
            }
            // CPE-1623 follow-up: `validate()` used to sanitise ONLY `Rename.template`, even though
            // `Convert.to_ext` feeds the exact same joined output path (`plan()`'s Convert arm sets
            // `ext` to `to_ext`'s sanitised-but-not-separator-checked value, which `join()` then
            // concatenates straight into the output string). `plan()`'s own containment backstop still
            // caught an escaping `to_ext` — this was never exploitable — but the field-level check gave
            // no early warning for this op, and a caller relying on `validate()` alone (skipping `plan()`)
            // wasn't protected at all. Same rule, same helper, as Rename's template.
            MediaOp::Convert { to_ext } if template_escapes_directory(to_ext) => {
                return Err(format!(
                    "convert extension \"{to_ext}\" can't contain \\, /, or \"..\" — it can only \
                     change the file's extension, not its folder"
                ));
            }
            MediaOp::Resize { max_px } if *max_px == 0 => {
                return Err("resize max_px must be > 0".into());
            }
            MediaOp::Rename { template } if template.trim().is_empty() => {
                return Err("rename needs a non-empty template".into());
            }
            MediaOp::Rename { template } if template_escapes_directory(template) => {
                return Err(format!(
                    "rename template \"{template}\" can't contain \\, /, or \"..\" — it can only change \
                     the file's name, not its folder"
                ));
            }
            MediaOp::Compress { quality } if *quality == 0 || *quality > 100 => {
                return Err(format!("compress quality must be 1-100 (got {quality})"));
            }
            MediaOp::Watermark { opacity, .. } if *opacity > 100 => {
                return Err(format!("watermark opacity must be 0-100 (got {opacity})"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Split a path string into (dir_with_trailing_sep, stem, ext_without_dot). Handles `/` and `\`; a
/// leading-dot dotfile (`.env`) is treated as all-stem, no ext.
fn split(path: &str) -> (String, String, String) {
    let sep = path.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    let (dir, name) = path.split_at(sep);
    match name.rfind('.') {
        Some(dot) if dot > 0 => (dir.to_string(), name[..dot].to_string(), name[dot + 1..].to_string()),
        _ => (dir.to_string(), name.to_string(), String::new()),
    }
}

fn join(dir: &str, stem: &str, ext: &str) -> String {
    if ext.is_empty() {
        format!("{dir}{stem}")
    } else {
        format!("{dir}{stem}.{ext}")
    }
}

/// The filename (no directory) of a path, for use in a short human summary — e.g. a watermark
/// overlay's own path is usually long, but the summary only needs `logo.png`.
fn basename(path: &str) -> String {
    let (_, stem, ext) = split(path);
    join("", &stem, &ext)
}

/// Case-fold `s` **only** on the platforms whose default filesystem is case-insensitive (Windows,
/// default macOS/APFS). Never on Linux/other Unix, where folding case would wrongly treat two distinct,
/// real files as one (CPE-1613 explicitly calls this out: don't make the check case-insensitive
/// unconditionally on Linux). This is a platform-default assumption, not a live filesystem probe — a
/// case-sensitive volume mounted on Windows/macOS, or a case-insensitive one (exFAT/vfat) mounted on
/// Linux, is out of scope; see the CPE-1613 work log for the reasoning.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn fold_case(s: &str) -> String {
    s.to_lowercase()
}
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn fold_case(s: &str) -> String {
    s.to_string()
}

/// Split `path` into normalized `/`-joined path components, resolving `.` and lexical `..` segments and
/// dropping empty/duplicate separators — purely textual, no filesystem access. Treats both `/` and `\`
/// as separators (matching [`split`] above) so a Windows-style path normalizes sanely even when the
/// process itself is running on Linux (as CI's Linux leg does for these tests). Preserves a leading root
/// marker (POSIX `/`, or an uppercased `C:/`-style drive prefix) so an absolute and a relative path never
/// lexically collide.
fn lexical_normalize(path: &str) -> String {
    let bytes = path.as_bytes();
    let root: String = if matches!(bytes.first(), Some(b'/') | Some(b'\\')) {
        "/".to_string()
    } else if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        format!("{}:/", (bytes[0] as char).to_ascii_uppercase())
    } else {
        String::new()
    };

    let mut parts: Vec<&str> = Vec::new();
    for raw in path.split(['/', '\\']) {
        match raw {
            "" | "." => continue,
            ".." => {
                if matches!(parts.last(), Some(p) if *p != "..") {
                    parts.pop();
                } else if root.is_empty() {
                    parts.push("..");
                }
                // ".." above an absolute root is lexically discarded — can't go any higher.
            }
            other => parts.push(other),
        }
    }
    format!("{root}{}", parts.join("/"))
}

/// Split `path` into `(parent_dir, final_component)` using the same dual-separator rule as [`split`].
/// `None` for a bare filename with no directory part, or a path ending in a separator (nothing to
/// canonicalize as a "final component").
fn parent_and_name(path: &str) -> Option<(&str, &str)> {
    let sep = path.rfind(['/', '\\'])?;
    let (dir, name) = (&path[..sep], &path[sep + 1..]);
    if dir.is_empty() || name.is_empty() {
        return None;
    }
    Some((dir, name))
}

/// Thin seam over [`std::fs::canonicalize`] so a test can count how many real syscalls a call makes
/// (CPE-1613 perf regression guard) without asserting flaky wall-clock timing. Behaviourally identical to
/// calling `std::fs::canonicalize` directly outside `#[cfg(test)]`.
fn canonicalize_path<P: AsRef<std::path::Path>>(p: P) -> std::io::Result<std::path::PathBuf> {
    #[cfg(test)]
    CANONICALIZE_CALLS.with(|c| c.set(c.get() + 1));
    std::fs::canonicalize(p)
}

#[cfg(test)]
thread_local! {
    /// Per-test-thread call counter for [`canonicalize_path`]. Rust's default test harness runs each
    /// `#[test]` fn on its own thread, so a thread-local (rather than a process-global atomic) keeps
    /// concurrently-running tests from polluting each other's counts.
    static CANONICALIZE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
/// `pub(crate)` (not just this module's own tests): [`crate::batch_execute::execute_plan_walk`] re-derives
/// the identical containment check `plan()` uses, via the shared [`output_escapes_input_dir`] — its own
/// perf regression guard (PR #828 attempt 3's "ALSO" follow-up: `plan()` had a guard, the execute-side copy
/// of the exact same check did not) needs this same counter, from a test in a different file.
#[cfg(test)]
pub(crate) fn reset_canonicalize_call_count() {
    CANONICALIZE_CALLS.with(|c| c.set(0));
}
#[cfg(test)]
pub(crate) fn canonicalize_call_count() -> usize {
    CANONICALIZE_CALLS.with(|c| c.get())
}

/// A normalized "identity" for a path — the key [`same_file`]/`plan()`'s collision set compare instead of
/// scanning pairwise (CPE-1613 perf fix). Two paths are the same file iff [`path_key`] returns an equal
/// key for both; see [`path_key`]'s doc for how each tier is derived. `Resolved`'s `PathBuf` is always a
/// canonical **directory** (never a bare filename), so it hashes/compares cheaply and consistently
/// regardless of which tier produced it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PathKey {
    /// Either the path itself resolved on disk (canonicalized, then split into its parent + final
    /// component — following symlinks/junctions and case-insensitive/NFC-NFD lookups for free), or only
    /// its parent directory resolved and the final component is compared as literal, case-folded text.
    /// Both tiers land in this one variant because splitting an already-fully-resolved canonical path
    /// into (parent, name) loses no information versus comparing the full canonical paths directly, so a
    /// path that fully resolves and a path whose parent-only resolves are still directly comparable.
    Resolved(std::path::PathBuf, String),
    /// Neither the path nor its parent exists on disk (e.g. bare in-memory strings in most unit tests): a
    /// purely lexical, filesystem-free normalization, case-folded per platform.
    Lexical(String),
}

/// Shared type for [`path_key`]'s per-call memoized parent-canonicalize cache — pulled out to a named
/// alias so [`output_escapes_input_dir`] (used by both this module's `plan()` and, as of the IPC-bypass
/// fix, [`crate::batch_execute::execute_plan_walk`]) can be threaded a cache across an entire batch
/// without either caller having to spell out the `HashMap<String, Option<PathBuf>>` type by hand.
pub(crate) type ParentCache = std::collections::HashMap<String, Option<std::path::PathBuf>>;

/// Map `path` to its [`PathKey`] — the one shared "is this the same file?" identity (CPE-1613) backing
/// both `plan()`'s non-destructive guarantee/collision set and [`same_file`] (and, transitively,
/// [`crate::batch_execute::any_in_place`]'s `confirmed_overwrite` refusal check). Strongest signal first,
/// falling back only when a stronger one isn't available:
///
/// 1. **The path resolves on disk:** ask the OS for its canonical identity ([`canonicalize_path`]) and
///    split that into (canonical parent, final component). This resolves symlinks/junctions to their real
///    target, AND — because canonicalize returns the file's own *stored* path/casing rather than the
///    literal input string — folds case-only differences on a case-insensitive filesystem and (on
///    macOS/APFS, which resolves lookups normalization-insensitively) Unicode NFC/NFD differences, for
///    free, with zero per-platform special-casing here. `fold_case` is still applied to the split-off name
///    for consistency with tier 2, though it's a no-op in practice: two on-disk names that differ only by
///    case can't coexist on a case-insensitive filesystem.
/// 2. **The path itself doesn't resolve (the common case for a planned OUTPUT, which usually doesn't
///    exist yet), but its *parent* directory does:** canonicalize the parent (for every path `plan()`
///    produces, that's the input's own already-existing directory, so this almost always succeeds — and
///    is memoized in `parent_cache` since a batch overwhelmingly shares one parent) and pair it with the
///    literal final path component, case-folded per [`fold_case`]'s platform rule. Catches the ticket's
///    worked example (`IMG_1.JPG` vs `IMG_1.jpg`) and trailing-separator/`.`/`..` variants of an existing
///    directory.
/// 3. **Neither the path nor its parent exists on disk** — e.g. a unit test using bare in-memory strings:
///    fall back to a purely lexical, filesystem-free comparison ([`lexical_normalize`]), still case-folded
///    per platform. Doesn't catch symlinks/junctions or macOS NFC/NFD (those need real files to resolve),
///    but does catch case, separator, and `.`/`..` variants.
fn path_key(path: &str, parent_cache: &mut ParentCache) -> PathKey {
    if let Ok(canonical) = canonicalize_path(path) {
        if let (Some(parent), Some(name)) = (canonical.parent(), canonical.file_name()) {
            return PathKey::Resolved(parent.to_path_buf(), fold_case(&name.to_string_lossy()));
        }
        // A canonicalized path with no parent/file_name at all (e.g. a bare drive/volume root) — no
        // sensible (parent, name) split; fall through to the lexical tier below.
    }

    if let Some((dir, name)) = parent_and_name(path) {
        let canon_dir = parent_cache
            .entry(dir.to_string())
            .or_insert_with(|| canonicalize_path(dir).ok())
            .clone();
        if let Some(canon_dir) = canon_dir {
            return PathKey::Resolved(canon_dir, fold_case(name));
        }
    }

    PathKey::Lexical(fold_case(&lexical_normalize(path)))
}

/// **The one shared "is this the same file?" definition (CPE-1613)**, used by both `plan()`'s
/// non-destructive guarantee/collision set and [`crate::batch_execute::any_in_place`]'s
/// `confirmed_overwrite` refusal check. `a == b` is a cheap fast path; otherwise this is just
/// `path_key(a) == path_key(b)` (see [`path_key`]'s doc for the tiered resolution strategy) using a
/// throwaway per-call parent cache — callers comparing many paths against each other in a loop (like
/// `plan()`'s collision set) should compute [`path_key`] directly with a shared cache instead of calling
/// `same_file` pairwise, or the O(1)-per-key win is lost.
pub fn same_file(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let mut cache = std::collections::HashMap::new();
    path_key(a, &mut cache) == path_key(b, &mut cache)
}

/// **The one shared containment check (CPE-1623, broadened by the IPC-bypass follow-up):** true when
/// `output`'s directory is NOT the same place as `input`'s own directory, per [`path_key`] — i.e. this
/// output would leave the folder its input lives in. Used by BOTH `plan()`'s own backstop below (catches
/// a caller that invokes `plan()` directly, bypassing `validate()`) AND
/// [`crate::batch_execute::execute_plan_walk`]'s pre-write re-check (catches a caller that hand-builds a
/// `PlannedItem` and skips `plan()` **entirely** — the IPC surface `batch_media_execute_stream` took
/// `items: Vec<PlannedItem>` straight off the wire with zero invariants of its own, so nothing on that
/// path had ever re-derived containment before this fix; see `batch_execute`'s module doc for the
/// demonstrated bypass). Fails closed: [`path_key`] never panics or silently no-ops on an IO error/missing
/// parent/non-canonicalizable path, it falls back tier-by-tier to a purely lexical, filesystem-free
/// comparison ([`lexical_normalize`]) that still correctly resolves `.`/`..` segments — so an unresolvable
/// path still gets a real containment answer, never a free pass.
///
/// Cheap fast path: textually-identical directories (the overwhelmingly common case — every non-Rename,
/// non-Convert op, and any Rename/Convert value without a separator) never touch [`path_key`] at all; only
/// an `output` whose directory actually differs from `input`'s pays for the (still O(1)-amortized, thanks
/// to `parent_cache`) resolution that confirms whether that's a real escape or just a resolvable variant
/// (trailing separator, `.`/`..`, symlink) of the same place.
///
/// **Two structural gaps closed (reviewer, PR #828 attempt 2) — [`split`] is a plain filename splitter,
/// not a full path parser, and the directory-identity comparison above trusted it too far:**
///
/// - **Finding A — a bare-filename input.** `split` finds `dir == ""` when `input` has no directory
///   component at all (a relative filename with nothing before it). `validate()`/[`template_escapes_directory`]
///   now reject `:` in a template outright, but a caller that skips `validate()` (calls `plan()` directly)
///   could still produce a Windows drive-relative output like `"C:foo.jpg"` — `split` finds no separator
///   in that either, so `out_dir` is ALSO `""`, the two empty strings compare equal, and the fast path
///   above would wave a drive-relative escape straight through with zero [`path_key`] calls. Guarded
///   explicitly: with no directory component, the computed name itself must contain no `:`.
/// - **Finding B — a dot-segment final component.** An extensionless input plus a template that's
///   literally `".."` (bypassing `validate()`, which rejects that template outright) produces an `output`
///   whose FINAL path component is a bare `..`/`.` — `split` hands that back as an ordinary-looking "stem"
///   with no separator involved, so `out_dir == dir` and the check above never even asks whether this is a
///   real filename. A dot-segment final component never denotes an ordinary file (it resolves to the
///   directory itself or its parent) — rejected outright, independent of the directory-identity check.
///   (With an extensioned input this can't happen: `join` appends the extension, turning a `".."` stem
///   into the literal, harmless filename `"...ext"`.)
/// - **Finding C — link-as-final-component (PR #828 attempt 3).** Even with A and B guarded, `out_dir ==
///   dir` was still a bare TEXT comparison — it never asked what `output`'s final component actually IS
///   on disk. A symlink, junction, or hard link whose *name* sits inside the input's own directory can
///   still alias data physically outside it, and the fast path waved that straight through with zero
///   resolution. See [`link_alias_escapes`] for the fix and exactly what it can and cannot prove.
pub(crate) fn output_escapes_input_dir(input: &str, output: &str, parent_cache: &mut ParentCache) -> bool {
    let out_final = output.rsplit(['/', '\\']).next().unwrap_or(output);
    if out_final == "." || out_final == ".." {
        return true;
    }

    let (dir, _, _) = split(input);
    let (out_dir, out_stem, _) = split(output);
    if dir.is_empty() && out_stem.contains(':') {
        return true;
    }
    if out_dir == dir {
        // The directory TEXT matches — the common case, and historically an immediate `false` here. But
        // `output` may already exist on disk as a link whose real identity isn't where its name suggests
        // (Finding C above); resolve that before trusting the string match. `None` means `output` doesn't
        // exist yet, or is an ordinary single-linked file — the overwhelming common case, costing exactly
        // one extra `symlink_metadata` stat and falling through to the prior `false` unchanged.
        return link_alias_escapes(output, &dir, parent_cache).unwrap_or(false);
    }
    path_key(&out_dir, parent_cache) != path_key(&dir, parent_cache)
}

/// Resolves whether `output` — already known to sit in the same TEXTUAL directory as the input (the
/// `out_dir == dir` fast path in [`output_escapes_input_dir`]) — is actually a link whose real data lives
/// outside `input_dir`. Returns `None` when there is nothing link-shaped to resolve (`output` doesn't
/// exist yet, or is an ordinary file with a single name), so the caller's prior `false` stands unchanged —
/// **no new false positives** on a plain output file that merely already exists in the selected folder.
///
/// **Symlinks and junctions:** read the link's own stored target ([`std::fs::read_link`]) rather than
/// `canonicalize`, specifically because `canonicalize` requires the WHOLE chain to exist and therefore
/// fails outright on a **dangling** symlink (target not created yet) — exactly the shape the audit
/// demonstrated needs zero batch-job flags to exploit (`Path::is_file()` on a dangling symlink is `false`,
/// so nothing downstream ever treats it as "already occupied"). A relative target is resolved against
/// `output`'s own parent directory (the same rule the OS itself uses); the resolved location's directory
/// is then compared via [`path_key`] against `input_dir`, identically to every other containment decision
/// in this module. An unreadable link (the `is_symlink()` bit is set but `read_link` itself fails — a
/// permission error or a TOCTOU race) is **not** treated as "nothing to resolve": failing open on an
/// unreadable link would be worse than the false positive of refusing it, so this returns `Some(true)`.
///
/// **Hard links — deliberately NOT fully resolved.** A hard link has no "target" to read: every name for
/// the same underlying data is equally real, and there is no directory-entry field that names a file's
/// OTHER links. The only way to enumerate them is to walk every directory on the volume comparing
/// (volume-serial, file-index) identity per entry — disproportionate to pay per planned item, and not
/// attempted here. Instead: if a real file already sits at `output` and its **link count** (the one cheap
/// signal `std::fs::Metadata` DOES expose, via the platform `MetadataExt` trait — `number_of_links()` on
/// Windows, `nlink()` on Unix) is more than 1, some other name for this same data exists somewhere this
/// fn cannot see. It might be inside `input_dir`, or it might not — there is no way to tell without the
/// walk this deliberately doesn't do — so this fails closed and refuses rather than guessing "probably
/// fine". **This is the one gap left open by design**, not an oversight: a batch job that plans to write
/// through an existing multiply-linked file inside the selected folder is refused even when every one of
/// its other names is also harmlessly inside that same folder, because this fn has no cheap way to know
/// that. See the module doc's "Link-as-final-component" paragraph for the full reasoning.
fn link_alias_escapes(output: &str, input_dir: &str, parent_cache: &mut ParentCache) -> Option<bool> {
    let meta = std::fs::symlink_metadata(output).ok()?;
    let file_type = meta.file_type();

    if file_type.is_symlink() {
        return Some(match std::fs::read_link(output) {
            Ok(target) => {
                let resolved = if target.is_absolute() {
                    target
                } else {
                    std::path::Path::new(output)
                        .parent()
                        .map(|p| p.join(&target))
                        .unwrap_or(target)
                };
                let resolved_str = resolved.to_string_lossy().into_owned();
                let (resolved_dir, _, _) = split(&resolved_str);
                path_key(&resolved_dir, parent_cache) != path_key(input_dir, parent_cache)
            }
            Err(_) => true,
        });
    }

    if file_type.is_file() && hard_link_count(output, &meta) > 1 {
        return Some(true);
    }

    None
}

/// Platform-specific hard link count for [`link_alias_escapes`]'s fail-closed check — bare
/// [`std::fs::Metadata`] doesn't expose this everywhere. Defaults to `1` (never treated as
/// multiply-linked) on any platform/error where the count can't be read, matching this module's existing
/// "resolve where possible, never invent a signal we can't back up" posture elsewhere (e.g. [`fold_case`]);
/// that default is fine here specifically because it just falls through to the ORIGINAL, already-audited
/// `out_dir == dir → false` behaviour, not a bypass of a check that used to run.
///
/// **Unix:** `MetadataExt::nlink()` reads straight off the already-fetched [`std::fs::Metadata`] — no
/// extra syscall.
///
/// **Windows:** `std::os::windows::fs::MetadataExt::number_of_links()` would be the equivalent one-liner,
/// but it (and `file_index()`/`volume_serial_number()`, which would be needed for full hard-link identity
/// resolution) are still gated behind the unstable `windows_by_handle` feature (rust-lang/rust#63010) on
/// stable Rust. Falls back to the raw Win32 call the std wrapper would eventually make anyway
/// (`CreateFileW` + `GetFileInformationByHandle`'s `nNumberOfLinks`) via the `windows` crate already
/// vendored for [`crate::high_contrast`] — one extra open+query, but only when `output` already exists as
/// an ordinary file (the branch this is called from), never per-item for the common "nothing there yet" case.
#[cfg(unix)]
fn hard_link_count(_output: &str, meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.nlink()
}
#[cfg(windows)]
fn hard_link_count(output: &str, _meta: &std::fs::Metadata) -> u64 {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_NORMAL,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let wide: Vec<u16> =
        std::path::Path::new(output).as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 string kept alive for the whole call. Read-only open
    // of an already-existing file (`GENERIC_READ` + `OPEN_EXISTING`, full sharing so this never contends
    // with the batch's own later read/write of the same path) — no create/write/truncate side effect. The
    // handle is closed on every path before returning.
    unsafe {
        let Ok(handle) = CreateFileW(
            PCWSTR(wide.as_ptr()),
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            HANDLE::default(),
        ) else {
            return 1; // couldn't open (permission/race) — nothing more we can prove, matches the fallback
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        let links =
            if GetFileInformationByHandle(handle, &mut info).is_ok() { info.nNumberOfLinks as u64 } else { 1 };
        let _ = CloseHandle(handle);
        links
    }
}
#[cfg(not(any(windows, unix)))]
fn hard_link_count(_output: &str, _meta: &std::fs::Metadata) -> u64 {
    1
}

/// Plan the batch: for each input compute its output path (applying the ops' effect on name/extension),
/// keep it non-destructive + collision-free when `non_destructive`, and summarise. Ordered like `inputs`.
/// **Not purely in-memory** despite the module's original "no filesystem" framing (CPE-1623): computing
/// [`PathKey`]s and checking real on-disk existence both call [`canonicalize_path`]/[`std::path::Path`]
/// metadata — see the CPE-1613 module doc for why that's cheap in the common case. **Refuses the whole
/// batch** ([`Err`], no partial [`Vec`]) rather than silently dropping or reshaping the offending item:
/// this mirrors [`crate::batch_execute::execute_plan_walk`]'s own "refuse the batch, don't guess"
/// treatment of a destructive write (CPE-1590/1599) — a computed output escaping the folder the user
/// picked is a safety violation, not an ordinary per-file failure like an unreadable input.
///
/// **Collision-set performance (CPE-1613):** the non-destructive collision set is a `HashSet<PathKey>`,
/// not a pairwise string/`same_file` scan — O(1) lookup instead of O(n) per item (which made the whole
/// batch O(n²); see the module doc). `parent_cache` memoizes each unique parent directory's
/// canonicalization once for the whole call, since a batch overwhelmingly shares one parent.
///
/// **Containment (CPE-1623):** `validate()` already rejects any `Rename` template containing a path
/// separator or `..`, so under the one production call path (`batch_media_plan` calls `validate()` before
/// `plan()`) this only ever fires for a caller that invokes `plan()` directly, bypassing `validate()` —
/// devtools, a future automation surface, or a bug in a future op that also builds a stem. The engine
/// stays the actual enforcement point, not just the one UI's template field. Zero extra cost for every
/// ordinary item: `out_dir` is compared to `dir` as plain strings first, and only a template that actually
/// changed the directory portion of the joined output pays for the (still O(1)-amortized) `path_key`
/// resolution that confirms it.
///
/// **Real-filesystem non-destructive guarantee (CPE-1623):** the collision-avoidance disambiguation below
/// used to only ever check against this batch's OWN inputs/outputs (`used`) — a computed name that
/// happened to already exist as some unrelated file never selected into the batch would silently
/// overwrite it, even in the supposedly-safe default mode. It now also treats a real existing file at the
/// candidate output as occupied, exactly like a collision with `used`, and renames past it the same way —
/// a single `Path::is_file()` stat per item (not a `canonicalize`), so the common "the first candidate
/// name is free" case costs one cheap syscall, not a regression to the per-item cost this module's own
/// CPE-1613 fix eliminated.
pub fn plan(job: &BatchJob, inputs: &[String]) -> Result<Vec<PlannedItem>, String> {
    let mut parent_cache: std::collections::HashMap<String, Option<std::path::PathBuf>> =
        std::collections::HashMap::new();
    // Computed once per input (not re-derived per collision check below) and reused by index — avoids
    // redundant `canonicalize` syscalls for the same input path.
    let input_keys: Vec<PathKey> = inputs.iter().map(|p| path_key(p, &mut parent_cache)).collect();
    let mut used: std::collections::HashSet<PathKey> = std::collections::HashSet::new();
    // Pre-seed with the inputs so non-destructive outputs never collide with a source file.
    if job.non_destructive {
        used.extend(input_keys.iter().cloned());
    }

    inputs
        .iter()
        .enumerate()
        .map(|(i, input)| -> Result<PlannedItem, String> {
            let (dir, mut stem, mut ext) = split(input);
            let mut parts: Vec<String> = Vec::new();
            let mut suffix = String::new();

            for op in &job.ops {
                match op {
                    MediaOp::Resize { max_px } => {
                        parts.push(format!("resize→{max_px}px"));
                        suffix = format!("{suffix}-{max_px}");
                    }
                    MediaOp::Convert { to_ext } => {
                        let e = to_ext.trim().trim_start_matches('.').to_ascii_lowercase();
                        parts.push(format!("convert→{e}"));
                        ext = e;
                    }
                    MediaOp::Rotate { degrees } => {
                        parts.push(format!("rotate {degrees}°"));
                        suffix = format!("{suffix}-rot{degrees}");
                    }
                    MediaOp::Flip { horizontal } => {
                        parts.push(if *horizontal { "flip-h".into() } else { "flip-v".into() });
                        suffix = format!("{suffix}-{}", if *horizontal { "fliph" } else { "flipv" });
                    }
                    MediaOp::Rename { template } => {
                        stem = template
                            .replace("{stem}", &stem)
                            .replace("{n}", &(i + 1).to_string())
                            .replace("{ext}", &ext);
                        suffix.clear(); // an explicit rename supersedes derived suffixes
                        parts.push("rename".into());
                    }
                    MediaOp::StripMetadata => parts.push("strip-metadata".into()),
                    // No suffix (mirrors StripMetadata): compress changes bytes, not dimensions/name, so
                    // it relies on the non-destructive collision guard's generic "-out" fallback below.
                    MediaOp::Compress { quality } => parts.push(format!("compress q{quality}")),
                    // Optional by construction: an empty `image` means "no watermark configured", so the
                    // op contributes NOTHING to the plan — no summary text, no suffix — same as if the op
                    // weren't in the list at all. A non-empty image gets a summary line but, like
                    // Compress/StripMetadata, no dedicated suffix; the generic non-destructive fallback
                    // below still keeps the output distinct from the input.
                    MediaOp::Watermark { image, position, opacity } => {
                        if !image.trim().is_empty() {
                            parts.push(format!("watermark {} {} {opacity}%", basename(image), position.as_str()));
                        }
                    }
                }
            }

            let mut out_stem = format!("{stem}{suffix}");
            let mut output = join(&dir, &out_stem, &ext);

            // CPE-1623: constrain the computed output to the input's own directory — plan() has never
            // taken a separate "target dir" parameter; each item's implicit target is always the folder
            // its own input already lives in. [`output_escapes_input_dir`] is the one shared definition
            // of this check — also used by `batch_execute::execute_plan_walk`'s independent pre-write
            // re-check, so there is exactly one place that decides "did this leave the folder?", not two
            // definitions that could drift apart.
            if output_escapes_input_dir(input, &output, &mut parent_cache) {
                return Err(format!(
                    "computed output for \"{input}\" would land at \"{output}\", outside its own folder \
                     — a Convert extension or Rename template can only change a file's name/extension, \
                     never its folder"
                ));
            }

            if job.non_destructive {
                // Guarantee output != input and no two plans share an output — disambiguate with -2, -3…
                // "Same file" is decided by `path_key` equality, not raw string equality (CPE-1613): a
                // case/separator/`.`-`..` variant of an existing name must be caught too, or this
                // "non-destructive" promise doesn't hold on a case-insensitive filesystem. Keyed lookups
                // (not a pairwise `same_file` scan) keep this O(1) per check — see the module doc.
                let mut out_key = path_key(&output, &mut parent_cache);
                if out_key == input_keys[i] && suffix.is_empty() {
                    out_stem = format!("{stem}-out");
                    output = join(&dir, &out_stem, &ext);
                    out_key = path_key(&output, &mut parent_cache);
                }
                let base = out_stem.clone();
                let mut n = 2;
                // CPE-1623: also treat a REAL existing file at the candidate output as occupied, not just
                // a collision with this batch's own `used` set — see the fn doc. `Path::is_file()` is a
                // single stat, not a `canonicalize`, so this doesn't reintroduce the O(n²) cost CPE-1613
                // fixed.
                while used.contains(&out_key) || std::path::Path::new(&output).is_file() {
                    out_stem = format!("{base}-{n}");
                    output = join(&dir, &out_stem, &ext);
                    out_key = path_key(&output, &mut parent_cache);
                    n += 1;
                }
                used.insert(out_key);
            }

            let summary = if parts.is_empty() { "no-op".into() } else { parts.join(", ") };
            Ok(PlannedItem { input: input.clone(), output, summary })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn validate_rejects_bad_jobs() {
        assert!(validate(&BatchJob::new(vec![])).is_err());
        assert!(validate(&BatchJob::new(vec![MediaOp::Rotate { degrees: 45 }])).is_err());
        assert!(validate(&BatchJob::new(vec![MediaOp::Convert { to_ext: "  ".into() }])).is_err());
        assert!(validate(&BatchJob::new(vec![MediaOp::Resize { max_px: 0 }])).is_err());
        assert!(validate(&BatchJob::new(vec![MediaOp::Resize { max_px: 800 }])).is_ok());
    }

    #[test]
    fn validate_rejects_out_of_range_compress_quality() {
        assert!(validate(&BatchJob::new(vec![MediaOp::Compress { quality: 0 }])).is_err());
        assert!(validate(&BatchJob::new(vec![MediaOp::Compress { quality: 101 }])).is_err());
        assert!(validate(&BatchJob::new(vec![MediaOp::Compress { quality: 1 }])).is_ok());
        assert!(validate(&BatchJob::new(vec![MediaOp::Compress { quality: 100 }])).is_ok());
    }

    #[test]
    fn compress_summary_and_no_forced_suffix() {
        let job = BatchJob::new(vec![MediaOp::Compress { quality: 80 }]);
        let out = plan(&job, &v(&["/pics/cat.jpg"])).unwrap();
        assert_eq!(out[0].summary, "compress q80");
        // No dedicated suffix (mirrors StripMetadata) — the generic non-destructive fallback renames
        // the sole-op case to "-out" so the output still differs from the input.
        assert_eq!(out[0].output, "/pics/cat-out.jpg");
    }

    #[test]
    fn validate_rejects_out_of_range_watermark_opacity() {
        let op = |opacity| MediaOp::Watermark { image: "logo.png".into(), position: Corner::default(), opacity };
        assert!(validate(&BatchJob::new(vec![op(101)])).is_err());
        assert!(validate(&BatchJob::new(vec![op(0)])).is_ok()); // 0 is valid (a fully-invisible watermark)
        assert!(validate(&BatchJob::new(vec![op(100)])).is_ok());
    }

    #[test]
    fn validate_does_not_require_the_watermark_image_to_exist() {
        // Empty (unset) is explicitly fine at validate time — checked only at apply, per the
        // "optional, none if unset" requirement.
        let op = MediaOp::Watermark { image: String::new(), position: Corner::default(), opacity: 50 };
        assert!(validate(&BatchJob::new(vec![op])).is_ok());
    }

    #[test]
    fn watermark_with_empty_image_contributes_nothing_to_the_plan() {
        // Empty image ⇒ the op adds no summary text and no suffix — it's as if it weren't in the ops
        // list at all. The non-destructive collision guard still renames to "-out" (the same generic
        // fallback Compress/StripMetadata rely on), since *some* job with >=1 op was still submitted.
        let job = BatchJob::new(vec![MediaOp::Watermark {
            image: String::new(),
            position: Corner::BottomRight,
            opacity: 80,
        }]);
        let out = plan(&job, &v(&["/pics/cat.jpg"])).unwrap();
        assert_eq!(out[0].summary, "no-op");
        assert_eq!(out[0].output, "/pics/cat-out.jpg");

        // In overwrite mode (no collision guard), an empty-image watermark truly changes nothing.
        let mut job2 = job.clone();
        job2.non_destructive = false;
        let out2 = plan(&job2, &v(&["/pics/cat.jpg"])).unwrap();
        assert_eq!(out2[0].output, "/pics/cat.jpg");
    }

    #[test]
    fn watermark_summary_names_the_overlay_corner_and_opacity() {
        let job = BatchJob::new(vec![MediaOp::Watermark {
            image: "/assets/logo.png".into(),
            position: Corner::TopLeft,
            opacity: 40,
        }]);
        let out = plan(&job, &v(&["/pics/cat.jpg"])).unwrap();
        assert_eq!(out[0].summary, "watermark logo.png top_left 40%");
        // No dedicated suffix (mirrors Compress/StripMetadata) — falls back to the generic "-out".
        assert_eq!(out[0].output, "/pics/cat-out.jpg");
    }

    #[test]
    fn resize_is_non_destructive_by_default() {
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 1024 }]);
        let out = plan(&job, &v(&["/pics/cat.jpg"])).unwrap();
        assert_eq!(out[0].output, "/pics/cat-1024.jpg"); // suffix keeps it off the source
        assert_eq!(out[0].summary, "resize→1024px");
    }

    #[test]
    fn convert_changes_extension_and_lowercases() {
        let job = BatchJob::new(vec![MediaOp::Convert { to_ext: ".PNG".into() }]);
        let out = plan(&job, &v(&["/pics/cat.jpg"])).unwrap();
        assert_eq!(out[0].output, "/pics/cat.png"); // different ext ⇒ already non-destructive
    }

    #[test]
    fn cpe_1613_non_destructive_convert_forces_a_distinct_name_even_when_only_extension_case_changes() {
        // The ticket's worked example, at plan()'s non-destructive guarantee: input "IMG_1.JPG",
        // Convert→jpg lower-cases only the extension to "IMG_1.jpg". Before CPE-1613, "output != input"
        // was raw string equality, so this looked like a different path and got a free pass — even
        // though it's the SAME FILE on a case-insensitive filesystem. The guarantee must now force the
        // "-out" suffix there too, matching the acceptance criteria ("produces a genuinely different
        // file, or refuses").
        let job = BatchJob::new(vec![MediaOp::Convert { to_ext: "jpg".into() }]);
        let out = plan(&job, &v(&["/pics/IMG_1.JPG"])).unwrap();
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        assert_eq!(out[0].output, "/pics/IMG_1-out.jpg");
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        assert_eq!(out[0].output, "/pics/IMG_1.jpg"); // genuinely a different file on a case-sensitive fs
    }

    #[test]
    fn cpe_1613_overwrite_mode_reports_the_worked_example_as_in_place_via_same_file() {
        // Same worked example, but with "write to new files" OFF: `plan()` no longer forces a distinct
        // name, so the output IS "IMG_1.jpg" — and `same_file` (not `==`) is what `any_in_place` /
        // `execute_plan_walk`'s refusal check must use to recognise it as in-place on a case-insensitive
        // filesystem. This test only pins plan()'s output; batch_execute.rs's own tests cover the
        // refusal itself.
        let mut job = BatchJob::new(vec![MediaOp::Convert { to_ext: "jpg".into() }]);
        job.non_destructive = false;
        let out = plan(&job, &v(&["/pics/IMG_1.JPG"])).unwrap();
        assert_eq!(out[0].output, "/pics/IMG_1.jpg");
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        assert!(same_file(&out[0].input, &out[0].output));
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        assert!(!same_file(&out[0].input, &out[0].output));
    }

    #[test]
    fn rename_template_expands_stem_and_index() {
        let job = BatchJob::new(vec![MediaOp::Rename { template: "photo-{n}".into() }]);
        let out = plan(&job, &v(&["/a/x.jpg", "/a/y.jpg"])).unwrap();
        assert_eq!(out[0].output, "/a/photo-1.jpg");
        assert_eq!(out[1].output, "/a/photo-2.jpg");
    }

    #[test]
    fn same_target_collisions_are_disambiguated() {
        // Two inputs in different dirs both renamed to the same stem in the SAME dir → -2 suffix.
        let job = BatchJob::new(vec![MediaOp::Rename { template: "out".into() }]);
        let out = plan(&job, &v(&["/a/x.jpg", "/a/y.jpg"])).unwrap();
        assert_eq!(out[0].output, "/a/out.jpg");
        assert_eq!(out[1].output, "/a/out-2.jpg");
    }

    #[test]
    fn overwrite_mode_keeps_the_input_path() {
        let mut job = BatchJob::new(vec![MediaOp::Resize { max_px: 512 }, MediaOp::StripMetadata]);
        job.non_destructive = false;
        let out = plan(&job, &v(&["/p/a.jpg"])).unwrap();
        assert_eq!(out[0].output, "/p/a-512.jpg"); // suffix still applied, but no collision guard
        assert_eq!(out[0].summary, "resize→512px, strip-metadata");
    }

    #[test]
    fn multiple_ops_compose_suffix_and_summary() {
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 800 }, MediaOp::Rotate { degrees: 90 }]);
        let out = plan(&job, &v(&["c:\\img\\p.png"])).unwrap();
        assert_eq!(out[0].output, "c:\\img\\p-800-rot90.png");
        assert_eq!(out[0].summary, "resize→800px, rotate 90°");
    }

    // ---- CPE-1613: same_file — the shared "is this the same file?" definition -----------------------

    /// A fresh, uniquely-named per-test scratch dir, backed by `tempfile::TempDir` (auto-cleaned on
    /// drop) — mirrors `organize_apply.rs`'s test helper.
    fn scratch(tag: &str) -> tempfile::TempDir {
        tempfile::Builder::new().prefix(&format!("cpe-batchmedia-{tag}-")).tempdir().unwrap()
    }

    #[test]
    fn same_file_worked_example_case_only_extension_difference_is_platform_gated() {
        // The ticket's worked example: Convert→jpg lower-cases only the extension, so
        // "IMG_1.JPG" -> "IMG_1.jpg" — a DIFFERENT string, but the SAME file on a case-insensitive
        // filesystem (Windows, default macOS). On Linux (case-sensitive ext4 etc.) these are two
        // genuinely distinct possible files, so `same_file` must NOT fold them there. Neither path
        // exists on disk, so this exercises the purely lexical fallback (branch 3).
        let a = "/pics/IMG_1.JPG";
        let b = "/pics/IMG_1.jpg";
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        assert!(same_file(a, b), "a case-only difference must be the same file on {}", std::env::consts::OS);
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        assert!(!same_file(a, b), "a case-only difference is a DIFFERENT file on a case-sensitive filesystem");
    }

    #[test]
    fn same_file_trailing_separator_is_ignored() {
        assert!(same_file("/pics/cat.jpg", "/pics/cat.jpg/"));
        assert!(same_file("c:\\img\\p.png", "c:\\img\\p.png\\"));
    }

    #[test]
    fn same_file_dot_and_dotdot_segments_resolve_lexically() {
        assert!(same_file("/pics/x/../cat.jpg", "/pics/cat.jpg"));
        assert!(same_file("/pics/./cat.jpg", "/pics/cat.jpg"));
        assert!(same_file("/pics/a/b/../../cat.jpg", "/pics/cat.jpg"));
        // Genuinely different files must stay distinct.
        assert!(!same_file("/pics/a/cat.jpg", "/pics/b/cat.jpg"));
    }

    #[test]
    fn same_file_separator_style_does_not_matter() {
        assert!(same_file("c:\\img\\p.png", "c:/img/p.png"));
        assert!(same_file("/pics/a/cat.jpg", "\\pics\\a\\cat.jpg"));
    }

    #[test]
    fn same_file_reflexive_and_distinct_names() {
        assert!(same_file("/pics/cat.jpg", "/pics/cat.jpg"));
        assert!(!same_file("/pics/cat.jpg", "/pics/dog.jpg"));
        assert!(!same_file("/pics/cat.jpg", "/pics/cat.png"));
    }

    #[test]
    fn same_file_resolves_real_files_via_canonicalize_even_with_a_case_variant_path() {
        // With a REAL file on disk, canonicalize succeeds for a case-variant spelling of its name on a
        // case-insensitive filesystem (the OS resolves the lookup regardless of the case typed), so this
        // exercises the strongest branch (1) rather than the lexical fallback.
        let dir = scratch("real-case");
        let real = dir.path().join("photo.png");
        std::fs::write(&real, b"x").unwrap();
        let alt_case = dir.path().join("PHOTO.PNG");

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        assert!(
            same_file(&real.to_string_lossy(), &alt_case.to_string_lossy()),
            "a real file's case-variant path must canonicalize to the same identity on {}",
            std::env::consts::OS
        );
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            // On a case-sensitive filesystem the alt-case path doesn't exist at all, so this falls back
            // to the parent-canonicalize branch and correctly reports them as distinct.
            assert!(!same_file(&real.to_string_lossy(), &alt_case.to_string_lossy()));
        }
    }

    #[test]
    fn same_file_follows_a_symlink_to_its_target() {
        let dir = scratch("symlink");
        let target = dir.path().join("original.png");
        std::fs::write(&target, b"x").unwrap();
        let link = dir.path().join("link.png");

        match crate::links::create_symlink(&target.to_string_lossy(), &link.to_string_lossy()) {
            Ok(()) => {
                assert!(
                    same_file(&target.to_string_lossy(), &link.to_string_lossy()),
                    "a symlink and its target are the same underlying file"
                );
            }
            Err(_) => { /* unprivileged Windows — symlink creation is gated, skip like links.rs's own tests */ }
        }
    }

    #[cfg(windows)]
    #[test]
    fn same_file_resolves_a_file_reached_through_a_windows_junction() {
        // Junctions target DIRECTORIES and need no elevation (unlike symlinks), so this test isn't
        // gated behind the unprivileged-Windows skip pattern.
        let dir = scratch("junction");
        let real_dir = dir.path().join("real");
        std::fs::create_dir(&real_dir).unwrap();
        let real_file = real_dir.join("photo.png");
        std::fs::write(&real_file, b"x").unwrap();

        let junction_dir = dir.path().join("via-junction");
        crate::links::create_junction(&real_dir.to_string_lossy(), &junction_dir.to_string_lossy())
            .expect("junction creation needs no elevation");
        let via_junction_file = junction_dir.join("photo.png");

        assert!(
            same_file(&real_file.to_string_lossy(), &via_junction_file.to_string_lossy()),
            "a file reached through a directory junction is the same underlying file as via the real path"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn same_file_treats_nfc_and_nfd_forms_of_a_real_filename_as_the_same_file() {
        // Only exercisable on macOS/APFS, which resolves filename lookups normalization-insensitively —
        // this is the "where feasible" case the ticket calls out; the other two CI legs (Windows/Linux)
        // don't normalize filenames at the filesystem level, so there is nothing analogous to assert
        // there. "café" in NFC (precomposed é, U+00E9) vs NFD (e + combining acute, U+0065 U+0301).
        let dir = scratch("nfd");
        let nfc_name = "cafe\u{00E9}.png"; // café.png, precomposed
        let nfd_name = "cafe\u{0065}\u{0301}.png"; // cafe + combining acute accent
        assert_ne!(nfc_name, nfd_name, "sanity: these really are different byte sequences");

        let real = dir.path().join(nfc_name);
        std::fs::write(&real, b"x").unwrap();
        let nfd_path = dir.path().join(nfd_name);

        assert!(
            same_file(&real.to_string_lossy(), &nfd_path.to_string_lossy()),
            "NFC and NFD spellings of the same name must resolve to the same file on macOS/APFS"
        );
    }

    // ---- CPE-1613 follow-up: collision-set performance regression guard -------------------------------

    #[test]
    fn cpe_1613_plan_collision_check_makes_a_bounded_number_of_canonicalize_calls_not_quadratic() {
        // Perf regression guard for the reviewer finding on PR #818: `plan()`'s non-destructive collision
        // set used to be a `Vec<String>` scanned pairwise via `same_file` — O(n) `same_file` calls per
        // item, each issuing up to 2 *uncached* `canonicalize` syscalls, so a single-folder batch was
        // O(n²) syscalls overall (measured: 2000 files, release build, ~718s — see the ticket's work log
        // for the exact before/after numbers). `path_key`'s `HashSet` + memoized parent-canonicalize cache
        // should make this O(n) — a small constant number of syscalls per file, not proportional to n².
        // Counting syscalls (via the `canonicalize_path` test seam) instead of asserting wall-clock time
        // keeps this deterministic on loaded/slow CI runners, per CLAUDE.md's flaky-timing guidance.
        let dir = scratch("cpe1613-perf-guard");
        let n = 300usize;
        let inputs: Vec<String> = (0..n)
            .map(|i| {
                let p = dir.path().join(format!("photo{i:04}.jpg"));
                std::fs::write(&p, b"x").unwrap();
                p.to_string_lossy().to_string()
            })
            .collect();

        let job = BatchJob::new(vec![MediaOp::Compress { quality: 80 }]);
        reset_canonicalize_call_count();
        let out = plan(&job, &inputs).unwrap();
        let calls = canonicalize_call_count();
        assert_eq!(out.len(), n);

        // A generous linear bound: O(n) allows a small constant number of canonicalize calls per file
        // (pre-seed pass + the self-collision/while-loop checks per item). O(n²) would blow far past this
        // even at n=300 (300*300/2 = 45,000 pairwise comparisons). 10x n leaves headroom for the exact
        // constant while still comfortably rejecting a quadratic regression.
        assert!(
            calls <= n * 10,
            "expected O(n) canonicalize calls (bound {}), got {calls} for n={n} files — the collision \
             check may have regressed to a pairwise O(n²) scan",
            n * 10
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    #[test]
    fn cpe_1623_containment_check_for_a_directory_changing_rename_stays_bounded() {
        // Reviewer caveat (PR #828 attempt 2): the quadratic-regression guard above uses
        // `MediaOp::Compress`, which NEVER enters the CPE-1623 containment branch — `out_dir == dir`
        // textually for every item, so `path_key` is never called for it at all. That guard has a blind
        // spot for the containment check's own cost. This test exercises that branch on EVERY item: a
        // Rename template of `"./{stem}"` changes `out_dir` TEXTUALLY (adds a `"./"` segment) but resolves
        // to the exact same real directory, so `output_escapes_input_dir` genuinely runs `path_key`'s full
        // resolution for both `out_dir` and `dir` on every single item (the fast path is skipped), and
        // still correctly reports "contained" (`plan()` succeeds, not refused).
        let dir = scratch("cpe1623-containment-perf-guard");
        let n = 300usize;
        let inputs: Vec<String> = (0..n)
            .map(|i| {
                let p = dir.path().join(format!("photo{i:04}.jpg"));
                std::fs::write(&p, b"x").unwrap();
                p.to_string_lossy().to_string()
            })
            .collect();

        let mut job = BatchJob::new(vec![MediaOp::Rename { template: "./{stem}".into() }]);
        job.non_destructive = false; // isolate the containment check's own cost from collision-avoidance
        reset_canonicalize_call_count();
        let out = plan(&job, &inputs).unwrap_or_else(|e| panic!("a same-directory rename must not be refused: {e}"));
        let calls = canonicalize_call_count();
        assert_eq!(out.len(), n);

        // Same generous linear bound as the guard above — O(n) allows a small constant number of
        // canonicalize calls per file (here: two path_key resolutions per item, one for `out_dir` and one
        // for `dir`, both memoized after the first). O(n²) would blow far past this even at n=300.
        assert!(
            calls <= n * 10,
            "expected O(n) canonicalize calls even when every item genuinely resolves the containment \
             check (bound {}), got {calls} for n={n} files — the containment check may have regressed to \
             a per-item uncached resolution",
            n * 10
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    // ---- CPE-1623 follow-up: link-as-final-component defeats containment (PR #828 attempt 3) ---------
    //
    // `out_dir == dir` (the fast path taken when the output's directory TEXT matches the input's own) used
    // to return `false` immediately — never asking what `output`'s final component actually IS on disk.
    // These tests exercise [`output_escapes_input_dir`] directly (unit-level; `batch_execute`'s test module
    // has the end-to-end byte-proof versions run through `execute_plan`).

    /// **Negative control, confirmed against pre-fix HEAD:** before this fix, a hard link whose NAME sits
    /// inside `input`'s own directory but whose DATA is the same file as something outside it produced
    /// `escapes == false` purely because `out_dir == dir` as strings — the exact bypass the finding
    /// describes. This is the regression test for that: must now report `true`.
    #[test]
    fn cpe_1623_hard_link_alias_within_the_same_directory_text_escapes() {
        let dir = scratch("cpe1623-hardlink-escape");
        let selected = dir.path().join("selected");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&selected).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("important.jpg");
        std::fs::write(&victim, b"victim").unwrap();
        let link = selected.join("link.jpg");
        crate::links::create_hard_link(&victim.to_string_lossy(), &link.to_string_lossy())
            .expect("hard link creation needs no elevation on any platform");

        let input = selected.join("photo.jpg").to_string_lossy().to_string();
        let output = link.to_string_lossy().to_string();
        // Sanity: the directory TEXT really does match — this is exactly the fast path that used to skip
        // resolution entirely.
        assert_eq!(split(&input).0, split(&output).0, "sanity: directory text must match for this test");

        let mut cache = ParentCache::new();
        assert!(
            output_escapes_input_dir(&input, &output, &mut cache),
            "a hard-linked output whose data lives outside the selected folder must be reported as escaping"
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// **Negative control, confirmed against pre-fix HEAD:** a DANGLING symlink (target doesn't exist yet)
    /// whose name sits inside `input`'s own directory but whose stored target names a path outside it. Even
    /// worse than the hard-link case pre-fix: `canonicalize` can't resolve a dangling chain at all, so
    /// nothing downstream could have caught this via the ordinary `path_key` route either — it needed the
    /// raw `read_link` handling this fix adds. Symlink creation needs Developer Mode / elevation on
    /// Windows — skips cleanly (not fails) when unavailable, matching `links.rs`'s own test pattern.
    #[test]
    fn cpe_1623_dangling_symlink_alias_within_the_same_directory_text_escapes() {
        let dir = scratch("cpe1623-dangling-symlink-escape");
        let selected = dir.path().join("selected");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&selected).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("newly-planted.jpg"); // deliberately does not exist yet
        let link = selected.join("link.jpg");
        if crate::links::create_symlink(&victim.to_string_lossy(), &link.to_string_lossy()).is_err() {
            eprintln!(
                "skipping dangling-symlink containment test: could not create a symlink in this \
                 environment (Windows needs Developer Mode or elevation)"
            );
            let _ = std::fs::remove_dir_all(dir.path());
            return;
        }
        assert!(!victim.exists(), "sanity: the target must not exist yet — the dangling case");

        let input = selected.join("photo.jpg").to_string_lossy().to_string();
        let output = link.to_string_lossy().to_string();
        assert_eq!(split(&input).0, split(&output).0, "sanity: directory text must match for this test");

        let mut cache = ParentCache::new();
        assert!(
            output_escapes_input_dir(&input, &output, &mut cache),
            "a dangling symlink whose stored target names a path outside the selected folder must be \
             reported as escaping, with no target ever needing to exist on disk"
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// Same shape, but a LIVE (non-dangling) symlink — extra coverage beyond the two demonstrated PoCs,
    /// since the fix is general to any symlink final component, not just the dangling case.
    #[test]
    fn cpe_1623_live_symlink_alias_within_the_same_directory_text_escapes() {
        let dir = scratch("cpe1623-live-symlink-escape");
        let selected = dir.path().join("selected");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&selected).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("important.jpg");
        std::fs::write(&victim, b"victim").unwrap();
        let link = selected.join("link.jpg");
        if crate::links::create_symlink(&victim.to_string_lossy(), &link.to_string_lossy()).is_err() {
            eprintln!("skipping live-symlink containment test: could not create a symlink in this environment");
            let _ = std::fs::remove_dir_all(dir.path());
            return;
        }

        let input = selected.join("photo.jpg").to_string_lossy().to_string();
        let output = link.to_string_lossy().to_string();
        let mut cache = ParentCache::new();
        assert!(
            output_escapes_input_dir(&input, &output, &mut cache),
            "a live symlink whose target resolves outside the selected folder must be reported as escaping"
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// **No new false positives, part 1:** a symlink whose target legitimately stays INSIDE the same
    /// selected directory must not be flagged — only cross-directory aliasing is a containment violation.
    #[test]
    fn cpe_1623_symlink_pointing_back_inside_the_same_directory_does_not_escape() {
        let dir = scratch("cpe1623-symlink-inside");
        std::fs::create_dir_all(dir.path()).unwrap();
        let real = dir.path().join("real.jpg");
        std::fs::write(&real, b"data").unwrap();
        let link = dir.path().join("link.jpg");
        if crate::links::create_symlink(&real.to_string_lossy(), &link.to_string_lossy()).is_err() {
            eprintln!("skipping same-directory symlink test: could not create a symlink in this environment");
            let _ = std::fs::remove_dir_all(dir.path());
            return;
        }

        let input = dir.path().join("photo.jpg").to_string_lossy().to_string();
        let output = link.to_string_lossy().to_string();
        let mut cache = ParentCache::new();
        assert!(
            !output_escapes_input_dir(&input, &output, &mut cache),
            "a symlink whose target stays inside the selected folder must NOT be reported as escaping"
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// **No new false positives, part 2 (the common case):** an ordinary output file that merely already
    /// exists in the selected folder — a plain regular file, exactly one name, no link involved at all —
    /// must be handled exactly as before: not an escape. This is the overwhelmingly common real-filesystem
    /// shape the collision-avoidance disambiguation in `plan()` deals with every day.
    #[test]
    fn cpe_1623_ordinary_pre_existing_output_file_is_unaffected() {
        let dir = scratch("cpe1623-ordinary-existing-output");
        std::fs::create_dir_all(dir.path()).unwrap();
        let input = dir.path().join("photo.jpg");
        std::fs::write(&input, b"input").unwrap();
        let output = dir.path().join("vacation.jpg"); // a real, single-linked, unrelated existing file
        std::fs::write(&output, b"pre-existing unrelated content").unwrap();

        let mut cache = ParentCache::new();
        assert!(
            !output_escapes_input_dir(
                &input.to_string_lossy(),
                &output.to_string_lossy(),
                &mut cache
            ),
            "an ordinary pre-existing output file in the same folder must not be flagged as escaping"
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// Manual-only timing measurement (CPE-1623 follow-up to the CPE-1613 perf fix): `#[ignore]`d so it
    /// never runs in CI (avoids flaky wall-clock assertions, per CLAUDE.md), but gives a real number for
    /// the Foreman's requested "state your own measured `plan()` timing for 2000 files" — run explicitly
    /// with `cargo test --release -- --ignored --nocapture cpe_1623_plan_timing_for_2000_files`.
    #[test]
    #[ignore]
    fn cpe_1623_plan_timing_for_2000_files() {
        let dir = scratch("cpe1623-timing-2000");
        let n = 2000usize;
        let inputs: Vec<String> = (0..n)
            .map(|i| {
                let p = dir.path().join(format!("photo{i:04}.jpg"));
                std::fs::write(&p, b"x").unwrap();
                p.to_string_lossy().to_string()
            })
            .collect();
        let job = BatchJob::new(vec![MediaOp::Compress { quality: 80 }]);

        let start = std::time::Instant::now();
        let out = plan(&job, &inputs).unwrap();
        let elapsed = start.elapsed();
        assert_eq!(out.len(), n);
        println!("plan() for {n} files in one directory took {elapsed:?}");

        let _ = std::fs::remove_dir_all(dir.path());
    }

    // ---- CPE-1623: rename template can't escape the input's own directory ----------------------------

    #[test]
    fn cpe_1623_validate_rejects_rename_templates_with_separators_or_traversal() {
        let reject = |t: &str| {
            let job = BatchJob::new(vec![MediaOp::Rename { template: t.into() }]);
            let err = validate(&job).expect_err(&format!("expected {t:?} to be rejected"));
            assert!(err.contains("folder") || err.contains(".."), "unexpected message for {t:?}: {err}");
        };
        reject("../evil");
        reject("..\\evil");
        reject("sub/name");
        reject("sub\\name");
        reject("..");
        reject(" .. "); // whole segment once trimmed — still a traversal
        reject("a/../b");
        reject("x/..");
        reject("C:foo"); // reviewer finding A: drive-relative reference
        reject("secrets:hidden"); // colon anywhere, not just drive-letter position

        // Ordinary templates — no separators, no ":", no WHOLE-SEGMENT ".." — must still validate fine.
        // "shot..final"/"v1..2" are the UAT tester's exact worked examples: two literal dots inside an
        // otherwise ordinary filename, no separator anywhere, so there is nothing to walk through.
        for ok in [
            "{stem}", "{stem}-{n}", "photo-{n}", "vacation 2024", "{stem}_backup",
            "shot..final", "v1..2", "a..b", "...",
        ] {
            let job = BatchJob::new(vec![MediaOp::Rename { template: ok.into() }]);
            assert!(validate(&job).is_ok(), "expected {ok:?} to validate");
        }
    }

    #[test]
    fn cpe_1623_convert_extension_is_sanitised_the_same_way_as_rename_template() {
        // The Convert-extension gap: to_ext feeds the exact same join()'d output path but wasn't checked
        // at all before this fix. Mirrors the Rename-template accept/reject split exactly (same helper).
        for bad in ["../evil", "..\\evil", "sub/ext", "C:foo", "..", "a/.."] {
            let job = BatchJob::new(vec![MediaOp::Convert { to_ext: bad.into() }]);
            let err = validate(&job).expect_err(&format!("expected convert to_ext {bad:?} to be rejected"));
            assert!(err.contains("convert extension"), "message should name the actual op: {err}");
        }
        for ok in ["jpg", "PNG", ".webp", "shot..final"] {
            let job = BatchJob::new(vec![MediaOp::Convert { to_ext: ok.into() }]);
            assert!(validate(&job).is_ok(), "expected convert to_ext {ok:?} to validate");
        }
    }

    #[test]
    fn cpe_1623_plan_refuses_a_traversal_template_even_when_validate_is_bypassed() {
        // Defense in depth: plan() itself refuses, not just validate() — this simulates a caller that
        // calls plan() directly (devtools, a future automation surface) without ever calling validate()
        // first. Bare in-memory paths (not on disk), so this exercises the purely lexical fallback tier.
        let job = BatchJob::new(vec![MediaOp::Rename {
            template: "..\\..\\cpe1613_traversal_victim\\important".into(),
        }]);
        let err = plan(&job, &v(&["/pics/traversal_workdir/innocuous.jpg"]))
            .expect_err("a template that walks the output outside its own directory must be refused");
        assert!(err.contains("folder"), "refusal reason should explain the containment violation: {err}");
    }

    #[test]
    fn cpe_1623_convert_extension_traversal_is_refused_with_an_accurate_op_name() {
        // Reviewer finding C: the containment backstop is op-agnostic (it fires regardless of which op
        // produced the escaping output) but the refusal message used to hard-code "rename template" even
        // when the actual op was Convert. Bypasses validate() (which would now also catch this at the
        // to_ext field-level check) to exercise plan()'s own independent backstop directly.
        let job = BatchJob::new(vec![MediaOp::Convert { to_ext: "..\\..\\victim\\important".into() }]);
        let err = plan(&job, &v(&["/pics/traversal_workdir/innocuous.jpg"]))
            .expect_err("a Convert extension that walks the output outside its own directory must be refused");
        assert!(err.contains("folder"), "refusal reason: {err}");
        assert!(!err.contains("rename template"), "must not misattribute a Convert escape to Rename: {err}");
        assert!(err.contains("Convert"), "refusal reason should name Convert as a possible cause: {err}");
    }

    #[test]
    fn cpe_1623_reviewer_finding_a_bare_filename_input_drive_relative_template_is_refused() {
        // Reviewer finding A (PR #828 attempt 2): a BARE-FILENAME input (no directory component at all)
        // makes split()'s `dir` textually empty. A Windows drive-relative template ("C:foo") produces an
        // output whose directory portion is ALSO textually empty (split only recognises `/`/`\`, never
        // `:`), so the old fast-path `out_dir == dir` comparison (two empty strings) would silently accept
        // a drive-relative escape. Bypasses validate() (which now also rejects `:` at the field level) to
        // exercise plan()'s own independent structural backstop.
        let job = BatchJob::new(vec![MediaOp::Rename { template: "C:foo".into() }]);
        let err = plan(&job, &v(&["innocuous.jpg"]))
            .expect_err("a drive-relative output for a bare-filename input must be refused, not planned");
        assert!(err.contains("folder"), "refusal reason: {err}");
    }

    #[test]
    fn cpe_1623_reviewer_finding_b_dotdot_final_component_is_refused() {
        // Reviewer finding B (PR #828 attempt 2): an EXTENSIONLESS input plus a template that's literally
        // ".." produces an output whose FINAL path component is a bare ".." — split() hands that back as
        // an ordinary-looking stem with no separator, so the old check never even asked whether it denotes
        // a real file. Bypasses validate() (which already rejects a whole-segment ".." template) to
        // exercise plan()'s own independent structural backstop. Extensionless matters: with an extension,
        // join() appends it and turns the ".." stem into the literal, harmless filename "...ext" (covered
        // by cpe_1623_dotdot_only_rejected_as_a_whole_path_segment_not_any_occurrence's accept list).
        let job = BatchJob::new(vec![MediaOp::Rename { template: "..".into() }]);
        let err = plan(&job, &v(&["/pics/a/traversal_workdir/innocuous"]))
            .expect_err("an output whose final component is a bare \"..\" must be refused, not planned");
        assert!(err.contains("folder"), "refusal reason: {err}");
    }

    #[test]
    fn cpe_1623_dotdot_only_rejected_as_a_whole_path_segment_not_any_occurrence() {
        // UAT follow-up: ".." embedded in an ordinary filename (no separator anywhere) is not a traversal
        // risk — the template only ever substitutes into the STEM, so with no separator there is only ONE
        // segment, and it's a traversal risk only if that whole segment IS "..". Plans successfully, and
        // the output stays in the input's own folder (contrast with the refused cases above/below).
        for ok in ["shot..final", "v1..2", "a..b", "..."] {
            let job = BatchJob::new(vec![MediaOp::Rename { template: ok.into() }]);
            let out = plan(&job, &v(&["/pics/vacation/photo1.jpg"]))
                .unwrap_or_else(|e| panic!("expected {ok:?} to plan successfully: {e}"));
            assert_eq!(out[0].output, format!("/pics/vacation/{ok}.jpg"), "template {ok:?}");
        }
        // Genuine traversal still refused at the plan() layer too (bypassing validate()). Bare ".." isn't
        // in this list: against an EXTENSIONED input (as used here), ".." as a Rename stem plans to the
        // literal, harmless filename "...jpg" (join() appends the extension) — it only denotes a real
        // parent-directory reference against an EXTENSIONLESS input, which
        // cpe_1623_reviewer_finding_b_dotdot_final_component_is_refused covers on its own.
        for bad in ["../x", "..\\x", "a/../../b", "x/.."] {
            let job = BatchJob::new(vec![MediaOp::Rename { template: bad.into() }]);
            let err = plan(&job, &v(&["/pics/a/b/photo1.jpg"]))
                .expect_err(&format!("expected {bad:?} to be refused"));
            assert!(err.contains("folder"), "refusal reason for {bad:?}: {err}");
        }
    }

    #[test]
    fn cpe_1623_unicode_lookalike_slash_characters_are_accepted_not_path_separators() {
        // Resolves the auditor's inconclusive finding definitively: U+2215 (DIVISION SLASH), U+FF0F
        // (FULLWIDTH SOLIDUS), and U+FF3C (FULLWIDTH REVERSE SOLIDUS) are distinct Unicode scalars from
        // ASCII '/' and '\\' — the char-based `contains` checks in template_escapes_directory correctly do
        // NOT match them, so validate() accepts these templates, and split()/join() (which only ever split
        // on literal '/'/'\\') treat the character as an ordinary part of the filename, not a separator.
        // Proven with REAL files on disk (not just string assertions) so this is authoritative, not a
        // guess about OS behaviour.
        let dir = scratch("cpe1623-unicode-lookalikes");
        for (tag, ch) in [("division-slash", '\u{2215}'), ("fullwidth-solidus", '\u{FF0F}'), ("fullwidth-reverse-solidus", '\u{FF3C}')] {
            let input = dir.path().join(format!("{tag}.jpg"));
            std::fs::write(&input, b"x").unwrap();
            let template = format!("sub{ch}evil");

            let job = BatchJob::new(vec![MediaOp::Rename { template: template.clone() }]);
            assert!(validate(&job).is_ok(), "{tag}: {template:?} must validate (not an ASCII separator)");

            let out = plan(&job, &[input.to_string_lossy().to_string()])
                .unwrap_or_else(|e| panic!("{tag}: {template:?} must plan successfully: {e}"));
            // The computed output is a SINGLE file directly inside the input's own directory — the
            // look-alike character became part of the filename, not a directory boundary.
            let expected = dir.path().join(format!("{template}.jpg"));
            assert_eq!(out[0].output, expected.to_string_lossy(), "{tag}: output must stay in the input's own folder");

            // Real-filesystem proof: the OS actually accepts this as one ordinary filename (it's not one
            // of NTFS's 9 reserved characters `< > : " / \ | ? *`).
            std::fs::write(&out[0].output, b"written").unwrap();
            assert!(std::path::Path::new(&out[0].output).is_file(), "{tag}: the OS must accept this as a real filename");
        }
        let _ = std::fs::remove_dir_all(dir.path());
    }

    #[test]
    fn cpe_1623_ordinary_rename_templates_without_separators_are_unaffected() {
        // No new false alarms: a template with no separator/".." must plan exactly as before.
        let job = BatchJob::new(vec![MediaOp::Rename { template: "{stem}-final".into() }]);
        let out = plan(&job, &v(&["/pics/vacation/photo1.jpg", "/pics/vacation/photo2.jpg"])).unwrap();
        assert_eq!(out[0].output, "/pics/vacation/photo1-final.jpg");
        assert_eq!(out[1].output, "/pics/vacation/photo2-final.jpg");
    }

    #[test]
    fn cpe_1623_directory_traversal_rename_is_refused_with_real_files_on_disk() {
        // The ticket's exact reproduction, on-disk: a rename template that walks "up and over" into a
        // sibling directory containing an unrelated victim file. plan() must refuse the WHOLE batch
        // ([`Err`]) before any bytes are ever read or written — never mind execute_plan. Two levels deep
        // so "..\\..\\" from innocuous.jpg's own directory lands EXACTLY on `victim_dir` (one ".." undoes
        // "traversal_workdir", the second undoes "a") — a precise demonstration, not just "escapes
        // somewhere" (confirmed against the pre-fix code as a negative control: this exact template
        // silently overwrote `important.jpg` there — see the ticket / PR description for the details).
        let root = scratch("cpe1623-traversal");
        let workdir = root.path().join("a").join("traversal_workdir");
        let victim_dir = root.path().join("cpe1613_traversal_victim");
        std::fs::create_dir_all(&workdir).unwrap();
        std::fs::create_dir_all(&victim_dir).unwrap();
        let input = workdir.join("innocuous.jpg");
        std::fs::write(&input, b"innocuous original bytes").unwrap();
        let victim = victim_dir.join("important.jpg");
        let victim_original = b"VICTIM ORIGINAL CONTENT - must not be touched".to_vec();
        std::fs::write(&victim, &victim_original).unwrap();

        let job = BatchJob::new(vec![MediaOp::Rename {
            template: "..\\..\\cpe1613_traversal_victim\\important".into(),
        }]);
        assert!(job.non_destructive, "the default, supposedly-safe mode — matches the ticket's repro");

        let err = plan(&job, &[input.to_string_lossy().to_string()])
            .expect_err("the traversal template must be refused, not silently planned");
        assert!(err.contains("folder"), "refusal reason: {err}");

        // Byte-for-byte proof, not a trust-the-return-value check: read the victim back off disk.
        assert_eq!(std::fs::read(&victim).unwrap(), victim_original, "the victim file must be untouched");

        let _ = std::fs::remove_dir_all(root.path());
    }

    #[test]
    fn cpe_1623_non_destructive_mode_steps_around_a_real_pre_existing_unrelated_file() {
        // Fix #3 (the non-traversal half): even with no ".." in sight, a rename that happens to land on
        // the name of a REAL file this batch never selected must not silently overwrite it — plan()'s
        // "non-destructive" promise now checks real disk state, not just the batch's own working set, so
        // it disambiguates past the occupied name exactly like a same-batch collision.
        let dir = scratch("cpe1623-foreign-collision");
        let input = dir.path().join("photo.jpg");
        std::fs::write(&input, b"input bytes").unwrap();
        let foreign = dir.path().join("vacation.jpg"); // NOT part of the batch's inputs
        let foreign_original = b"unrelated pre-existing file".to_vec();
        std::fs::write(&foreign, &foreign_original).unwrap();

        let job = BatchJob::new(vec![MediaOp::Rename { template: "vacation".into() }]); // would collide
        let out = plan(&job, &[input.to_string_lossy().to_string()]).unwrap();

        assert_ne!(out[0].output, foreign.to_string_lossy(), "must not plan straight onto the foreign file");
        assert_eq!(out[0].output, dir.path().join("vacation-2.jpg").to_string_lossy());
        assert_eq!(std::fs::read(&foreign).unwrap(), foreign_original, "the foreign file must be untouched");

        let _ = std::fs::remove_dir_all(dir.path());
    }
}
