/**
 * CPE-1424 (epic CPE-1417) — the certificate-management context-menu entries are pane-aware, mirroring
 * CPE-1377/1384's established pattern (see App.paneBBulkOps.test.ts / App.paneBContextMenu.test.ts):
 * "Create certificate here…" from pane B's empty area (or a folder row) must default CreateCertDialog's
 * output folder to pane B's own folder, not pane A's; "Issue cert from this CSR…" / "Sign with this as
 * CA…" from a pane-B cert/CSR row must pre-fill SignCertDialog with THAT file, not anything from pane A.
 * Also covers the file-type gating itself (CSR vs. cert-shaped vs. JWT vs. an ordinary file) end-to-end
 * through the mounted app, since ContextMenu.test.ts only exercises the component in isolation.
 *
 * Same mounted-App-with-mocked-backend dual-pane harness as App.paneBBulkOps.test.ts / App.paneBContextMenu.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveDualPane, savePaneBPath } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

const file = (name: string, dir: string, extension: string): DirEntry => ({
  name,
  path: `${dir}\\${name}`,
  is_dir: false,
  size: 10,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension,
  hidden: false,
  is_symlink: false,
});

const PATH_A = "C:\\d";
const PATH_B = "C:\\dB";
const drives: Place[] = [{ name: "Local Disk (C:)", path: PATH_A, kind: "drive" }];
const entriesA: DirEntry[] = [file("alpha.txt", PATH_A, "txt")];
const entriesB: DirEntry[] = [
  file("bravo.txt", PATH_B, "txt"),
  file("bravo.csr", PATH_B, "csr"),
  file("bravo.pem", PATH_B, "pem"),
  file("bravo.jwt", PATH_B, "jwt"),
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
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

let certCreateCalls: { params: unknown; certPath: string; keyPath: string }[] = [];
let certIssueCalls: { csrPath: string; caCertPath: string; caKeyPath: string; validityDays: number; outCertPath: string }[] = [];

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  certCreateCalls = [];
  certIssueCalls = [];
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const listingFor = (path: unknown) => (path === PATH_B ? entriesB : entriesA);
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return listingFor(args.path);
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const data = listingFor(args.path);
        ch.onmessage(data);
        return data.length;
      }
      case "parent_dir": return null;
      case "cert_create": {
        certCreateCalls.push(args as { params: unknown; certPath: string; keyPath: string });
        return null;
      }
      case "cert_issue_from_csr": {
        certIssueCalls.push(args as typeof certIssueCalls[number]);
        return null;
      }
      default: return null;
    }
  });
});

/** Boot straight into dual-pane, navigate pane A into its drive, wait for both panes to settle — same
 *  helper as App.paneBContextMenu.test.ts. */
async function bootDualPane() {
  saveDualPane(true);
  savePaneBPath(PATH_B);
  render(App);
  await waitFor(() => expect(screen.getByText("bravo.txt")).toBeTruthy());
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());
  const paneAWrap = screen.getByText("alpha.txt").closest(".pane-col") as HTMLElement;
  const paneBWrap = screen.getByText("bravo.txt").closest(".pane-col") as HTMLElement;
  return { paneAWrap, paneBWrap };
}

describe("App — 'Create certificate here…' is pane-aware (CPE-1424)", () => {
  it("from pane B's EMPTY-AREA context menu, defaults CreateCertDialog's output folder to pane B's own folder, not pane A's", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    // Pane A stays the "active" pane throughout — the pane-B menu must still target pane B by construction.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(screen.getByText("alpha.txt"));

    const paneBRows = paneBWrap.querySelector(".rows") as HTMLElement;
    await fireEvent.contextMenu(paneBRows);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Create certificate here…"));

    const dialog = await screen.findByRole("dialog");
    expect((within(dialog).getByTestId("cert-create-folder") as HTMLInputElement).value).toBe(PATH_B);

    await fireEvent.input(within(dialog).getByTestId("cert-create-cn"), { target: { value: "svc.local" } });
    await fireEvent.click(within(dialog).getByTestId("cert-create-confirm"));

    await waitFor(() => expect(certCreateCalls.length).toBe(1));
    expect(certCreateCalls[0].certPath).toBe(`${PATH_B}\\svc.local.pem`); // NOT PATH_A
    expect(certCreateCalls[0].keyPath).toBe(`${PATH_B}\\svc.local.key`);
  });

  it("single-pane mode: 'Create certificate here…' still targets pane A's own folder", async () => {
    saveDualPane(false);
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());

    const rows = screen.getByText("alpha.txt").closest(".rows") as HTMLElement;
    await fireEvent.contextMenu(rows);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Create certificate here…"));

    const dialog = await screen.findByRole("dialog");
    expect((within(dialog).getByTestId("cert-create-folder") as HTMLInputElement).value).toBe(PATH_A);
  });
});

describe("App — cert/CSR/JWT context-menu rows are pane-aware + gated by file type (CPE-1424)", () => {
  it("'Issue cert from this CSR…' from a pane-B .csr row pre-fills SignCertDialog with pane B's file, not pane A's", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    await fireEvent.click(paneAWrap);
    await fireEvent.click(screen.getByText("alpha.txt"));

    const csrRow = within(paneBWrap).getByText("bravo.csr").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(csrRow);
    const menu = within(await screen.findByRole("menu"));
    expect(menu.queryByText("Sign with this as CA…")).toBeNull(); // gated: a .csr offers issue, not sign-as-CA
    await fireEvent.click(menu.getByText("Issue cert from this CSR…"));

    const dialog = await screen.findByRole("dialog");
    expect((within(dialog).getByTestId("cert-sign-csr") as HTMLInputElement).value).toBe(`${PATH_B}\\bravo.csr`);

    await fireEvent.input(within(dialog).getByTestId("cert-sign-ca-cert"), { target: { value: `${PATH_B}\\ca.pem` } });
    await fireEvent.input(within(dialog).getByTestId("cert-sign-ca-key"), { target: { value: `${PATH_B}\\ca.key` } });
    await fireEvent.click(within(dialog).getByTestId("cert-sign-confirm"));

    await waitFor(() => expect(certIssueCalls.length).toBe(1));
    expect(certIssueCalls[0].csrPath).toBe(`${PATH_B}\\bravo.csr`); // NOT anything from pane A
  });

  it("'Sign with this as CA…' from a pane-B .pem row pre-fills the CA certificate field", async () => {
    const { paneBWrap } = await bootDualPane();
    const pemRow = within(paneBWrap).getByText("bravo.pem").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(pemRow);
    const menu = within(await screen.findByRole("menu"));
    expect(menu.queryByText("Issue cert from this CSR…")).toBeNull(); // gated: a cert file offers sign-as-CA, not issue
    await fireEvent.click(menu.getByText("Sign with this as CA…"));

    const dialog = await screen.findByRole("dialog");
    expect((within(dialog).getByTestId("cert-sign-ca-cert") as HTMLInputElement).value).toBe(`${PATH_B}\\bravo.pem`);
  });

  it("a .jwt row offers 'Inspect JWT' but no cert rows", async () => {
    const { paneBWrap } = await bootDualPane();
    const jwtRow = within(paneBWrap).getByText("bravo.jwt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(jwtRow);
    const menu = within(await screen.findByRole("menu"));
    expect(menu.getByText("Inspect JWT")).toBeTruthy();
    expect(menu.queryByText("Issue cert from this CSR…")).toBeNull();
    expect(menu.queryByText("Sign with this as CA…")).toBeNull();
  });

  it("an ordinary .txt row offers none of the cert/JWT rows", async () => {
    const { paneBWrap } = await bootDualPane();
    const txtRow = within(paneBWrap).getByText("bravo.txt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(txtRow);
    const menu = within(await screen.findByRole("menu"));
    expect(menu.queryByText("Issue cert from this CSR…")).toBeNull();
    expect(menu.queryByText("Sign with this as CA…")).toBeNull();
    expect(menu.queryByText("Inspect JWT")).toBeNull();
    expect(menu.queryByText("Inspect")).toBeNull();
  });
});
