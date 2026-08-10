<script lang="ts">
  import { createEventDispatcher, tick, onMount } from "svelte";
  import { windowRange, ensureVisibleOffset } from "../virtualize";
  import Icon from "./Icon.svelte";
  import ThumbnailImage from "./ThumbnailImage.svelte";
  import LinkBadge from "./LinkBadge.svelte";
  import VaultBadge from "./VaultBadge.svelte";
  import { t } from "../i18n";
  import { formatSize } from "../format";
  import { formatDate } from "../datetime";
  import { iconFor, typeName, hasThumbnail } from "../filetypes";
  import {
    columnsTemplate, resizeColumnTo, boundaryOffsets, COLUMN_DEFAULTS, fullMins,
  } from "../columns";
  import { isSelected } from "../selection";
  import { setDragData, isValidDrop, hoverEffect, resolveEffect } from "../dnd";
  import { startFileDrag, resolveDragIcon, isTauriEnv } from "../dragOut";
  import { commands } from "../bindings.gen";
  import type { Selection } from "../selection";
  import type { DirEntry, SortKey, SortDir, ViewMode, DensityMode } from "../types";
  import type { AvailableColumn, MetadataCell } from "../bindings.gen";
  import type { AgentActivity } from "../agentActivity";
  import { folderActivityKindNorm, folderOwnerNorm, normalizeActivityByKind } from "../agentActivity";
  import { colorForActor } from "../agentColors";
  import { friendlyActor } from "../agentConflicts";
  import type { AgentSession } from "../sidecar";
  import { tags, entryFor, labelColor } from "../tags";
  import { evaluateRules, type ColorRule } from "../colorRules";

  export let entries: DirEntry[] = [];
  /** Agent Watch (CPE-399): per-path live activity, keyed by absolute path. Empty ⇒ no
      annotations, so the list is visually unchanged when not watching an agent. */
  export let activity: Record<string, AgentActivity> = {};
  /** Currently-running agent sessions (CPE-1116), joined against an activity entry's `actor` so the
      owner heat-map + legend can show an agent's name/colour slot instead of a bare sessionId.
      Optional — an empty list is fine (falls back to a deterministic per-id colour + shortened id
      label), matching {@link colorForActor} / {@link friendlyActor}'s graceful degradation. */
  export let sessions: AgentSession[] = [];
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
  // Owner-coloured heat-map legend (CPE-1116): the distinct actors currently present in the
  // activity map, "You" first, then known agent sessions sorted for a stable order, "Unknown"
  // last. Empty ⇒ no legend, matching "off means off" — the row containing it renders nothing.
  $: legendActors = Array.from(new Set(Object.values(activity).map((a) => a.actor || "unknown"))).sort(
    (a, b) => {
      if (a === b) return 0;
      if (a === "user") return -1;
      if (b === "user") return 1;
      if (a === "unknown") return 1;
      if (b === "unknown") return -1;
      return a.localeCompare(b);
    },
  );
  export let selection: Selection;
  export let sortKey: SortKey = "name";
  export let sortDir: SortDir = "asc";
  export let view: ViewMode = "details";
  // Row/tile density (CPE-1526 wired the prop; CPE-1527 — epic CPE-1488 "compact/dense view mode" —
  // consumes it below): "comfortable" (the default) is today's row/tile pitch, byte-identical to before
  // this ticket; "compact" tightens the details/list row height and the icons/gallery tile pitch. See
  // `ROW_H_COMFORTABLE`/`ROW_H_COMPACT`/`detailsRowH` in the virtualization section below — the single
  // source of truth fed into both the CSS and the CPE-690/766 fixed-height windowing math.
  export let density: DensityMode = "comfortable";
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
  /** Absolute path of the currently-open archive when these rows are its synthetic in-zip children
      (CPE-673/674), else `null`. When set, each `entry.path` is an in-archive-relative path (not a real
      filesystem path — see `canDrag`'s comment) that must be extracted to a real temp file before it can
      be dragged to another OS app; see `onDragStart`'s archive branch. `null` for every ordinary (on-disk)
      view, so a plain folder's drag-out is byte-for-byte unaffected. */
  export let archivePath: string | null = null;
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
    needSizes: string[];
    /** Active metadata columns' widths changed (CPE-1146), parallel to `activeMetaColumns` by id. */
    resizeMetaColumns: { id: string; width: number }[];
    /** The visible-row paths an active metadata column still needs cells for, one entry per column
     *  that has any (CPE-1146) — the caller (ExplorerPane) streams `metadata_column_cells` for them
     *  and merges results back into `metaCells`. */
    needMetaCells: { columnId: string; paths: string[] }[];
    /** The header "Columns…" affordance was clicked (CPE-1146) — the caller opens the picker. */
    openColumnPicker: void;
  }>();

  /** Recursive folder-size column (CPE-750): when on, folder rows show their computed subtree size from
      `folderSizes` (or "…" while pending), and the component asks the parent to fill sizes for visible
      folders that aren't cached yet. Off ⇒ folders show blank in the Size column, exactly as before. */
  export let showFolderSizes = false;
  export let folderSizes: Map<string, number> = new Map();

  /** Details-view column widths (Name/Date/Type/Size), bound from the parent so they
      persist; the trailing spacer is implicit (CPE-350). */
  export let columnWidths: number[] = COLUMN_DEFAULTS.slice();
  /** Active metadata columns (CPE-1146, epic CPE-707), already resolved against the CPE-1145 catalog
      + given their own width, in display order — appended after the 4 built-ins. Empty (the default)
      renders byte-for-byte the pre-CPE-1146 fixed 4-column list. */
  export let activeMetaColumns: { col: AvailableColumn; width: number }[] = [];
  /** Cell cache for the active metadata columns: columnId -> path -> cell (CPE-1146). Owned by the
      caller (ExplorerPane), which streams fills for the visible-row `needMetaCells` requests below and
      supersedes by folder-navigation generation token (STREAMING.md). A path with no entry yet simply
      renders blank until its batch lands — never a crash, never blocks the row. */
  export let metaCells: Map<string, Map<string, MetadataCell>> = new Map();

  // The combined widths/mins the grid template + resize handles operate over: the 4 built-ins, then
  // one track per active metadata column.
  $: allWidths = columnWidths.concat(activeMetaColumns.map((ac) => ac.width));
  $: allMins = fullMins(activeMetaColumns.length);
  $: colTemplate = columnsTemplate(allWidths);
  // Right-edge offset of each column, for placing the drag handles. 10px = .columns pad-left.
  $: handleOffsets = boundaryOffsets(allWidths, 10);

  /** Split a resized combined-widths array back into the built-in `columnWidths` prop + the parallel
      `activeMetaColumns` widths, and tell the parent both changed (only the meta event fires when
      there are no active metadata columns, matching the pre-CPE-1146 wire shape exactly). */
  function applyResizedWidths(next: number[]) {
    const n = columnWidths.length;
    columnWidths = next.slice(0, n);
    activeMetaColumns = activeMetaColumns.map((ac, idx) => ({ ...ac, width: next[n + idx] }));
  }
  function emitResize() {
    dispatch("resizeColumns", columnWidths);
    if (activeMetaColumns.length) {
      dispatch("resizeMetaColumns", activeMetaColumns.map((ac) => ({ id: ac.col.id, width: ac.width })));
    }
  }

  /** Drag a column's right edge to resize it; the layout updates live and persists on
      release. `stopPropagation` keeps the click off the sort-header button. */
  function startColResize(e: PointerEvent, i: number) {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const widths = allWidths;
    const mins = allMins;
    const startW = widths[i];
    const move = (ev: PointerEvent) => {
      applyResizedWidths(resizeColumnTo(widths, i, startW + (ev.clientX - startX), mins));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      emitResize();
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
    applyResizedWidths(resizeColumnTo(allWidths, i, allWidths[i] + step, allMins));
    emitResize();
  }

  /** Label for resize-handle `i`'s aria-label, over the combined built-in + metadata column set. */
  function handleLabel(i: number): string {
    if (i < COLUMNS.length) return $t(COLUMNS[i].labelKey);
    return activeMetaColumns[i - COLUMNS.length]?.col.label ?? "";
  }

  /** Paths being dragged, and the folder row currently hovered as a target. */
  export let draggedPaths: string[] = [];

  let dropIndex = -1;

  // CPE-1372: best-effort same-volume signal for the currently-hovered drop target, so the hover
  // cursor can show "copy" for a cross-volume drop instead of always defaulting to "move" (the
  // authoritative decision is still made at drop, in App.svelte's dropInto). Re-fetched only when
  // the hovered destination changes (not on every dragover tick) — keyed by dest so a stale result
  // from a different row is never applied. Reset whenever a drag ends.
  let hoverVolumeDest = "";
  let hoverSameVolume: boolean | null = null;

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

  // Native OS drag-out preview icon (CPE-672), resolved to an absolute path once at mount so an Alt-drag
  // has it ready synchronously (see onDragStart). `null` until resolved / outside Tauri / resolution fails
  // — `resolveDragIcon` returns `null` rather than a relative fallback (CPE-1269) so this can never hold a
  // non-absolute path; `startFileDrag` treats a missing icon here as "resolve it itself" as a fallback, so
  // an unresolved icon never blocks a drag.
  let dragOutIcon: string | null = null;

  function onDragStart(e: DragEvent, i: number) {
    if (renamingPath || Date.now() < suppressDragUntil) {
      e.preventDefault();
      return;
    }
    // Drag the whole selection if the grabbed row is part of it; otherwise
    // just the grabbed row (Explorer's behaviour).
    const selEntries = isSelected(selection, i)
      ? entries.filter((_, j) => isSelected(selection, j))
      : [entries[i]];
    const paths = selEntries.map((x) => x.path);

    // ── Archive extract-on-drag-out (CPE-674) ───────────────────────────────────────────────────────
    // Archive rows (`archivePath` set — CPE-673) carry SYNTHETIC in-zip paths, not real filesystem
    // paths, so neither the internal HTML5 drag (its drop handlers resolve real paths) nor a native OS
    // drag (the OS needs a real file to hand to the drop target) can use them as-is. Alt-drag here means
    // "extract, then drag OUT": stage each selected file entry to a real temp file via the existing
    // `extractArchiveEntryAny` (CPE-1180/1182 backend, already ships zip/tar/tar.gz/7z), then hand the
    // resulting temp paths to the same native-drag wrapper CPE-672 uses. A plain (no-Alt) drag on an
    // archive row stays a no-op — exactly as it was before this ticket (the row wasn't draggable at all
    // while `canDrag` was false; see the `draggable` binding below) — never internal, never native.
    if (archivePath) {
      if (!e.altKey || !isTauriEnv()) {
        // Not the drag-out gesture (or no native-drag bridge): swallow the browser's native drag start
        // so a plain drag on a read-only archive row does nothing, matching pre-CPE-674 behaviour.
        e.preventDefault();
        return;
      }
      e.preventDefault();
      draggedPaths = [];
      const mode = resolveEffect({ ctrlKey: e.ctrlKey, shiftKey: e.shiftKey }, null);
      const zip = archivePath;
      const icon = dragOutIcon || undefined;
      // Directories can't be staged by the single-entry extractor — skip them gracefully rather than
      // failing the whole drag; extraction must finish before the native drag can start (there's no real
      // file to hand the OS until then), so this whole branch is async.
      const fileEntries = selEntries.filter((x) => !x.is_dir);
      void (async () => {
        const tempPaths: string[] = [];
        for (const f of fileEntries) {
          try {
            const r = await commands.extractArchiveEntryAny(zip, f.path);
            if (r.status === "ok") tempPaths.push(r.data);
          } catch {
            // Skip an entry that failed to stage (unsupported/corrupt) — drag whatever DID extract.
          }
        }
        if (tempPaths.length > 0) void startFileDrag(tempPaths, { icon, mode });
      })();
      return;
    }

    // ── Native OS drag-OUT (CPE-672) ────────────────────────────────────────────────────────────────
    // Coexistence approach = option (B) from the research: keep the HTML5 internal drag as the DEFAULT
    // and opt INTO a native OS drag with a discriminator — holding **Alt** while starting the drag. The
    // plugin's `startDrag` launches a NATIVE OS drag that PRE-EMPTS the HTML5 DataTransfer drag that
    // internal folder/sidebar drops (dnd.ts `setDragData` → ExplorerPane/Sidebar drop handlers) rely on;
    // the two can't run in one gesture. So a PLAIN drag stays 100% internal (zero regression — the
    // non-negotiable constraint), and Alt-drag is the explicit "take this out of the app" gesture. Gated
    // on `canDrag` (so read-only archive rows never native-drag — their paths are synthetic) and
    // `isTauriEnv()` (so a plain browser / non-Tauri target falls straight through to the HTML5 path,
    // no-op and no error). Chosen over option (A) — unifying everything on the native drag — because (A)
    // is a far bigger refactor that would route every internal drop through Tauri hit-testing, exactly the
    // regression risk constraint #1 forbids.
    if (canDrag && e.altKey && isTauriEnv()) {
      // Suppress the HTML5 internal drag so only the native OS drag runs; `startFileDrag` itself is a
      // no-op-safe async call (feature-gated, never throws). `mode` follows the same Ctrl=copy/Shift=move
      // convention as internal drops; with neither held it resolves to copy — the safe default that never
      // removes the source when dropping into another app.
      e.preventDefault();
      draggedPaths = [];
      const mode = resolveEffect({ ctrlKey: e.ctrlKey, shiftKey: e.shiftKey }, null);
      void startFileDrag(paths, { icon: dragOutIcon || undefined, mode });
      return;
    }

    draggedPaths = paths;
    setDragData(e.dataTransfer, paths);
    setDragBadge(e, paths.length);
  }

  function onDragEnd() {
    draggedPaths = [];
    dropIndex = -1;
    hoverVolumeDest = "";
    hoverSameVolume = null;
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
    const dest = entries[i].path;
    // CPE-1372: kick off the same-volume check once per distinct hovered destination (never per
    // dragover tick) so the cursor can catch up to a cross-volume target; mirrors dropInto's own
    // best-effort check (App.svelte) and the same fail-safe `.catch(() => false)`.
    if (dest !== hoverVolumeDest) {
      hoverVolumeDest = dest;
      hoverSameVolume = null;
      if (draggedPaths.length > 0) {
        commands.sameVolume(draggedPaths[0], dest).then(
          (same) => {
            if (hoverVolumeDest === dest) hoverSameVolume = same;
          },
          () => {
            if (hoverVolumeDest === dest) hoverSameVolume = false;
          },
        );
      }
    }
    if (e.dataTransfer) {
      e.dataTransfer.dropEffect = hoverEffect({ ctrlKey: e.ctrlKey, shiftKey: e.shiftKey }, hoverSameVolume);
    }
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
    // Stop here so the pane-level catch-all in ExplorerPane (CPE-1154) doesn't ALSO dispatch a
    // second contextEmpty for the same click — this handler already covers the `.rows`/`.empty-state`
    // boxes; the pane catch-all is only for the blank pixels those boxes don't cover.
    e.stopPropagation();
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
  // Row/tile pitch by density (CPE-1527, epic CPE-1488): the details/list row height is this fixed
  // constant, not DOM-measured — it's the SAME value both the `--row-h` CSS custom property on `.rows`
  // below (which `.row` reads via `height: var(--row-h)`, app.css) and the virtualizer's `rowH` are set
  // from, so the CPE-690/766 fixed-height-virtualization invariant (rendered row height == the
  // virtualizer's rowH) holds by construction — they can't drift apart. Comfortable keeps today's exact
  // 30px pitch (app.css's `--row-h` default), so the default view stays pixel-identical.
  const ROW_H_COMFORTABLE = 30;
  const ROW_H_COMPACT = 22;
  let rowsEl: HTMLDivElement | undefined;
  let scrollEl: HTMLElement | null = null;
  let effScroll = 0; // px of `.rows` content scrolled above the scroller's top fold
  let viewportH = 0;
  // Measured row/tile pitch (row height, + row-gap for grids). Details/list is set directly from
  // `detailsRowH` below (never measured); icons/gallery keeps the real DOM measurement, since tile
  // height is content-driven (icon + 2-line name + padding), all of which shrink under the
  // `.density-compact` CSS when compact.
  let rowH = density === "compact" ? ROW_H_COMPACT : ROW_H_COMFORTABLE;
  let cols = 1; // measured items-per-row (1 for details/list, N for the auto-fill grids)
  let rowGapPx = 0; // measured grid row-gap, to compensate the spacers' own gap inside the grid
  let rafPending = false;

  $: isGrid = view === "icons" || view === "gallery";
  $: virtualize = entries.length >= VIRTUALIZE_THRESHOLD;
  $: detailsRowH = density === "compact" ? ROW_H_COMPACT : ROW_H_COMFORTABLE;
  // Sync immediately (no measure/tick delay) so a live density toggle repaints the windowing on the very
  // next tick — details/list pitch is fixed by density, not measured, so there's nothing to wait on.
  $: if (!isGrid) rowH = detailsRowH;
  // Icons/gallery tile geometry, shrunk under compact (fed to <Icon>/<ThumbnailImage> in the markup).
  $: iconsIconSize = density === "compact" ? 28 : 40;
  $: iconsThumbSize = density === "compact" ? 32 : 48;
  $: galleryIconSize = density === "compact" ? 64 : 88;
  $: galleryThumbSize = density === "compact" ? 80 : 128;

  $: win =
    virtualize && rowH > 0 && viewportH > 0
      ? windowRange(effScroll, viewportH, rowH, entries.length, cols, OVERSCAN_ROWS)
      : { start: 0, end: entries.length, padTop: 0, padBottom: 0 };

  $: windowed = virtualize
    ? entries.slice(win.start, win.end).map((entry, k) => ({ entry, i: win.start + k }))
    : entries.map((entry, i) => ({ entry, i }));

  // Ask the parent to fill recursive sizes for the folders currently on screen that aren't cached yet
  // (CPE-750). Runs only when the column is on, and only for the virtualized/visible window — so scrolling
  // pulls sizes in on demand and an off column costs nothing. `folderSizes` in the deps re-checks as fills
  // land, shrinking the request to [] once the visible folders are covered.
  $: if (showFolderSizes) {
    const need = windowed
      .filter((w) => w.entry.is_dir && !folderSizes.has(w.entry.path))
      .map((w) => w.entry.path);
    if (need.length) dispatch("needSizes", need);
  }

  // Lazy metadata-cell fetch (CPE-1146): ask the caller to fill cells for whichever VISIBLE rows each
  // active column doesn't have cached yet — mirrors the folder-sizes pattern above exactly. Re-runs as
  // `windowed` changes (scroll) and as `metaCells` fills in (shrinking the request to nothing once the
  // visible window is covered), so scrolling pulls new columns' worth of cells in on demand.
  $: if (activeMetaColumns.length) {
    const reqs = activeMetaColumns
      .map((ac) => {
        const cached = metaCells.get(ac.col.id);
        const paths = windowed.filter((w) => !cached?.has(w.entry.path)).map((w) => w.entry.path);
        return { columnId: ac.col.id, paths };
      })
      .filter((r) => r.paths.length > 0);
    if (reqs.length) dispatch("needMetaCells", reqs);
  }

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
    if (isGrid) {
      const cs = getComputedStyle(rowsEl);
      // The computed `grid-template-columns` resolves `auto-fill` to concrete tracks — count them.
      cols = Math.max(1, cs.gridTemplateColumns.split(" ").filter((s) => s && s !== "none").length);
      rowGapPx = parseFloat(cs.rowGap) || 0;
      // Tile height is content-driven (icon size + 2-line name + padding — all shrunk by the
      // `.density-compact` CSS when compact), so keep measuring it off a real rendered tile (never a
      // spacer — those are `.vspacer`) rather than a fixed constant.
      const firstRow = rowsEl.querySelector<HTMLElement>(".row");
      if (firstRow) {
        const h = firstRow.getBoundingClientRect().height;
        if (h > 0) rowH = h + rowGapPx;
      }
    } else {
      // Details/list pitch is density-fixed (see `detailsRowH` above), not measured — resynced here too
      // so a fresh measureGeometry() call (e.g. right after mount) can't race the reactive assignment.
      cols = 1;
      rowGapPx = 0;
      rowH = detailsRowH;
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
    // Pre-warm the native drag-out preview icon (CPE-672) so an Alt-drag has an absolute icon path ready
    // synchronously rather than awaiting a Tauri IPC round-trip mid-gesture. Best-effort: outside Tauri it
    // resolves to the relative fallback and the Alt-drag branch never fires anyway (isTauriEnv gate).
    void resolveDragIcon().then((p) => (dragOutIcon = p));
    // May be too early (`.rows` not rendered yet) — measureGeometry()/wireScroller() are idempotent and
    // the reactive re-measure below picks it up once `.rows` exists.
    measureGeometry();
    return () => {
      scrollEl?.removeEventListener("scroll", onScrollOrResize);
      roInstance?.disconnect();
    };
  });

  // Re-measure after the folder/view/density changes (rows re-laid-out) so the window is correct on the
  // next paint — density included (CPE-1527) so a live compact/comfortable toggle re-measures the
  // icons/gallery tile pitch; details/list resyncs immediately via the reactive `rowH` assignment above.
  $: if (rowsEl) {
    void entries.length;
    void view;
    void density;
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
    <!-- Column-picker button (CPE-1147): pinned to the header's LEFT edge via absolute positioning
         so it never occupies a grid track — the file ROWS below have no matching leading track, and
         adding one here would desync every header boundary/resizer from its row cells. The Name
         header (first `.col` below) gets left padding (`.col.name`) so its label/chevron clears the
         button instead of sitting under it. -->
    <button
      class="col columns-btn"
      data-testid="open-column-picker"
      title={$t("fl.columnsButton")}
      aria-label={$t("fl.columnsButton")}
      on:click={() => dispatch("openColumnPicker")}
    ><Icon name="spreadsheet" size={13} /></button>
    {#each COLUMNS as col (col.key)}
      <button
        class="col"
        class:num={col.num}
        class:name={col.key === "name"}
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
    {#each activeMetaColumns as ac (ac.col.id)}
      <!-- Metadata column headers (CPE-1146): same sort-header behaviour as the built-ins, keyed by the
           `meta:<id>` sort-key convention so headerClick/the parent's compare need no special-casing. -->
      <button
        class="col meta"
        on:click={() => headerClick(`meta:${ac.col.id}`)}
        title={$t("fl.sortBy", { col: ac.col.label })}
      >
        {ac.col.label}
        {#if sortKey === `meta:${ac.col.id}`}
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
        aria-label={$t("fl.resizeColumn", { col: handleLabel(i) })}
        aria-valuenow={Math.round(allWidths[i])}
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
  <div bind:this={rowsEl} class="rows" class:grid={view === "icons" || view === "gallery"} class:gallery={view === "gallery"} class:density-compact={density === "compact"} style="--filelist-cols: {colTemplate}; --row-h: {detailsRowH}px" on:contextmenu={emptyContext}>
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
      {@const insideOwner = entry.is_dir ? folderOwnerNorm(activitySets, entry.path) : null}
      {@const ownerActor = act ? act.actor ?? "unknown" : insideOwner}
      {@const ownerColor = ownerActor ? colorForActor(ownerActor, sessions) : null}
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
        class:rule-tinted={!!ruleStyle.color}
        style={`${ownerColor ? `--agent-accent: ${ownerColor};` : ""}${tagEntry.label ? `--label-color: ${labelColor(tagEntry.label)};` : ""}${ruleStyle.color ? `--rule-color: ${ruleStyle.color};` : ""}`}
        data-agent-kind={act?.kind ?? ""}
        data-drop-path={entry.is_dir ? entry.path : null}
        bind:this={rowEls[i]}
        role="button"
        tabindex="0"
        draggable={!renamingPath && (canDrag || !!archivePath)}
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
          {#if (view === "icons" || view === "gallery") && !entry.is_dir && hasThumbnail(entry.name)}
            <ThumbnailImage path={entry.path} size={view === "gallery" ? galleryThumbSize : iconsThumbSize} fallback={iconFor(entry)} />
          {:else}
            <Icon name={iconFor(entry)} size={view === "gallery" ? galleryIconSize : view === "icons" ? iconsIconSize : 16} />
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
          {#if entry.is_symlink}
            <!-- Link badge (CPE-1208, epic CPE-715): only mounted for symlink rows — a folder with no
                 links renders zero of these, so the hot listing path is untouched (PURPOSE.md). The
                 target/broken lookup inside LinkBadge is itself lazy (fetched on visibility/hover). -->
            <LinkBadge path={entry.path} />
          {/if}
          {#if !entry.is_dir && entry.extension === "cpevault"}
            <!-- Vault lock/unlock badge (CPE-1249, epic CPE-738): only mounted for `.cpevault` rows, so a
                 folder with no vaults renders zero of these (hot listing path untouched, PURPOSE.md). The
                 locked/unlocked state derives purely from the reactive `vaults` store. -->
            <VaultBadge path={entry.path} />
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
          <span class="cell num">
            {#if entry.is_dir}
              {#if showFolderSizes}{folderSizes.has(entry.path) ? formatSize(folderSizes.get(entry.path) ?? 0) : "…"}{/if}
            {:else}
              {formatSize(entry.size)}
            {/if}
          </span>
          {#each activeMetaColumns as ac (ac.col.id)}
            <!-- Metadata cells (CPE-1146): the pre-formatted `display` string, per CPE-1145's guidance
                 (never reimplement byte/float/dimension formatting here). Not yet cached (still
                 in flight, or scrolled past the visible window that triggered a fetch) → blank, never
                 a placeholder that could be mistaken for a confirmed empty value. A cached but genuinely
                 empty/unsupported cell renders a dim "—" (AC: never blocks the row). -->
            {@const cell = metaCells.get(ac.col.id)?.get(entry.path)}
            <span class="cell meta">
              {#if cell}
                {#if cell.cell === "Empty" || cell.display === ""}
                  <span class="meta-empty">—</span>
                {:else}
                  {cell.display}
                {/if}
              {/if}
            </span>
          {/each}
        {/if}
      </div>
    {/each}
    {#if botPad > 0}
      <div class="vspacer" style="height: {botPad}px" aria-hidden="true" />
    {/if}
  </div>
{/if}

{#if legendActors.length > 0}
  <!-- Owner-coloured heat-map legend (CPE-1116): maps each colour currently in use on the accent
       bars above to its actor. Purely additive — rendered only while the activity map is
       non-empty, so a plain (not-watching) list looks byte-identical to before this ticket. -->
  <div class="agent-legend" role="list" aria-label={$t("fl.agentLegend")}>
    {#each legendActors as a (a)}
      <span class="agent-legend-pill" role="listitem" style="--agent-accent: {colorForActor(a, sessions)}">
        <span class="agent-legend-swatch" aria-hidden="true" />
        <span class="agent-legend-name">{friendlyActor(a, sessions)}</span>
      </span>
    {/each}
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
     rows with no activity are untouched (off means off). CPE-1116: the accent bar's colour is
     the row's OWNER (`--agent-accent`, set inline per-row from `colorForActor`), not the
     activity kind — the kind still shows via the badge below. `--agent-unknown` is the
     fallback for the rare case a row is active without a resolved owner colour. */
  .row.agent-active {
    box-shadow: inset 3px 0 0 var(--agent-accent, var(--agent-unknown));
    animation: agent-pulse 900ms ease-out;
  }
  @keyframes agent-pulse {
    from { background: color-mix(in srgb, var(--agent-accent, var(--agent-unknown)) 26%, transparent); }
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
  .agent-badge.removed { background: var(--danger); }
  /* CPE-405: a read is a consult, not a change — a muted, hollow badge. */
  .agent-badge.read {
    background: transparent;
    color: var(--text-muted, #9a9a9a);
    border: 1px solid var(--border, #5a5a5a);
  }
  /* A folder whose subtree the agent is changing — a soft accent so you can follow it down
     (CPE-402), now coloured by the subtree's owning actor (CPE-1116) via the same inline
     `--agent-accent`. */
  .row.agent-inside:not(.agent-active) {
    box-shadow: inset 3px 0 0 color-mix(in srgb, var(--agent-accent, var(--agent-unknown)) 55%, transparent);
  }
  /* CPE-742: a subtree the agent has ONLY read (not changed) gets a dimmer tint than the write
     heat above — consistent with CPE-405's "a read is the weakest signal". Write outranks read, so a
     folder being edited keeps the stronger accent (and `folderOwnerNorm` only lets read-paths vote
     when the subtree has no writes at all — CPE-1116). */
  .row.agent-inside-read:not(.agent-active) {
    box-shadow: inset 3px 0 0 color-mix(in srgb, var(--agent-accent, var(--agent-unknown)) 45%, transparent);
  }
  .agent-inside-dot {
    flex: 0 0 auto;
    margin-left: 8px;
    font-size: 9px;
    line-height: 1;
    color: var(--agent-accent, var(--agent-unknown));
    opacity: 0.8;
  }
  .row.agent-inside-read .agent-inside-dot {
    opacity: 0.6;
  }

  /* Tags (CPE-638): a tagged file gets a small colour dot before its name and its tags as chips
     after it; a labelled file also gets a soft left accent bar. Purely additive — an untagged row
     is untouched. Agent Watch's own accent bar (agent-active/inside) takes precedence over the
     label tint so a live change is never masked. */
  .row.tagged:not(.agent-active):not(.agent-inside) {
    box-shadow: inset 3px 0 0 var(--label-color);
  }
  /* Rule-based row tint (CPE-775): a matched row gets a soft wash of the rule's colour, blended with
     `color-mix` so it reads identically in light/dark on top of the list surface. It sits *under*
     selection/hover — those paint their own opaque background, so the tint only shows on a resting row
     (and never masks a live Agent Watch row). An untinted row (no rule matched) is byte-for-byte the
     old row, so an empty rule set costs nothing. */
  .row.rule-tinted:not(.selected):not(:hover):not(.agent-active):not(.agent-inside) {
    background: color-mix(in srgb, var(--rule-color) 14%, transparent);
  }
  /* A thin left accent bar keeps the rule visible even while the row is selected/hovered — mirrors the
     CPE-638 `.tagged` bar, and yields to a tag label bar (tag wins the bar; the rule keeps the wash + chip)
     and to Agent Watch. */
  .row.rule-tinted:not(.tagged):not(.agent-active):not(.agent-inside) {
    box-shadow: inset 3px 0 0 var(--rule-color);
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

  /* Compact density (CPE-1527, epic CPE-1488): tighter tile pitch — smaller minimum tile width and
     gap; tile padding/icon size shrink below via `.density-compact .row.view-icons/.view-gallery` and
     the `iconsIconSize`/`galleryIconSize`/etc. reactive sizes in the script. Comfortable (no class) is
     completely untouched, so the default view stays pixel-identical to before this ticket. */
  .rows.grid.density-compact {
    gap: 4px;
    padding: 6px;
  }
  .rows.grid.density-compact:not(.gallery) {
    grid-template-columns: repeat(auto-fill, minmax(90px, 1fr));
  }
  .rows.grid.gallery.density-compact {
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 6px;
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

  /* Compact density (CPE-1527): tighter tile padding/gap — shrinks the measured tile height that
     `measureGeometry()` feeds the virtualizer as `rowH`, same fixed-per-view-pitch invariant
     (CPE-690/766), just a smaller constant driven by real layout rather than a guessed number. */
  .rows.density-compact .row.view-icons,
  .rows.density-compact .row.view-gallery {
    gap: 3px;
    padding: 6px 4px;
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

  /* Owner-coloured heat-map legend (CPE-1116): a row of pills that REFLOWS — the container wraps
     onto more rows and grows its height, while each pill keeps its text on one line and never
     shrinks (tick-tacks convention, see CLAUDE.md "Pills / chips / badges"). Rendered only while
     the activity map is non-empty (see the {#if} above), so an idle list has zero footprint. */
  .agent-legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 6px 10px;
    border-top: 1px solid var(--border);
  }
  .agent-legend-pill {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: 0 0 auto;
    padding: 2px 8px 2px 6px;
    border-radius: 999px;
    background: var(--hover);
    font-size: 11px;
    color: var(--text-dim);
    white-space: nowrap;
  }
  .agent-legend-swatch {
    flex: 0 0 auto;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--agent-accent, var(--agent-unknown));
  }
  .agent-legend-name {
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
