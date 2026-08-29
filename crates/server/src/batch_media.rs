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
/// anywhere **on Windows only** (CPE-1640 — see [`colon_is_a_path_character`]), or a **whole-segment**
/// `..` traversal (CPE-1623; narrowed by a UAT follow-up — see below).
/// The template only ever substitutes into the file's STEM (or, for Convert, the extension) — see
/// `plan()`'s `Rename`/`Convert` arms, which run the result straight through [`join`] alongside the
/// input's own unchanged directory — so it has no legitimate reason to name a directory, let alone walk
/// out of one, at all.
///
/// A literal separator (or `:`) is checked as a plain substring deliberately (not "is this a whole path
/// segment"): `template.replace("{stem}", ...)` means the attacker doesn't need it to occupy a whole
/// segment on its own (see the ticket's worked example, `"..\\..\\cpe1613_traversal_victim\\important"`,
/// which has no `{stem}`/`{n}`/`{ext}` token at all and is used completely literally) — and a filename
/// can never legitimately contain a raw `/` or `\` at all on any mainstream filesystem, so there is no
/// false-positive risk in flagging either of them anywhere they appear. **`:` is the exception and is now
/// gated to Windows (CPE-1640)**: it is reserved on NTFS (drive separator *and* alternate-data-stream
/// separator) but is an ordinary, legal filename character on Linux and macOS, where rejecting it was a
/// pure false positive — see [`colon_is_a_path_character`].
///
/// **`:` (reviewer finding, PR #828 attempt 2).** A template like `"C:foo"` contains none of the three
/// original characters this fn checked — only `/`, `\`, `..` were rejected — so it passed straight
/// through both here and [`classify_output_containment`]'s directory-identity comparison for a
/// **bare-filename input** (no directory component at all, so both `dir` and the computed `out_dir` are
/// textually empty and compare equal): a classic Windows drive-relative reference, which resolves against
/// drive `C:`'s *current directory* at write time — not the folder the user picked. Rejecting `:` outright
/// closes it at this field-level layer **on Windows**; [`classify_output_containment`] carries a second,
/// independent guard for the same case (see its doc) so a caller that skips this check entirely (bypassing
/// `validate()`) is still caught. Both are Windows-gated (CPE-1640): off Windows there is no drive-relative
/// syntax for `C:foo` to mean, so the string is simply a filename containing a colon.
///
/// **`..` is different (UAT follow-up to CPE-1623).** The very first cut of this check flagged `..`
/// as a **substring** — `template.contains("..")` — which rejected perfectly ordinary filenames like
/// `"shot..final"` or a version stamp `"v1..2"` that contain the two characters but can never walk
/// anywhere: with no separator present at all, the whole template is exactly ONE path segment, so `..`
/// is only a traversal risk when it occupies that entire segment. Once any separator (or, on Windows, `:`)
/// has already failed the check above and returned `true`, there's nothing further to decide here — so by
/// the time this line runs, `template` contains no `/` or `\` (nor `:` on Windows), and "is `..` a whole segment"
/// reduces to "is the (trimmed) template exactly `..`". This stays exactly as strict for every case the
/// module's own tests already pinned (`".."`, `"../evil"`, `"..\\evil"`, `"a/../b"` all still contain a
/// separator and are still rejected above) while accepting the two the auditor's own worked examples name.
fn template_escapes_directory(template: &str) -> bool {
    if template.contains('/') || template.contains('\\') {
        return true;
    }
    if colon_is_a_path_character() && template.contains(':') {
        return true;
    }
    template.trim() == ".."
}

/// **`:` is a Windows rule, not a universal one (CPE-1640).** The colon rejection above was added for two
/// Windows-only reasons — `C:foo` is a *drive-relative* reference that resolves against drive `C:`'s own
/// current directory, and a colon anywhere else in a Windows path is the NTFS **alternate-data-stream**
/// separator (CPE-1624) — but it shipped with no platform gate at all, so it fired identically on Linux and
/// macOS, where `:` is an ordinary, legal filename character. A Linux user typing a perfectly reasonable
/// template (a timestamp like `10:30am-photo`, or `session:final`) was refused for a reason that does not
/// exist on their machine. CI could not catch it: the rule was *consistently* wrong, and a 3-OS matrix only
/// detects *inconsistency*.
///
/// Relaxing it off Windows cannot reopen a containment escape. This check is a friendly, field-level early
/// warning; the actual guarantee is [`classify_output_containment`], which runs on the fully-substituted
/// output path, is unconditional on every platform, and is re-derived independently by
/// [`crate::batch_execute::execute_plan_walk`] for outputs that never went through `plan()` at all. Nor does
/// it reopen CPE-1624's alternate-data-stream hole: that is closed on Windows inside
/// [`classify_output_containment`] itself (see [`final_component_names_alternate_stream`]), which a
/// template-level check never covered anyway — it cannot see a hand-built `PlannedItem`.
///
/// A `const fn`-shaped `cfg!` (not `#[cfg]` blocks) deliberately: the rule compiles on **all three** CI legs,
/// so the surrounding code is type-checked and clippy-linted everywhere rather than only on Windows.
fn colon_is_a_path_character() -> bool {
    cfg!(windows)
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
pub(crate) enum PathKey {
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
/// **This cache is only ever safe for a check that does not authorise a write, and the security audit of
/// PR #848 proved why (findings 2 and 3).** An earlier version of this doc claimed the memos were sound
/// because they hold "facts about *directories*, which the batch neither creates nor moves". That is the
/// wrong invariant. The real requirement is that **no other principal can re-point them**, and that is
/// simply false for anything reached through a symlink or a junction — which any directory path may be.
///
/// Two concrete refutations, both against that earlier reasoning:
///
/// - **`dir_scans` (executed).** The old note argued a stale census could only err fail-closed, reasoning
///   about a name being *added* or *removed* in isolation. It missed the **swap**: delete an inside hard
///   link and create an outside one in the same window, and `links` is unchanged at 2 while the memo
///   still reports 2 names inside, so `inside >= links` passes. Measured end to end: `written = 2`,
///   `skipped = []`, and a file outside the selected folder went 34 → 168 bytes holding the batch's
///   output. The identical sequence against a **fresh** cache correctly returns [`Containment::Escapes`]
///   — the engine already knew; the memo blinded it.
/// - **`dir_ids`.** Memoized by path *string*, and [`identity_following_links`] resolves *through* links,
///   so a directory link re-pointed mid-batch is invisible. Unlike a census this staleness is not even
///   monotone: it feeds an equality comparison with nothing re-probed alongside it to contradict it.
///
/// So the rule is now structural rather than argued: **the write-time authority
/// ([`open_output_verified`]) builds its own cache, per item, and never receives one.** Whatever this
/// cache is reused for can therefore only ever *refuse* a batch early — it can no longer permit a write.
/// [`crate::batch_execute::execute_plan_walk`] still threads one through its up-front scan, where a stale
/// answer costs at worst a batch refused for a condition that has since cleared.
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

    /// Drop every memo of a **live filesystem fact** — a directory's identity and a directory's link
    /// census — keeping only `parents`, which is a pure path-canonicalization memo. Called at the top of
    /// every [`classify_output_containment`], so those two can only ever be reused *within* a single
    /// decision, never across two. See that function for the two demonstrated exploits this closes
    /// (security audit findings 2 and 3, PR #848).
    fn forget_live_facts(&mut self) {
        self.dir_ids.clear();
        self.dir_scans.clear();
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
///
/// `pub(crate)` (not just this module's own tests, as of CPE-1667): [`crate::batch_execute`] builds a
/// `HashSet<PathKey>` of a batch's own inputs once per batch and probes it with this fn directly — an
/// O(1) membership test instead of the O(n) pairwise [`same_file`] scan `is_foreign_overwrite` used to
/// run, which sat *inside* [`crate::batch_execute::execute_one`]'s verify-to-write window.
pub(crate) fn path_key(path: &str, parent_cache: &mut ParentCache) -> PathKey {
    // CPE-1624 finding B: `X.JPG:hidden` and `X.JPG` are one file on disk (the same MFT record), so they
    // must key identically. Stripped once, up front, so every tier below — canonicalize, parent+name, and
    // the purely lexical fallback — sees the underlying file. No-op off Windows.
    let path = strip_stream_suffix(path);

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
    // **Security audit findings 2 and 3 (PR #848): the live-filesystem memos are per-CALL scratch, never
    // cross-call state.** Both were demonstrated to make this function replay an answer about a
    // filesystem that had since changed — the census by a hard-link swap that keeps the link count at 2
    // (executed: a file outside the folder went 34 → 168 bytes), the directory-identity memo by
    // re-pointing a directory link, which nothing else re-probes to contradict. Rather than asking every
    // caller to remember to build a fresh cache, the footgun is removed here: whatever cache is threaded
    // in, this call always resolves live filesystem facts itself. Only `parents` — a pure
    // path-canonicalization memo, and CPE-1613's O(n) fix for `plan()`'s collision set — survives across
    // calls, and it can no longer authorise anything on its own: the write-time authority
    // ([`open_output_verified`]) builds its own cache per item regardless.
    parent_cache.forget_live_facts();

    let out_final = output.rsplit(['/', '\\']).next().unwrap_or(output);
    if out_final == "." || out_final == ".." {
        return Containment::Escapes;
    }

    let (dir, _, _) = split(input);
    let (out_dir, out_stem, _) = split(output);
    if colon_is_a_path_character() && dir.is_empty() && out_stem.contains(':') {
        return Containment::Escapes;
    }
    // CPE-1624 finding B: an NTFS alternate data stream. Checked BEFORE the directory comparison, because
    // an ADS output is not a directory question at all — `selected\C:foo.png` names a hidden stream of the
    // file `selected\C`, which sits perfectly *inside* the selected folder.
    if final_component_names_alternate_stream(out_final) {
        return Containment::Refused(WHY_ALTERNATE_STREAM);
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

/// The verdict on one planned output (CPE-1642, extended by CPE-1624). Multi-valued on purpose: "I could
/// not establish this output's identity" is a distinct fact from "this output provably leaves the folder",
/// and conflating them produced a refusal message that told the user something untrue. `Refused` is a
/// third distinct fact for the same reason — an alternate-data-stream output does *not* leave the folder
/// and is *not* unverifiable; it is a place a batch may not write at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Containment {
    /// Proven to land inside the input's own folder.
    Inside,
    /// Proven to land somewhere else.
    Escapes,
    /// Provably inside the folder by path, but not a place a batch may write — the payload is the true
    /// reason. Currently only [`WHY_ALTERNATE_STREAM`] (CPE-1624 finding B).
    Refused(&'static str),
    /// Could not be established — **treated exactly like `Escapes` by every caller** (fail closed); the
    /// payload is the true reason, for an accurate refusal message.
    Unverifiable(&'static str),
}

/// **Windows NTFS alternate data streams (CPE-1624 finding B).** A colon inside a *final path component*
/// is NTFS' alternate-data-stream separator: writing to `C:foo.png` where `C` is an ordinary file in the
/// same folder does **not** create a file called `C:foo.png` — it writes hidden bytes into the `foo.png`
/// stream of the existing, unrelated file `C`. That file's visible size and content never change, Explorer
/// shows nothing, and `Path::is_file()` on a never-before-existing stream path returns `false`, so both the
/// planner's collision check and [`crate::batch_execute::is_foreign_overwrite`] see "nothing is there".
///
/// The security audit reproduced it end-to-end from the ordinary rename box (template `"C:foo"` against a
/// folder containing a plain file named `C`): `plan()` computed a *contained* output in the *same
/// directory*, the CPE-1623 containment check correctly saw no escape, and `execute_plan` returned
/// `Ok(written: 1)` with 120 bytes of transformed PNG readable at the stream path. Any colon does it
/// (`secrets:hidden` as well as a drive-letter shape), so this is scoped to colons generally.
///
/// **Rejected outright at the engine boundary** rather than "resolved to the underlying file": no
/// legitimate Batch Media flow produces one — `plan()`'s own path construction never emits a colon — so
/// there is nothing to preserve, and refusing is the one answer that cannot be wrong. This lives in
/// [`classify_output_containment`], which BOTH `plan()` and [`crate::batch_execute::execute_plan_walk`]
/// call, so it also covers a hand-built `PlannedItem` that never went through `plan()` — the surface a
/// template-level check can never reach. It is deliberately independent of
/// [`template_escapes_directory`]'s colon rule, which CPE-1640 has now gated to Windows: that one is a
/// friendly field-level echo, this one is the enforcement point.
///
/// Windows-only via [`colon_is_a_path_character`]: on Linux/macOS a colon in a filename is an ordinary
/// character naming an ordinary file, and refusing it would be exactly the CPE-1640 false positive.
/// A *whole path's* drive prefix (`C:\dir\x.jpg`) is never seen here — the caller passes the FINAL
/// component, which for any rooted path has already had the drive prefix split off.
fn final_component_names_alternate_stream(final_component: &str) -> bool {
    colon_is_a_path_character() && final_component.contains(':')
}

/// [`final_component_names_alternate_stream`] for a **whole path** — splits the final component off
/// first. `pub(crate)` so [`crate::batch_execute::is_foreign_overwrite`] can enforce the rule itself
/// rather than relying on being called after [`classify_output_containment`] (security audit finding 4,
/// PR #848: an unenforced call-ordering convention is not a guarantee).
pub(crate) fn names_alternate_stream(path: &str) -> bool {
    final_component_names_alternate_stream(path.rsplit(['/', '\\']).next().unwrap_or(path))
}

/// Drop a Windows alternate-data-stream suffix from a path's final component, so [`path_key`] (and hence
/// [`same_file`]) resolves `X.JPG:hidden` to the SAME identity as `X.JPG` — the second half of CPE-1624
/// finding B, measured on a real file: `same_file("…\IMG_1.JPG", "…\IMG_1.JPG:hidden")` returned `false`,
/// because [`parent_and_name`] splits on `/` and `\` only, leaving the colon inside the name component
/// where it never matched lexically or under case-folding. The two paths are the same MFT record, so
/// every "would this write touch that file?" question must answer yes.
///
/// **Not applied to a drive-relative reference.** With no directory separator anywhere and a colon at
/// index 1 after an ASCII letter, `C:foo` is drive `C:`'s current directory + `foo` — an entirely
/// different file from `C`, so stripping there would fuse two unrelated paths into one identity (the
/// fail-OPEN direction). Left intact; [`classify_output_containment`] refuses that shape separately.
///
/// No-op off Windows ([`colon_is_a_path_character`]), where a colon is an ordinary filename character and
/// `photo:final.jpg` is a real, distinct file that must never collapse onto `photo`.
///
/// **Reach audit — the one direction where "same file" *relaxes* a rule.** Making two paths compare equal
/// is the conservative answer at three of [`same_file`]'s four uses (`plan()`'s collision set renames
/// past it; [`crate::batch_execute::is_foreign_overwrite`]'s first arm demands confirmation for it). The
/// fourth is not: `is_foreign_overwrite`'s `!items.iter().any(|other| same_file(&other.input, …))`
/// treats "this output IS one of the batch's own inputs" as *permitted*, so fusing `X.JPG:evil` onto the
/// selected input `X.JPG` would license writing the stream without confirmation. That is unreachable by
/// construction: this stripping and [`final_component_names_alternate_stream`]'s outright refusal are
/// gated on the *same* [`colon_is_a_path_character`], and the refusal runs first — in both `plan()` and
/// `execute_plan_walk`, ahead of any overwrite question — so on every platform where a path could be
/// stripped, that path has already been refused. The two must stay co-gated.
fn strip_stream_suffix(path: &str) -> &str {
    if !colon_is_a_path_character() {
        return path;
    }
    let name_start = path.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    let name = &path[name_start..];
    let Some(colon) = name.find(':') else { return path };
    if colon == 0 {
        return path; // ":stream" — no base name to attribute it to; leave it alone.
    }
    if name_start == 0 && colon == 1 && name.as_bytes()[0].is_ascii_alphabetic() {
        return path; // drive-relative `C:foo`, not a stream of a file called `C`
    }
    // `:` is ASCII, so `name_start + colon` is always a char boundary.
    &path[..name_start + colon]
}

/// A file's **true filesystem identity** (CPE-1642): the pair every OS uses to answer "are these two
/// names the same object?" — `(volume serial number, 64-bit file index)` on Windows via
/// `GetFileInformationByHandle`, `(dev, ino)` on Unix. Comparing identities is what makes symlink chains,
/// junctions, hard links and any future link shape collapse into ONE question, instead of a growing
/// catalogue of path-string patterns to pattern-match (the approach CPE-1623 exhausted).
///
/// `index` is `u128` so both platforms' widest form fits without truncation.
///
/// `pub(crate)` since CPE-1672: [`crate::vault_manager`]'s session shredder pins each object it is about
/// to overwrite by identity for the same reason this module does — a *name* can be re-pointed between any
/// check and the write that follows it, and only the object can be pinned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FileIdentity {
    pub(crate) volume: u64,
    pub(crate) index: u128,
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
    pub(crate) fn is_degenerate(self) -> bool {
        self.volume == 0 || self.index == 0
    }
}

/// Gate a freshly-probed [`FileFacts`] on [`FileIdentity::is_degenerate`]: an identity that identifies
/// nothing is *unreadable*, never a real one. Pure, so the volume this defends against (which CI has no
/// access to) can be reproduced in a unit test by injecting the degenerate value directly.
fn facts_or_unreadable(facts: FileFacts) -> Probe {
    if facts.id.is_degenerate() {
        Probe::Unreadable(WHY_PROBE_FAILED)
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
    /// is exactly CPE-1642 finding B (a contended open used to fall back to "assume unlinked"). Carries
    /// the true reason (CPE-1652 finding A added a second one), so the refusal message stays accurate
    /// rather than blaming a lock for an unrecognised reparse tag.
    Unreadable(&'static str),
}

/// An identity→name-count census of ONE directory (CPE-1642), used to decide whether a multiply-linked
/// output's other names are all accounted for inside the folder the user selected.
#[derive(Debug, Default)]
struct DirLinkScan {
    counts: std::collections::HashMap<FileIdentity, u64>,
    /// At least one entry's identity could not be read, so `counts` may undercount — a shortfall must
    /// then be reported as *unverifiable*, not as a proven escape.
    incomplete: bool,
    /// The folder held more than [`census_cap`] entries and the scan stopped early (CPE-1652 finding B).
    /// Like `incomplete` this means `counts` may undercount, but it gets its own flag so the refusal can
    /// say *why* — "this folder is too big to check cheaply" is a different fact from "something in it
    /// was unreadable", and the user's remedy differs.
    capped: bool,
}

const WHY_PROBE_FAILED: &str = "the planned output exists but its filesystem identity could not be read \
                               (it may be locked by another process)";
const WHY_CHAIN_FAILED: &str = "the planned output is a link whose chain could not be followed to a real \
                               location (unreadable, cyclic, or too many hops)";
const WHY_DIR_IDENTITY_FAILED: &str = "the folder a linked output resolves into could not be identified";
const WHY_CENSUS_FAILED: &str = "the selected folder could not be enumerated to account for the output's \
                                 other hard links";
/// CPE-1652 finding B: the census is bounded, and a folder past the bound degrades to a refusal rather
/// than to a slow scan — fail closed, never fail slow.
const WHY_CENSUS_TOO_BIG: &str = "the selected folder holds too many entries to account for the output's \
                                  other hard links within a bounded scan";
/// CPE-1652 finding A: a reparse tag with the name-surrogate bit set that this code does not understand.
/// Reparse tags are a Windows concept, so this reason is only ever produced by the Windows probe — the
/// same `allow(dead_code)` shape [`WHY_MAX_PATH_AMBIGUOUS`] needs, and the reason CI's Linux/macOS legs
/// exist: a Windows-only reference is invisible from a Windows dev box.
#[cfg_attr(not(windows), allow(dead_code))]
const WHY_SURROGATE_TAG: &str = "the planned output is a reparse point whose type is not understood, and \
                                 whose tag says it stands in for another name — a write would follow it \
                                 somewhere this check cannot predict";
/// CPE-1642 REV-G's second line of defence: at or past `MAX_PATH`, a "path not found"/"invalid name" is
/// indistinguishable from a truncation, and must never be read as "nothing is there".
#[cfg_attr(not(windows), allow(dead_code))]
const WHY_MAX_PATH_AMBIGUOUS: &str = "the planned output's path is at or past Windows' legacy MAX_PATH \
                                      limit and could not be opened, which is indistinguishable from the \
                                      path having been truncated";
/// CPE-1624 finding B — see [`final_component_names_alternate_stream`].
const WHY_ALTERNATE_STREAM: &str = "names an NTFS alternate data stream (a \":\" in its final path \
                                    component), which would write hidden bytes into a different, \
                                    unrelated file instead of creating a file of its own";

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
        Probe::Unreadable(why) => Containment::Unverifiable(why),
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
                Probe::Unreadable(why) => Containment::Unverifiable(why),
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
        // way to prove it). Sound even for a partial census: `counts` can only ever UNDERcount (a capped
        // or incomplete scan misses entries, never invents them), so having already found every one of
        // this file's names inside the folder is a proof that does not depend on the scan being complete.
        Containment::Inside
    } else if scan.capped {
        // CPE-1652 finding B: the folder is past the bounded-scan cap, so the shortfall may be an
        // artefact of stopping early rather than a real outside name. Refuse (fail closed) instead of
        // either claiming a proven escape or paying an unbounded scan.
        Containment::Unverifiable(WHY_CENSUS_TOO_BIG)
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
            Probe::Unreadable(why) => return Err(why),
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
///
/// **Bounded (CPE-1652 finding B).** Cheap as this is *relative to the work it replaces*, it is still one
/// `CreateFileW`/`symlink_metadata` per entry of whatever folder the user selected, on a user-facing path
/// — on a 100k-entry folder that is 100k handle opens, against PURPOSE.md's fast/small/predictable
/// tiebreaker. The scan therefore stops at [`census_cap`] entries and marks itself `capped`, which
/// [`real_target_containment`] turns into [`Containment::Unverifiable`] unless containment was already
/// *proven* by the names found so far. Fail closed, never fail slow: the cap can only ever cause a
/// refusal, never an acceptance (see the `inside >= links` arm's soundness note).
fn scan_dir_link_census(dir: &std::path::Path) -> Option<DirLinkScan> {
    let entries = std::fs::read_dir(dir).ok()?;
    let cap = census_cap();
    let mut scan = DirLinkScan::default();
    let mut seen = 0usize;
    for entry in entries {
        seen += 1;
        if seen > cap {
            scan.capped = true;
            break;
        }
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
            Probe::Link | Probe::Unreadable(_) => scan.incomplete = true,
        }
    }
    Some(scan)
}

/// How many entries [`scan_dir_link_census`] will identify before giving up and refusing (CPE-1652
/// finding B). 20,000 is deliberately far above any folder a person browses by hand (and above every
/// folder this app's own perf tests build) while keeping the absolute worst case bounded at ~20k handle
/// opens — tens of milliseconds — instead of growing with the folder. The census is only reached at all
/// when a planned output turns out to be **multiply linked**, which no ordinary batch ever is, so in
/// practice this ceiling is never approached.
fn census_cap() -> usize {
    const MAX_CENSUS_ENTRIES: usize = 20_000;
    #[cfg(test)]
    if let Some(n) = CENSUS_CAP_OVERRIDE.with(|c| c.get()) {
        return n;
    }
    MAX_CENSUS_ENTRIES
}

#[cfg(test)]
thread_local! {
    /// Test-only override for [`census_cap`], so the cap's fail-closed behaviour can be proven against a
    /// three-entry folder instead of building a 20,001-entry one. Thread-local for the same reason
    /// `CANONICALIZE_CALLS` is: the default test harness runs each `#[test]` on its own thread.
    static CENSUS_CAP_OVERRIDE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// `pub(crate)` so [`crate::batch_execute`]'s tests can prove the cap fails closed through the real
/// `execute_plan` entry point, not just through this module's internals.
#[cfg(test)]
pub(crate) fn set_census_cap_for_test(cap: Option<usize>) {
    CENSUS_CAP_OVERRIDE.with(|c| c.set(cap));
}

/// What a Windows reparse point actually is, as far as this module's "probe and writer must agree" rule is
/// concerned (CPE-1652 finding A). Pure `u32` logic, compiled on **every** platform (only *called* on
/// Windows) so all three CI legs type-check and unit-test the classification table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
enum ReparseKind {
    /// Not a reparse point at all — an ordinary file or directory.
    NotReparse,
    /// A link this module knows how to follow: `IO_REPARSE_TAG_SYMLINK` or `IO_REPARSE_TAG_MOUNT_POINT`
    /// (a junction). [`follow_link_chain`] walks these.
    Link,
    /// A reparse point with **no** name-surrogate bit: a cloud placeholder (OneDrive), a dedup stub, a
    /// compression/HSM filter. These are ordinary files to every reader — a write lands on this very file
    /// — so the probe describes them as the real file they are. Calling them links would strand ordinary
    /// batches in OneDrive-backed folders.
    OpaqueData,
    /// A reparse point whose tag carries the **name-surrogate** bit (`0x2000_0000`) but which this module
    /// does not recognise. The bit's whole meaning is "this object stands in for another named object", so
    /// a write *will* be redirected — somewhere this code cannot predict. Fail closed.
    UnknownSurrogate,
}

/// The classification table for [`ReparseKind`] (CPE-1652 finding A).
///
/// **Why this replaced `std`'s `is_symlink()`.** The previous probe asked
/// `std::fs::symlink_metadata(path).file_type().is_symlink()`, and `std` answers `true` for exactly two
/// tags — `IO_REPARSE_TAG_SYMLINK` and `IO_REPARSE_TAG_MOUNT_POINT`. Every *other* reparse point was
/// therefore reported as the real file it appeared to be. That is right for a cloud placeholder or a dedup
/// stub (which the old doc comment cited as the justification) but those are **non-surrogate** tags — a
/// different set from the one the rule actually covered. For a *name-surrogate* tag `std` doesn't know,
/// the probe opened the stub with `FILE_FLAG_OPEN_REPARSE_POINT` and described the stub, while the
/// subsequent write — plain `std::fs::write`, no such flag — followed the reparse point wherever it leads.
/// Probe and writer disagreed: the same shape of bug as the `MAX_PATH` fail-open (REV-G) that blocked
/// PR #840, and the one thing this module's rules forbid.
///
/// **Reach diff versus what it replaces.** Every case `std` called a symlink is still `Link` (identical
/// tags). Every case `std` called an ordinary file is still `OpaqueData` **unless** its tag sets the
/// name-surrogate bit, which is the strictly-narrower set that now fails closed. Nothing that used to be
/// refused is now allowed; the only movement is in the safe direction. The tag is read from the **handle
/// already open** (`GetFileInformationByHandleEx`), not from a second path-based call, so it also inherits
/// [`verbatim_wide`]'s `MAX_PATH` reach rather than re-deriving it — and cannot disagree with the
/// attributes it is classified alongside.
#[cfg_attr(not(windows), allow(dead_code))]
fn classify_reparse_tag(file_attributes: u32, reparse_tag: u32) -> ReparseKind {
    /// `FILE_ATTRIBUTE_REPARSE_POINT`.
    const ATTR_REPARSE_POINT: u32 = 0x0000_0400;
    /// A junction/mount point.
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
    /// An NTFS symbolic link.
    const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;
    /// `IsReparseTagNameSurrogate` — "this tag stands in for another named entity".
    const NAME_SURROGATE: u32 = 0x2000_0000;

    if file_attributes & ATTR_REPARSE_POINT == 0 {
        return ReparseKind::NotReparse;
    }
    match reparse_tag {
        IO_REPARSE_TAG_SYMLINK | IO_REPARSE_TAG_MOUNT_POINT => ReparseKind::Link,
        t if t & NAME_SURROGATE != 0 => ReparseKind::UnknownSurrogate,
        _ => ReparseKind::OpaqueData,
    }
}

/// Does this **name** belong to an object that has more than one name? (CPE-1857)
///
/// `pub(crate)` so [`crate::revert_engine`] can *classify* a refusal
/// [`crate::fsutil::copy_file_onto_no_follow`] has already issued — the engine splits refusals into
/// transient ("fix the cause and re-running works") and permanent ("nothing about re-running changes
/// this"), and a hard link is emphatically the second kind: a file does not stop having a second name
/// because the user runs the revert again. Telling them to retry is precisely the loop CPE-1845 exists
/// to stop sending people round.
///
/// **This is a PATH question and it is only ever safe to ask AFTER the write has been decided.** By the
/// time the engine calls this the bytes have already been written or already been refused, so this
/// chooses WORDING and can no longer choose where a byte goes. That is the one property that makes the
/// same call unsafe before a write — see [`crate::fsutil::copy_file_onto_no_follow`], which reads the
/// count off its open handle for exactly that reason and never from here.
///
/// Answers `false` for everything it cannot positively call multiply linked — an absent name, a link, an
/// unreadable probe, a platform with no identity model. Failing "open" is right *here* and only here:
/// the fallback is `transient`, which is the classification this path had for every refusal before
/// CPE-1857, so an unknown answer degrades to the previous behaviour rather than to a new wrong one.
/// # The crate-wide sweep, and the decision taken for every row (CPE-1857 AC2)
///
/// CPE-1857's acceptance criterion is that whatever is chosen applies to **every** writer in the crate,
/// not just the revert path. Every production write site under `crates/server/src` that can land on a
/// **pre-existing** name was enumerated (54 production sites from 68 raw `fs::copy` / `fs::write` /
/// `File::create` / `OpenOptions` hits, with `#[cfg(test)]` and doc lines stripped) and given a verdict.
/// The table is the record; **a partial sweep presented as complete is this repo's most-repeated
/// defect**, so the rows that were deliberately left alone are listed as loudly as the rows that changed.
///
/// **The scoping rule, stated once.** A write can only be redirected through a hard link if it lands on
/// a name that **already exists** — a name claimed with `create_new`/`O_EXCL`/`CREATE_NEW` has exactly
/// one link, so every `create_exclusive` / `claim_file_slot` / `claim_dir_slot` /
/// `copy_file_into_claimed_slot` / `create_empty_zip` / `save_manifest` / `split_join` site is
/// **structurally immune** and appears nowhere below. Likewise every site guarded by `clobber_refusal`
/// (`create_slot_refusal`, `rename_slot_refusal` — `folder_template::stamp_nodes`, `copilot`'s tree
/// copy) refuses *any* occupant, so a hard link is already refused there as an occupant.
///
/// ## Refused (the rule below now applies)
///
/// | writer | destination chosen by | where |
/// |---|---|---|
/// | `fsutil::copy_file_onto_no_follow` — `revert_engine::apply_write` + `snapshot_capture::restore` | a **checkpoint manifest** | on the open handle, `facts.links > 1` |
/// | `archive::entry_sink_action` — rows 16/19/20/21/22 of the CPE-1733 table (zip, 7z, tar sinks) | an **archive entry** | this function |
/// | `transfer::download_tree`'s leaf | a **remote server** | this function |
/// | `batch_media::open_output_verified` | the user, but audited per batch | already did, since CPE-1642 — a dir census, kinder than a flat refusal |
///
/// ## Accepted, explicitly — CPE-1857's third option, taken on purpose per row
///
/// | writer(s) | why the limit is accepted rather than closed |
/// |---|---|
/// | `archive`'s six compressors (`compress_to_zip{,_encrypted,_streamed,…}`, `compress_to_targz{,_streamed}`) and the two `.gz` extract leaves | the destination is a name **the human typed into a Save dialog** and confirmed overwriting. No untrusted input picks it, so the threat model this ticket is about does not reach them, and refusing a hard-linked archive destination would be a capability loss with nothing behind it. |
/// | `backup.rs::copy_one_verified` (`fs::copy(src, dst)`, no guard of any kind) | user-named destination **root**, `rel` from our own scan of the source tree. Same reasoning — but this one has *no* link guard at all, not even for symlinks, and it deserves its own ticket rather than a line edited in passing here. **Flagged, not fixed.** |
/// | `secure_shred::shred_file` | writing through the existing inode **is the operation**. A shred of a multiply-linked file destroying the data at every name is what a shred means. |
/// | `native_meta`'s Windows ADS write | a hard link shares the inode and therefore shares the alternate data streams. That is inode semantics, not a redirected write. |
/// | the private JSON stores and journals — `settings`, `tags`, `column_config`, `macro_store`, `snapshot_schedule`, `folder_template`'s catalog, `tray_quick`, `connections`, `audit_journal`, `checkpoint_store`, `metrics_journal`, `replay_baseline`, `index`, `semantic_index`, `vector_index`, `known_hosts`, `snapshot_capture::save_store`, `thumbnail`'s cache, `bin/ticket_mcp` | fixed names the app owns and rewrites, inside its own app-data directory. Planting a hard link at one needs local filesystem write access, which is outside a threat model whose whole premise is that a **planted manifest cannot create a link, only aim at one**. Refusing would break settings saving and journal appends for a hazard nothing in the model can stage. |
/// | `snapshot_capture::capture`'s `fs::copy` into `blobs/<hash>` | already gated: `classify_target_slot(...) == Occupied → continue`, and a hard link is an ordinary existing file, so it reads `Occupied` and is skipped rather than written through. |
/// | `provider::LocalProvider::write` (reached by `transfer::upload_tree`) | the "remote" path is the user's own chosen upload target, not server-chosen. `download_tree` is the direction untrusted data picks the name, and that is the direction that changed. |
///
/// # `!facts.is_dir` is load-bearing, and leaving it out reddened two of the three CI legs
///
/// **On Unix every directory has `nlink >= 2` by construction** — its own `.` entry plus the entry the
/// parent holds for it, plus one more per subdirectory. On Windows a directory's `nNumberOfLinks` is
/// just one. So a link-count rule without this clause refuses **every** directory on Linux and macOS
/// and **no** directory on Windows — precisely the shape that passes a Windows-only local run and reds
/// the matrix. Measured: `cpe1759_a_link_entry_overwrites_an_ordinary_file_but_a_directory_is_a_failure`
/// went red on `ubuntu-latest` and `macos-latest` and green on `windows-latest` from exactly this,
/// turning a tar link entry's "cannot displace a DIRECTORY, that is the write failing" **abort** into a
/// hard-link **skip**.
///
/// Excluding directories costs nothing this exists for: a directory is not something a file's bytes can
/// be written into, and every caller refuses one on its own terms already. This function's job is only
/// the *hard link* question, which is a question about files.
///
/// **CPE-1881: no longer called from production code**, kept anyway rather than deleted. Its one
/// production caller, `revert_engine::apply_write`'s refusal classifier, switched to calling
/// [`name_links`] directly so it can also read the link COUNT (to report a grouped hard-link refusal
/// without a second filesystem probe) — the same answer this function computes, just not thrown away.
/// Retiring this function outright would also orphan the table immediately above, which several other
/// modules' doc comments cross-reference by name; kept `#[allow(dead_code)]` with its own test still
/// pinning the three-way answer (`One`/`Many`/`NoFileHere` via `name_links`, exercised through this
/// wrapper) rather than moving or duplicating that documentation.
#[allow(dead_code)]
pub(crate) fn name_is_multiply_linked(path: &std::path::Path) -> bool {
    matches!(name_links(path), NameLinks::Many(_))
}

/// How many names the object at `path` has — the **three-valued** form of the CPE-1857 question, for the
/// callers that decide a write with it rather than a wording.
///
/// # Why three values, and why the three call sites answer `Unknown` differently
///
/// [`name_is_multiply_linked`] above collapses this to a `bool` and treats `Unknown` as "no". That is
/// right at its **one** caller, `revert_engine::apply_write`'s refusal *classifier*: the write there is
/// already settled, so an unknown answer picks `transient` — the classification that path had for every
/// refusal before CPE-1857 — and degrades to the previous behaviour rather than to a new wrong one.
///
/// It is **wrong** at a caller that is deciding whether bytes move, and CPE-1857's Security Auditor
/// found exactly that: `archive::entry_sink_action` and `transfer::download_tree` are *gates*, and a
/// gate that answers "no" when it cannot tell fails **open** — the guard is present, silent, and the
/// write proceeds. Compare `resolve_output_containment`, which maps [`Probe::Unreadable`] to
/// `Containment::Unverifiable` and refuses. Both gates refuse on [`NameLinks::Unknown`], on the same
/// terms they already refuse an unreadable *link* verdict.
///
/// **Where those two gates are now — CPE-1913 moved them, and this paragraph used to describe a world
/// that no longer exists.** It said `archive` aborts and `transfer` records the entry in `undelivered`
/// "matching `LeafProbe::Uninspectable`". `LeafProbe` is gone, and neither the **zip** extraction loop
/// nor `download_tree` asks this function anything any more: both open their destination through
/// [`crate::open_beneath::create_beneath`] and ask [`handle_facts`] instead, which answers about the
/// **open handle** rather than about a name. The outcomes are unchanged and deliberately so — an
/// undescribable handle is `crate::fsutil::claim_destination_handle`'s fail-closed arm, carrying
/// `Refusal { policy: false }`, so `archive` still aborts and `transfer` still ends `Err` — but the
/// question that produces them is a different one, asked one layer down.
///
/// This function is still the gate for the **tar** and **7z** legs via `archive::entry_sink_action`
/// (`archive` aborts, matching `entry_slot_action`'s `Unknown` arm and
/// `cpe1759_an_unreadable_slot_aborts_both_tar_paths…`), and it is still the *classifier* for
/// `revert_engine::apply_write`'s hard-link count after a refusal has already been decided.
///
/// **`Unknown` is now rare on purpose, which is what makes failing closed on it payable.** Before the
/// [`probe_no_follow`] / [`probe_facts_no_follow`] split, a degenerate identity — every object on some
/// network redirectors — produced `Unreadable`, so failing closed here would have refused every entry
/// landing on a name that **already exists** on such a share. Not literally every entry: `Probe::Absent`
/// never passes through `real_facts`, so a first-time extraction into an empty folder would still have
/// worked end to end. The damage was serious anyway, because the archive answer is an *abort* — the
/// first pre-existing name kills the remainder of an overwrite-extraction. (Measured by the independent
/// Security Auditor with the split reverted: a two-entry zip extracted `aaa.txt` normally and then
/// refused and stopped on the pre-existing `note.txt`.) Reading `links` off the ungated facts answers
/// those correctly, and leaves `Unknown` for what it should always have meant: the probe genuinely could
/// not read the name.
pub(crate) enum NameLinks {
    /// Provably exactly one name: nothing here for this rule to refuse.
    One,
    /// Provably more than one name — the count, so the refusal can say it.
    Many(u64),
    /// Nothing at this name, or it is a link, or it is a directory. **On Unix a directory's `nlink` is
    /// `>= 2` by construction** (its own `.`, the parent's entry, one per subdirectory) while on Windows
    /// it is 1, so a rule that did not fold directories in here refused every directory on two of the
    /// three CI legs and none on the third — measured, see this module's `cpe_1857_…` test.
    NoFileHere,
    /// Something is there and its link count could **not** be read. Never "no" at a gate.
    Unknown(&'static str),
}

pub(crate) fn name_links(path: &std::path::Path) -> NameLinks {
    // `probe_facts_no_follow`, NOT `probe_no_follow`: the degeneracy gate answers an identity question
    // and this is not one. See `probe_no_follow`'s doc for the fail-open that caused.
    match probe_facts_no_follow(path) {
        Probe::Real(facts) if facts.is_dir => NameLinks::NoFileHere,
        Probe::Real(facts) if facts.links > 1 => NameLinks::Many(facts.links),
        Probe::Real(_) => NameLinks::One,
        Probe::Absent | Probe::Link => NameLinks::NoFileHere,
        Probe::Unreadable(why) => NameLinks::Unknown(why),
    }
}

/// Probe a path **without following links**, then gate the result on
/// [`FileIdentity::is_degenerate`] — the identity question every *containment* caller asks.
///
/// **Split from [`probe_facts_no_follow`] by CPE-1857, and the split is the whole of that finding.**
/// The gate below throws the ENTIRE probe away when the identity is degenerate, because a containment
/// decision that compares identities cannot be made from an identity that identifies nothing. But
/// `FileFacts::links` is *not* an identity: on the network redirectors [`FileIdentity::is_degenerate`]
/// exists for, `GetFileInformationByHandle` succeeds and `nNumberOfLinks` is present and correct while
/// the file index is zero. Funnelling the link-count question through this gate discarded a good answer
/// and returned [`Probe::Unreadable`] — which [`name_links`]'s first cut then treated as "not multiply
/// linked", so on **a network share, a first-class destination for this app**, extraction and download
/// wrote through a pre-existing hard link exactly as before, with the guard present and silent.
///
/// So: identity questions come through here; the link-count question goes to [`name_links`], which reads
/// [`probe_facts_no_follow`] directly. Nothing else changed — every existing caller still gets the gate.
fn probe_no_follow(path: &std::path::Path) -> Probe {
    match probe_facts_no_follow(path) {
        Probe::Real(facts) => facts_or_unreadable(facts),
        other => other,
    }
}

/// Wrap freshly-probed facts as [`Probe::Real`] — the one place [`probe_facts_no_follow`]'s two
/// `#[cfg]` arms converge, so the test seam below applies identically on both.
fn real_facts(facts: FileFacts) -> Probe {
    #[cfg(test)]
    match PROBE_INJECTION.with(|c| c.get()) {
        Some(ProbeInjection::DegenerateIdentity) => {
            return Probe::Real(FileFacts { id: FileIdentity { volume: 0, index: 0 }, ..facts })
        }
        Some(ProbeInjection::Unreadable) => return Probe::Unreadable(WHY_PROBE_FAILED),
        // Matched explicitly rather than folded into `None`: these variants are `handle_facts`'s, and a
        // wildcard here would silently start swallowing any later addition — which is exactly how this
        // arm earned its comment, since CPE-1958's `HandleUnderReportsLinks` landed on it as a compile
        // error rather than as a silent behaviour change to the by-PATH probe.
        Some(
            ProbeInjection::HandleUndescribable
            | ProbeInjection::HandleUnderReportsLinks
            | ProbeInjection::StagingCreateFails,
        ) => {}
        None => {}
    }
    Probe::Real(facts)
}

/// What [`set_probe_injection_for_test`] can make a probe pretend to be.
///
/// **Both shapes exist because neither can be staged on CI or on a developer's box**, and CPE-1857's
/// Security Auditor found a live fail-open in the first of them. A real SMB/NFS redirector that returns a
/// zero file index is not something a test can conjure, and the auditor tried a denied
/// `FILE_READ_ATTRIBUTES` ACE for the second and it does not reach this path on Windows. A fail-open no
/// test can reach is one that comes back, so the seam is the instrument that makes both reachable.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeInjection {
    /// Succeed, with a link count and directoriness that are correct, and an identity that identifies
    /// nothing — the documented behaviour of several network redirectors ([`FileIdentity::is_degenerate`]).
    DegenerateIdentity,
    /// Fail to establish anything about the name at all.
    Unreadable,
    /// Make [`handle_facts`] return `None` — the OPEN HANDLE cannot be described.
    ///
    /// A different question from [`Self::Unreadable`], which blinds the by-**path** probe
    /// [`probe_facts_no_follow`]. CPE-1913 moved every write leg's link-count and directoriness
    /// question off the path and onto the handle, so this is the shape that now has to fail closed —
    /// and, exactly like the other two, it cannot be staged: `GetFileInformationByHandle` on a handle
    /// the OS just returned succeeds on every filesystem a test can reach, and `File::metadata` on a
    /// live fd is close to unfailable. A fail-open no test can reach is one that comes back.
    HandleUndescribable,
    /// Make [`handle_facts`] report `links: 1` no matter what the OS said — the **lying predicate** for
    /// the one fact in this module that an attacker can genuinely change between the open and the read
    /// (CPE-1958).
    ///
    /// Unlike its two siblings this is not staging an unreachable failure: `nNumberOfLinks` reading 1 on
    /// a multiply-linked object is exactly what a `hard_link` / `remove_file` racer produces, measured at
    /// 35 in 2,000 trials against the pre-CPE-1958 `fsutil::overwrite_confirmed_no_follow`. What cannot
    /// be staged is the *timing*, so this injects the outcome instead and lets the assertion be about
    /// the filesystem: with the count lying, does a write still stay out of the file outside the folder?
    /// A racing proof of the same property is a coin toss per run; this one is deterministic, which is
    /// why it is the version CI gets.
    HandleUnderReportsLinks,
    /// Make `fsutil`'s staging opener fail (CPE-1961) — the **only** way left to exercise a failed
    /// write through [`VerifiedOutput::write_all`].
    ///
    /// Before CPE-1961 that test needed no seam at all: `write_all` truncated and wrote through the
    /// handle it was handed, so passing it a **read-only** handle failed the very first step on every
    /// platform. Staging removed that, and correctly — the destination handle is now read for its
    /// metadata and its carry-over and never written through, so a read-only one succeeds, which is
    /// the fix working rather than a test going stale.
    ///
    /// What is left to fail is the exclusive create of the staging sibling, and *that* cannot be staged
    /// portably: it needs a directory this process may open but not create in, which is one `chmod` on
    /// Unix and a hand-built denying DACL on Windows. So the outcome is injected instead and the
    /// assertions stay where they belong — on the filesystem: is the file this call created gone, and
    /// is the file it did **not** create still holding its own bytes?
    StagingCreateFails,
}

#[cfg(test)]
thread_local! {
    /// Thread-local for [`CENSUS_CAP_OVERRIDE`]'s reason: the default harness runs each `#[test]` on its
    /// own thread.
    static PROBE_INJECTION: std::cell::Cell<Option<ProbeInjection>> = const { std::cell::Cell::new(None) };
}

/// `pub(crate)` so [`crate::archive`]'s and [`crate::transfer`]'s tests can drive both shapes through
/// their **real** entry points rather than through this module's internals — which is the whole point,
/// since CPE-1857's finding 1 was in how those two call sites read this module's answer, not in the
/// answer itself.
///
/// Set it back to `None` before the test returns; the [`ProbeReset`] guard does that even on a panic.
#[cfg(test)]
pub(crate) fn set_probe_injection_for_test(v: Option<ProbeInjection>) {
    PROBE_INJECTION.with(|c| c.set(v));
}

/// The [`ProbeInjection::StagingCreateFails`] seam, read by `fsutil`'s staging openers (CPE-1961).
///
/// A function rather than a `pub(crate)` on the thread-local, so the vocabulary has one owner here and
/// `fsutil` asks a question instead of reaching into this module's state — the same shape
/// `handle_facts_injected_links` already uses.
#[cfg(test)]
pub(crate) fn probe_injection_is_staging_create_failure() -> bool {
    matches!(PROBE_INJECTION.with(|c| c.get()), Some(ProbeInjection::StagingCreateFails))
}

/// RAII reset for [`set_probe_injection_for_test`] — an injected probe that outlives its test would
/// silently corrupt every later test on the same thread, which is a far worse failure than the one it
/// is there to find.
#[cfg(test)]
pub(crate) struct ProbeReset;

#[cfg(test)]
impl ProbeReset {
    pub(crate) fn arm(v: ProbeInjection) -> Self {
        set_probe_injection_for_test(Some(v));
        ProbeReset
    }
}

#[cfg(test)]
impl Drop for ProbeReset {
    fn drop(&mut self) {
        set_probe_injection_for_test(None);
    }
}

/// Probe a path's identity **without following links** — Unix reads it straight off the one
/// `symlink_metadata` call this module already made before CPE-1642 (`dev`/`ino`/`nlink` are all on
/// `MetadataExt`, no extra syscall).
#[cfg(unix)]
fn probe_facts_no_follow(path: &std::path::Path) -> Probe {
    use std::os::unix::fs::MetadataExt;
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Probe::Link,
        Ok(meta) => real_facts(FileFacts {
            id: FileIdentity { volume: meta.dev(), index: u128::from(meta.ino()) },
            links: meta.nlink(),
            is_dir: meta.is_dir(),
        }),
        // ENOTDIR (a path component isn't a directory) means nothing can exist at this path either.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(20) => Probe::Absent,
        Err(_) => Probe::Unreadable(WHY_PROBE_FAILED),
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
/// A reparse point is then classified by its real **tag** ([`classify_reparse_tag`], CPE-1652 finding A),
/// read from this same handle by [`reparse_tag_of`]: a symlink/junction is a [`Probe::Link`], a
/// non-surrogate reparse point (cloud placeholder, dedup stub) is reported as the real file it is — those
/// are ordinary files to every reader, and calling them links would strand ordinary batches in
/// OneDrive-backed folders — and a **name-surrogate** tag this code does not recognise is
/// [`Probe::Unreadable`], because the write (which does *not* pass `FILE_FLAG_OPEN_REPARSE_POINT`) would
/// follow it somewhere the probe cannot predict. That last case is what `std`'s `is_symlink()` used to get
/// wrong here.
///
/// **The path handed to `CreateFileW` goes through [`verbatim_wide`] first (CPE-1642, reviewer finding
/// REV-G/REV-G2).** Without it the probe and the writer address different sets of files: every write in
/// this crate goes through `std::fs`, which applies the same `\\?\` transformation and therefore reaches
/// past `MAX_PATH`, while a raw `CreateFileW` is capped at it. That mismatch made an over-`MAX_PATH` output
/// fail to open with `ERROR_PATH_NOT_FOUND`, classify as [`Probe::Absent`], and fail OPEN.
#[cfg(windows)]
fn probe_facts_no_follow(path: &std::path::Path) -> Probe {
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
        // CPE-1652 finding A: read the actual reparse TAG off the same handle, before closing it — but
        // ONLY when the attributes say there is a reparse point to classify. The first cut of this asked
        // unconditionally, which the independent reviewer caught as a real regression in the exact path
        // CPE-1652 exists to make cheaper: an extra `GetFileInformationByHandleEx` on **every** probe of
        // **every** ordinary file, and the link census probes up to `census_cap()` entries — roughly
        // doubling its per-entry syscall cost for a value that is then not consulted. Ordinary files
        // (the overwhelming majority of every census) now pay nothing for it.
        let tag = if ok && info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            reparse_tag_of(handle)
        } else {
            None
        };
        let _ = CloseHandle(handle);
        if !ok {
            return Probe::Unreadable(WHY_PROBE_FAILED);
        }
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            // A reparse point whose tag could not be read cannot be classified, and an unclassifiable
            // reparse point is exactly the "probe and writer may disagree" case — refuse.
            let Some(tag) = tag else { return Probe::Unreadable(WHY_SURROGATE_TAG) };
            match classify_reparse_tag(info.dwFileAttributes, tag) {
                ReparseKind::Link => return Probe::Link,
                ReparseKind::UnknownSurrogate => return Probe::Unreadable(WHY_SURROGATE_TAG),
                // Non-surrogate (cloud placeholder, dedup stub): an ordinary file to every reader, and a
                // write really does land on this file. Fall through to the identity below.
                ReparseKind::NotReparse | ReparseKind::OpaqueData => {}
            }
        }
        real_facts(FileFacts {
            id: FileIdentity {
                volume: u64::from(info.dwVolumeSerialNumber),
                index: (u128::from(info.nFileIndexHigh) << 32) | u128::from(info.nFileIndexLow),
            },
            links: u64::from(info.nNumberOfLinks),
            is_dir: info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
        })
    }
}

/// Read a reparse point's **tag** off an already-open handle (CPE-1652 finding A), via
/// `GetFileInformationByHandleEx(FileAttributeTagInfo)`. `None` when the query fails, which the caller
/// treats as "unclassifiable ⇒ refuse" for anything carrying `FILE_ATTRIBUTE_REPARSE_POINT`.
///
/// Handle-based on purpose: the alternative (`std::fs::symlink_metadata`, what this replaces) is a second
/// **path**-based resolution, which can disagree with the handle already open — a different file if the
/// name was swapped in between, and a different reach if the path form differs. One handle, one answer.
#[cfg(windows)]
fn reparse_tag_of(handle: windows::Win32::Foundation::HANDLE) -> Option<u32> {
    use windows::Win32::Storage::FileSystem::{
        GetFileInformationByHandleEx, FileAttributeTagInfo, FILE_ATTRIBUTE_TAG_INFO,
    };

    // SAFETY: `handle` is a live handle owned by the caller (closed by it, not here). `info` is a
    // correctly-sized, properly-aligned out-parameter of exactly the type `FileAttributeTagInfo` names,
    // and its size is passed as the buffer length — the standard shape for this API.
    unsafe {
        let mut info: FILE_ATTRIBUTE_TAG_INFO = std::mem::zeroed();
        GetFileInformationByHandleEx(
            handle,
            FileAttributeTagInfo,
            std::ptr::addr_of_mut!(info).cast(),
            std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
        .ok()?;
        Some(info.ReparseTag)
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
pub(crate) fn verbatim_wide(path: &std::path::Path) -> Vec<u16> {
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
        return Probe::Unreadable(WHY_MAX_PATH_AMBIGUOUS);
    }

    let kind = std::io::Error::from_raw_os_error(code as i32).kind();
    if kind == std::io::ErrorKind::NotFound
        || matches!(code, ERROR_INVALID_NAME | ERROR_BAD_PATHNAME | ERROR_DIRECTORY)
    {
        Probe::Absent
    } else {
        Probe::Unreadable(WHY_PROBE_FAILED)
    }
}

/// Fail-closed stub for any platform that is neither Windows nor Unix: with no way to establish identity,
/// an existing output can never be proven contained. Nothing this crate ships targets such a platform
/// (CI is Windows + macOS + Linux); this exists so the module cannot silently compile into a
/// pattern-matching-only build.
#[cfg(not(any(windows, unix)))]
fn probe_no_follow(path: &std::path::Path) -> Probe {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Probe::Unreadable(WHY_PROBE_FAILED),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Probe::Absent,
        Err(_) => Probe::Unreadable(WHY_PROBE_FAILED),
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

/// A planned output that has been **opened** for writing and verified **on that handle** — the fix for
/// the security audit's finding 1 on PR #848 (executed, byte-level data loss outside the selected folder).
///
/// The audit measured the previous design precisely: the path-based re-check passed in 25 µs, the image
/// transform that ran after it took 528 ms, and `fs::write` came only then — so the entire transform sat
/// inside the check-to-write window. An unprivileged `mklink /H outside\victim.txt selected\photo-1000.png`
/// landing anywhere in that window redirected the write onto a file outside the folder: `written = 1`,
/// `skipped = []`, victim 35 → 17120 bytes, with `confirmed_overwrite` false and never consulted. No
/// amount of re-checking *by path* can fix that, because the name and the object it denotes are two
/// different things and only the object can be pinned.
///
/// So this is "one handle, one answer" — the same argument [`reparse_tag_of`] already makes:
///
/// 1. **Claim the name atomically.** `create_new` (`O_CREAT|O_EXCL` / `CREATE_NEW`) either creates the
///    output or tells us something was already there — no window in which a third party can slip a link
///    in at that name, and no ambiguity about whether cleanup on refusal is ours to do.
/// 2. **Never follow a link at the final component.** `O_NOFOLLOW` on Unix, `FILE_FLAG_OPEN_REPARSE_POINT`
///    on Windows, so an existing symlink/junction yields either an error or a handle to the *link itself*,
///    which step 3 refuses. The link-swap class stops being a race and becomes structurally impossible.
/// 3. **Decide on the handle, not the path.** Identity, hard-link count and directoriness are read from
///    the open handle via `fstat`/`GetFileInformationByHandle`; a multiply-linked file is settled against a
///    **freshly scanned** census. Nothing is replayed from a memo (finding 2) and nothing is re-resolved
///    from a path string (finding 3).
/// 4. **Write through that same handle.** Truncate + write on the object already pinned in step 1-2.
///
/// **What this closes, and what it does not.** The link-swap and name-substitution classes are closed
/// outright: after step 1 the name is taken, so a later `hard_link`/`symlink` at it simply fails. The
/// residual is one irreducible race: an attacker adding a *new outside name* for the very object we hold
/// between step 3's link-count read and step 4's write, versus the 528 ms the audit exploited. Closing it
/// entirely would need filesystem locking the platforms do not offer for this case, so it is stated here
/// rather than papered over — and, per CPE-1667, stated with numbers actually measured on both of the two
/// paths through this window, not a single figure asserted for both.
///
/// **All figures below are `--release` measurements.** Under `cargo test` — what CI runs, and what a
/// reader re-running these tests will see — the same two tests print **491.8 ms transform / 207,700 ns
/// window** and **534.4 ms / 566,400 ns**, because a debug build's image transform is roughly 12x slower.
/// The conclusions hold in both profiles (both windows are hundreds of microseconds; neither branch is
/// decisively the wider one), but do not be surprised when the printed numbers do not match these
/// (PR #856 review). The `canonicalize` **counts** are profile-independent and are the durable figures.
///
/// - **`created == true`** (the output did not exist — no foreign-overwrite question to ask): a handful
///   of syscalls, genuinely microseconds. Measured here: **224,400 ns** for a single 2000×2000-pixel item
///   against a 39.8 ms transform. (The security audit measured 174,600 ns / 97,100 ns — single item /
///   400-item batch — against a 445.6 ms transform on its own machine; different hardware, same order of
///   magnitude.)
/// - **`!created`** (something already occupied the name, so `is_foreign_overwrite` — "is that something
///   one of this batch's own selected inputs?" — runs *inside* this window before the write). Before
///   CPE-1667 this scanned the batch's inputs pairwise with [`same_file`], `O(n)` per check, so THIS
///   branch's window scaled with batch size rather than being "two syscalls" — measured directly, on a
///   300-item batch with the matching input placed at the far end (the linear scan's worst case): **18.6
///   ms** (898 `canonicalize` calls) on unmodified `main`. CPE-1667 replaced the scan with a single lookup
///   into a `HashSet<PathKey>` of the batch's own inputs, built once outside this window
///   ([`crate::batch_execute`]'s `input_path_keys`): the same 300-item worst case now measures **434,000
///   ns** (3 `canonicalize` calls) — batch-size-independent, and the same order of magnitude as
///   `created == true` above rather than orders of magnitude wider. Neither branch is decisively the
///   wider one now.
pub(crate) struct VerifiedOutput {
    file: std::fs::File,
    /// True when *this* call created the file, so a later refusal knows the empty file is ours to remove
    /// (and, just as importantly, that a refusal on a file we did NOT create must leave it untouched).
    created: bool,
}

impl VerifiedOutput {
    /// True when the output did not exist before this call — the write-time answer to "is anything being
    /// overwritten at all?", read from the atomic create rather than from a `Path::is_file()` stat that
    /// could be stale by the time it matters.
    pub(crate) fn created(&self) -> bool {
        self.created
    }

    /// Truncate and write. Separate from opening so the caller can apply its own last checks (the
    /// foreign-overwrite question, which needs the batch's own item list) between verification and the
    /// first destructive byte.
    ///
    /// **Cleans up on failure, same as [`Self::abandon`] (CPE-1667).** If `set_len`/`write_all`/`flush`
    /// itself errors — a genuine disk I/O fault (disk full, device yanked mid-write, quota hit), never an
    /// adversarial path, since every attacker-controllable rejection already returned before any byte was
    /// touched — the file this call created would otherwise be left behind empty or holding a partial
    /// write. `path` is the same string the caller already has: this struct deliberately holds no path of
    /// its own (see the struct doc for why identity is decided on the handle, never a name), so the caller
    /// passes it through exactly as it does for [`Self::abandon`].
    /// **CPE-1961: this no longer writes through the handle [`open_output_verified`] checked.**
    ///
    /// It used to be `set_len(0)` + `write_all` on `self.file`, and that made every guard in
    /// `open_output_verified` — the `links > 1` census in particular — a check-then-use. `links` is a
    /// property of the object at the moment it is read: hard-link a file outside the folder to the
    /// output, let the open land on that object, unlink the second name before `handle_facts` runs, and
    /// the count reads `1` while these bytes destroy a file the batch was never allowed to touch.
    /// Measured live, victim bytes read back off disk: **24 / 2,000 (Windows/NTFS)**,
    /// **2,890 / 10,000 (Linux/ext4)**.
    ///
    /// **The Windows figure was never a safety property.** `classify_output_containment` runs before
    /// the open and refuses a flickering destination outright, so far fewer trials reach the identical
    /// check-then-use (728 writes reported `Ok` here against `fsutil`'s 1,234 in the same run). That is
    /// a *path gauntlet* — CPE-1929's shape, where a guard survives because an earlier check happens to
    /// reject most attempts — and Linux does not have it, which is the whole of the two-order-of-
    /// magnitude gap between the platforms. It is recorded rather than relied on.
    ///
    /// So the bytes now go into an exclusively-created sibling and are committed over the name with a
    /// rename, through the one implementation `fsutil` already owns
    /// (`fsutil::stage_bytes_over_checked_handle` → `stage_and_replace_at`). What that costs — a
    /// changed file id, the destination's ACL and named streams carried explicitly instead of by
    /// `ReplaceFileW`, write access to the output's *folder*, one `fsync` and one rename — is CPE-1958's
    /// cost list, reached here for the same reason and documented at the same site.
    ///
    /// **One cost is this engine's own and is not on that list: a hard-linked output stops tracking.**
    /// A batch item whose output is a second name for a file elsewhere *inside the selected folder* is
    /// allowed (the census says every name is accounted for), and it used to write through the shared
    /// object, so both names showed the new bytes. Now only the output name does; the sibling keeps the
    /// old ones. That is arguably the more correct answer for an operation asked to write one output,
    /// and it is the same trade `fsutil::claim_destination_handle` makes — but it is a behaviour change,
    /// so it is stated here and it is why
    /// `batch_execute::tests::cpe_1652_a_census_past_the_cap_refuses_the_write_rather_than_allowing_it`
    /// has to re-link its fixture between its two phases.
    ///
    /// **Red-proofed by sabotage, run rather than argued.** Reverting this body to the pre-CPE-1961
    /// `set_len(0)` + `write_all` + `flush` through `self.file` makes
    /// `cpe_1961_a_write_never_lands_in_a_file_outside_the_batchs_folder` fail with the outside file
    /// holding `ATTACKER PAYLOAD` **and the call returning `Ok(())`** — a successful-looking batch write
    /// that destroyed a file the batch was never pointed at.
    pub(crate) fn write_all(self, bytes: &[u8], path: &str) -> Result<(), String> {
        // CPE-1963: the handle is handed over rather than lent. It is closed before the commit, because
        // the handle-relative rename cannot replace a destination anything still holds open — see that
        // function's `drop(destination)` for the measurement. Destructured first so the `created` flag
        // survives the move.
        let VerifiedOutput { file, created } = self;
        let result = crate::fsutil::stage_bytes_over_checked_handle(
            std::path::Path::new(path),
            bytes,
            file,
            created,
        )
        .map_err(|e| format!("could not write output: {e}"));
        if result.is_err() && created {
            let _ = std::fs::remove_file(path);
        }
        result
    }

    /// Give up without writing, removing the file **only** if this call created it.
    pub(crate) fn abandon(self, path: &str) {
        if self.created {
            drop(self.file);
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Unix `O_NOFOLLOW`. Hard-coded per target rather than taking a `libc` dependency the crate does not
/// otherwise have. The value is asserted at runtime by
/// `secaudit_open_output_verified_refuses_a_symlink_final_component`: if it were wrong the open would
/// follow the link and that test would fail, so a bad constant cannot ship silently.
#[cfg(target_os = "linux")]
const O_NOFOLLOW: i32 = 0o400000;
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd", target_os = "dragonfly"))]
const O_NOFOLLOW: i32 = 0x0100;
/// Any other Unix: 0 is a no-op flag, so the open would follow a link — the post-open `symlink_metadata`
/// belt-and-braces check in [`open_output_verified`] is what refuses there. No such platform is shipped
/// or tested; this exists so the module cannot fail to compile into one.
#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd", target_os = "dragonfly"))))]
const O_NOFOLLOW: i32 = 0;

/// `FILE_FLAG_OPEN_REPARSE_POINT` — the same flag [`probe_no_follow`] passes, so the writer and the probe
/// address objects the same way.
#[cfg(windows)]
pub(crate) const FILE_FLAG_OPEN_REPARSE_POINT_U32: u32 = 0x0020_0000;

// **CPE-1961's un-shadowing seam.** What to hard-link over the output between
// `classify_output_containment` and `open_no_follow`, as `(victim, at)`. (A plain `//` comment:
// rustdoc does not document a macro invocation and `-D unused-doc-comments` is right to say so.)
//
// # Why a seam rather than a fixture
//
// The `links > 1` census below was **shadowed** (CPE-1929's exact tell, measured): with
// `if false && facts.links > 1` the whole suite stayed green at **2,430 passed / 0 failed**, and with
// the predicate forced to lie (`HandleFacts { links: 1, ..facts }`) it stayed green at **2,430** as
// well. Two green sabotages on one guard.
//
// The shadowing check is [`classify_output_containment`]'s **path**-side link probe, which runs before
// the open and refuses a multiply-linked output whose other name is outside the folder. Every fixture
// anyone had planted the link *before* the call, so the path probe always answered first.
//
// **Neither "reorder" nor "delete" is the right answer here, and that is not a dodge.** The two guards
// do not answer the same question at the same moment: the path probe reads the name *before* the open,
// the census reads the object *after* it, and a hard link planted **in between** is caught by exactly
// one of them — the census. It cannot be moved earlier (there is no handle before the open) and it is
// not redundant (it covers a window nothing else does). What it lacked was a fixture that could reach
// that window, so this seam builds one, and
// `cpe_1961_a_link_planted_after_the_path_check_is_still_caught_by_the_handle_census` is the coverage.
// With that test in place the *disable* half of the pair goes red, which is what a reachable guard
// looks like. Same shape, and the same reason, as `open_beneath::between_descent_and_leaf`.
#[cfg(test)]
thread_local! {
    static LINK_BETWEEN_CONTAINMENT_AND_OPEN: std::cell::RefCell<Option<(String, String)>> =
        const { std::cell::RefCell::new(None) };
}

// **CPE-1959's use of the same window, for the other shadowed guard.** What GUID reparse tag to plant
// on the output between `classify_output_containment` and `open_no_follow`, as `(at, tag)`. (A plain
// `//` comment, for the reason the seam above gives: rustdoc does not document a macro invocation, and
// `-D unused-doc-comments` is right to say so — measured here as a real warning before it was demoted.)
//
// The narrowed reparse refusal below is reachable from only ONE fixture class without this — a real
// symlink — because `classify_reparse_tag` answers `WHY_SURROGATE_TAG` by path, before the open, for
// every *other* surrogate tag. So for the non-symlink surrogate class the handle check was still
// shadowed exactly the way CPE-1929 describes. Arming this plants the tag on an output that was an
// ordinary file when containment looked at it, which is the only way to hand the handle check a
// non-symlink surrogate at all.
//
// **What that buys is precise, and an earlier draft of this comment overstated it** (PR #1103 review).
// It does NOT make the handle check the *sole* decider: `std`'s `is_symlink` reads the **same**
// name-surrogate bit off a path that `reparse_name_surrogate` reads off a handle — this file already
// says so at `open_output_verified`'s reorder note — so the `symlink_metadata` net below the handle
// check reaches this fixture too, and with the handle check disabled it is what answers. Measured: see
// half 4's own assertion message. What the seam buys is that the handle check answers **first**, on
// wording unique to it, for a class no other fixture can deliver — which is coverage the crate did not
// have.
//
// **The guard is not redundant with that net, and the reason is stronger than "sole decider" was**: the
// handle check reads the OBJECT. An attacker who plants the tag before the open and removes it before
// the `symlink_metadata` defeats the path net and not this one.
//
// Same one-shot, take-don't-read discipline as the hard-link seam above, and the same reason.
//
// **`#[cfg(all(test, windows))]`, and the gate is the whole seam rather than just the plant** — this
// leg is Windows-only *by construction*, three times over: reparse points only exist there,
// `HandleFacts::is_reparse_point` is hard-coded `false` everywhere else, and the fixture-maker
// `fsutil::make_guid_reparse_point` is itself `#[cfg(windows)]`. The consumer
// (`cpe_1959_…_and_a_placeholder_is_written`) is `#[cfg(windows)]` for the first two reasons already,
// so on Linux/macOS every item here would be *both* uncallable and unused.
//
// **Round 4 shipped this arm ungated and it was a compile error on Linux and macOS** —
// `E0425: cannot find function make_guid_reparse_point in module crate::fsutil`, with rustc's own
// *"found an item that was configured out"* note pointing at the `#[cfg(windows)]` on it. Three
// reviewers all worked on Windows and all three were green; the 3-OS matrix is what caught it. The
// generalisable check, which costs nothing and runs anywhere: **when new code names a symbol, confirm
// the symbol's own `#[cfg]` is no narrower than the call site's.** `make_guid_reparse_point` is
// `#[cfg(windows)]`; this was `#[cfg(test)]`; that is the bug, visible by grep from any machine.
#[cfg(all(test, windows))]
thread_local! {
    static SURROGATE_BETWEEN_CONTAINMENT_AND_OPEN: std::cell::RefCell<Option<(String, u32)>> =
        const { std::cell::RefCell::new(None) };
    /// Whether the plant actually took. `make_guid_reparse_point` can decline on a volume that will not
    /// hold one, and a test that cannot tell "the guard refused" from "the fixture never armed" is the
    /// CPE-1923 trap — so the verdict is recorded here for the test to read rather than inferred.
    static SURROGATE_PLANTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Arm the seam above for exactly one call. Taken rather than read, so an armed test cannot leak into
/// the next one on the same thread even if it panics before the call it armed.
#[cfg(test)]
pub(crate) fn link_between_containment_and_open_for_test(victim: &str, at: &str) {
    LINK_BETWEEN_CONTAINMENT_AND_OPEN
        .with(|c| *c.borrow_mut() = Some((victim.to_string(), at.to_string())));
}

/// Arm the reparse-tag seam for exactly one call (CPE-1959). Clears the previous verdict so a stale
/// `true` from an earlier test cannot be read as this one's. Windows-only — see the seam's own comment.
#[cfg(all(test, windows))]
pub(crate) fn surrogate_between_containment_and_open_for_test(at: &str, tag: u32) {
    SURROGATE_BETWEEN_CONTAINMENT_AND_OPEN.with(|c| *c.borrow_mut() = Some((at.to_string(), tag)));
    SURROGATE_PLANTED.with(|c| c.set(false));
}

/// Did the last armed reparse-tag plant take? See [`SURROGATE_PLANTED`]. Windows-only.
#[cfg(all(test, windows))]
pub(crate) fn surrogate_was_planted_for_test() -> bool {
    SURROGATE_PLANTED.with(std::cell::Cell::get)
}

/// Apply an armed GUID reparse tag. Windows-only, because `make_guid_reparse_point` is: split into its
/// own function rather than left as a `#[cfg]`-ed statement inside [`between_containment_and_open`] so
/// the gate sits on an item, where it is visible to a reader scanning declarations.
///
/// **There is deliberately NO non-Windows arm**, and that is the safety-relevant half of the choice.
/// The alternative — a stub that disarms and sets `SURROGATE_PLANTED` to `false` — would compile
/// everywhere and read as portable, and a later edit dropping the `#[cfg(windows)]` from half 4's test
/// would then make the leg *silently pass* on Linux against a fixture that was never planted. That is
/// the unarmed-fixture-reporting-success shape this seam's flag exists to prevent. With no stub, the
/// only consumer is Windows-gated and the compiler enforces it.
#[cfg(all(test, windows))]
fn plant_armed_surrogate() {
    let armed_tag = SURROGATE_BETWEEN_CONTAINMENT_AND_OPEN.with(|c| c.borrow_mut().take());
    if let Some((at, tag)) = armed_tag {
        let ok = crate::fsutil::make_guid_reparse_point(std::path::Path::new(&at), tag, false);
        SURROGATE_PLANTED.with(|c| c.set(ok));
    }
}

#[cfg(test)]
fn between_containment_and_open() {
    let armed = LINK_BETWEEN_CONTAINMENT_AND_OPEN.with(|c| c.borrow_mut().take());
    if let Some((victim, at)) = armed {
        let _ = std::fs::remove_file(&at);
        let _ = crate::links::create_hard_link(&victim, &at);
    }
    #[cfg(windows)]
    plant_armed_surrogate();
}

#[cfg(not(test))]
#[inline]
fn between_containment_and_open() {}

/// Open `output` for writing and verify **on the resulting handle** that writing through it stays inside
/// `input`'s own folder. See [`VerifiedOutput`] for why this replaced the path-based re-check.
///
/// `cache` is deliberately **not** a parameter: this builds its own, per call, so no census
/// ([`ParentCache::dir_scan`]) and no directory identity ([`ParentCache::dir_identity`]) can be replayed
/// from an earlier item or from plan time. That is security-audit findings 2 and 3, and the cost argument
/// against it does not apply — the census is only reached when the output is *multiply linked*, which no
/// ordinary batch output ever is.
pub(crate) fn open_output_verified(input: &str, output: &str) -> Result<VerifiedOutput, String> {
    let mut cache = ParentCache::new();
    // Structural + directory-level containment first: dot-segments, drive-relative shapes, alternate data
    // streams, and "is the output's directory the input's own directory". These are questions about the
    // PATH, so they cannot be answered from a handle; they run against a fresh cache every time.
    match classify_output_containment(input, output, &mut cache) {
        Containment::Inside => {}
        Containment::Escapes => {
            return Err(format!(
                "refusing at write time: \"{output}\" does not stay inside this file's own folder. \
                 Nothing was written for this file"
            ));
        }
        Containment::Refused(why) => {
            return Err(format!("refusing at write time: \"{output}\" {why}. Nothing was written for this file"));
        }
        Containment::Unverifiable(why) => {
            return Err(format!(
                "refusing at write time: couldn't verify that \"{output}\" stays inside this file's own \
                 folder — {why}. Nothing was written for this file; this is a refusal to guess"
            ));
        }
    }

    // The path gauntlet is done and the open has not happened yet. Test-only seam — compiled out of a
    // shipped binary. See its definition for the shadowed guard it exists to un-shadow (CPE-1961).
    between_containment_and_open();
    let (file, created) = open_no_follow(std::path::Path::new(output))
        .map_err(|e| format!("could not open output for writing: {e}"))?;
    let verified = VerifiedOutput { file, created };

    // **The handle answers the link question first (CPE-1929 — the same reorder CPE-1896 made in
    // `fsutil`).** This used to sit *below* the `symlink_metadata(output)` path check that now follows
    // it, and that ordering made this refusal untestable for every fixture anyone had: on Windows
    // `std`'s `is_symlink` reads the same name-surrogate bit, so every symlink and junction was caught
    // by the path check first and this branch only ever saw reparse points nobody had a fixture for.
    // Measured on CPE-1929: `if false && facts.is_reparse_point` left the lib suite **unchanged** — 2,423
    // passed / 0 failed at the time of measurement, which was the whole suite — and forcing
    // `is_reparse_point` to a lying `false` changed no `batch_media` behaviour at all
    // — the two-sabotage tell for a shadowed guard. Handle-first is also the *more trustworthy* order:
    // the object this question is asked of is the one the bytes will land in and it cannot be
    // substituted after the open, whereas the name can be swapped either way in the window before a
    // `symlink_metadata`. Covered now by
    // `cpe_1959_a_surrogate_at_the_output_is_refused_by_the_handle_check_and_a_placeholder_is_written`,
    // and the same sabotage on the merged state is **2,460 passed / 1 failed** against a 2,461 / 0
    // baseline — that test is the red-proof; all three sabotages and the `TMP` caveat that reproduces
    // them are at the refusal itself. (It replaced CPE-1929's
    // `..._a_non_surrogate_reparse_point_at_the_output_is_refused_by_the_handle_check`, which pinned the
    // opposite verdict; see the doctrine section below.)
    //
    // # The reparse-point doctrine split is RESOLVED here, in `fsutil`'s direction (CPE-1959)
    //
    // Until now this refusal read the **bare** `FILE_ATTRIBUTE_REPARSE_POINT` bit while
    // `fsutil::overwrite_confirmed_no_follow` and `fsutil::claim_destination_handle` narrowed to
    // `reparse_name_surrogate`, so the crate wrote a dehydrated cloud placeholder at one site and refused
    // it at another. That was non-regressive but it was two doctrines about one input class. It now asks
    // the narrow question, and **calling `reparse_name_surrogate` IS the derivation** — that function is
    // this crate's single owner of the tag rule (CPE-1933), so no comment here needs to claim agreement
    // with `fsutil`; the shared callee is the agreement. Every symlink, junction and mount point carries
    // the surrogate bit and is still refused, by this same check, before the path check below.
    //
    // **The reason is not the asymmetry argument this site used to carry, which was tested rather than
    // inherited and does not survive.** That argument was: a refused batch item is *skipped*, so
    // over-refusing costs the user one item, whereas a refused restore has failed at its only job. Three
    // things are wrong with it.
    //
    // 1. **The mechanism the refusal defended is gone (CPE-1961).** Its stated property was "a batch
    //    never writes *through* one". `VerifiedOutput::write_all` no longer writes through this handle at
    //    all: it hands the handle to `fsutil::stage_bytes_over_checked_handle`, whose own doc says it
    //    performs "the same three steps `overwrite_confirmed_no_follow` performs after its own checks, in
    //    the same order, for the same reasons; only the checks in front differ". So a placeholder here is
    //    **replaced by name** — the bytes go into a sibling created with `create_new` — exactly as at
    //    `overwrite_confirmed_no_follow`, whose docblock already recorded that this is "a different
    //    question with a different answer (the user gets a real file and loses the placeholder, rather
    //    than a possibly-corrupt object)". CPE-1896's three unresolved worries about writing *through* a
    //    `FILE_OPEN_REPARSE_POINT` handle — `Ok(16)` onto an object then unopenable by path, the reparse
    //    bit surviving `set_len(0)`, `landed_inside` diverging — are unreachable on a staged write. This
    //    site's checks were the last difference between the two, for this input class.
    //
    //    **And the result cannot come back a reparse point either, which is the stronger half and is
    //    structural rather than observed** (PR #1103 review). The staged sibling is a fresh `create_new`
    //    file; the only things carried onto it from the destination are `HandleCarryover`'s, and both
    //    relevant carriers exclude the tag by construction — `read_alternate_data_streams` filters to
    //    `BACKUP_ALTERNATE_DATA` and explicitly skips `BACKUP_REPARSE_DATA`, and
    //    `carried_attribute_mask()` is `HIDDEN|SYSTEM|ARCHIVE|NOT_CONTENT_INDEXED` with no
    //    `REPARSE_POINT` in it. So a filter driver behind a non-surrogate tag (WCI, ProjFS) cannot act
    //    on this write: the bytes never reach it, and nothing hands the tag to the file that replaces
    //    the name. Half 1 of the test measures exactly that — the reparse bit is **gone** afterwards.
    // 2. **The cost of over-refusing is not one item, and the code says so.** OneDrive dehydrates exactly
    //    the files it has already synced, which on a re-run are exactly this batch's own previous
    //    outputs, so the refusal is systematic over the selection rather than incidental — CPE-1896's
    //    "turned every dehydrated file into a failed backup entry", one engine over. And the user is not
    //    shown a list: `batch_execute::execute_plan_walk`'s loop pushes `(input, reason)` into
    //    `BatchReport::skipped`, and `App.svelte`'s `notice.convertedWithSkipped` renders a count plus
    //    **`report.skipped[0]`** only — "{written} converted, {failed} skipped: first \"{name}\" —
    //    {reason}". So the whole user-visible evidence for a fully-dehydrated folder is one sentence,
    //    and until this change that sentence was the link-shaped one below, which for a placeholder is
    //    false (it is not a symlink, a junction, or a name standing in for another) and names no action.
    //    That is the input CPE-1959 was filed to get, derived from the code rather than from asking
    //    anyone.
    // 3. **CPE-1957's unprivileged-plantable finding cuts toward narrowing, not away from it.** Its
    //    Security Auditor planted the real Microsoft non-surrogate tags (`0x9000001A` OneDrive
    //    Files-On-Demand, `0x80000018` Windows Container Isolation) with `FSCTL_SET_REPARSE_POINT` from
    //    an unprivileged process, so "only cloud sync makes these" is false. But an attacker who can
    //    plant a non-surrogate tag on the output can equally plant a **surrogate** one, which both
    //    doctrines refuse. All the bare bit denied was the non-surrogate tag — which by definition does
    //    not redirect the name — on a name this function is about to replace by rename inside a folder it
    //    has already contained. The bare bit was therefore an attacker-**triggerable** refusal, not an
    //    attacker-blocking one: its only marginal effect was to let anyone who can write to the output
    //    folder deny the user their batch item.
    //
    // CPE-1957 answered the same question at `vault_manager`'s wipe and narrowed there **at both of its
    // checks — the by-path `probe.is_link` in `shred_dir_pinned` and the handle check in
    // `overwrite_pinned_file`** — but declined to let that settle this, rightly, because over-refusing at
    // a wipe leaves plaintext on the volume, which is the opposite cost function from a skip. This site
    // is decided on its own three reasons above, not on that one. The remaining per-site verdicts are
    // enumerated in the ticket rather than counted here (CPE-1932: a number written here would be a
    // second, unguarded copy).
    let facts = match handle_facts(&verified.file) {
        Some(f) if !f.id.is_degenerate() => f,
        _ => {
            verified.abandon(output);
            return Err(format!(
                "refusing at write time: \"{output}\" was opened but its filesystem identity could not be \
                 read, so there is no way to tell what a write would land on. Nothing was written"
            ));
        }
    };
    // **Three sabotages, all run on this narrowed predicate, Windows 11 / NTFS, `cargo test -p
    // cpe-server --lib`.** Baseline **2,461 passed / 0 failed / 14 ignored**; each sabotage is
    // **2,460 / 1**, the one failure being `cpe_1959_…` every time. This ticket replaced one test with
    // one test, so it moves the count by zero; the figures were **re-taken after CPE-1957 (PR #1101)
    // merged**, which is what took them from 2,460 to 2,461, rather than left stale by a merge.
    //
    // - **Disabled** (`if false && …`): red.
    // - **Predicate forced to lie** (`reparse_name_surrogate` replaced by a `Some(false)` that still
    //   calls it, so the call is made and the answer is a lie rather than absent): red.
    // - **Un-narrowed back to the bare bit** (`if facts.is_reparse_point`): red. This third one is the
    //   doctrine measurement rather than the reachability one — it is what makes "a placeholder is
    //   written here now" a measured fact and not an assertion.
    //
    // **`TMP` must be on the platform default to reproduce those numbers, and this is not a footnote**
    // (PR #1103 review). Round 1 of this comment reported 2,458 / 2 and blamed "running from a worktree
    // nested inside the repo". That was measured false: the same nested worktree with a normal `TMP`
    // gives 2,460 / 0. The cause is `TMP`'s **location** — placing it inside the repo tree makes
    // `ticket_board::…nearest_project_root_walks_up_to_the_ticketing_folder` find the repo's own
    // `Ticketing/` above the tempdir, and pushes `archive::…cpe1774…` past a path-length limit. A reader
    // reproducing from a nested worktree with a default `TMP` would have got a baseline this comment did
    // not predict, and no way to tell whether the deltas above were stale.
    //
    // Two green sabotages would have meant this refusal was shadowed (CPE-1929); none of the three is
    // green. **Getting there took a correction worth recording**: round 1's fixture for the surrogate
    // half was a GUID surrogate tag, which is refused by `WHY_SURROGATE_TAG` at containment *before the
    // open*, so the disable sabotage came back green and read as safety. The test's own doc has the two
    // fixtures that reach here and what each one proves.
    if facts.is_reparse_point && reparse_name_surrogate(&verified.file).unwrap_or(true) {
        verified.abandon(output);
        return Err(format!(
            "refusing at write time: \"{output}\" is a link that stands in for another name (a symlink, \
             a junction, or a mount point), and a batch never writes through one — such a name's target \
             can be re-pointed after any check, even a dangling link that happens to point back inside \
             this same folder. Nothing was written for this file"
        ));
    }

    // **Second net, and deliberately unreachable on every platform this ships on — do not be alarmed
    // when sabotaging it leaves the suite green (CPE-1929).** On Windows every symlink and junction
    // carries `FILE_ATTRIBUTE_REPARSE_POINT`, so the handle check above has already refused it. On
    // Linux/macOS/BSD `O_NOFOLLOW` makes the *open itself* fail on a link at the final component, so
    // control never arrives here at all. What is left is exactly one case: a Unix that is none of those,
    // where [`O_NOFOLLOW`] is compiled as `0`, the open follows the link, and `is_reparse_point` is
    // hard-coded `false`. No such platform is built or tested, so this branch is untestable *by
    // construction* rather than merely untested. It is kept because the alternative on that platform is
    // writing through a link, and it is argued rather than demonstrated.
    //
    // **The number, because this ticket's own rule is that an argument is not a measurement.** With
    // this backstop and its twin in `fsutil::overwrite_confirmed_no_follow` BOTH disabled
    // (`if false && std::fs::symlink_metadata(..)`), the lib suite is **2,425 passed / 0 failed / 11
    // ignored** — identical to baseline. Both disabled together, so the figure covers each of them
    // individually as well. That green is the expected result here, not a missing test.
    //
    // **"Unreachable" above is an ORDERING claim, and CPE-1959 measured it as one** — worth pinning
    // because a reader can easily upgrade it to "nothing gets here", which is false and would make the
    // guard above look redundant. Disable the handle refusal and this net answers immediately: half 4
    // of `cpe_1959_…` reds carrying *this* branch's sentence, on a fixture whose tag `std` and this
    // check agree is a name surrogate. It is unreachable because something refuses first, not because
    // nothing can see the input. What the handle check has that this does not is the OBJECT: a tag
    // removed between the open and this `symlink_metadata` defeats this net and not that one.
    if std::fs::symlink_metadata(output).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
        verified.abandon(output);
        return Err(format!(
            "refusing at write time: \"{output}\" is a link, and a batch never writes through one — a \
             link's target can be re-pointed after any check, even a dangling link that happens to point \
             back inside this same folder. Nothing was written for this file"
        ));
    }
    if facts.is_dir {
        verified.abandon(output);
        return Err(format!("refusing at write time: \"{output}\" is a directory. Nothing was written"));
    }

    // The only remaining way a write here reaches outside the folder is a hard link: a second name for
    // this very object, living somewhere else. Settled against a census scanned NOW, never a memo.
    //
    // **CPE-1961 demoted this from a defence to a POLICY verdict.** `VerifiedOutput::write_all` no
    // longer writes through the handle these facts came off, so a lying count can no longer destroy
    // anything; what this refusal still does is tell the user that the output they chose has another
    // name the batch was never allowed to touch, which is worth saying.
    //
    // **It was SHADOWED, and it is not any more (CPE-1929's pair, all four runs recorded).** Before
    // this ticket: `if false && facts.links > 1` left the suite **2,430 passed / 0 failed**, and
    // `HandleFacts { links: 1, ..facts }` left it **2,430 / 0** as well — two green sabotages, the
    // exact tell. The shadowing check is `classify_output_containment`'s path-side link probe above,
    // which answers first for every fixture that plants its link before the call.
    //
    // **Reordering was impossible and deleting would have been wrong**, and the reason is that the two
    // are not asking the same question at the same time: the path probe reads the NAME before the open,
    // this reads the OBJECT after it, and a hard link planted **in between** is visible to exactly one
    // of them — this one. So the missing thing was a fixture that could reach that window, and
    // `between_containment_and_open` above is it. With
    // `cpe_1961_a_link_planted_after_the_path_check_is_still_caught_by_the_handle_census` in the suite,
    // BOTH sabotages now red — **2,430 passed / 1 failed** each — and the failing test is that one.
    if facts.links > 1 {
        let (dir, _, _) = split(input);
        let key = if dir.is_empty() { "." } else { &dir };
        let Some(scan) = scan_dir_link_census(std::path::Path::new(key)) else {
            verified.abandon(output);
            return Err(format!("refusing at write time: \"{output}\" — {WHY_CENSUS_FAILED}. Nothing was written"));
        };
        let inside = scan.counts.get(&facts.id).copied().unwrap_or(0);
        if inside < facts.links {
            verified.abandon(output);
            let why = if scan.capped {
                WHY_CENSUS_TOO_BIG
            } else if scan.incomplete {
                WHY_CENSUS_FAILED
            } else {
                "at least one of its other names lives outside the selected folder, so writing here would \
                 change a file the batch was never allowed to touch"
            };
            return Err(format!(
                "refusing at write time: \"{output}\" has {} names and only {inside} of them are in the \
                 selected folder — {why}. Nothing was written for this file",
                facts.links
            ));
        }
    }

    Ok(verified)
}

/// Identity facts read from an **already-open handle** — no path involved, so nothing can have been
/// substituted between the open and this read.
///
/// `pub(crate)` since CPE-1672 — see [`FileIdentity`].
pub(crate) struct HandleFacts {
    pub(crate) id: FileIdentity,
    pub(crate) links: u64,
    pub(crate) is_dir: bool,
    pub(crate) is_reparse_point: bool,
}

/// The [`ProbeInjection::HandleUndescribable`] seam, asked by both real arms of [`handle_facts`] before
/// they do anything, so the injection cannot be honoured on one platform and not the other (CPE-1913).
#[cfg(test)]
fn handle_facts_injected_none() -> bool {
    matches!(PROBE_INJECTION.with(|c| c.get()), Some(ProbeInjection::HandleUndescribable))
}

#[cfg(not(test))]
#[inline]
fn handle_facts_injected_none() -> bool {
    false
}

/// The [`ProbeInjection::HandleUnderReportsLinks`] seam (CPE-1958), asked by both real arms of
/// [`handle_facts`] for the same reason [`handle_facts_injected_none`] is: an injection honoured on one
/// platform and not the other is a test that passes for a different reason on each runner.
#[cfg(test)]
fn handle_facts_injected_links() -> Option<u64> {
    matches!(PROBE_INJECTION.with(|c| c.get()), Some(ProbeInjection::HandleUnderReportsLinks)).then_some(1)
}

#[cfg(not(test))]
#[inline]
fn handle_facts_injected_links() -> Option<u64> {
    None
}

#[cfg(unix)]
pub(crate) fn handle_facts(file: &std::fs::File) -> Option<HandleFacts> {
    use std::os::unix::fs::MetadataExt;
    if handle_facts_injected_none() {
        return None;
    }
    let meta = file.metadata().ok()?;
    Some(HandleFacts {
        id: FileIdentity { volume: meta.dev(), index: u128::from(meta.ino()) },
        links: handle_facts_injected_links().unwrap_or_else(|| meta.nlink()),
        is_dir: meta.is_dir(),
        // Unix has no reparse points; `O_NOFOLLOW` already refused a symlink at the final component.
        is_reparse_point: false,
    })
}

#[cfg(windows)]
pub(crate) fn handle_facts(file: &std::fs::File) -> Option<HandleFacts> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT,
    };

    if handle_facts_injected_none() {
        return None;
    }
    let handle = HANDLE(file.as_raw_handle() as isize);
    // SAFETY: `handle` is borrowed from a live `File` that outlives this call; `info` is a correctly-sized
    // out-parameter. Read-only query — no ownership taken, nothing closed here.
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        if GetFileInformationByHandle(handle, &mut info).is_err() {
            return None;
        }
        Some(HandleFacts {
            id: FileIdentity {
                volume: u64::from(info.dwVolumeSerialNumber),
                index: (u128::from(info.nFileIndexHigh) << 32) | u128::from(info.nFileIndexLow),
            },
            links: handle_facts_injected_links().unwrap_or_else(|| u64::from(info.nNumberOfLinks)),
            is_dir: info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
            is_reparse_point: info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0,
        })
    }
}

/// **`None` here means "cannot tell", and every caller must treat it as a refusal** — CPE-1913 round 2.
///
/// This arm returned `None` under a comment that read "fail closed on a platform whose identity model
/// this module does not know". That was backwards about the *consumer*: `None` is a value, and whether
/// it fails closed or open is decided where it is read. CPE-1913 round 1 read it inside an
/// `if let Some(facts)` with no `else` and let the write proceed — fail **open**, the exact shape
/// CPE-1857 exists to close. `crate::fsutil::claim_destination_handle` now refuses on it, so the claim
/// this comment makes is true again; it is worded as an obligation on the reader rather than as a
/// property of this arm, because that is where it actually lives.
///
/// The arm itself is unreachable in a shipped build: `crate::open_beneath` is `#[cfg(any(unix, windows))]`
/// and the crate deliberately does not compile without it.
#[cfg(not(any(windows, unix)))]
pub(crate) fn handle_facts(_file: &std::fs::File) -> Option<HandleFacts> {
    None
}

/// `IO_REPARSE_TAG_NAME_SURROGATE` — the tag bit Microsoft sets on exactly those reparse points whose
/// name **stands in for another name**: `IO_REPARSE_TAG_SYMLINK` (`0xA000000C`) and
/// `IO_REPARSE_TAG_MOUNT_POINT` (`0xA0000003`, junctions) among them. It is *not* set on tags that
/// merely decorate an object which is still itself — OneDrive Files-On-Demand (`0x9000001A`, measured
/// by CPE-1896's Security Auditor: surrogate bit **clear**, directory bit set), NTFS dedup, WOF/WIM
/// compression, ProjFS, app-exec links.
///
/// One owner for the constant and the rule, because two guards now depend on it and must not drift:
/// `open_beneath`'s per-component directory walk, and
/// `fsutil::copy_file_onto_destination_handle`'s final-component guard.
///
/// `#[cfg(windows)]` because reparse points only exist there and the only reader is the Windows arm of
/// [`reparse_name_surrogate`] below. Ungated, it is `dead_code` on Linux and macOS, and CI runs
/// `-D warnings` — so it reddened **every** job that compiles this crate on those two, while three
/// rounds of local `cargo clippy` on Windows stayed clean. The lesson is the gate, not the constant:
/// anything here that only the Windows arm touches needs one.
#[cfg(windows)]
pub(crate) const IO_REPARSE_TAG_NAME_SURROGATE: u32 = 0x2000_0000;

/// Does this handle's reparse point make the name **stand in for another name**?
///
/// [`HandleFacts::is_reparse_point`] answers the far broader question "does this object carry *any*
/// reparse tag", which is true of a great deal that is not a link — cloud placeholders, dedup, WOF,
/// ProjFS. This asks the narrow one, off `FILE_ATTRIBUTE_TAG_INFO`, in a single handle query that
/// covers both the attribute bit and the tag.
///
/// **Returns `Option`, and the `None` is the point.** `None` means *the description could not be read*,
/// which is a different answer from "not a surrogate". **Both callers now fail closed**
/// (`unwrap_or(true)`), and the default still does not live in here, because the choice is the
/// caller's to argue:
///
/// - `fsutil::copy_file_onto_destination_handle` (the final component, where the bytes go) — "I could
///   not tell whether this name stands for another name" is not a licence to write through it, and
///   there is no later check to catch it.
/// - `open_beneath::sys::name_surrogate_at` (a directory component of the walk) — **changed from
///   `unwrap_or(false)` by CPE-1938 round 2.** It used to fail open, on the grounds that containment
///   there did not rest on this check: a genuine surrogate was caught one component later by NT itself
///   (`ERROR_CANT_RESOLVE_FILENAME`, measured by neutering the check and re-running the CPE-1889
///   junction harm test, which still refused), so all the check bought was naming the link one
///   component earlier.
///
/// **That justification was a claim about another site's control flow, and CPE-1938 is the change that
/// falsified it** — which is [[derive-provenance-dont-claim-it]] (CPE-1933) landing on a doc comment
/// rather than on a test. It was true while every caller's descent was followed by another NT open.
/// `archive::entry_component_action` calls `create_dir_beneath` as a **verification-only** pass in
/// front of a by-path third-party unpacker: for `sub/leaf.txt` the chain is one component, so
/// `name_surrogate_at` is the only thing that can refuse and NT is never asked again. Forced to the
/// old fail-open value on Windows 11, a junction at `dest/sub` restored the CPE-1938 defect silently
/// (`Ok(done: 1, skipped: 0)`, payload in `dest/other`). The split is gone; see that function for the
/// measurement.
///
/// The `None` arm's untestability is what makes fail-closed free here as well as correct: nothing can
/// make `GetFileInformationByHandleEx` fail on a handle just opened successfully, so no shipping path
/// changes behaviour.
///
/// **Both defaults rest on reading, not on measurement, and in a module this heavily measured that is
/// worth saying out loud.** The `None` arm is untestable by construction: no fixture can make
/// `GetFileInformationByHandleEx` fail on a handle that has just been opened successfully, which is the
/// only state either caller reaches it from. Everything either caller does with a `Some` is pinned by
/// sabotage-verified tests; the `None` arm has no test and cannot have one, so it is argued rather than
/// demonstrated. If it ever becomes reachable in the field — a network redirector that opens a handle
/// and then refuses to describe it — **both callers now refuse**, which is the safe direction at each
/// of them. That sentence used to end "…and the walk's allow is backstopped by NT, which is the whole
/// reason the split is where it is"; the split is gone, for the reason given above.
#[cfg(windows)]
pub(crate) fn reparse_name_surrogate(file: &std::fs::File) -> Option<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FileAttributeTagInfo, GetFileInformationByHandleEx, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_ATTRIBUTE_TAG_INFO,
    };
    let size = u32::try_from(std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>()).ok()?;
    // SAFETY: the handle is borrowed from a live `File` that outlives this call; `info` is a
    // correctly-sized out-parameter matching the information class. Read-only query, nothing closed.
    unsafe {
        let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle() as isize),
            FileAttributeTagInfo,
            std::ptr::addr_of_mut!(info).cast(),
            size,
        )
        .ok()?;
        Some(
            info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
                && info.ReparseTag & IO_REPARSE_TAG_NAME_SURROGATE != 0,
        )
    }
}

/// Unix has no reparse points, so [`HandleFacts::is_reparse_point`] is always `false` there and no
/// caller reaches a decision that depends on this. Defined rather than `cfg`-ed away at every site.
#[cfg(not(windows))]
pub(crate) fn reparse_name_surrogate(_file: &std::fs::File) -> Option<bool> {
    Some(false)
}

/// Open for writing without following a link at the final component, reporting whether *we* created it.
/// `create_new` first so the create is atomic (`O_EXCL`/`CREATE_NEW`): either the name was free and is
/// now ours, or something was already there and we open that existing object explicitly.
///
/// `pub(crate)` since CPE-1846: `fsutil::copy_file_onto_no_follow` — the write half of snapshot restore
/// and checkpoint revert — needs step 2 of this module's four-step pattern (never follow a link at the
/// final component) without step 1's *refusal* of an existing name, because overwriting an existing file
/// is exactly what a restore means. Sharing this function rather than spelling the flags a second time is
/// the point: [`O_NOFOLLOW`] and [`FILE_FLAG_OPEN_REPARSE_POINT_U32`] are hard-coded per target here and
/// pinned by `secaudit_open_output_verified_refuses_a_symlink_final_component`, so a second copy of the
/// constants could drift out from under that test without anything failing.
pub(crate) fn open_no_follow(path: &std::path::Path) -> std::io::Result<(std::fs::File, bool)> {
    let mut create = std::fs::OpenOptions::new();
    create.write(true).create_new(true);
    let mut existing = std::fs::OpenOptions::new();
    existing.write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        create.custom_flags(O_NOFOLLOW);
        existing.custom_flags(O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        create.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT_U32);
        existing.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT_U32);
    }
    match create.open(path) {
        Ok(f) => Ok((f, true)),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok((existing.open(path)?, false)),
        Err(e) => Err(e),
    }
}

/// Open an **existing** file for writing without following a link at the final component (CPE-1672).
///
/// [`open_no_follow`]'s sibling for the one caller that must never create anything: the vault session
/// shredder, which overwrites files it has already enumerated. Creating a missing name there would be a
/// bug (it would write shred patterns into a file that was not in the tree), so this has no `create`
/// mode at all — a vanished name is an `Err`, never a fresh empty file. Same no-follow flags, so the
/// handle it returns is addressable by [`handle_facts`] on exactly the terms [`open_no_follow`]'s is.
pub(crate) fn open_existing_no_follow(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT_U32);
    }
    opts.open(path)
}

/// Open an **existing** file for READING without following a link at the final component (CPE-1896).
///
/// [`open_existing_no_follow`]'s read-only twin, and the difference is load-bearing rather than
/// stylistic. `backup::landed_inside` opens the file it has just written purely to read its identity
/// back off the handle, and a backup legitimately copies **read-only files** — `copy_file_onto_no_follow`
/// carries the source's permissions onto the destination, so asking for write access there would fail
/// with `PermissionDenied` on exactly the ordinary case and turn every read-only file in a backup into
/// a reported failure. Read access is all an identity probe needs.
///
/// No `create` mode at all: the caller is asking about something that must already exist, and
/// materialising an empty file at a name that vanished would answer the wrong question entirely. Same
/// no-follow flags as its siblings, so the handle is addressable by [`handle_facts`] on identical terms.
pub(crate) fn open_existing_no_follow_read(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT_U32);
    }
    opts.open(path)
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
/// How many candidate output names in a row may come back [`crate::fsutil::TargetSlot::Unknown`] before
/// [`plan`] gives up on the item (CPE-1705).
///
/// Mirrors `unique_target`'s `MAX_CONSECUTIVE_UNKNOWN_SLOTS` in `src-tauri`, and exists for the same
/// reason: once an unknown slot is treated as occupied, an unreadable output *directory* makes **every**
/// candidate unknown, and an unbounded search then never terminates. On the dead mount where that happens
/// each stat can block for seconds, so an unbounded loop is not merely slow, it is a hang. A run this long
/// cannot be a real name collision — it means the folder itself cannot be read.
const MAX_CONSECUTIVE_UNKNOWN_OUTPUT_SLOTS: usize = 8;

/// **Real-filesystem non-destructive guarantee (CPE-1623):** the collision-avoidance disambiguation below
/// used to only ever check against this batch's OWN inputs/outputs (`used`) — a computed name that
/// happened to already exist as some unrelated file never selected into the batch would silently
/// overwrite it, even in the supposedly-safe default mode. It now also treats a real existing file at the
/// candidate output as occupied, exactly like a collision with `used`, and renames past it the same way —
/// a single `Path::try_exists()` stat per item (not a `canonicalize`), so the common "the first candidate
/// name is free" case costs one cheap syscall, not a regression to the per-item cost this module's own
/// CPE-1613 fix eliminated. **CPE-1705** replaced that probe's original `Path::is_file()` with the shared
/// three-state [`crate::fsutil::classify_target_slot`]: a stat that merely *failed* is no longer read as
/// "this name is free", and a run of [`MAX_CONSECUTIVE_UNKNOWN_OUTPUT_SLOTS`] unreadable candidates
/// refuses the item rather than looping.
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
                Containment::Refused(why) => {
                    return Err(format!(
                        "refusing \"{input}\": the computed output \"{output}\" {why}"
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
                // a collision with this batch's own `used` set — see the fn doc. A single `try_exists`
                // stat, not a `canonicalize`, so this doesn't reintroduce the O(n²) cost CPE-1613 fixed.
                //
                // CPE-1705: this was `Path::is_file()`, which folds every stat failure into `false` and
                // so handed an unreadable candidate back as a free output name. This is the **planner**
                // feeding the executor CPE-1696 hardened, which means the collapse is not latent here:
                // the executor now refuses an output it cannot prove free, so a plan built on the false
                // premise fails at execution instead of at planning — the user gets a refusal about a
                // name they never chose. An unknown slot is skipped exactly like an occupied one; the
                // loop's job is to find *some* free name and it can simply try the next.
                //
                // **The bound is not optional here, and it is why this is not a one-line swap.** This
                // loop had no termination condition other than finding a free name, which was safe only
                // because the old `is_file()` answered `false` for an unreadable slot and so *always*
                // terminated on the first candidate. Skipping unknowns without a bound turns an
                // unreadable output directory — where EVERY candidate is `Unknown` — from a silent
                // overwrite into an infinite loop, i.e. trades data loss for a hang. `unique_target` hit
                // exactly this in CPE-1696 and bounded its run at 8. This site has somewhere better to
                // go than a fallback name: the planner returns `Result` per item, so it refuses to plan
                // an output in a directory it cannot see, and nothing is written at all.
                //
                // **`unknown_run` counts CONSECUTIVE unknowns, and the reset below is load-bearing.** The
                // bound means "the folder itself is unreadable"; a *run* is the only evidence of that.
                // Without the reset, unknowns scattered among real collisions accumulate across the whole
                // walk and eventually refuse a batch that was merely landing in a busy directory with one
                // unreadable file in it — a rare hang-guard turned into a routine false refusal.
                //
                // **There is exactly ONE reset, deliberately.** The first version had two — one for an
                // in-batch `used` collision, one for an on-disk `Occupied` — and the PR #893 review found
                // that breaking the `used` one redded nothing, because reaching it needs an in-batch
                // collision *interleaved between* unknowns, which no reasonable test constructs. Rather
                // than write a baroque test for a second copy of one decision, the two collision cases now
                // fold into a single `Taken` arm that the ordinary interleaved test does exercise. An arm
                // no test can reach is a liability whether or not it is correct today.
                let mut unknown_run = 0usize;
                loop {
                    enum Slot {
                        Free,
                        Taken,
                        Unknown,
                    }
                    let slot = if used.contains(&out_key) {
                        Slot::Taken // already claimed by this batch
                    } else {
                        // CPE-1769: was a bare `Path::try_exists()`, which FOLLOWS the final component —
                        // a dangling link at the candidate output resolves to "nothing there" and this
                        // loop, exactly like the `unique_target`/`resolve_conflict` siblings CPE-1715
                        // fixed, read that as Free and stopped advancing. `name_pick_slot_probe` is the
                        // shared fsutil helper built for precisely this shape: a name-picking loop that
                        // must *advance past* an occupied candidate, not refuse at it. A link (dangling
                        // or live) now folds into `Occupied` here, same as a real file, so the loop tries
                        // the next candidate instead of handing the executor a name a later
                        // `fs::copy`/`fs::write` would follow straight through.
                        match crate::fsutil::classify_target_slot(&crate::fsutil::name_pick_slot_probe(
                            std::path::Path::new(&output),
                        )) {
                            crate::fsutil::TargetSlot::Free => Slot::Free,
                            crate::fsutil::TargetSlot::Occupied => Slot::Taken, // a real file, or a link, on disk
                            crate::fsutil::TargetSlot::Unknown => Slot::Unknown,
                        }
                    };
                    match slot {
                        Slot::Free => break,
                        // The single reset: any *proven* collision, from either source, breaks the run.
                        Slot::Taken => unknown_run = 0,
                        Slot::Unknown => {
                            unknown_run += 1;
                            if unknown_run >= MAX_CONSECUTIVE_UNKNOWN_OUTPUT_SLOTS {
                                return Err(format!(
                                    "refusing to plan an output for \"{input}\": the output folder \
                                     \"{dir}\" could not be read — {MAX_CONSECUTIVE_UNKNOWN_OUTPUT_SLOTS} \
                                     candidate names in a row could not be checked, so no name can be \
                                     shown to be free. Nothing was planned or written; this is a \
                                     refusal to guess, not a detected collision"
                                ));
                            }
                        }
                    }
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

    /// **CPE-1857 — the three answers [`name_is_multiply_linked`] must give, pinned in one test that
    /// runs on every platform.** The middle row is the one that cost a red matrix: on Unix a directory
    /// has `nlink >= 2` by construction, on Windows it has 1, so a rule that forgot `!is_dir` was green
    /// locally on Windows and red on both Unix legs. Asserted here rather than only through the archive
    /// sink, so the next reader meets the platform asymmetry at the rule itself.
    #[test]
    fn cpe_1857_the_link_count_rule_answers_no_for_a_plain_file_and_for_a_directory() {
        let d = scratch("cpe1857-linkcount");
        let plain = d.path().join("plain.txt");
        std::fs::write(&plain, b"one name only").unwrap();
        let dir = d.path().join("a-directory");
        std::fs::create_dir_all(dir.join("with-a-child")).unwrap();

        assert!(!name_is_multiply_linked(&plain), "a file with one name is not multiply linked");
        assert!(
            !name_is_multiply_linked(&dir),
            "a DIRECTORY must answer no on every platform — on Unix its link count is >= 2 by \
             construction (its own `.` plus the parent's entry plus one per subdirectory), and a rule \
             that reads that as a hard link refuses every directory on Linux and macOS while passing a \
             Windows-only local run"
        );
        assert!(!name_is_multiply_linked(&d.path().join("nothing-here")), "an absent name answers no");

        let second = d.path().join("second-name.txt");
        if std::fs::hard_link(&plain, &second).is_err() {
            crate::skip_notice!(
                "SKIPPING the positive leg of cpe_1857_the_link_count_rule_answers_no_for_a_plain_file_\
                 and_for_a_directory: no hard-link support here. The negative legs above still ran."
            );
            return;
        }
        // Liveness: the two names really are one object, proved by writing through one and reading the
        // other — not by trusting `hard_link`'s `Ok`.
        std::fs::write(&second, b"written through the second name").unwrap();
        assert_eq!(
            std::fs::read(&plain).ok().as_deref(),
            Some(&b"written through the second name"[..]),
            "fixture is inert: the two names are not one object"
        );
        assert!(name_is_multiply_linked(&plain), "both names of one file must answer yes");
        assert!(name_is_multiply_linked(&second), "both names of one file must answer yes");
    }

    // ---- CPE-1705: the PLANNER must not read a refused stat as a free output name -------------------

    /// A single unreadable candidate must not be planned as a free output name.
    ///
    /// **This test was deleted once as "vacuous" and restored — the deletion was the error.** Under a
    /// target-only deny it did pass against the unfixed `Path::is_file()` probe, because `fs::metadata`
    /// falls back to `FindFirstFileW` and reads the entry out of the parent. `deny_stat_of` now also
    /// denies `(RD)` on the parent, which kills that fallback, so `is_file()` answers `false` on a file
    /// that is really there and the `assert_ne!` below fires exactly as it was always meant to. A test
    /// that looks vacuous can be a test whose *construction* is incomplete — check the mechanism before
    /// deleting the assertion.
    #[test]
    fn cpe_1705_plan_does_not_accept_an_output_name_it_cannot_stat() {
        use std::io::Write;
        #[cfg(not(windows))]
        {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1705] SKIPPED the plan() unreadable-candidate leg on this platform: the Unix deny \
                 mechanism chmods the PARENT directory, which would make every candidate in the batch \
                 unreadable rather than one. NOTHING in this test covered that route on this run; \
                 `cpe_1705_plan_still_renames_past_a_readable_existing_output` carries the honest case on \
                 every OS."
            );
        }
        #[cfg(windows)]
        {
            let d = scratch("cpe1705-plan-one-denied");
            let input = d.path().join("cat.jpg");
            std::fs::write(&input, b"input").unwrap();
            // **The unreadable slot has to be the SECOND candidate, not the first — measured.** `plan()`
            // runs `classify_output_containment` on the first computed output *before* the disambiguation
            // loop, and that CPE-1623 guard already refuses an output whose filesystem identity it cannot
            // read. Denying the first candidate makes `plan()` return a containment refusal and never
            // reach the loop under test, so the test would be measuring a different guard.
            std::fs::write(d.path().join("cat-800.jpg"), b"FIRST OCCUPANT").unwrap();
            let candidate = d.path().join("cat-800-2.jpg");
            std::fs::write(&candidate, b"VICTIM ORIGINAL").unwrap();

            struct Restore<'a>(&'a std::path::Path, &'a std::path::Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    crate::fsutil::undo_deny_stat_of(self.0, self.1);
                }
            }
            let _r = Restore(&candidate, d.path());

            if !crate::fsutil::deny_stat_of(&candidate) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1705] SKIPPED the plan() denied-candidate leg: could not deny stat of {} on \
                     this machine. NOTHING in this test covered that route on this run.",
                    candidate.display()
                );
                return;
            }

            let job = BatchJob::new(vec![MediaOp::Resize { max_px: 800 }]);
            let out = plan(&job, &v(&[&input.to_string_lossy()])).unwrap();
            assert_ne!(
                std::path::Path::new(&out[0].output),
                candidate,
                "an output candidate whose stat was refused must NOT be planned as free — the executor \
                 would then be asked to write over a file the planner never proved empty"
            );
            assert_eq!(
                std::path::Path::new(&out[0].output),
                d.path().join("cat-800-3.jpg"),
                "it must step PAST the unreadable candidate to the next name, not abort the item — an \
                 unknown is occupied, not fatal, at a site whose job is to find some free name"
            );
            crate::fsutil::undo_deny_stat_of(&candidate, d.path());
            assert_eq!(
                std::fs::read(&candidate).unwrap(),
                b"VICTIM ORIGINAL".to_vec(),
                "and the unreadable file's bytes must be untouched"
            );
        }
    }

    /// CPE-1696 hardened `execute_plan`; this is the `plan()` that feeds it. The probe was
    /// `Path::is_file()`, which folds every stat failure into `false`, so an output candidate whose stat
    /// was refused was accepted as free.
    ///
    /// # Why this asserts on the BOUND, having measured that the obvious assertion is vacuous
    ///
    /// The first version of this test denied one candidate and asserted the planner did not choose it.
    /// That passes against the **unfixed** code, measured: reverting this loop to `Path::is_file()` and
    /// re-running it recompiled and went green. `fs::metadata` — which `is_file()` is — opens with a
    /// desired-access mask of `0` and **no deny ACE refuses it**, so the old probe answers `true` for a
    /// denied file, calls it occupied, and advances exactly as the new one does. At a site whose only
    /// observable is *which name was chosen*, old and new are indistinguishable on any single candidate.
    ///
    /// They diverge past [`MAX_CONSECUTIVE_UNKNOWN_OUTPUT_SLOTS`]: the fixed loop counts consecutive
    /// unknowns and **refuses the item**; the old one keeps walking and returns a name. So that is what
    /// this asserts. It is also the assertion worth having, because the bound is the genuinely new and
    /// genuinely risky code — without it, treating unknown-as-occupied turns this loop, which previously
    /// always terminated on its first candidate, into a non-terminating one against an unreadable folder.
    ///
    /// Windows-only real-syscall leg (`deny_stat_of` denies the target itself there; the Unix mechanism
    /// denies the parent, which would break reading the inputs too).
    #[test]
    fn cpe_1705_plan_refuses_an_output_folder_it_cannot_read_instead_of_looping() {
        use std::io::Write;
        #[cfg(not(windows))]
        {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1705] SKIPPED the plan() unreadable-candidate leg on this platform: the Unix deny \
                 mechanism chmods the PARENT directory, which would make every candidate in the batch \
                 unreadable rather than one. NOTHING in this test covered that route on this run; \
                 `cpe_1705_plan_still_renames_past_a_readable_existing_output` carries the honest case on \
                 every OS."
            );
        }
        #[cfg(windows)]
        {
            let d = scratch("cpe1705-plan-denied");
            let input = d.path().join("cat.jpg");
            std::fs::write(&input, b"input").unwrap();
            // **The first candidate must stay READABLE — measured, and it changes the staging.** `plan()`
            // runs `classify_output_containment` on the first computed output *before* this loop, and
            // that CPE-1623 guard already refuses an output whose filesystem identity it cannot read.
            // Denying the first candidate makes `plan()` return a containment refusal and never reach the
            // loop under test, so the test would be asserting about a different guard entirely.
            std::fs::write(d.path().join("cat-800.jpg"), b"FIRST OCCUPANT").unwrap();

            // Then deny a run of MAX_CONSECUTIVE_UNKNOWN_OUTPUT_SLOTS candidates: `-2`, `-3`, … Each has
            // to be a real file for the ACE to attach to.
            let mut denied: Vec<std::path::PathBuf> = Vec::new();
            for n in 2..(2 + MAX_CONSECUTIVE_UNKNOWN_OUTPUT_SLOTS) {
                let p = d.path().join(format!("cat-800-{n}.jpg"));
                std::fs::write(&p, b"VICTIM ORIGINAL").unwrap();
                denied.push(p);
            }

            struct Restore<'a>(&'a [std::path::PathBuf], &'a std::path::Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    for p in self.0 {
                        crate::fsutil::undo_deny_stat_of(p, self.1);
                    }
                }
            }
            let _r = Restore(&denied, d.path());

            if !denied.iter().all(|p| crate::fsutil::deny_stat_of(p)) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1705] SKIPPED the plan() unreadable-folder leg: could not deny stat of the \
                     candidate outputs on this machine. NOTHING in this test covered that route on this \
                     run."
                );
                return;
            }

            let job = BatchJob::new(vec![MediaOp::Resize { max_px: 800 }]);
            // Pre-fix (`Path::is_file()`) this returned Ok with output `cat-800-10.jpg` — a name in a
            // folder the planner could not read, handed to an executor that will refuse it.
            let err = plan(&job, &v(&[&input.to_string_lossy()])).expect_err(
                "a run of unreadable output candidates must refuse the item, not keep walking",
            );
            assert!(
                err.contains("could not be read") && err.contains("refusal to guess"),
                "the refusal must name the uncertainty: {err}"
            );

            for p in &denied {
                crate::fsutil::undo_deny_stat_of(p, d.path());
                assert_eq!(std::fs::read(p).unwrap(), b"VICTIM ORIGINAL".to_vec(), "nothing may be touched");
            }
        }
    }

    /// **`unknown_run` must count CONSECUTIVE unknowns.** Interleave unreadable candidates with ordinary
    /// readable collisions so that the *total* number of unknowns exceeds
    /// [`MAX_CONSECUTIVE_UNKNOWN_OUTPUT_SLOTS`] but no run of them ever does: the planner must walk
    /// through and return a name, not refuse.
    ///
    /// Written because the PR #893 review broke the two `unknown_run = 0` resets individually and found
    /// **neither redded anything** — the bound tests only ever presented an uninterrupted run. Without the
    /// resets this is a real user-visible misbehaviour: a batch landing in a busy folder that happens to
    /// contain a few unreadable files gets refused outright, with a message blaming the whole folder.
    #[test]
    fn cpe_1705_scattered_unreadable_candidates_do_not_accumulate_into_a_refusal() {
        use std::io::Write;
        #[cfg(not(windows))]
        {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1705] SKIPPED the interleaved-unknowns leg on this platform: the Unix deny mechanism \
                 chmods the PARENT, making every candidate unreadable rather than alternating ones. \
                 NOTHING in this test covered the unknown_run reset on this run."
            );
        }
        #[cfg(windows)]
        {
            let d = scratch("cpe1705-plan-interleaved");
            let input = d.path().join("cat.jpg");
            std::fs::write(&input, b"input").unwrap();
            // First candidate readable-occupied so the CPE-1623 containment check passes (see the
            // sibling test), then alternate occupied / denied for well past the bound's worth of
            // unknowns. 2..22 gives 10 denied candidates — more than MAX_CONSECUTIVE_UNKNOWN_OUTPUT_SLOTS
            // in total, but never two in a row.
            std::fs::write(d.path().join("cat-800.jpg"), b"OCCUPIED").unwrap();
            let mut denied: Vec<std::path::PathBuf> = Vec::new();
            for n in 2..22u32 {
                let p = d.path().join(format!("cat-800-{n}.jpg"));
                std::fs::write(&p, b"OCCUPIED").unwrap();
                if n % 2 == 0 {
                    denied.push(p);
                }
            }

            struct Restore<'a>(&'a [std::path::PathBuf], &'a std::path::Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    for p in self.0 {
                        crate::fsutil::undo_deny_stat_of(p, self.1);
                    }
                }
            }
            let _r = Restore(&denied, d.path());

            if !denied.iter().all(|p| crate::fsutil::deny_stat_of(p)) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1705] SKIPPED the interleaved-unknowns leg: could not deny stat of the candidate \
                     outputs on this machine. NOTHING in this test covered the unknown_run reset on this \
                     run."
                );
                return;
            }

            let job = BatchJob::new(vec![MediaOp::Resize { max_px: 800 }]);
            let out = plan(&job, &v(&[&input.to_string_lossy()])).expect(
                "unknowns broken up by real collisions must NOT accumulate into a refusal — the bound \
                 means \"this folder is unreadable\", and a folder with readable files in it is not",
            );
            assert_eq!(
                std::path::Path::new(&out[0].output),
                d.path().join("cat-800-22.jpg"),
                "it must walk past every occupied and unreadable candidate to the first free name"
            );
        }
    }

    /// The ungated sibling on every OS: a *readable* existing output is still renamed past (the CPE-1623
    /// guarantee), and a free name is still used as-is. A planner that treated everything as occupied
    /// would silently rename every output.
    #[test]
    fn cpe_1705_plan_still_renames_past_a_readable_existing_output() {
        let d = scratch("cpe1705-plan-ok");
        let input = d.path().join("cat.jpg");
        std::fs::write(&input, b"input").unwrap();
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 800 }]);

        // Nothing in the way: the first candidate is used.
        let out = plan(&job, &v(&[&input.to_string_lossy()])).unwrap();
        assert_eq!(std::path::Path::new(&out[0].output), d.path().join("cat-800.jpg"));

        // Now occupy it with a readable file: the planner must step past to `-2`, not overwrite.
        std::fs::write(d.path().join("cat-800.jpg"), b"KEEP ME").unwrap();
        let out = plan(&job, &v(&[&input.to_string_lossy()])).unwrap();
        assert_eq!(std::path::Path::new(&out[0].output), d.path().join("cat-800-2.jpg"));
        assert_eq!(std::fs::read(d.path().join("cat-800.jpg")).unwrap(), b"KEEP ME".to_vec());
    }

    /// **CPE-1769.** This loop's own comment says it "Mirrors `unique_target`" — and it copied that
    /// helper's logic by hand instead of calling it, so it copied the pre-CPE-1715 bug too: a bare
    /// `Path::try_exists()` FOLLOWS the final path component, so a **dangling** link at the first
    /// candidate name resolves to "nothing there" and the loop stopped advancing, handing the executor
    /// the link's own name as the output. A later `fs::copy`/`fs::write` at that name does not follow the
    /// final component, so it writes through the link — potentially outside the destination folder.
    ///
    /// Staged with [`crate::fsutil::make_dangling_link`], which falls back to an NTFS junction on an
    /// unprivileged Windows runner (no `SeCreateSymbolicLinkPrivilege`), so this test covers both the
    /// symlink and junction legs without branching. Cleanup is `d`'s own `TempDir` drop, armed before any
    /// assertion runs — no trailing `remove_dir_all`.
    #[test]
    fn cpe_1769_plan_steps_past_a_dangling_link_at_the_candidate_output_instead_of_reusing_it() {
        let d = scratch("cpe1769-plan-dangling-link");
        let input = d.path().join("cat.jpg");
        std::fs::write(&input, b"input").unwrap();
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 800 }]);

        let first_candidate = d.path().join("cat-800.jpg");
        if !crate::fsutil::make_dangling_link(&first_candidate) {
            let _ = std::io::Write::write_all(
                &mut std::io::stderr(),
                b"[CPE-1769] SKIPPED plan()'s dangling-link leg: this machine could not stage a link \
                  or junction at all (no privilege and junction creation also failed).\n",
            );
            return;
        }

        let out = plan(&job, &v(&[&input.to_string_lossy()])).unwrap();

        // THE HARM, on the filesystem, before trusting the planned `output` string: the link at the
        // first candidate must still be exactly the untouched link it was — `plan()` never writes bytes
        // itself, but if it had handed this name back as the chosen output, the executor's later write
        // would have destroyed or written through it.
        assert!(
            std::fs::symlink_metadata(&first_candidate).is_ok_and(|m| m.file_type().is_symlink()),
            "the dangling link at the first candidate name must survive planning completely untouched"
        );
        assert_ne!(
            std::path::Path::new(&out[0].output),
            first_candidate.as_path(),
            "plan() must not hand back a name occupied by a dangling link as though it were free — that \
             is the write-through this ticket exists to close"
        );
        assert_eq!(
            std::path::Path::new(&out[0].output),
            d.path().join("cat-800-2.jpg"),
            "the loop must advance to the next candidate exactly as it does for a real occupied file"
        );
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
            crate::skip_notice!(
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
            crate::skip_notice!("skipping live-symlink containment test: could not create a symlink in this environment");
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
            crate::skip_notice!("skipping same-directory symlink test: could not create a symlink in this environment");
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
                crate::skip_notice!(
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

    /// **The belt-and-braces length guard in `classify_open_failure` must be pinned by a test of its
    /// own.** Layer 1 (`verbatim_wide`) normally keeps the probe from ever seeing a truncation error, so
    /// every filesystem-level test reaches this function through a path where the guard is inert — delete
    /// the guard and the whole suite still passes. That is not acceptable here: an independent review
    /// demonstrated that with layer 1 disabled, this guard is the only thing standing between a
    /// past-`MAX_PATH` symlink alias and the victim's bytes (it degrades the verdict to `Unverifiable`,
    /// which every caller refuses on). This ticket exists because prior rounds each eroded a guard no test
    /// was holding, so pin it directly: the function is pure, so no filesystem is needed.
    #[cfg(windows)]
    #[test]
    fn cpe_1642_classify_open_failure_length_guard_is_pinned() {
        const ERROR_FILE_NOT_FOUND: u32 = 2;
        const ERROR_PATH_NOT_FOUND: u32 = 3;
        const ERROR_ACCESS_DENIED: u32 = 5;
        const ERROR_INVALID_NAME: u32 = 123;
        const ERROR_BAD_PATHNAME: u32 = 161;
        const ERROR_FILENAME_EXCED_RANGE: u32 = 206;
        /// One wide char past the unprefixed limit, i.e. a length only a verbatim path can address.
        const LONG: usize = 300;
        /// Comfortably inside the unprefixed limit.
        const SHORT: usize = 100;

        // The four codes that mean "this path was too long to even parse" must NOT be read as `Absent`
        // once the path is at or past the limit — reading them as `Absent` is precisely the fail-open the
        // reviewer exploited (probe says "nothing there", `std::fs` writes anyway).
        for code in [
            ERROR_PATH_NOT_FOUND,
            ERROR_INVALID_NAME,
            ERROR_BAD_PATHNAME,
            ERROR_FILENAME_EXCED_RANGE,
        ] {
            assert!(
                matches!(classify_open_failure(code, LONG), Probe::Unreadable(_)),
                "os error {code} on a past-MAX_PATH path must fail CLOSED as Unreadable, never Absent"
            );
        }

        // Same codes below the limit keep their ordinary meaning: there, `ERROR_PATH_NOT_FOUND` really
        // does mean a missing parent, so treating it as `Absent` is correct and costs nothing.
        assert!(
            matches!(classify_open_failure(ERROR_PATH_NOT_FOUND, SHORT), Probe::Absent),
            "a short path's ERROR_PATH_NOT_FOUND genuinely means 'nothing there'"
        );

        // The deliberate exclusion: `ERROR_FILE_NOT_FOUND` stays `Absent` at ANY length. This is the
        // ordinary "the output doesn't exist yet" case — the guard must not make every deep-folder batch
        // unverifiable. The exclusion is safe because with layer 1 in place the path is never truncated,
        // so a `2` really does mean the leaf is absent.
        for len in [SHORT, LONG] {
            assert!(
                matches!(classify_open_failure(ERROR_FILE_NOT_FOUND, len), Probe::Absent),
                "ERROR_FILE_NOT_FOUND must stay Absent at length {len} — this is the common legitimate path"
            );
        }

        // The boundary itself, from both sides, so a future off-by-one in the comparison is caught.
        assert!(
            matches!(classify_open_failure(ERROR_PATH_NOT_FOUND, 259), Probe::Absent),
            "259 wide chars is still addressable unprefixed"
        );
        assert!(
            matches!(classify_open_failure(ERROR_PATH_NOT_FOUND, 260), Probe::Unreadable(_)),
            "260 wide chars (NUL included) is the limit — at it, truncation is possible"
        );

        // An unrelated failure is unreadable regardless of length: something is there, we just could not
        // identify it. Never `Absent`.
        for len in [SHORT, LONG] {
            assert!(
                matches!(classify_open_failure(ERROR_ACCESS_DENIED, len), Probe::Unreadable(_)),
                "a permissions failure is Unreadable, not Absent"
            );
        }
    }

    /// **An over-`MAX_PATH` output that is an ordinary, absent file must still be ALLOWED.** Without this,
    /// the test above could pass by refusing every long path, and every legitimate deep-folder batch would
    /// break.
    ///
    /// Note this reaches `classify_open_failure` only through layer 1 (`verbatim_wide`), so it exercises
    /// the `ERROR_FILE_NOT_FOUND`-stays-`Absent` half and *not* the length guard itself — the guard is
    /// pinned directly by `cpe_1642_classify_open_failure_length_guard_is_pinned` below.
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
                matches!(facts_or_unreadable(facts(bad)), Probe::Unreadable(_)),
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
        // `:` moved to `cpe_1640_the_colon_rule_is_windows_only` — it is a Windows rule (drive-relative
        // reference + NTFS alternate-data-stream separator) and an ordinary filename character elsewhere,
        // so it cannot be asserted uniformly here. Still rejected on Windows; still checked, per-platform.
        if colon_is_a_path_character() {
            reject("C:foo"); // reviewer finding A: drive-relative reference
            reject("secrets:hidden"); // colon anywhere, not just drive-letter position
        }

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
        // "C:foo" lives in `cpe_1640_convert_extension_colon_rule_is_windows_only_too` — the colon half
        // of this rule is Windows-only (CPE-1640), so it can't be asserted uniformly in this list.
        for bad in ["../evil", "..\\evil", "sub/ext", "..", "a/.."] {
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
        //
        // **Windows-only as of CPE-1640.** Drive-relative syntax is a Windows concept: off Windows
        // `C:foo.jpg` is simply a file called `C:foo.jpg` in the current directory — contained, and
        // refusing it was the false positive that ticket is about. The structural backstop is gated to
        // match, so this asserts the platform-appropriate answer on both sides rather than skipping.
        let job = BatchJob::new(vec![MediaOp::Rename { template: "C:foo".into() }]);
        let planned = plan(&job, &v(&["innocuous.jpg"]));
        if colon_is_a_path_character() {
            let err = planned
                .expect_err("a drive-relative output for a bare-filename input must be refused, not planned");
            assert!(err.contains("folder") || err.contains("alternate data stream"), "refusal reason: {err}");
        } else {
            let out = planned.expect("off Windows this is an ordinary filename in the current directory");
            assert_eq!(out[0].output, "C:foo.jpg");
        }
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

    // ---- CPE-1640: the colon rule is Windows logic, and was being applied everywhere -----------------

    /// A colon is reserved on NTFS but is an **ordinary filename character** on Linux and macOS, so a
    /// perfectly reasonable template (`10:30am-photo`, `session:final`) was refused off Windows for a
    /// reason that does not exist there. CI could not see it: the rule was *consistently* wrong, and a
    /// 3-OS matrix only detects *inconsistency* — so this test asserts the platform-appropriate answer
    /// per leg rather than one answer everywhere.
    #[test]
    fn cpe_1640_the_colon_rule_is_windows_only() {
        for t in ["10:30am-photo", "session:final", "C:foo", "secrets:hidden"] {
            let job = BatchJob::new(vec![MediaOp::Rename { template: t.into() }]);
            if colon_is_a_path_character() {
                assert!(
                    validate(&job).is_err(),
                    "{t:?} must still be refused on Windows: `:` is the drive separator AND the NTFS \
                     alternate-data-stream separator"
                );
            } else {
                assert!(
                    validate(&job).is_ok(),
                    "{t:?} must be ACCEPTED off Windows, where `:` is an ordinary filename character"
                );
            }
            // Separators are universal and stay refused on every platform.
            let sep = BatchJob::new(vec![MediaOp::Rename { template: format!("{t}/x") }]);
            assert!(validate(&sep).is_err(), "a path separator must be refused on every platform");
        }
    }

    /// The same gate on the Convert-extension half of `validate()` (they share one helper, so this pins
    /// that they cannot drift apart).
    #[test]
    fn cpe_1640_convert_extension_colon_rule_is_windows_only_too() {
        let job = BatchJob::new(vec![MediaOp::Convert { to_ext: "j:pg".into() }]);
        assert_eq!(
            validate(&job).is_err(),
            colon_is_a_path_character(),
            "the Convert extension check must use the same platform rule as the Rename template check"
        );
    }

    /// The acceptance criterion the ticket calls out explicitly: relaxing the *field-level* colon check
    /// off Windows must not relax **containment**, which is the actual guarantee and is unconditional.
    /// A separator-bearing template still escapes on every platform, and a colon-bearing template plans
    /// to a path that provably stays in the input's own folder off Windows.
    #[test]
    fn cpe_1640_containment_is_unaffected_by_the_relaxed_colon_rule() {
        // Genuine traversal: refused on every platform, at plan()'s own backstop (validate() bypassed).
        let escaping = BatchJob::new(vec![MediaOp::Rename { template: "../evil".into() }]);
        assert!(
            plan(&escaping, &v(&["/pics/vacation/photo1.jpg"])).is_err(),
            "a traversal template must be refused on EVERY platform, colon rule or not"
        );

        let colon = BatchJob::new(vec![MediaOp::Rename { template: "10:30am".into() }]);
        let planned = plan(&colon, &v(&["/pics/vacation/photo1.jpg"]));
        if colon_is_a_path_character() {
            assert!(planned.is_err(), "on Windows the colon output is an alternate data stream — refused");
        } else {
            let out = planned.expect("off Windows a colon is an ordinary filename character");
            assert_eq!(
                out[0].output, "/pics/vacation/10:30am.jpg",
                "the output must be a single component inside the input's own folder"
            );
        }
    }

    // ---- CPE-1624 finding B: NTFS alternate data streams ---------------------------------------------

    /// The pure decision table. `:` in a FINAL path component is an alternate-data-stream separator on
    /// Windows and an ordinary character everywhere else, so this asserts per-leg.
    #[test]
    fn cpe_1624_alternate_stream_detection_is_windows_only_and_looks_at_the_final_component() {
        assert_eq!(final_component_names_alternate_stream("C:foo.png"), colon_is_a_path_character());
        assert_eq!(final_component_names_alternate_stream("IMG_1.JPG:hidden"), colon_is_a_path_character());
        assert_eq!(final_component_names_alternate_stream("secrets:hidden"), colon_is_a_path_character());
        // An ordinary final component is never a stream, on any platform — including the one a rooted
        // path leaves behind once its drive prefix has been split off.
        assert!(!final_component_names_alternate_stream("photo.jpg"));
        assert!(!final_component_names_alternate_stream("shot..final.jpg"));
    }

    /// `strip_stream_suffix` is what makes `X.JPG` and `X.JPG:hidden` key to ONE identity. Two shapes must
    /// survive untouched even on Windows: a drive-relative `C:foo` (a different file from `C`, so fusing
    /// them would be the fail-OPEN direction) and a leading-colon name with no base to attribute it to.
    #[test]
    fn cpe_1624_stream_suffix_stripping_is_windows_only_and_spares_drive_relative_paths() {
        if colon_is_a_path_character() {
            assert_eq!(strip_stream_suffix(r"C:\pics\IMG_1.JPG:hidden"), r"C:\pics\IMG_1.JPG");
            assert_eq!(strip_stream_suffix("/pics/IMG_1.JPG:hidden"), "/pics/IMG_1.JPG");
            assert_eq!(strip_stream_suffix(r"sub\C:foo.png"), r"sub\C");
        } else {
            // Off Windows a colon is part of the real filename — collapsing it would fuse two distinct
            // files into one identity, which is exactly the CPE-1640 class of false positive.
            assert_eq!(strip_stream_suffix("/pics/IMG_1.JPG:hidden"), "/pics/IMG_1.JPG:hidden");
        }
        // Never stripped anywhere: a drive-relative reference, and a name that is only a stream.
        assert_eq!(strip_stream_suffix("C:foo"), "C:foo");
        assert_eq!(strip_stream_suffix(":stream"), ":stream");
        assert_eq!(strip_stream_suffix("/pics/IMG_1.JPG"), "/pics/IMG_1.JPG");
    }

    /// **The ticket's measured finding, on a real file:** `same_file("…\IMG_1.JPG", "…\IMG_1.JPG:hidden")`
    /// returned `false` — the two paths are the same MFT record, so every "would this write touch that
    /// file?" question had been answering no. Off Windows the same pair is genuinely two different files
    /// and must stay distinct (CPE-1640's rule, applied to identity rather than to templates).
    #[test]
    fn cpe_1624_same_file_recognises_an_alternate_data_stream_as_the_same_file() {
        let dir = scratch("cpe1624-ads-same-file");
        let base = dir.path().join("IMG_1.JPG");
        std::fs::write(&base, b"real photo bytes").unwrap();
        let base_s = base.to_string_lossy().to_string();
        let stream_s = format!("{base_s}:hidden");

        assert_eq!(
            same_file(&base_s, &stream_s),
            colon_is_a_path_character(),
            "on Windows an ADS path names the same underlying file; elsewhere ':' is just a filename \
             character and the two are distinct files"
        );
        // Symmetric, and never accidentally equal to an unrelated neighbour.
        assert_eq!(same_file(&stream_s, &base_s), colon_is_a_path_character());
        let other = dir.path().join("IMG_2.JPG").to_string_lossy().to_string();
        assert!(!same_file(&stream_s, &other), "unrelated files must never fuse");

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// The engine boundary, not just the template field: an output whose final component carries a colon
    /// is refused by the ONE check both `plan()` and `execute_plan_walk` call — so it also covers a
    /// hand-built `PlannedItem` that never went through `plan()`, which no template-level rule can reach.
    /// The refusal must say what is TRUE (CPE-1642's rule): the stream stays *inside* the folder, so
    /// calling it an escape would be a lie.
    #[test]
    fn cpe_1624_an_alternate_data_stream_output_is_refused_at_the_shared_engine_boundary() {
        let mut cache = ParentCache::new();
        let verdict = classify_output_containment(
            "/pics/workdir/photo.png",
            "/pics/workdir/C:foo.png",
            &mut cache,
        );
        if colon_is_a_path_character() {
            assert_eq!(
                verdict,
                Containment::Refused(WHY_ALTERNATE_STREAM),
                "an ADS output must be refused outright — and NOT as an escape: the bytes stay in the \
                 folder, they just land hidden on an unrelated file"
            );
        } else {
            assert_eq!(
                verdict,
                Containment::Inside,
                "off Windows this is an ordinary filename inside the input's own folder"
            );
        }
    }

    // ---- CPE-1652 finding A: name-surrogate reparse tags --------------------------------------------

    /// The classification table `std`'s `is_symlink()` used to stand in for. Pure `u32` logic, so all
    /// three CI legs run it even though only Windows ever calls it in production.
    ///
    /// **Injected tags, not planted files** — the shapes that matter (a cloud placeholder, a dedup stub,
    /// a WCI/appexeclink surrogate) cannot be created on a CI machine or on this developer's box, so the
    /// tag values are asserted directly. **Not exercised against a real on-disk object:** every
    /// non-symlink, non-junction tag below. The two that *can* be planted (symlink, junction) are covered
    /// by `cpe_1652_the_probe_never_calls_a_redirecting_reparse_point_an_ordinary_file`.
    #[test]
    fn cpe_1652_reparse_tag_classification_fails_closed_on_an_unknown_name_surrogate() {
        const ATTR_REPARSE: u32 = 0x0000_0400;
        const ATTR_NORMAL: u32 = 0x0000_0080;

        // Not a reparse point at all — the tag field is meaningless and must be ignored.
        assert_eq!(classify_reparse_tag(ATTR_NORMAL, 0xA000_000C), ReparseKind::NotReparse);
        assert_eq!(classify_reparse_tag(0, 0), ReparseKind::NotReparse);

        // The two `std` knows: IO_REPARSE_TAG_SYMLINK and IO_REPARSE_TAG_MOUNT_POINT (a junction).
        assert_eq!(classify_reparse_tag(ATTR_REPARSE, 0xA000_000C), ReparseKind::Link);
        assert_eq!(classify_reparse_tag(ATTR_REPARSE, 0xA000_0003), ReparseKind::Link);

        // Non-surrogate tags: a write lands on THIS file, so they are ordinary files — the cloud
        // placeholder / dedup-stub case the old doc comment cited, which is genuinely fine.
        for ordinary in [
            0x9000_001A_u32, // IO_REPARSE_TAG_CLOUD (OneDrive placeholder)
            0x8000_0017,     // IO_REPARSE_TAG_WOF (compressed/WIM-backed file)
            0x8000_0013,     // IO_REPARSE_TAG_DEDUP
        ] {
            assert_eq!(
                classify_reparse_tag(ATTR_REPARSE, ordinary),
                ReparseKind::OpaqueData,
                "tag {ordinary:#x} has no name-surrogate bit — refusing it would strand ordinary \
                 batches in OneDrive-backed folders"
            );
        }

        // Name-surrogate tags this code does not know: the write WILL be redirected somewhere the probe
        // cannot predict, so it must fail closed rather than describe the stub as an ordinary file.
        for surrogate in [
            0xA000_0019_u32, // IO_REPARSE_TAG_GLOBAL_REPARSE
            0xA000_0027,     // IO_REPARSE_TAG_WCI
            0xA000_0030,     // vendor tag, surrogate bit set
            0x2000_0000,     // the bare bit, no other structure
        ] {
            assert_eq!(
                classify_reparse_tag(ATTR_REPARSE, surrogate),
                ReparseKind::UnknownSurrogate,
                "tag {surrogate:#x} sets the name-surrogate bit and must fail closed"
            );
        }

        // Property form: the ONLY surrogate tags allowed through as links are the two known ones.
        for high in 0u32..=0xFF {
            let tag = 0x2000_0000 | (high << 8);
            let kind = classify_reparse_tag(ATTR_REPARSE, tag);
            assert!(
                matches!(kind, ReparseKind::UnknownSurrogate | ReparseKind::Link),
                "a name-surrogate tag must never be classified as ordinary data: {tag:#x} -> {kind:?}"
            );
        }
    }

    /// The AC's "the probe and the writer provably agree" for every reparse shape this suite can build.
    /// The writer is plain `std::fs::write`, which does **not** pass `FILE_FLAG_OPEN_REPARSE_POINT` and
    /// therefore follows any redirecting reparse point; the probe must never describe such a path as an
    /// ordinary file, or the two are looking at different objects.
    #[test]
    fn cpe_1652_the_probe_never_calls_a_redirecting_reparse_point_an_ordinary_file() {
        let dir = scratch("cpe1652-probe-agrees");
        let target = dir.path().join("target.jpg");
        std::fs::write(&target, b"target bytes").unwrap();

        // An ordinary file is Real — the negative control (without it, "everything is a Link" would pass).
        assert!(
            matches!(probe_no_follow(&target), Probe::Real(_)),
            "an ordinary file must probe as a real file, or every batch on this machine would be refused"
        );

        let link = dir.path().join("link.jpg");
        if try_symlink(&target, &link, "cpe_1652_the_probe_never_calls_a_redirecting_reparse_point...") {
            assert!(
                matches!(probe_no_follow(&link), Probe::Link),
                "a symlink redirects the writer, so the probe must report a link, not the stub"
            );
        }

        // A junction (IO_REPARSE_TAG_MOUNT_POINT) needs no elevation on Windows, unlike a symlink.
        #[cfg(windows)]
        {
            let real_dir = dir.path().join("real_dir");
            std::fs::create_dir_all(&real_dir).unwrap();
            let junction = dir.path().join("junction");
            if junction::create(&real_dir, &junction).is_ok() {
                assert!(
                    matches!(probe_no_follow(&junction), Probe::Link),
                    "a junction redirects the writer, so the probe must report a link"
                );
            }
        }

        let _ = std::fs::remove_dir_all(dir.path());
    }

    // ---- CPE-1652 finding B: the link-census cost cliff ---------------------------------------------

    /// The cap must **fail closed**: past the bound the verdict degrades to `Unverifiable` (which every
    /// caller refuses on), never to "allowed because we stopped looking". The positive control in the
    /// same test is what makes that meaningful — with the ordinary cap the identical folder is allowed.
    #[test]
    fn cpe_1652_the_census_cap_fails_closed_instead_of_scanning_an_unbounded_folder() {
        let dir = scratch("cpe1652-census-cap");
        let selected = dir.path().join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let a = selected.join("a.jpg");
        std::fs::write(&a, b"shared bytes").unwrap();
        // A second NAME for the same data, inside the selected folder: multiply-linked, which is the only
        // shape that reaches the census at all.
        let b = selected.join("b.jpg");
        if std::fs::hard_link(&a, &b).is_err() {
            crate::skip_notice!(
                "SKIPPING cpe_1652_the_census_cap_fails_closed: this filesystem does not support hard \
                 links — this test verified NOTHING"
            );
            let _ = std::fs::remove_dir_all(dir.path());
            return;
        }
        for i in 0..6 {
            std::fs::write(selected.join(format!("filler{i}.jpg")), b"filler").unwrap();
        }
        let input = selected.join("photo.jpg").to_string_lossy().to_string();
        let output = b.to_string_lossy().to_string();

        // Positive control: with the ordinary cap the census completes, finds both names inside the
        // folder, and correctly ALLOWS the write (the CPE-1642 false positive that was retired).
        set_census_cap_for_test(None);
        let mut cache = ParentCache::new();
        assert_eq!(
            classify_output_containment(&input, &output, &mut cache),
            Containment::Inside,
            "an uncapped census must still allow a file whose every name is inside the selected folder"
        );

        // Capped below the folder's entry count: the shortfall may be an artefact of stopping early, so
        // the verdict degrades to a refusal rather than either a false escape or an unbounded scan.
        set_census_cap_for_test(Some(1));
        let mut capped_cache = ParentCache::new();
        let verdict = classify_output_containment(&input, &output, &mut capped_cache);
        set_census_cap_for_test(None);
        assert_eq!(
            verdict,
            Containment::Unverifiable(WHY_CENSUS_TOO_BIG),
            "past the cap the census must fail CLOSED with its own accurate reason"
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// Manual-only measurement for the ticket's "state a number for the large-folder census case"
    /// (`cargo test --release -- --ignored --nocapture cpe_1652_census_timing_for_a_large_folder`).
    /// `#[ignore]`d like the CPE-1623 plan-timing test: wall-clock assertions are flaky in CI.
    #[test]
    #[ignore]
    fn cpe_1652_census_timing_for_a_large_folder() {
        let dir = scratch("cpe1652-census-timing");
        let selected = dir.path().join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let n = 20_000usize;
        for i in 0..n {
            std::fs::write(selected.join(format!("f{i:06}.jpg")), b"x").unwrap();
        }
        let start = std::time::Instant::now();
        let scan = scan_dir_link_census(&selected).expect("the selected folder must enumerate");
        let elapsed = start.elapsed();
        println!(
            "census of {n} entries took {elapsed:?} (capped={}, distinct identities={})",
            scan.capped,
            scan.counts.len()
        );

        set_census_cap_for_test(Some(500));
        let start = std::time::Instant::now();
        let capped = scan_dir_link_census(&selected).expect("the selected folder must enumerate");
        let capped_elapsed = start.elapsed();
        set_census_cap_for_test(None);
        println!("same folder with the cap at 500 took {capped_elapsed:?} (capped={})", capped.capped);

        let _ = std::fs::remove_dir_all(dir.path());
    }

    // ==== SECURITY AUDIT (PR #848) — attack attempts ==================================================

    fn probe_label(p: &Probe) -> String {
        match p {
            Probe::Absent => "Absent".into(),
            Probe::Link => "Link".into(),
            Probe::Real(f) => format!("Real(links={}, is_dir={})", f.links, f.is_dir),
            Probe::Unreadable(w) => format!("Unreadable({w})"),
        }
    }

    /// **The no-follow open, which is what makes the link-swap class structurally impossible** (security
    /// audit finding 1). A symlink at the output's final component must never be written through, however
    /// it got there — and this is also the runtime assertion that the hard-coded [`O_NOFOLLOW`] value is
    /// right for this target: were it wrong, the open would quietly follow the link and the target's bytes
    /// would change, failing here rather than shipping.
    #[test]
    fn secaudit_open_output_verified_refuses_a_symlink_final_component() {
        let dir = scratch("secaudit-nofollow");
        let selected = dir.path().join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let input = selected.join("photo.png");
        std::fs::write(&input, b"input bytes").unwrap();
        let target = selected.join("target.png");
        let target_bytes = b"the link target's own bytes".to_vec();
        std::fs::write(&target, &target_bytes).unwrap();

        let link = selected.join("out.png");
        if !try_symlink(&target, &link, "secaudit_open_output_verified_refuses_a_symlink_final_component") {
            let _ = std::fs::remove_dir_all(dir.path());
            return;
        }

        let err = open_output_verified(&input.to_string_lossy(), &link.to_string_lossy())
            .err()
            .expect("a symlink at the output must be refused, never followed");
        assert!(err.contains("link"), "the refusal must name the reason: {err}");
        assert_eq!(
            std::fs::read(&target).unwrap(),
            target_bytes,
            "the link's TARGET must be byte-for-byte untouched — if this fails, O_NOFOLLOW did not take \
             effect on this platform and the open followed the link"
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// **CPE-1667.** The belt-and-braces symlink refusal covers the attack case fine ("…is a link, and a
    /// batch never writes through one — a link's target can be re-pointed after any check"), but says
    /// nothing to a DIFFERENT, non-adversarial user: someone who left behind a **dangling** symlink that
    /// happens to point back inside their OWN selected folder. That case used to be allowed (nothing
    /// existed at the name, so `create_new` just created a fresh file there) and is now refused along with
    /// every other link — correctly, but silently, as if it were the same attack. The message must name
    /// that case, not just recite reasoning that doesn't apply to it.
    ///
    /// `#[cfg(windows)]`: `O_NOFOLLOW` on Unix makes the *open itself* fail on any symlink final
    /// component — dangling or not — so this message is only reachable in practice on Windows, where
    /// `FILE_FLAG_OPEN_REPARSE_POINT` opens the reparse point object itself rather than erroring.
    ///
    /// **Which check produces it changed with CPE-1929.** It used to come from the
    /// `symlink_metadata(output).is_symlink()` path check; that check now sits *below* the handle's
    /// `is_reparse_point` refusal, which is what answers here (every symlink carries the reparse
    /// attribute). The wording moved with the decision, so this assertion still pins the same
    /// user-facing promise — it is now pinned on the answer that cannot be substituted after the open.
    #[cfg(windows)]
    #[test]
    fn cpe_1667_the_symlink_refusal_names_the_dangling_inside_the_folder_case() {
        let dir = scratch("cpe1667-symlink-message");
        let selected = dir.path().join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let input = selected.join("photo.png");
        std::fs::write(&input, b"input bytes").unwrap();

        // A dangling link whose target path is back INSIDE this same folder — the harmless, user-caused
        // case the message must acknowledge, not the attack case above.
        let dangling_target = selected.join("does-not-exist.png");
        let link = selected.join("out.png");
        if !try_symlink(
            &dangling_target,
            &link,
            "cpe_1667_the_symlink_refusal_names_the_dangling_inside_the_folder_case",
        ) {
            let _ = std::fs::remove_dir_all(dir.path());
            return;
        }

        let err = open_output_verified(&input.to_string_lossy(), &link.to_string_lossy())
            .err()
            .expect("a dangling symlink at the output must still be refused, never created through");
        assert!(
            err.contains("dangling"),
            "the refusal must name the dangling-inside-the-folder case, not just recite the attack \
             reasoning: {err}"
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// **CPE-1959 — the doctrine test, replacing CPE-1929's
    /// `..._a_non_surrogate_reparse_point_at_the_output_is_refused_by_the_handle_check`.** That test
    /// pinned the opposite verdict (a bare-bit refusal of every reparse point). It is replaced rather
    /// than loosened: the two halves below pin the *new* verdict exactly as hard, and the surrogate half
    /// still carries CPE-1929's whole reason for existing — it is the fixture that proves the handle
    /// check, not the `symlink_metadata` path check, is the decider.
    ///
    /// Halves 1 and 2 are the two-halves shape `fsutil`'s
    /// `cpe_1929_overwrite_confirmed_refuses_a_surrogate_but_writes_a_non_surrogate_reparse_point`
    /// already uses, pointed at this engine: the two outputs differ in **exactly one bit**
    /// (`0x2000_1959` is `0x0000_1959` with `IO_REPARSE_TAG_NAME_SURROGATE` set), which is the only
    /// arrangement that lets the test claim the **tag** is what decides rather than some other
    /// difference between the fixtures.
    ///
    /// **There are two more halves, and the reason for each is a measurement, not symmetry.** Round 1
    /// of this test used the surrogate GUID half to pin the handle check and it passed — *and went on
    /// passing with the handle refusal disabled outright*, the CPE-1929 tell. An unrecognised surrogate
    /// tag is classified `ReparseKind::UnknownSurrogate` by the by-path `classify_reparse_tag` and
    /// refused as `WHY_SURROGATE_TAG` **before the open**, and that constant's own wording contains the
    /// phrase the assertion was reading. So the GUID pair cannot reach the narrowed handle check at
    /// all.
    ///
    /// - **Half 3** is the one fixture that reaches it unaided: a tag `classify_reparse_tag`
    ///   recognises and passes through as `Probe::Link` — a real symlink. It is what makes the disable
    ///   sabotage red.
    /// - **Half 4** covers what half 3 cannot. Half 3 only proves the check fires for a *recognised*
    ///   tag; every other surrogate is answered by `WHY_SURROGATE_TAG` first, so the handle check's
    ///   coverage of the **non-symlink surrogate class** was shadowed in CPE-1929's exact sense. Round 2
    ///   stopped documenting that gap and closed it with the seam the crate already owned:
    ///   `surrogate_between_containment_and_open_for_test` plants the tag in the window *after* the path
    ///   probe has passed the name as an ordinary file, so the handle check is the **first** thing that
    ///   can see it.
    ///
    /// **Round 2 wrote "sole decider" there and round 3 measured that false** — worth keeping, because
    /// it is the same class of claim this whole ticket exists to remove. `std`'s `is_symlink` reads the
    /// same name-surrogate bit off a path that `reparse_name_surrogate` reads off a handle, so the
    /// `symlink_metadata` net below the handle check reaches half 4's fixture as well; with the handle
    /// check disabled, that net is what answers. Half 4 pins **ordering plus wording**, not visibility.
    /// The guard's non-redundancy rests on something better anyway: it reads the OBJECT, so a tag
    /// removed after the open defeats the path net and not this one.
    ///
    /// Two legs, two different claims: half 3 says the guard fires, half 4 says it fires **first** for a
    /// class no other fixture can deliver to it.
    ///
    /// **Every verdict is read off the filesystem, never off a `Result`** — CPE-1957's lesson is that a
    /// silent skip returns `Ok` too, and here the mirror-image trap is real: `open_output_verified`
    /// returns a `VerifiedOutput` that has not written anything yet, so an `Ok` from it proves nothing
    /// about bytes. The non-surrogate half therefore drives the whole write through `write_all` and then
    /// reads the file back; the surrogate half reads the untouched bytes back and re-checks the reparse
    /// attribute is still there, so "refused" means "the object on disk is as it was", not "an `Err` was
    /// returned".
    ///
    /// The `!is_symlink()` assertion on each half is not decoration — it is what stops the path check
    /// below the handle check from answering for free and making either half prove nothing.
    /// `make_guid_reparse_point` needs no privilege and no filter driver (see its doc), so this cannot
    /// silently degrade into a skip on a normal runner.
    ///
    /// Windows-only by construction: `HandleFacts::is_reparse_point` is hard-coded `false` on Unix.
    #[cfg(windows)]
    #[test]
    fn cpe_1959_a_surrogate_at_the_output_is_refused_by_the_handle_check_and_a_placeholder_is_written() {
        use std::os::windows::fs::MetadataExt as _;

        /// Non-Microsoft (bit 31 clear), NOT a name surrogate (bit 29 clear) — the dehydrated-cloud-file
        /// shape, which `std::fs::FileType::is_symlink` answers `false` for.
        const NON_SURROGATE_FILE_TAG: u32 = 0x0000_1959;
        /// The same tag with `IO_REPARSE_TAG_NAME_SURROGATE` set, and nothing else changed.
        const SURROGATE_FILE_TAG: u32 = 0x2000_1959;
        const REPARSE_ATTR: u32 = 0x400;

        let dir = scratch("cpe1959-doctrine");
        let selected = dir.path().join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let input = selected.join("photo.png");
        std::fs::write(&input, b"input bytes").unwrap();

        // --- Half 1: the non-surrogate placeholder must be WRITTEN, and written correctly. ---
        let placeholder = selected.join("out.png");
        std::fs::write(&placeholder, b"stale").unwrap();
        if !crate::fsutil::make_guid_reparse_point(&placeholder, NON_SURROGATE_FILE_TAG, false) {
            crate::skip_notice!(
                "SKIPPING cpe_1959_a_surrogate_at_the_output_is_refused_by_the_handle_check_and_a_placeholder_is_written: \
                 could not plant a GUID reparse point on this volume. NOTHING on this run covered the \
                 handle-side reparse doctrine in open_output_verified."
            );
            let _ = std::fs::remove_dir_all(dir.path());
            return;
        }
        assert!(
            std::fs::symlink_metadata(&placeholder).unwrap().file_attributes() & REPARSE_ATTR != 0,
            "fixture is inert: no FILE_ATTRIBUTE_REPARSE_POINT on the placeholder, so this half would \
             pass for an ordinary file"
        );
        assert!(
            !std::fs::symlink_metadata(&placeholder).unwrap().file_type().is_symlink(),
            "fixture is shadowed: std calls the non-surrogate placeholder a symlink, so the \
             symlink_metadata path check would refuse it and this half could never pass"
        );

        let placeholder_s = placeholder.to_string_lossy().to_string();
        let verified = open_output_verified(&input.to_string_lossy(), &placeholder_s).expect(
            "a reparse point that does not stand in for another name must be written — refusing it is \
             what turned every dehydrated cloud output into a skipped batch item (CPE-1896's rule, \
             CPE-1959's decision)",
        );
        verified.write_all(b"CONVERTED BYTES", &placeholder_s).expect("the staged write must commit");
        // THE assertion, and it is on the filesystem rather than on the `Ok` above: `open_output_verified`
        // returns before a byte is written, so an `Ok` from it is not evidence of anything.
        assert_eq!(
            std::fs::read(&placeholder).unwrap(),
            b"CONVERTED BYTES",
            "the placeholder's name must be holding the batch's output bytes"
        );
        // And the write went through CPE-1961's staging commit, so the placeholder was REPLACED by an
        // ordinary file rather than written through — which is the fact this ticket's decision rests on.
        // `carried_attribute_mask` deliberately does not carry FILE_ATTRIBUTE_REPARSE_POINT.
        assert!(
            std::fs::symlink_metadata(&placeholder).unwrap().file_attributes() & REPARSE_ATTR == 0,
            "the output must be an ordinary file now: a surviving reparse bit would mean the bytes were \
             written THROUGH the placeholder, which is the outcome CPE-1896 could not rule out and \
             CPE-1961's stage-and-rename is what rules out here"
        );

        // --- Half 2: the surrogate, differing in exactly one bit, must still be REFUSED. ---
        let surrogate = selected.join("out2.png");
        let surrogate_bytes = b"the surrogate's own bytes".to_vec();
        std::fs::write(&surrogate, &surrogate_bytes).unwrap();
        if !crate::fsutil::make_guid_reparse_point(&surrogate, SURROGATE_FILE_TAG, false) {
            crate::skip_notice!(
                "SKIPPING the surrogate half of \
                 cpe_1959_a_surrogate_at_the_output_is_refused_by_the_handle_check_and_a_placeholder_is_written: \
                 could not plant a surrogate GUID reparse point on this volume."
            );
            let _ = std::fs::remove_dir_all(dir.path());
            return;
        }
        let err = open_output_verified(&input.to_string_lossy(), &surrogate.to_string_lossy())
            .err()
            .expect("a name-surrogate reparse point at the output must still be refused");
        // **Which guard answers here is measured, not assumed, and it is NOT the handle check.** Round 1
        // of this test asserted the handle refusal's `"stands in for another name"` wording and passed —
        // then passed again with the handle refusal disabled outright, because `WHY_SURROGATE_TAG`
        // (the path-side `classify_reparse_tag` verdict for a surrogate tag this module does not
        // recognise) contains that same phrase and runs *before* the open. A GUID surrogate is therefore
        // refused at containment, one guard earlier, and cannot reach the tag check on the handle. That
        // is pinned as what it is, so nobody re-derives it: half 3 below is the leg that reaches the
        // handle check, and this half's job is the one-bit-apart claim — the *tag* is what decides
        // written-vs-refused — not which guard says it.
        assert!(
            err.contains("a write would follow it somewhere this check cannot predict"),
            "the path-side WHY_SURROGATE_TAG verdict must be the one that fired for an unrecognised \
             surrogate tag; if this moved, the shadowing note above it is stale: {err}"
        );
        // Refused means the object on disk is as it was, not merely that an `Err` came back. It cannot
        // be read back with `fs::read` the way half 1's output can — a *surrogate* GUID tag with no
        // filter driver behind it answers `ERROR_CANT_ACCESS_FILE` (1920) to an ordinary open, measured
        // here — which is why the untouched-ness is asserted on the metadata instead: the reparse bit
        // still set (so nothing replaced it, as a successful staged commit would have) and the same
        // `$DATA` length it was given (so nothing truncated or rewrote it).
        let after = std::fs::symlink_metadata(&surrogate).unwrap();
        assert!(
            after.file_attributes() & REPARSE_ATTR != 0,
            "a refused output must still be the reparse point it was — a cleared reparse bit would mean \
             the staged commit replaced the name, which is exactly what the refusal must prevent"
        );
        assert_eq!(
            after.len(),
            surrogate_bytes.len() as u64,
            "a refused output's bytes must be untouched"
        );

        // --- Half 3: the surrogate class that actually REACHES the narrowed handle check. ---
        //
        // `classify_reparse_tag` sends an unrecognised surrogate to `WHY_SURROGATE_TAG` before the open
        // (half 2) but sends `IO_REPARSE_TAG_SYMLINK`/`MOUNT_POINT` through as `Probe::Link`, which
        // containment lets past — so a real symlink is the only fixture in the crate that arrives at the
        // tag check on the handle. Dangling-inside-the-folder, the same shape `cpe_1667_…` uses, because
        // that is the one guaranteed to survive containment.
        //
        // The assertion is on the parenthetical, not on `"stands in for another name"`: that phrase is in
        // `WHY_SURROGATE_TAG` too (half 2 measured it), so it would pass for free if the fixture ever
        // stopped reaching here. `"(a symlink, a junction, or a mount point)"` belongs to the handle
        // refusal alone — the `symlink_metadata` path check below it says only "is a link".
        let dangling = selected.join("out3.png");
        if try_symlink(
            &selected.join("does-not-exist.png"),
            &dangling,
            "half 3 of cpe_1959_a_surrogate_at_the_output_is_refused_by_the_handle_check_and_a_placeholder_is_written",
        ) {
            let err = open_output_verified(&input.to_string_lossy(), &dangling.to_string_lossy())
                .err()
                .expect("a symlink at the output must still be refused after the narrowing");
            assert!(
                err.contains("(a symlink, a junction, or a mount point)"),
                "the HANDLE-side tag refusal must be the one that fired — both the containment verdict \
                 and the symlink_metadata path check produce sentences a looser assertion would accept \
                 for free: {err}"
            );
            assert!(
                std::fs::symlink_metadata(&dangling).unwrap().file_type().is_symlink(),
                "a refused link must still be a link — a staged commit replacing it with an ordinary \
                 file is exactly what this refusal exists to prevent"
            );
        }

        // --- Half 4: the NON-SYMLINK surrogate class, delivered to the handle check as the FIRST
        //             decider (CPE-1959 round 2, scoped correctly in round 3). ---
        //
        // Half 3 leaves a real gap and it is worth naming precisely: it proves the handle check fires,
        // but only for a tag `classify_reparse_tag` recognises. For every *other* surrogate tag the
        // by-path `WHY_SURROGATE_TAG` verdict answers first (half 2 measured that), so the handle
        // check's coverage of that class was still shadowed in CPE-1929's exact sense — safe, and
        // unverifiable, at the same time.
        //
        // `between_containment_and_open` is the window that delivers it: the path probe has already run
        // and seen an **ordinary file**, so containment cannot answer. Same seam, same reason, as
        // `cpe_1961_a_link_planted_after_the_path_check_is_still_caught_by_the_handle_census`.
        //
        // **What this leg does NOT prove, because round 2's comment claimed it and round 3 measured it
        // false.** It is not the case that nothing else can see this fixture. The `symlink_metadata`
        // net below the handle check reads the same name-surrogate bit off the path, and the planted
        // tag is still on disk when it looks — so it reaches this input too. The claim here is
        // **ordering**: the handle check answers first, and its wording is unique to it, which is what
        // the assertion below reads. Non-redundancy comes from the object-vs-name difference, not from
        // visibility.
        let planted_late = selected.join("out4.png");
        let late_bytes = b"ordinary when containment looked".to_vec();
        std::fs::write(&planted_late, &late_bytes).unwrap();
        let planted_late_s = planted_late.to_string_lossy().to_string();
        surrogate_between_containment_and_open_for_test(&planted_late_s, SURROGATE_FILE_TAG);
        let late = open_output_verified(&input.to_string_lossy(), &planted_late_s);
        if surrogate_was_planted_for_test() {
            let err = late.err().expect(
                "a surrogate tag planted after the path check must still be refused — on the handle, \
                 which is what must answer first here",
            );
            assert!(
                err.contains("(a symlink, a junction, or a mount point)"),
                "the HANDLE-side tag refusal must be what answered FIRST here. WHY_SURROGATE_TAG cannot \
                 have: containment saw an ordinary file. The symlink_metadata check below it CAN — it \
                 reads the same name-surrogate bit off the path — and with the handle check disabled \
                 this assertion fails carrying that check's sentence, \"is a link, and a batch never \
                 writes through one\" (measured, PR #1103 review). So what this leg pins is ordering \
                 plus wording, not visibility; the guard's non-redundancy is that it reads the OBJECT, \
                 so a tag removed after the open defeats the net below and not this one. Got: {err}"
            );
            let after_late = std::fs::symlink_metadata(&planted_late).unwrap();
            assert!(
                after_late.file_attributes() & REPARSE_ATTR != 0,
                "the refused output must still carry the tag that was planted on it"
            );
            assert_eq!(
                after_late.len(),
                late_bytes.len() as u64,
                "a refused output's bytes must be untouched"
            );
        } else {
            crate::skip_notice!(
                "SKIPPING half 4 of \
                 cpe_1959_a_surrogate_at_the_output_is_refused_by_the_handle_check_and_a_placeholder_is_written: \
                 could not plant a surrogate GUID reparse point at the seam. NOTHING on this run covered \
                 the handle check against a NON-SYMLINK surrogate."
            );
        }

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// A refusal must leave no litter: when the verification creates the output and then refuses, it
    /// removes the empty file it made — but it must never remove a file it did NOT create.
    #[test]
    fn secaudit_a_refused_write_leaves_no_file_it_created_and_never_deletes_one_it_did_not() {
        let dir = scratch("secaudit-abandon");
        let selected = dir.path().join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let input = selected.join("photo.png");
        std::fs::write(&input, b"input bytes").unwrap();

        // Created by us, then abandoned -> gone.
        let fresh = selected.join("fresh.png").to_string_lossy().to_string();
        let v = open_output_verified(&input.to_string_lossy(), &fresh).expect("an ordinary output opens");
        assert!(v.created(), "the output did not exist, so this call must report having created it");
        v.abandon(&fresh);
        assert!(!std::path::Path::new(&fresh).exists(), "an output we created and abandoned must be removed");

        // Pre-existing, then abandoned -> untouched, contents intact.
        let existing = selected.join("existing.png");
        let existing_bytes = b"someone else's bytes".to_vec();
        std::fs::write(&existing, &existing_bytes).unwrap();
        let existing_s = existing.to_string_lossy().to_string();
        let v = open_output_verified(&input.to_string_lossy(), &existing_s).expect("opens");
        assert!(!v.created(), "an output that already existed must NOT be reported as created");
        v.abandon(&existing_s);
        assert_eq!(
            std::fs::read(&existing).unwrap(),
            existing_bytes,
            "abandoning must never remove or truncate a file this call did not create"
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// **CPE-1667.** `write_all` used to have no cleanup on failure: if `set_len`/`write_all`/`flush`
    /// itself errored on a file THIS call created — a genuine disk I/O fault, never any of the adversarial
    /// paths above (those are all refused before a single byte is touched) — the empty or
    /// partially-written file was left on disk. Injects a real, deterministic, cross-platform I/O failure
    /// rather than trying to fill a disk (which the UAT could not force): open a *second*, read-only
    /// handle to the freshly-created output and hand THAT to `VerifiedOutput` in place of the writable one
    /// `open_output_verified` would have produced. Every one of `set_len`/`write_all`/`flush` needs write
    /// access, so the very first of them fails — on every platform, no `libc`/OS-specific trick needed —
    /// and the cleanup path is exercised for real, not simulated.
    #[test]
    fn cpe_1667_write_all_removes_a_file_it_created_when_the_write_itself_fails() {
        let dir = scratch("cpe1667-write-fail-cleanup");
        let selected = dir.path().join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let output = selected.join("out.png");
        let output_s = output.to_string_lossy().to_string();

        // Claim the name exactly like `open_output_verified` itself would, so `created` is genuinely true.
        std::fs::OpenOptions::new().write(true).create_new(true).open(&output).unwrap();
        assert!(output.exists(), "sanity: the file exists before the injected failure");

        // **CPE-1961 moved this injection, and the move is the fix working.** It used to hand a
        // second, READ-ONLY handle to the output: `write_all` truncated and wrote through the handle
        // it was given, so the first step failed on every platform with no OS-specific trick. It no
        // longer writes through that handle at all — it reads the metadata and the carry-over off it
        // and stages the bytes into a fresh sibling — so a read-only handle now SUCCEEDS. Keeping the
        // old injection would have left this test green while asserting nothing.
        //
        // What can still fail is the exclusive create of the staging sibling, and that needs a
        // directory this process may open but not create in — one `chmod` on Unix, a hand-built
        // denying DACL on Windows. So it is injected, and every assertion below is still about the
        // FILESYSTEM. See `ProbeInjection::StagingCreateFails`.
        let writable = std::fs::OpenOptions::new().write(true).open(&output).unwrap();
        let verified = VerifiedOutput { file: writable, created: true };

        let reset = ProbeReset::arm(ProbeInjection::StagingCreateFails);
        let err = verified
            .write_all(b"whatever bytes would have gone here", &output_s)
            .expect_err("the injected staging-create failure must fail the write — \
                         this test verifies NOTHING if it doesn't");
        drop(reset);
        println!("CPE-1667 injected write failure: {err}");

        assert!(
            !output.exists(),
            "CPE-1667: a write that fails on a file THIS call created must leave nothing behind — \
             {output:?} is still on disk"
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// **CPE-1667 companion.** The failure-cleanup test above only proves cleanup happens for a file THIS
    /// call created. `abandon`'s own rule — never touch a file we did NOT create — must hold for a failed
    /// `write_all` too, or a disk fault on someone else's pre-existing output would delete their file
    /// instead of just failing the write.
    #[test]
    fn cpe_1667_write_all_never_removes_a_pre_existing_file_when_the_write_fails() {
        let dir = scratch("cpe1667-write-fail-preexisting");
        let selected = dir.path().join("selected");
        std::fs::create_dir_all(&selected).unwrap();
        let output = selected.join("existing.png");
        let original_bytes = b"someone else's bytes, already on disk".to_vec();
        std::fs::write(&output, &original_bytes).unwrap();
        let output_s = output.to_string_lossy().to_string();

        // CPE-1961: the injection moved from a read-only handle to the staging create — see the
        // sibling test above for why the old one now succeeds instead of failing.
        let writable = std::fs::OpenOptions::new().write(true).open(&output).unwrap();
        let verified = VerifiedOutput { file: writable, created: false };

        let reset = ProbeReset::arm(ProbeInjection::StagingCreateFails);
        let err = verified
            .write_all(b"an attacker or a bug should not be able to blank this out", &output_s)
            .expect_err("the injected staging-create failure must still fail the write");
        drop(reset);
        println!("CPE-1667 injected write failure (pre-existing file): {err}");

        assert!(output.exists(), "the pre-existing file must not be deleted just because its write failed");
        assert_eq!(
            std::fs::read(&output).unwrap(),
            original_bytes,
            "a failed write on a file we did NOT create must leave its original bytes untouched"
        );

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// **CPE-1961's guard for this engine, and the one CI runs.** The racer in `fsutil` measures the
    /// hole; this pins the property that closed it, with no race in it at all.
    ///
    /// # Two halves, because the Windows figure was a shield and the shield has to be shown
    ///
    /// The output really is a second name for a file outside the batch's folder -- a genuine hard
    /// link, not a mock.
    ///
    /// **Half one** goes through the front door and asserts that [`open_output_verified`] **refuses**
    /// it. That refusal comes from `classify_output_containment`, which runs *before* the open and
    /// resolves the output by PATH. It is worth pinning in its own right, and it is also the reason
    /// this site raced so much lower on Windows than `fsutil`'s (24 / 2,000 against 166 / 2,000, while
    /// both were ~2,800 per 10,000 on Linux): far fewer trials ever reached the check-then-use. **That
    /// is a path gauntlet, not containment** -- CPE-1929's shape -- and it is stated here rather than
    /// leaned on.
    ///
    /// **Half two** is the containment property itself. It bypasses the gauntlet by building the
    /// `VerifiedOutput` directly -- exactly the state the engine would be in if the link count had
    /// lied, which is what the racer produces and what
    /// [`ProbeInjection::HandleUnderReportsLinks`] injects at the sibling site -- and then asserts on
    /// the **filesystem**: the file outside the folder still holds its own bytes, and the output holds
    /// the new ones. Before CPE-1961 `write_all` truncated and wrote through that very handle, so this
    /// half fails against the old body with the outside file holding the batch's payload.
    ///
    /// The injection seam is not used here because it cannot be: it makes `handle_facts` lie, and
    /// `handle_facts` is not what the path gauntlet asks. Constructing the state directly is the
    /// honest way to reach the code under test, and it is what the two `cpe_1667_*` tests above
    /// already do.
    #[test]
    fn cpe_1961_a_write_never_lands_in_a_file_outside_the_batchs_folder() {
        const UNTOUCHED: &[u8] = b"UNTOUCHED";
        const PAYLOAD: &[u8] = b"ATTACKER PAYLOAD";

        let dir = scratch("cpe1961-batch-containment");
        let selected = dir.path().join("selected");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&selected).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("RACE_VICTIM.txt");
        std::fs::write(&victim, UNTOUCHED).unwrap();
        let input = selected.join("input.bin");
        std::fs::write(&input, b"the batch's input").unwrap();

        let output = selected.join("output.bin");
        if crate::links::create_hard_link(&victim.to_string_lossy(), &output.to_string_lossy()).is_err()
        {
            crate::skip_notice!(
                "SKIPPING cpe_1961_a_write_never_lands_in_a_file_outside_the_batchs_folder: no \
                 hard-link support here -- this test verified NOTHING"
            );
            let _ = std::fs::remove_dir_all(dir.path());
            return;
        }
        let (input_s, output_s) =
            (input.to_string_lossy().into_owned(), output.to_string_lossy().into_owned());

        // Half one: the shield. Recorded, not relied on.
        let shielded = open_output_verified(&input_s, &output_s);
        assert!(
            shielded.is_err(),
            "the path gauntlet in front of the open must still refuse a multiply-linked output whose \
             other name is outside the folder"
        );

        // Half two: the containment, with the gauntlet out of the way.
        let (file, created) = open_no_follow(&output).expect("the planted link opens no-follow");
        assert!(!created, "the output was planted before this call, so it cannot report as created");
        let result = VerifiedOutput { file, created }.write_all(PAYLOAD, &output_s);

        assert_eq!(
            std::fs::read(&victim).unwrap(),
            UNTOUCHED,
            "a file OUTSIDE the batch's folder was written through the output's name -- the write went \
             through the handle instead of into a staged sibling (result was: {result:?})"
        );
        assert_eq!(
            std::fs::read(&output).unwrap(),
            PAYLOAD,
            "the batch write must still land at the output the plan named (result was: {result:?})"
        );
        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// **CPE-1961: the `links > 1` census was shadowed, and this is what un-shadows it.**
    ///
    /// Measured pair before this test existed (CPE-1929's tell, both halves run):
    /// `if false && facts.links > 1` left the suite at **2,430 passed / 0 failed**, and forcing the
    /// predicate to lie (`HandleFacts { links: 1, ..facts }`) left it at **2,430** too. The shadowing
    /// check is [`classify_output_containment`]'s path-side link probe, which runs before the open —
    /// every fixture planted its link before the call, so the path question always answered first.
    ///
    /// The census is not redundant with it: the path probe reads the **name** before the open and the
    /// census reads the **object** after it, so a link planted *between* them is caught by exactly one
    /// of the two. This test plants it there, through
    /// [`link_between_containment_and_open_for_test`], and asserts on the filesystem.
    ///
    /// With this test present, `if false && facts.links > 1` reds it. That is the difference between a
    /// guard that is safe and one that is verified.
    #[test]
    fn cpe_1961_a_link_planted_after_the_path_check_is_still_caught_by_the_handle_census() {
        let dir = scratch("cpe1961-census-window");
        let selected = dir.path().join("selected");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&selected).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("VICTIM.txt");
        std::fs::write(&victim, b"UNTOUCHED").unwrap();
        let input = selected.join("input.bin");
        std::fs::write(&input, b"the batch's input").unwrap();
        // An ORDINARY file at the output, so the path gauntlet in front of the open says "inside".
        let output = selected.join("output.bin");
        std::fs::write(&output, b"an ordinary previous output").unwrap();

        let (input_s, output_s) =
            (input.to_string_lossy().into_owned(), output.to_string_lossy().into_owned());
        link_between_containment_and_open_for_test(&victim.to_string_lossy(), &output_s);
        let verdict = open_output_verified(&input_s, &output_s);

        if std::fs::read(&output).ok().as_deref() == Some(b"an ordinary previous output".as_slice()) {
            crate::skip_notice!(
                "SKIPPING cpe_1961_a_link_planted_after_the_path_check_is_still_caught_by_the_handle_\
                 census: the seam could not create a hard link here -- this test verified NOTHING"
            );
            let _ = std::fs::remove_dir_all(dir.path());
            return;
        }

        let err = verdict.err().expect(
            "a link planted between the path check and the open leaves the output with a second name \
             OUTSIDE the folder, and only the handle-side census can see it",
        );
        assert!(
            err.contains("names") && err.contains("selected folder"),
            "the refusal must be the census's, naming the count and the folder: {err}"
        );
        assert_eq!(
            std::fs::read(&victim).unwrap(),
            b"UNTOUCHED",
            "nothing may have been written through the planted link"
        );
        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// SEC-1: the memoized `dir_scans` census versus a freshly-probed link count.
    #[test]
    fn secaudit_stale_census_memo_allows_an_inside_name_to_be_swapped_for_an_outside_one() {
        let dir = scratch("secaudit-census-memo");
        let selected = dir.path().join("selected");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&selected).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let out = selected.join("out.png");
        std::fs::write(&out, b"the user's own bytes").unwrap();
        let decoy = selected.join("decoy.png");
        if std::fs::hard_link(&out, &decoy).is_err() {
            crate::skip_notice!("SKIP secaudit census memo: no hard links here — VERIFIED NOTHING");
            return;
        }
        let out_s = out.to_string_lossy().to_string();

        // 1. Up-front check (execute_plan_walk's pre-loop): links=2, both names inside -> Inside.
        //    This is what populates cache.dir_scans["…\selected\"].
        let mut cache = ParentCache::new();
        assert_eq!(
            classify_output_containment(&out_s, &out_s, &mut cache),
            Containment::Inside,
            "precondition: both names inside the folder, so the up-front check allows it"
        );

        // 2. The attacker's window: delete the INSIDE second name, add an OUTSIDE one.
        //    The link count is unchanged (2), so only the census can detect the swap.
        let victim = outside.join("victim.png");
        std::fs::remove_file(&decoy).unwrap();
        std::fs::hard_link(&out, &victim).unwrap();

        // 3. Control — a FRESH census sees the truth and refuses.
        let mut fresh = ParentCache::new();
        assert_eq!(
            classify_output_containment(&out_s, &out_s, &mut fresh),
            Containment::Escapes,
            "control: a fresh census proves a name now lives outside the folder"
        );

        // 4. The write-time re-check, using the SAME cache execute_plan_walk threads through.
        let verdict = classify_output_containment(&out_s, &out_s, &mut cache);
        println!("SEC-1 write-time verdict with the shared cache: {verdict:?}");
        assert_ne!(
            verdict,
            Containment::Inside,
            "EXPLOIT: the stale census memo allowed a write whose bytes land outside the folder"
        );
    }

    /// SEC-2: past-MAX_PATH probe/writer agreement — the fail-open class the brief says to assume present.
    #[test]
    fn secaudit_probe_and_writer_agree_past_max_path() {
        let dir = scratch("secaudit-maxpath");
        let mut deep = dir.path().to_path_buf();
        while deep.to_string_lossy().len() < 300 {
            deep = deep.join("abcdefghijklmnopqrstuvwxyz0123456789");
        }
        std::fs::create_dir_all(&deep).unwrap();
        let victim = deep.join("victim.png");
        let victim_s = victim.to_string_lossy().to_string();
        println!("SEC-2 path length = {}", victim_s.len());
        assert!(victim_s.len() > 260);
        std::fs::write(&victim, b"a stranger's long-path bytes").unwrap();

        // The writer can reach it...
        assert!(std::fs::read(&victim).is_ok(), "std::fs reaches past MAX_PATH");
        // ...so the probe must too. `Absent` here would be the fail-open.
        let p = probe_no_follow(&victim);
        println!("SEC-2 probe = {}", probe_label(&p));
        assert!(!matches!(p, Probe::Absent), "EXPLOIT: probe says Absent for a file the writer can clobber");
        assert!(matches!(p, Probe::Real(_)), "probe should resolve it fully");
        // …and same_file must fuse the two spellings the engine compares.
        assert!(same_file(&victim_s, &victim_s));

        let _ = std::fs::remove_dir_all(dir.path());
    }

    /// SEC-3: Windows reserved device names as an output (`NUL.png`, `CON.png`, `COM1.png`).
    #[test]
    fn secaudit_reserved_device_name_outputs() {
        let dir = scratch("secaudit-devices");
        let d = dir.path();
        for name in ["NUL.png", "CON.png", "COM1.png", "nul", "AUX.jpg"] {
            let out = d.join(name).to_string_lossy().to_string();
            let input = d.join("photo.jpg").to_string_lossy().to_string();
            let mut cache = ParentCache::new();
            let verdict = classify_output_containment(&input, &out, &mut cache);
            let wrote = std::fs::write(&out, b"12345").is_ok();
            let readback = std::fs::read(&out).map(|b| b.len());
            println!("SEC-3 {name}: verdict={verdict:?} write_ok={wrote} readback={readback:?}");
        }
        let _ = std::fs::remove_dir_all(d);
    }

    /// SEC-4: Win32 silently strips trailing dots/spaces on non-verbatim paths. Does an output spelled
    /// `photo.png ` (which the writer lands on `photo.png`) get compared against the real `photo.png`?
    #[test]
    fn secaudit_trailing_dot_and_space_outputs_alias_an_existing_file() {
        let dir = scratch("secaudit-trailing");
        let d = dir.path();
        let real = d.join("photo.png");
        std::fs::write(&real, b"a stranger's file").unwrap();
        let real_s = real.to_string_lossy().to_string();
        for suffix in [" ", ".", "..", "  ", ". "] {
            let spelled = format!("{real_s}{suffix}");
            let fused = same_file(&real_s, &spelled);
            let is_file = std::path::Path::new(&spelled).is_file();
            let probe = probe_no_follow(std::path::Path::new(&spelled));
            println!(
                "SEC-4 {:?}: same_file={fused} is_file={is_file} probe={}", 
                suffix, probe_label(&probe)
            );
            if cfg!(windows) {
                assert!(
                    fused && is_file,
                    "EXPLOIT: `{spelled}` writes onto `{real_s}` but the engine does not see them as one file"
                );
            }
        }
        let _ = std::fs::remove_dir_all(d);
    }

    /// SEC-5: Unicode colon look-alikes — does Windows fold any of them into an ADS separator?
    #[test]
    fn secaudit_unicode_colon_lookalikes_are_not_stream_separators() {
        let dir = scratch("secaudit-unicolon");
        let d = dir.path();
        let host = d.join("host.png");
        std::fs::write(&host, b"host bytes").unwrap();
        let host_len = std::fs::metadata(&host).unwrap().len();
        for (label, ch) in
            [("U+2236", '\u{2236}'), ("U+A789", '\u{a789}'), ("U+FF1A", '\u{ff1a}'), ("U+F03A", '\u{f03a}')]
        {
            let spelled = format!("{}{ch}stream", host.to_string_lossy());
            let refused = final_component_names_alternate_stream(
                spelled.rsplit(['/', '\\']).next().unwrap_or(&spelled),
            );
            let wrote = std::fs::write(&spelled, b"attacker payload").is_ok();
            let host_now = std::fs::metadata(&host).unwrap().len();
            let separate = std::path::Path::new(&spelled).is_file();
            println!(
                "SEC-5 {label}: refused_as_ADS={refused} write_ok={wrote} host_len {host_len}->{host_now} \
                 separate_file={separate}"
            );
            assert_eq!(host_now, host_len, "EXPLOIT: {label} wrote into the host file");
        }
        let _ = std::fs::remove_dir_all(d);
    }

    /// SEC-6: `::$DATA` and a stream on a directory.
    #[test]
    fn secaudit_dollar_data_and_directory_streams() {
        let dir = scratch("secaudit-dollardata");
        let d = dir.path();
        let input = d.join("photo.png").to_string_lossy().to_string();
        for out in [
            format!("{}", d.join("host.png::$DATA").to_string_lossy()),
            format!("{}", d.join("host.png:s:$DATA").to_string_lossy()),
            format!("{}:dirstream", d.to_string_lossy()),
        ] {
            let mut cache = ParentCache::new();
            let verdict = classify_output_containment(&input, &out, &mut cache);
            println!("SEC-6 {out}\n   -> {verdict:?}");
            if cfg!(windows) {
                assert!(
                    !matches!(verdict, Containment::Inside),
                    "EXPLOIT: a stream path was accepted: {out}"
                );
            }
        }
        let _ = std::fs::remove_dir_all(d);
    }
}
