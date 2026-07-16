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
  import { t, locale, SUPPORTED_LOCALES, type Locale } from "../i18n";

  const dispatch = createEventDispatcher<{ select: string }>();

  // Item labels are i18n keys (CPE-481) resolved via `$t` at render; `hint` is a key combo (not
  // translated). A `sep` is a divider.
  type Item = { id: string; labelKey: string; hint?: string } | { sep: true };
  interface Menu {
    id: string;
    /** i18n key for the top-level title (falls back to English/key). */
    labelKey: string;
    items: Item[];
  }

  const menus: Menu[] = [
    {
      id: "file",
      labelKey: "menu.file",
      items: [{ id: "exit", labelKey: "mi.exit", hint: "Alt+F4" }],
    },
    {
      id: "tools",
      labelKey: "menu.tools",
      items: [
        { id: "content-search", labelKey: "mi.searchInFiles", hint: "Ctrl+Shift+F" },
        { id: "find-duplicates", labelKey: "mi.findDuplicates" },
        { sep: true },
        { id: "copy-file-names", labelKey: "mi.copyFileNames" },
        { id: "copy-file-list", labelKey: "mi.copyFileList" },
        { id: "save-file-list", labelKey: "mi.saveFileList" },
      ],
    },
    {
      id: "app",
      labelKey: "menu.application",
      items: [
        { id: "check-updates", labelKey: "mi.checkUpdates" },
        { id: "settings", labelKey: "mi.settings" },
        { sep: true },
        { id: "shortcuts", labelKey: "mi.shortcuts", hint: "F1" },
        { id: "documentation", labelKey: "mi.documentation" },
        { id: "about", labelKey: "mi.about" },
      ],
    },
  ];

  function pickLocale(code: Locale) {
    locale.set(code);
    close();
  }

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
        {$t(menu.labelKey)}
      </button>

      {#if openId === menu.id}
        <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
        <div
          class="menu-drop"
          role="menu"
          tabindex="-1"
          aria-label={$t(menu.labelKey)}
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
                {$t(item.labelKey)}
                {#if item.hint}<span class="hint">{item.hint}</span>{/if}
              </button>
            {/if}
          {/each}
        </div>
      {/if}
    </div>
  {/each}

  <!-- Language picker (CPE-362): switches the app locale live; a check marks the active one. -->
  <div class="menu-wrap">
    <button
      class="menu-title"
      class:active={openId === "language"}
      type="button"
      role="menuitem"
      aria-haspopup="menu"
      aria-expanded={openId === "language"}
      title={$t("menu.language")}
      on:click|stopPropagation={() => toggle("language")}
      on:mouseenter={() => hover("language")}
    >
      🌐 {$t("menu.language")}
    </button>
    {#if openId === "language"}
      <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
      <div class="menu-drop" role="menu" tabindex="-1" aria-label={$t("menu.language")} on:click|stopPropagation>
        {#each SUPPORTED_LOCALES as l (l.code)}
          <button class="mb-item" role="menuitemradio" aria-checked={$locale === l.code} on:click={() => pickLocale(l.code)}>
            <span class="check" aria-hidden="true">{$locale === l.code ? "✓" : ""}</span>
            {l.name}
          </button>
        {/each}
      </div>
    {/if}
  </div>
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
  .check {
    display: inline-block;
    width: 14px;
    color: var(--accent);
  }
  .mb-sep {
    height: 1px;
    background: var(--border);
    margin: 4px 6px;
  }
</style>
