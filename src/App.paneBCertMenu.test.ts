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
// Pane A carries its own cert + JWT rows too (CPE-1438), so "Inspect" can be proven pane-aware from
// EITHER pane — the overlay must decode the file from the pane the menu was opened over, not the other.
const entriesA: DirEntry[] = [
  file("alpha.txt", PATH_A, "txt"),
  file("alpha.pem", PATH_A, "pem"),
  file("alpha.jwt", PATH_A, "jwt"),
];
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
// CPE-1438: paths the Inspect overlay's viewers actually fetch a decode for — proof the action WORKED
// (mounted a viewer that hit the backend), not the old silent no-op, and that it targeted the right pane.
let jwtPreviewPaths: string[] = [];
let certDecodePaths: string[] = [];

// Minimal decode payloads the reused JwtPreview/CertPreview viewers render (shapes from their own specs).
const jwtDecode = {
  alg: "HS256", typ: "JWT", kid: null,
  header_json: '{"alg":"HS256"}', payload_json: '{"sub":"abc"}',
  exp: null, iat: null, nbf: null, expired: null, not_yet_valid: null,
  signature_present: true, signature_len: 32, error: null,
};
const certDecode = {
  kind: "certificate", encoding: "PEM",
  certificate: {
    subject: "CN=inspected.local", issuer: "CN=inspected.local", serial: "01", version: "v3",
    not_before: "2026-01-01T00:00:00Z", not_after: "2027-01-01T00:00:00Z",
    expired: false, not_yet_valid: false,
    signature_algorithm: "sha256WithRSAEncryption",
    public_key: { algorithm: "RSA", size_bits: 2048, curve: null },
    subject_alt_names: [], is_ca: false, key_usage: [], extended_key_usage: [],
    sha256_fingerprint: "aa", sha1_fingerprint: "bb",
  },
  csr: null, public_key: null, private_key: null, error: null,
};

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  certCreateCalls = [];
  certIssueCalls = [];
  jwtPreviewPaths = [];
  certDecodePaths = [];
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
      case "jwt_preview": {
        jwtPreviewPaths.push(args.path as string);
        return jwtDecode;
      }
      case "cert_decode": {
        certDecodePaths.push(args.path as string);
        return certDecode;
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

// CPE-1438 (epic CPE-1417): "Inspect" / "Inspect JWT" was a silent no-op in dual-pane — the inline
// preview slot the action relied on is occupied by pane B's ExplorerPane, so `inspectCryptoFile`'s flag
// flips did nothing. The fix routes dual-pane Inspect through an overlay (InspectCryptoDialog) that
// reuses JwtPreview/CertPreview. These specs cross BOTH composed features (CPE-677 dual-pane × CPE-1424
// inspect) that each passed in isolation, asserting the action produces a REAL decode — not a no-op —
// and honors the pane the menu was opened over. Mutation check: revert `inspectCryptoFile` to the old
// flag-only body (drop the `if (dualPane) { … cryptoInspectFor … }` branch) and every case here fails
// (no overlay ever mounts, no jwt_preview/cert_decode call fires).
describe("App — 'Inspect'/'Inspect JWT' WORKS in dual-pane via an overlay, pane-aware (CPE-1438)", () => {
  it("'Inspect JWT' on a pane-B .jwt row opens the overlay and decodes THAT file (not a no-op)", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    // Pane A stays "active" — the pane-B menu must still inspect pane B's file, not pane A's.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(within(paneAWrap).getByText("alpha.txt"));

    const jwtRow = within(paneBWrap).getByText("bravo.jwt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(jwtRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Inspect JWT"));

    // Overlay mounted with the JWT viewer + real decoded fields — the old code showed nothing.
    const dialog = await screen.findByTestId("crypto-inspect-dialog");
    await waitFor(() => expect(within(dialog).getByTestId("jwt-preview")).toBeTruthy());
    expect(within(dialog).getByText("HS256")).toBeTruthy();
    // Decoded pane B's file, never pane A's.
    expect(jwtPreviewPaths).toContain(`${PATH_B}\\bravo.jwt`);
    expect(jwtPreviewPaths).not.toContain(`${PATH_A}\\alpha.jwt`);
  });

  it("'Inspect' on a pane-B cert (.pem) row opens the overlay and decodes THAT file (not a no-op)", async () => {
    const { paneBWrap } = await bootDualPane();
    const pemRow = within(paneBWrap).getByText("bravo.pem").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(pemRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Inspect"));

    const dialog = await screen.findByTestId("crypto-inspect-dialog");
    await waitFor(() => expect(within(dialog).getByTestId("cert-preview")).toBeTruthy());
    // Self-signed cert: subject == issuer, so the DN renders twice — real decoded content, not a no-op.
    expect(within(dialog).getAllByText("CN=inspected.local").length).toBeGreaterThan(0);
    expect(certDecodePaths).toContain(`${PATH_B}\\bravo.pem`);
  });

  it("'Inspect JWT' on a pane-A .jwt row inspects pane A's file even when pane B is active", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();
    // Make pane B "active" first — the pane-A menu must still target pane A by construction.
    await fireEvent.click(paneBWrap);
    await fireEvent.click(within(paneBWrap).getByText("bravo.txt"));

    const jwtRow = within(paneAWrap).getByText("alpha.jwt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(jwtRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Inspect JWT"));

    const dialog = await screen.findByTestId("crypto-inspect-dialog");
    await waitFor(() => expect(within(dialog).getByTestId("jwt-preview")).toBeTruthy());
    expect(jwtPreviewPaths).toContain(`${PATH_A}\\alpha.jwt`);
    expect(jwtPreviewPaths).not.toContain(`${PATH_B}\\bravo.jwt`);
  });

  it("'Inspect' on a pane-A cert (.pem) row inspects pane A's file", async () => {
    const { paneAWrap } = await bootDualPane();
    const pemRow = within(paneAWrap).getByText("alpha.pem").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(pemRow);
    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Inspect"));

    const dialog = await screen.findByTestId("crypto-inspect-dialog");
    await waitFor(() => expect(within(dialog).getByTestId("cert-preview")).toBeTruthy());
    expect(certDecodePaths).toContain(`${PATH_A}\\alpha.pem`);
  });

  it("closes the overlay on the Close button", async () => {
    const { paneBWrap } = await bootDualPane();
    const jwtRow = within(paneBWrap).getByText("bravo.jwt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(jwtRow);
    await fireEvent.click(within(await screen.findByRole("menu")).getByText("Inspect JWT"));
    const dialog = await screen.findByTestId("crypto-inspect-dialog");

    await fireEvent.click(within(dialog).getByTestId("crypto-inspect-close"));
    await waitFor(() => expect(screen.queryByTestId("crypto-inspect-dialog")).toBeNull());
  });

  it("single-pane mode keeps the inline preview — no overlay (must not regress)", async () => {
    saveDualPane(false);
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.jwt")).toBeTruthy());

    const jwtRow = screen.getByText("alpha.jwt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(jwtRow);
    await fireEvent.click(within(await screen.findByRole("menu")).getByText("Inspect JWT"));

    // Single-pane routes to the inline PreviewPane (unchanged) — it must NOT pop the dual-pane overlay.
    expect(screen.queryByTestId("crypto-inspect-dialog")).toBeNull();
  });
});
