// CPE-1152 — burn down the Agent-Watch **restore-panel + checkpoint-marker** visual residual
// (CPE-1126 / CPE-1151, epic CPE-732's GUI cap; the visual rung of epic CPE-579). Drives the real built
// app to the Agent Watch drawer's Replay tab, seeds a REAL checkpoint through the running app's own
// `checkpoint_create` command, then asserts a checkpoint MARKER pins onto the scrubber, that clicking it
// opens the restore panel with a real revert preview (counts + drift), and that arming the revert echoes
// the drift count — capturing both surfaces as screenshots for the Visual Critic (CPE-1148).
//
// == Why drive the app's real `checkpoint_create` (not a hand-written on-disk store) ==
// A marker needs a CPE-1123 snapshot-store entry, and clicking it needs a VALID manifest + blobs so
// `checkpoint_preview_revert` returns real counts/drift instead of an error. Rather than mirror that
// whole content-addressed store shape by hand in Node (brittle — and a *faked* marker the ticket
// forbids), this spec calls the genuine `checkpoint_create` command through the running webview via
// `window.__TAURI_INTERNALS__.invoke` — the exact runtime `@tauri-apps/api/core`'s `invoke` dispatches
// through (src/lib/invoke.ts → core `invoke`). That writes the real store via the production code path,
// and the frontend's own `checkpoint_list` (AgentTimeline.svelte, CPE-1126) reads it straight back. No
// faked marker, no duplicated on-disk format.
//
// == Identifier-agnostic by construction ==
// Unlike the journal/history fixtures in wdio.conf.ts (which Node-write into the *base*-identifier
// app-data dir), this spec never touches app-data from Node: the running app both writes (create) and
// reads (list) the checkpoint, so it lands in whatever `app_data_dir` THIS binary uses — the base
// `com.example.crossplatformexplorer` OR the sidecar build's `.sidecar` override — with no path
// assumption. The scrubber range comes from live-timeline entries seeded via
// `__CPE_TEST_INGEST_ACTIVITY__` (in-memory, CPE-1135), so this spec is also fully independent of
// seedReplayFixture's on-disk journal+baseline — it exercises only the checkpoint marker/restore layer.
//
// == The two `root` strings must be byte-identical ==
// `checkpoint_create(root)` and the frontend's `checkpoint_list(currentPath)` both key the per-root
// store by `sha256(root)` (checkpoint_store::root_key); they only agree if the two `root` strings match
// exactly. `App.svelte` sets `currentPath = navigate(window.__CPE_OPEN_DIR__)` and `navigate` stores its
// argument verbatim in the tab history, so the checkpoint is created with that SAME
// `window.__CPE_OPEN_DIR__` value — guaranteeing the frontend finds it (no slash/case/trailing-slash
// drift, which a raw tmpDir string from the runner process could introduce).
//
// == Marker lands in range ==
// `checkpoint_create`'s recorded `ts` is "now"; the two seeded live entries are placed at `ts ± 30s`, so
// the scrubber's `[firstAt, lastAt]` brackets the checkpoint and the marker pins mid-track with
// `inRange: true` (not clamped to an edge) — exactly the on-track marker the ticket asks for.
import { expect } from "chai";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literals rather than importing from wdio.conf.ts — matches this harness's convention of not
// reaching across the runner/worker process boundary (see open-dir.smoke.ts's FIXTURE_NAME comment).
const REPLAY_SESSION_ID = "gui-smoke-replay";
const CHECKPOINT_LABEL = "before smoke edits";
/** A file created AFTER the checkpoint so the revert preview reports it as drift (a "delete" action that
 *  can't be attributed to the watched session → surfaced in `drift_paths`). */
const DRIFT_NAME = "CPE-1152-drift.txt";
/** A synthetic activity path for the two seeded live-timeline entries (never read off disk). */
const ACTIVITY_NAME = "CPE-1152-activity.txt";

/** Result of the in-webview `checkpoint_create` invoke, marshalled back through `executeAsync`. */
type CreateResult =
  | { ok: true; root: string; manifestId: string; ts: number }
  | { ok: false; error: string };

/** Candidate app-data dirs to clean the per-root checkpoint store from — both the base identifier and the
 *  sidecar build's `.sidecar` override, since this spec runs against whichever binary is present. Mirrors
 *  wdio.conf.ts#resolveAppDataDir; the win32 branch is what CI exercises. */
function candidateAppDataDirs(): string[] {
  const conf = JSON.parse(
    fs.readFileSync(path.resolve(__dirname, "..", "..", "src-tauri", "tauri.conf.json"), "utf-8"),
  ) as { identifier: string };
  const ids = [conf.identifier, `${conf.identifier}.sidecar`];
  const root =
    process.platform === "win32"
      ? (process.env.APPDATA ?? path.join(os.homedir(), "AppData", "Roaming"))
      : process.platform === "darwin"
        ? path.join(os.homedir(), "Library", "Application Support")
        : (process.env.XDG_DATA_HOME ?? path.join(os.homedir(), ".local", "share"));
  return ids.map((id) => path.join(root, id));
}

describe("CPE-1152 — headless GUI smoke: checkpoint marker + restore panel render for the Visual Critic", () => {
  let tmpDir = "";
  let createdRoot = "";

  before(() => {
    ({ tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string });
  });

  // CPE-1149 discipline: on a failing run leave a shot of the failure state; the inline snaps below are
  // only reached on a pass. Non-arrow fn so Mocha binds `this`.
  afterEach(async function () {
    await snapFailure(this.currentTest, "checkpoint-restore-panel");
  });

  after(() => {
    // Best-effort teardown, mirroring wdio.conf.ts's restore discipline. On CI's ephemeral runner this is
    // cosmetic; locally it keeps a real developer's app-data free of this run's throwaway checkpoint store
    // (keyed by sha256(root), so it can never collide with a real one) and of the staged drift file (which
    // lives in the shared tmpDir onComplete rm-rf's — removed here too so a later spec in the same tmpDir
    // isn't perturbed).
    if (createdRoot) {
      const key = crypto.createHash("sha256").update(createdRoot).digest("hex");
      for (const dir of candidateAppDataDirs()) {
        try {
          fs.rmSync(path.join(dir, "checkpoints", key), { recursive: true, force: true });
        } catch {
          // best effort
        }
      }
    }
    if (tmpDir) {
      try {
        fs.rmSync(path.join(tmpDir, DRIFT_NAME), { force: true });
      } catch {
        // best effort
      }
    }
  });

  it("pins a checkpoint marker, opens the restore panel with counts + drift, and echoes drift on arm", async () => {
    // 1) Wait for the initial `--open=<tmpDir>` navigation (also asserted in open-dir.smoke.ts) so
    //    `currentPath` — and therefore `window.__CPE_OPEN_DIR__` — is settled before we key a store to it.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    // 1b) Seed the drift file's ORIGINAL content BEFORE the checkpoint so the checkpoint captures it.
    //     After the checkpoint we overwrite it (step 3), making the revert an OVERWRITE of newer work —
    //     the canonical drift case the confirm echo warns about ("… will be lost").
    const driftPath = path.join(tmpDir, DRIFT_NAME);
    fs.writeFileSync(driftPath, "original content — captured by the checkpoint\n", "utf-8");

    // 2) Create a REAL checkpoint through the running app's own `checkpoint_create` command, keyed by the
    //    EXACT path string the frontend's `checkpoint_list(currentPath)` will use (`window.__CPE_OPEN_DIR__`
    //    — see the header comment). `executeAsync` (not `execute`) because the invoke is a promise.
    const created = await browser.executeAsync<CreateResult, [string]>(
      (label, done) => {
        const w = window as unknown as {
          __CPE_OPEN_DIR__?: string;
          __TAURI_INTERNALS__?: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
        };
        const root = w.__CPE_OPEN_DIR__;
        if (!root) {
          done({ ok: false, error: "window.__CPE_OPEN_DIR__ missing — was the app launched with --open?" });
          return;
        }
        const internals = w.__TAURI_INTERNALS__;
        if (!internals || typeof internals.invoke !== "function") {
          done({ ok: false, error: "window.__TAURI_INTERNALS__.invoke missing — not a Tauri webview?" });
          return;
        }
        internals
          .invoke("checkpoint_create", { root, label })
          .then((res) => {
            const cp = (res as { checkpoint: { manifest_id: string; ts: number } }).checkpoint;
            done({ ok: true, root, manifestId: cp.manifest_id, ts: cp.ts });
          })
          .catch((e: unknown) => done({ ok: false, error: e instanceof Error ? e.message : String(e) }));
      },
      CHECKPOINT_LABEL,
    );

    expect(created.ok, `checkpoint_create failed: ${created.ok ? "" : created.error}`).to.equal(true);
    if (!created.ok) return; // narrows the union for TS; the expect above already failed the test
    createdRoot = created.root;
    const { manifestId } = created;
    const checkpointTs = created.ts;
    expect(manifestId, "checkpoint_create returned a manifest id").to.be.a("string").and.not.equal("");

    // 3) Stage drift: OVERWRITE the captured file with newer content AFTER the checkpoint. The preview's
    //    fresh scan_dir sees it diverging from the checkpoint (an OVERWRITE on revert) and — since the
    //    change isn't attributable to the watched session — surfaces it as drift. Runner-side Node fs write
    //    into the same physical folder the app opened (path-string form is irrelevant to a same-dir write).
    fs.writeFileSync(driftPath, "CHANGED after the checkpoint — newer work that a revert would overwrite\n", "utf-8");

    // 4) Seed the watched session (started) — same wire shape a real sidecar emits over `ai-console://session`
    //    (agentSessions.ts#ingestSessionState). Its `cwd` matches the opened folder so `.agent-log-btn`
    //    arms and the drawer becomes reachable (identical to replay.smoke.ts / cost-history.smoke.ts).
    await browser.execute(
      (cwd: string, sessionId: string) => {
        const hook = (window as unknown as { __CPE_TEST_INGEST_SESSION__?: (s: string) => void })
          .__CPE_TEST_INGEST_SESSION__;
        if (!hook) throw new Error("window.__CPE_TEST_INGEST_SESSION__ is missing — is the app in --test-mode?");
        hook(
          `session:${JSON.stringify({
            event: "started",
            sessionId,
            agentId: "claude",
            agentName: "Claude Code",
            provider: "openrouter",
            model: "sonnet",
            cwd,
          })}`,
        );
      },
      tmpDir,
      REPLAY_SESSION_ID,
    );

    // 5) Seed two LIVE timeline entries BRACKETING the checkpoint's ts (ts-30s, ts+30s) so `sliderRange`
    //    (agentReplay.ts) is a non-degenerate window that CONTAINS the checkpoint — the marker then pins
    //    mid-track with `inRange: true` rather than clamped to an edge. Two separate calls so the entries
    //    land distinct `at` timestamps (not one batch-wide `now`).
    const activityPath = path.join(tmpDir, ACTIVITY_NAME);
    for (const [kind, at] of [
      ["created", checkpointTs - 30_000],
      ["modified", checkpointTs + 30_000],
    ] as const) {
      await browser.execute(
        (payload: string, ts: number) => {
          const hook = (
            window as unknown as { __CPE_TEST_INGEST_ACTIVITY__?: (p: string, at?: number) => void }
          ).__CPE_TEST_INGEST_ACTIVITY__;
          if (!hook) throw new Error("window.__CPE_TEST_INGEST_ACTIVITY__ is missing — is the app in --test-mode?");
          hook(payload, ts);
        },
        JSON.stringify([{ kind, path: activityPath, actor: REPLAY_SESSION_ID }]),
        at,
      );
    }

    // 6) Open the Agent Watch drawer and switch to the Replay tab (same drawer/tab dance as replay.smoke.ts).
    const openBtn = await $(".agent-log-btn");
    await openBtn.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .agent-log-btn to appear once the watched session is active",
    });
    await openBtn.waitForClickable({ timeout: 10_000 });
    await openBtn.click();

    await browser.waitUntil(async () => (await $$(".tl-tabbar .tab").length) === 5, {
      timeout: 10_000,
      timeoutMsg: "expected the Agent Watch drawer's 5-tab strip to render",
    });

    const tabs = $$(".tl-tabbar .tab");
    let replayTab: WebdriverIO.Element | undefined;
    for await (const tab of tabs) {
      const html = await tab.getHTML({ includeSelectorTag: false });
      if (html.includes("Replay")) {
        replayTab = tab;
        break;
      }
    }
    expect(replayTab, 'expected a tab in .tl-tabbar labelled "Replay"').to.not.equal(undefined);
    await replayTab!.waitForClickable({ timeout: 10_000 });
    await replayTab!.click();

    // 7) Core assertion: the checkpoint MARKER rendered — `checkpoint_list(currentPath)` read back the store
    //    we just wrote and `checkpointMarkers(range, checkpoints)` mapped it onto the track. If the root
    //    strings had drifted, or the store weren't a real one, this marker would never appear.
    const markerSel = `[data-testid="checkpoint-marker-${manifestId}"]`;
    await browser.waitUntil(async () => (await $(markerSel)).isExisting(), {
      timeout: 15_000,
      timeoutMsg: "expected a checkpoint marker to pin onto the scrubber from the seeded checkpoint",
    });
    const marker = await $(markerSel);
    expect(await marker.isExisting(), "expected the checkpoint marker to render").to.equal(true);
    // In range (bracketed by the seeded entries) → NOT the edge-clamped `.out` variant.
    const markerClass = (await marker.getAttribute("class")) ?? "";
    expect(markerClass, "expected the marker to be in-range (not clamped to a track edge)").to.not.include("out");

    // 8) Click the marker → the restore panel opens and its revert preview resolves against the real store.
    await marker.click();

    const panel = await $('[data-testid="checkpoint-restore-panel"]');
    await panel.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the restore panel to open when the marker is clicked",
    });
    expect(await panel.isExisting(), "expected checkpoint-restore-panel to render").to.equal(true);

    // The preview counts render once `checkpoint_preview_revert` resolves (proves a VALID manifest, not an
    // error state — the panel would otherwise show `checkpoint-preview-error`).
    const counts = await $('[data-testid="checkpoint-counts"]');
    await counts.waitForExist({
      timeout: 15_000,
      timeoutMsg: "expected checkpoint-counts to render once the real revert preview resolved",
    });
    expect(await counts.isExisting(), "expected the revert-preview counts to render").to.equal(true);

    // The staged drift file must surface as drift — the whole point of the drift echo the Visual Critic
    // needs to see. (A load error would have left `checkpoint-preview-error` and no drift warning.)
    const driftWarn = await $('[data-testid="checkpoint-drift-warning"]');
    await driftWarn.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected a drift warning from the file staged after the checkpoint",
    });
    expect(await driftWarn.isExisting(), "expected the drift warning to render").to.equal(true);

    // 9) Capture the restore panel (counts + drift) for the Visual Critic — AFTER the assertions above.
    await snap("checkpoint-restore-panel");

    // 10) Arm the revert (first click of the two-step confirm) so the confirm panel — and its drift echo
    //     (CPE-1151) — render. We deliberately do NOT click "Yes, revert": the screenshot is the goal, and
    //     leaving the tree untouched keeps the run side-effect-free.
    const armBtn = await $('[data-testid="checkpoint-revert-btn"]');
    await armBtn.waitForClickable({ timeout: 10_000 });
    await armBtn.click();

    const confirm = await $('[data-testid="checkpoint-confirm-revert"]');
    await confirm.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the two-step confirm panel to appear after arming the revert",
    });
    expect(await confirm.isExisting(), "expected checkpoint-confirm-revert to render").to.equal(true);

    // The CPE-1151 drift echo inside the confirm (only shown when the preview reported drift).
    const confirmDrift = await $('[data-testid="checkpoint-confirm-drift"]');
    await confirmDrift.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the confirm panel to echo the drift count (CPE-1151)",
    });
    expect(await confirmDrift.isExisting(), "expected checkpoint-confirm-drift to render").to.equal(true);

    // 11) Capture the armed confirm (with the drift echo) — the second Visual-Critic surface.
    await snap("checkpoint-revert-confirm");
  });
});
