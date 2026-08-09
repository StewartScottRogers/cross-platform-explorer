---
id: CPE-1013
title: Fix mojibake in macro rename templates with non-ASCII literal text
type: bug
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
closed: 2026-07-24
epic: CPE-739
estimate: 30m
---

## Summary
Found by the 2026-07-24 sprint bug-audit. In `crates/server/src/action_macro.rs`, `expand_template`
scanned the template byte-by-byte and its literal-copy fallback (line ~194) did `out.push(bytes[i] as char)`.
Casting a raw UTF-8 byte to `char` is a Latin-1 decode, so any **non-ASCII character in a rename template's
literal text** was corrupted into mojibake — one byte at a time.

**Confirmed repro:** template `"café_{n}.txt"`, `n=1` → `"cafÃ©_1.txt"` (the `é` bytes `C3 A9` became `Ã©`).
Token substitutions (`{name}`/`{stem}`/`{ext}`/`{n}`) were fine; only literal template text between tokens was
corrupted. This reached **real file-rename operations** via `plan()`.

## Fix (shipped, PR #337)
Literal-copy branch now consumes a whole UTF-8 char:
```rust
let ch = template[i..].chars().next().unwrap();
out.push(ch);
i += ch.len_utf8();
```
The `{`/`}` token-scan stays byte-based (both are single-byte ASCII, so those indices/slices remain on char
boundaries).

## Acceptance Criteria
- [x] Non-ASCII literal template text round-trips correctly — 4 regression tests added (accented `café_{n}`,
      CJK `目录_{n}`, emoji `📁_{n}`, and a mixed-token case).
- [x] Existing `action_macro` tests still pass; ASCII-template behaviour unchanged.
- [x] `cargo test -p cpe-server action_macro` green (18/18); clippy clean both feature modes; no new deps.

## Work Log
2026-07-24 (sprint) — Diagnosed by the audit researcher, fixed by a worker, independently reviewed
(APPROVE — full suite 561/561, clippy both modes clean, no scope creep). Merged in PR #337.
