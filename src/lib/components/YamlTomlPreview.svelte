<script lang="ts">
  /**
   * YAML/TOML structured preview (CPE-1617, epic CPE-1568 slice 7): parses `.yaml`/`.yml`/`.toml` into
   * the same plain-JS value shape JSON does (object/array/string/number/boolean/null) and renders it
   * with the EXISTING `JsonTree.svelte`/`jsonTree.ts` — no fork, no generalization needed there: the
   * parsed value is round-tripped through `JSON.stringify` (see `load()` below), which is lossless for
   * every value shape either parser produces, so `JsonTree`'s own render-safety caps (`MAX_CHILDREN`,
   * `AUTO_COLLAPSE_DEPTH`) apply for free. Self-contained like NotebookPreview/LogPreview (CPE-1616/
   * CPE-1618): fetches its own file content from `path` rather than routing through PreviewPane's shared
   * text-loading state, and — matching those two siblings — has no declared preview-pane actions of its
   * own; the parse result (success / genuine error / deliberately-unsupported-construct) is always
   * shown inline, immediately, on load rather than gated behind a click.
   *
   * **TOML is hand-rolled** (`preview/toml.ts`) rather than a third-party dependency — see that module's
   * doc comment for why. **YAML is a deliberately BOUNDED SUBSET** (`preview/yaml.ts`): full YAML
   * (anchors/aliases, tags, block scalars, multi-document, …) is hard enough that a parser attempting
   * full coverage risks silently mis-parsing a construct it doesn't really understand — worse than no
   * structured view. Anything outside the subset degrades EXPLICITLY: `parseYaml` reports
   * `unsupported:true` with a specific reason (see `preview/yaml.ts`'s module doc comment for the exact
   * list), rendered here as a neutral "can't show a structured view: <reason>" banner over the raw text
   * — never a blank pane, and never a guessed/possibly-wrong tree.
   *
   * **Never conflates a parse failure with an empty file** (this crew's own repeated lesson — see
   * `preview/notebook.ts`'s doc comment): an empty file, a load error, a genuine parse error, an
   * unsupported-construct degrade, and a real structured tree are five structurally distinct states
   * below, each with its own `data-testid`. "Empty" also covers a whitespace/comment-only file (PR 833
   * review finding #10) — that parses SUCCESSFULLY (YAML: `null`; TOML: `{}`) but renders no more
   * usefully than a truly empty file, so `load()` folds that case into the same state rather than
   * showing a bare degenerate tree node.
   */
  import { commands } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import { PREVIEW_MAX_BYTES } from "../preview/loaders";
  import { parseYaml } from "../preview/yaml";
  import { parseToml } from "../preview/toml";
  import JsonTree from "./JsonTree.svelte";

  /** The file's path. */
  export let path: string;
  /** Which parser to run — set by the matching `provider.ts` entry (`canPreview` already gated on the
   *  extension, so this is never ambiguous). */
  export let format: "yaml" | "toml";

  /** How much of the raw file text to show as the degrade-to-text fallback when parsing fails/is
   *  unsupported — capped independently of `PREVIEW_MAX_BYTES` so a huge-but-unparseable file can't
   *  stall the DOM either (mirrors NotebookPreview's own `RAW_FALLBACK_CHARS`). */
  const RAW_FALLBACK_CHARS = 100_000;

  let loading = false;
  let loadError = "";
  let isEmpty = false;
  let parseErrorMessage = "";
  let parseUnsupported = false;
  /** Set only once a parse SUCCEEDS — the parsed value, round-tripped through JSON so `JsonTree.svelte`
   *  can render it unchanged. `null` in every other state (loading/error/empty/unsupported). */
  let jsonText: string | null = null;
  let rawFallback = "";
  let rawFallbackTruncated = false;

  // Request-id guard (mirrors NotebookPreview/LogPreview's own reqId): a fast path/format change
  // mid-load must stop touching state for the superseded file.
  let reqId = 0;

  let loadedKey = "";
  $: if (path && `${format}:${path}` !== loadedKey) {
    loadedKey = `${format}:${path}`;
    void load();
  }

  async function load() {
    const mine = ++reqId;
    loading = true;
    loadError = "";
    isEmpty = false;
    parseErrorMessage = "";
    parseUnsupported = false;
    jsonText = null;
    rawFallback = "";
    rawFallbackTruncated = false;

    let text: string;
    try {
      // Reuses the existing preview read cap (`PREVIEW_MAX_BYTES`) rather than inventing a new one —
      // the backend errors BEFORE reading when the file exceeds it (see `read_file_text_impl`'s
      // `fs::metadata` check), so this never reads-then-discards a huge file.
      text = unwrap(await commands.readFileText(path, PREVIEW_MAX_BYTES));
    } catch (e) {
      if (mine === reqId) {
        loadError = String(e);
        loading = false;
      }
      return;
    }
    if (mine !== reqId) return;

    if (text.length === 0) {
      isEmpty = true;
      loading = false;
      return;
    }

    // Branched (rather than a single ternary'd `result`) so TypeScript ties each parser's own result
    // shape to its call — `TomlParseResult` has no `unsupported` field at all (see toml.ts: every
    // out-of-scope TOML construct is a plain error, never a deliberate degrade), only `YamlParseResult`
    // does.
    let ok: boolean;
    let value: unknown;
    if (format === "yaml") {
      const result = parseYaml(text);
      ok = result.ok;
      if (result.ok) value = result.value;
      else {
        parseErrorMessage = result.error;
        parseUnsupported = result.unsupported;
      }
    } else {
      const result = parseToml(text);
      ok = result.ok;
      if (result.ok) value = result.value;
      else parseErrorMessage = result.error;
    }

    if (!ok) {
      rawFallbackTruncated = text.length > RAW_FALLBACK_CHARS;
      rawFallback = rawFallbackTruncated ? text.slice(0, RAW_FALLBACK_CHARS) : text;
      loading = false;
      return;
    }

    // A whitespace/comment-only file (non-zero bytes, so the `text.length === 0` check above didn't
    // catch it) parses SUCCESSFULLY but to nothing meaningful: YAML resolves an all-comment document to
    // `null`, TOML resolves an all-comment file to `{}`. Rendering that as a lone "null" tree node or an
    // empty-braces tree is technically correct but reads as broken to a user who just opened a comment-
    // only file — treat "parsed successfully but contains nothing" as the same explicit empty state as a
    // truly empty file, rather than a bare/degenerate tree (CPE-1617 PR 833 review finding #10).
    const isEffectivelyEmpty =
      format === "yaml"
        ? value === null
        : typeof value === "object" && value !== null && Object.keys(value as object).length === 0;
    if (isEffectivelyEmpty) {
      isEmpty = true;
      loading = false;
      return;
    }

    jsonText = JSON.stringify(value);
    loading = false;
  }
</script>

<div class="yt-preview" data-testid="yamltoml-preview" data-format={format}>
  {#if loading}
    <p class="yt-note">Loading…</p>
  {:else if loadError}
    <p class="yt-error" data-testid="yamltoml-load-error">Can't preview this file: {loadError}</p>
  {:else if isEmpty}
    <p class="yt-note" data-testid="yamltoml-empty">This file is empty.</p>
  {:else if jsonText !== null}
    <div class="yt-tree" data-testid="yamltoml-tree">
      <JsonTree text={jsonText} />
    </div>
  {:else}
    {#if parseUnsupported}
      <div class="yt-banner" data-testid="yamltoml-unsupported">
        <span>Can't show a structured view: {parseErrorMessage}. Showing the raw file content instead.</span>
      </div>
    {:else}
      <div class="yt-banner warn" data-testid="yamltoml-parse-error">
        <span>
          This doesn't look like valid {format === "yaml" ? "YAML" : "TOML"}: {parseErrorMessage}. Showing
          the raw file content instead.
        </span>
      </div>
    {/if}
    <pre class="yt-raw-fallback" data-testid="yamltoml-raw-fallback">{rawFallback}</pre>
    {#if rawFallbackTruncated}
      <p class="yt-note">Showing the first {RAW_FALLBACK_CHARS.toLocaleString()} characters.</p>
    {/if}
  {/if}
</div>

<style>
  /* Theme-only colours throughout (CLAUDE.md) — every value below is an existing app.css token shared
     with NotebookPreview/LogPreview, no new tokens introduced. */
  .yt-preview { padding: 12px; font-size: 12.5px; height: 100%; box-sizing: border-box; overflow: auto; }
  .yt-note { color: var(--text-faint); font-size: 11.5px; margin: 4px 0; }
  .yt-error { color: var(--danger); white-space: pre-wrap; overflow-wrap: anywhere; }
  .yt-tree { height: 100%; }

  .yt-banner {
    display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-radius: var(--radius);
    background: var(--surface-alt); border: 1px solid var(--border); color: var(--text-dim);
    margin-bottom: 12px; font-size: 11.5px;
  }
  .yt-banner.warn {
    color: var(--danger-on-tint); border-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 8%, var(--surface));
  }
  .yt-raw-fallback {
    background: var(--surface-alt); border: 1px solid var(--border); border-radius: var(--radius);
    padding: 8px; overflow-x: auto; white-space: pre-wrap; overflow-wrap: anywhere;
    font-family: var(--mono, ui-monospace, monospace); font-size: 11.5px;
  }
</style>
