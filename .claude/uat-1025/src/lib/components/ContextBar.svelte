<script lang="ts">
  /**
   * Aggregated folder-context strip (CPE-235). Shows every context the current
   * folder matches as a chip (icon + label = the badge + summary), and each
   * context's actions as buttons. Dumb: it dispatches the chosen action for App
   * to run. Hidden when the folder has no recognized context, so plain folders
   * are unchanged.
   */
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import type { FolderContext, FolderAction } from "../folderContext";

  export let contexts: FolderContext[] = [];

  const dispatch = createEventDispatcher<{ action: FolderAction }>();

  $: actions = contexts.flatMap((c) => c.actions);
</script>

{#if contexts.length > 0}
  <div class="context-bar" aria-label="Folder context">
    <div class="ctx-chips">
      {#each contexts as c (c.id)}
        <span class="ctx-chip" title={c.detail ? `${c.label} · ${c.detail}` : c.label}>
          <Icon name={c.icon} size={14} />
          <span class="ctx-name">{c.label}{#if c.detail} <span class="ctx-detail">· {c.detail}</span>{/if}</span>
        </span>
      {/each}
    </div>
    <div class="ctx-actions">
      {#each actions as a (a.id)}
        <button class="ctx-action" on:click={() => dispatch("action", a)}>{a.label}</button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .context-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    padding: 6px 12px;
    background: var(--surface-alt);
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  /* Tick-tack reflow: the rows wrap the pills (row+column gap) and grow height; each pill keeps its
     text on one line (nowrap) + never shrinks, so labels never wrap inside and overflow. */
  .ctx-chips { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .ctx-chip {
    flex: 0 0 auto;
    white-space: nowrap;
    display: inline-flex; align-items: center; gap: 6px;
    height: 24px; padding: 0 10px;
    border-radius: 12px;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
  }
  .ctx-detail { color: var(--text-dim); }
  .ctx-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; margin-left: auto; }
  .ctx-action {
    flex: 0 0 auto;
    white-space: nowrap;
    height: 26px; padding: 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
  }
  .ctx-action:hover { background: var(--hover); }
</style>
