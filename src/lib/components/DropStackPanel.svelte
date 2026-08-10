<script lang="ts">
  // Drop Stack panel (CPE-1532, epic CPE-1489): the dockable panel that actually shows what's on the
  // stack CPE-1530's store accumulated. Off by default (PURPOSE.md tiebreaker: zero cost on the plain
  // explorer when unused) — this file renders only a small toggle handle until the user opens it. This
  // ticket ships listing + per-item remove + clear-all only; Move-all/Copy-all land in CPE-1533 (a
  // separate ticket touching this same component file, sequenced after this one).
  import { dropStackEntries, removeFromDropStack, clearDropStack } from "../dropStack";
  import Icon from "./Icon.svelte";

  let open = false;

  function toggle(): void {
    open = !open;
  }

  /** Just the filename for the pill's visible label; the full path is the pill's title/tooltip. */
  function basename(path: string): string {
    const norm = path.replace(/\\/g, "/").replace(/\/+$/, "");
    const parts = norm.split("/").filter(Boolean);
    return parts.length ? parts[parts.length - 1] : path;
  }
</script>

<div class="drop-stack-handle">
  <button
    class="handle-btn"
    aria-expanded={open}
    aria-label={open ? "Hide Drop Stack" : "Show Drop Stack"}
    title="Drop Stack"
    on:click={toggle}
  >
    <Icon name="paperclip" size={14} />
    <span class="handle-label">Drop Stack</span>
    {#if $dropStackEntries.length > 0}
      <span class="handle-count">{$dropStackEntries.length}</span>
    {/if}
  </button>
</div>

{#if open}
  <div class="drop-stack-panel" role="region" aria-label="Drop Stack">
    <div class="dsp-header">
      <span class="dsp-title">Drop Stack</span>
      <div class="dsp-header-actions">
        {#if $dropStackEntries.length > 0}
          <button class="dsp-clear" on:click={() => clearDropStack()}>Clear all</button>
        {/if}
        <button class="x" title="Close" aria-label="Close Drop Stack panel" on:click={() => (open = false)}>
          <Icon name="close" size={12} />
        </button>
      </div>
    </div>

    {#if $dropStackEntries.length === 0}
      <div class="dsp-empty">Drop Stack is empty — right-click a file → Add to Drop Stack</div>
    {:else}
      <div class="dsp-pills">
        {#each $dropStackEntries as entry (entry.path)}
          <span class="dsp-pill" title={entry.path}>
            <span class="dsp-pill-name">{basename(entry.path)}</span>
            <button
              class="dsp-pill-x"
              title="Remove from Drop Stack"
              aria-label={`Remove ${basename(entry.path)} from Drop Stack`}
              on:click={() => removeFromDropStack(entry.path)}
            >
              <Icon name="close" size={10} />
            </button>
          </span>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .drop-stack-handle {
    position: fixed;
    left: 14px;
    bottom: 14px;
    z-index: 149;
  }
  .handle-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.14);
  }
  .handle-btn:hover { background: var(--hover); }
  .handle-label { white-space: nowrap; }
  .handle-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-size: 10px;
    font-variant-numeric: tabular-nums;
  }

  .drop-stack-panel {
    position: fixed;
    left: 14px;
    bottom: 50px;
    z-index: 150;
    width: min(360px, 90vw);
    max-height: min(50vh, 420px);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.22);
  }
  .dsp-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .dsp-title { flex: 1; font-size: 13px; font-weight: 600; }
  .dsp-header-actions { display: flex; align-items: center; gap: 6px; flex: none; }
  .dsp-clear {
    height: 22px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 11px;
    white-space: nowrap;
  }
  .dsp-clear:hover { background: var(--hover); }
  .x { width: 22px; height: 22px; display: grid; place-items: center; border-radius: 4px; flex: 0 0 auto; }
  .x:hover { background: var(--surface-alt); }

  .dsp-empty {
    padding: 16px 12px;
    color: var(--text-dim);
    font-size: 12px;
  }

  /* Tick-tack reflow: the container wraps pills onto more rows and grows height; each pill keeps its
     text on one line (nowrap) and never shrinks, with a max-width + ellipsis for a long name/path. */
  .dsp-pills {
    display: flex;
    align-items: flex-start;
    align-content: flex-start;
    flex-wrap: wrap;
    gap: 6px;
    padding: 10px;
    overflow-y: auto;
  }
  .dsp-pill {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 100%;
    height: 24px;
    padding: 0 4px 0 10px;
    border-radius: 12px;
    border: 1px solid var(--border-strong);
    background: var(--surface-alt);
    color: var(--text);
    font-size: 12px;
  }
  .dsp-pill-name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 220px;
  }
  .dsp-pill-x {
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    flex: 0 0 auto;
  }
  .dsp-pill-x:hover { background: var(--surface); }
</style>
