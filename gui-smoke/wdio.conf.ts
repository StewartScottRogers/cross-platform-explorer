// wdio.conf.ts — CPE-1045 headless GUI smoke test harness.
//
// Drives the REAL built cross-platform-explorer.exe through tauri-driver + WebdriverIO and asserts
// `--open <dir>` (CPE-1043, fixed by CPE-1044) actually navigated the explorer into that folder. This
// file is deliberately isolated in gui-smoke/ with its own package.json/lockfile/tsconfig — it never
// touches the app's package.json, src-tauri, or the root vitest suite.
//
// Prereqs (see README.md): `cargo install tauri-driver --locked`, a matching msedgedriver on PATH
// (Windows), and — CRITICAL — a REAL release build via the Tauri CLI's `build` subcommand:
//     npm run build && npm run tauri build -- --no-bundle
//
// --- Foreground disruption on an interactive machine (see README.md "Running this locally") -----
// This launches a REAL, visible, focus-stealing window — there is currently no way to avoid that.
// `--x`/`--y` off-screen placement was tried and rejected: this app's own CPE-600 geometry code
// deliberately clamps the window fully onto the monitor ("off-screen protection... never
// ungrabbable", crates/server/src/geometry.rs `resolve()`), so an off-screen `--x`/`--y` is
// silently pulled back on-screen — it is not a usable escape hatch, not a bug to route around here.
// A real fix (a non-activating/off-screen/`--test-mode` launch) is tracked as a CPE-1046 follow-up
// on the app side; until that lands, do not run this suite on a machine someone is actively using —
// CI (windows-latest, no interactive user) is the intended place to run it.
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// --- CPE-1044 guard --------------------------------------------------------------------------
// Tauri decides "embed frontendDist" vs "load devUrl" (localhost:1420) at compile time based on
// whether the invocation went through the Tauri CLI's `build` subcommand — NOT on the cargo
// profile and NOT on `--no-bundle`. A raw `cargo build` (bypassing the Tauri CLI entirely) falls
// back to the dev server, which is exactly the CPE-1044 bug (`--open` landed on Home because the
// binary never had the real frontend embedded). So this harness MUST point at a binary produced
// by `npm run tauri build` (the CLI's build subcommand) — never a bare `cargo build` output.
//
// We build with `--no-bundle` in CI to skip installer packaging + updater-artifact signing (which
// requires TAURI_SIGNING_PRIVATE_KEY secrets this job intentionally does not carry) — that flag
// only skips *packaging*, it does not change the dev-vs-build cfg decision above, so the compiled
// exe still embeds the real frontend. `--debug`, on the other hand, is NOT used here: keep this
// pointed at a genuine release-profile CLI build so the smoke test matches what ships.
// CPE-1171: this was already OS-generic before the Linux CI leg existed — audited, not changed.
// `--no-bundle` produces the same unpackaged `src-tauri/target/release/<name>[.exe]` binary on every
// OS, so the only per-OS bit is the `.exe` suffix below.
const APP_BINARY = path.resolve(
  __dirname,
  "..",
  "src-tauri",
  "target",
  "release",
  process.platform === "win32" ? "cross-platform-explorer.exe" : "cross-platform-explorer",
);

if (!fs.existsSync(APP_BINARY)) {
  throw new Error(
    `[gui-smoke] release binary not found at:\n  ${APP_BINARY}\n\n` +
      "Build it first with a REAL Tauri CLI build (never a bare `cargo build`, and never `--debug` —\n" +
      "see the CPE-1044 comment above):\n" +
      "  npm run build && npm run tauri build -- --no-bundle\n",
  );
}

// CPE-1171: also already OS-generic — `tauri-driver` itself is what picks the NATIVE driver per OS
// (`which::which("msedgedriver.exe")` on Windows vs `which::which("WebKitWebDriver")` on Linux, both
// via a plain PATH lookup — no `--native-driver` override needed as long as CI installs the matching
// driver package, which gui-smoke.yml's two jobs each do). Nothing here has to branch on that.
const TAURI_DRIVER_BIN = path.resolve(
  os.homedir(),
  ".cargo",
  "bin",
  process.platform === "win32" ? "tauri-driver.exe" : "tauri-driver",
);

// The temp dir + marker file (read by specs/open-dir.smoke.ts) are seeded once, in the main
// process, before any worker/session starts — and handed off via a small state file rather than
// relying on env-var inheritance into the (possibly forked) worker process.
export const STATE_FILE = path.resolve(__dirname, ".smoke-state.json");
export const MARKER_NAME = "CPE-1045-marker.txt";

// CPE-1096: a tiny, known source file seeded into the same temp dir so the smoke suite can select
// it in the file list and assert the code-preview's code-intelligence UI (outline strip + per-line
// rows + minimap, CPE-1090/1091) actually renders — burning down the manual visual-verification
// debt those two tickets shipped with (headless-only coverage until now). Deliberately small but
// non-trivial: a struct + two functions (>=1 outline pill each) with multi-line bodies (foldable
// blocks), so `code_intel::analyze` (crates/server/src/code_intel.rs) populates outline, folds, and
// minimap — the minimap in particular is populated for ANY non-empty text (see
// `minimap::minimap_rows`), so asserting on it is safe, not just best-effort.
export const FIXTURE_NAME = "CPE-1096-fixture.rs";
const FIXTURE_SOURCE = `// CPE-1096 gui-smoke fixture — exercises the code-preview outline/rows/minimap.
pub struct Widget {
    pub name: String,
}

pub fn greet(name: &str) -> String {
    let msg = format!("Hello, {name}!");
    msg
}

pub fn describe(widget: &Widget) -> String {
    if widget.name.is_empty() {
        "unnamed".to_string()
    } else {
        widget.name.clone()
    }
}
`;

// --- CPE-1143: auto-organize preview fixture --------------------------------------------------
// A few extra mixed-kind files dropped straight into the seeded tmpDir (alongside MARKER_NAME/
// FIXTURE_NAME above) so `organize.smoke.ts`'s grouped preview (OrganizeDialog.svelte, driven by
// the real `organize_plan` command) has real variety to group — MARKER_NAME (.txt) and
// FIXTURE_NAME (.rs) alone only cover two of `organize.rs`'s `kind_category` buckets
// (Documents/Code); these add Images/Archives/Audio so a by-kind OR by-extension preview always
// renders more than one group. Content is throwaway (never opened/parsed — `organize_plan` only
// reads name/extension/size/mtime metadata), and — unlike the history/replay fixtures above, which
// live in the REAL shared app-data directory — these files live entirely inside the disposable
// tmpDir `onComplete` already `rm -rf`s, so no separate backup/restore pair is needed here.
export const ORGANIZE_PNG_NAME = "CPE-1143-photo.png";
export const ORGANIZE_ZIP_NAME = "CPE-1143-archive.zip";
export const ORGANIZE_MP3_NAME = "CPE-1143-song.mp3";

function seedOrganizeFixture(tmpDir: string): void {
  fs.writeFileSync(path.join(tmpDir, ORGANIZE_PNG_NAME), "not a real png, just bytes\n", "utf-8");
  fs.writeFileSync(path.join(tmpDir, ORGANIZE_ZIP_NAME), "not a real zip, just bytes\n", "utf-8");
  fs.writeFileSync(path.join(tmpDir, ORGANIZE_MP3_NAME), "not a real mp3, just bytes\n", "utf-8");
}

// --- CPE-1144: Batch-Media dialog fixture -------------------------------------------------------
// Batch-media plans IMAGE transforms (BatchMediaDialog.svelte, crates/server/src/batch_media.rs),
// so — unlike CPE-1143's organize fixture just above, which only needs the right *extension*
// (`organize_plan` reads name/extension/size/mtime metadata and never opens the file) — this fixture
// writes genuinely decodable PNGs. `batch_media::plan` (the command backing the live preview) is
// itself pure path-string manipulation and never opens the file either, so a fake-bytes `.png` would
// already render a preview today; these are still real, valid PNGs so the fixture stays honest with
// what a real batch-media flow encounters and keeps working if `plan` ever starts inspecting bytes.
//
// Hand-rolled rather than pulled in as a dependency: a minimal single-IDAT truecolour (colour type 2)
// PNG, built + CRC32'd here with the exact algorithm the PNG spec mandates (ISO 3309 / the same table
// zlib/gzip use), and structurally self-checked (crc recompute + zlib inflate round-trip on every
// chunk, IHDR field readback) before ever being wired in here.
const PNG_CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function pngCrc32(buf: Buffer): number {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = PNG_CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type: string, data: Buffer): Buffer {
  const typeBuf = Buffer.from(type, "ascii");
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(pngCrc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([len, typeBuf, data, crc]);
}

/** A tiny, valid, decodable truecolour PNG — no external deps, just `node:zlib`'s deflate. */
function tinyPng(width: number, height: number, rgb: [number, number, number]): Buffer {
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdrData = Buffer.alloc(13);
  ihdrData.writeUInt32BE(width, 0);
  ihdrData.writeUInt32BE(height, 4);
  ihdrData[8] = 8; // bit depth
  ihdrData[9] = 2; // colour type: truecolour (RGB)
  ihdrData[10] = 0; // compression method
  ihdrData[11] = 0; // filter method
  ihdrData[12] = 0; // interlace method
  const rowBytes = 1 + width * 3;
  const raw = Buffer.alloc(rowBytes * height);
  for (let y = 0; y < height; y++) {
    const rowStart = y * rowBytes;
    raw[rowStart] = 0; // per-scanline filter type: None
    for (let x = 0; x < width; x++) {
      const px = rowStart + 1 + x * 3;
      raw[px] = rgb[0];
      raw[px + 1] = rgb[1];
      raw[px + 2] = rgb[2];
    }
  }
  return Buffer.concat([
    sig,
    pngChunk("IHDR", ihdrData),
    pngChunk("IDAT", zlib.deflateSync(raw)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

export const BATCH_MEDIA_PNG_A_NAME = "CPE-1144-photo-a.png";
export const BATCH_MEDIA_PNG_B_NAME = "CPE-1144-photo-b.png";

function seedBatchMediaFixture(tmpDir: string): void {
  fs.writeFileSync(path.join(tmpDir, BATCH_MEDIA_PNG_A_NAME), tinyPng(4, 4, [200, 60, 60]));
  fs.writeFileSync(path.join(tmpDir, BATCH_MEDIA_PNG_B_NAME), tinyPng(4, 4, [60, 120, 200]));
}

// --- CPE-1203 / CPE-1205: similar-images fixture ------------------------------------------------
// Seed two near-duplicate images for the Find-similar-images dialog (SimilarImagesDialog.svelte,
// CPE-1202). The backend (crates/server/src/image_similarity.rs) reduces each image to a perceptual
// dHash and clusters by Hamming distance, then EXCLUDES featureless images (CPE-1205: a hash whose
// popcount is near 0 or 64 carries no discriminative structure — solid fills AND smooth monotonic
// gradients both collapse to an all-zero hash and would false-cluster). So the fixture must be a
// genuinely *structured* image: a **multi-band** pattern (alternating black/white vertical bands) has
// sharp column transitions that hash near half-set (popcount ~32), well inside the retained band. The
// same pattern at two different sizes is a genuine near-duplicate pair (the resize preserves the
// column structure), while being far (in Hamming bits) from the flat batch-media solids and unaffected
// by the featureless guard, so the two seeded bands form their OWN group of exactly two. NOTE: a
// gradient is NOT usable here anymore — it hashes all-zero and is dropped by the guard.
export const SIMILAR_PNG_A_NAME = "CPE-1203-scene-a.png";
export const SIMILAR_PNG_B_NAME = "CPE-1203-scene-b.png";

/** A structured `width×height` image of `blocks` alternating black/white VERTICAL bands — a valid
 *  truecolour PNG built with the same chunk/CRC/deflate machinery as {@link tinyPng}. The sharp column
 *  transitions give a dHash near half-set bits (structured, survives the CPE-1205 featureless guard);
 *  two sizes of it are perceptual near-duplicates under dHash. `width` should be a multiple of `blocks`
 *  so each band is a whole number of pixels wide. */
function bandedPng(width: number, height: number, blocks = 9): Buffer {
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdrData = Buffer.alloc(13);
  ihdrData.writeUInt32BE(width, 0);
  ihdrData.writeUInt32BE(height, 4);
  ihdrData[8] = 8; // bit depth
  ihdrData[9] = 2; // colour type: truecolour (RGB)
  const blockPx = Math.max(1, Math.floor(width / blocks));
  const rowBytes = 1 + width * 3;
  const raw = Buffer.alloc(rowBytes * height);
  for (let y = 0; y < height; y++) {
    const rowStart = y * rowBytes;
    raw[rowStart] = 0; // per-scanline filter type: None
    for (let x = 0; x < width; x++) {
      const v = Math.floor(x / blockPx) % 2 === 0 ? 0 : 255; // alternating black/white bands
      const px = rowStart + 1 + x * 3;
      raw[px] = v;
      raw[px + 1] = v;
      raw[px + 2] = v;
    }
  }
  return Buffer.concat([
    sig,
    pngChunk("IHDR", ihdrData),
    pngChunk("IDAT", zlib.deflateSync(raw)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

function seedSimilarImagesFixture(tmpDir: string): void {
  // Same 9-band vertical pattern at two sizes → a structured near-duplicate pair that survives the
  // CPE-1205 featureless guard (widths are multiples of 9 so bands are whole pixels).
  fs.writeFileSync(path.join(tmpDir, SIMILAR_PNG_A_NAME), bandedPng(216, 160));
  fs.writeFileSync(path.join(tmpDir, SIMILAR_PNG_B_NAME), bandedPng(108, 80));
}

// --- CPE-1221: near-duplicate DOCUMENTS fixture -------------------------------------------------
// Seed two near-identical .md files (plus one unrelated) for the Find-similar-documents dialog
// (NearDuplicatesDialog.svelte, CPE-1204). The backend (crates/server/src/document_similarity.rs)
// SimHashes each candidate's text and single-link-clusters at Hamming distance <= 8
// (`DEFAULT_MAX_DISTANCE`). This fixture follows the same shape as `simhash.rs`'s own
// `near_identical_text_is_closer_than_unrelated_text` test (base paragraph + lightly-edited near-copy +
// unrelated paragraph). Measured directly against these exact strings: the near pair (one word changed
// + a short sentence appended) lands at Hamming distance 3 — well inside the threshold — and the
// unrelated paragraph lands at 15 — well outside it. So the near pair provably clusters and the third
// doc stays a singleton; the distances above are measured against these fixture strings, which are not
// byte-identical to the Rust test's constants.
export const NEAR_DUP_DOC_A_NAME = "CPE-1221-notes-a.md";
export const NEAR_DUP_DOC_B_NAME = "CPE-1221-notes-b.md";
export const NEAR_DUP_DOC_UNRELATED_NAME = "CPE-1221-unrelated.md";

const NEAR_DUP_PARAGRAPH_A =
  "The quick brown fox jumps over the lazy dog near the riverbank. " +
  "It happens every morning just after sunrise, when the grass is still wet with dew. " +
  "Local farmers say the fox has been doing this for several years without fail.";

// Same paragraph as A with ONE word changed ("morning" -> "afternoon") and one short sentence
// appended — a realistic "lightly edited" near-duplicate, matching simhash.rs's own fixture.
const NEAR_DUP_PARAGRAPH_A_EDITED =
  "The quick brown fox jumps over the lazy dog near the riverbank. " +
  "It happens every afternoon just after sunrise, when the grass is still wet with dew. " +
  "Local farmers say the fox has been doing this for several years without fail. " +
  "A neighbor recently started photographing it.";

const NEAR_DUP_PARAGRAPH_UNRELATED =
  "Quarterly revenue increased twelve percent, driven mainly by strong demand in the northern sales " +
  "region and a new pricing structure for enterprise customers. The finance team expects margins to " +
  "hold steady into the next fiscal year.";

function seedNearDupDocsFixture(tmpDir: string): void {
  fs.writeFileSync(path.join(tmpDir, NEAR_DUP_DOC_A_NAME), NEAR_DUP_PARAGRAPH_A, "utf-8");
  fs.writeFileSync(path.join(tmpDir, NEAR_DUP_DOC_B_NAME), NEAR_DUP_PARAGRAPH_A_EDITED, "utf-8");
  fs.writeFileSync(path.join(tmpDir, NEAR_DUP_DOC_UNRELATED_NAME), NEAR_DUP_PARAGRAPH_UNRELATED, "utf-8");
}

// --- CPE-1130: cost-History fixture -----------------------------------------------------------
// Seed a synthetic session-metrics journal (`history.jsonl`) into the REAL app-data directory this
// exact build reads from, so the Agent Watch drawer's cost-History tab (AgentTimeline.svelte,
// CPE-1114 — `.hd-bar`/`.hd-*` elements) has rows to roll up and render when the smoke suite opens
// it. This burns down the manual-verification-debt row for that view (MANUAL-TEST-BURNDOWN.md).
//
// The on-disk shape is CPE-1113's journal (crates/server/src/metrics_journal.rs): one
// `SessionMetricsRecord` JSON object per line, camelCase, at
// `<app-data-dir>/agent-metrics/history.jsonl`, where `app-data-dir` is Tauri's own
// `app_data_dir()` resolution — `<OS app-data root>/<bundle identifier>`
// (src-tauri/src/server_ctx.rs -> `app.path().app_data_dir()`). The identifier is read straight out
// of tauri.conf.json rather than hardcoded so a future rename can't silently break this fixture.
const TAURI_CONF = JSON.parse(
  fs.readFileSync(path.resolve(__dirname, "..", "src-tauri", "tauri.conf.json"), "utf-8"),
) as { identifier: string };

/** `<OS app-data root>/<bundle identifier>`, matching Tauri's `app_data_dir()`. CPE-1171 added a
 *  `ubuntu-latest` CI leg alongside the original `windows-latest` one (see gui-smoke.yml's header
 *  comment), so both the win32 (`APPDATA`) and linux (`XDG_DATA_HOME`) branches are now exercised by
 *  CI; the darwin branch remains best-effort for local runs only — macOS has no `tauri-driver`
 *  support (no WKWebView WebDriver) and stays attended. */
function resolveAppDataDir(): string {
  if (process.platform === "win32") {
    const appData = process.env.APPDATA ?? path.join(os.homedir(), "AppData", "Roaming");
    return path.join(appData, TAURI_CONF.identifier);
  }
  if (process.platform === "darwin") {
    return path.join(os.homedir(), "Library", "Application Support", TAURI_CONF.identifier);
  }
  const xdgData = process.env.XDG_DATA_HOME ?? path.join(os.homedir(), ".local", "share");
  return path.join(xdgData, TAURI_CONF.identifier);
}

/** One JSONL line matching `SessionMetricsRecord`'s camelCase wire shape — the exact fields
 *  `metrics_history` deserializes (see the `camelcase_wire_shape_matches_the_frontend` Rust test in
 *  metrics_journal.rs, which this fixture is deliberately shaped to match). */
function fixtureRecord(opts: {
  sessionId: string;
  agentId: string;
  agentName: string;
  model: string;
  daysAgo: number;
  costUsd: number;
  totalTokens: number;
}): string {
  const started = Date.now() - opts.daysAgo * 24 * 60 * 60 * 1000;
  const ended = started + 5 * 60 * 1000; // a nominal 5-minute session
  const inputTokens = Math.round(opts.totalTokens * 0.6);
  const outputTokens = opts.totalTokens - inputTokens;
  return JSON.stringify({
    sessionId: opts.sessionId,
    agentId: opts.agentId,
    agentName: opts.agentName,
    provider: "openrouter",
    model: opts.model,
    cwd: "/gui-smoke/cpe-1130-fixture",
    startedAt: started,
    endedAt: ended,
    wallClockMs: ended - started,
    inputTokens,
    outputTokens,
    totalTokens: opts.totalTokens,
    costUsd: opts.costUsd,
    filesTouched: 3,
    churnBytes: 2048,
    editCount: 5,
  });
}

/** Three rows across three days and two agents/models — enough for the over-time bars (>=1
 *  `.hd-bar`), the by-model table, and the by-agent table to all have real, non-empty content.
 *  Exported so the falsifiability check (README "Falsification evidence") can point at the same
 *  generator when constructing the deliberately-EMPTY variant. */
export function historyFixtureLines(): string[] {
  return [
    fixtureRecord({
      sessionId: "gui-smoke-hist-1",
      agentId: "claude",
      agentName: "Claude Code",
      model: "sonnet",
      daysAgo: 2,
      costUsd: 1.23,
      totalTokens: 4200,
    }),
    fixtureRecord({
      sessionId: "gui-smoke-hist-2",
      agentId: "claude",
      agentName: "Claude Code",
      model: "opus",
      daysAgo: 1,
      costUsd: 0.45,
      totalTokens: 1500,
    }),
    fixtureRecord({
      sessionId: "gui-smoke-hist-3",
      agentId: "copilot",
      agentName: "Copilot",
      model: "gpt-5",
      daysAgo: 0,
      costUsd: 0.08,
      totalTokens: 600,
    }),
  ];
}

/** Backup of whatever was at the fixture path before we overwrote it (or `null` if nothing was
 *  there), so a LOCAL run never permanently clobbers a real developer's own Agent Watch history —
 *  restored in `onComplete` below, mirroring the tmpDir cleanup pattern. On CI's ephemeral runner
 *  this is always `null` (ephemeral) — the restore there just deletes the fixture file we made. */
let historyFixtureBackup: { file: string; original: string | null } | null = null;

function seedHistoryFixture(): void {
  const dir = path.join(resolveAppDataDir(), "agent-metrics");
  fs.mkdirSync(dir, { recursive: true });
  const file = path.join(dir, "history.jsonl");
  historyFixtureBackup = { file, original: fs.existsSync(file) ? fs.readFileSync(file, "utf-8") : null };
  fs.writeFileSync(file, historyFixtureLines().join("\n") + "\n", "utf-8");
}

function restoreHistoryFixture(): void {
  if (!historyFixtureBackup) return;
  const { file, original } = historyFixtureBackup;
  if (original === null) {
    fs.rmSync(file, { force: true });
  } else {
    fs.writeFileSync(file, original, "utf-8");
  }
  historyFixtureBackup = null;
}

// --- CPE-1135: Replay-scrubber fixture ----------------------------------------------------------
// Seed the on-disk audit journal + baseline for a FIXED synthetic session id into the REAL app-data
// dir this build reads from, so the Agent Watch drawer's Replay tab (AgentTimeline.svelte, CPE-1094 —
// `replay_load`, CPE-1110) has something to load when the smoke suite opens it. Mirrors the CPE-1130
// history-fixture discipline above: seed in `onPrepare`, restore/delete in `onComplete`.
//
// On-disk shapes (discovered from the backend, not invented — see crates/server/src/audit_journal.rs
// and replay_baseline.rs, and their round-trip tests):
//   - Journal: `<app-data-dir>/audit/<session>.jsonl` (`audit_journal::session_file`, called from
//     `audit_dir()` in src-tauri/src/lib.rs), one `AuditEvent` JSON object per line. `AuditEvent` has
//     NO serde rename — the wire field names ARE the Rust field names: `ts`, `session`, `kind`,
//     `path`, and optional `actor`/`detail` (both `#[serde(default, skip_serializing_if =
//     "Option::is_none")]`, so an old/minimal line without them still deserializes — the
//     `actor_round_trips_and_old_lines_without_actor_read_as_none` test constructs exactly this).
//   - Baseline: `<app-data-dir>/audit/<session>.baseline.json` (`replay_baseline::baseline_file`,
//     same `audit_dir()` base — a DISTINCT extension so `audit_journal::list_sessions`'s `.jsonl`
//     filter never picks it up). Shape: `{ paths: string[], truncated: bool }` (`Baseline`, no rename).
//
// Paths are rooted at the SAME tmpDir `--open=<dir>` already navigated into (onPrepare below), so the
// reconstructed listing (`AgentTimeline.svelte`'s `.rp-recon-list`, fed by
// `replayFold.ts#childrenAt(state, currentPath)`) has real content to show, not just the empty state —
// `currentPath` is that exact tmpDir string once navigation lands (open-dir.smoke.ts already proves
// the breadcrumb/listing reflect it verbatim).
export const REPLAY_SESSION_ID = "gui-smoke-replay";
export const REPLAY_BASELINE_NAME = "CPE-1135-existing.txt";
export const REPLAY_CREATED_NAME = "CPE-1135-created.txt";

let replayFixtureBackup: {
  journalFile: string;
  journalOriginal: string | null;
  baselineFile: string;
  baselineOriginal: string | null;
} | null = null;

function seedReplayFixture(tmpDir: string): void {
  const dir = path.join(resolveAppDataDir(), "audit");
  fs.mkdirSync(dir, { recursive: true });
  const journalFile = path.join(dir, `${REPLAY_SESSION_ID}.jsonl`);
  const baselineFile = path.join(dir, `${REPLAY_SESSION_ID}.baseline.json`);
  replayFixtureBackup = {
    journalFile,
    journalOriginal: fs.existsSync(journalFile) ? fs.readFileSync(journalFile, "utf-8") : null,
    baselineFile,
    baselineOriginal: fs.existsSync(baselineFile) ? fs.readFileSync(baselineFile, "utf-8") : null,
  };

  // Baseline: one pre-existing file under tmpDir, exactly as `replay_baseline::capture` would record
  // it at watch-start (a flat list of absolute paths; this fixture needs no nested directories).
  const baseline = { paths: [path.join(tmpDir, REPLAY_BASELINE_NAME)], truncated: false };
  fs.writeFileSync(baselineFile, JSON.stringify(baseline), "utf-8");

  // Journal: two events for the same path, 30s apart, so the fold has something to apply on top of
  // the baseline (a create then a modify) — mirrors `replay_session.rs`'s own
  // `loads_events_in_order_with_bounds_and_summary` fixture shape.
  const started = Date.now() - 60_000;
  const createdPath = path.join(tmpDir, REPLAY_CREATED_NAME);
  const events = [
    { ts: started, session: REPLAY_SESSION_ID, kind: "created", path: createdPath },
    { ts: started + 30_000, session: REPLAY_SESSION_ID, kind: "modified", path: createdPath },
  ];
  fs.writeFileSync(journalFile, events.map((e) => JSON.stringify(e)).join("\n") + "\n", "utf-8");
}

function restoreReplayFixture(): void {
  if (!replayFixtureBackup) return;
  const { journalFile, journalOriginal, baselineFile, baselineOriginal } = replayFixtureBackup;
  if (journalOriginal === null) fs.rmSync(journalFile, { force: true });
  else fs.writeFileSync(journalFile, journalOriginal, "utf-8");
  if (baselineOriginal === null) fs.rmSync(baselineFile, { force: true });
  else fs.writeFileSync(baselineFile, baselineOriginal, "utf-8");
  replayFixtureBackup = null;
}

// --- CPE-1154: empty-folder fixture -----------------------------------------------------------
// An EMPTY subdirectory of the seeded tmpDir, so context-menu.smoke.ts can navigate into it and
// exercise the confirmed native-menu-leak repro: right-clicking the BLANK pane area of a folder whose
// `.empty-state` box is centred and does NOT fill the pane. Just an empty dir — no files inside — so
// the explorer renders its empty state, and nothing here perturbs the other specs' file fixtures.
export const EMPTY_DIR_NAME = "CPE-1154-empty-folder";

function seedEmptyFolderFixture(tmpDir: string): void {
  fs.mkdirSync(path.join(tmpDir, EMPTY_DIR_NAME), { recursive: true });
}

// --- CPE-1207: New-Link dialog fixture ----------------------------------------------------------
// A DEDICATED empty subdirectory the New-Link click-through creates its hardlink inside, kept
// separate from CPE-1154's `EMPTY_DIR_NAME` so this spec's write (the created hardlink file) never
// perturbs context-menu.smoke.ts's "stays empty" assumption for that folder. Blank pane, same
// right-click reachability as CPE-1154's repro.
export const NEW_LINK_DIR_NAME = "CPE-1207-new-link-folder";

function seedNewLinkFixture(tmpDir: string): void {
  fs.mkdirSync(path.join(tmpDir, NEW_LINK_DIR_NAME), { recursive: true });
}

// --- CPE-1181: archive-browse (.tar.gz) fixture -------------------------------------------------
// Extends the already-working in-app ZIP browsing (CPE-242/1179) to tar/tar.gz/tgz/7z/iso —
// `read_archive_entries` (crates/server/src/archive.rs) is already format-agnostic across all of
// those, so a hand-built, genuinely valid `.tar.gz` is enough to exercise the frontend's new
// `ARCHIVE_EXTS` entry + `enterArchive`/`archiveChildren` (App.svelte) for a non-zip format.
//
// No external `tar` dependency: a minimal single-entry USTAR header (100-byte name, octal
// mode/uid/gid/size/mtime fields, the standard "sum bytes with the checksum field blanked to
// spaces" checksum algorithm, "ustar\0"/"00" magic/version) followed by the (zero-padded to a
// 512-byte boundary) file data and two 512-byte zero end-of-archive blocks — the same shape the
// backend's own `tar::Builder`-based tests (`extract_archive_unpacks_a_tar_gz`,
// `read_archive_entries_lists_tar_contents` in archive.rs) construct, just written by hand here
// since gui-smoke has no Rust access. `zlib.gzipSync` (real gzip, not raw deflate) wraps it, so the
// backend's gzip-vs-plain-tar dispatch (by trying gzip decode first) sees a genuine gzip stream.
export const ARCHIVE_TARGZ_NAME = "CPE-1181-archive.tar.gz";
export const ARCHIVE_TARGZ_INNER_NAME = "CPE-1181-note.txt";
const ARCHIVE_TARGZ_INNER_CONTENT = "packed inside a tar.gz for CPE-1181\n";

function tarChecksumPlaceholder(): string {
  return "        "; // 8 ASCII spaces — the checksum field's value while the sum itself is computed
}

/** One 512-byte USTAR header block for a single regular-file entry. */
function tarHeader(name: string, size: number, mtimeSec: number): Buffer {
  const header = Buffer.alloc(512, 0);
  header.write(name, 0, "utf-8"); // name: 100 bytes
  header.write("0000644\0", 100, "utf-8"); // mode: 8 bytes
  header.write("0000000\0", 108, "utf-8"); // uid: 8 bytes
  header.write("0000000\0", 116, "utf-8"); // gid: 8 bytes
  header.write(`${size.toString(8).padStart(11, "0")}\0`, 124, "utf-8"); // size: 12 bytes (octal)
  header.write(`${mtimeSec.toString(8).padStart(11, "0")}\0`, 136, "utf-8"); // mtime: 12 bytes (octal)
  header.write(tarChecksumPlaceholder(), 148, "utf-8"); // chksum: 8 bytes, blanked for the sum below
  header[156] = 0x30; // typeflag '0' — regular file
  header.write("ustar\0", 257, "utf-8"); // magic: 6 bytes
  header.write("00", 263, "utf-8"); // version: 2 bytes

  let sum = 0;
  for (let i = 0; i < 512; i++) sum += header[i];
  header.write(`${sum.toString(8).padStart(6, "0")}\0 `, 148, "utf-8"); // final chksum: 6 octal digits + NUL + space
  return header;
}

/** A minimal, genuinely valid single-entry `.tar.gz` (gzip-wrapped USTAR). */
function buildSingleEntryTarGz(entryName: string, content: string): Buffer {
  const data = Buffer.from(content, "utf-8");
  const mtimeSec = Math.floor(Date.now() / 1000);
  const pad = (512 - (data.length % 512)) % 512;
  const tarBuf = Buffer.concat([
    tarHeader(entryName, data.length, mtimeSec),
    data,
    Buffer.alloc(pad, 0),
    Buffer.alloc(1024, 0), // two zero blocks mark end-of-archive
  ]);
  return zlib.gzipSync(tarBuf);
}

function seedArchiveBrowseFixture(tmpDir: string): void {
  const targz = buildSingleEntryTarGz(ARCHIVE_TARGZ_INNER_NAME, ARCHIVE_TARGZ_INNER_CONTENT);
  fs.writeFileSync(path.join(tmpDir, ARCHIVE_TARGZ_NAME), targz);
}

// --- CPE-1208: link-badge fixture (epic CPE-715) ------------------------------------------------
// An intact symlink + a broken symlink in the seeded tmpDir, so link-badge.smoke.ts can pin the
// FileList link badge + broken state (CPE-1208) against the REAL built app's `is_symlink` field
// (CPE-1206) and `link_status` command (CPE-804). Symlink creation is unprivileged on POSIX but needs
// either Developer Mode or an elevated process on Windows (`fs.symlinkSync` throws `EPERM` otherwise)
// — this is best-effort: on failure it leaves `linkBadgeFixture.supported: false` in STATE_FILE and
// the spec skips its assertions rather than failing the whole suite over a sandbox permission gap.
export const LINK_TARGET_NAME = "CPE-1208-target.txt";
export const LINK_GOOD_NAME = "CPE-1208-good-link.txt";
export const LINK_BROKEN_NAME = "CPE-1208-broken-link.txt";

function seedLinkBadgeFixture(tmpDir: string): boolean {
  const targetPath = path.join(tmpDir, LINK_TARGET_NAME);
  fs.writeFileSync(targetPath, "CPE-1208 link target\n", "utf-8");
  try {
    fs.symlinkSync(targetPath, path.join(tmpDir, LINK_GOOD_NAME), "file");
    // Points at a name that is never created — permanently broken, no race with the rest of onPrepare.
    fs.symlinkSync(path.join(tmpDir, "CPE-1208-does-not-exist.txt"), path.join(tmpDir, LINK_BROKEN_NAME), "file");
    return true;
  } catch {
    return false; // unprivileged symlink creation (e.g. Windows without Developer Mode/elevation)
  }
}

// --- CPE-1237: mixed-format thumbnail-gallery fixture (epic CPE-718) ----------------------------
// A dedicated subfolder (isolation, same reasoning as CPE-1207's link folder) carrying one file per
// format the streaming thumbnail client (`thumbnailClient.ts`, wired through `thumb_queue`/
// `thumb_cache`) is expected to render a REAL tile for, plus one it must gracefully fall back on:
//   - a genuinely decodable PNG (raster — the pre-CPE-1237 baseline format)
//   - a genuinely valid minimal SVG (CPE-1236's rasterizer — the new non-photo format)
//   - a byte-garbage `.ttf` (extension says "thumbnailable", content doesn't decode) — proves the
//     pipeline's failure path still shows the type icon instead of a broken tile or a crash
// A real font is deliberately NOT hand-built here (a byte-correct minimal sfnt is a lot of surface
// for a fixture); the fallback case is exactly as meaningful with garbage bytes, since `hasThumbnail`
// gates on extension alone — the point is the FRONTEND still asks for it and copes with the miss.
export const THUMB_GALLERY_DIR_NAME = "CPE-1237-thumbnail-gallery";
export const THUMB_GALLERY_PNG_NAME = "CPE-1237-photo.png";
export const THUMB_GALLERY_SVG_NAME = "CPE-1237-icon.svg";
export const THUMB_GALLERY_BADFONT_NAME = "CPE-1237-bad.ttf";
const THUMB_GALLERY_SVG_CONTENT =
  '<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40"><rect width="100" height="40" fill="#2b7"/></svg>';

function seedThumbnailGalleryFixture(tmpDir: string): void {
  const dir = path.join(tmpDir, THUMB_GALLERY_DIR_NAME);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, THUMB_GALLERY_PNG_NAME), tinyPng(24, 24, [80, 140, 220]));
  fs.writeFileSync(path.join(dir, THUMB_GALLERY_SVG_NAME), THUMB_GALLERY_SVG_CONTENT, "utf-8");
  fs.writeFileSync(path.join(dir, THUMB_GALLERY_BADFONT_NAME), "not a real font, just bytes\n", "utf-8");
}

// --- CPE-1268: pdf + video real-thumbnail regression pin (guards CPE-1267) ----------------------
// CPE-1267 was a silent drift: the frontend `hasThumbnail` gate (src/lib/filetypes.ts) fell behind
// the backend `thumb_source` dispatch, so the grid never even REQUESTED pdf/video thumbnails. The
// always-run backstop is the two-sided parity guard (filetypes.test.ts ↔ thumb_source.rs); THIS
// gui-smoke fixture is the end-to-end proof at the GUI: a real one-page PDF + a real tiny video, in
// their own isolated subfolder, that the Gallery view must render as REAL `.thumb-img` tiles.
//
// NOTE on native deps: pdf rendering needs pdfium and video needs ffmpeg. The smoke binary is an
// UNBUNDLED `--no-bundle` release build, so those resolve via the OS search path (PATH / system
// library), NOT from `bundle.resources`. ffmpeg is reliably on PATH in CI (the video-thumb install
// step) and on dev machines, so the video tile renders; pdfium is often absent, so the PDF tile's
// real-render assertion is best-effort (logged, never fails the run) while its tile MOUNT — the actual
// CPE-1267 regression surface — is asserted unconditionally. gui-smoke is non-blocking anyway (CPE-1048).
export const THUMB_FORMATS_DIR_NAME = "CPE-1268-pdf-video-thumbnails";
export const THUMB_FORMATS_PDF_NAME = "CPE-1268-doc.pdf";
export const THUMB_FORMATS_VIDEO_NAME = "CPE-1268-clip.mp4";

/** A strictly-valid one-page PDF (200×200) drawing a filled blue rectangle so the rendered first-page
 *  thumbnail has real, non-blank content. Built by hand with byte-accurate xref offsets — no PDF lib.
 *  `latin1` throughout so byte lengths equal string lengths for the offset/`/Length` maths. */
function minimalOnePagePdf(): Buffer {
  const objs: string[] = [];
  objs[1] = "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
  objs[2] = "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n";
  objs[3] =
    "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << >> >>\nendobj\n";
  const content = "0 0 1 rg\n20 20 160 160 re\nf\n";
  objs[4] = `4 0 obj\n<< /Length ${content.length} >>\nstream\n${content}endstream\nendobj\n`;

  let body = "%PDF-1.4\n";
  const offsets: number[] = [];
  for (let i = 1; i <= 4; i++) {
    offsets[i] = Buffer.byteLength(body, "latin1");
    body += objs[i];
  }
  const xrefStart = Buffer.byteLength(body, "latin1");
  let xref = "xref\n0 5\n0000000000 65535 f \n";
  for (let i = 1; i <= 4; i++) {
    xref += `${offsets[i].toString().padStart(10, "0")} 00000 n \n`;
  }
  const trailer = `trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n${xrefStart}\n%%EOF\n`;
  return Buffer.from(body + xref + trailer, "latin1");
}

/** Seeds the CPE-1268 pdf/video subfolder. The PDF is always written; the video is generated with the
 *  PATH `ffmpeg` (same binary the app resolves), and skipped — with a log — if ffmpeg isn't available
 *  at seed time (in which case the app couldn't render it either). Returns which fixtures were seeded so
 *  the spec knows what to assert. */
function seedThumbnailFormatsFixture(tmpDir: string): { pdf: boolean; video: boolean } {
  const dir = path.join(tmpDir, THUMB_FORMATS_DIR_NAME);
  fs.mkdirSync(dir, { recursive: true });

  fs.writeFileSync(path.join(dir, THUMB_FORMATS_PDF_NAME), minimalOnePagePdf());

  let video = false;
  const videoPath = path.join(dir, THUMB_FORMATS_VIDEO_NAME);
  try {
    const r = spawnSync(
      "ffmpeg",
      ["-y", "-f", "lavfi", "-i", "testsrc=duration=1:size=160x120:rate=5", "-pix_fmt", "yuv420p", videoPath],
      { stdio: "ignore" },
    );
    video = r.status === 0 && fs.existsSync(videoPath) && fs.statSync(videoPath).size > 0;
    if (!video) {
      // eslint-disable-next-line no-console
      console.warn("[gui-smoke] CPE-1268: ffmpeg unavailable/failed at seed — video tile will be skipped");
    }
  } catch {
    // eslint-disable-next-line no-console
    console.warn("[gui-smoke] CPE-1268: ffmpeg not on PATH at seed — video tile will be skipped");
  }
  return { pdf: true, video };
}

// --- CPE-1241: shred-dialog fixture (epic CPE-738) ----------------------------------------------
// A DEDICATED subfolder + throwaway file for shred-dialog.smoke.ts's render pin of
// `ShredConfirmDialog` (CPE-1240) — the one destructive surface in the app with NO trash fallback
// (`shred_paths` overwrites bytes then unlinks). The spec MUST NEVER actually click "Shred
// permanently" (that would irreversibly destroy the file), but isolating it in its own subfolder,
// separate from every other spec's fixtures, is belt-and-braces in case a future edit to that spec
// ever slips and does confirm — the blast radius stays this one throwaway file, and the whole tmpDir
// is `rm -rf`'d in `onComplete` regardless.
export const SHRED_DIR_NAME = "CPE-1241-shred-folder";
export const SHRED_FILE_NAME = "CPE-1241-shred-me.txt";

function seedShredFixture(tmpDir: string): void {
  const dir = path.join(tmpDir, SHRED_DIR_NAME);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, SHRED_FILE_NAME),
    "CPE-1241 throwaway gui-smoke fixture — the spec must never actually shred this file.\n",
    "utf-8",
  );
}

// --- CPE-1226: TransferPanel archive-op row fixture ---------------------------------------------
// A DEDICATED subfolder + throwaway source file (same isolation reasoning as CPE-1207/1241/1237
// above) so transfer-panel.smoke.ts can drive a REAL "Compress to ZIP" through the context menu and
// capture the operations/transfer panel's completed archive-op row (CPE-1184: archive icon + "N
// items compressed" wording) without perturbing — or being perturbed by — any other spec's fixtures.
// The name is deliberately deterministic so the spec can assert the exact created zip's filename:
// `doCompress` (App.svelte) names it after the source's stem (`compressBaseName`/`stripExt`), so
// TRANSFER_PANEL_SRC_NAME's ".txt" source yields a same-stem ".zip".
export const TRANSFER_PANEL_DIR_NAME = "CPE-1226-transfer-panel-folder";
export const TRANSFER_PANEL_SRC_NAME = "CPE-1226-compress-me.txt";

function seedTransferPanelFixture(tmpDir: string): void {
  const dir = path.join(tmpDir, TRANSFER_PANEL_DIR_NAME);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, TRANSFER_PANEL_SRC_NAME),
    "CPE-1226 gui-smoke fixture — compressed for real through the operations/transfer panel.\n",
    "utf-8",
  );
}

// --- CPE-1249: encrypted-vault mount/browse fixture (epic CPE-738) ------------------------------
// A DEDICATED subfolder + a genuinely-valid, pre-sealed `.cpevault` blob so vault.smoke.ts can drive
// the REAL unlock → browse → lock flow against a real vault (no mocked backend). The blob is the exact
// envelope + `age` passphrase format the production backend reads (`vault_crypto`), generated ONCE with
// a low scrypt work factor (fast to decrypt on CI) by `crates/server/examples/gen_vault_fixture.rs` —
// regenerate + repaste the base64 below with `cargo run -p cpe-server --example gen_vault_fixture`, and
// keep VAULT_FIXTURE_PASSPHRASE / the sealed inner names in sync with that example's constants.
//
// Sealed contents (the tree that appears once unlocked):
//   CPE-1249-inside.txt          "top secret vault contents"
//   notes/CPE-1249-hello.txt     "hello from inside the vault"
//
// Seeded as a ROOT-LEVEL FILE, deliberately NOT in its own subfolder: a new top-level FOLDER sorts
// before every file, shifting all file rows down one — which pushed fold-sensitive rows in sibling specs
// (archive-browse/archive-password's `CPE-1181-archive.tar.gz`, shred's file) below the visible fold at
// the default window height and broke their CDP clicks (CPE-1249 full-suite review). As a file named
// `CPE-1249-*` it sorts AFTER every other seeded file, so it shifts nothing and keeps the shared tmpDir's
// folder count identical to main. vault.smoke.ts scrolls it into view before activating it.
export const VAULT_FIXTURE_NAME = "CPE-1249-secret.cpevault";
export const VAULT_FIXTURE_PASSPHRASE = "open-sesame-1249";
export const VAULT_FIXTURE_INNER_NAME = "CPE-1249-inside.txt";
const VAULT_FIXTURE_BASE64 =
  "Q1BFVkxUMQEAYWdlLWVuY3J5cHRpb24ub3JnL3YxCi0+IHNjcnlwdCBWWGIraUdjN0RLTDJGRy9WVFVJNFpnIDQKdGxP" +
  "b05NSVlvMk5hL0ZxMXQxRGtkVFBOS09PM2tqYVpYbWQ4Umo4OU4vWQotLS0gY0ZlT20weGNxQkY2RHBpM3dvMHYrQ29Y" +
  "K1lLclgvd00xa2s0YVdVMThMSQr5iCEGTm7Pt0P89gommK/zoGcT/UV6AzZEDBWBcNUQYlBmB0K5K42wUhubaqySVPq" +
  "r+Mif/L3RENo9sGT8ELP7C2ggMRIh5NRbcCuzvTaWeDBpWMpfF0mS5rXvTSiUXNOToE9C9HLbBQEk1OuciMmHvKux53" +
  "WKFhklhFEAtoP+i7n2yyf3eurxiF/VpCQ2G7WAkyws5saRRM3U1nKzJ22tL9VLBfBtvj7Mp4jhbQ==";

function seedVaultFixture(tmpDir: string): void {
  fs.writeFileSync(path.join(tmpDir, VAULT_FIXTURE_NAME), Buffer.from(VAULT_FIXTURE_BASE64, "base64"));
}

// --- CPE-1250: vault-CREATE source fixture (epic CPE-738) ---------------------------------------
// A small plaintext tree the "Create encrypted vault…" folder action (VaultCreateDialog.svelte) seals
// into a `.cpevault`. Seeded as a subfolder INSIDE the CPE-1241 shred fixture folder (same epic,
// CPE-738) — deliberately NOT as a new TOP-LEVEL folder: a new top-level folder sorts before every file
// and shifts the root file rows down one, the exact fold regression CPE-1249 hit (archive-browse/
// -password's root `.tar.gz` row dropping below the visible fold, breaking their non-scrolling CDP
// clicks). Nesting it keeps the tmpDir ROOT listing byte-identical to main, so no sibling spec's row
// positions move. It adds ONE folder row to the shred folder's own listing, which shred-dialog.smoke.ts
// is unaffected by (it locates CPE-1241-shred-me.txt by name scan). The `payload` subfolder is what gets
// sealed; the created blob lands as `payload.cpevault` beside it (inside VAULT_CREATE_PARENT_DIR), so it
// never even appears in the shred folder's direct listing.
export const VAULT_CREATE_PARENT_DIR = "CPE-1250-vault-create"; // nested inside SHRED_DIR_NAME
export const VAULT_CREATE_SRC_DIR = "payload"; // the folder the spec seals
export const VAULT_CREATE_BLOB_NAME = "payload.cpevault"; // default sibling dest = <foldername>.cpevault

function seedVaultCreateFixture(tmpDir: string): void {
  const src = path.join(tmpDir, SHRED_DIR_NAME, VAULT_CREATE_PARENT_DIR, VAULT_CREATE_SRC_DIR);
  fs.mkdirSync(path.join(src, "notes"), { recursive: true });
  fs.writeFileSync(path.join(src, "CPE-1250-secret.txt"), "seal me into a vault\n", "utf-8");
  fs.writeFileSync(path.join(src, "notes", "CPE-1250-more.txt"), "nested plaintext\n", "utf-8");
}

// --- File-Health GUI verification fixture (sprint QA, epic CPE-1002) -------------------------
// Seeds the 3 findings the File-Health panel's `mismatch`/`orphan`/`empty` tabs need for an
// end-to-end proof over a REAL Tauri ipc::Channel (the `dangling` tab's finding already exists —
// `seedLinkBadgeFixture`'s LINK_BROKEN_NAME broken symlink, shared at tmpDir root — so it is not
// re-seeded here). Isolated in its own subfolder so it never perturbs any other spec's row-position
// assumptions in the shared tmpDir.
export const FILE_HEALTH_DIR_NAME = "file-health-gui-verify";
export const FILE_HEALTH_MISMATCH_NAME = "fh-disguised.jpg";
export const FILE_HEALTH_ORPHAN_NAME = "fh-orphan.srt";
export const FILE_HEALTH_EMPTY_SUBDIR_NAME = "fh-empty-nested";

function seedFileHealthFixture(tmpDir: string): void {
  const dir = path.join(tmpDir, FILE_HEALTH_DIR_NAME);
  fs.mkdirSync(dir, { recursive: true });
  // Real PE/MZ header bytes claiming to be a .jpg — sniffs as "Windows executable/library" per
  // `crates/server/src/type_mismatch_scan.rs`'s own
  // `pe_disguised_as_jpg_is_flagged_with_right_claimed_and_detected` test fixture.
  fs.writeFileSync(path.join(dir, FILE_HEALTH_MISMATCH_NAME), Buffer.from([0x4d, 0x5a, 0x90, 0x00, 0x03, 0x00]));
  // A subtitle sidecar with no matching video primary (`.mp4`/`.mkv`/…) anywhere in the same
  // directory — `crates/server/src/orphan_sidecars.rs`'s default rules report it as orphaned.
  fs.writeFileSync(path.join(dir, FILE_HEALTH_ORPHAN_NAME), "1\n00:00:00,000 --> 00:00:01,000\nhi\n", "utf-8");
  // An empty nested subfolder — cascade-empty, reported by `find_empty_dirs`
  // (`crates/server/src/empty_dirs_scan.rs`). The PARENT dir is not itself empty (it holds the two
  // files above), so only this nested folder is expected to be reported.
  fs.mkdirSync(path.join(dir, FILE_HEALTH_EMPTY_SUBDIR_NAME), { recursive: true });
}

// --- Declutter GUI verification fixture (CPE-1329, epic CPE-979) -------------------------------
// Seeds one file per `ClutterReason` (`crates/server/src/organize.rs`) into its own subfolder, so
// the Declutter dialog's real `organize_clutter` scan has one finding to flag under each of the
// four reason groups it renders. Isolated in its own subfolder so it never perturbs any other
// spec's row-position assumptions in the shared tmpDir.
export const DECLUTTER_DIR_NAME = "declutter-gui-verify";
export const DECLUTTER_ZERO_BYTE_NAME = "declutter-empty.log";
export const DECLUTTER_INSTALLER_NAME = "declutter-setup.exe";
export const DECLUTTER_TEMP_NAME = "declutter-movie.mp4.part";
export const DECLUTTER_BACKUP_NAME = "declutter-notes.txt.bak";

function seedDeclutterFixture(tmpDir: string): void {
  const dir = path.join(tmpDir, DECLUTTER_DIR_NAME);
  fs.mkdirSync(dir, { recursive: true });
  // A zero-byte file — `find_clutter`'s most-definitive reason, flagged purely by `size == 0`.
  fs.writeFileSync(path.join(dir, DECLUTTER_ZERO_BYTE_NAME), "");
  // An installer extension (`.exe`) — needs non-zero content so it doesn't get claimed by the
  // (checked-first) zero-byte rule instead.
  fs.writeFileSync(path.join(dir, DECLUTTER_INSTALLER_NAME), Buffer.from([0x4d, 0x5a, 0x90, 0x00]));
  // A partial/temporary download (`.part`) — matched by `name_lower.ends_with(".part")`.
  fs.writeFileSync(path.join(dir, DECLUTTER_TEMP_NAME), "partial download bytes\n", "utf-8");
  // A backup/leftover (`.bak` extension) — matched by `entry.ext == "bak"`.
  fs.writeFileSync(path.join(dir, DECLUTTER_BACKUP_NAME), "old content\n", "utf-8");
}

// --- CPE-1331: Metadata Studio fixture (epic CPE-725) -------------------------------------------
// A DEDICATED subfolder + a tiny but genuinely-valid ID3v2.3-tagged "mp3" so metadata-studio.smoke.ts
// can select it, open the real Metadata Studio dialog (MetadataStudioDialog.svelte, CPE-1041) via the
// Command Palette, and assert its EDITABLE fields actually render against a real IPC round trip
// (`metadata_read` / `metadata_writable`, crates/server/src/media_meta.rs).
//
// The file is NOT a playable audio stream — like ORGANIZE_MP3_NAME above, the app never decodes
// audio here — but unlike that throwaway fixture, this one carries a hand-built, byte-accurate ID3v2
// tag (see `read_id3v2` in crates/server/src/media_meta_read.rs: it parses the tag purely off the
// leading `ID3` header + syncsafe size + frames, entirely independent of whether the rest of the file
// is a real MPEG stream). `is_writable("mp3")` is unconditional on extension
// (crates/server/src/media_meta.rs), so this fixture round-trips through the dialog's real "writable"
// gate without needing any audio payload at all. Isolated in its own subfolder, same reasoning as the
// CPE-1241/1226/1249 fixtures above.
export const METADATA_STUDIO_DIR_NAME = "metadata-studio-gui-verify";
export const METADATA_STUDIO_MP3_NAME = "CPE-1331-song.mp3";
export const METADATA_STUDIO_TITLE = "CPE-1331 Song Title";
export const METADATA_STUDIO_ARTIST = "CPE-1331 Artist";

/** Encode a 28-bit ID3v2 "syncsafe" integer (7 usable bits per byte, matching `read_id3v2`'s own
 *  `syncsafe28` decoder) into the 4-byte big-endian form the tag header expects. */
function id3Syncsafe28(n: number): Buffer {
  return Buffer.from([(n >> 21) & 0x7f, (n >> 14) & 0x7f, (n >> 7) & 0x7f, n & 0x7f]);
}

/** One ID3v2.3 text frame: 4-byte id + 4-byte big-endian (non-syncsafe — v2.3 frame sizes are plain
 *  big-endian per `read_id3v2`'s `major == 3` branch) size + 2 flag bytes (unset) + a 1-byte Latin-1
 *  encoding marker (`0x00`) + the text itself. */
function id3TextFrame(id: string, text: string): Buffer {
  const body = Buffer.concat([Buffer.from([0x00]), Buffer.from(text, "latin1")]);
  const size = Buffer.alloc(4);
  size.writeUInt32BE(body.length, 0);
  return Buffer.concat([Buffer.from(id, "latin1"), size, Buffer.from([0x00, 0x00]), body]);
}

/** A minimal, genuinely-valid ID3v2.3 tag (header + text frames, no audio payload needed — see the
 *  fixture header comment above) that `read_id3v2` parses into real editable `MetaField`s. */
function minimalId3v2Mp3(frames: Array<[string, string]>): Buffer {
  const frameBytes = Buffer.concat(frames.map(([id, text]) => id3TextFrame(id, text)));
  const header = Buffer.concat([Buffer.from("ID3", "latin1"), Buffer.from([3, 0, 0]), id3Syncsafe28(frameBytes.length)]);
  return Buffer.concat([header, frameBytes]);
}

function seedMetadataStudioFixture(tmpDir: string): void {
  const dir = path.join(tmpDir, METADATA_STUDIO_DIR_NAME);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, METADATA_STUDIO_MP3_NAME),
    minimalId3v2Mp3([
      ["TIT2", METADATA_STUDIO_TITLE],
      ["TPE1", METADATA_STUDIO_ARTIST],
    ]),
  );
}

// --- CPE-1358: sample-navigation fixture (epic CPE-1148) ----------------------------------------
// A COPY of the real repo `samples/` tree, seeded into its own subfolder of the shared tmpDir, for
// `samples.smoke.ts` to walk — every file the app claims to support a preview for, opened for real and
// asserted not to crash the app (the CPE-1357 PDF-preview-crash class of bug). Copied (not opened via a
// second `--open=<dir>`) because this harness's `wdio.conf.ts`/`onPrepare` launches exactly ONE shared
// app session that every spec in the suite navigates around inside — the same reasoning every other
// `seed*Fixture` above follows (CPE-1143/1207/1241/1226/1249/1329/…), not a special case invented here.
// `fs.cpSync`'s recursive copy means new samples added to the real `samples/` tree are picked up
// automatically the next time this runs — no filename list to keep in sync, matching the ticket's "drive
// it data-first off the sample tree" requirement. `README.md` is excluded (documentation, not a fixture
// — `sampleCoverage.test.ts` excludes it too, for the same reason).
export const SAMPLES_DIR_NAME = "CPE-1358-samples";
const REAL_SAMPLES_DIR = path.resolve(__dirname, "..", "samples");

function seedSamplesFixture(tmpDir: string): void {
  const dest = path.join(tmpDir, SAMPLES_DIR_NAME);
  fs.cpSync(REAL_SAMPLES_DIR, dest, {
    recursive: true,
    filter: (src) => path.basename(src) !== "README.md",
  });
}

let tauriDriver: ChildProcess | undefined;
let shuttingDown = false;

function killTauriDriver() {
  shuttingDown = true;
  tauriDriver?.kill();
}

export const config: WebdriverIO.Config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: 4444,
  path: "/",
  specs: ["./specs/**/*.smoke.ts"],
  maxInstances: 1,
  capabilities: [
    {
      // "wry" is Tauri's own webview wrapper (WebView2 on Windows / WebKitGTK on Linux) — not a
      // real browser name.
      browserName: "wry",
      // WebdriverIO v9 auto-upgrades to the WebDriver BiDi protocol whenever the remote end
      // advertises support. Modern msedgedriver does advertise it, but its BiDi
      // `browsingContext` model does not reliably attach to wry's embedded WebView2 control (in
      // testing it stays on the driver's own "about:blank" placeholder context forever, so every
      // DOM query returns empty — the app itself is running and navigating fine, WebdriverIO is
      // just looking at the wrong context). Forcing classic WebDriver (HTTP/JSON) sidesteps that
      // and talks to the actual webview.
      "wdio:enforceWebDriverClassic": true,
      "tauri:options": {
        application: APP_BINARY,
        // Populated in onPrepare once the temp dir exists (see below).
        //
        // tauri-driver itself DOES forward `args` into the native driver's launch options
        // (`ms:edgeOptions.args` on Windows / `webkitgtk:browserOptions.args` on Linux — verified
        // by reading tauri-driver's own source, crates/tauri-driver/src/server.rs
        // `TauriOptions { application, args }`). BUT empirically (verified here with a direct
        // WebDriver `/session` POST + inspecting the spawned process's real command line via
        // `Get-CimInstance Win32_Process`), msedgedriver's OWN arg handling silently DROPS a bare
        // positional token that doesn't look like a `--switch`: `args: ['--open', tmpDir]` arrived
        // at the app as just `--open` with the path gone (msedgedriver logs "Ignoring switch with
        // invalid name: <path>" when this happens — that log line is not just a warning, the arg is
        // actually discarded). The fix: encode it as ONE Chromium-style `--switch=value` token —
        // `--open=<tmpDir>` — which msedgedriver preserves verbatim (including the space in this
        // machine's own username, confirmed via the same process-inspection probe). So do NOT split
        // `--open` and its value into two array entries.
        args: [] as string[],
        // CPE-1048: a Tauri maintainer reported (tauri discussion #10122) that adding an empty
        // `webviewOptions: {}` here — with a recent tauri-driver — resolved a Windows "session not
        // created" failure; it nudges the driver into proper WebView2 mode. Cheap, zero-risk,
        // shipped alongside the WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS env var in gui-smoke.yml
        // (the actual DevToolsActivePort fix) rather than instead of it.
        // CPE-1171: left unconditional (not Windows-gated) — tauri-driver's own `TauriOptions` struct
        // only *has* a `webview_options` field under `#[cfg(target_os = "windows")]`; on the Linux
        // build of tauri-driver that key simply isn't a struct field, and serde silently ignores
        // unknown JSON keys by default (no `deny_unknown_fields` on that struct), so this is a no-op
        // on the Linux leg rather than an error.
        webviewOptions: {},
      },
    } as WebdriverIO.Capabilities,
  ],
  logLevel: "info",
  framework: "mocha",
  // CPE-1048: modest bump for a slow first WebView2 cold start in CI (no GPU / restricted session)
  // — not a fix for a hard sandbox/GPU crash on its own, just headroom so a slow-but-successful
  // session isn't misread as a failure.
  connectionRetryTimeout: 180_000,
  // CPE-1594: "spec" stays for a human-readable log; the "json" reporter writes one machine-readable
  // result file per spec-file worker into `.results/` (gitignored — run output, same treatment as
  // `.screenshots/`) — `scripts/run-ratchet.ts` reads those files as the ratchet's source of truth
  // instead of parsing the "spec" reporter's text, which is not a contract. `outputFileFormat` names
  // each file after its worker `cid` so `maxInstances: 1`'s sequential-but-still-one-worker-per-spec
  // runs never clobber each other's output.
  reporters: [
    "spec",
    [
      "json",
      {
        outputDir: "./.results",
        outputFileFormat: (opts: { cid: string }) => `wdio-${opts.cid}.json`,
      },
    ],
  ],
  mochaOpts: {
    ui: "bdd",
    // CPE-1481: already generous (90s per `it`, well under the 35-min job cap raised by this same
    // ticket) — one slow/hung test fails on its own instead of eating the whole job's budget.
    // Checked, not duplicated: this was already the value here before CPE-1481, no change needed.
    timeout: 90_000,
  },

  // Seed the temp dir + marker file BEFORE any session starts, and wire the launch args (capability
  // `tauri:options.args`) so the app opens off-screen and non-focused (CPE-1046/1047 — can't grab a
  // CI display) directly into the seeded dir via `--open=<dir>` — the exact path CPE-1043 shipped and
  // CPE-1044 fixed (see the single-token comment above for why it's one `--open=<dir>` token here
  // rather than the two-token `--open <dir>` form a human would type). Negative geometry MUST use the
  // `=` form too — `--x -4000` fails clap parsing, `--x=-4000` doesn't. Only real app flags
  // (clap-parsed) go here — Chromium browser flags belong in the WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
  // env var (gui-smoke.yml), never in this array.
  onPrepare: (_config, capabilities) => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "cpe-gui-smoke-"));
    fs.writeFileSync(path.join(tmpDir, MARKER_NAME), "CPE-1045 smoke marker\n", "utf-8");
    // CPE-1096: seed the code-preview fixture alongside the marker, in the same temp dir, so the
    // suite can open it in the same session without a second app launch.
    fs.writeFileSync(path.join(tmpDir, FIXTURE_NAME), FIXTURE_SOURCE, "utf-8");

    // CPE-1143: seed a few mixed-kind files for the auto-organize preview (see the block above).
    seedOrganizeFixture(tmpDir);

    // CPE-1144: seed two real, decodable PNGs for the Batch-Media dialog's plan preview (see the
    // block above) — same tmpDir, same single app launch.
    seedBatchMediaFixture(tmpDir);

    // CPE-1203 / CPE-1205: seed two structured (multi-band) near-duplicate PNGs for the
    // Find-similar-images dialog (see the block above) — same tmpDir, same single app launch.
    seedSimilarImagesFixture(tmpDir);

    // CPE-1221: seed two near-identical .md docs + one unrelated doc for the Find-similar-documents
    // dialog (see the block above) — same tmpDir, same single app launch.
    seedNearDupDocsFixture(tmpDir);

    // CPE-1130: seed the cost-History journal fixture into the real app-data dir (see the block
    // above) before the app process ever starts, so `metrics_history` has rows to read the moment
    // the History tab is opened.
    seedHistoryFixture();

    // CPE-1135: seed the Replay-tab's audit journal + baseline fixture, rooted at this same tmpDir,
    // before the app process ever starts (see the block above).
    seedReplayFixture(tmpDir);

    // CPE-1154: seed an empty subdirectory for the native-context-menu-leak repro (see block above).
    seedEmptyFolderFixture(tmpDir);

    // CPE-1207: seed a dedicated empty subdirectory for the New-Link dialog click-through (see block
    // above) — separate from CPE-1154's folder so this spec's write doesn't touch that one's fixture.
    seedNewLinkFixture(tmpDir);

    // CPE-1181: seed a genuinely valid single-entry .tar.gz for the non-zip archive-browse pin (see
    // block above) — same tmpDir, same single app launch.
    seedArchiveBrowseFixture(tmpDir);

    // CPE-1208: seed an intact + a broken symlink for the link-badge render pin (see block above) —
    // same tmpDir, same single app launch. Best-effort; the spec checks `linkBadgeFixture.supported`.
    const linkBadgeSupported = seedLinkBadgeFixture(tmpDir);

    // CPE-1237: seed the mixed-format (PNG/SVG/bad-font) subfolder for the streaming-thumbnail-client
    // gallery render pin (see block above) — same tmpDir, same single app launch.
    seedThumbnailGalleryFixture(tmpDir);

    // CPE-1268: seed a real one-page PDF + a tiny (ffmpeg-generated) video for the pdf/video real-
    // thumbnail regression pin guarding CPE-1267 (see block above). Best-effort video (needs ffmpeg).
    const thumbFormatsFixture = seedThumbnailFormatsFixture(tmpDir);

    // CPE-1241: seed a dedicated throwaway file for the ShredConfirmDialog render pin (see block
    // above) — its own subfolder so nothing else is ever at risk.
    seedShredFixture(tmpDir);

    // CPE-1226: seed a dedicated subfolder + source file for the TransferPanel archive-op-row render
    // pin (see block above) — its own subfolder, isolated from every other spec's fixtures.
    seedTransferPanelFixture(tmpDir);

    // CPE-1249: seed a dedicated subfolder + a real pre-sealed `.cpevault` blob for the vault
    // mount/browse/lock flow (see block above) — same tmpDir, same single app launch.
    seedVaultFixture(tmpDir);

    // CPE-1250: seed a small plaintext tree (nested inside the CPE-1241 shred folder — see the block
    // above for why it is NOT a new top-level folder) for the "Create encrypted vault…" flow.
    seedVaultCreateFixture(tmpDir);

    // File-Health GUI verification (sprint QA, epic CPE-1002): seed the mismatch/orphan/empty
    // findings for file-health.smoke.ts's 4-tab end-to-end pass (see block above).
    seedFileHealthFixture(tmpDir);

    // CPE-1329: seed one file per ClutterReason for declutter.smoke.ts's Declutter-dialog pass
    // (see block above) — same tmpDir, same single app launch.
    seedDeclutterFixture(tmpDir);

    // CPE-1331: seed the ID3-tagged mp3 for the Metadata Studio dialog's editable-fields render pin
    // (see block above) — same tmpDir, same single app launch.
    seedMetadataStudioFixture(tmpDir);

    // CPE-1358: seed a copy of the real samples/ tree for the sample-navigation smoke walk (see block
    // above) — same tmpDir, same single app launch.
    seedSamplesFixture(tmpDir);

    const caps = capabilities as unknown as Array<{ "tauri:options": { args: string[] } }>;
    caps[0]["tauri:options"].args = ["--test-mode", "--x=-4000", `--open=${tmpDir}`];

    fs.writeFileSync(
      STATE_FILE,
      JSON.stringify({
        tmpDir,
        linkBadgeFixture: { supported: linkBadgeSupported },
        thumbFormatsFixture,
      }),
      "utf-8",
    );
  },

  // tauri-driver proxies WebDriver requests to the platform's native driver (msedgedriver on
  // Windows, WebKitWebDriver on Linux), which is what actually launches the app binary.
  beforeSession: () => {
    tauriDriver = spawn(TAURI_DRIVER_BIN, [], {
      stdio: [null, process.stdout, process.stderr],
    });
    tauriDriver.on("error", (error) => {
      console.error("[gui-smoke] tauri-driver error:", error);
      process.exit(1);
    });
    tauriDriver.on("exit", (code) => {
      if (!shuttingDown) {
        console.error("[gui-smoke] tauri-driver exited early with code:", code);
        process.exit(1);
      }
    });
  },

  // Note: afterSession might not run if the session fails to start, so onComplete (below) also
  // kills tauri-driver — same belt-and-braces pattern as the official Tauri WebDriver example.
  afterSession: () => {
    killTauriDriver();
  },

  onComplete: () => {
    killTauriDriver();
    try {
      const { tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {
      // best effort — nothing to clean up if the state file was never written (e.g. onPrepare
      // itself threw before reaching that point).
    }
    fs.rmSync(STATE_FILE, { force: true });
    // CPE-1130: restore whatever was at the history.jsonl fixture path before this run (or delete it
    // if we created it fresh) — see the comment on `historyFixtureBackup` above.
    restoreHistoryFixture();
    // CPE-1135: same restore discipline for the Replay-tab journal + baseline fixture.
    restoreReplayFixture();
  },
};

for (const sig of ["exit", "SIGINT", "SIGTERM", "SIGHUP"] as const) {
  process.on(sig, killTauriDriver);
}
