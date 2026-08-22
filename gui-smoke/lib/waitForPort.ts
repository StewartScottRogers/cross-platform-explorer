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
