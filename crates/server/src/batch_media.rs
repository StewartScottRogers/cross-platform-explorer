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
//! `batch_execute::execute_plan_walk`'s pre-write re-check, via [`classify_output_containment`]) and the
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
//! [`classify_output_containment`] check `plan()` uses — see that module's doc for the refusal. The engine is
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
//! explicitly in [`classify_output_containment`] itself (not just `validate()`'s template-level `:` rejection
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
//! blind spot. The containment check therefore resolves the final component before trusting `out_dir ==
//! dir` at all.
//!
//! **Resolve the output's IDENTITY; stop pattern-matching link shapes (CPE-1642).** Three rounds of audit
//! on the fix above each closed the shape that had been demonstrated and left the next one open: raw text
//! → one-hop links → chains → contended reads. Two escapes survived: (A) a same-directory symlink
//! **chain**, because the check read exactly ONE hop and compared only that target's *directory* — so
//! `linkA → linkB → outside/victim` passed, and the outside victim's bytes really did change; and (B) the
//! Windows hard-link count **failed open**, defaulting to "1 link" whenever its `GENERIC_READ` open
//! failed, so an ordinary unprivileged process holding the file with `share_mode(0)` (or an AV scanner,
//! or another thread of the same batch) turned a fail-closed rule into a fail-open one. The space of link
//! shapes is not bounded by our imagination, so the check no longer enumerates it: it resolves the
//! output's **true filesystem identity** — `(volume serial, file index)` on Windows,
//! `(dev, ino)` on Unix — and compares identities. Chains are walked to their real end, hard links are
//! settled by a census of the one folder the user selected, and **every failure to establish identity is
//! a refusal**, never a pass. See [`classify_output_containment`] and [`resolve_output_containment`].
//! That also let the one deliberate false positive go: a multiply-linked file whose every name is inside
//! the selected folder reaches nothing outside it, and is now correctly allowed. Refusals distinguish
//! "provably left the folder" from "couldn't be verified" ([`Containment`]) — telling a user their file
//! escaped when it demonstrably didn't is its own defect.
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
/// through both here and [`classify_output_containment`]'s directory-identity comparison for a
/// **bare-filename input** (no directory component at all, so both `dir` and the computed `out_dir` are
/// textually empty and compare equal): a classic Windows drive-relative reference, which resolves against
/// drive `C:`'s *current directory* at write time — not the folder the user picked. Rejecting `:` outright
/// closes it at this field-level layer; [`classify_output_containment`] carries a second, independent guard
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
/// the identical containment check `plan()` uses, via the shared [`classify_output_containment`] — its own
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

/// Per-batch memo shared by every containment decision — threaded through [`classify_output_containment`]
/// (used by both this module's `plan()` and, as of the IPC-bypass fix,
/// [`crate::batch_execute::execute_plan_walk`]) so an entire batch pays each *directory-level* filesystem
/// question exactly once. Three memos, all keyed by directory path string:
///
/// - `parents` — [`path_key`]'s canonicalized-parent memo (CPE-1613; the original meaning of this type,
///   which used to be a bare `HashMap` type alias).
/// - `dir_ids` — a directory's [`FileIdentity`] (CPE-1642), resolved through links.
/// - `dir_scans` — a directory's identity→name-count census ([`DirLinkScan`]), built lazily and only when
///   an output turns out to be multiply-linked.
///
/// **Deliberately holds no memo for an individual output path's own probe.** Every call re-probes the
/// `output` it is asked about, so calling the containment check again immediately before a write (the
/// per-write re-check CPE-1624 adds) genuinely re-resolves that file's identity rather than replaying a
/// plan-time answer — the cache narrows the TOCTOU window instead of widening it. Only facts about
/// *directories* (which the batch neither creates nor moves) are reused.
#[derive(Debug, Default)]
pub(crate) struct ParentCache {
    parents: std::collections::HashMap<String, Option<std::path::PathBuf>>,
    dir_ids: std::collections::HashMap<String, Option<FileIdentity>>,
    dir_scans: std::collections::HashMap<String, Option<DirLinkScan>>,
}

impl ParentCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A directory's [`FileIdentity`], memoized. `""` (a bare-filename input's "directory") means the
    /// process's current directory, spelled `"."` — the same place the OS would resolve such a path
    /// against. `None` means the identity could not be established, which every caller must treat as a
    /// containment FAILURE, never as a pass.
    fn dir_identity(&mut self, dir: &str) -> Option<FileIdentity> {
        let key = if dir.is_empty() { "." } else { dir };
        if let Some(hit) = self.dir_ids.get(key) {
            return *hit;
        }
        let id = identity_following_links(std::path::Path::new(key));
        self.dir_ids.insert(key.to_string(), id);
        id
    }

    /// A directory's identity→name-count census ([`DirLinkScan`]), memoized. Built at most once per
    /// directory per batch, and only reached when an output is actually multiply-linked.
    fn dir_scan(&mut self, dir: &str) -> Option<&DirLinkScan> {
        let key = if dir.is_empty() { "." } else { dir };
        self.dir_scans
            .entry(key.to_string())
            .or_insert_with(|| scan_dir_link_census(std::path::Path::new(key)))
            .as_ref()
    }
}

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
            .parents
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
    let mut cache = ParentCache::new();
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
///   resolution.
/// - **Findings A & B of CPE-1642 — pattern-matching link *shapes* can't be finished.** The fix for C
///   read exactly ONE symlink hop and defaulted a failed hard-link-count read to "not linked", so a
///   two-hop symlink chain and a merely-contended file both escaped. Both are gone: this now resolves
///   the output's real filesystem IDENTITY once — see [`resolve_output_containment`] — and every failure
///   to establish that identity is a refusal, never a pass.
///
/// Boolean form, kept for the tests that assert the yes/no answer directly. **Production callers use
/// [`classify_output_containment`]** — collapsing "provably escapes" and "couldn't be verified" into one
/// `true` is what produced a refusal message that stated something untrue (CPE-1642).
#[cfg(test)]
pub(crate) fn output_escapes_input_dir(input: &str, output: &str, parent_cache: &mut ParentCache) -> bool {
    classify_output_containment(input, output, parent_cache) != Containment::Inside
}

/// **The one shared containment check every production caller uses** — the three-valued form of the
/// question above (CPE-1642), so a caller rendering a refusal can say what is *actually* true. Reporting "would land outside its own
/// input's folder" for an output this check merely could not verify is itself a defect: nothing left the
/// folder, the check just couldn't prove it hadn't.
///
/// Order of decisions, cheapest and most structural first:
/// 1. Findings A/B above (a dot-segment final component; a drive-relative output for a bare-filename
///    input) — pure text, no filesystem.
/// 2. Directory identity: does `output`'s directory resolve to `input`'s own? Textually-identical
///    directories (the overwhelmingly common case) skip [`path_key`] entirely; anything else pays the
///    O(1)-amortized resolution.
/// 3. **Output identity** ([`resolve_output_containment`]): what the final component actually IS on disk,
///    resolved through the whole link chain to a real (volume, file-index)/(dev, ino) identity.
///
/// **CPE-1624 seam:** this is a pure function of `(input, output, cache)` with no plan-time state — step 3
/// re-probes `output` on every call (see [`ParentCache`]) — so calling it again immediately before each
/// write is a genuine re-resolution, not a replay of the planning answer.
pub(crate) fn classify_output_containment(
    input: &str,
    output: &str,
    parent_cache: &mut ParentCache,
) -> Containment {
    let out_final = output.rsplit(['/', '\\']).next().unwrap_or(output);
    if out_final == "." || out_final == ".." {
        return Containment::Escapes;
    }

    let (dir, _, _) = split(input);
    let (out_dir, out_stem, _) = split(output);
    if dir.is_empty() && out_stem.contains(':') {
        return Containment::Escapes;
    }
    // `out_dir == dir` is the cheap fast path (identical TEXT ⇒ identical place); only a genuinely
    // different directory string pays for `path_key`'s resolution, which still folds trailing separators,
    // `.`/`..` segments, junctions and case differences into one answer.
    if out_dir != dir && path_key(&out_dir, parent_cache) != path_key(&dir, parent_cache) {
        return Containment::Escapes;
    }

    // The output's *directory* is the input's own. That is necessary but NOT sufficient: the final
    // component itself can be a link (or one of several hard-linked names) whose real data lives
    // elsewhere. Resolve its identity.
    resolve_output_containment(output, &dir, parent_cache)
}

/// The verdict on one planned output (CPE-1642). Three-valued on purpose: "I could not establish this
/// output's identity" is a distinct fact from "this output provably leaves the folder", and conflating
/// them produced a refusal message that told the user something untrue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Containment {
    /// Proven to land inside the input's own folder.
    Inside,
    /// Proven to land somewhere else.
    Escapes,
    /// Could not be established — **treated exactly like `Escapes` by every caller** (fail closed); the
    /// payload is the true reason, for an accurate refusal message.
    Unverifiable(&'static str),
}

/// A file's **true filesystem identity** (CPE-1642): the pair every OS uses to answer "are these two
/// names the same object?" — `(volume serial number, 64-bit file index)` on Windows via
/// `GetFileInformationByHandle`, `(dev, ino)` on Unix. Comparing identities is what makes symlink chains,
/// junctions, hard links and any future link shape collapse into ONE question, instead of a growing
/// catalogue of path-string patterns to pattern-match (the approach CPE-1623 exhausted).
///
/// `index` is `u128` so both platforms' widest form fits without truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FileIdentity {
    volume: u64,
    index: u128,
}

impl FileIdentity {
    /// **An identity that identifies nothing (CPE-1642, reviewer finding F2).** `GetFileInformationByHandle`
    /// is documented as not supplying a usable file index on several network redirectors — it *succeeds* and
    /// hands back a zero index. Every object on such a volume would then carry the SAME identity, so the two
    /// places that compare identities — the landing directory versus the selected directory in
    /// [`resolve_output_containment`], and the census lookup in [`real_target_containment`] — would judge
    /// every unrelated directory "the same place" and wave a symlink out of the folder straight through.
    /// That is a fail-open the ticket's own rule forbids, and it is invisible from the call site because the
    /// API reported success. Zero is not a legal value on either platform (`ino`/`st_dev` 0 denotes no file
    /// / no device on Unix; a zero file index or volume serial is Windows' "not supported here"), so a zero
    /// in either half means **identity unknown**, and every caller must refuse rather than compare it.
    fn is_degenerate(self) -> bool {
        self.volume == 0 || self.index == 0
    }
}

/// Gate a freshly-probed [`FileFacts`] on [`FileIdentity::is_degenerate`]: an identity that identifies
/// nothing is *unreadable*, never a real one. Pure, so the volume this defends against (which CI has no
/// access to) can be reproduced in a unit test by injecting the degenerate value directly.
fn facts_or_unreadable(facts: FileFacts) -> Probe {
    if facts.id.is_degenerate() {
        Probe::Unreadable
    } else {
        Probe::Real(facts)
    }
}

/// The [`identity_following_links`] counterpart of [`facts_or_unreadable`] — `None` (which
/// [`ParentCache::dir_identity`]'s callers already treat as a containment FAILURE) for a degenerate
/// identity.
fn identity_or_none(id: FileIdentity) -> Option<FileIdentity> {
    if id.is_degenerate() {
        None
    } else {
        Some(id)
    }
}

/// What one identity probe of an existing path yields: who it is, how many names it has, and whether it
/// is a directory.
#[derive(Debug, Clone, Copy)]
struct FileFacts {
    id: FileIdentity,
    /// Hard-link count — how many names in the whole filesystem refer to this same data.
    links: u64,
    is_dir: bool,
}

/// The outcome of probing a path **without following links** — the first question
/// [`resolve_output_containment`] asks about an output.
enum Probe {
    /// Nothing exists at this path (so there is no identity to alias).
    Absent,
    /// A symlink or junction; its chain has to be walked before anything can be said.
    Link,
    /// A real file or directory, with its identity established.
    Real(FileFacts),
    /// Something is there but its identity could NOT be established. **Never treat as `Absent`** — this
    /// is exactly CPE-1642 finding B (a contended open used to fall back to "assume unlinked").
    Unreadable,
}

/// An identity→name-count census of ONE directory (CPE-1642), used to decide whether a multiply-linked
/// output's other names are all accounted for inside the folder the user selected.
#[derive(Debug, Default)]
struct DirLinkScan {
    counts: std::collections::HashMap<FileIdentity, u64>,
    /// At least one entry's identity could not be read, so `counts` may undercount — a shortfall must
    /// then be reported as *unverifiable*, not as a proven escape.
    incomplete: bool,
}

const WHY_PROBE_FAILED: &str = "the planned output exists but its filesystem identity could not be read \
                               (it may be locked by another process)";
const WHY_CHAIN_FAILED: &str = "the planned output is a link whose chain could not be followed to a real \
                               location (unreadable, cyclic, or too many hops)";
const WHY_DIR_IDENTITY_FAILED: &str = "the folder a linked output resolves into could not be identified";
const WHY_CENSUS_FAILED: &str = "the selected folder could not be enumerated to account for the output's \
                                 other hard links";

/// **The identity half of the containment check (CPE-1642).** `output` is already known to sit in the
/// input's own directory *by path*; this asks what it actually IS on disk and where the bytes a write
/// would land on actually live. Replaces the previous shape-matching (`link_alias_escapes` +
/// `hard_link_count`), which read exactly one symlink hop and defaulted an unreadable hard-link count to
/// "not linked".
///
/// 1. **Probe the final component without following links.** Absent ⇒ nothing to alias, `Inside` (the
///    overwhelmingly common case: one stat/attribute open, no canonicalize). Unreadable ⇒ `Unverifiable`
///    — *this is finding B*: a merely contended file must never be waved through.
/// 2. **A link ⇒ walk the WHOLE chain** ([`follow_link_chain`]), not one hop, resolving each relative
///    target against its own link's parent exactly as the OS does, and refusing on an unreadable link,
///    a cycle, or an absurd chain length. `read_link` (not `canonicalize`) so a **dangling** target — the
///    shape that needs no batch-job flag to exploit, since `Path::is_file()` is `false` for it — still
///    resolves. The chain's terminal path is where a write truly lands, so its *parent directory's*
///    identity, not its spelling, is compared against the selected folder's.
/// 3. **A real file ⇒ ask how many names it has.** One name ⇒ `Inside`. More than one ⇒ the other names
///    are the question, and they are answerable *cheaply and exactly for the case that matters*: census
///    the selected folder once ([`DirLinkScan`]) and count how many of its entries share this identity.
///    All of them accounted for inside ⇒ `Inside` — the "all names harmlessly inside the folder" case
///    CPE-1623 had to refuse as a known false positive is now correctly allowed. Fewer ⇒ a name provably
///    exists outside the folder ⇒ `Escapes`. Census unavailable/incomplete ⇒ `Unverifiable`.
///
/// Every arm that cannot prove containment returns [`Containment::Unverifiable`], and every caller
/// refuses on it. There is no path through this function that answers `Inside` by default.
fn resolve_output_containment(output: &str, input_dir: &str, cache: &mut ParentCache) -> Containment {
    match probe_no_follow(std::path::Path::new(output)) {
        Probe::Absent => Containment::Inside,
        Probe::Unreadable => Containment::Unverifiable(WHY_PROBE_FAILED),
        Probe::Real(facts) => real_target_containment(facts, input_dir, cache),
        Probe::Link => {
            let terminal = match follow_link_chain(output) {
                Ok(t) => t,
                Err(why) => return Containment::Unverifiable(why),
            };
            let Some(parent) = terminal.parent() else {
                return Containment::Unverifiable(WHY_DIR_IDENTITY_FAILED);
            };
            let landing = cache.dir_identity(&parent.to_string_lossy());
            let selected = cache.dir_identity(input_dir);
            match (landing, selected) {
                (Some(a), Some(b)) if a == b => {}
                (Some(_), Some(_)) => return Containment::Escapes,
                _ => return Containment::Unverifiable(WHY_DIR_IDENTITY_FAILED),
            }
            // The chain lands in the selected folder itself — but its terminal may still be one of
            // several hard-linked names, so it gets the same identity treatment as a direct output.
            match probe_no_follow(&terminal) {
                Probe::Absent => Containment::Inside, // dangling, but dangling *inside* the folder
                Probe::Real(facts) => real_target_containment(facts, input_dir, cache),
                Probe::Link => Containment::Unverifiable(WHY_CHAIN_FAILED),
                Probe::Unreadable => Containment::Unverifiable(WHY_PROBE_FAILED),
            }
        }
    }
}

/// Step 3 of [`resolve_output_containment`]: a real (non-link) target whose directory is already known to
/// be the selected folder. The only remaining way for a write here to touch data outside the folder is a
/// hard link — a second name for the same identity living somewhere else.
fn real_target_containment(facts: FileFacts, input_dir: &str, cache: &mut ParentCache) -> Containment {
    if facts.is_dir || facts.links <= 1 {
        return Containment::Inside;
    }
    let Some(scan) = cache.dir_scan(input_dir) else {
        return Containment::Unverifiable(WHY_CENSUS_FAILED);
    };
    let inside = scan.counts.get(&facts.id).copied().unwrap_or(0);
    if inside >= facts.links {
        // Every one of this file's names is a name in the selected folder — nothing can be reached
        // outside it, so this is genuinely safe (CPE-1623 had to refuse this case for want of a cheap
        // way to prove it).
        Containment::Inside
    } else if scan.incomplete {
        Containment::Unverifiable(WHY_CENSUS_FAILED)
    } else {
        // The file has more names than this folder holds: at least one of them is somewhere else.
        Containment::Escapes
    }
}

/// Walk a symlink/junction chain from `start` to the real path it ultimately names (CPE-1642 finding A —
/// the previous code read exactly ONE hop, so `linkA → linkB → outside/victim` passed containment because
/// `linkB` was textually in the right folder). A relative target resolves against its own link's parent,
/// the same rule the OS applies. Stops at the first non-link, **including a path that doesn't exist** —
/// that is the dangling case, and where it *would* be created is exactly what containment must judge.
///
/// Fails closed (`Err`) on an unreadable link, a chain that revisits its own previous hop, or one longer
/// than [`MAX_LINK_HOPS`] (which also bounds any cycle a plain equality check misses).
fn follow_link_chain(start: &str) -> Result<std::path::PathBuf, &'static str> {
    const MAX_LINK_HOPS: usize = 40;
    let mut current = std::path::PathBuf::from(start);
    for _ in 0..MAX_LINK_HOPS {
        match probe_no_follow(&current) {
            Probe::Link => {}
            Probe::Absent | Probe::Real(_) => return Ok(current),
            Probe::Unreadable => return Err(WHY_PROBE_FAILED),
        }
        let target = std::fs::read_link(&current).map_err(|_| WHY_CHAIN_FAILED)?;
        let next = if target.is_absolute() {
            target
        } else {
            current.parent().map(|p| p.join(&target)).unwrap_or(target)
        };
        if next == current {
            return Err(WHY_CHAIN_FAILED);
        }
        current = next;
    }
    Err(WHY_CHAIN_FAILED)
}

/// Census one directory's entries by [`FileIdentity`] — how many NAMES in this folder refer to each
/// distinct file. Bounded to the single selected directory (never a volume walk), non-recursive, and
/// reached only when an output is actually multiply-linked, so its cost is proportional to the one folder
/// the user picked and is memoized per batch by [`ParentCache::dir_scan`].
///
/// Symlink entries are skipped deliberately: a symlink *pointing at* the file is not another hard link to
/// it, and counting one would inflate the census and could mask a real out-of-folder name.
fn scan_dir_link_census(dir: &std::path::Path) -> Option<DirLinkScan> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut scan = DirLinkScan::default();
    for entry in entries {
        let Ok(entry) = entry else {
            scan.incomplete = true;
            continue;
        };
        match entry.file_type() {
            Ok(t) if t.is_symlink() => continue,
            Ok(_) => {}
            Err(_) => {
                scan.incomplete = true;
                continue;
            }
        }
        match probe_no_follow(&entry.path()) {
            Probe::Real(facts) => *scan.counts.entry(facts.id).or_insert(0) += 1,
            Probe::Absent => {} // vanished mid-scan; it holds no name now
            Probe::Link | Probe::Unreadable => scan.incomplete = true,
        }
    }
    Some(scan)
}

/// Probe a path's identity **without following links** — Unix reads it straight off the one
/// `symlink_metadata` call this module already made before CPE-1642 (`dev`/`ino`/`nlink` are all on
/// `MetadataExt`, no extra syscall).
#[cfg(unix)]
fn probe_no_follow(path: &std::path::Path) -> Probe {
    use std::os::unix::fs::MetadataExt;
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Probe::Link,
        Ok(meta) => facts_or_unreadable(FileFacts {
            id: FileIdentity { volume: meta.dev(), index: u128::from(meta.ino()) },
            links: meta.nlink(),
            is_dir: meta.is_dir(),
        }),
        // ENOTDIR (a path component isn't a directory) means nothing can exist at this path either.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(20) => Probe::Absent,
        Err(_) => Probe::Unreadable,
    }
}

/// Windows identity probe. `std::os::windows::fs::MetadataExt`'s `volume_serial_number()`/`file_index()`/
/// `number_of_links()` are still gated behind the unstable `windows_by_handle` feature
/// (rust-lang/rust#63010) on stable Rust, so this makes the same `CreateFileW` +
/// `GetFileInformationByHandle` call the std wrapper would, via the `windows` crate already vendored for
/// [`crate::high_contrast`] — one open per probe, the same cost as the `symlink_metadata` it replaces.
///
/// **Two details are load-bearing, both CPE-1642 finding B:**
/// - `FILE_READ_ATTRIBUTES` (not `GENERIC_READ`) as the desired access. Windows' share-mode conflict check
///   ignores the attribute-read rights, so this open still succeeds against a file another process holds
///   with `share_mode(0)` — the exact contention that made the old `GENERIC_READ` open fail and silently
///   report "1 link".
/// - A failed open is classified by its real error: only "there is nothing here" (`NotFound` and the
///   malformed-path codes, which can never name an existing file) becomes [`Probe::Absent`]. Every other
///   failure — sharing violation, access denied, anything unclassified — is [`Probe::Unreadable`], which
///   the caller refuses on.
///
/// `FILE_FLAG_BACKUP_SEMANTICS` lets the same call open directories (needed to identify a folder);
/// `FILE_FLAG_OPEN_REPARSE_POINT` keeps it from following a link, so the probe describes the name itself.
/// A reparse point that std does not consider a symlink (a cloud placeholder, a dedup stub) is reported as
/// the real file it is, not as a link — those are ordinary files to every reader, and calling them links
/// would strand ordinary batches in OneDrive-backed folders.
///
/// **The path handed to `CreateFileW` goes through [`verbatim_wide`] first (CPE-1642, reviewer finding
/// REV-G/REV-G2).** Without it the probe and the writer address different sets of files: every write in
/// this crate goes through `std::fs`, which applies the same `\\?\` transformation and therefore reaches
/// past `MAX_PATH`, while a raw `CreateFileW` is capped at it. That mismatch made an over-`MAX_PATH` output
/// fail to open with `ERROR_PATH_NOT_FOUND`, classify as [`Probe::Absent`], and fail OPEN.
#[cfg(windows)]
fn probe_no_follow(path: &std::path::Path) -> Probe {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let wide = verbatim_wide(path);
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 string kept alive for the whole call. Attributes-only
    // open of an already-existing file (`OPEN_EXISTING`, full sharing) — no create/write/truncate side
    // effect, and no data access. The handle is closed on every path before returning.
    unsafe {
        let handle = match CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            HANDLE::default(),
        ) {
            Ok(h) => h,
            Err(_) => return classify_open_failure(GetLastError().0, wide.len()),
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        let ok = GetFileInformationByHandle(handle, &mut info).is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return Probe::Unreadable;
        }
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
            && std::fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(true)
        {
            return Probe::Link;
        }
        facts_or_unreadable(FileFacts {
            id: FileIdentity {
                volume: u64::from(info.dwVolumeSerialNumber),
                index: (u128::from(info.nFileIndexHigh) << 32) | u128::from(info.nFileIndexLow),
            },
            links: u64::from(info.nNumberOfLinks),
            is_dir: info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
        })
    }
}

/// **`std`'s `maybe_verbatim`, reimplemented (CPE-1642, reviewer finding REV-G).** Every write in this crate
/// goes through `std::fs`, which runs each path through this same transformation before calling into Win32
/// — so it reaches paths longer than `MAX_PATH`. The identity probe calls Win32 directly, and without the
/// same transformation it is capped at `MAX_PATH` while the writer is not: the probe would report "nothing
/// is here" for a file the writer could happily overwrite. The probe must address exactly the set of files
/// the writer does, or the safety check is checking a different filesystem.
///
/// The `\\?\` prefix **disables all path normalisation** in the kernel, so it may only be applied to a path
/// that is already fully-qualified, `.`/`..`-free and back-slash-separated. `GetFullPathNameW` produces
/// exactly that; the prefix is then chosen by shape, mirroring `std`'s table:
/// `C:\…` ⇒ `\\?\C:\…`, `\\.\…` ⇒ `\\?\…`, `\\server\share` ⇒ `\\?\UNC\server\share`, and an
/// already-verbatim (`\\?\`) or NT (`\??\`) path is returned untouched.
///
/// **Cost:** a path already verbatim, or shorter than the legacy limit and not UNC, is returned after two
/// slice comparisons and a length test — no syscall, no extra allocation beyond the `Vec<u16>` the raw
/// encoding already required. The per-entry directory census therefore pays nothing for this.
#[cfg(windows)]
fn verbatim_wide(path: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetFullPathNameW;

    const SEP: u16 = b'\\' as u16;
    const ALT_SEP: u16 = b'/' as u16;
    const QUERY: u16 = b'?' as u16;
    const COLON: u16 = b':' as u16;
    const DOT: u16 = b'.' as u16;
    /// `\\?\`
    const VERBATIM: &[u16] = &[SEP, SEP, QUERY, SEP];
    /// `\??\`
    const NT: &[u16] = &[SEP, QUERY, QUERY, SEP];
    /// `\\?\UNC\`
    const UNC: &[u16] = &[SEP, SEP, QUERY, SEP, b'U' as u16, b'N' as u16, b'C' as u16, SEP];
    /// `CreateDirectoryW`'s 248, the tighter of the two Windows limits — the same number `std` uses, so the
    /// probe switches to the verbatim form no later than the writer does.
    const LEGACY_MAX_PATH: usize = 248;

    let raw: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    if raw.starts_with(VERBATIM) || raw.starts_with(NT) || raw.len() <= 1 {
        return raw;
    }
    if raw.len() < LEGACY_MAX_PATH && !matches!(raw.as_slice(), [SEP | ALT_SEP, SEP | ALT_SEP, ..]) {
        return raw;
    }

    // Fully qualify first — the verbatim prefix would otherwise freeze a relative path, a `/`, or a `..`
    // segment into a name the filesystem cannot resolve.
    let mut buf = vec![0u16; raw.len().max(LEGACY_MAX_PATH) + 64];
    loop {
        // SAFETY: `raw` is NUL-terminated and outlives the call; `buf` is a live, correctly-sized slice.
        let n = unsafe { GetFullPathNameW(PCWSTR(raw.as_ptr()), Some(&mut buf), None) } as usize;
        if n == 0 {
            // Couldn't qualify it; hand back the raw form. `classify_open_failure`'s length guard then
            // refuses to read a MAX_PATH truncation failure as "nothing is there".
            return raw;
        }
        if n < buf.len() {
            buf.truncate(n);
            break;
        }
        buf = vec![0u16; n + 1];
    }

    let (prefix, body): (&[u16], &[u16]) = match buf.as_slice() {
        // `C:\…` — a drive-rooted path.
        [_, COLON, SEP, ..] => (VERBATIM, &buf[..]),
        // `\\.\…` — a device path; `\\?\` is the same namespace without normalisation.
        [SEP, SEP, DOT, SEP, ..] => (VERBATIM, &buf[4..]),
        // Already verbatim / NT — leave alone.
        [SEP, SEP, QUERY, SEP, ..] | [SEP, QUERY, QUERY, SEP, ..] => (&[], &buf[..]),
        // `\\server\share\…` — the UNC spelling drops the leading `\\`.
        [SEP, SEP, ..] => (UNC, &buf[2..]),
        // Anything else (a rooted-but-driveless path, say) gains nothing from the prefix.
        _ => (&[], &buf[..]),
    };
    let mut out = Vec::with_capacity(prefix.len() + body.len() + 1);
    out.extend_from_slice(prefix);
    out.extend_from_slice(body);
    out.push(0);
    out
}

/// Split a failed Windows open into "nothing is there" versus "something is there and I couldn't read
/// it". Only codes that mean the path names nothing at all may become [`Probe::Absent`]; `std`'s own error
/// classification covers the not-found family, and the malformed-path codes are listed explicitly because
/// a name the filesystem rejects outright can never denote an existing file either.
///
/// **Belt and braces for the `MAX_PATH` mismatch (CPE-1642, reviewer finding REV-G).** [`verbatim_wide`]
/// is what keeps the probe and the writer addressing the same files; this is the second line of defence for
/// when it cannot (a `GetFullPathNameW` that fails, a shape its table leaves unprefixed). At or past
/// `MAX_PATH`, a "path not found"/"invalid name" is exactly what a *truncation* looks like, and the one
/// thing it must never be read as is "nothing is there" — so at that length those codes are
/// [`Probe::Unreadable`], which both callers refuse on. `ERROR_FILE_NOT_FOUND` (the ordinary "this output
/// doesn't exist yet" answer, and the overwhelmingly common one) is deliberately NOT in that set, so the
/// common path costs nothing.
#[cfg(windows)]
fn classify_open_failure(code: u32, wide_len: usize) -> Probe {
    const ERROR_INVALID_NAME: u32 = 123;
    const ERROR_BAD_PATHNAME: u32 = 161;
    const ERROR_FILENAME_EXCED_RANGE: u32 = 206;
    const ERROR_DIRECTORY: u32 = 267;
    const ERROR_PATH_NOT_FOUND: u32 = 3;
    /// Windows' unprefixed path limit, NUL included.
    const MAX_PATH: usize = 260;

    if wide_len >= MAX_PATH
        && matches!(
            code,
            ERROR_PATH_NOT_FOUND | ERROR_INVALID_NAME | ERROR_BAD_PATHNAME | ERROR_FILENAME_EXCED_RANGE
        )
    {
        return Probe::Unreadable;
    }

    let kind = std::io::Error::from_raw_os_error(code as i32).kind();
    if kind == std::io::ErrorKind::NotFound
        || matches!(code, ERROR_INVALID_NAME | ERROR_BAD_PATHNAME | ERROR_DIRECTORY)
    {
        Probe::Absent
    } else {
        Probe::Unreadable
    }
}

/// Fail-closed stub for any platform that is neither Windows nor Unix: with no way to establish identity,
/// an existing output can never be proven contained. Nothing this crate ships targets such a platform
/// (CI is Windows + macOS + Linux); this exists so the module cannot silently compile into a
/// pattern-matching-only build.
#[cfg(not(any(windows, unix)))]
fn probe_no_follow(path: &std::path::Path) -> Probe {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Probe::Unreadable,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Probe::Absent,
        Err(_) => Probe::Unreadable,
    }
}

/// A path's identity **with** links followed — used for directories (the folder a resolved chain lands
/// in, and the selected folder itself), where the target's identity is the whole point.
#[cfg(unix)]
fn identity_following_links(path: &std::path::Path) -> Option<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).ok()?;
    identity_or_none(FileIdentity { volume: meta.dev(), index: u128::from(meta.ino()) })
}

/// Windows counterpart. Uses [`verbatim_wide`] for the same reason [`probe_no_follow`] does — the selected
/// folder and a resolved chain's landing folder can both sit past `MAX_PATH`, and a directory this could not
/// open would be reported as "identity unknown", which refuses the whole batch (CPE-1642 REV-G).
#[cfg(windows)]
fn identity_following_links(path: &std::path::Path) -> Option<FileIdentity> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let wide = verbatim_wide(path);
    // SAFETY: as `probe_no_follow` — attributes-only `OPEN_EXISTING` open of a NUL-terminated path kept
    // alive for the call, handle closed on every path. No `FILE_FLAG_OPEN_REPARSE_POINT` here: this one
    // deliberately follows links to identify the real directory.
    unsafe {
        let handle = CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            HANDLE::default(),
        )
        .ok()?;
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        let ok = GetFileInformationByHandle(handle, &mut info).is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return None;
        }
        identity_or_none(FileIdentity {
            volume: u64::from(info.dwVolumeSerialNumber),
            index: (u128::from(info.nFileIndexHigh) << 32) | u128::from(info.nFileIndexLow),
        })
    }
}

#[cfg(not(any(windows, unix)))]
fn identity_following_links(_path: &std::path::Path) -> Option<FileIdentity> {
    None
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
    let mut parent_cache = ParentCache::new();
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
            // its own input already lives in. `classify_output_containment` is the one shared definition
            // of this check — also used by `batch_execute::execute_plan_walk`'s independent pre-write
            // re-check, so there is exactly one place that decides "did this leave the folder?", not two
            // definitions that could drift apart.
            // CPE-1642: three-valued, so the refusal says what is actually TRUE. "Would land outside its
            // own folder" is a lie about an output the check merely could not verify — nothing left the
            // folder in that case; identity resolution failed, and refusing was the safe response.
            match classify_output_containment(input, &output, &mut parent_cache) {
                Containment::Inside => {}
                Containment::Escapes => {
                    return Err(format!(
                        "computed output for \"{input}\" would land at \"{output}\", outside its own \
                         folder — a Convert extension or Rename template can only change a file's \
                         name/extension, never its folder"
                    ));
                }
                Containment::Unverifiable(why) => {
                    return Err(format!(
                        "refusing \"{input}\": couldn't verify that the computed output \"{output}\" \
                         stays inside its own folder — {why}. Nothing was written; this is a refusal to \
                         guess, not a detected escape"
                    ));
                }
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

    // ---- CPE-1642: resolve the output's IDENTITY, don't pattern-match link shapes --------------------
    //
    // The CPE-1623 fix above read exactly ONE symlink hop and defaulted an unreadable hard-link count to
    // "not linked". These tests reproduce the two escapes that left open (findings A and B in the ticket),
    // plus the negative controls that must still be allowed — including the one CPE-1623 had to refuse as
    // a known false positive.

    /// Create a symlink or skip the test with a VISIBLE reason (Windows needs Developer Mode/elevation).
    /// Returns `false` when the caller should return early — never silently passes.
    fn try_symlink(target: &std::path::Path, link: &std::path::Path, test: &str) -> bool {
        match crate::links::create_symlink(&target.to_string_lossy(), &link.to_string_lossy()) {
            Ok(()) => true,
            Err(e) => {
                eprintln!(
                    "SKIPPING {test}: could not create a symlink in this environment ({e}) — Windows \
                     needs Developer Mode or elevation. This test did NOT verify anything."
                );
                false
            }
        }
    }

    /// **CPE-1642 finding A — the ticket's exact PoC.** `linkA → linkB` (relative, same directory) and
    /// `linkB → outside/important.jpg`. The old check read ONE hop, saw that `linkB` was textually in the
    /// selected folder, and returned "contained" — the outside victim's bytes then changed for real.
    /// **Negative control:** identical to the pre-fix demonstration, which returned `false` here.
    #[test]
    fn cpe_1642_two_hop_symlink_chain_escapes() {
        let dir = scratch("cpe1642-symlink-chain");
        let selected = dir.path().join("selected");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&selected).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("important.jpg");
        std::fs::write(&victim, b"victim").unwrap();
        let link_b = selected.join("linkB.jpg");
        let link_a = selected.join("linkA.jpg");
        if !try_symlink(&victim, &link_b, "cpe_1642_two_hop_symlink_chain_escapes") {
            return;
        }
        // Relative target, so hop 1 resolves to a name that is TEXTUALLY inside the selected folder —
        // exactly what fooled the one-hop check.
        if !try_symlink(std::path::Path::new("linkB.jpg"), &link_a, "cpe_1642_two_hop_symlink_chain_escapes")
        {
            return;
        }
        assert_eq!(
            std::fs::read_link(&link_a).unwrap(),
            std::path::PathBuf::from("linkB.jpg"),
            "sanity: hop 1's stored target must be a bare relative name inside the selected folder"
        );

        let input = selected.join("photo.jpg").to_string_lossy().to_string();
        let output = link_a.to_string_lossy().to_string();
        let mut cache = ParentCache::new();
        assert!(
            output_escapes_input_dir(&input, &output, &mut cache),
            "a symlink CHAIN whose far end lands outside the selected folder must be refused — reading \
             only the first hop is what let this through"
        );
    }

    /// **Negative control for the chain fix:** the same two-hop shape, but the chain lands back on a real
    /// file INSIDE the selected folder. Must still be allowed — otherwise the chain test above could pass
    /// vacuously by refusing every symlink.
    #[test]
    fn cpe_1642_symlink_chain_landing_back_inside_the_folder_is_allowed() {
        let dir = scratch("cpe1642-chain-inside");
        let selected = dir.path().join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let real = selected.join("real.jpg");
        std::fs::write(&real, b"data").unwrap();
        let link_b = selected.join("linkB.jpg");
        let link_a = selected.join("linkA.jpg");
        let test = "cpe_1642_symlink_chain_landing_back_inside_the_folder_is_allowed";
        if !try_symlink(std::path::Path::new("real.jpg"), &link_b, test) {
            return;
        }
        if !try_symlink(std::path::Path::new("linkB.jpg"), &link_a, test) {
            return;
        }

        let input = selected.join("photo.jpg").to_string_lossy().to_string();
        let output = link_a.to_string_lossy().to_string();
        let mut cache = ParentCache::new();
        assert_eq!(
            classify_output_containment(&input, &output, &mut cache),
            Containment::Inside,
            "a chain that resolves back inside the selected folder must NOT be refused"
        );
    }

    /// A symlink CYCLE must terminate and fail closed — and be reported as *unverifiable*, not as a proven
    /// escape (nothing left the folder; the chain simply has no real end).
    #[test]
    fn cpe_1642_symlink_cycle_is_refused_as_unverifiable_and_terminates() {
        let dir = scratch("cpe1642-cycle");
        let selected = dir.path().join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let link_a = selected.join("linkA.jpg");
        let link_b = selected.join("linkB.jpg");
        let test = "cpe_1642_symlink_cycle_is_refused_as_unverifiable_and_terminates";
        // linkA -> linkB (dangling for the moment), then linkB -> linkA closes the loop.
        if !try_symlink(std::path::Path::new("linkB.jpg"), &link_a, test) {
            return;
        }
        if !try_symlink(std::path::Path::new("linkA.jpg"), &link_b, test) {
            return;
        }

        let input = selected.join("photo.jpg").to_string_lossy().to_string();
        let output = link_a.to_string_lossy().to_string();
        let mut cache = ParentCache::new();
        match classify_output_containment(&input, &output, &mut cache) {
            Containment::Unverifiable(_) => {}
            other => panic!("a symlink cycle must be refused as unverifiable, got {other:?}"),
        }
    }

    /// **CPE-1642 finding B — the hard-link check used to FAIL OPEN under contention.** With
    /// `selected/link.jpg` hard-linked to a file outside the folder (correctly refused when uncontended),
    /// an ordinary unprivileged process holding an exclusive handle (`share_mode(0)`) made the old
    /// `GENERIC_READ` open fail, which defaulted the link count to 1 and returned "contained".
    /// **Negative control:** this exact sequence returned `false` before the fix. Windows-only because
    /// `share_mode` is the Windows mechanism for making an open fail; the read is `nlink` off one stat on
    /// Unix, which has no equivalent failure mode.
    #[cfg(windows)]
    #[test]
    fn cpe_1642_hard_link_alias_is_still_refused_while_the_file_is_held_exclusively() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = scratch("cpe1642-contended-hardlink");
        let selected = dir.path().join("selected");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&selected).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("important.jpg");
        std::fs::write(&victim, b"victim").unwrap();
        let link = selected.join("link.jpg");
        crate::links::create_hard_link(&victim.to_string_lossy(), &link.to_string_lossy())
            .expect("hard link creation needs no elevation on any platform");

        // An ordinary process (this one) holding the file with NO sharing — an AV scanner, another app,
        // or a second thread of the same batch would do exactly this.
        let hold = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&link)
            .expect("holding an exclusive handle needs no privilege");
        assert!(
            std::fs::OpenOptions::new().read(true).open(&link).is_err(),
            "sanity: with share_mode(0) held, an ordinary GENERIC_READ open of this path must fail — \
             that failure is what used to be misread as \"only one link\""
        );

        let input = selected.join("photo.jpg").to_string_lossy().to_string();
        let output = link.to_string_lossy().to_string();
        let mut cache = ParentCache::new();
        let verdict = classify_output_containment(&input, &output, &mut cache);
        // `assert_eq!(…, Escapes)`, not `assert_ne!(…, Inside)` (reviewer finding F5): the weaker form
        // would pass even if the identity probe were entirely broken and answered `Unverifiable` for
        // everything, which is exactly the mechanism this test exists to prove. Reaching `Escapes` requires
        // the `FILE_READ_ATTRIBUTES` open to have SUCCEEDED against the `share_mode(0)` holder, read a real
        // link count of 2, and censused the folder to find only one of those names inside it.
        assert_eq!(
            verdict,
            Containment::Escapes,
            "a hard link to a file outside the selected folder must be a PROVEN escape even while the file \
             is held exclusively — a contended read must never be treated as \"not linked\", and must not \
             degrade to merely \"unverifiable\" either"
        );
        drop(hold);
    }

    /// **The positive control for the test above (reviewer finding F5).** Same exclusive `share_mode(0)`
    /// holder, but both hard-linked names live INSIDE the selected folder. It must still come back
    /// `Inside` — proving the contended case above reaches `Escapes` because the probe genuinely read the
    /// file's identity through the contention, not because contention refuses everything.
    #[cfg(windows)]
    #[test]
    fn cpe_1642_contended_hard_links_wholly_inside_the_folder_are_still_allowed() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = scratch("cpe1642-contended-inside");
        let selected = dir.path().join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let first = selected.join("a.jpg");
        std::fs::write(&first, b"data").unwrap();
        let second = selected.join("b.jpg");
        crate::links::create_hard_link(&first.to_string_lossy(), &second.to_string_lossy())
            .expect("hard link creation needs no elevation on any platform");

        let hold = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&first)
            .expect("holding an exclusive handle needs no privilege");
        assert!(
            std::fs::OpenOptions::new().read(true).open(&first).is_err(),
            "sanity: the exclusive hold must actually block an ordinary GENERIC_READ open"
        );

        let input = selected.join("photo.jpg").to_string_lossy().to_string();
        let output = first.to_string_lossy().to_string();
        let mut cache = ParentCache::new();
        assert_eq!(
            classify_output_containment(&input, &output, &mut cache),
            Containment::Inside,
            "contention alone must not refuse a file whose every name is inside the selected folder — \
             otherwise the contended-escape test above passes vacuously"
        );
        drop(hold);
    }

    /// **The false positive CPE-1623 documented as a deliberate gap, now fixed.** Two hard-linked names
    /// that BOTH live inside the selected folder alias nothing outside it, so writing through one is
    /// contained. Resolving identity makes this provable with a census of the one folder the user picked
    /// (never a volume walk). Doubles as the negative control for the contended test above: a check that
    /// refused every multiply-linked file would pass that one vacuously.
    #[test]
    fn cpe_1642_hard_links_wholly_inside_the_selected_folder_are_allowed() {
        let dir = scratch("cpe1642-hardlinks-inside");
        let selected = dir.path().join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let first = selected.join("a.jpg");
        std::fs::write(&first, b"data").unwrap();
        let second = selected.join("b.jpg");
        crate::links::create_hard_link(&first.to_string_lossy(), &second.to_string_lossy())
            .expect("hard link creation needs no elevation on any platform");

        let input = selected.join("photo.jpg").to_string_lossy().to_string();
        let output = first.to_string_lossy().to_string();
        let mut cache = ParentCache::new();
        assert_eq!(
            classify_output_containment(&input, &output, &mut cache),
            Containment::Inside,
            "a multiply-linked file whose every name is inside the selected folder reaches nothing \
             outside it, so it must not be refused"
        );
    }

    /// One name inside the folder, one outside — the census finds fewer names than the file has, which
    /// PROVES a name exists elsewhere. Distinct from the "couldn't verify" verdict, and the distinction is
    /// what the refusal message depends on being true.
    #[test]
    fn cpe_1642_hard_link_with_a_name_outside_the_folder_is_a_proven_escape() {
        let dir = scratch("cpe1642-hardlink-outside");
        let selected = dir.path().join("selected");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&selected).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("important.jpg");
        std::fs::write(&victim, b"victim").unwrap();
        let link = selected.join("link.jpg");
        crate::links::create_hard_link(&victim.to_string_lossy(), &link.to_string_lossy()).unwrap();

        let input = selected.join("photo.jpg").to_string_lossy().to_string();
        let output = link.to_string_lossy().to_string();
        let mut cache = ParentCache::new();
        assert_eq!(
            classify_output_containment(&input, &output, &mut cache),
            Containment::Escapes,
            "a hard link with a name outside the selected folder is a PROVEN escape, not merely unverified"
        );
    }

    // ---- CPE-1642 round 2 (reviewer findings REV-G / F1, F2): the probe must address the SAME set of -----
    // ---- files the writer does, and an identity that identifies nothing is not an identity -------------

    /// **REV-G — over-`MAX_PATH` output, at the containment-check level.** The end-to-end, byte-proven
    /// version of this lives in `batch_execute` (`cpe_1642_over_max_path_symlink_alias_…`); this is the
    /// fast unit-level guard on the same mechanism. Before the fix `CreateFileW` on the raw path failed
    /// with `ERROR_PATH_NOT_FOUND`, which became `Probe::Absent` and therefore `Containment::Inside` — a
    /// fail-open, and strictly worse than the code CPE-1642 replaced. Windows-only: `MAX_PATH` is a Windows
    /// limit and `symlink_metadata` on Unix has no equivalent truncation.
    #[cfg(windows)]
    #[test]
    fn cpe_1642_symlink_alias_past_max_path_is_a_proven_escape() {
        let dir = scratch("cpe1642-longpath-unit");
        let mut deep = dir.path().to_path_buf();
        while deep.to_string_lossy().chars().count() < 300 {
            deep = deep.join("padpadpadpadpadpadpadpadpadpadpadpadpadpad");
        }
        let selected = deep.join("selected");
        let outside = deep.join("outside");
        std::fs::create_dir_all(&selected).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        assert!(
            selected.to_string_lossy().chars().count() > 260,
            "sanity: the selected folder must sit past MAX_PATH or this test proves nothing"
        );

        let victim = outside.join("important.jpg");
        std::fs::write(&victim, b"victim").unwrap();
        // A RELATIVE target, resolved against the link's own parent exactly as the OS does — which also
        // means the probe has to cope with a `..` segment inside an over-MAX_PATH path.
        let link = selected.join("linkA.jpg");
        let target = std::path::Path::new("..").join("outside").join("important.jpg");
        if !try_symlink(&target, &link, "cpe_1642_symlink_alias_past_max_path_is_a_proven_escape") {
            return;
        }

        let input = selected.join("photo.jpg").to_string_lossy().to_string();
        let output = link.to_string_lossy().to_string();
        let mut cache = ParentCache::new();
        assert_eq!(
            classify_output_containment(&input, &output, &mut cache),
            Containment::Escapes,
            "a symlink out of an over-MAX_PATH folder must be a PROVEN escape — the identity probe has to \
             reach every path `std::fs` (and therefore the writer) reaches"
        );
        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// **An over-`MAX_PATH` output that is an ordinary, absent file must still be ALLOWED.** Without this,
    /// the test above could pass by refusing every long path, and every legitimate deep-folder batch would
    /// break. Also pins the belt-and-braces length guard in `classify_open_failure` to the codes that
    /// actually mean truncation: a plain `ERROR_FILE_NOT_FOUND` at any length is still `Absent`.
    #[cfg(windows)]
    #[test]
    fn cpe_1642_ordinary_absent_output_past_max_path_is_still_allowed() {
        let dir = scratch("cpe1642-longpath-ok");
        let mut deep = dir.path().to_path_buf();
        while deep.to_string_lossy().chars().count() < 300 {
            deep = deep.join("padpadpadpadpadpadpadpadpadpadpadpadpadpad");
        }
        std::fs::create_dir_all(&deep).unwrap();
        let input = deep.join("photo.jpg");
        std::fs::write(&input, b"x").unwrap();
        let output = deep.join("photo-800.jpg"); // does not exist yet — the overwhelmingly common case

        let mut cache = ParentCache::new();
        assert_eq!(
            classify_output_containment(
                &input.to_string_lossy(),
                &output.to_string_lossy(),
                &mut cache
            ),
            Containment::Inside,
            "an ordinary not-yet-existing output in a deep folder must not be refused"
        );
        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// **A path handed in already in `\\?\` verbatim form.** `std::fs` returns such paths untouched, so the
    /// probe must too — re-prefixing would produce `\\?\\\?\C:\…`, which names nothing, and (with the
    /// length guard) would refuse every deep batch. `canonicalize` is the ordinary way one of these reaches
    /// the engine. The symlink still has to be caught: the verbatim spelling must change nothing about the
    /// verdict. There was no `\\?\` coverage in this module at all before.
    #[cfg(windows)]
    #[test]
    fn cpe_1642_verbatim_prefixed_paths_are_probed_not_re_prefixed() {
        let dir = scratch("cpe1642-verbatim");
        let selected = dir.path().join("selected");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&selected).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("important.jpg");
        std::fs::write(&victim, b"victim").unwrap();
        let link = selected.join("linkA.jpg");
        if !try_symlink(&victim, &link, "cpe_1642_verbatim_prefixed_paths_are_probed_not_re_prefixed") {
            return;
        }

        // `canonicalize` hands back the `\\?\`-prefixed spelling on Windows.
        let canon_selected = std::fs::canonicalize(&selected).unwrap();
        assert!(
            canon_selected.to_string_lossy().starts_with("\\\\?\\"),
            "sanity: this test needs a verbatim path, got {}",
            canon_selected.display()
        );
        let input = canon_selected.join("photo.jpg").to_string_lossy().to_string();
        let output = canon_selected.join("linkA.jpg").to_string_lossy().to_string();
        let mut cache = ParentCache::new();
        assert_eq!(
            classify_output_containment(&input, &output, &mut cache),
            Containment::Escapes,
            "a symlink out of the folder must be caught just the same when the paths arrive in \\\\?\\ form"
        );

        // Positive control: an ordinary absent output in verbatim form must still be allowed, so the
        // assertion above can't be passing because verbatim paths are simply never openable.
        let plain = canon_selected.join("photo-800.jpg").to_string_lossy().to_string();
        assert_eq!(
            classify_output_containment(&input, &plain, &mut cache),
            Containment::Inside,
            "an ordinary not-yet-existing output must not be refused just for arriving in \\\\?\\ form"
        );
    }

    /// **F2 — a degenerate identity is not an identity.** `GetFileInformationByHandle` *succeeds* on
    /// several network redirectors while supplying no usable file index: it returns zero. Every object on
    /// such a volume would then compare EQUAL, so the landing-directory-versus-selected-directory test
    /// would pass for any directory anywhere and a symlink out of the folder would be judged contained —
    /// a fail-open invisible from the call site, because the API said "OK".
    ///
    /// Injected rather than reproduced: no such volume is available to CI (or to the reviewer), and the
    /// guard is a property of the values, not of the syscall.
    #[test]
    fn cpe_1642_degenerate_identity_is_never_accepted_as_a_real_one() {
        let real = FileIdentity { volume: 0x9ABC_DEF0, index: 42 };
        let no_index = FileIdentity { volume: 0x9ABC_DEF0, index: 0 };
        let other_no_index = FileIdentity { volume: 0x9ABC_DEF0, index: 7_000 - 7_000 };
        let no_volume = FileIdentity { volume: 0, index: 42 };

        // The trap itself: without the guard, two *unrelated* objects on an index-less volume are equal.
        assert_eq!(
            no_index, other_no_index,
            "sanity: on a volume with no file index every object carries the same identity — that is \
             exactly why a zero index must never be compared"
        );

        assert!(!real.is_degenerate(), "an ordinary identity must keep working");
        assert!(no_index.is_degenerate(), "a zero file index identifies nothing");
        assert!(no_volume.is_degenerate(), "a zero volume serial identifies nothing");

        let facts = |id| FileFacts { id, links: 1, is_dir: true };
        assert!(
            matches!(facts_or_unreadable(facts(real)), Probe::Real(_)),
            "a real identity must probe as Real"
        );
        for bad in [no_index, no_volume] {
            assert!(
                matches!(facts_or_unreadable(facts(bad)), Probe::Unreadable),
                "a degenerate identity must probe as Unreadable (which every caller refuses on), not Real"
            );
            assert_eq!(
                identity_or_none(bad),
                None,
                "a degenerate directory identity must be None — `dir_identity`'s callers treat None as a \
                 containment failure, so nothing can compare equal to it"
            );
        }
        assert_eq!(identity_or_none(real), Some(real));
    }

    /// The degeneracy guard has to leave the ordinary case alone: a real directory on a real volume must
    /// still yield an identity, or every batch on this machine would be refused.
    #[test]
    fn cpe_1642_a_real_directory_still_has_a_readable_identity() {
        let dir = scratch("cpe1642-real-identity");
        let id = identity_following_links(dir.path())
            .expect("an ordinary local directory must still resolve to a usable identity");
        assert!(!id.is_degenerate(), "a real local directory must not look degenerate: {id:?}");
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
