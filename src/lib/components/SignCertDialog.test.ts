/**
 * SignCertDialog (CPE-1423, epic CPE-1417): the ISSUE-FROM-CSR dialog gating `cert_issue_from_csr`.
 * Verifies prefill from the pane-aware context menu (`csrPath`/`caCertPath` props), the smart default
 * output path, the issue-enabled gate, the three file Browse pickers + the save picker, and the
 * created/error dispatch contract. Mocks the Tauri `invoke` boundary + the native dialogs, same
 * convention as VaultCreateDialog.test.ts / CreateCertDialog.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import SignCertDialog from "./SignCertDialog.svelte";

let issueOk = true;
const invoke = vi.fn(async (cmd: string, _args?: unknown) => {
  if (cmd === "cert_issue_from_csr") {
    if (!issueOk) throw new Error("failed to parse CSR");
    return null;
  }
  throw new Error(`unexpected invoke: ${cmd}`);
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args),
  Channel: class {},
}));
const openDialog = vi.fn(async (_opts?: unknown) => "/picked/file.pem");
const saveDialog = vi.fn(async (_opts?: unknown) => "/picked/issued.crt");
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (opts?: unknown) => openDialog(opts),
  save: (opts?: unknown) => saveDialog(opts),
}));

beforeEach(() => {
  issueOk = true;
  invoke.mockClear();
  openDialog.mockClear();
  saveDialog.mockClear();
});

describe("SignCertDialog — prefill + smart default output path", () => {
  it("prefills csrPath from the 'Issue cert from this CSR…' context-menu action", () => {
    render(SignCertDialog, { csrPath: "/a/service.csr", outDir: "/out" });
    expect((screen.getByTestId("cert-sign-csr") as HTMLInputElement).value).toBe("/a/service.csr");
    // Default output path derives from the CSR's basename + the target folder.
    expect((screen.getByTestId("cert-sign-out") as HTMLInputElement).value).toBe("/out/service.crt");
  });

  it("prefills caCertPath from the 'Sign with this as CA…' context-menu action", () => {
    render(SignCertDialog, { caCertPath: "/a/ca.pem", outDir: "/out" });
    expect((screen.getByTestId("cert-sign-ca-cert") as HTMLInputElement).value).toBe("/a/ca.pem");
  });

  it("nothing is prefilled from the command palette (no file context)", () => {
    render(SignCertDialog, {});
    expect((screen.getByTestId("cert-sign-csr") as HTMLInputElement).value).toBe("");
    expect((screen.getByTestId("cert-sign-ca-cert") as HTMLInputElement).value).toBe("");
    expect((screen.getByTestId("cert-sign-out") as HTMLInputElement).value).toBe("");
  });
});

describe("SignCertDialog — issue-enabled gate", () => {
  it("keeps Issue disabled until every path field is set", async () => {
    render(SignCertDialog, { csrPath: "/a/service.csr", outDir: "/out" });
    const issue = screen.getByTestId("cert-sign-confirm") as HTMLButtonElement;
    expect(issue.disabled).toBe(true); // CA cert/key still empty

    await fireEvent.input(screen.getByTestId("cert-sign-ca-cert"), { target: { value: "/a/ca.pem" } });
    expect(issue.disabled).toBe(true);

    await fireEvent.input(screen.getByTestId("cert-sign-ca-key"), { target: { value: "/a/ca.key" } });
    expect(issue.disabled).toBe(false); // output path already defaulted from the CSR + outDir
  });
});

describe("SignCertDialog — Browse pickers", () => {
  it("each Browse button opens the native picker and adopts the chosen path", async () => {
    render(SignCertDialog, { outDir: "/out" });

    await fireEvent.click(screen.getByTestId("cert-sign-csr-browse"));
    await waitFor(() => expect((screen.getByTestId("cert-sign-csr") as HTMLInputElement).value).toBe("/picked/file.pem"));

    await fireEvent.click(screen.getByTestId("cert-sign-ca-cert-browse"));
    await waitFor(() => expect((screen.getByTestId("cert-sign-ca-cert") as HTMLInputElement).value).toBe("/picked/file.pem"));

    await fireEvent.click(screen.getByTestId("cert-sign-ca-key-browse"));
    await waitFor(() => expect((screen.getByTestId("cert-sign-ca-key") as HTMLInputElement).value).toBe("/picked/file.pem"));

    expect(openDialog).toHaveBeenCalledTimes(3);

    await fireEvent.click(screen.getByTestId("cert-sign-out-browse"));
    await waitFor(() => expect((screen.getByTestId("cert-sign-out") as HTMLInputElement).value).toBe("/picked/issued.crt"));
    expect(saveDialog).toHaveBeenCalledTimes(1);
  });
});

describe("SignCertDialog — backend wiring + dispatch", () => {
  it("Issue calls cert_issue_from_csr with the right args and dispatches created", async () => {
    const { component } = render(SignCertDialog, {
      csrPath: "/a/service.csr",
      caCertPath: "/a/ca.pem",
      outDir: "/out",
    });
    const created = vi.fn();
    component.$on("created", (e: CustomEvent<string>) => created(e.detail));

    await fireEvent.input(screen.getByTestId("cert-sign-ca-key"), { target: { value: "/a/ca.key" } });
    await fireEvent.click(screen.getByTestId("cert-sign-confirm"));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("cert_issue_from_csr", {
        csrPath: "/a/service.csr",
        caCertPath: "/a/ca.pem",
        caKeyPath: "/a/ca.key",
        validityDays: 365,
        outCertPath: "/out/service.crt",
      }),
    );
    await waitFor(() => expect(created).toHaveBeenCalledWith("/out/service.crt"));
  });

  it("surfaces a backend error inline and via the error event, staying open", async () => {
    issueOk = false;
    const { component } = render(SignCertDialog, {
      csrPath: "/a/service.csr",
      caCertPath: "/a/ca.pem",
      outDir: "/out",
    });
    const errorSpy = vi.fn();
    const created = vi.fn();
    component.$on("error", (e: CustomEvent<string>) => errorSpy(e.detail));
    component.$on("created", created);

    await fireEvent.input(screen.getByTestId("cert-sign-ca-key"), { target: { value: "/a/ca.key" } });
    await fireEvent.click(screen.getByTestId("cert-sign-confirm"));

    await waitFor(() => expect(errorSpy).toHaveBeenCalledTimes(1));
    expect(errorSpy.mock.calls[0][0].toLowerCase()).toContain("parse csr");
    expect(screen.getByTestId("cert-sign-error")).toBeTruthy();
    expect(created).not.toHaveBeenCalled();
  });

  it("Cancel dispatches close without calling the backend", async () => {
    const { component } = render(SignCertDialog, { outDir: "/out" });
    const close = vi.fn();
    component.$on("close", close);
    await fireEvent.click(screen.getByTestId("cert-sign-cancel"));
    expect(close).toHaveBeenCalledTimes(1);
    expect(invoke).not.toHaveBeenCalled();
  });
});
