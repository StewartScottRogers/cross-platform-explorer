//! Macro **executor resolution + undo model** (CPE-1187, epic CPE-739): the pure bridge between
//! [`crate::action_macro::plan`]'s abstract, filesystem-free [`crate::action_macro::PlannedOp`]s and
//! the concrete, collision-safe op list the CPE-1188 Tauri command layer actually applies.
//!
//! [`resolve`] threads each input's on-disk path through its macro steps — a rename/move/convert
//! changes it, a tag does not — producing an ordered [`ResolvedOp`] list plus a same-length,
//! same-order [`InverseOp`] list: applying every inverse **in reverse order** undoes the run. Along
//! the way it enforces the scope guard (reusing [`crate::op_plan::within_root`]'s logic): every
//! resolved rename/move/convert destination must stay within the working `root`, so a macro can
//! never write outside the folder it was run on — even via a maliciously-crafted rename template
//! containing `..`/path separators.
//!
//! Deliberately **pure** — no filesystem access, no [`crate::ctx::ServerCtx`]. The actual
//! `fs::rename`/tag-store writes happen in the CPE-1188 Tauri command, which calls the existing
//! `rename_entry`/`move_exact`/`set_tags` primitives (and a media re-encode for `convert`) driven by
//! this module's resolved plan; imported macros never auto-run — that gate lives entirely in the
//! command/UI layer, this module only ever computes a plan when explicitly asked to.
//!
//! **Tag inverse fidelity (CPE-1194):** this pure module always emits `"untag"` as a tag step's
//! inverse — it has no filesystem access, so it cannot know whether the label was already present on
//! a path before the run. The CPE-1188 apply layer (`src-tauri`'s `macro_apply_run`) snapshots the
//! pre-run tag state right before applying each `tag` op and, when the label was already there,
//! rewrites that op's [`InverseOp::kind`] in the returned [`ResolvedRun`] from `"untag"` to `"tag"` —
//! re-asserting a label that's already present is a no-op, so undo leaves a pre-existing tag alone
//! instead of stripping it. This module's shape (and its default `"untag"` emission) is unchanged;
//! only the apply layer's returned data is corrected against real state.
//!
//! **Convert inverse (CPE-1194):** a `convert` step's inverse always gets the dedicated
//! `"restore_convert"` kind (see [`InverseOp`]) rather than reusing the forward `"convert"` kind —
//! this is structural, not data-dependent, so it's decided here in the pure resolver. The apply layer
//! trashes the pre-convert original instead of deleting it, so `"restore_convert"` can restore those
//! exact bytes from the OS trash instead of re-encoding back to the source format.

use std::collections::HashSet;

use crate::action_macro::{plan, validate, ActionMacro};
use crate::op_plan::within_root;

/// One concrete, ready-to-apply operation the CPE-1188 command executes in order. `from` is this
/// input's path just before the step; `to` is its path just after (equal to `from` for `tag`, which
/// never moves anything); `detail` is the primitive's argument — a tag label for `tag`, otherwise
/// informational only (the apply layer derives the actual rename/move/convert target from `to`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ResolvedOp {
    pub from: String,
    pub kind: String,
    pub detail: String,
    pub to: String,
}

/// The inverse of one [`ResolvedOp`]. Applying a run's inverses **in reverse order** restores the
/// pre-run state. `kind` is `"untag"` (not `"tag"`) for a tag step's inverse as resolved here — the
/// CPE-1188 apply layer may rewrite it to `"tag"` post-hoc when the label was already present before
/// the run (CPE-1194: a no-op restore, not a strip); `rename`/`move` reuse their forward kind (the
/// swapped `from`/`to`/`detail` is what makes them reverse it). `convert` does **not** reuse its
/// forward kind — its inverse is the dedicated `"restore_convert"` kind (CPE-1194): the forward step
/// trashes the pre-convert original rather than deleting it, so the inverse restores those exact
/// bytes from the OS trash instead of re-encoding back to the source format.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InverseOp {
    pub from: String,
    pub kind: String,
    pub detail: String,
    pub to: String,
}

/// A resolved run: the ops to apply (in order) and their inverses (same order — apply the inverses
/// in **reverse** to undo the run).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ResolvedRun {
    pub ops: Vec<ResolvedOp>,
    pub inverses: Vec<InverseOp>,
}

/// One resolved rename/move/convert destination that is currently occupied on the real filesystem —
/// found by a preflight scan run **before** any op in a [`ResolvedRun`] is applied (CPE-1891). This
/// module stays pure and touches no filesystem itself; the scan that produces these lives in the apply
/// layer (`src-tauri`'s `macro_preflight_collisions`, alongside `macro_apply_run`) for the same reason
/// that layer already owns every other real-filesystem decision this module's own doc comment
/// describes.
///
/// `confirmable` is the whole point of this ticket: **true** for an ordinary occupied name (a plain
/// pre-existing file) — a `macro_run` call with `confirmed_overwrite: true` may overwrite it. **false**
/// for a link (live or dangling) or a slot that could not be read at all — CPE-1734's write-through
/// refusal stays absolute no matter what the user confirms; a link collision is still reported here so
/// the user can SEE it (the same visibility CPE-1869 established for the revert hold-back list), it
/// just has no confirm that unblocks it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MacroCollision {
    /// Index into `ResolvedRun::ops`/`inverses`, so the caller can correlate a collision back to the
    /// exact op it belongs to.
    pub op_index: usize,
    pub from: String,
    pub to: String,
    /// `"rename"` / `"move"` / `"convert"` — the only kinds a collision can occur at.
    pub kind: String,
    pub confirmable: bool,
    pub reason: String,
}

/// Resolve `m` over `inputs` into a collision-safe [`ResolvedRun`], rejecting (with **every**
/// violation reported, not just the first) any resolved rename/move/convert destination that would
/// land outside `root` — see [`within_root`]. Also rejects a malformed macro (see
/// [`crate::action_macro::validate`]). Pure — no filesystem access.
pub fn resolve(m: &ActionMacro, inputs: &[String], root: &str) -> Result<ResolvedRun, Vec<String>> {
    validate(m).map_err(|e| vec![e])?;
    if inputs.is_empty() {
        return Ok(ResolvedRun::default());
    }

    let planned = plan(m, inputs);
    let steps_per_input = m.steps.len();
    debug_assert_eq!(planned.len(), inputs.len() * steps_per_input);

    // Seed with every original input so a rename/move/convert never lands on an untouched selection
    // member either — mirrors `batch_media::plan`'s collision guard.
    let mut used: HashSet<String> = inputs.iter().cloned().collect();
    let mut ops = Vec::with_capacity(planned.len());
    let mut inverses = Vec::with_capacity(planned.len());
    let mut errors = Vec::new();

    for (i, input) in inputs.iter().enumerate() {
        let mut current = input.clone();
        let start = i * steps_per_input;
        for (step_idx, op) in planned[start..start + steps_per_input].iter().enumerate() {
            let op_n = start + step_idx + 1; // 1-based, across the whole run, for error messages
            match op.kind.as_str() {
                "tag" => {
                    ops.push(ResolvedOp {
                        from: current.clone(),
                        kind: "tag".into(),
                        detail: op.detail.clone(),
                        to: current.clone(),
                    });
                    inverses.push(InverseOp {
                        from: current.clone(),
                        kind: "untag".into(),
                        detail: op.detail.clone(),
                        to: current.clone(),
                    });
                }
                "rename" => {
                    let dir = parent_dir_with_sep(&current);
                    let target = dedupe(&dir, &op.detail, &mut used);
                    if !within_root(root, &target) {
                        errors.push(format!(
                            "op {op_n}: rename target {target:?} escapes the scope root {root:?}"
                        ));
                    }
                    let old_name = file_name(&current).to_string();
                    ops.push(ResolvedOp {
                        from: current.clone(),
                        kind: "rename".into(),
                        detail: op.detail.clone(),
                        to: target.clone(),
                    });
                    inverses.push(InverseOp {
                        from: target.clone(),
                        kind: "rename".into(),
                        detail: old_name,
                        to: current.clone(),
                    });
                    current = target;
                }
                "move" => {
                    let dir = dir_with_trailing_sep(&op.detail);
                    let name = file_name(&current).to_string();
                    let target = dedupe(&dir, &name, &mut used);
                    if !within_root(root, &target) {
                        errors.push(format!(
                            "op {op_n}: move target {target:?} escapes the scope root {root:?}"
                        ));
                    }
                    let old_dir = trim_trailing_sep(&parent_dir_with_sep(&current));
                    ops.push(ResolvedOp {
                        from: current.clone(),
                        kind: "move".into(),
                        detail: op.detail.clone(),
                        to: target.clone(),
                    });
                    inverses.push(InverseOp {
                        from: target.clone(),
                        kind: "move".into(),
                        detail: old_dir,
                        to: current.clone(),
                    });
                    current = target;
                }
                "convert" => {
                    let dir = parent_dir_with_sep(&current);
                    let (stem, old_ext) = split_stem_ext(file_name(&current));
                    let new_name = if op.detail.is_empty() {
                        stem.to_string()
                    } else {
                        format!("{stem}.{}", op.detail)
                    };
                    let target = dedupe(&dir, &new_name, &mut used);
                    if !within_root(root, &target) {
                        errors.push(format!(
                            "op {op_n}: convert target {target:?} escapes the scope root {root:?}"
                        ));
                    }
                    ops.push(ResolvedOp {
                        from: current.clone(),
                        kind: "convert".into(),
                        detail: op.detail.clone(),
                        to: target.clone(),
                    });
                    inverses.push(InverseOp {
                        from: target.clone(),
                        kind: "restore_convert".into(),
                        detail: old_ext.to_string(),
                        to: current.clone(),
                    });
                    current = target;
                }
                other => errors.push(format!("op {op_n}: unknown planned op kind {other:?}")),
            }
        }
    }

    if errors.is_empty() {
        Ok(ResolvedRun { ops, inverses })
    } else {
        Err(errors)
    }
}

/// Return `path`'s final path component, splitting on both `/` and `\` (platform-agnostic).
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Split a filename into `(stem, ext)` where `ext` excludes the dot. A leading-dot name
/// (`.gitignore`) or a name with no dot has an empty `ext` (mirrors `action_macro::split_name`).
fn split_stem_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(pos) if pos > 0 => (&name[..pos], &name[pos + 1..]),
        _ => (name, ""),
    }
}

/// The directory portion of `path`, **including** its trailing separator (empty string, meaning
/// "no directory prefix", when `path` has none). Preserves whichever separator `path` itself uses.
fn parent_dir_with_sep(path: &str) -> String {
    match path.rfind(['/', '\\']) {
        Some(i) => path[..=i].to_string(),
        None => String::new(),
    }
}

/// Strip a single trailing `/` or `\` from `dir` (used to turn a `parent_dir_with_sep` result back
/// into a plain destination-directory string for an inverse `move`'s `detail`).
fn trim_trailing_sep(dir: &str) -> String {
    dir.trim_end_matches(['/', '\\']).to_string()
}

/// Ensure `dest` ends with a separator, so a filename can be appended directly. Picks whichever
/// separator `dest` already contains (defaulting to `/`) rather than assuming a platform.
fn dir_with_trailing_sep(dest: &str) -> String {
    if dest.ends_with('/') || dest.ends_with('\\') {
        dest.to_string()
    } else {
        let sep = if dest.contains('\\') && !dest.contains('/') { '\\' } else { '/' };
        format!("{dest}{sep}")
    }
}

/// Join `dir` (already including its trailing separator, or empty) + `name` into a candidate path,
/// disambiguating with a deterministic `-2`, `-3`, … suffix (before the extension) against every
/// path already claimed in `used` this run — mirrors `batch_media::plan`'s collision guard so two
/// macro steps (or two different inputs) can never plan the same destination.
fn dedupe(dir: &str, name: &str, used: &mut HashSet<String>) -> String {
    let candidate = format!("{dir}{name}");
    if !used.contains(&candidate) {
        used.insert(candidate.clone());
        return candidate;
    }
    let (stem, ext) = split_stem_ext(name);
    let mut n = 2;
    loop {
        let next = if ext.is_empty() {
            format!("{dir}{stem}-{n}")
        } else {
            format!("{dir}{stem}-{n}.{ext}")
        };
        if !used.contains(&next) {
            used.insert(next.clone());
            return next;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_macro::MacroStep;
    use std::collections::BTreeSet;

    fn m(steps: Vec<MacroStep>) -> ActionMacro {
        ActionMacro { name: "m".into(), steps }
    }

    // ---- scope guard --------------------------------------------------------------------------

    #[test]
    fn out_of_root_move_dest_is_rejected() {
        let macro_ = m(vec![MacroStep::Move { dest: "/outside".into() }]);
        let errs = resolve(&macro_, &["/project/a.txt".into()], "/project").unwrap_err();
        assert_eq!(errs.len(), 1, "got: {errs:?}");
        assert!(errs[0].contains("escapes the scope root"), "got: {errs:?}");
    }

    #[test]
    fn dotdot_escape_via_move_dest_is_rejected() {
        let macro_ = m(vec![MacroStep::Move { dest: "/project/../secret".into() }]);
        let errs = resolve(&macro_, &["/project/a.txt".into()], "/project").unwrap_err();
        assert!(errs[0].contains("escapes the scope root"), "got: {errs:?}");
    }

    #[test]
    fn traversal_via_rename_template_literal_text_is_rejected() {
        // A rename template's literal text (not just tokens) could smuggle a `..` climb — the
        // scope guard must still catch it since `within_root` works on the resolved path, not on
        // the raw template.
        let macro_ = m(vec![MacroStep::Rename { template: "../../evil.txt".into() }]);
        let errs = resolve(&macro_, &["/project/sub/a.txt".into()], "/project").unwrap_err();
        assert!(errs[0].contains("escapes the scope root"), "got: {errs:?}");
    }

    #[test]
    fn in_root_move_is_accepted() {
        let macro_ = m(vec![MacroStep::Move { dest: "/project/Archive".into() }]);
        let run = resolve(&macro_, &["/project/a.txt".into()], "/project").unwrap();
        assert_eq!(run.ops.len(), 1);
        assert_eq!(run.ops[0].to, "/project/Archive/a.txt");
    }

    #[test]
    fn invalid_macro_is_rejected_before_resolution() {
        let macro_ = m(vec![]); // empty steps -> validate() rejects
        let errs = resolve(&macro_, &["/project/a.txt".into()], "/project").unwrap_err();
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn empty_inputs_resolve_to_an_empty_run() {
        let macro_ = m(vec![MacroStep::Tag { label: "x".into() }]);
        let run = resolve(&macro_, &[], "/project").unwrap();
        assert!(run.ops.is_empty() && run.inverses.is_empty());
    }

    // ---- collision-safe naming -----------------------------------------------------------------

    #[test]
    fn colliding_rename_targets_are_deterministically_disambiguated() {
        let macro_ = m(vec![MacroStep::Rename { template: "out.txt".into() }]);
        let inputs = vec!["/project/a.txt".into(), "/project/b.txt".into()];
        let run = resolve(&macro_, &inputs, "/project").unwrap();
        assert_eq!(run.ops[0].to, "/project/out.txt");
        assert_eq!(run.ops[1].to, "/project/out-2.txt");
    }

    #[test]
    fn colliding_move_targets_are_deterministically_disambiguated() {
        // Same basename, different source dirs, both moved into the same destination.
        let macro_ = m(vec![MacroStep::Move { dest: "/project/dst".into() }]);
        let inputs = vec!["/project/a/x.txt".into(), "/project/b/x.txt".into()];
        let run = resolve(&macro_, &inputs, "/project").unwrap();
        assert_eq!(run.ops[0].to, "/project/dst/x.txt");
        assert_eq!(run.ops[1].to, "/project/dst/x-2.txt");
    }

    #[test]
    fn rename_never_collides_with_an_untouched_input() {
        // Renaming a.txt to b.txt must not silently overwrite the OTHER selected input named b.txt.
        let macro_ = m(vec![MacroStep::Rename { template: "b.txt".into() }]);
        let inputs = vec!["/project/a.txt".into(), "/project/b.txt".into()];
        let run = resolve(&macro_, &inputs, "/project").unwrap();
        assert_eq!(run.ops[0].to, "/project/b-2.txt", "must dodge the untouched second input");
    }

    // ---- windows paths / dotfiles --------------------------------------------------------------

    #[test]
    fn windows_paths_preserve_backslash_separators() {
        let macro_ = m(vec![MacroStep::Rename { template: "renamed.txt".into() }]);
        let run = resolve(&macro_, &[r"C:\work\a.txt".into()], r"C:\work").unwrap();
        assert_eq!(run.ops[0].to, r"C:\work\renamed.txt");
    }

    #[test]
    fn convert_preserves_dotfile_stem_with_no_extension() {
        let macro_ = m(vec![MacroStep::Convert { to_ext: "bak".into() }]);
        let run = resolve(&macro_, &["/project/.gitignore".into()], "/project").unwrap();
        assert_eq!(run.ops[0].to, "/project/.gitignore.bak");
    }

    // ---- multi-step chaining: later steps see the RESULT of earlier ones -----------------------

    #[test]
    fn move_then_tag_operates_on_the_renamed_moved_path() {
        let macro_ = m(vec![
            MacroStep::Rename { template: "b.txt".into() },
            MacroStep::Move { dest: "/project/Archive".into() },
            MacroStep::Tag { label: "done".into() },
        ]);
        let run = resolve(&macro_, &["/project/a.txt".into()], "/project").unwrap();
        assert_eq!(run.ops.len(), 3);
        assert_eq!(run.ops[0].to, "/project/b.txt"); // rename
        assert_eq!(run.ops[1].from, "/project/b.txt"); // move picks up the renamed path
        assert_eq!(run.ops[1].to, "/project/Archive/b.txt");
        assert_eq!(run.ops[2].from, "/project/Archive/b.txt"); // tag sees the final path
        assert_eq!(run.ops[2].to, "/project/Archive/b.txt");
    }

    // ---- inverse round-trip, simulated at the resolution level (CPE-1187 acceptance) -----------

    /// A minimal in-memory simulation of "what a run + its undo would do to disk state", entirely
    /// at the path/tag level — no real I/O. `existing` is the set of paths currently present;
    /// `tags` maps a path to its attached labels (following the entry across a rename/move/convert,
    /// exactly like the real `retag_path` command keeps tags attached to a moved file).
    #[derive(Debug, Clone, Default, PartialEq)]
    struct SimState {
        existing: BTreeSet<String>,
        tags: std::collections::BTreeMap<String, BTreeSet<String>>,
    }

    fn apply_forward(state: &mut SimState, op: &ResolvedOp) {
        match op.kind.as_str() {
            "tag" => {
                state.tags.entry(op.to.clone()).or_default().insert(op.detail.clone());
            }
            _ => {
                state.existing.remove(&op.from);
                state.existing.insert(op.to.clone());
                if let Some(t) = state.tags.remove(&op.from) {
                    state.tags.insert(op.to.clone(), t);
                }
            }
        }
    }

    fn apply_inverse(state: &mut SimState, inv: &InverseOp) {
        match inv.kind.as_str() {
            "untag" => {
                if let Some(t) = state.tags.get_mut(&inv.to) {
                    t.remove(&inv.detail);
                    if t.is_empty() {
                        state.tags.remove(&inv.to);
                    }
                }
            }
            _ => {
                state.existing.remove(&inv.from);
                state.existing.insert(inv.to.clone());
                if let Some(t) = state.tags.remove(&inv.from) {
                    state.tags.insert(inv.to.clone(), t);
                }
            }
        }
    }

    fn run_and_undo_roundtrips(macro_: &ActionMacro, inputs: &[String]) {
        let run = resolve(macro_, inputs, "/project").expect("resolve should succeed");
        let initial = SimState {
            existing: inputs.iter().cloned().collect(),
            tags: Default::default(),
        };
        let mut state = initial.clone();
        for op in &run.ops {
            apply_forward(&mut state, op);
        }
        // A non-trivial macro actually changed something (sanity check the test is meaningful).
        if !run.ops.is_empty() {
            assert_ne!(state, initial, "forward apply should have changed the simulated state");
        }
        for inv in run.inverses.iter().rev() {
            apply_inverse(&mut state, inv);
        }
        assert_eq!(state, initial, "apply-then-invert (in reverse) must restore the original state");
    }

    #[test]
    fn rename_inverse_round_trips() {
        let macro_ = m(vec![MacroStep::Rename { template: "{stem}_v2.{ext}".into() }]);
        run_and_undo_roundtrips(&macro_, &["/project/a.txt".into()]);
    }

    #[test]
    fn move_inverse_round_trips() {
        let macro_ = m(vec![MacroStep::Move { dest: "/project/Archive".into() }]);
        run_and_undo_roundtrips(&macro_, &["/project/a.txt".into(), "/project/b.txt".into()]);
    }

    #[test]
    fn tag_inverse_round_trips() {
        let macro_ = m(vec![MacroStep::Tag { label: "done".into() }]);
        run_and_undo_roundtrips(&macro_, &["/project/a.txt".into()]);
    }

    #[test]
    fn convert_inverse_round_trips() {
        let macro_ = m(vec![MacroStep::Convert { to_ext: "png".into() }]);
        run_and_undo_roundtrips(&macro_, &["/project/photo.jpg".into()]);
    }

    #[test]
    fn full_multistep_sequence_inverse_round_trips() {
        let macro_ = m(vec![
            MacroStep::Rename { template: "{stem}_tidy.{ext}".into() },
            MacroStep::Move { dest: "/project/Archive".into() },
            MacroStep::Tag { label: "done".into() },
            MacroStep::Convert { to_ext: "bak".into() },
        ]);
        run_and_undo_roundtrips(
            &macro_,
            &["/project/a.txt".into(), "/project/b.txt".into(), "/project/c.txt".into()],
        );
    }
}
