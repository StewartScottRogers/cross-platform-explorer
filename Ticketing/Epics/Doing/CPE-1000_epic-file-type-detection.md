---
id: CPE-1000
title: "EPIC: True file-type detection & extension-mismatch flagging"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
created: 2026-07-24
closed:
---

> **Filed + activated 2026-07-24** (sprint, Foreman). First slice = the **pure magic-byte type detector +
> mismatch check** (CPE-1001). The column/badge UI is attended.

## Goal
Identify a file's **real type from its content** (magic-byte signature) — PNG, JPEG, PDF, ZIP, GIF, ELF,
Windows PE, etc. — independent of its filename, and **flag files whose content doesn't match their
extension** (a `.jpg` that's actually an `.exe`, a `.pdf` that's really a ZIP). Useful for spotting
mislabeled, corrupt, or suspicious files, and for choosing the right preview/handler.

## Why
Extensions lie — by mistake (a renamed file) or on purpose (malware disguised as an image). The explorer
already previews by extension; knowing the *true* type makes previews correct and surfaces suspicious
mismatches. Pure byte-prefix matching: no AI, no new deps, delete-testable.

## Rough scope (areas, not child tickets)
- A **pure detector**: `detect_type(bytes) -> Option<FileType>` matching known signatures (at offset 0 and a
  few known offsets), a `FileType` enum with canonical extensions, and `mismatch(bytes, ext)` flagging a
  content/extension disagreement.
- Wiring: sniff the leading bytes of files (reuse existing read paths) to power a "true type" column + a
  mismatch warning badge.
- A review surface: list files whose content doesn't match their extension; offer to rename to the correct
  extension.

## Open questions (resolve at activation)
- Signature coverage (which formats) + how many leading bytes to read.
- How to present a mismatch (badge / column / dedicated view) without alarming on benign cases (e.g.
  `.jpeg` vs `.jpg`, container formats like `.docx` = ZIP).
- Handling ambiguous/container formats (ZIP-based: docx/xlsx/jar/apk) gracefully.

## Definition of Done
- The app can report a file's true type from content and flag content/extension mismatches.
- The detector is pure + cargo-tested; unused ⇒ no cost.

## Child tickets
1. **CPE-1001** — Pure magic-byte type detector (`cpe-server`): `detect_type(bytes) -> Option<FileType>`,
   `FileType` (+ canonical extensions), `mismatch(bytes, ext)`. Cargo-tested, no new deps.
   *Headless — buildable now.*
2. **CPE-1002+** — File-sniffing wiring (leading-bytes read) + the true-type column + mismatch badge/review
   UI + safe rename-to-correct-extension. **GUI/attended.**

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Remaining (DoD review 2026-07-30):** True-type column + mismatch badge/review UI unbuilt (only pure detector).
