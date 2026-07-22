// CPE-548: shared busy-tracking `invoke` wrapper — the global boundary for the busy cursor (CPE-547).
//
// Importing `invoke` from HERE (instead of `@tauri-apps/api/core`) makes any Tauri command call flip
// the app-wide wait cursor when it runs long, for free — the busy tracker (CPE-482, `./busy`) handles
// the ~150 ms debounce + ref-counting, so fast calls never flicker and overlapping calls clear only on
// the last one. This is the single mechanism the epic's "everywhere" goal needs: call sites opt IN just
// by choosing this import.
//
// Streaming / self-progress call sites (agent sessions, updater downloads, sidecar streaming) that
// already show their own progress import `rawInvoke` instead, so they don't double-signal (see CPE-550).
import { invoke as coreInvoke, Channel as TauriChannel } from "@tauri-apps/api/core";
import { withBusy } from "./busy";
import { diagnosticsEnabled, recordCall } from "./diagnostics";

// ---- Transport seam (CPE-819, epic CPE-810) -----------------------------------------------------
//
// `invoke`/`rawInvoke` are the single chokepoint every backend call flows through, so this is the one
// place to make the whole GUI run either against **local in-process Tauri IPC** (the default) or a
// **remote server** speaking the contract envelope — chosen by config, with every call site unchanged
// (they just import `invoke`/`rawInvoke`). Only the active transport differs; busy-cursor + Diagnostics
// timing wrap the transport, so both behaviours survive the swap. The remote transport implementation +
// the streaming (`ipc::Channel`) equivalents land with the reference server (CPE-820); this is the seam
// they plug into, and it defaults to local so single-user/in-process behaviour is byte-for-byte unchanged.

/** A streaming sink: the caller sets `onmessage`, the transport pushes batches to it. Both a Tauri
 *  `ipc::Channel` (local) and the remote transport's `RemoteChannel` satisfy this shape, so a streaming
 *  call site is transport-agnostic (CPE-819). Create one via {@link createChannel}, never `new Channel()`. */
export interface StreamChannel<T = unknown> {
  onmessage: ((message: T) => void) | null;
}

/** A backend-call transport — same call shape as Tauri's `invoke`, plus a matching streaming channel. */
export interface Transport {
  invoke<T = unknown>(...args: Parameters<typeof coreInvoke>): Promise<T>;
  /** A streaming channel bound to this transport (local → Tauri `ipc::Channel`; remote → `RemoteChannel`). */
  createChannel<T = unknown>(): StreamChannel<T>;
}

/** The local transport: historical in-process Tauri IPC. The default; zero overhead. */
export const localTransport: Transport = {
  invoke: <T = unknown>(...args: Parameters<typeof coreInvoke>): Promise<T> => coreInvoke<T>(...args),
  createChannel: <T = unknown>(): StreamChannel<T> => new TauriChannel<T>() as unknown as StreamChannel<T>,
};

let activeTransport: Transport = localTransport;

/** Select the active transport (CPE-819). Pass `null` to reset to local IPC. Called once at startup from
 *  config; until then, and whenever local, behaviour is exactly the pre-seam in-process path. */
export function setTransport(t: Transport | null): void {
  activeTransport = t ?? localTransport;
}

/** Whether a non-local (remote) transport is currently active. */
export function isRemoteTransport(): boolean {
  return activeTransport !== localTransport;
}

/** Time a backend call for Diagnostics (CPE-758) when it's on — no-op / zero overhead when off. The
 *  command name is the first invoke arg. Records on both resolve and reject so failures still show. */
function timed<T>(cmd: unknown, p: Promise<T>): Promise<T> {
  if (!diagnosticsEnabled()) return p;
  const t0 = performance.now();
  return p.then(
    (v) => { recordCall(String(cmd), performance.now() - t0, true); return v; },
    (e) => { recordCall(String(cmd), performance.now() - t0, false); throw e; },
  );
}

/**
 * The untracked invoke — like `@tauri-apps/api/core`'s `invoke`, with NO busy-cursor tracking.
 * Use this only for streaming / self-progress operations that opt out of the global wait cursor
 * (CPE-550). Everywhere else, prefer the default {@link invoke} export. Still Diagnostics-timed (CPE-758)
 * and routed through the active {@link Transport} (CPE-819).
 */
export function rawInvoke<T = unknown>(...args: Parameters<typeof coreInvoke>): Promise<T> {
  return timed(args[0], activeTransport.invoke<T>(...args));
}

/**
 * Create a streaming {@link StreamChannel} bound to the active {@link Transport} (CPE-819). Under the
 * local transport this is a real Tauri `ipc::Channel` (native streaming, byte-for-byte the historical
 * path); under a remote transport it's a channel the transport feeds from `stream_item` frames. Streaming
 * call sites use this instead of `new Channel()` so they work over either transport unchanged.
 */
export function createChannel<T = unknown>(): StreamChannel<T> {
  return activeTransport.createChannel<T>();
}

/**
 * `invoke`, wrapped so a long-running call raises the app-wide busy cursor (CPE-547). A drop-in
 * replacement for `@tauri-apps/api/core`'s `invoke`: same arguments, same return value, and errors
 * propagate unchanged. The busy guard is released on BOTH resolve and reject, so a failing command can
 * never leave the cursor stuck. Routed through the active {@link Transport} (CPE-819).
 */
export function invoke<T = unknown>(...args: Parameters<typeof coreInvoke>): Promise<T> {
  return withBusy(() => timed(args[0], activeTransport.invoke<T>(...args)));
}
