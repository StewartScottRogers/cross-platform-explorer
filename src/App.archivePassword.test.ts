/**
 * CPE-1182/1183: archive password support end-to-end (extract prompt + create-with-password) plus
 * the extract-to… destination picker and the .tar.gz compress format choice. Integration tests
 * driving the real App with a mocked Tauri backend — same harness as App.features.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
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

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  invoke.mockReset();
  openDialog.mockReset();
});

describe("archive password — extract prompt + retry (CPE-1182)", () => {
  it("prompts for a password on an AES-encrypted zip, re-prompts on a wrong password, then extracts on the right one", async () => {
    mockBackend([file("secret.zip", "zip")], {
      extract_archive: () => {
        throw new Error("unsupported Zip archive: Password required to decrypt file");
      },
      extract_zip_encrypted: (args) => {
        if (args.password !== "hunter2") throw new Error("The password provided is incorrect");
        return "C:\\d\\secret";
      },
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("secret.zip")).toBeTruthy());

    rightClickRow("secret.zip");
    await waitFor(() => expect(screen.getByText("Extract")).toBeTruthy());
    await fireEvent.click(screen.getByText("Extract"));

    // The password dialog appears (no password known yet -> plain extract errored above).
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
        (c) => c[0] === "extract_zip_encrypted" && (c[1] as Record<string, unknown>).password === "hunter2",
      );
      expect(call).toBeTruthy();
    });
    // The dialog closes once the right password succeeds.
    await waitFor(() => expect(screen.queryByTestId("password-field")).toBeNull());
  });

  it("Cancel aborts cleanly — no extract_zip_encrypted call, dialog closes", async () => {
    mockBackend([file("secret.zip", "zip")], {
      extract_archive: () => {
        throw new Error("Password required to decrypt file");
      },
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("secret.zip")).toBeTruthy());

    rightClickRow("secret.zip");
    await waitFor(() => expect(screen.getByText("Extract")).toBeTruthy());
    await fireEvent.click(screen.getByText("Extract"));
    await screen.findByTestId("password-field");

    await fireEvent.click(screen.getByTestId("cancel-btn"));

    await waitFor(() => expect(screen.queryByTestId("password-field")).toBeNull());
    expect(invoke.mock.calls.some((c) => c[0] === "extract_zip_encrypted")).toBe(false);
  });

  it("entering (double-clicking into) a locked archive also prompts, then extracts it alongside instead of browsing in place", async () => {
    mockBackend([file("locked.zip", "zip")], {
      read_archive_entries: () => {
        throw new Error("The password provided is incorrect");
      },
      extract_zip_encrypted: (args) => {
        if (args.password !== "opensesame") throw new Error("The password provided is incorrect");
        return "C:\\d\\locked";
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
        (c) => c[0] === "extract_zip_encrypted" && (c[1] as Record<string, unknown>).password === "opensesame",
      );
      expect(call).toBeTruthy();
    });
  });
});

describe("extract to… (CPE-1183)", () => {
  it("extracts into the folder chosen from the native picker (alongside extract-here)", async () => {
    mockBackend([file("bundle.zip", "zip")], {
      extract_archive: (args) => args.dest,
    });
    openDialog.mockResolvedValue("D:\\chosen");
    await enterDrive();
    await waitFor(() => expect(screen.getByText("bundle.zip")).toBeTruthy());

    rightClickRow("bundle.zip");
    await waitFor(() => expect(screen.getByText("Extract to…")).toBeTruthy());
    await fireEvent.click(screen.getByText("Extract to…"));

    await waitFor(() => {
      const call = invoke.mock.calls.find((c) => c[0] === "extract_archive");
      expect(call).toBeTruthy();
      expect((call![1] as Record<string, unknown>).dest).toBe("D:\\chosen");
    });
  });

  it("a cancelled picker makes no extract_archive call", async () => {
    mockBackend([file("bundle.zip", "zip")]);
    openDialog.mockResolvedValue(null);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("bundle.zip")).toBeTruthy());

    rightClickRow("bundle.zip");
    await waitFor(() => expect(screen.getByText("Extract to…")).toBeTruthy());
    await fireEvent.click(screen.getByText("Extract to…"));

    await waitFor(() => expect(openDialog).toHaveBeenCalled());
    expect(invoke.mock.calls.some((c) => c[0] === "extract_archive")).toBe(false);
  });
});

describe("compress with password (CPE-1182)", () => {
  it("collects a password via the dialog, then calls compress_to_zip_encrypted", async () => {
    mockBackend([file("report.txt", "txt")], {
      compress_to_zip_encrypted: (args) => args.dest,
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
      const call = invoke.mock.calls.find((c) => c[0] === "compress_to_zip_encrypted");
      expect(call).toBeTruthy();
      expect((call![1] as Record<string, unknown>).password).toBe("s3cr3t");
    });
  });

  it("an empty password re-prompts instead of calling the backend", async () => {
    mockBackend([file("report.txt", "txt")], {
      compress_to_zip_encrypted: (args) => args.dest,
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("report.txt")).toBeTruthy());

    rightClickRow("report.txt");
    await waitFor(() => expect(screen.getByText("Compress with password…")).toBeTruthy());
    await fireEvent.click(screen.getByText("Compress with password…"));
    await screen.findByTestId("password-field");

    await fireEvent.click(screen.getByTestId("ok-btn")); // submit with the field still empty

    await waitFor(() => expect(screen.getByTestId("password-error")).toBeTruthy());
    expect(invoke.mock.calls.some((c) => c[0] === "compress_to_zip_encrypted")).toBe(false);
  });
});

describe("compress to .tar.gz (CPE-1183)", () => {
  it("routes through compress_archive with a .tar.gz destination (not the hardcoded compress_to_zip)", async () => {
    mockBackend([file("report.txt", "txt")], {
      compress_archive: (args) => args.dest,
    });
    await enterDrive();
    await waitFor(() => expect(screen.getByText("report.txt")).toBeTruthy());

    rightClickRow("report.txt");
    await waitFor(() => expect(screen.getByText("Compress to .tar.gz")).toBeTruthy());
    await fireEvent.click(screen.getByText("Compress to .tar.gz"));

    await waitFor(() => {
      const call = invoke.mock.calls.find((c) => c[0] === "compress_archive");
      expect(call).toBeTruthy();
      expect((call![1] as Record<string, unknown>).dest).toMatch(/\.tar\.gz$/);
    });
    expect(invoke.mock.calls.some((c) => c[0] === "compress_to_zip")).toBe(false);
  });
});
