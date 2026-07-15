<script lang="ts">
  /**
   * A classic desktop menu bar pinned to the very top of the window (CPE-227,
   * extended in CPE-229). It is data-driven and dumb: it renders the `menus`
   * table below and dispatches a `select` event carrying the chosen item id.
   * App.svelte owns what each id does. Standard menubar behaviour: click a title
   * to open it, hovering another title while one is open switches to it, and it
   * closes on Escape, click-away, or after a choice.
   */
  import { createEventDispatcher } from "svelte";

  const dispatch = createEventDispatcher<{ select: string }>();

  type Item = { id: string; label: string; hint?: string } | { sep: true };
  interface Menu {
    id: string;
    label: string;
    items: Item[];
  }

  const menus: Menu[] = [
    {
      id: "file",
      label: "File",
      items: [{ id: "exit", label: "Exit", hint: "Alt+F4" }],
    },
    {
      id: "tools",
      label: "Tools",
      items: [
        { id: "content-search", label: "Search in files…", hint: "Ctrl+Shift+F" },
        { id: "find-duplicates", label: "Find duplicate files…" },
      ],
    },
    {
      id: "app",
      label: "Application",
      items: [
        { id: "pop-out-preview", label: "Pop out preview", hint: "Ctrl+Shift+O" },
        { id: "check-updates", label: "Check for Updates…" },
        { id: "settings", label: "Settings…" },
        { sep: true },
        { id: "shortcuts", label: "Keyboard shortcuts", hint: "F1" },
        { id: "documentation", label: "Documentation" },
        { id: "about", label: "About" },
      ],
    },
  ];

  /** Id of the open top-level menu, or null when the bar is idle. */
  let openId: string | null = null;

  const isSep = (i: Item): i is { sep: true } => "sep" in i;

  function toggle(id: string) {
    openId = openId === id ? null : id;
  }
  /** While a menu is open, sliding onto another title opens that one instead. */
  function hover(id: string) {
    if (openId !== null) openId = id;
  }
  function close() {
    openId = null;
  }
  function choose(id: string) {
    close();
    dispatch("select", id);
  }
</script>

<svelte:window
  on:click={close}
  on:keydown={(e) => e.key === "Escape" && close()}
/>

<div class="menubar" role="menubar" aria-label="Application menu">
  {#each menus as menu (menu.id)}
    <div class="menu-wrap">
      <button
        class="menu-title"
        class:active={openId === menu.id}
        type="button"
        role="menuitem"
        aria-haspopup="menu"
        aria-expanded={openId === menu.id}
        on:click|stopPropagation={() => toggle(menu.id)}
        on:mouseenter={() => hover(menu.id)}
      >
        {menu.label}
      </button>

      {#if openId === menu.id}
        <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
        <div
          class="menu-drop"
          role="menu"
          tabindex="-1"
          aria-label={menu.label}
          on:click|stopPropagation
        >
          {#each menu.items as item}
            {#if isSep(item)}
              <div class="mb-sep" role="separator" />
            {:else}
              <button
                class="mb-item"
                role="menuitem"
                on:click={() => choose(item.id)}
              >
                {item.label}
                {#if item.hint}<span class="hint">{item.hint}</span>{/if}
              </button>
            {/if}
          {/each}
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .menubar {
    display: flex;
    align-items: center;
    height: 28px;
    padding: 0 4px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
    flex: none;
    user-select: none;
  }
  .menu-wrap {
    position: relative;
  }
  .menu-title {
    height: 24px;
    padding: 0 10px;
    border-radius: var(--radius);
    color: var(--text);
    font-size: 13px;
  }
  .menu-title.active {
    background: var(--active);
  }
  .menu-drop {
    position: absolute;
    top: calc(100% + 2px);
    left: 0;
    z-index: 60;
    min-width: 200px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.16);
  }
  .mb-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    text-align: left;
    border-radius: var(--radius);
    color: var(--text);
  }
  .hint {
    margin-left: auto;
    color: var(--text-faint);
    font-size: 12px;
  }
  .mb-sep {
    height: 1px;
    background: var(--border);
    margin: 4px 6px;
  }
</style>
