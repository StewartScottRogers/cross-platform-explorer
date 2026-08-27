// CPE-1882 — CLI entry point + CI job body for the generalised real-browser layout guard. Starts the
// ONE shared dev server (vite.harness.layout-guard.config.ts), sweeps every (case × width) in
// cases.mjs through engine.mjs, prints a result line per combination, and exits non-zero — naming
// exactly what moved — the moment any of them fails.
//
// Run:  node scripts/dev-harness/layout-guard/run.mjs
//   or: npm run harness:layout-guard
//
// Cost (measured locally, Windows, 2 cases / 15 width combinations total): ~35s dev-server cold start
// (first module-graph compile) + ~1-2s per width thereafter ≈ under a minute end to end. Cheap enough,
// and needs no WebDriver/native driver install and no `tauri build`, unlike gui-smoke — see this
// ticket's PR description for the full cost accounting and why it runs on every push/PR rather than
// being path-filtered.

import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { CASES } from "./cases.mjs";
import { REPO_ROOT, defaultChromePath, runAllCases } from "./engine.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
void __dirname; // kept for symmetry with sibling harness scripts; REPO_ROOT comes from engine.mjs

const CHROME = defaultChromePath();
// CPE-1882 UAT/reviewer finding: this used to be a fixed `4331` with `--strictPort`. This repo
// routinely runs many worktrees on one dev machine concurrently (the NORMAL condition here, not an
// edge case) — a fixed port meant a second concurrent `run.mjs` (a different worktree, same codebase)
// could have its OWN vite fail to bind (port taken) while `waitForHttp` below still got a 200 from the
// FIRST worktree's already-running server, racing ahead to measure a completely different worktree's
// harness pages under this run's name. A live case of exactly this: `.tv-sync-badge`, a fixture that
// exists in NO worktree's committed code, showed up in one worktree's measurement — because it was
// another worktree's own in-progress fixture. `process.pid` is unique per concurrently-running process
// on one machine, so deriving the port from it makes an accidental collision between two SEPARATE
// `run.mjs` invocations astronomically unlikely (as opposed to the OLD code's certainty of collision
// the moment two ran at once). `waitForViteBoundHere` below is the second, independent layer: even if a
// collision somehow still occurred, it refuses to proceed unless THIS run's own vite process announces
// binding THIS exact port on its own stdout — not merely "something answered HTTP on this port".
const DEV_PORT = Number(process.env.HARNESS_DEV_PORT || 30000 + (process.pid % 20000));

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function waitForHttp(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url);
      if (res.ok) return true;
    } catch {
      /* not up yet */
    }
    await sleep(150);
  }
  return false;
}

/** Waits for THIS process's own vite child to announce, on its OWN stdout, that it bound `port` —
 *  vite prints e.g. "Local:   http://localhost:<port>/" once it is actually listening. This is the
 *  authoritative signal (as opposed to `waitForHttp`'s generic "something answered", which a foreign
 *  process's already-running server on the same port can satisfy just as well — see the comment on
 *  `DEV_PORT` above for the real collision this closes). Rejects immediately and by name if vite's own
 *  stderr reports the port is already taken, rather than waiting out the full timeout for a doomed run. */
function waitForViteBoundHere(vite, port, timeoutMs) {
  return new Promise((resolve, reject) => {
    let settled = false;
    const settle = (fn, arg) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      fn(arg);
    };
    const onData = (buf) => {
      // vite colours its "ready"/"Local:" banner with ANSI escapes, and inserts one right between
      // "localhost:" and the port digits themselves (`localhost:\x1b[1m37999\x1b[22m/`) — a plain
      // `.includes("localhost:" + port)` never matches the raw bytes. Strip escape codes first.
      const text = buf.toString().replace(/\x1b\[[0-9;]*m/g, "");
      if (text.includes(`localhost:${port}`) && /local/i.test(text)) settle(resolve, undefined);
      if (/eaddrinuse|already in use|port .* is in use/i.test(text)) {
        settle(reject, new Error(`vite could not bind port ${port} — a foreign process already holds it: ${text.trim()}`));
      }
    };
    vite.stdout.on("data", onData);
    vite.stderr.on("data", onData);
    vite.once("exit", (code) => {
      if (code !== null && code !== 0) settle(reject, new Error(`vite exited (code ${code}) before announcing it bound port ${port}`));
    });
    const timer = setTimeout(
      () => settle(reject, new Error(`vite never announced binding port ${port} within ${timeoutMs}ms`)),
      timeoutMs,
    );
  });
}

function formatProblems(r) {
  const lines = [];
  for (const m of r.overlaps) lines.push(`    OVERLAP        ${m}`);
  for (const m of r.clipBreaches) lines.push(`    CLIP-BREACH    ${m}`);
  for (const m of r.textOverflows) lines.push(`    TEXT-OVERFLOW  ${m}`);
  for (const m of r.unpainted) lines.push(`    UNREACHABLE    ${m}`);
  for (const m of r.missing) lines.push(`    MISSING        ${m}`);
  return lines;
}

async function main() {
  console.log(`[layout-guard] chrome: ${CHROME}`);
  console.log("[layout-guard] starting vite dev server…");
  // `shell: true` on Windows: spawning "npm"/"npm.cmd" directly (no shell) throws EINVAL — see the
  // identical comment in sidebar-drop-stack-overlap/check.mjs.
  const vite = spawn("npm", ["run", "harness:layout-guard-server", "--", "--port", String(DEV_PORT), "--strictPort"], {
    cwd: REPO_ROOT,
    stdio: ["ignore", "pipe", "pipe"],
    shell: true,
  });

  try {
    // Two independent checks, in order: (1) THIS process's own vite child must announce, on its own
    // stdout, that it bound this exact port — see `waitForViteBoundHere`'s header for why a generic
    // HTTP 200 alone is not proof of that. (2) only once that is true, a real HTTP round-trip as a
    // last sanity check (belt + suspenders; (1) is the load-bearing one).
    await waitForViteBoundHere(vite, DEV_PORT, 30000);
    const devUp = await waitForHttp(`http://localhost:${DEV_PORT}/`, 10000);
    if (!devUp) throw new Error("vite announced binding the port but the dev server never answered HTTP");
    console.log(`[layout-guard] dev server up on :${DEV_PORT} (pid ${process.pid})`);

    const totalWidths = CASES.reduce((n, k) => n + k.widths.length, 0);
    console.log(`[layout-guard] sweeping ${CASES.length} case(s), ${totalWidths} width combination(s)…`);

    const results = await runAllCases({ cases: CASES, devServerBase: `http://localhost:${DEV_PORT}`, chromePath: CHROME });

    let failures = 0;
    for (const r of results) {
      const problems = formatProblems(r);
      if (problems.length > 0) {
        failures++;
        console.error(`[layout-guard] ${r.case} @ ${r.width}px: FAIL`);
        for (const line of problems) console.error(line);
      } else {
        console.log(`[layout-guard] ${r.case} @ ${r.width}px: OK`);
      }
    }

    if (failures > 0) {
      console.error(
        `\n[layout-guard] FAIL — ${failures}/${results.length} case/width combination(s) failed. See above for what moved.`,
      );
      process.exitCode = 1;
    } else {
      console.log(`\n[layout-guard] PASS — clean at all ${results.length} case/width combination(s).`);
    }
  } finally {
    // See sidebar-drop-stack-overlap/check.mjs's identical comment: `shell: true` means `vite.pid` is
    // the shell, not the real dev-server process, on Windows — `taskkill /T` kills the whole tree.
    if (process.platform === "win32") {
      spawn("taskkill", ["/pid", String(vite.pid), "/T", "/F"], { stdio: "ignore" });
    } else {
      vite.kill();
    }
  }
}

main().catch((e) => {
  console.error("[layout-guard] FAIL:", e);
  process.exit(1);
});
