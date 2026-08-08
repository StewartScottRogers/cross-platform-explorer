---
slug: competitive-file-managers-2026-08-08
title: Competitive landscape — TUI + GUI file managers vs CPE ("GUI that beats a TUI")
tags: [product, epics, competitive, tui, gui, directory-opus, pm-reference]
status: current
created: 2026-08-08
---
## User goal
Make CPE the best GUI explorer ever — one that competes with a TUI. Surveyed the TUI peers and the power-GUI /
orthodox-commander landscape for epic inspiration. Companion to [[superfile-pm-reference]] / the superfile entry.

## Apps surveyed
- **TUI:** Yazi, ranger, nnn, lf, vifm, Midnight Commander (mc), broot, xplr, felix, clifm, fff, llama (+ superfile).
- **GUI/commanders:** Directory Opus (gold standard), Total Commander, XYplorer, One/Multi/Double Commander,
  muCommander, Krusader, Explorer++, Q-Dir, Files (files.community), Nautilus, Nemo, Thunar, KDE Dolphin;
  macOS: Path Finder, ForkLift, Commander One, Marta.

## Bottom line
**CPE already matches or exceeds nearly all of them.** Shipped + often ahead of the originals: dual-pane
(CPE-617), color labels (CPE-709), folder-size column (CPE-750), metadata columns (CPE-707), checksums
(CPE-412/737), folder compare (CPE-722/777/779) AND sync (CPE-495/497), thumbnails/previews (CPE-718/724/1433),
archives-as-folders (CPE-705), macros/user-commands (CPE-711/739), workspaces (CPE-708), embedded terminal
(CPE-714), command palette + frecency Spotlight (CPE-704), disk treemap (CPE-751), symlinks (CPE-715), native
tags/xattrs (CPE-717), dedupe (CPE-420/997). The genuine gaps are few.

## Genuine gaps → filings (2026-08-08)
**New epics (Proposed):**
- **CPE-1487 Keyboard Navigation Mode** (vim-modal, opt-in) — the #1 keyboard-first differentiator TUIs have;
  depends on CPE-1484 keymap store. Frontend.
- **CPE-1488 Compact/dense view mode** — the "information density" TUI strength; cheap post-virtualization; the
  most on-purpose (fast/small/predictable made visible). Frontend.
- **CPE-1489 Drop Stack** — the ONE genuinely novel/unclaimed feature (Path Finder): cross-navigation
  multi-source file basket; GUI-only (needs persistence+DnD). Reuses CPE-711 selection + CPE-613 transfer queue.
**New tickets:**
- **CPE-1490** finish image compare (side-by-side/onion-skin/pixel-heatmap) — deferred CPE-722 scope; GUI-exclusive.
- **CPE-1491** file split/join — small classic utility; Low (least differentiating).

## Prioritize EXISTING epics (survey's strongest recommendation — pull forward, don't refile)
- **CPE-661 Universal drag-and-drop** — the single most GUI-exclusive capability on the whole survey (a TUI
  structurally can't do OS-level DnD). Proposed, not built → activate.
- **CPE-616 Remote & cloud FS (SFTP/SMB/WebDAV/S3/cloud)** — every power GUI has it; Proposed. (Check the
  CPE-1461/1462 transfer-layer hardening — groundwork may already exist.)
- **CPE-688 Explorer 10× perf** — engineering shipped (virtualization/coalescing/instrumentation); only the
  attended benchmark closes it. "Sub-frame latency" is the #1 TUI-vs-GUI argument.
- **CPE-1484 Hotkey customization** — filed, dormant; activate (prereq for CPE-1487).

## USER DECISION owed
Theming/accent customization (from the superfile pass) — cuts against the documented light-theme-only
convention; surfaced to the user, not auto-filed.

## Not a fit
Terminal-graphics image preview, `cd_on_quit`, Miller columns (also a TUI thing), user-remap preview-handler
(niche), a general third-party plugin *execution* runtime (security-sensitive, off-purpose; the on-brand path
is a sidecar Mega-Feature later, not core).
