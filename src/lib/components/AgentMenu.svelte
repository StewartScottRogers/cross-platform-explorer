<script lang="ts">
  /** Right-click menu for closing AI Console sessions (CPE-457, CPE-489). On a specific Agents leaf it
      offers "Close <this session>" AND "Close all"; on the toolbar button (no `sessionId`) just the
      close-all action. The owner decides what each action does; this only positions + dispatches. */
  import { createEventDispatcher, onMount } from "svelte";
  import Icon from "./Icon.svelte";

  export let x = 0;
  export let y = 0;
  /** The close-all action's label (e.g. "Close all consoles"). */
  export let label = "Close AI Console";
  /** When set, a per-session close is offered for this session id (CPE-489). */
  export let sessionId: string | undefined = undefined;
  /** Human label for the per-session close item (e.g. "claude · sonnet-4.5"). */
  export let sessionLabel = "this session";

  const dispatch = createEventDispatcher<{ confirm: void; closeOne: string; close: void }>();

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
  {#if sessionId}
    <button class="row danger" role="menuitem" on:click={() => { dispatch("closeOne", sessionId); dispatch("close"); }}>
      <Icon name="close" size={15} /> Close {sessionLabel}
    </button>
    <div class="sep" role="separator" />
  {/if}
  <button class="row" class:danger={!sessionId} role="menuitem" on:click={() => { dispatch("confirm"); dispatch("close"); }}>
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
  .row:hover { background: var(--active); }
  .sep { height: 1px; margin: 4px 6px; background: var(--border); }
</style>
