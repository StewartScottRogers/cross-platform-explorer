<script lang="ts">
  // Agent Board — Kanban view over the real Tickets/ folders (CPE-521, epic CPE-503). Columns are the
  // workflow folders; dragging a card to another column calls `board_move` (the file moves + its
  // status frontmatter updates), keeping the board and the CLI /ticketing-* flow in one source of
  // truth. Read + drag only — agent dispatch is wave 2 (CPE-522). Backed by the CPE-520 commands.
  import { createEventDispatcher, onMount } from "svelte";
  import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
  import { invoke } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-958)
  import { lsGet, lsSet, lsBool } from "../persist";
  import Icon from "./Icon.svelte";
  import HelpButton from "./HelpButton.svelte";
  import {
    BOARD_LANES, groupByLane, isValidMove, ticketTask,
    clampBoardSize, loadBoardSize, saveBoardSize,
    epicProgress, doneWithArchived, filterCards,
    EPIC_COLUMNS, groupEpicsByColumn, archivedEpics, filterEpics,
    type Card, type Lane, type Epic,
  } from "../board";

  /** The folder the board scans (`<root>/Tickets/…`) — defaults to the folder being browsed. */
  export let root: string;

  /** Render as a dedicated window filling its viewport (CPE-843): drops the modal dim-backdrop and the
      centred floating panel so the board fills the standalone Agent Board window. Default `false` keeps
      the embedded overlay behaviour unchanged. */
  export let windowed = false;

  const dispatch = createEventDispatcher<{ close: void; launch: { id: string; task: string }; popout: void }>();

  // The board is a *project* tool: it stays pointed at the last project you chose (persisted), so it
  // doesn't reset to wherever you happen to be browsing. Falls back to the current folder (CPE-551).
  const BOARD_ROOT_KEY = "cpe.boardRoot";
  const savedRoot = (): string | null => lsGet(BOARD_ROOT_KEY);
  let boardRoot = savedRoot() ?? root;

  let cards: Card[] = [];
  let loading = true;
  let error = "";
  // Distinguish "nothing loaded" from "loading": an empty result across every source means this folder
  // has no readable Tickets/ — show a helpful prompt instead of a blank panel (CPE-551).
  $: isEmpty = !loading && !error && cards.length === 0 && archived.length === 0 && epics.length === 0;
  let note = ""; // last action, shown in the status bar (CPE-529)
  let dragId: string | null = null;
  let overCol: Lane | null = null;

  // Free-text card filter (CPE-555): narrows the visible cards by id/title/tag/type/priority. Empty = all.
  let boardQuery = "";
  $: filtered = filterCards(cards, boardQuery);
  // The filter excludes everything (but there ARE cards) — show a hint, not blank lanes (CPE-560).
  $: noMatch = !loading && !error && cards.length > 0 && boardQuery.trim() !== "" && filtered.length === 0;
  function onSearchKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") { boardQuery = ""; e.stopPropagation(); }
  }

  // Copy a card's ticket id to the clipboard (handy for commits/branches), with a brief ✓ (CPE-564).
  let copiedId: string | null = null;
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  async function copyId(id: string) {
    try { await navigator.clipboard.writeText(id); } catch { /* clipboard unavailable — ignore */ }
    copiedId = id;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copiedId = null), 1100);
  }

  $: grouped = groupByLane(filtered);

  // --- View preferences (CPE-556): remember the view mode + archived toggle across opens, like the
  // board root/size. A malformed/absent value degrades to the defaults (board view, archived hidden). --
  const VIEW_KEY = "cpe.boardView";
  const ARCHIVED_KEY = "cpe.boardArchived";
  const savedView = (): "board" | "epics" => (lsGet(VIEW_KEY) === "epics" ? "epics" : "board");
  const savedArchived = (): boolean => lsBool(ARCHIVED_KEY, false);
  const persistView = (v: "board" | "epics") => lsSet(VIEW_KEY, v);
  const persistArchived = (v: boolean) => lsSet(ARCHIVED_KEY, v ? "1" : "0");

  // --- Done archival (CPE-531): recent Done (top-level) shown by default; archived (dated Done/**
  // subfolders) loaded separately + shown only on demand, so the board stays fast as Done grows. ----
  let archived: Card[] = [];
  let showArchived = savedArchived();
  $: persistArchived(showArchived);
  $: doneDisplay = doneWithArchived(grouped.Done, filterCards(archived, boardQuery), showArchived);

  // --- Epic-organized view (CPE-530): a Board ⇄ Epics toggle. ------------------------------------
  let viewMode: "board" | "epics" = savedView();
  $: persistView(viewMode);
  let epics: Epic[] = [];

  // Epics as a kanban (CPE-922): epics laid out across Backlog/Doing/Done like the tickets board, with
  // the Done column's archive toggle surfacing closed epics from the dated Done/** subfolders.
  $: epicCols = groupEpicsByColumn(filterEpics(epics, boardQuery));
  $: archivedEpicList = archivedEpics(filterCards(archived, boardQuery));
  $: epicDoneDisplay = showArchived ? [...epicCols.Done, ...archivedEpicList] : epicCols.Done;
  /** Click an epic card → jump to the Board filtered to that epic's tickets (filterCards matches epic). */
  function drillEpic(id: string) { boardQuery = id; viewMode = "board"; }

  // --- Resizable panel (CPE-529): drag the corner grip; size clamped + persisted. -----------------
  const saved = loadBoardSize();
  let panelW = saved?.w ?? Math.min(1100, typeof window !== "undefined" ? window.innerWidth * 0.96 : 1100);
  let panelH = saved?.h ?? Math.min(760, typeof window !== "undefined" ? window.innerHeight * 0.92 : 760);
  function startResize(e: MouseEvent) {
    e.preventDefault();
    const sx = e.clientX, sy = e.clientY, sw = panelW, sh = panelH;
    const move = (ev: MouseEvent) => {
      const c = clampBoardSize(sw + (ev.clientX - sx), sh + (ev.clientY - sy), window.innerWidth, window.innerHeight);
      panelW = c.w; panelH = c.h;
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      saveBoardSize(panelW, panelH);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  async function load() {
    loading = true;
    error = "";
    try {
      cards = await commands.boardCards(boardRoot);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      cards = [];
    } finally {
      loading = false;
    }
  }
  async function loadEpics() {
    try {
      epics = await invoke<Epic[]>("board_epics", { root: boardRoot });
    } catch {
      epics = [];
    }
  }
  async function loadArchived() {
    try {
      archived = await invoke<Card[]>("board_archived", { root: boardRoot });
    } catch {
      archived = [];
    }
  }
  function refresh() { load(); loadEpics(); loadArchived(); }
  onMount(async () => {
    // No explicit project chosen yet? auto-detect the project you're inside — the nearest ancestor of the
    // browsed folder that has a Tickets/ folder (CPE-554) — before scanning. A saved/chosen root wins.
    if (!savedRoot()) {
      try {
        const detected = await invoke<string | null>("find_project_root", { start: root });
        if (detected) boardRoot = detected;
      } catch { /* ignore — fall back to the current folder (→ empty-state) */ }
    }
    refresh();
  });

  /** Point the board at a different project folder (one that has a Tickets/ folder), and remember it. */
  async function chooseProject() {
    let dest: string | string[] | null;
    try {
      dest = await openFolderDialog({
        directory: true,
        multiple: false,
        defaultPath: boardRoot || undefined,
        title: "Choose a project folder (one that has a Tickets/ folder)",
      });
    } catch {
      return; // dialog unavailable / errored — no-op
    }
    if (!dest || typeof dest !== "string") return; // cancelled
    boardRoot = dest;
    lsSet(BOARD_ROOT_KEY, boardRoot);
    refresh();
  }

  function onDragStart(e: DragEvent, id: string) {
    dragId = id;
    if (e.dataTransfer) {
      e.dataTransfer.setData("text/plain", id);
      e.dataTransfer.effectAllowed = "move";
    }
  }

  async function onDrop(lane: Lane) {
    overCol = null;
    const id = dragId;
    dragId = null;
    if (!id) return;
    const card = cards.find((c) => c.id === id);
    if (!card) return;
    try {
      if (lane === "Review") {
        // The virtual Review lane is the `review` tag on a Doing card — send it there.
        if (card.column !== "Doing") await invoke("board_move", { root, id, toColumn: "Doing" });
        await invoke("board_review", { root, id, on: true });
      } else {
        // A real column: clear any review mark, then move if the folder actually changes.
        if (card.tags.includes("review")) await invoke("board_review", { root, id, on: false });
        if (card.column !== lane) await invoke("board_move", { root, id, toColumn: lane });
      }
      note = lane === "Review" ? `Sent ${id} to review` : `Moved ${id} → ${lane}`;
      error = "";
    } catch (e) {
      error = `Couldn't move ${id}: ` + (e instanceof Error ? e.message : String(e));
    }
    await load(); // reconcile with the folders (also picks up CLI changes)
  }

  /** Dispatch a card to an agent (CPE-522): move it to Doing, then hand off to the Agent Deck scoped
      to this folder with the ticket injected as its task (CPE-313). The console's own launcher is the
      agent chooser and defaults to the last-used agent/provider/model. Explicit action — never on a drag. */
  async function dispatchCard(card: Card) {
    if (card.column !== "Doing" && isValidMove(cards, card.id, "Doing")) {
      cards = cards.map((c) => (c.id === card.id ? { ...c, column: "Doing" } : c));
      try {
        await invoke("board_move", { root, id: card.id, toColumn: "Doing" });
      } catch (e) {
        error = `Couldn't move ${card.id}: ` + (e instanceof Error ? e.message : String(e));
      }
      await load();
    }
    dispatch("launch", { id: card.id, task: ticketTask(card) });
  }
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="board-overlay" class:windowed on:click={(e) => { if (e.target === e.currentTarget) dispatch("close"); }}>
  <div class="board-panel" style={windowed ? "width: 100%; height: 100%;" : `width: ${panelW}px; height: ${panelH}px;`}>
    <div class="board-titlebar">
      <span class="board-title"><Icon name="code" size={15} /> Agent Board</span>
      <div class="board-tools">
        <input class="board-search" bind:value={boardQuery} on:keydown={onSearchKeydown} placeholder="Filter cards…" spellcheck="false" aria-label="Filter cards" title="Filter cards by id, title, tag, type, or priority (Esc clears)" />
        <button class="board-btn" class:active={viewMode === "board"} on:click={() => (viewMode = "board")} title="Kanban columns">▦ Board</button>
        <button class="board-btn" class:active={viewMode === "epics"} on:click={() => (viewMode = "epics")} title="Organize by epic">◧ Epics</button>
        <button class="board-btn" on:click={chooseProject} title={"Project: " + boardRoot + "\nChoose a different project folder"}>📁 Project</button>
        <button class="board-btn" on:click={refresh} title="Refresh from the Tickets/ folders">Refresh</button>
        <HelpButton section="agent-board" on:help />
        {#if !windowed}
          <button class="board-x board-popout" title="Open in its own window" aria-label="Open Agent Board in its own window" on:click={() => dispatch("popout")}>⧉</button>
        {/if}
        <button class="board-x" title="Close" aria-label="Close" on:click={() => dispatch("close")}>×</button>
      </div>
    </div>

    {#if error}<div class="board-status error">{error}</div>{/if}

    {#if loading}
      <div class="board-empty">Loading the board…</div>
    {:else if noMatch}
      <div class="board-empty">No cards match “{boardQuery.trim()}”.</div>
    {:else if isEmpty}
      <div class="board-empty board-noproject">
        <p class="np-title">No tickets found here.</p>
        <p class="np-body">The board reads a project's <code>Tickets/</code> folder, but none was found in:</p>
        <p class="np-path">{boardRoot}</p>
        <button class="board-btn np-choose" on:click={chooseProject}>📁 Choose a project folder…</button>
      </div>
    {:else if viewMode === "epics"}
      <!-- Epics kanban (CPE-922): epics as cards across Backlog/Doing/Done, mirroring the tickets board;
           the Done column carries the same "+N archived" toggle for closed epics. Read-only (moving an
           epic between states means activating/closing it, which is a heavier /ticketing-epic action). -->
      <div class="board-columns">
        {#each EPIC_COLUMNS as col}
          {@const list = col === "Done" ? epicDoneDisplay : epicCols[col]}
          <div class="board-col">
            <div class="board-col-head">
              <span class="board-col-name">{col}</span>
              <span class="board-col-count">{list.length}</span>
              {#if col === "Done" && archivedEpicList.length}
                <button class="archive-toggle" on:click={() => (showArchived = !showArchived)}
                  title="Archived (closed) epics live in the dated Done/ subfolders">{showArchived ? "hide" : `+${archivedEpicList.length} archived`}</button>
              {/if}
            </div>
            <div class="board-col-body">
              {#each list as e (e.id)}
                {@const p = epicProgress(cards, e.id)}
                <!-- svelte-ignore a11y-no-static-element-interactions a11y-click-events-have-key-events -->
                <div class="card epic-card" class:is-done={col === "Done"} on:click={() => drillEpic(e.id)}
                  role="button" tabindex="0" on:keydown={(ev) => (ev.key === "Enter" || ev.key === " ") && drillEpic(e.id)}
                  title={"Show " + e.id + "’s tickets on the board"}>
                  <div class="card-top">
                    <span class="card-id">{e.id}</span>
                    <button class="card-copy" title="Copy id" aria-label={"Copy " + e.id}
                      on:click|stopPropagation={() => copyId(e.id)} on:mousedown|stopPropagation>{copiedId === e.id ? "✓" : "⧉"}</button>
                    {#if e.status}<span class="epic-status">{e.status}</span>{/if}
                  </div>
                  <div class="card-title">{e.title}</div>
                  <div class="epic-progress" title={p.done + " of " + p.total + " tickets done"}>
                    <div class="epic-bar"><span style="width:{p.total ? Math.round((100 * p.done) / p.total) : 0}%"></span></div>
                    <span class="epic-count">{p.done}/{p.total}</span>
                  </div>
                </div>
              {/each}
              {#if list.length === 0}<div class="board-col-empty">—</div>{/if}
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <div class="board-columns">
        {#each BOARD_LANES as col}
          {@const list = col === "Done" ? doneDisplay : grouped[col]}
          <!-- svelte-ignore a11y-no-static-element-interactions -->
          <div
            class="board-col"
            class:over={overCol === col}
            on:dragover|preventDefault={() => (overCol = col)}
            on:dragleave={() => (overCol === col && (overCol = null))}
            on:drop|preventDefault={() => onDrop(col)}
          >
            <div class="board-col-head">
              <span class="board-col-name">{col}</span>
              <span class="board-col-count">{list.length}</span>
              {#if col === "Done" && archived.length}
                <button class="archive-toggle" on:click={() => (showArchived = !showArchived)}
                  title="Archived Done tickets live in the dated Done/ subfolders">{showArchived ? "hide" : `+${archived.length} archived`}</button>
              {/if}
            </div>
            <div class="board-col-body">
              {#each list as c (c.id)}
                <!-- svelte-ignore a11y-no-static-element-interactions -->
                <div class="card" draggable="true" on:dragstart={(e) => onDragStart(e, c.id)} title={c.title}>
                  <div class="card-top">
                    <span class="card-id">{c.id}</span>
                    <button class="card-copy" title="Copy id" aria-label={"Copy " + c.id}
                      on:click|stopPropagation={() => copyId(c.id)} on:mousedown|stopPropagation>{copiedId === c.id ? "✓" : "⧉"}</button>
                    {#if c.priority}<span class="card-pri">{c.priority}</span>{/if}
                  </div>
                  <div class="card-title">{c.title}</div>
                  {#if c.tags.length || c.epic || c.sprint}
                    <div class="card-tags">
                      {#if c.epic}<span class="pill epic">{c.epic}</span>{/if}
                      {#if c.sprint}<span class="pill sprint">{c.sprint}</span>{/if}
                      {#each c.tags as t}<span class="pill">{t}</span>{/each}
                    </div>
                  {/if}
                  <div class="card-actions">
                    <button class="dispatch-btn" title="Move to Doing + open an agent scoped to this ticket"
                      on:click|stopPropagation={() => dispatchCard(c)}>▶ Dispatch</button>
                  </div>
                </div>
              {/each}
              {#if list.length === 0}<div class="board-col-empty">—</div>{/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Bottom status bar (CPE-529): per-column counts, the root folder, and the last action/error. -->
    <div class="board-statusbar">
      <span class="sb-counts">
        {#each BOARD_LANES as l}<span class="sb-col">{l} <b>{grouped[l].length}</b></span>{/each}
      </span>
      <span class="sb-msg" class:error={!!error}>{error || note || ""}</span>
      <span class="sb-root" title={root}>{root}</span>
    </div>

    <!-- Resize grip (CPE-529). -->
    <!-- svelte-ignore a11y-no-static-element-interactions -->
    <div class="board-grip" title="Drag to resize" on:mousedown={startResize}></div>
  </div>
</div>

<style>
  .board-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.45); display: flex;
    align-items: center; justify-content: center; z-index: 60; }
  /* Windowed mode (CPE-843): the board IS the window — no dim backdrop, no centring, fill the viewport. */
  .board-overlay.windowed { position: static; inset: auto; background: transparent; height: 100vh; padding: 0; }
  .board-overlay.windowed .board-panel {
    max-width: none; max-height: none; border: 0; border-radius: 0; box-shadow: none;
  }
  /* Size comes from inline width/height (CPE-529 resizable); max keeps it inside the viewport. */
  .board-panel { max-width: 98vw; max-height: 96vh; display: flex; flex-direction: column; position: relative;
    background: var(--surface); color: var(--text); border: 1px solid var(--border-strong);
    border-radius: 8px; box-shadow: 0 16px 48px rgba(0,0,0,0.4); overflow: hidden; }

  .board-titlebar { display: flex; align-items: center; justify-content: space-between;
    padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .board-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .board-tools { display: flex; align-items: center; gap: 8px; }
  .board-btn { font: inherit; font-size: 12px; height: 28px; padding: 0 12px; border-radius: 6px; cursor: pointer;
    border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); }
  .board-btn:hover { background: rgba(128,128,128,0.14); }
  .board-search { font: inherit; font-size: 12px; height: 28px; padding: 0 10px; border-radius: 6px;
    border: 1px solid var(--border); background: var(--bg); color: var(--text); width: 160px; min-width: 90px; }
  .board-search::placeholder { color: var(--text-faint); }
  .board-search:focus { outline: none; border-color: var(--accent); }
  .board-btn.active { background: var(--accent); border-color: var(--accent); color: #fff; }

  /* Epics kanban (CPE-922): epics as cards across Backlog/Doing/Done, reusing the board column styles. */
  .epic-status { font-size: 10px; text-transform: uppercase; letter-spacing: .03em; color: var(--text-dim);
    margin-left: auto; }
  .epic-card { cursor: pointer; }
  .epic-card.is-done { opacity: .72; }
  .epic-card:hover { border-color: var(--accent); }
  .epic-progress { display: flex; align-items: center; gap: 8px; margin-top: 8px; }
  .epic-bar { flex: 1; height: 5px; border-radius: 3px; background: rgba(128,128,128,0.22); overflow: hidden; }
  .epic-bar span { display: block; height: 100%; background: var(--accent); border-radius: 3px; }
  .epic-count { font-size: 10px; color: var(--text-dim); font-variant-numeric: tabular-nums; }
  .board-x { border: 0; background: transparent; color: var(--text-dim); font-size: 20px; cursor: pointer;
    line-height: 1; padding: 0 4px; border-radius: 4px; }
  .board-x:hover { background: rgba(128,128,128,0.18); color: var(--text); }
  .board-popout { font-size: 15px; }

  .board-status { padding: 6px 14px; font-size: 12px; border-bottom: 1px solid var(--border); }
  .board-status.error { color: #e0706b; }
  .board-empty { flex: 1; display: grid; place-items: center; color: var(--text-dim); }
  .board-noproject { align-content: center; gap: 6px; text-align: center; padding: 24px; }
  .board-noproject .np-title { font-size: 15px; font-weight: 600; color: var(--text); margin: 0; }
  .board-noproject .np-body { margin: 0; font-size: 13px; }
  .board-noproject .np-path { margin: 0; font-size: 12px; color: var(--text); font-family: var(--mono, monospace);
    background: rgba(128,128,128,0.12); padding: 6px 10px; border-radius: 6px; max-width: 90%;
    overflow-wrap: anywhere; }
  .board-noproject .np-choose { margin-top: 8px; height: 32px; }

  .board-columns { flex: 1; display: flex; gap: 10px; padding: 12px; overflow-x: auto; min-height: 0; }
  .board-col { flex: 1 1 0; min-width: 190px; display: flex; flex-direction: column;
    background: var(--surface-alt); border: 1px solid var(--border); border-radius: 8px; min-height: 0; }
  .board-col.over { border-color: var(--accent); box-shadow: inset 0 0 0 1px var(--accent); }
  .board-col-head { display: flex; align-items: center; justify-content: space-between; padding: 8px 10px;
    border-bottom: 1px solid var(--border); font-size: 12px; }
  .board-col-name { font-weight: 600; text-transform: uppercase; letter-spacing: .04em; opacity: .8; }
  .board-col-count { font-variant-numeric: tabular-nums; opacity: .6; }
  .archive-toggle { margin-left: 4px; font: inherit; font-size: 10px; padding: 1px 6px; border-radius: 4px;
    border: 1px solid var(--border-strong); background: var(--surface); color: var(--text-dim); cursor: pointer; }
  .archive-toggle:hover { background: rgba(128,128,128,0.14); color: var(--text); }
  .board-col-body { flex: 1; overflow-y: auto; padding: 8px; display: flex; flex-direction: column; gap: 8px; }
  .board-col-empty { text-align: center; color: var(--text-faint); padding: 8px 0; }

  .card { background: var(--surface); border: 1px solid var(--border-strong); border-radius: 6px;
    padding: 8px 10px; cursor: grab; }
  .card:active { cursor: grabbing; }
  .card-top { display: flex; align-items: center; justify-content: space-between; gap: 6px; margin-bottom: 4px; }
  .card-id { font-size: 11px; font-variant-numeric: tabular-nums; opacity: .7; }
  .card-pri { font-size: 10px; opacity: .55; }
  /* Copy-id affordance (CPE-564): unobtrusive, revealed on card hover. */
  .card-copy { margin-right: auto; font: inherit; font-size: 11px; line-height: 1; padding: 1px 4px;
    border-radius: 4px; color: var(--text-faint); cursor: pointer; opacity: 0; transition: opacity .12s; }
  .card:hover .card-copy, .card-copy:focus-visible { opacity: .75; }
  .card-copy:hover { background: rgba(128,128,128,0.16); color: var(--text); opacity: 1; }
  .card-title { font-size: 12px; line-height: 1.35; }
  /* Tag pills reflow (tick-tack rule): the row wraps + grows; each pill stays one line + doesn't shrink. */
  .card-tags { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 6px; }
  .pill { white-space: nowrap; flex: 0 0 auto; max-width: 100%; overflow: hidden; text-overflow: ellipsis;
    font-size: 10px; padding: 1px 7px; border-radius: 999px; border: 1px solid var(--border-strong);
    background: var(--surface-alt); }
  .pill.epic { border-color: #a24bd0; color: #a24bd0; }
  .pill.sprint { border-color: #3a72b5; color: #3a72b5; }

  /* Bottom status bar (CPE-529). */
  .board-statusbar { display: flex; align-items: center; gap: 14px; height: 26px; flex: 0 0 auto;
    padding: 0 12px; background: var(--surface-alt); border-top: 1px solid var(--border);
    font-size: 11px; color: var(--text-dim); }
  .sb-counts { display: flex; gap: 10px; flex: 0 0 auto; }
  .sb-col { white-space: nowrap; } .sb-col b { font-variant-numeric: tabular-nums; color: var(--text); }
  .sb-msg { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: center; }
  .sb-msg.error { color: #e0706b; }
  .sb-root { flex: 0 1 auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; opacity: .7; direction: rtl; }
  /* Resize grip (CPE-529). */
  .board-grip { position: absolute; right: 0; bottom: 0; width: 16px; height: 16px; cursor: nwse-resize; z-index: 2;
    background: repeating-linear-gradient(135deg, transparent 0 2px, rgba(128,128,128,.6) 2px 3px);
    -webkit-mask: linear-gradient(135deg, transparent 45%, #000 45%); mask: linear-gradient(135deg, transparent 45%, #000 45%); }
  /* Dispatch appears on hover/focus so the card stays uncluttered at rest (CPE-522). */
  .card-actions { display: flex; justify-content: flex-end; margin-top: 6px; opacity: 0; transition: opacity .1s; }
  .card:hover .card-actions, .card:focus-within .card-actions { opacity: 1; }
  .dispatch-btn { font: inherit; font-size: 11px; height: 22px; padding: 0 8px; border-radius: 5px; cursor: pointer;
    border: 1px solid var(--accent); background: var(--accent); color: #fff; }
  .dispatch-btn:hover { filter: brightness(1.08); }
</style>
