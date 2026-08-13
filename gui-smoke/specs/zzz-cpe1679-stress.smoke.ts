// SCRATCH — CPE-1679 evidence gathering ONLY. Not part of the shipped suite; lives on the
// `cpe-1679-stress-experiment` branch and is deleted before the real fix PR is opened (see the
// ticket's PR body for the numbers this produced). Repeats the settle-check for the four
// `.mp-media` samples (audio/track.ogg, audio/track.mp3, audio/track.flac, video/clip.mp4) N times
// each in ONE shared app session (reusing openSampleFile/waitForPreviewToSettle exactly as
// samples.smoke.ts does), catches failures without throwing, and prints one greppable summary line
// per attempt PLUS a final tally — real ubuntu-latest + Xvfb + WebKitGTK, not a local guess.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { browser } from "@wdio/globals";
import { openSampleFile, assertAppStillAlive } from "../lib/samplesNav.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
const SAMPLES_DIR_NAME = "CPE-1358-samples";
const ITERATIONS = Number(process.env.CPE1679_ITERATIONS ?? 20);

const FILES: Array<{ dir: string; name: string }> = [
  { dir: "audio", name: "track.ogg" },
  { dir: "audio", name: "track.mp3" },
  { dir: "audio", name: "track.flac" },
  { dir: "video", name: "clip.mp4" },
];

describe("CPE-1679 SCRATCH — media settle-check stress", () => {
  let samplesRootAbs = "";

  before(() => {
    const state = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir?: string };
    if (!state.tmpDir) throw new Error("expected STATE_FILE to carry the seeded tmpDir");
    samplesRootAbs = path.join(state.tmpDir, SAMPLES_DIR_NAME);
  });

  it(`opens each media sample ${ITERATIONS} times and tallies pass/fail`, async function () {
    this.timeout(ITERATIONS * FILES.length * 25_000 + 60_000);
    const tally: Record<string, { pass: number; fail: number; fallback: number; aborted: number }> = {};
    let sessionDead = false;

    outer: for (const f of FILES) {
      const key = `${f.dir}/${f.name}`;
      tally[key] = { pass: 0, fail: 0, fallback: 0, aborted: 0 };
      const dirAbs = path.join(samplesRootAbs, f.dir);

      for (let i = 1; i <= ITERATIONS; i++) {
        if (sessionDead) {
          tally[key].aborted++;
          continue;
        }
        try {
          await openSampleFile(dirAbs, f.name, { timeoutMs: 20_000 });
          // Distinguish "real content settled" from "graceful-fallback settled" — both currently
          // COUNT as pass once the CPE-1679 fix lands, but we want the split for the report.
          const fallbackVisible = (await browser.$$(".mp-fallback").length) > 0;
          if (fallbackVisible) tally[key].fallback++;
          tally[key].pass++;
          await assertAppStillAlive(`stress-pass ${key} #${i}`);
          console.log(`CPE-1679-STRESS,${key},${i},PASS,${fallbackVisible ? "fallback" : "media"}`);
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          // A dead WebDriver session ("sessionId is required for this command" / "invalid session id")
          // is NOT a settle-check failure — it means the whole webview died and every remaining
          // attempt would just be recording the same non-signal. Stop hammering it and mark the rest
          // ABORTED rather than polluting the tally with fake failures (this happened on an earlier,
          // tighter-loop version of this harness with no pause between attempts — see PR body).
          if (/session ?id/i.test(msg) && /required|invalid|not found/i.test(msg)) {
            sessionDead = true;
            tally[key].aborted++;
            console.log(`CPE-1679-STRESS,${key},${i},SESSION-DEAD,${msg.replace(/[\r\n,]+/g, " ").slice(0, 200)}`);
            continue outer;
          }
          tally[key].fail++;
          console.log(`CPE-1679-STRESS,${key},${i},FAIL,${msg.replace(/[\r\n,]+/g, " ").slice(0, 200)}`);
        }
        // Give GStreamer/WebKitGTK a beat to tear down the outgoing <audio>/<video> pipeline before the
        // next `{#key entry.path}` remount — a tight back-to-back loop with NO pause killed the whole
        // WebDriver session outright on an earlier run of this harness (session death after ~5 rapid
        // attempts), which is a different failure mode than the 20s settle-timeout this ticket is about.
        await browser.pause(1_500);
      }
    }

    for (const [key, t] of Object.entries(tally)) {
      console.log(
        `CPE-1679-STRESS-SUMMARY,${key},pass=${t.pass},fail=${t.fail},fallback=${t.fallback},aborted=${t.aborted},total=${t.pass + t.fail}`,
      );
    }
  });
});
