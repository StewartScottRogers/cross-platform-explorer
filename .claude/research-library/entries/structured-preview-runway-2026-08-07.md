---
question: "After the media-player epic, what genuinely-unbuilt HEADLESS feature runway is left (verified against code, not briefs)?"
date: 2026-08-07
status: current
tags: [frontier, headless, preview-providers, eml, ics, vcf, structured-viewer, cpe-1433, crypto-viewer-template, runway, well-nearly-dry]
---

# Structured-preview runway — ~2 shifts of real headless work, then the well is dry

**Verified against CODE (not epic briefs), because multiple "candidate" epics turned out already-built.**
Confirmed done-and-wired this session while scouting: **format readers** (DICOM/camera-RAW/RAR — commands +
`loaders.ts` + `provider.ts` entries all shipped, CPE-1345/1346/1347/1350), **checkpoint-rollback** (CPE-732:
`checkpoint_revert*` commands + 9-module snapshot engine), **file-type detection** (CPE-1000: FileHealth mismatch
tab + true-type column). Do NOT re-propose these.

## The real runway = structured previews for text-mapped file types (epic CPE-1433)
Three mainstream types today fall through to the plain-text/"code" provider; the closed CPE-079/092/093 shipped
ONLY that text mapping and **explicitly deferred** structured parsing ("future backend enhancement"). Each
follows the shipped crypto-viewer template arm-for-arm: `jwt_preview.rs` → `jwt_preview` command →
`JwtPreview.svelte` + jsdom test, provider entry before `text`.
- **CPE-1434 `.eml`** (HIGH) — headers + MIME-part tree + attachments + sanitized text body. `email_preview.rs`.
- **CPE-1435 `.ics`** (MED) — VEVENT what/when/where/who card. Zero-dep RFC 5545 line-unfold parser.
- **CPE-1436 `.vcf`** (MED) — contact card. Zero-dep vCard parser.
VERIFIED-UNBUILT: `provider.ts` has no eml/ics/vcf kind; `filetypes.ts` maps all three → `"code"`; grep of
`crates/server/src` found no email/ical/vcard parser module. Shared wiring (provider ordering + PreviewPane
import + `generate_handler!` list) means 1435/1436 serialize AFTER 1434.

## Honest verdict — ~2 shifts, not a clean 3
The eml/ics/vcf trio is ~1–1.5 shifts of genuine net-new feature work. The two hardening tickets (CPE-1414 SVG
use-cycle guard, CPE-1415 sevenz catch_unwind) add ~0.5 shift. **Beyond that the vein thins to gold-plating**
(subtitle/WKT/FASTA structured views — do NOT manufacture) and the next increment needs the USER (attended GUI
punch-list, Mac, signing cert, model/creds). **Correction to the 2026-08-07 dry-note:** its proposed "jsdom
render-spec pivot" is ALSO largely consumed — 112 components / 227 test files already; nearly every dialog has a
`.test.ts`; the remaining MVD rows (GUI-e2e, cross-OS, real-network, binary-swap) need real builds, not jsdom.
So jsdom-pin filler is NOT a viable 3rd shift either. After the trio + hardening, wrap and get the user.

Supersedes-extends [[headless-well-dry-post-dualpane-2026-08-07]] (confirms dry + adds the one remaining vein).
