//! AI-copilot **operation-plan model** (CPE-990, epic CPE-977): the pure, filesystem-free structured plan
//! a natural-language instruction will later compile to — plus its safety validator and a dry-run summary.
//!
//! This is deliberately the *safe middle*: there is **no LLM here and no I/O**. A plan is a closed,
//! whitelisted list of [`FileOp`]s (move/rename/delete/mkdir/copy) — no free-form or shell op exists *by
//! construction*, so a plan is always inspectable. [`validate`] enforces a safety envelope ([`PlanLimits`]:
//! a scope `root` every path must stay under + a cap on op count) and reports **all** violations at once,
//! and [`summarize`] produces the per-kind counts a confirm dialog shows before anything runs.
//!
//! Pure std + serde only — the headless core the planner seam and the preview/confirm UI both drive.
//! Complements the checkpoint revert plan ([`crate::restore_plan`]) and the user-macro expansion
//! ([`crate::action_macro`]): those act on filesystem state; this is the *copilot's* intent-level plan.

/// One concrete file operation. A **closed, whitelisted** set — the only vocabulary a plan (and therefore
/// the NL translator that emits one) may use, so there is no free-form/shell escape hatch by construction.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FileOp {
    /// Move `src` to `dst` (both full paths within the scope root).
    Move { src: String, dst: String },
    /// Rename the entry at `path` to the bare name `new_name` (a filename, not a path).
    Rename { path: String, new_name: String },
    /// Delete the entry at `path`.
    Delete { path: String },
    /// Create the directory `path`.
    Mkdir { path: String },
    /// Copy `src` to `dst` (both full paths within the scope root).
    Copy { src: String, dst: String },
}

/// An ordered plan: the concrete ops an instruction compiled to, executed top-to-bottom.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FileOpPlan {
    pub ops: Vec<FileOp>,
}

/// The safety envelope a plan is validated against: a scope `root` every path must stay under (no escaping
/// the current tree, even via `..`), and `max_ops`, a hard cap on how many operations one plan may carry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlanLimits {
    pub max_ops: usize,
    pub root: String,
}

impl PlanLimits {
    /// A conservative default cap — a copilot bulk-op plan touching more than this many entries should be
    /// re-planned or split rather than run in one confirm.
    pub const DEFAULT_MAX_OPS: usize = 1000;

    /// Build limits with an explicit cap.
    pub fn new(root: impl Into<String>, max_ops: usize) -> Self {
        Self { max_ops, root: root.into() }
    }

    /// Build limits for `root` using [`DEFAULT_MAX_OPS`](Self::DEFAULT_MAX_OPS).
    pub fn with_root(root: impl Into<String>) -> Self {
        Self::new(root, Self::DEFAULT_MAX_OPS)
    }
}

/// Validate `plan` against `limits`, returning **every** violation (not just the first) so the caller can
/// surface a complete list. `Ok(())` means the plan is well-formed and in-scope.
///
/// Checks: (a) the op count must not exceed `limits.max_ops`; (b) every path field (a move/copy `src`+`dst`,
/// a rename/delete/mkdir `path`) must be non-empty and stay within `limits.root` — including rejecting `..`
/// traversal that would climb out of the root; (c) a rename's `new_name` must be non-empty and a bare name.
///
/// Pure and string-based: no canonicalisation, no filesystem access. See [`within_root`] for the scope rule.
pub fn validate(plan: &FileOpPlan, limits: &PlanLimits) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if plan.ops.len() > limits.max_ops {
        errors.push(format!(
            "plan has {} ops, exceeds the cap of {}",
            plan.ops.len(),
            limits.max_ops
        ));
    }

    let root = &limits.root;
    for (i, op) in plan.ops.iter().enumerate() {
        let n = i + 1; // 1-based for human-facing messages, matching sibling modules.

        // Every path-typed field must be a non-empty, in-scope path.
        for (field, value) in op.path_fields() {
            if value.is_empty() {
                errors.push(format!("op {n}: {field} must not be empty"));
            } else if !within_root(root, value) {
                errors.push(format!("op {n}: {field} {value:?} escapes the scope root {root:?}"));
            }
        }

        // A rename's target is a *name*, not a path: reject empty, separators, and `.`/`..` — a rename may
        // not relocate an entry or climb the tree (that is what Move is for, and it is scope-checked above).
        if let FileOp::Rename { new_name, .. } = op {
            if new_name.is_empty() {
                errors.push(format!("op {n}: new_name must not be empty"));
            } else if new_name.contains('/')
                || new_name.contains('\\')
                || new_name == "."
                || new_name == ".."
            {
                errors.push(format!(
                    "op {n}: new_name {new_name:?} must be a bare filename (no path separators or `..`)"
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Is `path` within `root` after logically resolving `.`/`..` — with **no** filesystem access?
///
/// Both strings are split into segments on either separator (`/` or `\`), dropping empty and `.` segments;
/// a `..` pops the previous segment. If a `..` would pop above the (virtual) filesystem root the path has
/// escaped and this returns `false`. Otherwise the resolved path is in scope iff its segment list *starts
/// with* the resolved root's segments. So `root/a/../b` stays in, while `root/../sibling` and `../up` do not.
pub fn within_root(root: &str, path: &str) -> bool {
    let (root_segs, root_ok) = normalize(root);
    let (path_segs, path_ok) = normalize(path);
    // A path whose `..` climbs above the filesystem root is always out of scope. A malformed root is treated
    // as escaping too, so nothing validates against a broken envelope.
    if !root_ok || !path_ok {
        return false;
    }
    path_segs.len() >= root_segs.len() && path_segs[..root_segs.len()] == root_segs[..]
}

/// Resolve `s` into its logical path segments. Returns `(segments, ok)` where `ok` is `false` if a `..`
/// popped above the virtual root (an escape). Splits on both `/` and `\`, drops empty and `.` segments.
fn normalize(s: &str) -> (Vec<&str>, bool) {
    let mut stack: Vec<&str> = Vec::new();
    for seg in s.split(['/', '\\']) {
        match seg {
            "" | "." => {} // root/leading/trailing/duplicate separators and `.` are no-ops
            ".." => {
                if stack.pop().is_none() {
                    return (stack, false); // climbed above the root — escape
                }
            }
            other => stack.push(other),
        }
    }
    (stack, true)
}

impl FileOp {
    /// The path-typed fields of this op as `(field_name, value)` pairs, for scope validation. `new_name`
    /// (a bare name, not a path) is deliberately excluded — it is validated separately.
    fn path_fields(&self) -> Vec<(&'static str, &str)> {
        match self {
            FileOp::Move { src, dst } => vec![("src", src), ("dst", dst)],
            FileOp::Copy { src, dst } => vec![("src", src), ("dst", dst)],
            FileOp::Rename { path, .. } => vec![("path", path)],
            FileOp::Delete { path } => vec![("path", path)],
            FileOp::Mkdir { path } => vec![("path", path)],
        }
    }
}

/// The dry-run tally of a plan — the per-kind counts a confirm dialog shows before the plan runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlanSummary {
    pub moves: usize,
    pub renames: usize,
    pub deletes: usize,
    pub mkdirs: usize,
    pub copies: usize,
}

impl PlanSummary {
    /// Total number of operations the plan carries.
    pub fn total(&self) -> usize {
        self.moves + self.renames + self.deletes + self.mkdirs + self.copies
    }

    /// True when the plan has no operations (nothing to confirm).
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// Count each kind of op in `plan` for the dry-run preview. Pure — counts only, no I/O.
pub fn summarize(plan: &FileOpPlan) -> PlanSummary {
    let mut s = PlanSummary::default();
    for op in &plan.ops {
        match op {
            FileOp::Move { .. } => s.moves += 1,
            FileOp::Rename { .. } => s.renames += 1,
            FileOp::Delete { .. } => s.deletes += 1,
            FileOp::Mkdir { .. } => s.mkdirs += 1,
            FileOp::Copy { .. } => s.copies += 1,
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "/project";

    fn plan(ops: Vec<FileOp>) -> FileOpPlan {
        FileOpPlan { ops }
    }

    fn limits() -> PlanLimits {
        PlanLimits::new(ROOT, 10)
    }

    #[test]
    fn clean_in_scope_plan_validates() {
        let p = plan(vec![
            FileOp::Mkdir { path: "/project/Archive".into() },
            FileOp::Move {
                src: "/project/a.pdf".into(),
                dst: "/project/Archive/a.pdf".into(),
            },
            FileOp::Copy {
                src: "/project/a.pdf".into(),
                dst: "/project/backup/a.pdf".into(),
            },
            FileOp::Rename { path: "/project/b.txt".into(), new_name: "c.txt".into() },
            FileOp::Delete { path: "/project/tmp".into() },
        ]);
        assert_eq!(validate(&p, &limits()), Ok(()));
    }

    #[test]
    fn out_of_scope_path_is_rejected() {
        // A sibling outside root, and a plain-prefix miss.
        let p = plan(vec![
            FileOp::Delete { path: "/etc/passwd".into() },
            FileOp::Mkdir { path: "/projectX/sneaky".into() }, // shares text prefix but not a segment
        ]);
        let errs = validate(&p, &limits()).unwrap_err();
        assert_eq!(errs.len(), 2, "got: {errs:?}");
        assert!(errs.iter().all(|e| e.contains("escapes the scope root")));
    }

    #[test]
    fn dotdot_escape_is_rejected() {
        // `..` that climbs out of root, even though it textually starts with root.
        let p = plan(vec![
            FileOp::Move {
                src: "/project/a".into(),
                dst: "/project/../secret/a".into(),
            },
            FileOp::Delete { path: "/project/sub/../../up".into() },
        ]);
        let errs = validate(&p, &limits()).unwrap_err();
        // dst escapes (1) + delete path escapes (1) = 2; the in-scope `src` is fine.
        assert_eq!(errs.len(), 2, "got: {errs:?}");
        assert!(errs.iter().all(|e| e.contains("escapes")));
    }

    #[test]
    fn in_scope_dotdot_stays_valid() {
        // `..` that resolves back inside root is fine.
        let p = plan(vec![FileOp::Move {
            src: "/project/x/../a.pdf".into(),
            dst: "/project/Archive/a.pdf".into(),
        }]);
        assert_eq!(validate(&p, &limits()), Ok(()));
    }

    #[test]
    fn over_cap_plan_is_rejected() {
        let ops = (0..5)
            .map(|i| FileOp::Mkdir { path: format!("/project/d{i}") })
            .collect();
        let lim = PlanLimits::new(ROOT, 3);
        let errs = validate(&plan(ops), &lim).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("exceeds the cap of 3"), "got: {errs:?}");
    }

    #[test]
    fn empty_path_and_new_name_are_rejected() {
        let p = plan(vec![
            FileOp::Delete { path: "".into() },
            FileOp::Rename { path: "/project/f".into(), new_name: "".into() },
        ]);
        let errs = validate(&p, &limits()).unwrap_err();
        // empty delete path + empty new_name = 2 violations.
        assert_eq!(errs.len(), 2, "got: {errs:?}");
        assert!(errs.iter().any(|e| e.contains("path must not be empty")));
        assert!(errs.iter().any(|e| e.contains("new_name must not be empty")));
    }

    #[test]
    fn rename_target_with_separator_is_rejected() {
        let p = plan(vec![FileOp::Rename {
            path: "/project/f".into(),
            new_name: "sub/g".into(),
        }]);
        let errs = validate(&p, &limits()).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("bare filename"), "got: {errs:?}");
    }

    #[test]
    fn returns_multiple_violations_at_once() {
        // Over cap AND an out-of-scope path AND an empty path AND an empty new_name — all reported together.
        let lim = PlanLimits::new(ROOT, 2);
        let p = plan(vec![
            FileOp::Delete { path: "/etc/x".into() }, // out of scope
            FileOp::Mkdir { path: "".into() },        // empty path
            FileOp::Rename { path: "/project/a".into(), new_name: "".into() }, // empty new_name
        ]);
        let errs = validate(&p, &lim).unwrap_err();
        // 1 (over cap: 3 > 2) + 1 (out of scope) + 1 (empty path) + 1 (empty new_name) = 4.
        assert_eq!(errs.len(), 4, "got: {errs:?}");
    }

    #[test]
    fn windows_style_root_and_paths() {
        let lim = PlanLimits::new(r"C:\work", 10);
        let ok = plan(vec![FileOp::Move {
            src: r"C:\work\a.txt".into(),
            dst: r"C:\work\sub\a.txt".into(),
        }]);
        assert_eq!(validate(&ok, &lim), Ok(()));

        let bad = plan(vec![FileOp::Delete { path: r"C:\Windows\system32".into() }]);
        assert!(validate(&bad, &lim).is_err());
    }

    #[test]
    fn summarize_counts_each_kind_and_total() {
        let p = plan(vec![
            FileOp::Move { src: "/project/a".into(), dst: "/project/b".into() },
            FileOp::Move { src: "/project/c".into(), dst: "/project/d".into() },
            FileOp::Rename { path: "/project/e".into(), new_name: "f".into() },
            FileOp::Delete { path: "/project/g".into() },
            FileOp::Mkdir { path: "/project/h".into() },
            FileOp::Copy { src: "/project/i".into(), dst: "/project/j".into() },
        ]);
        let s = summarize(&p);
        assert_eq!(s.moves, 2);
        assert_eq!(s.renames, 1);
        assert_eq!(s.deletes, 1);
        assert_eq!(s.mkdirs, 1);
        assert_eq!(s.copies, 1);
        assert_eq!(s.total(), 6);
        assert!(!s.is_empty());
    }

    #[test]
    fn empty_plan_summary_is_empty() {
        let s = summarize(&FileOpPlan::default());
        assert!(s.is_empty());
        assert_eq!(s.total(), 0);
    }

    #[test]
    fn plan_serde_round_trip() {
        let p = plan(vec![
            FileOp::Move { src: "/project/a".into(), dst: "/project/b".into() },
            FileOp::Rename { path: "/project/b".into(), new_name: "c".into() },
            FileOp::Delete { path: "/project/tmp".into() },
            FileOp::Mkdir { path: "/project/new".into() },
            FileOp::Copy { src: "/project/c".into(), dst: "/project/new/c".into() },
        ]);
        let json = serde_json::to_string(&p).expect("serialize");
        let back: FileOpPlan = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(p, back);
        // The whitelisted variants tag by snake_case name in the wire form.
        assert!(json.contains("\"move\""));
        assert!(json.contains("\"new_name\""));
    }
}
