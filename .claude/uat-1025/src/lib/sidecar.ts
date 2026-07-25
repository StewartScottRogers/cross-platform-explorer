import { unwrap } from "./invoke";
import { commands } from "./bindings.gen"; // typed client (CPE-964)

/**
 * Frontend client for the sidecar platform (ADR 0001, docs/adr/0001-sidecar-platform.md).
 *
 * The backend commands exist only when the app is built with the `sidecar-platform`
 * Cargo feature (CPE-272). When it isn't, invoking them rejects — so every call here
 * degrades gracefully to an "off" result rather than throwing, keeping the plain
 * explorer completely unaffected (the delete-test, applied to the UI layer).
 *
 * This is the data layer the management panel (CPE-274) and the pane mount (CPE-271)
 * build on.
 */

/** Ids of the sidecars registered in the bundled + user registry. Returns `[]` when the
 *  platform feature is off or the command is otherwise unavailable. */
export async function listSidecars(): Promise<string[]> {
  try {
    const ids = await commands.sidecarRegistryIds();
    return Array.isArray(ids) ? ids : [];
  } catch {
    return [];
  }
}

/**
 * Start (or reuse) the Agent Deck sidecar and return the URL of the UI it serves, so the
 * caller can mount it in an iframe pane (CPE-271). Returns `null` when the platform is off
 * or the sidecar couldn't start — never throws.
 */
/**
 * Append an explorer "work on this" context to the Agent Deck loopback URL as query
 * params (CPE-313). The console page reads `cwd`/`task` client-side to pre-scope a
 * session — a decoupled hand-off: the explorer never touches console internals, it just
 * opens a URL. Empty/absent values are omitted so the plain open is unchanged.
 */
export function consoleUrlWith(baseUrl: string, cwd?: string, task?: string, session?: string): string {
  const params = new URLSearchParams();
  if (cwd && cwd.trim()) params.set("cwd", cwd.trim());
  if (task && task.trim()) params.set("task", task.trim());
  if (session && session.trim()) params.set("session", session.trim()); // CPE-532: focus this tab on open
  const qs = params.toString();
  if (!qs) return baseUrl;
  return baseUrl + (baseUrl.includes("?") ? "&" : "?") + qs;
}

export async function startAiConsole(): Promise<string | null> {
  try {
    const url = unwrap(await commands.sidecarStartAiConsole());
    return typeof url === "string" && url.length > 0 ? url : null;
  } catch {
    return null;
  }
}

/**
 * Start the Agent Board sidecar and return the loopback URL of the Kanban UI it serves (CPE-853), or
 * `null` when unavailable (plain build / start failed) so the caller can fall back. `root` scopes which
 * project's `Tickets/` the board reads.
 */
export async function startAgentBoard(root?: string): Promise<string | null> {
  try {
    const url = unwrap(await commands.sidecarStartAgentBoard(root ?? null));
    return typeof url === "string" && url.length > 0 ? url : null;
  } catch {
    return null;
  }
}

/**
 * Extract the mount URL from a sidecar's `ui:<url>` Status announcement (CPE-271), or
 * `null` if it isn't one. Only loopback URLs are accepted — a sidecar must not point the
 * host's iframe at an off-machine address.
 */
export function parseUiAnnouncement(state: string): string | null {
  const prefix = "ui:";
  if (!state.startsWith(prefix)) return null;
  const url = state.slice(prefix.length);
  const loopback = url.startsWith("http://127.0.0.1:") || url.startsWith("http://localhost:");
  return loopback ? url : null;
}

// --- Agent Watch: active session registry (CPE-396) -----------------------------------

/** A live coding-agent session launched from the Agent Deck, as surfaced to the explorer.
 *  `cwd` is the agent's Project folder — the anchor the explorer navigates to / watches. */
export interface AgentSession {
  sessionId: string;
  agentId: string;
  agentName: string;
  provider: string;
  model: string;
  cwd: string;
}

/** A session lifecycle announcement decoded from a `session:<json>` Status (CPE-396). */
export interface SessionAnnouncement {
  event: "started" | "ended";
  session: AgentSession;
}

/**
 * Decode a `session:<json>` Status announcement into a typed lifecycle event, or `null` if it
 * isn't one / is malformed (sibling of {@link parseUiAnnouncement}). Kept pure + defensive so a
 * bad frame can never throw into the event listener; the wire format is unit-tested.
 */
export function parseSessionAnnouncement(state: string): SessionAnnouncement | null {
  const prefix = "session:";
  if (!state.startsWith(prefix)) return null;
  let o: Record<string, unknown>;
  try {
    o = JSON.parse(state.slice(prefix.length));
  } catch {
    return null;
  }
  const event = o.event;
  if (event !== "started" && event !== "ended") return null;
  const sessionId = typeof o.sessionId === "string" ? o.sessionId : "";
  if (!sessionId) return null;
  const str = (v: unknown) => (typeof v === "string" ? v : "");
  return {
    event,
    session: {
      sessionId,
      agentId: str(o.agentId),
      agentName: str(o.agentName),
      provider: str(o.provider),
      model: str(o.model),
      cwd: str(o.cwd),
    },
  };
}

/**
 * Pure reducer for the active-session list: a "started" adds/replaces the session (keyed by id),
 * an "ended" drops it. Order is preserved with newest appended, so the UI is stable.
 */
export function applySessionAnnouncement(
  list: AgentSession[],
  a: SessionAnnouncement,
): AgentSession[] {
  const rest = list.filter((s) => s.sessionId !== a.session.sessionId);
  return a.event === "ended" ? rest : [...rest, a.session];
}

// --- Agent Watch: filesystem activity (CPE-398) ---------------------------------------

/** One coalesced filesystem action under a watched Project folder, as emitted by the host. `read`
 *  (CPE-405) comes from the agent's own tool-output stream, not the FS watcher, and is the weakest
 *  signal — a file the agent consulted rather than changed. */
export interface FsActivity {
  kind: "created" | "modified" | "removed" | "renamed" | "read";
  path: string;
}

/** Start watching an agent's Project folder for filesystem activity (CPE-398). Resolves `false`
 *  when the platform is off or the path can't be watched — never throws. */
export async function startAgentWatch(path: string): Promise<boolean> {
  try {
    unwrap(await commands.agentWatchStart(path));
    return true;
  } catch {
    return false;
  }
}

/** Stop the active filesystem watch (CPE-398). Safe + idempotent when the platform is off. */
export async function stopAgentWatch(): Promise<void> {
  try {
    await commands.agentWatchStop();
  } catch {
    /* platform off — nothing to stop */
  }
}

/** Normalize the `ai-console://fs-activity` event payload into a clean, typed list, dropping
 *  anything malformed. Kept pure so the host→UI wire format is unit-testable headlessly. */
export function normalizeFsActivity(payload: unknown): FsActivity[] {
  if (!Array.isArray(payload)) return [];
  const kinds = new Set(["created", "modified", "removed", "renamed", "read"]);
  const out: FsActivity[] = [];
  for (const item of payload) {
    const kind = (item as { kind?: unknown })?.kind;
    const path = (item as { path?: unknown })?.path;
    if (typeof kind === "string" && kinds.has(kind) && typeof path === "string" && path) {
      out.push({ kind: kind as FsActivity["kind"], path });
    }
  }
  return out;
}

/** Whether the sidecar platform is active in this build (i.e. the command exists). */
export async function platformActive(): Promise<boolean> {
  try {
    await commands.sidecarRegistryIds();
    return true;
  } catch {
    return false;
  }
}

// --- Capability consent (CPE-296) -----------------------------------------------------

/** A capability a sidecar can request. Serialized snake_case to match the Rust enum. */
export type Capability = "context" | "secrets" | "storage" | "events" | "network";

/** The persisted consent picture for a sidecar: what it asks for, what's granted, and
 *  what still needs a decision (an undecided capability prompts the consent sheet). */
export interface ConsentState {
  requested: Capability[];
  granted: Capability[];
  undecided: Capability[];
}

/** Plain-language description + risk note per capability, shown in the consent sheet. */
export const CAPABILITY_INFO: Record<
  Capability,
  { label: string; description: string; sensitive: boolean }
> = {
  context: {
    label: "Explorer context",
    description: "Read the current folder and selection you're viewing in the explorer.",
    sensitive: false,
  },
  secrets: {
    label: "Secrets",
    description:
      "Store and read credentials (like API keys) in your OS keychain, scoped to this sidecar.",
    sensitive: true,
  },
  storage: {
    label: "Private storage",
    description: "Keep its own files in a private per-sidecar folder.",
    sensitive: false,
  },
  events: {
    label: "Notifications",
    description: "Send status and progress notifications to the app.",
    sensitive: false,
  },
  network: {
    label: "Network",
    description: "Make network connections (e.g. to a provider's API).",
    sensitive: true,
  },
};

/** Requested/granted/undecided capabilities for a sidecar. Returns `null` when the
 *  platform is off or the sidecar isn't in the registry — never throws. */
export async function consentState(id: string): Promise<ConsentState | null> {
  try {
    const s = unwrap(await commands.sidecarConsentState(id));
    return s && Array.isArray(s.requested) ? s : null;
  } catch {
    return null;
  }
}

/** Record a consent decision: `granted` are approved, the rest of `decided` are denied.
 *  Resolves `false` if the platform is off or the call failed. */
export async function setConsent(
  id: string,
  granted: Capability[],
  decided: Capability[],
): Promise<boolean> {
  try {
    unwrap(await commands.sidecarSetConsent(id, granted, decided));
    return true;
  } catch {
    return false;
  }
}

/** Revoke a previously-granted capability. Takes effect on the sidecar's next launch. */
export async function revokeCapability(id: string, capability: Capability): Promise<boolean> {
  try {
    unwrap(await commands.sidecarRevokeCapability(id, capability));
    return true;
  } catch {
    return false;
  }
}

// --- Management UI (CPE-274) -----------------------------------------------------------

/** A registered sidecar's identity, contract compatibility, and running/enabled state. */
export interface SidecarInfo {
  id: string;
  name: string;
  version: string;
  contract: string;
  compatible: boolean;
  running: boolean;
  enabled: boolean;
  /** Whether the sidecar's launchable binary actually resolves (CPE-863) — false = missing binary. */
  binary_ok: boolean;
  requested: Capability[];
  granted: Capability[];
}

/** Details for every registered sidecar (management panel). `[]` when the platform is off. */
export async function sidecarDetails(): Promise<SidecarInfo[]> {
  try {
    const rows = unwrap(await commands.sidecarDetails());
    return Array.isArray(rows) ? rows : [];
  } catch {
    return [];
  }
}

/** The outcome of a repair attempt (CPE-863): the re-checked binary presence + the steps taken. */
export interface SidecarRepair {
  id: string;
  binary_ok: boolean;
  actions: string[];
}

/** Best-effort self-heal for a sidecar (CPE-863): reap orphan daemons, clear a wedged connection + the
 *  stored error, and re-check the binary. `null` when the platform is off or the call failed. */
export async function repairSidecar(id: string): Promise<SidecarRepair | null> {
  try {
    return unwrap(await commands.sidecarRepair(id));
  } catch {
    return null;
  }
}

/** Stop a running sidecar. Resolves `false` if the platform is off or the call failed. */
export async function stopSidecar(id: string): Promise<boolean> {
  try {
    unwrap(await commands.sidecarStop(id));
    return true;
  } catch {
    return false;
  }
}

/** Enable or disable a sidecar (disabling also stops it). */
export async function setEnabled(id: string, enabled: boolean): Promise<boolean> {
  try {
    unwrap(await commands.sidecarSetEnabled(id, enabled));
    return true;
  } catch {
    return false;
  }
}

// --- Health, last error & logs (CPE-323) ----------------------------------------------

/** One redacted log line from a sidecar's diagnostics. */
export interface DiagLogLine {
  /** Severity, lower-case: `trace` | `debug` | `info` | `warn` | `error`. */
  level: string;
  /** The log message — already redacted host-side; never contains a secret. */
  message: string;
}

/** A sidecar's health snapshot for the management panel: running state, the last error
 *  that stopped it (redacted, `null` when healthy), and recent redacted log lines. */
export interface SidecarDiagnostics {
  id: string;
  running: boolean;
  last_error: string | null;
  logs: DiagLogLine[];
}

/** An empty (but valid) diagnostics record — the graceful "nothing to show" fallback used
 *  when the platform is off or the call fails, so the panel never has to special-case null. */
export function emptyDiagnostics(id: string): SidecarDiagnostics {
  return { id, running: false, last_error: null, logs: [] };
}

/** Fetch a sidecar's last error + recent redacted log lines (CPE-323). Returns an empty
 *  record rather than throwing when the platform is off or the sidecar has no diagnostics. */
export async function sidecarDiagnostics(id: string): Promise<SidecarDiagnostics> {
  try {
    const d = unwrap(await commands.sidecarDiagnostics(id));
    if (!d || typeof d !== "object") return emptyDiagnostics(id);
    return {
      id: typeof d.id === "string" ? d.id : id,
      running: !!d.running,
      last_error: typeof d.last_error === "string" ? d.last_error : null,
      logs: Array.isArray(d.logs) ? d.logs : [],
    };
  } catch {
    return emptyDiagnostics(id);
  }
}
