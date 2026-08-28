<script lang="ts">
  /**
   * Checkpoint & rollback (CPE-1125, epic CPE-732). The headless-driveable command surface over the
   * CPE-1123 command layer (`checkpoint_create/list/preview_revert/revert/revert_one`): capture the
   * current root into a labelled checkpoint, list what's been captured, preview a revert (the
   * restore-plan summary + a drift report), then revert — either the whole tree or a single path. A
   * revert overwrites/deletes files on disk directly (no Recycle Bin), so both revert actions arm an
   * inline confirmation panel first; nothing destructive fires on a single click. This is the minimal
   * palette-driven flow — the richer visual restore panel + timeline markers are the deferred GUI cap
   * (CPE-1126).
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import type { Checkpoint, CheckpointFailure, RevertPreview, RevertOutcome, FileDiff } from "../bindings.gen";
  import Icon from "./Icon.svelte";
  import DiffPeek from "./DiffPeek.svelte"; // reused as-is (CPE-1197 frontend half, epic CPE-735)
  import RevertOutcomePanel from "./RevertOutcomePanel.svelte"; // CPE-1845 — shared, reason-carrying
  import { summarizeRevert } from "../revertHoldBack";
  import { t } from "../i18n";
  import { displaySafePath } from "../filename";

  /** Root folder to checkpoint/revert (pre-filled with the current folder by App). */
  export let initialPath = "";

  const dispatch = createEventDispatcher<{ reverted: void; cancel: void; help: void }>();

  let path = initialPath;
  let label = "";
  let revertOnePath = "";

  let checkpoints: Checkpoint[] = [];
  /** Failed pre-write checkpoint attempts for `path` (CPE-1600) — kept in a SEPARATE list from
   *  `checkpoints`, never merged into that shape: a `CheckpointFailure` carries no `manifest_id`, so it
   *  structurally cannot be passed to `doPreview`/`armRevert` (both take a `Checkpoint`). Rendered
   *  interleaved with `checkpoints` by timestamp (see {@link rows}) so a failed attempt shows up exactly
   *  where a real checkpoint would have been, but with a visibly different row and no action buttons at
   *  all — not merely disabled ones — so it can never be mistaken for a restore point. */
  let checkpointFailures: CheckpointFailure[] = [];
  let selected: Checkpoint | null = null;
  let preview: RevertPreview | null = null;
  let outcome: RevertOutcome | null = null;
  /** Which destructive action is armed and awaiting a second click: null when nothing is pending. */
  let confirming: "all" | "one" | null = null;

  let loading = false;
  let error = "";
  let note = "";

  /** The drift path whose "Open diff" panel is expanded, or null (CPE-1197 frontend half). */
  let diffOpenPath: string | null = null;
  let diffLoading = false;
  let diffResult: FileDiff | null = null;
  /** Clean-error text (binary/oversize/unknown-path) from `checkpointDiffFile` — shown as a small
   *  notice, never a crash (mirrors the backend's error-rather-than-truncate convention). */
  let diffError = "";

  async function loadList() {
    if (!path.trim()) { checkpoints = []; checkpointFailures = []; return; }
    loading = true; error = "";
    try {
      // Two independent reads (CPE-1600): a failure to load the failures list must not blank the real
      // checkpoints (and vice versa) — `Promise.allSettled`, not `Promise.all`, so one side's error
      // can't hide the other's data.
      const [cpResult, cfResult] = await Promise.allSettled([
        commands.checkpointList(path.trim()),
        commands.checkpointFailuresList(path.trim()),
      ]);
      checkpoints = cpResult.status === "fulfilled" ? unwrap(cpResult.value) ?? [] : [];
      checkpointFailures = cfResult.status === "fulfilled" ? unwrap(cfResult.value) ?? [] : [];
      if (cpResult.status === "rejected") throw cpResult.reason;
      if (cfResult.status === "rejected") throw cfResult.reason;
    } catch (e) { error = String(e); } finally { loading = false; }
  }

  onMount(loadList);

  /** One row in the merged, newest-first list: either a real checkpoint (selectable, previewable,
   *  revertable) or a failed attempt (display-only — see `checkpointFailures`'s doc). A discriminated
   *  union rather than one shared shape, so the template can never wire a revert/preview action to a
   *  failure row by accident — there's no `manifest_id` field on that branch to pass one. */
  type Row = { kind: "ok"; cp: Checkpoint } | { kind: "failed"; cf: CheckpointFailure; key: string };
  $: rows = ([
    ...checkpoints.map((cp): Row => ({ kind: "ok", cp })),
    ...checkpointFailures.map((cf, i): Row => ({ kind: "failed", cf, key: `${cf.ts}-${i}` })),
  ] as Row[]).sort((a, b) => (b.kind === "ok" ? b.cp.ts : b.cf.ts) - (a.kind === "ok" ? a.cp.ts : a.cf.ts));

  function resetOutcome() {
    preview = null;
    outcome = null;
    confirming = null;
    diffOpenPath = null;
    diffResult = null;
    diffError = "";
  }

  /** Toggle the "Open diff" panel for one drift path: fetches before (checkpoint) / after (live) text via
   *  `checkpointDiffFile` and hands it to `DiffPeek` (reused, not rebuilt). Binary/oversize/unknown-path
   *  errors from the backend surface as a small inline notice instead of throwing/crashing the dialog. */
  async function toggleDiff(relPath: string) {
    if (diffOpenPath === relPath) { diffOpenPath = null; return; }
    diffOpenPath = relPath;
    diffResult = null;
    diffError = "";
    if (!selected) return;
    diffLoading = true;
    // Guard against a stale response: if the user opens row A then row B before A's call resolves,
    // A must not write its diff into B's open panel. Only apply the result if this row is still the
    // one open (same generation-token idea as ContentSearchDialog).
    const forPath = relPath;
    try {
      const result = unwrap(await commands.checkpointDiffFile(path.trim(), selected.manifest_id, relPath));
      if (diffOpenPath === forPath) diffResult = result;
    } catch (e) {
      if (diffOpenPath === forPath) diffError = String(e instanceof Error ? e.message : e);
    } finally {
      if (diffOpenPath === forPath) diffLoading = false;
    }
  }

  async function doCreate() {
    if (!path.trim()) return;
    loading = true; error = ""; note = "";
    try {
      const created = unwrap(await commands.checkpointCreate(path.trim(), label.trim()));
      label = "";
      note = `Checkpoint "${created.checkpoint.label || created.checkpoint.manifest_id}" captured — ` +
        `${created.new_blobs} new, ${created.reused_blobs} reused` +
        (created.skipped.length ? `, ${created.skipped.length} skipped` : "") + ".";
      await loadList();
    } catch (e) { error = String(e); } finally { loading = false; }
  }

  async function doPreview(cp: Checkpoint) {
    selected = cp;
    resetOutcome();
    loading = true; error = "";
    try {
      // No watched session in this dialog → `null` keeps the conservative drift behaviour (CPE-1151).
      preview = unwrap(await commands.checkpointPreviewRevert(path.trim(), cp.manifest_id, null));
    } catch (e) { error = String(e); } finally { loading = false; }
  }

  /** Arm the whole-tree revert — the caller must click the confirm panel's "Revert" to actually run it. */
  function armRevert(cp: Checkpoint) {
    selected = cp;
    outcome = null;
    confirming = "all";
  }

  /** Arm a single-path revert — same two-step confirm, scoped to `revertOnePath`. */
  function armRevertOne(cp: Checkpoint) {
    if (!revertOnePath.trim()) { error = "Enter a path to revert first."; return; }
    selected = cp;
    outcome = null;
    confirming = "one";
  }

  function cancelConfirm() {
    confirming = null;
  }

  async function doRevertConfirmed() {
    if (!selected) return;
    const cp = selected;
    const kind = confirming;
    confirming = null;
    loading = true; error = ""; note = "";
    try {
      outcome = kind === "one"
        ? unwrap(await commands.checkpointRevertOne(path.trim(), cp.manifest_id, revertOnePath.trim()))
        : unwrap(await commands.checkpointRevert(path.trim(), cp.manifest_id));
      // CPE-1845: the note is the same one-statement-plus-a-count the panel shows, so the two never
      // disagree — and it no longer calls a deliberate hold-back "skipped" alongside real failures. It
      // keeps the leading verb because it renders at the TOP of the dialog, in the slot shared with
      // "Checkpoint … captured", detached from the panel that says what it is about.
      note = `Revert — ${summarizeRevert(outcome).headline[0].toLowerCase()}${summarizeRevert(outcome).headline.slice(1)}`;
      dispatch("reverted");
    } catch (e) { error = String(e); } finally { loading = false; }
  }

  const fmtTime = (ms: number) => new Date(ms).toLocaleString();
  const fmtBytes = (n: number) => {
    if (n < 1024) return `${n} B`;
    const units = ["KB", "MB", "GB", "TB"];
    let v = n / 1024, i = 0;
    while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
    return `${v.toFixed(1)} ${units[i]}`;
  };
  const shortId = (id: string) => (id.length > 12 ? `${id.slice(0, 12)}…` : id);
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && !confirming && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => !confirming && dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Checkpoint & rollback" on:click|stopPropagation>
    <div class="head-row">
      <h2>Checkpoint & rollback</h2>
      <button class="docs" title="Open documentation" aria-label="Open documentation" data-testid="help-btn"
        on:click={() => dispatch("help")}><Icon name="book" size={15} /></button>
    </div>
    <p>Capture the current tree into a labelled checkpoint, then revert to it later — the whole tree or a
       single path. Preview first: it shows what would change and flags <strong>drift</strong> (anything
       changed since the checkpoint) so a revert never surprises you.</p>

    <div class="paths">
      <input class="path" placeholder="Folder to checkpoint…" bind:value={path} aria-label="Root folder"
        data-testid="checkpoint-path" on:change={loadList} />
      <button class="btn" data-testid="refresh-btn" disabled={loading || !path.trim()} on:click={loadList}>Refresh</button>
    </div>

    <div class="create-row">
      <input class="label-input" placeholder="Checkpoint label (optional)…" bind:value={label}
        aria-label="Checkpoint label" data-testid="checkpoint-label" />
      <button class="btn primary" data-testid="create-btn" disabled={loading || !path.trim()} on:click={doCreate}>
        Create checkpoint
      </button>
    </div>

    {#if error}<div class="err" data-testid="error">{error}</div>{/if}
    {#if note}<div class="note" data-testid="note">{note}</div>{/if}

    <div class="list" data-testid="checkpoint-list">
      {#if !path.trim()}
        <div class="empty">Enter a folder to see its checkpoints.</div>
      {:else if loading && rows.length === 0}
        <div class="empty">Loading…</div>
      {:else if rows.length === 0}
        <div class="empty">No checkpoints yet — create one above.</div>
      {:else}
        {#each rows as row (row.kind === "ok" ? row.cp.manifest_id : row.key)}
          {#if row.kind === "ok"}
            {@const cp = row.cp}
            <div class="cp-row" class:selected={selected?.manifest_id === cp.manifest_id} data-testid="cp-{cp.manifest_id}">
              <div class="cp-info">
                <span class="cp-label">{cp.label || shortId(cp.manifest_id)}</span>
                <span class="cp-meta">{fmtTime(cp.ts)} · {shortId(cp.manifest_id)}</span>
              </div>
              <div class="cp-actions">
                <button class="mini" data-testid="preview-btn-{cp.manifest_id}" disabled={loading} on:click={() => doPreview(cp)}>Preview</button>
                <button class="mini danger" data-testid="revert-btn-{cp.manifest_id}" disabled={loading} on:click={() => armRevert(cp)}>Revert…</button>
              </div>
            </div>
          {:else}
            {@const cf = row.cf}
            <!-- CPE-1600: structurally distinct from a `cp-row` — no manifest id, no Preview/Revert
                 buttons at all (not disabled ones: ABSENT), a "ban" icon, and danger-tinted styling, so
                 this can never read as "here's a checkpoint you could restore". `title` restates the
                 failure as a tooltip since there's no room for the full reason inline. -->
            <div class="cp-row cp-row-failed" data-testid="cpf-{row.key}"
              title="{$t('ckpt.failedTitle')}: {cf.reason}">
              <Icon name="ban" size={16} />
              <div class="cp-info">
                <span class="cp-label cp-label-failed">{$t("ckpt.failedTitle")}</span>
                <span class="cp-meta">{fmtTime(cf.ts)} · {cf.operation}</span>
                <span class="cp-reason">{cf.reason}</span>
              </div>
            </div>
          {/if}
        {/each}
      {/if}
    </div>

    {#if selected}
      <div class="revert-one">
        <input class="path-input" placeholder="Path under the root to revert alone…" bind:value={revertOnePath}
          aria-label="Path to revert" data-testid="revert-one-path" />
        <button class="mini danger" data-testid="revert-one-btn" disabled={loading || !revertOnePath.trim()}
          on:click={() => selected && armRevertOne(selected)}>Revert this path…</button>
      </div>
    {/if}

    {#if preview}
      <div class="preview" data-testid="preview-panel">
        <div class="preview-lead" data-testid="preview-lead">Reverting will:</div>
        <div class="preview-counts">
          <span title="Files you deleted come back">restore {preview.creates}</span>
          <span title="Changed files are reset to the checkpoint">overwrite {preview.overwrites}</span>
          <span title="Files added since the checkpoint are removed">delete {preview.deletes}</span>
          <span title="Total bytes written back to disk">{fmtBytes(preview.bytes_written)} to write</span>
          <span class:drift={preview.drift_count > 0}
            title="Changed since this checkpoint — reverting overwrites that newer work">{preview.drift_count} changed since this checkpoint</span>
        </div>
        {#if preview.drift_count > 0}
          <div class="drift-list" data-testid="drift-list">
            {#each preview.drift_paths as p (p)}
              <div class="drift-row">
                <div class="drift-item" title={displaySafePath(p)}>
                  <span class="drift-path">{displaySafePath(p)}</span>
                  <button class="mini diff-btn" data-testid="diff-btn-{p}" disabled={loading}
                    on:click={() => toggleDiff(p)}>{diffOpenPath === p ? "Close diff" : "Open diff"}</button>
                </div>
                {#if diffOpenPath === p}
                  <div class="diff-panel" data-testid="diff-panel-{p}">
                    {#if diffLoading}
                      <div class="diff-status">Loading diff…</div>
                    {:else if diffError}
                      <div class="diff-status diff-error" data-testid="diff-error">{diffError}</div>
                    {:else if diffResult}
                      <DiffPeek before={diffResult.before} after={diffResult.after} />
                    {/if}
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    {#if outcome}
      <div class="outcome" data-testid="outcome-panel">
        <RevertOutcomePanel {outcome} testid="outcome" verb="Reverted" root={path} />
      </div>
    {/if}

    {#if confirming && selected}
      <div class="confirm" data-testid="confirm-revert">
        <p class="confirm-msg">
          {#if confirming === "all"}
            This overwrites/recreates/deletes files under <strong>{displaySafePath(path)}</strong> to match
            checkpoint <strong>{selected.label || shortId(selected.manifest_id)}</strong>. This cannot be undone.
          {:else}
            This reverts <strong>{displaySafePath(revertOnePath)}</strong> alone to checkpoint
            <strong>{selected.label || shortId(selected.manifest_id)}</strong>. This cannot be undone.
          {/if}
        </p>
        <div class="confirm-actions">
          <button class="btn" data-testid="confirm-cancel-btn" on:click={cancelConfirm}>Cancel</button>
          <button class="btn primary danger" data-testid="confirm-yes-btn" on:click={doRevertConfirmed}>
            Yes, revert
          </button>
        </div>
      </div>
    {/if}

    <div class="actions">
      <button class="btn primary" on:click={() => dispatch("cancel")}>Close</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 680px; max-width: 95vw; max-height: 90vh; overflow: auto; background: var(--surface); border: 1px solid var(--dialog-border); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  .head-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  /* CPE-1635: a flex item's default `min-width: auto` floors it at its own content width, which can
     force the row (and, at extreme squeeze, the dialog) wider than intended instead of letting the
     title give way. `min-width: 0` lets h2 shrink below that floor; the nowrap/ellipsis trio then makes
     it truncate gracefully rather than wrap or overflow once it actually runs out of room (e.g. a
     longer translated string at a narrow width). */
  h2 { font-size: 16px; margin-bottom: 8px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* flex-shrink: 0 (CPE-1635): the title (h2, above) is the one flex sibling that should give way —
     this icon button must stay at its declared 26x26 instead of also shrinking and distorting the SVG. */
  .docs { display: grid; place-items: center; height: 26px; width: 26px; padding: 0; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); flex-shrink: 0; }
  p { color: var(--text-dim); font-size: 12.5px; margin-bottom: 12px; line-height: 1.5; }
  .paths, .create-row, .revert-one { display: flex; gap: 8px; margin-bottom: 8px; }
  .path, .label-input, .path-input { flex: 1 1 auto; height: 30px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); min-width: 0; }
  .err { color: var(--danger); font-size: 12.5px; margin-bottom: 8px; }
  .note { color: var(--accent-text); font-size: 12.5px; margin-bottom: 8px; }
  .list { max-height: 30vh; overflow: auto; border: 1px solid var(--border); border-radius: var(--radius); margin-bottom: 8px; }
  .empty { padding: 12px; color: var(--text-dim); font-size: 12.5px; }
  .cp-row { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 8px 10px; border-bottom: 1px solid var(--border); }
  .cp-row:last-child { border-bottom: none; }
  .cp-row.selected { background: var(--surface-alt); }
  .cp-info { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .cp-label { font-size: 13px; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cp-meta { font-size: 11px; color: var(--text-dim); }
  .cp-actions { display: flex; gap: 6px; flex: 0 0 auto; }
  /* CPE-1600: a failed checkpoint attempt must never read like a restorable one — danger-tinted left
     border + background (same color-mix recipe `.confirm` below already uses for a destructive-action
     panel), a bold danger-colored heading instead of a checkpoint label, and — most importantly — no
     `.cp-actions` at all, so there's nothing here to click into a preview/revert flow. */
  .cp-row-failed { align-items: flex-start; gap: 8px; border-left: 3px solid var(--danger); background: color-mix(in srgb, var(--danger) 6%, var(--surface)); color: var(--danger); }
  .cp-label-failed { color: var(--danger); font-weight: 600; }
  .cp-reason { font-size: 11.5px; color: var(--text-dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .mini { height: 26px; padding: 0 10px; font-size: 12px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .mini.danger:not(:disabled) { border-color: var(--danger); color: var(--danger); }
  .mini:disabled { opacity: 0.4; }
  .preview { border: 1px solid var(--border); border-radius: var(--radius); padding: 10px; margin-bottom: 8px; }
  .preview-lead { font-size: 12px; font-weight: 600; color: var(--text); margin-bottom: 6px; }
  .preview-counts { display: flex; flex-wrap: wrap; gap: 12px; font-size: 12px; color: var(--text-dim); }
  .preview-counts span { white-space: nowrap; flex: 0 0 auto; }
  .preview-counts .drift { color: var(--warn); font-weight: 600; }
  .drift-list { margin-top: 8px; max-height: 26vh; overflow: auto; }
  .drift-row { padding: 2px 0; }
  .drift-item { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .drift-path { flex: 1 1 auto; font-size: 12px; color: var(--text-dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .diff-btn { flex: 0 0 auto; height: 22px; padding: 0 8px; font-size: 11px; }
  .diff-panel { margin: 4px 0 6px; }
  .diff-status { padding: 6px 8px; font-size: 12px; color: var(--text-dim); }
  .diff-status.diff-error { color: var(--warn); }
  .outcome { font-size: 12.5px; color: var(--text); margin-bottom: 8px; }
  .confirm { border: 1px solid var(--danger); border-radius: var(--radius); padding: 12px; margin-bottom: 8px; background: color-mix(in srgb, var(--danger) 8%, var(--surface)); }
  .confirm-msg { color: var(--text); font-size: 12.5px; margin-bottom: 10px; }
  .confirm-actions { display: flex; justify-content: flex-end; gap: 8px; }
  .actions { display: flex; justify-content: flex-end; margin-top: 6px; }
  .btn { height: 30px; padding: 0 14px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary.danger { background: var(--danger-fill); border-color: var(--danger-fill); }
</style>
