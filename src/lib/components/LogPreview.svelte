<script lang="ts">
  /**
   * Log-file preview (CPE-1618, epic CPE-1568 slice 8): per-line severity-level highlight + filter for
   * `.log` files. Self-contained like NotebookPreview/CertPreview/FontPreview — fetches its own file
   * content from `path` rather than routing through PreviewPane's shared text-loading state.
   *
   * Rendering follows the per-line-row gutter precedent the code preview established for hljs blobs
   * (Library entry `hljs-blob-to-per-line-rows.md`): one row per source line, a small level badge in a
   * gutter column, the line's own text next to it — except there's no hljs blob to split here, `logViewer
   * .ts`'s `parseLog` already returns one row per line directly.
   *
   * A log file is untrusted, attacker-influenced input (a service that logs request bodies, a
   * scraped/downloaded log, …): `parseLog` never throws, every line is rendered via plain `{text}`
   * interpolation (Svelte auto-escapes — never `{@html}`), and raw ANSI colour-code garbage a real
   * colourised logger emits is stripped by `parseLog` (reusing notebook.ts's `stripAnsi`) before this
   * component ever sees it. Every cap (line count, per-line length, per-line detection scan window) bounds
   * work examined, not just what's rendered — see logViewer.ts's module doc comment.
   */
  import { commands } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import { PREVIEW_MAX_BYTES } from "../preview/loaders";
  import { parseLog, filterLines, ALL_LEVELS, type ParsedLog, type LogLevel } from "../preview/logViewer";

  /** The log file's path. */
  export let path: string;

  const LEVEL_LABEL: Record<LogLevel, string> = {
    error: "Error",
    warn: "Warn",
    info: "Info",
    debug: "Debug",
    trace: "Trace",
  };

  let loading = false;
  let loadError = "";
  let log: ParsedLog | null = null;
  let activeLevels: Set<LogLevel> = new Set(ALL_LEVELS);
  let showUnleveled = true;

  // Request-id guard (mirrors NotebookPreview's reqId): a fast path-change mid-load must stop touching
  // state for the superseded file.
  let reqId = 0;

  let loadedPath = "";
  $: if (path && path !== loadedPath) { loadedPath = path; void load(); }

  async function load() {
    const mine = ++reqId;
    loading = true;
    loadError = "";
    log = null;

    let text: string;
    try {
      text = unwrap(await commands.readFileText(path, PREVIEW_MAX_BYTES));
    } catch (e) {
      if (mine === reqId) {
        loadError = String(e);
        loading = false;
      }
      return;
    }
    if (mine !== reqId) return;

    log = parseLog(text);
    activeLevels = new Set(ALL_LEVELS);
    showUnleveled = true;
    loading = false;
  }

  function toggleLevel(level: LogLevel) {
    const next = new Set(activeLevels);
    if (next.has(level)) next.delete(level);
    else next.add(level);
    activeLevels = next;
  }

  function toggleUnleveled() {
    showUnleveled = !showUnleveled;
  }

  $: visibleLines = log ? filterLines(log.lines, { levels: activeLevels, showUnleveled }) : [];
  $: unleveledCount = log ? log.lines.length - ALL_LEVELS.reduce((n, lvl) => n + log!.counts[lvl], 0) : 0;
  $: hasUnleveled = unleveledCount > 0;
</script>

<div class="log-preview" data-testid="log-preview">
  {#if loading}
    <p class="log-note">Loading…</p>
  {:else if loadError}
    <p class="log-error" data-testid="log-load-error">Can't preview this file: {loadError}</p>
  {:else if log}
    {#if log.lines.length === 0}
      <p class="log-note">This file is empty.</p>
    {:else}
      <div class="log-filterbar" data-testid="log-filterbar">
        {#each ALL_LEVELS as level (level)}
          <button
            type="button"
            class="log-chip"
            data-level={level}
            class:active={activeLevels.has(level)}
            data-testid="log-filter-chip-{level}"
            on:click={() => toggleLevel(level)}
          >
            {LEVEL_LABEL[level]} ({log.counts[level]})
          </button>
        {/each}
        {#if hasUnleveled}
          <button
            type="button"
            class="log-chip"
            data-level="none"
            class:active={showUnleveled}
            data-testid="log-filter-chip-unleveled"
            on:click={toggleUnleveled}
          >
            Other ({unleveledCount})
          </button>
        {/if}
        <span class="log-count" data-testid="log-visible-count">
          Showing {visibleLines.length} of {log.lines.length} line{log.lines.length === 1 ? "" : "s"}
        </span>
      </div>

      {#if log.linesCapped}
        <p class="log-note" data-testid="log-lines-capped">
          Showing the first {log.lines.length.toLocaleString()} of {log.totalLines.toLocaleString()} lines.
        </p>
      {/if}

      <div class="log-body" data-testid="log-body">
        {#each visibleLines as line (line.index)}
          <div class="log-row" data-level={line.level ?? "none"} data-testid="log-row">
            <span class="log-gutter">{line.index + 1}</span>
            <span class="log-badge" data-testid="log-badge">{line.level ? LEVEL_LABEL[line.level] : ""}</span>
            <span class="log-text">{line.text}{line.truncated ? "…" : ""}</span>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .log-preview { padding: 12px; font-size: 12.5px; display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .log-note { color: var(--text-faint); font-size: 11.5px; margin: 4px 0; }
  .log-error { color: var(--danger); white-space: pre-wrap; overflow-wrap: anywhere; }

  /* Filter chips reflow rather than overflow their container (CLAUDE.md tick-tack convention) — the
     container wraps onto more rows and grows its height; each chip keeps its own text on one line. */
  .log-filterbar {
    display: flex; flex-wrap: wrap; align-items: center; gap: 6px;
    margin-bottom: 8px;
  }
  .log-chip {
    display: flex; flex: 0 0 auto; align-items: center;
    white-space: nowrap;
    padding: 3px 9px;
    border-radius: 999px;
    border: 1px solid var(--border-strong);
    background: var(--surface-alt);
    color: var(--text-dim);
    font-size: 11px;
    cursor: pointer;
  }
  .log-chip:hover { background: var(--hover); }
  /* Active state: a tinted fill + coloured text/border (never solid-fill + white text) — sidesteps the
     white-on-solid-colour contrast gap already tracked separately as CPE-1632, and needs no new token. */
  .log-chip.active {
    background: color-mix(in srgb, var(--accent) 16%, var(--surface));
    color: var(--accent); border-color: var(--accent); font-weight: 600;
  }
  .log-chip[data-level="error"].active {
    background: color-mix(in srgb, var(--danger) 16%, var(--surface));
    color: var(--danger); border-color: var(--danger);
  }
  .log-chip[data-level="warn"].active {
    background: color-mix(in srgb, var(--log-warn) 16%, var(--surface));
    color: var(--log-warn); border-color: var(--log-warn);
  }
  .log-count { margin-left: auto; color: var(--text-faint); font-size: 11px; white-space: nowrap; }

  /* Bounded, scrollable region (CPE-1618) — a capped-but-still-large log (up to MAX_LINES rows) scrolls
     within its own container rather than the whole pane, so the filter bar/notes above stay in view. */
  .log-body {
    flex: 1; min-height: 0; overflow-y: auto;
    border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--surface);
    font-family: var(--mono, ui-monospace, monospace); font-size: 12px;
  }
  .log-row {
    display: flex; align-items: baseline; gap: 8px;
    padding: 1px 8px;
    border-left: 3px solid transparent;
    white-space: pre-wrap; overflow-wrap: anywhere;
  }
  .log-row:nth-child(odd) { background: var(--surface-alt); }
  .log-row[data-level="error"] { border-left-color: var(--danger); }
  .log-row[data-level="warn"] { border-left-color: var(--log-warn); }
  .log-row[data-level="info"] { border-left-color: var(--accent); }
  .log-gutter {
    flex: 0 0 auto; min-width: 3.5em; text-align: right;
    color: var(--text-faint); user-select: none;
  }
  .log-badge {
    flex: 0 0 auto; min-width: 4.2em;
    font-weight: 600; text-transform: uppercase; font-size: 10.5px; letter-spacing: 0.02em;
    color: var(--text-faint);
  }
  .log-row[data-level="error"] .log-badge { color: var(--danger); }
  .log-row[data-level="warn"] .log-badge { color: var(--log-warn); }
  .log-row[data-level="info"] .log-badge { color: var(--accent); }
  .log-row[data-level="debug"] .log-badge { color: var(--text-dim); }
  .log-row[data-level="trace"] .log-badge { color: var(--text-faint); }
  .log-text { flex: 1; min-width: 0; color: var(--text); }
</style>
