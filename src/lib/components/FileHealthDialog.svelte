<script lang="ts">
  /**
   * File Health panel, SLICE 1 (CPE-1315, epic CPE-1002): a tabbed dialog shell surfacing the pure
   * detectors built (but never surfaced) for that epic — `safetyReport.ts`'s `buildFileHealth` already
   * unifies all five (`type-mismatch` / `zip-bomb` / `dangling-link` / `orphaned-sidecar` / `empty-folder`),
   * but this slice wires only ONE tab end to end: **dangling / cyclic symlinks**, over the STREAMING
   * `find_dangling_links_stream` command (CPE-1299) — the first frontend consumer of a `_stream` command
   * that isn't already covered by DuplicatesDialog/SimilarImagesDialog's shape. Future slices add a tab
   * per remaining category; `TABS` below is the extension point (add an entry + a matching `{#if}` block).
   *
   * Mirrors NearDuplicatesDialog's read-only reveal-dialog skeleton (intro → Scan → loading → error →
   * empty → results, `searchGen` supersede token, `reveal(path)` → navigate+close) crossed with
   * SimilarImagesDialog's STREAMING shape (`rawInvoke` + `createChannel` from `../invoke`, append
   * batches, flip `loading` off on the first batch). The in-dialog tab strip (`.tabs`/`.tab`/`.tab.active`)
   * follows MetadataStudioDialog's local convention (docs/design/TABS.md's accent-top-bar treatment,
   * scoped to a dialog rather than the main window's `.tabbar`).
   *
   * Cancellation mirrors ExplorerPane's `list_dir_stream`/`cancel_dir_stream` convention (CPE-665): the
   * frontend-supplied `streamId` is just `searchGen`, so a rescan cancels exactly the PRIOR generation's
   * still-in-flight walk before starting the next one.
   */
  import { createEventDispatcher } from "svelte";
  import { rawInvoke, createChannel } from "../invoke";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import { baseName, parentDir } from "../contentSearch";
  import type { DanglingLink, DanglingReport, DanglingReason } from "../bindings.gen";

  export let root = "";

  const dispatch = createEventDispatcher<{ close: void; navigate: string }>();

  /** Tab shell extension point — add an entry here (+ a matching `{#if activeTab === …}` block) to wire
   *  another File-Health category. Only `dangling` is wired this slice. */
  type TabId = "dangling";
  const TABS: { id: TabId; labelKey: string; icon: string }[] = [
    { id: "dangling", labelKey: "fh.tabDangling", icon: "link-broken" },
  ];
  let activeTab: TabId = "dangling";

  /** A rendered dangling/cyclic link with a stable `id` for the `{#each}` key (batches only carry
   *  `path`/`reason`). */
  interface LinkRow {
    id: number;
    path: string;
    reason: DanglingReason;
  }

  let loading = false;
  let error = "";
  let started = false;
  let links: LinkRow[] = [];
  let scanned = 0;
  let truncated = false;
  let nextId = 0;
  let searchGen = 0; // also doubles as the frontend-supplied streamId (mirrors ExplorerPane's list_dir_stream)

  function reasonLabel(reason: DanglingReason): string {
    return reason === "Cyclic" ? $t("fh.reasonCyclic") : $t("fh.reasonMissing");
  }

  async function run() {
    // A rescan supersedes whatever the previous scan's walk is still draining — cancel the PRIOR
    // generation's stream before starting the next one (CPE-1299's registered cancel flag).
    if (searchGen > 0) {
      void rawInvoke("cancel_dangling_links_stream", { streamId: searchGen }).catch(() => {});
    }
    loading = true;
    error = "";
    started = true;
    links = [];
    scanned = 0;
    truncated = false;
    const gen = ++searchGen;
    try {
      const channel = createChannel<DanglingLink[]>();
      channel.onmessage = (batch) => {
        if (gen !== searchGen) return; // superseded by a newer scan — drop stale rows
        links = [...links, ...batch.map((l) => ({ id: nextId++, path: l.path, reason: l.reason }))];
        loading = false; // first batch is in — reveal it
      };
      // Streaming opts out of the busy cursor (rawInvoke, not the typed `commands.*` client, which
      // routes through the busy `invoke` and can't accept a transport-agnostic StreamChannel) — matches
      // SimilarImagesDialog / the STREAMING convention.
      const final = await rawInvoke<DanglingReport>("find_dangling_links_stream", {
        root,
        excludes: [],
        streamId: gen,
        onLink: channel,
      });
      if (gen === searchGen) {
        scanned = final.scanned;
        truncated = final.truncated;
      }
    } catch (e) {
      if (gen === searchGen) {
        error = String(e);
        links = [];
      }
    } finally {
      // An EMPTY result streams NO batch (mirrors SimilarImagesDialog/CPE-1202) — clearing `loading`
      // here on the awaited resolution is what stops an empty scan spinning forever.
      if (gen === searchGen) loading = false;
    }
  }

  function reveal(path: string) {
    dispatch("navigate", path);
    dispatch("close");
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label={$t("fh.title")} on:click|stopPropagation>
    <header>
      <h2>{$t("fh.title")}</h2>
      <span class="root" title={root}>{baseName(root) || root}</span>
      <button class="x" data-testid="fh-close-btn" title={$t("common.close")} on:click={() => dispatch("close")}>
        <Icon name="close" size={14} />
      </button>
    </header>

    <div class="tabs" role="tablist">
      {#each TABS as tab (tab.id)}
        <button
          class="tab"
          class:active={tab.id === activeTab}
          role="tab"
          aria-selected={tab.id === activeTab}
          data-testid={`fh-tab-${tab.id}`}
          on:click={() => (activeTab = tab.id)}
        >
          <Icon name={tab.icon} size={13} />
          {$t(tab.labelKey)}
        </button>
      {/each}
    </div>

    {#if activeTab === "dangling"}
      {#if !started}
        <div class="intro">
          <p>{$t("fh.intro")}</p>
          <button class="btn primary" data-testid="fh-scan-btn" on:click={run}>{$t("fh.scan")}</button>
        </div>
      {:else if loading}
        <p class="dim">{$t("fh.scanning")}</p>
      {:else if error}
        <p class="err">{error}</p>
        <button class="mini" data-testid="fh-rescan-btn" on:click={run}>{$t("fh.scan")}</button>
      {:else if links.length === 0}
        <p class="dim" data-testid="fh-none">{$t("fh.none", { count: scanned.toLocaleString() })}</p>
        <button class="mini" data-testid="fh-rescan-btn" on:click={run}>{$t("fh.scan")}</button>
      {:else}
        <div class="summary">
          <span>
            {links.length === 1
              ? $t("fh.summaryOne", { count: links.length })
              : $t("fh.summaryMany", { count: links.length })}
            <span class="dim"> · {$t("fh.scanned", { count: scanned.toLocaleString() })}</span>
            {#if truncated}<span class="dim"> {$t("fh.capped")}</span>{/if}
          </span>
          <button class="mini" data-testid="fh-rescan-btn" on:click={run}>{$t("fh.scan")}</button>
        </div>
        <div class="results">
          <div class="rows">
            {#each links as l (l.id)}
              <button class="row" data-testid="fh-row" title={l.path} on:click={() => reveal(l.path)}>
                <Icon name="link-broken" size={14} />
                <span class="name">{baseName(l.path)}</span>
                <span class="loc">{parentDir(l.path)}</span>
                <span class="reason" data-testid="fh-reason">{reasonLabel(l.reason)}</span>
              </button>
            {/each}
          </div>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog {
    width: 640px; max-width: 94vw; max-height: 82vh; display: flex; flex-direction: column;
    background: var(--surface); border: 1px solid var(--dialog-border); border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 14px 16px 16px;
  }
  header { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  h2 { font-size: 16px; }
  .root { color: var(--text-dim); font-size: 12px; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .x { width: 28px; height: 28px; display: grid; place-items: center; }
  /* In-dialog tab strip (docs/design/TABS.md's accent-top-bar treatment, scoped locally like
     MetadataStudioDialog — the main window's global `.tabbar`/`.tab` is sized for file tabs, not this). */
  .tabs { display: flex; flex-wrap: wrap; gap: 4px; margin: 0 0 10px; border-bottom: 1px solid var(--border); }
  .tab {
    flex: 0 0 auto; display: flex; align-items: center; gap: 6px; white-space: nowrap;
    padding: 7px 14px; font-size: 12.5px; color: var(--text-dim); background: var(--surface-alt);
    border: 1px solid var(--border); border-bottom: none; border-top: 2px solid transparent; border-radius: 7px 7px 0 0;
  }
  .tab.active { color: var(--text); background: var(--surface); border-top: 2px solid var(--accent); font-weight: 600; }
  .intro { padding: 8px 0; display: grid; gap: 12px; }
  .intro p { color: var(--text-dim); font-size: 13px; line-height: 1.5; }
  .btn { height: 32px; padding: 0 16px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); justify-self: start; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .summary { font-size: 12px; color: var(--text-dim); margin-bottom: 6px; display: flex; align-items: center; gap: 10px; }
  .summary .mini { margin-left: auto; flex: 0 0 auto; }
  .mini { height: 24px; padding: 0 10px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); font-size: 12px; }
  .mini:hover { background: var(--surface); }
  .results { overflow: auto; }
  /* Rows reflow: the container wraps pills onto more rows and grows; each pill keeps its text on one
     line and doesn't shrink — including the nested reason badge (CLAUDE.md's tick-tacks rule). */
  .rows { display: flex; flex-wrap: wrap; gap: 6px; padding: 4px 6px; }
  .row {
    flex: 0 0 auto; max-width: 360px; display: flex; align-items: center; gap: 6px; white-space: nowrap;
    padding: 5px 10px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt);
  }
  .row:hover { background: var(--surface); border-color: var(--border-strong); }
  .name { font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 160px; }
  .loc { font-size: 11px; color: var(--text-faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 120px; }
  .reason {
    flex: 0 0 auto; white-space: nowrap; font-size: 10px; padding: 2px 7px; border-radius: 999px;
    background: var(--surface); border: 1px solid var(--border); color: var(--text-dim);
  }
  .dim { color: var(--text-faint); }
  .err { color: var(--danger); }
</style>
