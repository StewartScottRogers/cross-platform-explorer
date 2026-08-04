<script lang="ts">
  /**
   * File Health panel, SLICE 1+2 (CPE-1315/CPE-1316, epic CPE-1002): a tabbed dialog shell surfacing the
   * pure detectors built (but never surfaced) for that epic — `safetyReport.ts`'s `buildFileHealth`
   * already unifies all five (`type-mismatch` / `zip-bomb` / `dangling-link` / `orphaned-sidecar` /
   * `empty-folder`). Slice 1 wired the first tab (**dangling / cyclic symlinks**) end to end over the
   * STREAMING `find_dangling_links_stream` command (CPE-1299) — the first frontend consumer of a
   * `_stream` command that isn't already covered by DuplicatesDialog/SimilarImagesDialog's shape. Slice 2
   * adds two more tabs — **type mismatches** (`find_type_mismatches_stream`) and **orphan sidecars**
   * (`find_orphan_sidecars_stream`) — copying the exact same wiring per tab. Future slices add the
   * remaining categories; `TABS` below is the extension point (add an entry + a matching `{#if}` block).
   *
   * Mirrors NearDuplicatesDialog's read-only reveal-dialog skeleton (intro → Scan → loading → error →
   * empty → results, a per-tab generation supersede token, `reveal(path)` → navigate+close) crossed with
   * SimilarImagesDialog's STREAMING shape (`rawInvoke` + `createChannel` from `../invoke`, append
   * batches, flip `loading` off on the first batch). The in-dialog tab strip (`.tabs`/`.tab`/`.tab.active`)
   * follows MetadataStudioDialog's local convention (docs/design/TABS.md's accent-top-bar treatment,
   * scoped to a dialog rather than the main window's `.tabbar`).
   *
   * Cancellation mirrors ExplorerPane's `list_dir_stream`/`cancel_dir_stream` convention (CPE-665): each
   * tab has its OWN generation counter (`searchGen` / `mismatchGen` / `orphanGen`) that doubles as the
   * frontend-supplied `streamId` for THAT scan's `_stream` command, paired with THAT scan's own
   * `cancel_*_stream` command — the three scans never share a counter or a cancel call, so rescanning one
   * tab can never cancel (or be superseded by) another tab's in-flight walk.
   */
  import { createEventDispatcher } from "svelte";
  import { rawInvoke, createChannel } from "../invoke";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import { baseName, parentDir } from "../contentSearch";
  import type {
    DanglingLink,
    DanglingReport,
    DanglingReason,
    MismatchHit,
    MismatchReport,
    OrphanSidecarResult,
  } from "../bindings.gen";

  export let root = "";
  /** Which tab to open on (CPE-1316) — lets a Tools-menu / command-palette entry that targets one
   *  specific detector (e.g. "Find type mismatches…") open the panel scoped straight to that tab, instead
   *  of always landing on the first one. Defaults to slice 1's dangling-links tab. */
  export let initialTab: TabId = "dangling";

  const dispatch = createEventDispatcher<{ close: void; navigate: string }>();

  /** Tab shell extension point — add an entry here (+ a matching `{#if activeTab === …}` block) to wire
   *  another File-Health category. `dangling` (slice 1), `mismatch` + `orphan` (slice 2) are wired. */
  type TabId = "dangling" | "mismatch" | "orphan";
  const TABS: { id: TabId; labelKey: string; icon: string }[] = [
    { id: "dangling", labelKey: "fh.tabDangling", icon: "link-broken" },
    { id: "mismatch", labelKey: "fh.tabMismatch", icon: "ban" },
    { id: "orphan", labelKey: "fh.tabOrphan", icon: "unknown" },
  ];
  let activeTab: TabId = initialTab;

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

  /** A rendered type-mismatch hit with a stable `id` for the `{#each}` key (batches only carry the
   *  `MismatchHit` fields). */
  interface MismatchRow {
    id: number;
    path: string;
    claimedExt: string;
    detectedLabel: string;
    detectedExt: string;
  }

  let mismatchLoading = false;
  let mismatchError = "";
  let mismatchStarted = false;
  let mismatchHits: MismatchRow[] = [];
  let mismatchScanned = 0;
  let mismatchTruncated = false;
  let mismatchNextId = 0;
  // This tab's OWN generation counter, also doubling as its OWN frontend-supplied streamId — entirely
  // independent of `searchGen`/`orphanGen` so a mismatch rescan can never cancel or be superseded by
  // either of the other two scans.
  let mismatchGen = 0;

  async function runMismatch() {
    // A rescan supersedes whatever the PRIOR mismatch scan's walk is still draining — cancel that
    // generation's stream (never the dangling/orphan scans' streamIds, which live in their own counters).
    if (mismatchGen > 0) {
      void rawInvoke("cancel_type_mismatches_stream", { streamId: mismatchGen }).catch(() => {});
    }
    mismatchLoading = true;
    mismatchError = "";
    mismatchStarted = true;
    mismatchHits = [];
    mismatchScanned = 0;
    mismatchTruncated = false;
    const gen = ++mismatchGen;
    try {
      const channel = createChannel<MismatchHit[]>();
      channel.onmessage = (batch) => {
        if (gen !== mismatchGen) return; // superseded by a newer mismatch scan — drop stale rows
        mismatchHits = [
          ...mismatchHits,
          ...batch.map((h) => ({
            id: mismatchNextId++,
            path: h.path,
            claimedExt: h.claimed_ext,
            detectedLabel: h.detected_label,
            detectedExt: h.detected_ext,
          })),
        ];
        mismatchLoading = false; // first batch is in — reveal it
      };
      const final = await rawInvoke<MismatchReport>("find_type_mismatches_stream", {
        root,
        excludes: [],
        streamId: gen,
        onHit: channel,
      });
      if (gen === mismatchGen) {
        mismatchScanned = final.scanned;
        mismatchTruncated = final.truncated;
      }
    } catch (e) {
      if (gen === mismatchGen) {
        mismatchError = String(e);
        mismatchHits = [];
      }
    } finally {
      // An EMPTY result streams NO batch — clearing `loading` here on the awaited resolution is what
      // stops an empty scan spinning forever (mirrors the dangling tab).
      if (gen === mismatchGen) mismatchLoading = false;
    }
  }

  /** A rendered orphan sidecar with a stable `id` for the `{#each}` key (batches carry plain path
   *  strings — there's no per-row metadata beyond the path itself). */
  interface OrphanRow {
    id: number;
    path: string;
  }

  let orphanLoading = false;
  let orphanError = "";
  let orphanStarted = false;
  let orphans: OrphanRow[] = [];
  let orphanScanned = 0;
  let orphanTruncated = false;
  let orphanNextId = 0;
  // This tab's OWN generation counter / streamId — independent of `searchGen`/`mismatchGen` (same
  // reasoning as `mismatchGen` above).
  let orphanGen = 0;

  async function runOrphan() {
    // A rescan supersedes whatever the PRIOR orphan scan's walk is still draining — cancel that
    // generation's stream only (never the dangling/mismatch scans' streamIds).
    if (orphanGen > 0) {
      void rawInvoke("cancel_orphan_sidecars_stream", { streamId: orphanGen }).catch(() => {});
    }
    orphanLoading = true;
    orphanError = "";
    orphanStarted = true;
    orphans = [];
    orphanScanned = 0;
    orphanTruncated = false;
    const gen = ++orphanGen;
    try {
      const channel = createChannel<string[]>();
      channel.onmessage = (batch) => {
        if (gen !== orphanGen) return; // superseded by a newer orphan scan — drop stale rows
        orphans = [...orphans, ...batch.map((p) => ({ id: orphanNextId++, path: p }))];
        orphanLoading = false; // first batch is in — reveal it
      };
      // recursive:true (CPE-1316 spec) — sidecars are paired only against primaries in the same
      // directory, so a recursive walk is needed to cover subfolders too.
      const final = await rawInvoke<OrphanSidecarResult>("find_orphan_sidecars_stream", {
        root,
        recursive: true,
        excludes: [],
        streamId: gen,
        onOrphan: channel,
      });
      if (gen === orphanGen) {
        orphanScanned = final.scanned;
        orphanTruncated = final.truncated;
      }
    } catch (e) {
      if (gen === orphanGen) {
        orphanError = String(e);
        orphans = [];
      }
    } finally {
      if (gen === orphanGen) orphanLoading = false;
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
    {:else if activeTab === "mismatch"}
      {#if !mismatchStarted}
        <div class="intro">
          <p>{$t("fh.introMismatch")}</p>
          <button class="btn primary" data-testid="fh-scan-btn" on:click={runMismatch}>{$t("fh.scan")}</button>
        </div>
      {:else if mismatchLoading}
        <p class="dim">{$t("fh.scanning")}</p>
      {:else if mismatchError}
        <p class="err">{mismatchError}</p>
        <button class="mini" data-testid="fh-rescan-btn" on:click={runMismatch}>{$t("fh.scan")}</button>
      {:else if mismatchHits.length === 0}
        <p class="dim" data-testid="fh-none">{$t("fh.noneMismatch", { count: mismatchScanned.toLocaleString() })}</p>
        <button class="mini" data-testid="fh-rescan-btn" on:click={runMismatch}>{$t("fh.scan")}</button>
      {:else}
        <div class="summary">
          <span>
            {mismatchHits.length === 1
              ? $t("fh.summaryOneMismatch", { count: mismatchHits.length })
              : $t("fh.summaryManyMismatch", { count: mismatchHits.length })}
            <span class="dim"> · {$t("fh.scanned", { count: mismatchScanned.toLocaleString() })}</span>
            {#if mismatchTruncated}<span class="dim"> {$t("fh.capped")}</span>{/if}
          </span>
          <button class="mini" data-testid="fh-rescan-btn" on:click={runMismatch}>{$t("fh.scan")}</button>
        </div>
        <div class="results">
          <div class="rows">
            {#each mismatchHits as h (h.id)}
              <button class="row" data-testid="fh-row" title={h.path} on:click={() => reveal(h.path)}>
                <Icon name="ban" size={14} />
                <span class="name">{baseName(h.path)}</span>
                <span class="loc">{parentDir(h.path)}</span>
                <span class="reason" data-testid="fh-reason">
                  {$t("fh.mismatchBadge", { claimed: h.claimedExt, detected: h.detectedLabel })}
                </span>
              </button>
            {/each}
          </div>
        </div>
      {/if}
    {:else if activeTab === "orphan"}
      {#if !orphanStarted}
        <div class="intro">
          <p>{$t("fh.introOrphan")}</p>
          <button class="btn primary" data-testid="fh-scan-btn" on:click={runOrphan}>{$t("fh.scan")}</button>
        </div>
      {:else if orphanLoading}
        <p class="dim">{$t("fh.scanning")}</p>
      {:else if orphanError}
        <p class="err">{orphanError}</p>
        <button class="mini" data-testid="fh-rescan-btn" on:click={runOrphan}>{$t("fh.scan")}</button>
      {:else if orphans.length === 0}
        <p class="dim" data-testid="fh-none">{$t("fh.noneOrphan", { count: orphanScanned.toLocaleString() })}</p>
        <button class="mini" data-testid="fh-rescan-btn" on:click={runOrphan}>{$t("fh.scan")}</button>
      {:else}
        <div class="summary">
          <span>
            {orphans.length === 1
              ? $t("fh.summaryOneOrphan", { count: orphans.length })
              : $t("fh.summaryManyOrphan", { count: orphans.length })}
            <span class="dim"> · {$t("fh.scanned", { count: orphanScanned.toLocaleString() })}</span>
            {#if orphanTruncated}<span class="dim"> {$t("fh.capped")}</span>{/if}
          </span>
          <button class="mini" data-testid="fh-rescan-btn" on:click={runOrphan}>{$t("fh.scan")}</button>
        </div>
        <div class="results">
          <div class="rows">
            {#each orphans as o (o.id)}
              <button class="row" data-testid="fh-row" title={o.path} on:click={() => reveal(o.path)}>
                <Icon name="unknown" size={14} />
                <span class="name">{baseName(o.path)}</span>
                <span class="loc">{parentDir(o.path)}</span>
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
