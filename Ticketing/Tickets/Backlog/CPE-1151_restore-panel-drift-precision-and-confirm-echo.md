---
id: CPE-1151
title: "Restore panel: session-precise drift + echo drift count inside the revert confirm"
type: feature
component: Frontend
priority: high
status: Backlog
tags: ready
created: 2026-07-30
epic: CPE-732
---

## Summary
Direct outcome of the user's GUI verify of CPE-1126 (2026-07-30). The user looked at the restore-panel
revert-confirm and asked for two concrete refinements:

1. **Precise drift** — today the restore panel calls `checkpoint_preview_revert` with **no session**, so the
   backend's conservative fallback surfaces *every* path that diverges from the checkpoint as "drift" —
   including the watched agent's OWN expected edits. Make it precise: pass the watched session so only
   changes made **outside** that agent are counted as drift.
2. **Echo drift in the confirm** — the drift count/list currently sits in the preview area ABOVE the red
   "Yes, revert" confirm box. Restate the drift inside the confirm box so a revert that would clobber
   drifted work makes that unmissable at the moment of the destructive click.

## Background (the seam already exists)
- `cpe_server::checkpoint_store::checkpoint_preview_revert(&ctx, root, manifest_id, session: Option<..>)`
  ALREADY implements the precise path: `Some(session)` folds the agent's touched-set (from the audit
  journal) so only paths outside it are drift; `None` = today's conservative "every diverging path is
  drift". (See the doc comment on `RevertPreview` / the fn in `crates/server/src/checkpoint_store.rs`.)
- The **Tauri command** `checkpoint_preview_revert` in `src-tauri/src/lib.rs` (~line 2589) currently hardcodes
  `None` with a comment naming CPE-1126 as the reason. So part 1 is: expose an optional `session` param on the
  command and pass it through.
- The frontend restore panel lives in `src/lib/components/AgentTimeline.svelte`; that component already has a
  `sessionId` prop (the watched session) it can pass. The typed client is `commands.checkpointPreviewRevert`.

## Acceptance Criteria
- [ ] **Command:** `checkpoint_preview_revert` (src-tauri/src/lib.rs) gains an optional `session: Option<String>`
      param and passes it to the domain fn instead of hardcoded `None`. The `#[specta]` binding is regenerated
      (`cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`) so `bindings.gen.ts`
      is in sync (CI's drift guard must pass). `CheckpointDialog.svelte`'s existing call keeps working (pass
      `null`/omit — no behaviour change there unless trivially wired).
- [ ] **Precise drift wiring:** `AgentTimeline.svelte`'s restore panel passes its watched `sessionId` (or
      `null` when none) to `checkpointPreviewRevert`, so drift excludes the agent's own expected edits. When
      there is no watched session, behaviour is unchanged (conservative).
- [ ] **Echo drift in the confirm:** when `drift_count > 0`, the two-step revert confirm box itself states the
      drift (e.g. "N file(s) changed since this checkpoint will be lost"), in addition to the existing plan
      preview. Theme vars only; reflow; the existing two-step gate + "cannot be undone" wording is preserved
      (do NOT weaken the safety gate — CPE-1150's tests must stay green).
- [ ] Tests: a unit/component test asserts the drift-echo appears in the confirm only when `drift_count > 0`;
      the existing `checkpointMarkers` + confirm-gate tests still pass. `npm run check` green;
      `cargo clippy --all-targets --features sidecar-platform -- -D warnings` clean; relevant `cargo test`
      for `checkpoint_store` still green.

## Notes
- Epic CPE-732. This is the refinement pass that lets CPE-1126's "confirm-to-revert is safe/clear" AC close.
- Backend command-signature change ⇒ **bindings regen is mandatory** (see [[regen-specta-bindings-on-struct-change]]).
- Keep the plain-explorer delete-test intact: all checkpoint code stays `sidecar-platform`-gated.
