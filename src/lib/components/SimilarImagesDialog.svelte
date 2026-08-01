<script lang="ts">
  /**
   * Find similar (near-duplicate) images (CPE-1202, epic CPE-997) — the review UI over the
   * `find_similar_images` engine (CPE-1200/1201). Scans the current folder for images that *look*
   * alike (a re-encode / resize / light edit of the same picture, grouped by perceptual dHash) and
   * lets the user reclaim space by removing the redundant copies.
   *
   * This is the perceptual complement of {@link DuplicatesDialog} and is deliberately modelled on it —
   * same streaming-append + generation-token supersede, same keeper-guarded cleanup helpers
   * (`keepsOnePerGroup` / `pruneGroups` from ../duplicates). Because it DELETES USER FILES, the safety
   * rails are non-negotiable and mirror the exact-duplicate dialog's:
   *   1. cleanup uses `commands.deleteToTrash` — recoverable Recycle Bin, NEVER a hard delete;
   *   2. it is guarded by `keepsOnePerGroup`, so at least one image per group always survives;
   *   3. NOTHING is selected by default (no auto-select-all) — the user opts in per image;
   *   4. "Move to Bin" is DISABLED whenever the current selection would remove every copy of any group;
   *   5. a best-effort checkpoint (`commands.checkpointCreate`) is taken before a bulk move, so the
   *      action is reversible beyond the Bin too. A checkpoint failure never blocks the (already
   *      reversible) trash move.
   *
   * Per the STREAMING-liveness convention it streams over an `ipc::Channel`; image clustering is a
   * whole-set operation so the (single) group batch arrives after the walk — but on an EMPTY result the
   * backend sends NO batch, so `loading` is cleared on the awaited result resolution, not solely on the
   * first batch (reviewer note, CPE-1202).
   */
  import { createEventDispatcher } from "svelte";
  import { rawInvoke, createChannel } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import Icon from "./Icon.svelte";
  import ThumbnailImage from "./ThumbnailImage.svelte";
  import { t } from "../i18n";
  import { baseName, parentDir } from "../contentSearch";
  import { keepsOnePerGroup, pruneGroups } from "../duplicates";
  import type { SimGroup, SimResult } from "../bindings.gen";

  export let root = "";

  const dispatch = createEventDispatcher<{ close: void; navigate: string }>();

  /** A rendered group. `SimGroup` carries only `paths`, so we assign a stable `id` on arrival to key
   *  the `{#each}` across a prune (paths mutate; ids don't). */
  interface SimGroupView {
    id: number;
    paths: string[];
  }

  let loading = false;
  let error = "";
  let started = false;
  let groups: SimGroupView[] = [];
  let filesScanned = 0;
  let truncated = false;
  let nextId = 0;

  // Paths the user has marked for removal. A Set, reassigned to trigger reactivity. SAFETY: starts
  // EMPTY — nothing is auto-selected (CPE-1202).
  let selected = new Set<string>();
  let deleting = false;

  // SAFETY guard: cleanable only if something is selected AND every group still keeps a copy.
  $: canClean = selected.size > 0 && keepsOnePerGroup(groups, selected);

  function toggle(path: string) {
    if (selected.has(path)) selected.delete(path);
    else selected.add(path);
    selected = new Set(selected);
  }

  /** Convenience: tick every image except the first of each group — always keeps one, so it can never
   *  arm a would-delete-all selection. Off by default (nothing is pre-selected). */
  function selectExtras() {
    selected = new Set(groups.flatMap((g) => g.paths.slice(1)));
  }

  async function cleanUp() {
    if (!canClean) return;
    const paths = [...selected];
    deleting = true;
    try {
      // Best-effort checkpoint before the bulk move so it's reversible beyond the Bin too. A failure
      // here must never block the (already recoverable) trash move — swallow it.
      try {
        await commands.checkpointCreate(root, "Before removing similar images");
      } catch {
        /* checkpoint is a bonus safety net, not a gate */
      }
      await commands.deleteToTrash(paths); // returns OpResult[] directly (no Result wrapper)
      groups = pruneGroups(groups, selected);
      selected = new Set();
    } catch (e) {
      error = String(e);
    } finally {
      deleting = false;
    }
  }

  let searchGen = 0; // supersede a stale scan when a newer one starts (mirrors DuplicatesDialog)

  async function run() {
    loading = true;
    error = "";
    started = true;
    groups = [];
    filesScanned = 0;
    truncated = false;
    selected = new Set();
    const gen = ++searchGen;
    try {
      const channel = createChannel<SimGroup[]>();
      channel.onmessage = (batch) => {
        if (gen !== searchGen) return; // superseded by a newer scan — drop stale groups
        groups = [...groups, ...batch.map((g) => ({ id: nextId++, paths: g.paths }))];
        loading = false; // first (and only) batch is in — reveal them
      };
      // Streaming opts out of the busy cursor (rawInvoke), matching DuplicatesDialog / the STREAMING
      // convention — the typed `commands.*Stream` client wraps the busy `invoke` and can't accept a
      // transport-agnostic `StreamChannel`, so streaming call sites use `rawInvoke` directly.
      const final = await rawInvoke<SimResult>("find_similar_images_stream", { root, onGroup: channel });
      if (gen === searchGen) {
        filesScanned = final.files_scanned;
        truncated = final.truncated;
      }
    } catch (e) {
      if (gen === searchGen) {
        error = String(e);
        groups = [];
      }
    } finally {
      // CRITICAL (CPE-1202): an EMPTY result streams NO batch, so `onmessage` never fires — clearing
      // `loading` here on the awaited resolution is what stops an empty scan spinning forever.
      if (gen === searchGen) loading = false;
    }
  }

  function goToFile(path: string) {
    // Dispatch the FILE path — the host reveals it (navigates to its folder AND selects it).
    dispatch("navigate", path);
    dispatch("close");
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label={$t("sim.title")} on:click|stopPropagation>
    <header>
      <h2>{$t("sim.title")}</h2>
      <span class="root" title={root}>{baseName(root) || root}</span>
      <button class="x" data-testid="sim-close-btn" title={$t("common.close")} on:click={() => dispatch("close")}>
        <Icon name="close" size={14} />
      </button>
    </header>

    {#if !started}
      <div class="intro">
        <p>{$t("sim.intro")}</p>
        <button class="btn primary" data-testid="sim-scan-btn" on:click={run}>{$t("sim.scan")}</button>
      </div>
    {:else if loading}
      <p class="dim">{$t("sim.scanning")}</p>
    {:else if error}
      <p class="err">{error}</p>
      <button class="mini" data-testid="sim-rescan-btn" on:click={run}>{$t("sim.scan")}</button>
    {:else if groups.length === 0}
      <p class="dim" data-testid="sim-none">{$t("sim.none", { count: filesScanned.toLocaleString() })}</p>
      <button class="mini" data-testid="sim-rescan-btn" on:click={run}>{$t("sim.scan")}</button>
    {:else}
      <div class="summary">
        <span>
          {groups.length === 1
            ? $t("sim.summaryOne", { count: groups.length })
            : $t("sim.summaryMany", { count: groups.length })}
          <span class="dim"> · {$t("sim.scanned", { count: filesScanned.toLocaleString() })}</span>
          {#if truncated}<span class="dim"> {$t("sim.capped")}</span>{/if}
        </span>
        <span class="cleanup">
          <button class="mini" data-testid="sim-rescan-btn" on:click={run}>{$t("sim.scan")}</button>
          <button class="mini" on:click={selectExtras} title={$t("sim.selectExtrasTip")}>{$t("sim.selectExtras")}</button>
          <button class="mini danger" data-testid="sim-move-btn" disabled={!canClean || deleting} on:click={cleanUp}>
            {deleting ? $t("sim.removing") : $t("sim.moveToBin", { count: selected.size })}
          </button>
        </span>
      </div>
      <div class="results">
        {#each groups as g (g.id)}
          <div class="group" data-testid="sim-group">
            <div class="ghead">
              <Icon name="image" size={13} />
              {$t("sim.groupHead", { count: g.paths.length })}
            </div>
            <div class="cards">
              {#each g.paths as p (p)}
                <div class="card" data-testid="sim-image" class:picked={selected.has(p)}>
                  <label class="pick" title={$t("sim.markForBin")}>
                    <input type="checkbox" checked={selected.has(p)} on:change={() => toggle(p)} />
                  </label>
                  <button class="thumb-btn" title={p} on:click={() => goToFile(p)}>
                    <ThumbnailImage path={p} size={104} />
                    <span class="name">{baseName(p)}</span>
                    <span class="loc">{parentDir(p)}</span>
                  </button>
                </div>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog {
    width: 720px; max-width: 94vw; max-height: 82vh; display: flex; flex-direction: column;
    background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 14px 16px 16px;
  }
  header { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  h2 { font-size: 16px; }
  .root { color: var(--text-dim); font-size: 12px; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .x { width: 28px; height: 28px; display: grid; place-items: center; }
  .intro { padding: 8px 0; display: grid; gap: 12px; }
  .intro p { color: var(--text-dim); font-size: 13px; line-height: 1.5; }
  .btn { height: 32px; padding: 0 16px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); justify-self: start; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .summary { font-size: 12px; color: var(--text-dim); margin-bottom: 6px; display: flex; align-items: center; gap: 10px; }
  .cleanup { margin-left: auto; display: flex; gap: 6px; flex: 0 0 auto; }
  .mini { height: 24px; padding: 0 10px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); font-size: 12px; }
  .mini:hover { background: var(--surface); }
  .mini.danger:not(:disabled) { border-color: #c42b1c; color: #c42b1c; }
  .mini:disabled { opacity: 0.5; }
  .results { overflow: auto; }
  .group { margin-bottom: 14px; }
  .ghead { display: flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; padding: 3px 6px; }
  /* Thumbnails reflow: cards wrap onto more rows and the group grows; each card keeps a fixed width. */
  .cards { display: flex; flex-wrap: wrap; gap: 10px; padding: 4px 6px; }
  .card {
    flex: 0 0 auto; width: 116px; display: flex; flex-direction: column; align-items: center; gap: 4px;
    padding: 6px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface-alt); position: relative;
  }
  .card.picked { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent) inset; }
  .pick { position: absolute; top: 6px; left: 6px; display: inline-flex; align-items: center; z-index: 1; background: var(--surface); border-radius: 4px; padding: 1px; }
  .thumb-btn { display: flex; flex-direction: column; align-items: center; gap: 3px; width: 100%; padding: 2px; }
  .thumb-btn:hover .name { text-decoration: underline; }
  .name { font-size: 12px; max-width: 104px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .loc { font-size: 11px; color: var(--text-faint); max-width: 104px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dim { color: var(--text-faint); }
  .err { color: #c42b1c; }
</style>
