---
id: CPE-060
title: Preview pane Phase 1 — provider registry + self-contained PreviewPane component
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

First increment of [[CPE-059]]. Build the pluggable preview architecture and a self-contained
`PreviewPane` component, with file access injected as props so the whole thing is unit/jsdom-testable
with no backend, no asset-protocol config, and no Rust (all of which are deferred to Phase 2 because
they can't be verified on this machine). Delivers: a `PreviewProvider` registry, a `pickProvider`
selector, and a `PreviewPane` that renders image and text/markdown previews (or a fallback slot).

## Scope (this ticket)

- `src/lib/preview/provider.ts` — `PreviewProvider` interface, ordered registry, `pickProvider(entry)`
  returning the best match or a metadata fallback (`kind: "none"`).
- Providers: image (category image), markdown (.md/.markdown), text (category text/code); everything
  else → fallback.
- `src/lib/components/PreviewPane.svelte` — props `{ entry, assetUrl, loadText }`. Renders `<img>` for
  images via `assetUrl`, `<pre>` text for text/markdown via `loadText` (with loading/error states and
  stale-request cancellation), and a `<slot>` fallback for everything else.
- Unit tests for `pickProvider`; jsdom tests for `PreviewPane`.

## Explicitly NOT in this ticket (Phase 2 — CPE-061)

- Wiring `PreviewPane` into the App right-pane and a Preview/Details toggle + persisted setting.
- `convertFileSrc` + enabling the Tauri asset protocol (config) for real image loading.
- A Rust `read_file_text` command (size-capped) for real text loading.
- Markdown rendering + syntax highlighting libraries (Phase 2/3).

## Acceptance Criteria

- [ ] `pickProvider` selects image/markdown/text correctly, folders and unknown types → fallback
- [ ] `PreviewPane` renders an `<img>` (src from injected `assetUrl`) for an image entry
- [ ] `PreviewPane` renders file text (from injected `loadText`) for a text entry, with a loading state
- [ ] A newer selection cancels a slower in-flight text load (no stale content for the wrong file)
- [ ] Non-previewable entries render the fallback slot (no img/pre)
- [ ] `npm run check` clean; full suite green
- [ ] CPE-059 updated with the phasing and a link to this + the Phase 2 ticket

## Resolution

Added `src/lib/preview/provider.ts` (`PreviewProvider` interface, ordered registry of image/markdown/
text providers, `pickProvider` with a metadata fallback) and `src/lib/components/PreviewPane.svelte`
(props `entry`/`assetUrl`/`loadText`; renders `<img>` for images, `<pre>` text for text/markdown with
loading/error states and stale-request cancellation via a monotonic request id, `<slot>` fallback
otherwise). 4 `pickProvider` unit tests + 4 `PreviewPane` jsdom tests (image src, text load, error
state, folder fallback). `npm run check` 0 errors; suite 154 passed; `vite build` clean. Committed,
merged to `main`, pushed. Phase 2 (app wiring, asset protocol, Rust read command, markdown/highlight)
tracked in CPE-061.

## Work Log

2026-07-11 — Nightshift: started CPE-059. Confirmed asset protocol is NOT enabled and there is no file-read command — both unverifiable locally — so Phase 1 is the injectable, fully-testable core; integration deferred to Phase 2.

## Notes

Provider order matters: markdown before text (`.md` is category "text"). Markdown renders as raw text
in Phase 1; a real renderer comes later.
