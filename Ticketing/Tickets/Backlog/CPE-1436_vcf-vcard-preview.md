---
id: CPE-1436
title: ".vcf vCard structured preview (contact card)"
type: Feature
status: Backlog
priority: Medium
component: Full-stack
tags: [ready]
epic: CPE-1433
created: 2026-08-07
---
## Scope
Structured preview for `.vcf` (vCard 3.0/4.0) files. Same template as CPE-1434/1435.

**Backend** — new `crates/server/src/vcard_preview.rs`: `vcard_preview(bytes: &[u8]) -> VcardPreview`
(specta::Type), ZERO new deps (hand-rolled). Handle line unfolding, split multiple VCARD blocks, decode
FN (formatted name), N (structured name), ORG, TITLE, TEL (list, with TYPE param), EMAIL (list, with TYPE),
ADR (list), URL, BDAY, and PHOTO **presence only** (do NOT inline/return the photo bytes — a boolean/size is
enough, consistent with not shipping heavy blobs over IPC). Tolerate property parameters. Malformed → graceful
partial / `Err`, never panic.

**Command + frontend + tests + docs** — mirror CPE-1434: thin `vcard_preview` command (spawn_blocking,
size-guarded, registered + bindings regen), a `vcard` provider kind for `.vcf` before text, a loader,
`VcardPreview.svelte` (contact card: name/org/title heading; TEL/EMAIL/ADR/URL rows; reflowing type pills;
"photo present" note). cargo tests (a hand-built vCard with multiple TEL/EMAIL + TYPE params + a folded ADR;
a malformed card → graceful; multiple cards in one file). Provider-selection + jsdom render specs. Extend
`src/docs/30-structured-previews.md` (same slug). Add a small sample `.vcf` under `samples/`.

## Acceptance
- Opening a `.vcf` shows a contact card; multiple cards in one file are handled; malformed degrades to text/hex
  (no panic); photo is presence-only, never returned over IPC.
- Zero new deps; backend pure + cargo-tested; provider + render specs pass; bindings regen; check + cargo test +
  vitest green.

## Notes
Third child of epic CPE-1433. Build AFTER CPE-1434 merges (shares wiring with 1434/1435). Can run in parallel
with CPE-1435 only if the shared-wiring conflict is managed (different provider entries, but same files) —
safest to serialize 1434 → then 1435 & 1436 back-to-back rebasing on 1434.
