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
   *
   * CPE-1599: that confirm step used to be a **pure frontend invariant** — the engine (`execute_plan_walk`)
   * had no idea a confirmation was ever required, so any other caller of `batch_media_execute_stream`
   * (devtools, a future automation surface) could skip it entirely. The engine now refuses an in-place
   * plan unless `BatchJob.confirmed_overwrite` is set, so this dialog's "Overwrite N files" button (via
   * {@link confirmOverwriteJob}, `../batchMedia`'s single named seam for setting that flag) is the ONLY
   * place in the app that can make the backend actually perform an in-place write.
   */
  import { createEventDispatcher, onDestroy } from "svelte";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import { commands } from "../bindings.gen";
  import type { BatchReport, Corner, MediaOp, OpResult, PlannedItem } from "../bindings.gen";
  import { rawInvoke, createChannel, unwrap, type StreamChannel } from "../invoke";
  import { t } from "../i18n";
  import {
    confirmOverwriteJob,
    mediaOpLabel,
    opsToJob,
    overwritesInPlace,
    progressPercent,
    skipRows,
    templateEscapesDirectory,
    uniqueParentDirs,
  } from "../batchMedia";
  import type { CheckpointPartial } from "../batchMedia";
  import { baseName } from "../contentSearch";
  import { recordCheckpointFailure } from "../checkpointFailures";

  /** Extensions the native "choose a watermark image" picker offers — mirrors the batch-media encoder's
   *  decode-capable set closely enough for a logo/stamp overlay (a superset of {@link canBatchTransform}
   *  is fine here since the overlay is only ever decoded, never re-encoded). */
  const WATERMARK_IMAGE_EXTS = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff"];

  /** Full paths of the (already image-filtered) selection to operate on. */
  export let paths: string[] = [];

  // `checkpointFailures`/`checkpointPartial` ride on BOTH events (CPE-1590, extended CPE-1599): the
  // in-dialog warning is dismissable by Escape or a backdrop click, so the parent needs both on the
  // cancel path too or a habitual keystroke discards them unread. Kept as two SEPARATE lists — a folder
  // with NO checkpoint at all is a materially worse outcome than one with a checkpoint missing a few
  // files, and collapsing them into one list previously blurred that distinction in the copy.
  const dispatch = createEventDispatcher<{
    apply: { report: BatchReport; checkpointFailures: string[]; checkpointPartial: CheckpointPartial[] };
    cancel: { checkpointFailures: string[]; checkpointPartial: CheckpointPartial[] };
  }>();

  // ---- ordered op list ----
  let ops: MediaOp[] = [];
  let nonDestructive = true;

  // ---- "add op" form ----
  let opKind: MediaOp["op"] = "resize";
  let resizeMaxPx = 1024;
  let convertExt = "webp";
  let rotateDegrees: "90" | "180" | "270" = "90";
  let flipDir: "horizontal" | "vertical" = "horizontal";
  /** The Rename op's pre-filled default template — a plain JS constant (not embedded literally in markup,
   *  which Svelte would otherwise try to parse `{tokens}` in as mustache expressions). Left at this
   *  default, Rename reproduces the input's stem verbatim — a narrow overwrite-in-place edge case the
   *  CPE-1590 overwrite-hint copy below calls out by referencing this same constant. */
  const RENAME_DEFAULT_TEMPLATE = "{stem}";
  let renameTemplate = RENAME_DEFAULT_TEMPLATE;
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
        // CPE-1623 follow-up: to_ext feeds the exact same joined output path as a Rename template did —
        // reject an escaping extension here too, before "+ Add" is even enabled, mirroring the backend's
        // now-broadened validate() rejection (previously Convert's extension went unchecked at this layer).
        const e = ext.trim().replace(/^\.+/, "");
        return e && !templateEscapesDirectory(e) ? { op: "convert", to_ext: e } : null;
      }
      case "rotate":
        return { op: "rotate", degrees: Number(degrees) as 90 | 180 | 270 };
      case "flip":
        return { op: "flip", horizontal: flip === "horizontal" };
      case "rename": {
        // CPE-1623: a template containing a path separator or ".." could move the computed output
        // outside the folder the user picked — reject it here (before "+ Add" is even enabled) so the
        // user is told before they click, mirroring the backend's own validate() rejection.
        const trimmed = template.trim();
        return trimmed && !templateEscapesDirectory(trimmed) ? { op: "rename", template: trimmed } : null;
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
  /** Folders whose best-effort pre-overwrite `checkpointCreate` FAILED OUTRIGHT (CPE-1590 code-review
   *  follow-up) — these have NO recovery net at all. Kept strictly separate from `checkpointPartial`
   *  below (CPE-1599 UAT follow-up): a folder with zero protection is a materially worse outcome than one
   *  with a checkpoint that's merely missing a few files, and the copy for each must say so plainly rather
   *  than blur the two into one softened sentence. A non-empty list forces the dialog to hold open (like a
   *  skip) so the warning is actually seen, even on an otherwise-clean run. Reset at the start of every
   *  apply(). */
  let checkpointFailures: string[] = [];
  /** Folders whose pre-overwrite `checkpointCreate` SUCCEEDED but left `skippedCount` file(s) uncaptured
   *  (oversize/budget) — the checkpoint exists, but doesn't cover everything about to be overwritten
   *  (CPE-1599 UAT follow-up). The confirm panel promises this checkpoint as the recovery net, so a
   *  partial one must never be silent either — but it is a strictly better situation than
   *  `checkpointFailures`, and must read that way, not as "no checkpoint at all". Reset at the start of
   *  every apply(). */
  let checkpointPartial: CheckpointPartial[] = [];

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
    checkpointFailures = [];
    checkpointPartial = [];

    if (needsOverwriteConfirm) {
      // Best-effort pre-write checkpoint of every folder about to lose original files (CPE-1590), taken
      // ONCE per affected folder before any byte is touched — mirrors MetadataStudioDialog/
      // DeclutterDialog/SimilarImagesDialog's "checkpoint before an irreversible batch" convention. A
      // checkpoint failure must never BLOCK the (now explicitly confirmed) write — the user already
      // consented on the confirm panel — but it must also never be silent: the confirm text promises this
      // checkpoint as the recovery net for a write that otherwise has none, so an outright failure is
      // recorded in `checkpointFailures` (no net at all) and a merely-incomplete one in `checkpointPartial`
      // (a net that's missing some files) — kept as two separate lists (CPE-1599 UAT follow-up) so the
      // results panel below can tell the user which situation they're actually in.
      for (const dir of uniqueParentDirs(overwriteItems.map((it) => it.input))) {
        try {
          const created = unwrap(await commands.checkpointCreate(dir, "Before batch media overwrite"));
          if (created.skipped.length > 0) {
            // CPE-1599: `checkpoint_create` captures with an unlimited budget today, but its `skipped`
            // field exists precisely so a future budget cap can't silently omit files from "recovery"
            // without this call site noticing — inspect it now, before that trap exists, rather than
            // discarding `created` entirely. The checkpoint DID succeed here, just not completely, so
            // this is `checkpointPartial`, not `checkpointFailures`.
            console.warn(
              `Batch media: pre-overwrite checkpoint for "${dir}" left ${created.skipped.length} file(s) uncaptured (recovery incomplete)`,
              created.skipped,
            );
            checkpointPartial = [...checkpointPartial, { dir, skippedCount: created.skipped.length }];
          }
        } catch (e) {
          console.error("Batch media: pre-overwrite checkpoint failed (proceeding with confirmed write)", e);
          checkpointFailures = [...checkpointFailures, dir];
          // CPE-1600: durable record alongside the console line + the in-dialog warning, so this doesn't
          // vanish once the dialog closes and the ~5s `showNotice` banner has scrolled by unread.
          void recordCheckpointFailure(dir, "Before batch media overwrite", e);
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

    // CPE-1599: the engine refuses to run an in-place plan without `confirmed_overwrite` set. This is the
    // ONLY place that flag is ever flipped to true — and only once `apply()` has actually been reached via
    // the confirm panel's "Overwrite N files" button (the sole caller of `apply()` when
    // `needsOverwriteConfirm` is true; see `handleApplyClick`). A plan with nothing to confirm sends the
    // job unchanged (`confirmed_overwrite` stays the `false` `opsToJob` always builds it as).
    const jobToSend = needsOverwriteConfirm ? confirmOverwriteJob(job) : job;

    try {
      // Raw, not the typed `commands.batchMediaExecuteStream` (which routes through the busy-cursor
      // `invoke`) — this dialog renders its own progress, per BUSY-CURSOR.md.
      const report = await rawInvoke<BatchReport>("batch_media_execute_stream", {
        items: planned,
        job: jobToSend,
        onResult: ch,
      });
      detachChannel();
      applying = false;
      if (report.skipped.length > 0 || checkpointFailures.length > 0 || checkpointPartial.length > 0) {
        // Hold the dialog open on a results panel so the user sees exactly which files were skipped and
        // why (CPE-1115), AND — even on an otherwise-clean run — which folder(s) have no pre-overwrite
        // checkpoint at all or only a partial one (CPE-1590/CPE-1599), instead of either vanishing behind
        // a transient toast or closing outright while the user still believes the promised recovery net
        // exists. "Done" finishes the flow.
        completed = report;
      } else {
        // Nothing skipped and no checkpoint problem (the branch above already held the dialog open for
        // any of those) — both lists are guaranteed empty here, but still passed explicitly so the parent
        // always receives the same event shape regardless of which path dispatched it.
        dispatch("apply", { report, checkpointFailures, checkpointPartial });
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
    // CPE-1590/CPE-1599: carry any checkpoint failures/partials out with the report. The in-dialog warning
    // only reaches a user who is looking at the screen when the run ends and doesn't reflexively dismiss
    // it; the parent folds this into the app-level notice so the warning survives every dismissal path.
    if (report) dispatch("apply", { report, checkpointFailures, checkpointPartial });
  }

  /** Back out of the (CPE-1590) overwrite confirm panel without touching anything — the dialog itself
   *  stays open on the op list/preview exactly as it was. */
  function cancelOverwriteConfirm() {
    showOverwriteConfirm = false;
  }

  function cancel() {
    if (applying) return; // no mid-run cancel in v1 — let it finish rather than leaving a half-applied job
    detachChannel();
    // CPE-1590/CPE-1599: Escape and the backdrop click both land here, including while a checkpoint
    // warning is showing — so carry both lists out rather than letting a habitual keystroke discard them
    // unread.
    dispatch("cancel", { checkpointFailures, checkpointPartial });
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

      {#if opKind === "rename" && renameTemplate.trim() && templateEscapesDirectory(renameTemplate.trim())}
        <!-- CPE-1623: told before they click — "+ Add" above is already disabled via `pendingOp`, this
             names WHY so the user isn't left guessing at a silently-disabled button. -->
        <div class="overwrite-hint" data-testid="rename-escape-hint">{$t("bm.renameEscapes")}</div>
      {/if}
      {#if opKind === "convert" && convertExt.trim().replace(/^\.+/, "") && templateEscapesDirectory(convertExt.trim().replace(/^\.+/, ""))}
        <!-- CPE-1623 follow-up: same rule, same warning shape, for the Convert extension field — it feeds
             the exact same joined output path a Rename template does, and previously had no field-level
             warning at all even though the backend now rejects it too. -->
        <div class="overwrite-hint" data-testid="convert-escape-hint">{$t("bm.convertEscapes")}</div>
      {/if}

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
          Flip, Convert, and Rename usually produce a differently-named file too — though a narrow edge
          case (Convert to the extension a file already has, or Rename left at its default
          "{RENAME_DEFAULT_TEMPLATE}" template) can still land on the same name. Either way, the plan
          preview below always shows the real planned path, and Apply always confirms first whenever it
          would actually overwrite something.
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

    {#if checkpointFailures.length > 0}
      <!-- CPE-1590 code-review follow-up, kept BLUNT on purpose (CPE-1599 UAT round): this is the "zero
           protection" case — the confirm panel promises a pre-overwrite checkpoint as the recovery net
           for a write that has no other one, and here that checkpoint was never taken at all. Softening
           this copy to match the partial-checkpoint case below would understate real risk on the most
           destructive operation in the app, so it stays unambiguous: no checkpoint, no net, own backup
           only. Holds the dialog open and names exactly which folder(s) this applies to. -->
      <div class="checkpoint-warn checkpoint-warn-failed" data-testid="checkpoint-warning">
        <strong>No checkpoint was taken</strong> for {checkpointFailures.length}
        folder{checkpointFailures.length === 1 ? "" : "s"} before the overwrite — the write went ahead (you
        already confirmed it), but there is nothing to revert to for:
        <ul class="checkpoint-warn-list">
          {#each checkpointFailures as dir}
            <li title={dir}>{baseName(dir) || dir}</li>
          {/each}
        </ul>
        Your only recovery for files in {checkpointFailures.length === 1 ? "that folder" : "those folders"}
        now is your own backup.
      </div>
    {/if}

    {#if checkpointPartial.length > 0}
      <!-- CPE-1599 UAT follow-up: deliberately SEPARATE from `checkpoint-warn-failed` above and worded
           less alarmingly — a checkpoint WAS taken here, it just doesn't cover every file about to be
           overwritten. True and never overstates protection, but merging this with an outright "no
           checkpoint at all" folder would make zero protection read as a minor gap, which is the one
           thing this warning exists to prevent. -->
      <div class="checkpoint-warn checkpoint-warn-partial" data-testid="checkpoint-warning-partial">
        <strong>The checkpoint didn't fully cover</strong> {checkpointPartial.length}
        folder{checkpointPartial.length === 1 ? "" : "s"} before the overwrite — the write went ahead (you
        already confirmed it), and recovery is incomplete for:
        <ul class="checkpoint-warn-list">
          {#each checkpointPartial as p}
            <li title={p.dir}>
              {baseName(p.dir) || p.dir} — {p.skippedCount} file{p.skippedCount === 1 ? "" : "s"} not captured
            </li>
          {/each}
        </ul>
        Everything else in {checkpointPartial.length === 1 ? "that folder" : "those folders"} IS covered by
        the checkpoint; only the file(s) listed above would need your own backup instead.
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
          <strong>not on the Undo (Ctrl+Z) stack</strong>. The app will attempt to checkpoint the affected
          folder{uniqueParentDirs(overwriteItems.map((it) => it.input)).length === 1 ? "" : "s"} first as a
          recovery net — if that fails you'll see a clear warning naming which folder(s) it didn't cover,
          rather than it failing silently — but the write itself proceeds either way once you confirm, so
          the only guaranteed way back is your own backup.
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

  /* CPE-1983 — one stable height (CPE-1968's decision, reused). The output-name preview re-renders as
     the format/quality/naming controls above it change, so under a centred dialog those controls
     moved out from under the pointer mid-adjustment. `px` rather than `vh`: a fixed-pitch list whose
     rows do not scale with the window. 200px is exactly the old cap. */
  .preview {
    height: 200px;
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
  .rowp .to { color: var(--accent-text); }
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
  .btn.primary.danger { background: var(--danger-fill); border-color: var(--danger-fill); }
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

  /* CPE-1590 code-review follow-up: a failed pre-overwrite checkpoint must be as visible as the promise
     of it was — same danger-toned treatment as the confirm panel, shown in the post-run results area. */
  .checkpoint-warn {
    margin-bottom: 14px;
    border: 1px solid var(--danger);
    border-radius: var(--radius);
    background: var(--surface-alt);
    padding: 8px 10px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text);
  }
  .checkpoint-warn-list {
    margin: 4px 0;
    padding-left: 18px;
  }
  .checkpoint-warn-list li { color: var(--text); font-weight: 500; }

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
