/**
 * VaultCreateDialog (CPE-1250, epic CPE-738): the CREATE dialog gating `vault_create`. Verifies the
 * passphrase-match gate, the sibling-default destination, the shred-checkbox → honest-warning gate, the
 * remember-checkbox default + wiring, and the created/error dispatch contract. Mocks the Tauri `invoke`
 * boundary + the native save dialog, same convention as ShredConfirmDialog.test.ts.
 *
 * CPE-1646 adds a dedicated describe block proving `confirmed` is set by a genuinely separate act from
 * `shredOriginal` — the bug this ticket fixed was the dialog passing the very same variable for both.
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
  it("Create calls vault_create with folder/dest/passphrase/shredOriginal/confirmed and dispatches created", async () => {
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
        confirmed: false,
      }),
    );
    await waitFor(() => expect(created).toHaveBeenCalledWith("/home/me/Secrets.cpevault"));
    // Not remembering by default → no keychain write.
    expect(invoke).not.toHaveBeenCalledWith("vault_remember_passphrase", expect.anything());
  });

  // CPE-1630/1646: this dialog is the ONE caller allowed to set `confirmed: true` on `vault_create` — the
  // backend engine refuses to shred the original unless it's set (mirroring CPE-1611's `shred_paths`
  // gate). `confirmed` is set by the SEPARATE "I understand…" acknowledgement checkbox rendered inside
  // the warning panel, not by the shred checkbox itself — see the dedicated describe block below for the
  // tests that pin down the separation. This one just confirms the end-to-end wiring still works when
  // both boxes are checked.
  it("passes confirmed: true alongside shredOriginal: true once BOTH the shred box and its acknowledgement are checked", async () => {
    render(VaultCreateDialog, base);
    await fireEvent.click(screen.getByTestId("vault-shred"));
    await fireEvent.click(screen.getByTestId("vault-shred-confirm"));
    await fireEvent.input(screen.getByTestId("vault-passphrase"), { target: { value: "pw" } });
    await fireEvent.input(screen.getByTestId("vault-passphrase-confirm"), { target: { value: "pw" } });
    await fireEvent.click(screen.getByTestId("vault-create-confirm"));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("vault_create", {
        folder: "/home/me/Secrets",
        dest: "/home/me/Secrets.cpevault",
        passphrase: "pw",
        shredOriginal: true,
        confirmed: true,
      }),
    );
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

// CPE-1646: the dialog used to pass ONE variable (`shredOriginal`) as BOTH the intent AND the consent
// argument to `vault_create` — `commands.vaultCreate(folderPath, dest, passphrase, shredOriginal,
// shredOriginal)`. A test that only asserts "the backend was called with confirmed: true" can't tell
// that apart from a correctly-separated implementation, since both produce the same call once the user
// has gone through the full checked-and-submitted flow. These tests are built to distinguish the two: they
// assert that flipping the INTENT alone — with the separate acknowledgement never touched — can NOT put
// the dialog into a state where `vault_create` would be called with `confirmed: true`. Under the old
// (collapsed) code, checking the shred box alone flips `confirmed` to `true` for free; these go red
// against that shape and green against the fix.
describe("VaultCreateDialog — CPE-1646: shred consent is a separate act from shred intent", () => {
  it("checking the shred checkbox (intent) alone never satisfies consent, no matter how many times it's toggled", async () => {
    render(VaultCreateDialog, base);
    await fireEvent.input(screen.getByTestId("vault-passphrase"), { target: { value: "pw" } });
    await fireEvent.input(screen.getByTestId("vault-passphrase-confirm"), { target: { value: "pw" } });

    const create = screen.getByTestId("vault-create-confirm") as HTMLButtonElement;
    expect(create.disabled).toBe(false); // valid + non-destructive: no friction

    const shred = screen.getByTestId("vault-shred") as HTMLInputElement;
    await fireEvent.click(shred); // intent -> true
    expect(create.disabled).toBe(true); // intent alone must not grant consent

    // Toggling intent off and back on repeatedly must not launder consent through repetition either —
    // this is the concrete "future drift" risk the ticket named (a restored draft, a deep-link default,
    // a copy-paste refactor re-setting shredOriginal to true). End on intent = true (checked).
    await fireEvent.click(shred); // -> false
    await fireEvent.click(shred); // -> true
    expect(shred.checked).toBe(true);
    expect(create.disabled).toBe(true);

    // Force the click through anyway: the component's own guard (not just the disabled attribute) must
    // refuse to call the backend with an unconfirmed shred.
    await fireEvent.click(create);
    expect(invoke).not.toHaveBeenCalledWith("vault_create", expect.anything());
  });

  it("only the separate acknowledgement checkbox — not the shred checkbox — can make confirmed: true reach the backend", async () => {
    render(VaultCreateDialog, base);
    await fireEvent.input(screen.getByTestId("vault-passphrase"), { target: { value: "pw" } });
    await fireEvent.input(screen.getByTestId("vault-passphrase-confirm"), { target: { value: "pw" } });
    await fireEvent.click(screen.getByTestId("vault-shred"));

    const create = screen.getByTestId("vault-create-confirm") as HTMLButtonElement;
    expect(create.disabled).toBe(true); // intent set, consent not yet given

    await fireEvent.click(screen.getByTestId("vault-shred-confirm")); // the distinct affirmative act
    expect(create.disabled).toBe(false);

    await fireEvent.click(create);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("vault_create", {
        folder: "/home/me/Secrets",
        dest: "/home/me/Secrets.cpevault",
        passphrase: "pw",
        shredOriginal: true,
        confirmed: true,
      }),
    );
  });

  it("unchecking the shred box drops the consent too, so re-checking it demands a fresh acknowledgement", async () => {
    render(VaultCreateDialog, base);
    await fireEvent.input(screen.getByTestId("vault-passphrase"), { target: { value: "pw" } });
    await fireEvent.input(screen.getByTestId("vault-passphrase-confirm"), { target: { value: "pw" } });

    const shred = screen.getByTestId("vault-shred") as HTMLInputElement;
    const create = screen.getByTestId("vault-create-confirm") as HTMLButtonElement;

    await fireEvent.click(shred);
    await fireEvent.click(screen.getByTestId("vault-shred-confirm"));
    expect(create.disabled).toBe(false);

    await fireEvent.click(shred); // uncheck — intent off, warning + ack unmount
    expect(shred.checked).toBe(false);
    await fireEvent.click(shred); // recheck — intent on again, a FRESH ack checkbox is mounted
    expect((screen.getByTestId("vault-shred-confirm") as HTMLInputElement).checked).toBe(false);
    expect(create.disabled).toBe(true);
  });

  it("with the shred option off entirely, Create is never gated by the (absent) acknowledgement", async () => {
    render(VaultCreateDialog, base);
    await fireEvent.input(screen.getByTestId("vault-passphrase"), { target: { value: "pw" } });
    await fireEvent.input(screen.getByTestId("vault-passphrase-confirm"), { target: { value: "pw" } });

    expect(screen.queryByTestId("vault-shred-confirm")).toBeNull();
    expect((screen.getByTestId("vault-create-confirm") as HTMLButtonElement).disabled).toBe(false);
  });
});
