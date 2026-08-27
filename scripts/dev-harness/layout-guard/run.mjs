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
const DEV_PORT = Number(process.env.HARNESS_DEV_PORT || 4331);

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
  let viteFailed = false;
  vite.on("exit", (code) => {
    if (code !== null && code !== 0) viteFailed = true;
  });

  try {
    const devUp = await waitForHttp(`http://localhost:${DEV_PORT}/`, 20000);
    if (!devUp || viteFailed) throw new Error("vite dev server never came up");
    console.log(`[layout-guard] dev server up on :${DEV_PORT}`);

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
