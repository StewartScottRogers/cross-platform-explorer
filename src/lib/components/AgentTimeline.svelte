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
  } from "../agentReplay";
  import { foldOverlaps, overlapHasUnknown, friendlyActor, relativeLabel } from "../agentConflicts";

  export let entries: TimelineEntry[] = [];
  export let agentName = "agent";
  /** sessionId of the agent currently being watched, if any (CPE-1098) — lets the cost tab flag
   *  which reporting session is the one on screen when several sessions report usage. */
  export let sessionId = "";
  /** Currently-running agent sessions (CPE-1100) — joined against an overlap's actor ids so the
   *  Radar tab can show an agent's name instead of a bare sessionId. Empty is fine (falls back to a
   *  shortened id); this component never fetches sessions itself. */
  export let sessions: AgentSession[] = [];

  const dispatch = createEventDispatcher<{ navigate: string; close: void }>();

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

  // ---------- Replay tab (CPE-1094) / Cost tab (CPE-1098) / Radar tab (CPE-1100) ----------
  /** "Live" (default, today's list), "Replay" (scrub through the session's history), "Cost" (live
   *  per-session token/USD usage), or "Radar" (activity-overlap signal across distinct actors). */
  let tab: "live" | "replay" | "cost" | "radar" = "live";

  // ---------- Radar tab (CPE-1100) ----------
  /** Paths touched by ≥2 distinct actors within the overlap window, most-recent first (pure fold
   *  over `entries` — no new listener/timer; see agentConflicts.ts for the hedged wording rationale). */
  $: overlaps = foldOverlaps(entries);

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
  };

  // Reset local scrub/play state whenever the timeline empties (stop-watching, CPE-400's
  // clearActivity) or a different agent is now being watched — never let a dangling interval or a
  // stale `t` bleed into the next session (AGENT-WATCH.md "off means off").
  let lastAgentName = agentName;
  $: if (entries.length === 0 || agentName !== lastAgentName) {
    lastAgentName = agentName;
    resetReplay();
  }

  // Also clear the interval outright when the component itself is torn down (drawer closed) —
  // belt-and-braces alongside the reset above so play never outlives its mount.
  onDestroy(stopPlaying);

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
      <div class="rp-clock">{new Date(t).toLocaleTimeString()} <span class="rp-frac">({Math.round(sliderFraction(range, t) * 100)}%)</span></div>

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
  {:else}
    <!-- Radar tab (CPE-1100): activity OVERLAP, not "conflict" — a raw watcher can't prove two
         touches came from unrelated actors vs. the same agent revisiting its own file, so the
         wording is deliberately hedged (agentConflicts.ts). -->
    {#if overlaps.length === 0}
      <div class="tl-empty">No overlapping activity — nothing has been touched by more than one actor recently.</div>
    {:else}
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
  .rp-slider {
    display: block;
    width: calc(100% - 20px);
    margin: 2px 10px 0;
    accent-color: var(--accent, #2f6fed);
    flex: 0 0 auto;
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
  .rp-list {
    border-top: 1px solid var(--border, #3a3a3a);
  }
  .rp-row {
    cursor: default;
  }
  .rp-current-row {
    background: color-mix(in srgb, var(--accent, #2f6fed) 14%, transparent);
    border-radius: 5px;
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
</style>

