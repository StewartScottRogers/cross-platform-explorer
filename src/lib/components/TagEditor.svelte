<script lang="ts">
  /** Tag + colour-label editor for one entry (CPE-637, epic CPE-614). Shows the entry's current
      tags as removable chips, an input to add more (Enter adds), and a single-select colour-label
      swatch row. Applying persists via `setEntryTags`; cancelling changes nothing. Idle until opened. */
  import { createEventDispatcher, onMount } from "svelte";
  import { get } from "svelte/store";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import { tags as tagStore, entryFor, setEntryTags, LABEL_COLORS } from "../tags";

  /** Absolute path of the entry being tagged. */
  export let path: string;
  /** Display name, shown in the heading so the user knows what they're tagging. */
  export let name = "";

  const dispatch = createEventDispatcher<{ close: void }>();

  // Seed from the current store entry — a working copy, committed only on Apply.
  const initial = entryFor(get(tagStore), path);
  let tags: string[] = [...initial.tags];
  let label: string = initial.label;
  let draft = "";
  let input: HTMLInputElement;
  let saving = false;

  // The selectable labels: "" (none) first, then each colour key.
  const LABELS: string[] = Object.keys(LABEL_COLORS);

  onMount(() => {
    input?.focus();
  });

  function addTag() {
    const value = draft.trim();
    if (value && !tags.includes(value)) tags = [...tags, value];
    draft = "";
  }

  function removeTag(tag: string) {
    tags = tags.filter((x) => x !== tag);
  }

  function onInputKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      addTag();
    } else if (e.key === "Backspace" && draft === "" && tags.length > 0) {
      // Empty input + Backspace peels the last chip, like most tag inputs.
      e.preventDefault();
      tags = tags.slice(0, -1);
    }
  }

  async function apply() {
    if (saving) return;
    saving = true;
    addTag(); // fold any half-typed tag in before saving
    try {
      await setEntryTags(path, tags, label);
      dispatch("close");
    } finally {
      saving = false;
    }
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label={$t("tags.title")} on:click|stopPropagation>
    <h2>{$t("tags.title")}</h2>
    {#if name}<p class="subject" title={name}>{name}</p>{/if}

    <div class="field">
      <div class="chips">
        {#each tags as tag (tag)}
          <span class="chip">
            <span class="chip-text">{tag}</span>
            <button class="chip-x" title={$t("tags.remove")} aria-label={$t("tags.remove")} on:click={() => removeTag(tag)}>
              <Icon name="close" size={11} />
            </button>
          </span>
        {/each}
        {#if tags.length === 0}
          <span class="empty">{$t("tags.none")}</span>
        {/if}
      </div>
      <input
        bind:this={input}
        bind:value={draft}
        spellcheck="false"
        aria-label={$t("tags.addLabel")}
        placeholder={$t("tags.addPlaceholder")}
        on:keydown={onInputKey}
      />
    </div>

    <div class="field">
      <span class="section-label">{$t("tags.colorLabel")}</span>
      <div class="swatches">
        {#each LABELS as key (key)}
          <button
            class="swatch"
            class:selected={label === key}
            class:none={key === ""}
            style={key === "" ? "" : `--sw: ${LABEL_COLORS[key]}`}
            title={$t(`tags.color.${key === "" ? "none" : key}`)}
            aria-label={$t(`tags.color.${key === "" ? "none" : key}`)}
            aria-pressed={label === key}
            on:click={() => (label = key)}
          >
            {#if key === ""}<Icon name="ban" size={16} />{:else if label === key}<Icon name="check" size={14} />{/if}
          </button>
        {/each}
      </div>
    </div>

    <div class="actions">
      <button class="btn" on:click={() => dispatch("close")}>{$t("tags.cancel")}</button>
      <button class="btn primary" disabled={saving} on:click={apply}>{$t("tags.apply")}</button>
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
    width: 420px;
    max-width: 90vw;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 { font-size: 16px; margin-bottom: 4px; }
  .subject {
    color: var(--text-dim);
    font-size: 12.5px;
    margin-bottom: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .field { margin-bottom: 16px; }
  .section-label {
    display: block;
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 8px;
  }
  /* Reflow row: chips wrap onto more rows; each chip keeps its text on one line (tick-tacks). */
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 8px;
    min-height: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: 0 0 auto;
    max-width: 100%;
    padding: 2px 4px 2px 8px;
    background: var(--surface-alt);
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 12px;
    white-space: nowrap;
  }
  .chip-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 220px;
  }
  .chip-x {
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 999px;
    color: var(--text-dim);
    flex: 0 0 auto;
  }
  .chip-x:hover { background: var(--hover); color: var(--text); }
  .empty { color: var(--text-faint); font-size: 12px; align-self: center; }
  input {
    width: 100%;
    height: 34px;
    padding: 0 10px;
    font: inherit;
    color: var(--text);
    background: #fff;
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    outline: none;
  }
  .swatches { display: flex; flex-wrap: wrap; gap: 8px; }
  .swatch {
    width: 26px;
    height: 26px;
    border-radius: 999px;
    background: var(--sw, var(--surface-alt));
    border: 2px solid transparent;
    display: grid;
    place-items: center;
    color: #fff;
    flex: 0 0 auto;
  }
  .swatch.none {
    background: var(--surface-alt);
    color: var(--text-dim);
    border: 1px solid var(--border);
  }
  .swatch.selected {
    border-color: var(--text);
    box-shadow: 0 0 0 2px var(--surface), 0 0 0 4px var(--accent);
  }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
  .btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
  }
  .btn:disabled { opacity: 0.5; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover:not(:disabled) { background: var(--accent-hover); }
</style>
