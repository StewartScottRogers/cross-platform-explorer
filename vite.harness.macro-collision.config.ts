import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// CPE-1891 verification harness — visual evidence for the macro-run collision confirm panel. Mounts the
// REAL `MacroRunConfirm.svelte` directly, with `window.__TAURI_INTERNALS__.invoke` shimmed BEFORE the
// component's module graph loads (the exact seam `@tauri-apps/api/core`'s `invoke()` calls into —
// `window.__TAURI_INTERNALS__.invoke(cmd, args, options)` — so the real `invoke.ts` wrapper and the real
// generated `commands.*` client run unmodified on top of it). No mocked module, no aliasing: the same
// code path a live Tauri IPC call would take, minus the OS boundary.
//
// Not the RevertOutcomePanel harness's shape (mount N cases side by side in plain panels) — MacroRun
// Confirm renders its own `position:fixed; inset:0` backdrop, so only one instance is mounted per page
// load, picked by `?case=blocked|mixed`, and `?theme=light|dark` sets the theme the same way
// `src/lib/theme.ts` does in the real app.
//
// Usage: `npm run harness:macro-collision`, then open
// http://localhost:4333/scripts/dev-harness/macro-collision/index.html?case=blocked&theme=dark
export default defineConfig({
  plugins: [svelte({ hot: false })],
  optimizeDeps: {
    entries: ["scripts/dev-harness/macro-collision/index.html"],
  },
  server: {
    port: 4333,
    strictPort: true,
  },
});
