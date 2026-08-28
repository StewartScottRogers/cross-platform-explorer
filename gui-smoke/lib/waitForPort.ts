// CPE-1772 / CPE-1832 — a bounded TCP-readiness poll, extracted out of `wdio.conf.ts` so the mechanism
// the whole tauri-driver-startup fix rests on is independently unit-testable (`waitForPort.test.ts`)
// without needing a real `tauri-driver`/`tauri build` round trip. See `wdio.conf.ts`'s `beforeSession`
// for the two startup races this closes (wdio -> tauri-driver's own port, and tauri-driver -> the native
// WebDriver's port it proxies to) and why BOTH need this, not just one.
//
// THE TRAP this exists to avoid: a fixed `sleep(budgetMs)` would also make the observed flake go away on
// a normal day, but it trades a flake for a slower run on every run (paying the worst case every time)
// and STILL races on a sufficiently loaded runner (the thing it slept for could take longer than the
// sleep). Polling with a real connect attempt returns the instant the listener is actually ready —
// typically far under `budgetMs` — and only ever waits the full budget when the listener genuinely never
// comes up, which is exactly the case that should fail loudly. `waitForPort.test.ts`'s "resolves quickly"
// case is the falsifiable check for this: it would go red under a `sleep(budgetMs)` implementation, which
// could never resolve before `budgetMs` elapsed no matter how fast the listener actually was.
import net from "node:net";

/** Polls `host:port` with a real TCP connect attempt (not an HTTP request — the listener doesn't need to
 *  answer anything yet, just accept the socket) every `intervalMs` until one succeeds or `budgetMs`
 *  elapses. Resolves on success; on timeout it REJECTS rather than silently letting the caller proceed to
 *  a doomed first request — a listener that never opens its port is a real, nameable failure, not
 *  something to paper over. `label` names WHICH listener this call is waiting for, so the timeout error
 *  is self-explanatory (distinguishing, e.g., "tauri-driver" from "the native WebDriver it spawns" —
 *  callers waiting on more than one listener need their own failure to say which one never came up). */
export function waitForPort(
  host: string,
  port: number,
  budgetMs: number,
  label: string,
  intervalMs = 200,
): Promise<void> {
  const deadline = Date.now() + budgetMs;
  return new Promise((resolve, reject) => {
    const attempt = () => {
      const socket = net.connect({ host, port }, () => {
        socket.end();
        resolve();
      });
      socket.on("error", () => {
        socket.destroy();
        if (Date.now() >= deadline) {
          reject(
            new Error(
              `[gui-smoke] ${label} never accepted a connection on ${host}:${port} within ${budgetMs}ms — ` +
                "it may have crashed or failed to start rather than just started slowly; check the driver " +
                "output above this error for the real cause.",
            ),
          );
        } else {
          setTimeout(attempt, intervalMs);
        }
      });
    };
    attempt();
  });
}

/**
 * The MIRROR of `waitForPort`: polls until NOTHING is accepting connections on `host:port`, i.e. the
 * previous listener has actually released it.
 *
 * CPE-1910 round 2. `scripts/run-suite.ts` spawns attempt 2 the instant attempt 1's process tree closes,
 * and attempt 2's `startTauriDriver` binds the same two FIXED ports (4444/4445) that attempt 1 was using.
 * `wdio.conf.ts`'s own teardown (`killTauriDriver`) is a bare non-waiting `tauriDriver?.kill()`, which
 * returns the moment SIGTERM is queued — not when the process is gone. `wdio.conf.ts`'s in-process
 * respawn already refuses to race exactly this, in its own words: *"a `.kill()` returns the instant
 * SIGTERM is queued, not when the process is gone, and racing that would have the readiness wait below
 * succeed against the DYING listener"*, and it solves it with `killAndWaitForExit`. A job-level retry
 * crosses a process boundary, so it cannot reuse that handle — this is the same guarantee from outside.
 *
 * The consequence of skipping it is not a slow start, it is a WRONG SUCCESS: attempt 2's `waitForPort`
 * connects to attempt 1's dying listener, calls the driver ready, and the real tauri-driver's bind then
 * fails — whereupon `startTauriDriver`'s own `exit` handler calls `process.exit(1)`. Port 4445 is the
 * worse of the two: the native WebKitWebDriver is a GRANDCHILD, never signalled directly, and it is
 * precisely the process already in a bad state on the path being retried.
 *
 * Resolves `true` when the port is free, `false` on timeout — a boolean rather than a rejection because
 * the caller (a retry driver) must be able to say loudly that the settle did not happen and still let the
 * attempt proceed to produce the authoritative error. `false` is never silently equivalent to `true`.
 *
 * NOT a fixed sleep, for `waitForPort`'s reasons above: on the overwhelmingly common path the port is
 * already free and the first probe returns in single-digit milliseconds, so a retry pays nothing.
 */
export function waitForPortFree(
  host: string,
  port: number,
  budgetMs: number,
  intervalMs = 200,
): Promise<boolean> {
  const deadline = Date.now() + budgetMs;
  return new Promise((resolve) => {
    const attempt = (): void => {
      const socket = net.connect({ host, port }, () => {
        // Someone is still accepting. Close our probe and try again until the deadline.
        socket.end();
        if (Date.now() >= deadline) resolve(false);
        else setTimeout(attempt, intervalMs);
      });
      socket.on("error", () => {
        // Connect refused/failed — nothing is listening, which is what "free" means here.
        socket.destroy();
        resolve(true);
      });
    };
    attempt();
  });
}
