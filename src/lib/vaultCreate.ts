// Pure, DOM/Tauri-free helpers for the vault CREATE dialog (CPE-1250, epic CPE-738). The dialog
// (VaultCreateDialog.svelte) owns the backend call + the passphrase secret; these functions are just the
// validation + default-destination logic, so they can be unit-tested without a webview (vaultCreate.test.ts).
//
// Nothing here ever touches, stores, or logs the passphrase VALUE beyond comparing the two typed fields
// for equality (mirroring vaultStore.ts's "never a plaintext file, never a log line" discipline).

/** The extension every sealed `.cpevault` blob carries. */
export const VAULT_EXT = ".cpevault";

/** Split a path into `{ parent, base, sep }`, preserving the separator style used in `path` (POSIX `/`
 *  or Windows `\`) so a computed default destination matches the folder's own convention. */
function splitPath(path: string): { parent: string; base: string; sep: string } {
  const idx = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  if (idx < 0) return { parent: "", base: path, sep: "/" };
  return { parent: path.slice(0, idx), base: path.slice(idx + 1), sep: path[idx] };
}

/**
 * The default vault destination for `folderPath`: `<foldername>.cpevault` placed as a SIBLING of the
 * folder (in the folder's PARENT directory), never inside it. The backend refuses a destination inside
 * the folder when shredding (`vault_manager::create_vault`'s data-loss guard), so a sibling default keeps
 * the destructive path valid by construction. Separator-preserving; tolerates a trailing separator.
 */
export function siblingVaultDest(folderPath: string): string {
  const trimmed = folderPath.replace(/[\\/]+$/, ""); // drop any trailing separator first
  const { parent, base, sep } = splitPath(trimmed);
  const name = `${base}${VAULT_EXT}`;
  return parent ? `${parent}${sep}${name}` : name;
}

/** Result of validating the passphrase + its confirmation. */
export interface PassphraseCheck {
  /** Both fields are non-empty and identical. */
  ok: boolean;
  /** A user-facing message when `ok` is false, else "". */
  error: string;
}

/**
 * Validate the passphrase against its confirmation. Two blocking cases, in priority order: an empty
 * passphrase ("enter a passphrase") before a mismatch ("they don't match"), so the message tracks what
 * the user should fix next. A forgotten passphrase is unrecoverable by design — the dialog warns of that
 * separately; this only guards against an accidental typo via the confirm field.
 */
export function checkPassphrases(passphrase: string, confirm: string): PassphraseCheck {
  if (passphrase.length === 0) return { ok: false, error: "Enter a passphrase." };
  if (passphrase !== confirm) return { ok: false, error: "The passphrases don't match." };
  return { ok: true, error: "" };
}

/** Whether the "Create vault" button should be enabled: valid matching passphrases, a non-empty
 *  destination, and no create already in flight. Pure — drives the dialog's disabled state + is unit-tested. */
export function canCreate(state: {
  passphrase: string;
  confirm: string;
  dest: string;
  busy: boolean;
}): boolean {
  return (
    !state.busy && state.dest.trim().length > 0 && checkPassphrases(state.passphrase, state.confirm).ok
  );
}

/** Whether the honest destructive warning should be shown: exactly when "securely delete the original"
 *  is checked. Trivial, but named + tested so the shred-checkbox → warning gate is a pinned contract. */
export function shouldShowShredWarning(shredOriginal: boolean): boolean {
  return shredOriginal;
}

/**
 * Whether "Create vault" may actually be submitted (CPE-1646). Layers a SECOND, independent gate on
 * top of {@link canCreate}: when "securely delete the original" is intended (`shredOriginal`), the
 * dialog also requires a genuinely separate acknowledgement (`shredConfirmed`) — never derived from
 * `shredOriginal` itself. This is the fix for the bug CPE-1646 raised: the dialog used to pass the same
 * `shredOriginal` value as both the intent AND the consent argument to `vault_create`, re-creating the
 * "one flag doing double duty" collapse CPE-1599 exists to prevent. With this gate, ticking the shred
 * checkbox alone can never satisfy the create-enabled condition — only the SEPARATE acknowledgement
 * control (rendered inside the warning panel) can — so intent and consent can't drift into each other
 * through a future refactor, restored draft, or deep-link default.
 */
export function canSubmitCreate(state: {
  passphrase: string;
  confirm: string;
  dest: string;
  busy: boolean;
  shredOriginal: boolean;
  shredConfirmed: boolean;
}): boolean {
  return canCreate(state) && (!state.shredOriginal || state.shredConfirmed);
}
