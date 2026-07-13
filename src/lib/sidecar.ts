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
