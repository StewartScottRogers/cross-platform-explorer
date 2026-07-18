<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { invoke } from "../invoke";
  import Icon from "./Icon.svelte";
  import SidebarNode from "./SidebarNode.svelte";
  import { iconFor } from "../filetypes";
  import { formatSize, diskUsage } from "../format";
  import { t } from "../i18n";
  import { sessionColor, sessionNum, shortModel } from "../sessionChip";
  import type { DirEntry, Place, Favorite } from "../types";
  import type { AgentSession } from "../sidecar";
  import type { SmartFolder } from "../smartFolders";

  export let places: Place[] = [];
  export let drives: Place[] = [];
  /** User-starred files and folders, shown in the quick-access section (CPE-340). */
  export let favorites: Favorite[] = [];
  /** Live coding-agent sessions from the AI Console (Agent Watch, CPE-397). Each row
      navigates the explorer to the agent's Project folder. Empty ⇒ the section is hidden. */
  export let sessions: AgentSession[] = [];
  /** Free/total bytes per drive path for the usage bars (CPE-406). Absent ⇒ no bar. */
  export let driveUsage: Record<string, { free: number; total: number }> = {};
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

  const dispatch = createEventDispatcher<{
    navigate: string;
    openFile: string;
    home: void;
    repos: void;
    board: void;
    workbench: void;
    agentMenu: { x: number; y: number; sessionId?: string; sessionLabel?: string };
    openSession: { sessionId: string; cwd: string };
    drop: { paths: string[]; dest: string; copy: boolean };
    filterTag: string;
    tagMenu: { x: number; y: number; tag: string };
    openSmartFolder: SmartFolder;
    smartFolderMenu: { x: number; y: number; id: string; name: string };
  }>();

  /** Favorites section collapse state (transient, like the Home twisties). */
  let favOpen = true;
  /** Agents (Agent Watch) section collapse state. */
  let agentsOpen = true;
  /** Tags section collapse state (CPE-639). */
  let tagsOpen = true;
  /** Smart Folders section collapse state (CPE-667). */
  let smartOpen = true;
  const extOf = (name: string) => {
    const i = name.lastIndexOf(".");
    return i > 0 ? name.slice(i + 1).toLowerCase() : "";
  };
  /** Last path segment of a Project folder, for the compact agent-row subtitle. */
  const baseName = (p: string) => norm(p).split("/").pop() || p;

  /** The navigation-pane path currently hovered as a drop target, or "" for none. */
  let dropPath = "";

  /** Normalise separators so parent/child checks work on both platforms. */
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "");

  /**
   * A drop is valid only onto a folder that is not one of the dragged items and
   * not inside one of them — dropping a folder into its own descendant would
   * move a directory inside itself.
   */
  function validTarget(dest: string): boolean {
    if (draggedPaths.length === 0 || !dest) return false;
    const d = norm(dest);
    return !draggedPaths.some((p) => {
      const s = norm(p);
      return d === s || d.startsWith(s + "/");
    });
  }

  function onDragOver(e: DragEvent, dest: string) {
    if (!validTarget(dest)) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = e.ctrlKey ? "copy" : "move";
    dropPath = dest;
  }

  function onDrop(e: DragEvent, dest: string) {
    if (!validTarget(dest)) return;
    e.preventDefault();
    const paths = [...draggedPaths];
    const copy = e.ctrlKey;
    dropPath = "";
    dispatch("drop", { paths, dest, copy });
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
      const entries = await invoke<DirEntry[]>("list_dir", { path });
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

  /** A node is highlighted when it is the current folder or the selected one. */
  const isMarked = (p: string) =>
    !isHome && (p === currentPath || (selectedPath !== "" && p === selectedPath));
</script>

<div class="navigation-pane" role="region" aria-label="Navigation">
  {#if sessions.length > 0}
    <div class="nav-item agents-head">
      <button class="twisty" class:open={agentsOpen} title={agentsOpen ? "Collapse" : "Expand"} on:click={() => (agentsOpen = !agentsOpen)}>
        <Icon name="chev-right" size={12} />
      </button>
      <Icon name="code" />
      <span class="label agents-title">{$t("sidebar.agents")}</span>
    </div>
    {#if agentsOpen}
      <div class="nav-children">
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
    <div class="navigation-pane-sep" />
  {/if}
  {#if favorites.length > 0}
    <div class="nav-item fav-head">
      <button class="twisty" class:open={favOpen} title={favOpen ? "Collapse" : "Expand"} on:click={() => (favOpen = !favOpen)}>
        <Icon name="chev-right" size={12} />
      </button>
      <Icon name="star" />
      <span class="label fav-title">Favorites</span>
    </div>
    {#if favOpen}
      <div class="nav-children">
        {#each favorites as f (f.path)}
          <button
            class="nav-item fav-item"
            class:active={isMarked(f.path)}
            title={f.path}
            on:click={() => dispatch(f.is_dir ? "navigate" : "openFile", f.path)}
          >
            <span class="twisty hidden" />
            <Icon name={f.is_dir ? "folder" : iconFor({ is_dir: false, extension: extOf(f.name) })} />
            <span class="label">{f.name}</span>
          </button>
        {/each}
      </div>
    {/if}
    <div class="navigation-pane-sep" />
  {/if}
  {#if tagList.length > 0}
    <div class="nav-item fav-head">
      <button class="twisty" class:open={tagsOpen} title={tagsOpen ? "Collapse" : "Expand"} on:click={() => (tagsOpen = !tagsOpen)}>
        <Icon name="chev-right" size={12} />
      </button>
      <Icon name="tag" />
      <span class="label fav-title">Tags</span>
    </div>
    {#if tagsOpen}
      <div class="nav-children">
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
    <div class="navigation-pane-sep" />
  {/if}
  {#if smartFolders.length > 0}
    <div class="nav-item fav-head">
      <button class="twisty" class:open={smartOpen} title={smartOpen ? "Collapse" : "Expand"} on:click={() => (smartOpen = !smartOpen)}>
        <Icon name="chev-right" size={12} />
      </button>
      <Icon name="filter" />
      <span class="label fav-title">{$t("smart.section")}</span>
    </div>
    {#if smartOpen}
      <div class="nav-children">
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
    <div class="navigation-pane-sep" />
  {/if}
  <button class="nav-item" class:active={isHome} on:click={() => dispatch("home")}>
    <span class="twisty hidden" />
    <Icon name="home" />
    <span class="label">Home</span>
  </button>
  <button class="nav-item" disabled title="Gallery — not implemented yet">
    <span class="twisty hidden" />
    <Icon name="gallery" />
    <span class="label">Gallery</span>
  </button>
  <button class="nav-item" title="Browse GitHub and other code repositories" on:click={() => dispatch("repos")}>
    <span class="twisty hidden" />
    <Icon name="code" />
    <span class="label">{$t("sidebar.repositories")}</span>
  </button>

  <button class="nav-item" title="Agent Board — Kanban over this folder's Tickets/" on:click={() => dispatch("board")}>
    <span class="twisty hidden" />
    <Icon name="documents" />
    <span class="label">Agent Board</span>
  </button>

  <button class="nav-item" title="Workbench — view this folder's git diff" on:click={() => dispatch("workbench")}>
    <span class="twisty hidden" />
    <Icon name="details" />
    <span class="label">Workbench</span>
  </button>

  <div class="navigation-pane-sep" />

  {#each [...places, ...drives] as place, i (place.path)}
    {@const open = expanded.has(place.path)}
    {@const isDrive = i >= places.length}
    {#if isDrive && i === places.length}
      <div class="navigation-pane-sep" />
    {/if}
    <div>
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div
        class="nav-item"
        class:active={isMarked(place.path)}
        class:droptarget={dropPath === place.path}
        on:dragover={(e) => onDragOver(e, place.path)}
        on:dragleave={() => (dropPath = dropPath === place.path ? "" : dropPath)}
        on:drop={(e) => onDrop(e, place.path)}
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
          {place.name}
        </button>
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
  {/each}
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
  /* Shared session-identity chip (CPE-490): same colour+number as the AI Console tab, so a leaf and
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
  .drive-bar-fill.full { background: #b5433a; }
  .drive-free { font-size: 10px; opacity: 0.55; }
</style>
