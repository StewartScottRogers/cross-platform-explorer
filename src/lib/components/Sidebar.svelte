<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import Icon from "./Icon.svelte";
  import SidebarNode from "./SidebarNode.svelte";
  import { iconFor } from "../filetypes";
  import { displaySafeName, displaySafePath } from "../filename";
  import { formatSize, diskUsage } from "../format";
  import { t } from "../i18n";
  import { sessionColor, sessionNum, shortModel } from "../sessionChip";
  import type { DirEntry, Place, Favorite, Connection, NetShare, DensityMode } from "../types";
  import type { AgentSession } from "../sidecar";
  import type { SmartFolder } from "../smartFolders";
  import type { SavedSearch } from "../savedSearch";
  import { isValidDrop, hoverEffect } from "../dnd";
  import { sidebarSections, isOpen, toggleSection } from "../sidebarSections";
  import { sidebarOrder, orderStyleMap, reorderSection, resetSidebarOrder, moveSection } from "../sidebarOrder";
  import {
    dedupeShares,
    hasAnyNetworkRows,
    stateOf,
    stateTitle,
    discoveredShareToFormInput,
    isSavableScheme,
    type ConnState,
  } from "../network";
  import type { ConnectionFormInput } from "../network";

  export let places: Place[] = [];
  export let drives: Place[] = [];
  // Row/chrome density (CPE-1526 threaded the prop, CPE-1528 consumes it): "compact" applies the
  // `.compact` CSS class on the root `.navigation-pane` (see app.css's "Compact density (CPE-1528)"
  // rules) — tighter row height/padding for places/pins/drives/sections without clipping icons or
  // labels. "comfortable" (the default) renders pixel-identical to before this ticket. CPE-1529
  // will add the toggle control that flips this prop; this ticket only consumes it.
  export let density: DensityMode = "comfortable";
  /** User-starred files and folders, shown in the quick-access section (CPE-340). */
  export let favorites: Favorite[] = [];
  /** Live coding-agent sessions from the Agent Deck (Agent Watch, CPE-397). Each row
      navigates the explorer to the agent's Project folder. Empty ⇒ the section is hidden. */
  export let sessions: AgentSession[] = [];
  /** Free/total bytes per drive path for the usage bars (CPE-406). Absent ⇒ no bar. */
  export let driveUsage: Record<string, { free: number; total: number }> = {};
  /** Which drive paths are REMOVABLE and thus safely ejectable (CPE-1278). Only these rows get the
   *  eject affordance — fixed/system/network drives never do. Absent/false ⇒ no eject button. */
  export let driveRemovable: Record<string, boolean> = {};
  export let currentPath = "";
  export let isHome = false;
  /** The middle pane's currently selected folder (or ""), for two-way highlight
      sync (CPE-236). */
  export let selectedPath = "";
  /** Paths currently being dragged from the file list (CPE-043). */
  export let draggedPaths: string[] = [];
  /** All tags with counts, for the Tags section (CPE-639). Empty ⇒ the section is hidden. */
  export let tagList: [string, number][] = [];
  /** The active tag filter (or ""), for the highlight. */
  export let selectedTag = "";
  /** Saved smart folders, for the Smart Folders section (CPE-667). Empty ⇒ the section is hidden. */
  export let smartFolders: SmartFolder[] = [];
  /** The id of the currently-open smart folder (or ""), for the highlight. */
  export let activeSmartFolder = "";
  /** Saved STRUCTURED searches (CPE-1229, epic CPE-978): a `Condition[]` query, distinct from the
      tag-only smart folders above. Its own "Saved Searches" section; empty ⇒ hidden. */
  export let savedSearches: SavedSearch[] = [];
  /** The id of the currently-open saved search (or ""), for the highlight. */
  export let activeSavedSearch = "";
  /** Saved remote-connection profiles (CPE-1513, epic CPE-1498) — tier 1 of the Network section. */
  export let connections: Connection[] = [];
  /** OS-discovered network shares (CPE-1163's `list_network_shares`) — tier 2, deduped against
   *  `connections` below. Empty (after dedup) with no saved connections ⇒ the section is hidden. */
  export let networkShares: NetShare[] = [];
  /** Windows-native WNet-discovered shares (CPE-1519's `discover_network_windows`, `kind: "discovered"`) —
   *  tier 3, "Discovered on your network". Deduped against BOTH `connections` (tier 1) and `networkShares`
   *  (tier 2). Always empty on non-Windows (the backend command is a compiled no-op stub there) or when
   *  Windows "Network discovery" is off / nothing is advertising — the tier then renders nothing, keeping
   *  the plain explorer quiet (CLAUDE.md's additive-mode guarantee). */
  export let discoveredShares: NetShare[] = [];
  /** Live, client-tracked connect state per connection name (CPE-1513) — see `network.ts` module docs
   *  for why this can't come from the backend yet. Absent ⇒ "disconnected" (saved, not connected). */
  export let connectionStates: Record<string, ConnState> = {};
  /** The last connect-attempt error per connection name, for the state dot's tooltip. */
  export let connectionErrors: Record<string, string> = {};
  /** Whether this platform can browse the OS Trash (CPE-1560, epic CPE-1486): mirrors the
   *  Windows/Linux-only `can_restore_from_trash` gate (`trash::os_limited` isn't implemented on macOS).
   *  True shows a clickable "Open Trash" row; false shows an inert row pointing at Finder instead, so
   *  the Trash section is never broken/empty-looking on macOS. Defaults true so the section behaves
   *  normally before App's startup probe resolves. */
  export let canBrowseTrash = true;
  $: dedupedShares = dedupeShares(networkShares, connections);
  $: dedupedDiscovered = dedupeShares(discoveredShares, connections, networkShares);
  $: networkOpen = isOpen($sidebarSections, "network");
  $: trashOpen = isOpen($sidebarSections, "trash");

  const dispatch = createEventDispatcher<{
    navigate: string;
    openFile: string;
    home: void;
    repos: void;
    board: void;
    workbench: void;
    agentMenu: { x: number; y: number; sessionId?: string; sessionLabel?: string };
    openSession: { sessionId: string; cwd: string };
    drop: { paths: string[]; dest: string; ctrlKey: boolean; shiftKey: boolean };
    filterTag: string;
    tagMenu: { x: number; y: number; tag: string };
    openSmartFolder: SmartFolder;
    smartFolderMenu: { x: number; y: number; id: string; name: string };
    openSavedSearch: SavedSearch;
    savedSearchMenu: { x: number; y: number; id: string; name: string };
    /** Right-clicking a DRIVE row (CPE-1158): opens the same folder-like drive menu as a Home drive
     *  tile, targeting the drive's root path. Only drive rows dispatch this. */
    driveContext: { x: number; y: number; path: string; name: string };
    /** Safely eject a removable drive (CPE-1278). Only removable drive rows dispatch this; App runs
     *  the backend `eject_drive`, toasts the outcome, and refreshes the drive list. */
    eject: { path: string; name: string };
    /** Open the "+ Add a connection" popover (CPE-1513) — from the Network section's header "+" button,
     *  (CPE-1516) its own empty-state "＋ Add a connection" row when there are no connections/shares
     *  yet, now that the section itself is permanent rather than appearing only once something exists, or
     *  (CPE-1519) a "Discovered on your network" row, which supplies `prefill` — the form opens
     *  pre-populated with that discovered share's scheme/host/path instead of blank. */
    networkAdd: { x: number; y: number; prefill?: ConnectionFormInput };
    /** Connect a saved connection (CPE-1513): App resolves/prompts for a secret if needed, then navigates
     *  into the connection's location — works via CPE-1511's remote `list_dir` routing. */
    networkConnect: Connection;
    /** Right-clicking a saved-connection row (CPE-1513): App opens `NetworkConnectionMenu`
     *  (Connect/Disconnect · Edit · Forget). */
    networkContext: { x: number; y: number; conn: Connection };
    /** Open the browsable Trash view (CPE-1560, epic CPE-1486) — only dispatched when
     *  `canBrowseTrash` is true; the macOS row is inert and never dispatches this. */
    openTrash: void;
  }>();

  // Every sidebar section's collapse state now comes from one persisted store (CPE-675), so a layout the
  // user sets sticks and all sections behave identically. Unset = open (fresh install is fully expanded).
  $: exploreOpen = isOpen($sidebarSections, "explore");
  $: placesOpen = isOpen($sidebarSections, "places");
  $: drivesOpen = isOpen($sidebarSections, "drives");
  $: favOpen = isOpen($sidebarSections, "favorites");
  $: agentsOpen = isOpen($sidebarSections, "agents");
  $: tagsOpen = isOpen($sidebarSections, "tags");
  $: smartOpen = isOpen($sidebarSections, "smart");
  $: savedSearchOpen = isOpen($sidebarSections, "savedSearch");

  // Section ORDER (CPE-1520): a persisted array of section ids drives each section's CSS `order` —
  // deliberately orthogonal to the collapse state above (reordering a section never opens/closes it,
  // and toggling one never moves it). `omap` maps id → a flex `order` index; every top-level child of
  // `.navigation-pane` below carries an explicit `style="order:…"` from it (including separators, which
  // reuse their owning section's value — same-order flex items keep source order, so a separator that
  // sits right after a section's own markup renders right after that section wherever it ends up).
  $: omap = orderStyleMap($sidebarOrder);

  // Drag-to-reorder a section HEADER (CPE-1520): local-only state, distinct from the file-drag
  // machinery above (`draggedPaths`/`dropPath`, which drags files from the file list onto folder
  // targets) — this drags a section id onto another section's header to reorder the sidebar itself.
  //
  // CPE-1525: this used to be HTML5 drag-and-drop (`draggable="true"` + `dragstart/dragover/drop` on
  // the whole header). That's dead in the shipped app: Tauri's webview OS drag-drop handler is enabled
  // (required for file-drop-into-window, see App.svelte's `onDragDropEvent`), and on Windows/WebView2 an
  // enabled Tauri drag-drop handler swallows in-page HTML5 drag events before they ever fire — a known
  // Tauri/WebView2 limitation, not fixable by flipping `dragDrop:false` (that would kill file-drop).
  // Pointer events are NOT intercepted, so the grip now drives the reorder itself via
  // pointerdown/move/up + `setPointerCapture`, hit-testing the header under the pointer with
  // `elementFromPoint` (unaffected by capture — capture only retargets events, not geometry queries).
  // The pure reducer (`sidebarOrder.ts` — `reorderSection`/`moveSection`/etc.) is unchanged; only this
  // DOM interaction layer moved.
  let draggingSection = "";
  let dragOverSection = "";
  let dragOverBefore = true;
  let dragActive = false; // past the movement threshold — a real drag, not a click
  let dragStartX = 0;
  let dragStartY = 0;
  const SECTION_DRAG_THRESHOLD = 4; // px of pointer movement before a grip press counts as a drag

  /** Walk up from whatever element is under (x, y) to the nearest section header carrying
   *  `data-section-id`. Geometric hit-testing — unaffected by pointer capture. */
  function sectionHeaderAt(x: number, y: number): HTMLElement | null {
    let el = document.elementFromPoint(x, y) as HTMLElement | null;
    while (el && !el.dataset.sectionId) el = el.parentElement;
    return el;
  }

  function onGripPointerDown(e: PointerEvent, id: string) {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    draggingSection = id;
    dragActive = false;
    dragOverSection = "";
    dragStartX = e.clientX;
    dragStartY = e.clientY;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onGripPointerMove(e: PointerEvent) {
    if (!draggingSection) return;
    if (!dragActive) {
      if (Math.hypot(e.clientX - dragStartX, e.clientY - dragStartY) < SECTION_DRAG_THRESHOLD) return;
      dragActive = true;
    }
    const header = sectionHeaderAt(e.clientX, e.clientY);
    const id = header?.dataset.sectionId;
    if (!id || id === draggingSection) {
      dragOverSection = "";
      return;
    }
    const rect = header!.getBoundingClientRect();
    dragOverSection = id;
    dragOverBefore = e.clientY < rect.top + rect.height / 2;
  }

  function onGripPointerUp(e: PointerEvent) {
    if (!draggingSection) return;
    if (dragActive && dragOverSection) reorderSection(draggingSection, dragOverSection, dragOverBefore);
    if ((e.currentTarget as HTMLElement).hasPointerCapture?.(e.pointerId)) {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    }
    endSectionDrag();
  }

  function onGripPointerCancel() {
    endSectionDrag();
  }

  function endSectionDrag() {
    draggingSection = "";
    dragOverSection = "";
    dragActive = false;
  }

  /** Keyboard fallback (CPE-1525): the grip is itself a focusable, keyboard-actionable control —
   *  ArrowUp/ArrowDown move the section one slot without any pointer drag at all (more reliable, and
   *  accessible). Escape cancels an in-progress pointer drag. */
  function onGripKeyDown(e: KeyboardEvent, id: string) {
    if (e.key === "Escape" && draggingSection) {
      e.preventDefault();
      endSectionDrag();
      return;
    }
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      moveSection(id, e.key === "ArrowUp" ? -1 : 1);
    }
  }

  const extOf = (name: string) => {
    const i = name.lastIndexOf(".");
    return i > 0 ? name.slice(i + 1).toLowerCase() : "";
  };
  /** Last path segment of a Project folder, for the compact agent-row subtitle. */
  const baseName = (p: string) => norm(p).split("/").pop() || p;

  /** The navigation-pane path currently hovered as a drop target, or "" for none. */
  let dropPath = "";

  // CPE-1372: best-effort same-volume signal for the currently-hovered drop target (e.g. dragging
  // onto a different drive in the Drives section), mirrors FileList.svelte's own cache — re-fetched
  // only when the hovered destination changes, and reset whenever the hover moves off it.
  let hoverVolumeDest = "";
  let hoverSameVolume: boolean | null = null;

  /** Normalise separators so parent/child checks work on both platforms. */
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "");

  /**
   * A drop is valid only onto a folder that is not one of the dragged items and
   * not inside one of them — dropping a folder into its own descendant would
   * move a directory inside itself.
   */
  function validTarget(dest: string): boolean {
    return isValidDrop(draggedPaths, dest);
  }

  // CPE-1372: `draggedPaths` is bound from the file list, so it goes back to empty the moment a
  // drag ends there (drop, cancel, or dragend) — piggyback on that to clear the cached signal below
  // rather than needing our own dragend listener.
  $: if (draggedPaths.length === 0) {
    hoverVolumeDest = "";
    hoverSameVolume = null;
  }

  function onDragOver(e: DragEvent, dest: string) {
    if (!validTarget(dest)) return;
    e.preventDefault();
    // Same best-effort same-volume check as FileList.svelte's onDragOver: fetched once per distinct
    // hovered destination, never per dragover tick.
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
    dropPath = dest;
  }

  function onDrop(e: DragEvent, dest: string) {
    if (!validTarget(dest)) return;
    e.preventDefault();
    const paths = [...draggedPaths];
    dropPath = "";
    dispatch("drop", { paths, dest, ctrlKey: e.ctrlKey, shiftKey: e.shiftKey });
  }

  // Lazily-loaded children per path, and which nodes are expanded.
  let expanded = new Set<string>();
  let children: Record<string, DirEntry[]> = {};
  let loadingPaths = new Set<string>();

  /** Load a node's sub-folders once, tolerating unreadable dirs. */
  async function loadChildren(path: string): Promise<void> {
    if (children[path]) return;
    loadingPaths.add(path);
    loadingPaths = loadingPaths;
    try {
      const entries = unwrap(await commands.listDir(path));
      children[path] = entries
        .filter((e) => e.is_dir)
        .sort((a, b) => a.name.localeCompare(b.name));
      children = children;
    } catch {
      // Unreadable folder: record an empty child list so the UI shows
      // "No folders" rather than spinning forever.
      children[path] = [];
      children = children;
    } finally {
      loadingPaths.delete(path);
      loadingPaths = loadingPaths;
    }
  }

  async function toggle(path: string) {
    if (expanded.has(path)) {
      expanded.delete(path);
      expanded = expanded;
      return;
    }
    expanded.add(path);
    expanded = expanded;
    await loadChildren(path);
  }

  const isAncestorOrSelf = (anc: string, p: string) => {
    const a = norm(anc), b = norm(p);
    return b === a || b.startsWith(a + "/");
  };

  // Two-way sync (CPE-236): reveal a path by expanding the tree from its root
  // place/drive down to it, loading each level lazily. Keeps the left tree in
  // step with where the middle pane is (current folder) and what's selected.
  let revealing = new Set<string>();
  async function revealPath(path: string): Promise<void> {
    if (!path || isHome || revealing.has(path)) return;
    revealing.add(path);
    try {
      const roots = [...places, ...drives];
      let cur = roots.find((r) => isAncestorOrSelf(r.path, path))?.path;
      let guard = 0;
      while (cur && norm(cur) !== norm(path) && guard++ < 64) {
        await loadChildren(cur);
        expanded.add(cur);
        expanded = expanded;
        const next = (children[cur] ?? []).find((c) => isAncestorOrSelf(c.path, path));
        if (!next) break;
        cur = next.path;
      }
    } finally {
      revealing.delete(path);
    }
  }

  // Reveal the current folder and any selected subfolder as they change.
  $: revealPath(currentPath);
  $: if (selectedPath) revealPath(selectedPath);

  /** A node is highlighted when it is the current folder or the selected one. Compares through `norm()`
   *  (CPE-1737 round 2), like every other path comparison in this file (`isAncestorOrSelf`, `revealPath`)
   *  — a raw `===` here missed a remote directory's own tree node (`p`, always the listing-row spelling,
   *  trailing '/' included for a directory) against `currentPath`/`selectedPath` reached via Up/breadcrumb/
   *  favourite/session-restore (never slashed): the highlight would light up on first entry, then vanish
   *  the moment you left and came back by any other route. */
  const isMarked = (p: string) =>
    !isHome && (norm(p) === norm(currentPath) || (selectedPath !== "" && norm(p) === norm(selectedPath)));
</script>

<div class="navigation-pane" role="region" aria-label="Navigation" class:compact={density === "compact"}>
  {#if sessions.length > 0}
    <div
      class="nav-item agents-head section-head"
      class:drop-before={dragOverSection === "agents" && dragOverBefore}
      class:drop-after={dragOverSection === "agents" && !dragOverBefore}
      style="order:{omap.agents}"
      data-section-id="agents"
    >
      <span
        class="section-grip"
        class:grabbing={dragActive && draggingSection === "agents"}
        role="button"
        tabindex="0"
        title="Drag to reorder the Agents section (or focus + Arrow Up/Down)"
        on:pointerdown={(e) => onGripPointerDown(e, "agents")}
        on:pointermove={onGripPointerMove}
        on:pointerup={onGripPointerUp}
        on:pointercancel={onGripPointerCancel}
        on:keydown={(e) => onGripKeyDown(e, "agents")}
      ><Icon name="grip" size={12} /></span>
      <button class="twisty" class:open={agentsOpen} title={agentsOpen ? "Collapse" : "Expand"} on:click={() => toggleSection("agents")}>
        <Icon name="chev-right" size={12} />
      </button>
      <Icon name="code" />
      <span class="label agents-title">{$t("sidebar.agents")}</span>
    </div>
    {#if agentsOpen}
      <div class="nav-children" style="order:{omap.agents}">
        {#each sessions as s (s.sessionId)}
          {@const model = shortModel(s.model)}
          <button
            class="nav-item agent-item"
            class:active={isMarked(s.cwd)}
            title={`${s.agentName}${s.provider ? " · " + s.provider : ""}${s.model ? " · " + s.model : ""} · ${s.cwd}  (double-click to open its tab · right-click for more)`}
            on:click={() => dispatch("navigate", s.cwd)}
            on:dblclick={() => dispatch("openSession", { sessionId: s.sessionId, cwd: s.cwd })}
            on:contextmenu|preventDefault|stopPropagation={(e) =>
              dispatch("agentMenu", {
                x: e.clientX,
                y: e.clientY,
                sessionId: s.sessionId,
                sessionLabel: `${s.agentName || s.agentId || "Agent"}${model ? " · " + model : ""}`,
              })}
          >
            <span class="twisty hidden" />
            <span class="agent-chip" style="background:{sessionColor(s.sessionId)}">{sessionNum(s.sessionId)}</span>
            <span class="label agent-label">
              <span class="agent-name">{s.agentName || s.agentId || "Agent"}{#if model}<span class="agent-model"> · {model}</span>{/if}</span>
              <span class="agent-folder">{baseName(s.cwd)}</span>
            </span>
          </button>
        {/each}
      </div>
    {/if}
    <div class="navigation-pane-sep" style="order:{omap.agents}" />
  {/if}
  {#if favorites.length > 0}
    <div
      class="nav-item fav-head section-head"
      class:drop-before={dragOverSection === "favorites" && dragOverBefore}
      class:drop-after={dragOverSection === "favorites" && !dragOverBefore}
      style="order:{omap.favorites}"
      data-section-id="favorites"
    >
      <span
        class="section-grip"
        class:grabbing={dragActive && draggingSection === "favorites"}
        role="button"
        tabindex="0"
        title="Drag to reorder the Favorites section (or focus + Arrow Up/Down)"
        on:pointerdown={(e) => onGripPointerDown(e, "favorites")}
        on:pointermove={onGripPointerMove}
        on:pointerup={onGripPointerUp}
        on:pointercancel={onGripPointerCancel}
        on:keydown={(e) => onGripKeyDown(e, "favorites")}
      ><Icon name="grip" size={12} /></span>
      <button class="twisty" class:open={favOpen} title={favOpen ? "Collapse" : "Expand"} on:click={() => toggleSection("favorites")}>
        <Icon name="chev-right" size={12} />
      </button>
      <Icon name="star" />
      <span class="label fav-title">Favorites</span>
    </div>
    {#if favOpen}
      <div class="nav-children" style="order:{omap.favorites}">
        {#each favorites as f (f.path)}
          <button
            class="nav-item fav-item"
            class:active={isMarked(f.path)}
            title={displaySafePath(f.path)}
            on:click={() => dispatch(f.is_dir ? "navigate" : "openFile", f.path)}
          >
            <span class="twisty hidden" />
            <Icon name={f.is_dir ? "folder" : iconFor({ is_dir: false, extension: extOf(f.name) })} />
            <span class="label">{displaySafeName(f.name)}</span>
          </button>
        {/each}
      </div>
    {/if}
    <div class="navigation-pane-sep" style="order:{omap.favorites}" />
  {/if}
  {#if tagList.length > 0}
    <div
      class="nav-item fav-head section-head"
      class:drop-before={dragOverSection === "tags" && dragOverBefore}
      class:drop-after={dragOverSection === "tags" && !dragOverBefore}
      style="order:{omap.tags}"
      data-section-id="tags"
    >
      <span
        class="section-grip"
        class:grabbing={dragActive && draggingSection === "tags"}
        role="button"
        tabindex="0"
        title="Drag to reorder the Tags section (or focus + Arrow Up/Down)"
        on:pointerdown={(e) => onGripPointerDown(e, "tags")}
        on:pointermove={onGripPointerMove}
        on:pointerup={onGripPointerUp}
        on:pointercancel={onGripPointerCancel}
        on:keydown={(e) => onGripKeyDown(e, "tags")}
      ><Icon name="grip" size={12} /></span>
      <button class="twisty" class:open={tagsOpen} title={tagsOpen ? "Collapse" : "Expand"} on:click={() => toggleSection("tags")}>
        <Icon name="chev-right" size={12} />
      </button>
      <Icon name="tag" />
      <span class="label fav-title">Tags</span>
    </div>
    {#if tagsOpen}
      <div class="nav-children" style="order:{omap.tags}">
        {#each tagList as [tag, count] (tag)}
          <button
            class="nav-item fav-item"
            class:active={selectedTag === tag}
            title={`${count} item${count === 1 ? "" : "s"} tagged “${tag}” — click to filter, right-click to rename/delete`}
            on:click={() => dispatch("filterTag", tag)}
            on:contextmenu|preventDefault={(e) => dispatch("tagMenu", { x: e.clientX, y: e.clientY, tag })}
          >
            <span class="twisty hidden" />
            <Icon name="tag" />
            <span class="label">{tag}</span>
            <span class="tag-count">{count}</span>
          </button>
        {/each}
      </div>
    {/if}
    <div class="navigation-pane-sep" style="order:{omap.tags}" />
  {/if}
  {#if smartFolders.length > 0}
    <div
      class="nav-item fav-head section-head"
      class:drop-before={dragOverSection === "smart" && dragOverBefore}
      class:drop-after={dragOverSection === "smart" && !dragOverBefore}
      style="order:{omap.smart}"
      data-section-id="smart"
    >
      <span
        class="section-grip"
        class:grabbing={dragActive && draggingSection === "smart"}
        role="button"
        tabindex="0"
        title="Drag to reorder the Smart Folders section (or focus + Arrow Up/Down)"
        on:pointerdown={(e) => onGripPointerDown(e, "smart")}
        on:pointermove={onGripPointerMove}
        on:pointerup={onGripPointerUp}
        on:pointercancel={onGripPointerCancel}
        on:keydown={(e) => onGripKeyDown(e, "smart")}
      ><Icon name="grip" size={12} /></span>
      <button class="twisty" class:open={smartOpen} title={smartOpen ? "Collapse" : "Expand"} on:click={() => toggleSection("smart")}>
        <Icon name="chev-right" size={12} />
      </button>
      <Icon name="filter" />
      <span class="label fav-title">{$t("smart.section")}</span>
    </div>
    {#if smartOpen}
      <div class="nav-children" style="order:{omap.smart}">
        {#each smartFolders as sf (sf.id)}
          <button
            class="nav-item fav-item"
            class:active={activeSmartFolder === sf.id}
            title={$t("smart.itemTip", { tag: sf.tag })}
            on:click={() => dispatch("openSmartFolder", sf)}
            on:contextmenu|preventDefault={(e) => dispatch("smartFolderMenu", { x: e.clientX, y: e.clientY, id: sf.id, name: sf.name })}
          >
            <span class="twisty hidden" />
            <Icon name="filter" />
            <span class="label">{sf.name}</span>
          </button>
        {/each}
      </div>
    {/if}
    <div class="navigation-pane-sep" style="order:{omap.smart}" />
  {/if}
  {#if savedSearches.length > 0}
    <div
      class="nav-item fav-head section-head"
      class:drop-before={dragOverSection === "savedSearch" && dragOverBefore}
      class:drop-after={dragOverSection === "savedSearch" && !dragOverBefore}
      style="order:{omap.savedSearch}"
      data-section-id="savedSearch"
    >
      <span
        class="section-grip"
        class:grabbing={dragActive && draggingSection === "savedSearch"}
        role="button"
        tabindex="0"
        title="Drag to reorder the Saved Searches section (or focus + Arrow Up/Down)"
        on:pointerdown={(e) => onGripPointerDown(e, "savedSearch")}
        on:pointermove={onGripPointerMove}
        on:pointerup={onGripPointerUp}
        on:pointercancel={onGripPointerCancel}
        on:keydown={(e) => onGripKeyDown(e, "savedSearch")}
      ><Icon name="grip" size={12} /></span>
      <button class="twisty" class:open={savedSearchOpen} title={savedSearchOpen ? "Collapse" : "Expand"} on:click={() => toggleSection("savedSearch")}>
        <Icon name="chev-right" size={12} />
      </button>
      <Icon name="search" />
      <span class="label fav-title">{$t("smart.searchSection")}</span>
    </div>
    {#if savedSearchOpen}
      <div class="nav-children" style="order:{omap.savedSearch}">
        {#each savedSearches as ss (ss.id)}
          <button
            class="nav-item fav-item"
            class:active={activeSavedSearch === ss.id}
            title={$t("smart.searchItemTip")}
            on:click={() => dispatch("openSavedSearch", ss)}
            on:contextmenu|preventDefault={(e) => dispatch("savedSearchMenu", { x: e.clientX, y: e.clientY, id: ss.id, name: ss.name })}
          >
            <span class="twisty hidden" />
            <Icon name="search" />
            <span class="label">{ss.name}</span>
          </button>
        {/each}
      </div>
    {/if}
    <div class="navigation-pane-sep" style="order:{omap.savedSearch}" />
  {/if}
  <div
    class="nav-item fav-head section-head"
    class:drop-before={dragOverSection === "explore" && dragOverBefore}
    class:drop-after={dragOverSection === "explore" && !dragOverBefore}
    style="order:{omap.explore}"
    data-section-id="explore"
  >
    <span
      class="section-grip"
      class:grabbing={dragActive && draggingSection === "explore"}
      role="button"
      tabindex="0"
      title="Drag to reorder the Explore section (or focus + Arrow Up/Down)"
      on:pointerdown={(e) => onGripPointerDown(e, "explore")}
      on:pointermove={onGripPointerMove}
      on:pointerup={onGripPointerUp}
      on:pointercancel={onGripPointerCancel}
      on:keydown={(e) => onGripKeyDown(e, "explore")}
    ><Icon name="grip" size={12} /></span>
    <button class="twisty" class:open={exploreOpen} title={exploreOpen ? "Collapse" : "Expand"} aria-expanded={exploreOpen} on:click={() => toggleSection("explore")}>
      <Icon name="chev-right" size={12} />
    </button>
    <Icon name="home" />
    <span class="label fav-title">{$t("sidebar.explore")}</span>
  </div>
  {#if exploreOpen}
    <button class="nav-item" style="order:{omap.explore}" class:active={isHome} on:click={() => dispatch("home")}>
      <span class="twisty hidden" />
      <Icon name="home" />
      <span class="label">Home</span>
    </button>
    <button class="nav-item" style="order:{omap.explore}" disabled title="Gallery — not implemented yet">
      <span class="twisty hidden" />
      <Icon name="gallery" />
      <span class="label">Gallery</span>
    </button>
    <button class="nav-item" style="order:{omap.explore}" title="Browse GitHub and other code repositories" on:click={() => dispatch("repos")}>
      <span class="twisty hidden" />
      <Icon name="code" />
      <span class="label">{$t("sidebar.repositories")}</span>
    </button>
    <button class="nav-item" style="order:{omap.explore}" title="Agent Board — Kanban over this folder's Ticketing/" on:click={() => dispatch("board")}>
      <span class="twisty hidden" />
      <Icon name="documents" />
      <span class="label">Agent Board</span>
    </button>
    <button class="nav-item" style="order:{omap.explore}" title="Workbench — view this folder's git diff" on:click={() => dispatch("workbench")}>
      <span class="twisty hidden" />
      <Icon name="details" />
      <span class="label">Workbench</span>
    </button>
  {/if}

  <div class="navigation-pane-sep" style="order:{omap.explore}" />

  {#if places.length > 0}
    <div
      class="nav-item fav-head section-head"
      class:drop-before={dragOverSection === "places" && dragOverBefore}
      class:drop-after={dragOverSection === "places" && !dragOverBefore}
      style="order:{omap.places}"
      data-section-id="places"
    >
      <span
        class="section-grip"
        class:grabbing={dragActive && draggingSection === "places"}
        role="button"
        tabindex="0"
        title="Drag to reorder the Quick Access section (or focus + Arrow Up/Down)"
        on:pointerdown={(e) => onGripPointerDown(e, "places")}
        on:pointermove={onGripPointerMove}
        on:pointerup={onGripPointerUp}
        on:pointercancel={onGripPointerCancel}
        on:keydown={(e) => onGripKeyDown(e, "places")}
      ><Icon name="grip" size={12} /></span>
      <button class="twisty" class:open={placesOpen} title={placesOpen ? "Collapse" : "Expand"} aria-expanded={placesOpen} on:click={() => toggleSection("places")}>
        <Icon name="chev-right" size={12} />
      </button>
      <Icon name="folder" />
      <span class="label fav-title">{$t("sidebar.quickAccess")}</span>
    </div>
  {/if}
  {#each [...places, ...drives] as place, i (place.path)}
    {@const open = expanded.has(place.path)}
    {@const isDrive = i >= places.length}
    {#if isDrive && i === places.length}
      <div class="navigation-pane-sep" style="order:{omap.places}" />
      {#if drives.length > 0}
        <div
          class="nav-item fav-head section-head"
          class:drop-before={dragOverSection === "drives" && dragOverBefore}
          class:drop-after={dragOverSection === "drives" && !dragOverBefore}
          style="order:{omap.drives}"
          data-section-id="drives"
        >
          <span
            class="section-grip"
            class:grabbing={dragActive && draggingSection === "drives"}
            role="button"
            tabindex="0"
            title="Drag to reorder the Drives section (or focus + Arrow Up/Down)"
            on:pointerdown={(e) => onGripPointerDown(e, "drives")}
            on:pointermove={onGripPointerMove}
            on:pointerup={onGripPointerUp}
            on:pointercancel={onGripPointerCancel}
            on:keydown={(e) => onGripKeyDown(e, "drives")}
          ><Icon name="grip" size={12} /></span>
          <button class="twisty" class:open={drivesOpen} title={drivesOpen ? "Collapse" : "Expand"} aria-expanded={drivesOpen} on:click={() => toggleSection("drives")}>
            <Icon name="chev-right" size={12} />
          </button>
          <Icon name="drive" />
          <span class="label fav-title">{$t("sidebar.drives")}</span>
        </div>
      {/if}
    {/if}
    {#if isDrive ? drivesOpen : placesOpen}
    <div style="order:{isDrive ? omap.drives : omap.places}">
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div
        class="nav-item"
        class:active={isMarked(place.path)}
        class:droptarget={dropPath === place.path}
        data-drop-path={place.path}
        on:dragover={(e) => onDragOver(e, place.path)}
        on:dragleave={() => (dropPath = dropPath === place.path ? "" : dropPath)}
        on:drop={(e) => onDrop(e, place.path)}
        on:contextmenu={(e) => {
          // Drive rows get a folder-like right-click menu (CPE-1158); place rows don't (unchanged).
          if (!isDrive) return;
          e.preventDefault();
          // stopPropagation so the event doesn't bubble to window, where ContextMenu.svelte's
          // window-level dismisser would instantly close the just-opened menu (CPE-1159 — same
          // open-then-close race CPE-1157 fixed for the pane; matches the agentMenu handler above).
          e.stopPropagation();
          dispatch("driveContext", { x: e.clientX, y: e.clientY, path: place.path, name: place.name });
        }}
      >
        <button
          class="twisty"
          class:open
          title={open ? "Collapse" : "Expand"}
          on:click={() => toggle(place.path)}
        >
          <Icon name="chev-right" size={12} />
        </button>
        <Icon name={place.kind} />
        <button
          class="label"
          style="text-align:left"
          on:click={() => dispatch("navigate", place.path)}
        >
          {displaySafeName(place.name)}
        </button>
        {#if isDrive && driveRemovable[place.path]}
          <button
            class="eject-btn"
            title={`Safely eject ${displaySafeName(place.name)}`}
            aria-label={`Safely eject ${displaySafeName(place.name)}`}
            on:click|stopPropagation={() => dispatch("eject", { path: place.path, name: place.name })}
          >
            <Icon name="eject" size={13} />
          </button>
        {/if}
      </div>

      {#if isDrive && driveUsage[place.path]}
        {@const u = driveUsage[place.path]}
        {@const usage = diskUsage(u.free, u.total)}
        <div class="drive-usage" title={`${formatSize(u.free)} free of ${formatSize(u.total)}`}>
          <div class="drive-bar">
            <div class="drive-bar-fill {usage.severity}" style="width:{usage.usedPct}%" />
          </div>
          <span class="drive-free">{formatSize(u.free)} free</span>
        </div>
      {/if}

      {#if open}
        <div class="nav-children">
          {#if loadingPaths.has(place.path)}
            <div class="nav-empty">Loading…</div>
          {:else if (children[place.path] ?? []).length === 0}
            <div class="nav-empty">No folders</div>
          {:else}
            {#each children[place.path] as child (child.path)}
              <SidebarNode
                node={child}
                {expanded}
                {children}
                {loadingPaths}
                {dropPath}
                marked={isMarked}
                onToggle={toggle}
                onNavigate={(p) => dispatch("navigate", p)}
                {onDragOver}
                onDragLeave={(p) => (dropPath = dropPath === p ? "" : dropPath)}
                {onDrop}
              />
            {/each}
          {/if}
        </div>
      {/if}
    </div>
    {/if}
  {/each}

  <!-- Network section (CPE-1513, epic CPE-1498; permanent per CPE-1516): the visible entry point for the
       SFTP/WebDAV backend — three deduped tiers (saved connections, then OS-discovered `net use`/mount
       shares, then CPE-1519's Windows-native "Discovered on your network" WNet shares — each deduped
       against everything above it). Always rendered as a peer of Drives (discoverable before the first
       connection exists, like Drives is always shown with just one drive); when all three tiers are empty
       the body is just the "＋ Add a connection" control + a one-line hint, so the plain explorer stays
       visually quiet (CLAUDE.md's additive-mode guarantee) rather than gaining a heavier permanent
       section — this is the common case off Windows, or on Windows with network discovery off. -->
  <div class="navigation-pane-sep" style="order:{omap.drives}" />
  <div
    class="nav-item fav-head section-head"
    class:drop-before={dragOverSection === "network" && dragOverBefore}
    class:drop-after={dragOverSection === "network" && !dragOverBefore}
    style="order:{omap.network}"
    data-section-id="network"
  >
    <span
      class="section-grip"
      class:grabbing={dragActive && draggingSection === "network"}
      role="button"
      tabindex="0"
      title="Drag to reorder the Network section (or focus + Arrow Up/Down)"
      on:pointerdown={(e) => onGripPointerDown(e, "network")}
      on:pointermove={onGripPointerMove}
      on:pointerup={onGripPointerUp}
      on:pointercancel={onGripPointerCancel}
      on:keydown={(e) => onGripKeyDown(e, "network")}
    ><Icon name="grip" size={12} /></span>
    <button class="twisty" class:open={networkOpen} title={networkOpen ? "Collapse" : "Expand"} aria-expanded={networkOpen} on:click={() => toggleSection("network")}>
      <Icon name="chev-right" size={12} />
    </button>
    <Icon name="globe" />
    <span class="label fav-title">Network</span>
    <button
      class="add-btn"
      title="Add a connection"
      aria-label="Add a connection"
      on:click|stopPropagation={(e) => dispatch("networkAdd", { x: e.clientX, y: e.clientY })}
    >
      <Icon name="plus" size={12} />
    </button>
  </div>
  {#if networkOpen}
    <div class="nav-children" style="order:{omap.network}">
      {#each connections as conn (conn.name)}
        {@const state = stateOf(connectionStates, conn.name)}
        <button
          class="nav-item fav-item"
          title={`${conn.scheme}://${conn.host} — ${stateTitle(state, connectionErrors[conn.name])} (right-click for more)`}
          on:click={() => dispatch("networkConnect", conn)}
          on:contextmenu|preventDefault|stopPropagation={(e) =>
            dispatch("networkContext", { x: e.clientX, y: e.clientY, conn })}
        >
          <span class="twisty hidden" />
          <span class="state-dot state-{state}" aria-hidden="true" />
          <Icon name="globe" />
          <span class="label">{displaySafeName(conn.name)}</span>
        </button>
      {/each}
      {#each dedupedShares as s (s.path)}
        <button
          class="nav-item fav-item"
          title={`${displaySafePath(s.path)} — an OS-discovered network location; manage it from Home's Shared tab`}
          on:click={() => dispatch("navigate", s.path)}
        >
          <span class="twisty hidden" />
          <Icon name="drive" />
          <span class="label">{displaySafeName(s.name)}</span>
        </button>
      {/each}
      {#each dedupedDiscovered as s (s.path)}
        <!-- "Discovered on your network" tier (CPE-1519): a Windows WNet-found `\\server\share` the user
             hasn't connected to yet. There's no generic SMB client to browse it directly yet (scope stays
             discovery + pre-filled add this slice — see network.ts's SUPPORTED_SCHEMES doc), so the whole
             row's action is the one-click "＋ Add a connection" pre-fill rather than a navigate.
             CPE-1524: an mDNS row can carry a scheme discovery can see but no provider can save yet (e.g.
             `nfs://` — no NFS provider until CPE-1505). The row stays visible either way; only the add
             affordance gates on `isSavableScheme`, so it opens automatically once a provider lands. -->
        {@const prefill = discoveredShareToFormInput(s)}
        {@const savable = isSavableScheme(prefill.scheme)}
        <button
          class="nav-item fav-item"
          disabled={!savable}
          title={savable
            ? `${displaySafePath(s.path)} — discovered on your network; click to add it as a connection`
            : `${displaySafePath(s.path)} — discovered on your network; ${prefill.scheme.toUpperCase()} isn't supported yet`}
          on:click={(e) => savable && dispatch("networkAdd", { x: e.clientX, y: e.clientY, prefill })}
        >
          <span class="twisty hidden" />
          <span class="state-dot state-discovered" aria-hidden="true" title="Discovered — not yet added" />
          <Icon name="globe" />
          <span class="label">{displaySafeName(s.name)}</span>
          {#if savable}
            <span class="discover-add-hint" aria-hidden="true"><Icon name="plus" size={11} /></span>
          {/if}
        </button>
      {/each}
      {#if !hasAnyNetworkRows(connections, dedupedShares, dedupedDiscovered)}
        <!-- Empty state (CPE-1516): the section's own "first connection" affordance, replacing the old
             static Explore "Network…" row now that the section is always present. -->
        <button
          class="nav-item fav-item"
          title="Add a saved SFTP/WebDAV connection"
          on:click={(e) => dispatch("networkAdd", { x: e.clientX, y: e.clientY })}
        >
          <span class="twisty hidden" />
          <Icon name="plus" size={12} />
          <span class="label">＋ Add a connection</span>
        </button>
        <div class="nav-empty">No connections yet — add an SFTP or WebDAV server.</div>
      {/if}
    </div>
  {/if}

  <!-- Trash (CPE-1560, epic CPE-1486): the browsable OS Trash — deliberately NOT part of the
       drag-reorderable section set (SECTION_IDS/omap) above, same reasoning as the reset row below: a
       single-purpose, always-last section doesn't need to be dragged around, so it skips `sidebarOrder.ts`
       entirely and just carries a fixed CSS order past every reorderable section but before the reset
       row. It still uses the generic per-id `sidebarSections` store for collapse state (any string key
       works there), matching every other section's persisted open/closed behaviour. -->
  <div class="navigation-pane-sep" style="order:900" />
  <div class="nav-item fav-head section-head" data-section-id="trash" style="order:900">
    <button class="twisty" class:open={trashOpen} title={trashOpen ? "Collapse" : "Expand"} aria-expanded={trashOpen} on:click={() => toggleSection("trash")}>
      <Icon name="chev-right" size={12} />
    </button>
    <Icon name="delete" />
    <span class="label fav-title">{$t("sidebar.trash")}</span>
  </div>
  {#if trashOpen}
    <div class="nav-children" style="order:900">
      {#if canBrowseTrash}
        <button
          class="nav-item fav-item"
          title={$t("trash.openTip")}
          on:click={() => dispatch("openTrash")}
        >
          <span class="twisty hidden" />
          <Icon name="delete" />
          <span class="label">{$t("trash.open")}</span>
        </button>
      {:else}
        <!-- macOS: `trash::os_limited` can't list/restore here, so rather than a broken or silently
             empty view, the row itself explains where to go instead (Finder's own Trash). Rendered as
             a disabled button (not a navigate) — same "visible but inert" treatment as an unsupported
             mDNS discovery row above. -->
        <button class="nav-item fav-item" disabled title={$t("trash.macMessage")}>
          <span class="twisty hidden" />
          <Icon name="delete" />
          <span class="label">{$t("trash.macLabel")}</span>
        </button>
      {/if}
    </div>
  {/if}

  <!-- Reset section order (CPE-1520): a small, always-last affordance to undo any drag reordering above.
       Fixed order (well past any section's) so it never gets swept into the reorderable set itself. -->
  <div class="sidebar-reset-row" style="order:1000">
    <button
      class="sidebar-reset-btn"
      title="Reset the sidebar sections back to their default order"
      on:click={() => resetSidebarOrder()}
    >
      <Icon name="refresh" size={12} />
      <span>Reset section order</span>
    </button>
  </div>
</div>

<style>
  /* Only valid targets ever highlight, so an illegal drop is visibly impossible
     rather than merely rejected after the fact. */
  .nav-item.droptarget {
    background: var(--selection);
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  /* The Favorites section header reads as a heading, not a navigable row (CPE-340). */
  .fav-head { cursor: default; }
  .fav-title { font-weight: 600; }
  .tag-count { margin-left: auto; font-size: 11px; color: var(--text-faint); font-variant-numeric: tabular-nums; }
  /* Agents (Agent Watch) section — a running coding agent's Project folder (CPE-397). */
  .agents-head { cursor: default; }
  .agents-title { font-weight: 600; }
  .agent-label { display: flex; flex-direction: column; line-height: 1.15; overflow: hidden; }
  .agent-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .agent-model { opacity: 0.6; }
  .agent-folder { font-size: 11px; opacity: 0.6; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Shared session-identity chip (CPE-490): same colour+number as the Agent Deck tab, so a leaf and
     its tab correlate at a glance. */
  .agent-chip {
    flex: 0 0 auto;
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 5px;
    color: #fff;
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }
  /* Drive usage bar (CPE-406) — a thin used/free indicator under each drive, like Explorer. */
  .drive-usage { padding: 2px 10px 6px 34px; display: flex; flex-direction: column; gap: 2px; }
  .drive-bar { height: 5px; border-radius: 3px; background: rgba(128, 128, 128, 0.28); overflow: hidden; }
  .drive-bar-fill { height: 100%; border-radius: 3px; background: var(--accent, #2f6fed); }
  .drive-bar-fill.warn { background: #b5872b; }
  .drive-bar-fill.full { background: var(--danger); }
  .drive-free { font-size: 10px; opacity: 0.55; }
  /* Eject affordance on REMOVABLE drive rows only (CPE-1278). Sits at the row's trailing edge; faint
     until the row is hovered, then clear, so it never competes with the drive name at rest. */
  .eject-btn {
    flex: 0 0 auto;
    display: inline-grid;
    place-items: center;
    margin-left: auto;
    margin-right: 4px;
    padding: 2px;
    border-radius: 4px;
    color: var(--text);
    opacity: 0.45;
    cursor: pointer;
  }
  .eject-btn:hover { opacity: 1; background: var(--selection); }
  .nav-item:hover .eject-btn { opacity: 0.8; }
  /* Network section (CPE-1513) — the "+" mirrors the eject button's reveal-on-hover treatment so the
     header stays uncluttered at rest. */
  .add-btn {
    flex: 0 0 auto;
    display: inline-grid;
    place-items: center;
    margin-left: auto;
    padding: 2px;
    border-radius: 4px;
    color: var(--text);
    opacity: 0.55;
    cursor: pointer;
  }
  .add-btn:hover { opacity: 1; background: var(--selection); }
  /* Saved-connection status dot (CPE-1513): connected / saved-disconnected / error. Semantic colours
     for a status indicator (not menu text, so MENUS.md's "theme-only, never red" item-text rule doesn't
     apply here) — same pattern as the existing drive-usage bar's warn/full states above. */
  .state-dot {
    flex: 0 0 auto;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-faint);
  }
  .state-dot.state-connected { background: var(--success); }
  .state-dot.state-error { background: var(--danger); }
  /* Discovered-share status dot (CPE-1519): tier 3's "not yet added" cue — accent, not the faint default,
     since (unlike a never-connected saved connection) this row IS actionable right now via its own
     "＋ Add a connection" click. */
  .state-dot.state-discovered { background: var(--accent); }
  /* Discovered-row "＋" hint (CPE-1519): mirrors the eject-btn/add-btn reveal-on-hover treatment — faint
     at rest, clear on row hover — so the row reads as "click to add" without a second, separately-clickable
     control (the whole row is the one click). */
  .discover-add-hint {
    flex: 0 0 auto;
    display: inline-grid;
    place-items: center;
    margin-left: auto;
    opacity: 0.45;
    color: var(--text);
  }
  .nav-item:hover .discover-add-hint { opacity: 1; }

  /* Section drag-to-reorder (CPE-1520, pointer-events since CPE-1525): every section header carries a
     grip affordance; the grip itself (not the whole header) is the pointer-events drag handle, since
     WebView2 swallows HTML5 draggable DnD on the header but not pointer events on a child. The grip is
     faint until the header (or the grip itself) is hovered, matching the eject/add-btn reveal-on-hover
     treatment above. */
  .section-grip {
    flex: 0 0 auto;
    display: inline-grid;
    place-items: center;
    margin-right: -2px;
    color: var(--text-faint);
    opacity: 0.5;
    cursor: grab;
    touch-action: none; /* keep the browser from turning a grip drag into a scroll/pan gesture */
  }
  .section-grip.grabbing { cursor: grabbing; }
  .section-head:hover .section-grip { opacity: 1; }
  .section-grip:focus-visible { opacity: 1; outline: 2px solid var(--accent); outline-offset: 1px; border-radius: 3px; }
  /* Drop indicator: a thin accent bar at the header's top or bottom edge, showing where the dragged
     section will land relative to this one — inset so it never shifts layout. */
  .section-head.drop-before { box-shadow: inset 0 2px 0 0 var(--accent); }
  .section-head.drop-after { box-shadow: inset 0 -2px 0 0 var(--accent); }

  /* Reset section order (CPE-1520) — a small, quiet affordance pinned to the very bottom of the pane
     (fixed order:1000 in the markup) so it's always reachable regardless of how sections are arranged. */
  .sidebar-reset-row { padding: 8px 4px 0; }
  .sidebar-reset-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 6px;
    border-radius: var(--radius);
    color: var(--text-faint);
    font-size: 11px;
    opacity: 0.7;
  }
  .sidebar-reset-btn:hover { opacity: 1; background: var(--selection); color: var(--text); }
</style>
