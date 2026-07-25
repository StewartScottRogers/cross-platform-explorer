<script lang="ts">
  /**
   * Agent Watch session activity timeline (CPE-400) — a durable, scrollable history of every
   * filesystem action the agent took this session, newest first. The transient strip (CPE-399)
   * shows the last few fading changes; this is the full log for review. Clicking an entry
   * navigates the explorer to the change's containing folder.
   */
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import DiffPeek from "./DiffPeek.svelte";
  import DiffSideBySide from "./DiffSideBySide.svelte";
  import ConsultedFiles from "./ConsultedFiles.svelte";
  import type { TimelineEntry } from "../agentActivity";
  import { agentDiffs, diffFor, diffLineStats } from "../agentDiffs";

  export let entries: TimelineEntry[] = [];
  export let agentName = "agent";

  const dispatch = createEventDispatcher<{ navigate: string; close: void }>();

  /** Which entry's before/after peek is currently revealed (hover/focus), or null (CPE-745). */
  let openId: number | null = null;
  /** The write whose full side-by-side diff is open in the modal, or null (CPE-746). */
  let sbs: { path: string; before: string; after: string } | null = null;
  /** A write (created/modified) can carry a captured before/after diff; reads/renames/removes don't. */
  const isWrite = (k: TimelineEntry["kind"]) => k === "created" || k === "modified";

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
</script>

<aside class="timeline" aria-label="Agent activity timeline">
  <header class="tl-head">
    <span class="tl-title">Activity — {agentName}</span>
    <span class="tl-count">{entries.length}</span>
    <button class="tl-close" title="Close" on:click={() => dispatch("close")}>
      <Icon name="close" size={14} />
    </button>
  </header>
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
</style>
