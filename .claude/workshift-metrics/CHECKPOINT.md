# Workshift Checkpoint

## RUN 2026-08-07 (CLI, "Do 3 workshifts") — SHIFT WRAPPED: 8 PRs merged, feature well DRY, needs user
**State:** `main` @ origin `ee16d93e` (clean, 0 worktrees). App last built + running = v0.57.61-sidecar
(media player). The 8 PRs below are code-merged but NOT yet in a fresh installed build — a rebuild is the
natural next step whenever the user wants to exercise them live.

### Shipped this run (8 PRs / 11 tickets)
- CPE-1432 pane-aware Space quick-look (#701) · CPE-1415 sevenz catch_unwind mitigation (#702) ·
  CPE-1427/1428 cert-create RSA-4096 test + cert-sign hardening (#703)
- **EPIC CPE-1433 structured previews CLOSED** — CPE-1434 .eml (#704), CPE-1435 .ics + CPE-1436 .vcf (#705):
  hand-rolled zero-dep parsers, HTML sanitized to text (no remote loads), vCard PHOTO presence-only, panic batteries.
- CPE-1438 dual-pane crypto Inspect overlay (#706) — Inspect now works in dual-pane (was a no-op).
- CPE-1440/1441 security dep bumps (#707) — quick-xml High DoS (RUSTSEC-2026-0194/0195 via calamine 0.26→0.36)
  + dompurify XSS (3.4.13), fixed in BOTH lockfiles incl. src-tauri (shipped app).

### Parked / deferred / blocked (all documented, warm for pickup)
- **CPE-1414** (Deferred) — SVG use-cycle guard; 3-attempt circuit-breaker; low-risk (256KB-probe only, prod
  safe); PR #700 left as DRAFT with the sound roxmltree base; exact ~2-line remaining fix documented
  (xlink:href-first precedence + svgtypes::IRI fragment parse to mirror usvg's resolve_href).
- CPE-1437 (Deferred) SVG deep-acyclic use-chain small-stack overflow · CPE-1439 (Backlog) archive-ext provider
  gap (verify backend first) · CPE-1442 (Blocked) rsa Marvin, await rsa 0.10 · CPE-1443 (Deferred) dev-toolchain
  svelte4→5/vite5→8/vitest2→4 migration (dev-only advisories, big-design).

### FRONTIER — the clean headless FEATURE well is DRY (verified, not assumed)
Two independent opus scouts + the drained queue (Backlog empty of features, Doing empty, Blocked = user-gated)
confirm it. The remaining headless work is only the low-value CPE-1414/1437 SVG hardening. **The next real
increment needs the USER:** attended GUI verification punch-list (the 8 merged PRs), macOS (no tauri-driver
path), signing cert (CPE-002), a live agent session for the cost-ledger tab (CPE-1098), the 3D-model viewer
(CPE-118), real-network E2E (CPE-819/820), or a fresh user-directed feature/epic.

**To resume:** a fresh session should NOT hunt for more headless features (verified dry — don't re-scout, read
Library [[structured-preview-runway-2026-08-07]]). Instead: (a) rebuild v0.57.62-sidecar so the user can
exercise the 8 merged PRs live, and/or (b) wait for the user to name an attended epic / provide a resource /
give direction. Board clean + green. Lock released.
