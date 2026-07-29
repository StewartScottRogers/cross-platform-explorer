---
id: CPE-1116
title: "Conflict radar: owner-coloured activity heat-map + legend"
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-730
---

## Summary
CPE-730 DoD item — "the heat-map colours by owner." A per-path/per-folder activity heat-map already ships in
`FileList.svelte` (left accent bar coloured by activity **kind**, not agent). Every activity item now carries
`actor` (sessionId / `"user"` / `"unknown"`, CPE-1101) but `normalizeActivityByKind` throws it away. Colour the
heat-map by the **owning agent** instead, with a legend. FRONTEND only. Design + ground truth:
`.claude/research-library/entries/conflict-radar-close-plan.md` (Ticket A).

## Design (buildable)
- `src/lib/agentActivity.ts` (`ActivitySets` ~:206 / `normalizeActivityByKind` ~:212) — carry `actor` alongside
  each normalized path (don't drop it); add `folderOwnerNorm(sets, dir): string | null` mirroring the sidecar
  `conflict_owner::attribute` + `conflict_region::roll_up` logic (write outranks read; most-touches wins; tie →
  lexically-least actor; deterministic; empty → null).
- new `src/lib/agentColors.ts` — `colorForActor(actor, sessions): string` returning a **theme var**, assigned by
  a *stable* index = position of the actor's sessionId in the sorted session list (fallback: id-hash mod N).
  `"user"` → a dedicated var, `"unknown"` → neutral muted var.
- `src/lib/components/FileList.svelte` (~:475-481, +`<style>`) — set the row accent from `colorForActor(owner)`
  instead of the per-kind literal; add a small legend (swatch + `friendlyActor`) shown only when the activity
  map is non-empty.
- **Palette (decided — Opt A):** fixed `--agent-1..--agent-6` vars in `:root` (light+dark), cycled by stable
  index. N is 1–4 in practice; >6 concurrent agents may reuse a colour (acceptable). No HSL-from-hash (would
  violate theme-vars-only + light/dark contrast).

## ⚠ Guardrails
- Frontend only — never touches any Rust crate. No new deps. Theme vars only; light+dark both legible. Legend
  pills reflow (tick-tacks). **Off-means-off:** empty `fsActivity` → no owner, no accent, no legend (byte-
  identical to today when not watching; single/no colour when one session runs).

## Acceptance Criteria
- [ ] Each active/inside folder row is tinted by the theme-var colour of its owning actor (deterministic owner
      rule); a legend maps each visible colour → `friendlyActor`, hidden when no activity.
- [ ] Colour assignment stable across re-renders for a given session set; `"user"`/`"unknown"` fixed treatments.
- [ ] No new deps; off-means-off preserved (no colour with 0 sessions); `npm run check` clean; `npm test` green.

## Tests
- `agentActivity` units for `folderOwnerNorm` (single owner / contested→top editor / tie→lexically-least /
  write-outranks-read / empty→null — port `conflict_owner.rs` + `conflict_region.rs` cases).
- `agentColors` stable-index + user/unknown mapping test.

## Work Log
2026-07-26 (workshift) — Filed from the CPE-730 close plan (Plan agent). **Blocked on CPE-1112 merging** — both
edit `FileList.svelte`; serialize to avoid a merge collision. Also sequences BEFORE CPE-1118 (both edit
`agentActivity.ts` type region). Palette decided Opt A (fixed theme vars) per go-with-recommendation.

2026-07-26 (workshift) — Built (PR #434, merged be360da2). Independent Reviewer APPROVE + UAT PASS: owner rule mirrors conflict_owner/conflict_region (write>read, most-touches, tie->lexically-least, empty->null), colour stability + user/unknown fixed vars, Okabe-Ito palette in single :root (app is LIGHT-ONLY — no dark theme exists, both gates confirmed), i18n gate green, off-means-off held. Follow-up CPE-1120: ExplorerPane doesn't pass `sessions` to FileList yet -> colours use the djb2-hash fallback in production instead of the stable sorted-session index (deterministic, so AC holds; wire after CPE-1112/#432 which owns ExplorerPane).
