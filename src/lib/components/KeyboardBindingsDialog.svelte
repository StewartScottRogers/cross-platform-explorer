<script lang="ts">
  /**
   * Keyboard shortcuts VIEWER (CPE-1548, epic CPE-1484 "hotkey customization"). Read-only —
   * remapping/reset controls land in CPE-1549 onto this same file. Mirrors
   * `ShortcutsDialog.svelte`'s backdrop/dialog/Escape/click-away structure and grouped-column
   * layout (the same visual language as the "?" cheat sheet), but is driven by `keymap.ts`'s live
   * `ACTIONS` registry + the caller-supplied effective `keymap` (via `chordFor`/`formatChord`)
   * instead of the static `SHORTCUT_GROUPS` table, so it reflects real user overrides once CPE-1549
   * lands. `ShortcutsDialog.svelte` itself is untouched and stays as the quick "?" reference.
   */
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { ACTIONS, chordFor, formatChord, type Keymap } from "../keymap";

  export let keymap: Keymap;

  const dispatch = createEventDispatcher<{ close: void }>();

  let query = "";

  // Group order follows ACTIONS' registry order (Navigation/Tabs/Selection/File actions/View/
  // General), same as ShortcutsDialog's SHORTCUT_GROUPS order — no separate sort needed.
  $: groups = (() => {
    const q = query.trim().toLowerCase();
    const order: string[] = [];
    const byGroup = new Map<string, { description: string; chord: string }[]>();
    for (const action of ACTIONS) {
      const chord = chordFor(keymap, action.id);
      if (q && !action.description.toLowerCase().includes(q) && !action.group.toLowerCase().includes(q)) {
        continue;
      }
      if (!byGroup.has(action.group)) {
        byGroup.set(action.group, []);
        order.push(action.group);
      }
      byGroup.get(action.group)!.push({ description: action.description, chord });
    }
    return order.map((title) => ({ title, items: byGroup.get(title)! }));
  })();
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Keyboard shortcuts"
    on:click|stopPropagation
  >
    <h2>
      <span class="ic"><Icon name="keyboard" size={18} /></span>
      Keyboard shortcuts
      <button class="x" title="Close (Esc)" aria-label="Close" on:click={() => dispatch("close")}>
        <Icon name="close" size={16} />
      </button>
    </h2>

    <div class="search">
      <Icon name="search" size={14} />
      <input
        type="text"
        placeholder="Filter shortcuts…"
        bind:value={query}
        data-testid="keyboard-bindings-filter"
        spellcheck="false"
        autocomplete="off"
      />
    </div>

    <div class="groups" data-testid="keyboard-bindings-groups">
      {#if groups.length === 0}
        <div class="empty">No shortcuts match "{query}".</div>
      {:else}
        {#each groups as group (group.title)}
          <section>
            <h3>{group.title}</h3>
            {#each group.items as item (item.description)}
              <div class="row">
                <span class="desc">{item.description}</span>
                <kbd class:unbound={!item.chord}>{formatChord(item.chord)}</kbd>
              </div>
            {/each}
          </section>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
    display: grid;
    place-items: center;
    z-index: 200;
  }
  .dialog {
    width: 720px;
    max-width: 92vw;
    max-height: 86vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--dialog-border);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 16px;
    margin-bottom: 14px;
  }
  .ic { display: grid; place-items: center; color: var(--accent); }
  .x { margin-left: auto; padding: 4px; border-radius: var(--radius); color: var(--text-dim); }
  .x:hover { background: var(--active); color: var(--text); }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    height: 32px;
    margin-bottom: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text-dim);
    flex: 0 0 auto;
  }
  .search input {
    flex: 1 1 auto;
    min-width: 0;
    height: 100%;
    border: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    outline: none;
  }
  .groups {
    overflow-y: auto;
    columns: 2;
    column-gap: 28px;
  }
  .empty {
    columns: initial;
    color: var(--text-dim);
    font-size: 12.5px;
    padding: 12px 2px;
  }
  section {
    break-inside: avoid;
    margin-bottom: 16px;
  }
  h3 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-dim);
    margin-bottom: 6px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 28px;
  }
  .desc { color: var(--text); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  kbd {
    margin-left: auto;
    flex: none;
    font-family: ui-monospace, monospace;
    font-size: 11.5px;
    color: var(--text);
    background: var(--surface-alt);
    border: 1px solid var(--border-strong);
    border-bottom-width: 2px;
    border-radius: 5px;
    padding: 2px 7px;
    white-space: nowrap;
  }
  kbd.unbound {
    color: var(--text-dim);
    font-style: italic;
    border-style: dashed;
  }
</style>
