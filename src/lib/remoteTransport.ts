// CPE-819 (epic CPE-810): the remote `Transport` — runs the whole GUI against a headless `cpe-net`
// reference server over a WebSocket instead of in-process Tauri IPC. It plugs into the transport seam in
// `./invoke` (`setTransport`), so every call site (`invoke`/`rawInvoke`) reaches the remote server
// unchanged; only the active transport differs.
//
// The frontend is the browser end of the CPE-811 envelope: each `invoke(cmd, args)` becomes a `request`
// message on a WebSocket text frame, correlated by a monotonic `id`, and resolves when the matching
// `response` frame arrives. The connection opens with the Hello→Welcome handshake (the server speaks it
// over the same socket, slice 2). Streaming (`stream_item`/`stream_end`) is a further slice; this base
// transport carries the request/response surface (the ~113 explorer commands).
//
// std-only on the server side, and here just the browser-native `WebSocket` — no extra deps. The socket
// factory is injectable so the handshake + correlation logic is unit-testable headlessly against a mock.

import type { Transport } from "./invoke";

/** The frame schema + contract version this client speaks (mirrors `cpe_contract`). */
const SCHEMA_VERSION = 1;
const CONTRACT_VERSION = { major: 1, minor: 0 } as const;

/** The structured error a `response`/`error` frame carries (mirrors `cpe_contract::ContractError`). */
export interface RemoteError {
  code: string;
  message: string;
  retryable: boolean;
}

/** The minimal WebSocket surface this transport uses — the browser `WebSocket` satisfies it, and a test
 *  can supply a fake. */
export interface SocketLike {
  send(data: string): void;
  close(): void;
  onopen: ((ev?: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onerror: ((ev?: unknown) => void) | null;
  onclose: ((ev?: unknown) => void) | null;
}

/** How to open a socket to `url`. Defaults to the browser `WebSocket`; injectable for tests. */
export type SocketFactory = (url: string) => SocketLike;

const defaultFactory: SocketFactory = (url) => new WebSocket(url) as unknown as SocketLike;

interface Pending {
  resolve: (value: unknown) => void;
  reject: (reason: unknown) => void;
}

/** An `Error` that keeps the contract error code + retryable flag so call sites can branch on them. */
export class RemoteCallError extends Error {
  readonly code: string;
  readonly retryable: boolean;
  constructor(err: RemoteError) {
    super(err.message);
    this.name = "RemoteCallError";
    this.code = err.code;
    this.retryable = err.retryable;
  }
}

/** A {@link Transport} backed by a `cpe-net` server over WebSocket (CPE-819). */
export class RemoteTransport implements Transport {
  private readonly url: string;
  private readonly clientId: string;
  private readonly clientVersion: string;
  private readonly factory: SocketFactory;

  private socket: SocketLike | null = null;
  private ready: Promise<void> | null = null;
  private handshaked = false;
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();

  constructor(
    url: string,
    opts: { clientId?: string; clientVersion?: string; socketFactory?: SocketFactory } = {},
  ) {
    this.url = url;
    this.clientId = opts.clientId ?? "cpe-gui";
    this.clientVersion = opts.clientVersion ?? "0";
    this.factory = opts.socketFactory ?? defaultFactory;
  }

  /** Open the socket and complete the Hello→Welcome handshake, once. Subsequent calls await the same
   *  connection. A failed connection is not cached, so a later call can retry. */
  private connect(): Promise<void> {
    if (this.ready) return this.ready;
    this.ready = new Promise<void>((resolve, reject) => {
      const socket = this.factory(this.url);
      this.socket = socket;
      socket.onopen = () => {
        socket.send(
          JSON.stringify({
            schema_version: SCHEMA_VERSION,
            id: 0,
            message: {
              type: "hello",
              client_id: this.clientId,
              client_version: this.clientVersion,
              contract_version: CONTRACT_VERSION,
            },
          }),
        );
      };
      socket.onmessage = (ev) => this.onMessage(String(ev.data), resolve, reject);
      socket.onerror = () => reject(new Error("remote transport: websocket error"));
      socket.onclose = () => {
        // A close before/after handshake fails everything in flight and resets so the next call reconnects.
        this.failAll(new Error("remote transport: connection closed"));
      };
    }).catch((e) => {
      this.ready = null;
      this.handshaked = false;
      throw e;
    });
    return this.ready;
  }

  private onMessage(data: string, resolveReady: () => void, rejectReady: (e: unknown) => void): void {
    let env: { id?: number; message?: { type?: string; [k: string]: unknown } };
    try {
      env = JSON.parse(data);
    } catch {
      return; // ignore anything that isn't a JSON envelope
    }
    const msg = env.message;
    if (!msg || typeof msg.type !== "string") return;

    if (!this.handshaked) {
      if (msg.type === "welcome") {
        this.handshaked = true;
        resolveReady();
      } else if (msg.type === "rejected") {
        rejectReady(new Error(`remote transport: handshake rejected — ${String(msg.reason ?? "")}`));
      }
      return;
    }

    const id = env.id;
    if (typeof id !== "number") return;
    const p = this.pending.get(id);
    if (!p) return;
    this.pending.delete(id);

    if (msg.type === "response") {
      const result = msg.result as { Ok?: unknown; Err?: RemoteError } | undefined;
      if (result && "Ok" in result) {
        p.resolve(result.Ok);
      } else if (result && result.Err) {
        p.reject(new RemoteCallError(result.Err));
      } else {
        p.reject(new Error("remote transport: malformed response"));
      }
    } else if (msg.type === "error") {
      p.reject(new RemoteCallError(msg as unknown as RemoteError));
    }
  }

  private failAll(err: Error): void {
    for (const p of this.pending.values()) p.reject(err);
    this.pending.clear();
    this.ready = null;
    this.handshaked = false;
    this.socket = null;
  }

  /** Same shape as Tauri's `invoke`: send a `request` and resolve with its `response` result. */
  async invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    await this.connect();
    const socket = this.socket;
    if (!socket) throw new Error("remote transport: not connected");
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      socket.send(
        JSON.stringify({
          schema_version: SCHEMA_VERSION,
          id,
          message: { type: "request", method: cmd, params: args ?? {} },
        }),
      );
    });
  }

  /** Close the socket and reject anything still in flight. */
  close(): void {
    this.socket?.close();
    this.failAll(new Error("remote transport: closed"));
  }
}
