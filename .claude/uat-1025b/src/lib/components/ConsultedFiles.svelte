<script lang="ts">
  /**
   * Agent Watch "consulted files" panel (CPE-741, epic CPE-726). A durable, deduped, newest-first list
   * of the files the agent has READ this session (from the CPE-405 tool-output read stream), so you can
   * see the context it gathered — distinct from the fading activity annotations and the interleaved
   * timeline. Reads are the weakest signal, so it's styled dim/hollow like the CPE-405 read badge.
   * Clicking an entry navigates the explorer to the file's folder. Shows nothing when empty.
   */
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { agentConsulted } from "../agentActivity";

  const dispatch = createEventDispatcher<{ navigate: string }>();

  let collapsed = false;
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "");
  const baseOf = (p: string) => norm(p).split("/").pop() || p;
  const dirOf = (p: string) => {
    const n = norm(p);
    const i = n.lastIndexOf("/");
    return i > 0 ? n.slice(0, i) : "";
  };
</script>

{#if $agentConsulted.length > 0}
  <section class="consulted" aria-label="Files the agent has consulted">
    <button class="c-head" on:click={() => (collapsed = !collapsed)} aria-expanded={!collapsed}>
      <Icon name={collapsed ? "chev-right" : "chev-down"} size={14} />
      <span class="c-title">Consulted</span>
      <span class="c-count">{$agentConsulted.length}</span>
    </button>
    {#if !collapsed}
      <ul class="c-list">
        {#each $agentConsulted as e (e.path)}
          <li>
            <button class="c-row" title={e.path} on:click={() => dispatch("navigate", dirOf(e.path))}>
              <span class="c-badge">read</span>
              <span class="c-name">{baseOf(e.path)}</span>
              {#if e.count > 1}<span class="c-mult" aria-label="read {e.count} times">×{e.count}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

<style>
  .consulted {
    border-bottom: 1px solid var(--border, #3a3a3a);
  }
  .c-head {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 10px;
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    opacity: 0.85;
  }
  .c-title {
    flex: 1;
    text-align: left;
  }
  .c-count {
    font-size: 11px;
    font-weight: 600;
    padding: 1px 7px;
    border-radius: 999px;
    color: var(--text-muted, #9a9a9a);
    border: 1px solid var(--border, #4a4a4a);
  }
  .c-list {
    list-style: none;
    margin: 0;
    padding: 0 4px 4px;
    max-height: 30vh;
    overflow-y: auto;
  }
  .c-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 4px 8px;
    border: 0;
    background: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
    border-radius: 5px;
    font: inherit;
    font-size: 12.5px;
  }
  .c-row:hover {
    background: rgba(128, 128, 128, 0.14);
  }
  /* CPE-405: a read is the weakest signal — hollow, muted, subordinate to change badges. */
  .c-badge {
    flex: 0 0 auto;
    padding: 0 6px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    line-height: 16px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-muted, #9a9a9a);
    border: 1px solid var(--border, #4a4a4a);
  }
  .c-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .c-mult {
    flex: 0 0 auto;
    font-size: 10.5px;
    opacity: 0.6;
    font-variant-numeric: tabular-nums;
  }
</style>
