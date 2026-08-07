---
id: CPE-1386
title: "Dual-pane: archive/compress/extract/secure-vault ops aren't available (pane-routed) from a pane-B menu"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-06
---

## Problem (CPE-1384 follow-up — deferred by design)

CPE-1384 routed duplicate/batch-rename/batch-media/copy-to/move-to for pane B, but deferred the archive
family — **compress, extract, archive-safety, secure-vault (create/shred)**. These are currently hidden from a
pane-B context menu (gated off via `!ctxInPaneB`, so **no wrong-pane risk today** — they simply aren't offered
in pane B), which is safe but incomplete parity. They were deferred because they queue through the global
`pendingArchiveOps` / `transfer://done` completion machinery, whose async completion callbacks are keyed by
transfer id and carry **no pane context** — so pane-routing them needs that queue to thread the originating
pane through to the completion callback (a bigger refactor than CPE-1384's scope). Secure-vault-create also has
an optional destructive shred-original path, so it needs the CPE-1370 snapshot-safe capture.

## Fix direction

Thread the originating pane (`inPaneB` snapshot captured at menu-open) through `pendingArchiveOps`/the
transfer-queue completion path so an archive/extract/vault op invoked from pane B operates on pane B's
selection + target folder and refreshes the correct pane(s) on completion (reuse the
`refreshPasteAffectedPanes` both-panes pattern). Apply the `snapshotConfirmTarget` safety model to the
vault-shred destructive path. Then un-hide these rows for pane B. Touches `src/App.svelte` +
`src/lib/components/ContextMenu.svelte` + the transfer-queue/pendingArchiveOps plumbing. Add per-op pane-B
routing tests incl. snapshot-safety for the destructive vault-shred path.
