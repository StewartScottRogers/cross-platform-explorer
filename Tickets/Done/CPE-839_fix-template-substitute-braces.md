---
id: CPE-839
title: "Bug: folder-template substitute swallows {tokens} inside braced content (code files)"
type: bug
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-740
---

## Summary
Found during manual testing of CPE-835 (folder templates). `folder_template::substitute` failed to
replace a `{token}` when it sat inside other braces — e.g. a captured source file
`fn main() { println!("{name}"); }` stamped out **unchanged** (still `{name}`), while a plain
`# {name}` in a README substituted correctly.

Root cause: the original scanner found the first `{`, took everything up to the next `}` as one "token",
and — not finding that whole span in the variable map — emitted it verbatim. For a code file the first
`{` is the function's opening brace, so the entire body (including the real `{name}` inside) was treated
as one unknown token and passed through untouched.

## Fix
A **token** is now strictly `{` + an identifier (`[A-Za-z0-9_]+`) + `}`. Any other `{` — a code brace, a
`{` followed by a space, an unmatched brace — is literal. So tokens embedded in braced content are
substituted and stray braces are preserved.

## Acceptance Criteria
- [x] `substitute("fn main() { println!(\"{name}\"); }", {name:Acme})` → `fn main() { println!("Acme"); }`.
- [x] Non-identifier `{...}` (e.g. `{a-b}`, `{ name }`) and code braces (`{}`) are left verbatim.
- [x] Existing behaviour preserved: `{name}` replaced, unknown `{token}` verbatim, dangling `{` literal.
- [x] Regression test added; `cargo test` green in `crates/server`; `cargo clippy --all-targets -D warnings` clean.

## Resolution
Rewrote `folder_template::substitute` to scan for `{`, read a run of identifier chars, and only treat it
as a token when immediately closed by `}`. Added `substitute_handles_tokens_inside_code_braces` covering
the code-file case, brace-then-space, non-identifier `{a-b}`, and `{ name }`. `crates/server` suite green
(the folder_template group went 9 → 10 tests); clippy `--all-targets -D warnings` clean.

Surfaced by the CPE-838 manual-test harness (`template_demo`) — exactly the kind of contents-with-braces
case the CPE-835 unit tests (clean `{name}` only) didn't cover.

## Work Log
- 2026-07-21 — Found via `template_demo` manual test: a captured `main.rs` stamped with `{name}` intact.
  Root-caused the greedy `{…}` span; rewrote `substitute` to identifier-only tokens + regression test.
  Fix verified; closing.
