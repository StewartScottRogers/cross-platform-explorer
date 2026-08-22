// CPE-1832 — headless unit tests for `waitForPort.ts`, the bounded TCP-readiness poll `wdio.conf.ts`'s
// `beforeSession` relies on to close BOTH tauri-driver startup races (CPE-1772's wdio -> tauri-driver
// race, and CPE-1832's tauri-driver -> native-WebDriver race). Runs under Node's built-in test runner via
// `tsx` (same convention as `shard.test.ts`/`ratchet.test.ts`):
//   npm run test:unit          (from gui-smoke/)
//
// These tests exercise real `net.Server` listeners on real (ephemeral) loopback ports — no mocking of
// `net` itself — so they are proving the actual TCP behaviour the fix depends on, not a stand-in for it.
import assert from "node:assert/strict";
import net from "node:net";
import { after, describe, it } from "node:test";
import { waitForPort } from "./waitForPort.js";

/** Starts a bare TCP listener on an OS-assigned ephemeral port and returns it plus the port number. */
function listenEphemeral(): Promise<{ server: net.Server; port: number }> {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        reject(new Error("expected an AddressInfo from an ephemeral listen()"));
        return;
      }
      resolve({ server, port: address.port });
    });
  });
}

describe("waitForPort — the startup-race guard beforeSession relies on", () => {
  const openServers: net.Server[] = [];
  after(() => {
    for (const s of openServers) s.close();
  });

  it("resolves once a real listener is accepting connections", async () => {
    const { server, port } = await listenEphemeral();
    openServers.push(server);
    // Does not throw — the whole point of the assertion.
    await waitForPort("127.0.0.1", port, 2_000, "test listener");
  });

  // THE CORE FALSIFIABILITY CHECK for the "no fixed sleep" acceptance criterion (CPE-1832). A
  // `sleep(budgetMs)` implementation can never resolve before `budgetMs` has elapsed, no matter how fast
  // the listener actually became ready — so giving it a listener that opens almost immediately, against a
  // budget an order of magnitude larger, distinguishes "polls and returns as soon as ready" from "always
  // waits the full budget". A generous margin (elapsed comfortably under budget/2) keeps this from being
  // timing-flaky on a loaded CI runner while still failing hard against a sleep-based implementation.
  it("resolves quickly once the listener comes up, rather than always waiting the full budget", async () => {
    const { server, port } = await listenEphemeral();
    // Not yet accepting new connections deliberately delayed: close it and re-listen after a short delay
    // to simulate "starts fast, but not instantly" — the real-world shape of a spawned driver process.
    await new Promise<void>((resolve) => server.close(() => resolve()));

    const budgetMs = 5_000;
    const readyAfterMs = 150;
    const relisten = setTimeout(() => {
      const relistened = net.createServer();
      relistened.listen(port, "127.0.0.1");
      openServers.push(relistened);
    }, readyAfterMs);

    const start = Date.now();
    try {
      await waitForPort("127.0.0.1", port, budgetMs, "test listener");
    } finally {
      clearTimeout(relisten);
    }
    const elapsed = Date.now() - start;
    assert.ok(
      elapsed < budgetMs / 2,
      `expected waitForPort to return well before the ${budgetMs}ms budget once the listener was ready ` +
        `after ${readyAfterMs}ms, but it took ${elapsed}ms — a fixed-sleep implementation would always ` +
        `take the full budget regardless of how fast the listener actually came up`,
    );
  });

  it("REJECTS with a bounded-deadline error naming the label and address when nothing ever listens", async () => {
    // A port nothing is listening on and nothing ever will be — 1 is a well-known reserved port that is
    // never bindable/listenable by an unprivileged test process, so this is deterministic, not "probably
    // free right now".
    const budgetMs = 300;
    const start = Date.now();
    await assert.rejects(
      () => waitForPort("127.0.0.1", 1, budgetMs, "the never-ready listener", 50),
      (err: unknown) => {
        assert.ok(err instanceof Error);
        assert.match(err.message, /the never-ready listener/);
        assert.match(err.message, /127\.0\.0\.1:1/);
        assert.match(err.message, new RegExp(`${budgetMs}ms`));
        return true;
      },
    );
    const elapsed = Date.now() - start;
    // Bounded, not indefinite: the whole point of "poll with a deadline, not sleep forever" — allow slack
    // above budgetMs for the final failed connect attempt's own OS-level timing, but it must not run away.
    assert.ok(elapsed < budgetMs + 2_000, `expected the deadline to actually bound the wait, took ${elapsed}ms`);
  });
});
