import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import path from "node:path";

// CPE-1635: a standalone dev-server config for scripts/dev-harness/checkpoint-narrow — mounts the REAL
// CheckpointDialog.svelte with its two backend-talking imports (`../invoke`, `../bindings.gen`)
// redirected to canned mocks, so a real browser can be pointed at it to verify layout at narrow widths
// without a running Tauri backend. Entirely separate from vite.config.ts / the app's own dev server
// (different port, never used for `npm run tauri dev` or the production build) — the alias below is
// scoped to raw-specifier strings that only appear inside this harness's own import graph, so it can
// never affect the real app.
//
// Usage: `npm run harness:checkpoint-narrow`, then open
// http://localhost:4319/scripts/dev-harness/checkpoint-narrow/index.html?w=420&theme=dark (query params:
// `w` = simulated viewport width in px, default 420; `theme` = "light"|"dark"; `stress=1` swaps the
// dialog's h2 title for an artificially long one — see inner-main.ts). The page is an OUTER shell that
// sizes an <iframe> to `w`; see index.html's header comment for why an iframe (not a plain sized <div>)
// is required for `vw`-based CSS to actually simulate a narrow OS window in a real, non-clamped browser
// tab. Reusable for any other dialog's narrow-width check — swap the mounted component + its mocks.
export default defineConfig({
  plugins: [svelte({ hot: false })],
  resolve: {
    alias: [
      {
        find: /^\.\.\/invoke$/,
        replacement: path.resolve(__dirname, "scripts/dev-harness/checkpoint-narrow/mocks/invoke.ts"),
      },
      {
        find: /^\.\.\/bindings\.gen$/,
        replacement: path.resolve(__dirname, "scripts/dev-harness/checkpoint-narrow/mocks/bindings.gen.ts"),
      },
    ],
  },
  // Vite's dev-time dependency scanner defaults to crawling EVERY `.html` file in the project (not just
  // the one being navigated to) to build its pre-bundle entry list — which drags in the repo's real
  // root `index.html` -> App.svelte -> the whole component tree, including other components that import
  // named exports (`invoke`, `createChannel`, `rawInvoke`) our minimal `../invoke` mock doesn't provide
  // (it only needs to cover what CheckpointDialog.svelte itself uses: `unwrap`). Pinning the scan to
  // just this harness's own entry avoids that unrelated breakage.
  optimizeDeps: {
    entries: [
      "scripts/dev-harness/checkpoint-narrow/index.html",
      "scripts/dev-harness/checkpoint-narrow/inner.html",
    ],
  },
  server: {
    port: 4319,
    strictPort: true,
  },
});
