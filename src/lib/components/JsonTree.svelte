<script lang="ts">
  // JSON tree preview (CPE-1573, epic CPE-1568): a collapsible object/array/primitive tree over a JSON
  // file's raw text, replacing the plain pretty-printed `<pre>` as the default view (PreviewPane keeps a
  // toggle back to raw text — see its `jsonTreeView` state). Self-contained like JwtPreview/CertPreview,
  // but takes `text` directly rather than loading its own data: the pane already has the raw file text
  // loaded for the `json` provider's normal text pipeline (it's `editable`), so there's no reason for a
  // second fetch here.
  //
  // Two-way bridge with PreviewPane's generic action bar (CPE-1570 pattern), via Svelte context so
  // `JsonTreeNode.svelte`'s recursive instances don't need the wiring prop-drilled through every level:
  //   - UP: `setFocus` reports the clicked node's copy-path/copy-value up through `onValues`, exactly
  //     like `JwtPreview`'s `onValues` reports its claims/header — keyed to match the `json` provider's
  //     declared action ids in `preview/provider.ts`.
  //   - DOWN: `commandSignal` carries PreviewPane's collapse-all/expand-all command (from `ctx.dispatch`)
  //     down to every node — the counterpart `values` doesn't have, since those actions must reach INTO
  //     the tree rather than just read data out of it.
  import { setContext } from "svelte";
  import { writable } from "svelte/store";
  import JsonTreeNode from "./JsonTreeNode.svelte";
  import { t } from "../i18n";
  import { parseJson, pathToString, copyableValue } from "../preview/jsonTree";

  /** The JSON file's raw text. */
  export let text = "";
  /** A collapse-all/expand-all command from the action bar. `token` must change for a repeated command
   *  (Collapse-all twice in a row) to re-trigger — a plain `id` string wouldn't be seen as a new value by
   *  Svelte's reactivity. */
  export let command: { id: string; token: number } | null = null;
  /** Reports the focused node's copyable path/value up to PreviewPane, keyed to the `json` provider's
   *  action ids (`copy-path`/`copy-value`) — same contract as JwtPreview's `onValues`. */
  export let onValues: (values: Record<string, string>) => void = () => {};

  $: parsed = parseJson(text);

  const commandSignal = writable<{ id: string; token: number } | null>(null);
  $: commandSignal.set(command);

  function setFocus(segments: (string | number)[], value: unknown) {
    onValues({
      "copy-path": pathToString(segments),
      "copy-value": copyableValue(value),
    });
  }

  setContext("json-tree", { commandSignal, setFocus });
</script>

{#if !parsed.ok}
  <p class="jt-error" data-testid="json-tree-invalid">{$t("pv.json.invalid", { error: parsed.error })}</p>
{:else}
  <div class="jt-root" data-testid="json-tree-root">
    <JsonTreeNode value={parsed.value} keyLabel={null} segments={[]} depth={0} />
  </div>
{/if}

<style>
  .jt-error { padding: 12px; color: var(--danger); white-space: pre-wrap; overflow-wrap: anywhere; }
  .jt-root { padding: 8px 6px; overflow: auto; height: 100%; box-sizing: border-box; }
</style>
