<script lang="ts">
  // Command Palette (CPE-602): Ctrl+Shift+P → type to find and run any action. Renders the pure
  // `filterCommands` result; keyboard-first (↑/↓/Enter/Esc). Theme-correct via CSS variables.
  import { createEventDispatcher, tick } from "svelte";
  import { filterCommands, isEnabled, type Command } from "../commandPalette";

  export let commands: Command[] = [];

  const dispatch = createEventDispatcher<{ close: void }>();

  let query = "";
  let active = 0;
  let listEl: HTMLDivElement | undefined;

  $: results = filterCommands(commands, query);
  // Keep the highlight in range as the filter narrows.
  $: if (active >= results.length) active = Math.max(0, results.length - 1);

  function run(i: number) {
    const hit = results[i];
    if (!hit || !isEnabled(hit.command)) return;
    dispatch("close");
    hit.command.run();
  }

  async function move(delta: number) {
    if (!results.length) return;
    active = (active + delta + results.length) % results.length;
    await tick();
    listEl?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); dispatch("close"); }
    else if (e.key === "ArrowDown") { e.preventDefault(); move(1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); move(-1); }
    else if (e.key === "Enter") { e.preventDefault(); run(active); }
  }
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="cp-overlay" on:click|self={() => dispatch("close")}>
  <div class="cp-panel" role="dialog" aria-label="Command palette">
    <!-- svelte-ignore a11y-autofocus -->
    <input
      class="cp-input"
      autofocus
      bind:value={query}
      on:keydown={onKeydown}
      placeholder="Type a command…"
      spellcheck="false"
      aria-label="Search commands"
    />
    <div class="cp-list" bind:this={listEl}>
      {#if results.length === 0}
        <div class="cp-empty">No matching commands.</div>
      {:else}
        {#each results as { command }, i (command.id)}
          <!-- svelte-ignore a11y-no-static-element-interactions -->
          <div
            class="cp-row"
            class:active={i === active}
            class:disabled={!isEnabled(command)}
            data-i={i}
            role="button"
            tabindex="-1"
            on:mousemove={() => (active = i)}
            on:click={() => run(i)}
          >
            {#if command.group}<span class="cp-group">{command.group}</span>{/if}
            <span class="cp-label">{command.label}</span>
            {#if command.shortcut}<span class="cp-shortcut">{command.shortcut}</span>{/if}
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .cp-overlay {
    position: fixed; inset: 0; z-index: 200;
    background: rgba(0, 0, 0, 0.3);
    display: flex; justify-content: center; align-items: flex-start;
    padding-top: 12vh;
  }
  .cp-panel {
    width: min(640px, 92vw);
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.35);
    overflow: hidden;
    display: flex; flex-direction: column;
    max-height: 70vh;
  }
  .cp-input {
    font: inherit; font-size: 15px;
    padding: 12px 14px;
    border: 0; border-bottom: 1px solid var(--border);
    background: transparent; color: var(--text);
    outline: none; width: 100%;
  }
  .cp-list { overflow-y: auto; padding: 6px; }
  .cp-empty { padding: 16px 14px; color: var(--text-dim); font-size: 13px; }
  .cp-row {
    display: flex; align-items: center; gap: 10px;
    padding: 8px 10px; border-radius: 6px; cursor: pointer;
    font-size: 13px; white-space: nowrap;
  }
  .cp-row.active { background: var(--accent); color: #fff; }
  .cp-row.active .cp-group, .cp-row.active .cp-shortcut { color: rgba(255, 255, 255, 0.75); }
  .cp-row.disabled { opacity: 0.45; cursor: default; }
  .cp-group {
    flex: 0 0 auto; font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em;
    color: var(--text-faint); min-width: 74px;
  }
  .cp-label { flex: 1 1 auto; overflow: hidden; text-overflow: ellipsis; }
  .cp-shortcut {
    flex: 0 0 auto; font-size: 11px; color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
</style>
