// Frontend vault mount store (CPE-1249, epic CPE-738): tracks which `.cpevault` blobs are currently
// unlocked and where their decrypted plaintext lives (the "session dir" passed to `vaultUnlock`). It is
// the reactive source both the tree/listing lock badge (VaultBadge.svelte) and the unlocked-vault banner
// (VaultBanner.svelte) read, plus thin action wrappers around the merged backend vault commands
// (CPE-1248). Idle by default — an empty store costs nothing, so the plain explorer is unaffected until a
// vault is actually unlocked.
//
// The pure reducers + derivations below are DOM/Tauri-free (unit-tested in vaultStore.test.ts); only the
// store tail + the unlock/lock actions touch the backend. The passphrase flows through `unlockVault`'s
// argument straight into the backend command and is NEVER stored here, put in the store, or logged
// (mirrors vault_manager.rs: "never a plaintext file, never a log line").

import { writable, type Readable } from "svelte/store";
import { commands } from "./bindings.gen";
import { unwrap } from "./invoke";
import { appCacheDir, join } from "@tauri-apps/api/path";

/** Unlocked vaults: absolute blob path → the live session directory its plaintext is extracted into. */
export type VaultState = Record<string, string>;

/** The two badge states a `.cpevault` row can present. */
export type VaultBadge = "locked" | "unlocked";

// ---------------------------------------------------------------------------
// Pure reducers + derivations (no DOM / no Tauri) — unit-tested.
// ---------------------------------------------------------------------------

/** Record `blobPath` as unlocked into `sessionDir`. Returns a new state (never mutates). */
export function recordUnlocked(state: VaultState, blobPath: string, sessionDir: string): VaultState {
  return { ...state, [blobPath]: sessionDir };
}

/** Drop `blobPath`'s unlocked mapping. Returns a new state; an unknown path is a no-op copy. */
export function recordLocked(state: VaultState, blobPath: string): VaultState {
  if (!(blobPath in state)) return { ...state };
  const next = { ...state };
  delete next[blobPath];
  return next;
}

/** Is `blobPath` currently unlocked? Pure. */
export function isUnlocked(state: VaultState, blobPath: string): boolean {
  return Object.prototype.hasOwnProperty.call(state, blobPath);
}

/** The live session directory for an unlocked `blobPath`, or `undefined`. Pure. */
export function sessionDirFor(state: VaultState, blobPath: string): string | undefined {
  return state[blobPath];
}

/** The badge a `.cpevault` row should show, derived purely from the store. */
export function badgeFor(state: VaultState, blobPath: string): VaultBadge {
  return isUnlocked(state, blobPath) ? "unlocked" : "locked";
}

/** Does `path` live inside `dir` (or equal it)? Separator-tolerant (`/` and `\`) so it works whether the
 *  navigated path uses POSIX or Windows separators. Pure. */
function pathInside(path: string, dir: string): boolean {
  return path === dir || path.startsWith(dir + "/") || path.startsWith(dir + "\\");
}

/** The blob path of the unlocked vault whose session dir contains `path` (i.e. we're browsing inside that
 *  vault right now), or `null`. Drives the unlocked-vault banner. Pure. */
export function vaultOfSessionPath(state: VaultState, path: string): string | null {
  for (const [blob, dir] of Object.entries(state)) {
    if (pathInside(path, dir)) return blob;
  }
  return null;
}

/** A friendly display name for a vault blob path: its base name minus the `.cpevault` extension. Pure. */
export function vaultDisplayName(blobPath: string): string {
  const base = blobPath.split(/[\\/]/).pop() ?? blobPath;
  return base.replace(/\.cpevault$/i, "");
}

/** Map a raw backend unlock error to distinct, user-facing copy (CPE-1249 AC: BadPassphrase vs Corrupt).
 *  The backend surfaces `VaultError::Display` strings (vault_crypto.rs), so classify by their wording. Pure —
 *  never echoes a passphrase (the error strings carry none). */
export function classifyUnlockError(raw: unknown): string {
  const msg = String(raw ?? "").toLowerCase();
  if (msg.includes("passphrase") || msg.includes("password")) return "Wrong password — try again.";
  if (msg.includes("corrupt") || msg.includes("tamper")) return "This vault file is damaged and can't be opened.";
  if (msg.includes("magic") || msg.includes("not a cpe vault")) return "This file isn't a valid vault.";
  if (msg.includes("version")) return "This vault was made by a newer version and can't be opened.";
  return `Couldn't unlock the vault: ${String(raw)}`;
}

/** How a failed `vaultLock` should be reported and recovered from (CPE-1654). */
export interface LockFailure {
  /** `tamper` — the backend refused because the session path is no longer the directory it extracted
   *  into (a link was swapped in). It wiped nothing, DROPPED its mapping, and left the vault's own file
   *  sealed. `reseal` — the edits couldn't be written back into the vault. `transient` — the wipe failed
   *  (a file still in use, a read-only file). The last two leave the vault unlocked. */
  kind: "tamper" | "reseal" | "transient";
  /** Whether clicking Lock again could plausibly work. `false` for `tamper`: every retry re-resolves the
   *  same tampered path and fails the same way, so offering a retry would be a lie. Also gates whether
   *  the caller navigates BACK into the session dir — for a tamper refusal it must not, since that path
   *  now points at somewhere else entirely (the user's own Documents, in the demonstrated exploit). */
  retryable: boolean;
  /** User-facing copy: what happened, what it means for their files, and what to do. */
  message: string;
}

/** Classify a failed lock (CPE-1654) into distinct, honest copy + the right recovery.
 *
 *  Before this, EVERY lock failure was reported as "some files may still be in use. Try again." — which
 *  after CPE-1647 is wrong for a containment/tamper refusal: retrying can never help there, the vault is
 *  in fact sealed, and the backend has already dropped its mapping. Classified by wording, like
 *  {@link classifyUnlockError}; the phrases below are produced by `vault_manager.rs` — `UNTRUSTED_SESSION`
 *  (whose doc comment points back here) and `reseal_failed` — and only by those. Pure. */
export function classifyLockError(raw: unknown, name: string): LockFailure {
  const msg = String(raw ?? "").toLowerCase();
  // ORDER MATTERS: the tamper refusal's own copy says "nothing was re-sealed", so it must be recognised
  // BEFORE the re-seal case or it would be mis-sorted into the retryable branch — the exact confusion
  // this function exists to prevent. Its marker phrase is the narrower of the two, and no re-seal failure
  // ever carries it (`UNTRUSTED_SESSION` in vault_manager.rs is produced by one function only).
  if (msg.includes("no longer be trusted")) {
    return {
      kind: "tamper",
      retryable: false,
      message:
        `Couldn't lock "${name}" — its private unlocked folder was moved or replaced, so the app ` +
        `stopped trusting it. Nothing was deleted and the vault file itself is unchanged, so the vault ` +
        `is sealed and is now shown as locked.`,
    };
  }
  if (msg.includes("could not re-seal")) {
    return {
      kind: "reseal",
      retryable: true,
      message:
        `Couldn't lock "${name}" — your changes couldn't be saved back into the vault. Nothing was ` +
        `deleted: the vault file is unchanged and your files are still in the unlocked folder, so you ` +
        `can try again or copy them out first.`,
    };
  }
  return {
    kind: "transient",
    retryable: true,
    message: `Couldn't lock "${name}" — some files may still be in use. Try again.`,
  };
}

// ---------------------------------------------------------------------------
// Reactive store + backend actions.
// ---------------------------------------------------------------------------

const store = writable<VaultState>({});

/** Reactive unlocked-vault map: blob path → session dir (empty until a vault is unlocked). */
export const vaults: Readable<VaultState> = store;

/** Test seam (CPE-1249): how a session dir is allocated and how the backend is reached, injectable so the
 *  unlock/lock actions are unit-testable without Tauri. Production uses {@link defaultDeps}. */
export interface VaultDeps {
  /** Allocate a fresh, unique, app-private session directory path for a new unlock. */
  allocSessionDir: () => Promise<string>;
  /** Decrypt `blobPath` with `passphrase` into `sessionDir` (throws on failure — wrong pass / corrupt). */
  unlock: (blobPath: string, passphrase: string, sessionDir: string) => Promise<void>;
  /** Lock `blobPath`: the backend securely wipes its session dir (throws if the wipe fails). */
  lock: (blobPath: string) => Promise<void>;
}

/** A unique, unpredictable, app-private session dir: a `crypto.randomUUID()` subdir under the OS app-cache
 *  dir. While the vault is unlocked its plaintext lives HERE on disk — the browsable-mount tradeoff the epic
 *  accepts for v1 (see vault_manager.rs "The mount tradeoff"); `vaultLock` shreds + removes this dir. */
async function defaultAllocSessionDir(): Promise<string> {
  const base = await appCacheDir();
  return join(base, "vault-sessions", crypto.randomUUID());
}

const defaultDeps: VaultDeps = {
  allocSessionDir: defaultAllocSessionDir,
  unlock: async (blobPath, passphrase, sessionDir) => {
    unwrap(await commands.vaultUnlock(blobPath, passphrase, sessionDir));
  },
  lock: async (blobPath) => {
    unwrap(await commands.vaultLock(blobPath));
  },
};

/**
 * Unlock a vault and record it. Allocates a session dir, calls the backend to decrypt into it, and — ONLY
 * on success — records the unlocked mapping. A failed unlock (wrong passphrase / corrupt blob) throws and
 * leaves NO store entry (CPE-1249 AC). Returns the session dir so the caller can navigate into it. The
 * passphrase is never retained here.
 */
export async function unlockVault(
  blobPath: string,
  passphrase: string,
  deps: VaultDeps = defaultDeps,
): Promise<string> {
  const sessionDir = await deps.allocSessionDir();
  await deps.unlock(blobPath, passphrase, sessionDir); // throws on bad pass / corrupt → nothing recorded
  store.update((s) => recordUnlocked(s, blobPath, sessionDir));
  return sessionDir;
}

/**
 * Lock a vault. The backend re-seals the session dir back into the blob (CPE-1645) and then securely
 * wipes it; the mapping is dropped from the store only after that succeeds, so a failed re-seal or wipe
 * leaves the vault reported unlocked (retryable) — matching the backend's own semantics. The caller MUST
 * navigate OUT of the session dir first (it is about to be wiped).
 *
 * The one failure that still clears the mapping is a **tamper refusal** (CPE-1654): there the backend has
 * already dropped its own mapping and the vault's file is sealed and untouched, so keeping ours would
 * leave a banner and a Lock button for a session that no longer exists on either side. The error is still
 * re-thrown so the caller can report it — see {@link classifyLockError}.
 */
export async function lockVault(blobPath: string, deps: VaultDeps = defaultDeps): Promise<void> {
  try {
    // The backend re-seals + wipes the session dir; it throws (and keeps state) if either fails.
    await deps.lock(blobPath);
  } catch (e) {
    if (classifyLockError(e, blobPath).kind === "tamper") store.update((s) => recordLocked(s, blobPath));
    throw e;
  }
  store.update((s) => recordLocked(s, blobPath));
}

/** Reset the store to empty. Test-only helper for isolation between unit tests. */
export function resetVaults(): void {
  store.set({});
}
