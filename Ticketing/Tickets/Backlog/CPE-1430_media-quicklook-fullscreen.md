---
id: CPE-1430
title: "Full-screen quick-look media player + folder stepping"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-720
created: 2026-08-07
---
## Scope
A spacebar quick-look full-screen player for media, stepping through the current folder's media with the keyboard.

- **Open:** pressing **Space** on a selected media file (or a "Full screen" control in the CPE-1429 transport)
  opens a full-screen overlay player.
- **Step:** **←/→** move to the previous/next media file **in the current folder**, ordered via the shipped
  **`playlist::Playlist`** model (CPE-943) — honor its repeat (off/one/all) + shuffle. Loop/repeat behavior comes
  from the Playlist cursor, not ad-hoc.
- **Controls:** reuse CPE-1429's **`mediaTransport.ts`** controller + transport UI inside the overlay
  (play/pause/seek/volume/speed). **Esc** (or the same Space) closes and returns to the pane.
- **Overlay conventions:** visible border per the dialog standard, light theme, click-outside/Esc to close,
  focus-trapped, no screen-hijack. Autoplay the stepped-to item.

**Testability:** unit-test the **stepping/keyboard logic** (a pure module or the overlay's controller): building
the folder's media playlist, next/prev honoring repeat+shuffle, Space/Esc/arrow key handling, wrap-around. jsdom
render-spec: overlay mounts on Space, arrow steps to the next media src, Esc closes. Assert real wiring.

**Docs:** extend `src/docs/29-media-player.md` (from CPE-1429) with the quick-look + keyboard section.

## Acceptance
- Space opens a full-screen player for the selected media; ←/→ step through the folder's media (Playlist order +
  repeat/shuffle); Esc closes.
- Reuses CPE-1429's transport controller (no duplicate transport logic).
- Stepping/keyboard logic unit-tested; render-spec passes; `npm run check` + `npx vitest run` green.

## Notes
Depends on **CPE-1429** (transport controller + media provider) — build after it merges. Part of epic CPE-720.
