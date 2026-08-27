// wdio.conf.ts — CPE-1045 headless GUI smoke test harness.
//
// Drives the REAL built cross-platform-explorer.exe through tauri-driver + WebdriverIO and asserts
// `--open <dir>` (CPE-1043, fixed by CPE-1044) actually navigated the explorer into that folder. This
// file is deliberately isolated in gui-smoke/ with its own package.json/lockfile/tsconfig — it never
// touches the app's package.json, src-tauri, or the root vitest suite.
//
// Prereqs (see README.md): `cargo install tauri-driver --version 2.0.6 --locked` (CPE-1843 pinned that
// version in CI and here — see TAURI_DRIVER_PORT below for what depends on it), a matching msedgedriver on PATH
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
import { formatShardAbort, isTransportDead } from "./lib/driverHealth.js";
import { resetAppState } from "./lib/resetAppState.js";
import { assignShardSpecs, parseShardId, shardResultFilePrefix } from "./lib/shard.js";
import { listSpecFiles, specRunPath } from "./lib/specFiles.js";
import { waitForPort } from "./lib/waitForPort.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// --- CPE-1753: spec sharding -------------------------------------------------------------------
// A fully healthy green `gui-smoke-linux` run measured 41.70 min against its own 45-minute job cap, and
// suite duration scales with spec count (0.73-1.80 min per added spec file), so two to four more specs
// would have breached it. `gui-smoke.yml` now runs the suite as a matrix of shard jobs, each setting
// GUI_SMOKE_SHARD_INDEX/GUI_SMOKE_SHARD_TOTAL; `assignShardSpecs` deals the spec files out
// deterministically so every file lands in exactly one shard. Unset (a local `npm test`, the Windows
// leg) means "run everything", byte-identical to the pre-sharding behaviour.
//
// `parseShardId` THROWS if only one of the two env vars is set — see its own comment. A wdio.conf that
// quietly ran the whole suite in every shard, or one shard's slice while believing it was the whole
// suite, is exactly the failure this harness cannot afford.
const SHARD_ID = parseShardId(process.env);
const SHARD_SPECS = SHARD_ID ? assignShardSpecs(listSpecFiles(path.resolve(__dirname, "specs")), SHARD_ID) : undefined;
if (SHARD_ID && SHARD_SPECS) {
  // eslint-disable-next-line no-console
  console.log(
    `[gui-smoke] shard ${SHARD_ID.shardIndex} of ${SHARD_ID.shardTotal} — running ${SHARD_SPECS.length} ` +
      `spec file(s): ${SHARD_SPECS.join(", ")}`,
  );
}

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

// CPE-1832: tauri-driver is a two-port intermediary, not a single listener, and both ports are named
// here explicitly (passed to `spawn` below as `--port`/`--native-port`) rather than left to its
// internal defaults — see `beforeSession`'s comment for why that distinction is the actual fix.
// TAURI_DRIVER_PORT is tauri-driver's OWN front door (matches `config.port` below, and tauri-driver
// 2.0.6's own `--port` default — `cli.rs`). NATIVE_DRIVER_PORT is the port tauri-driver spawns the
// REAL platform WebDriver (WebKitWebDriver on Linux, msedgedriver on Windows) on and proxies every
// request to (`cli.rs`'s `--native-port` default, also 4445 — verified by reading the vendored
// tauri-driver-2.0.6 source in the local cargo registry cache, `src/{main,cli,server}.rs`).
//
// CPE-1843: because these two constants and the `--port`/`--native-port` flags below are a contract with
// a binary CI installs from crates.io, `gui-smoke.yml` now pins `cargo install tauri-driver --version
// 2.0.6` at BOTH of its install sites rather than taking whatever is newest. Note WHICH parts of that
// contract can rot quietly: `--port`/`--native-port` are passed explicitly below, so their upstream
// defaults are overridden and only the flag NAMES matter — and a rename fails loudly, since tauri-driver
// rejects unknown args. The silent one is `--native-host`, which we do NOT pass: the waits below poll
// "127.0.0.1" purely because that is its default, so a future re-default would send the native driver
// somewhere nothing ever looks. Re-verify `src/cli.rs` when bumping that pin.
const TAURI_DRIVER_PORT = 4444;
const NATIVE_DRIVER_PORT = 4445;

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

// --- CPE-1827: Trash titlebar overflow-menu fixture -------------------------------------------
// trash-titlebar.smoke.ts needs ONE genuine entry sitting in the real OS Recycle Bin/Trash before the
// app even launches — `list_trash_stream` reads whatever is actually there, so there is no
// lower-level seam to inject a fake entry through. An earlier version of this fixture wrote a plain
// file into the listing and had the SPEC delete it via the app's own UI (right-click -> the quickrow
// "Delete (Del)" icon button); that hung indefinitely on this machine (WebDriver never returned a
// result for the click, CDP or classic) with no diagnostic pointing at a cause. Whether that is a
// genuine issue with this app's delete flow or an environment quirk of automating a native
// Recycle-Bin move against an off-screen `--test-mode` window is exactly the kind of question CPE-1822
// (this ticket's own "no gui-smoke coverage of Trash at all" prerequisite) should chase down with time
// to spare — not a blocker for CPE-1827, which is about the TITLEBAR, not the delete flow. Moving the
// seed to a native, pre-launch OS call sidesteps the hang entirely while still exercising the real
// thing this spec actually needs: a genuine `list_trash_stream` entry.
//
// Windows: `Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(..., OnlyErrorDialogs,
// SendToRecycleBin)` — the standard silent (no "are you sure?" prompt) recycle-bin move, run
// synchronously via PowerShell before the app process starts.
// Linux: `gio trash` (GNOME's freedesktop.org-Trash-spec-compliant CLI, the same mechanism the `trash`
// crate itself targets there) if present on PATH; best-effort, like `seedThumbnailFormatsFixture`'s
// ffmpeg check above — returns `supported: false` rather than throwing when unavailable so the spec
// can skip cleanly instead of failing the whole suite over missing CI tooling.
// macOS: TrashView never mounts there (`canBrowseTrash` gates it off, per TrashView.svelte's own
// header) — not attempted.
export const TRASH_TITLEBAR_DIR_NAME = "CPE-1827-trash-titlebar"; // nested inside SHRED_DIR_NAME
export const TRASH_TITLEBAR_FILE_NAME = "CPE-1827-fixture.txt";

function seedTrashTitlebarFixture(tmpDir: string): boolean {
  const dir = path.join(tmpDir, SHRED_DIR_NAME, TRASH_TITLEBAR_DIR_NAME);
  fs.mkdirSync(dir, { recursive: true });
  const filePath = path.join(dir, TRASH_TITLEBAR_FILE_NAME);
  fs.writeFileSync(
    filePath,
    "CPE-1827 throwaway gui-smoke fixture — moved to the real OS Trash before the app launches.\n",
    "utf-8",
  );

  if (process.platform === "win32") {
    const psCommand =
      "Add-Type -AssemblyName Microsoft.VisualBasic; " +
      "[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile(" +
      `'${filePath.replace(/'/g, "''")}', ` +
      "[Microsoft.VisualBasic.FileIO.UIOption]::OnlyErrorDialogs, " +
      "[Microsoft.VisualBasic.FileIO.RecycleOption]::SendToRecycleBin)";
    // CPE-1827: measured well over 30s on the dev machine for this exact call (the VB.NET FileIO
    // Recycle-Bin move appears to do real work — AV scan of the target, COM/shell init, or similar —
    // before returning, even though it completes with no visible UI and no error). A generous 2-minute
    // budget here costs nothing but one-time `onPrepare` latency; too short would report a false
    // `supported: false` and silently skip this spec's own coverage.
    const result = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", psCommand], {
      timeout: 120_000,
    });
    return result.status === 0 && !fs.existsSync(filePath);
  }

  if (process.platform === "linux") {
    const result = spawnSync("gio", ["trash", filePath], { timeout: 30_000 });
    return result.status === 0 && !fs.existsSync(filePath);
  }

  return false;
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

// --- CPE-1955: bounded containment for a mid-shard WebDriver transport death -------------------------
// See `lib/driverHealth.ts`'s header for the measured four-failure chain this exists for, and
// `recoverSession` below for how the budget is spent. Worker-local like `tauriDriver` above: each worker
// is its own process with its own fresh import of this file, so one shard can never lend or borrow
// another's budget.

/** How many times ONE worker may respawn tauri-driver mid-shard. One. This is a bounded in-job retry,
 *  never an exemption: when it is spent, `respawnTauriDriver` throws, the shard is marked aborted, and the
 *  ratchet reds exactly as it does today. Raising this would trade a fast, legible red for a slow one —
 *  each respawn costs a full driver + app cold start — and would not fix anything a single restart cannot,
 *  since a transport that dies twice in one shard is a real, reportable defect and not a blip. */
const MAX_DRIVER_RESPAWNS = 1;
let driverRespawnsUsed = 0;

/** Latched once the transport is confirmed gone AND the respawn budget is spent. Suppresses further
 *  doomed `resetAppState` attempts (which cannot succeed and buried the diagnosis under ~10k lines of
 *  identical stack traces in all four observed failures). It does NOT suppress, skip, or excuse any spec:
 *  every remaining file's runnables still execute, still fail against the dead socket, and are still
 *  recorded and reported by name. */
let shardAborted = false;

// CPE-1866: which spec file the most recently seen test/hook belonged to — `beforeTest`/`beforeHook`
// below use a change in this value to detect "a new spec file's runnables have started" and trigger the
// reset. Module-level and worker-local exactly like `tauriDriver` above: each worker is its own process
// with its own fresh import of this file, so this never needs resetting between workers.
//
// WHY NOT `beforeSuite`/`afterSuite` (what the first version of this fix used, and why it did nothing):
// `@wdio/mocha-framework` wires the config-level `beforeSuite`/`afterSuite` hooks as `beforeAll`/
// `afterAll` on mocha's OWN implicit ROOT suite (`this._runner.suite.beforeAll(...)` — read straight out
// of `node_modules/@wdio/mocha-framework/build/index.js`), and its `prepareMessage` hardcodes their
// payload to `this._runner.suite.suites[0]` — the FIRST loaded suite, always, no matter which one is
// actually running. For the pre-CPE-1866 one-file-per-worker shape those two facts were invisible: root
// suite = that one file's suite, `suites[0]` = the only suite there is. Grouping breaks BOTH assumptions
// at once: `beforeSuite` fires exactly ONCE per worker (a root `beforeAll`, not "once per child suite"),
// and even that one firing reports the FIRST file's identity forever. CPE-1866's first real CI run
// proved this empirically, not just by reading source: the reset never ran a second time (confirmed via
// this file's OWN `[gui-smoke][timing]` log — `beforeSuite:start` for `archive-browse.smoke.ts` appears
// exactly once, and the very next line is the WORKER-level `after:testsDone`, i.e. mocha's root
// `beforeAll` fired, its single child ran, its root `afterAll` fired, done), and the drawer-leak cascade
// this ticket's Work Log documents was consequently NOT actually fixed by that commit's `resetAppState`
// call — it silently never executed. `beforeTest`/`afterTest` (used below) do NOT share this bug: their
// payload is `this._runner.test`, mocha's actual LIVE current-test pointer, updated per real test — see
// `prepareMessage`'s `case "beforeTest": case "afterTest": params.payload = this._runner?.test;`.
let currentSpecFile: string | undefined;

// --- CPE-1866: per-spec-file result recording, replacing the "json" reporter -------------------
// See the `reporters` config option's own comment for WHY: grouping a shard's specs into one worker
// (this file's `specs` comment) means the built-in "json" reporter can no longer tell which spec file a
// suite/test came from. `beforeTest`/`beforeHook`/`afterTest`/`afterHook` below all carry `test.file`
// correctly (see `currentSpecFile`'s comment above for why those, not `beforeSuite`/`afterSuite`), so
// results accumulate per FILE in this map as tests/hooks complete.
//
// CPE-1866 review finding: this used to batch every file's results in memory and flush them ALL in one
// pass from the WORKER-level `after` hook, at the very end. The ratchet still reds correctly on a
// mid-shard crash via its "SUITE DID NOT COMPLETE" clause (`scripts/run-ratchet.ts` tolerates a missing
// `.results/` entirely), so that was never a GATE hole — but it destroyed the FORENSIC trail exactly
// when it matters most: every already-completed file in the shard would lose its own result too, taking
// down the evidence for files that ran cleanly along with whatever actually crashed. Fixed by flushing
// each file's own chunk to disk as SOON as that file's runnables finish — i.e. the moment
// `handleRunnableStart` detects the NEXT file starting — via `flushFileResult`, with the WORKER-level
// `after` hook below now only responsible for flushing the LAST file (nothing else has a "next file"
// transition to trigger it). `RESULTS_DIR` matches the built-in reporter's old `outputDir` so
// `scripts/run-ratchet.ts`'s existing `.results/*.json` glob picks these up unchanged either way.
const RESULTS_DIR = path.resolve(__dirname, ".results");
interface FileResultAccumulator {
  suiteName: string;
  start: string;
  end: string;
  tests: { name: string; state: string }[];
  hooks: { title: string; state: string; error?: unknown }[];
}
const fileResults = new Map<string, FileResultAccumulator>();

/** Returns (creating if necessary) the accumulator for `file`, and bumps its `end` timestamp — called
 *  from every `afterTest`/`afterHook`, so the file's recorded span always covers its last-seen record
 *  even if `beforeTest`/`beforeHook` never got a chance to run for it (defensive: should not happen in
 *  practice, since a hook/test always has a `before*` counterpart, but a crash mid-hook is exactly the
 *  kind of case this ticket's own analysis says must not silently lose data). */
function accumulatorFor(file: string, suiteName: string): FileResultAccumulator {
  let acc = fileResults.get(file);
  if (!acc) {
    acc = { suiteName, start: new Date().toISOString(), end: new Date().toISOString(), tests: [], hooks: [] };
    fileResults.set(file, acc);
  }
  acc.end = new Date().toISOString();
  return acc;
}

/** Writes `file`'s accumulated result to disk NOW (if it has one — a file whose every runnable crashed
 *  before `afterTest`/`afterHook` ever fired leaves nothing to flush, and that absence is itself the
 *  honest signal the ratchet's completeness check reads) and removes it from {@link fileResults}, so a
 *  file's evidence survives independently of whatever happens to the REST of the shard afterward. */
function flushFileResult(file: string): void {
  const acc = fileResults.get(file);
  if (!acc) return;
  fs.mkdirSync(RESULTS_DIR, { recursive: true });
  const stem = path.basename(file).replace(/\.ts$/, "");
  const outFile = path.join(RESULTS_DIR, `wdio-${shardResultFilePrefix(SHARD_ID)}${stem}.json`);
  fs.writeFileSync(
    outFile,
    JSON.stringify({
      specs: [file],
      start: acc.start,
      end: acc.end,
      suites: [{ name: acc.suiteName, tests: acc.tests, hooks: acc.hooks }],
    }),
    "utf-8",
  );
  fileResults.delete(file);
}

/** Called from both `beforeTest` and `beforeHook` (see `currentSpecFile`'s comment for why both) with
 *  the `.file` of whichever runnable is about to run. A no-op unless `file` differs from the last one
 *  seen — i.e. this is the FIRST runnable (test or hook) of a spec file, the "once per spec file"
 *  boundary this whole mechanism exists to detect reliably in a grouped worker. Skips the reset on the
 *  very first file of the worker (see the `beforeTest`/`beforeHook` config hooks' own comment); resets
 *  on every one after that.
 *
 *  CPE-1866 review finding — containment. A shard is now ONE session for many spec files, so a genuine
 *  hang/crash anywhere can in principle take out every file after it (architecturally impossible before
 *  this ticket, when every file started its own cold process) — real CI evidence of exactly that shape
 *  exists in this ticket's own history (a run where 13 of 14 files in a shard never reported, albeit from
 *  a different bug, since fixed). `resetAppState` specifically is now guarded: if it throws, this
 *  restarts the WHOLE session (`browser.reloadSession()` — the SAME capabilities, so a genuinely fresh
 *  app launch, exactly what a crashed/wedged webview needs) and retries ONCE. If the retry also fails,
 *  it is allowed to throw for real — this file's current test/hook fails loudly and attributably (not
 *  silently swallowed into a false pass), and because this is scoped to `resetAppState`'s own call, mocha
 *  still moves on to whatever comes after rather than the whole worker dying. This does not cover every
 *  possible hang (a test's OWN code hanging mid-assertion is bounded only by `mochaOpts.timeout`, 90s,
 *  same as before this ticket) — it specifically closes the "reset itself is what's broken" case, the
 *  one new failure mode this ticket introduces. */
async function handleRunnableStart(file: string): Promise<void> {
  if (file === currentSpecFile) return;
  const previous = currentSpecFile;
  if (previous !== undefined) flushFileResult(previous);
  const label = path.basename(file);
  logPhase("handleRunnableStart:newFile", label);

  // CPE-1955 — THE ATTRIBUTION FIX, and the single line that turns the four observed "only 1 of 14
  // reported" failures into a named diagnosis. `currentSpecFile` used to be assigned at the very END of
  // this function, i.e. only on the path where the reset SUCCEEDED. `flushFileResult` is only ever
  // called with `currentSpecFile` (here, and once more from the worker-level `after` hook), so the
  // moment the reset path below threw, `currentSpecFile` froze on the last file that had reset cleanly
  // — and stayed frozen for the rest of the shard. Every later spec's runnables still ran, still failed
  // against the dead transport, and `afterTest`/`afterHook` still accumulated their results into
  // `fileResults` (WDIO swallows a throwing config hook rather than aborting the worker —
  // `executeHooksWithArgs` in `@wdio/utils` resolves with the error and `testFrameworkFnWrapper` carries
  // on to the runnable, which is why the cascade kept going), but NOTHING ever flushed those
  // accumulators to disk. Thirteen spec files' worth of loud, attributable evidence was recorded and
  // then dropped on the floor, leaving the ratchet to report the aftermath — 1/14 reported, 0 new
  // failing cases — instead of the cause. Measured, not inferred: the failing run's own
  // `gui-smoke-results-ubuntu-shard-2` artifact (run 33107885332) contains exactly one file,
  // `wdio-shard-2-of-4-archive-browse.smoke.json`, for a shard that had visibly executed all fourteen.
  //
  // Advancing it HERE, before anything that can throw, means each spec file's own accumulator is the one
  // that gets flushed at the next transition regardless of what happens in between. A shard that dies
  // now reports every spec BY NAME with its real error, which is still RED (loudly so — see
  // `formatShardAbort`'s note about not mistaking one death for N regressions) and no longer a mystery.
  // It also means the reset is attempted exactly ONCE per spec file, which is what this mechanism always
  // claimed to do: the old freeze made `file !== currentSpecFile` true for every subsequent runnable, so
  // a dead transport re-attempted the reset two-to-four times per file and buried the log in ~10k lines
  // of identical stack traces.
  currentSpecFile = file;

  if (previous === undefined) {
    logPhase("handleRunnableStart:firstFileSkipReset", label);
    return;
  }
  // CPE-1955: once the transport is confirmed gone and the bounded respawn is spent, re-running the
  // reset for every remaining file cannot succeed — it only buries the diagnosis. Skip it and let each
  // remaining file's own runnables fail against the dead socket, which is the honest record of what
  // happened to them and is exactly what makes them appear in the ratchet by name.
  if (shardAborted) {
    logPhase("handleRunnableStart:skipResetShardAborted", label);
    return;
  }
  const { tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  try {
    await resetAppState(tmpDir);
    logPhase("handleRunnableStart:resetDone", label);
  } catch (err) {
    console.error(
      `[gui-smoke] resetAppState failed before ${label} — restarting the session and retrying once:`,
      err,
    );
    logPhase("handleRunnableStart:resetFailedRestartingSession", label);
    try {
      await recoverSession(err);
      await resetAppState(tmpDir);
      logPhase("handleRunnableStart:resetDoneAfterSessionRestart", label);
    } catch (recoveryErr) {
      // CPE-1955: distinguish "the retry also failed" (pre-existing behaviour: throw, this file fails
      // loudly and attributably, mocha moves on) from "the WHOLE transport is gone" (new: say so once,
      // in terms a reader can act on, and stop re-attempting a reset that provably cannot work).
      if (isTransportDead(recoveryErr) || isTransportDead(err)) {
        shardAborted = true;
        console.error(formatShardAbort(label, remainingSpecsAfter(file), recoveryErr));
        logPhase("handleRunnableStart:shardAbortedTransportDead", label);
      }
      throw recoveryErr;
    }
  }
}

/** CPE-1955: how many of this shard's spec files come AFTER `file`, for the abort diagnosis. Falls back
 *  to 0 (rendered as "it was the last one") rather than guessing when the file isn't in the shard plan —
 *  an unsharded local run has no plan, and a wrong count in a diagnostic must not be stated confidently. */
function remainingSpecsAfter(file: string): number {
  if (!SHARD_SPECS) return 0;
  const idx = SHARD_SPECS.indexOf(path.basename(file));
  if (idx < 0) return 0;
  return SHARD_SPECS.length - idx - 1;
}

/** CPE-1955 — the bounded containment. `browser.reloadSession()` alone is the RIGHT answer to an app that
 *  has wedged (it asks the live driver for a brand-new session with the same capabilities, i.e. a genuine
 *  fresh app launch), and it demonstrably works: job 98697809924 recovered that way in 35.4 s from the
 *  identical reset failure on the identical spec. But it is powerless when the failure is the driver
 *  itself being gone, because its very first act is `DELETE /session/<id>` to a socket that no longer has
 *  anyone behind it. In the four observed failures that DELETE is the last thing that happens before
 *  tauri-driver starts logging "connection closed before message completed" and then "Connection refused
 *  (os error 111)" for the rest of the shard.
 *
 *  So: when the cause looks like the transport rather than the app, restart tauri-driver FIRST (which
 *  respawns the native WebKitWebDriver behind it and re-runs the same both-ports readiness wait
 *  `beforeSession` uses), then reload the session against the fresh proxy. The respawn budget is ONE per
 *  worker — see `MAX_DRIVER_RESPAWNS`. Bounded, inside the job, and when it is spent this throws, the
 *  shard is marked aborted, and the ratchet reds exactly as it does today: this is containment, never an
 *  exemption, and it cannot turn a red run green. */
async function recoverSession(cause: unknown): Promise<void> {
  if (isTransportDead(cause)) await respawnTauriDriver();
  try {
    await browser.reloadSession();
  } catch (err) {
    // The reload itself is also where the transport death is FIRST observable when the reset failed for
    // an ordinary app-level reason (the observed chain: a soft breadcrumb assertion, then the DELETE
    // that kills the driver). One more bounded attempt, if the budget has not already been spent.
    if (!isTransportDead(err)) throw err;
    await respawnTauriDriver();
    await browser.reloadSession();
  }
}

/** CPE-1955: kill the current tauri-driver, WAIT for it to actually exit (so its listening sockets are
 *  released before the replacement tries to bind the same two fixed ports — a `.kill()` returns the
 *  instant SIGTERM is queued, not when the process is gone, and racing that would have the readiness
 *  wait below succeed against the DYING listener), then start a fresh one. Throws once the budget is
 *  spent so the caller reds rather than looping. */
async function respawnTauriDriver(): Promise<void> {
  if (driverRespawnsUsed >= MAX_DRIVER_RESPAWNS) {
    throw new Error(
      `[gui-smoke] the WebDriver transport is gone and the tauri-driver respawn budget (${MAX_DRIVER_RESPAWNS}) is spent — not retrying again (CPE-1955)`,
    );
  }
  driverRespawnsUsed += 1;
  console.error(
    `[gui-smoke] the WebDriver transport looks gone — respawning tauri-driver (respawn ${driverRespawnsUsed} of ${MAX_DRIVER_RESPAWNS}, CPE-1955)`,
  );
  logPhase("respawnTauriDriver:start", "(worker)");
  const dying = tauriDriver;
  tauriDriver = undefined; // supersede first: the old child's own exit handler checks identity below
  if (dying) await killAndWaitForExit(dying, 10_000);
  logPhase("respawnTauriDriver:oldProcessGone", "(worker)");
  await startTauriDriver("(respawn)");
  logPhase("respawnTauriDriver:driverReady", "(worker)");
}

/** Sends SIGTERM, waits up to `budgetMs` for the real `exit`, then escalates to SIGKILL and waits again.
 *  Resolves either way — a driver we could not kill is a problem the caller's own readiness wait will
 *  surface as a loud, bounded failure, not something to hang the whole worker on. */
function killAndWaitForExit(child: ChildProcess, budgetMs: number): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve();
  return new Promise<void>((resolve) => {
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      setTimeout(finish, 1_000);
    }, budgetMs);
    let done = false;
    function finish(): void {
      if (done) return;
      done = true;
      clearTimeout(timer);
      resolve();
    }
    child.once("exit", finish);
    child.kill();
  });
}

/** Spawns tauri-driver and does not return until BOTH its front door (its own port) and its back door
 *  (the native WebKitWebDriver/msedgedriver it spawns) are actually accepting connections. Extracted from
 *  `beforeSession` by CPE-1955 so the mid-shard respawn above starts the driver by exactly the same path,
 *  readiness waits included, rather than a second hand-rolled copy that could drift from it.
 *
 *  The `error`/`exit` handlers are bound to the child they were attached to and no-op if `tauriDriver` has
 *  since moved on — a deliberate respawn supersedes the old process before killing it, and its eventual
 *  `exit` must NOT be read as "the driver died unexpectedly" and take the worker down with `process.exit(1)`. */
async function startTauriDriver(specLabel: string): Promise<void> {
  const child = spawn(
    TAURI_DRIVER_BIN,
    ["--port", String(TAURI_DRIVER_PORT), "--native-port", String(NATIVE_DRIVER_PORT)],
    { stdio: [null, process.stdout, process.stderr] },
  );
  tauriDriver = child;
  child.on("error", (error) => {
    if (tauriDriver !== child) return;
    console.error("[gui-smoke] tauri-driver error:", error);
    process.exit(1);
  });
  child.on("exit", (code) => {
    if (tauriDriver !== child) return; // superseded by a CPE-1955 respawn — expected, not a failure
    if (!shuttingDown) {
      console.error("[gui-smoke] tauri-driver exited early with code:", code);
      process.exit(1);
    }
  });
  // Front door first (fast: a bare TCP bind) — see the CPE-1772 half of `beforeSession`'s comment.
  await waitForPort("127.0.0.1", TAURI_DRIVER_PORT, 10_000, "tauri-driver");
  logPhase("beforeSession:frontDoorReady", specLabel);
  // Back door second (slower: a whole other driver binary has to start) — see the CPE-1832 half of that
  // comment. This is the actual fix for the observed "Connection refused" evidence.
  await waitForPort(
    "127.0.0.1",
    NATIVE_DRIVER_PORT,
    20_000,
    "the native WebDriver (WebKitWebDriver/msedgedriver, spawned by tauri-driver)",
  );
}

function killTauriDriver() {
  shuttingDown = true;
  tauriDriver?.kill();
}

// --- CPE-1866: phase-breakdown timing, measurement-only ----------------------------------------
// CPE-1858 measured the ~29.5 s/spec session overhead as ONE number (`span - sum(test durations)`).
// Nobody has measured what it is actually spent on. This logs a labelled, millisecond-resolution
// timestamp at each phase boundary of a worker's lifecycle — driver-process start, app-launch/session-
// create, test execution, and teardown — so a real CI run's log can be diffed into a breakdown instead
// of treated as one opaque constant. Every worker is its own child process (a fresh import of this file
// per spec file — see the CPE-1772/1832 comments on `beforeSession` below for why), so `specLabel` is
// read fresh in each one from the args WDIO's hooks are called with, not from `SHARD_SPECS` above (which
// is this worker's WHOLE shard assignment, not the one file it is currently running).
function logPhase(label: string, specLabel: string): void {
  // eslint-disable-next-line no-console
  console.log(`[gui-smoke][timing] ${new Date().toISOString()} spec=${specLabel} phase=${label}`);
}

function specLabelFrom(specs: readonly string[] | undefined): string {
  if (!specs || specs.length === 0) return "(unknown)";
  return specs.map((s) => path.basename(s)).join(",");
}

export const config: WebdriverIO.Config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: TAURI_DRIVER_PORT,
  path: "/",
  // CPE-1753: this shard's assigned subset, or the whole flat `specs/` directory when unsharded. The
  // list is enumerated (via `lib/specFiles.ts`) rather than left as a `./specs/**/*.smoke.ts` glob so
  // that wdio, the shard manifest, and the ratchet's expected-spec-count all read the SAME list from the
  // SAME function — a partition computed from a different list than the expectation is checked against
  // is how a spec file ends up assigned to nobody and expected by nobody.
  //
  // CPE-1866: wrapped in ONE outer array so this is a SINGLE grouped job, not N flat ones. WDIO's local
  // runner gives each TOP-LEVEL `specs` entry its own worker (and therefore its own WebDriver session —
  // `beforeSession`/`before`/`after`/`afterSession` above all fire once per entry); nesting an array
  // inside it runs every file in that nested array, in order, inside ONE worker/ONE session instead.
  // This is THE fix: measured on a real CI run (this ticket's Work Log), the ~29.5s/spec fixed cost
  // CPE-1858 found is >99% app-launch/session-create (~30.4-32.6s per spec, essentially constant across
  // 33 samples) and <0.3s driver-process spawn — so the only lever that moves it is launching the app
  // FEWER TIMES, not launching it faster. Grouping trades that repeated launch for ONE launch per shard
  // (or per whole run, unsharded) plus `resetAppState` (see `lib/resetAppState.ts` and the `beforeSuite`
  // hook below) between files — see this file's and that module's header comments for what that costs
  // in isolation and how it's proven safe.
  specs: [(SHARD_SPECS ?? listSpecFiles(path.resolve(__dirname, "specs"))).map(specRunPath)],
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
  // CPE-1772 root cause for the "GUI smoke shard 2 hangs on a waitUntil poll, produces no further
  // output, and is killed by the ~30-minute step timeout" failure (first seen on PR #923,
  // `specs/samples.smoke.ts`, re-run alone passes clean): `waitForPreviewToSettle`
  // (`lib/samplesNav.ts`) already has its own explicit, bounded deadline (20s by default), but that
  // bound is measured BETWEEN condition polls — it cannot fire while a single poll is still stuck
  // inside one WebDriver command. If the WebKitGTK/Xvfb session becomes unresponsive mid-poll (a
  // crashed/wedged compositor, not an app bug — confirmed by CPE-1679's `.mp-fallback` finding that
  // the app itself had already reached a real terminal state on a similar hang), that ONE command
  // does not fail fast: WDIO's default `connectionRetryCount` (3) means it retries up to
  // `connectionRetryTimeout` (above) THREE MORE TIMES before giving up, so one wedged command can
  // silently eat up to 4 x 180s = 12 minutes with zero log output — and it is never just one command.
  // `specs/samples.smoke.ts`'s own `afterEach` (see `lib/snap.ts#snapFailure`) issues a screenshot
  // command against the SAME wedged session immediately after, and the next test's setup does too —
  // each can independently eat another ~12 minutes, which stacks past the 30-minute job timeout with
  // no diagnostic ever written, exactly matching "no further output" + "killed at the step timeout"
  // rather than a normal test failure. This is also why re-running the SAME job alone passes clean:
  // the WebKitGTK/Xvfb wedge is a per-session, per-run condition, not a deterministic app or test
  // defect — retrying is legitimate here ONLY because the cause has been identified, not by default.
  //
  // Fix: cap retries at 0. A wedged command now fails after ONE `connectionRetryTimeout` (180s) —
  // still generous relative to a normal command (which returns in milliseconds) — instead of silently
  // multiplying to ~12 minutes. A hang now surfaces as a real, timestamped WebdriverIO error within
  // single-digit minutes, `afterEach`'s `snapFailure` still runs and still has a real chance to
  // capture a screenshot (see its CPE-1149 note), and the existing "Classify suite log" step
  // (CPE-1728) and the shard's Ratchet verdict both get a log to read instead of a bare
  // `##[error]The operation was canceled.` with nothing to classify.
  //
  // CORRECTION to an earlier version of this comment, which claimed `connectionRetryCount` "was never
  // doing anything useful" for the CPE-1048 slow-cold-start case — that is right about the SLOW-
  // RESPONSE window (a request that connects but takes a while to answer, which
  // `connectionRetryTimeout` alone already covers) but wrong about a DIFFERENT window this repo relies
  // on the default retry count for: `beforeSession` (below) spawns `tauri-driver` and returns
  // immediately, with no readiness check, before WDIO's first `POST /session` to `127.0.0.1:4444`. If
  // that request lands before the child process has bound the port, the result is an immediate
  // `ECONNREFUSED` — a REFUSED connection, not a slow one — which `connectionRetryTimeout` cannot
  // cover at all (it bounds how long a request waits for a response, not whether a connection is
  // accepted in the first place). The retry count was the only thing papering over that race on a
  // slow/contended runner exactly like the ones this ticket's evidence is about, so removing it here
  // without addressing that window would have traded a well-diagnosed hang for a worse-diagnosed
  // flake. Fixed at the actual source instead: `beforeSession` below now polls the port until
  // `tauri-driver` is actually accepting connections (bounded, ~10s budget) before returning, so
  // `connectionRetryCount: 0` no longer needs to cover that race by accident.
  //
  // What this does NOT explain or fix, stated honestly (CPE-1772's own "further evidence" section):
  // three OTHER occurrences in the same investigation were cancelled during `Install Linux system
  // dependencies` (shard 1, PR #924), a plain shard failure with no code signal (shard 3, PR #926),
  // and `Install tauri-driver` in the SHARED BUILD job itself (PR #933) — none of those touch a
  // WebDriver session at all (the build job compiles the app and installs tools; no browser session
  // exists yet), so this fix cannot be the whole story. That pattern looks like the runner pool
  // itself reclaiming/cancelling jobs under contention (six PRs' matrices in flight at once on the
  // night investigated) rather than an in-process hang — gui-smoke.yml's step-level `timeout-minutes`
  // added on the setup steps below turns THAT class into a fast, named failure too where it can, but
  // an externally-cancelled job is not something this workflow file can make retry itself. CPE-1781
  // (same PR) reduces how much of that contention `main`'s own bookkeeping traffic was contributing.
  connectionRetryCount: 0,
  // CPE-1594: "spec" stays for a human-readable log. The machine-readable result file `scripts/run-
  // ratchet.ts` reads as its source of truth USED to come from the built-in "json" reporter — CPE-1866
  // dropped it (see the `afterTest`/`afterHook`/`afterSuite` hooks below, which replace it) because that
  // reporter buffers everything into ONE `ResultSet` per WORKER, and grouping every shard's specs into
  // ONE worker (see this file's `specs` comment) means it can no longer tell which of a worker's several
  // spec files a given suite/test came from — `@wdio/json-reporter`'s own `TestSuite` type
  // (`node_modules/@wdio/json-reporter/build/types.d.ts`) carries no per-suite file identity, only the
  // worker-level `specs: string[]`, so the OLD (pre-grouping) code's own doc comment on
  // `lib/ratchet.ts#RawResultChunk` already flagged the fallback for a multi-spec chunk as "attributing
  // the chunk's suites to every spec path it lists" — exactly wrong once a worker's `specs` genuinely has
  // more than one entry with real, DIFFERENT suites inside. Rather than teach `lib/ratchet.ts` (a
  // heavily-tested, load-bearing gate — 1106 lines of its own tests) to disambiguate a shape its input
  // format cannot actually distinguish, the fix is upstream: write one correctly-scoped
  // `RawResultChunk`-shaped file PER SPEC FILE ourselves, from hooks that already carry `test.file` on
  // every record, so `lib/ratchet.ts` never has to guess.
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    // CPE-1481: already generous (90s per `it`, well under the 35-min job cap raised by this same
    // ticket) — one slow/hung test fails on its own instead of eating the whole job's budget.
    // Checked, not duplicated: this was already the value here before CPE-1481, no change needed.
    //
    // CPE-1702: if you're writing a long-running loop inside a single `it()` (e.g. a stress harness
    // that repeats openSampleFile/waitForPreviewToSettle many times), do NOT reach for `this.timeout()`
    // inside the test body to extend past this 90s ceiling — it is not reliably honoured by
    // @wdio/mocha-framework (see webdriverio/webdriverio#1794) and this repo has real CI evidence of
    // it failing to take effect: CPE-1679's throwaway stress harness called `this.timeout(2_060_000)`
    // and still died at wall-clock ~90.000s on all three of its real runs, every time WDIO's own
    // mocha-timeout teardown (`deleteSession()`) firing mid-loop, not a WebKitGTK/GStreamer leak. Full
    // writeup: this file's README, "The stress-harness 'session death' is `mochaOpts.timeout`, not a
    // WebKitGTK/GStreamer leak (CPE-1702)". Keep any single stress `it()` well under 90s, or split it
    // across multiple `it()`s so each gets a fresh 90s budget. Don't "fix" this by raising the 90s
    // value itself, either — that's the exact CPE-1481 fail-fast guard the comment above this one
    // protects for the real suite (one slow/hung test fails on its own instead of eating the whole
    // job's budget); a long-running scratch harness should size itself under the ceiling or take its
    // own per-spec override, not weaken it for everyone else.
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

    // CPE-1827: seed a dedicated throwaway file (nested inside the CPE-1241 shred folder, same
    // reasoning) and move it to the real OS Trash BEFORE the app launches (see the block above for why
    // it's done natively here rather than by the spec driving the app's own delete UI) — best-effort;
    // the spec checks `trashTitlebarFixture.supported`.
    const trashTitlebarSupported = seedTrashTitlebarFixture(tmpDir);

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
        trashTitlebarFixture: { supported: trashTitlebarSupported },
      }),
      "utf-8",
    );
  },

  // tauri-driver proxies WebDriver requests to the platform's native driver (msedgedriver on
  // Windows, WebKitWebDriver on Linux), which is what actually launches the app binary.
  //
  // CPE-1772: `beforeSession` used to spawn and return immediately, with no readiness check, before
  // WDIO issues its first `POST /session` to `127.0.0.1:4444`. On a slow/contended runner (spawn ->
  // bind can take longer than usual under the runner-pool contention this ticket's evidence is about)
  // that first request can land before the port is open, which is an immediate `ECONNREFUSED` —
  // nothing `connectionRetryTimeout` covers, since that only bounds a slow RESPONSE, not a refused
  // CONNECT. The default `connectionRetryCount` of 3 used to paper over this by accident; now that
  // it's capped at 0 (see that config option's own comment for why), the FIRST `waitForPort` call below
  // is what actually covers that window instead.
  //
  // CPE-1832: that single wait was not the whole race. Real CI evidence AFTER the CPE-1772 fix had
  // already landed (PR #972, run 32434348500, shard 2 — only the first spec failed, every later one in
  // the same job connected fine, and the one configured retry landed inside the same window and failed
  // the same way) showed a DIFFERENT error than a refused connect to tauri-driver's own port: a log line
  // FROM tauri-driver itself, "Error serving connection: hyper::Error(User(Service), client error
  // (Connect) ... Connection refused (os error 111))". Root-caused by reading tauri-driver 2.0.6's own
  // vendored source (local cargo registry cache, `tauri-driver-2.0.6/src/{main,server,cli}.rs`, not
  // guessed): `main.rs` spawns the REAL native driver (WebKitWebDriver / msedgedriver) as a child
  // process and, WITHOUT waiting for it to finish binding its own port, immediately calls
  // `server::run(...)`, which opens tauri-driver's OWN port (`TAURI_DRIVER_PORT`, 4444) and starts
  // accepting connections right away. So tauri-driver's front door can be open — passing the CPE-1772
  // wait — while its back door isn't: `server.rs`'s `forward_to_native_driver` proxies every request to
  // `http://127.0.0.1:${NATIVE_DRIVER_PORT}` (4445 by default, `cli.rs`'s `--native-port` default,
  // which we now pass explicitly below instead of relying on it staying 4445 across a future
  // tauri-driver version), and a request landing before THAT port is open gets exactly the "Connection
  // refused (os error 111)" from the evidence — tauri-driver's own log, not WDIO's, which is why it read
  // differently from the CPE-1772 failure even though both are startup races. The two ports are not
  // simultaneous: 4444 is a bare TCP bind (near-instant); 4445 needs a whole other binary (WebKitWebDriver
  // under Xvfb, or msedgedriver) to actually start and bind, which is measurably slower — so waiting for
  // only the first port closes exactly the race CPE-1772 named and leaves this second one wide open.
  //
  // Fix: wait for BOTH ports, in order, before returning — so WDIO's first `POST /session` (and any
  // retry, which reuses this same beforeSession-spawned process) never reaches tauri-driver until the
  // WHOLE proxy chain, not just its front door, is actually ready. Each wait is a real, bounded TCP
  // poll (see `waitForPort`'s own comment) with a labelled, distinguishing error on timeout — a genuine
  // tauri-driver crash still fails loud and fast, it just no longer gets misread as a slow start.
  beforeSession: async (_config, _capabilities, specs) => {
    const specLabel = specLabelFrom(specs as readonly string[] | undefined);
    logPhase("beforeSession:start", specLabel);
    await startTauriDriver(specLabel);
    // CPE-1866: everything from `beforeSession:start` to here is the driver-process phase — spawning
    // tauri-driver and waiting for both its front door (bare TCP bind) and back door (the native
    // WebKitWebDriver/msedgedriver binary actually starting) to accept connections. NONE of this has
    // launched the app under test yet — that happens next, when WDIO issues its first `POST /session`
    // and the native driver spawns APP_BINARY.
    logPhase("beforeSession:driverReady", specLabel);
  },

  // CPE-1866: fires once per WORKER — since `specs` above now groups a shard's whole assignment into
  // ONE nested array, that means once per SHARD (once per whole run, unsharded), not once per spec file
  // any more. Fires right after WDIO's `POST /session` has returned — i.e. after the native driver has
  // launched the real app process and the session is live. The gap between this and
  // `beforeSession:driverReady` above is the app-launch/session-create phase: the actual
  // Tauri/WebView2/WebKitGTK cold start, not the driver plumbing around it. This is the ONE time that
  // cost is paid for the whole worker now — see `beforeSuite` below for what happens between files
  // instead.
  before: (_capabilities, specs) => {
    logPhase("before:sessionReady", specLabelFrom(specs as readonly string[] | undefined));
  },

  // CPE-1866: the reset trigger, and the per-file result-accumulator's start marker — see
  // `currentSpecFile`'s comment (above `killTauriDriver`) for why this is `beforeTest`/`beforeHook`, not
  // `beforeSuite`/`afterSuite` (the first version of this fix used those; they never fire per file in a
  // grouped worker, so that version's reset silently never ran). Both hooks funnel into the same
  // `handleRunnableStart`: a `test`/hook object's `.file` changing from what was last seen means a NEW
  // spec file's runnables have started, which is "once per spec file" REGARDLESS of whether the new file
  // opens with a `before()` hook (routes through `beforeHook` first) or goes straight to its first `it()`
  // (routes through `beforeTest` first) — covering both shapes this suite's 41 spec files actually use.
  // Skip the reset before the very FIRST file (the app is already at the seeded tmpDir root from
  // `--open=<tmpDir>` or, unsharded, from `before:sessionReady` above — resetting there is redundant
  // work, not a correctness need); run `resetAppState` before every one after that so each file starts
  // from the same known-clean state a fresh relaunch used to provide for free. See
  // `lib/resetAppState.ts`'s header for exactly what "clean" means and why it's safe.
  beforeTest: async (test) => {
    await handleRunnableStart(test.file);
  },
  beforeHook: async (test) => {
    await handleRunnableStart(test.file);
  },

  // CPE-1866: `test.title` is the plain `it()` title (mocha convention — `fullTitle()` is the composed
  // one), matching `known-failing.json`'s own "`test` = the it() title, verbatim" contract, so this needs
  // no translation before it becomes a `RawResultChunk`'s `tests[].name`. `result.skipped`/`.passed` (not
  // a `state` string — the WDIO `Test` object itself carries no such field, see `@wdio/types`'
  // `Frameworks.Suite`/`Test`) are mapped to the three states `toCaseStatus` already recognises.
  // `accumulatorFor` also bumps that file's recorded `end` timestamp on every call.
  afterTest: (test, _context, result) => {
    const acc = accumulatorFor(test.file, test.parent);
    acc.tests.push({ name: test.title, state: result.skipped ? "skipped" : result.passed ? "passed" : "failed" });
  },

  // CPE-1866: fires for EVERY hook (before/beforeEach/after/afterEach), passing or not — only a FAILING
  // one is meaningful to `reduceResultChunks` (it turns a failing hook into a synthetic case; a passing
  // hook is silently dropped there via `if (!hook.error) continue`), so only push when `result.error` is
  // actually set. Harmless to push every hook instead, but this avoids inflating each file's JSON with
  // dozens of no-op entries for `before`/`afterEach` hooks that never do anything spec-visible.
  // `accumulatorFor` still runs unconditionally (before the `error` check) so a PASSING hook still bumps
  // that file's `end` timestamp — otherwise a file whose last runnable is a clean `afterEach` would report
  // an `end` stuck at its last recorded test instead of its true finish time.
  afterHook: (test, _context, result) => {
    const acc = accumulatorFor(test.file, test.parent);
    if (!result.error) return;
    acc.hooks.push({ title: test.title, state: "failed", error: result.error });
  },

  // CPE-1866: fires once per WORKER, after the LAST spec file's last runnable in this worker's whole
  // group has run and BEFORE `afterSession` tears the session down — genuinely once-per-worker is
  // correct here (unlike the old `beforeSuite`/`afterSuite` attempt). Every file EXCEPT the last one has
  // already been flushed incrementally by `handleRunnableStart` the moment the NEXT file started (see
  // `flushFileResult`'s own comment for why: a shard-end-only flush loses every already-completed file's
  // evidence on a mid-shard crash, exactly when the forensic trail matters most) — this is only reachable
  // for the LAST file, which has no "next file" transition to trigger its own flush. Replaces the "json"
  // reporter (see the `reporters` option's own comment for why).
  after: (_result, _capabilities, specs) => {
    logPhase("after:testsDone", specLabelFrom(specs as readonly string[] | undefined));
    if (currentSpecFile !== undefined) flushFileResult(currentSpecFile);
  },

  // Note: afterSession might not run if the session fails to start, so onComplete (below) also
  // kills tauri-driver — same belt-and-braces pattern as the official Tauri WebDriver example.
  afterSession: (_config, _capabilities, specs) => {
    const specLabel = specLabelFrom(specs as readonly string[] | undefined);
    logPhase("afterSession:start", specLabel);
    killTauriDriver();
    // CPE-1866: this fires the instant `.kill()` is called, not when the process has actually exited
    // (SIGTERM delivery + the child's own shutdown are async) — it bounds "how long this hook itself
    // took to run", not the full teardown. Real process-exit timing is in `tauriDriver`'s own "exit"
    // handler above if that ever needs measuring; it isn't part of the worker's own wall-clock budget
    // since WDIO does not wait on it before moving to the next spec file.
    logPhase("afterSession:killIssued", specLabel);
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
