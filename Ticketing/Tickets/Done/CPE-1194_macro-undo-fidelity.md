---
id: CPE-1194
title: "Macro undo fidelity: trash-based convert restore + snapshot-based tag inverse"
type: chore
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-31
epic: CPE-739
estimate: 1-2h
closed: 2026-08-01
---

## Summary
Two undo-fidelity gaps from the CPE-1187/1188 review (PR #498), now documented in code, to be truly fixed here:

1. **Convert undo is a lossy re-encode.** `macro_convert_in_place` deletes the original, so undo re-encodes back
   (quality loss), not a byte-exact restore. Route the original to the OS trash on convert so undo can restore
   the real bytes.
2. **Tag inverse can drop a pre-existing tag.** `untag` on undo removes the label regardless of whether the
   user had it before the run. Snapshot the pre-run tag state and restore exactly on undo.

## Acceptance Criteria
- [x] Convert-undo restores the original bytes (via trash), not a re-encode; `cargo test` proves byte-equality.
- [x] Tag-undo restores the exact pre-run tag set (a pre-existing label survives undo).
- [x] `cargo test` + clippy green.

## Resolution
Both fidelity gaps fixed in the macro undo path (`crates/server/src/macro_run.rs` +
`src-tauri/src/lib.rs`, no frontend changes):

1. **Convert undo now restores via trash, byte-exact.** `macro_convert_in_place` (lib.rs) routes the
   pre-convert original to the OS trash (`trash::delete`) instead of `fs::remove_file`. The pure
   `macro_run::resolve` (cpe-server) now always emits a dedicated `"restore_convert"` inverse kind for
   a `convert` step (was reusing the forward `"convert"` kind) — structural, not data-dependent, so
   decided in the pure resolver. A new `macro_restore_converted` (lib.rs) handles that kind: restores
   the original's exact bytes from the trash via the existing `restore_from_trash_impl`, then trashes
   the now-redundant converted file too (never a silent permanent delete on an undo path, per project
   convention). Gated to Windows/Linux in the regression test — same platform limit as the existing
   `restore_from_trash` (CPE-044): `trash::os_limited` has no macOS listing/restore API, so macOS
   honestly errors on that inverse rather than pretending to restore.
2. **Tag undo now snapshots pre-run state.** `macro_run::resolve` still always emits `"untag"` as a tag
   step's inverse (it's pure, no filesystem access, so it can't know pre-run state). The apply layer,
   `macro_apply_run` (lib.rs), now snapshots whether the label was already present at the path
   immediately before applying each `tag` op (`macro_tag_already_present`, backed by
   `cpe_server::tags::load`) and, when it was, rewrites that op's `InverseOp::kind` in the returned
   `ResolvedRun` from `"untag"` to `"tag"` — re-asserting an already-present label is a no-op restore,
   so undo never strips a tag the user had before the run. An unrelated, untouched tag was already safe
   under the old code (the removal always filtered to the exact label); the fix specifically covers the
   case where the macro's own label was already there.

Regression tests added (`src-tauri/src/lib.rs`, `mod tests`):
- `macro_run_convert_step_then_undo_restores_the_original_bytes_via_trash` (Windows/Linux only) — runs
  a real `Convert{to_ext:"jpg"}` macro step against a real PNG on disk, undoes it, and asserts the
  restored bytes are byte-for-byte identical to the pre-run original (a lossy re-encode would fail
  this). Also asserts the converted file is gone (trashed) after undo.
- `macro_run_tag_step_preserves_a_pre_existing_tag_after_undo` — pre-tags a path with the SAME label
  the macro attaches (plus an unrelated label), runs the macro, asserts the corrected inverse kind is
  `"tag"` not `"untag"`, undoes, and asserts both labels survive.
- `macro_run_tag_step_with_no_pre_existing_tag_still_untags_on_undo` — confirms the un-corrected
  CPE-1187 behavior still holds when the label was genuinely new: inverse stays `"untag"` and undo
  fully removes the tag entry.

Verified: `cargo test -p cpe-server` (1160 passed), `cd src-tauri && cargo test` (88 passed, incl. all
5 macro tests), `cargo clippy --all-targets -- -D warnings` clean in all four checked feature
combinations (src-tauri default + `sidecar-platform`; cpe-server default + `index`). No frontend
touched, so `npm run check`/`npm test` not required. No specta struct shape changed (only doc comments
and runtime string values), confirmed by the `typed_bindings_are_committed_and_routed_through_busy_cursor`
test staying green with the committed `bindings.gen.ts` untouched.

## Work Log
- 2026-07-31 — Filed by Foreman (sprint) from the PR #498 review findings 3a/3b. The rollback-honesty blocker
  (finding 2) was fixed inline in #498; these two fidelity improvements are the follow-up.
- 2026-08-01 — Picked up. Estimate: 1-2h. Plan: dedicated `"restore_convert"` inverse kind (trash-based
  restore) for convert; apply-time tag-presence snapshot correcting the tag inverse kind for tag.
- 2026-08-01 — Implemented both fixes, added 3 regression tests exercising real trash I/O and real tag
  snapshotting (not simulated). `cargo test` + clippy green in every checked mode. Closing as Done.
