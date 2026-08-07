/**
 * CreateCertDialog (CPE-1423, epic CPE-1417): the CREATE dialog gating `cert_create`. Verifies the
 * smart-default filenames from the CN, the create-enabled gate, the SAN DNS/IP reflowing pill inputs,
 * the folder Browse picker, and the created/error dispatch contract. Mocks the Tauri `invoke` boundary +
 * the native folder-picker, same convention as VaultCreateDialog.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import CreateCertDialog from "./CreateCertDialog.svelte";

let createOk = true;
const invoke = vi.fn(async (cmd: string, _args?: unknown) => {
  if (cmd === "cert_create") {
    if (!createOk) throw new Error("common name must not be empty");
    return null;
  }
  throw new Error(`unexpected invoke: ${cmd}`);
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args),
  Channel: class {},
}));
const folderDialog = vi.fn(async (_opts?: unknown) => "/picked/out");
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: (opts?: unknown) => folderDialog(opts) }));

beforeEach(() => {
  createOk = true;
  invoke.mockClear();
  folderDialog.mockClear();
});

describe("CreateCertDialog — smart filename defaults + validation", () => {
  it("defaults the cert/key filenames from the common name", async () => {
    render(CreateCertDialog, { outDir: "/out" });
    await fireEvent.input(screen.getByTestId("cert-create-cn"), { target: { value: "svc.local" } });
    expect((screen.getByTestId("cert-create-certname") as HTMLInputElement).value).toBe("svc.local.pem");
    expect((screen.getByTestId("cert-create-keyname") as HTMLInputElement).value).toBe("svc.local.key");
  });

  it("stops auto-tracking a filename once the user edits it directly", async () => {
    render(CreateCertDialog, { outDir: "/out" });
    await fireEvent.input(screen.getByTestId("cert-create-certname"), { target: { value: "custom.pem" } });
    await fireEvent.input(screen.getByTestId("cert-create-cn"), { target: { value: "svc.local" } });
    expect((screen.getByTestId("cert-create-certname") as HTMLInputElement).value).toBe("custom.pem");
    // The untouched key filename still tracks the CN.
    expect((screen.getByTestId("cert-create-keyname") as HTMLInputElement).value).toBe("svc.local.key");
  });

  it("keeps Create disabled until CN + output path are set", async () => {
    render(CreateCertDialog, { outDir: "" });
    const create = screen.getByTestId("cert-create-confirm") as HTMLButtonElement;
    expect(create.disabled).toBe(true);

    await fireEvent.input(screen.getByTestId("cert-create-cn"), { target: { value: "svc.local" } });
    expect(create.disabled).toBe(true); // still no output folder

    await fireEvent.input(screen.getByTestId("cert-create-folder"), { target: { value: "/out" } });
    expect(create.disabled).toBe(false);
  });
});

describe("CreateCertDialog — SAN pill inputs (tick-tacks)", () => {
  it("adds a DNS SAN on Enter and removes it via its pill button", async () => {
    render(CreateCertDialog, { outDir: "/out" });
    const input = screen.getByTestId("cert-create-dns-input");
    await fireEvent.input(input, { target: { value: "alt.local" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(screen.getByText("alt.local")).toBeTruthy();
    expect((input as HTMLInputElement).value).toBe(""); // draft cleared after commit

    await fireEvent.click(screen.getByTestId("cert-create-dns-remove-alt.local"));
    expect(screen.queryByText("alt.local")).toBeNull();
  });

  it("adds an IP SAN on comma", async () => {
    render(CreateCertDialog, { outDir: "/out" });
    const input = screen.getByTestId("cert-create-ip-input");
    await fireEvent.input(input, { target: { value: "127.0.0.1" } });
    await fireEvent.keyDown(input, { key: "," });
    expect(screen.getByText("127.0.0.1")).toBeTruthy();
  });

  it("Backspace on an empty draft peels the last pill", async () => {
    render(CreateCertDialog, { outDir: "/out" });
    const input = screen.getByTestId("cert-create-dns-input");
    await fireEvent.input(input, { target: { value: "a.local" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(screen.getByText("a.local")).toBeTruthy();

    await fireEvent.keyDown(input, { key: "Backspace" });
    expect(screen.queryByText("a.local")).toBeNull();
  });
});

describe("CreateCertDialog — Browse picker", () => {
  it("Browse opens the native folder dialog and adopts the chosen folder", async () => {
    render(CreateCertDialog, { outDir: "/out" });
    await fireEvent.click(screen.getByTestId("cert-create-folder-browse"));
    await waitFor(() => expect(folderDialog).toHaveBeenCalledTimes(1));
    expect(folderDialog.mock.calls[0][0]).toMatchObject({ directory: true });
    await waitFor(() =>
      expect((screen.getByTestId("cert-create-folder") as HTMLInputElement).value).toBe("/picked/out"),
    );
  });
});

describe("CreateCertDialog — backend wiring + dispatch", () => {
  it("Create calls cert_create with the right params + paths and dispatches created", async () => {
    const { component } = render(CreateCertDialog, { outDir: "/out" });
    const created = vi.fn();
    component.$on("created", (e: CustomEvent<string>) => created(e.detail));

    await fireEvent.input(screen.getByTestId("cert-create-cn"), { target: { value: "svc.local" } });
    await fireEvent.click(screen.getByTestId("cert-create-confirm"));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("cert_create", {
        params: {
          common_name: "svc.local",
          san_dns: [],
          san_ips: [],
          validity_days: 365,
          key_type: "ec_p256",
          is_ca: false,
        },
        certPath: "/out/svc.local.pem",
        keyPath: "/out/svc.local.key",
      }),
    );
    await waitFor(() => expect(created).toHaveBeenCalledWith("/out/svc.local.pem"));
  });

  it("includes SAN DNS/IP entries, key type, and is_ca in the call", async () => {
    render(CreateCertDialog, { outDir: "/out" });
    await fireEvent.input(screen.getByTestId("cert-create-cn"), { target: { value: "svc.local" } });
    const dns = screen.getByTestId("cert-create-dns-input");
    await fireEvent.input(dns, { target: { value: "alt.local" } });
    await fireEvent.keyDown(dns, { key: "Enter" });
    const ip = screen.getByTestId("cert-create-ip-input");
    await fireEvent.input(ip, { target: { value: "10.0.0.1" } });
    await fireEvent.keyDown(ip, { key: "Enter" });
    await fireEvent.click(screen.getByTestId("cert-create-isca"));
    await fireEvent.change(screen.getByTestId("cert-create-keytype"), { target: { value: "rsa_4096" } });

    await fireEvent.click(screen.getByTestId("cert-create-confirm"));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "cert_create",
        expect.objectContaining({
          params: expect.objectContaining({
            san_dns: ["alt.local"],
            san_ips: ["10.0.0.1"],
            key_type: "rsa_4096",
            is_ca: true,
          }),
        }),
      ),
    );
  });

  it("surfaces a backend error inline and via the error event, staying open", async () => {
    createOk = false;
    const { component } = render(CreateCertDialog, { outDir: "/out" });
    const errorSpy = vi.fn();
    const created = vi.fn();
    component.$on("error", (e: CustomEvent<string>) => errorSpy(e.detail));
    component.$on("created", created);

    await fireEvent.input(screen.getByTestId("cert-create-cn"), { target: { value: "svc.local" } });
    await fireEvent.click(screen.getByTestId("cert-create-confirm"));

    await waitFor(() => expect(errorSpy).toHaveBeenCalledTimes(1));
    expect(errorSpy.mock.calls[0][0].toLowerCase()).toContain("common name");
    expect(screen.getByTestId("cert-create-error")).toBeTruthy();
    expect(created).not.toHaveBeenCalled();
    expect(screen.getByTestId("cert-create-cancel")).toBeTruthy(); // still open
  });

  it("Cancel dispatches close without calling the backend", async () => {
    const { component } = render(CreateCertDialog, { outDir: "/out" });
    const close = vi.fn();
    component.$on("close", close);
    await fireEvent.click(screen.getByTestId("cert-create-cancel"));
    expect(close).toHaveBeenCalledTimes(1);
    expect(invoke).not.toHaveBeenCalled();
  });
});
