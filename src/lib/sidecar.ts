import { invoke } from "@tauri-apps/api/core";

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
    const ids = await invoke<string[]>("sidecar_registry_ids");
    return Array.isArray(ids) ? ids : [];
  } catch {
    return [];
  }
}

/**
 * Start (or reuse) the AI Console sidecar and return the URL of the UI it serves, so the
 * caller can mount it in an iframe pane (CPE-271). Returns `null` when the platform is off
 * or the sidecar couldn't start — never throws.
 */
export async function startAiConsole(): Promise<string | null> {
  try {
    const url = await invoke<string>("sidecar_start_ai_console");
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

/** Whether the sidecar platform is active in this build (i.e. the command exists). */
export async function platformActive(): Promise<boolean> {
  try {
    await invoke<string[]>("sidecar_registry_ids");
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
    const s = await invoke<ConsentState>("sidecar_consent_state", { id });
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
    await invoke("sidecar_set_consent", { id, granted, decided });
    return true;
  } catch {
    return false;
  }
}

/** Revoke a previously-granted capability. Takes effect on the sidecar's next launch. */
export async function revokeCapability(id: string, capability: Capability): Promise<boolean> {
  try {
    await invoke("sidecar_revoke_capability", { id, capability });
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
  requested: Capability[];
  granted: Capability[];
}

/** Details for every registered sidecar (management panel). `[]` when the platform is off. */
export async function sidecarDetails(): Promise<SidecarInfo[]> {
  try {
    const rows = await invoke<SidecarInfo[]>("sidecar_details");
    return Array.isArray(rows) ? rows : [];
  } catch {
    return [];
  }
}

/** Stop a running sidecar. Resolves `false` if the platform is off or the call failed. */
export async function stopSidecar(id: string): Promise<boolean> {
  try {
    await invoke("sidecar_stop", { id });
    return true;
  } catch {
    return false;
  }
}

/** Enable or disable a sidecar (disabling also stops it). */
export async function setEnabled(id: string, enabled: boolean): Promise<boolean> {
  try {
    await invoke("sidecar_set_enabled", { id, enabled });
    return true;
  } catch {
    return false;
  }
}
