<script lang="ts">
  /** Right-click menu for closing AI Console sessions (CPE-457, CPE-489). On a specific Agents leaf it
      offers "Close <this session>" AND "Close all"; on the toolbar button (no `sessionId`) just the
      close-all action. The owner decides what each action does; this only positions + dispatches. */
  import { createEventDispatcher, onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { sessionColor, sessionNum } from "../sessionChip";

  export let x = 0;
  export let y = 0;
  /** The close-all action's label (e.g. "Close all consoles"). */
  export let label = "Close AI Console";
  /** When set, a per-session close is offered for this session id (CPE-489). */
  export let sessionId: string | undefined = undefined;
  /** Human label for the per-session close item (e.g. "claude · sonnet-4.5"). */
  export let sessionLabel = "this session";

  const dispatch = createEventDispatcher<{ confirm: void; closeOne: string; open: string; close: void }>();

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
    <!-- Open the AI Console focused on this session's tab (CPE-532). -->
    <button class="row" role="menuitem" on:click={() => { dispatch("open", sessionId); dispatch("close"); }}>
      <span class="menu-chip" style="background:{sessionColor(sessionId)}">{sessionNum(sessionId)}</span>
      Open {sessionLabel}
    </button>
    <button class="row" role="menuitem" on:click={() => { dispatch("closeOne", sessionId); dispatch("close"); }}>
      <!-- The same colour+number chip as the leaf (CPE-493), so it's unambiguous which session closes. -->
      <span class="menu-chip" style="background:{sessionColor(sessionId)}">{sessionNum(sessionId)}</span>
      Close {sessionLabel}
    </button>
    <div class="sep" role="separator" />
  {/if}
  <button class="row" role="menuitem" on:click={() => { dispatch("confirm"); dispatch("close"); }}>
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
    white-space: nowrap; /* one line always (CPE-753) */
  }
  /* Item text uses the theme's --text (never a hard-coded colour); hover comes from the global
     `button:hover → var(--hover)` (app.css), matching ContextMenu/TabMenu. See docs/design/MENUS.md. */
  .sep { height: 1px; margin: 4px 6px; background: var(--border); }
  /* Same session-identity chip as the left-pane leaf + the AI Console tab (CPE-490/493). */
  .menu-chip {
    flex: 0 0 auto;
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 5px;
    color: #fff;
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }
</style>
