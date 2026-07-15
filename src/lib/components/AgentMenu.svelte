<script lang="ts">
  /** Right-click menu for closing the AI Console (CPE-457) — shown on an Agents leaf ("Close AI
      Console") and on the AI Console toolbar button ("Close all consoles"). The owner decides the
      label + what confirming does; this just positions and dispatches. */
  import { createEventDispatcher, onMount } from "svelte";
  import Icon from "./Icon.svelte";

  export let x = 0;
  export let y = 0;
  export let label = "Close AI Console";

  const dispatch = createEventDispatcher<{ confirm: void; close: void }>();

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
  tabindex="-1"
  bind:this={el}
  style="left:{left}px; top:{top}px"
  on:click|stopPropagation
  on:contextmenu|stopPropagation|preventDefault
>
  <button class="row danger" role="menuitem" on:click={() => { dispatch("confirm"); dispatch("close"); }}>
    <Icon name="close" size={15} /> {label}
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
  }
  .row.danger { color: #d05656; }
</style>
