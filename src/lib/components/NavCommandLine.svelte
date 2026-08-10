<script lang="ts">
  /**
   * Navigation Mode's mini ":" command-line bar (CPE-1554, epic CPE-1487). Resolves what the user
   * types against the caller-supplied `commands` list via `filterNavCommands` (which itself reuses
   * `commandPalette.ts`'s own matcher — no forked registry). Mirrors `CommandPalette.svelte`'s
   * keyboard idiom (↑/↓ to move, Enter to pick, Escape to cancel) but as a bottom-of-pane status-bar
   * prompt instead of a centered modal, and it never calls `command.run()` itself — it only reports
   * the resolved `Command` (or that nothing matched) and lets the caller decide what to do with it.
   *
   * INERT: this component is unreachable from the running app until CPE-1556 mounts it and feeds it
   * the real `paletteCommands` array, wiring it to Navigation Mode's `startCommand` intent.
   */
  import { createEventDispatcher, tick } from "svelte";
  import { filterNavCommands } from "../navCommandLine";
  import type { Command } from "../commandPalette";

  /** The candidate verbs to filter against — supplied by the caller, not sourced internally. */
  export let commands: Command[] = [];

  const dispatch = createEventDispatcher<{ run: Command; cancel: void }>();

  let query = "";
  let active = 0;
  let listEl: HTMLDivElement | undefined;

  $: results = filterNavCommands(commands, query);
  // Keep the highlight in range as the filter narrows.
  $: if (active >= results.length) active = Math.max(0, results.length - 1);

  function run(i: number) {
    const hit = results[i];
    if (!hit) return;
    dispatch("run", hit);
  }

  async function move(delta: number) {
    if (!results.length) return;
    active = (active + delta + results.length) % results.length;
    await tick();
    listEl?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); dispatch("cancel"); }
    else if (e.key === "ArrowDown") { e.preventDefault(); move(1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); move(-1); }
    else if (e.key === "Enter") { e.preventDefault(); run(active); }
  }
</script>

<div class="ncl-wrap" data-testid="nav-command-line">
  <div class="ncl-bar">
    <span class="ncl-prefix" aria-hidden="true">:</span>
    <!-- svelte-ignore a11y-autofocus -->
    <input
      class="ncl-input"
      autofocus
      bind:value={query}
      on:keydown={onKeydown}
      spellcheck="false"
      aria-label="Navigation command"
      data-testid="ncl-input"
    />
  </div>
  {#if results.length === 0}
    <div class="ncl-empty" data-testid="ncl-empty">No matching command</div>
  {:else}
    <div class="ncl-list" bind:this={listEl}>
      {#each results as command, i (command.id)}
        <!-- svelte-ignore a11y-no-static-element-interactions a11y-click-events-have-key-events -->
        <div
          class="ncl-row"
          class:active={i === active}
          data-i={i}
          data-testid="ncl-row"
          role="button"
          tabindex="-1"
          on:mousemove={() => (active = i)}
          on:click={() => run(i)}
        >
          {#if command.group}<span class="ncl-group">{command.group}</span>{/if}
          <span class="ncl-label">{command.label}</span>
          {#if command.shortcut}<span class="ncl-shortcut">{command.shortcut}</span>{/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .ncl-wrap {
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--dialog-border);
    border-radius: 8px;
    box-shadow: 0 -4px 20px rgba(0, 0, 0, 0.2);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    max-height: 40vh;
  }
  .ncl-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }
  .ncl-prefix {
    font-family: ui-monospace, monospace;
    font-weight: 700;
    color: var(--text-dim);
    flex: 0 0 auto;
  }
  .ncl-input {
    font-family: ui-monospace, monospace;
    font-size: 13px;
    border: 0;
    background: transparent;
    color: var(--text);
    outline: none;
    width: 100%;
  }
  .ncl-list { overflow-y: auto; padding: 4px; }
  .ncl-empty { padding: 10px 14px; color: var(--text-dim); font-size: 12.5px; }
  .ncl-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 12.5px;
    white-space: nowrap;
  }
  /* Selected-row treatment follows the menu standard (docs/design/MENUS.md): a `--selection` tint,
     not a solid accent fill, so item text stays `var(--text)` — no white-on-accent override needed. */
  .ncl-row.active { background: var(--selection); }
  .ncl-group {
    flex: 0 0 auto;
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-faint);
    min-width: 68px;
  }
  .ncl-label { flex: 1 1 auto; overflow: hidden; text-overflow: ellipsis; }
  .ncl-shortcut {
    flex: 0 0 auto;
    font-size: 10.5px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
</style>
