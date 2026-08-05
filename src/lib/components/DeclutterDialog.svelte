<script lang="ts">
  /**
   * Declutter — junk-review dialog (CPE-1329, epic CPE-979 "AI auto-organize & declutter"). The
   * `organize_clutter` command + `find_clutter` engine (CPE-994) were built and cargo-tested but never
   * had a frontend caller — the `ClutterReason` model's `label()` doc-comment says "for the declutter
   * UI" that was never shipped. This is that UI: a rules-based, model-free surface (the AI classifier is
   * a SEPARATE, gated concern) that lists junk findings for a folder and lets the user safely send
   * selected junk to the Recycle Bin. Epic CPE-979's DoD requires junk suggestions be *surfaced, never
   * auto-actioned*, so nothing here runs until the user explicitly selects items and confirms.
   *
   * Modelled on {@link NearDuplicatesDialog} but SIMPLER — a flat junk list, no per-group keeper guard
   * (there are no groups; each finding is independent junk, so nothing stops a user from binning every
   * flagged item). It reuses that dialog's exact safety rails:
   *   1. cleanup uses `commands.deleteToTrash` — recoverable Recycle Bin, NEVER a hard delete;
   *   2. NOTHING is selected by default (no auto-select-all) — the user opts in per item;
   *   3. "Move selected to Bin" is DISABLED whenever nothing is selected;
   *   4. a best-effort checkpoint (`commands.checkpointCreate`) is taken before the bulk move, wrapped in
   *      `unwrap` (CPE-1328's lesson: a Rust `Err(String)` rejects the raw promise without the generated
   *      binding rethrowing it, so an unwrapped `{status:"error"}` envelope would otherwise vanish
   *      silently) — but the checkpoint failure is still NON-BLOCKING: it's logged and the trash move
   *      proceeds regardless, exactly like NearDuplicatesDialog's own checkpoint call.
   *
   * `organize_clutter` is a modest collect-to-vec command (like `find_similar_documents`/`_folders`), not
   * a `_stream` command, so this opts INTO the busy cursor via the plain-awaited typed `commands.*`
   * client rather than `rawInvoke` + a Channel ([[prefer-streaming-liveness]] targets large/slow
   * producers; a single folder's clutter scan isn't one).
   *
   * `ClutterFinding.name` is a bare filename (the backend's `OrganizeEntry`/`ClutterFinding` shapes never
   * carry a full path — see `crates/server/src/organize.rs`), so this dialog joins it with `root` itself
   * (mirroring `savedSearch.ts`'s private `joinScanPath` / `FileHealthDialog`'s `mismatchFixTarget`
   * join — preserve whichever separator `root` already uses rather than hardcoding one).
   */
  import { createEventDispatcher } from "svelte";
  import { unwrap } from "../invoke";
  import { commands, type ClutterReason } from "../bindings.gen";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import { baseName } from "../contentSearch";

  export let root = "";

  const dispatch = createEventDispatcher<{ close: void; navigate: string }>();

  /** A rendered finding with a stable `id` for the `{#each}` key, plus its full path (the backend only
   *  carries the bare `name`). */
  interface FindingRow {
    id: number;
    name: string;
    path: string;
    reason: ClutterReason;
  }

  /** Join `root` with a child `name`, matching whichever separator `root` already uses — mirrors
   *  `savedSearch.ts`'s private `joinScanPath` (not exported there, so re-implemented rather than
   *  reaching into an internal helper). */
  function joinPath(dir: string, name: string): string {
    const sep = dir.includes("\\") ? "\\" : "/";
    return dir.endsWith(sep) ? dir + name : dir + sep + name;
  }

  // Display order, most-definitive reason first (mirrors the backend's own `clutter_reason` check
  // order in `crates/server/src/organize.rs`).
  const REASON_ORDER: ClutterReason[] = ["zero_byte", "installer", "temp_or_partial", "backup"];

  function reasonLabel(reason: ClutterReason): string {
    switch (reason) {
      case "zero_byte":
        return $t("dc.reasonZeroByte");
      case "installer":
        return $t("dc.reasonInstaller");
      case "temp_or_partial":
        return $t("dc.reasonTempOrPartial");
      case "backup":
        return $t("dc.reasonBackup");
      default:
        return reason;
    }
  }

  let loading = false;
  let error = "";
  let started = false;
  let findings: FindingRow[] = [];
  let nextId = 0;
  let searchGen = 0; // supersede a stale scan when a newer one starts (mirrors NearDuplicatesDialog)

  // Paths the user has marked for removal. A Set, reassigned to trigger reactivity. SAFETY: starts
  // EMPTY — nothing is auto-selected (mirrors NearDuplicatesDialog/SimilarImagesDialog).
  let selected = new Set<string>();
  let deleting = false;

  // No per-group keeper guard here (there are no groups — each finding is independent junk); the only
  // safety gate is "something must be selected".
  $: canClean = selected.size > 0;

  // Group findings by reason for display, in REASON_ORDER, dropping empty buckets.
  $: groups = REASON_ORDER.map((reason) => ({
    reason,
    rows: findings.filter((f) => f.reason === reason),
  })).filter((g) => g.rows.length > 0);

  function toggle(path: string) {
    if (selected.has(path)) selected.delete(path);
    else selected.add(path);
    selected = new Set(selected);
  }

  async function cleanUp() {
    if (!canClean) return;
    const paths = [...selected];
    deleting = true;
    try {
      // Best-effort checkpoint before the bulk move so it's reversible beyond the Bin too. A failure
      // here must never block the (already recoverable) trash move — swallow it. `unwrap` also catches
      // a `{status:"error"}` envelope (a Rust-side `Err(String)` rejects the raw promise without the
      // generated binding rethrowing it, since it only rethrows `Error` instances) so that case is
      // logged the same as a thrown error instead of silently vanishing (CPE-1328).
      try {
        unwrap(await commands.checkpointCreate(root, "Before removing clutter"));
      } catch (e) {
        console.error("Declutter: pre-cleanup checkpoint failed (proceeding with trash move)", e);
      }
      await commands.deleteToTrash(paths); // returns OpResult[] directly (no Result wrapper)
      const removed = new Set(paths);
      findings = findings.filter((f) => !removed.has(f.path));
      selected = new Set();
    } catch (e) {
      error = String(e);
    } finally {
      deleting = false;
    }
  }

  async function run() {
    loading = true;
    error = "";
    started = true;
    findings = [];
    selected = new Set();
    const gen = ++searchGen;
    try {
      const result = await commands.organizeClutter(root);
      if (gen !== searchGen) return; // superseded by a newer scan — drop this stale response
      if (result.status === "error") {
        error = result.error;
        return;
      }
      findings = result.data.map((f) => ({
        id: nextId++,
        name: f.name,
        path: joinPath(root, f.name),
        reason: f.reason,
      }));
    } catch (e) {
      if (gen === searchGen) {
        error = String(e);
        findings = [];
      }
    } finally {
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
  <div class="dialog" role="dialog" aria-modal="true" aria-label={$t("dc.title")} on:click|stopPropagation>
    <header>
      <h2>{$t("dc.title")}</h2>
      <span class="root" title={root}>{baseName(root) || root}</span>
      <button class="x" data-testid="dc-close-btn" title={$t("common.close")} on:click={() => dispatch("close")}>
        <Icon name="close" size={14} />
      </button>
    </header>

    {#if !started}
      <div class="intro">
        <p>{$t("dc.intro")}</p>
        <button class="btn primary" data-testid="dc-scan-btn" on:click={run}>{$t("dc.scan")}</button>
      </div>
    {:else if loading}
      <p class="dim">{$t("dc.scanning")}</p>
    {:else if error}
      <p class="err">{error}</p>
      <button class="mini" data-testid="dc-rescan-btn" on:click={run}>{$t("dc.scan")}</button>
    {:else if findings.length === 0}
      <p class="dim" data-testid="dc-none">{$t("dc.none")}</p>
      <button class="mini" data-testid="dc-rescan-btn" on:click={run}>{$t("dc.scan")}</button>
    {:else}
      <div class="summary">
        <span>
          {findings.length === 1
            ? $t("dc.summaryOne", { count: findings.length })
            : $t("dc.summaryMany", { count: findings.length })}
        </span>
        <span class="cleanup">
          <button class="mini" data-testid="dc-rescan-btn" on:click={run}>{$t("dc.scan")}</button>
          <button class="mini danger" data-testid="dc-move-btn" disabled={!canClean || deleting} on:click={cleanUp}>
            {deleting ? $t("dc.removing") : $t("dc.moveToBin", { count: selected.size })}
          </button>
        </span>
      </div>
      <div class="results">
        {#each groups as g (g.reason)}
          <div class="group" data-testid="dc-group">
            <div class="ghead">
              <Icon name="delete" size={13} />
              {reasonLabel(g.reason)} ({g.rows.length})
            </div>
            <div class="items">
              {#each g.rows as f (f.id)}
                <div class="row" class:picked={selected.has(f.path)} data-testid="dc-row">
                  <label class="pick" title={$t("dc.markForBin")}>
                    <input type="checkbox" checked={selected.has(f.path)} on:change={() => toggle(f.path)} />
                  </label>
                  <button class="item" title={f.path} on:click={() => reveal(f.path)}>
                    <span class="name">{f.name}</span>
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
    width: 640px; max-width: 94vw; max-height: 82vh; display: flex; flex-direction: column;
    background: var(--surface); border: 1px solid var(--dialog-border); border-radius: 10px;
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
  .mini.danger:not(:disabled) { border-color: var(--danger); color: var(--danger); }
  .mini:disabled { opacity: 0.5; }
  .results { overflow-y: auto; overflow-x: hidden; padding-bottom: 8px; }
  .group { margin-bottom: 12px; }
  .ghead { display: flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; padding: 3px 6px; }
  /* Rows reflow: the group wraps items onto more rows and grows; each row keeps its text on one line
     (CLAUDE.md's tick-tacks rule). */
  .items { display: flex; flex-wrap: wrap; gap: 6px; padding: 4px 6px; }
  .row { flex: 0 0 auto; display: flex; align-items: center; gap: 4px; }
  .row.picked .item { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent) inset; }
  .pick { display: inline-flex; align-items: center; }
  .item {
    flex: 0 0 auto; max-width: 280px; display: flex; align-items: center; gap: 6px;
    padding: 5px 10px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt);
  }
  .item:hover { background: var(--surface); border-color: var(--border-strong); }
  .name { font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 240px; }
  .dim { color: var(--text-faint); }
  .err { color: var(--danger); }
</style>
