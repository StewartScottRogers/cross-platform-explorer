// CPE-1882 — CLI entry point + CI job body for the generalised real-browser layout guard. Starts the
// ONE shared dev server (vite.harness.layout-guard.config.ts), sweeps every (case × width) in
// cases.mjs through engine.mjs, prints a result line per combination, and exits non-zero — naming
// exactly what moved — the moment any of them fails.
//
// Run:  node scripts/dev-harness/layout-guard/run.mjs
//   or: npm run harness:layout-guard
//
// Cost (measured locally, Windows, 2 cases / 12 width combinations total): ~1s dev-server cold start
// (npm cache warm) + well under a second per width thereafter — under half a minute end to end. Needs
// no WebDriver/native driver install and no `tauri build`, unlike gui-smoke. Local timing is NOT a
// stand-in for CI timing, though: the first real CI run of this job (before `runAllCases` in engine.mjs
// was changed to reuse ONE Chrome instance for the whole sweep, rather than launching a fresh one per
// width) hit the job's own 10-minute cap and was cancelled without completing — see
// `.github/workflows/gui-smoke.yml`'s `layout-guard` job comment and `runAllCases`'s own header for the
// root cause and the fix. See this ticket's PR description for the full cost accounting (including a
// real CI number once one exists) and why it runs on every push/PR rather than being path-filtered.

import path from "node:path";
import { fileURLToPath } from "node:url";
import { CASES } from "./cases.mjs";
import { defaultChromePath, runAllCases } from "./engine.mjs";
// CPE-1968 moved the dev server's whole lifecycle — pid-derived port, "is this OUR vite" handshake,
// process-TREE teardown — into ./dev-server.mjs so a second harness could reuse it rather than
// paraphrase it. Every comment that used to live at those call sites moved with the code; read that
// file's header before changing any of it, because each rule there is a fixed CI incident.
import { harnessPortFor, startDevServer, stopDevServer } from "./dev-server.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
void __dirname; // kept for symmetry with sibling harness scripts; REPO_ROOT comes from engine.mjs

const CHROME = defaultChromePath();
// Pid-derived, never fixed — see dev-server.mjs's header for the concurrent-worktree collision that
// rule closes and for the second, independent "is this OUR vite" layer that backs it up.
const DEV_PORT = Number(process.env.HARNESS_DEV_PORT || harnessPortFor());

function formatProblems(r) {
  const lines = [];
  for (const m of r.overlaps) lines.push(`    OVERLAP        ${m}`);
  for (const m of r.clipBreaches) lines.push(`    CLIP-BREACH    ${m}`);
  for (const m of r.textOverflows) lines.push(`    TEXT-OVERFLOW  ${m}`);
  for (const m of r.unpainted) lines.push(`    UNREACHABLE    ${m}`);
  for (const m of r.missing) lines.push(`    MISSING        ${m}`);
  for (const m of r.boundsViolations ?? []) lines.push(`    BOUNDS         ${m}`);
  for (const m of r.offScreen ?? []) lines.push(`    OFF-SCREEN     ${m}`);
  for (const m of r.clickFailures ?? []) lines.push(`    CLICK-MISS     ${m}`);
  return lines;
}

// CPE-1883: `rectBoundsInfo`/`pseudoOnScreenInfo`/`clickReachesInfo` are recorded whether their check
// passed or failed — printed unconditionally (not gated on `problems.length > 0`) so a ticket's measured
// before/after numbers come straight out of this console output instead of a bespoke one-off script.
function formatRectInfo(r) {
  const lines = (r.rectBoundsInfo ?? []).map(
    (m) => `    rect  ${m.selector} width=${m.width}px height=${m.height}px`,
  );
  for (const m of r.pseudoOnScreenInfo ?? []) {
    lines.push(
      `    pos   ${m.selector} (${m.edge}:0) left=${m.left}px right=${m.right}px viewport=[0, ${m.innerWidth}]`,
    );
  }
  for (const m of r.clickReachesInfo ?? []) {
    lines.push(
      `    click ${m.selector} at (${m.x}, ${m.y}) -> clicked=${m.clicked} hit=${m.hitTag ?? "n/a"}`,
    );
  }
  return lines;
}

async function main() {
  console.log(`[layout-guard] chrome: ${CHROME}`);
  console.log("[layout-guard] starting vite dev server…");
  // `startDevServer` does not return until BOTH checks pass: this child announced binding this exact
  // port on its own stdout, and a real HTTP round-trip answered. Its header carries the spawn flags'
  // reasoning (`shell: true` for Windows EINVAL, `detached` for the POSIX process group) and the CI
  // hang that each of them fixes.
  const vite = await startDevServer(DEV_PORT);

  try {
    console.log(`[layout-guard] dev server up on :${DEV_PORT} (pid ${process.pid})`);

    const totalWidths = CASES.reduce((n, k) => n + k.widths.length, 0);
    console.log(`[layout-guard] sweeping ${CASES.length} case(s), ${totalWidths} width combination(s)…`);

    const results = await runAllCases({ cases: CASES, devServerBase: `http://localhost:${DEV_PORT}`, chromePath: CHROME });

    let failures = 0;
    for (const r of results) {
      const problems = formatProblems(r);
      const rectInfo = formatRectInfo(r);
      if (problems.length > 0) {
        failures++;
        console.error(`[layout-guard] ${r.case} @ ${r.width}px: FAIL`);
        for (const line of problems) console.error(line);
      } else {
        console.log(`[layout-guard] ${r.case} @ ${r.width}px: OK`);
      }
      for (const line of rectInfo) console.log(line);
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
    // Kills the whole process TREE, not just the direct child. dev-server.mjs's header records why
    // that distinction is the root cause of a real 14.7-minute CI hang rather than a tidiness point.
    stopDevServer(vite);
  }
}

main()
  .catch((e) => {
    console.error("[layout-guard] FAIL:", e);
    process.exitCode = 1;
  })
  .finally(() => {
    // CPE-1882 CI-round-4 fix, the real backstop: even with the process-GROUP kill above, do not trust
    // Node's natural "exit once the event loop drains" behaviour for a script that just spawned a
    // multi-process shell pipeline — a single missed handle (a stream that didn't get an EOF, a timer
    // that didn't clear) hangs the WHOLE CI job silently until its external timeout kills it, exactly
    // what happened on a real run. Exiting explicitly, with whatever `process.exitCode` was already set
    // to (0 if never touched), makes this script's own termination unconditional instead of a hope.
    process.exit(process.exitCode ?? 0);
  });
