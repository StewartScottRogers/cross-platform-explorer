---
id: CPE-1328
title: "Checkpoint 'saved' status must reflect real success (fix swallowed Err(String))"
type: bug
component: frontend
priority: medium
status: Done
tags: ready
created: 2026-08-05
epic: CPE-725
---

## Summary
Two independent reviewers (on PR #620 near-dup cleanup and PR #621 metadata checkpoint) flagged the same
pre-existing pattern: `await commands.checkpointCreate(...)` is not unwrapped, and per `bindings.gen.ts` the
generated wrapper only *throws* when the rejection is an `Error` instance. A Rust-side `Err(String)` (typical
for a Tauri `Result<T, String>` command) rejects with a plain string, which the wrapper turns into
`{status:"error"}` **without throwing** — so the `catch` never fires, `checkpointed` becomes `true`, and the
UI shows "(checkpoint saved)" even when the checkpoint genuinely failed at the domain level. The behaviour is
still best-effort/non-blocking (the save/delete proceeds either way), but the *status text can lie*.

## Build
- Make the checkpoint call treat a `{status:"error"}` result the same as a thrown error for the purpose of the
  "checkpoint saved" indication. Options (pick the cleanest, match existing conventions):
  - unwrap the result and check `status === "ok"` before setting the `checkpointed`/`saved`-suffix flag; or
  - inspect the returned `Result` envelope explicitly.
- Fix it in **both** call sites that share the pattern: `src/lib/components/MetadataStudioDialog.svelte`
  (CPE-1325) and `src/lib/components/SimilarImagesDialog.svelte` (the original). If CPE-1324's
  `NearDuplicatesDialog.svelte` copied the same un-unwrapped pattern, fix it there too.
- Keep it non-blocking: a failed checkpoint must still let the save/delete proceed — only the *status
  indication* changes (don't claim a checkpoint that didn't happen).
- Add the success-suffix test that PR #621 UAT noted was missing: assert the "(checkpoint saved)" suffix shows
  on real success AND does NOT show when the checkpoint returns an error envelope.

## Acceptance criteria
- A checkpoint that returns an error envelope (not just a thrown `Error`) no longer shows "(checkpoint saved)".
- The save/delete still proceeds on checkpoint failure (non-blocking preserved).
- All three dialogs (Metadata, SimilarImages, NearDuplicates if applicable) share the corrected behaviour.
- Tests assert both the truthful-success and truthful-failure suffix/indication. `npm run check` clean; no new
  deps.

## Notes
- FRONTEND-ONLY — merge on the Frontend CI job.
- Touches `MetadataStudioDialog.svelte` (serialize behind CPE-1326/1327) + `SimilarImagesDialog.svelte` +
  possibly `NearDuplicatesDialog.svelte`. Source: reviewer findings on #620/#621, 2026-08-05.
