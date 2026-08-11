<script lang="ts">
  /**
   * Archive safety check (CPE-1318, epic CPE-1002) — surfaces the built-but-unwired
   * `analyze_archive_safety` backend command (CPE-1281/1287) behind a right-click "Check archive
   * safety…" action on archive files. A single plain (non-streaming) call scores the archive's
   * compression ratio for zip-bomb-like expansion risk; this dialog shows the overall ratio, the
   * compressed→uncompressed size, every flagged entry, and a clear DANGER indicator when the backend's
   * `report.dangerous` flag trips.
   *
   * Modeled on {@link NearDuplicatesDialog}'s shell (header, close, dim/error states) but simpler: this
   * is a single scan with no follow-up action (read-only, like the File Health tabs) so it runs
   * automatically on mount rather than waiting for an explicit "Scan" click — the ticket's whole point
   * is a one-click "what's in this archive" answer from the context menu.
   *
   * Two "couldn't actually scan this" states, both deliberately NEVER falling through to the plain safe
   * banner: `result.unreadable` (CPE-1320) means the archive itself couldn't be opened at all (corrupt,
   * not a zip); `result.unreadable_entries > 0` (CPE-1591, widened by CPE-1602) means the archive opened
   * fine but one or more entries inside it couldn't be read — either because they're password-protected
   * (this dialog has no password prompt) or because an entry looked suspicious and its bounded
   * verification ran out of budget before reaching a verdict. The backend doesn't currently distinguish
   * *which* of those happened (CPE-1612), so the copy below (`arcsafe.encrypted`) names both possibilities
   * rather than asserting encryption. A zero-entries password-protected zip used to collapse to the same
   * `entries_scanned: 0, dangerous: false` shape as a genuinely safe empty archive; `unreadable_entries`
   * makes that case structurally distinct so it can never render as "No zip-bomb risk detected" again.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "../invoke"; // busy-cursor wrapper (BUSY-CURSOR.md) — never raw @tauri-apps/api/core
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import { baseName } from "../contentSearch";
  import { formatSize } from "../format";
  import type { ArchiveSafetyReport } from "../bindings.gen";

  /** The archive file's full path — the single selected entry the context menu gated on. */
  export let path = "";

  const dispatch = createEventDispatcher<{ close: void }>();

  let loading = true;
  let error = "";
  let result: ArchiveSafetyReport | null = null;

  /** Non-zero-friendly size formatter — {@link formatSize} returns "" for 0 bytes (fine for a directory
   *  listing column, wrong here: an empty/corrupt archive's "0 B → 0 B" should stay legible). */
  function sizeLabel(bytes: number): string {
    return bytes > 0 ? formatSize(bytes) : "0 B";
  }

  function ratioLabel(r: number): string {
    return `${r.toFixed(1)}x`;
  }

  async function run() {
    loading = true;
    error = "";
    result = null;
    try {
      result = await invoke<ArchiveSafetyReport>("analyze_archive_safety", { path });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(run);
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label={$t("arcsafe.title")} on:click|stopPropagation>
    <header>
      <Icon name="archive" size={16} />
      <h2>{$t("arcsafe.title")}</h2>
      <span class="root" title={path}>{baseName(path)}</span>
      <button class="x" data-testid="as-close-btn" title={$t("common.close")} on:click={() => dispatch("close")}>
        <Icon name="close" size={14} />
      </button>
    </header>

    {#if loading}
      <p class="dim" data-testid="as-loading">{$t("arcsafe.scanning")}</p>
    {:else if error}
      <p class="err" data-testid="as-error">{error}</p>
      <button class="mini" data-testid="as-retry-btn" on:click={run}>{$t("arcsafe.retry")}</button>
    {:else if result?.unreadable}
      <!-- CPE-1320: a corrupt/unreadable ZIP must NEVER render as "No zip-bomb risk" — before this
           fix `analyze_archive_safety` collapsed a genuinely-unscanned archive to the same
           zero-entries/not-dangerous shape as a valid empty one, so a corrupt file silently read as
           safe. `result.unreadable` distinguishes the two; this is a dedicated unknown/error state,
           styled like `.err` (never the safe banner). -->
      <p class="err" data-testid="as-unreadable">{$t("arcsafe.unreadable")}</p>
    {:else if result && !result.report.dangerous && result.unreadable_entries > 0}
      <!-- CPE-1591: the archive itself opened fine (unlike the CPE-1320 case above), but one or more
           entries couldn't be read — either password-protected, or (CPE-1602) suspicious-looking entries
           whose bounded verification ran out of budget; see `arcsafe.encrypted` (CPE-1612), which names
           both rather than assuming encryption. Those entries were never sized or scored, so
           `report.dangerous === false` here means "we don't know", not "safe" — before this fix that shape
           rendered the same reassuring safe banner as a fully-scanned clean archive. Only gated when
           nothing dangerous was already found among whatever WAS readable: a real danger signal from the
           readable portion still takes priority (below) rather than being hidden behind "couldn't fully
           assess". -->
      <p class="err" data-testid="as-encrypted">{$t("arcsafe.encrypted", { count: result.unreadable_entries })}</p>
    {:else if result}
      {#if result.report.dangerous}
        <!-- DANGER treatment: --danger is an app-wide THEME variable (see app.css :root), same token
             CheckpointDialog/ConfirmDialog/ShredConfirmDialog use for their destructive confirms — never
             a hard-coded red (MENUS.md). -->
        <div class="banner danger" data-testid="as-danger">
          <Icon name="ban" size={16} />
          <span>{$t("arcsafe.dangerous")}</span>
        </div>
        {#if result.unreadable_entries > 0}
          <!-- Mixed archive: danger was already found among the readable entries, so that verdict still
               leads — but there were ALSO entries this scan couldn't read, so say so rather than implying
               a complete scan. -->
          <p class="dim" data-testid="as-partial-note">{$t("arcsafe.encrypted", { count: result.unreadable_entries })}</p>
        {/if}
      {:else}
        <div class="banner safe" data-testid="as-safe">
          <Icon name="check" size={16} />
          <span>{$t("arcsafe.safe")}</span>
        </div>
      {/if}

      <dl class="stats">
        <dt>{$t("arcsafe.ratio")}</dt>
        <dd data-testid="as-ratio">{ratioLabel(result.report.overall_ratio)}</dd>
        <dt>{$t("arcsafe.sizes")}</dt>
        <dd data-testid="as-sizes">{sizeLabel(result.report.total_compressed)} → {sizeLabel(result.report.total_uncompressed)}</dd>
        <dt>{$t("arcsafe.entries")}</dt>
        <dd data-testid="as-entries">
          {result.entries_scanned.toLocaleString()}
          {#if result.truncated}<span class="dim"> {$t("arcsafe.capped")}</span>{/if}
        </dd>
        {#if result.unreadable_entries > 0}
          <dt>{$t("arcsafe.unreadableEntries")}</dt>
          <dd data-testid="as-unreadable-entries">{result.unreadable_entries.toLocaleString()}</dd>
        {/if}
      </dl>

      {#if result.report.flagged.length > 0}
        <div class="flagged-head">{$t("arcsafe.flaggedHead", { count: result.report.flagged.length })}</div>
        <!-- Reflow rule (tick-tacks): the container wraps pills onto more rows and grows; each pill
             keeps its own text on one line (never wraps inside the pill). -->
        <div class="pills" data-testid="as-flagged">
          {#each result.report.flagged as f (f.name)}
            <span class="pill" title={f.name}>
              <Icon name="archive" size={13} />
              <span class="pname">{f.name}</span>
              <span class="pratio">{ratioLabel(f.ratio)}</span>
            </span>
          {/each}
        </div>
      {:else}
        <p class="dim" data-testid="as-none-flagged">{$t("arcsafe.noneFlagged")}</p>
      {/if}
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog {
    width: 560px; max-width: 94vw; max-height: 82vh; display: flex; flex-direction: column; overflow: auto;
    background: var(--surface); border: 1px solid var(--dialog-border); border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 14px 16px 16px;
  }
  header { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  h2 { font-size: 16px; }
  .root { color: var(--text-dim); font-size: 12px; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .x { width: 28px; height: 28px; display: grid; place-items: center; }
  .dim { color: var(--text-faint); }
  .err { color: var(--danger); }
  .mini { height: 24px; padding: 0 10px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); font-size: 12px; }
  .mini:hover { background: var(--surface); }
  .banner {
    display: flex; align-items: center; gap: 8px; padding: 8px 10px; border-radius: var(--radius);
    font-size: 13px; font-weight: 600; margin-bottom: 10px;
  }
  /* --danger is the app-wide theme token (app.css :root) — never a hard-coded hex, per MENUS.md. */
  .banner.danger { color: var(--danger); border: 1px solid var(--danger); background: color-mix(in srgb, var(--danger) 10%, var(--surface)); }
  .banner.safe { color: var(--text-dim); border: 1px solid var(--border); background: var(--surface-alt); }
  .stats { display: grid; grid-template-columns: auto 1fr; gap: 4px 10px; font-size: 12.5px; margin-bottom: 10px; }
  .stats dt { color: var(--text-faint); }
  .stats dd { color: var(--text); }
  .flagged-head { font-size: 12px; font-weight: 600; margin-bottom: 6px; color: var(--text-dim); }
  /* Reflow container: wraps onto more rows and grows height rather than letting pills shrink. */
  .pills { display: flex; flex-wrap: wrap; gap: 6px; }
  .pill {
    flex: 0 0 auto; max-width: 320px; display: flex; align-items: center; gap: 6px;
    padding: 5px 10px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt);
    white-space: nowrap;
  }
  .pname { font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 200px; }
  .pratio { font-size: 11px; color: var(--danger); font-weight: 600; }
</style>
