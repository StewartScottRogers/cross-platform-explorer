// The shared layout-guard dev server's lifecycle — start it, prove it is OURS, kill its whole tree.
//
// CPE-1968 lifted this OUT of run.mjs rather than let a second harness grow a second copy of it
// (CLAUDE.md, CPE-1950: "where the duplication is removable, remove it"). Every line here is
// load-bearing and was paid for by a real incident, which is exactly why a paraphrased second copy
// would have been worse than useless:
//
//  - the PORT is derived from the pid, because this repo routinely runs many worktrees on one machine
//    at once. A fixed port let one run's vite fail to bind while `waitForHttp` still got a 200 from
//    ANOTHER worktree's already-running server, and the run raced ahead measuring the wrong tree's
//    pages under its own name. That is not hypothetical: a `.tv-sync-badge` fixture that existed in no
//    worktree's committed code turned up in one worktree's measurement.
//  - `waitForViteBoundHere` waits for THIS child's own stdout to announce THIS port. A generic HTTP
//    200 is satisfied just as happily by the foreign server above, so it is the belt, not the braces.
//    Vite's banner is ANSI-coloured mid-token (`localhost:\x1b[1m37999\x1b[22m/`), so the escapes must
//    be stripped before matching or it never matches at all.
//  - `shell: true` because spawning `npm`/`npm.cmd` directly on Windows throws EINVAL.
//  - `detached` on POSIX puts the child in its OWN process group so the teardown can kill the group.
//    Without it, `kill()` reaped only the `sh` wrapper and left `npm`/`vite`/`esbuild` orphans holding
//    this process's stdout pipe open, so Node never saw EOF and never exited: CI job 98383557464
//    printed PASS in under 20 seconds and then sat for ~14.7 more minutes until GitHub force-killed it.
//    On Windows the equivalent is `taskkill /T`, because with `shell: true` the pid is the shell's.
import { spawn } from "node:child_process";
import { REPO_ROOT } from "./engine.mjs";

/** A port unlikely to collide with a concurrently-running harness in another worktree. */
export function harnessPortFor(pid = process.pid) {
  return 30000 + (pid % 20000);
}

export function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

export async function waitForHttp(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      if ((await fetch(url)).ok) return true;
    } catch {
      /* not up yet */
    }
    await sleep(150);
  }
  return false;
}

/** Resolve once THIS child announces, on its own stdout, that it bound `port`. Rejects by name if
 *  vite reports the port taken, rather than waiting out the whole timeout for a doomed run. */
export function waitForViteBoundHere(vite, port, timeoutMs) {
  return new Promise((resolve, reject) => {
    let settled = false;
    const settle = (fn, arg) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      fn(arg);
    };
    const onData = (buf) => {
      const text = buf.toString().replace(/\x1b\[[0-9;]*m/g, "");
      if (text.includes(`localhost:${port}`) && /local/i.test(text)) settle(resolve, undefined);
      if (/eaddrinuse|already in use|port .* is in use/i.test(text)) {
        settle(
          reject,
          new Error(`vite could not bind port ${port} — a foreign process already holds it: ${text.trim()}`),
        );
      }
    };
    vite.stdout.on("data", onData);
    vite.stderr.on("data", onData);
    vite.once("exit", (code) => {
      if (code !== null && code !== 0) {
        settle(reject, new Error(`vite exited (code ${code}) before announcing it bound port ${port}`));
      }
    });
    const timer = setTimeout(
      () => settle(reject, new Error(`vite never announced binding port ${port} within ${timeoutMs}ms`)),
      timeoutMs,
    );
  });
}

/**
 * Start the shared layout-guard dev server on `port` and return its child process, only once BOTH
 * checks pass: this child announced binding this port, and a real HTTP round-trip answered.
 */
export async function startDevServer(port, { bindTimeoutMs = 30000, httpTimeoutMs = 10000 } = {}) {
  const vite = spawn("npm", ["run", "harness:layout-guard-server", "--", "--port", String(port), "--strictPort"], {
    cwd: REPO_ROOT,
    stdio: ["ignore", "pipe", "pipe"],
    shell: true,
    detached: process.platform !== "win32",
  });
  try {
    await waitForViteBoundHere(vite, port, bindTimeoutMs);
    if (!(await waitForHttp(`http://localhost:${port}/`, httpTimeoutMs))) {
      throw new Error("vite announced binding the port but the dev server never answered HTTP");
    }
  } catch (e) {
    stopDevServer(vite);
    throw e;
  }
  return vite;
}

/** Kill the dev server's WHOLE process tree. See this file's header for why the direct child is not
 *  enough on either platform. Safe to call from a `finally`: it never throws. */
export function stopDevServer(vite) {
  if (!vite?.pid) return;
  if (process.platform === "win32") {
    spawn("taskkill", ["/pid", String(vite.pid), "/T", "/F"], { stdio: "ignore" });
  } else {
    try {
      process.kill(-vite.pid, "SIGKILL");
    } catch {
      vite.kill("SIGKILL");
    }
  }
}
