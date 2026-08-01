<script lang="ts">
  /**
   * Near-duplicate DOCUMENTS + near-identical FOLDERS review UI (CPE-1204, epic CPE-997 stretch) — the
   * minimal frontend entry point over the `find_similar_documents` / `find_similar_folders` engines
   * (`cpe_server::document_similarity` / `cpe_server::folder_similarity_scan`). One dialog handles both
   * via the `kind` prop, mirroring {@link SimilarImagesDialog}'s shape (intro → scan → grouped results)
   * but deliberately smaller: this is **read-only** (no cleanup/delete action — the ticket's acceptance
   * criteria is wiring the scan, not a removal workflow), so there's no selection state, no keeper guard,
   * and no trash integration. Clicking an item just reveals it, like `DuplicatesDialog`'s file rows.
   *
   * Both backends are modest collect-to-vec commands (not streamed) per the ticket's stated scope — the
   * typed `commands.*` client already routes through the busy-cursor `invoke` wrapper
   * ([[prefer-streaming-liveness]] applies to *large/slow* producers; a folder/document near-dup scan
   * over a reasonable tree is neither).
   */
  import { createEventDispatcher } from "svelte";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import { baseName, parentDir } from "../contentSearch";

  export let root = "";
  /** Which engine to run — `documents` (SimHash text near-dup) or `folders` (Jaccard folder near-dup). */
  export let kind: "documents" | "folders" = "documents";

  const dispatch = createEventDispatcher<{ close: void; navigate: string }>();

  /** A rendered group with a stable `id` for the `{#each}` key (the backend only carries `paths`). */
  interface GroupView {
    id: number;
    paths: string[];
  }

  let loading = false;
  let error = "";
  let started = false;
  let groups: GroupView[] = [];
  let scannedCount = 0;
  let truncated = false;
  let nextId = 0;
  let searchGen = 0; // supersede a stale scan when a newer one starts (mirrors SimilarImagesDialog)

  $: title = kind === "documents" ? $t("nd.titleDocs") : $t("nd.titleFolders");
  $: icon = kind === "documents" ? "document" : "folder";

  async function run() {
    loading = true;
    error = "";
    started = true;
    groups = [];
    scannedCount = 0;
    truncated = false;
    const gen = ++searchGen;
    try {
      if (kind === "documents") {
        const result = await commands.findSimilarDocuments(root);
        if (gen !== searchGen) return;
        if (result.status === "error") {
          error = result.error;
          return;
        }
        groups = result.data.groups.map((g) => ({ id: nextId++, paths: g.paths }));
        scannedCount = result.data.files_scanned;
        truncated = result.data.truncated;
      } else {
        const result = await commands.findSimilarFolders(root);
        if (gen !== searchGen) return;
        if (result.status === "error") {
          error = result.error;
          return;
        }
        groups = result.data.groups.map((g) => ({ id: nextId++, paths: g.paths }));
        scannedCount = result.data.folders_scanned;
        truncated = result.data.truncated;
      }
    } catch (e) {
      if (gen === searchGen) {
        error = String(e);
        groups = [];
      }
    } finally {
      if (gen === searchGen) loading = false;
    }
  }

  function reveal(path: string) {
    // Dispatch the path — the host reveals it (navigates to its folder/self and selects it).
    dispatch("navigate", path);
    dispatch("close");
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label={title} on:click|stopPropagation>
    <header>
      <h2>{title}</h2>
      <span class="root" title={root}>{baseName(root) || root}</span>
      <button class="x" data-testid="nd-close-btn" title={$t("common.close")} on:click={() => dispatch("close")}>
        <Icon name="close" size={14} />
      </button>
    </header>

    {#if !started}
      <div class="intro">
        <p>{$t("nd.intro")}</p>
        <button class="btn primary" data-testid="nd-scan-btn" on:click={run}>{$t("nd.scan")}</button>
      </div>
    {:else if loading}
      <p class="dim">{$t("nd.scanning")}</p>
    {:else if error}
      <p class="err">{error}</p>
      <button class="mini" data-testid="nd-rescan-btn" on:click={run}>{$t("nd.scan")}</button>
    {:else if groups.length === 0}
      <p class="dim" data-testid="nd-none">{$t("nd.none", { count: scannedCount.toLocaleString() })}</p>
      <button class="mini" data-testid="nd-rescan-btn" on:click={run}>{$t("nd.scan")}</button>
    {:else}
      <div class="summary">
        <span>
          {groups.length === 1
            ? $t("nd.summaryOne", { count: groups.length })
            : $t("nd.summaryMany", { count: groups.length })}
          <span class="dim"> · {$t("sim.scanned", { count: scannedCount.toLocaleString() })}</span>
          {#if truncated}<span class="dim"> {$t("sim.capped")}</span>{/if}
        </span>
        <button class="mini" data-testid="nd-rescan-btn" on:click={run}>{$t("nd.scan")}</button>
      </div>
      <div class="results">
        {#each groups as g (g.id)}
          <div class="group" data-testid="nd-group">
            <div class="ghead">
              <Icon name={icon} size={13} />
              {$t("nd.groupHead", { count: g.paths.length })}
            </div>
            <div class="items">
              {#each g.paths as p (p)}
                <button class="item" data-testid="nd-item" title={p} on:click={() => reveal(p)}>
                  <Icon name={icon} size={14} />
                  <span class="name">{baseName(p)}</span>
                  <span class="loc">{parentDir(p)}</span>
                </button>
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
    width: 640px; max-width: 94vw; max-height: 82vh; display: flex; flex-direction: column;
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
  .summary .mini { margin-left: auto; flex: 0 0 auto; }
  .mini { height: 24px; padding: 0 10px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); font-size: 12px; }
  .mini:hover { background: var(--surface); }
  .results { overflow: auto; }
  .group { margin-bottom: 12px; }
  .ghead { display: flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; padding: 3px 6px; }
  /* Rows reflow: the group wraps items onto more rows and grows; each row keeps its text on one line. */
  .items { display: flex; flex-wrap: wrap; gap: 6px; padding: 4px 6px; }
  .item {
    flex: 0 0 auto; max-width: 280px; display: flex; align-items: center; gap: 6px;
    padding: 5px 10px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt);
  }
  .item:hover { background: var(--surface); border-color: var(--border-strong); }
  .name { font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 160px; }
  .loc { font-size: 11px; color: var(--text-faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100px; }
  .dim { color: var(--text-faint); }
  .err { color: #c42b1c; }
</style>
