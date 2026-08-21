<script lang="ts">
  // ExplorerPane (CPE-676, epic CPE-617): the middle file-listing region — Home screen, the Agent-Watch
  // activity strip, the tag-filter indicator, and the FileList itself. Extracted from App.svelte as the
  // first step toward a reusable pane that can be instantiated twice for dual-pane commander mode. For now
  // it is presentational: App still owns the explorer state and passes it in via props/binds and receives
  // actions via events. Subsequent slices push state ownership down into this component.
  import { createEventDispatcher, tick, onMount } from "svelte";
  import { rawInvoke, createChannel } from "../invoke";
  import { friendlyError } from "../format";
  import Icon from "./Icon.svelte";
  import HomeView from "./HomeView.svelte";
  import FileList from "./FileList.svelte";
  import Toolbar from "./Toolbar.svelte";
  import ContextBar from "./ContextBar.svelte";
  import { t } from "../i18n";
  import * as settings from "../settings";
  import { baseName } from "../contentSearch";
  import { fsActivity, agentTimeline } from "../agentActivity";
  import { click as selClick, selectedIndices, emptySelection, remapByPath, type Selection } from "../selection";
  import { sortEntries, sortByMetaColumn } from "../sort";
  import { makeEntryMatcher } from "../entrySearch";
  import { matchesFileFilter } from "../filetypes";
  import { filterEntriesByTag } from "../tagFilter";
  import { tags } from "../tags";
  import type { FolderAction, FolderContext } from "../folderContext";
  import type { DirEntry, Place, SortKey, SortDir, ViewMode, RecentFile, Favorite, NetShare, DensityMode } from "../types";
  import type { ColorRule } from "../colorRules";
  import type { AgentSession } from "../sidecar";
  import type { ActiveMetaColumn } from "../columns";
  import type { AvailableColumn, MetadataCell, ListDirResult, StreamDirResult } from "../bindings.gen";
  import { commands } from "../bindings.gen"; // typed client (CPE-964) — the non-streamed collect variant
  import { metaColumnCatalog, ensureMetaColumnCatalog } from "../metaColumnCatalog";
  import { canonicalPath } from "../paths";

  /** True when the Home screen should show (App: `isHome && !smartFolder`). */
  export let inHome = false;
  // Row/chrome density (CPE-1526, foundation slice of epic CPE-1488): threaded straight through to
  // <FileList> below. Additive-only for now — FileList ignores it until CPE-1527/1528 add the
  // compact row styling; "comfortable" (the default) renders exactly as today.
  export let density: DensityMode = "comfortable";
  export let places: Place[] = [];
  export let drives: Place[] = [];
  export let pins: string[] = [];
  export let recents: RecentFile[] = [];
  export let favorites: Favorite[] = [];
  export let recentFolders: RecentFile[] = [];
  // Home "Shared" tab (CPE-1163): network/mapped shares + their loading flag; App owns the fetch.
  export let shared: NetShare[] = [];
  export let sharedLoading = false;

  // Agent-Watch strip (CPE-399).
  export let activeWatchCwd = "";
  export let watchedAgentName = "";
  export let recentChanges: { path: string; kind: string }[] = [];
  export let showTimeline = false;
  /** Currently-running agent sessions (CPE-1116/CPE-1120), forwarded straight into `<FileList>` so its
   *  owner heat-map + legend resolve colours via the stable sorted-session index and show agent names
   *  instead of falling back to the per-id hash + shortened sessionId. Optional — an empty list (default)
   *  preserves today's behaviour exactly ("off means off"). */
  export let sessions: AgentSession[] = [];
  /** Read-only reconstructed listing to show INSTEAD OF the live `visible` list while Replay mode is
   *  active (CPE-1112, epic CPE-728 slice e — the file-pane graduate of CPE-1111's in-drawer view).
   *  `null` (default, "off") ⇒ the pane renders the live listing exactly as before; this prop is purely
   *  additive, this component never assigns to it, and it never feeds back into `entries`/`visible`/
   *  `shown` or the fetch pipeline below — the caller (App, via AgentTimeline's pure
   *  `replayOverlay.ts` derivation) owns it and setting it back to `null` restores the live listing on
   *  the very next reactive tick, with no separate cleanup step. */
  export let replayOverlay: DirEntry[] | null = null;

  // The listing + its display state.
  export let showHidden = false;
  export let folderContexts: FolderContext[] = [];
  // The raw directory listing — owned here now (CPE-676 domino 3), bound back to App (whose loadPath still
  // populates it). The pane derives the whole sort/hidden/search/type/tag pipeline down to `visible`.
  export let entries: DirEntry[] = [];
  // Overrides App supplies for the non-plain views: the smart-folder matches, or the archive children.
  // Archive mode disables the hidden/search/type/tag filters (raw list, only sorted).
  export let smartOverride: DirEntry[] | null = null;
  export let archiveOverride: DirEntry[] | null = null;
  /** Absolute path of the open archive when `archiveOverride` is active (CPE-673/674), else `null`.
      Forwarded straight to `<FileList>` so an Alt-drag on an archive row can extract-then-drag-out; see
      `FileList.svelte`'s `archivePath` prop doc for the flow. */
  export let archivePath: string | null = null;
  // The base list the pipeline runs on, resolved from the plain listing + the active-view overrides.
  $: baseEntries = archiveOverride ?? smartOverride ?? entries;
  $: rawList = archiveOverride != null;
  export let search = "";
  export let fileFilter = "all";
  export let foldersFirst = true;
  /** The filtered + sorted listing shown in the FileList. Derived + owned here; bound back to App. */
  export let visible: DirEntry[] = [];
  /** The pre-filter (hidden-only) listing, bound back to App for the status-bar "X of Y" total. */
  export let shown: DirEntry[] = [];
  export let selectedTag = "";
  export let error = "";
  export let loading = false;
  /** How many entries the CURRENT listing left out because their name could not be shown safely
   *  (CPE-1708) — always 0 for a local folder or an unfiltered remote backend (SFTP/WebDAV/FTP). Bound
   *  back to App so the status bar can say so; reset to 0 at the start of every `loadListing` and set
   *  from whichever fetch actually completes (the stream's terminal result, or the cache/revalidate
   *  path), so it always reflects the listing currently on screen — never a stale value left over from
   *  the previous folder. See `loadListing`'s body for why this must NOT be a synthetic row in
   *  `entries` (CPE-1704 round 2's rejected approach, documented on the backend's `ListDirResult`). */
  export let filteredHidden = 0;
  /** How many entries the CURRENT listing left out because they could NOT BE READ (CPE-1780) — a
   *  `metadata()`/`readdir` failure the walk hit mid-listing, always 0 for a remote listing (that
   *  failure mode is local-walk-specific; see `ListDirResult::unreadable`'s doc in
   *  `crates/server/src/model.rs`). Deliberately a SEPARATE count from `filteredHidden` above: that one
   *  means "a name could not be shown safely" (the row was never even seen); this one means "the row was
   *  seen but couldn't be stat'd". Same reset/refresh lifecycle as `filteredHidden` — 0 at the start of
   *  every `loadListing`, set from whichever fetch actually completes, so it never shows a stale value
   *  left over from the previous folder. */
  export let unreadableCount = 0;
  export let cutPaths: string[] = [];
  export let renamingPath = "";
  export let renameValue = "";
  export let canDrag = true;
  /** Rule-based coloring rule set (CPE-776), threaded through to the FileList rows. */
  export let colorRules: ColorRule[] = [];
  /** Recursive folder-size column (CPE-750). */
  export let showFolderSizes = false;
  export let folderSizes: Map<string, number> = new Map();
  export let view: ViewMode = "details";
  export let sortKey: SortKey = "name";
  export let sortDir: SortDir = "asc";
  export let columnWidths: number[] = [];
  /** Active metadata columns for THIS pane's current folder (CPE-1146, epic CPE-707): id + width, in
      display order. Bound from App, which loads/saves it per-folder (`settings.ts`
      `metaColumnsByFolder`) on navigation. Empty (the default) ⇒ no metadata columns — the pane behaves
      exactly as before CPE-1146. */
  export let activeMetaColumns: ActiveMetaColumn[] = [];
  export let selection: Selection;
  export let draggedPaths: string[] = [];
  export let rowEls: HTMLElement[] = [];
  /** The entries under the current selection, derived from `selection` + `visible` and owned here (CPE-676).
   * Bound back out to App so its file/nav operations read the active pane's selection. */
  export let selectedEntries: DirEntry[] = [];

  // ---- derived listing (CPE-676 domino 2) — the sort/hidden/search/type/tag pipeline, owned here.
  // In `rawList` mode (archive browsing) none of the filters apply: the base list is only sorted.
  $: searching = search.trim().length > 0;
  $: shown = rawList ? baseEntries : baseEntries.filter((e) => showHidden || !e.hidden);
  // Power-filters (CPE-1088): size:/date:|modified:/type:/ext:/path: + boolean OR/NOT/-/parens over the
  // bare-name glob matcher. Compiled once per keystroke here, not per entry (see entrySearch.ts).
  $: searchMatcher = makeEntryMatcher(search);
  $: filtered = !rawList && searching ? shown.filter((e) => searchMatcher(e)) : shown;
  $: typeFiltered =
    !rawList && fileFilter !== "all" ? filtered.filter((e) => matchesFileFilter(e, fileFilter)) : filtered;
  $: tagFiltered =
    !rawList && selectedTag ? filterEntriesByTag(typeFiltered, $tags, selectedTag) : typeFiltered;
  // Recursive-size sort key (CPE-750): a not-yet-computed folder resolves to -1 so pending folders cluster.
  $: sizeOf = showFolderSizes
    ? (e: DirEntry) => (e.is_dir ? (folderSizes.get(e.path) ?? -1) : e.size)
    : undefined;
  // A `meta:<id>` sortKey (CPE-1146) sorts by that metadata column's typed value instead of the 4
  // built-in keys — but ONLY if the id is one of THIS pane's own active columns. `sortKey` is a global
  // (App-level) value shared by both dual-pane panes (CPE-677): Pane B never gets a metadata-column set
  // wired (out of scope, see the ticket's Notes), so without this check it would otherwise try to
  // fetch-on-sort a column it has no header/cells for. Any other case (a stale `meta:` key for a column
  // this pane no longer has active) falls through to the normal built-in sort, degrading gracefully.
  $: activeSortColumn =
    sortKey.startsWith("meta:") && activeMetaColumns.some((c) => c.id === sortKey.slice(5))
      ? sortKey.slice(5)
      : null;
  $: visible = activeSortColumn
    ? sortByMetaColumn(tagFiltered, (p) => metaCells.get(activeSortColumn as string)?.get(p)?.cell ?? "Empty", sortDir, foldersFirst)
    : sortEntries(tagFiltered, sortKey as SortKey, sortDir, foldersFirst, sizeOf);

  // CPE-1369 — keep the selection pinned to its FILES when `visible` re-orders/re-filters IN PLACE.
  // `selection` is a set of ROW INDICES into `visible` (see the Replay note below `selectedEntries`), so a
  // sort (column header OR CommandBar), a type/tag filter, or a streamed batch appended mid-selection would
  // otherwise leave the indices pointing at DIFFERENT files — silently moving both the highlight and every
  // op target (Delete / rename / copy / extract / …) to the wrong file, with no "selection lost" cue. So on
  // every `visible` change, remap the selection by PATH, recovering the selected paths from the PREVIOUS
  // `visible`. This depends ONLY on `visible` (the fn arg) — reading `selection`/`prevVisible` inside the
  // helper doesn't make them reactive deps — so it can't self-loop. Navigation is unaffected: App clears
  // `selection` before a real load (keepSelection=false) so there's nothing to remap, and its own
  // keepSelection remap produces the same result. Only reassigns when the index SET actually changes, so an
  // unchanged selection keeps its anchor/lead (no gratuitous churn or scroll jump).
  let prevVisible: DirEntry[] = [];
  $: reconcileSelectionToVisible(visible);
  function reconcileSelectionToVisible(vis: DirEntry[]) {
    const prev = prevVisible;
    prevVisible = vis;
    if (prev === vis || prev.length === 0) return; // first paint / nothing was shown before
    const idx = selectedIndices(selection);
    if (idx.length === 0) return; // nothing selected (also the post-navigation cleared-selection case)
    const paths = idx.map((i) => prev[i]?.path).filter((p): p is string => !!p);
    const remapped = remapByPath(paths, vis);
    const unchanged =
      remapped.indices.size === selection.indices.size &&
      [...selection.indices].every((i) => remapped.indices.has(i));
    if (!unchanged) selection = remapped;
  }

  // ---- Metadata columns (CPE-1146, epic CPE-707) --------------------------------------------------
  // The catalog is a shared, app-wide singleton (metaColumnCatalog.ts) — fetched once regardless of how
  // many `<ExplorerPane>`s exist. Resolve THIS pane's active ids against it for FileList's header/cell
  // rendering; an id the catalog doesn't (yet, or no longer) offer is simply dropped from the resolved
  // list — it still round-trips in `activeMetaColumns`/persisted settings untouched.
  onMount(() => { void ensureMetaColumnCatalog(); });
  $: resolvedMetaColumns = activeMetaColumns
    .map((ac) => {
      const col = $metaColumnCatalog.find((a) => a.id === ac.id);
      return col ? { col, width: ac.width } : null;
    })
    .filter((x): x is { col: AvailableColumn; width: number } => x !== null);

  // Cell cache: columnId -> path -> cell. Merges in place as streamed/collected batches land; a fetch
  // guards against writing after the folder has been navigated away from (the same `loadGen` token
  // `loadListing` below bumps) or after the column was removed mid-fetch, so a superseded result never
  // paints (STREAMING.md generation-token convention, extended to metadata cells).
  let metaCells = new Map<string, Map<string, MetadataCell>>();
  function mergeMetaCells(columnId: string, cells: MetadataCell[]): void {
    const next = new Map(metaCells);
    const forCol = new Map(next.get(columnId) ?? []);
    for (const c of cells) forCol.set(c.path, c);
    next.set(columnId, forCol);
    metaCells = next;
  }

  /** Lazy visible-window fetch (CPE-1145's streamed `metadata_column_cells`), fired by FileList's
   *  `needMetaCells` for whichever rows an active column doesn't have cached yet. Uses `rawInvoke`, not
   *  the busy-tracked `invoke` — a stream shows its own (implicit, in-place) progress and must not also
   *  raise the app-wide busy cursor (BUSY-CURSOR.md). */
  async function fetchMetaCells(columnId: string, paths: string[]): Promise<void> {
    const col = $metaColumnCatalog.find((a) => a.id === columnId);
    if (!col || paths.length === 0) return;
    const gen = loadGen;
    const channel = createChannel<MetadataCell[]>();
    channel.onmessage = (batch) => {
      if (gen !== loadGen || !activeMetaColumns.some((ac) => ac.id === columnId)) return; // superseded
      mergeMetaCells(columnId, batch);
    };
    try {
      await rawInvoke("metadata_column_cells", { paths, column: col.column, onCell: channel });
    } catch {
      // A failed column fetch just leaves those cells blank — never blocks the row (CPE-1146 AC).
    }
  }
  function onNeedMetaCells(reqs: { columnId: string; paths: string[] }[]): void {
    for (const r of reqs) void fetchMetaCells(r.columnId, r.paths);
  }

  /** Fetch-on-sort (CPE-1146): clicking a metadata column header needs EVERY row's cell to sort
   *  correctly, not just the visible window — the streamed collect-to-vec variant fetches the whole
   *  folder in one deliberate, user-initiated call, so it goes through the busy-tracked `commands.*`
   *  client (not `rawInvoke`) like any other explicit one-shot operation. Fires once per distinct
   *  `sortKey`, not on every entries-batch re-sort while a folder is still streaming in. */
  let lastSortFetchKey = "";
  $: if (activeSortColumn && sortKey !== lastSortFetchKey) {
    lastSortFetchKey = sortKey;
    void fetchAllMetaCellsForSort(activeSortColumn);
  }
  $: if (!activeSortColumn) lastSortFetchKey = "";
  async function fetchAllMetaCellsForSort(columnId: string): Promise<void> {
    const col = $metaColumnCatalog.find((a) => a.id === columnId);
    const paths = entries.map((e) => e.path);
    if (!col || paths.length === 0) return;
    const gen = loadGen;
    try {
      const cells = await commands.metadataColumnCellsCollect(paths, col.column);
      if (gen !== loadGen) return; // a newer navigation superseded this folder
      mergeMetaCells(columnId, cells);
    } catch {
      // Sort just falls back to a stable Empty-tiebreak (name) order — never blocks the row.
    }
  }

  // ---- Replay-mode overlay (CPE-1112) — a read-only swap of what FileList renders, never of `visible`
  // itself. `visible` (and the `entries` it's derived from) keep deriving from the real listing pipeline
  // above, untouched, the whole time Replay mode is on — so the live store is provably never mutated by
  // this feature and reappears exactly as it was the instant `replayOverlay` goes back to `null`.
  // `paneEntries` is the ONLY thing that changes: the array actually handed to FileList.
  $: paneEntries = replayOverlay ?? visible;
  $: inReplay = replayOverlay !== null;

  // SECURITY/DATA-INTEGRITY (CPE-1112 rework, found by independent review + UAT): `selection` is a set
  // of ROW INDICES, meaningful only against the array FileList is currently showing. While `inReplay`,
  // that array is the overlay (`paneEntries`), whose rows are a DIFFERENT set/order than `visible` for
  // the same `currentPath` — so `selectedIndices(selection).map(i => visible[i])` can silently resolve
  // an overlay row the user clicked to an unrelated LIVE file. Every op that reads `selectedEntries`
  // (Delete, F2 rename, Ctrl-X/Ctrl-C, Ctrl-D duplicate, copy-path, properties, extract, terminal-here,
  // tags, batch-rename, batch-media, the command-palette file commands, ...) would then act on that
  // wrong file — a real "read-only reconstruction" data-loss hole. So: `selectedEntries` is forced empty
  // for the ENTIRE time `inReplay` is true, unconditionally — it never indexes into `visible` in that
  // state, regardless of what `selection` itself holds.
  $: selectedEntries = inReplay
    ? []
    : selectedIndices(selection).map((i) => visible[i]).filter(Boolean);

  // Belt-and-braces: actively clear `selection` itself the INSTANT Replay mode turns on (not merely its
  // derived `selectedEntries`) — so no stale highlighted row lingers under the overlay banner, and no
  // later Shift-click can extend a range from an anchor that meant something different in the overlay's
  // row order. One-shot on the off->on edge (a plain `$: if (inReplay) selection = emptySelection()`
  // would fight any click made WHILE overlay rows are shown — those are already neutralized by the
  // `!inReplay` guards below/on FileList's dispatched events, and by `selectedEntries` above regardless).
  let wasInReplay = false;
  $: {
    if (inReplay && !wasInReplay) selection = emptySelection();
    wasInReplay = inReplay;
  }

  // ---- listing fetch + directory cache (CPE-676 domino 3b) — the pane owns fetching its own listing.
  // A generation token supersedes an in-flight stream when the caller navigates away; the LRU cache
  // (CPE-756) lets a navigation paint instantly and revalidates in the background. Reloads after a
  // mutation pass useCache=false so our own changes never show stale. Populates the bound `entries`
  // (+ `loading`/`error`); returns whether this load is still the current one (false = superseded).
  let loadGen = 0;
  /** The stream id most recently handed to `list_dir_stream` — tracked explicitly (CPE-1780 Reviewer
   *  round 2) rather than derived as `loadGen - 1`. `invalidateListing()` below can bump `loadGen`
   *  WITHOUT ever starting a stream (a "phantom" generation) — deriving the id to cancel from mere
   *  adjacency would then target a stream id that was never used, while the REAL backend walk for the
   *  folder just left keeps running to completion, defeating CPE-665 cancellation. `0` means "nothing is
   *  currently owed a cancel" — either no stream has started yet, or the last one already finished or was
   *  already cancelled. */
  let lastStreamId = 0;
  const dirCache = new Map<string, DirEntry[]>(); // insertion order == LRU recency
  const DIR_CACHE_MAX = 48;
  // Keyed by canonicalPath (CPE-1737 round 2), NOT the raw path: a remote directory row's `path`
  // legitimately carries a trailing '/' that navigating to the SAME folder via Up/breadcrumb/typed
  // address/favourite does not reproduce. Without this, the two spellings would occupy two separate LRU
  // slots — a post-mutation `useCache=false` reload invalidating one spelling would leave the other
  // stale, so the pane could keep showing pre-mutation rows depending on which route got you there. The
  // VALUES (the listing itself) are untouched — each row keeps its own real `path`, trailing slash
  // included for a directory, so FileList's keyed `{#each}` still sees the distinct paths it needs.
  function cacheGet(path: string): DirEntry[] | undefined {
    const key = canonicalPath(path);
    const v = dirCache.get(key);
    if (v) { dirCache.delete(key); dirCache.set(key, v); }
    return v;
  }
  function cachePut(path: string, list: DirEntry[]): void {
    const key = canonicalPath(path);
    dirCache.delete(key);
    dirCache.set(key, list);
    while (dirCache.size > DIR_CACHE_MAX) dirCache.delete(dirCache.keys().next().value as string);
  }
  const sameListing = (a: DirEntry[], b: DirEntry[]): boolean =>
    a.length === b.length && a.every((e, i) => e.path === b[i].path && e.size === b[i].size && e.modified === b[i].modified);
  async function revalidateDir(path: string, gen: number): Promise<void> {
    try {
      const fresh = await rawInvoke<ListDirResult>("list_dir", { path });
      cachePut(path, fresh.entries);
      if (gen === loadGen) {
        if (!sameListing(entries, fresh.entries)) entries = fresh.entries;
        // CPE-1708: refresh the honest count too — a cache-served view starts at 0 (unknown) until this
        // background revalidation actually asks the provider, see `loadListing`'s cache branch below.
        filteredHidden = fresh.filtered;
        unreadableCount = fresh.unreadable; // CPE-1780: same reasoning, the sibling count
      }
    } catch { /* keep the cached view */ }
  }

  /** Bump the generation token WITHOUT starting a new load (CPE-1780) — called by the caller (App)
   *  whenever it moves the view away from this pane's plain folder listing without routing through
   *  `loadListing`: to Home, into an archive, into a smart folder, or into a saved structured search.
   *  Without this, a `revalidateDir`/stream scheduled for the folder just left can still fire afterward
   *  and pass its `gen === loadGen` check, silently reassigning `entries` (and `filteredHidden`/
   *  `unreadableCount`) for a view that is no longer showing that folder — the CPE-756 class of bug: the
   *  generation token must cover every way you can LEAVE a listing, not just every way you enter one.
   *
   *  CPE-1780 Reviewer round 2 (two proven regressions, both fixed here rather than deferred to the next
   *  real `loadListing`):
   *   1. Cancel the ACTUAL in-flight backend stream (`lastStreamId`, if any) HERE, at the moment we're
   *      leaving — not derived as `loadGen - 1` at the NEXT load (see `lastStreamId`'s doc for why that
   *      derivation breaks once this function can bump `loadGen` without starting a stream). Without this
   *      the backend keeps walking the folder we just left to completion even though nothing on screen
   *      still wants it, exactly what CPE-665 exists to prevent.
   *   2. Settle `loading`/`error` here too. `loadListing`'s own `finally` only clears `loading` when
   *      `gen === loadGen` — a check THIS bump just invalidates for any load still in flight. None of
   *      App's `exitSmartFolder`/`exitStructuredSearch`/`exitArchive` reload the plain listing either, so
   *      without settling here a load in flight when the user opens a smart folder / archive / structured
   *      search would leave `loading` stuck true forever: `FileList` checks `loading` AHEAD of the
   *      overlay's own entries, so the overlay would render "Loading…" over its rows indefinitely.
   *
   *  Reviewer round 2 non-blocking note: a stale-while-revalidate `setTimeout` still pending when this
   *  runs is silently discarded (its later `revalidateDir` will see `gen !== loadGen` and no-op) — the
   *  folder just left with a cache-served view simply won't revalidate again until the user opens it as a
   *  plain listing once more. Acceptable for now (correctness over freshness; nothing renders stale data),
   *  but worth knowing if a future change makes re-scheduling cheap. */
  export function invalidateListing(): void {
    if (lastStreamId > 0) {
      rawInvoke("cancel_dir_stream", { streamId: lastStreamId }).catch(() => {});
      lastStreamId = 0;
    }
    loadGen++;
    loading = false;
    error = "";
  }

  /** Fetch + stream `path` into `entries`. Owns supersede + cache. Returns false if superseded (the caller
   *  must then skip its post-load work). App keeps the navigation orchestration + HOME handling. */
  export async function loadListing(path: string, useCache = false): Promise<boolean> {
    const gen = ++loadGen;
    // Stop the backend walking the folder we just left (CPE-665) — the ACTUAL last-started stream id
    // (`lastStreamId`, see its doc), not `gen - 1`; no-op if nothing is currently owed a cancel.
    if (lastStreamId > 0) {
      rawInvoke("cancel_dir_stream", { streamId: lastStreamId }).catch(() => {});
      lastStreamId = 0;
    }

    // Perf instrumentation (CPE-691/CPE-1304): time-to-first-paint (first batch actually in the DOM)
    // and time-to-settled (stream done AND the reactive `visible = sortEntries(...)` pipeline above has
    // re-derived), so the CPE-688 10× target is a measured before/after rather than a vibe. Dev-gated —
    // `import.meta.env.DEV` is a Vite compile-time constant, so this whole block dead-code-eliminates out
    // of the production bundle. Console marks only; CPE-757 removed the on-screen readout, don't bring it
    // back (see FileList.perf-budget.test.ts / CPE-1304 for the falsifiable multi-size budget instead).
    const perfOn = import.meta.env.DEV;
    if (perfOn) performance.mark(`listing:start:${gen}`);
    const perfStart = perfOn ? performance.now() : 0;
    let perfPainted = false;
    const markPainted = () => {
      if (!perfOn || perfPainted) return;
      perfPainted = true;
      performance.mark(`listing:first-paint:${gen}`);
      performance.measure(`listing first-paint "${path}"`, `listing:start:${gen}`, `listing:first-paint:${gen}`);
      console.debug(`[perf] first paint "${path}" ${Math.round(performance.now() - perfStart)}ms`);
    };

    const servedFromCache = useCache ? cacheGet(path) : undefined;
    if (servedFromCache) {
      entries = servedFromCache;
      // CPE-1708: the cache only stores `DirEntry[]`, not the paired filtered count, so a cache-served
      // paint can't know it yet — 0 (never a stale remembered value) until the stale-while-revalidate
      // pass below asks the provider again and `revalidateDir` sets the real number moments later.
      // CPE-1780: `unreadableCount` follows the exact same reasoning (its own separate count).
      filteredHidden = 0;
      unreadableCount = 0;
      loading = false;
      markPainted();
      await tick(); // let the reactive `visible` derive before the caller's post-load hooks read it
    } else {
      entries = [];
      filteredHidden = 0;
      unreadableCount = 0;
      loading = true;
      // This IS the stream id about to be started below (`streamId: gen` on `list_dir_stream`) — record
      // it so a later cancel (from a real navigation OR `invalidateListing()`) targets the walk that is
      // ACTUALLY running, not a value merely adjacent to `loadGen` (CPE-1780 Reviewer round 2).
      lastStreamId = gen;
      try {
        // Coalesce stream batches (CPE-689): buffer and flush once per animation frame so `visible`
        // re-sorts a handful of times, not once per 256-row batch; the first frame still paints live.
        const channel = createChannel<DirEntry[]>();
        let buffer: DirEntry[] = [];
        let flushQueued = false;
        const flush = () => {
          flushQueued = false;
          if (gen !== loadGen || buffer.length === 0) { buffer = []; return; }
          entries = entries.concat(buffer);
          buffer = [];
          loading = false; // first real rows are in the DOM — drop the "Loading…" placeholder
          markPainted();
        };
        channel.onmessage = (batch) => {
          if (gen !== loadGen) return; // superseded — drop stale rows
          buffer.push(...batch);
          if (!flushQueued) { flushQueued = true; requestAnimationFrame(flush); }
        };
        const streamResult = await rawInvoke<StreamDirResult>("list_dir_stream", { path, streamId: gen, onEntry: channel });
        if (gen === loadGen && buffer.length > 0) flush();
        // CPE-1708: this is the pane's actual first-paint path for a fresh (non-cached) listing, so this
        // is where a filtered remote folder's count reaches the screen on the VERY FIRST view of it, not
        // only after a later cache-hit revalidation. `?? 0` tolerates a bare-number legacy shape (there
        // is none in production, but a defensive default costs nothing and keeps this resilient to a
        // test double that doesn't model the full result shape).
        if (gen === loadGen) {
          filteredHidden = streamResult?.filtered ?? 0;
          unreadableCount = streamResult?.unreadable ?? 0; // CPE-1780: sibling count, same first-paint path
        }
      } catch (e) {
        if (gen === loadGen) { entries = []; error = friendlyError(String(e)); }
      } finally {
        if (gen === loadGen) loading = false;
        // This stream settled on its own (success or error) — nothing left running for `gen`, so it's no
        // longer owed a cancel. Guarded by identity (not `gen === loadGen`): a NEWER load may already have
        // recorded ITS OWN `lastStreamId` by the time this `finally` runs, and this must not clobber that.
        if (lastStreamId === gen) lastStreamId = 0;
      }
      if (gen === loadGen) cachePut(path, entries);
    }

    if (gen !== loadGen) {
      // Superseded (the user navigated again before this one settled) — still clear this gen's marks so
      // rapid navigation doesn't leak `listing:start:<gen>` entries into the perf buffer forever.
      if (perfOn) performance.clearMarks(`listing:start:${gen}`);
      return false;
    }

    // "Settled" = stream done AND `visible` has actually re-sorted, not just the raw fetch finishing —
    // `tick()` flushes the reactive `visible = sortEntries(...)`/`sortByMetaColumn(...)` statements above
    // so this mark lands after the sort a caller would actually see painted (CPE-1304 AC).
    if (perfOn) {
      await tick();
      performance.mark(`listing:settled:${gen}`);
      performance.measure(`listing settled "${path}"`, `listing:start:${gen}`, `listing:settled:${gen}`);
      console.debug(`[perf] settled "${path}" ${Math.round(performance.now() - perfStart)}ms — ${entries.length} entries`);
      performance.clearMarks(`listing:start:${gen}`);
      performance.clearMarks(`listing:first-paint:${gen}`);
      performance.clearMarks(`listing:settled:${gen}`);
      performance.clearMeasures(`listing first-paint "${path}"`);
      performance.clearMeasures(`listing settled "${path}"`);
    }

    // Stale-while-revalidate (CPE-756): a cache-served folder re-lists in the background.
    if (servedFromCache && !error) setTimeout(() => revalidateDir(path, gen), 300);
    return true;
  }

  const dispatch = createEventDispatcher<{
    navigate: string;
    openRecent: string;
    /** Display-only Home selection (CPE-1132): a single-clicked Recent/Favorite file should drive
     *  the right preview/detail pane without becoming a FileList op target — forwarded from
     *  `<HomeView>`'s `select` event since Home has no `<FileList>`/`selectedEntries` of its own. */
    homeSelect: string;
    /** A Home DRIVE tile was right-clicked (CPE-1158). Forwarded straight up even while `inHome` — this
     *  is deliberately distinct from `contextEmpty`/`paneContext`, which stay suppressed on Home, so the
     *  drive tiles get a menu WITHOUT re-introducing an empty-area menu on the blank Home background. */
    driveContext: { x: number; y: number; path: string; name: string };
    /** A Home Recent/Favorites/Folders ROW was right-clicked (CPE-1162). Forwarded straight up even
     *  while `inHome` — deliberately distinct from `contextEmpty`/`paneContext` (which stay suppressed
     *  on Home), exactly like `driveContext`, so the rows get a menu WITHOUT re-introducing an
     *  empty-area menu on the blank Home background. */
    homeItemContext: { x: number; y: number; path: string; is_dir: boolean; view: "recent" | "favorites" | "folders" | "shared"; kind?: string };
    unpin: string;
    unfavorite: string;
    removeRecent: string;
    removeRecentFolder: string;
    clearRecents: void;
    /** Home "Shared" tab was opened (CPE-1163) — App (re)loads the network shares. */
    loadShared: void;
    /** A "＋ Add network location" address was submitted (CPE-1163). */
    addNetworkLocation: string;
    /** Remove a user-added network location by path (CPE-1163). */
    removeNetworkLocation: string;
    open: DirEntry;
    rowContext: { x: number; y: number; index: number };
    contextEmpty: { x: number; y: number };
    commitRename: string;
    drop: { paths: string[]; dest: string; ctrlKey: boolean; shiftKey: boolean };
    contextAction: FolderAction;
    /** A metadata column's width changed (CPE-1146) — the caller persists it per-folder. */
    resizeMetaColumns: { id: string; width: number }[];
    /** The header "Columns…" affordance was clicked (CPE-1146) — the caller opens the picker. */
    openColumnPicker: void;
  }>();

  /** CPE-1154: catch-all right-click over the ENTIRE file pane, so ANY blank pixel — the pane's own
   *  padding, the space around a centred empty-folder box, the gap below a short list, even the sticky
   *  column header — opens the app's empty-area menu, not just the `.rows`/`.empty-state` boxes (which
   *  don't fill the pane and were the source of the CPE-1154 leak). FileList's `rowContext`
   *  `stopPropagation`s so a right-click on a real row never reaches here (its item menu wins), and its
   *  `emptyContext` also `stopPropagation`s so those handled regions don't double-dispatch — this fires
   *  only for the otherwise-unhandled pane pixels. Never on Home (not a folder) or in replay (read-only,
   *  mirroring the `on:contextEmpty` guard below). `preventDefault` also belt-and-braces the native menu
   *  even though App's window-level suppressor already does. */
  function paneContext(e: MouseEvent) {
    if (inHome || inReplay) return;
    e.preventDefault();
    // CPE-1157: STOP the event here, exactly like FileList's `emptyContext`/`rowContext` do. Without
    // this the menu we're about to open was dismissed ~5ms later: the same `contextmenu` event kept
    // bubbling to `window`, where ContextMenu.svelte's own `<svelte:window on:contextmenu={close}>`
    // (its click-outside/right-click-elsewhere dismisser) fired and closed the just-opened empty-area
    // menu. It only bit the CATCH-ALL pane pixels (blank area below a populated `.rows`, pane padding)
    // — the `.rows`/`.empty-state`/row handlers already `stopPropagation`, which is why on-item and
    // truly-empty folders looked fine. Confirmed with the CPE-1155 CDP harness (present:true → 5ms →
    // present:false). `preventDefault` above still kills the native WebView2 menu even though we no
    // longer reach the window-level suppressor.
    e.stopPropagation();
    dispatch("contextEmpty", { x: e.clientX, y: e.clientY });
  }
</script>

<Toolbar label={$t("tb.fileList")}>
  <div class="settings-row">
    <span>{$t("menu.view")}</span>
    <select bind:value={view} on:change={() => settings.saveView(view)}>
      <option value="details">{$t("view.details")}</option>
      <option value="list">{$t("view.list")}</option>
      <option value="icons">{$t("tb.icons")}</option>
      <option value="gallery">{$t("view.gallery")}</option>
    </select>
  </div>
  <div class="settings-row">
    <span>{$t("tb.sortBy")}</span>
    <select bind:value={sortKey} on:change={() => settings.saveSortKey(sortKey)}>
      <option value="name">{$t("sort.name")}</option>
      <option value="modified">{$t("tb.modified")}</option>
      <option value="type">{$t("sort.type")}</option>
      <option value="size">{$t("sort.size")}</option>
    </select>
  </div>
  <div class="settings-row">
    <span>{$t("tb.direction")}</span>
    <select bind:value={sortDir} on:change={() => settings.saveSortDir(sortDir)}>
      <option value="asc">{$t("cmd.ascending")}</option>
      <option value="desc">{$t("cmd.descending")}</option>
    </select>
  </div>
  <div class="settings-row">
    <span>{$t("cmd.showHidden")}</span>
    <input type="checkbox" bind:checked={showHidden}
      on:change={() => settings.saveShowHidden(showHidden)} />
  </div>
</Toolbar>
<ContextBar contexts={folderContexts} on:action={(e) => dispatch("contextAction", e.detail)} />
<!-- svelte-ignore a11y-no-static-element-interactions -->
<div class="filelist-pane" role="region" aria-label={$t("tb.fileList")} on:contextmenu={paneContext}>
{#if inHome}
  <HomeView
    {places}
    {drives}
    {pins}
    {recents}
    {favorites}
    {recentFolders}
    {shared}
    {sharedLoading}
    on:navigate={(e) => dispatch("navigate", e.detail)}
    on:openFile={(e) => dispatch("openRecent", e.detail)}
    on:select={(e) => dispatch("homeSelect", e.detail)}
    on:driveContext={(e) => dispatch("driveContext", e.detail)}
    on:homeItemContext={(e) => dispatch("homeItemContext", e.detail)}
    on:unpin={(e) => dispatch("unpin", e.detail)}
    on:unfavorite={(e) => dispatch("unfavorite", e.detail)}
    on:removeRecent={(e) => dispatch("removeRecent", e.detail)}
    on:removeRecentFolder={(e) => dispatch("removeRecentFolder", e.detail)}
    on:clearRecents={() => dispatch("clearRecents")}
    on:loadShared={() => dispatch("loadShared")}
    on:addNetworkLocation={(e) => dispatch("addNetworkLocation", e.detail)}
    on:removeNetworkLocation={(e) => dispatch("removeNetworkLocation", e.detail)}
  />
{:else}
  {#if inReplay}
    <!-- Replay-mode overlay banner (CPE-1112): makes it unmistakable the pane is showing a read-only
         reconstruction, not the live folder — the live listing/agent strip never renders alongside it. -->
    <div class="replay-strip" role="status">
      <Icon name="recent" size={13} />
      <span class="replay-strip-label">Replay mode — read-only reconstruction, live listing paused</span>
    </div>
  {:else if activeWatchCwd}
    <div class="agent-strip" role="status">
      <span class="agent-dot" />
      <span class="agent-strip-label">{$t("agent.watch", { name: watchedAgentName })}</span>
      {#each recentChanges as c (c.path)}
        <span class="agent-chip {c.kind}" title={c.path}>{c.kind === "removed" ? "−" : c.kind === "created" ? "+" : "~"} {baseName(c.path)}</span>
      {/each}
      {#if recentChanges.length === 0}
        <span class="agent-strip-idle">{$t("agent.watching")}</span>
      {/if}
      <button class="agent-log-btn" on:click={() => (showTimeline = !showTimeline)} title={$t("agent.showLog")}>
        {$t("agent.log")} {$agentTimeline.length ? `(${$agentTimeline.length})` : ""}
      </button>
    </div>
  {/if}
  {#if selectedTag}
    <div class="tag-filter-bar">
      <Icon name="tag" size={13} />
      <span class="tf-label">{selectedTag}</span>
      <span class="tf-count">{visible.length}</span>
      <button class="tf-clear" title="Clear tag filter" aria-label="Clear tag filter" on:click={() => (selectedTag = "")}>
        <Icon name="close" size={12} />
      </button>
    </div>
  {/if}
  <FileList
    entries={paneEntries}
    activity={!inReplay && activeWatchCwd ? $fsActivity : {}}
    {sessions}
    {selection}
    {sortKey}
    {sortDir}
    {view}
    {density}
    {error}
    {loading}
    {searching}
    {cutPaths}
    {renamingPath}
    canDrag={canDrag && !inReplay}
    archivePath={!inReplay && archiveOverride != null ? archivePath : null}
    {renameValue}
    {columnWidths}
    activeMetaColumns={resolvedMetaColumns}
    {metaCells}
    {colorRules}
    showFolderSizes={showFolderSizes && !inReplay}
    {folderSizes}
    on:needSizes
    on:resizeColumns={(e) => { columnWidths = e.detail; settings.saveColumnWidths(columnWidths); }}
    on:needMetaCells={(e) => onNeedMetaCells(e.detail)}
    on:resizeMetaColumns={(e) => dispatch("resizeMetaColumns", e.detail)}
    on:openColumnPicker={() => dispatch("openColumnPicker")}
    bind:rowEls
    bind:draggedPaths
    on:click={(e) => { if (!inReplay) selection = selClick(selection, e.detail.index, e.detail); }}
    on:open={(e) => { if (!inReplay) dispatch("open", e.detail); }}
    on:sort={(e) => {
      sortKey = e.detail.key; sortDir = e.detail.dir;
      settings.saveSortKey(sortKey); settings.saveSortDir(sortDir);
    }}
    on:context={(e) => { if (!inReplay) dispatch("rowContext", e.detail); }}
    on:contextEmpty={(e) => { if (!inReplay) dispatch("contextEmpty", e.detail); }}
    on:commitRename={(e) => { if (!inReplay) dispatch("commitRename", e.detail); }}
    on:cancelRename={() => (renamingPath = "")}
    on:drop={(e) => { if (!inReplay) dispatch("drop", e.detail); }}
  />
{/if}
</div>

<style>
  /* Replay-mode overlay banner (CPE-1112) — mutually exclusive with the Agent-Watch strip below (never
     rendered simultaneously: `inReplay` short-circuits it in the template above), theme-vars only so it
     reads identically light/dark. */
  .replay-strip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
    background: color-mix(in srgb, var(--warn) 14%, var(--surface));
    border-bottom: 1px solid var(--border);
    overflow: hidden;
    white-space: nowrap;
  }
  .replay-strip-label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Agent Watch activity strip (CPE-399) — a thin live banner above the file list, shown only
     while the explorer is inside a running agent's Project folder. */
  .agent-strip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    font-size: 12px;
    background: color-mix(in srgb, var(--accent) 10%, var(--surface));
    border-bottom: 1px solid var(--border);
    overflow: hidden;
    white-space: nowrap;
  }
  .agent-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #3a9d4a;
    flex: 0 0 auto;
    animation: agent-dot-pulse 1.6s ease-in-out infinite;
  }
  @keyframes agent-dot-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.35; }
  }
  .agent-strip-label { font-weight: 600; flex: 0 0 auto; }
  .agent-strip-idle { opacity: 0.6; }
  .agent-chip {
    flex: 0 0 auto;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 11px;
    color: #fff;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 180px;
  }
  .agent-chip.created { background: #3a9d4a; }
  .agent-chip.modified { background: var(--warn-fill); }
  .agent-chip.renamed { background: #3a72b5; }
  .agent-chip.removed { background: var(--danger-fill); }
  .agent-log-btn {
    flex: 0 0 auto;
    margin-left: auto;
    height: 20px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
  }
  .agent-log-btn:hover { background: var(--surface-alt); }

  /* Active tag-filter indicator above the list (CPE-655). */
  .tag-filter-bar {
    display: flex; align-items: center; gap: 6px;
    padding: 4px 10px; margin: 4px 6px 0; border-radius: 6px;
    background: var(--surface-alt); border: 1px solid var(--border);
    font-size: 12px; color: var(--text);
  }
  .tf-label { font-weight: 600; }
  .tf-count { color: var(--text-faint); font-variant-numeric: tabular-nums; }
  .tf-clear { margin-left: auto; width: 20px; height: 20px; display: grid; place-items: center; border-radius: 4px; color: var(--text-dim); }
  .tf-clear:hover { background: var(--surface); color: var(--text); }
</style>
