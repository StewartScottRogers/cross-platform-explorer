<script lang="ts">
  /**
   * Agent Watch session activity timeline (CPE-400) — a durable, scrollable history of every
   * filesystem action the agent took this session, newest first. The transient strip (CPE-399)
   * shows the last few fading changes; this is the full log for review. Clicking an entry
   * navigates the explorer to the change's containing folder.
   */
  import { createEventDispatcher, onDestroy } from "svelte";
  import Icon from "./Icon.svelte";
  import DiffPeek from "./DiffPeek.svelte";
  import DiffSideBySide from "./DiffSideBySide.svelte";
  import ConsultedFiles from "./ConsultedFiles.svelte";
  import type { TimelineEntry } from "../agentActivity";
  import type { AgentSession } from "../sidecar";
  import { agentDiffs, diffFor, diffLineStats } from "../agentDiffs";
  import { agentCost, formatTokens, formatUsd } from "../agentCost";
  import {
    agentSessionMetrics,
    emptySessionAccumulator,
    deriveSessionMetrics,
    formatBytes,
    formatDuration,
    formatPerMinute,
  } from "../agentSessionMetrics";
  import {
    sliderRange,
    sliderFraction,
    entriesUpTo,
    currentEntry,
    nextTimestamp,
    prevTimestamp,
    isMultiplyEdited,
    isWriteKind,
    cadenceForSpeed,
    checkpointMarkers,
  } from "../agentReplay";
  import { foldOverlaps, overlapHasUnknown, friendlyActor, relativeLabel } from "../agentConflicts";
  import { foldRenameConflicts, renameConflictNote } from "../agentRenameConflicts";
  import { commands } from "../bindings.gen";
  import type {
    ReplayData,
    Baseline,
    ReplayEntry as ReplayRow,
    SessionMetricsRecord,
    Checkpoint,
    RevertPreview,
    RevertOutcome,
  } from "../bindings.gen";
  import type { DirEntry } from "../types";
  import { stateAtFrom, childrenAt, type FsState } from "../replayFold";
  import { rollup, overTime } from "../agentMetricsRollup";
  import { resolveOverlay, type ReplaySource } from "../replayOverlay";

  export let entries: TimelineEntry[] = [];
  export let agentName = "agent";
  /** sessionId of the agent currently being watched, if any (CPE-1098) — lets the cost tab flag
   *  which reporting session is the one on screen when several sessions report usage. */
  export let sessionId = "";
  /** Currently-running agent sessions (CPE-1100) — joined against an overlap's actor ids so the
   *  Radar tab can show an agent's name instead of a bare sessionId. Empty is fine (falls back to a
   *  shortened id); this component never fetches sessions itself. */
  export let sessions: AgentSession[] = [];
  /** The folder currently navigated in the explorer (CPE-1111, epic CPE-728 slice d) — the root the
   *  Replay tab reconstructs a listing for via `childrenAt`. Empty means the reconstructed root. This
   *  component never navigates on its own; it only reads this prop. */
  export let currentPath = "";

  const dispatch = createEventDispatcher<{
    navigate: string;
    close: void;
    /** CPE-1112 (epic CPE-728 slice e): the read-only reconstructed listing to show in the MAIN file
     *  pane instead of its live listing, or `null` to show the live listing. Fired on every change to
     *  Replay mode's own toggle, the scrub position, the reconstructed folder, or the loaded session —
     *  purely derived via `replayOverlay.ts`'s `resolveOverlay`, never imperative. The listener (App)
     *  owns forwarding this straight into `ExplorerPane`'s `replayOverlay` prop. */
    replayOverlay: DirEntry[] | null;
  }>();

  /** Which entry's before/after peek is currently revealed (hover/focus), or null (CPE-745). */
  let openId: number | null = null;
  /** The write whose full side-by-side diff is open in the modal, or null (CPE-746). */
  let sbs: { path: string; before: string; after: string } | null = null;
  /** A write (created/modified) can carry a captured before/after diff; reads/renames/removes don't. */
  const isWrite = isWriteKind;

  const KIND_LABEL: Record<TimelineEntry["kind"], string> = {
    created: "new",
    modified: "edited",
    removed: "deleted",
    renamed: "moved",
    read: "read", // CPE-405: consulted, not changed — a dimmer, distinct signal
  };
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "");
  const baseOf = (p: string) => norm(p).split("/").pop() || p;
  const dirOf = (p: string) => {
    const n = norm(p);
    const i = n.lastIndexOf("/");
    return i > 0 ? n.slice(0, i) : "";
  };
  const clock = (at: number) => new Date(at).toLocaleTimeString();

  // ---------- Replay tab (CPE-1094) / Cost tab (CPE-1098) / Radar tab (CPE-1100) / History tab (CPE-1114) ----------
  /** "Live" (default, today's list), "Replay" (scrub through the session's history), "Cost" (live
   *  per-session token/USD usage), "Radar" (activity-overlap signal across distinct actors), or
   *  "History" (cross-session rollup from the persisted metrics journal). */
  let tab: "live" | "replay" | "cost" | "radar" | "history" = "live";

  // ---------- Radar tab (CPE-1100) ----------
  /** Paths touched by ≥2 distinct actors within the overlap window, most-recent first (pure fold
   *  over `entries` — no new listener/timer; see agentConflicts.ts for the hedged wording rationale). */
  $: overlaps = foldOverlaps(entries);
  /** Competing renames (CPE-1118) — same-`from`/diff-`to` divergences and diff-`from`/same-`to`
   *  collisions across distinct actors, folded from the same `entries` (no new listener/timer; see
   *  agentRenameConflicts.ts, which mirrors the sidecar's conflict_rename.rs). */
  $: renameConflicts = foldRenameConflicts(entries);

  // ---------- Cost ledger tab (CPE-1098 tokens+cost; CPE-1107 fuller per-session metrics) ----------
  /** Reporting/active sessions, current-watched one first (if present), then the rest sorted by id for
   *  a stable order. The union of `agentCost` (has reported usage) and `agentSessionMetrics` (has
   *  file/edit/churn/wall-clock activity) so a session that's touched files before its first cost
   *  report still shows up. Advisory — best-effort figures, never billing. */
  $: costList = (() => {
    const ids = new Set<string>([...Object.keys($agentCost), ...Object.keys($agentSessionMetrics)]);
    const all = [...ids];
    const mine = all.filter((id) => id === sessionId);
    const others = all.filter((id) => id !== sessionId).sort((a, b) => a.localeCompare(b));
    return [...mine, ...others].map((id) =>
      deriveSessionMetrics($agentSessionMetrics[id] ?? emptySessionAccumulator(id), $agentCost[id]),
    );
  })();

  // ---------- History tab (CPE-1114, epic CPE-731 slice c — final slice) ----------
  // Cross-session rollup of the persisted metrics journal (CPE-1113's `commands.metricsHistory()`),
  // separate from the live per-session Cost tab above. Session-independent, so it's read PULL-ONLY:
  // once, the first time this tab is opened this mount — no listener, no timer, no polling. The whole
  // drawer is destroyed/recreated each time it's closed/reopened (App.svelte's `{#if showTimeline}`),
  // so "on open" here is simply "on first History-tab visit this mount".
  /** Raw rows from the journal, or `null` before the first load this mount. */
  let historyRecords: SessionMetricsRecord[] | null = null;
  let historyLoading = false;
  let historyError = "";
  /** Generation token: guards a slow load from clobbering a result if the drawer/component is torn
   *  down mid-flight (mirrors `replayGen`). */
  let historyGen = 0;
  /** Which series the over-time bars show. */
  let historyMetric: "cost" | "tokens" = "cost";

  async function loadHistory() {
    const g = ++historyGen;
    historyLoading = true;
    historyError = "";
    try {
      const res = await commands.metricsHistory();
      if (g !== historyGen) return; // superseded
      if (res.status === "ok") {
        historyRecords = res.data;
      } else {
        historyError = String(res.error);
      }
    } catch (e) {
      if (g !== historyGen) return;
      historyError = e instanceof Error ? e.message : String(e);
    } finally {
      if (g === historyGen) historyLoading = false;
    }
  }

  // Load once on History-tab enter — never while the tab is closed, never on a timer. A load error is
  // left alone (no retry storm); reopening the drawer (a fresh mount) tries again.
  $: if (tab === "history" && historyRecords === null && !historyLoading && !historyError) {
    loadHistory();
  }

  $: historyRollup = historyRecords ? rollup(historyRecords) : null;
  $: historyOverTime = historyRecords ? overTime(historyRecords, "day") : [];
  $: historyMax = historyOverTime.reduce(
    (m, p) => Math.max(m, historyMetric === "cost" ? p.costUsd : p.totalTokens),
    0,
  );

  /** `part` as a percentage of `total`, division-safe — "—" rather than a bogus/NaN percentage when
   *  `total` is 0 (e.g. every recorded session cost exactly $0). */
  function historyShare(part: number, total: number): string {
    if (!Number.isFinite(part) || !Number.isFinite(total) || total <= 0) return "—";
    return `${((part / total) * 100).toFixed(1)}%`;
  }

  const historyBarDate = (bucketStart: number): string => new Date(bucketStart).toLocaleDateString();

  /** Selected scrub position — an epoch ms timestamp somewhere in `[range.firstAt, range.lastAt]`. */
  let t = 0;
  let playing = false;
  let playTimer: ReturnType<typeof setInterval> | null = null;

  /** Base cadence at 1× — how often play advances to the next entry (ticket: ~1 entry / 400ms). */
  const PLAY_INTERVAL_MS = 400;

  /** Playback speed multiplier (CPE-1104) — one of `SPEEDS`, default 1×. */
  let speed = 1;
  const SPEEDS = [0.5, 1, 2, 4] as const;

  const stopPlaying = () => {
    playing = false;
    if (playTimer !== null) {
      clearInterval(playTimer);
      playTimer = null;
    }
  };

  const resetReplay = () => {
    stopPlaying();
    t = 0;
    // Belt-and-braces alongside the sessionId-keyed invalidation below: entries emptying (watch
    // stopped/cleared) must never leave a stale reconstruction on screen.
    replayGen += 1;
    replayData = null;
    replayBaseline = null;
    replayLoadError = "";
    replayLoading = false;
    // CPE-1112: a fresh/cleared session must never inherit Replay mode being on from a prior one —
    // setting this triggers `replayOverlayActive`'s reactive recompute, which dispatches `null` and
    // restores the main pane's live listing on its own (no separate cleanup call needed).
    replayMode = false;
    // CPE-1126: never carry another agent/session's checkpoint markers or restore panel over.
    clearCheckpoints();
  };

  // Reset local scrub/play state whenever the timeline empties (stop-watching, CPE-400's
  // clearActivity) or a different agent is now being watched — never let a dangling interval or a
  // stale `t` bleed into the next session (AGENT-WATCH.md "off means off").
  let lastAgentName = agentName;
  $: if (entries.length === 0 || agentName !== lastAgentName) {
    lastAgentName = agentName;
    resetReplay();
  }

  // Also clear the interval outright when the component itself is torn down (drawer closed) — and
  // explicitly restore the main pane (CPE-1112): the drawer can be closed mid-scrub with the overlay
  // showing, and once this component is gone no further reactive statement will fire to send the
  // restoring `null` on its own, so it must be sent here as the final, explicit step.
  onDestroy(() => {
    stopPlaying();
    dispatch("replayOverlay", null);
    // CPE-1126: the drawer can be closed with a checkpoint restore panel open — invalidate any
    // in-flight checkpoint/preview load so it can't resolve into a torn-down component.
    clearCheckpoints();
  });

  $: range = sliderRange(entries);
  // Snap `t` into range whenever it falls outside the current span — covers both the initial
  // t===0 default (jumps to the end, showing the fullest replay) and the range shrinking/shifting
  // under a live-growing timeline.
  $: if (range && (t < range.firstAt || t > range.lastAt)) t = range.lastAt;

  $: replayFrozen = range ? entriesUpTo(entries, t) : [];
  $: replayCurrent = range ? currentEntry(entries, t) : null;
  $: replayDiff = replayCurrent && isWrite(replayCurrent.kind) ? diffFor($agentDiffs, replayCurrent.path) : null;
  $: replayMultiplyEdited = replayCurrent ? isMultiplyEdited(entries, replayCurrent.path) : false;
  $: atEnd = !!range && t >= range.lastAt;
  $: atStart = !!range && t <= range.firstAt;

  // ---------- Reconstructed folder listing (CPE-1111, epic CPE-728 slice d) ----------
  // `replay_load` (CPE-1110) ships the session's durable audit journal + baseline once; scrubbing then
  // re-derives "what did this folder look like at time t" entirely client-side via the pure
  // `replayFold.ts` fold (no per-tick IPC). PULL-ONLY: the load below only ever runs while the Replay
  // tab is open and a session is being watched — nothing runs while the tab/drawer is closed.
  let replayData: ReplayData | null = null;
  let replayBaseline: Baseline | null = null;
  let replayLoading = false;
  let replayLoadError = "";
  /** Generation token: bumped whenever `sessionId` changes so a slow load for a since-superseded
   *  session can never clobber a newer one's result. */
  let replayGen = 0;
  let lastReplaySessionId = "";

  // A session change (new watch, or watch stopped → sessionId cleared) invalidates any in-flight load
  // and drops the stale reconstruction — never show session A's folder under session B's scrubber.
  $: if (sessionId !== lastReplaySessionId) {
    lastReplaySessionId = sessionId;
    replayGen += 1;
    replayData = null;
    replayBaseline = null;
    replayLoadError = "";
    replayLoading = false;
  }

  async function loadReplayData(session: string) {
    const g = ++replayGen;
    replayLoading = true;
    replayLoadError = "";
    try {
      const res = await commands.replayLoad(session);
      if (g !== replayGen) return; // superseded by a newer session/tab-enter
      if (res.status === "ok") {
        replayData = res.data.replay;
        replayBaseline = res.data.baseline;
      } else {
        replayLoadError = String(res.error);
      }
    } catch (e) {
      if (g !== replayGen) return;
      replayLoadError = e instanceof Error ? e.message : String(e);
    } finally {
      if (g === replayGen) replayLoading = false;
    }
  }

  // Load on Replay-tab enter (pull-only, once per session) — never while the tab is closed, never on a
  // timer. A prior load error is left alone (no retry storm); a new session clears it above and tries
  // again on next tab-enter.
  $: if (tab === "replay" && sessionId && !replayData && !replayLoading && !replayLoadError) {
    loadReplayData(sessionId);
  }

  /** Split `path` into normalized `/`-segments — mirrors `replayFold.ts`'s internal `segments()` so a
   *  path compares identically here regardless of which separator produced it. Kept local (a tiny
   *  presentation-only helper) rather than exported from `replayFold.ts`, which stays untouched. */
  const replaySegs = (p: string) => p.replace(/\\/g, "/").split("/").filter((s) => s.length > 0);

  /** Every normalized path (as a joined segment string) that has at least one deeper entry in `state` —
   *  i.e. a reconstructed directory. Best-effort: `ReplayEntry` carries no `is_dir` field, so a
   *  currently-empty directory (nothing visible inside it at this scrub moment) can't be distinguished
   *  from a file this way; this mirrors "does this folder show anything inside it right now". */
  function replayDirSet(state: FsState): Set<string> {
    const dirs = new Set<string>();
    for (const path of state.keys()) {
      const segs = replaySegs(path);
      for (let i = 1; i < segs.length; i++) dirs.add(segs.slice(0, i).join("/"));
    }
    return dirs;
  }

  /** The reconstructed live file-set at the current scrub time `t`, seeded from the baseline — pure,
   *  local, no IPC; re-derives on every tick. */
  $: replayState = replayData ? stateAtFrom(replayBaseline, replayData.events, t) : null;
  $: replayDirs = replayState ? replayDirSet(replayState) : new Set<string>();
  /** The reconstructed folder listing for `currentPath` at time `t`, each entry flagged dir/file. */
  $: replayListing = replayState
    ? childrenAt(replayState, currentPath).map((e: ReplayRow) => ({
        ...e,
        isDir: replayDirs.has(replaySegs(e.path).join("/")),
      }))
    : [];

  /** Human label for a reconstructed entry's last-touch kind — extends `KIND_LABEL` with `"baseline"`
   *  (a pre-existing entry the fold seeded from the baseline snapshot, never itself an agent action). */
  const replayKindLabel = (k: string): string =>
    k === "baseline" ? "existing" : (KIND_LABEL as Record<string, string>)[k] ?? k;

  // ---------- Replay-mode file-pane overlay (CPE-1112, epic CPE-728 slice e) ----------
  // "Replay mode": an explicit toggle (off by default) that additionally mirrors the SAME
  // reconstruction already computed above (`replayState`, keyed by `currentPath`) into the MAIN
  // explorer file pane, via a `replayOverlay` event the parent (App) forwards straight into
  // `ExplorerPane`'s `replayOverlay` prop. The in-drawer listing above (`replayListing`) is completely
  // untouched by any of this and stays the always-available fallback (CPE-1111) — this is purely
  // additive. Gating goes through `replayOverlay.ts#resolveOverlay`, a pure function, so "off-means-off"
  // and "restore-on-exit" are true by construction: whenever `replayOverlayActive` goes false (the
  // toggle flips off, OR the user leaves the Replay tab) the very next reactive recompute dispatches
  // `null` and the main pane's live listing reappears — there is no imperative teardown to forget.
  let replayMode = false;

  /** Active only while BOTH the toggle is on AND the Replay tab is the one on screen — tabbing away
   *  (without touching the toggle) restores the live pane exactly like switching the toggle off. */
  $: replayOverlayActive = replayMode && tab === "replay";
  $: replaySource = replayData
    ? ({ baseline: replayBaseline, events: replayData.events } as ReplaySource)
    : null;
  $: dispatch("replayOverlay", resolveOverlay(replayOverlayActive, replaySource, t, currentPath));

  // ---------- Checkpoint markers + restore panel (CPE-1126, epic CPE-732 GUI cap) ----------
  // The visual restore layer over the CPE-1123/1125 command surface: checkpoints captured for the
  // watched folder appear as pins on the Replay scrubber, and selecting one opens a compact restore
  // panel (revert plan + drift warning + a two-step confirm, mirroring CheckpointDialog's safety). The
  // checkpoints belong to a FOLDER (the drawer's `currentPath`), not to the session, so they're keyed
  // by path. PULL-ONLY: loaded once on Replay-tab enter (and again if the folder changes while the tab
  // is open), never on a timer, never while the tab/drawer is closed. off-means-off (AGENT-WATCH.md):
  // markers, panel, and loaded list all clear when the tab is left, the session/agent changes, or the
  // drawer is destroyed.
  let checkpoints: Checkpoint[] = [];
  let checkpointLoading = false;
  let checkpointError = "";
  /** Generation token — a slow list for a since-superseded folder/tab can't clobber a newer result. */
  let checkpointGen = 0;
  let lastCheckpointPath = "";

  /** The checkpoint whose restore plan is open in the panel below the scrubber, or null. */
  let selectedCheckpoint: Checkpoint | null = null;
  let revertPreview: RevertPreview | null = null;
  let revertPreviewLoading = false;
  let revertPreviewError = "";
  let revertPreviewGen = 0;
  /** True once "Revert to this checkpoint…" is armed and awaiting the confirm panel's second click. */
  let cpConfirming = false;
  let reverting = false;
  let revertOutcome: RevertOutcome | null = null;
  let revertError = "";

  $: cpMarkers = checkpointMarkers(range, checkpoints);

  /** Clear just the restore panel (selection + preview + confirm + outcome) — the loaded list stays. */
  function clearCheckpointRestore() {
    revertPreviewGen += 1; // invalidate any in-flight preview
    selectedCheckpoint = null;
    revertPreview = null;
    revertPreviewLoading = false;
    revertPreviewError = "";
    cpConfirming = false;
    reverting = false;
    revertOutcome = null;
    revertError = "";
  }

  /** Full teardown of the checkpoint layer — off-means-off. Invalidates in-flight loads too. */
  function clearCheckpoints() {
    checkpointGen += 1;
    checkpoints = [];
    checkpointError = "";
    checkpointLoading = false;
    lastCheckpointPath = "";
    clearCheckpointRestore();
  }

  async function loadCheckpoints(root: string) {
    const g = ++checkpointGen;
    checkpointLoading = true;
    checkpointError = "";
    try {
      const res = await commands.checkpointList(root);
      if (g !== checkpointGen) return; // superseded by a newer folder/tab-enter/teardown
      if (res.status === "ok") {
        checkpoints = Array.isArray(res.data) ? res.data : []; // never let a null/odd payload crash markers
      } else {
        checkpointError = String(res.error);
      }
    } catch (e) {
      if (g !== checkpointGen) return;
      checkpointError = e instanceof Error ? e.message : String(e);
    } finally {
      if (g === checkpointGen) checkpointLoading = false;
    }
  }

  // Pull-only load: on Replay-tab enter with a folder set, or when the folder changes while the tab is
  // open. A prior load error is left as-is (no retry storm); re-entering the tab / changing folder
  // re-tries. Mirrors the `lastReplaySessionId` guard pattern — the `!==` guard prevents a re-run loop.
  $: if (tab === "replay" && currentPath && currentPath !== lastCheckpointPath) {
    lastCheckpointPath = currentPath;
    clearCheckpointRestore();
    loadCheckpoints(currentPath);
  }

  // off-means-off: leaving the Replay tab tears the whole checkpoint layer down (markers + panel +
  // loaded list), exactly like the drawer closing. Guarded so it only fires once per exit.
  $: if (tab !== "replay" && (checkpoints.length || lastCheckpointPath || selectedCheckpoint)) {
    clearCheckpoints();
  }

  async function loadRevertPreview(cp: Checkpoint) {
    if (!currentPath) return;
    const g = ++revertPreviewGen;
    revertPreviewLoading = true;
    revertPreviewError = "";
    revertPreview = null;
    try {
      const res = await commands.checkpointPreviewRevert(currentPath, cp.manifest_id);
      if (g !== revertPreviewGen) return;
      if (res.status === "ok") {
        revertPreview = res.data;
      } else {
        revertPreviewError = String(res.error);
      }
    } catch (e) {
      if (g !== revertPreviewGen) return;
      revertPreviewError = e instanceof Error ? e.message : String(e);
    } finally {
      if (g === revertPreviewGen) revertPreviewLoading = false;
    }
  }

  /** Click a marker: stop playback, jump the scrubber to the checkpoint's moment, and open its
   *  restore panel (loading the revert preview). Out-of-range `ts` is snapped back into the track by
   *  the existing range-clamp reactive above — the panel still shows the true checkpoint either way. */
  function selectCheckpoint(cp: Checkpoint) {
    stopPlaying();
    t = cp.ts;
    selectedCheckpoint = cp;
    cpConfirming = false;
    revertOutcome = null;
    revertError = "";
    loadRevertPreview(cp);
  }

  const armCheckpointRevert = () => { cpConfirming = true; };
  const cancelCheckpointRevert = () => { cpConfirming = false; };

  async function doCheckpointRevert() {
    if (!selectedCheckpoint || !currentPath) return;
    const cp = selectedCheckpoint;
    cpConfirming = false;
    reverting = true;
    revertError = "";
    revertOutcome = null;
    try {
      const res = await commands.checkpointRevert(currentPath, cp.manifest_id);
      if (res.status === "ok") {
        revertOutcome = res.data;
        // A revert changes the tree, so the plan is now stale — refresh both the list and the preview.
        loadCheckpoints(currentPath);
        loadRevertPreview(cp);
      } else {
        revertError = String(res.error);
      }
    } catch (e) {
      revertError = e instanceof Error ? e.message : String(e);
    } finally {
      reverting = false;
    }
  }

  const cpTime = (ms: number) => new Date(ms).toLocaleString();
  const cpShortId = (id: string) => (id.length > 12 ? `${id.slice(0, 12)}…` : id);
  const cpMarkerTitle = (m: { cp: Checkpoint; inRange: boolean }) =>
    `${m.cp.label || cpShortId(m.cp.manifest_id)} — ${cpTime(m.cp.ts)}` +
    (m.inRange ? "" : " (outside the recorded window — pinned to the track edge)");

  const jumpStart = () => {
    stopPlaying();
    if (range) t = range.firstAt;
  };
  const jumpEnd = () => {
    stopPlaying();
    if (range) t = range.lastAt;
  };
  const stepForward = () => {
    const nxt = nextTimestamp(entries, t);
    if (nxt !== null) t = nxt;
    else stopPlaying(); // already at the end — nothing further to step to
  };
  const stepBack = () => {
    stopPlaying();
    const prv = prevTimestamp(entries, t);
    if (prv !== null) t = prv;
  };
  const onSliderInput = (ev: Event) => {
    stopPlaying();
    t = Number((ev.target as HTMLInputElement).value);
  };
  // Creates the play interval at the current `speed`'s cadence. Only ever called while `playing` is
  // (or is about to become) true — pause/end/unmount/watch-off all go through stopPlaying instead.
  const startPlaying = () => {
    playTimer = setInterval(() => {
      const nxt = nextTimestamp(entries, t);
      if (nxt === null) {
        stopPlaying();
        return;
      }
      t = nxt;
    }, cadenceForSpeed(PLAY_INTERVAL_MS, speed));
  };

  const togglePlay = () => {
    if (playing) {
      stopPlaying();
      return;
    }
    if (!range || atEnd) return; // nothing to play
    playing = true;
    startPlaying();
  };

  /** Change playback speed (CPE-1104). Mid-play, this clears + re-creates the interval at the new
   *  cadence — reusing the same timer field, so there's never more than one interval alive. Not
   *  playing? Just remember the choice for the next play. */
  const selectSpeed = (s: number) => {
    speed = s;
    if (playing && playTimer !== null) {
      clearInterval(playTimer);
      playTimer = null;
      startPlaying();
    }
  };
</script>

<aside class="timeline" aria-label="Agent activity timeline">
  <header class="tl-head">
    <span class="tl-title">Activity — {agentName}</span>
    <span class="tl-count">{entries.length}</span>
    <button class="tl-close" title="Close" on:click={() => dispatch("close")}>
      <Icon name="close" size={14} />
    </button>
  </header>
  <div class="tabbar tl-tabbar" role="tablist" aria-label="Timeline view">
    <button
      class="tab"
      class:active={tab === "live"}
      role="tab"
      aria-selected={tab === "live"}
      on:click={() => (tab = "live")}
    ><span class="tab-label">Live</span></button>
    <button
      class="tab"
      class:active={tab === "replay"}
      role="tab"
      aria-selected={tab === "replay"}
      on:click={() => (tab = "replay")}
    ><span class="tab-label">Replay</span></button>
    <button
      class="tab"
      class:active={tab === "cost"}
      role="tab"
      aria-selected={tab === "cost"}
      on:click={() => (tab = "cost")}
    ><span class="tab-label">Cost</span></button>
    <button
      class="tab"
      class:active={tab === "radar"}
      role="tab"
      aria-selected={tab === "radar"}
      on:click={() => (tab = "radar")}
    ><span class="tab-label">Radar</span></button>
    <button
      class="tab"
      class:active={tab === "history"}
      role="tab"
      aria-selected={tab === "history"}
      on:click={() => (tab = "history")}
    ><span class="tab-label">History</span></button>
  </div>

  {#if tab === "live"}
    <!-- Files the agent has READ this session (CPE-741) — a durable consulted set above the activity log. -->
    <ConsultedFiles on:navigate />
    {#if entries.length === 0}
      <div class="tl-empty">No activity yet — changes appear here as the agent works.</div>
    {:else}
      <ul class="tl-list">
        {#each entries as e (e.id)}
          {@const diff = isWrite(e.kind) ? diffFor($agentDiffs, e.path) : null}
          {@const stats = diff ? diffLineStats($agentDiffs, e.path) : null}
          <li
            class:has-diff={!!diff}
            on:mouseenter={() => { if (diff) openId = e.id; }}
            on:mouseleave={() => { if (openId === e.id) openId = null; }}
          >
            <button
              class="tl-row"
              title={diff ? `${e.path} — hover to see what changed` : e.path}
              on:click={() => dispatch("navigate", dirOf(e.path))}
              on:focus={() => { if (diff) openId = e.id; }}
              on:blur={() => { if (openId === e.id) openId = null; }}
            >
              <span class="tl-badge {e.kind}">{KIND_LABEL[e.kind]}</span>
              <span class="tl-name">{baseOf(e.path)}</span>
              {#if stats}<span class="tl-stat" aria-label="lines added and removed">+{stats.add} −{stats.del}</span>{/if}
              <span class="tl-time">{clock(e.at)}</span>
            </button>
            {#if diff && openId === e.id}
              <div class="tl-peek">
                <button
                  class="tl-expand"
                  on:click={() => (sbs = { path: e.path, before: diff.before, after: diff.after })}
                >Open full diff ⤢</button>
                <DiffPeek before={diff.before} after={diff.after} />
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {:else if tab === "replay"}
    <!-- Replay tab (CPE-1094): scrub back and forth through this session's activity history. -->
    {#if !range}
      <div class="tl-empty">
        Not enough activity to replay yet — the scrubber needs at least two recorded actions.
      </div>
    {:else}
      <div class="rp-transport">
        <button
          class="rp-btn"
          title="Jump to start"
          disabled={atStart}
          on:click={jumpStart}
        >⏮</button>
        <button
          class="rp-btn"
          title="Step back"
          disabled={atStart}
          on:click={stepBack}
        >◀</button>
        <button
          class="rp-btn rp-play"
          title={playing ? "Pause" : "Play"}
          disabled={atEnd && !playing}
          on:click={togglePlay}
        >{playing ? "Pause" : "Play"}</button>
        <button
          class="rp-btn"
          title="Step forward"
          disabled={atEnd}
          on:click={stepForward}
        >▶</button>
        <button
          class="rp-btn"
          title="Jump to end"
          disabled={atEnd}
          on:click={jumpEnd}
        >⏭</button>
      </div>
      <div class="rp-speed" role="group" aria-label="Playback speed">
        {#each SPEEDS as s (s)}
          <button
            class="rp-speed-btn"
            class:active={speed === s}
            aria-pressed={speed === s}
            on:click={() => selectSpeed(s)}
          >{s}×</button>
        {/each}
      </div>
      <!-- Scrubber track wrapped so checkpoint marker pins (CPE-1126) can be absolutely positioned
           over the same span as the slider thumb (`left: fraction*100%`). -->
      <div class="rp-track">
        <input
          class="rp-slider"
          type="range"
          min={range.firstAt}
          max={range.lastAt}
          step="1"
          value={t}
          disabled={range.firstAt === range.lastAt}
          on:input={onSliderInput}
          aria-label="Replay position"
          aria-valuetext={new Date(t).toLocaleTimeString()}
        />
        {#if cpMarkers.length > 0}
          <div class="rp-markers" data-testid="checkpoint-markers" aria-hidden="false">
            {#each cpMarkers as m (m.cp.manifest_id)}
              <button
                class="rp-marker"
                class:out={!m.inRange}
                class:sel={selectedCheckpoint?.manifest_id === m.cp.manifest_id}
                style="left: {m.fraction * 100}%"
                title={cpMarkerTitle(m)}
                aria-label={`Checkpoint ${m.cp.label || cpShortId(m.cp.manifest_id)}`}
                data-testid="checkpoint-marker-{m.cp.manifest_id}"
                on:click={() => selectCheckpoint(m.cp)}
              ></button>
            {/each}
          </div>
        {/if}
      </div>
      <div class="rp-clock">{new Date(t).toLocaleTimeString()} <span class="rp-frac">({Math.round(sliderFraction(range, t) * 100)}%)</span></div>

      <!-- Checkpoint restore panel (CPE-1126): revert plan + drift warning + two-step confirm,
           mirroring CheckpointDialog's safety pattern. Opens when a marker is selected. -->
      {#if selectedCheckpoint}
        <div class="cp-restore" data-testid="checkpoint-restore-panel">
          <div class="cp-restore-head">
            <span class="cp-restore-title">Restore to checkpoint</span>
            <button class="cp-restore-close" title="Dismiss" aria-label="Dismiss restore panel" on:click={clearCheckpointRestore}>
              <Icon name="close" size={12} />
            </button>
          </div>
          <div class="cp-restore-id" title={selectedCheckpoint.manifest_id}>
            {selectedCheckpoint.label || cpShortId(selectedCheckpoint.manifest_id)}
            <span class="cp-restore-meta">{cpTime(selectedCheckpoint.ts)}</span>
          </div>

          {#if revertPreviewLoading}
            <div class="tl-empty cp-restore-empty">Previewing revert…</div>
          {:else if revertPreviewError}
            <div class="cp-restore-err" data-testid="checkpoint-preview-error">Couldn't preview revert: {revertPreviewError}</div>
          {:else if revertPreview}
            <div class="cp-counts" data-testid="checkpoint-counts">
              <span class="cp-count">creates {revertPreview.creates}</span>
              <span class="cp-count">overwrites {revertPreview.overwrites}</span>
              <span class="cp-count">deletes {revertPreview.deletes}</span>
              <span class="cp-count">{formatBytes(revertPreview.bytes_written)} to write</span>
              <span class="cp-count" class:drift={revertPreview.drift_count > 0}>drift {revertPreview.drift_count}</span>
            </div>
            {#if revertPreview.drift_count > 0}
              <div class="cp-drift-warn" data-testid="checkpoint-drift-warning">
                <strong>{revertPreview.drift_count} file{revertPreview.drift_count === 1 ? "" : "s"} changed since this checkpoint</strong>
                — reverting overwrites that newer work. Review before you revert.
              </div>
              <div class="cp-drift-list">
                {#each revertPreview.drift_paths as p (p)}<div class="cp-drift-item" title={p}>{p}</div>{/each}
              </div>
            {/if}
          {/if}

          {#if revertOutcome}
            <div class="cp-outcome" data-testid="checkpoint-outcome">
              Reverted — applied {revertOutcome.applied} change{revertOutcome.applied === 1 ? "" : "s"}{#if revertOutcome.skipped.length}, skipped {revertOutcome.skipped.length}{/if}.
            </div>
          {/if}
          {#if revertError}<div class="cp-restore-err">{revertError}</div>{/if}

          {#if cpConfirming}
            <div class="cp-confirm" data-testid="checkpoint-confirm-revert">
              <p class="cp-confirm-msg">
                This overwrites, recreates, and deletes files under <strong>{currentPath}</strong> to match
                <strong>{selectedCheckpoint.label || cpShortId(selectedCheckpoint.manifest_id)}</strong>. This cannot be undone.
              </p>
              <div class="cp-confirm-actions">
                <button class="cp-btn" data-testid="checkpoint-confirm-cancel" on:click={cancelCheckpointRevert}>Cancel</button>
                <button class="cp-btn danger" data-testid="checkpoint-confirm-yes" disabled={reverting} on:click={doCheckpointRevert}>
                  Yes, revert
                </button>
              </div>
            </div>
          {:else}
            <div class="cp-restore-actions">
              <button
                class="cp-btn danger"
                data-testid="checkpoint-revert-btn"
                disabled={reverting || revertPreviewLoading || !currentPath}
                on:click={armCheckpointRevert}
              >Revert to this checkpoint…</button>
            </div>
          {/if}
        </div>
      {/if}

      {#if replayCurrent}
        <div class="rp-current">
          <span class="tl-badge {replayCurrent.kind}">{KIND_LABEL[replayCurrent.kind]}</span>
          <span class="tl-name" title={replayCurrent.path}>{baseOf(replayCurrent.path)}</span>
          <span class="tl-time">{clock(replayCurrent.at)}</span>
        </div>
        {#if replayMultiplyEdited}
          <div class="rp-badge-stale">
            content at this point not retained — showing latest
          </div>
        {/if}
        {#if replayDiff}
          <div class="tl-peek rp-peek">
            <button
              class="tl-expand"
              on:click={() => (sbs = replayDiff && replayCurrent ? { path: replayCurrent.path, before: replayDiff.before, after: replayDiff.after } : null)}
            >Open full diff ⤢</button>
            <DiffPeek before={replayDiff.before} after={replayDiff.after} />
          </div>
        {/if}
      {/if}

      <!-- Reconstructed folder listing (CPE-1111, epic CPE-728 slice d): "what did this folder look
           like at this scrub moment", pre-existing + event-derived entries together. Loaded once per
           session on tab-enter (pull-only); re-derives per tick with no IPC. A load error falls back
           silently to the classic frozen event list below — the tab never breaks. -->
      {#if replayLoading}
        <div class="tl-empty rp-recon-empty">Loading reconstruction…</div>
      {:else if replayData}
        <div class="rp-recon">
          <div class="rp-recon-head">
            <span class="rp-recon-title">Reconstruction at scrub time (read-only)</span>
            {#if currentPath}<span class="rp-recon-path" title={currentPath}>{currentPath}</span>{/if}
            <!-- CPE-1112 (epic CPE-728 slice e): graduate this same reconstruction from the drawer to
                 the main file pane while scrubbing. Off by default; flipping it off (or leaving this
                 tab) restores the live pane on the next tick — see `replayOverlayActive` above. -->
            <label class="rp-overlay-toggle">
              <input type="checkbox" bind:checked={replayMode} />
              Show in file pane
            </label>
          </div>
          {#if replayMode}
            <div class="rp-overlay-note">
              Live listing paused in the main pane — showing this reconstruction there instead.
            </div>
          {/if}
          {#if replayListing.length === 0}
            <div class="tl-empty rp-recon-empty">Nothing reconstructed for this folder at this point.</div>
          {:else}
            <ul class="tl-list rp-recon-list">
              {#each replayListing as re (re.path)}
                <li>
                  <span class="tl-row rp-row rp-recon-row">
                    <Icon name={re.isDir ? "folder" : "document"} size={14} />
                    <span class="tl-name" title={re.path}>{re.name}</span>
                    <span class="tl-badge {re.kind}">{replayKindLabel(re.kind)}</span>
                    {#if re.kind !== "baseline"}<span class="tl-time">{clock(re.ts)}</span>{/if}
                  </span>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}

      <div class="rp-hist-head">Activity up to this point</div>
      <ul class="tl-list rp-list">
        {#each replayFrozen as e (e.id)}
          <li class:rp-current-row={replayCurrent?.id === e.id}>
            <span class="tl-row rp-row">
              <span class="tl-badge {e.kind}">{KIND_LABEL[e.kind]}</span>
              <span class="tl-name">{baseOf(e.path)}</span>
              <span class="tl-time">{clock(e.at)}</span>
            </span>
          </li>
        {/each}
      </ul>
    {/if}
  {:else if tab === "cost"}
    <!-- Cost tab (CPE-1098): live per-session usage bridged from the sidecar's best-effort PTY usage
         scrape (CPE-1097) — advisory figures, never billing (see the note below the list). -->
    {#if costList.length === 0}
      <div class="tl-empty">No usage reported yet — figures appear here once the agent reports usage.</div>
    {:else}
      <div class="cl-note">
        Best-effort figures scraped from the agent's own output — not billing. Files/edits/churn/
        wall-clock are approximations derived from the activity this app observed, not an authoritative
        agent-side count.
      </div>
      <ul class="cl-list">
        {#each costList as c (c.sessionId)}
          <li class="cl-card" class:cl-current={c.sessionId === sessionId && sessionId !== ""}>
            <div class="cl-head">
              <span class="cl-session" title={c.sessionId}>{c.sessionId}</span>
              {#if c.sessionId === sessionId && sessionId !== ""}<span class="cl-chip">watched</span>{/if}
            </div>
            <div class="cl-row"><span class="cl-label">Input tokens</span><span class="cl-value">{formatTokens(c.inputTokens)}</span></div>
            <div class="cl-row"><span class="cl-label">Output tokens</span><span class="cl-value">{formatTokens(c.outputTokens)}</span></div>
            <div class="cl-row"><span class="cl-label">Total tokens</span><span class="cl-value">{formatTokens(c.totalTokens)}</span></div>
            <div class="cl-row"><span class="cl-label">Cost (USD)</span><span class="cl-value">{formatUsd(c.costUsd)}</span></div>
            <div class="cl-sep"></div>
            <div class="cl-row"><span class="cl-label">Files touched</span><span class="cl-value">{formatTokens(c.filesTouched)}</span></div>
            <div class="cl-row"><span class="cl-label">Edits</span><span class="cl-value">{formatTokens(c.editCount)}</span></div>
            <div class="cl-row"><span class="cl-label">Churn</span><span class="cl-value">{formatBytes(c.churnBytes)}</span></div>
            <div class="cl-row"><span class="cl-label">Wall-clock</span><span class="cl-value">{formatDuration(c.wallClockMs)}</span></div>
            {#if c.tokensPerMinute !== undefined || c.usdPerFile !== undefined || c.churnPer1kTokens !== undefined}
              <div class="cl-sep"></div>
              {#if c.tokensPerMinute !== undefined}
                <div class="cl-row"><span class="cl-label">Tokens/min</span><span class="cl-value">{formatPerMinute(c.tokensPerMinute)}</span></div>
              {/if}
              {#if c.usdPerFile !== undefined}
                <div class="cl-row"><span class="cl-label">USD/file</span><span class="cl-value">{formatUsd(c.usdPerFile)}</span></div>
              {/if}
              {#if c.churnPer1kTokens !== undefined}
                <div class="cl-row"><span class="cl-label">Churn/1k tok</span><span class="cl-value">{formatBytes(c.churnPer1kTokens)}</span></div>
              {/if}
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {:else if tab === "radar"}
    <!-- Radar tab (CPE-1100): activity OVERLAP, not "conflict" — a raw watcher can't prove two
         touches came from unrelated actors vs. the same agent revisiting its own file, so the
         wording is deliberately hedged (agentConflicts.ts). -->
    {#if overlaps.length === 0 && renameConflicts.length === 0}
      <div class="tl-empty">No overlapping activity — nothing has been touched by more than one actor recently.</div>
    {:else}
      {#if overlaps.length > 0}
        <ul class="tl-list rd-list">
          {#each overlaps as o (o.path)}
            <li class="rd-item">
              <button
                class="tl-row rd-row"
                title={o.path}
                on:click={() => dispatch("navigate", dirOf(o.path))}
              >
                <span class="tl-name">{baseOf(o.path)}</span>
                <span class="tl-time">{relativeLabel(o.lastAt, Date.now())}</span>
              </button>
              <div class="rd-actors">
                {#each o.actors as a (a)}
                  <span class="rd-pill">{friendlyActor(a, sessions)}</span>
                {/each}
              </div>
              {#if overlapHasUnknown(o)}
                <div class="rd-note">Includes an unresolved actor — attribution here is best-effort.</div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
      {#if renameConflicts.length > 0}
        <!-- Competing renames (CPE-1118): same-from divergence / same-to collision across distinct
             actors, mirroring conflict_rename.rs. Extends this tab rather than adding a second
             empty state — the "No overlapping activity" state above already covers "nothing here". -->
        <div class="rd-section-title">Competing renames</div>
        <ul class="tl-list rd-list">
          {#each renameConflicts as rc (rc.kind + ':' + rc.path)}
            <li class="rd-item">
              <button
                class="tl-row rd-row"
                title={rc.path}
                on:click={() => dispatch("navigate", dirOf(rc.path))}
              >
                <span class="rd-kind-badge rd-kind-{rc.kind}">{rc.kind === "divergence" ? "diverged" : "collided"}</span>
                <span class="tl-name">{baseOf(rc.path)}</span>
                <span class="tl-time">{relativeLabel(rc.lastAt, Date.now())}</span>
              </button>
              <div class="rd-actors">
                {#each rc.actors as a (a)}
                  <span class="rd-pill">{friendlyActor(a, sessions)}</span>
                {/each}
              </div>
              <div class="rd-note">{renameConflictNote(rc.kind)}</div>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  {:else}
    <!-- History tab (CPE-1114, epic CPE-731 slice c — final slice): cross-session rollup read from the
         persisted metrics journal (`commands.metricsHistory()`, CPE-1113). Pull-only — see the load
         guard above; nothing runs here while this tab/the drawer is closed. -->
    {#if historyLoading}
      <div class="tl-empty">Loading session history…</div>
    {:else if historyError}
      <div class="tl-empty">Couldn't load session history: {historyError}</div>
    {:else if !historyRollup || historyRollup.totals.sessions === 0}
      <div class="tl-empty">No session history yet — rows appear here once a watched session ends.</div>
    {:else}
      <div class="cl-note">
        Cross-session totals recorded on this machine — best-effort figures scraped from each agent's
        own output, not billing. Churn/files/wall-clock are approximations.
      </div>
      <div class="hd-body">
        <div class="hd-totals">
          <div class="hd-stat"><span class="hd-stat-label">Sessions</span><span class="hd-stat-value">{formatTokens(historyRollup.totals.sessions)}</span></div>
          <div class="hd-stat"><span class="hd-stat-label">Total cost</span><span class="hd-stat-value">{formatUsd(historyRollup.totals.costUsd)}</span></div>
          <div class="hd-stat"><span class="hd-stat-label">Total tokens</span><span class="hd-stat-value">{formatTokens(historyRollup.totals.totalTokens)}</span></div>
          <div class="hd-stat"><span class="hd-stat-label">Total time</span><span class="hd-stat-value">{formatDuration(historyRollup.totals.wallClockMs)}</span></div>
          <div class="hd-stat"><span class="hd-stat-label">Files touched</span><span class="hd-stat-value">{formatTokens(historyRollup.totals.filesTouched)}</span></div>
          <div class="hd-stat"><span class="hd-stat-label">Churn</span><span class="hd-stat-value">{formatBytes(historyRollup.totals.churnBytes)}</span></div>
        </div>

        {#if historyRollup.ratios.tokensPerMinute !== undefined || historyRollup.ratios.usdPerSession !== undefined || historyRollup.ratios.usdPerFile !== undefined || historyRollup.ratios.churnPer1kTokens !== undefined}
          <div class="hd-section-title">Throughput</div>
          <div class="hd-ratios">
            {#if historyRollup.ratios.tokensPerMinute !== undefined}
              <div class="cl-row"><span class="cl-label">Tokens/min</span><span class="cl-value">{formatPerMinute(historyRollup.ratios.tokensPerMinute)}</span></div>
            {/if}
            {#if historyRollup.ratios.usdPerSession !== undefined}
              <div class="cl-row"><span class="cl-label">USD/session</span><span class="cl-value">{formatUsd(historyRollup.ratios.usdPerSession)}</span></div>
            {/if}
            {#if historyRollup.ratios.usdPerFile !== undefined}
              <div class="cl-row"><span class="cl-label">USD/file</span><span class="cl-value">{formatUsd(historyRollup.ratios.usdPerFile)}</span></div>
            {/if}
            {#if historyRollup.ratios.churnPer1kTokens !== undefined}
              <div class="cl-row"><span class="cl-label">Churn/1k tok</span><span class="cl-value">{formatBytes(historyRollup.ratios.churnPer1kTokens)}</span></div>
            {/if}
          </div>
        {/if}

        <div class="hd-section-title">By model</div>
        <div class="hd-table-wrap">
          <table class="hd-table">
            <thead>
              <tr><th>Model</th><th>Sessions</th><th>Tokens</th><th>Cost</th><th>Share</th></tr>
            </thead>
            <tbody>
              {#each [...historyRollup.byModel.values()] as row (row.model)}
                <tr>
                  <td class="hd-key" title={row.model}>{row.model}</td>
                  <td>{formatTokens(row.sessions)}</td>
                  <td>{formatTokens(row.totalTokens)}</td>
                  <td>{formatUsd(row.costUsd)}</td>
                  <td>{historyShare(row.costUsd, historyRollup.totals.costUsd)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>

        <div class="hd-section-title">By agent</div>
        <div class="hd-table-wrap">
          <table class="hd-table">
            <thead>
              <tr><th>Agent</th><th>Sessions</th><th>Tokens</th><th>Cost</th><th>Share</th></tr>
            </thead>
            <tbody>
              {#each [...historyRollup.byAgent.values()] as row (row.agentId)}
                <tr>
                  <td class="hd-key" title={row.agentName}>{row.agentName}</td>
                  <td>{formatTokens(row.sessions)}</td>
                  <td>{formatTokens(row.totalTokens)}</td>
                  <td>{formatUsd(row.costUsd)}</td>
                  <td>{historyShare(row.costUsd, historyRollup.totals.costUsd)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>

        <div class="hd-section-title-row">
          <span class="hd-section-title">Over time</span>
          <div class="rp-speed hd-metric-toggle" role="group" aria-label="Over-time series">
            <button
              class="rp-speed-btn"
              class:active={historyMetric === "cost"}
              aria-pressed={historyMetric === "cost"}
              on:click={() => (historyMetric = "cost")}
            >Cost</button>
            <button
              class="rp-speed-btn"
              class:active={historyMetric === "tokens"}
              aria-pressed={historyMetric === "tokens"}
              on:click={() => (historyMetric = "tokens")}
            >Tokens</button>
          </div>
        </div>
        {#if historyOverTime.length === 0}
          <div class="tl-empty hd-chart-empty">Not enough history yet to chart.</div>
        {:else}
          <svg
            class="hd-chart"
            viewBox="0 0 {historyOverTime.length * 16} 60"
            preserveAspectRatio="none"
            role="img"
            aria-label="{historyMetric === 'cost' ? 'Cost' : 'Tokens'} per day"
          >
            {#each historyOverTime as p, i (p.bucketStart)}
              {@const v = historyMetric === "cost" ? p.costUsd : p.totalTokens}
              {@const h = historyMax > 0 ? Math.max(1, (v / historyMax) * 56) : 1}
              <rect
                x={i * 16 + 3}
                y={60 - h}
                width="10"
                height={h}
                rx="2"
                class="hd-bar"
              ><title>{historyBarDate(p.bucketStart)}: {historyMetric === "cost" ? formatUsd(v) : formatTokens(v)}</title></rect>
            {/each}
          </svg>
        {/if}
      </div>
    {/if}
  {/if}
</aside>

{#if sbs}
  <DiffSideBySide path={sbs.path} before={sbs.before} after={sbs.after} on:close={() => (sbs = null)} />
{/if}

<style>
  .timeline {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: 340px;
    max-width: 90vw;
    z-index: 60;
    display: flex;
    flex-direction: column;
    background: var(--surface, #1e1e1e);
    color: var(--text, #eaeaea);
    border-left: 1px solid var(--border, #3a3a3a);
    box-shadow: -8px 0 24px rgba(0, 0, 0, 0.28);
  }
  .tl-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px 8px 12px;
    border-bottom: 1px solid var(--border, #3a3a3a);
    font-size: 13px;
    font-weight: 600;
  }
  .tl-title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tl-count {
    font-size: 11px;
    font-weight: 600;
    padding: 1px 7px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent, #2f6fed) 22%, transparent);
  }
  .tl-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
    border-radius: 4px;
  }
  .tl-close:hover {
    background: rgba(128, 128, 128, 0.18);
  }
  .tl-empty {
    padding: 16px 14px;
    font-size: 12px;
    opacity: 0.65;
    line-height: 1.5;
  }
  .tl-list {
    list-style: none;
    margin: 0;
    padding: 4px;
    overflow-y: auto;
    flex: 1;
  }
  .tl-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 8px;
    border: 0;
    background: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
    border-radius: 5px;
    font: inherit;
    font-size: 12.5px;
  }
  .tl-row:hover {
    background: rgba(128, 128, 128, 0.14);
  }
  .tl-badge {
    flex: 0 0 auto;
    padding: 0 6px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    line-height: 16px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #fff;
  }
  .tl-badge.created { background: #3a9d4a; }
  .tl-badge.modified { background: #b5872b; }
  .tl-badge.renamed { background: #3a72b5; }
  .tl-badge.removed { background: #b5433a; }
  /* CPE-405: a read is the weakest signal — a hollow, muted badge, visually subordinate to changes. */
  .tl-badge.read {
    background: transparent;
    color: var(--text-muted, #9a9a9a);
    border: 1px solid var(--border, #4a4a4a);
  }
  /* CPE-1111: a reconstructed entry the fold seeded from the baseline snapshot (pre-existing, never
     itself an agent action this session) — same hollow/muted treatment as a read. */
  .tl-badge.baseline {
    background: transparent;
    color: var(--text-muted, #9a9a9a);
    border: 1px solid var(--border, #4a4a4a);
  }
  .tl-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* A subtle changed-lines summary on write rows that carry a captured diff (CPE-745). */
  .tl-stat {
    flex: 0 0 auto;
    font-size: 10.5px;
    opacity: 0.7;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.02em;
  }
  .has-diff .tl-row {
    /* Hint that this row has more to show on hover/focus. */
    cursor: help;
  }
  .tl-peek {
    padding: 0 8px 2px 8px;
  }
  .tl-expand {
    display: inline-block;
    margin: 0 0 3px;
    padding: 1px 8px;
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 4px;
    background: var(--surface-alt, transparent);
    color: var(--accent, #2f6fed);
    font-size: 10.5px;
    cursor: pointer;
  }
  .tl-expand:hover {
    background: rgba(128, 128, 128, 0.14);
  }
  .tl-time {
    flex: 0 0 auto;
    font-size: 11px;
    opacity: 0.55;
    font-variant-numeric: tabular-nums;
  }

  /* ---------- Replay tab (CPE-1094) ---------- */
  /* Reuses the app-wide .tabbar/.tab/.tab.active convention (TABS.md); just fit it to the drawer. */
  .tl-tabbar {
    padding: 6px 8px 0;
    background: var(--surface, #1e1e1e);
    flex: 0 0 auto;
    /* CPE-1130 (found via gui-smoke's new cost-History smoke test): the base `.tab` class's
       120px min-width (TABS.md, sized for the wide main-window tabbar) means 5 tabs
       (Live/Replay/Cost/Radar/History) can never fit this drawer's 340px/90vw width on one row —
       the last tab silently overflowed past the document's edge and was unclickable. Reflow onto a
       second row rather than overflow (same tick-tack wrap-container convention used for pill rows
       elsewhere in this file), belt-and-braces alongside the narrower min-width below.
       Still reuses `.tab`/`.tab.active` as-is (TABS.md) — only the container wraps. */
    flex-wrap: wrap;
    row-gap: 2px;
  }
  .tl-tabbar .tab {
    /* Shrink the floor for this narrow context — see the `.tl-tabbar` comment above. `.tab-label`
       already ellipsis-truncates (app.css), so a tighter tab still reads fine. */
    min-width: 60px;
    padding: 0 6px;
  }
  .rp-transport {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 8px 10px 4px;
    flex: 0 0 auto;
  }
  .rp-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 28px;
    height: 26px;
    padding: 0 6px;
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 5px;
    background: var(--surface-alt, transparent);
    color: var(--text, inherit);
    font-size: 12px;
    cursor: pointer;
  }
  .rp-btn:hover:not(:disabled) {
    background: rgba(128, 128, 128, 0.18);
  }
  .rp-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .rp-play {
    color: var(--accent, #2f6fed);
    border-color: var(--accent, #2f6fed);
  }
  /* Speed selector (CPE-1104): a small segmented control, reflowing per the tick-tack convention
     (flex-wrap container; each pill keeps its own text on one line and doesn't shrink). */
  .rp-speed {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 0 10px 6px;
    flex: 0 0 auto;
  }
  .rp-speed-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    white-space: nowrap;
    flex: 0 0 auto;
    min-width: 30px;
    height: 22px;
    padding: 0 6px;
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 5px;
    background: var(--surface-alt, transparent);
    color: var(--text, inherit);
    font-size: 11px;
    cursor: pointer;
  }
  .rp-speed-btn:hover:not(.active) {
    background: rgba(128, 128, 128, 0.18);
  }
  .rp-speed-btn.active {
    color: var(--accent, #2f6fed);
    border-color: var(--accent, #2f6fed);
    font-weight: 600;
  }
  /* CPE-1126: the slider now lives inside `.rp-track` so marker pins can overlay the same span.
     The track carries the horizontal inset the slider used to own; the slider fills it. */
  .rp-track {
    position: relative;
    margin: 2px 10px 0;
    flex: 0 0 auto;
  }
  .rp-slider {
    display: block;
    width: 100%;
    margin: 0;
    accent-color: var(--accent, #2f6fed);
  }
  /* Marker overlay: pins sit above the track, click-through everywhere except on a pin itself. The
     small horizontal insets keep an edge-pinned marker fully visible rather than half-clipped. */
  .rp-markers {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    bottom: 0;
    pointer-events: none;
  }
  .rp-marker {
    position: absolute;
    top: 50%;
    width: 10px;
    height: 14px;
    margin-left: -5px;
    padding: 0;
    transform: translateY(-50%);
    border: 1px solid var(--surface, #1e1e1e);
    border-radius: 3px;
    background: var(--accent, #2f6fed);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent, #2f6fed) 60%, transparent);
    cursor: pointer;
    pointer-events: auto;
  }
  .rp-marker:hover {
    background: color-mix(in srgb, var(--accent, #2f6fed) 82%, #fff);
  }
  .rp-marker.sel {
    background: var(--accent, #2f6fed);
    box-shadow: 0 0 0 2px var(--accent, #2f6fed);
    height: 18px;
  }
  /* Out-of-range (clamped to a track edge): a subtly distinct hollow/muted pin so it reads as
     "outside the recorded window" rather than a normal in-window checkpoint. */
  .rp-marker.out {
    background: var(--surface-alt, #2a2a2a);
    border-color: var(--text-muted, #9a9a9a);
    box-shadow: none;
    opacity: 0.75;
  }
  .rp-marker.out:hover {
    background: color-mix(in srgb, var(--text-muted, #9a9a9a) 30%, var(--surface-alt, #2a2a2a));
  }
  .rp-clock {
    padding: 2px 10px 8px;
    font-size: 11px;
    opacity: 0.7;
    text-align: center;
    font-variant-numeric: tabular-nums;
    flex: 0 0 auto;
  }
  .rp-frac {
    opacity: 0.8;
  }
  .rp-current {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 8px 4px;
    padding: 6px 8px;
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 6px;
    background: var(--surface-alt, transparent);
    font-size: 12.5px;
    flex: 0 0 auto;
  }
  .rp-current .tl-name {
    font-weight: 600;
  }
  /* Fidelity caveat (CPE-1094): agentDiffs only retains the latest write per path, so a path edited
     more than once this session can't show the diff *as of this scrub position* — say so plainly
     rather than silently showing a diff for a different moment. */
  .rp-badge-stale {
    margin: 0 8px 6px;
    padding: 4px 8px;
    border-radius: 5px;
    background: color-mix(in srgb, var(--warn, #b5872b) 20%, transparent);
    color: var(--text, inherit);
    font-size: 10.5px;
    line-height: 1.4;
  }
  .rp-peek {
    margin: 0 8px 6px;
    padding: 0;
  }
  .rp-row {
    cursor: default;
  }
  .rp-current-row {
    background: color-mix(in srgb, var(--accent, #2f6fed) 14%, transparent);
    border-radius: 5px;
  }

  /* ---------- Checkpoint restore panel (CPE-1126, epic CPE-732 GUI cap) ---------- */
  /* A real visible border (not just a shadow), theme vars only — mirrors CheckpointDialog's language. */
  .cp-restore {
    margin: 4px 8px 8px;
    padding: 8px 10px 10px;
    border: 1px solid var(--border-strong, var(--border, #3a3a3a));
    border-radius: 6px;
    background: var(--surface-alt, transparent);
    flex: 0 0 auto;
  }
  .cp-restore-head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 2px;
  }
  .cp-restore-title {
    flex: 1 1 auto;
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-muted, #9a9a9a);
  }
  .cp-restore-close {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text, inherit);
    cursor: pointer;
    border-radius: 4px;
  }
  .cp-restore-close:hover {
    background: rgba(128, 128, 128, 0.18);
  }
  .cp-restore-id {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text, inherit);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-bottom: 6px;
  }
  .cp-restore-meta {
    margin-left: 6px;
    font-size: 10.5px;
    font-weight: 400;
    color: var(--text-muted, #9a9a9a);
  }
  .cp-restore-empty {
    padding: 6px 0;
  }
  .cp-restore-err {
    padding: 4px 0;
    font-size: 11.5px;
    color: var(--danger, #c0392b);
  }
  /* Counts row — tick-tacks: reflow onto more rows, each pill one-line and non-shrinking. */
  .cp-counts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 4px;
  }
  .cp-count {
    flex: 0 0 auto;
    white-space: nowrap;
    padding: 1px 8px;
    border-radius: 999px;
    border: 1px solid var(--border, #3a3a3a);
    background: var(--surface, transparent);
    color: var(--text-muted, #9a9a9a);
    font-size: 10.5px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .cp-count.drift {
    color: var(--warn, #b8860b);
    border-color: var(--warn, #b8860b);
  }
  /* Prominent drift warning when reverting would clobber work changed since the checkpoint. */
  .cp-drift-warn {
    margin: 4px 0;
    padding: 6px 8px;
    border: 1px solid var(--warn, #b8860b);
    border-radius: 5px;
    background: color-mix(in srgb, var(--warn, #b8860b) 16%, transparent);
    color: var(--text, inherit);
    font-size: 11px;
    line-height: 1.4;
  }
  .cp-drift-list {
    max-height: 12vh;
    overflow: auto;
    margin-bottom: 4px;
  }
  .cp-drift-item {
    padding: 1px 0;
    font-size: 11px;
    color: var(--text-muted, #9a9a9a);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cp-outcome {
    margin: 4px 0;
    font-size: 11.5px;
    color: var(--text, inherit);
  }
  .cp-restore-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 4px;
  }
  .cp-btn {
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--border-strong, var(--border, #3a3a3a));
    border-radius: 5px;
    background: var(--surface, transparent);
    color: var(--text, inherit);
    font-size: 12px;
    cursor: pointer;
  }
  .cp-btn:hover:not(:disabled) {
    background: rgba(128, 128, 128, 0.14);
  }
  .cp-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  /* Destructive treatment reuses CheckpointDialog's red — a literal here, matching that dialog, since
     the palette has no dedicated destructive token; this is the one sanctioned red (a revert). */
  .cp-btn.danger {
    border-color: #c42b1c;
    color: #fff;
    background: #c42b1c;
  }
  .cp-btn.danger:hover:not(:disabled) {
    background: #b0271a;
  }
  /* Two-step confirm panel — same red-tinted surface CheckpointDialog uses. */
  .cp-confirm {
    margin-top: 6px;
    padding: 8px 10px;
    border: 1px solid #c42b1c;
    border-radius: 5px;
    background: color-mix(in srgb, #c42b1c 8%, var(--surface, #1e1e1e));
  }
  .cp-confirm-msg {
    margin: 0 0 8px;
    font-size: 11.5px;
    line-height: 1.45;
    color: var(--text, inherit);
  }
  .cp-confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  /* ---------- Reconstructed folder listing (CPE-1111, epic CPE-728 slice d) ---------- */
  .rp-recon {
    border-top: 1px solid var(--border, #3a3a3a);
    flex: 0 1 auto;
    /* This section can grow with the listing but must still yield scroll room to the rest of the
       drawer, so it scrolls internally rather than pushing the transport off-screen. */
    max-height: 45%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .rp-recon-head {
    display: flex;
    flex-wrap: wrap; /* tick-tacks: reflow rather than overflow if the path is long */
    align-items: baseline;
    gap: 6px;
    padding: 6px 10px 2px;
    flex: 0 0 auto;
  }
  .rp-recon-title {
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-muted, #9a9a9a);
  }
  .rp-recon-path {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10.5px;
    opacity: 0.75;
  }
  .rp-recon-empty {
    padding: 6px 10px 10px;
  }
  /* CPE-1112: the Replay-mode toggle (tick-tack row — reflows onto its own line via flex-wrap on
     .rp-recon-head above rather than overflowing it) + the confirming note shown while it's on. */
  .rp-overlay-toggle {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
    font-size: 10.5px;
    font-weight: 600;
    color: var(--text-muted, #9a9a9a);
    cursor: pointer;
    white-space: nowrap;
  }
  .rp-overlay-toggle input {
    margin: 0;
    accent-color: var(--accent, #2f6fed);
  }
  .rp-overlay-note {
    margin: 0 10px 4px;
    padding: 3px 7px;
    border-radius: 5px;
    background: color-mix(in srgb, var(--warn, #b5872b) 18%, transparent);
    color: var(--text, inherit);
    font-size: 10px;
    line-height: 1.4;
    flex: 0 0 auto;
  }
  .rp-recon-list {
    overflow-y: auto;
    flex: 1 1 auto;
    min-height: 0;
    padding: 0 4px 6px;
  }
  .rp-recon-row {
    gap: 6px;
  }
  .rp-hist-head {
    padding: 6px 10px 2px;
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-muted, #9a9a9a);
    border-top: 1px solid var(--border, #3a3a3a);
    flex: 0 0 auto;
  }

  /* ---------- Cost ledger tab (CPE-1098) ---------- */
  .cl-note {
    margin: 8px 10px 4px;
    padding: 6px 8px;
    border-radius: 5px;
    background: var(--surface-alt, transparent);
    border: 1px solid var(--border, #3a3a3a);
    color: var(--text-muted, #9a9a9a);
    font-size: 10.5px;
    line-height: 1.4;
    flex: 0 0 auto;
  }
  .cl-list {
    list-style: none;
    margin: 0;
    padding: 6px 10px 10px;
    overflow-y: auto;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .cl-card {
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 6px;
    padding: 8px 10px;
    background: var(--surface-alt, transparent);
  }
  .cl-current {
    border-color: var(--accent, #2f6fed);
  }
  .cl-head {
    display: flex;
    flex-wrap: wrap; /* tick-tacks: the chip row reflows instead of overflowing the card */
    align-items: center;
    gap: 6px;
    margin-bottom: 4px;
  }
  .cl-session {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text, inherit);
  }
  .cl-chip {
    flex: 0 0 auto;
    white-space: nowrap;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text, inherit);
    background: color-mix(in srgb, var(--accent, #2f6fed) 22%, transparent);
  }
  .cl-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    padding: 2px 0;
    font-size: 12px;
  }
  .cl-label {
    color: var(--text-muted, #9a9a9a);
  }
  .cl-value {
    color: var(--text, inherit);
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
  /* CPE-1107: a thin divider between the tokens+cost rows and the fuller files/churn/wall-clock and
     throughput sections, so the card reads as grouped facts rather than one flat list. */
  .cl-sep {
    margin: 4px 0;
    border-top: 1px solid var(--border, #3a3a3a);
  }

  /* ---------- Radar tab (CPE-1100): activity-overlap panel ---------- */
  .rd-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .rd-item {
    padding: 4px 4px 8px;
    border-bottom: 1px solid var(--border, #3a3a3a);
  }
  .rd-item:last-child {
    border-bottom: 0;
  }
  /* .rd-row reuses .tl-row's layout/hover as-is — it's the click target that navigates to the path. */
  .rd-actors {
    /* Tick-tacks: the pill row reflows onto more lines rather than overflowing the drawer. */
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    padding: 2px 8px 0;
  }
  .rd-pill {
    flex: 0 0 auto;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 1px 8px;
    border-radius: 999px;
    border: 1px solid var(--border, #3a3a3a);
    background: var(--surface-alt, transparent);
    color: var(--text, inherit);
    font-size: 10.5px;
    font-weight: 600;
  }
  .rd-note {
    margin: 4px 8px 0;
    padding: 3px 7px;
    border-radius: 5px;
    background: color-mix(in srgb, var(--warn, #b5872b) 18%, transparent);
    color: var(--text, inherit);
    font-size: 10px;
    line-height: 1.4;
  }
  /* CPE-1118: "Competing renames" sub-section within the Radar tab — a small heading separating it
     from the activity-overlap list above, theme vars only (no dark overrides; app is light-only). */
  .rd-section-title {
    margin: 10px 4px 4px;
    padding-top: 8px;
    border-top: 1px solid var(--border, #3a3a3a);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted, #9a9a9a);
  }
  .rd-kind-badge {
    flex: 0 0 auto;
    padding: 0 6px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    line-height: 16px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text, inherit);
    border: 1px solid var(--border, #3a3a3a);
  }
  .rd-kind-divergence {
    background: color-mix(in srgb, var(--warn, #b5872b) 22%, transparent);
  }
  .rd-kind-collision {
    background: color-mix(in srgb, var(--accent, #2f6fed) 22%, transparent);
  }

  /* ---------- History tab (CPE-1114, epic CPE-731 slice c): cross-session rollup dashboard ---------- */
  .hd-body {
    overflow-y: auto;
    flex: 1;
    padding: 4px 10px 12px;
  }
  /* Totals strip: stat tiles reflow onto more rows rather than overflowing the narrow drawer (same
     wrap-container principle as the tick-tack pill rows elsewhere in this file). */
  .hd-totals {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 6px;
  }
  .hd-stat {
    flex: 1 1 90px;
    min-width: 90px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 8px;
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 6px;
    background: var(--surface-alt, transparent);
  }
  .hd-stat-label {
    font-size: 10px;
    color: var(--text-muted, #9a9a9a);
    white-space: nowrap;
  }
  .hd-stat-value {
    font-size: 13px;
    font-weight: 600;
    color: var(--text, inherit);
    font-variant-numeric: tabular-nums;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hd-section-title {
    margin: 10px 0 4px;
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-muted, #9a9a9a);
  }
  .hd-section-title-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    margin-top: 10px;
  }
  .hd-section-title-row .hd-section-title {
    margin: 0;
  }
  .hd-metric-toggle {
    padding: 0;
    justify-content: flex-end;
  }
  .hd-ratios {
    display: flex;
    flex-direction: column;
  }
  /* Tables reflow via horizontal scroll rather than squeezing columns unreadably in the 340px drawer. */
  .hd-table-wrap {
    overflow-x: auto;
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 6px;
  }
  .hd-table {
    width: 100%;
    min-width: 280px;
    border-collapse: collapse;
    font-size: 11px;
  }
  .hd-table th,
  .hd-table td {
    padding: 4px 7px;
    text-align: right;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .hd-table th:first-child,
  .hd-table td:first-child {
    text-align: left;
  }
  .hd-table th {
    color: var(--text-muted, #9a9a9a);
    font-weight: 600;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.02em;
    border-bottom: 1px solid var(--border, #3a3a3a);
  }
  .hd-table td {
    color: var(--text, inherit);
    border-bottom: 1px solid var(--border, #3a3a3a);
  }
  .hd-table tr:last-child td {
    border-bottom: 0;
  }
  .hd-key {
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hd-chart-empty {
    padding: 8px 0 0;
  }
  /* Hand-rolled SVG bar chart (no chart dependency) — cost or tokens per UTC day, scaled to the
     tallest bar in view. A native <title> per bar supplies a hover tooltip (date + value). */
  .hd-chart {
    display: block;
    width: 100%;
    height: 70px;
    margin-top: 4px;
  }
  .hd-bar {
    fill: var(--accent, #2f6fed);
  }
</style>

