---
id: CPE-1429
title: "Audio & video playback + transport in the preview pane"
type: Feature
status: Backlog
priority: High
component: Frontend
tags: [ready]
epic: CPE-720
created: 2026-08-07
---
## Scope
Give the preview pane real temporal-media playback (the app has none today). When a media file is selected,
render a native `<video>` (video) or `<audio>` (audio) element fed by the local file, with a custom transport.

**Formats** (native webview codecs — no new decoder dep): audio `mp3/wav/ogg/flac/m4a/aac/opus`;
video `mp4/webm/ogg/mov`. First-match-wins in the preview provider registry via a **new `media` kind inserted
BEFORE the generic text/hex handlers** (mirror how `jwt`/`cert` were inserted in `src/lib/preview/provider.ts`).

**Serving the bytes:** use Tauri `convertFileSrc` (asset protocol, **Range-streamed** — do NOT data-URL a large
video). Verify/extend the `assetProtocol` capability scope in `src-tauri/tauri.conf.json` + `capabilities/` so the
webview may load the selected file (the app already previews images/PDF in-webview — reuse that path if it already
grants asset access; otherwise widen the scope minimally). If asset access can't be granted for a path, fall back
to the unsupported message.

**Transport UI** (custom, themed — not the raw browser controls): play/pause, a seekable scrub bar with
current-time / duration, volume + mute, playback speed (0.5–2x), loop toggle. Follow the app conventions (light
theme, MENUS/TABS standards, tick-tack reflow if any pill row). Keep controls on one line; reflow if needed.

**Graceful fallback:** on the media element's `error` event (unsupported codec/container) show a clear message +
an "Open externally" action (reuse the existing open-in-default-app command). No cost when no media is selected.

**Testability (retire MVD):** put the transport logic in a **pure `src/lib/mediaTransport.ts`** controller
(state machine over play/pause/seek/volume/rate/loop + time formatting) with a `mediaTransport.test.ts`; add a
provider-selection test (media kind chosen for the listed extensions, BEFORE text/hex) and a jsdom render-spec for
the transport component (controls present, play/pause toggles, scrub seeks, error → fallback). Assert real
wiring, not hollow renders.

**Docs (CPE-579):** add `src/docs/29-media-player.md` + a `section → media-player` entry in
`src/lib/sectionDocs.ts` (guard test must pass).

**Samples:** add a couple of tiny public-domain/CC0 media clips under `samples/` (or a `samples/media/`) so the
player can be exercised — keep them small; if a binary clip is awkward to commit, note in the ticket that the
user will drop a file in to test.

## Acceptance
- Selecting an audio or video file plays it in the preview pane with a working themed transport (play/pause,
  seek, volume, speed, loop).
- Unsupported formats degrade to a message + open-externally; no media selected ⇒ no cost.
- `mediaTransport.ts` is pure + unit-tested; provider selection + render specs pass; `npm run check` +
  `npx vitest run` green; docs + sectionDocs added (guard passes).

## Notes
Core slice of epic CPE-720. CPE-1430 (full-screen quick-look + folder stepping) reuses this transport
controller, so keep `mediaTransport.ts`'s API clean and export it. Waveform/keyframe strip is deferred (CPE-1431).
