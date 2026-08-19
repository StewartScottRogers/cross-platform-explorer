/**
 * CPE-1634 acceptance criterion: "Interpolated values render correctly and in a natural position in at
 * least ja, ru, de and es — verified by a test that renders in a non-English locale, not by inspection."
 *
 * Two layers:
 *  - A unit-level sweep (`translate()` directly) over a representative sample of the ~117 new interpolated
 *    keys, across all four required locales, proving (a) each locale actually has a REAL translation (not
 *    silently falling back to English) and (b) every `{placeholder}` in the template was substituted with
 *    the caller's value — no stray `{name}`/`{count}`/… survives in the output.
 *  - One real end-to-end render (mirrors `App.blockedNoticesI18n.test.ts`'s CPE-1627 pattern): select a
 *    real file, press Ctrl+C (the default "copy" chord — see `src/lib/keymap.ts`), and assert the notice
 *    banner shows the German catalog's translated + interpolated text, not the English source.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { locale, translate, type Locale } from "./lib/i18n";
import type { DirEntry, Place } from "./lib/types";

const REQUIRED_LOCALES: Locale[] = ["ja", "ru", "de", "es"];

// A representative sample of CPE-1634's new interpolated keys: single-placeholder, multi-placeholder,
// and one/many plural-variant pairs, spanning several of the notice families touched by this ticket.
const SAMPLES: { key: string; params: Record<string, string | number> }[] = [
  { key: "notice.copiedName", params: { name: "report.pdf" } },
  { key: "notice.macroGone", params: { name: "Nightly cleanup" } },
  { key: "notice.vaultReadFailed", params: { name: "secrets.cpevault", error: "permission denied" } },
  { key: "notice.extractedTo", params: { entry: "archive.zip", dest: "Documents/archive" } },
  { key: "notice.selectedItemsMatchingOne", params: { count: 1, pattern: "*.md" } },
  { key: "notice.selectedItemsMatchingMany", params: { count: 7, pattern: "*.md" } },
  { key: "op.failedSingleMove", params: { name: "budget.xlsx", error: "disk full" } },
  { key: "op.failedMany", params: { failed: 3, total: 10, name: "notes.txt", error: "locked" } },
  { key: "notice.checkpointFailedExtraPartial", params: { name: "Photos", extra: 2, partialCount: 1 } },
];

describe("CPE-1634 interpolated notices render correctly in ja/ru/de/es (unit level)", () => {
  for (const loc of REQUIRED_LOCALES) {
    it(`${loc}: every sampled key is genuinely translated with placeholders substituted`, () => {
      for (const { key, params } of SAMPLES) {
        const en = translate("en", key, params);
        const translated = translate(loc, key, params);

        // Really translated — not silently falling back to the English source string.
        expect(translated, `${loc}.${key} should differ from the English source`).not.toBe(en);

        // No unresolved {placeholder} left in the output.
        expect(translated, `${loc}.${key} left a placeholder unsubstituted`).not.toMatch(/\{\w+\}/);

        // Every interpolated VALUE actually made it into the rendered string (proves substitution, not
        // just that the template resolved to *some* other text).
        for (const value of Object.values(params)) {
          expect(
            translated.includes(String(value)),
            `${loc}.${key} is missing interpolated value "${value}": "${translated}"`,
          ).toBe(true);
        }
      }
    });
  }
});

// ---- end-to-end: a real notice, rendered live, in German -----------------------------------------

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

const entry = (name: string, path: string): DirEntry => ({
  name,
  path,
  is_dir: false,
  size: 1024,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: name.includes(".") ? name.split(".").pop()! : "",
  hidden: false,
  is_symlink: false,
});

const listings: Record<string, DirEntry[]> = {
  "C:\\d": [entry("report.pdf", "C:\\d\\report.pdf")],
};

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
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  locale.set("de");

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: listings[args?.path as string] ?? [], filtered: 0 };
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        const list = listings[args?.path as string] ?? [];
        if (list.length) ch.onmessage(list);
        return list.length;
      }
      case "parent_dir": return null;
      default: return null;
    }
  });

  // navigator.clipboard.writeText isn't used by doCopy() (it stages a clipboard object, not the OS
  // clipboard — see App.svelte's doCopy), so no clipboard mock is needed for this trigger.
});

afterEach(() => {
  locale.set("en"); // module singleton — don't bleed into other test files
});

describe("a real interpolated notice renders translated in German (CPE-1634, end-to-end)", () => {
  it("Ctrl+C on a selected file shows the German 'Copied 1 item.' notice, not the English one", async () => {
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);

    const row = await screen.findByText("report.pdf");
    await fireEvent.click(row); // select it (single-selection click)

    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });

    const expected = translate("de", "notice.copiedItemsOne", { count: 1 });
    await waitFor(() => expect(screen.getByText(expected)).toBeTruthy());
    expect(expected).not.toBe(translate("en", "notice.copiedItemsOne", { count: 1 }));
  });
});
