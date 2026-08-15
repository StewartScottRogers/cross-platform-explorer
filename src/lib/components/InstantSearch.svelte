<script lang="ts">
  /**
   * Instant Search overlay (CPE-1139, epic CPE-703) — the keyboard-first GLOBAL search over the
   * `index_search`/`index_build`/`index_status` engine (CPE-1137). Unlike `FileNameSearchDialog`
   * (Ctrl+P, recursive walk of the *current* folder), this searches the resident cross-volume index —
   * "Everything-style" instant find, independent of where you're browsing. Opened by **Ctrl+K** (see
   * `App.svelte`'s `handleKeydown`; free — not used by anything else).
   *
   * Off-means-off (CPE-1137's design goal): nothing is indexed until the user asks. If `index_status`
   * shows no resident volume, this shows a "Build index" affordance instead of a blank/empty results
   * list — that's the opt-in surface. The index is built for the whole DRIVE that owns the current
   * folder (an instant *cross-folder* index only makes sense over a volume, not one folder), falling
   * back to the home directory's drive when opened from the Home screen with no folder open.
   *
   * Streaming (STREAMING.md): both `index_search` and `index_build` are self-progress operations this
   * component renders its own UI for, so — like `FileNameSearchDialog`/`BatchMediaDialog` — they go
   * through `rawInvoke` + a `Channel`, not the busy-cursor `invoke` (BUSY-CURSOR.md). A newer keystroke
   * supersedes an in-flight search by generation token; stale batches are dropped.
   */
  import { createEventDispatcher, onMount, tick } from "svelte";
  import { commands } from "../bindings.gen";
  import type { IndexHit, BuildStats } from "../bindings.gen";
  import { rawInvoke, createChannel, unwrap } from "../invoke";
  import { t } from "../i18n";
  import { parentDir, highlightSegments } from "../contentSearch";
  import { moveActive, volumeIdForRoot, resolveBuildRoot, extensionOf } from "../instantSearch";
  import { iconFor } from "../filetypes";
  import { displaySafeName, displaySafePath } from "../filename";
  import Icon from "./Icon.svelte";

  /** The current folder (empty when on the Home screen — the component falls back to `homeDir`). */
  export let root = "";

  const dispatch = createEventDispatcher<{ close: void; navigate: string; help: void }>();

  const DEBOUNCE_MS = 150;
  const LIMIT = 200;

  let query = "";
  let active = 0;
  let hits: IndexHit[] = [];
  let loading = false;
  let error = "";
  let searched = false;

  // Index residency (off-means-off): null while the initial `index_status` check is in flight.
  let hasIndex: boolean | null = null;
  let homeDir: string | null = null;

  let building = false;
  let buildStats: BuildStats | null = null;
  let buildError = "";

  // Supersede token (STREAMING.md): a newer search drops batches still arriving from a stale one.
  let searchGen = 0;
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  let listEl: HTMLDivElement | undefined;

  $: buildRoot = resolveBuildRoot(root, homeDir);
  // Keep the highlight in range as results narrow/grow.
  $: if (active >= hits.length) active = Math.max(0, hits.length - 1);

  onMount(async () => {
    try {
      const status = await commands.indexStatus();
      hasIndex = status.length > 0;
    } catch {
      hasIndex = false;
    }
    if (!root) {
      try {
        homeDir = await commands.homeDir().then(unwrap);
      } catch {
        homeDir = null;
      }
    }
  });

  function scheduleSearch() {
    if (debounceTimer) clearTimeout(debounceTimer);
    if (!hasIndex) return; // off-means-off: don't search while no index is resident
    const q = query;
    debounceTimer = setTimeout(() => void runSearch(q), DEBOUNCE_MS);
  }

  async function runSearch(q: string) {
    const trimmed = q.trim();
    active = 0;
    if (!trimmed) {
      hits = [];
      loading = false;
      error = "";
      searched = false;
      searchGen += 1; // supersede any still-in-flight search — its batches are now stale
      return;
    }
    const gen = ++searchGen;
    loading = true;
    error = "";
    searched = true;
    hits = [];
    try {
      const channel = createChannel<IndexHit[]>();
      channel.onmessage = (batch) => {
        if (gen !== searchGen) return; // superseded by a newer query — drop stale hits
        hits = hits.concat(batch);
        loading = false; // first hits are in — reveal them
      };
      await rawInvoke<number>("index_search", { query: trimmed, limit: LIMIT, onHit: channel });
    } catch (e) {
      if (gen === searchGen) {
        error = String(e);
        hits = [];
      }
    } finally {
      if (gen === searchGen) loading = false;
    }
  }

  async function buildIndex() {
    if (!buildRoot || building) return;
    building = true;
    buildError = "";
    buildStats = null;
    const volumeId = volumeIdForRoot(buildRoot);
    try {
      const channel = createChannel<BuildStats>();
      channel.onmessage = (stats) => { buildStats = stats; };
      const final = await rawInvoke<BuildStats>("index_build", { root: buildRoot, volumeId, onProgress: channel });
      buildStats = final;
      hasIndex = true;
      if (query.trim()) scheduleSearch();
    } catch (e) {
      buildError = String(e);
    } finally {
      building = false;
    }
  }

  function choose(i: number) {
    const hit = hits[i];
    if (!hit) return;
    dispatch("navigate", hit.path);
    dispatch("close");
  }

  async function move(delta: number) {
    if (!hits.length) return;
    active = moveActive(active, delta, hits.length);
    await tick();
    listEl?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); dispatch("close"); }
    else if (e.key === "ArrowDown") { e.preventDefault(); void move(1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); void move(-1); }
    else if (e.key === "Enter") { e.preventDefault(); choose(active); }
  }
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="is-overlay" on:click|self={() => dispatch("close")}>
  <div class="is-panel" role="dialog" aria-modal="true" aria-label={$t("search.instantTitle")}>
    <header>
      <h2>{$t("search.instantTitle")}</h2>
      <button class="docs" title={$t("search.docsTitle")} aria-label={$t("search.docsTitle")} on:click={() => dispatch("help")}><Icon name="book" size={15} /></button>
      <button class="x" title={$t("common.close")} aria-label={$t("common.close")} on:click={() => dispatch("close")}><Icon name="close" size={14} /></button>
    </header>

    <!-- svelte-ignore a11y-autofocus -->
    <input
      class="is-input"
      autofocus
      bind:value={query}
      on:input={scheduleSearch}
      on:keydown={onKeydown}
      placeholder={$t("search.instantPlaceholder")}
      spellcheck="false"
      autocomplete="off"
      aria-label={$t("search.instantPlaceholder")}
    />

    <div class="is-body">
      {#if hasIndex === false}
        <div class="is-offer">
          <p class="is-offer-title">{$t("search.instantOffTitle")}</p>
          <p class="dim">{$t("search.instantOffBody")}</p>
          {#if building}
            <p class="dim building">{$t("search.buildingIndex", { count: buildStats?.dirs_scanned ?? 0 })}</p>
          {:else if buildError}
            <p class="err">{buildError}</p>
          {/if}
          {#if !building}
            <button class="btn primary" on:click={buildIndex} disabled={!buildRoot}>{$t("search.buildIndex")}</button>
          {/if}
          {#if !buildRoot}<p class="dim">{$t("search.instantOpenFolderFirst")}</p>{/if}
        </div>
      {:else if loading}
        <p class="dim">{$t("search.searching")}</p>
      {:else if error}
        <p class="err">{error}</p>
      {:else if !searched}
        <p class="dim">{$t("search.instantTypeHint")}</p>
      {:else if hits.length === 0}
        <p class="dim">{$t("search.instantNoMatches")}</p>
      {:else}
        <div class="is-list" bind:this={listEl}>
          {#each hits as h, i (h.path)}
            <!-- svelte-ignore a11y-no-static-element-interactions -->
            <div
              class="is-row"
              class:active={i === active}
              data-i={i}
              role="button"
              tabindex="-1"
              title={displaySafePath(h.path)}
              on:mousemove={() => (active = i)}
              on:click={() => choose(i)}
            >
              <Icon name={iconFor({ is_dir: h.is_dir, extension: extensionOf(h.name), name: h.name })} size={14} />
              <span class="is-name">{#each highlightSegments(h.name, query) as seg}{#if seg.match}<mark class="hl">{displaySafeName(seg.text)}</mark>{:else}{displaySafeName(seg.text)}{/if}{/each}</span>
              <span class="is-dir">{displaySafePath(parentDir(h.path))}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .is-overlay {
    position: fixed; inset: 0; z-index: 200;
    background: rgba(0, 0, 0, 0.3);
    display: flex; justify-content: center; align-items: flex-start;
    padding-top: 12vh;
  }
  .is-panel {
    width: min(680px, 92vw);
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--dialog-border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.35);
    overflow: hidden;
    display: flex; flex-direction: column;
    max-height: 70vh;
    padding: 12px 14px 14px;
  }
  header { display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }
  h2 { font-size: 15px; flex: 1; }
  .x, .docs {
    width: 26px; height: 26px; display: grid; place-items: center;
    color: var(--text-dim); border-radius: var(--radius); font-size: 13px;
  }
  .x:hover, .docs:hover { color: var(--text); background: var(--hover); }
  .is-input {
    font: inherit; font-size: 15px;
    height: 36px; padding: 0 12px;
    border: 1px solid var(--border-strong); border-radius: var(--radius);
    background: var(--surface-alt); color: var(--text);
    outline: none; width: 100%;
  }
  .is-body { margin-top: 10px; overflow: auto; }
  .is-list { display: flex; flex-direction: column; }
  .is-row {
    display: flex; align-items: center; gap: 8px; width: 100%; text-align: left;
    padding: 6px 8px; border-radius: var(--radius); font-size: 13px; cursor: pointer;
  }
  .is-row.active { background: var(--accent); color: #fff; }
  .is-row.active .is-dir { color: rgba(255, 255, 255, 0.75); }
  .is-row.active :global(.hl) { background: transparent; color: #fff; text-decoration: underline; }
  .is-name { flex: 0 1 auto; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .is-dir { flex: 1 1 auto; min-width: 0; color: var(--text-faint); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  :global(mark.hl) { background: var(--accent); color: #fff; border-radius: 2px; padding: 0 1px; }
  .dim { color: var(--text-faint); font-size: 13px; }
  .err { color: var(--danger); font-size: 13px; }
  .is-offer { display: flex; flex-direction: column; gap: 6px; align-items: flex-start; padding: 6px 2px; }
  .is-offer-title { font-weight: 600; font-size: 14px; }
  .is-offer .building { font-variant-numeric: tabular-nums; }
  .btn { height: 32px; padding: 0 16px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); color: var(--text); margin-top: 4px; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:disabled { opacity: 0.5; cursor: default; }
</style>
