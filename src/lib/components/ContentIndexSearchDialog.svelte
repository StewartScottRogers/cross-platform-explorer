<script lang="ts">
  /**
   * File-content search (CPE-1263, epic CPE-976) — the UI over the local content-index engine wired by
   * CPE-1262 (`content_index_build` / `content_search`). Unlike `ContentSearchDialog` (line-by-line grep
   * of literal text, CPE-417) or `InstantSearch` (file NAMES across a whole drive, CPE-1139), this ranks
   * files by how well their INDEXED TEXT matches the query, via a small dependency-free local embedder
   * (`FakeEmbedder`) — works fully offline, no key. Framed in copy as "search file contents": the
   * embedder is pluggable (a better model can drop in later behind the same seam) so this deliberately
   * does not oversell "AI"/"semantic" (epic CPE-976 Notes).
   *
   * Off-means-off, mirroring `InstantSearch`'s "Build index" affordance: a folder with no persisted
   * content index yet shows a "build the index" prompt instead of a blank/error results list — the
   * `index_exists` flag on `ContentSearchOutcome` is the clean signal (CPE-1262). A cheap `content_search`
   * probe (empty query, k=0 — a no-op search that only checks whether an index is loadable, per
   * `content_index.rs`'s `empty_query_or_zero_k_yields_no_hits_but_index_exists_is_still_true` test) tells
   * this dialog which state to render on open, without requiring the user to type first.
   *
   * Streaming (STREAMING.md): `content_index_build` streams `ContentIndexProgress` batches over its own
   * channel, so — like `InstantSearch`'s `index_build` — it goes through `rawInvoke` + `createChannel`
   * (BUSY-CURSOR.md: self-progress operations opt OUT of the busy cursor, since they render their own).
   * `content_search` is a normal bounded call, made via the typed `commands.contentSearch` client, which
   * is itself wired to `src/lib/invoke.ts`'s busy-cursor `invoke` (see `bindings.gen.ts`'s `TAURI_INVOKE`
   * import) — so it raises the app-wide wait cursor for free on a slow lookup, per house style. Query
   * input is debounced, and a generation token supersedes an in-flight search/probe so a stale response
   * arriving after a newer one can never overwrite fresher results (STREAMING.md).
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { commands } from "../bindings.gen";
  import type { ContentHit, ContentIndexBuildStats, ContentIndexProgress } from "../bindings.gen";
  import { rawInvoke, createChannel, unwrap } from "../invoke";
  import { t } from "../i18n";
  import { baseName, relativeToRoot, scorePercent, highlightSegments } from "../contentSearch";
  import Icon from "./Icon.svelte";

  export let root = "";

  const dispatch = createEventDispatcher<{ close: void; navigate: string; help: void }>();

  const DEBOUNCE_MS = 250;
  const K = 25;

  let query = "";
  let hits: ContentHit[] = [];
  let loading = false;
  let error = "";
  let searched = false;

  // Index residency for THIS root (off-means-off, CPE-1262's `index_exists` signal): null while the
  // opening probe is in flight.
  let indexExists: boolean | null = null;

  let building = false;
  let buildProgress: ContentIndexProgress | null = null;
  let buildStats: ContentIndexBuildStats | null = null;
  let buildError = "";

  // Supersede token (STREAMING.md): a newer search/probe drops a still-in-flight older one's result.
  let gen = 0;
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  onMount(() => void probe());

  /** A cheap no-op search (empty query, k=0) that only asks "does a persisted index exist for `root`" —
   *  content_search's `index_exists` flag answers that without needing a real query first. */
  async function probe() {
    const g = ++gen;
    try {
      const outcome = unwrap(await commands.contentSearch(root, "", 0));
      if (g === gen) indexExists = outcome.index_exists;
    } catch {
      if (g === gen) indexExists = false; // treat a failed probe as "needs build", not a crash
    }
  }

  function scheduleSearch() {
    if (debounceTimer) clearTimeout(debounceTimer);
    if (!indexExists) return; // off-means-off: nothing to search against yet
    const q = query;
    debounceTimer = setTimeout(() => void runSearch(q), DEBOUNCE_MS);
  }

  async function runSearch(q: string) {
    const trimmed = q.trim();
    if (!trimmed) {
      hits = [];
      loading = false;
      error = "";
      searched = false;
      gen += 1; // supersede any still-in-flight search — its result is now stale
      return;
    }
    const g = ++gen;
    loading = true;
    error = "";
    searched = true;
    try {
      const outcome = unwrap(await commands.contentSearch(root, trimmed, K));
      if (g !== gen) return; // superseded by a newer query — drop the stale result
      hits = outcome.hits;
      indexExists = outcome.index_exists;
    } catch (e) {
      if (g === gen) {
        error = String(e);
        hits = [];
      }
    } finally {
      if (g === gen) loading = false;
    }
  }

  async function buildIndex() {
    if (building) return;
    building = true;
    buildError = "";
    buildProgress = null;
    buildStats = null;
    try {
      const channel = createChannel<ContentIndexProgress>();
      channel.onmessage = (p) => { buildProgress = p; };
      const final = await rawInvoke<ContentIndexBuildStats>("content_index_build", { root, onProgress: channel });
      buildStats = final;
      indexExists = true;
      if (query.trim()) scheduleSearch();
    } catch (e) {
      buildError = String(e);
    } finally {
      building = false;
    }
  }

  function choose(hit: ContentHit) {
    // Dispatch the FILE path — the host reveals it (navigates to its folder AND selects it), same
    // "navigate" contract as ContentSearchDialog/FileNameSearchDialog/InstantSearch.
    dispatch("navigate", hit.path);
    dispatch("close");
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <header>
      <h2>{$t("search.byContentTitle")}</h2>
      <span class="root" title={root}>{baseName(root) || root}</span>
      {#if indexExists}
        <button class="rebuild" title={$t("search.rebuildContentIndex")} disabled={building} on:click={buildIndex}>
          <Icon name="refresh" size={13} /> {$t("search.rebuildContentIndex")}
        </button>
      {/if}
      <button class="docs" title={$t("search.docsTitle")} aria-label={$t("search.docsTitle")} on:click={() => dispatch("help")}><Icon name="book" size={15} /></button>
      <button class="x" title={$t("common.close")} aria-label={$t("common.close")} on:click={() => dispatch("close")}><Icon name="close" size={14} /></button>
    </header>

    <!-- svelte-ignore a11y-autofocus -->
    <input
      class="q"
      placeholder={$t("search.byContentPlaceholder")}
      bind:value={query}
      on:input={scheduleSearch}
      autofocus
      spellcheck="false"
      autocomplete="off"
      aria-label={$t("search.byContentPlaceholder")}
      disabled={!indexExists}
    />

    <div class="results">
      {#if indexExists === false}
        <div class="offer">
          <p class="offer-title">{$t("search.byContentNeedsBuildTitle")}</p>
          <p class="dim">{$t("search.byContentNeedsBuildBody")}</p>
          {#if building}
            <p class="dim building">{$t("search.buildingContentIndex", { count: buildProgress?.files_indexed ?? 0 })}</p>
            {#if buildProgress?.current_path}<p class="dim path" title={buildProgress.current_path}>{buildProgress.current_path}</p>{/if}
          {:else if buildError}
            <p class="err">{buildError}</p>
          {/if}
          {#if !building}
            <button class="btn primary" on:click={buildIndex}>{$t("search.buildContentIndex")}</button>
          {/if}
        </div>
      {:else if indexExists === null}
        <p class="dim">{$t("search.checkingContentIndex")}</p>
      {:else if loading}
        <p class="dim">{$t("search.searching")}</p>
      {:else if error}
        <p class="err">{error}</p>
      {:else if !searched}
        <p class="dim">{$t("search.byContentTypeHint")}</p>
      {:else if hits.length === 0}
        <p class="dim">{$t("search.byContentNoMatches")}</p>
      {:else}
        {#if building}
          <p class="dim building">{$t("search.buildingContentIndex", { count: buildProgress?.files_indexed ?? 0 })}</p>
        {/if}
        <p class="summary">
          {hits.length === 1 ? $t("search.matchOne", { count: hits.length }) : $t("search.matchMany", { count: hits.length })}
        </p>
        <div class="list">
          {#each hits as h (h.path)}
            <button class="hit" on:click={() => choose(h)} title={h.path}>
              <div class="hit-head">
                <Icon name="file" size={13} />
                <span class="name">{baseName(h.path)}</span>
                <span class="path">{relativeToRoot(h.path, root)}</span>
                <span class="score-pill" title={$t("search.byContentScoreTitle")}>
                  <span class="score-bar"><span class="score-fill" style="width: {scorePercent(h.score)}%"></span></span>
                  <span class="score-num">{scorePercent(h.score)}%</span>
                </span>
              </div>
              {#if h.snippet}
                <p class="snippet">{#each highlightSegments(h.snippet, query) as seg}{#if seg.match}<mark class="hl">{seg.text}</mark>{:else}{seg.text}{/if}{/each}</p>
              {/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog {
    width: 680px; max-width: 94vw; max-height: 82vh; display: flex; flex-direction: column;
    background: var(--surface); border: 1px solid var(--dialog-border); border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 14px 16px 16px;
  }
  header { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  h2 { font-size: 16px; }
  .root { color: var(--text-dim); font-size: 12px; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .x { width: 28px; height: 28px; display: grid; place-items: center; }
  .docs { width: 28px; height: 28px; display: grid; place-items: center; color: var(--text-dim); }
  .docs:hover { color: var(--text); }
  .rebuild {
    display: inline-flex; align-items: center; gap: 4px; height: 26px; padding: 0 10px; font-size: 12px;
    color: var(--text-dim); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt);
    white-space: nowrap; flex: 0 0 auto;
  }
  .rebuild:hover:not(:disabled) { color: var(--text); }
  .rebuild:disabled { opacity: 0.5; }
  .q {
    width: 100%; height: 34px; padding: 0 10px; font: inherit;
    border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text);
  }
  .q:disabled { opacity: 0.6; }
  .results { margin-top: 10px; overflow: auto; }
  .dim { color: var(--text-faint); font-size: 13px; }
  .err { color: var(--danger); font-size: 13px; }
  .offer { display: flex; flex-direction: column; gap: 6px; align-items: flex-start; padding: 6px 2px; }
  .offer-title { font-weight: 600; font-size: 14px; color: var(--text); }
  .offer .building { font-variant-numeric: tabular-nums; }
  .offer .path { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; }
  .btn { height: 32px; padding: 0 16px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); color: var(--text); margin-top: 4px; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .summary { font-size: 12px; color: var(--text-dim); margin: 6px 0; }
  .list { display: flex; flex-direction: column; gap: 4px; }
  .hit { display: block; width: 100%; text-align: left; padding: 6px 8px; border-radius: var(--radius); }
  .hit:hover { background: var(--surface-alt); }
  /* Tick-tacks (CPE conventions): the head row reflows onto more lines rather than squeezing/wrapping
     text inside any one pill — the name/path stay ellipsised, the score pill never shrinks or wraps. */
  .hit-head { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .name { font-weight: 600; font-size: 13px; color: var(--text); flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .path { flex: 1 1 120px; min-width: 0; color: var(--text-faint); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .score-pill { display: inline-flex; align-items: center; gap: 5px; flex: 0 0 auto; white-space: nowrap; }
  .score-bar { width: 46px; height: 6px; border-radius: 3px; background: var(--border); overflow: hidden; flex: 0 0 auto; }
  .score-fill { display: block; height: 100%; background: var(--accent); border-radius: 3px; }
  .score-num { font-size: 11px; color: var(--text-faint); font-variant-numeric: tabular-nums; flex: 0 0 auto; }
  .snippet {
    margin: 4px 0 0; padding-left: 19px; font-size: 12px; color: var(--text-dim);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .snippet :global(mark.hl) { background: var(--accent); color: #fff; border-radius: 2px; padding: 0 1px; }
</style>
