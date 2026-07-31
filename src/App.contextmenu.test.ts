/**
 * CPE-1154: App installs ONE window-level `contextmenu` suppressor so the native WebView2/Edge
 * browser menu ("Back / Refresh / Save as / Print / …") can never appear anywhere in the app —
 * including the blank pane pixels, the toolbar, the sidebar, and Home that the per-element
 * `on:contextmenu` handlers never covered. It must `preventDefault` (kill the native menu) without
 * `stopPropagation` (so the per-element handlers that open the CUSTOM menu still run), and it must be
 * torn down on unmount so it never outlives the component.
 *
 * Same Tauri-backend mocking discipline as App.test.ts so App mounts under jsdom.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke, convertFileSrc: (p: string) => `asset://${p}`, Channel }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return [];
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return [];
      case "parent_dir": return null;
      default: return null;
    }
  });
});

/** A real, cancelable contextmenu event dispatched at `window` — what a right-click on any
 *  otherwise-unhandled pixel bubbles up to. */
function windowRightClick(): MouseEvent {
  const ev = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
  window.dispatchEvent(ev);
  return ev;
}

describe("App native context-menu suppression (CPE-1154)", () => {
  it("preventDefaults a window-level contextmenu once mounted (kills the native browser menu)", async () => {
    // Baseline: nothing suppresses it before the app mounts.
    const before = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    window.dispatchEvent(before);
    expect(before.defaultPrevented).toBe(false);

    render(App);
    await new Promise((r) => setTimeout(r, 50)); // let onMount register the listener

    const ev = windowRightClick();
    expect(ev.defaultPrevented).toBe(true);
  });

  it("removes the suppressor on unmount so it never outlives the app", async () => {
    const { unmount } = render(App);
    await new Promise((r) => setTimeout(r, 50));
    expect(windowRightClick().defaultPrevented).toBe(true); // active while mounted

    unmount();
    const after = windowRightClick();
    expect(after.defaultPrevented).toBe(false); // torn down
  });
});
