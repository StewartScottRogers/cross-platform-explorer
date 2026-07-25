<script lang="ts">
  // Contextual "Documents" affordance (CPE-763): a small book button any section's header can drop in to
  // open the docs viewer straight to *its own* page — so every section has direct access to its docs, not
  // just the main toolbar/menu. It stays dumb: it dispatches `help` with the section id and the owning
  // pane forwards it up to App's openDocs(). Matches the toolbar's help affordance (the `book` glyph).
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import type { Section } from "../sectionDocs";

  export let section: Section;
  export let size = 15;
  /** Show the "Docs" label (default). Set false for a bare icon in tight icon rows. */
  export let label = true;

  const dispatch = createEventDispatcher<{ help: Section }>();
</script>

<button
  class="help-btn"
  class:labeled={label}
  title="Documents for this section"
  aria-label="Documents for this section"
  on:click|stopPropagation={() => dispatch("help", section)}
>
  <Icon name="book" {size} />
  {#if label}<span class="help-label">Docs</span>{/if}
</button>

<style>
  .help-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    flex: 0 0 auto;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 3px 6px;
    border-radius: 5px;
    font: inherit;
    font-size: 12px;
  }
  /* Labeled variant reads as a real button (bordered chip) so it's discoverable in a header. */
  .help-btn.labeled {
    border-color: var(--border, #3a3a3a);
    background: var(--surface-alt, transparent);
  }
  .help-btn:hover {
    background: rgba(128, 128, 128, 0.18);
    color: var(--text);
  }
  .help-label {
    line-height: 1;
  }
</style>
