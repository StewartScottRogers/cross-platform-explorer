import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// CPE-1660: a standalone dev-server config for scripts/dev-harness/statusbar-notice — mounts the REAL
// StatusBar.svelte (no backend-talking imports to alias/mock: its only Tauri import,
// `@tauri-apps/api/window`, is only invoked on a user mousedown and is already try/catch-guarded to be
// a no-op outside Tauri) so a real browser can measure `.statusbar`'s rendered height with a short vs.
// a long notice, at a genuinely narrow CSS viewport. Mirrors vite.harness.config.ts's iframe-viewport
// pattern (see scripts/dev-harness/statusbar-notice/index.html for why an iframe, not a sized <div>,
// is required to escape headless Chrome's `--window-size` clamp under `--headless=new`). Entirely
// separate from vite.config.ts / the app's own dev server; dev-only, not part of the production build.
//
// Usage: `npm run harness:statusbar-notice`, then open
// http://localhost:4320/scripts/dev-harness/statusbar-notice/index.html?w=900&notice=long
export default defineConfig({
  plugins: [svelte({ hot: false })],
  optimizeDeps: {
    entries: [
      "scripts/dev-harness/statusbar-notice/index.html",
      "scripts/dev-harness/statusbar-notice/inner.html",
    ],
  },
  server: {
    port: 4327,
    strictPort: true,
  },
});
