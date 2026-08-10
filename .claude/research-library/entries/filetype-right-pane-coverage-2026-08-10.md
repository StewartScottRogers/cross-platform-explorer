---
title: "What file types does the right pane cover, and how to make it custom-per-type (view + actions)?"
slug: filetype-right-pane-coverage-2026-08-10
date: 2026-08-10
status: current
tags: [cpe-1568, preview, right-pane, provider-registry, actions, json-tree, notebook, fonts, file-type, pm-reference]
---

## The system today (extend, don't fork)
- **`src/lib/preview/provider.ts`** — ordered registry of **20 `PreviewProvider`s** (`id`/`label`/`kind`/`editable`/`canPreview`);
  first match wins, `pickProvider()` never null (folder→FOLDER, unmatched→FALLBACK). **No per-provider `actions` field exists** —
  actions are wired ad hoc outside this file (ContextMenu gets ~20 pre-computed booleans from App.svelte).
- **`PreviewPane.svelte`** (~1437 lines) — dispatcher: big `{#if provider.kind===…}` chain + ~15 load-state machines.
  Ships text edit/save, wrap, text context menu, code-intel overlay (CPE-724), 3D geometry summary (CPE-1334). Has a
  `.preview-edit-bar` (Edit/Wrap buttons) — the precedent to reuse for a generic action bar.
- Backend `file_type.rs` = 48-variant magic sniffer (detection only; frontend matches on extension/`categoryOf`, not this).
  `*_preview.rs`: binary/data(sqlite/parquet/xlsx paged grid+SQL)/email(full MIME)/ical/vcard/jwt/image.
- Typed viewers: Jwt/Cert (inline copy buttons), Email/Ical/Vcard (display-only), ImageCompareView (rich bespoke actions).

## Out of scope (owned elsewhere) — do NOT re-scope
EXE/DLL/.class/.jar inspect+decompile = **CPE-1561 (Binary Studio)**. Full 3D render (rotate/orbit) = **CPE-118**
(only geometry metadata ships today). Both flagged related-but-separate.

## Confirmed gaps (value×cheapness ranked)
1. **Action-declaration groundwork** (unlocks all) — no `actions` on providers today.
2. **JSON tree view** — today just pretty-printed `<pre>`; want collapsible tree + copy path/value/format/validate.
3. **Single-file image actions** (rotate/convert/copy/set-as) — batch-media backend exists (CPE-1093) but gated ≥2 selected; wire single-file.
4. **Archive actions in-pane** (Extract/Extract-to/Check-safety exist only in ContextMenu).
5. **Fonts** — specimen ships; want glyph grid + copy-glyph/install (pure-Rust ttf-parser — check deps first).
6. **Notebook .ipynb** — ZERO coverage today (falls to JSON/text); ipynb is JSON → pure-JS cell renderer.
7. **YAML/TOML structured view + validate** (check serde_yaml/toml already deps before adding).
8. Log viewer (level highlight/filter), subtitle cue view — stretch.

## Architecture (recommended)
Add a declarative `actions?: PreviewAction[]` to `PreviewProvider` (`{id, labelKey ($t per MENUS.md), icon, enabled?(ctx), run(ctx)}`);
`PreviewActionCtx` carries `{entry, text, selectionText, loader/invoke helpers}` so actions call through `invoke.ts`
(BUSY-CURSOR.md). Render a **generic action bar** in PreviewPane (style like `.preview-edit-bar`); migrate existing
Edit/Wrap/Jwt/Cert buttons onto it incrementally. Backend logic in `cpe-server` behind ServerCtx + thin command; large
payloads stream (STREAMING.md). Do NOT unify with ContextMenu's boolean-prop pattern in the same slice (bigger untangle, not blocking).

## Slice plan → CPE-1568 children (Foreman assigns IDs)
1 groundwork (action-bar types + render + migrate JWT/Cert/Edit as worked example) → 2 JSON tree → 3 single-file image
actions → 4 archive actions in-pane → 5 font glyph grid → 6 notebook viewer → 7 YAML/TOML tree+validate → 8 log viewer.
Slice 1 is the highest-priority unblocker; all others depend on it. Slice 1 is disjoint from trash (Sidebar/backend) work.
