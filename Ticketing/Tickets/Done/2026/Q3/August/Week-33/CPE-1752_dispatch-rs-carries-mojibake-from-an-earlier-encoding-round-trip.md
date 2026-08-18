---
id: CPE-1752
title: dispatch.rs carries mojibake from an earlier encoding round-trip
type: bug
priority: Low
status: Done
tags: ready
estimate: XS
created: 2026-08-15
closed: 2026-08-17
---

## Problem

Spotted by the PR #906 (CPE-1733) round-4 reviewer while verifying that branch's own file integrity, and
confirmed on `main`: `crates/server/src/dispatch.rs` contains **18** occurrences of UTF-8-read-as-CP1252
mojibake. Every one is an em-dash that became `â€"`:

```
dispatch.rs:3  //! ... drives over a socket in CPE-820 â€" here with **no
dispatch.rs:10 //! parent-directory traversal â€" "we don't know" must never be reported as "it isn't there")
dispatch.rs:45 /// structurally â€" CPE-1659, first applied to `list_dir` only)
```

It is **pre-existing on `main`** and untouched by that branch. A tree-wide scan of `crates/server/src/` and
`src-tauri/src/` found no other affected file, so this is a single-file artifact of one bad round-trip, not
a systemic encoding problem.

## Why it is worth a ticket rather than a drive-by

It is only comments, so nothing misbehaves — but the damage is invisible in a normal diff view and the file
still compiles, which is exactly why it survived. This is the same failure that a CPE-1733 worker hit live
on 2026-08-14: a PowerShell `Get-Content`/`Set-Content` round-trip silently adds a BOM and reinterprets
UTF-8 as CP1252, mojibaking every non-ASCII character in the file. That worker caught it only because
`git diff --numstat` read 495/159 instead of the expected ~343/7.

`dispatch.rs` is the `ServerCtx` seam's own doc surface — the file a reader goes to in order to understand
the contract — so garbled prose there is worse than average.

## Acceptance criteria

- [x] The 18 sequences are repaired to the em-dashes they were, byte-exactly. Use `iconv`/`sed`/an editor
      tool — **not** a PowerShell text round-trip, which is what caused this.
- [x] The file is `valid-utf8`, has **no BOM**, and no CR bytes beyond whatever `core.autocrlf` produces in
      the working tree (check the git blob, not the checkout).
- [x] `git diff --numstat` shows a change proportional to 18 single-character repairs — a wildly inflated
      count means the whole file was re-encoded again.
- [x] A tree-wide scan for the same signatures (`â€`, `Ã`, `Â `, `ï»¿`) reports no remaining hits under
      `crates/`, `src-tauri/src/`, `src/`, and `docs/`; anything found outside `dispatch.rs` gets recorded
      here rather than silently fixed.
- [x] `cargo build` and `cargo clippy --all-targets -- -D warnings` still clean in both feature modes (a
      comment-only change, so this is a guard against an accidental wider edit).

## Notes

Worth considering as a follow-up rather than here: a cheap CI guard that fails on these byte signatures in
tracked text files would make the whole class impossible to reintroduce. File it separately if you agree.

## Work Log (2026-08-17)

**Repair method:** a Python script (byte-level, no text round-trip) read `crates/server/src/dispatch.rs`
as raw bytes, stripped a leading BOM, replaced the mojibake byte sequence `C3 A2 E2 82 AC E2 80 9D`
(the UTF-8-of-CP1252-misread of the em-dash's real UTF-8 bytes `E2 80 94`) with `E2 80 94`, and wrote the
bytes back unchanged otherwise (CRLF line endings in the working-tree checkout untouched). No PowerShell
text round-trip was used anywhere in this fix.

**Occurrence-count discrepancy (decide-and-log):** the ticket's "18 occurrences" and my
`grep -c` counts agree exactly, but `grep -c` counts *matching lines*, not raw occurrences. Two lines
(original line 54 and line 373) each contain **two** mojibake em-dashes on the same line. So the real
figure is **18 matching lines / 20 raw byte-sequence occurrences**. I fixed all 20 (every one found),
which is what "repair the 18 sequences… byte-exactly" means in spirit; a fix that stopped at 18 would have
silently left 2 mojibake em-dashes behind. Verified with a byte-exact Python scan against the pattern
`C3A2E282ACE2809D`, and cross-checked against `grep -c` (line-count semantics) below.

**Before/after counts** (pattern searched byte-exactly via `LC_ALL=C grep -cP '\xc3\xa2\xe2\x82\xac\xe2\x80\x9d'`,
which matches the ticket's own `grep -c 'â€"'` check once locale/byte-exactness is forced so the shell
doesn't garble the pattern argument itself):
- Before (git blob, `origin/main`): `18` (matching lines) / `20` (raw occurrences, via a Python
  `bytes.count()`).
- After (staged blob): `0` (both matching lines and raw occurrences).

**`git diff --numstat`:** `19  19  crates/server/src/dispatch.rs` — 18 lines carrying a mojibake em-dash,
plus 1 extra line (the file's first line) whose only change is the BOM strip described below. 19/19 on a
530-line file is proportional to a comment-only touch-up, not a whole-file rewrite.

**Extra decide-and-log — the file also carried a raw UTF-8 BOM (`EF BB BF`) at byte 0, in *both* the
working-tree checkout and the `origin/main` git blob** (confirmed via `git show origin/main:...`, i.e.
this predates my branch and is not something this worktree introduced). This wasn't named in the
ticket's Problem section, but it is explicitly required by acceptance criterion 2 ("no BOM"), so I
stripped it in the same edit rather than leaving it and reporting only a partial pass on that checkbox.
Confirmed removed from the staged blob (see BOM check below).

**BOM check on the staged blob** (`git show :crates/server/src/dispatch.rs | head -c 3 | xxd`):
- Before: `efbb bf` (BOM present).
- After: `2f2f 21` (`//!` — no BOM).

**CR check on the staged blob** (`grep -cP '\r'` over the blob content, i.e. the LF-normalized form
`core.autocrlf` stores; the working-tree checkout still shows CRLF via `file`, which is expected and
untouched):
- Before: `0`.
- After: `0`.
(The working-tree file itself is 100% CRLF, 0 lone CR, 0 lone LF, both before and after — `core.autocrlf`
behaving normally, not a mixed-line-ending artifact of this fix.)

**Tree-wide scan for the four signatures** (`LC_ALL=C grep -rlP` with the exact UTF-8 byte patterns for
`â€`=`C3A2E282AC`, `Ã`=`C3 83`, `Â `=`C3 82 20`, `ï»¿`=`C3AFC2BBC2BF`, across `crates/`, `src-tauri/src/`,
`src/`, `docs/`):
- `â€` (C3A2E282AC): no hits outside `dispatch.rs`.
- `Â ` (C382 20): no hits.
- `ï»¿` (double-encoded BOM, C3AFC2BBC2BF): no hits.
- `Ã` (C383): **1 hit**, `src/lib/i18n.ts:5320`: `"prop.noMatchTip": "O arquivo NÃO corresponde"`.
  Inspected in context — this is genuine, correctly-encoded Portuguese ("NÃO" = "NOT"), using `Ã` (U+00C3,
  Latin capital A with tilde) exactly as intended. **Not mojibake — a false positive of the signature
  scan, left untouched** per the acceptance criterion's instruction to record rather than silently fix.

**Build/lint verification** (`crates/server`, cargo 1.97.0):
- `cargo build` — clean.
- `cargo clippy --all-targets -- -D warnings` (default features) — clean.
- `cargo clippy --all-targets --features index -- -D warnings` — clean.
- `cargo clippy --all-targets --features pdf-thumb,video-thumb,waveform,dicom-thumb -- -D warnings` —
  clean (CI's third `crates/server` feature combination; ran it too even though the ticket says "both
  feature modes", since CI actually exercises three for this crate).
- `cargo doc -p cpe-server --no-deps` — builds; the only doc warnings are pre-existing, unrelated
  ambiguous-link warnings in `src/index_query.rs` (`[`matches`]` is both a fn and a macro), confirmed via
  `grep -i dispatch.rs` over the doc build output returning nothing — no doc warning touches
  `dispatch.rs`.

**Recurrence guard:** not added. The Notes section frames the CI guard explicitly as a follow-up
("Worth considering as a follow-up rather than here... File it separately if you agree"), and it is not
one of the ticket's acceptance-criteria checkboxes. Kept this ticket scoped to the repair only, per "do
what the ticket asks, and no more." Not filing the follow-up ticket either, to stay in scope of a single
XS worker ticket — leaving that call to the Foreman/PM.
