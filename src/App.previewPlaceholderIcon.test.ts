/**
 * Integration test (CPE-1234, epic CPE-978): when a virtual view (a structured saved search, or a
 * tag-only smart folder) is open with NO file selected, the preview pane's no-selection placeholder
 * must NOT reuse the Home glyph — that contradicts the breadcrumb ("Home › <name>"), the search box,
 * and the status bar, which all correctly say the user is inside a saved search / smart folder.
 *
 * Caught by the CPE-1233 Visual Critic pass: `DetailsPane.svelte`'s "no selection" hero hard-coded
 * `<Icon name="home" .../>` for every case, including these virtual views. The fix threads a
 * `folderIcon` prop down from `App.svelte` (`search` for a structured saved search, `filter` for a
 * tag smart folder — the SAME glyphs the sidebar "Saved Searches" / "Smart Folders" sections use),
 * defaulting to `"home"` everywhere else (Home itself, archives, and real folders — unchanged).
 *
 * This test renders the REAL App (mirroring `App.savedSearch.test.ts` / `App.smartFolderLiveRefresh
 * .test.ts`'s precedent) and inspects the actual `<svg>` markup emitted for the placeholder hero, so
 * it fails against the pre-fix behavior (which always emitted the Home glyph's distinctive orange
 * roof stroke) rather than just asserting a prop was threaded.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { savedSearches, addSavedSearch } from "./lib/savedSearchStore";
import { smartFolders, saveSmartFolder } from "./lib/smartFolders";
import { setEntryTags } from "./lib/tags";
import type { Place } from "./lib/types";
import type { TreeNode } from "./lib/bindings.gen";

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

const scannedTree: TreeNode[] = [
  { name: "keep.md", isDir: false, size: 10, modified: 1_700_000_000_000 },
];

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
// Opening either virtual view arms the CPE-1230 live-refresh listener, which wraps the REAL
// `@tauri-apps/api/event.listen` — that needs the Tauri IPC bridge jsdom lacks. No-op it, same fix as
// `App.savedSearch.test.ts` / `App.smartFolderLiveRefresh.test.ts`.
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  savedSearches.set([]);
  smartFolders.set([]);
  Element.prototype.scrollIntoView = vi.fn();

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return [];
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        ch.onmessage([]);
        return 0;
      }
      case "parent_dir": return null;
      case "scan_tree": return scannedTree;
      case "entries_for_paths": {
        const paths = (args?.paths as string[] | undefined) ?? [];
        return paths.map((p) => ({
          name: p.split("\\").pop() ?? p,
          path: p,
          is_dir: false,
          size: 1,
          modified: 1_700_000_000_000,
          extension: "",
          hidden: false,
          is_symlink: false,
        }));
      }
      case "set_tags": {
        const path = args?.path as string;
        const tagList = (args?.tags as string[] | undefined) ?? [];
        const label = (args?.label as string | undefined) ?? "";
        return { [path]: { tags: tagList, label } };
      }
      default: return null;
    }
  });
});

/** The hero `<svg>` behind the "<name> (N items)" placeholder — there is exactly one at a time (the
 *  preview pane and the details-tab fallback are mutually exclusive). */
function heroSvgHtml(): string {
  const svg = document.querySelector(".hero svg");
  expect(svg).toBeTruthy();
  return svg!.outerHTML;
}

describe("preview-pane no-selection placeholder icon (CPE-1234)", () => {
  it("uses the search glyph — NOT Home's — for an open structured saved search", async () => {
    addSavedSearch("Markdown docs", [{ kind: "ext", exts: ["md"] }], "all", "C:\\d");
    render(App);
    await screen.findAllByText("Local Disk (C:)");

    await fireEvent.click(await screen.findByText("Markdown docs"));

    // The virtual view is open (breadcrumb/label say so) with nothing selected — the placeholder shows.
    await waitFor(() => expect(screen.getByText("Markdown docs (1 item)")).toBeTruthy());

    const svg = heroSvgHtml();
    // The search glyph (magnifying glass): a circle + short diagonal handle, the exact same markup the
    // sidebar's "Saved Searches" section uses for this search.
    expect(svg).toContain('cx="11" cy="11" r="6"');
    // Home's glyph has a distinctive orange roof stroke — must be absent here.
    expect(svg).not.toContain("#c94f18");
  });

  it("uses a non-Home glyph for an open tag smart folder", async () => {
    render(App);
    await screen.findAllByText("Local Disk (C:)");
    // Tag AFTER mount: initTags() runs once on mount and would stomp a pre-mount tag back to the
    // (mocked, empty) backend store.
    await setEntryTags("C:\\d\\a.txt", ["invoice"], "");
    saveSmartFolder("Invoices", "invoice");

    await fireEvent.click(await screen.findByText("Invoices"));

    await waitFor(() => expect(screen.getByText("Invoices (1 item)")).toBeTruthy());

    const svg = heroSvgHtml();
    // The filter/funnel glyph — the same markup the sidebar's "Smart Folders" section uses for this
    // entry — and definitely not Home's orange-roof glyph.
    expect(svg).toContain('d="M4 5h16l-6 7v6l-4 2v-8z"');
    expect(svg).not.toContain("#c94f18");
  });

  it("still uses the Home glyph for the real, unaffected Home placeholder", async () => {
    render(App);
    await screen.findAllByText("Local Disk (C:)");

    await waitFor(() => expect(screen.getByText(/^Home \(/)).toBeTruthy());

    const svg = heroSvgHtml();
    expect(svg).toContain("#c94f18");
  });
});
