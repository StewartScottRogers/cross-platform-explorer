import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// CPE-1869: a standalone dev-server config for scripts/dev-harness/revert-heldback-copy — mounts the
// REAL RevertOutcomePanel.svelte directly (it has no Tauri-talking imports to alias/mock: it only
// takes a `RevertOutcome` prop) with the app's real src/app.css, so a real browser can confirm the
// copy-full-list affordance renders correctly and is gated correctly (present on the unrestorable-key
// case, absent on the alias/collision and retryable cases) in both themes. No iframe/narrow-width
// machinery needed here, unlike vite.harness.config.ts/vite.harness.statusbar.config.ts — nothing under
// test here depends on viewport width. Entirely separate from vite.config.ts / the app's own dev
// server; dev-only, not part of the production build.
//
// Usage: `npm run harness:revert-heldback-copy`, then open
// http://localhost:4329/scripts/dev-harness/revert-heldback-copy/index.html?theme=dark
export default defineConfig({
  plugins: [svelte({ hot: false })],
  optimizeDeps: {
    entries: ["scripts/dev-harness/revert-heldback-copy/index.html"],
  },
  server: {
    port: 4329,
    strictPort: true,
  },
});
