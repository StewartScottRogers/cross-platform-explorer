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
  import { loadLogWindow } from "../preview/loaders";
  import { formatSize } from "../format";
  import { parseLog, filterLines, pushLogPage, ALL_LEVELS, type ParsedLog, type LogLevel } from "../preview/logViewer";
  import type { LogWindow } from "../bindings.gen";

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

  // The currently-shown bounded window (CPE-1637) — `read_log_window` reads at most PREVIEW_MAX_BYTES at
  // a time (the tail by default) instead of `read_file_text`'s all-or-nothing whole-file cap, so a
  // multi-megabyte real log (CBS.log, dism.log, …) is viewable instead of refused outright. `null` only
  // while loading/errored — never conflated with "empty file" (that's `log.lines.length === 0`, a
  // *loaded* zero-byte window) or "read failed" (`loadError`); see the three-state note in the template.
  let win: LogWindow | null = null;

  // Windows visited this file-open, tail-first (`pages[0]` is the tail; deeper indices are progressively
  // older). "Load earlier" grows this list on demand (one bounded backend read per new page), capped at
  // `MAX_CACHED_LOG_PAGES` (CPE-1644 B′) via `pushLogPage` so exhaustively paging backward through a huge
  // file doesn't re-accumulate the whole thing in memory — a slower, opt-in version of the exact problem
  // CPE-1637's windowed reads exist to fix. Re-visiting an already-cached page is just a pointer move — no
  // re-fetch, no re-scan; "Back to latest" is the one exception (see `jumpToLatest`), since a cached tail
  // page can go stale while the file keeps growing.
  let pages: LogWindow[] = [];
  let pageIndex = 0;

  // The file's size at the moment this preview was opened (CPE-1644 Part B) — compared against the
  // freshly-refetched tail's `file_len` on "Back to latest" so the view can say the file has grown, not
  // just silently show more lines with no explanation.
  let openedFileLen = 0;

  // Request-id guard (mirrors NotebookPreview's reqId): a fast path-change mid-load must stop touching
  // state for the superseded file.
  let reqId = 0;

  let loadedPath = "";
  $: if (path && path !== loadedPath) { loadedPath = path; void load(); }

  function showPage(w: LogWindow) {
    win = w;
    log = parseLog(w.text);
    activeLevels = new Set(ALL_LEVELS);
    showUnleveled = true;
  }

  async function load() {
    const mine = ++reqId;
    loading = true;
    loadError = "";
    win = null;
    log = null;
    pages = [];
    pageIndex = 0;

    let w: LogWindow;
    try {
      w = await loadLogWindow(path, null);
    } catch (e) {
      if (mine === reqId) {
        loadError = String(e);
        loading = false;
      }
      return;
    }
    if (mine !== reqId) return;

    pages = [w];
    pageIndex = 0;
    openedFileLen = w.file_len;
    showPage(w);
    loading = false;
  }

  /** Page further back (CPE-1637): reuses an already-fetched older page if we've been here before this
   *  file-open, otherwise issues exactly one more bounded `read_log_window` ending at the current page's
   *  (already line-aligned) start — never a wider or repeated read of the same bytes. Caches the new page
   *  via `pushLogPage` (CPE-1644 B′), which bounds how many pages accumulate no matter how many times
   *  "Load earlier" is clicked. */
  async function loadOlder() {
    if (!win || win.at_start || loading) return;
    if (pageIndex + 1 < pages.length) {
      pageIndex += 1;
      showPage(pages[pageIndex]);
      return;
    }
    const mine = ++reqId;
    loading = true;
    loadError = "";
    let w: LogWindow;
    try {
      w = await loadLogWindow(path, win.window_start);
    } catch (e) {
      if (mine === reqId) {
        loadError = String(e);
        loading = false;
      }
      return;
    }
    if (mine !== reqId) return;
    pages = pushLogPage(pages, w);
    pageIndex = pages.length - 1;
    showPage(w);
    loading = false;
  }

  /** Back to the tail (CPE-1644 Part B): re-fetches rather than just moving the pointer back to the
   *  cached `pages[0]`, so a log that's actively being appended to while the preview is open actually
   *  shows the current tail — not the snapshot from when the file was opened. The old cached pages are
   *  dropped: a re-fetched tail makes them stale reference points (their byte offsets are relative to the
   *  file's size at open-time), so a subsequent "Load earlier" starts fresh from the new tail. */
  async function jumpToLatest() {
    if (loading) return;
    const mine = ++reqId;
    loading = true;
    loadError = "";
    let w: LogWindow;
    try {
      w = await loadLogWindow(path, null);
    } catch (e) {
      if (mine === reqId) {
        loadError = String(e);
        loading = false;
      }
      return;
    }
    if (mine !== reqId) return;
    pages = [w];
    pageIndex = 0;
    showPage(w);
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
  // Counts lines with no filterLevel at all — i.e. truly unrelated to any group (CPE-1638) — rather than
  // lines with no OWN level, so a stack-trace continuation (which now filters with its header) isn't
  // double-counted under "Other" too; toggling "Other" off no longer affects it, only its header's chip does.
  $: unleveledCount = log ? log.lines.filter((l) => l.filterLevel === null).length : 0;
  $: hasUnleveled = unleveledCount > 0;
  // `false` whenever the whole file fit in one window (small file, unchanged pre-CPE-1637 behavior) —
  // only a genuinely partial view (tail of a huge file, or a paged-back earlier page) shows the byte-range
  // note + paging controls below.
  $: windowed = win !== null && !(win.at_start && win.at_end);
  // CPE-1644 Part B: once a re-fetched tail (via "Back to latest") turns out bigger than the file was at
  // open-time, say so explicitly rather than silently showing more lines with no explanation.
  $: fileGrew = win !== null && openedFileLen > 0 && win.file_len > openedFileLen;
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
            aria-pressed={activeLevels.has(level)}
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
            aria-pressed={showUnleveled}
            data-testid="log-filter-chip-unleveled"
            on:click={toggleUnleveled}
          >
            Other ({unleveledCount})
          </button>
        {/if}
        <span class="log-count" data-testid="log-visible-count" aria-live="polite">
          Showing {visibleLines.length} of {log.lines.length} line{log.lines.length === 1 ? "" : "s"}
        </span>
      </div>

      {#if windowed && win}
        <p class="log-note" data-testid="log-window-note">
          {#if win.at_end}
            Showing the last {formatSize(win.window_end - win.window_start)} of this {formatSize(win.file_len)} file
            (bytes {win.window_start.toLocaleString()}–{win.window_end.toLocaleString()} of {win.file_len.toLocaleString()}).
          {:else}
            Showing bytes {win.window_start.toLocaleString()}–{win.window_end.toLocaleString()} of
            {win.file_len.toLocaleString()} ({formatSize(win.file_len)} total) — not the end of the file.
          {/if}
          {#if !win.line_aligned}
            This window has no line break in it, so it starts mid-line.
          {/if}
          {#if fileGrew}
            The file has grown since this preview was opened.
          {/if}
        </p>
        <div class="log-page-controls" data-testid="log-page-controls">
          <button
            type="button"
            class="log-page-btn"
            data-testid="log-load-older"
            disabled={win.at_start || loading}
            on:click={loadOlder}
          >
            ◀ Load earlier
          </button>
          {#if !win.at_end}
            <button
              type="button"
              class="log-page-btn"
              data-testid="log-jump-latest"
              disabled={loading}
              on:click={jumpToLatest}
            >
              Back to latest ▶
            </button>
          {/if}
        </div>
      {/if}

      {#if log.linesCapped}
        <p class="log-note" data-testid="log-lines-capped">
          Showing the first {log.lines.length.toLocaleString()} of {log.totalLines.toLocaleString()} lines
          of this window.
        </p>
      {/if}

      <!-- svelte-ignore a11y-no-noninteractive-tabindex -- deliberate WCAG SCR29 "scrollable region"
           technique (Visual Critic finding on PR 829): before this, a keyboard user could Tab through
           the filter chips and then had no way to reach or scroll the log body at all (mouse wheel /
           scrollbar drag only). tabindex=0 makes this DIV a single, real tab stop that native arrow
           keys/Page Up/Page Down scroll once focused — individual rows are NOT tab stops. -->
      <div class="log-body" data-testid="log-body" tabindex="0" aria-label="Log output">
        {#each visibleLines as line (line.index)}
          <div
            class="log-row"
            data-level={line.level ?? "none"}
            data-group-level={line.filterLevel ?? "none"}
            data-continuation={line.isContinuation}
            data-testid="log-row"
          >
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
  /* Active state: a tinted fill + coloured border carries the level identity (never solid-fill + white
     text) — sidesteps the white-on-solid-colour contrast gap already tracked separately as CPE-1632, and
     needs no new token. The LABEL itself renders in --text rather than the level colour (Visual Critic
     finding on PR 829): the level colour at 16% tint only clears ~4.2-4.5:1 against its own tinted
     background in some theme/level combinations (measured), short of WCAG AA's 4.5:1 floor for normal
     text in both themes. --text against the same tint is >=8.9:1 in every combination (see
     LogPreview.contrast.test.ts), and the border + tinted fill remain the level's colour signal — the
     border alone only needs the 3:1 non-text floor, which it already clears. */
  .log-chip.active {
    background: color-mix(in srgb, var(--accent) 16%, var(--surface));
    color: var(--text); border-color: var(--accent); font-weight: 600;
  }
  .log-chip[data-level="error"].active {
    background: color-mix(in srgb, var(--danger) 16%, var(--surface));
    color: var(--text); border-color: var(--danger);
  }
  .log-chip[data-level="warn"].active {
    background: color-mix(in srgb, var(--log-warn) 16%, var(--surface));
    color: var(--text); border-color: var(--log-warn);
  }
  .log-count { margin-left: auto; color: var(--text-faint); font-size: 11px; white-space: nowrap; }

  /* Windowed-read paging (CPE-1637) — mirrors HexView's `.pg` prev/next buttons: a plain bordered button,
     disabled (not hidden) at the boundary it can't page past, so the control's presence itself tells the
     user paging exists. */
  .log-page-controls { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
  .log-page-btn {
    height: 24px; padding: 0 10px;
    border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--surface); color: var(--text);
    font-size: 11px; cursor: pointer;
  }
  .log-page-btn:hover:not(:disabled) { background: var(--hover); }
  .log-page-btn:disabled { opacity: 0.4; cursor: default; }

  /* Bounded, scrollable region (CPE-1618) — a capped-but-still-large log (up to MAX_LINES rows) scrolls
     within its own container rather than the whole pane, so the filter bar/notes above stay in view. */
  .log-body {
    flex: 1; min-height: 0; overflow-y: auto;
    border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--surface);
    font-family: var(--mono); font-size: 12px;
  }
  /* Keyboard-reachable scroll region (Visual Critic finding on PR 829): `tabindex="0"` on the div above
     makes it a single tab stop (not every row) that arrow keys/Page Down/Page Up natively scroll once
     focused. Focus ring matches the app-wide button:focus-visible treatment (app.css) rather than
     inventing a new one. */
  .log-body:focus-visible {
    outline: 2px solid var(--accent); outline-offset: -2px;
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

  /* Stack-trace continuation lines (CPE-1638): grouped into a header's level for FILTERING purposes only
     — `line.level` stays null for these, so the badge/full-strength border above never fire for them
     (that's what keeps a 10-frame trace from painting as ten separate errors, a risk the Visual Critic
     flagged on the original PR). They still get a faint, indented thread in the group's colour so the
     grouping reads visually, at a fraction of the header's visual weight — "subordinate", not "re-tinted". */
  .log-row[data-continuation="true"] { padding-left: 24px; }
  .log-row[data-continuation="true"][data-group-level="error"] {
    border-left-color: color-mix(in srgb, var(--danger) 35%, transparent);
  }
  .log-row[data-continuation="true"][data-group-level="warn"] {
    border-left-color: color-mix(in srgb, var(--log-warn) 35%, transparent);
  }
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
  .log-row[data-level="info"] .log-badge { color: var(--accent-text); }
  .log-row[data-level="debug"] .log-badge { color: var(--text-dim); }
  .log-row[data-level="trace"] .log-badge { color: var(--text-faint); }
  .log-text { flex: 1; min-width: 0; color: var(--text); }
</style>
