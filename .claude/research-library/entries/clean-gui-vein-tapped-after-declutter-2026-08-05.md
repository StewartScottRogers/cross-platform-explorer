---
title: "Is there a clean next GUI epic (backend-exists) to build autonomously, or is the vein tapped?"
date: 2026-08-05
tags: [frontier, gui, backend-exists, unwired-commands, declutter, cpe-979, organize_clutter, vein-tapped, cpe-1329]
status: current
---

## Question
After this shift shipped File-Health exclude UI (CPE-1323), near-dup cleanup (CPE-1324), and Metadata Studio
polish (CPE-1325–1328), is there any remaining CLEAN slice — backend built+wired, frontend missing/thin,
autonomously verifiable with no user resource — or is the GUI-with-existing-backend vein tapped?

## Finding
**Exactly ONE clean slice remained: the Declutter junk-review dialog (epic CPE-979), filed as CPE-1329.**
After that, the vein is TAPPED — remaining work is NEEDS-BACKEND or USER-GATED.

- **CPE-1329 Declutter (CLEAN, built this shift):** `organize_clutter` command (`src-tauri/src/lib.rs:4604`,
  binding `commands.organizeClutter`) + `find_clutter` engine (`crates/server/src/organize.rs:170`,
  `organize_apply.rs:46`) are built + cargo-tested with ZERO frontend callers. `ClutterReason`
  {ZeroByte, Installer, TempOrPartial, Backup} each has a human `label()` doc-commented "for the declutter UI"
  — a UI was planned, never built. Model-free / creds-free (the AI classifier is CPE-979's SEPARATE gated
  part), plain collect command → jsdom-testable + gui-smoke-pinnable, exactly the CPE-1324 shape. Simpler than
  near-dup: flat junk list, no per-group keeper guard.

## Rejected candidates (evidence — do NOT re-research these)
- **Archive suite (CPE-705)** — wired: compress/extract/extract-to/tar.gz/password + analyzeArchiveSafety
  (ContextMenu.svelte, App.svelte). Not a gap.
- **Near-dup images/docs/folders (CPE-997/1002)** — full parity: SimilarImagesDialog (CPE-1202) +
  NearDuplicatesDialog (CPE-1324) both have keeper-guarded cleanup.
- **Drive-bay (CPE-716)** — eject/ejectable/network-shares/disconnect all wired. The one unused cmd
  `driveType` is USER-GATED (unix impl returns "fixed"; needs real removable/network hardware to verify badges).
- **Audio/video player (CPE-720)** — NEEDS BACKEND (only the `playlist` model exists; no decode/transport).
- **Code-intel (CPE-724) / advanced-selection (CPE-711)** — done (blame-gutter was optional).
- **Instant-index (CPE-703)** — indexBuild/indexSearch surfaced via InstantSearch.svelte; deeper = big-design attended.
- **Thumbnail pipeline (CPE-718)** — thumbnailsStream wired (ThumbnailImage.svelte, channel).
- **Backup apply (CPE-735)** — applyBackupPlan wired (BackupDashboard.svelte).
- **Folder item-counts in Properties (CPE-815)** — `folderStats` unwired but duplicates the size PropertiesDialog
  already shows; borderline FILLER, deliberately skipped.
- **QA burndown MVD rows** — all GUI-driver/macOS/binary-swap/two-host-network = attended, unchanged since the
  2026-07-29 sweep.

## How to apply
After CPE-1329 merges, do NOT dispatch another clean-GUI hunt — this sweep + [[headless-frontier-tapped-2026-07-29]]
agree the well is dry. Next real work needs the user: a model key / embedder (CPE-979 AI classifier, semantic
search, copilot, OCR), a Mac, a signing cert, SFTP/cloud creds, Docker (net-E2E), or real removable-drive
hardware — or a NEEDS-BACKEND engine (audio/video decode, unix drive classification). Take an attended/backend
epic WITH the user; do not manufacture filler.
