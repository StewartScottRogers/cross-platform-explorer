<script lang="ts">
  import { createEventDispatcher, tick, onMount } from "svelte";
  import { windowRange, ensureVisibleOffset } from "../virtualize";
  import Icon from "./Icon.svelte";
  import ThumbnailImage from "./ThumbnailImage.svelte";
  import { t } from "../i18n";
  import { formatSize } from "../format";
  import { formatDate } from "../datetime";
  import { iconFor, typeName, isImage } from "../filetypes";
  import { columnsTemplate, resizeColumnTo, boundaryOffsets, COLUMN_DEFAULTS } from "../columns";
  import { isSelected } from "../selection";
  import { setDragData, isValidDrop, hoverEffect } from "../dnd";
  import type { Selection } from "../selection";
  import type { DirEntry, SortKey, SortDir, ViewMode } from "../types";
  import type { AgentActivity } from "../agentActivity";
  import { folderActivityKindNorm, normalizeActivityByKind } from "../agentActivity";
  import { tags, entryFor, labelColor } from "../tags";
  import { evaluateRules, type ColorRule } from "../colorRules";

  export let entries: DirEntry[] = [];
  /** Agent Watch (CPE-399): per-path live activity, keyed by absolute path. Empty ⇒ no
      annotations, so the list is visually unchanged when not watching an agent. */
  export let activity: Record<string, AgentActivity> = {};
  /** Human labels for the row badge, by activity kind. */
  const ACTIVITY_LABEL_KEY: Record<AgentActivity["kind"], string> = {
    created: "fl.badgeNew",
    modified: "fl.badgeEdited",
    removed: "fl.badgeDeleted",
    renamed: "fl.badgeMoved",
    read: "fl.badgeRead", // CPE-405: consulted, not changed
  };
  // The active paths, split into writes vs reads and recomputed only when the activity map changes —
  // used to light up folder rows whose subtree the agent is changing (CPE-402), with a cooler tint for
  // subtrees it has only *read* (CPE-742). Normalized once here (not per folder row) so the per-row
  // descendant check is a cheap prefix test (CPE-698).
  $: activitySets = normalizeActivityByKind(activity);
  export let selection: Selection;
  export let sortKey: SortKey = "name";
  export let sortDir: SortDir = "asc";
  export let view: ViewMode = "details";
  export let error = "";
  export let loading = false;
  export let searching = false;
  export let cutPaths: string[] = [];
  /** Rule-based coloring/labels (CPE-776): the active, ordered rule set. Empty ⇒ rows are unstyled, so
      the list looks identical when no rules exist. `evaluateRules` takes the first enabled matching rule. */
  export let colorRules: ColorRule[] = [];
  // A single timestamp so olderThan/newerThan rules evaluate consistently across all rows; recomputed
  // whenever the rule set changes (referencing `colorRules` makes this reactive block depend on it).
  let rulesNow = Date.now();
  $: {
    colorRules;
    rulesNow = Date.now();
  }

  /** Path currently being renamed inline, or "" for none. */
  export let renamingPath = "";
  /** Whether drag-and-drop is active for these rows. False in read-only virtual views like an open
      archive (CPE-673), whose rows are synthetic in-zip paths, not real files — so dragging them out or
      dropping onto them would be meaningless. */
  export let canDrag = true;
  /** Initial text for the inline editor. */
  export let renameValue = "";

  export let rowEls: HTMLElement[] = [];

  const dispatch = createEventDispatcher<{
    click: { index: number; ctrl: boolean; shift: boolean };
    open: DirEntry;
    sort: { key: SortKey; dir: SortDir };
    context: { x: number; y: number; index: number };
    contextEmpty: { x: number; y: number };
    commitRename: string;
    cancelRename: void;
    drop: { paths: string[]; dest: string; ctrlKey: boolean; shiftKey: boolean };
    resizeColumns: number[];
  }>();

  /** Details-view column widths (Name/Date/Type/Size), bound from the parent so they
      persist; the trailing spacer is implicit (CPE-350). */
  export let columnWidths: number[] = COLUMN_DEFAULTS.slice();
  $: colTemplate = columnsTemplate(columnWidths);
  // Right-edge offset of each column, for placing the drag handles. 10px = .columns pad-left.
  $: handleOffsets = boundaryOffsets(columnWidths, 10);

  /** Drag a column's right edge to resize it; the layout updates live and persists on
      release. `stopPropagation` keeps the click off the sort-header button. */
  function startColResize(e: PointerEvent, i: number) {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startW = columnWidths[i];
    const move = (ev: PointerEvent) => {
      columnWidths = resizeColumnTo(columnWidths, i, startW + (ev.clientX - startX));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      dispatch("resizeColumns", columnWidths);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  /** Keyboard resize for a focused column divider — ← / → nudge the width (Shift = bigger
      step), so the columns are usable without a mouse (CPE-314 a11y). */
  function onResizeKey(e: KeyboardEvent, i: number) {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const step = (e.shiftKey ? 32 : 8) * (e.key === "ArrowLeft" ? -1 : 1);
    columnWidths = resizeColumnTo(columnWidths, i, columnWidths[i] + step);
    dispatch("resizeColumns", columnWidths);
  }

  /** Paths being dragged, and the folder row currently hovered as a target. */
  export let draggedPaths: string[] = [];

  let dropIndex = -1;

  // Double-click vs drag (CPE-236): in a webview the second press of a double-
  // click, with a hair of movement, can start a native drag and eat the "open".
  // Suppress dragging briefly when a press lands right after another on the same
  // row (i.e. the 2nd click of a double-click), so dblclick reliably fires. A
  // real drag — single press then actual movement — is unaffected.
  let lastPressAt = 0;
  let lastPressIndex = -1;
  let suppressDragUntil = 0;

  function onRowPointerDown(i: number) {
    const now = Date.now();
    if (now - lastPressAt < 450 && lastPressIndex === i) suppressDragUntil = now + 600;
    lastPressAt = now;
    lastPressIndex = i;
  }

  function onDragStart(e: DragEvent, i: number) {
    if (renamingPath || Date.now() < suppressDragUntil) {
      e.preventDefault();
      return;
    }
    // Drag the whole selection if the grabbed row is part of it; otherwise
    // just the grabbed row (Explorer's behaviour).
    const paths = isSelected(selection, i)
      ? entries.filter((_, j) => isSelected(selection, j)).map((x) => x.path)
      : [entries[i].path];
    draggedPaths = paths;
    setDragData(e.dataTransfer, paths);
    setDragBadge(e, paths.length);
  }

  function onDragEnd() {
    draggedPaths = [];
    dropIndex = -1;
  }

  /** A themed drag image showing the item count for a multi-selection drag (CPE-669). Appended to the
      body (so it inherits theme vars) and removed after the browser has snapshotted it. */
  function setDragBadge(e: DragEvent, count: number) {
    if (!e.dataTransfer || count < 2) return;
    const badge = document.createElement("div");
    badge.textContent = $t("dnd.itemCount", { count });
    badge.style.cssText =
      "position:absolute; top:-1000px; left:-1000px; padding:4px 10px; border-radius:6px;" +
      "background:var(--accent); color:#fff; font:600 12px system-ui,sans-serif; white-space:nowrap;";
    document.body.appendChild(badge);
    e.dataTransfer.setDragImage(badge, -8, -8);
    setTimeout(() => badge.remove(), 0);
  }

  /** Only folders are valid targets (plus the shared self/descendant rule); no targets when DnD is off. */
  function validTarget(i: number): boolean {
    if (!canDrag) return false;
    const entry = entries[i];
    return !!entry?.is_dir && isValidDrop(draggedPaths, entry.path);
  }

  function onDragOver(e: DragEvent, i: number) {
    if (!validTarget(i)) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = hoverEffect(e);
    dropIndex = i;
  }

  function onDrop(e: DragEvent, i: number) {
    if (!validTarget(i)) return;
    e.preventDefault();
    const paths = [...draggedPaths];
    const dest = entries[i].path;
    onDragEnd();
    dispatch("drop", { paths, dest, ctrlKey: e.ctrlKey, shiftKey: e.shiftKey });
  }

  const COLUMNS: { key: SortKey; labelKey: string; num?: boolean }[] = [
    { key: "name", labelKey: "sort.name" },
    { key: "modified", labelKey: "sort.modified" },
    { key: "type", labelKey: "sort.type" },
    { key: "size", labelKey: "sort.size", num: true },
  ];

  function headerClick(key: SortKey) {
    const dir: SortDir = key === sortKey && sortDir === "asc" ? "desc" : "asc";
    dispatch("sort", { key, dir });
  }

  let editEl: HTMLInputElement | undefined;
  $: if (renamingPath && editEl) focusEditor();

  async function focusEditor() {
    await tick();
    if (!editEl) return;
    editEl.focus();
    // Select the stem, not the extension — renaming "photo.png" shouldn't make
    // it trivially easy to destroy the extension by typing over it.
    const dot = renameValue.lastIndexOf(".");
    if (dot > 0) editEl.setSelectionRange(0, dot);
    else editEl.select();
  }

  function onEditorKey(e: KeyboardEvent) {
    e.stopPropagation(); // list shortcuts must never fire while editing
    if (e.key === "Enter") {
      e.preventDefault();
      dispatch("commitRename", (e.currentTarget as HTMLInputElement).value);
    } else if (e.key === "Escape") {
      e.preventDefault();
      dispatch("cancelRename");
    }
  }

  function rowClick(e: MouseEvent, i: number) {
    dispatch("click", {
      index: i,
      ctrl: e.ctrlKey || e.metaKey,
      shift: e.shiftKey,
    });
  }

  function rowContext(e: MouseEvent, i: number) {
    e.preventDefault();
    e.stopPropagation();
    dispatch("context", { x: e.clientX, y: e.clientY, index: i });
  }

  function emptyContext(e: MouseEvent) {
    e.preventDefault();
    dispatch("contextEmpty", { x: e.clientX, y: e.clientY });
  }

  // Membership sets recomputed only when the source arrays change, so each of the ~30–50 on-screen
  // rows (post-virtualization, CPE-692) does an O(1) lookup instead of an O(n) Array.includes scan —
  // the drag case in particular was O(rows × selection) on every re-render.
  $: cutSet = new Set(cutPaths);
  $: draggedSet = new Set(draggedPaths);

  // ── Virtualization (CPE-690 details, CPE-766 icons/gallery grids; epic CPE-688) ─────────────────
  // Render only the visible window for large folders across every uniform-row view — details/list
  // (columns = 1) and the icon/gallery grids (columns = N) — so a 10k-file folder paints in fixed cost
  // instead of building a DOM node per entry. Folders below the threshold render in FULL, exactly as
  // before — the common case pays nothing (PURPOSE.md). The `.rows` block keeps its true scroll height
  // via top/bottom spacer divs (full-width via `grid-column` in the grids), so the ancestor
  // `.filelist-pane` scroller and its sticky header behave unchanged. Rows carry their ABSOLUTE index, so
  // every selection / rowEls / DnD / rename path below is untouched. Grid tiles are made uniform-height
  // (fixed 2-line name; tag chips hidden in grid) so the fixed-row-height math holds; column count and
  // tile pitch are measured from the live grid so they survive pane resize and view switches.
  const VIRTUALIZE_THRESHOLD = 100;
  const OVERSCAN_ROWS = 6;
  let rowsEl: HTMLDivElement | undefined;
  let scrollEl: HTMLElement | null = null;
  let effScroll = 0; // px of `.rows` content scrolled above the scroller's top fold
  let viewportH = 0;
  let rowH = 30; // measured row/tile pitch (row height, + row-gap for grids); falls back to --row-h
  let cols = 1; // measured items-per-row (1 for details/list, N for the auto-fill grids)
  let rowGapPx = 0; // measured grid row-gap, to compensate the spacers' own gap inside the grid
  let rafPending = false;

  $: isGrid = view === "icons" || view === "gallery";
  $: virtualize = entries.length >= VIRTUALIZE_THRESHOLD;

  $: win =
    virtualize && rowH > 0 && viewportH > 0
      ? windowRange(effScroll, viewportH, rowH, entries.length, cols, OVERSCAN_ROWS)
      : { start: 0, end: entries.length, padTop: 0, padBottom: 0 };

  $: windowed = virtualize
    ? entries.slice(win.start, win.end).map((entry, k) => ({ entry, i: win.start + k }))
    : entries.map((entry, i) => ({ entry, i }));

  // Spacer heights. In the grids each spacer is itself a full-width grid row, so it introduces one
  // row-gap of its own above/below the rendered slice — subtract it back out so the tiles land exactly
  // at their absolute row position. In the (block) list/details views there is no gap to compensate.
  $: topPad = virtualize ? (isGrid ? Math.max(0, win.padTop - rowGapPx) : win.padTop) : 0;
  $: botPad = virtualize ? (isGrid ? Math.max(0, win.padBottom - rowGapPx) : win.padBottom) : 0;

  let roInstance: ResizeObserver | undefined;
  let scrollerWired = false;

  // The `.filelist-pane` scroller (and thus `.rows`) often isn't in the DOM yet when this component
  // first mounts — the folder is still loading, or we arrived from an empty/Home state — so acquire it
  // LAZILY the first time `.rows` exists and wire the scroll/resize listeners then. A one-shot capture in
  // onMount silently left virtualization disabled after a Home→folder navigation (found GUI-verifying
  // CPE-766; also repairs that path for the CPE-690 details view).
  function wireScroller() {
    if (scrollerWired || !rowsEl) return;
    scrollEl = rowsEl.closest<HTMLElement>(".filelist-pane") ?? null;
    if (!scrollEl) return;
    scrollEl.addEventListener("scroll", onScrollOrResize, { passive: true });
    // ResizeObserver isn't present in every environment (e.g. jsdom) — guard so wiring never throws.
    if (typeof ResizeObserver !== "undefined") {
      roInstance = new ResizeObserver(onScrollOrResize);
      roInstance.observe(scrollEl);
    }
    scrollerWired = true;
  }

  function measureGeometry() {
    wireScroller();
    if (!scrollEl || !rowsEl) return;
    const cRect = scrollEl.getBoundingClientRect();
    viewportH = cRect.height;
    const rRect = rowsEl.getBoundingClientRect();
    effScroll = Math.max(0, cRect.top - rRect.top);
    // First rendered tile/row (never a spacer — those are `.vspacer`).
    const firstRow = rowsEl.querySelector<HTMLElement>(".row");
    if (isGrid) {
      const cs = getComputedStyle(rowsEl);
      // The computed `grid-template-columns` resolves `auto-fill` to concrete tracks — count them.
      cols = Math.max(1, cs.gridTemplateColumns.split(" ").filter((s) => s && s !== "none").length);
      rowGapPx = parseFloat(cs.rowGap) || 0;
      if (firstRow) {
        const h = firstRow.getBoundingClientRect().height;
        if (h > 0) rowH = h + rowGapPx;
      }
    } else {
      cols = 1;
      rowGapPx = 0;
      if (firstRow) {
        const h = firstRow.getBoundingClientRect().height;
        if (h > 0) rowH = h;
      }
    }
  }

  function onScrollOrResize() {
    if (rafPending) return;
    rafPending = true;
    requestAnimationFrame(() => {
      rafPending = false;
      measureGeometry();
    });
  }

  onMount(() => {
    // May be too early (`.rows` not rendered yet) — measureGeometry()/wireScroller() are idempotent and
    // the reactive re-measure below picks it up once `.rows` exists.
    measureGeometry();
    return () => {
      scrollEl?.removeEventListener("scroll", onScrollOrResize);
      roInstance?.disconnect();
    };
  });

  // Re-measure after the folder/view changes (rows re-laid-out) so the window is correct on the next paint.
  $: if (rowsEl) {
    void entries.length;
    void view;
    tick().then(measureGeometry);
  }

  // When virtualizing, an OFF-window lead row isn't in the DOM, so App's `rowEls[lead].scrollIntoView`
  // can't reach it — scroll the container to it instead. In-window leads are left to that existing
  // scrollIntoView; non-virtualized behaviour is entirely untouched. Grid-aware via the measured `cols`.
  $: if (virtualize && rowH > 0 && viewportH > 0) ensureLeadVisibleVirtual(selection.lead);
  function ensureLeadVisibleVirtual(lead: number) {
    if (lead < 0 || !scrollEl) return;
    if (lead >= win.start && lead < win.end) return; // in window → existing scrollIntoView handles it
    const target = ensureVisibleOffset(lead, effScroll, viewportH, rowH, entries.length, cols);
    if (target !== effScroll) scrollEl.scrollTop += target - effScroll;
  }
</script>

{#if view === "details" && !error && !loading && entries.length > 0}
  <div class="columns" style="--filelist-cols: {colTemplate}">
    {#each COLUMNS as col (col.key)}
      <button
        class="col"
        class:num={col.num}
        on:click={() => headerClick(col.key)}
        title={$t("fl.sortBy", { col: $t(col.labelKey) })}
      >
        {$t(col.labelKey)}
        {#if sortKey === col.key}
          <span class="sortchev">
            <Icon name={sortDir === "asc" ? "chev-up" : "chev-down"} size={12} />
          </span>
        {/if}
      </button>
    {/each}
    {#each handleOffsets as x, i (i)}
      <!-- A focusable separator is the valid ARIA "window splitter" pattern; the lint
           flags the tabindex/handlers as if it were plain text, so suppress those. -->
      <!-- svelte-ignore a11y-no-static-element-interactions a11y-no-noninteractive-tabindex a11y-no-noninteractive-element-interactions -->
      <span
        class="col-resize"
        style="left: {x}px"
        role="separator"
        aria-orientation="vertical"
        aria-label={$t("fl.resizeColumn", { col: COLUMNS[i] ? $t(COLUMNS[i].labelKey) : "" })}
        aria-valuenow={Math.round(columnWidths[i])}
        tabindex="0"
        title={$t("fl.resizeTip")}
        on:pointerdown={(e) => startColResize(e, i)}
        on:keydown={(e) => onResizeKey(e, i)}
      />
    {/each}
  </div>
{/if}

{#if error}
  <div class="empty-state">
    <span class="empty-icon"><Icon name="ban" size={40} /></span>
    <p class="error">{error}</p>
  </div>
{:else if loading}
  <div class="empty-state"><p>{$t("fl.loading")}</p></div>
{:else if entries.length === 0}
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div class="empty-state" on:contextmenu={emptyContext}>
    <span class="empty-icon"><Icon name="folder" size={40} /></span>
    <p>{searching ? $t("fl.noMatch") : $t("fl.empty")}</p>
  </div>
{:else}
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div bind:this={rowsEl} class="rows" class:grid={view === "icons" || view === "gallery"} class:gallery={view === "gallery"} style="--filelist-cols: {colTemplate}" on:contextmenu={emptyContext}>
    {#if topPad > 0}
      <div class="vspacer" style="height: {topPad}px" aria-hidden="true" />
    {/if}
    {#each windowed as { entry, i } (entry.path)}
      <!--
        The view class MUST stay namespaced as "view-{view}".
        Interpolating the bare view name gave every row the class `details`,
        which collides with the global `.details` DetailsPane rule
        (display:flex; padding:20px) — that overrode the row's grid layout and
        clipped every row to nothing. The list rendered 18 blank strips while
        the status bar correctly reported "18 items". Shipped in v0.5.0. CPE-045.
      -->
      {@const insideKind = entry.is_dir ? folderActivityKindNorm(activitySets, entry.path) : null}
      {@const tagEntry = entryFor($tags, entry.path)}
      {@const act = activity[entry.path]}
      {@const ruleStyle = colorRules.length ? evaluateRules(entry, colorRules, rulesNow) : {}}
      <div
        class="row view-{view}"
        class:selected={isSelected(selection, i)}
        class:cut={cutSet.has(entry.path)}
        class:lead={selection.lead === i}
        class:droptarget={dropIndex === i}
        class:dragging={draggedSet.has(entry.path)}
        class:agent-active={!!act}
        class:agent-inside={!!insideKind}
        class:agent-inside-read={insideKind === "read"}
        class:tagged={!!tagEntry.label}
        style={tagEntry.label ? `--label-color: ${labelColor(tagEntry.label)}` : ""}
        data-agent-kind={act?.kind ?? ""}
        data-drop-path={entry.is_dir ? entry.path : null}
        bind:this={rowEls[i]}
        role="button"
        tabindex="0"
        draggable={!renamingPath && canDrag}
        on:pointerdown={() => onRowPointerDown(i)}
        on:dragstart={(e) => onDragStart(e, i)}
        on:dragend={onDragEnd}
        on:dragover={(e) => onDragOver(e, i)}
        on:dragleave={() => (dropIndex = dropIndex === i ? -1 : dropIndex)}
        on:drop={(e) => onDrop(e, i)}
        on:click|stopPropagation={(e) => rowClick(e, i)}
        on:dblclick={() => dispatch("open", entry)}
        on:contextmenu={(e) => rowContext(e, i)}
        on:keydown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            dispatch("open", entry);
          }
        }}
      >
        <span class="cell name">
          {#if (view === "icons" || view === "gallery") && !entry.is_dir && isImage(entry.name)}
            <ThumbnailImage path={entry.path} size={view === "gallery" ? 128 : 48} fallback={iconFor(entry)} />
          {:else}
            <Icon name={iconFor(entry)} size={view === "gallery" ? 88 : view === "icons" ? 40 : 16} />
          {/if}
          {#if tagEntry.label}
            <span class="label-dot" style="background: {labelColor(tagEntry.label)}" title={tagEntry.label} aria-hidden="true" />
          {/if}
          {#if renamingPath === entry.path}
            <input
              class="rename"
              bind:this={editEl}
              value={renameValue}
              on:keydown={onEditorKey}
              on:click|stopPropagation
              on:dblclick|stopPropagation
              on:blur={(e) => dispatch("commitRename", e.currentTarget.value)}
            />
          {:else}
            <span class="ellip" style={ruleStyle.color ? `color: ${ruleStyle.color}` : ""}>{entry.name}</span>
          {/if}
          {#if ruleStyle.label && renamingPath !== entry.path}
            <span class="rule-label" style={ruleStyle.color ? `background: ${ruleStyle.color}` : ""}>{ruleStyle.label}</span>
          {/if}
          {#if tagEntry.tags.length > 0 && renamingPath !== entry.path}
            <span class="tag-chips">
              {#each tagEntry.tags as tag (tag)}
                <span class="tag-chip">{tag}</span>
              {/each}
            </span>
          {/if}
          {#if act}
            <span class="agent-badge {act.kind}">{$t(ACTIVITY_LABEL_KEY[act.kind])}</span>
          {:else if insideKind}
            <span class="agent-inside-dot" title={$t("fl.agentInside")}>●</span>
          {/if}
        </span>

        {#if view === "details"}
          <span class="cell dim">{formatDate(entry.modified)}</span>
          <span class="cell dim">{typeName(entry)}</span>
          <span class="cell num">{entry.is_dir ? "" : formatSize(entry.size)}</span>
        {/if}
      </div>
    {/each}
    {#if botPad > 0}
      <div class="vspacer" style="height: {botPad}px" aria-hidden="true" />
    {/if}
  </div>
{/if}

<style>
  .ellip {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rename {
    flex: 1;
    min-width: 0;
    font: inherit;
    padding: 1px 4px;
    border: 1px solid var(--accent);
    border-radius: 3px;
    background: #fff;
    color: var(--text);
    outline: none;
  }

  /* Cut items dim until the paste completes — the affordance Explorer uses, so
     a pending move is visible rather than invisible state. */
  .row.cut {
    opacity: 0.45;
  }

  /* Agent Watch (CPE-399): a file the agent just touched gets a left accent bar + a kind
     badge, pulsing briefly on appearance so a live change draws the eye. Purely additive —
     rows with no activity are untouched (off means off). */
  .row.agent-active {
    box-shadow: inset 3px 0 0 var(--agent-accent, #3a9d4a);
    animation: agent-pulse 900ms ease-out;
  }
  @keyframes agent-pulse {
    from { background: color-mix(in srgb, var(--agent-accent, #3a9d4a) 26%, transparent); }
    to { background: transparent; }
  }
  .agent-badge {
    flex: 0 0 auto;
    margin-left: 8px;
    padding: 0 6px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    line-height: 16px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #fff;
    white-space: nowrap;
  }
  .agent-badge.created { background: #3a9d4a; }
  .agent-badge.modified { background: #b5872b; }
  .agent-badge.renamed { background: #3a72b5; }
  .agent-badge.removed { background: #b5433a; }
  /* CPE-405: a read is a consult, not a change — a muted, hollow badge. */
  .agent-badge.read {
    background: transparent;
    color: var(--text-muted, #9a9a9a);
    border: 1px solid var(--border, #5a5a5a);
  }
  /* Per-kind left accent, driven by the row's data attribute. */
  .row.agent-active[data-agent-kind="created"] { --agent-accent: #3a9d4a; }
  .row.agent-active[data-agent-kind="modified"] { --agent-accent: #b5872b; }
  .row.agent-active[data-agent-kind="renamed"] { --agent-accent: #3a72b5; }
  .row.agent-active[data-agent-kind="removed"] { --agent-accent: #b5433a; }
  /* CPE-405: dimmer accent for a read, so consulted files read as subordinate to changed ones. */
  .row.agent-active[data-agent-kind="read"] { --agent-accent: #6b6b6b; }
  /* A folder whose subtree the agent is changing — a soft accent so you can follow it down (CPE-402). */
  .row.agent-inside:not(.agent-active) {
    box-shadow: inset 3px 0 0 color-mix(in srgb, var(--accent, #2f6fed) 55%, transparent);
  }
  /* CPE-742: a subtree the agent has ONLY read (not changed) gets a cooler, dimmer tint than the write
     heat above — consistent with CPE-405's "a read is the weakest signal". Write outranks read, so a
     folder being edited keeps the accent bar. */
  .row.agent-inside-read:not(.agent-active) {
    box-shadow: inset 3px 0 0 color-mix(in srgb, var(--text-muted, #6b6b6b) 45%, transparent);
  }
  .agent-inside-dot {
    flex: 0 0 auto;
    margin-left: 8px;
    font-size: 9px;
    line-height: 1;
    color: var(--accent, #2f6fed);
    opacity: 0.8;
  }
  .row.agent-inside-read .agent-inside-dot {
    color: var(--text-muted, #6b6b6b);
    opacity: 0.6;
  }

  /* Tags (CPE-638): a tagged file gets a small colour dot before its name and its tags as chips
     after it; a labelled file also gets a soft left accent bar. Purely additive — an untagged row
     is untouched. Agent Watch's own accent bar (agent-active/inside) takes precedence over the
     label tint so a live change is never masked. */
  .row.tagged:not(.agent-active):not(.agent-inside) {
    box-shadow: inset 3px 0 0 var(--label-color);
  }
  .label-dot {
    flex: 0 0 auto;
    width: 9px;
    height: 9px;
    border-radius: 999px;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.15) inset;
  }
  /* Chip row reflows (wraps + grows) in icons view; in the fixed-height details/list rows it stays
     on one line and is clipped by the cell's overflow — the name keeps priority (tick-tacks rule:
     chips never wrap their own text). */
  .tag-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    min-width: 0;
    flex: 0 1 auto;
  }
  .tag-chip {
    flex: 0 0 auto;
    max-width: 140px;
    padding: 0 6px;
    border-radius: 999px;
    font-size: 10.5px;
    line-height: 16px;
    background: var(--surface-alt);
    border: 1px solid var(--border);
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row.view-icons .tag-chips {
    justify-content: center;
    width: 100%;
  }
  /* Rule label (CPE-776): a small pill next to the name, tinted by the rule's colour. Follows the
     tick-tacks rule — one line, never wraps its own text (max-width + ellipsis). */
  .rule-label {
    flex: 0 0 auto;
    max-width: 140px;
    margin-left: 6px;
    padding: 0 6px;
    border-radius: 999px;
    font-size: 10.5px;
    line-height: 16px;
    background: var(--accent);
    color: #fff;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row.lead:not(.selected) {
    outline: 1px dotted var(--text-faint);
    outline-offset: -1px;
  }

  /* Only valid drop targets ever highlight, so an invalid drop is visibly
     impossible rather than merely rejected after the fact. */
  .row.droptarget {
    background: var(--selection);
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .row.dragging {
    opacity: 0.5;
  }

  .row.view-list {
    grid-template-columns: 1fr;
  }

  .rows.grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(124px, 1fr));
    gap: 6px;
    padding: 10px;
  }
  /* Gallery: larger tiles for a photo light-table (CPE-658). */
  .rows.grid.gallery {
    grid-template-columns: repeat(auto-fill, minmax(184px, 1fr));
    gap: 10px;
  }

  /* A virtualization spacer spans the full grid width so it stands in for whole tile rows
     above/below the rendered window (CPE-766); in the block list/details views grid-column is
     simply ignored and it behaves as a plain-height block (CPE-690). */
  .vspacer {
    grid-column: 1 / -1;
    width: 100%;
  }

  /* Icon + gallery tiles share one column-tile layout (CPE-766 gives gallery the layout it was
     missing). Fixed tile geometry keeps every tile the SAME height, which the fixed-row-height
     windowing math depends on: a fixed 2-line name below, chips hidden in grid (the colour dot still
     signals a tag), and overflow clipped so a stray badge can't grow one tile taller than its row. */
  .row.view-icons,
  .row.view-gallery {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    height: auto;
    padding: 12px 6px;
    text-align: center;
    overflow: hidden;
  }

  .row.view-icons :global(.cell.name),
  .row.view-gallery :global(.cell.name) {
    flex-direction: column;
    gap: 6px;
    width: 100%;
  }

  /* Tag chips reflow to variable heights, which would break uniform tile height; in the grids the
     colour dot before the name is enough to flag a tag, and the full chips remain in details/list. */
  .rows.grid .tag-chips {
    display: none;
  }

  /* Column resize handles — thin hit-targets straddling each column's right edge (CPE-350).
     .columns is position:sticky, so these absolute handles are contained by it. */
  .col-resize {
    position: absolute;
    top: 0;
    height: 100%;
    width: 7px;
    margin-left: -3px;
    cursor: col-resize;
    z-index: 6;
  }
  .col-resize:hover {
    background: var(--accent);
    opacity: 0.5;
  }

  /* The name box occupies a FIXED two lines (not just a max) so every tile is the same height
     regardless of filename length — the precondition for fixed-row-height windowing (CPE-766).
     Longer names clamp with an ellipsis; shorter ones keep the reserved second line. */
  .row.view-icons .ellip,
  .row.view-gallery .ellip {
    width: 100%;
    white-space: normal;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    line-height: 1.25;
    height: 2.5em; /* 2 lines × 1.25 line-height */
  }
</style>
