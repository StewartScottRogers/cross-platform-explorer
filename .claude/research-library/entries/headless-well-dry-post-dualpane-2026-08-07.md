---
question: "After the dual-pane pane-B parity program (CPE-1370–1388), is any clean headless feature vein left?"
date: 2026-08-07
status: current
tags: [frontier, headless, tapped, mvd, qa-automation, dual-pane, cpe-617, user-gated]
---

# Headless feature well is dry after the dual-pane program

**Verdict (independently re-verified across all 34 epics, not trusting prior notes):** the clean,
locally-verifiable headless FEATURE vein is exhausted. The dual-pane pane-B parity program
(CPE-1370–1388, ~19 tickets / 11 PRs) drained the last big vein; CPE-1386 (pane-B archive/vault routing)
is the final self-contained headless slice. Every epic shows the identical pattern: pure/backend core
shipped + cargo/vitest-tested, remaining scope is GUI-attended / Mac / cert / creds / model-key gated.

## Per-epic disposition (all remaining scope is user-gated)
- CPE-661 (universal DnD), CPE-705/707/717/724/740, CPE-711/978 — Done.
- CPE-688 (10x perf) — virtualization/coalescing/instrumentation shipped; only an attended before/after benchmark left.
- CPE-712 shell-citizen — Win+Linux glue shipped; macOS (a Mac) + default-FM handshake left.
- CPE-713 tray / CPE-716 drive-bay — classification/model shipped; tray icon/eject/badges are GUI.
- CPE-720 AV player — Playlist model shipped; decode/transport is webview/GUI.
- CPE-725 media-meta — read/write arc shipped; residual format-risky write-back + GPS map UI.
- CPE-729/732 — classifier/checkpoint commands shipped; restore-panel/approval UI attended (MVD).
- CPE-616 remote FS — headless surface COMPLETE (URI/provider/SFTP/WebDAV/cpe-vfs); Connections UI + keychain left.
- CPE-810 client-server — cpe-net loop + Dispatcher + security chain shipped; frontend transport seam + remote GUI loop left.
- CPE-1000/1002 file-type/inspection — pure detectors Done; column/badge/review UI left.
- CPE-118 3D — geometry reader shipped; three.js WebGL viewer (GPU, big dep) attended.
- CPE-976/977/979/980 AI — pure seams (plan_organize, OcrEngine/FakeOcr) shipped; real engines need model backend/keys.
- Blocked: CPE-002 (code-signing cert), CPE-118 (GPU viewer).

Also re-audited (clean, no bugs): `gridnav.ts`, `tabs.ts`, `virtualize.ts`; repo-wide TODO/FIXME/HACK sweep found nothing load-bearing.

## What IS still headless-buildable (the productive pivot)
**QA-automation infra** — jsdom/vitest render-specs for the many "backend-done, GUI-pending" panels
(File-Health, metadata-edit, near-dup review, checkpoint restore, approval prompt, Connections, file-type
badges). Mounting a panel with a mocked backend and asserting render + control→command wiring + empty/error
states RETIRES its MVD row while staying locally verifiable (tight vitest loop). This is the QA-Architect's
mission and the sanctioned "keep building while the user is away" path — NOT flaky tauri-driver gui-smoke
(WebView2/DevToolsActivePort history, offsite-CI-only; last resort). See the QA-Architect plan.

## To resume / for the user
The next FEATURE increment anywhere needs the user: an attended GUI build→deploy→run verification session
(long legitimate punch-list), a Mac, a signing cert, AI model/API keys, or SFTP/cloud/Docker creds. Supersedes
[[dual-pane-paneB-parity-vein-2026-08-06]] (that vein now drained).
