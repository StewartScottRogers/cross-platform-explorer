<script lang="ts">
  // One node in the JSON tree (CPE-1573, epic CPE-1568): a recursive row (`<svelte:self>` for children)
  // rendering an object/array as an expandable branch or a primitive as a leaf. Reads the collapse-all/
  // expand-all broadcast and reports the clicked node's path/value up, both via the `json-tree` context
  // `JsonTree.svelte` (the root) sets up — see that file for the shape.
  import { getContext } from "svelte";
  import type { Writable } from "svelte/store";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import {
    typeOf,
    isExpandable,
    childEntries,
    formatPrimitive,
    defaultExpanded,
    pathToString,
    MAX_CHILDREN,
  } from "../preview/jsonTree";

  /** This node's decoded value (object/array/primitive). */
  export let value: unknown;
  /** The property name (object) or index (array) this node sits under; `null` only for the tree root. */
  export let keyLabel: string | number | null;
  /** Ancestor path segments down to (not including) this node's own `keyLabel` — `pathToString` renders
   *  the full path including it. */
  export let segments: (string | number)[];
  /** Nesting depth, root = 0 — drives indentation and the auto-collapse-past-depth safety cap. */
  export let depth: number;

  interface JsonTreeCtx {
    commandSignal: Writable<{ id: string; token: number } | null>;
    setFocus: (segments: (string | number)[], value: unknown) => void;
  }
  const ctx = getContext<JsonTreeCtx>("json-tree");
  const commandSignal = ctx.commandSignal;

  $: kind = typeOf(value);
  $: expandable = isExpandable(value);
  $: entries = expandable ? childEntries(value) : [];

  let expanded = defaultExpanded(depth);
  // Lazy pagination (CPE-1573 large-file safety): only the first MAX_CHILDREN entries render until
  // "+N more" is clicked, so a huge flat array/object can't dump thousands of rows into the DOM at once.
  let visibleCount = MAX_CHILDREN;
  // The last command `token` this node has already applied — a broadcast command is a one-shot signal,
  // not a persistent state, so re-applying the SAME token every re-render would fight a manual toggle.
  let appliedToken = -1;

  $: if ($commandSignal && $commandSignal.token !== appliedToken) {
    appliedToken = $commandSignal.token;
    if (expandable) {
      if ($commandSignal.id === "collapse-all") {
        expanded = false;
      } else if ($commandSignal.id === "expand-all") {
        expanded = true;
        visibleCount = MAX_CHILDREN;
      }
    }
  }

  function toggle() {
    if (expandable) expanded = !expanded;
    ctx.setFocus(segments, value);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      toggle();
    }
  }

  function showMore() {
    visibleCount += MAX_CHILDREN;
  }

  $: summary = kind === "array" ? `[${entries.length}]` : kind === "object" ? `{${entries.length}}` : "";
</script>

<div class="jt-node" data-testid="json-node" data-json-kind={kind} data-json-path={pathToString(segments)}>
  <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
  <div
    class="jt-row"
    class:expandable
    style={`padding-left:${depth * 14}px`}
    role="button"
    tabindex="0"
    aria-expanded={expandable ? expanded : undefined}
    on:click={toggle}
    on:keydown={onKeydown}
  >
    {#if expandable}
      <Icon name={expanded ? "chev-down" : "chev-right"} size={12} />
    {:else}
      <span class="jt-spacer" aria-hidden="true"></span>
    {/if}
    {#if keyLabel !== null}
      <span class="jt-key">{keyLabel}</span><span class="jt-punct">:</span>
    {/if}
    {#if expandable}
      <span class="jt-summary jt-{kind}">{summary}</span>
    {:else}
      <span class="jt-val jt-{kind}">{formatPrimitive(value)}</span>
    {/if}
  </div>
  {#if expandable && expanded}
    <div class="jt-children">
      {#each entries.slice(0, visibleCount) as entry (entry.key)}
        <svelte:self
          value={entry.value}
          keyLabel={entry.segment}
          segments={[...segments, entry.segment]}
          depth={depth + 1}
        />
      {/each}
      {#if entries.length > visibleCount}
        <button class="jt-more" style={`margin-left:${(depth + 1) * 14}px`} on:click={showMore}>
          {$t("pv.json.moreItems", { count: entries.length - visibleCount })}
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* Theme-only colours throughout (MENUS.md) — every value below is an app.css token, never a hex.
     CPE-1919 replaced the original claim on this line ("no new ones introduced, so the WCAG guard
     doesn't need updating") with a guard: being all-tokens is NOT the same as being legible, since
     a token can be calibrated for a role other than the one it's used in here. Every colour role
     below is now enumerated and measured against every surface this tree is painted on, in all
     four themes, by src/app.css.accent-text-contrast.test.ts. */
  .jt-node { font-family: var(--mono); font-size: 12px; }
  .jt-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 1px 6px 1px 2px;
    border-radius: var(--radius);
    cursor: default;
    white-space: nowrap;
  }
  .jt-row.expandable { cursor: pointer; }
  .jt-row:hover { background: var(--surface-alt); }
  .jt-spacer { display: inline-block; width: 12px; }
  .jt-key { color: var(--text-dim); }
  .jt-punct { color: var(--text-faint); }
  .jt-summary { color: var(--text-faint); }
  .jt-val { overflow-wrap: anywhere; white-space: pre-wrap; }
  /* CPE-1919: --accent-text, NOT --accent. --accent is tuned for solid fills + icon glyphs (a 3:1
     bar); painted here as 12px running text it measured 3.70:1 on --bg / 3.21:1 on the --surface
     ground `.preview-pane` actually paints / 3.43:1 on the `.jt-row:hover` fill in the dark theme,
     all under WCAG 1.4.3's 4.5:1. See src/app.css.accent-text-contrast.test.ts, which re-derives
     every colour role in this file from this stylesheet rather than a hand-kept list. */
  .jt-val.jt-string { color: var(--accent-text); }
  .jt-val.jt-number,
  .jt-val.jt-boolean { color: var(--text); }
  .jt-val.jt-null { color: var(--text-faint); font-style: italic; }
  .jt-children { display: flex; flex-direction: column; }
  .jt-more {
    align-self: flex-start;
    margin-top: 2px;
    padding: 1px 6px;
    border: none;
    background: none;
    color: var(--text-dim);
    font-family: inherit;
    font-size: 11px;
    text-decoration: underline;
    cursor: pointer;
  }
  .jt-more:hover { color: var(--text); }
</style>
