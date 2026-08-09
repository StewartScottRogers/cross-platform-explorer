// Network sidebar section — pure logic (CPE-1513, epic CPE-1498). The "Network" left-pane section is the
// visible entry point for the SFTP/WebDAV backend the previous two tickets already ship: CPE-1510 (secrets
// live in the OS keychain, keyed by connection name) and CPE-1511 (a remote URI now routes through
// `list_dir` and browses like any folder). This module is the DOM/IO-free half — connection-row state
// mapping, OS-share dedup against saved connections, and the "+ Add a connection" form's validation — kept
// separate from the Svelte layer (Sidebar.svelte, NetworkConnectionForm.svelte, NetworkConnectionMenu.svelte,
// NetworkSecretPrompt.svelte) so it is plain-jsdom unit-testable with no Tauri mocking. See network.test.ts.

import type { AuthMethod, Connection, NetShare } from "./types";

/**
 * A saved connection's live, client-tracked status. There is no backend "is this session pooled/open" query
 * yet (CPE-1499's F1 slice only shipped connect-on-navigate + the keychain, not a session-status command),
 * so this is entirely client state, reset on every app restart:
 * - `"disconnected"` — saved, no successful connect this session (the default for every saved connection).
 * - `"connected"` — the most recent connect attempt (navigate into the connection's location) succeeded.
 * - `"error"` — the most recent connect attempt failed (bad credentials, unreachable host, a refused TOFU
 *   host-key change, …). The row's tooltip carries the actual error text (see {@link stateTitle}).
 */
export type ConnState = "connected" | "disconnected" | "error";

/** Default port per scheme — mirrors `cpe_server::connections::default_port` in Rust exactly, so a profile
 *  on the default port omits it from its location string on both sides. */
const DEFAULT_PORTS: Record<string, number> = { sftp: 22, ssh: 22, smb: 445, webdav: 80, davs: 443 };

/** The `scheme://user@host[:port]/path` a saved connection navigates to — re-derives what Rust's
 *  `Connection::location()` computes, so the sidebar can build the URI to navigate to (or preview it in the
 *  edit form) without a round trip. Pure. Kept in sync with the Rust version's default-port omission. */
export function connectionLocation(conn: Connection): string {
  const defPort = DEFAULT_PORTS[conn.scheme] ?? 0;
  const port = conn.port === defPort ? "" : `:${conn.port}`;
  const path = conn.path && conn.path.trim() ? conn.path : "/";
  return `${conn.scheme}://${conn.user}@${conn.host}${port}${path}`;
}

/** Whether `auth` ALWAYS needs a stored secret before a connect can even be attempted — password auth has
 *  nothing else to authenticate with. A key's passphrase is only SOMETIMES needed (an unencrypted key needs
 *  none), so a key connection is tried directly and only re-prompted reactively on failure; this helper
 *  covers the "always" half only (see Sidebar's connect handler for the reactive key-auth retry). Pure. */
export function secretAlwaysRequired(auth: AuthMethod): boolean {
  return auth.kind === "password";
}

/** Look up a connection's live state, defaulting to `"disconnected"` for a saved connection with no tracked
 *  state yet (fresh load / app restart — see the module doc above for why "connected" never survives a
 *  restart). Pure. */
export function stateOf(states: Record<string, ConnState>, name: string): ConnState {
  return states[name] ?? "disconnected";
}

/** The status-dot tooltip for a connection row — a compact, human summary of {@link ConnState}, including
 *  the tracked error text (if any) so "error" is never a mystery dot. Pure. */
export function stateTitle(state: ConnState, error?: string): string {
  if (state === "connected") return "Connected";
  if (state === "error") return error ? `Connection error: ${error}` : "Connection error";
  return "Saved — not connected";
}

/** Case-insensitive: does an OS-discovered share duplicate a saved connection? Best-effort — the two come
 *  from different worlds (an OS mapped-drive/mount's UNC or mountpoint string vs. a saved sftp/webdav host),
 *  so this treats a share as a duplicate when its name or path contains a saved connection's host. Pure. */
export function isDuplicateShare(share: NetShare, connections: Connection[]): boolean {
  const hay = `${share.name} ${share.path}`.toLowerCase();
  return connections.some((c) => c.host.trim() !== "" && hay.includes(c.host.toLowerCase()));
}

/** OS-discovered shares with any saved-connection duplicate removed (order preserved) — tier 2 of the
 *  sidebar section, deduped against tier 1. Pure. */
export function dedupeShares(shares: NetShare[], connections: Connection[]): NetShare[] {
  return shares.filter((s) => !isDuplicateShare(s, connections));
}

/** Whether the Network section has any connection/share rows to show. The section itself is a PERMANENT
 *  top-level peer of Drives (CPE-1516) — its header always renders — so this no longer gates the section's
 *  visibility; it only decides what the section's body shows: the real rows when true, or the "＋ Add a
 *  connection" control + a one-line empty hint when false, so the plain explorer stays visually quiet (see
 *  CLAUDE.md's mode-additive tiebreaker) even though the header is always present. Pure. */
export function hasAnyNetworkRows(connections: Connection[], dedupedShares: NetShare[]): boolean {
  return connections.length > 0 || dedupedShares.length > 0;
}

// ---- "+ Add a connection" inline form ------------------------------------------------------------------

/** The supported protocols for this slice (CPE-1513 scope: sftp/webdav "to start" per the ticket). */
export const SUPPORTED_SCHEMES = ["sftp", "webdav"] as const;

/** Raw text fields from the add/edit form — everything is a string (even port), matching what an `<input>`
 *  actually hands back, so `buildConnection` owns all the parsing/validation in one pure place. */
export interface ConnectionFormInput {
  name: string;
  scheme: string;
  host: string;
  /** Raw port text; blank ⇒ the scheme's default port. */
  port: string;
  user: string;
  authKind: "password" | "key";
  /** Private-key file path; only used/validated when `authKind === "key"`. */
  keyPath: string;
  /** Optional initial remote path; blank ⇒ the server's home/root. */
  path: string;
}

/** A blank add-form — the "+ Add a connection" control's default state. Pure. */
export function blankConnectionForm(): ConnectionFormInput {
  return { name: "", scheme: "sftp", host: "", port: "", user: "", authKind: "password", keyPath: "", path: "" };
}

/** The add-form pre-filled from an existing connection — "Edit" opens the same inline form via this. Pure. */
export function formFromConnection(conn: Connection): ConnectionFormInput {
  return {
    name: conn.name,
    scheme: conn.scheme,
    host: conn.host,
    port: String(conn.port),
    user: conn.user,
    // The inline form only edits password/key auth; the other AuthMethod variants
    // (anonymous/token/access_key, reserved for future cloud providers) collapse to
    // the password default rather than widening the form's authKind union.
    authKind: conn.auth.kind === "key" ? "key" : "password",
    keyPath: conn.auth.kind === "key" ? conn.auth.key_path : "",
    path: conn.path ?? "",
  };
}

/** Validate + build a `Connection` from the inline add/edit form's raw fields. Returns a user-facing error
 *  string instead of throwing, so the form can show it inline rather than an exception crossing into a
 *  Svelte event handler. Pure — every rule here is independently unit-tested (network.test.ts). */
export function buildConnection(input: ConnectionFormInput): Connection | string {
  const name = input.name.trim();
  const host = input.host.trim();
  const scheme = input.scheme.trim().toLowerCase();
  if (!name) return "Give the connection a name.";
  if (!(SUPPORTED_SCHEMES as readonly string[]).includes(scheme)) {
    return `Unsupported protocol "${input.scheme}" — choose sftp or webdav.`;
  }
  if (!host) return "Host is required.";

  let port = DEFAULT_PORTS[scheme] ?? 0;
  if (input.port.trim()) {
    const parsed = Number(input.port.trim());
    if (!Number.isInteger(parsed) || parsed < 1 || parsed > 65535) return "Port must be 1–65535.";
    port = parsed;
  }

  const auth: AuthMethod =
    input.authKind === "key" ? { kind: "key", key_path: input.keyPath.trim() } : { kind: "password" };
  if (auth.kind === "key" && !auth.key_path) return "Key file path is required for key auth.";

  const path = input.path.trim();
  return { name, scheme, host, port, user: input.user.trim(), auth, path: path ? path : undefined };
}
