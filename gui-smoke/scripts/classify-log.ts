// CPE-1728 — the I/O wrapper `npm run classify-log` invokes (mirrors `scripts/run-ratchet.ts`'s "pure
// lib module + thin CLI wrapper" split). Reads the suite step's captured stdout/stderr off disk and
// prints `lib/logSignature.ts`'s classification. This is the gui-smoke-linux job's step that answers
// "is this red about the app, or about the runner failing to paint?" without a human grepping the raw
// log by hand — see that module's header for the full rationale and PR #900 (CPE-1723) evidence.
//
// Deliberately ALWAYS exits 0: this step is advisory only, placed between the suite step and the
// Ratchet step in `gui-smoke.yml`, `if: always()` so it still runs (and still reports) on a
// timed-out/cancelled suite. It must never itself gate the job — that is the Ratchet step's job, and
// conflating the two would let a run's classification quietly change what's allowed to fail (the exact
// "hide the race" move CPE-1679 refused).
//
// Path is overridable via env var, same convention as GUI_SMOKE_RESULTS_DIR/GUI_SMOKE_KNOWN_FAILING in
// run-ratchet.ts:
//   GUI_SMOKE_SUITE_LOG   — path to the captured suite output (default "./.results/suite-output.log")
import fs from "node:fs";
import path from "node:path";
import { classifyLog, formatSignatureReport } from "../lib/logSignature.js";

const SUITE_LOG_PATH = process.env.GUI_SMOKE_SUITE_LOG ?? path.resolve(process.cwd(), ".results", "suite-output.log");

function main(): void {
  if (!fs.existsSync(SUITE_LOG_PATH)) {
    // Not an error: a run cancelled before the suite step ever started (e.g. a slow `npm ci`/`cargo
    // build` superseded mid-setup) never wrote this file, and that is already self-explanatory from the
    // job's own step list — nothing to classify.
    // eslint-disable-next-line no-console
    console.log(
      `[gui-smoke log-signature] no captured suite log at ${SUITE_LOG_PATH} — the suite step likely never ` +
        "started (cancelled during an earlier setup step). Nothing to classify.",
    );
    return;
  }

  const logText = fs.readFileSync(SUITE_LOG_PATH, "utf-8");
  const result = classifyLog(logText);
  for (const line of formatSignatureReport(result)) {
    // eslint-disable-next-line no-console
    console.log(line);
  }
}

main();
