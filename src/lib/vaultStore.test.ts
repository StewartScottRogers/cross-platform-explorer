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
  VaultLockError,
  type LockFailureCode,
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

  // ---- CPE-1654 + SEC-847 finding 3: a lock failure is classified by CODE, never by text -------
  //
  // A lock can fail four quite different ways, and the recovery is opposite between them. The first
  // version of this classifier matched substrings of the backend's message — and the security audit of
  // PR #847 showed that could not stand: the wipe and re-seal errors interpolate FULL FILE PATHS, so a
  // file *inside the vault* could choose its own name to impersonate a tamper refusal, and the UI would
  // then clear the banner and report the vault sealed while its whole decrypted tree was still on disk.
  // Everything below therefore switches on the backend's structured `code`.

  /** A structured backend failure, exactly as `defaultDeps.lock` re-throws it. */
  const lockError = (code: string, message = "backend detail") =>
    new VaultLockError(code as LockFailureCode, message);

  /** The EXACT string `shred_tree` produces for a file it cannot shred (`vault_manager.rs`), surfaced
   *  through `VaultError`'s `Display`. `{p}` is the full path of a file inside the unlocked session dir
   *  — a name the user (or anything that can write into the unlocked vault) chooses. This is the audit's
   *  attack string: an ordinary busy-file wipe failure wearing the tamper wording. */
  const WIPE_FAILURE_WITH_HOSTILE_FILENAME =
    "shred C:\\cache\\app\\vault-sessions\\abc-123\\why my landlord can no longer be trusted.txt: " +
    "The process cannot access the file because it is being used by another process. (os error 32)";

  it("a tamper refusal clears the vault entry, because the backend has already dropped it", async () => {
    const unlockDeps = fakeDeps();
    await unlockVault(BLOB, "pw", unlockDeps);
    const tampered = fakeDeps({
      lock: async () => {
        throw lockError("untrusted_session");
      },
    });
    await expect(lockVault(BLOB, tampered)).rejects.toThrow();
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
        throw lockError("reseal_failed", "could not re-seal … (disk full)");
      },
    });
    await expect(lockVault(BLOB, failing)).rejects.toThrow(/re-seal/);
    expect(isUnlocked(get(vaults), BLOB)).toBe(true);
    expect(sessionDirFor(get(vaults), BLOB)).toBe(SESSION);
  });

  // SEC-847 finding 3, the regression the audit demonstrated: a WIPE failure whose message carries the
  // tamper wording, because a file inside the vault is named that way. Misclassifying it strands the
  // decrypted tree on disk with no banner and tells the user the vault is sealed — every clause false.
  it("a wipe failure is still a wipe failure however the files inside the vault are named", async () => {
    const f = classifyLockError(lockError("wipe_failed", WIPE_FAILURE_WITH_HOSTILE_FILENAME));
    expect(f.kind).toBe("transient");
    expect(f.retryable).toBe(true);
    expect(f.messageKey).toBe("notice.vaultLockFailed");
    // The fixture must really carry the impersonating text, or this proves nothing.
    expect(f.reason).toMatch(/no longer be trusted/);
  });

  it("keeps the vault unlocked when a hostile-looking wipe failure comes back", async () => {
    await unlockVault(BLOB, "pw", fakeDeps());
    const failing = fakeDeps({
      lock: async () => {
        throw lockError("wipe_failed", WIPE_FAILURE_WITH_HOSTILE_FILENAME);
      },
    });
    await expect(lockVault(BLOB, failing)).rejects.toThrow(/shred/);
    // The backend kept its mapping (the wipe failed before it forgot the session) and the session dir is
    // still full of decrypted plaintext. The store MUST agree, or the banner + Lock button vanish.
    expect(isUnlocked(get(vaults), BLOB)).toBe(true);
    expect(sessionDirFor(get(vaults), BLOB)).toBe(SESSION);
  });

  it("classifies every failure shape onto its own message key and recovery", () => {
    const tamper = classifyLockError(lockError("untrusted_session"));
    expect(tamper.kind).toBe("tamper");
    expect(tamper.retryable).toBe(false); // → App must NOT navigate back into the tampered path
    expect(tamper.messageKey).toBe("notice.vaultLockTampered");

    const busy = classifyLockError(lockError("wipe_failed"));
    expect(busy.kind).toBe("transient");
    expect(busy.retryable).toBe(true);
    expect(busy.messageKey).toBe("notice.vaultLockFailed"); // the UAT-confirmed retry copy

    const reseal = classifyLockError(lockError("reseal_failed"));
    expect(reseal.kind).toBe("reseal");
    expect(reseal.retryable).toBe(true); // the working copy is still there — retrying is exactly right
    expect(reseal.messageKey).toBe("notice.vaultLockResealFailed");

    const inFlight = classifyLockError(lockError("already_locking"));
    expect(inFlight.kind).toBe("busy");
    expect(inFlight.retryable).toBe(true);
    expect(inFlight.messageKey).toBe("notice.vaultLockInProgress");

    // Every kind must be distinguishable — four codes, four keys.
    const keys = [tamper, busy, reseal, inFlight].map((f) => f.messageKey);
    expect(new Set(keys).size).toBe(4);
  });

  it("falls back to the SAFEST reading when there is no usable code", () => {
    // A transport error, an older backend, a thrown string: we do not know whether anything was
    // destroyed, so we must not claim the vault is sealed and must not clear the banner.
    for (const raw of [new Error("ipc transport died"), "some string", null, { code: "made_up" }]) {
      const f = classifyLockError(raw);
      expect(f.kind).toBe("transient");
      expect(f.retryable).toBe(true);
      expect(f.messageKey).toBe("notice.vaultLockFailed");
    }
  });

  it("does not clear the store for any failure except a tamper refusal", async () => {
    for (const code of ["reseal_failed", "wipe_failed", "already_locking"]) {
      resetVaults();
      await unlockVault(BLOB, "pw", fakeDeps());
      const failing = fakeDeps({
        lock: async () => {
          throw lockError(code);
        },
      });
      await expect(lockVault(BLOB, failing)).rejects.toThrow();
      expect(isUnlocked(get(vaults), BLOB), `${code} must leave the vault unlocked`).toBe(true);
    }
  });
});
