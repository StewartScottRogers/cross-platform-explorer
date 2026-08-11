//! Batch media operation planner (CPE-940, epic CPE-723): given a set of **non-destructive-by-default**
//! media transforms (resize / convert / rotate / flip / rename / strip-metadata) and a selection of input
//! files, compute the concrete per-file **output path** each will be written to — **collision-safe** — plus
//! a short human summary of the ops applied. Pure planning: no image work, no filesystem; the transform
//! engine executes the returned plan.

use std::collections::HashSet;

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

/// Reject a job that can't be executed: no ops, a bad rotation angle, an empty convert extension, or an
/// empty rename template.
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

/// Plan the batch: for each input compute its output path (applying the ops' effect on name/extension),
/// keep it non-destructive + collision-free when `non_destructive`, and summarise. Ordered like `inputs`.
pub fn plan(job: &BatchJob, inputs: &[String]) -> Vec<PlannedItem> {
    let mut used: HashSet<String> = HashSet::new();
    // Pre-seed with the inputs so non-destructive outputs never collide with a source file.
    if job.non_destructive {
        used.extend(inputs.iter().cloned());
    }

    inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
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

            if job.non_destructive {
                // Guarantee output != input and no two plans share an output — disambiguate with -2, -3…
                if output == *input && suffix.is_empty() {
                    out_stem = format!("{stem}-out");
                    output = join(&dir, &out_stem, &ext);
                }
                let base = out_stem.clone();
                let mut n = 2;
                while used.contains(&output) {
                    out_stem = format!("{base}-{n}");
                    output = join(&dir, &out_stem, &ext);
                    n += 1;
                }
                used.insert(output.clone());
            }

            let summary = if parts.is_empty() { "no-op".into() } else { parts.join(", ") };
            PlannedItem { input: input.clone(), output, summary }
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
        let out = plan(&job, &v(&["/pics/cat.jpg"]));
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
        let out = plan(&job, &v(&["/pics/cat.jpg"]));
        assert_eq!(out[0].summary, "no-op");
        assert_eq!(out[0].output, "/pics/cat-out.jpg");

        // In overwrite mode (no collision guard), an empty-image watermark truly changes nothing.
        let mut job2 = job.clone();
        job2.non_destructive = false;
        let out2 = plan(&job2, &v(&["/pics/cat.jpg"]));
        assert_eq!(out2[0].output, "/pics/cat.jpg");
    }

    #[test]
    fn watermark_summary_names_the_overlay_corner_and_opacity() {
        let job = BatchJob::new(vec![MediaOp::Watermark {
            image: "/assets/logo.png".into(),
            position: Corner::TopLeft,
            opacity: 40,
        }]);
        let out = plan(&job, &v(&["/pics/cat.jpg"]));
        assert_eq!(out[0].summary, "watermark logo.png top_left 40%");
        // No dedicated suffix (mirrors Compress/StripMetadata) — falls back to the generic "-out".
        assert_eq!(out[0].output, "/pics/cat-out.jpg");
    }

    #[test]
    fn resize_is_non_destructive_by_default() {
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 1024 }]);
        let out = plan(&job, &v(&["/pics/cat.jpg"]));
        assert_eq!(out[0].output, "/pics/cat-1024.jpg"); // suffix keeps it off the source
        assert_eq!(out[0].summary, "resize→1024px");
    }

    #[test]
    fn convert_changes_extension_and_lowercases() {
        let job = BatchJob::new(vec![MediaOp::Convert { to_ext: ".PNG".into() }]);
        let out = plan(&job, &v(&["/pics/cat.jpg"]));
        assert_eq!(out[0].output, "/pics/cat.png"); // different ext ⇒ already non-destructive
    }

    #[test]
    fn rename_template_expands_stem_and_index() {
        let job = BatchJob::new(vec![MediaOp::Rename { template: "photo-{n}".into() }]);
        let out = plan(&job, &v(&["/a/x.jpg", "/a/y.jpg"]));
        assert_eq!(out[0].output, "/a/photo-1.jpg");
        assert_eq!(out[1].output, "/a/photo-2.jpg");
    }

    #[test]
    fn same_target_collisions_are_disambiguated() {
        // Two inputs in different dirs both renamed to the same stem in the SAME dir → -2 suffix.
        let job = BatchJob::new(vec![MediaOp::Rename { template: "out".into() }]);
        let out = plan(&job, &v(&["/a/x.jpg", "/a/y.jpg"]));
        assert_eq!(out[0].output, "/a/out.jpg");
        assert_eq!(out[1].output, "/a/out-2.jpg");
    }

    #[test]
    fn overwrite_mode_keeps_the_input_path() {
        let mut job = BatchJob::new(vec![MediaOp::Resize { max_px: 512 }, MediaOp::StripMetadata]);
        job.non_destructive = false;
        let out = plan(&job, &v(&["/p/a.jpg"]));
        assert_eq!(out[0].output, "/p/a-512.jpg"); // suffix still applied, but no collision guard
        assert_eq!(out[0].summary, "resize→512px, strip-metadata");
    }

    #[test]
    fn multiple_ops_compose_suffix_and_summary() {
        let job = BatchJob::new(vec![MediaOp::Resize { max_px: 800 }, MediaOp::Rotate { degrees: 90 }]);
        let out = plan(&job, &v(&["c:\\img\\p.png"]));
        assert_eq!(out[0].output, "c:\\img\\p-800-rot90.png");
        assert_eq!(out[0].summary, "resize→800px, rotate 90°");
    }
}
