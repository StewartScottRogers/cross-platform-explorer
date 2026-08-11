import { defineConfig } from "vite";

// CPE-1631: a standalone dev-server config for scripts/dev-harness/hljs-theme — serves a plain page
// that runs the REAL src/lib/preview/highlight.ts against representative code samples and injects
// the output into the app's real markup + src/app.css, so a real browser (not jsdom, which can't see
// colour) can confirm the `.hljs-*` rules actually render legible, on-token syntax highlighting in
// both themes. No Svelte mount and no Tauri backend are involved (highlight.ts has neither
// dependency), so — unlike CPE-1635's vite.harness.config.ts — no svelte plugin or invoke/bindings
// mock aliasing is needed here; this is a separate config (rather than folding into
// vite.harness.config.ts) purely because its serving needs are simpler, on its own port so both
// harnesses can run side by side.
//
// Usage: `npm run harness:hljs-theme`, then open
// http://localhost:4320/scripts/dev-harness/hljs-theme/index.html?theme=dark (theme = "light"|"dark").
export default defineConfig({
  optimizeDeps: {
    entries: ["scripts/dev-harness/hljs-theme/index.html"],
  },
  server: {
    port: 4320,
    strictPort: true,
  },
});
