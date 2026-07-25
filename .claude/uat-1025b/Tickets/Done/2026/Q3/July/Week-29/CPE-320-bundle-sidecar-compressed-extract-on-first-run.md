---
id: CPE-320
title: "Fix: sidecar bundle build fails — single-file resource needs explicit target path"
type: Bug
status: Done
priority: High
component: Backend
estimate: 3h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

The feature bundle build (`tauri build --features sidecar-platform --config
src-tauri/tauri.sidecar.conf.json`) failed on Windows with `Access is denied. (os error
5)` while copying the sidecar resources. The 12 agent manifests (a `*.json` glob) always
copied; only the two **single-file** entries (`ai-console.exe`, `sidecar.json`) failed.

## Root cause

The overlay mapped each single file to a **trailing-slash directory** target,
`"sidecars/"`. Tauri copies a single-file source *to that path* — i.e. to a file named
`sidecars` — which collides with the `sidecars` **directory** the agent glob creates.
Windows returns `ERROR_ACCESS_DENIED` (os error 5) for copying a file onto a directory.

This was **misdiagnosed as Windows Defender** locking a freshly-built `.exe`. That theory
was disproved conclusively: a **gzip blob** (`ai-console.pack`, not a PE, nothing for an
AV to scan) mapped to `"sidecars/"` failed identically, and the same files copied by hand
into `sidecars/` succeed (a manual `cp` targets the directory correctly). No Defender
exclusion is needed; nothing ever locked the binary.

## Fix

Give each single-file bundle entry an explicit destination **file path**; keep the
trailing-slash directory only for the glob:

```json
"../sidecar/ai-console/target/release/ai-console.exe": "sidecars/ai-console.exe",
"../sidecar/ai-console/sidecar.json":                  "sidecars/sidecar.json",
"../sidecar/ai-console/agents/*.json":                 "sidecars/agents/"
```

The build now produces both installers (MSI + NSIS) with `sidecars/ai-console.exe`,
`sidecars/sidecar.json`, and `sidecars/agents/*.json` bundled, with **no admin and no
Defender change**, on this machine and CI alike.

An earlier attempt bundled the binary gzip-compressed and extracted it once at runtime
(to dodge the imagined AV lock). Once the real cause was found, that machinery was
**backed out** — it added a build step, a runtime unpack, and a `flate2` dependency to
solve a non-problem. The in-place raw binary is simpler for developer and user and has no
runtime-extraction failure surface, which is exactly what the reliability/simplicity bar
called for.

## Acceptance — met

- Feature bundle build succeeds on this machine with **no Defender exclusion** (verified:
  MSI + NSIS produced; `sidecars/` contains `ai-console.exe` + `sidecar.json` + 12 agents).
- Default (no-feature) build unchanged; delete-test holds.
- No new dependencies; pack/unpack machinery removed.

## Work Log
2026-07-13 — Chased an "Access is denied" bundle failure first attributed to Defender.
Built a compressed-bundle + extract-once path (host `pack` module + `pack_sidecar` bin +
`flate2`), which surfaced the truth: the `.pack` blob failed the same way, so the cause
was the `"sidecars/"` single-file target colliding with the agents directory, not AV.
Reverted the compression machinery; fixed the overlay to explicit target paths. Full
bundle build green (both installers). README + ADR updated with the real gotcha. Done.
