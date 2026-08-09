<script lang="ts">
  /** Right-click menu for a saved-connection row in the Network sidebar section (CPE-1513, epic CPE-1498).
      Connect/Disconnect (whichever applies to the row's current state) · Edit · Forget. Mirrors AgentMenu's
      `.ctx`/`.row` structure — theme-only colours, no red text, per docs/design/MENUS.md.
      "Mount as drive" is deliberately OMITTED (needs CPE-1500 OS-mount, not built — no stubbed dead item). */
  import { createEventDispatcher, onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import type { ConnState } from "../network";

  export let x = 0;
  export let y = 0;
  export let name = "";
  export let state: ConnState = "disconnected";

  const dispatch = createEventDispatcher<{
    connect: void;
    disconnect: void;
    edit: void;
    forget: void;
    close: void;
  }>();

  let el: HTMLDivElement;
  let left = x;
  let top = y;

  onMount(() => {
    const rect = el.getBoundingClientRect();
    const pad = 6;
    left = Math.max(pad, Math.min(x, window.innerWidth - rect.width - pad));
    top = Math.max(pad, Math.min(y, window.innerHeight - rect.height - pad));
    el.focus();
  });

  function act(fn: () => void) {
    fn();
    dispatch("close");
  }
</script>

<svelte:window
  on:click={() => dispatch("close")}
  on:contextmenu|preventDefault={() => dispatch("close")}
  on:keydown={(e) => e.key === "Escape" && dispatch("close")}
/>

<!-- svelte-ignore a11y-no-noninteractive-element-interactions a11y-click-events-have-key-events -->
<div
  class="ctx"
  role="menu"
  aria-label={`${name} actions`}
  tabindex="-1"
  bind:this={el}
  style="left:{left}px; top:{top}px"
  on:click|stopPropagation
  on:contextmenu|stopPropagation|preventDefault
>
  {#if state === "connected"}
    <button class="row" role="menuitem" on:click={() => act(() => dispatch("disconnect"))}>
      <Icon name="ban" size={15} /> Disconnect
    </button>
  {:else}
    <button class="row" role="menuitem" on:click={() => act(() => dispatch("connect"))}>
      <Icon name="globe" size={15} /> Connect
    </button>
  {/if}
  <button class="row" role="menuitem" on:click={() => act(() => dispatch("edit"))}>
    <Icon name="rename" size={15} /> Edit…
  </button>
  <div class="sep" role="separator" />
  <button class="row" role="menuitem" on:click={() => act(() => dispatch("forget"))}>
    <Icon name="delete" size={15} /> Forget
  </button>
</div>

<style>
  .ctx {
    position: fixed;
    z-index: 100;
    min-width: 190px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.16);
    outline: none;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    text-align: left;
    border-radius: var(--radius);
    white-space: nowrap;
  }
  /* Item text uses the theme's --text (never a hard-coded colour, never red for "Forget" — MENUS.md);
     hover comes from the global `button:hover → var(--hover)` (app.css). */
  .sep { height: 1px; margin: 4px 6px; background: var(--border); }
</style>
