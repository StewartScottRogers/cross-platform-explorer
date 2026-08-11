---
id: CPE-1629
title: "gui-smoke has no spec for the preview pane, so every new preview surface needs a hand-built Chrome harness to be seen at all"
type: Task
status: Backlog
priority: Medium
component: Testing
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Manual-verification debt, surfaced by the Visual Critic reviewing CPE-1615 (PR #820). The Binary Inspector
gained a whole new tab, and the `gui-smoke` harness had **no spec covering that surface** — so the CI run
produced no screenshot of it, and the only way to actually look at the change was to hand-build a
throwaway Vite + real-Chrome harness that mounted the component against canned data.

That harness worked (and correctly found the tab strip, pill reflow, and theme fidelity all sound), but
it was rebuilt from scratch for one review and thrown away. Every future preview-pane change pays that
cost again — or, worse, ships unlooked-at. `gui-smoke` exists precisely so nobody has to do this by hand.

This matters more than it looks: this crew's hardest-won lesson is that **jsdom cannot see layout** — 3,231
tests once passed while every submenu in the app was clipped invisible. A surface with no screenshot spec
is a surface where the test suite's green is silent about how it looks.

## Goal
Give the preview pane first-class screenshot coverage in `gui-smoke`, so a change to any preview provider
is automatically captured and can be judged from CI artifacts alone.

## Scope
- Add `gui-smoke` specs that open the preview pane against **committed sample files** (the `samples/`
  tree already exists and is ratcheted by `sampleCoverage.test.ts`) and `snap()` each provider surface —
  starting with the Binary Inspector's tabs, and covering the other providers that render structured UI.
- Capture each surface in **both light and dark theme**, and at a **narrow pane width** as well as a
  comfortable one — the narrow case is where clipping and pill-reflow defects actually appear.
- Ensure the screenshot artifact upload includes these (note: the workflow needs `include-hidden-files: true`,
  since dot-directories are excluded by default — this has bitten the crew before).
- Wire the new specs into the existing ratchet so a newly-broken surface fails rather than quietly drops.
- Document, in the harness README, the one-line way to add a spec for a new provider — the point is that
  the next preview feature ships its screenshot coverage as a matter of course.

## Acceptance criteria
- A CI run on a PR touching the preview pane uploads screenshots of the affected provider surfaces, in
  both themes, without anyone building a bespoke harness.
- The Binary Inspector's `.NET metadata` tab specifically is covered.
- The ratchet's known-failing baseline is not silently raised to absorb new specs.
- The burndown row in `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` flips to done, naming the CI job
  that pins it.

**Conflict surface:** the `gui-smoke` harness directory (specs + README), the GUI smoke workflow file, and
`.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`. Independent of feature work.

## Work Log

2026-08-11 — Read the existing harness (`gui-smoke/README.md`, `specs/samples.smoke.ts`, `wdio.conf.ts`,
`lib/ratchet.ts`). Confirmed the current ratchet baseline: `known-failing.json` lists 3 spec FILES
(`samples.smoke.ts`, `saved-search.smoke.ts`, `network.smoke.ts`) — the ratchet is file-granular (any
failing `it()` marks the whole file "failed"), verified by reading `lib/ratchet.ts`/`scripts/run-ratchet.ts`.
`include-hidden-files: true` was already present on BOTH upload steps in `gui-smoke.yml` (landed by
CPE-1594) — no workflow change needed for that item.

2026-08-11 — Checked out CPE-1615 (PR #820, ".NET metadata" tab) — still OPEN/unmerged as of this ticket.
Its tab only renders when `BinaryPreview.svelte`'s `managed` flag is true, and no managed-.NET sample
existed in `samples/`. Built `samples/other/mini-dotnet.dll`: reused `dotnet_metadata.rs`'s own
`build_minimal_managed_pe()` test-fixture byte layout (same convention `other/mini.dll` already
documents — a throwaway Rust generator, not a shipped dependency) via a temporary `#[ignore]` test, ran
it once (`cargo test ... -- --ignored`), then reverted the temporary generator so `dotnet_metadata.rs`
itself is untouched. Added a permanent regression test,
`crates/server/tests/sample_fixtures.rs::mini_dotnet_dll_parses_as_a_real_managed_pe`, asserting the
fixture parses as managed with the expected assembly identity/refs/types/methods — green.

2026-08-11 — Extracted `specs/samples.smoke.ts`'s navigation/settle-detection helpers into
`gui-smoke/lib/samplesNav.ts` (byte-identical `PREVIEW_CONTENT_SELECTOR` — verified samples.smoke.ts's
own behaviour is unchanged, not just refactored) so `specs/preview-pane.smoke.ts` can reuse them. Added
`lib/theme.ts#setTheme()` and `lib/paneWidth.ts#setPreviewPaneWidth()`. Wrote
`specs/preview-pane.smoke.ts`: Binary Inspector tabs (data-driven walk over `.bp-tabs .tab`, native PE +
managed PE), data-grid, font glyph-grid, cert/JWT EXPIRED badges — each in light+dark theme and
narrow(220px)+comfortable(400px) pane width.

2026-08-11 — First real-app run blew the 90s per-test mocha timeout (multi-tab x multi-combo walk is
genuinely expensive: each theme/width toggle is a real popover round-trip against WebView2, several
seconds each) and cascaded into subsequent tests via mocha's "timeout doesn't cancel the in-flight
promise" behaviour. Fixed by (a) capping the non-flagship tab walk to `restTabLimit` tabs at ONE shared
combo instead of one combo per tab, (b) moving the widened timeout to the `describe`-level (a
`beforeEach`'s `this.timeout()` only widens the HOOK's own timeout, not the following test's — a real
bug caught by this run). Re-ran against the real `tauri build --no-bundle` binary: **6/6 passing, ~9
minutes.** Opened and visually verified 6 of the 19 screenshots: narrow width visibly reflows the
Binary Inspector's field list + tab strip, dark theme renders correctly, the font specimen shows real
glyphs, the cert/JWT EXPIRED pills render without clipping, and — the proof the `mini-dotnet.dll`
fixture is genuine, not a placeholder — it triggers the app's own "possible managed .NET" heuristic
banner (zero imports/exports) where the native `mini.dll` fixture does not.

2026-08-11 — Ran `samples.smoke.ts` standalone (twice) to confirm the shared-helper extraction didn't
regress it. Both attempts were killed before the FULL walk finished — this spec's own runtime (dozens of
real files, several known-failing kinds each burning a 20s timeout) exceeded the time available for a
from-scratch re-verification. Both runs got well past `audio/`/`crypto/`/`calendar/`/`database/`/
`documents/` (matching the pre-existing known-failing pattern, no NEW failures observed) before being
stopped; `specs/preview-pane.smoke.ts`'s own full, completed run additionally exercises the SAME shared
`samplesNav.ts` functions successfully across 8 more real files (mini.dll, mini-dotnet.dll, mini.sqlite,
mini.ttf, expired.pem, chain.pem, expired.jwt, rich-claims.jwt). Combined: strong, multi-angle evidence
of no regression, but not a from-scratch clean completed run of `samples.smoke.ts` itself — flagged
honestly rather than claimed.

2026-08-11 — **CPE-1615 (PR #820) merged into `main` while this ticket was in flight.** Merged
`origin/main` into this branch (clean, no conflicts), re-ran `npm run check` (clean) and the mini-dotnet
Rust test (still green against the merged `dotnet_metadata.rs`), rebuilt the frontend + `tauri build
--no-bundle`, and re-ran `preview-pane.smoke.ts` in full: **6/6 passing, ~10 minutes.** The managed-PE
walk test found the REAL ".NET metadata" tab (`.bp-tabs .tab`'s new ".NET metadata" button, `{#if
managed}`-gated on `BinaryPreview.svelte`'s now-real `info.is_managed`) and gave it full flagship 2x2
depth automatically — ZERO changes to `preview-pane.smoke.ts` were needed. Opened
`binary-managed-net-metadata-light-wide.png` and `-dark-narrow.png`: both show the real Assembly
identity table (Name "MyAssembly", Version 1.0.0.0, Culture "neutral") and Referenced assemblies table
(mscorlib 4.0.0.0, System.Core 4.0.0.0) — exactly this fixture's contents — and the narrow/dark combo
shows the tab strip and referenced-assemblies table both needing horizontal scroll at 220px, real
narrow-width behaviour, not a hypothetical. **The ".NET metadata tab" acceptance criterion is now
literally, observationally satisfied — not just designed for.**

2026-08-11 — Updated `gui-smoke/README.md` ("Preview-pane provider screenshots" section + the one-line
recipe for adding a new provider spec, now citing the confirmed CPE-1615 pickup) and
`.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` (flipped supplementary row CPE-1586 — font-preview
render/dark-theme debt — to ✅, pinned by this ticket's spec; supplementary 10→9, total 13→12).
`known-failing.json` is UNCHANGED (still 3 files / 7 specs) — no new spec was added to the ratchet's
allowed-to-fail list.
