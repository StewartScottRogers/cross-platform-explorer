<script lang="ts">
  // Structured-data browser preview (CPE-849, epic CPE-721): opens a SQLite / Parquet / Excel-ODS file in
  // an interactive grid — table/sheet navigation, paged typed rows, client-side sort + filter of the page,
  // and a read-only SQL console for SQLite. Backed by the `data_browser_*` commands over cpe-server.
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)

  export let entry: { path: string; extension: string };

  interface Column { name: string; type: string }
  interface Page { columns: Column[]; rows: string[][]; total?: number | null }

  const LIMIT = 100;
  let sources: string[] = [];
  let source = "";
  let offset = 0;
  let page: Page | null = null;
  let loading = false;
  let error = "";
  let sql = "";
  let sqlMode = false;
  let filter = "";
  let sortCol: number | null = null;
  let sortDir: 1 | -1 = 1;

  $: isSqlite = /^(db|sqlite|sqlite3)$/i.test(entry?.extension ?? "");

  // Reload when the previewed file changes.
  let loadedPath = "";
  $: if (entry && entry.path !== loadedPath) { loadedPath = entry.path; void init(); }

  async function init() {
    error = ""; sqlMode = false; sql = ""; offset = 0; sortCol = null; filter = ""; page = null;
    try {
      sources = unwrap(await commands.dataBrowserSources(entry.path));
      source = sources[0] ?? "";
      await loadPage();
    } catch (e) {
      error = String(e); page = null;
    }
  }

  async function loadPage() {
    loading = true; error = "";
    try {
      page = sqlMode
        ? unwrap(await commands.dataBrowserQuery(entry.path, sql, offset, LIMIT))
        : unwrap(await commands.dataBrowserPage(entry.path, source, offset, LIMIT));
    } catch (e) {
      error = String(e); page = null;
    } finally {
      loading = false;
    }
  }

  function pickSource() { offset = 0; sqlMode = false; sortCol = null; void loadPage(); }
  function nextPage() { offset += LIMIT; void loadPage(); }
  function prevPage() { offset = Math.max(0, offset - LIMIT); void loadPage(); }
  function runSql() { if (!sql.trim()) return; sqlMode = true; offset = 0; sortCol = null; void loadPage(); }
  function clearSql() { sqlMode = false; offset = 0; void loadPage(); }

  function sortBy(i: number) {
    if (sortCol === i) sortDir = (sortDir === 1 ? -1 : 1);
    else { sortCol = i; sortDir = 1; }
  }

  // The rows shown: a client-side filter + sort of the current page (server paging stays authoritative).
  $: displayed = (() => {
    if (!page) return [] as string[][];
    let rows = page.rows;
    const q = filter.trim().toLowerCase();
    if (q) rows = rows.filter((r) => r.some((c) => c.toLowerCase().includes(q)));
    if (sortCol !== null) {
      const i = sortCol;
      const dir = sortDir;
      rows = [...rows].sort((a, b) => {
        const av = a[i] ?? "";
        const bv = b[i] ?? "";
        const an = Number(av);
        const bn = Number(bv);
        const cmp = (av !== "" && bv !== "" && !isNaN(an) && !isNaN(bn)) ? an - bn : av.localeCompare(bv);
        return cmp * dir;
      });
    }
    return rows;
  })();
</script>

<div class="data-browser">
  <div class="db-toolbar">
    {#if sources.length > 0}
      <select bind:value={source} on:change={pickSource} class="db-ctl" title={isSqlite ? "Table / view" : "Sheet"}>
        {#each sources as s}<option value={s}>{s}</option>{/each}
      </select>
    {/if}
    <input class="db-ctl db-filter" placeholder="Filter this page…" bind:value={filter} spellcheck="false" aria-label="Filter rows" />
    <span class="db-page">
      {#if page && page.rows.length > 0}rows {offset + 1}–{offset + page.rows.length}{#if page.total !== undefined} of {page.total}{/if}{/if}
    </span>
    <button class="db-btn" on:click={prevPage} disabled={offset === 0 || loading}>‹ Prev</button>
    <button class="db-btn" on:click={nextPage} disabled={loading || (page ? page.rows.length < LIMIT : true)}>Next ›</button>
  </div>

  {#if isSqlite}
    <div class="db-toolbar">
      <input class="db-ctl db-sql" placeholder="Read-only SQL — SELECT … (Enter to run)" bind:value={sql}
             on:keydown={(e) => { if (e.key === "Enter") runSql(); }} spellcheck="false" aria-label="SQL query" />
      <button class="db-btn" on:click={runSql} disabled={!sql.trim() || loading}>Run</button>
      {#if sqlMode}<button class="db-btn" on:click={clearSql}>Clear</button>{/if}
    </div>
  {/if}

  <div class="db-grid-wrap">
    {#if error}
      <div class="db-error">{error}</div>
    {:else if page}
      <table class="db-grid">
        <thead><tr>
          {#each page.columns as c, i}
            <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
            <th on:click={() => sortBy(i)} title={c.type || "column"}>
              {c.name}{#if sortCol === i}<span class="db-sort">{sortDir === 1 ? "▲" : "▼"}</span>{/if}
            </th>
          {/each}
        </tr></thead>
        <tbody>
          {#each displayed as row}
            <tr>{#each row as cell}<td>{cell}</td>{/each}</tr>
          {/each}
        </tbody>
      </table>
      {#if displayed.length === 0}<div class="db-empty">{loading ? "Loading…" : "No rows."}</div>{/if}
    {:else}
      <div class="db-empty">{loading ? "Loading…" : ""}</div>
    {/if}
  </div>
</div>

<style>
  .data-browser { display: flex; flex-direction: column; height: 100%; min-height: 0; font-size: 12px; }
  .db-toolbar { display: flex; gap: 6px; align-items: center; padding: 6px 8px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .db-ctl { font: inherit; height: 24px; padding: 0 6px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface-alt); color: var(--text); }
  .db-filter { flex: 1; min-width: 90px; }
  .db-sql { flex: 1; min-width: 120px; }
  .db-page { color: var(--text-dim); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .db-btn { font: inherit; font-size: 11px; height: 24px; padding: 0 8px; border: 1px solid var(--border-strong); border-radius: 4px; background: transparent; color: inherit; cursor: pointer; }
  .db-btn:hover:not(:disabled) { background: var(--hover); }
  .db-btn:disabled { opacity: 0.4; cursor: default; }
  .db-grid-wrap { flex: 1; overflow: auto; min-height: 0; }
  .db-grid { border-collapse: collapse; width: 100%; }
  .db-grid th, .db-grid td { border: 1px solid var(--border); padding: 2px 8px; text-align: left; white-space: nowrap; }
  .db-grid th { position: sticky; top: 0; background: var(--surface); cursor: pointer; user-select: none; z-index: 1; }
  .db-sort { margin-left: 4px; opacity: 0.7; }
  .db-error { padding: 12px; color: #b5872b; white-space: pre-wrap; }
  .db-empty { padding: 12px; color: var(--text-faint); }
</style>
