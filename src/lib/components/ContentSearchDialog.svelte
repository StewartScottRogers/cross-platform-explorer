<script lang="ts">
  /**
   * Search inside files (CPE-417) — the UI over the `search_file_contents` backend engine (CPE-416).
   * Runs against the currently-open folder, groups hits by file, and lets you jump to a result's
   * containing folder. An overlay so it stays out of the plain folder listing.
   */
  import { createEventDispatcher } from "svelte";
  import { rawInvoke, createChannel } from "../invoke";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import { groupMatches, baseName, highlightSegments, pushRecentSearch, type ContentSearchResult, type ContentMatch } from "../contentSearch";
  import { lsGet, lsSet, lsBool } from "../persist";

  const RECENTS_KEY = "cpe.contentSearchRecents";
  function loadRecents(): string[] {
    try {
      const v = JSON.parse(lsGet(RECENTS_KEY) ?? "[]");
      return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
    } catch { return []; }
  }
  const saveRecents = (list: string[]) => lsSet(RECENTS_KEY, JSON.stringify(list));
  let recents: string[] = loadRecents();

  export let root = "";

  const dispatch = createEventDispatcher<{ close: void; navigate: string }>();

  let query = "";
  // Remember the Match-case choice across opens (CPE-576).
  let caseSensitive = lsBool("cpe.contentSearchCase", false);
  $: lsSet("cpe.contentSearchCase", caseSensitive ? "1" : "0");
  let loading = false;
  let error = "";
  let searched = false;
  let searchGen = 0; // supersede a stale stream when a newer search starts (CPE-662)
  // The query + case setting actually searched, so highlighting matches the results (not a later edit).
  let searchedQuery = "";
  let searchedCase = false;
  let result: ContentSearchResult = { matches: [], files_scanned: 0, truncated: false };

  $: groups = groupMatches(result.matches);

  // Narrow the result files by name/path on big result sets (CPE-577). Client-side; empty = all.
  let resultFilter = "";
  $: shownGroups = resultFilter.trim()
    ? groups.filter((g) => g.path.toLowerCase().includes(resultFilter.trim().toLowerCase()))
    : groups;

  // Collapse a file's matches on big result sets (CPE-574) — the file header stays a jump-to link.
  let collapsedFiles = new Set<string>();
  function toggleFile(path: string) {
    if (collapsedFiles.has(path)) collapsedFiles.delete(path);
    else collapsedFiles.add(path);
    collapsedFiles = collapsedFiles;
  }

  async function run() {
    const q = query.trim();
    if (!q) return;
    loading = true;
    error = "";
    searched = true;
    searchedQuery = q;
    searchedCase = caseSensitive;
    recents = pushRecentSearch(recents, q);
    saveRecents(recents);
    result = { matches: [], files_scanned: 0, truncated: false };
    // Stream matches as the tree is walked (CPE-662) so a slow content search lists results
    // progressively instead of blocking on the whole scan; a generation token drops stale batches.
    const gen = ++searchGen;
    try {
      const channel = createChannel<ContentMatch[]>();
      channel.onmessage = (batch) => {
        if (gen !== searchGen) return; // superseded by a newer search — drop stale matches
        result = { ...result, matches: result.matches.concat(batch) };
        loading = false; // first matches are in — reveal them
      };
      const final = await rawInvoke<ContentSearchResult>("search_file_contents_stream", {
        root,
        query: q,
        caseSensitive,
        onMatch: channel,
      });
      if (gen === searchGen) result = { ...result, files_scanned: final.files_scanned, truncated: final.truncated };
    } catch (e) {
      if (gen === searchGen) {
        error = String(e);
        result = { matches: [], files_scanned: 0, truncated: false };
      }
    } finally {
      if (gen === searchGen) loading = false;
    }
  }

  function goToFile(path: string) {
    // Dispatch the FILE path — the host reveals it (navigates to its folder AND selects it, CPE-423).
    dispatch("navigate", path);
    dispatch("close");
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <header>
      <h2>{$t("search.inFilesTitle")}</h2>
      <span class="root" title={root}>{baseName(root) || root}</span>
      <button class="x" title={$t("common.close")} on:click={() => dispatch("close")}><Icon name="close" size={14} /></button>
    </header>

    <form class="query-row" on:submit|preventDefault={run}>
      <!-- svelte-ignore a11y-autofocus -->
      <input
        class="q"
        placeholder={$t("search.inFilesPlaceholder")}
        bind:value={query}
        autofocus
        spellcheck="false"
        autocomplete="off"
        list="cs-recents"
      />
      <datalist id="cs-recents">
        {#each recents as r}<option value={r}></option>{/each}
      </datalist>
      <label class="case" title={$t("search.matchCase")}><input type="checkbox" bind:checked={caseSensitive} /> Aa</label>
      <button class="btn primary" type="submit" disabled={!query.trim() || loading}>{$t("search.button")}</button>
    </form>

    <div class="results">
      {#if loading}
        <p class="dim">{$t("search.searching")}</p>
      {:else if error}
        <p class="err">{error}</p>
      {:else if searched && result.matches.length === 0}
        <p class="dim">{$t("search.noMatchesInFolder")}</p>
      {:else if result.matches.length > 0}
        {#if groups.length > 1}
          <input class="result-filter" bind:value={resultFilter} placeholder={$t("search.filterFiles")} spellcheck="false" aria-label={$t("search.filterResultsAria")} />
        {/if}
        <p class="summary">
          {$t("search.matchesInFiles", {
            matches: result.matches.length === 1 ? $t("search.matchOne", { count: result.matches.length }) : $t("search.matchMany", { count: result.matches.length }),
            files: groups.length === 1 ? $t("search.fileOne", { count: groups.length }) : $t("search.fileMany", { count: groups.length }),
          })}
          {#if resultFilter.trim()}<span class="dim"> · {$t("search.shown", { count: shownGroups.length })}</span>{/if}
          {#if result.truncated}<span class="dim"> {$t("search.truncated")}</span>{/if}
        </p>
        {#if resultFilter.trim() && shownGroups.length === 0}
          <p class="dim">{$t("search.noFilesMatch", { query: resultFilter.trim() })}</p>
        {/if}
        {#each shownGroups as g (g.path)}
          <div class="group">
            <div class="group-head">
              <button class="chev" on:click|stopPropagation={() => toggleFile(g.path)} aria-label={$t("search.toggleFile")} title={collapsedFiles.has(g.path) ? $t("home.expand") : $t("home.collapse")}>{collapsedFiles.has(g.path) ? "▸" : "▾"}</button>
              <button class="file" on:click={() => goToFile(g.path)} title={g.path}>
                <Icon name="file" size={13} /> {baseName(g.path)}
                <span class="count">{g.matches.length}</span>
              </button>
            </div>
            {#if !collapsedFiles.has(g.path)}
            {#each g.matches as mt (mt.line_number)}
              <button class="hit" on:click={() => goToFile(g.path)}>
                <span class="ln">{mt.line_number}</span><code
                  >{#each highlightSegments(mt.line, searchedQuery, searchedCase) as seg}{#if seg.match}<mark class="hl">{seg.text}</mark>{:else}{seg.text}{/if}{/each}</code>
              </button>
            {/each}
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog {
    width: 640px; max-width: 94vw; max-height: 82vh; display: flex; flex-direction: column;
    background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 14px 16px 16px;
  }
  header { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  h2 { font-size: 16px; }
  .root { color: var(--text-dim); font-size: 12px; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .x { width: 28px; height: 28px; display: grid; place-items: center; }
  .query-row { display: flex; gap: 8px; align-items: center; }
  .q { flex: 1; height: 32px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); }
  .case { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--text-dim); }
  .btn { height: 32px; padding: 0 16px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:disabled { opacity: 0.5; }
  .results { margin-top: 10px; overflow: auto; }
  .result-filter { width: 100%; height: 26px; padding: 0 10px; margin-bottom: 6px; font: inherit; font-size: 12px;
    border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt); }
  .result-filter:focus { outline: none; border-color: var(--accent); }
  .summary { font-size: 12px; color: var(--text-dim); margin-bottom: 6px; }
  .group { margin-bottom: 8px; }
  .group-head { display: flex; align-items: center; }
  .chev { flex: 0 0 auto; width: 18px; text-align: center; font-size: 10px; color: var(--text-faint); cursor: pointer; }
  .file { display: flex; align-items: center; gap: 6px; flex: 1; min-width: 0; text-align: left; font-weight: 600; font-size: 13px; padding: 4px 6px; border-radius: var(--radius); }
  .file:hover, .hit:hover { background: var(--surface-alt); }
  .count { margin-left: auto; color: var(--text-faint); font-weight: 400; }
  .hit { display: flex; gap: 8px; width: 100%; text-align: left; padding: 2px 6px 2px 22px; font-size: 12px; }
  .ln { color: var(--text-faint); min-width: 34px; text-align: right; font-variant-numeric: tabular-nums; }
  .hit code { font-family: ui-monospace, monospace; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Highlighted match within a result line (CPE-557) — theme-driven, not the browser default yellow. */
  .hit code .hl { background: var(--accent); color: #fff; border-radius: 2px; padding: 0 1px; }
  .dim { color: var(--text-faint); }
  .err { color: #c42b1c; }
</style>
