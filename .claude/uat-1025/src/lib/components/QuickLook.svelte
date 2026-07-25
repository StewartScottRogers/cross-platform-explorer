<script lang="ts">
  // Quick-look overlay (CPE-645, epic CPE-615): a full-screen preview of the selected image, opened
  // with Space. Navigation (←/→, Esc) is owned by App's keydown handler so there's no dual-listener
  // race; this component just renders the current image + mouse controls.
  import { createEventDispatcher } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import Icon from "./Icon.svelte";

  export let images: { path: string; name: string }[] = [];
  export let index = 0;

  const dispatch = createEventDispatcher<{ close: void; prev: void; next: void }>();
  $: current = images[index];
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="ql" on:click={() => dispatch("close")}>
  {#if images.length > 1}
    <button class="nav prev" title="Previous (←)" aria-label="Previous" on:click|stopPropagation={() => dispatch("prev")}>
      <Icon name="back" size={22} />
    </button>
    <button class="nav next" title="Next (→)" aria-label="Next" on:click|stopPropagation={() => dispatch("next")}>
      <Icon name="forward" size={22} />
    </button>
  {/if}
  {#if current}
    <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
    <img class="img" src={convertFileSrc(current.path)} alt={current.name} on:click|stopPropagation />
    <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
    <div class="bar" on:click|stopPropagation>
      <span class="name" title={current.name}>{current.name}</span>
      {#if images.length > 1}<span class="counter">{index + 1} / {images.length}</span>{/if}
    </div>
  {/if}
  <button class="close" title="Close (Esc / Space)" aria-label="Close" on:click|stopPropagation={() => dispatch("close")}>
    <Icon name="close" size={18} />
  </button>
</div>

<style>
  .ql {
    position: fixed; inset: 0; z-index: 260;
    background: rgba(0, 0, 0, 0.88);
    display: grid; place-items: center;
  }
  .img {
    max-width: 92vw; max-height: 86vh;
    object-fit: contain;
    box-shadow: 0 12px 60px rgba(0, 0, 0, 0.6);
    border-radius: 4px;
  }
  .bar {
    position: fixed; left: 50%; bottom: 18px; transform: translateX(-50%);
    display: flex; align-items: center; gap: 12px; max-width: 80vw;
    padding: 6px 14px; border-radius: 999px;
    background: rgba(0, 0, 0, 0.55); color: #fff; font-size: 13px;
  }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .counter { color: rgba(255, 255, 255, 0.7); font-variant-numeric: tabular-nums; }
  .nav, .close {
    position: fixed; display: grid; place-items: center;
    background: rgba(0, 0, 0, 0.4); color: #fff;
    border-radius: 999px; width: 44px; height: 44px;
  }
  .nav:hover, .close:hover { background: rgba(0, 0, 0, 0.7); }
  .prev { left: 18px; top: 50%; transform: translateY(-50%); }
  .next { right: 18px; top: 50%; transform: translateY(-50%); }
  .close { top: 18px; right: 18px; width: 36px; height: 36px; }
</style>
