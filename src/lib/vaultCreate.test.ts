// Pure create-dialog logic (CPE-1250, epic CPE-738): default destination = sibling, passphrase
// match/mismatch, the create-enabled gate, and the shred-checkbox → warning gate. DOM/Tauri-free.
import { describe, it, expect } from "vitest";
import {
  VAULT_EXT,
  siblingVaultDest,
  checkPassphrases,
  canCreate,
  shouldShowShredWarning,
} from "./vaultCreate";

describe("siblingVaultDest — default destination is a SIBLING of the folder", () => {
  it("puts <foldername>.cpevault in the folder's PARENT (POSIX)", () => {
    expect(siblingVaultDest("/home/me/Secrets")).toBe(`/home/me/Secrets${VAULT_EXT}`);
  });

  it("preserves Windows separators + drive paths", () => {
    expect(siblingVaultDest("C:\\Users\\me\\Secrets")).toBe(`C:\\Users\\me\\Secrets${VAULT_EXT}`);
  });

  it("tolerates a trailing separator", () => {
    expect(siblingVaultDest("/home/me/Secrets/")).toBe(`/home/me/Secrets${VAULT_EXT}`);
    expect(siblingVaultDest("C:\\Users\\me\\Secrets\\")).toBe(`C:\\Users\\me\\Secrets${VAULT_EXT}`);
  });

  it("never nests the blob INSIDE the folder (the shred data-loss guard's requirement)", () => {
    const folder = "/home/me/Secrets";
    const dest = siblingVaultDest(folder);
    expect(dest.startsWith(folder + "/")).toBe(false);
    expect(dest.startsWith(folder + "\\")).toBe(false);
  });

  it("handles a bare name with no parent", () => {
    expect(siblingVaultDest("Secrets")).toBe(`Secrets${VAULT_EXT}`);
  });
});

describe("checkPassphrases — must match, and neither empty", () => {
  it("accepts two identical non-empty passphrases", () => {
    expect(checkPassphrases("hunter2", "hunter2")).toEqual({ ok: true, error: "" });
  });

  it("rejects a mismatch with a clear message", () => {
    const r = checkPassphrases("hunter2", "hunter3");
    expect(r.ok).toBe(false);
    expect(r.error.toLowerCase()).toContain("match");
  });

  it("rejects an empty passphrase BEFORE reporting a mismatch", () => {
    const r = checkPassphrases("", "");
    expect(r.ok).toBe(false);
    expect(r.error.toLowerCase()).toContain("passphrase");
    // Priority: empty is reported even when the (also empty) confirm technically "matches".
    expect(r.error.toLowerCase()).not.toContain("match");
  });
});

describe("canCreate — the button-enabled gate", () => {
  const ok = { passphrase: "pw", confirm: "pw", dest: "/x/v.cpevault", busy: false };

  it("is true only when passphrases match, a dest is set, and not busy", () => {
    expect(canCreate(ok)).toBe(true);
  });

  it("is false while a create is in flight", () => {
    expect(canCreate({ ...ok, busy: true })).toBe(false);
  });

  it("is false when passphrases don't match", () => {
    expect(canCreate({ ...ok, confirm: "different" })).toBe(false);
  });

  it("is false when the passphrase is empty", () => {
    expect(canCreate({ ...ok, passphrase: "", confirm: "" })).toBe(false);
  });

  it("is false when the destination is blank", () => {
    expect(canCreate({ ...ok, dest: "   " })).toBe(false);
  });
});

describe("shouldShowShredWarning — shred checkbox gates the destructive warning", () => {
  it("shows the warning only when 'securely delete the original' is on", () => {
    expect(shouldShowShredWarning(true)).toBe(true);
    expect(shouldShowShredWarning(false)).toBe(false);
  });
});
