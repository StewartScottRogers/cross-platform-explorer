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
 *  on the default port omits it from its location string on both sides.
 *
 *  `s3: 443` (CPE-1686): S3 and every S3-compatible endpoint speaks HTTPS, so 443 is the default a blank
 *  Port field means. Rust's `default_port` still falls through to `0` for `"s3"` until **CPE-1685** adds the
 *  arm — that ticket must set it to **443** to keep this hand-mirror honest. `network.test.ts` asserts the
 *  literal on this side, so a change on either side has a test to break. */
export const DEFAULT_PORTS: Record<string, number> = {
  sftp: 22,
  ssh: 22,
  smb: 445,
  webdav: 80,
  davs: 443,
  ftp: 21,
  s3: 443,
};

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
 *  nothing else to authenticate with, and neither does access-key auth: SigV4 signs every request with the
 *  secret access key, and signing with a blank one produces a valid-looking request that comes back
 *  `SignatureDoesNotMatch` (CPE-1686; the backend half of that guard is CPE-1685), which sends the user
 *  hunting for a clock-skew or policy problem that doesn't exist. Prompting first is the honest failure.
 *  A key's passphrase is only SOMETIMES needed (an unencrypted key needs none), so a key connection is
 *  tried directly and only re-prompted reactively on failure; this helper covers the "always" half only
 *  (see Sidebar's connect handler for the reactive key-auth retry). Pure. */
export function secretAlwaysRequired(auth: AuthMethod): boolean {
  return auth.kind === "password" || auth.kind === "access_key";
}

/** What the inline secret prompt (NetworkSecretPrompt) should call the secret it's asking for — the OS
 *  keychain stores one secret per connection (CPE-1510), so the label is purely about naming it in words
 *  the user recognises from their provider's console. Pure. */
export function secretLabel(auth: AuthMethod): string {
  if (auth.kind === "key") return "Passphrase";
  if (auth.kind === "access_key") return "Secret access key";
  return "Password";
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

/** Trimmed, trailing-slash-stripped, lowercased — a share `path`'s dedup key. Mirrors
 *  `cpe_server::net_share::dedup_key` on the Rust side (case/slash-insensitive: share paths are
 *  case-insensitive on Windows, and treating them so cross-platform is harmless here). Pure. */
function shareDedupKey(path: string): string {
  return path.trim().replace(/[/\\]+$/, "").toLowerCase();
}

/** Case-insensitive: does a share duplicate a saved connection or an already-listed share? Best-effort —
 *  these come from different worlds (an OS mapped-drive/mount's UNC or mountpoint string, a WNet-discovered
 *  UNC, vs. a saved sftp/webdav/smb host), so two checks apply:
 *  - it's a duplicate of a saved connection when its name or path contains that connection's host;
 *  - (CPE-1519) it's a duplicate of an already-listed share (`existingShares` — tier 2's OS `net use`/mount
 *    rows, when dedupe-ing tier 3's WNet-discovered rows against them) when that share's own name/path
 *    contains this share's normalized path — e.g. a discovered `\\qnap\media` duplicates a mapped row whose
 *    name embeds the same UNC (`\\qnap\media (Z:)`).
 *  `existingShares` defaults to empty so the original tier-2-vs-tier-1 call site is unaffected. Pure. */
export function isDuplicateShare(share: NetShare, connections: Connection[], existingShares: NetShare[] = []): boolean {
  const hay = `${share.name} ${share.path}`.toLowerCase();
  if (connections.some((c) => c.host.trim() !== "" && hay.includes(c.host.toLowerCase()))) return true;
  const key = shareDedupKey(share.path);
  if (!key) return false;
  return existingShares.some((s) => `${s.name} ${s.path}`.toLowerCase().includes(key));
}

/** Shares with any saved-connection (and, when passed, already-listed-share) duplicate removed, order
 *  preserved. Tier 2 (OS `net use`/mount shares) dedupes against tier 1 (`dedupeShares(shares,
 *  connections)`, `existingShares` omitted); tier 3 (CPE-1519's WNet-discovered shares) dedupes against
 *  BOTH tier 1 and tier 2 (`dedupeShares(discovered, connections, dedupedTier2Shares)`). Pure. */
export function dedupeShares(shares: NetShare[], connections: Connection[], existingShares: NetShare[] = []): NetShare[] {
  return shares.filter((s) => !isDuplicateShare(s, connections, existingShares));
}

/** Whether the Network section has any connection/share rows to show. The section itself is a PERMANENT
 *  top-level peer of Drives (CPE-1516) — its header always renders — so this no longer gates the section's
 *  visibility; it only decides what the section's body shows: the real rows when true, or the "＋ Add a
 *  connection" control + a one-line empty hint when false, so the plain explorer stays visually quiet (see
 *  CLAUDE.md's mode-additive tiebreaker) even though the header is always present. `discoveredShares`
 *  (CPE-1519's tier 3) is optional so existing 2-arg call sites are unaffected. Pure. */
export function hasAnyNetworkRows(
  connections: Connection[],
  dedupedShares: NetShare[],
  discoveredShares: NetShare[] = [],
): boolean {
  return connections.length > 0 || dedupedShares.length > 0 || discoveredShares.length > 0;
}

// ---- "+ Add a connection" inline form ------------------------------------------------------------------

/** The supported protocols for this slice (CPE-1513 scope: sftp/webdav "to start" per the ticket), plus
 *  `smb` (CPE-1519): a discovered `\\server\share` pre-fills scheme `smb`, and the form must accept it
 *  rather than reject the very row it just offered to add. Saving an `smb` connection profile is honest
 *  about today's limits — there's no generic-remote SMB client yet (`Scheme::Smb` routes to a "not
 *  connected" message in `fs_route.rs`), matching the ticket's "don't build new SMB browsing" scope.
 *  `s3` joins as of CPE-1686 (epic CPE-1503): S3 and every S3-compatible store (MinIO, Backblaze B2,
 *  Wasabi, Google Cloud Storage's S3 API) are savable here, with access-key auth and the endpoint/region/
 *  bucket convention documented on {@link schemeFieldHints}. */
export const SUPPORTED_SCHEMES = ["sftp", "webdav", "smb", "ftp", "s3"] as const;

/** The supported protocols as a human list for an error message — `"sftp, webdav, smb, ftp, or s3"`.
 *  Derived from {@link SUPPORTED_SCHEMES} rather than hand-written (CPE-1686): the rejection string used to
 *  spell the four schemes out literally, so adding a fifth left the message telling the user to choose from
 *  a list that no longer matched what the dropdown offered. Now the seventh scheme needs no edit here.
 *  Pure. */
export function supportedSchemesSentence(): string {
  const schemes = [...SUPPORTED_SCHEMES];
  if (schemes.length === 1) return schemes[0];
  return `${schemes.slice(0, -1).join(", ")}, or ${schemes[schemes.length - 1]}`;
}

/** Whether `scheme` is one of `SUPPORTED_SCHEMES` — the single check gating whether a "＋ Add a
 *  connection" affordance can actually save. Case/whitespace-tolerant, matching how `buildConnection`
 *  normalizes its own `scheme` field. Pure — shared by `buildConnection`'s validation and the
 *  "Discovered on your network" tier's add-affordance gate (CPE-1524: an mDNS `nfs://` row parses fine
 *  but has no savable scheme yet), so when a provider lands (e.g. NFS via CPE-1505) and joins
 *  `SUPPORTED_SCHEMES`, every gate built on this helper opens automatically — no per-scheme
 *  special-casing. */
export function isSavableScheme(scheme: string): boolean {
  return (SUPPORTED_SCHEMES as readonly string[]).includes(scheme.trim().toLowerCase());
}

/** How the form lets a connection authenticate. `access_key` (CPE-1686) is S3's access key id + secret
 *  access key (SigV4); the other two are the SSH/HTTP-era pair the form has always offered. The remaining
 *  `AuthMethod` variants (`anonymous`, `token`) have no provider that consumes them yet, so the form still
 *  doesn't offer them. */
export type FormAuthKind = "password" | "key" | "access_key";

/** Raw text fields from the add/edit form — everything is a string (even port), matching what an `<input>`
 *  actually hands back, so `buildConnection` owns all the parsing/validation in one pure place.
 *
 *  **No field here ever holds secret material** — not a password, not a key passphrase, not an S3 secret
 *  access key. Every one of those is collected by NetworkSecretPrompt at connect time and lives in the OS
 *  keychain (CPE-1510), keyed by the connection's `name`; this form only ever carries the *non-secret* half
 *  (a key's path, an access key **id**). `network.test.ts` guards that invariant against a future field. */
export interface ConnectionFormInput {
  name: string;
  scheme: string;
  /** Host — for `s3`, the **endpoint** host (`s3.us-east-1.amazonaws.com`, `minio.lan`). See
   *  {@link schemeFieldHints} for why the S3 fields reuse the generic ones. */
  host: string;
  /** Raw port text; blank ⇒ the scheme's default port. */
  port: string;
  /** User — for `s3`, the **region** (blank ⇒ `us-east-1`). */
  user: string;
  authKind: FormAuthKind;
  /** Private-key file path; only used/validated when `authKind === "key"`. */
  keyPath: string;
  /** S3 access key **id** — not secret (it's the public half of the credential pair, like a username), so
   *  it's stored in the profile. Only used/validated when `authKind === "access_key"`. */
  accessKeyId: string;
  /** Optional initial remote path; blank ⇒ the server's home/root. For `s3` it's `/bucket[/prefix]` and is
   *  required — an object store has no "root" to land on. */
  path: string;
}

/** A blank add-form — the "+ Add a connection" control's default state. Pure. */
export function blankConnectionForm(): ConnectionFormInput {
  return {
    name: "",
    scheme: "sftp",
    host: "",
    port: "",
    user: "",
    authKind: "password",
    keyPath: "",
    accessKeyId: "",
    path: "",
  };
}

/** The auth kinds a scheme can actually use — S3 signs with an access key and nothing else, while the three
 *  shipped SSH/HTTP providers reject `AccessKey` outright. Offering the wrong one would let a user save a
 *  profile that can only ever fail at connect time, so the form's radios come from here. Pure (CPE-1686). */
export function authKindsFor(scheme: string): readonly FormAuthKind[] {
  return scheme.trim().toLowerCase() === "s3" ? (["access_key"] as const) : (["password", "key"] as const);
}

/** Keep a form's `authKind` legal when the protocol dropdown changes — switching to `s3` snaps a
 *  password/key form onto access-key auth, and switching away snaps back to password, so the radios never
 *  show a selection the scheme can't use. Pure (CPE-1686). */
export function coerceAuthKind(scheme: string, kind: FormAuthKind): FormAuthKind {
  const allowed = authKindsFor(scheme);
  return allowed.includes(kind) ? kind : allowed[0];
}

/** Per-scheme labels/placeholders for the three fields whose *meaning* changes with the protocol.
 *
 *  **The S3 field convention (CPE-1686 — decided here, handed to CPE-1685).** A `Connection` has exactly
 *  `{name, scheme, host, port, user, auth, path}` and gains no S3-only fields, so S3's endpoint/region/bucket
 *  map onto the existing ones:
 *  - `host` → the **endpoint** host (`s3.us-east-1.amazonaws.com`, `minio.lan`, `s3.us-west-004.backblazeb2.com`).
 *    An explicit endpoint is what makes the epic's "B2/GCS/Wasabi/MinIO come free" claim true — those can't
 *    be derived from a region — and it keeps `host` meaning "the server you connect to", as it does for
 *    every other scheme.
 *  - `port` → the endpoint port (blank ⇒ 443; MinIO's 9000 is typed in).
 *  - `user` → the **region** (blank ⇒ `us-east-1`, the universal default S3-compatible servers accept).
 *    S3 has no user — the credential is the access key id in `auth`.
 *  - `path` → `/bucket[/prefix]`, exactly the way a discovered SMB row puts its share in the path
 *    (`/media`). The bucket is the first segment; anything after it is the prefix to land on.
 *  So a saved profile's location string reads `s3://us-east-1@minio.lan:9000/my-bucket/prefix` and parses
 *  straight back through `location.rs`'s existing `{scheme,user,host,port,path}` split. Pure. */
export function schemeFieldHints(scheme: string): {
  hostLabel: string;
  hostPlaceholder: string;
  userLabel: string;
  userPlaceholder: string;
  pathLabel: string;
  pathPlaceholder: string;
} {
  if (scheme.trim().toLowerCase() === "s3") {
    return {
      hostLabel: "Endpoint",
      hostPlaceholder: "s3.us-east-1.amazonaws.com",
      userLabel: "Region",
      userPlaceholder: "us-east-1",
      pathLabel: "Bucket and prefix",
      pathPlaceholder: "/my-bucket/prefix",
    };
  }
  return {
    hostLabel: "Host",
    hostPlaceholder: "host.example.com",
    userLabel: "User",
    userPlaceholder: "(optional)",
    pathLabel: "Remote path",
    pathPlaceholder: "/ (server root)",
  };
}

/** The region a blank Region field means — every S3-compatible server accepts `us-east-1`, and SigV4 needs
 *  *some* region to sign with, so the saved profile records it explicitly rather than leaving the backend to
 *  guess. Pure (CPE-1686). */
export const DEFAULT_S3_REGION = "us-east-1";

/** The add-form pre-filled from an existing connection — "Edit" opens the same inline form via this. Pure. */
export function formFromConnection(conn: Connection): ConnectionFormInput {
  return {
    name: conn.name,
    scheme: conn.scheme,
    host: conn.host,
    port: String(conn.port),
    user: conn.user,
    // The inline form edits password/key/access_key auth; the remaining AuthMethod variants
    // (anonymous/token — no provider consumes them yet) collapse to the password default
    // rather than widening the form's authKind union.
    authKind: conn.auth.kind === "key" ? "key" : conn.auth.kind === "access_key" ? "access_key" : "password",
    keyPath: conn.auth.kind === "key" ? conn.auth.key_path : "",
    accessKeyId: conn.auth.kind === "access_key" ? conn.auth.id : "",
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
  const hints = schemeFieldHints(scheme);
  const isS3 = scheme === "s3";
  if (!name) return "Give the connection a name.";
  if (!isSavableScheme(scheme)) {
    return `Unsupported protocol "${input.scheme}" — choose ${supportedSchemesSentence()}.`;
  }
  if (!host) return `${hints.hostLabel} is required.`;

  let port = DEFAULT_PORTS[scheme] ?? 0;
  if (input.port.trim()) {
    const parsed = Number(input.port.trim());
    if (!Number.isInteger(parsed) || parsed < 1 || parsed > 65535) return "Port must be 1–65535.";
    port = parsed;
  }

  // An auth kind the scheme can't use would save a profile that can only ever fail at connect time (the
  // three SSH/HTTP providers reject AccessKey outright; S3 signs with nothing else), so refuse it here
  // rather than at connect. The form's radios come from the same `authKindsFor`, so this is a backstop.
  if (!authKindsFor(scheme).includes(input.authKind)) {
    return isS3
      ? "S3 authenticates with an access key — choose Access key."
      : `Access-key auth is only used by s3, not ${scheme}.`;
  }

  let auth: AuthMethod;
  if (input.authKind === "key") {
    const keyPath = input.keyPath.trim();
    if (!keyPath) return "Key file path is required for key auth.";
    auth = { kind: "key", key_path: keyPath };
  } else if (input.authKind === "access_key") {
    const id = input.accessKeyId.trim();
    if (!id) return "Access key ID is required for access-key auth.";
    // `secret_ref` names the OS-keychain entry holding the secret access key — it is a *label*, never the
    // secret. Connections are keyed in the keychain by their own `name` (CPE-1510's
    // `connection_secret_get/set`, Rust's `secret_for(access, &conn.name)`), so that's the ref this form
    // writes. CPE-1685 is the first backend consumer; this is the convention it should read.
    auth = { kind: "access_key", id, secret_ref: name };
  } else {
    auth = { kind: "password" };
  }

  let path = input.path.trim();
  let user = input.user.trim();
  if (isS3) {
    // An object store has no home directory to land on — every request is scoped to a bucket, so a profile
    // without one can't list anything. Normalise `my-bucket/prefix` to `/my-bucket/prefix` so the location
    // string is well-formed however the user typed it.
    if (path && !path.startsWith("/")) path = `/${path}`;
    const bucket = path.split("/").filter((seg) => seg.length > 0)[0] ?? "";
    if (!bucket) return "Bucket is required for s3 — use /bucket or /bucket/prefix.";
    if (!user) user = DEFAULT_S3_REGION;
  }
  return { name, scheme, host, port, user, auth, path: path ? path : undefined };
}

// ---- "Discovered on your network" tier (CPE-1519) ------------------------------------------------------

/** Split a `\\server\share[\sub...]` UNC token into its server and (first) share segment. Forward-slash
 *  tolerant (WNet/Explorer both accept either). `null` when it isn't UNC-shaped at all — defensive; the
 *  backend's `map_discovered_share` already only ever emits a `\\`-prefixed `path`, so this should always
 *  succeed for a real discovered row. Pure. */
function parseUncPath(path: string): { host: string; share: string } | null {
  const trimmed = path.trim().replace(/\//g, "\\");
  if (!trimmed.startsWith("\\\\")) return null;
  const [host, share = ""] = trimmed.slice(2).split("\\").filter((s) => s.length > 0);
  if (!host) return null;
  return { host, share };
}

/** Split a `scheme://host[:port]` address (no UNC involved) into its parts — the shape `cpe-mdns`'s
 *  `map_mdns_service` emits via `NetworkShare::to_url()` (CPE-1523: sftp/webdav/davs/ftp/nfs rows, the
 *  port folded into the authority and omitted when it's the scheme's default). `null` when the string
 *  isn't `scheme://…`-shaped, or the authority has no host at all. Doesn't validate the scheme itself —
 *  the caller/`buildConnection` does that (an unsupported scheme, e.g. `nfs`, still parses fine here; it
 *  just won't validate as a savable connection yet). Pure. */
function parseSchemeAuthority(path: string): { scheme: string; host: string; port: string } | null {
  const m = /^([a-z][a-z0-9+.-]*):\/\/([^/]+)/i.exec(path.trim());
  if (!m) return null;
  const scheme = m[1].toLowerCase();
  const authority = m[2];
  const colonIdx = authority.lastIndexOf(":");
  if (colonIdx <= 0) return authority ? { scheme, host: authority, port: "" } : null;
  const host = authority.slice(0, colonIdx);
  const port = authority.slice(colonIdx + 1);
  if (!host || !/^\d+$/.test(port)) return host ? { scheme, host, port: "" } : null;
  return { scheme, host, port };
}

/** Map a discovered share (`kind: "discovered"`) to a pre-filled "＋ Add a connection" form — one click
 *  from a discovered row to a ready-to-save connection, needing only a name (and credentials, if the
 *  share isn't anonymous). Two source shapes (CPE-1519 tier 3 + CPE-1523's mDNS rows share this tier):
 *  - a WNet-found `\\server\share` UNC → scheme `smb`, host = the server, path = `/share` (so the saved
 *    connection's `location()` round-trips back through `parse_share`/`location.rs` to the same
 *    host+share);
 *  - an mDNS-resolved `scheme://host[:port]` address (CPE-1523) → that scheme/host/port verbatim, name
 *    from the row's own label (its TXT friendly-name-or-hostname, already resolved server-side in
 *    `map_mdns_service`) so the prefilled name matches what the row displayed.
 *  Falls back to a blank `smb` form when `share.path` matches neither shape (shouldn't happen for a real
 *  backend row, but never throws either way). Pure. */
export function discoveredShareToFormInput(share: NetShare): ConnectionFormInput {
  const blank = blankConnectionForm();

  const unc = parseUncPath(share.path);
  if (unc) {
    return {
      ...blank,
      scheme: "smb",
      name: unc.share ? `${unc.host}-${unc.share}` : unc.host,
      host: unc.host,
      path: unc.share ? `/${unc.share}` : "",
    };
  }

  const authority = parseSchemeAuthority(share.path);
  if (authority) {
    return {
      ...blank,
      scheme: authority.scheme,
      name: share.name.trim() || authority.host,
      host: authority.host,
      port: authority.port,
    };
  }

  return { ...blank, scheme: "smb" };
}

// ---- Cross-platform discovery merge (CPE-1523) -----------------------------------------------------

/** Merge the WNet tier's Windows-only discovered rows with the cross-platform mDNS tier's, deduping by
 *  path (same case/slash-insensitive key `isDuplicateShare` uses) so a host both tiers happen to surface
 *  — e.g. a Windows box's WNet neighborhood AND its own mDNS SMB advertisement — collapses to one row
 *  rather than showing twice. `windows` rows win a duplicate (listed/kept first): WNet's UNC form is the
 *  more directly-connectable address for the smb rows both tiers can produce. Pure — mirrors
 *  `cpe_server::net_share::flatten_discovered`'s "first occurrence wins" dedup on the Rust side, and is
 *  the single merge point `loadDiscovered` calls after its `Promise.all`. */
export function mergeDiscovered(windows: NetShare[], mdns: NetShare[]): NetShare[] {
  const seen = new Set<string>();
  const out: NetShare[] = [];
  for (const share of [...windows, ...mdns]) {
    const key = shareDedupKey(share.path);
    if (!key || seen.has(key)) continue;
    seen.add(key);
    out.push(share);
  }
  return out;
}
