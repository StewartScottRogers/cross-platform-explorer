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
//! own doc for the containment check and the broadened real-filesystem collision guard that closes the
//! rest of the gap.
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

/// True when `template` could move a [`MediaOp::Rename`]'s computed output into a different directory
/// than its input: a path separator (`/` or `\`) or a literal `..` traversal segment (CPE-1623). The
/// template only ever substitutes into the file's STEM — see `plan()`'s `Rename` arm, which runs the
/// substituted result straight through [`join`] alongside the input's own unchanged directory — so it
/// has no legitimate reason to name a directory, let alone walk out of one, at all. Checked as plain
/// substrings (not "is this a whole path segment") deliberately: `template.replace("{stem}", ...)` means
/// the attacker doesn't need the traversal to occupy a whole segment on its own (see the ticket's worked
/// example, `"..\\..\\cpe1613_traversal_victim\\important"`, which has no `{stem}`/`{n}`/`{ext}` token at
/// all and is used completely literally).
fn template_escapes_directory(template: &str) -> bool {
    template.contains('/') || template.contains('\\') || template.contains("..")
}

/// Reject a job that can't be executed: no ops, a bad rotation angle, an empty convert extension, an
/// empty rename template, or (CPE-1623) a rename template that could walk the output outside the folder
/// the user picked.
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
#[cfg(test)]
fn reset_canonicalize_call_count() {
    CANONICALIZE_CALLS.with(|c| c.set(0));
}
#[cfg(test)]
fn canonicalize_call_count() -> usize {
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
fn path_key(path: &str, parent_cache: &mut std::collections::HashMap<String, Option<std::path::PathBuf>>) -> PathKey {
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
            // its own input already lives in. Cheap fast path: every non-Rename op, and any Rename
            // template without a separator, leaves `out_dir` textually identical to `dir`, so the
            // (still O(1)-amortized, but non-zero) `path_key` resolution below is skipped entirely.
            let (out_dir, _, _) = split(&output);
            if out_dir != dir {
                let out_dir_key = path_key(&out_dir, &mut parent_cache);
                let in_dir_key = path_key(&dir, &mut parent_cache);
                if out_dir_key != in_dir_key {
                    return Err(format!(
                        "rename template for \"{input}\" would write to \"{output}\", outside its own \
                         folder — a rename can only change a file's name, not its folder"
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
        reject("a/../b");

        // Ordinary templates — no separators, no ".." — must still validate fine.
        for ok in ["{stem}", "{stem}-{n}", "photo-{n}", "vacation 2024", "{stem}_backup"] {
            let job = BatchJob::new(vec![MediaOp::Rename { template: ok.into() }]);
            assert!(validate(&job).is_ok(), "expected {ok:?} to validate");
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
