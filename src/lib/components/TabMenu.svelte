<script lang="ts">
  /** Small right-click menu for a tab (CPE-357): Duplicate / Close others / Close to the
      right. App owns what each action does; this just positions and dispatches. */
  import { createEventDispatcher, onMount } from "svelte";
  import Icon from "./Icon.svelte";

  export let x = 0;
  export let y = 0;
  /** Whether there is more than one tab (enables "Close others"). */
  export let hasOthers = false;
  /** Whether tabs exist to the right of the target (enables "Close to the right"). */
  export let hasRight = false;

  const dispatch = createEventDispatcher<{ action: "duplicate" | "close-others" | "close-right"; close: void }>();

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

  function run(action: "duplicate" | "close-others" | "close-right") {
    dispatch("action", action);
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
  tabindex="-1"
  bind:this={el}
  style="left:{left}px; top:{top}px"
  on:click|stopPropagation
  on:contextmenu|stopPropagation|preventDefault
>
  <button class="row" role="menuitem" on:click={() => run("duplicate")}>
    <Icon name="plus" size={15} /> Duplicate tab
  </button>
  <button class="row" role="menuitem" disabled={!hasOthers} on:click={() => run("close-others")}>
    <Icon name="close" size={15} /> Close other tabs
  </button>
  <button class="row" role="menuitem" disabled={!hasRight} on:click={() => run("close-right")}>
    <Icon name="close" size={15} /> Close tabs to the right
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
  .row:disabled { opacity: 0.5; }
</style>
