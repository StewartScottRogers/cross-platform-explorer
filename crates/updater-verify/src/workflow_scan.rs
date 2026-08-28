//! CPE-1933 — shell-comment-, quote-, escape- and heredoc-aware splitting of a GitHub Actions
//! workflow into logical shell lines, for the Rust guards that scan `.github/workflows/*.yml`.
//!
//! ## Why this exists rather than a fourth ad-hoc filter
//!
//! Every guard that reads a workflow anchors on a substring — `gh release download`,
//! `--bin verify-release-artifacts`, `apt-get`. A **comment** containing that substring is then
//! parsed as if it were the real invocation. That is not hypothetical: `release-sidecar.yml` carries
//! two prose comments that mention `gh release download` while discussing it, and CPE-1908 round 2
//! found the sharper version — a `--expect-channel sidecar` sitting in a comment, a heredoc body, or
//! a quoted `echo` string reading as "coverage" and letting a 100%-plain manifest pass under a
//! `-sidecar` tag.
//!
//! The TypeScript side already solved this. `src/lib/shellScriptLines.ts` was extracted at CPE-1849
//! and hardened through CPE-1908 rounds 2 and 3 **specifically so a second hand-rolled stripper could
//! not disagree with the first one on an edge case**. CPE-1933's first draft ignored that and shipped
//! a fifth stripper implementing only the weakest of its three rules (blank whole-line comments),
//! which a trailing comment walked straight through:
//!
//! ```text
//! --expect-url-prefix "https://…/${TAG}/"  # was: --expect-channel sidecar
//! ```
//!
//! read `--expect-channel sidecar` out of the comment and passed.
//!
//! This module is a deliberate, faithful **port** of that reference implementation — the Rust guards
//! cannot import a `.ts` module, so one cross-language copy is unavoidable. What is avoidable is the
//! copy silently diverging, so the port does not merely claim fidelity (the exact defect this ticket
//! exists to kill): `tests::the_port_matches_the_typescript_reference_on_every_shared_case` reads
//! `src/lib/shellScriptLines.cases.json` — the shared case file the TypeScript suite runs against
//! too — and executes this implementation over it. Add a case on either side and both languages are
//! held to it.
//!
//! Both Rust consumers (`tests/release_workflow_wiring.rs` and `artifact_binding.rs`'s workflow
//! derivation) use this module, so there is exactly one Rust implementation, not two.

/// Strips a shell `#` comment from one line, respecting quotes and backslash escapes.
///
/// A `#` only opens a comment when it is unquoted **and starts a word** (line start, or preceded by
/// whitespace), so a real command whose argument carries a literal `#` — a URL fragment, a quoted
/// value — is not truncated and silently vanished from a scan.
///
/// A quote character only OPENS a quoted string at the same kind of boundary (line start, or
/// preceded by something other than a letter/digit/underscore), so an apostrophe mid-word (a
/// contraction in an `echo` message) is not misread as opening an unterminated quote that swallows
/// the rest of the line, comment included. Inside a double-quoted string a backslash escapes the
/// next character, so `"a \" b"` is not misread as closing early — which would leave a real trailing
/// comment stuck inside a phantom quote and never stripped.
///
/// KNOWN GAP N9 (CPE-1936, documented rather than fixed): a line whose quote is never closed comes
/// back unchanged, trailing comment included — `echo "oops # not stripped`. Pinned as a case in the
/// shared file so both languages answer it identically. The obvious fix (treat an unterminated quote
/// as a literal) would truncate the first line of a legal multi-line quoted string, which is the
/// unsafe direction; the real fix is cross-line quote state. See the reference implementation's
/// comment for the full reasoning.
///
/// Ported from `stripShellComment` in `src/lib/shellScriptLines.ts`; see this module's header.
pub fn strip_shell_comment(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut quote: Option<char> = None;
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if let Some(q) = quote {
            if ch == '\\' && q == '"' && i + 1 < chars.len() {
                i += 2; // an escaped char inside a double-quoted string does not end the quote
                continue;
            }
            if ch == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if ch == '\\' && i + 1 < chars.len() {
            i += 2; // a backslash-escaped quote outside any quote is a literal char, not an opener
            continue;
        }
        if ch == '"' || ch == '\'' {
            let opens_here = match i.checked_sub(1).map(|p| chars[p]) {
                None => true,
                Some(prev) => !(prev.is_ascii_alphanumeric() || prev == '_'),
            };
            if opens_here {
                quote = Some(ch);
            }
            i += 1;
            continue;
        }
        if ch == '#' && (i == 0 || chars[i - 1].is_whitespace()) {
            return chars[..i].iter().collect();
        }
        i += 1;
    }
    line.to_string()
}

/// One heredoc redirection found on a line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HeredocOpener {
    /// The word a terminator line must equal to close the body.
    delim: String,
    /// True for the `<<-` form, whose terminator may be indented. Bash strips leading TABS ONLY —
    /// measured 2026-08-27, a SPACE-indented `END` stays BODY for `<<-` — whereas this (like the
    /// reference) accepts ANY indent. Unreachable: no `<<-` exists in any workflow or `.sh` here.
    dashed: bool,
}

/// The first heredoc redirection on a line that STARTS a body (`<<DELIM`, `<<'DELIM'`, `<<"DELIM"`,
/// `<<-DELIM`) — never a here-string (`<<<`), and never a `<<` that is **inside a quoted string**.
///
/// Hand-scanned rather than regex-matched: this crate deliberately carries no `regex` dependency.
/// Ported from `heredocOpener` in `src/lib/shellScriptLines.ts`, whose comment carries the measured
/// before/after for CPE-1936's N8 (`echo "use <<EOF to start a heredoc"` swallowing the rest of the
/// step) and the three shapes left deliberately open (`$(( a << b ))`, two heredocs on one line, a
/// partially quoted delimiter). The quote tracking here uses exactly the rules `strip_shell_comment`
/// above uses, so the two agree on where a quoted region begins and ends.
fn heredoc_opener(line: &str) -> Option<HeredocOpener> {
    let chars: Vec<char> = line.chars().collect();
    let mut quote: Option<char> = None;
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if let Some(q) = quote {
            if ch == '\\' && q == '"' && i + 1 < chars.len() {
                i += 2; // an escaped char inside a double-quoted string does not end the quote
                continue;
            }
            if ch == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if ch == '\\' && i + 1 < chars.len() {
            i += 2; // a backslash-escaped quote outside any quote is a literal char, not an opener
            continue;
        }
        if ch == '"' || ch == '\'' {
            let opens_here = match i.checked_sub(1).map(|p| chars[p]) {
                None => true,
                Some(prev) => !(prev.is_ascii_alphanumeric() || prev == '_'),
            };
            if opens_here {
                quote = Some(ch);
            }
            i += 1;
            continue;
        }
        if ch != '<' || chars.get(i + 1) != Some(&'<') {
            i += 1;
            continue;
        }
        let mut j = i + 2;
        if chars.get(j) == Some(&'<') {
            i = j + 1; // `<<<` is a here-STRING: no body, keep scanning past ALL THREE `<`
            continue;
        }
        let mut dashed = false;
        if chars.get(j) == Some(&'-') {
            dashed = true;
            j += 1;
        }
        while chars.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        let opener = match chars.get(j) {
            Some(&q @ ('\'' | '"')) => {
                j += 1;
                Some(q)
            }
            _ => None,
        };
        let start = j;
        if chars.get(j).is_some_and(|c| c.is_ascii_alphabetic() || *c == '_') {
            j += 1;
            while chars.get(j).is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_') {
                j += 1;
            }
            let ident: String = chars[start..j].iter().collect();
            // A quoted delimiter must be closed by the same quote, exactly as the reference's old
            // `\1` backreference required.
            let closed = match opener {
                None => true,
                Some(q) => chars.get(j) == Some(&q),
            };
            if closed {
                return Some(HeredocOpener { delim: ident, dashed });
            }
        }
        i += 2;
    }
    None
}

/// The count of leading spaces/tabs on a physical line.
fn leading_indent(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

/// True when `raw` is the terminator line for an open heredoc.
///
/// Ported from `closesHeredoc` in `src/lib/shellScriptLines.ts`; that comment carries CPE-1936's N7
/// measurement (an INDENTED line closing a plain `<<EOF` early, so heredoc BODY lines were scanned as
/// live code) and the reason the indentation rule is bash's *relative to the opener's own indent*
/// rather than to column 0: this module is handed whole `.yml` FILES, where `release-sidecar.yml`'s
/// `cat > "$notes_file" <<'EOF'` and its `EOF` both sit ten spaces in, and a column-0 rule would leave
/// that heredoc open for the rest of the file and empty the scan.
fn closes_heredoc(raw: &str, opener: &HeredocOpener, opener_indent: usize) -> bool {
    let body = raw.strip_suffix('\r').unwrap_or(raw);
    let content = body.trim_start_matches([' ', '\t']);
    if content.trim_end() != opener.delim {
        return false;
    }
    opener.dashed || leading_indent(body) <= opener_indent
}

/// Splits a workflow (or a single `run:` script) into LOGICAL shell lines: backslash continuations
/// joined, `#` comments stripped, heredoc BODIES skipped entirely.
///
/// Without the continuation join, ordinary multi-line shell formatting — a flag and its value split
/// across a `\` — evades any scan requiring both on the same physical line. Without the heredoc skip,
/// a body line crafted to look exactly like a real invocation is scanned as if it were one, though a
/// heredoc body is inert data being fed to a command, never a separately-executed statement.
///
/// Ported from `logicalLines` in `src/lib/shellScriptLines.ts`.
pub fn logical_lines(run: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut pending = String::new();
    let mut heredoc: Option<(HeredocOpener, usize)> = None;
    for raw in run.split('\n') {
        if let Some((opener, indent)) = &heredoc {
            if closes_heredoc(raw, opener, *indent) {
                heredoc = None;
            }
            continue; // heredoc body (and its terminator) -- data, not a shell statement
        }
        let line = strip_shell_comment(raw).trim().to_string();
        // The opener's OWN indentation is measured on the raw physical line, not the trimmed one --
        // `closes_heredoc` compares the terminator's indent against it.
        if let Some(opener) = heredoc_opener(&line) {
            heredoc = Some((opener, leading_indent(raw)));
        }
        if let Some(head) = line.strip_suffix('\\') {
            pending.push_str(head.trim());
            pending.push(' ');
            continue;
        }
        let joined = format!("{pending}{line}").trim().to_string();
        if !joined.is_empty() {
            out.push(joined);
        }
        pending.clear();
    }
    if !pending.trim().is_empty() {
        out.push(pending.trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The shared case file the TypeScript reference implementation is also run against. Reading it
    /// here is what makes "faithful port" a checked fact rather than a provenance claim (CPE-1933).
    fn shared_cases() -> Vec<serde_json::Value> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("src")
            .join("lib")
            .join("shellScriptLines.cases.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let cases: Vec<serde_json::Value> = serde_json::from_str(&text).expect("cases json");
        assert!(
            cases.len() >= 8,
            "the shared shell-line case file came back with only {} cases. An empty or truncated \
             fixture would let this port agree with the reference vacuously (CPE-1932: enumerate, \
             don't recall).",
            cases.len()
        );
        cases
    }

    #[test]
    fn the_port_matches_the_typescript_reference_on_every_shared_case() {
        for case in shared_cases() {
            let name = case["name"].as_str().expect("case name");
            let input = case["input"].as_str().expect("case input");
            let expected: Vec<String> = case["expected"]
                .as_array()
                .expect("case expected")
                .iter()
                .map(|v| v.as_str().expect("expected line").to_string())
                .collect();
            assert_eq!(
                logical_lines(input),
                expected,
                "the Rust port of shellScriptLines.ts disagrees with the reference on case {name:?}. \
                 Both implementations are run against src/lib/shellScriptLines.cases.json precisely \
                 so they cannot drift apart (CPE-1933)."
            );
        }
    }

    /// The specific input CPE-1933's first draft passed and should not have.
    #[test]
    fn a_trailing_comment_cannot_smuggle_a_flag_back_into_a_scan() {
        let line =
            r#"  --expect-url-prefix "https://example.com/${TAG}/"  # was: --expect-channel sidecar"#;
        let stripped = strip_shell_comment(line);
        assert!(
            !stripped.contains("--expect-channel"),
            "a flag quoted inside a TRAILING comment must not survive stripping; got {stripped:?}"
        );
        assert!(stripped.contains("--expect-url-prefix"), "the real flag must survive");
    }

    #[test]
    fn a_literal_hash_inside_an_argument_is_not_treated_as_a_comment() {
        let line = r#"curl "https://example.com/page#anchor" --fail"#;
        assert_eq!(strip_shell_comment(line), line, "a quoted # is data, not a comment");
        let unquoted = "echo abc#def";
        assert_eq!(strip_shell_comment(unquoted), unquoted, "a # mid-word does not start a comment");
    }

    fn opener(delim: &str, dashed: bool) -> Option<HeredocOpener> {
        Some(HeredocOpener { delim: delim.to_string(), dashed })
    }

    #[test]
    fn a_here_string_does_not_open_a_heredoc_body() {
        // `done <<< "$names"` in release-sidecar.yml: a here-STRING. Treating it as a heredoc would
        // swallow every following line, silently emptying the scan.
        assert_eq!(heredoc_opener(r#"done <<< "$names""#), None);
        assert_eq!(heredoc_opener(r#"cat > "$f" <<'EOF'"#), opener("EOF", false));
        assert_eq!(heredoc_opener("cat <<-END"), opener("END", true));
    }

    /// CPE-1936 N8. `ffmpeg-pin-freshness.yml` really does write GitHub multi-line outputs this way,
    /// and the whole-file consumers scan that workflow, so this is a live shape rather than a latent
    /// one: before the fix everything after such a line dropped out of the scan entirely.
    #[test]
    fn a_heredoc_token_inside_a_quoted_string_opens_no_body() {
        assert_eq!(heredoc_opener(r#"echo "use <<EOF to start a heredoc""#), None);
        assert_eq!(heredoc_opener(r#"echo 'see <<EOF for details'"#), None);
        assert_eq!(heredoc_opener(r#"echo "failures<<PINFAIL_EOF" >> "$GITHUB_OUTPUT""#), None);
        // ...but a REAL heredoc later on the same line still opens.
        assert_eq!(heredoc_opener(r#"echo "a <<NOPE" && cat <<EOF"#), opener("EOF", false));
    }

    /// CPE-1936 N7. Real bash wants the delimiter alone on its own line; only `<<-` tolerates
    /// indentation. Before the fix an indented `EOF` closed a plain `<<EOF`, and the heredoc BODY was
    /// then scanned as live code.
    #[test]
    fn an_indented_terminator_closes_only_the_dash_form() {
        let plain = HeredocOpener { delim: "EOF".to_string(), dashed: false };
        assert!(closes_heredoc("EOF", &plain, 0));
        assert!(!closes_heredoc("  EOF", &plain, 0));
        assert!(!closes_heredoc("EOF # still body", &plain, 0));
        // Relative, not column 0: a whole `.yml` file indents the entire script.
        assert!(closes_heredoc("          EOF", &plain, 10));
        assert!(!closes_heredoc("            EOF", &plain, 10));
        let dashed = HeredocOpener { delim: "END".to_string(), dashed: true };
        assert!(closes_heredoc("\t\tEND", &dashed, 0));
    }

    #[test]
    fn a_heredoc_body_is_never_scanned_as_a_shell_statement() {
        let script = "cat <<'EOF'\n  cargo run --bin verify-release-artifacts -- --expect-channel sidecar\nEOF\necho done";
        assert_eq!(logical_lines(script), vec!["cat <<'EOF'".to_string(), "echo done".to_string()]);
    }
}
