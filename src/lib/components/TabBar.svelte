<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import type { DensityMode } from "../types";

  export let tabs: { id: number; title: string }[] = [];
  export let activeId: number;
  // Row/chrome density (CPE-1526 threaded the prop, CPE-1528 consumes it): "compact" applies the
  // `.compact` CSS class on the root `.tabbar` (see app.css's "Compact density (CPE-1528)" rules) —
  // a thinner strip/pitch that still honors TABS.md's accent-top-bar active tab + recessed-chip
  // inactive tabs (unchanged box-shadow/border rules, just shrunk). "comfortable" (the default)
  // renders pixel-identical to before this ticket.
  export let density: DensityMode = "comfortable";

  const dispatch = createEventDispatcher<{
    select: number; close: number; new: void;
    menu: { id: number; x: number; y: number };
  }>();
</script>

<div class="tabbar" class:compact={density === "compact"}>
  {#each tabs as tab (tab.id)}
    <button
      class="tab"
      class:active={tab.id === activeId}
      on:click={() => dispatch("select", tab.id)}
      on:contextmenu|preventDefault={(e) => dispatch("menu", { id: tab.id, x: e.clientX, y: e.clientY })}
      title={tab.title}
    >
      <Icon name="home" size={15} />
      <span class="tab-label">{tab.title}</span>
      {#if tabs.length > 1}
        <!-- svelte-ignore a11y-no-static-element-interactions a11y-click-events-have-key-events -->
        <span
          class="tab-close"
          role="button"
          tabindex="-1"
          title={$t("app.closeTab")}
          on:click|stopPropagation={() => dispatch("close", tab.id)}
          on:keydown|stopPropagation
        >
          <Icon name="close" size={12} />
        </span>
      {/if}
    </button>
  {/each}
  <button class="tab-new" title={$t("app.newTab")} on:click={() => dispatch("new")}>
    <Icon name="plus" size={15} />
  </button>
</div>
