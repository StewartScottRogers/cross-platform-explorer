# Workshift Checkpoint

## RUN 2026-08-07 (CLI, user-present GUI+feature session) — CRYPTO EPIC SHIPPED + RUNNING · next epic = CPE-720 media player
**State:** `main` @ origin (clean, 0 worktrees). App **built + installed + running**: v0.57.60-sidecar
(Cross-Platform Explorer (Sidecar) 0.57.60, host+sidecar timestamps verified matching). User reinstalled +
confirmed "Looks good."

### Shipped this session
- **Epic CPE-1417 (Crypto Inspector & Certificate Management) COMPLETE** — all 8 children merged:
  - CPE-1418 JWT decode (#692), CPE-1419 cert/CSR/key decode (#692), CPE-1420 cert create (#694),
    CPE-1421 cert sign/issue-from-CSR (#695), CPE-1422 preview-pane views (#693), CPE-1423+1424 cert
    management dialogs + pane-aware right-pane menu (#697), CPE-1425 samples/crypto/ (#691).
  - Guarantees held: private-key material NEVER displayed/returned/logged (algo+size only); narrow
    committed demo key under samples/crypto/ only (updater.key/.env still ignored).
- **CPE-1426 folder drill-down** (#696) — preview pane becomes a folder browser; click a subfolder to descend.
- Released v0.57.60-sidecar (bumped package.json + Cargo.toml + tauri.conf.json + Cargo.lock; dispatched
  "Release (sidecar-enabled)"; published draft; installed; launched).

### Low-pri backlog (all well-specified, none blocking)
- CPE-1414 — SVG mutual `<use>` cycle stack-overflow (SAFE on prod 2MB stacks; needs non-recursive cycle detector).
- CPE-1415 — defensive `catch_unwind` around sevenz-rust parse (already contained via spawn_blocking).
- CPE-1427 — cert-create RSA-4096 test.
- CPE-1428 — cert-sign `ensure_previewable_size` guard on 3 file reads + CSR-requests-CA test + comment nit.

### NEXT EPIC — ACTIVATED: CPE-720 Audio & Video Player Pane
The app has **zero temporal-media playback today**; only the CPE-943 Playlist model exists. Remaining DoD is
genuinely unbuilt + high-value + visible + user-verifiable-now. Uses the webview's native `<audio>`/`<video>`
(no heavy decoder dep → fits lean-core guardrail). Decomposed just-in-time into:
- **CPE-1429** — audio/video playback + transport in the preview pane (core): pure `mediaTransport.ts`
  controller (play/pause/seek/volume/speed/loop) + `<video>`/`<audio>` via `convertFileSrc` (Tauri asset
  protocol, Range-streamed — NOT data-URL; check assetProtocol capability scope) + new `media` provider kind
  before generic handlers + graceful unsupported-codec fallback (native `error` → message + open-externally)
  + jsdom tests + docs. *Build first.*
- **CPE-1430** — full-screen quick-look media player + folder stepping (spacebar opens; arrow keys prev/next
  across the folder's media via the CPE-943 Playlist model; Esc closes; reuses CPE-1429's transport
  controller). *Depends on 1429; build after it merges.*
- **CPE-1431** (DEFERRED follow-up) — waveform/keyframe scrub strip (heavier: extraction + caching; reuse
  thumbnail pipeline CPE-718). Not this round unless cheap.

**STATUS 2026-08-07 (cont.): epic CPE-720 CORE COMPLETE.**
- **CPE-1429** (#698 MERGED) — audio/video playback + full transport in the preview pane. Reviewer reproduced a
  mutation; one reviewer catch (stale gui-smoke `.preview-media`→`.mp-media` selector) fixed pre-merge.
  New: `src/lib/mediaTransport.ts` (pure controller), `src/lib/components/MediaPlayer.svelte`, `media` provider kind.
- **CPE-1430** (#699 MERGED) — Space full-screen quick-look + ←/→ folder stepping (repeat/shuffle mirror the
  Rust CPE-943 playlist, verified line-by-line incl. u64-wrap shuffle parity). New: `src/lib/mediaQuickLook.ts`,
  `MediaQuickLook.svelte`. Reviewer reproduced a mutation.
- **Building v0.57.61-sidecar** now (run 31223702866) to verify media live with the present user.
- Deferred/follow-ups: **CPE-1431** (waveform/keyframe strip, Deferred), **CPE-1432** (Space quick-look should
  honor active pane in dual-pane — low-pri, pre-existing CPE-645 pattern).

**To resume:** publish v0.57.61-sidecar draft (after asset check) → install (kill all cpe/ai-console procs first;
verify host+sidecar timestamps match) → launch → user plays a `samples/audio|video/` clip + Space quick-look.
Then PM picks the next epic (candidates whose backend ships + only GUI remains: CPE-716 drive-bay, CPE-713 tray,
CPE-714 terminal-dock, CPE-715 link-forge — VERIFY build-state before committing; several epics are
more-done-than-their-brief). Board clean + green.
