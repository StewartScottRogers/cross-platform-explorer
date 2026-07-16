<script lang="ts">
  // Agent Board — Kanban view over the real Tickets/ folders (CPE-521, epic CPE-503). Columns are the
  // workflow folders; dragging a card to another column calls `board_move` (the file moves + its
  // status frontmatter updates), keeping the board and the CLI /ticketing-* flow in one source of
  // truth. Read + drag only — agent dispatch is wave 2 (CPE-522). Backed by the CPE-520 commands.
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "./Icon.svelte";
  import { BOARD_COLUMNS, groupByColumn, isValidMove, ticketTask, type Card, type Column } from "../board";

  /** Repo root that contains the Tickets/ folder. */
  export let root: string;

  const dispatch = createEventDispatcher<{ close: void; launch: { id: string; task: string } }>();

  let cards: Card[] = [];
  let loading = true;
  let error = "";
  let dragId: string | null = null;
  let overCol: Column | null = null;

  $: grouped = groupByColumn(cards);

  async function load() {
    loading = true;
    error = "";
    try {
      cards = await invoke<Card[]>("board_cards", { root });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      cards = [];
    } finally {
      loading = false;
    }
  }
  onMount(load);

  function onDragStart(e: DragEvent, id: string) {
    dragId = id;
    if (e.dataTransfer) {
      e.dataTransfer.setData("text/plain", id);
      e.dataTransfer.effectAllowed = "move";
    }
  }

  async function onDrop(col: Column) {
    overCol = null;
    const id = dragId;
    dragId = null;
    if (!id || !isValidMove(cards, id, col)) return;
    // Optimistic: reflect the move immediately, then persist + reconcile.
    cards = cards.map((c) => (c.id === id ? { ...c, column: col } : c));
    try {
      await invoke("board_move", { root, id, toColumn: col });
    } catch (e) {
      error = `Couldn't move ${id}: ` + (e instanceof Error ? e.message : String(e));
    }
    await load(); // reconcile with the folders (also picks up CLI changes)
  }

  /** Dispatch a card to an agent (CPE-522): move it to Doing, then hand off to the AI Console scoped
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
<div class="board-overlay" on:click={(e) => { if (e.target === e.currentTarget) dispatch("close"); }}>
  <div class="board-panel">
    <div class="board-titlebar">
      <span class="board-title"><Icon name="code" size={15} /> Agent Board</span>
      <div class="board-tools">
        <button class="board-btn" on:click={load} title="Refresh from the Tickets/ folders">Refresh</button>
        <button class="board-x" title="Close" aria-label="Close" on:click={() => dispatch("close")}>×</button>
      </div>
    </div>

    {#if error}<div class="board-status error">{error}</div>{/if}

    {#if loading}
      <div class="board-empty">Loading the board…</div>
    {:else}
      <div class="board-columns">
        {#each BOARD_COLUMNS as col}
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
              <span class="board-col-count">{grouped[col].length}</span>
            </div>
            <div class="board-col-body">
              {#each grouped[col] as c (c.id)}
                <!-- svelte-ignore a11y-no-static-element-interactions -->
                <div class="card" draggable="true" on:dragstart={(e) => onDragStart(e, c.id)} title={c.title}>
                  <div class="card-top">
                    <span class="card-id">{c.id}</span>
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
              {#if grouped[col].length === 0}<div class="board-col-empty">—</div>{/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .board-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.45); display: flex;
    align-items: center; justify-content: center; z-index: 60; }
  .board-panel { width: min(1100px, 96vw); height: min(760px, 92vh); display: flex; flex-direction: column;
    background: var(--surface); color: var(--text); border: 1px solid var(--border-strong);
    border-radius: 8px; box-shadow: 0 16px 48px rgba(0,0,0,0.4); overflow: hidden; }

  .board-titlebar { display: flex; align-items: center; justify-content: space-between;
    padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .board-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .board-tools { display: flex; align-items: center; gap: 8px; }
  .board-btn { font: inherit; font-size: 12px; height: 28px; padding: 0 12px; border-radius: 6px; cursor: pointer;
    border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); }
  .board-btn:hover { background: rgba(128,128,128,0.14); }
  .board-x { border: 0; background: transparent; color: var(--text-dim); font-size: 20px; cursor: pointer;
    line-height: 1; padding: 0 4px; border-radius: 4px; }
  .board-x:hover { background: rgba(128,128,128,0.18); color: var(--text); }

  .board-status { padding: 6px 14px; font-size: 12px; border-bottom: 1px solid var(--border); }
  .board-status.error { color: #e0706b; }
  .board-empty { flex: 1; display: grid; place-items: center; color: var(--text-dim); }

  .board-columns { flex: 1; display: flex; gap: 10px; padding: 12px; overflow-x: auto; min-height: 0; }
  .board-col { flex: 1 1 0; min-width: 190px; display: flex; flex-direction: column;
    background: var(--surface-alt); border: 1px solid var(--border); border-radius: 8px; min-height: 0; }
  .board-col.over { border-color: var(--accent); box-shadow: inset 0 0 0 1px var(--accent); }
  .board-col-head { display: flex; align-items: center; justify-content: space-between; padding: 8px 10px;
    border-bottom: 1px solid var(--border); font-size: 12px; }
  .board-col-name { font-weight: 600; text-transform: uppercase; letter-spacing: .04em; opacity: .8; }
  .board-col-count { font-variant-numeric: tabular-nums; opacity: .6; }
  .board-col-body { flex: 1; overflow-y: auto; padding: 8px; display: flex; flex-direction: column; gap: 8px; }
  .board-col-empty { text-align: center; color: var(--text-faint); padding: 8px 0; }

  .card { background: var(--surface); border: 1px solid var(--border-strong); border-radius: 6px;
    padding: 8px 10px; cursor: grab; }
  .card:active { cursor: grabbing; }
  .card-top { display: flex; align-items: center; justify-content: space-between; gap: 6px; margin-bottom: 4px; }
  .card-id { font-size: 11px; font-variant-numeric: tabular-nums; opacity: .7; }
  .card-pri { font-size: 10px; opacity: .55; }
  .card-title { font-size: 12px; line-height: 1.35; }
  /* Tag pills reflow (tick-tack rule): the row wraps + grows; each pill stays one line + doesn't shrink. */
  .card-tags { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 6px; }
  .pill { white-space: nowrap; flex: 0 0 auto; max-width: 100%; overflow: hidden; text-overflow: ellipsis;
    font-size: 10px; padding: 1px 7px; border-radius: 999px; border: 1px solid var(--border-strong);
    background: var(--surface-alt); }
  .pill.epic { border-color: #a24bd0; color: #a24bd0; }
  .pill.sprint { border-color: #3a72b5; color: #3a72b5; }
  /* Dispatch appears on hover/focus so the card stays uncluttered at rest (CPE-522). */
  .card-actions { display: flex; justify-content: flex-end; margin-top: 6px; opacity: 0; transition: opacity .1s; }
  .card:hover .card-actions, .card:focus-within .card-actions { opacity: 1; }
  .dispatch-btn { font: inherit; font-size: 11px; height: 22px; padding: 0 8px; border-radius: 5px; cursor: pointer;
    border: 1px solid var(--accent); background: var(--accent); color: #fff; }
  .dispatch-btn:hover { filter: brightness(1.08); }
</style>
