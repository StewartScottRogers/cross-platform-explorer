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

  const dispatch = createEventDispatcher<{ help: Section }>();
</script>

<button
  class="help-btn"
  title="Documents for this section"
  aria-label="Documents for this section"
  on:click|stopPropagation={() => dispatch("help", section)}
>
  <Icon name="book" {size} />
</button>

<style>
  .help-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    border: 0;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 2px 5px;
    border-radius: 5px;
  }
  .help-btn:hover {
    background: rgba(128, 128, 128, 0.18);
    color: var(--text);
  }
</style>
