<script lang="ts">
  /**
   * Batch-Media dialog (CPE-1093, epic CPE-723): apply an ORDERED list of media ops (resize / convert /
   * rotate / flip / rename / strip-metadata) to the multi-image selection, with a live plan preview and
   * streamed progress. Modelled on `BatchRenameDialog.svelte` (dumb dialog: `paths` in, `apply`/`cancel`
   * events out) plus `WatchRulesDialog.svelte`'s add-row → pending-pill-list pattern for the ordered op
   * builder. Unlike batch rename, the plan itself needs the backend (collision-safe output paths live in
   * Rust, `cpe_server::batch_media::plan`), so the live preview is a debounced, generation-tokened IPC call
   * rather than pure client-side logic — and Apply is a *streamed* execute the dialog watches to completion
   * before telling the parent to report + refresh + close (CPE-1092 supplies both commands).
   *
   * CPE-1590: unchecking "Write to new files" arms a **silent, irreversible overwrite** of the selected
   * originals for any op combo with no dedicated output-renaming suffix (a lone Compress, Strip metadata,
   * or Watermark — `batch_media::plan` only guarantees `output != input` when `non_destructive` is true).
   * Apply now gates that case behind an explicit, can't-miss-it confirm step (mirrors `ShredConfirmDialog`'s
   * "no trash fallback" treatment) naming exactly how many originals will be overwritten, and — on
   * confirm — takes a best-effort pre-write `checkpointCreate` of every affected folder first, the same
   * "checkpoint before an irreversible batch" pattern `MetadataStudioDialog`/`DeclutterDialog`/
   * `SimilarImagesDialog` already use, so a confirmed overwrite is still recoverable via Checkpoints even
   * though it's deliberately never pushed onto the Ctrl+Z undo stack.
   */
  import { createEventDispatcher, onDestroy } from "svelte";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import { commands } from "../bindings.gen";
  import type { BatchReport, Corner, MediaOp, OpResult, PlannedItem } from "../bindings.gen";
  import { rawInvoke, createChannel, unwrap, type StreamChannel } from "../invoke";
  import { mediaOpLabel, opsToJob, overwritesInPlace, progressPercent, skipRows, uniqueParentDirs } from "../batchMedia";
  import { baseName } from "../contentSearch";

  /** Extensions the native "choose a watermark image" picker offers — mirrors the batch-media encoder's
   *  decode-capable set closely enough for a logo/stamp overlay (a superset of {@link canBatchTransform}
   *  is fine here since the overlay is only ever decoded, never re-encoded). */
  const WATERMARK_IMAGE_EXTS = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff"];

  /** Full paths of the (already image-filtered) selection to operate on. */
  export let paths: string[] = [];

  const dispatch = createEventDispatcher<{ apply: { report: BatchReport }; cancel: void }>();

  // ---- ordered op list ----
  let ops: MediaOp[] = [];
  let nonDestructive = true;

  // ---- "add op" form ----
  let opKind: MediaOp["op"] = "resize";
  let resizeMaxPx = 1024;
  let convertExt = "webp";
  let rotateDegrees: "90" | "180" | "270" = "90";
  let flipDir: "horizontal" | "vertical" = "horizontal";
  let renameTemplate = "{stem}";
  let compressQuality = 80;
  let watermarkImage = "";
  let watermarkPosition: Corner = "bottom_right";
  let watermarkOpacity = 80;
  /** Placeholder text for the rename-template field — a plain JS string, so the literal `{tokens}` in it
   *  aren't parsed as Svelte template expressions the way they would be if written directly in markup. */
  const renameTemplatePlaceholder = "{stem}-{n}";

  /** Build the op the current form fields describe, or `null` while it's incomplete/invalid. Takes every
   *  field as an explicit argument (rather than reading module-level state internally) so Svelte's reactive
   *  dependency scan — which only sees identifiers textually present in a `$:` statement — picks up every
   *  field it depends on. */
  function buildOp(
    kind: MediaOp["op"],
    maxPx: number,
    ext: string,
    degrees: "90" | "180" | "270",
    flip: "horizontal" | "vertical",
    template: string,
    quality: number,
    watermarkImg: string,
    watermarkPos: Corner,
    watermarkOp: number,
  ): MediaOp | null {
    switch (kind) {
      case "resize":
        return Number.isFinite(maxPx) && maxPx > 0 ? { op: "resize", max_px: Math.round(maxPx) } : null;
      case "convert": {
        const e = ext.trim().replace(/^\.+/, "");
        return e ? { op: "convert", to_ext: e } : null;
      }
      case "rotate":
        return { op: "rotate", degrees: Number(degrees) as 90 | 180 | 270 };
      case "flip":
        return { op: "flip", horizontal: flip === "horizontal" };
      case "rename": {
        const t = template.trim();
        return t ? { op: "rename", template: t } : null;
      }
      case "strip_metadata":
        return { op: "strip_metadata" };
      case "compress":
        return Number.isFinite(quality) && quality >= 1 && quality <= 100
          ? { op: "compress", quality: Math.round(quality) }
          : null;
      case "watermark":
        // Empty image is a valid, deliberate "no watermark configured" op (the backend treats it as a
        // no-op) — only the opacity range needs to validate for the op itself to build.
        return Number.isFinite(watermarkOp) && watermarkOp >= 0 && watermarkOp <= 100
          ? { op: "watermark", image: watermarkImg, position: watermarkPos, opacity: Math.round(watermarkOp) }
          : null;
      default:
        return null;
    }
  }

  $: pendingOp = buildOp(
    opKind,
    resizeMaxPx,
    convertExt,
    rotateDegrees,
    flipDir,
    renameTemplate,
    compressQuality,
    watermarkImage,
    watermarkPosition,
    watermarkOpacity,
  );

  /** Browse for a watermark overlay image via the native picker (CPE-1106) — same
   *  `@tauri-apps/plugin-dialog` `open` the app already uses elsewhere (see `App.svelte`). */
  async function browseForWatermarkImage() {
    try {
      const picked = await openFileDialog({
        directory: false,
        multiple: false,
        filters: [{ name: "Images", extensions: WATERMARK_IMAGE_EXTS }],
        title: "Choose a watermark image…",
      });
      if (typeof picked === "string") watermarkImage = picked;
    } catch {
      // Cancelled or unavailable — leave the current selection untouched.
    }
  }

  function addOp() {
    if (pendingOp) ops = [...ops, pendingOp];
  }
  function removeOp(i: number) {
    ops = ops.filter((_, idx) => idx !== i);
  }

  // ---- live plan preview (debounced + generation-tokened; the backend owns collision-safe paths) ----
  const DEBOUNCE_MS = 200;
  const MAX_PREVIEW = 300;

  $: job = opsToJob(ops, nonDestructive);

  let planned: PlannedItem[] = [];
  let planError: string | null = null;
  let planning = false;
  let planGen = 0;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  /** True once the user has clicked Apply on a plan that would overwrite originals in place, revealing
   *  the (CPE-1590) confirm panel below — reset on any change that could invalidate it. */
  let showOverwriteConfirm = false;

  // Both `job` and `paths` are referenced directly here so this re-runs on either changing.
  $: {
    job;
    paths;
    scheduleReplan();
    // Any edit that can change the plan invalidates a prior overwrite confirmation — never let a
    // confirm granted for one op combo silently carry over to a different (re-planned) one.
    showOverwriteConfirm = false;
  }

  function scheduleReplan() {
    if (debounceTimer) clearTimeout(debounceTimer);
    if (ops.length === 0) {
      // Nothing to plan yet — clear any stale preview/error instead of calling the backend.
      planned = [];
      planError = null;
      planning = false;
      return;
    }
    planning = true;
    debounceTimer = setTimeout(runPlan, DEBOUNCE_MS);
  }

  async function runPlan() {
    const gen = ++planGen; // stamp this call so a slower, now-stale response can't clobber a newer one
    const jobNow = job;
    const pathsNow = paths;
    try {
      const res = await commands.batchMediaPlan(jobNow, pathsNow);
      if (gen !== planGen) return; // superseded — drop it
      if (res.status === "ok") {
        planned = res.data;
        planError = null;
      } else {
        planned = [];
        planError = res.error;
      }
    } catch (e) {
      if (gen !== planGen) return;
      planned = [];
      planError = String(e);
    } finally {
      if (gen === planGen) planning = false;
    }
  }

  $: previewRows = planned.slice(0, MAX_PREVIEW);
  $: previewCappedTotal = planned.length > MAX_PREVIEW ? planned.length : 0;
  $: canApply = ops.length > 0 && planError === null && planned.length > 0 && !planning && !applying;

  // ---- CPE-1590: destructive-overwrite guard ----
  // Computed straight off the concrete planned paths (not re-derived op-combo heuristics), so it's
  // exact and robust to future ops: whenever the planner resolves output === input, that file's bytes
  // will be replaced in place with no way back through this dialog.
  $: overwriteItems = overwritesInPlace(planned);
  $: needsOverwriteConfirm = overwriteItems.length > 0;

  /** Apply button click: a plan that would overwrite originals in place opens the confirm panel instead
   *  of running immediately — the panel's own danger button is the one that actually calls {@link apply}. */
  function handleApplyClick() {
    if (!canApply) return;
    if (needsOverwriteConfirm) {
      showOverwriteConfirm = true;
    } else {
      apply();
    }
  }

  // ---- apply: streamed execute, progress rendered inside the dialog ----
  let applying = false;
  let applyError: string | null = null;
  let done = 0;
  let failed = 0;
  let total = 0;
  let activeChannel: StreamChannel<OpResult[]> | null = null;
  /** Set once the run finishes WITH skipped files: hold the dialog open on a results panel that lists every
   *  skipped file + reason (CPE-1115), so a skip is never silently dropped. Cleared → the dialog closes via
   *  the parent on "Done". A clean run (no skips) never sets this and closes immediately. */
  let completed: BatchReport | null = null;

  function detachChannel() {
    if (activeChannel) activeChannel.onmessage = null;
    activeChannel = null;
  }
  onDestroy(() => {
    detachChannel();
    if (debounceTimer) clearTimeout(debounceTimer);
  });

  async function apply() {
    if (!canApply) return;
    showOverwriteConfirm = false;
    applying = true;
    applyError = null;
    done = 0;
    failed = 0;
    total = planned.length;

    if (needsOverwriteConfirm) {
      // Best-effort pre-write checkpoint of every folder about to lose original files (CPE-1590), taken
      // ONCE per affected folder before any byte is touched — mirrors MetadataStudioDialog/
      // DeclutterDialog/SimilarImagesDialog's "checkpoint before an irreversible batch" convention. A
      // checkpoint failure must never block the (now explicitly confirmed) write; it's a bonus recovery
      // path layered on top of the confirm, not a gate.
      for (const dir of uniqueParentDirs(overwriteItems.map((it) => it.input))) {
        try {
          unwrap(await commands.checkpointCreate(dir, "Before batch media overwrite"));
        } catch (e) {
          console.error("Batch media: pre-overwrite checkpoint failed (proceeding with confirmed write)", e);
        }
      }
    }

    const ch = createChannel<OpResult[]>();
    activeChannel = ch;
    ch.onmessage = (batch) => {
      for (const r of batch) {
        done += 1;
        if (!r.ok) failed += 1;
      }
    };

    try {
      // Raw, not the typed `commands.batchMediaExecuteStream` (which routes through the busy-cursor
      // `invoke`) — this dialog renders its own progress, per BUSY-CURSOR.md.
      const report = await rawInvoke<BatchReport>("batch_media_execute_stream", {
        items: planned,
        job,
        onResult: ch,
      });
      detachChannel();
      applying = false;
      if (report.skipped.length > 0) {
        // Hold the dialog open on a results panel so the user sees exactly which files were skipped and
        // why, instead of a transient toast showing only the first (CPE-1115). "Done" finishes the flow.
        completed = report;
      } else {
        dispatch("apply", { report });
      }
    } catch (e) {
      detachChannel();
      applying = false;
      applyError = String(e);
    }
  }

  /** Acknowledge the skip results panel: hand the report to the parent (refresh + close). */
  function finish() {
    const report = completed;
    completed = null;
    if (report) dispatch("apply", { report });
  }

  /** Back out of the (CPE-1590) overwrite confirm panel without touching anything — the dialog itself
   *  stays open on the op list/preview exactly as it was. */
  function cancelOverwriteConfirm() {
    showOverwriteConfirm = false;
  }

  function cancel() {
    if (applying) return; // no mid-run cancel in v1 — let it finish rather than leaving a half-applied job
    detachChannel();
    dispatch("cancel");
  }
</script>

<svelte:window
  on:keydown={(e) => {
    if (e.key !== "Escape") return;
    // Escape backs out of the overwrite confirm panel first, one level at a time, rather than closing
    // the whole dialog straight through it.
    if (showOverwriteConfirm) cancelOverwriteConfirm();
    else cancel();
  }}
/>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={cancel}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Batch media" on:click|stopPropagation>
    <h2>Batch media — {paths.length} file{paths.length === 1 ? "" : "s"}</h2>

    <div class="opbuilder">
      <div class="brow">
        <select bind:value={opKind} aria-label="Operation">
          <option value="resize">Resize</option>
          <option value="convert">Convert</option>
          <option value="rotate">Rotate</option>
          <option value="flip">Flip</option>
          <option value="rename">Rename</option>
          <option value="strip_metadata">Strip metadata</option>
          <option value="compress">Compress</option>
          <option value="watermark">Watermark</option>
        </select>
        {#if opKind === "resize"}
          <input class="num" type="number" min="1" bind:value={resizeMaxPx} aria-label="Max size in pixels" />
          <span class="lbl">px (longest side)</span>
        {:else if opKind === "convert"}
          <input class="grow" placeholder="webp" bind:value={convertExt} aria-label="Target extension" />
        {:else if opKind === "rotate"}
          <select bind:value={rotateDegrees} aria-label="Rotate degrees">
            <option value="90">90°</option>
            <option value="180">180°</option>
            <option value="270">270°</option>
          </select>
        {:else if opKind === "flip"}
          <select bind:value={flipDir} aria-label="Flip direction">
            <option value="horizontal">Horizontal</option>
            <option value="vertical">Vertical</option>
          </select>
        {:else if opKind === "rename"}
          <input class="grow" placeholder={renameTemplatePlaceholder} bind:value={renameTemplate} aria-label="Rename template" />
        {:else if opKind === "compress"}
          <input class="num" type="number" min="1" max="100" bind:value={compressQuality} aria-label="Compress quality" />
          <span class="lbl">quality (1-100)</span>
        {:else if opKind === "watermark"}
          <button class="btn" type="button" on:click={browseForWatermarkImage}>Browse…</button>
          <span class="grow wm-path" title={watermarkImage || "No image chosen — no watermark"}>
            {watermarkImage ? baseName(watermarkImage) : "No image chosen (no watermark)"}
          </span>
          <select bind:value={watermarkPosition} aria-label="Watermark corner">
            <option value="top_left">Top-left</option>
            <option value="top_right">Top-right</option>
            <option value="bottom_left">Bottom-left</option>
            <option value="bottom_right">Bottom-right</option>
            <option value="center">Center</option>
          </select>
          <input class="num" type="number" min="0" max="100" bind:value={watermarkOpacity} aria-label="Watermark opacity" />
          <span class="lbl">opacity (0-100)</span>
        {/if}
        <button class="btn" data-testid="add-op-btn" disabled={!pendingOp} on:click={addOp}>+ Add</button>
      </div>

      {#if ops.length > 0}
        <div class="pills" data-testid="op-pills">
          {#each ops as op, i (i)}
            <span class="pill" data-testid="op-pill">
              <span class="pill-text">{mediaOpLabel(op)}</span>
              <button class="pill-x" aria-label="Remove operation" on:click={() => removeOp(i)}>✕</button>
            </span>
          {/each}
        </div>
      {/if}

      <label class="check">
        <input type="checkbox" bind:checked={nonDestructive} />
        Write to new files (non-destructive)
      </label>
      {#if !nonDestructive}
        <!-- CPE-1590: clearer labelling of what unchecking this actually does, always visible while
             unchecked (not only once a live overwriting plan is computed) so the risk is legible before
             you've even added an op that triggers it. -->
        <div class="overwrite-hint" data-testid="overwrite-hint">
          Unchecked: Compress, Strip metadata, and Watermark (alone or combined, with no other op that
          renames the output) overwrite your original files instead of creating new ones. Resize, Rotate,
          Flip, Convert, and Rename are unaffected — they always produce a differently-named file.
        </div>
      {/if}
    </div>

    <div class="preview" data-testid="plan-preview">
      {#if ops.length === 0}
        <div class="empty" data-testid="plan-preview-empty">Add an operation above to see a preview.</div>
      {:else}
        {#each previewRows as it (it.input)}
          <div class="rowp" data-testid="preview-row">
            <span class="from" title={it.input}>{baseName(it.input)}</span>
            <span class="arrow">→</span>
            <span class="to" title={it.output}>{baseName(it.output)}</span>
            <span class="summary">{it.summary}</span>
          </div>
        {/each}
        {#if previewCappedTotal > 0}
          <div class="capped">showing first {MAX_PREVIEW} of {previewCappedTotal}</div>
        {/if}
      {/if}
    </div>

    <div class="status">
      {#if planError}
        <span class="warn">{planError}</span>
      {:else if applyError}
        <span class="warn">{applyError}</span>
      {:else if ops.length === 0}
        <span>No operations yet.</span>
      {:else if planning}
        <span>Updating preview…</span>
      {:else}
        <span>{planned.length} file{planned.length === 1 ? "" : "s"} will be written.</span>
      {/if}
    </div>

    {#if applying || total > 0}
      <div class="progress" data-testid="batch-media-progress">
        <div class="bar"><div class="fill" class:err={failed > 0} style="width:{progressPercent(done, total)}%" /></div>
        <div class="sub">{done}/{total} done{failed > 0 ? `, ${failed} failed` : ""}</div>
      </div>
    {/if}

    {#if completed}
      <div class="skips" data-testid="batch-media-skips">
        <div class="skips-head">✓ {completed.written} written · ⚠ {completed.skipped.length} skipped</div>
        <ul class="skips-list">
          {#each skipRows(completed) as s}
            <li>
              <span class="skip-name" title={s.name}>{s.name}</span>
              <span class="skip-reason">— {s.reason}</span>
            </li>
          {/each}
        </ul>
        <div class="skips-note">
          Skipped files couldn't be processed (e.g. not a valid image) and were left untouched.
        </div>
      </div>
    {/if}

    {#if showOverwriteConfirm}
      <!-- CPE-1590: the destructive-overwrite confirm — replaces the normal action row so Apply can't
           still be clicked underneath it, names exactly how many originals will be overwritten, and
           requires its own explicit danger-styled click (not dismissible by a stray Enter/click on
           where Apply used to be). -->
      <div class="overwrite-confirm" data-testid="overwrite-confirm">
        <p class="overwrite-confirm-text">
          <strong>{overwriteItems.length}</strong> original file{overwriteItems.length === 1 ? "" : "s"}
          will be overwritten in place — the edited bytes replace the source file, and this is
          <strong>not on the Undo (Ctrl+Z) stack</strong>. A checkpoint of the affected folder
          {uniqueParentDirs(overwriteItems.map((it) => it.input)).length === 1 ? "" : "s"} will be taken
          first as a recovery net, but the only way back after that is reverting that checkpoint or your
          own backup.
        </p>
        <div class="overwrite-confirm-actions">
          <button class="btn" data-testid="overwrite-confirm-cancel" on:click={cancelOverwriteConfirm}>Cancel</button>
          <button class="btn primary danger" data-testid="overwrite-confirm-go" on:click={apply}>
            Overwrite {overwriteItems.length} file{overwriteItems.length === 1 ? "" : "s"}
          </button>
        </div>
      </div>
    {:else}
      <div class="actions">
        {#if completed}
          <button class="btn primary" on:click={finish} data-testid="batch-media-done">Done</button>
        {:else}
          <button class="btn" data-testid="cancel-btn" disabled={applying} on:click={cancel}>Cancel</button>
          <button class="btn primary" data-testid="apply-btn" disabled={!canApply} on:click={handleApplyClick}>{applying ? "Applying…" : "Apply"}</button>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
    display: grid;
    place-items: center;
    z-index: 200;
  }
  .dialog {
    width: 620px;
    max-width: 92vw;
    background: var(--surface);
    border: 1px solid var(--dialog-border);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 {
    font-size: 16px;
    margin-bottom: 14px;
  }
  .opbuilder {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 12px;
  }
  .brow { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .brow .grow { flex: 1 1 120px; }
  select, input:not([type="checkbox"]) {
    height: 30px;
    padding: 0 8px;
    font: inherit;
    color: var(--text);
    background: var(--surface-alt);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    min-width: 0;
  }
  .num { width: 90px; }
  .lbl { font-size: 12px; color: var(--text-dim); }
  .wm-path {
    font-size: 12px;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  /* Reflowing pill list (tick-tacks convention): the container wraps onto more rows and grows its
     height; each pill keeps its own text on one line and never shrinks. */
  .pills { display: flex; flex-wrap: wrap; gap: 6px; }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: 0 0 auto;
    max-width: 260px;
    padding: 3px 6px 3px 10px;
    border-radius: 999px;
    background: var(--accent);
    color: #fff;
    font-size: 11.5px;
  }
  .pill-text {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pill-x {
    flex: 0 0 auto;
    width: 16px;
    height: 16px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: #fff;
    font-size: 10px;
    line-height: 1;
  }
  .pill-x:hover { background: rgba(255, 255, 255, 0.25); }

  label.check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-dim);
  }

  /* CPE-1590: ambient reminder of what the unchecked box actually does, shown right below it. */
  .overwrite-hint {
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--danger);
    background: var(--surface-alt);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 6px 8px;
  }

  .preview {
    max-height: 200px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 6px;
    margin-bottom: 10px;
    font-size: 12px;
    font-family: ui-monospace, "Cascadia Code", "Consolas", monospace;
  }
  .empty { color: var(--text-dim); font-size: 12.5px; padding: 8px 2px; font-family: inherit; }
  .rowp {
    display: grid;
    grid-template-columns: 1fr auto 1fr 1.2fr;
    gap: 8px;
    align-items: center;
    padding: 2px 4px;
  }
  .rowp .from, .rowp .to, .rowp .summary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rowp .to { color: var(--accent); }
  .rowp .summary { color: var(--text-dim); }
  .arrow { color: var(--text-faint); }
  .capped { color: var(--text-faint); font-size: 11px; padding: 4px 4px 0; }

  .status {
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 10px;
    min-height: 16px;
  }
  .status .warn { color: var(--danger); }

  .progress { margin-bottom: 14px; }
  .bar { height: 6px; margin: 4px 0; background: var(--surface-alt); border-radius: 3px; overflow: hidden; }
  .fill { height: 100%; background: var(--accent); border-radius: 3px; transition: width 0.15s linear; }
  .fill.err { background: var(--danger); }
  .sub { font-size: 11px; color: var(--text-dim); font-variant-numeric: tabular-nums; }

  .actions { display: flex; justify-content: flex-end; gap: 8px; }
  .btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
  }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover:not(:disabled) { background: var(--accent-hover); }
  .btn:disabled { opacity: 0.5; }
  .btn.primary.danger { background: var(--danger); border-color: var(--danger); }
  .btn.primary.danger:hover:not(:disabled) { background: var(--danger-hover); }

  /* CPE-1590: the destructive-overwrite confirm panel — replaces the normal action row rather than
     floating another backdrop over it, so there's exactly one thing to click either way. */
  .overwrite-confirm {
    border: 1px solid var(--danger);
    border-radius: var(--radius);
    background: var(--surface-alt);
    padding: 10px 12px;
    margin-bottom: 4px;
  }
  .overwrite-confirm-text {
    font-size: 12.5px;
    line-height: 1.55;
    color: var(--text);
    margin: 0 0 10px;
  }
  .overwrite-confirm-actions { display: flex; justify-content: flex-end; gap: 8px; }

  /* CPE-1115: prominent skipped-files results panel — a skip is never silently dropped. */
  .skips {
    margin-bottom: 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-alt);
    padding: 8px 10px;
  }
  .skips-head { font-size: 12.5px; font-weight: 600; color: var(--text); margin-bottom: 6px; }
  .skips-list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 140px;
    overflow-y: auto;
    font-size: 12px;
  }
  .skips-list li { padding: 2px 0; display: flex; flex-wrap: wrap; gap: 4px; }
  .skip-name { color: var(--text); font-weight: 500; }
  .skip-reason { color: var(--danger); }
  .skips-note { margin-top: 6px; font-size: 11px; color: var(--text-dim); }
</style>
