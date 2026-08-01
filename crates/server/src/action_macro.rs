//! Scriptable-action / user-macro model (CPE-938, epic CPE-739): a pure, dependency-free description of a
//! reusable multi-step file operation — a named sequence of rename/move/tag/convert steps — plus its
//! validation and a **filesystem-free** expansion (`plan`) of the macro over a selection of input paths into
//! a flat, ordered list of concrete ops the caller can preview or execute.
//!
//! Deliberately std-only: this is the headless core the GUI, a hotkey binding, or a watched-folder rule all
//! drive. Nothing here touches disk — `plan` is a pure function of `(macro, inputs)` so it's fully testable.
//!
//! **Prompt-parameters (CPE-1190, additive):** any string field (a rename template, a move dest, a tag
//! label, a convert extension) may contain an `{ask:label}` token — a value the UI prompts the user for at
//! run time instead of baking it into the saved macro. [`plan`] is unchanged (it substitutes nothing, same
//! as before); [`plan_with_params`] is the new entry point that additionally substitutes each `{ask:label}`
//! occurrence with `params[label]` (an absent param resolves to nothing — a clean no-op, never a panic or a
//! broken plan). The prompt *dialog* that collects `params` is a separate, later ticket.

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
                    // CPE-1190 (additive): an `{ask:label}` prompt-parameter token is always
                    // well-formed regardless of its label — only the fixed `RENAME_TOKENS` are
                    // otherwise checked.
                    if !token.starts_with("ask:") && !RENAME_TOKENS.contains(&token.as_str()) {
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
///
/// Equivalent to [`plan_with_params`] with no params supplied — any `{ask:label}` token in a step's text is
/// left for `plan_with_params` to resolve; this function's behaviour is unchanged from before CPE-1190.
pub fn plan(m: &ActionMacro, inputs: &[String]) -> Vec<PlannedOp> {
    plan_with_params(m, inputs, &std::collections::BTreeMap::new())
}

/// [`plan`], additionally substituting every `{ask:label}` prompt-parameter token (in a rename template, a
/// move dest, a tag label, or a convert extension) with `params[label]` — an absent label resolves to
/// nothing (the token is simply dropped), so a partially-answered prompt never breaks the plan. **Pure**
/// (CPE-1190) — still touches no filesystem.
pub fn plan_with_params(
    m: &ActionMacro,
    inputs: &[String],
    params: &std::collections::BTreeMap<String, String>,
) -> Vec<PlannedOp> {
    let mut ops = Vec::new();
    for (idx, input) in inputs.iter().enumerate() {
        let n = idx + 1;
        for step in &m.steps {
            let op = match step {
                MacroStep::Rename { template } => PlannedOp {
                    input: input.clone(),
                    kind: "rename".into(),
                    detail: expand_template(&expand_ask(template, params), input, n),
                },
                MacroStep::Move { dest } => PlannedOp {
                    input: input.clone(),
                    kind: "move".into(),
                    detail: expand_ask(dest, params),
                },
                MacroStep::Tag { label } => PlannedOp {
                    input: input.clone(),
                    kind: "tag".into(),
                    detail: expand_ask(label, params),
                },
                MacroStep::Convert { to_ext } => PlannedOp {
                    input: input.clone(),
                    kind: "convert".into(),
                    detail: expand_ask(to_ext, params).trim_start_matches('.').to_string(),
                },
            };
            ops.push(op);
        }
    }
    ops
}

/// Substitute every `{ask:label}` occurrence in `s` with `params[label]` (dropped entirely — contributes
/// nothing — when `label` is absent from `params`). Every other `{token}` (including a bare `{ask}` with no
/// label, which isn't a valid prompt reference) is left completely untouched, byte-for-byte, so running this
/// over a template that contains no `{ask:...}` tokens at all — the case for every macro predating CPE-1190
/// — is a no-op.
fn expand_ask(s: &str, params: &std::collections::BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end) = s[i + 1..].find('}') {
                let token = &s[i + 1..i + 1 + end];
                if let Some(label) = token.strip_prefix("ask:") {
                    if let Some(value) = params.get(label) {
                        out.push_str(value);
                    }
                    i = i + 1 + end + 1;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
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
        let ch = template[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
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

    #[test]
    fn expand_template_preserves_non_ascii_literals_accented() {
        // CPE-1013: non-ASCII literal text (accented characters) should be preserved exactly.
        let macro_ = m(
            "accented",
            vec![MacroStep::Rename {
                template: "café_{n}.txt".into(),
            }],
        );
        let ops = plan(&macro_, &["input.txt".into()]);
        assert_eq!(ops[0].detail, "café_1.txt");
    }

    #[test]
    fn expand_template_preserves_non_ascii_literals_cjk() {
        // CPE-1013: CJK characters in literal text should be preserved exactly.
        let macro_ = m(
            "cjk",
            vec![MacroStep::Rename {
                template: "目录_{n}.{ext}".into(),
            }],
        );
        let ops = plan(&macro_, &["file.txt".into()]);
        assert_eq!(ops[0].detail, "目录_1.txt");
    }

    #[test]
    fn expand_template_preserves_non_ascii_literals_emoji() {
        // CPE-1013: emoji in literal text should be preserved exactly.
        let macro_ = m(
            "emoji",
            vec![MacroStep::Rename {
                template: "📁_{n}.{ext}".into(),
            }],
        );
        let ops = plan(&macro_, &["photo.jpg".into()]);
        assert_eq!(ops[0].detail, "📁_1.jpg");
    }

    #[test]
    fn expand_template_non_ascii_with_substitution_tokens() {
        // CPE-1013: ensure substitution tokens work correctly even when surrounded by non-ASCII text.
        let macro_ = m(
            "mixed",
            vec![MacroStep::Rename {
                template: "café_{stem}_día_{n}.{ext}".into(),
            }],
        );
        let ops = plan(&macro_, &["/path/my_file.pdf".into()]);
        assert_eq!(ops[0].detail, "café_my_file_día_1.pdf");
    }

    // ---- CPE-1190 (additive): {ask:label} prompt-parameters -----------------------------------

    fn params(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn validate_accepts_ask_tokens_in_rename_template() {
        let macro_ = m(
            "ask",
            vec![MacroStep::Rename {
                template: "{ask:project}_{stem}.{ext}".into(),
            }],
        );
        assert_eq!(validate(&macro_), Ok(()));
    }

    #[test]
    fn validate_still_rejects_a_truly_unknown_token() {
        // A bare `{ask}` (no `label:` suffix) is not a valid prompt reference and must still error,
        // same as any other unrecognised token — CPE-1190 only special-cases `ask:`-prefixed tokens.
        let macro_ = m(
            "bad",
            vec![MacroStep::Rename {
                template: "{ask}".into(),
            }],
        );
        assert!(validate(&macro_).is_err());
    }

    #[test]
    fn plan_leaves_ask_tokens_unresolved_and_unchanged_from_before_cpe_1190() {
        // `plan` (no params) must behave byte-for-byte as it did before CPE-1190 — an unresolved
        // `{ask:label}` in a rename template is simply not there (dropped), matching "absent param
        // resolves to nothing" — never a panic, never literal `{ask:...}` leaking into the output.
        let macro_ = m(
            "ask",
            vec![MacroStep::Rename {
                template: "{ask:prefix}{stem}.{ext}".into(),
            }],
        );
        let ops = plan(&macro_, &["/a/photo.jpg".into()]);
        assert_eq!(ops[0].detail, "photo.jpg");
    }

    #[test]
    fn plan_with_params_is_identical_to_plan_when_params_absent() {
        // A macro with no `{ask:...}` tokens at all must plan identically either way.
        let macro_ = m(
            "r",
            vec![MacroStep::Rename {
                template: "{stem}_{n}.{ext}".into(),
            }],
        );
        let inputs = vec!["/a/photo.jpg".into()];
        assert_eq!(plan(&macro_, &inputs), plan_with_params(&macro_, &inputs, &params(&[])));
    }

    #[test]
    fn plan_with_params_substitutes_ask_token_in_rename_template() {
        let macro_ = m(
            "ask",
            vec![MacroStep::Rename {
                template: "{ask:prefix}_{stem}.{ext}".into(),
            }],
        );
        let ops = plan_with_params(&macro_, &["/a/photo.jpg".into()], &params(&[("prefix", "vacation")]));
        assert_eq!(ops[0].detail, "vacation_photo.jpg");
    }

    #[test]
    fn plan_with_params_substitutes_ask_token_in_move_dest() {
        let macro_ = m(
            "ask",
            vec![MacroStep::Move {
                dest: "/archive/{ask:folder}".into(),
            }],
        );
        let ops = plan_with_params(&macro_, &["/a/photo.jpg".into()], &params(&[("folder", "2026")]));
        assert_eq!(ops[0].detail, "/archive/2026");
    }

    #[test]
    fn plan_with_params_substitutes_ask_token_in_tag_label() {
        let macro_ = m(
            "ask",
            vec![MacroStep::Tag {
                label: "{ask:label}".into(),
            }],
        );
        let ops = plan_with_params(&macro_, &["/a/photo.jpg".into()], &params(&[("label", "reviewed")]));
        assert_eq!(ops[0].detail, "reviewed");
    }

    #[test]
    fn plan_with_params_substitutes_ask_token_in_convert_ext() {
        let macro_ = m(
            "ask",
            vec![MacroStep::Convert {
                to_ext: "{ask:format}".into(),
            }],
        );
        let ops = plan_with_params(&macro_, &["/a/photo.jpg".into()], &params(&[("format", "webp")]));
        assert_eq!(ops[0].detail, "webp");
    }

    #[test]
    fn plan_with_params_absent_param_defaults_cleanly() {
        // Only some of the referenced params were answered — the unanswered one drops cleanly rather
        // than breaking the plan (no panic, no literal `{ask:...}` leaking through).
        let macro_ = m(
            "ask",
            vec![MacroStep::Rename {
                template: "{ask:missing}{stem}.{ext}".into(),
            }],
        );
        let ops = plan_with_params(&macro_, &["/a/photo.jpg".into()], &params(&[("other", "x")]));
        assert_eq!(ops[0].detail, "photo.jpg");
    }
}
