//! Scriptable-action / user-macro model (CPE-938, epic CPE-739): a pure, dependency-free description of a
//! reusable multi-step file operation — a named sequence of rename/move/tag/convert steps — plus its
//! validation and a **filesystem-free** expansion (`plan`) of the macro over a selection of input paths into
//! a flat, ordered list of concrete ops the caller can preview or execute.
//!
//! Deliberately std-only: this is the headless core the GUI, a hotkey binding, or a watched-folder rule all
//! drive. Nothing here touches disk — `plan` is a pure function of `(macro, inputs)` so it's fully testable.

/// One step in a macro. Each variant maps to an existing op primitive.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum MacroStep {
    /// Rename each input using a template with `{name}` (full filename), `{stem}` (name without extension),
    /// `{ext}` (extension, no dot), and `{n}` (1-based selection index) tokens.
    Rename { template: String },
    /// Move each input into `dest` (a directory path).
    Move { dest: String },
    /// Attach the tag `label` to each input.
    Tag { label: String },
    /// Convert each input to the extension `to_ext` (no leading dot).
    Convert { to_ext: String },
}

/// A named, reusable multi-step action.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ActionMacro {
    pub name: String,
    pub steps: Vec<MacroStep>,
}

/// One concrete, expanded operation produced by [`plan`]. `kind` is a stable machine tag
/// (`rename`/`move`/`tag`/`convert`); `detail` is the resolved argument (the new name, dest, label, or
/// target extension).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlannedOp {
    pub input: String,
    pub kind: String,
    pub detail: String,
}

/// The tokens a [`MacroStep::Rename`] template may contain.
const RENAME_TOKENS: &[&str] = &["name", "stem", "ext", "n"];

/// Validate a macro: reject an empty name, an empty step list, and per-step problems (unknown `{token}` in a
/// rename template, an empty move dest, an empty tag label, an empty target extension). `Ok(())` means the
/// macro is well-formed enough to [`plan`].
pub fn validate(m: &ActionMacro) -> Result<(), String> {
    if m.name.trim().is_empty() {
        return Err("macro name must not be empty".into());
    }
    if m.steps.is_empty() {
        return Err("macro must have at least one step".into());
    }
    for (i, step) in m.steps.iter().enumerate() {
        match step {
            MacroStep::Rename { template } => {
                if template.trim().is_empty() {
                    return Err(format!("step {}: rename template must not be empty", i + 1));
                }
                for token in tokens(template) {
                    if !RENAME_TOKENS.contains(&token.as_str()) {
                        return Err(format!(
                            "step {}: unknown token {{{}}} in rename template",
                            i + 1,
                            token
                        ));
                    }
                }
            }
            MacroStep::Move { dest } => {
                if dest.trim().is_empty() {
                    return Err(format!("step {}: move dest must not be empty", i + 1));
                }
            }
            MacroStep::Tag { label } => {
                if label.trim().is_empty() {
                    return Err(format!("step {}: tag label must not be empty", i + 1));
                }
            }
            MacroStep::Convert { to_ext } => {
                if to_ext.trim().is_empty() {
                    return Err(format!("step {}: convert extension must not be empty", i + 1));
                }
            }
        }
    }
    Ok(())
}

/// Expand `m` over `inputs` into a flat, ordered list of concrete ops. **Pure** — touches no filesystem.
///
/// Ordering is deterministic: inputs outer, steps inner. So for inputs `[a, b]` and steps `[s1, s2]` the
/// result is `[a·s1, a·s2, b·s1, b·s2]`. The `{n}` rename token is the 1-based index of the input within the
/// selection (not affected by step order).
pub fn plan(m: &ActionMacro, inputs: &[String]) -> Vec<PlannedOp> {
    let mut ops = Vec::new();
    for (idx, input) in inputs.iter().enumerate() {
        let n = idx + 1;
        for step in &m.steps {
            let op = match step {
                MacroStep::Rename { template } => PlannedOp {
                    input: input.clone(),
                    kind: "rename".into(),
                    detail: expand_template(template, input, n),
                },
                MacroStep::Move { dest } => PlannedOp {
                    input: input.clone(),
                    kind: "move".into(),
                    detail: dest.clone(),
                },
                MacroStep::Tag { label } => PlannedOp {
                    input: input.clone(),
                    kind: "tag".into(),
                    detail: label.clone(),
                },
                MacroStep::Convert { to_ext } => PlannedOp {
                    input: input.clone(),
                    kind: "convert".into(),
                    detail: to_ext.trim_start_matches('.').to_string(),
                },
            };
            ops.push(op);
        }
    }
    ops
}

/// Collect the `{token}` names appearing in `s` (without braces). Unterminated `{` is ignored.
fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end) = s[i + 1..].find('}') {
                out.push(s[i + 1..i + 1 + end].to_string());
                i = i + 1 + end + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Split a filename into `(stem, ext)` where `ext` excludes the dot. A leading-dot name (`.gitignore`) or a
/// name with no dot has an empty `ext`.
fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(pos) if pos > 0 => (&name[..pos], &name[pos + 1..]),
        _ => (name, ""),
    }
}

/// Return the final path component of `input`, splitting on both `/` and `\` so the logic is
/// platform-agnostic.
fn file_name(input: &str) -> &str {
    input
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(input)
}

/// Expand a rename template for a single `input` at 1-based selection index `n`.
fn expand_template(template: &str, input: &str, n: usize) -> String {
    let name = file_name(input);
    let (stem, ext) = split_name(name);
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end) = template[i + 1..].find('}') {
                let token = &template[i + 1..i + 1 + end];
                match token {
                    "name" => out.push_str(name),
                    "stem" => out.push_str(stem),
                    "ext" => out.push_str(ext),
                    "n" => out.push_str(&n.to_string()),
                    // Unknown tokens are left verbatim (validate() rejects them up front).
                    other => {
                        out.push('{');
                        out.push_str(other);
                        out.push('}');
                    }
                }
                i = i + 1 + end + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(name: &str, steps: Vec<MacroStep>) -> ActionMacro {
        ActionMacro {
            name: name.into(),
            steps,
        }
    }

    #[test]
    fn validate_ok() {
        let macro_ = m(
            "tidy",
            vec![
                MacroStep::Rename {
                    template: "{stem}_{n}.{ext}".into(),
                },
                MacroStep::Move {
                    dest: "/archive".into(),
                },
                MacroStep::Tag {
                    label: "done".into(),
                },
                MacroStep::Convert {
                    to_ext: "png".into(),
                },
            ],
        );
        assert_eq!(validate(&macro_), Ok(()));
    }

    #[test]
    fn validate_rejects_empty_name() {
        let macro_ = m(
            "   ",
            vec![MacroStep::Tag {
                label: "x".into(),
            }],
        );
        assert!(validate(&macro_).is_err());
    }

    #[test]
    fn validate_rejects_empty_steps() {
        let macro_ = m("noop", vec![]);
        assert!(validate(&macro_).is_err());
    }

    #[test]
    fn validate_rejects_unknown_token() {
        let macro_ = m(
            "bad",
            vec![MacroStep::Rename {
                template: "{stem}-{bogus}".into(),
            }],
        );
        let err = validate(&macro_).unwrap_err();
        assert!(err.contains("bogus"), "got: {err}");
    }

    #[test]
    fn validate_rejects_empty_rename_template() {
        let macro_ = m(
            "bad",
            vec![MacroStep::Rename {
                template: "  ".into(),
            }],
        );
        assert!(validate(&macro_).is_err());
    }

    #[test]
    fn validate_rejects_empty_move_dest() {
        let macro_ = m(
            "bad",
            vec![MacroStep::Move { dest: "".into() }],
        );
        assert!(validate(&macro_).is_err());
    }

    #[test]
    fn validate_rejects_empty_tag_label() {
        let macro_ = m(
            "bad",
            vec![MacroStep::Tag { label: "".into() }],
        );
        assert!(validate(&macro_).is_err());
    }

    #[test]
    fn validate_rejects_empty_convert_ext() {
        let macro_ = m(
            "bad",
            vec![MacroStep::Convert { to_ext: "".into() }],
        );
        assert!(validate(&macro_).is_err());
    }

    #[test]
    fn plan_expands_rename_template() {
        let macro_ = m(
            "r",
            vec![MacroStep::Rename {
                template: "{stem}_{n}.{ext}".into(),
            }],
        );
        let ops = plan(&macro_, &["/a/photo.jpg".into()]);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].kind, "rename");
        assert_eq!(ops[0].input, "/a/photo.jpg");
        assert_eq!(ops[0].detail, "photo_1.jpg");
    }

    #[test]
    fn plan_n_index_increments_per_input() {
        let macro_ = m(
            "r",
            vec![MacroStep::Rename {
                template: "img{n}.{ext}".into(),
            }],
        );
        let inputs = vec![
            "one.png".to_string(),
            "two.png".to_string(),
            "three.png".to_string(),
        ];
        let ops = plan(&macro_, &inputs);
        assert_eq!(ops[0].detail, "img1.png");
        assert_eq!(ops[1].detail, "img2.png");
        assert_eq!(ops[2].detail, "img3.png");
    }

    #[test]
    fn plan_ordering_inputs_outer_steps_inner() {
        let macro_ = m(
            "multi",
            vec![
                MacroStep::Tag {
                    label: "t".into(),
                },
                MacroStep::Move {
                    dest: "/dst".into(),
                },
            ],
        );
        let inputs = vec!["a".to_string(), "b".to_string()];
        let ops = plan(&macro_, &inputs);
        let seq: Vec<(&str, &str)> = ops
            .iter()
            .map(|o| (o.input.as_str(), o.kind.as_str()))
            .collect();
        assert_eq!(
            seq,
            vec![("a", "tag"), ("a", "move"), ("b", "tag"), ("b", "move")]
        );
    }

    #[test]
    fn plan_convert_strips_leading_dot() {
        let macro_ = m(
            "c",
            vec![MacroStep::Convert {
                to_ext: ".webp".into(),
            }],
        );
        let ops = plan(&macro_, &["x.png".into()]);
        assert_eq!(ops[0].detail, "webp");
    }

    #[test]
    fn plan_handles_windows_paths_and_dotfiles() {
        let macro_ = m(
            "r",
            vec![MacroStep::Rename {
                template: "{name}|{stem}|{ext}".into(),
            }],
        );
        let ops = plan(&macro_, &[r"C:\docs\.gitignore".into()]);
        // A leading-dot name has empty ext and the whole thing as stem.
        assert_eq!(ops[0].detail, ".gitignore|.gitignore|");
    }

    #[test]
    fn plan_empty_inputs_yields_no_ops() {
        let macro_ = m(
            "r",
            vec![MacroStep::Tag {
                label: "t".into(),
            }],
        );
        assert!(plan(&macro_, &[]).is_empty());
    }
}
