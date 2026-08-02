/**
 * VaultCreateDialog (CPE-1250, epic CPE-738): the CREATE dialog gating `vault_create`. Verifies the
 * passphrase-match gate, the sibling-default destination, the shred-checkbox → honest-warning gate, the
 * remember-checkbox default + wiring, and the created/error dispatch contract. Mocks the Tauri `invoke`
 * boundary + the native save dialog, same convention as ShredConfirmDialog.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import VaultCreateDialog from "./VaultCreateDialog.svelte";

let createOk = true;
const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "vault_create") {
    if (!createOk) throw new Error("refusing to shred: destination vault is inside the folder");
    return null;
  }
  if (cmd === "vault_remember_passphrase") return null;
  throw new Error(`unexpected invoke: ${cmd}`);
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args),
  Channel: class {},
}));
const saveDialog = vi.fn(async (_opts?: unknown) => "/picked/Chosen.cpevault");
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: (opts?: unknown) => saveDialog(opts) }));

const base = { folderPath: "/home/me/Secrets", folderName: "Secrets", rememberDefault: false };

beforeEach(() => {
  createOk = true;
  invoke.mockClear();
  saveDialog.mockClear();
});

describe("VaultCreateDialog — defaults + validation", () => {
  it("defaults the destination to a SIBLING <foldername>.cpevault", () => {
    render(VaultCreateDialog, base);
    const dest = screen.getByTestId("vault-dest") as HTMLInputElement;
    expect(dest.value).toBe("/home/me/Secrets.cpevault");
  });

  it("keeps Create disabled until both passphrases are non-empty and match", async () => {
    render(VaultCreateDialog, base);
    const create = screen.getByTestId("vault-create-confirm") as HTMLButtonElement;
    expect(create.disabled).toBe(true); // empty passphrases

    await fireEvent.input(screen.getByTestId("vault-passphrase"), { target: { value: "hunter2" } });
    await fireEvent.input(screen.getByTestId("vault-passphrase-confirm"), { target: { value: "nope" } });
    expect(create.disabled).toBe(true); // mismatch
    expect(screen.getByTestId("vault-passphrase-error")).toBeTruthy();

    await fireEvent.input(screen.getByTestId("vault-passphrase-confirm"), { target: { value: "hunter2" } });
    expect(screen.queryByTestId("vault-passphrase-error")).toBeNull();
    expect(create.disabled).toBe(false);
  });

  it("warns that a forgotten passphrase is unrecoverable", () => {
    render(VaultCreateDialog, base);
    const note = screen.getByTestId("vault-passphrase-note").textContent ?? "";
    expect(note.toLowerCase()).toContain("recover");
  });
});

describe("VaultCreateDialog — both password fields have a consistent show/hide toggle", () => {
  it("each twin field has its own eye toggle that flips its input between password and text", async () => {
    render(VaultCreateDialog, base);
    const passField = screen.getByTestId("vault-passphrase") as HTMLInputElement;
    const confirmField = screen.getByTestId("vault-passphrase-confirm") as HTMLInputElement;
    const passToggle = screen.getByTestId("vault-passphrase-toggle");
    const confirmToggle = screen.getByTestId("vault-passphrase-confirm-toggle");

    // Both start masked.
    expect(passField.type).toBe("password");
    expect(confirmField.type).toBe("password");

    // Independent reveal: toggling the passphrase does NOT reveal the confirm.
    await fireEvent.click(passToggle);
    expect(passField.type).toBe("text");
    expect(confirmField.type).toBe("password");
    expect(passToggle.getAttribute("aria-label")).toBe("Hide passphrase");

    await fireEvent.click(confirmToggle);
    expect(confirmField.type).toBe("text");

    // Toggle back to masked.
    await fireEvent.click(passToggle);
    expect(passField.type).toBe("password");
    expect(passToggle.getAttribute("aria-label")).toBe("Show passphrase");
  });
});

describe("VaultCreateDialog — shred checkbox gates the honest warning", () => {
  it("hides the destructive warning by default and shows it only when checked (off by default)", async () => {
    render(VaultCreateDialog, base);
    const shred = screen.getByTestId("vault-shred") as HTMLInputElement;
    expect(shred.checked).toBe(false);
    expect(screen.queryByTestId("vault-shred-warning")).toBeNull();

    await fireEvent.click(shred);
    const warn = screen.getByTestId("vault-shred-warning");
    const text = (warn.textContent ?? "").toLowerCase();
    // Honest tone reused from ShredConfirmDialog: permanent + best-effort on SSD/copy-on-write.
    expect(text).toContain("permanently");
    expect(text).toContain("best-effort");
    expect(text).toContain("ssd");
    expect(text).toContain("copy-on-write");
  });
});

describe("VaultCreateDialog — remember checkbox", () => {
  it("defaults the remember checkbox from the rememberDefault prop", () => {
    render(VaultCreateDialog, { ...base, rememberDefault: true });
    expect((screen.getByTestId("vault-remember") as HTMLInputElement).checked).toBe(true);
  });
});

describe("VaultCreateDialog — Browse picker", () => {
  it("Browse opens the native save dialog and adopts the chosen path", async () => {
    render(VaultCreateDialog, base);
    await fireEvent.click(screen.getByTestId("vault-dest-browse"));
    await waitFor(() => expect(saveDialog).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect((screen.getByTestId("vault-dest") as HTMLInputElement).value).toBe("/picked/Chosen.cpevault"),
    );
  });
});

describe("VaultCreateDialog — backend wiring + dispatch", () => {
  it("Create calls vault_create with folder/dest/passphrase/shredOriginal and dispatches created", async () => {
    const { component } = render(VaultCreateDialog, base);
    const created = vi.fn();
    component.$on("created", (e: CustomEvent<string>) => created(e.detail));

    await fireEvent.input(screen.getByTestId("vault-passphrase"), { target: { value: "pw" } });
    await fireEvent.input(screen.getByTestId("vault-passphrase-confirm"), { target: { value: "pw" } });
    await fireEvent.click(screen.getByTestId("vault-create-confirm"));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("vault_create", {
        folder: "/home/me/Secrets",
        dest: "/home/me/Secrets.cpevault",
        passphrase: "pw",
        shredOriginal: false,
      }),
    );
    await waitFor(() => expect(created).toHaveBeenCalledWith("/home/me/Secrets.cpevault"));
    // Not remembering by default → no keychain write.
    expect(invoke).not.toHaveBeenCalledWith("vault_remember_passphrase", expect.anything());
  });

  it("remembers the passphrase in the keychain when the checkbox is on", async () => {
    render(VaultCreateDialog, { ...base, rememberDefault: true });
    await fireEvent.input(screen.getByTestId("vault-passphrase"), { target: { value: "pw" } });
    await fireEvent.input(screen.getByTestId("vault-passphrase-confirm"), { target: { value: "pw" } });
    await fireEvent.click(screen.getByTestId("vault-create-confirm"));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("vault_remember_passphrase", {
        blobPath: "/home/me/Secrets.cpevault",
        passphrase: "pw",
      }),
    );
  });

  it("surfaces a backend VaultError inline and via the error event, staying open", async () => {
    createOk = false;
    const { component } = render(VaultCreateDialog, base);
    const errorSpy = vi.fn();
    const created = vi.fn();
    component.$on("error", (e: CustomEvent<string>) => errorSpy(e.detail));
    component.$on("created", created);

    await fireEvent.input(screen.getByTestId("vault-passphrase"), { target: { value: "pw" } });
    await fireEvent.input(screen.getByTestId("vault-passphrase-confirm"), { target: { value: "pw" } });
    await fireEvent.click(screen.getByTestId("vault-create-confirm"));

    await waitFor(() => expect(errorSpy).toHaveBeenCalledTimes(1));
    expect(errorSpy.mock.calls[0][0].toLowerCase()).toContain("refusing to shred");
    expect(screen.getByTestId("vault-create-error")).toBeTruthy();
    expect(created).not.toHaveBeenCalled();
    expect(screen.getByTestId("vault-cancel")).toBeTruthy(); // still open
  });

  it("Cancel dispatches close without calling the backend", async () => {
    const { component } = render(VaultCreateDialog, base);
    const close = vi.fn();
    component.$on("close", close);
    await fireEvent.click(screen.getByTestId("vault-cancel"));
    expect(close).toHaveBeenCalledTimes(1);
    expect(invoke).not.toHaveBeenCalled();
  });
});
