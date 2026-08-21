---
id: CPE-1783
title: dispatch.rs still carries three mojibake arrows CPE-1752 missed
type: bug
priority: Low
status: Backlog
tags: ready
estimate: XS
created: 2026-08-19
closed:
---

## Problem

Found while building the whole-repo mojibake guard for **CPE-1771**. The new guard
(`src/lib/mojibakeGuard.ts` / `mojibakeGuard.test.ts`) scans the tree for the same UTF-8-read-as-CP1252
signature CPE-1752 repaired in this exact file, and it still finds three live occurrences:

`crates/server/src/dispatch.rs`:

```
6:  //! Error taxonomy at the boundary: an unknown method â†’ [`ErrorCode::NotFound`], params that don't
7:  //! deserialize â†’ [`ErrorCode::BadRequest`]; a domain `Err(String)` from a **path-taking** handler goes
...
11: //! domain `Err(String)` â†’ [`ErrorCode::Internal`] via [`domain`]. A handler never panics the dispatcher.
```

Each `â†’` is the mojibake form of a rightwards arrow `→` (U+2192 read as CP1252, same failure mode as the
em-dash/ellipsis corruption CPE-1752 and CPE-1771 repaired elsewhere) — real corruption in a doc comment,
not a deliberate illustration. CPE-1752's own repair pass on this file apparently didn't catch these three.

CPE-1771 could not fix this directly: `crates/` was off-limits during that ticket's sprint slot (a
concurrent worker was live in it), so the guard allowlists these three exact lines with a reason pointing
at this ticket, to keep the guard's `main` result honest without blocking on file ownership. Closing this
ticket should **remove that allowlist entry**, not just fix the arrows.

## What to do

- Replace all three `â†’` occurrences in `crates/server/src/dispatch.rs` (lines 6, 7, 11) with a real `→`.
  Byte-exact edit — no PowerShell text round-trip (that is the root cause of this whole class of bug).
  Verify with `git diff --numstat`: expect ~3 changed lines, not a whole-file rewrite.
- Remove the three `crates/server/src/dispatch.rs` entries from `MOJIBAKE_ALLOWLIST` in
  `src/lib/mojibakeGuard.ts` (added by CPE-1771) once the arrows are repaired, and confirm
  `mojibakeGuard.test.ts` still passes with the allowlist entries gone (i.e. the guard now finds the file
  clean on its own, not because it's excused).

## Acceptance criteria

- [ ] The three `â†’` occurrences are repaired to `→`, verified against the git blob.
- [ ] `git diff --numstat` on `dispatch.rs` shows ~3 changed lines.
- [ ] The three matching entries are removed from `MOJIBAKE_ALLOWLIST` in `src/lib/mojibakeGuard.ts`.
- [ ] `npm run test -- mojibakeGuard` (or the full suite) still passes after the allowlist entries are
      removed.
- [ ] `cargo build` / `cargo test` for `crates/server` still pass (comment-only change, but confirm).

## Notes

Found by the mojibake guard built for **CPE-1771**, 2026-08-19. Related: CPE-1752 (the original
`dispatch.rs` repair, which evidently missed these three lines), CPE-1771 (the guard that found them and
allowlists them pending this ticket).

## Work Log

2026-08-20 17:37 UTC — Fixed all three mojibake arrows in dispatch.rs (lines 6, 7, 11) by replacing byte-exact UTF-8 sequences. Removed the corresponding three entries from MOJIBAKE_ALLOWLIST in mojibakeGuard.test.ts. All gates passed: cargo clippy --all-targets (-D warnings), cargo test (crates/server), and npx vitest run src/lib/mojibakeGuard.test.ts (42 tests, all passed).
