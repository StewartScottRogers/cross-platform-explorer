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
 * Lock a vault. The backend securely wipes the session dir; the mapping is dropped from the store only
 * after that succeeds, so a failed wipe leaves the vault reported unlocked (retryable) — matching the
 * backend's own semantics. The caller MUST navigate OUT of the session dir first (it is about to be
 * wiped).
 */
export async function lockVault(blobPath: string, deps: VaultDeps = defaultDeps): Promise<void> {
  await deps.lock(blobPath); // backend wipes the session dir; throws (and keeps state) if the wipe fails
  store.update((s) => recordLocked(s, blobPath));
}

/** Reset the store to empty. Test-only helper for isolation between unit tests. */
export function resetVaults(): void {
  store.set({});
}
