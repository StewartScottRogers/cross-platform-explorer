<script lang="ts">
  /**
   * Spotlight overlay (CPE-1216, epic CPE-704) — the global quick-launch overlay: type to fuzzy-search
   * actions, folders, files, and recent locations at once, sectioned and ranked by the
   * `spotlight_search`/`spotlight_frecent` backend cores (CPE-1214, built on CPE-937/948/952). Modeled
   * on `CommandPalette.svelte` (same overlay chrome, ↑/↓/Enter/Esc keyboard model, visible border,
   * theme vars) but sectioned like `InstantSearch.svelte`, with matched-character highlighting from
   * `SpotResult.positions` (see `highlightByPositions`, spotlightSources.ts).
   *
   * Opened two ways (CPE-1215 owns the OS hotkey + backend event; this component is transport-agnostic
   * about *why* it's open): the host toggles a boolean prop-driven `{#if}` exactly like every other
   * overlay in App.svelte. See App.svelte for the `spotlight:open` Tauri-event listener and the in-app
   * Command Palette trigger ("Spotlight (search everywhere)…") that opens it without the OS hotkey —
   * the latter is what makes this headlessly testable (gui-smoke/specs/spotlight.smoke.ts).
   *
   * Empty query shows the frecency default view (`spotlightFrecency.defaultView` — most-frecent first);
   * a typed query re-ranks live across every source via `spotlight_search`. File/folder-name hits are
   * fetched by a streaming recursive walk of `root` ([[prefer-streaming-liveness]] — see
   * `spotlightSources.streamFileHits`), so the action/folder/recent sections render immediately while
   * the (potentially slow) file walk continues to feed in more results.
   */
  import { createEventDispatcher, onMount, tick } from "svelte";
  import { commands } from "../bindings.gen";
  import type { SpotSection, ResultKind, Place, NameMatch, Visit } from "../bindings.gen";
  import type { Favorite } from "../types";
  import { isEnabled, type Command } from "../commandPalette";
  import { createHistory, type History } from "../history";
  import { buildSources, streamFileHits, highlightByPositions } from "../spotlightSources";
  import { recordVisit, defaultView } from "../spotlightFrecency";
  import * as settings from "../settings";
  import { t } from "../i18n";

  /** The current folder to scope the file/folder-name walk to (empty ⇒ no file source, e.g. on Home). */
  export let root = "";
  /** The full palette command set (App.svelte's `paletteCommands`) — doubles as the "action" source and
   *  the lookup used to actually run an activated action. */
  export let paletteCommands: Command[] = [];
  export let places: Place[] = [];
  export let favorites: Favorite[] = [];
  export let history: History = createHistory();

  const dispatch = createEventDispatcher<{ close: void; activate: { path: string; kind: ResultKind } }>();

  const DEBOUNCE_MS = 150;
  const PER_KIND_CAP = 8;

  const GROUP_LABEL: Record<ResultKind, string> = {
    action: "spotlight.groupAction",
    folder: "spotlight.groupFolder",
    file: "spotlight.groupFile",
    recent: "spotlight.groupRecent",
  };

  let query = "";
  let sections: SpotSection[] = [];
  let active = 0; // flat index across every row in every section, in render order
  let loading = false;
  let visits: Visit[] = [];
  let fileHits: NameMatch[] = [];
  let listEl: HTMLDivElement | undefined;

  // Supersede token (STREAMING.md): a newer query drops sections/batches still arriving from a stale one.
  let searchGen = 0;
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  // Sections re-shaped for rendering: each result carries its flat row index `i` (assigned in section →
  // in-section order, matching `rows` below) so the template doesn't need to re-derive it per row.
  $: renderSections = (() => {
    let i = 0;
    return sections.map((s) => ({
      kind: s.kind,
      rows: s.results.map((r) => ({ text: r.text, kind: s.kind, positions: r.positions, i: i++ })),
    }));
  })();
  // Flat row list for keyboard nav, carrying its section's kind along with each result.
  $: rows = renderSections.flatMap((s) => s.rows);
  $: if (active >= rows.length) active = Math.max(0, rows.length - 1);
  $: actionByLabel = new Map(paletteCommands.filter(isEnabled).map((c) => [c.label, c] as const));

  onMount(() => {
    visits = settings.loadSpotlightFrecency();
    void runDefault();
  });

  async function runDefault() {
    const gen = ++searchGen;
    loading = true;
    try {
      const secs = await defaultView(visits, Math.floor(Date.now() / 1000));
      if (gen === searchGen) sections = secs;
    } finally {
      if (gen === searchGen) loading = false;
    }
  }

  async function runAggregate(q: string, gen: number) {
    const sources = buildSources(paletteCommands, places, favorites, history, fileHits);
    try {
      const secs = await commands.spotlightSearch(q, sources, PER_KIND_CAP);
      if (gen === searchGen) sections = secs;
    } finally {
      if (gen === searchGen) loading = false;
    }
  }

  async function runSearch(q: string) {
    const trimmed = q.trim();
    if (!trimmed) return;
    const gen = ++searchGen;
    loading = true;
    fileHits = [];
    // First slice: action/folder/recent results render immediately — don't block on the (possibly
    // slow) recursive file walk below.
    await runAggregate(trimmed, gen);
    // File hits stream in progressively; each batch re-aggregates so the file section (and the overall
    // ranking) updates live rather than only appearing once the whole tree walk finishes.
    await streamFileHits(root, trimmed, (hits) => {
      if (gen !== searchGen) return; // superseded by a newer query — drop stale hits
      fileHits = hits;
      void runAggregate(trimmed, gen);
    });
  }

  function scheduleSearch() {
    if (debounceTimer) clearTimeout(debounceTimer);
    active = 0;
    if (!query.trim()) {
      fileHits = [];
      searchGen += 1; // supersede any still-in-flight search — its batches are now stale
      void runDefault();
      return;
    }
    const q = query;
    debounceTimer = setTimeout(() => void runSearch(q), DEBOUNCE_MS);
  }

  function activate(i: number) {
    const row = rows[i];
    if (!row) return;
    if (row.kind === "action") {
      const cmd = actionByLabel.get(row.text);
      dispatch("close");
      cmd?.run();
      return;
    }
    visits = recordVisit(visits, row.text);
    settings.saveSpotlightFrecency(visits);
    dispatch("activate", { path: row.text, kind: row.kind });
    dispatch("close");
  }

  async function move(delta: number) {
    if (!rows.length) return;
    active = (active + delta + rows.length) % rows.length;
    await tick();
    listEl?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); dispatch("close"); }
    else if (e.key === "ArrowDown") { e.preventDefault(); void move(1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); void move(-1); }
    else if (e.key === "Enter") { e.preventDefault(); activate(active); }
  }
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="sp-overlay" on:click|self={() => dispatch("close")}>
  <div class="sp-panel" role="dialog" aria-label={$t("spotlight.title")}>
    <!-- svelte-ignore a11y-autofocus -->
    <input
      class="sp-input"
      autofocus
      bind:value={query}
      on:input={scheduleSearch}
      on:keydown={onKeydown}
      placeholder={$t("spotlight.placeholder")}
      spellcheck="false"
      autocomplete="off"
      aria-label={$t("spotlight.ariaSearch")}
    />
    <div class="sp-body" bind:this={listEl}>
      {#if rows.length === 0}
        <div class="sp-empty">{query.trim() ? $t("spotlight.noMatches") : $t("spotlight.typeHint")}</div>
      {:else}
        {#each renderSections as section (section.kind)}
          <div class="sp-section">
            <div class="sp-section-label">{$t(GROUP_LABEL[section.kind])}</div>
            {#each section.rows as row (row.text)}
              <!-- svelte-ignore a11y-no-static-element-interactions -->
              <div
                class="sp-row"
                class:active={row.i === active}
                data-i={row.i}
                data-kind={section.kind}
                role="button"
                tabindex="-1"
                title={row.text}
                on:mousemove={() => (active = row.i)}
                on:click={() => activate(row.i)}
              >
                <span class="sp-text">
                  {#each highlightByPositions(row.text, row.positions) as seg}{#if seg.match}<mark class="sp-hl">{seg.text}</mark>{:else}{seg.text}{/if}{/each}
                </span>
              </div>
            {/each}
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .sp-overlay {
    position: fixed; inset: 0; z-index: 200;
    background: rgba(0, 0, 0, 0.3);
    display: flex; justify-content: center; align-items: flex-start;
    padding-top: 12vh;
  }
  .sp-panel {
    width: min(680px, 92vw);
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.35);
    overflow: hidden;
    display: flex; flex-direction: column;
    max-height: 70vh;
  }
  .sp-input {
    font: inherit; font-size: 15px;
    padding: 12px 14px;
    border: 0; border-bottom: 1px solid var(--border);
    background: transparent; color: var(--text);
    outline: none; width: 100%;
  }
  .sp-body { overflow-y: auto; padding: 6px; }
  .sp-empty { padding: 16px 14px; color: var(--text-dim); font-size: 13px; }
  .sp-section + .sp-section { margin-top: 4px; }
  .sp-section-label {
    padding: 6px 10px 2px;
    font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em;
    color: var(--text-faint);
  }
  .sp-row {
    display: flex; align-items: center; gap: 10px;
    padding: 8px 10px; border-radius: 6px; cursor: pointer;
    font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .sp-row.active { background: var(--accent); color: #fff; }
  /* Matched run pops on the accent fill via a translucent white tint (same accepted on-accent-fill
     pattern as the row's own #fff text) plus bold weight, rather than a hard-to-scan thin underline. */
  .sp-row.active :global(.sp-hl) {
    background: rgba(255, 255, 255, 0.3);
    color: #fff;
    font-weight: 700;
    text-decoration: none;
  }
  .sp-text { flex: 1 1 auto; overflow: hidden; text-overflow: ellipsis; }
  :global(mark.sp-hl) { background: var(--accent); color: #fff; border-radius: 2px; padding: 0 1px; font-weight: 700; }
</style>
