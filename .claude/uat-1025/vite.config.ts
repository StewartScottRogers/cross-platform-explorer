import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// @tauri-apps/cli sets TAURI_DEV_HOST when running on a device/emulator.
const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [svelte({ hot: false })],

  // Under Vitest, Svelte must resolve to its BROWSER build. With the default
  // node/SSR condition, components render markup but lifecycle hooks (onMount)
  // never fire — which silently makes every integration test meaningless.
  resolve: {
    conditions: process.env.VITEST ? ["browser"] : [],
  },

  // Component tests need a DOM. Pure-module tests run fine either way.
  test: {
    environment: "jsdom",
    globals: true,
    // Never collect tests from sub-agent git worktrees or the Rust target dir — they'd run stale
    // copies against the live source tree and report phantom failures.
    exclude: ["**/node_modules/**", "**/dist/**", "**/.claude/**", "**/target/**"],
    // @tauri-apps/* must be inlined, otherwise Vite pre-bundles it and
    // vi.mock() cannot intercept the import inside .svelte files.
    server: { deps: { inline: [/^@tauri-apps\//] } },
  },

  // Tauri expects a fixed port and fails if it is not available.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Don't watch the Rust source tree.
      ignored: ["**/src-tauri/**"],
    },
  },
}));
