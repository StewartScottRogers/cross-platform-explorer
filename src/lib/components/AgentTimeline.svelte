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
  import { agentDiffs, diffFor, diffLineStats } from "../agentDiffs";
  import {
    sliderRange,
    sliderFraction,
    entriesUpTo,
    currentEntry,
    nextTimestamp,
    prevTimestamp,
    isMultiplyEdited,
    isWriteKind,
  } from "../agentReplay";

  export let entries: TimelineEntry[] = [];
  export let agentName = "agent";

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

  // ---------- Replay tab (CPE-1094) ----------
  /** "Live" (default, today's list) vs "Replay" (scrub through the session's history). */
  let tab: "live" | "replay" = "live";

  /** Selected scrub position — an epoch ms timestamp somewhere in `[range.firstAt, range.lastAt]`. */
  let t = 0;
  let playing = false;
  let playTimer: ReturnType<typeof setInterval> | null = null;

  /** How often play advances to the next entry (ticket: ~1 entry / 400ms). */
  const PLAY_INTERVAL_MS = 400;

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
  const togglePlay = () => {
    if (playing) {
      stopPlaying();
      return;
    }
    if (!range || atEnd) return; // nothing to play
    playing = true;
    playTimer = setInterval(() => {
      const nxt = nextTimestamp(entries, t);
      if (nxt === null) {
        stopPlaying();
        return;
      }
      t = nxt;
    }, PLAY_INTERVAL_MS);
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
  {:else}
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
</style>

