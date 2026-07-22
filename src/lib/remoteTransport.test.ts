import { describe, it, expect } from "vitest";
import { RemoteTransport, RemoteCallError, type SocketLike } from "./remoteTransport";

// A fake `cpe-net` server: it answers the Hello with a Welcome, then routes each `request` envelope
// through `respond`, which returns the reply envelope's `message` (or null to stay silent). This proves
// the handshake + id-correlation logic without a real socket, exactly as the browser would drive it.
type Respond = (method: string, params: unknown, id: number) => Record<string, unknown> | null;

class MockSocket implements SocketLike {
  onopen: ((ev?: unknown) => void) | null = null;
  onmessage: ((ev: { data: unknown }) => void) | null = null;
  onerror: ((ev?: unknown) => void) | null = null;
  onclose: ((ev?: unknown) => void) | null = null;
  readonly sent: string[] = [];

  constructor(private readonly respond: Respond) {
    // Open on a microtask so the transport has attached its handlers first (mirrors a real socket).
    queueMicrotask(() => this.onopen?.());
  }

  send(data: string): void {
    this.sent.push(data);
    const env = JSON.parse(data);
    const msg = env.message;
    const reply = (message: Record<string, unknown>) =>
      queueMicrotask(() => this.onmessage?.({ data: JSON.stringify({ schema_version: 1, id: env.id, message }) }));

    if (msg.type === "hello") {
      reply({
        type: "welcome",
        server_id: "mock",
        server_version: "0",
        negotiated_version: { major: 1, minor: 0 },
      });
      return;
    }
    if (msg.type === "request") {
      const out = this.respond(msg.method, msg.params, env.id);
      if (out) reply(out);
    }
  }

  close(): void {
    this.onclose?.();
  }
}

const ok = (value: unknown) => ({ type: "response", result: { Ok: value } });
const err = (code: string, message: string, retryable = false) => ({
  type: "response",
  result: { Err: { code, message, retryable } },
});

function transportWith(respond: Respond): RemoteTransport {
  return new RemoteTransport("ws://mock", { socketFactory: (url) => new MockSocket(respond) as SocketLike & { url?: string } });
}

describe("RemoteTransport", () => {
  it("handshakes then resolves a call with its result", async () => {
    const t = transportWith((method, params) => {
      expect(method).toBe("list_dir");
      expect(params).toEqual({ path: "/tmp" });
      return ok([{ name: "a.txt" }]);
    });
    const entries = await t.invoke<{ name: string }[]>("list_dir", { path: "/tmp" });
    expect(entries).toEqual([{ name: "a.txt" }]);
  });

  it("sends Hello before any request", async () => {
    let firstMethod: string | undefined;
    const socketRef: { s?: MockSocket } = {};
    const t = new RemoteTransport("ws://mock", {
      socketFactory: () => {
        const s = new MockSocket((m) => {
          firstMethod ??= m;
          return ok(null);
        });
        socketRef.s = s;
        return s as SocketLike;
      },
    });
    await t.invoke("noop");
    // First frame the client sent must be the Hello; the request follows.
    const firstSent = JSON.parse(socketRef.s!.sent[0]);
    expect(firstSent.message.type).toBe("hello");
    expect(firstSent.message.contract_version).toEqual({ major: 1, minor: 0 });
    expect(firstMethod).toBe("noop");
  });

  it("rejects with a RemoteCallError carrying the contract code", async () => {
    const t = transportWith(() => err("not_found", "no such path", false));
    await expect(t.invoke("list_dir", { path: "/nope" })).rejects.toMatchObject({
      name: "RemoteCallError",
      code: "not_found",
      retryable: false,
      message: "no such path",
    });
    await expect(t.invoke("list_dir", { path: "/nope" })).rejects.toBeInstanceOf(RemoteCallError);
  });

  it("correlates concurrent calls to their own replies", async () => {
    // Reply out of order: the second request answers first, proving id-correlation (not FIFO).
    const t = transportWith((method) => (method === "fast" ? ok("F") : ok("S")));
    const [s, f] = await Promise.all([t.invoke<string>("slow"), t.invoke<string>("fast")]);
    expect(s).toBe("S");
    expect(f).toBe("F");
  });

  it("fails in-flight calls when the socket closes", async () => {
    let sock: MockSocket | undefined;
    const t = new RemoteTransport("ws://mock", {
      socketFactory: () => {
        // Never answer the request, so the pending call is outstanding when we force a close.
        sock = new MockSocket((m) => (m === "hangs" ? null : ok(null)));
        return sock as SocketLike;
      },
    });
    const pending = t.invoke("hangs");
    // Let the handshake + request send fully settle, then drop the connection.
    await new Promise((r) => setTimeout(r, 10));
    sock!.close();
    await expect(pending).rejects.toThrow(/connection closed/);
  });
});
