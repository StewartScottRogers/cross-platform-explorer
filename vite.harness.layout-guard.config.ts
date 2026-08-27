import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import path from "node:path";

// CPE-1882: the ONE dev-server config for scripts/dev-harness/layout-guard — the generalised real-
// browser layout harness that wires scripts/dev-harness/statusbar-notice/ (CPE-1659/1859/1836's
// prototype) and friends into CI. Every layout-guard CASE (scripts/dev-harness/layout-guard/cases.mjs)
// is served off this ONE server, on ONE port, rather than each case getting its own
// vite.harness.<name>.config.ts + own port the way checkpoint-narrow/statusbar-notice/revert-heldback
// each did before this ticket — that per-case-config shape is exactly the "touching harness internals"
// CPE-1882's own acceptance criterion says a new case must NOT require.
//
// Backend-talking imports are aliased to scripts/dev-harness/layout-guard/shared-mocks/*, which is
// deliberately GENERIC and PLUGGABLE (see that file's own header) rather than bespoke per component, so
// adding a new case that mounts a backend-talking component ALSO never touches this config — only the
// new case's own harness page (index.html + main.ts) and its entry in cases.mjs. Components with no
// backend-talking imports (StatusBar.svelte, RevertOutcomePanel.svelte) are unaffected by the alias.
//
// BOTH specifier depths are aliased — `../invoke`/`../bindings.gen` (written by a component under
// `src/lib/components/*.svelte`, one directory below `src/lib/`) AND `./invoke`/`./bindings.gen`
// (written by a plain service module living directly in `src/lib/*.ts`, e.g. `src/lib/tags.ts`).
// Reviewer finding (CPE-1882 UAT round 2): a case mounting `TagEditor.svelte` seeds its data through
// `setEntryTags()` in `src/lib/tags.ts`, which is one level shallower and imports the single-dot form —
// aliasing only the double-dot form let that call reach the REAL Tauri `invoke` in a plain browser
// (which throws), silently mounting the component with none of its seed data. Both forms are aliased so
// this class of gap can't recur for either the "component talks to the backend directly" shape (both
// shipped cases today) or the "component goes through a src/lib/*.ts service module" shape.
//
// Entirely separate from vite.config.ts / the app's own dev server (different port, never used for
// `npm run tauri dev` or the production build) — dev-only.
//
// Usage: `npm run harness:layout-guard-server`, then open e.g.
// http://localhost:4331/scripts/dev-harness/statusbar-notice/inner.html?notice=short — or drive it
// headlessly via `npm run harness:layout-guard` (scripts/dev-harness/layout-guard/run.mjs), which
// starts this server itself and tears it down when done. Same server the `layout-guard` CI job
// (.github/workflows/gui-smoke.yml) uses.
export default defineConfig({
  plugins: [svelte({ hot: false })],
  resolve: {
    alias: [
      {
        find: /^\.{1,2}\/invoke$/,
        replacement: path.resolve(__dirname, "scripts/dev-harness/layout-guard/shared-mocks/invoke.ts"),
      },
      {
        find: /^\.{1,2}\/bindings\.gen$/,
        replacement: path.resolve(__dirname, "scripts/dev-harness/layout-guard/shared-mocks/bindings.gen.ts"),
      },
    ],
  },
  // See checkpoint-narrow's identical option for why this is pinned to the harness pages themselves
  // rather than left to vite's default whole-project `.html` scan (which would drag in the real
  // src/index.html -> App.svelte -> the whole component tree, including components our minimal mocks
  // don't cover). A new case's own index.html does not strictly need to be listed here for the dev
  // server to SERVE it (this is a prebundle-scanner hint, not a routing allowlist) — it only needs
  // listing if the scanner's default whole-project crawl would otherwise choke on an unrelated file.
  optimizeDeps: {
    entries: [
      "scripts/dev-harness/statusbar-notice/inner.html",
      "scripts/dev-harness/trash-titlebar/index.html",
    ],
  },
  server: {
    port: 4331,
    strictPort: true,
  },
});
