/**
 * CPE-1182/1183: archive password support end-to-end (extract prompt + create-with-password) plus
 * the extract-to… destination picker and the .tar.gz compress format choice. Integration tests
 * driving the real App with a mocked Tauri backend — same harness as App.features.test.ts.
 *
 * CPE-1184: compress/extract now queue through `start_archive_compress`/`start_archive_extract`
 * (the transfer engine) instead of calling the one-shot `compress_*`/`extract_*` commands directly, so
 * every test below asserts against those two command names. The password check itself still happens
 * *synchronously* before anything is queued (`check_zip_password` on the backend, mirrored by these
 * mocks), so the prompt-and-retry UX these tests exercise is unchanged — only the command name + the
 * fact that a plain `password: null` is sent on the first attempt differ from before. What happens
 * *after* a successful queue (the "Compressed…"/"Extracted…" notice + folder refresh, which now arrives
 * async via a `transfer://done` event) isn't exercised here — see `transfers.test.ts` for the reducer
 * and the routing assertions below for proof the right command + args are used.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

const file = (name: string, extension: string): DirEntry => ({
  name,
  path: `C:\\d\\${name}`,
  is_dir: false,
  size: 1024,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension,
  hidden: false,
  is_symlink: false,
});

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke,
  convertFileSrc: (p: string) => `asset://${p}`,
  Channel,
}));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));

const { openDialog } = vi.hoisted(() => ({ openDialog: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openDialog, save: vi.fn() }));

// A minimal `@tauri-apps/api/event`-shaped bus (CPE-1184): `listen` records every handler registered
// for an event name (there are TWO independent `listen("transfer://done", …)` calls in play —
// `transfers.ts`'s `initTransfers` folds progress into the operations panel store, and App.svelte's own
// listener shows the notice/refreshes — so a single-handler map would silently drop one), `emitEvent`
// below drives them all. This is what lets the "queue-routed compress/extract with progress" tests below
// prove the `transfer://progress`/`transfer://done` events archive ops now stream actually reach the
// App end to end. Harmless for every other test in this file — they never emit anything, so this
// behaves like an inert `listen` that just resolves.
const { eventHandlers } = vi.hoisted(() => ({ eventHandlers: new Map<string, Array<(e: { payload: unknown }) => void>>() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
    const list = eventHandlers.get(event) ?? [];
    list.push(handler);
    eventHandlers.set(event, list);
    return () => eventHandlers.set(event, (eventHandlers.get(event) ?? []).filter((h) => h !== handler));
  }),
}));
function emitEvent(event: string, payload: unknown) {
  for (const handler of eventHandlers.get(event) ?? []) handler({ payload });
}

/** Install a backend whose list_dir returns `listing`, plus per-command overrides for the archive
 *  commands under test (each override receives the invoke args and returns/throws like the real
 *  command would). */
function mockBackend(listing: DirEntry[], overrides: Record<string, (args: Record<string, unknown>) => unknown> = {}) {
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    if (cmd in overrides) return overrides[cmd](args);
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return listing;
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        if (listing.length) ch.onmessage(listing);
        return listing.length;
      }
      case "parent_dir": return null;
      // The Preview pane independently summarises a selected archive; default it to empty so a test
      // that doesn't care about the preview doesn't crash PreviewPane's own `entries.length` read (which
      // otherwise throws mid-flush and can swallow an UNRELATED pending Svelte update in the same tick).
      case "read_archive_entries": return [];
      case "read_file_text": return "";
      default: return null;
    }
  });
}

/** Render App and navigate into the C: drive so we're in a real folder. */
async function enterDrive() {
  render(App);
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
}

/** Right-click a row (by its visible file name) — this is what opens the item context menu
 *  (App's `onRowContext`), selecting the row first exactly like the real FileList row handler. */
function rightClickRow(name: string) {
  const row = screen.getByText(name).closest(".row") as HTMLElement;
  fireEvent.contextMenu(row);
}

/** Scope a query to the open context-menu popup (`role="menu"`) rather than the whole document — the
 *  single previewed archive's right-pane action bar (CPE-1578) can render buttons with the SAME label
 *  text (Extract / Extract to…) as the context menu's own items, so an unscoped `screen.getByText(...)`
 *  would ambiguously match both. Mirrors `App.clipboardPaneRouting.test.ts`'s own `within(await
 *  screen.findByRole("menu"))` recipe. */
async function ctxMenu() {
  return within(await screen.findByRole("menu"));
}

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  invoke.mockReset();
  openDialog.mockReset();
  // Deliberately NOT clearing `eventHandlers` here: `transfers.ts`'s `initTransfers()` is a real
  // module-level singleton that calls `listen(...)` at most once for the whole test file (guarded by
  // its own `started` flag, matching the real app's lifetime-long subscription) — clearing the map here
  // would silently orphan that one-time registration, since nothing would ever re-register it. Every
  // OTHER listener (App.svelte's own `transfer://done` handler) properly unsubscribes on unmount via
  // its stored `unlisten`, so the map doesn't otherwise accumulate stale handlers across tests.
});

describe("archive password — extract prompt + retry (CPE-1182/1184)", () => {
  it("prompts for a password on an AES-encrypted zip, re-prompts on a wrong password, then extracts on the right one", async () => {
    mockBackend([file("secret.zip", "zip")], {
      // `start_archive_extract` runs the same synchronous password check the old `extract_archive`/
      // `extract_zip_encrypted` pair did, just consolidated into one command (CPE-1184): a `null`
      // password on an encrypted archive is a "password required" error; a wrong one is "incorrect".
      start_archive_extract: (args) => {
        const password = args.password as string | null;
        if (password === null) throw new Error("unsupported Zip archive: Password required to decrypt file");
        if (password !== "hunter2") throw new Error("The password provided is incorrect");
        return 1;
      },
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("secret.zip")).toBeTruthy());

    rightClickRow("secret.zip");
    const menu = await ctxMenu();
    await waitFor(() => expect(menu.getByText("Extract")).toBeTruthy());
    await fireEvent.click(menu.getByText("Extract"));

    // The password dialog appears (no password known yet -> the plain queue attempt errored above).
    const field = await screen.findByTestId("password-field");
    await fireEvent.input(field, { target: { value: "wrong" } });
    await fireEvent.click(screen.getByTestId("ok-btn"));

    // Wrong password: re-prompts with the error line rather than closing.
    await waitFor(() => expect(screen.getByTestId("password-error")).toBeTruthy());
    expect(screen.getByTestId("password-field")).toBeTruthy();

    await fireEvent.input(screen.getByTestId("password-field"), { target: { value: "hunter2" } });
    await fireEvent.click(screen.getByTestId("ok-btn"));

    await waitFor(() => {
      const call = invoke.mock.calls.find(
        (c) => c[0] === "start_archive_extract" && (c[1] as Record<string, unknown>).password === "hunter2",
      );
      expect(call).toBeTruthy();
    });
    // The dialog closes once the right password is accepted and the extract is queued (it doesn't wait
    // for the queued run to finish — that arrives later via `transfer://done`).
    await waitFor(() => expect(screen.queryByTestId("password-field")).toBeNull());
  });

  it("a non-password retry failure (CPE-1186) surfaces the real error instead of claiming 'Wrong password'", async () => {
    mockBackend([file("secret.zip", "zip")], {
      // The first (password: null) attempt looks exactly like a locked archive, so the prompt opens.
      // The retry after a password is entered fails for an UNRELATED reason (disk full mid-write) —
      // that error text must reach the user verbatim, not get relabelled "Wrong password".
      start_archive_extract: (args) => {
        const password = args.password as string | null;
        if (password === null) throw new Error("unsupported Zip archive: Password required to decrypt file");
        throw new Error("No space left on device");
      },
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("secret.zip")).toBeTruthy());

    rightClickRow("secret.zip");
    const menu = await ctxMenu();
    await waitFor(() => expect(menu.getByText("Extract")).toBeTruthy());
    await fireEvent.click(menu.getByText("Extract"));

    const field = await screen.findByTestId("password-field");
    await fireEvent.input(field, { target: { value: "hunter2" } });
    await fireEvent.click(screen.getByTestId("ok-btn"));

    // The dialog closes (no more re-prompting — this isn't a password problem) and the real error is
    // shown as the status-bar notice.
    await waitFor(() => expect(screen.queryByTestId("password-field")).toBeNull());
    await waitFor(() => expect(screen.getByText("Error: No space left on device")).toBeTruthy());
    expect(screen.queryByText("Wrong password — try again.")).toBeNull();
  });

  it("Cancel aborts cleanly — no password ever sent to start_archive_extract, dialog closes", async () => {
    mockBackend([file("secret.zip", "zip")], {
      start_archive_extract: (args) => {
        if (args.password === null) throw new Error("Password required to decrypt file");
        return 1;
      },
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("secret.zip")).toBeTruthy());

    rightClickRow("secret.zip");
    const menu = await ctxMenu();
    await waitFor(() => expect(menu.getByText("Extract")).toBeTruthy());
    await fireEvent.click(menu.getByText("Extract"));
    await screen.findByTestId("password-field");

    await fireEvent.click(screen.getByTestId("cancel-btn"));

    await waitFor(() => expect(screen.queryByTestId("password-field")).toBeNull());
    // The initial (password: null) attempt happened — that's what triggered the prompt — but the user
    // cancelled before submitting one, so no call ever carried an actual password.
    expect(
      invoke.mock.calls.some((c) => c[0] === "start_archive_extract" && (c[1] as Record<string, unknown>).password != null),
    ).toBe(false);
  });

  it("entering (double-clicking into) a locked archive also prompts, then extracts it alongside instead of browsing in place", async () => {
    mockBackend([file("locked.zip", "zip")], {
      read_archive_entries: () => {
        throw new Error("The password provided is incorrect");
      },
      start_archive_extract: (args) => {
        if (args.password !== "opensesame") throw new Error("The password provided is incorrect");
        return 1;
      },
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("locked.zip")).toBeTruthy());

    await fireEvent.dblClick(screen.getByText("locked.zip"));

    const field = await screen.findByTestId("password-field");
    await fireEvent.input(field, { target: { value: "opensesame" } });
    await fireEvent.click(screen.getByTestId("ok-btn"));

    await waitFor(() => {
      const call = invoke.mock.calls.find(
        (c) => c[0] === "start_archive_extract" && (c[1] as Record<string, unknown>).password === "opensesame",
      );
      expect(call).toBeTruthy();
    });
  });
});

describe("extract to… (CPE-1183/1184)", () => {
  it("queues the extract into the folder chosen from the native picker (alongside extract-here)", async () => {
    mockBackend([file("bundle.zip", "zip")], {
      start_archive_extract: () => 1,
    });
    openDialog.mockResolvedValue("D:\\chosen");
    await enterDrive();
    await waitFor(() => expect(screen.getByText("bundle.zip")).toBeTruthy());

    rightClickRow("bundle.zip");
    const menu = await ctxMenu();
    await waitFor(() => expect(menu.getByText("Extract to…")).toBeTruthy());
    await fireEvent.click(menu.getByText("Extract to…"));

    await waitFor(() => {
      const call = invoke.mock.calls.find((c) => c[0] === "start_archive_extract");
      expect(call).toBeTruthy();
      expect((call![1] as Record<string, unknown>).dest).toBe("D:\\chosen");
      expect((call![1] as Record<string, unknown>).password).toBeNull();
    });
  });

  it("a cancelled picker makes no start_archive_extract call", async () => {
    mockBackend([file("bundle.zip", "zip")]);
    openDialog.mockResolvedValue(null);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("bundle.zip")).toBeTruthy());

    rightClickRow("bundle.zip");
    const menu = await ctxMenu();
    await waitFor(() => expect(menu.getByText("Extract to…")).toBeTruthy());
    await fireEvent.click(menu.getByText("Extract to…"));

    await waitFor(() => expect(openDialog).toHaveBeenCalled());
    expect(invoke.mock.calls.some((c) => c[0] === "start_archive_extract")).toBe(false);
  });
});

describe("compress with password (CPE-1182/1184)", () => {
  it("collects a password via the dialog, then queues start_archive_compress with it", async () => {
    mockBackend([file("report.txt", "txt")], {
      start_archive_compress: () => 1,
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("report.txt")).toBeTruthy());

    rightClickRow("report.txt");
    await waitFor(() => expect(screen.getByText("Compress with password…")).toBeTruthy());
    await fireEvent.click(screen.getByText("Compress with password…"));

    const field = await screen.findByTestId("password-field");
    await fireEvent.input(field, { target: { value: "s3cr3t" } });
    await fireEvent.click(screen.getByTestId("ok-btn"));

    await waitFor(() => {
      const call = invoke.mock.calls.find((c) => c[0] === "start_archive_compress");
      expect(call).toBeTruthy();
      expect((call![1] as Record<string, unknown>).password).toBe("s3cr3t");
    });
  });

  it("an empty password re-prompts instead of calling the backend", async () => {
    mockBackend([file("report.txt", "txt")], {
      start_archive_compress: () => 1,
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("report.txt")).toBeTruthy());

    rightClickRow("report.txt");
    await waitFor(() => expect(screen.getByText("Compress with password…")).toBeTruthy());
    await fireEvent.click(screen.getByText("Compress with password…"));
    await screen.findByTestId("password-field");

    await fireEvent.click(screen.getByTestId("ok-btn")); // submit with the field still empty

    await waitFor(() => expect(screen.getByTestId("password-error")).toBeTruthy());
    expect(invoke.mock.calls.some((c) => c[0] === "start_archive_compress")).toBe(false);
  });
});

describe("compress to .tar.gz (CPE-1183/1184)", () => {
  it("queues start_archive_compress with a .tar.gz destination (not the hardcoded compress_to_zip)", async () => {
    mockBackend([file("report.txt", "txt")], {
      start_archive_compress: () => 1,
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("report.txt")).toBeTruthy());

    rightClickRow("report.txt");
    await waitFor(() => expect(screen.getByText("Compress to .tar.gz")).toBeTruthy());
    await fireEvent.click(screen.getByText("Compress to .tar.gz"));

    await waitFor(() => {
      const call = invoke.mock.calls.find((c) => c[0] === "start_archive_compress");
      expect(call).toBeTruthy();
      expect((call![1] as Record<string, unknown>).dest).toMatch(/\.tar\.gz$/);
      expect((call![1] as Record<string, unknown>).password).toBeNull();
    });
    expect(invoke.mock.calls.some((c) => c[0] === "compress_to_zip")).toBe(false);
    expect(invoke.mock.calls.some((c) => c[0] === "compress_archive")).toBe(false);
  });
});

describe("queue-routed compress/extract with progress (CPE-1184)", () => {
  it("a queued compress streams progress into the operations panel, then shows the archive-specific notice on completion", async () => {
    mockBackend([file("report.txt", "txt")], {
      start_archive_compress: () => 42,
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("report.txt")).toBeTruthy());

    rightClickRow("report.txt");
    await waitFor(() => expect(screen.getByText("Compress to ZIP")).toBeTruthy());
    await fireEvent.click(screen.getByText("Compress to ZIP"));
    await waitFor(() => expect(invoke.mock.calls.some((c) => c[0] === "start_archive_compress")).toBe(true));

    // Mid-run progress reaches the operations panel (CPE-1184's core promise: a large archive shows
    // live progress instead of a frozen UI).
    emitEvent("transfer://progress", {
      id: 42, op: "compress", total_bytes: 100, done_bytes: 40, total_items: 2, done_items: 1, current: "a.txt",
    });
    await waitFor(() => expect(screen.getByText("a.txt")).toBeTruthy());

    // Completion resolves the registered `onSuccess` — the archive-specific "Compressed…" notice, not
    // the generic copy one, proving `op` routing works end to end.
    emitEvent("transfer://done", {
      id: 42, op: "compress", transferred: 1, skipped: 0, failed: 0, cancelled: false, errors: [],
    });
    await waitFor(() => expect(screen.getByText(/Compressed 1 item to/)).toBeTruthy());
  });

  it("a cancelled queued extract shows the cancellation notice, not a generic failure", async () => {
    mockBackend([file("bundle.zip", "zip")], {
      start_archive_extract: () => 7,
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("bundle.zip")).toBeTruthy());

    rightClickRow("bundle.zip");
    const menu = await ctxMenu();
    await waitFor(() => expect(menu.getByText("Extract")).toBeTruthy());
    await fireEvent.click(menu.getByText("Extract"));
    await waitFor(() => expect(invoke.mock.calls.some((c) => c[0] === "start_archive_extract")).toBe(true));

    emitEvent("transfer://done", {
      id: 7, op: "extract", transferred: 0, skipped: 0, failed: 0, cancelled: true, errors: [],
    });
    await waitFor(() => expect(screen.getByText("Extraction cancelled.")).toBeTruthy());
  });
});
