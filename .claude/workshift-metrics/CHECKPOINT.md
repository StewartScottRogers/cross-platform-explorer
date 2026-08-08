# Workshift Checkpoint

## RUN 2026-08-07→08 (CLI, "run 12 workshifts in batches") — SHIFT WRAPPED: 8 PRs merged, SVG DoS class closed, well dry
**State:** `main` @ origin `ec7da796` (clean, 0 worktrees, Backlog EMPTY, Doing empty). Lock released. The 8 PRs below are
code-merged but NOT yet in a fresh installed build — a rebuild is the natural next step whenever the user wants to exercise
them live (though these are backend hardening fixes with no new user-facing surface, so a GUI verify is optional).

### Shipped this run (8 PRs / 9 tickets — all backend security/hygiene hardening)
- CPE-1439 archive-ext preview routing (#708) — xz/bz2/zst/lz/lzma → "compressed file" info preview (no decode), dmg/cab won't-fix.
- **EPIC-SIZED: SVG stack-overflow DoS class CLOSED** — CPE-1444 (#712) bounds the reference hops×nesting product for
  mask/pattern/marker/filter (multiplicative) + a 16MiB guaranteed-stack render; CPE-1445 (#713) bounds SVGZ gzip decompression
  (32MiB `.take`) + rejects double-gzip so usvg can't re-inflate. Supersedes the parked CPE-1437 (moved to Done). Verified by an
  opus adversarial auditor that broke the earlier attempts 4× and finally couldn't.
- CPE-1446 (#710) office/ebook zip-entry deflate-bomb OOM cap; CPE-1447+1449 (#711) thumbnail 128MiB size-gate relocated into
  decode_thumb_image AFTER the video early-dispatch (fixes a huge-image OOM AND a large-video over-block together);
  CPE-1448 (#714) doc-preview "(truncated)" marker visibility (3 edge cases); CPE-1450 (#715) flaky organize_apply test → TempDir.

### Deferred / Blocked (unchanged, all user-gated or deferred-by-choice)
- Deferred: CPE-1414 (SVG use-cycle guard — its crash is LIKELY resolved by the merged CPE-1437/1444 cycle+combined-cost guard;
  left Deferred pending a dedicated verify, could probably be closed), CPE-1431 (media waveform strip), CPE-1443 (dev-toolchain
  svelte4→5/vite migration, big-design).
- Blocked: CPE-002 (signing cert), CPE-118 (3D model viewer), CPE-1442 (rsa Marvin — await rsa 0.10).

### FRONTIER — headless well DRY (feature well + security vein both), backlog EMPTY (verified, not assumed)
A rigorous 5-vein researcher pass (QA-automation infra / bug-hunt of merged code / other untrusted readers / perf / coverage)
found NO substantial genuinely-headless work left without manufacturing filler — the only real items it found were the 3
security bugs shipped this run. QA-automation residuals are all gui-smoke/attended/live-agent/cert gated. The security vein is
substantially tapped (font/net/doc readers checked clean).

**To resume:** do NOT re-scout for headless features or another security sweep (both verified dry this run — read the Library
`resource-exhaustion-dos-sweep-2026-08-07` + `headless-well-dry-post-dualpane-2026-08-07`). The next real increment needs the
USER: (a) attended GUI verification / gui-smoke baseline blessing, (b) macOS (no tauri-driver path), (c) signing cert (CPE-002),
(d) a live agent session for the cost-ledger tab (CPE-1098), (e) two-host network E2E (CPE-819/820), or (f) a fresh user-directed
feature/epic. If the user provides any of those, pick it up; otherwise there is no honest headless work to build.

Tuned defaults: opus adversarial Sec-Auditor gates every untrusted-parser diff (sonnet reviewers missed the real bypass 4×);
reject-nested-input over predict-recursion; a #[tauri::command] doc-comment edit needs bindings.gen.ts regenerated.
