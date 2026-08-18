/**
 * CPE-1386 — the archive/vault op family (compress/extract/archive-safety/vault-create/shred) was
 * deferred by CPE-1384 and stayed hidden from a pane-B context menu (`ContextMenu`'s `compressible`/
 * `extractable`/`archiveSafetyEligible`/`shreddable`/`vaultable` props were forced off with
 * `!ctxInPaneB`), because these ops queue through the global `pendingArchiveOps` map / `transfer://done`
 * listener (compress/extract) or their own dialog-owned backend call (archive-safety/shred/vault-create),
 * neither of which carried any pane context.
 *
 * The fix:
 *  - `doCompress`/`doCompressAs`/`doCompressWithPassword`/`doExtract`/`doExtractTo`/`askArchiveSafety`
 *    all take the `inPaneB` `runAction` already computes (same pattern as CPE-1380/1384) and resolve their
 *    selection/target folder via `paneStateFor(inPaneB)` / `paneBPath` instead of the pane-A-only
 *    `selectedEntries`/`currentPath` globals.
 *  - `pendingArchiveOps` entries now carry a `dir` (the folder the op landed in/pulled from); the
 *    `transfer://done` listener refreshes via `refreshBatchApplyTarget(dir)` (reused from CPE-1387) so it
 *    refreshes whichever pane(s) actually show that folder, instead of a hard-coded pane-A `loadPath`.
 *  - `askShred`/`askVaultCreate` SNAPSHOT the selection/folder + pane into `shredConfirmFor`/
 *    `vaultCreateFor` at invocation time (before the dialog opens) — the dialogs bind their `paths`/
 *    `folderPath` props to that frozen snapshot, so a later pane switch while the dialog is open (shred
 *    has no undo; vault-create's "shred original" checkbox is also destructive) can never retarget the
 *    eventual backend call, mirroring `askDelete`'s `snapshotConfirmTarget` (CPE-1370).
 *
 * Same mounted-App-with-mocked-backend dual-pane harness as App.paneBBulkOps.test.ts (disk state +
 * backend command mocks) combined with App.archivePassword.test.ts's `transfer://done`-capable event bus
 * (needed here to prove the COMPLETION refresh lands on the right pane, not just the initial call).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveDualPane, savePaneBPath } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

const file = (name: string, dir: string): DirEntry => ({
  name,
  path: `${dir}\\${name}`,
  is_dir: false,
  size: 10,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: name.split(".").pop() ?? "",
  hidden: false,
  is_symlink: false,
});

const folder = (name: string, dir: string): DirEntry => ({
  name,
  path: `${dir}\\${name}`,
  is_dir: true,
  size: 0,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: "",
  hidden: false,
  is_symlink: false,
});

const PATH_A = "C:\\d";
const PATH_B = "C:\\dB";
const drives: Place[] = [{ name: "Local Disk (C:)", path: PATH_A, kind: "drive" }];

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
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

// Same multi-handler event bus as App.archivePassword.test.ts — App.svelte's own `transfer://done`
// listener AND `lib/transfers.ts`'s `initTransfers` both call `listen("transfer://done", …)`, so a
// single-handler stub would silently drop one of them.
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

let startArchiveCompressCalls: { paths: string[]; dest: string }[] = [];
let startArchiveExtractCalls: { path: string; dest: string }[] = [];
let analyzeArchiveSafetyCalls: string[] = [];
let shredPathsCalls: { paths: string[] }[] = [];
let vaultCreateCalls: { folder: string; dest: string; shredOriginal: boolean }[] = [];
let nextTransferId = 1;

// Mutable "disk" state, keyed by folder path — a completed compress/extract/vault-create pushes a new
// entry so the NEXT list_dir call proves the right pane actually refreshed (same trick as
// App.paneBBulkOps.test.ts), rather than merely asserting the backend call's args.
let disks: Record<string, DirEntry[]> = {};

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  eventHandlers.clear();
  startArchiveCompressCalls = [];
  startArchiveExtractCalls = [];
  analyzeArchiveSafetyCalls = [];
  shredPathsCalls = [];
  vaultCreateCalls = [];
  nextTransferId = 1;
  disks = {
    [PATH_A]: [file("alpha1.png", PATH_A), file("alpha2.png", PATH_A)],
    [PATH_B]: [
      file("bravo1.png", PATH_B),
      file("bravo2.png", PATH_B),
      file("bundle.zip", PATH_B),
      folder("secrets", PATH_B),
    ],
  };
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const listingFor = (path: unknown) => disks[path as string] ?? [];
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: listingFor(args.path), filtered: 0 };
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const data = listingFor(args.path);
        ch.onmessage(data);
        return data.length;
      }
      case "parent_dir": return null;
      case "read_archive_entries": return [];
      case "read_file_text": return "";
      case "start_archive_compress": {
        const paths = args.paths as string[];
        const dest = args.dest as string;
        startArchiveCompressCalls.push({ paths, dest });
        const dir = dest.slice(0, dest.lastIndexOf("\\"));
        const name = dest.slice(dest.lastIndexOf("\\") + 1);
        disks[dir] = [...(disks[dir] ?? []), file(name, dir)];
        return nextTransferId++;
      }
      case "start_archive_extract": {
        const path = args.path as string;
        const dest = args.dest as string;
        startArchiveExtractCalls.push({ path, dest });
        disks[dest] = disks[dest] ?? [];
        const dir = dest.slice(0, dest.lastIndexOf("\\"));
        const name = dest.slice(dest.lastIndexOf("\\") + 1);
        disks[dir] = [...(disks[dir] ?? []), folder(name, dir)];
        return nextTransferId++;
      }
      case "analyze_archive_safety": {
        analyzeArchiveSafetyCalls.push(args.path as string);
        return {
          report: { total_compressed: 1024, total_uncompressed: 2048, overall_ratio: 2.0, flagged: [], dangerous: false },
          entries_scanned: 3,
          truncated: false,
          unreadable: false,
        };
      }
      case "shred_paths": {
        const paths = args.paths as string[];
        shredPathsCalls.push({ paths });
        for (const p of paths) {
          for (const key of Object.keys(disks)) disks[key] = disks[key].filter((e) => e.path !== p);
        }
        return paths.map((p) => ({ path: p, ok: true, passes_done: 1, bytes_overwritten: 10 }));
      }
      case "vault_create": {
        const folderPath = args.folder as string;
        const dest = args.dest as string;
        const shredOriginal = args.shredOriginal as boolean;
        vaultCreateCalls.push({ folder: folderPath, dest, shredOriginal });
        const dir = dest.slice(0, dest.lastIndexOf("\\"));
        const name = dest.slice(dest.lastIndexOf("\\") + 1);
        disks[dir] = [...(disks[dir] ?? []), file(name, dir)];
        if (shredOriginal) {
          for (const key of Object.keys(disks)) disks[key] = disks[key].filter((e) => e.path !== folderPath);
        }
        return null;
      }
      default: return null;
    }
  });
});

/** Boot straight into dual-pane, navigate pane A into its drive, wait for both panes to settle — same
 *  helper as App.paneBBulkOps.test.ts. Returns each pane's `.pane-col` wrapper. */
async function bootDualPane() {
  saveDualPane(true);
  savePaneBPath(PATH_B);
  render(App);

  await waitFor(() => expect(screen.getByText("bravo1.png")).toBeTruthy());
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("alpha1.png")).toBeTruthy());

  const paneAWrap = screen.getByText("alpha1.png").closest(".pane-col") as HTMLElement;
  const paneBWrap = screen.getByText("bravo1.png").closest(".pane-col") as HTMLElement;
  return { paneAWrap, paneBWrap };
}

describe("App — Compress is pane-aware (CPE-1386)", () => {
  it("'Compress to ZIP' from a pane-B context menu is no longer hidden, compresses pane B's selection into pane B's folder, and refreshes pane B (not pane A) on completion", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png")); // pane A has its own selection

    await fireEvent.click(within(paneBWrap).getByText("bravo1.png"));
    const bravoRow = within(paneBWrap).getByText("bravo1.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    // Pre-fix, `compressible` was forced off for a pane-B-opened menu — this row never appeared.
    expect(menu.getByText("Compress to ZIP")).toBeTruthy();
    await fireEvent.click(menu.getByText("Compress to ZIP"));

    await waitFor(() => expect(startArchiveCompressCalls.length).toBe(1));
    expect(startArchiveCompressCalls[0].paths).toEqual([`${PATH_B}\\bravo1.png`]);
    expect(startArchiveCompressCalls[0].dest.startsWith(PATH_B)).toBe(true);

    emitEvent("transfer://done", {
      id: nextTransferId - 1, op: "compress", transferred: 1, skipped: 0, failed: 0, cancelled: false, errors: [],
    });

    // The new archive shows up in pane B's listing; pane A never refreshed/gained anything.
    await waitFor(() => expect(within(paneBWrap).queryByText(/\.zip$/)).toBeTruthy());
    expect(within(paneAWrap).queryByText(/\.zip$/)).toBeNull();
  });
});

describe("App — Extract is pane-aware (CPE-1386)", () => {
  it("'Extract' from a pane-B context menu extracts pane B's archive into pane B's folder, and refreshes pane B on completion", async () => {
    const { paneBWrap } = await bootDualPane();

    const zipRow = within(paneBWrap).getByText("bundle.zip").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(zipRow);
    const menu = within(await screen.findByRole("menu"));
    // Pre-fix, `extractable` was forced off for a pane-B-opened menu.
    expect(menu.getByText("Extract")).toBeTruthy();
    await fireEvent.click(menu.getByText("Extract"));

    await waitFor(() => expect(startArchiveExtractCalls.length).toBe(1));
    expect(startArchiveExtractCalls[0].path).toBe(`${PATH_B}\\bundle.zip`);
    expect(startArchiveExtractCalls[0].dest.startsWith(PATH_B)).toBe(true);

    emitEvent("transfer://done", {
      id: nextTransferId - 1, op: "extract", transferred: 3, skipped: 0, failed: 0, cancelled: false, errors: [],
    });

    await waitFor(() => expect(within(paneBWrap).queryByText("bundle")).toBeTruthy());
  });
});

describe("App — Check archive safety… is pane-aware (CPE-1386)", () => {
  it("scans pane B's archive, not pane A's, when opened from a pane-B context menu", async () => {
    const { paneBWrap } = await bootDualPane();

    const zipRow = within(paneBWrap).getByText("bundle.zip").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(zipRow);
    const menu = within(await screen.findByRole("menu"));
    // Pre-fix, `archiveSafetyEligible` was forced off for a pane-B-opened menu.
    expect(menu.getByText("Check archive safety…")).toBeTruthy();
    await fireEvent.click(menu.getByText("Check archive safety…"));

    await waitFor(() => expect(analyzeArchiveSafetyCalls.length).toBe(1));
    expect(analyzeArchiveSafetyCalls[0]).toBe(`${PATH_B}\\bundle.zip`);
  });
});

describe("App — Securely delete… (shred) is pane-aware + snapshot-safe (CPE-1386)", () => {
  it("shreds pane B's file, not pane A's, and refreshes pane B on completion", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png"));

    const bravoRow = within(paneBWrap).getByText("bravo1.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    // Pre-fix, `shreddable` was forced off for a pane-B-opened menu.
    expect(menu.getByText("Securely delete…")).toBeTruthy();
    await fireEvent.click(menu.getByText("Securely delete…"));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/bravo1\.png/)).toBeTruthy();
    await fireEvent.click(within(dialog).getByText("Shred permanently"));

    await waitFor(() => expect(shredPathsCalls.length).toBe(1));
    expect(shredPathsCalls[0].paths).toEqual([`${PATH_B}\\bravo1.png`]); // NOT alpha1.png

    await waitFor(() => expect(within(paneBWrap).queryByText("bravo1.png")).toBeNull());
    // Pane A untouched.
    expect(within(paneAWrap).getByText("alpha1.png")).toBeTruthy();
  });

  it("snapshot-safe: switching the active pane while the shred confirm is open doesn't retarget the (irreversible) shred", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();

    const bravoRow = within(paneBWrap).getByText("bravo1.png").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(bravoRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Securely delete…"));
    const dialog = await screen.findByRole("dialog");

    // The active pane changes WHILE the confirm dialog is still open.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png"));

    await fireEvent.click(within(dialog).getByText("Shred permanently"));

    await waitFor(() => expect(shredPathsCalls.length).toBe(1));
    // Still pane B's file — the pane switch after the dialog opened must not retarget an irreversible
    // shred onto pane A's alpha1.png (CPE-1370's snapshot-at-open-time reasoning, applied to CPE-1386).
    expect(shredPathsCalls[0].paths).toEqual([`${PATH_B}\\bravo1.png`]);
  });
});

describe("App — Create encrypted vault… is pane-aware + snapshot-safe (CPE-1386)", () => {
  it("seals pane B's folder, not pane A's, and refreshes pane B on completion", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png"));

    const secretsRow = within(paneBWrap).getByText("secrets").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(secretsRow);
    const menu = within(await screen.findByRole("menu"));
    // Pre-fix, `vaultable` was forced off for a pane-B-opened menu.
    expect(menu.getByText("Create encrypted vault…")).toBeTruthy();
    await fireEvent.click(menu.getByText("Create encrypted vault…"));

    const dialog = await screen.findByRole("dialog");
    await fireEvent.input(within(dialog).getByTestId("vault-passphrase"), { target: { value: "hunter2" } });
    await fireEvent.input(within(dialog).getByTestId("vault-passphrase-confirm"), { target: { value: "hunter2" } });
    await fireEvent.click(within(dialog).getByTestId("vault-create-confirm"));

    await waitFor(() => expect(vaultCreateCalls.length).toBe(1));
    expect(vaultCreateCalls[0].folder).toBe(`${PATH_B}\\secrets`); // NOT a pane-A folder
    expect(vaultCreateCalls[0].dest.startsWith(PATH_B)).toBe(true);

    await waitFor(() => expect(within(paneBWrap).queryByText(/secrets\.cpevault$/)).toBeTruthy());
    expect(within(paneAWrap).queryByText(/secrets\.cpevault$/)).toBeNull();
  });

  it("DESTRUCTIVE shred-original: the target folder is captured at invocation time, before the dialog opens — switching the active pane while typing the passphrase doesn't retarget which folder gets shredded", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();

    const secretsRow = within(paneBWrap).getByText("secrets").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(secretsRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Create encrypted vault…"));
    const dialog = await screen.findByRole("dialog");

    // Opt into the destructive "shred the original" path, and separately acknowledge it (CPE-1646: the
    // shred checkbox is intent, the "I understand…" checkbox is the distinct consent act — the dialog
    // won't submit on intent alone).
    await fireEvent.click(within(dialog).getByTestId("vault-shred"));
    await fireEvent.click(within(dialog).getByTestId("vault-shred-confirm"));

    // The active pane changes WHILE the passphrase dialog is still open.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha1.png"));

    await fireEvent.input(within(dialog).getByTestId("vault-passphrase"), { target: { value: "hunter2" } });
    await fireEvent.input(within(dialog).getByTestId("vault-passphrase-confirm"), { target: { value: "hunter2" } });
    await fireEvent.click(within(dialog).getByTestId("vault-create-confirm"));

    await waitFor(() => expect(vaultCreateCalls.length).toBe(1));
    // Still targets pane B's "secrets" folder (the row actually right-clicked), with shredOriginal true —
    // the later pane switch must not have retargeted the destructive call onto anything in pane A.
    expect(vaultCreateCalls[0]).toEqual({
      folder: `${PATH_B}\\secrets`,
      dest: vaultCreateCalls[0].dest,
      shredOriginal: true,
    });
    expect(vaultCreateCalls[0].dest.startsWith(PATH_B)).toBe(true);
    // Pane A's alpha1.png (selected mid-dialog) was never touched.
    expect(within(paneAWrap).getByText("alpha1.png")).toBeTruthy();
  });
});
