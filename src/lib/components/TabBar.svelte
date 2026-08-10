<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import type { DensityMode } from "../types";

  export let tabs: { id: number; title: string }[] = [];
  export let activeId: number;
  // Row/chrome density (CPE-1526, foundation slice of epic CPE-1488): received but not yet read —
  // this ticket only wires the prop through for CPE-1528 to consume later. "comfortable" (the
  // default) leaves the tab strip unchanged.
  export let density: DensityMode = "comfortable";
  // `export const` is NOT the fix svelte-check's hint suggests: it makes the prop non-writable, so a
  // parent's passed value is silently dropped (confirmed against the compiled writable-props check) —
  // this dummy reference just satisfies the unused-export-let lint until CPE-1528 reads density for real.
  const _densityRef = density;

  const dispatch = createEventDispatcher<{
    select: number; close: number; new: void;
    menu: { id: number; x: number; y: number };
  }>();
</script>

<div class="tabbar">
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
