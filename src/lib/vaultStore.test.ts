import { describe, it, expect, beforeEach } from "vitest";
import { get } from "svelte/store";
import {
  recordUnlocked,
  recordLocked,
  isUnlocked,
  sessionDirFor,
  badgeFor,
  vaultOfSessionPath,
  vaultDisplayName,
  classifyUnlockError,
  classifyLockError,
  vaults,
  unlockVault,
  lockVault,
  resetVaults,
  type VaultState,
  type VaultDeps,
} from "./vaultStore";

const BLOB = "/home/me/secrets.cpevault";
const SESSION = "/cache/app/vault-sessions/abc-123";

describe("vaultStore pure reducers (CPE-1249)", () => {
  it("records an unlocked vault → session-dir mapping without mutating the input", () => {
    const before: VaultState = {};
    const after = recordUnlocked(before, BLOB, SESSION);
    expect(before).toEqual({}); // untouched
    expect(after).toEqual({ [BLOB]: SESSION });
    expect(isUnlocked(after, BLOB)).toBe(true);
    expect(sessionDirFor(after, BLOB)).toBe(SESSION);
  });

  it("clears a locked vault; unknown paths are a no-op copy", () => {
    const state = recordUnlocked({}, BLOB, SESSION);
    const locked = recordLocked(state, BLOB);
    expect(isUnlocked(locked, BLOB)).toBe(false);
    expect(sessionDirFor(locked, BLOB)).toBeUndefined();
    // Unknown path: a fresh copy equal in content, not the same reference.
    const copy = recordLocked(state, "/nope.cpevault");
    expect(copy).toEqual(state);
    expect(copy).not.toBe(state);
  });

  it("derives the badge state purely from the store", () => {
    expect(badgeFor({}, BLOB)).toBe("locked");
    expect(badgeFor(recordUnlocked({}, BLOB, SESSION), BLOB)).toBe("unlocked");
  });

  it("finds which unlocked vault a browsed path belongs to (banner source), separator-tolerant", () => {
    const state = recordUnlocked({}, BLOB, SESSION);
    expect(vaultOfSessionPath(state, SESSION)).toBe(BLOB); // the root itself
    expect(vaultOfSessionPath(state, SESSION + "/notes/hello.txt")).toBe(BLOB); // nested
    expect(vaultOfSessionPath(state, SESSION + "\\notes")).toBe(BLOB); // windows sep
    expect(vaultOfSessionPath(state, "/somewhere/else")).toBeNull();
    // A sibling with a shared prefix must NOT match (prefix-with-separator, not bare startsWith).
    expect(vaultOfSessionPath(state, SESSION + "-decoy/file")).toBeNull();
  });

  it("derives a friendly vault name from the blob path", () => {
    expect(vaultDisplayName("/home/me/Secret Docs.cpevault")).toBe("Secret Docs");
    expect(vaultDisplayName("C:\\Users\\me\\taxes.CPEVAULT")).toBe("taxes");
    expect(vaultDisplayName("bare.cpevault")).toBe("bare");
  });

  it("classifies unlock errors into distinct copy for wrong-password vs damaged vs other", () => {
    expect(classifyUnlockError("incorrect passphrase")).toMatch(/wrong password/i);
    expect(classifyUnlockError("vault is corrupt or has been tampered with")).toMatch(/damaged/i);
    expect(classifyUnlockError("not a CPE vault (bad magic marker)")).toMatch(/isn't a valid vault/i);
    expect(classifyUnlockError("unsupported vault schema version 2")).toMatch(/newer version/i);
    expect(classifyUnlockError("disk on fire")).toMatch(/couldn't unlock/i);
    // The wrong-password branch must win over an unrelated word that happens to co-occur.
    expect(classifyUnlockError("incorrect passphrase")).not.toMatch(/damaged/i);
  });
});

/** A controllable fake backend + session-dir allocator for the action tests. */
function fakeDeps(overrides: Partial<VaultDeps> = {}): VaultDeps & {
  unlockCalls: Array<{ blob: string; pass: string; session: string }>;
  lockCalls: string[];
} {
  const unlockCalls: Array<{ blob: string; pass: string; session: string }> = [];
  const lockCalls: string[] = [];
  return {
    unlockCalls,
    lockCalls,
    allocSessionDir: async () => SESSION,
    unlock: async (blob, pass, session) => {
      unlockCalls.push({ blob, pass, session });
    },
    lock: async (blob) => {
      lockCalls.push(blob);
    },
    ...overrides,
  };
}

describe("vaultStore actions (CPE-1249)", () => {
  beforeEach(() => resetVaults());

  it("unlockVault records state + session dir and returns it, passing the passphrase to the backend", async () => {
    const deps = fakeDeps();
    const session = await unlockVault(BLOB, "hunter2", deps);
    expect(session).toBe(SESSION);
    expect(deps.unlockCalls).toEqual([{ blob: BLOB, pass: "hunter2", session: SESSION }]);
    expect(get(vaults)).toEqual({ [BLOB]: SESSION });
    expect(isUnlocked(get(vaults), BLOB)).toBe(true);
  });

  it("a failed unlock leaves NO stale store entry and no navigation target", async () => {
    const deps = fakeDeps({
      unlock: async () => {
        throw new Error("incorrect passphrase");
      },
    });
    await expect(unlockVault(BLOB, "wrong", deps)).rejects.toThrow(/passphrase/i);
    expect(get(vaults)).toEqual({}); // nothing recorded
    expect(isUnlocked(get(vaults), BLOB)).toBe(false);
  });

  it("lockVault clears the store entry after the backend wipe succeeds", async () => {
    const deps = fakeDeps();
    await unlockVault(BLOB, "pw", deps);
    expect(isUnlocked(get(vaults), BLOB)).toBe(true);
    await lockVault(BLOB, deps);
    expect(deps.lockCalls).toEqual([BLOB]);
    expect(get(vaults)).toEqual({});
    expect(isUnlocked(get(vaults), BLOB)).toBe(false);
  });

  it("a failed lock (wipe error) keeps the vault unlocked (retryable)", async () => {
    const unlockDeps = fakeDeps();
    await unlockVault(BLOB, "pw", unlockDeps);
    const failing = fakeDeps({
      lock: async () => {
        throw new Error("could not remove");
      },
    });
    await expect(lockVault(BLOB, failing)).rejects.toThrow(/remove/i);
    expect(isUnlocked(get(vaults), BLOB)).toBe(true); // still unlocked → retryable
    // The session dir mapping survives a failed lock, so App.lockActiveVault can navigate BACK into it to
    // re-expose the banner's Lock button for a retry (review #2 — no unreachable retry).
    expect(sessionDirFor(get(vaults), BLOB)).toBe(SESSION);
  });

  // ---- CPE-1654: a tamper refusal is not a busy file -------------------------------------------
  //
  // After CPE-1647 a lock can fail for a second, quite different reason: the session path no longer
  // resolves inside the app's own vault-sessions root (someone swapped a link in). The backend has
  // ALREADY dropped its mapping in that case, wipes nothing, and leaves the vault's own file sealed — so
  // the UI must clear its "unlocked" banner to match, must not offer a retry that can never work, and
  // (App.lockActiveVault) must not navigate into the tampered path. A busy-file failure is the opposite
  // in every respect and must keep working exactly as it did.

  /** The refusal wording the backend produces for a tamper/containment failure — `trustworthy_session`
   *  in `crates/server/src/vault_manager.rs`, surfaced through `VaultError`'s `Display`. Copied verbatim
   *  on purpose, including the "nothing was re-sealed" clause: that phrase overlaps the re-seal failure's
   *  wording, so this string is also the fixture that pins the classifier's ordering. */
  const TAMPER_ERROR =
    "vault format error: refusing to lock: the session directory can no longer be trusted — " +
    "refusing to lock: session directory C:\\cache\\vault-sessions\\abc-123 does not resolve inside " +
    "the app's own vault-sessions directory. Nothing was deleted and nothing was re-sealed; the " +
    "vault's own file is untouched, so the vault is sealed and is now reported locked.";

  it("a tamper refusal clears the vault entry, because the backend has already dropped it", async () => {
    const unlockDeps = fakeDeps();
    await unlockVault(BLOB, "pw", unlockDeps);
    const tampered = fakeDeps({
      lock: async () => {
        throw new Error(TAMPER_ERROR);
      },
    });
    await expect(lockVault(BLOB, tampered)).rejects.toThrow(/no longer be trusted/);
    // The vault IS locked as far as the backend is concerned — leaving it "unlocked" here would show a
    // banner (and a Lock button) for a session the backend has already forgotten.
    expect(isUnlocked(get(vaults), BLOB)).toBe(false);
    expect(sessionDirFor(get(vaults), BLOB)).toBeUndefined();
  });

  it("a re-seal failure keeps the vault unlocked so the user's edits stay reachable", async () => {
    const unlockDeps = fakeDeps();
    await unlockVault(BLOB, "pw", unlockDeps);
    const failing = fakeDeps({
      lock: async () => {
        throw new Error(
          "vault format error: could not re-seal the vault from its unlocked session (vault I/O " +
            "error: disk full) — nothing was deleted, the vault file is unchanged, and your files are " +
            "still in the unlocked folder",
        );
      },
    });
    await expect(lockVault(BLOB, failing)).rejects.toThrow(/re-seal/);
    expect(isUnlocked(get(vaults), BLOB)).toBe(true);
    expect(sessionDirFor(get(vaults), BLOB)).toBe(SESSION);
  });

  it("classifies the two failure shapes onto different messages and different recovery", () => {
    const tamper = classifyLockError(TAMPER_ERROR, "secrets");
    expect(tamper.kind).toBe("tamper");
    expect(tamper.retryable).toBe(false);
    expect(tamper.message).toMatch(/secrets/);
    expect(tamper.message).not.toMatch(/try again/i); // retrying can never help in this state
    expect(tamper.message).not.toMatch(/still be in use/i); // and it is NOT a busy file
    expect(tamper.message).toMatch(/nothing was deleted/i); // say what actually happened

    const busy = classifyLockError("could not remove: The process cannot access the file", "secrets");
    expect(busy.kind).toBe("transient");
    expect(busy.retryable).toBe(true);
    expect(busy.message).toMatch(/still be in use/i);
    expect(busy.message).toMatch(/try again/i);

    const reseal = classifyLockError(
      "vault format error: could not re-seal the vault from its unlocked session (disk full)",
      "secrets",
    );
    expect(reseal.kind).toBe("reseal");
    expect(reseal.retryable).toBe(true); // the working copy is still there — retrying is exactly right
    expect(reseal.message).toMatch(/couldn't be saved back|re-seal/i);
    expect(reseal.message).toMatch(/nothing was deleted/i);
    // A re-seal failure must never be mistaken for the tamper case, whose recovery is the opposite.
    expect(reseal.message).not.toMatch(/no longer be trusted/i);
  });
});
