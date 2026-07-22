<script lang="ts">
  /**
   * Find files by name (CPE-603) — the UI over the `find_files_by_name` backend engine. Searches the
   * currently-open folder recursively for entries whose NAME matches (substring, or a `*`/`?` glob),
   * lists the hits (folders first), and reveals the chosen one. A sibling of ContentSearchDialog,
   * which searches file *contents* instead.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { Channel } from "@tauri-apps/api/core";
  import { rawInvoke } from "../invoke";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import { baseName, parentDir, pushRecentSearch } from "../contentSearch";
  import { sortNameMatches, type NameSearchResult, type NameMatch } from "../fileNameSearch";
  import { lsGet, lsSet } from "../persist";

  const RECENTS_KEY = "cpe.nameSearchRecents";
  function loadRecents(): string[] {
    try {
      const v = JSON.parse(lsGet(RECENTS_KEY) ?? "[]");
      return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
    } catch { return []; }
  }
  const saveRecents = (list: string[]) => lsSet(RECENTS_KEY, JSON.stringify(list));
  let recents: string[] = loadRecents();

  export let root = "";
  /** When opened from the toolbar Search (CPE-866), pre-fill this query and run it immediately. */
  export let initialQuery = "";

  const dispatch = createEventDispatcher<{ close: void; navigate: string }>();

  let query = "";
  let loading = false;
  let error = "";
  let searched = false;
  let result: NameSearchResult = { matches: [], dirs_scanned: 0, truncated: false };
  // Supersede token (CPE-666): a new search drops batches still arriving from the previous one.
  let searchGen = 0;

  $: hits = sortNameMatches(result.matches);

  async function run() {
    const q = query.trim();
    if (!q) return;
    loading = true;
    error = "";
    searched = true;
    recents = pushRecentSearch(recents, q);
    saveRecents(recents);
    result = { matches: [], dirs_scanned: 0, truncated: false };
    // Stream hits as the tree is walked (CPE-666) so a search over a big tree lists results
    // progressively instead of blocking on the whole walk; the reactive `hits` re-sorts each batch.
    const gen = ++searchGen;
    try {
      const channel = new Channel<NameMatch[]>();
      channel.onmessage = (batch) => {
        if (gen !== searchGen) return; // superseded by a newer search — drop stale hits
        result = { ...result, matches: result.matches.concat(batch) };
        loading = false; // first hits are in — reveal them
      };
      const final = await rawInvoke<NameSearchResult>("find_files_by_name_stream", { root, query: q, onMatch: channel });
      if (gen === searchGen) result = { ...result, dirs_scanned: final.dirs_scanned, truncated: final.truncated };
    } catch (e) {
      if (gen === searchGen) {
        error = String(e);
        result = { matches: [], dirs_scanned: 0, truncated: false };
      }
    } finally {
      if (gen === searchGen) loading = false;
    }
  }

  // Opened from the toolbar Search with a query already typed (CPE-866): pre-fill + search at once.
  onMount(() => {
    if (initialQuery.trim()) {
      query = initialQuery;
      run();
    }
  });

  function goTo(path: string) {
    // Dispatch the entry path — the host reveals it (navigates to its folder AND selects it).
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
      <h2>{$t("search.findByNameTitle")}</h2>
      <span class="root" title={root}>{baseName(root) || root}</span>
      <button class="x" title={$t("common.close")} on:click={() => dispatch("close")}><Icon name="close" size={14} /></button>
    </header>

    <form class="query-row" on:submit|preventDefault={run}>
      <!-- svelte-ignore a11y-autofocus -->
      <input
        class="q"
        placeholder={$t("search.byNamePlaceholder")}
        bind:value={query}
        autofocus
        spellcheck="false"
        autocomplete="off"
        list="ns-recents"
      />
      <datalist id="ns-recents">
        {#each recents as r}<option value={r}></option>{/each}
      </datalist>
      <button class="btn primary" type="submit" disabled={!query.trim() || loading}>{$t("search.button")}</button>
    </form>

    <div class="results">
      {#if loading}
        <p class="dim">{$t("search.searching")}</p>
      {:else if error}
        <p class="err">{error}</p>
      {:else if searched && hits.length === 0}
        <p class="dim">{$t("search.noNameMatches")}</p>
      {:else if hits.length > 0}
        <p class="summary">
          {hits.length === 1 ? $t("search.matchOne", { count: hits.length }) : $t("search.matchMany", { count: hits.length })}
          {#if result.truncated}<span class="dim"> {$t("search.truncated")}</span>{/if}
        </p>
        {#each hits as h (h.path)}
          <button class="hit" on:click={() => goTo(h.path)} title={h.path}>
            <Icon name={h.is_dir ? "folder" : "file"} size={14} />
            <span class="name">{h.name}</span>
            <span class="dir">{parentDir(h.path)}</span>
          </button>
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
  .btn { height: 32px; padding: 0 16px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:disabled { opacity: 0.5; }
  .results { margin-top: 10px; overflow: auto; }
  .summary { font-size: 12px; color: var(--text-dim); margin-bottom: 6px; }
  .hit { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left; padding: 5px 6px; border-radius: var(--radius); font-size: 13px; }
  .hit:hover { background: var(--surface-alt); }
  .name { flex: 0 1 auto; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dir { flex: 1 1 auto; min-width: 0; color: var(--text-faint); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dim { color: var(--text-faint); }
  .err { color: #c42b1c; }
</style>
