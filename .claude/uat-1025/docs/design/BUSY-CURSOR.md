# Busy cursor & the `invoke` boundary

_Convention for the app-wide "is it working?" affordance (epic CPE-547; primitive CPE-482)._

## What it is

When any Tauri command runs longer than a short debounce (~150 ms), the app switches the mouse to the
OS wait cursor (`cursor: progress`) so a slow call never looks like a hang, and reverts the instant the
last in-flight call resolves. The mechanism is a ref-counted, debounced tracker in `src/lib/busy.ts`
(`beginBusy` / `withBusy`), surfaced app-wide by `body.busy { cursor: progress }` in `app.css`.

## The one rule: import `invoke` from the wrapper

```ts
import { invoke } from "../invoke";        // ✅ tracked — raises the busy cursor for free
```

(Use the correct relative path to `src/lib/invoke.ts` — `./invoke` from `src/lib`, `../invoke` from
`src/lib/components`, `./lib/invoke` from `src/App.svelte`. The repo has no `$lib` alias.)

Never import `invoke` from `@tauri-apps/api/core` in production code. The wrapper
(`src/lib/invoke.ts`) is a drop-in for the core function that wraps every call in `withBusy`, so a slow
command anywhere gives feedback with **zero** per-call-site code. A guard test
(`src/lib/invoke.guard.test.ts`) fails CI if any file bypasses the wrapper. (`convertFileSrc` and other
core exports are unaffected — still import those from `@tauri-apps/api/core`.)

## Opting out: `rawInvoke`

An operation that **already shows its own progress** should not also flip the wait cursor (no
double-signal). For those, use the untracked export:

```ts
import { rawInvoke } from "../invoke";      // ⛔ untracked — you own the progress UI
```

and add the file to `INVOKE_OPTOUT_ALLOWLIST` in `invoke.guard.test.ts` (one place) with a one-line
reason, so the guard test permits it.

### When to opt out

Opt out only when a **single `invoke` call blocks for the whole duration** of an operation whose
progress you render yourself (a percentage bar, a live status stream). It is **not** needed for:

- **Streaming via events / Channels.** Live output (agent sessions, `agent_watch_*`, sidecar activity)
  arrives over `@tauri-apps/api/event` `listen` / Channels; the `invoke` that *starts* the stream
  returns quickly, so tracking it is correct and brief.
- **The updater.** Download + install go through `@tauri-apps/plugin-updater`
  (`update.downloadAndInstall(onProgress)`), not core `invoke`, so they never touch the wrapper. Their
  own progress bar (`UpdateDialog.svelte`) is the signal.
- **Ordinary long file ops** (copy / move / clone) that have **no** progress bar of their own — there
  the busy cursor is the *only* signal and is exactly what we want.

## Audit status (CPE-550, 2026-07-16)

Every core-`invoke` call site was reviewed. **None** double-signals: the sole self-progress operation
(the updater) already bypasses the wrapper via the updater plugin, and all streaming is event/Channel
based. The opt-out allowlist is therefore **empty**. Revisit this doc + the allowlist if a future
feature adds a single blocking `invoke` that renders its own progress.
