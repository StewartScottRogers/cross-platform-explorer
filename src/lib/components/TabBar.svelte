<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";

  export let tabs: { id: number; title: string }[] = [];
  export let activeId: number;

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
