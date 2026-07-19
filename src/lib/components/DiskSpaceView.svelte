<script lang="ts">
  /**
   * Disk-usage "Space" analyzer (CPE-751, epic CPE-706): an interactive treemap of a folder's space,
   * each tile sized by its child's recursive size (from the `dir_children_sizes` backend, CPE-749).
   * Click a folder tile to drill in (breadcrumb climbs back); a side list surfaces the largest items;
   * tooltips show name / size / % of parent. Modal, themed, opt-in — scanning only happens while this
   * is open, and a superseded scan is discarded by generation (fast-when-off, PURPOSE.md). Reveal/delete
   * actions are CPE-752.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "../invoke";
  import { squarify, type Tile } from "../treemap";
  import { formatSize } from "../format";
  import Icon from "./Icon.svelte";

  export let path: string;

  interface Child {
    name: string;
    path: string;
    is_dir: boolean;
    size: number;
  }

  const dispatch = createEventDispatcher<{ navigate: string; close: void }>();

  const W = 900;
  const H = 520;

  let cur = path;
  let trail: string[] = [];
  let children: Child[] = [];
  let byKey: Record<string, Child> = {};
  let tiles: Tile[] = [];
  let total = 0;
  let loading = true;
  let error = "";
  let gen = 0;

  async function scan(dir: string) {
    const g = ++gen;
    loading = true;
    error = "";
    try {
      const kids = await invoke<Child[]>("dir_children_sizes", { path: dir });
      if (g !== gen) return; // a newer scan superseded this one
      children = kids.slice().sort((a, b) => b.size - a.size);
      total = children.reduce((s, c) => s + c.size, 0);
      byKey = Object.fromEntries(children.map((c) => [c.path, c]));
      tiles = squarify(
        children.map((c) => ({ key: c.path, size: c.size })),
        0,
        0,
        W,
        H,
      );
    } catch (e) {
      if (g !== gen) return;
      error = String(e);
      children = [];
      tiles = [];
      total = 0;
    } finally {
      if (g === gen) loading = false;
    }
  }

  function drill(c: Child) {
    if (!c.is_dir || c.size === 0) return;
    trail = [...trail, cur];
    cur = c.path;
    scan(cur);
  }
  function up() {
    if (trail.length === 0) return;
    cur = trail[trail.length - 1];
    trail = trail.slice(0, -1);
    scan(cur);
  }

  const baseOf = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "").split("/").pop() || p;
  const pct = (n: number) => (total > 0 ? Math.round((n / total) * 100) : 0);
  const labelable = (t: Tile) => t.w > 48 && t.h > 22;
  // Bigger share => more saturated fill; folders and files get a subtly different hue.
  const opacityFor = (c: Child) => 0.28 + 0.5 * (total > 0 ? c.size / total : 0);
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Disk usage" on:click|stopPropagation>
    <header class="sp-head">
      <Icon name="disk" size={16} />
      <button class="crumb up" disabled={trail.length === 0} on:click={up} title="Up one level">
        <Icon name="up" size={14} />
      </button>
      <span class="sp-path" title={cur}>{baseOf(cur)}</span>
      <span class="sp-total">{formatSize(total)}{loading ? " · scanning…" : ""}</span>
      <button class="sp-close" title="Close" on:click={() => dispatch("close")}>
        <Icon name="close" size={14} />
      </button>
    </header>

    <div class="sp-body">
      <div class="sp-map">
        {#if error}
          <div class="sp-msg">Couldn't scan this folder: {error}</div>
        {:else if !loading && tiles.length === 0}
          <div class="sp-msg">This folder is empty (nothing to show).</div>
        {:else}
          <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" role="img" aria-label="Treemap of folder sizes">
            {#each tiles as t (t.key)}
              {@const c = byKey[t.key]}
              <g
                class="tile"
                class:dir={c?.is_dir}
                role="button"
                tabindex="0"
                on:click={() => (c?.is_dir ? drill(c) : dispatch("navigate", cur))}
                on:keydown={(e) => (e.key === "Enter" && c ? (c.is_dir ? drill(c) : dispatch("navigate", cur)) : null)}
              >
                <title>{c?.name} — {formatSize(c?.size ?? 0)} ({pct(c?.size ?? 0)}%)</title>
                <rect
                  x={t.x}
                  y={t.y}
                  width={t.w}
                  height={t.h}
                  style="fill: color-mix(in srgb, var(--accent, #2f6fed) {Math.round(opacityFor(c) * 100)}%, transparent)"
                />
                {#if labelable(t)}
                  <text x={t.x + 6} y={t.y + 15} class="tl-name">{c?.name}</text>
                  <text x={t.x + 6} y={t.y + 29} class="tl-size">{formatSize(c?.size ?? 0)}</text>
                {/if}
              </g>
            {/each}
          </svg>
        {/if}
      </div>

      <aside class="sp-largest" aria-label="Largest items">
        <div class="sp-largest-head">Largest</div>
        <ul>
          {#each children.slice(0, 14) as c (c.path)}
            <li>
              <button
                class="lg-row"
                title={c.path}
                on:click={() => (c.is_dir ? drill(c) : dispatch("navigate", cur))}
              >
                <Icon name={c.is_dir ? "folder" : "document"} size={14} />
                <span class="lg-name">{c.name}</span>
                <span class="lg-size">{formatSize(c.size)}</span>
              </button>
            </li>
          {/each}
        </ul>
      </aside>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.28);
    display: grid;
    place-items: center;
    z-index: 210;
  }
  .dialog {
    width: 1120px;
    max-width: 96vw;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.28);
    overflow: hidden;
  }
  .sp-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 12px;
    border-bottom: 1px solid var(--border, #3a3a3a);
    font-size: 13px;
    font-weight: 600;
  }
  .crumb.up {
    display: inline-flex;
    padding: 2px 6px;
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 5px;
    background: var(--surface-alt, transparent);
    cursor: pointer;
  }
  .crumb.up:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .sp-path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sp-total {
    flex: 0 0 auto;
    font-size: 12px;
    color: var(--text-dim, #9a9a9a);
    font-variant-numeric: tabular-nums;
  }
  .sp-close {
    flex: 0 0 auto;
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 4px;
  }
  .sp-close:hover {
    background: rgba(128, 128, 128, 0.18);
  }
  .sp-body {
    display: flex;
    min-height: 0;
    flex: 1;
  }
  .sp-map {
    flex: 1;
    min-width: 0;
    display: flex;
  }
  .sp-map svg {
    width: 100%;
    height: 100%;
    display: block;
  }
  .sp-msg {
    margin: auto;
    padding: 24px;
    color: var(--text-dim, #9a9a9a);
    font-size: 13px;
  }
  .tile {
    cursor: pointer;
  }
  .tile rect {
    stroke: var(--surface, #1e1e1e);
    stroke-width: 1.5;
    transition: fill 0.1s;
  }
  .tile:hover rect,
  .tile:focus rect {
    stroke: var(--accent, #2f6fed);
    outline: none;
  }
  .tile.dir rect {
    stroke-dasharray: 0;
  }
  .tl-name {
    fill: var(--text, #eaeaea);
    font-size: 11px;
    font-weight: 600;
    pointer-events: none;
  }
  .tl-size {
    fill: var(--text-dim, #b8b8b8);
    font-size: 10px;
    pointer-events: none;
  }
  .sp-largest {
    flex: 0 0 260px;
    border-left: 1px solid var(--border, #3a3a3a);
    overflow-y: auto;
    padding: 6px;
  }
  .sp-largest-head {
    padding: 4px 8px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    opacity: 0.6;
  }
  .sp-largest ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .lg-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 8px;
    border: 0;
    background: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
    border-radius: 5px;
    font: inherit;
    font-size: 12.5px;
  }
  .lg-row:hover {
    background: rgba(128, 128, 128, 0.14);
  }
  .lg-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lg-size {
    flex: 0 0 auto;
    color: var(--text-dim, #9a9a9a);
    font-variant-numeric: tabular-nums;
    font-size: 11.5px;
  }
</style>
