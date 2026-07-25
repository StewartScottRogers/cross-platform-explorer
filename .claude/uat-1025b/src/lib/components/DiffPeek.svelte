<script lang="ts">
  /**
   * Compact inline diff peek for Agent Watch "Edit Diff Peek" (CPE-745, epic CPE-727). Given the
   * before/after text of an agent write (from the CPE-744 diff store), renders a small, scrollable,
   * line-level diff — removed lines then added lines with a little surrounding context — so you can
   * see *what changed* without leaving the timeline. Themed from variables (light/dark parity); the
   * full side-by-side view is CPE-746.
   */
  import { compactLineDiff, type DiffRow } from "../agentDiffs";

  export let before = "";
  export let after = "";
  /** Lines of unchanged context to show either side of the change. */
  export let context = 3;

  $: rows = compactLineDiff(before, after, context) as DiffRow[];
  $: hasChange = rows.some((r) => r.kind !== "context");
  const sign = (k: DiffRow["kind"]) => (k === "add" ? "+" : k === "del" ? "-" : " ");
</script>

{#if hasChange}
  <div class="peek" role="group" aria-label="What changed">
    {#each rows as r}
      <div class="line {r.kind}"><span class="gutter">{sign(r.kind)}</span><span class="text">{r.text}</span></div>
    {/each}
  </div>
{:else}
  <div class="peek empty">No text change captured.</div>
{/if}

<style>
  .peek {
    max-height: 220px;
    overflow: auto;
    margin: 2px 0 4px;
    padding: 4px 0;
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 5px;
    background: var(--surface-2, var(--surface, #181818));
    font-family: var(--mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace);
    font-size: 11.5px;
    line-height: 1.45;
  }
  .peek.empty {
    padding: 8px 10px;
    font-family: inherit;
    font-size: 12px;
    opacity: 0.6;
  }
  .line {
    display: flex;
    white-space: pre;
    padding: 0 6px;
  }
  .gutter {
    flex: 0 0 auto;
    width: 1.1em;
    text-align: center;
    opacity: 0.6;
    user-select: none;
  }
  .text {
    flex: 1;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  /* Add/remove tints via theme-mixable accents — readable in light and dark. */
  .line.add {
    background: color-mix(in srgb, #3a9d4a 22%, transparent);
  }
  .line.del {
    background: color-mix(in srgb, #b5433a 20%, transparent);
  }
  .line.context {
    opacity: 0.7;
  }
</style>
