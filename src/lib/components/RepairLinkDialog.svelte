<script lang="ts">
  /**
   * Repair link… dialog (CPE-1209, epic CPE-715): re-point a broken symlink at a found target.
   *
   * Owns its own backend calls, same reasoning as `NewLinkDialog` (CPE-1207): `commands.suggestRepair`
   * runs on mount (bounded-depth search under `searchRoots`, CPE-1027), and "Accept" re-creates the
   * symlink to the chosen path. `create_symlink` refuses to overwrite an existing path (it's a plain
   * `std::os::*::symlink` call), so replacing the broken link is delete-then-recreate — `deletePermanent`
   * unlinks ONLY the dead symlink entry itself (Rust's `remove_file` never follows a symlink, and a
   * broken link's own `is_dir()` is false since its target can't be stat'd), never anything it used to
   * point at. That two-step replace is exactly why the ticket calls for confirming first: an inline
   * confirm step gates the delete+recreate, matching `ConfirmDialog`'s "explicit yes before anything
   * destructive" convention without pulling in a second nested dialog component.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { get } from "svelte/store";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import { commands } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import { t, translate, locale } from "../i18n";

  /** Full path of the broken symlink being repaired. */
  export let linkPath: string;
  /** Display name (basename) for the dialog title/body. */
  export let linkName: string;
  /** Folders to search for a same-named replacement (CPE-1027) — in order, each a bounded-depth walk. */
  export let searchRoots: string[] = [];

  const dispatch = createEventDispatcher<{ repaired: string; error: string; close: void }>();

  type Phase = "loading" | "ready" | "confirming" | "repairing";
  let phase: Phase = "loading";
  let chosenTarget: string | null = null;
  let error = "";

  onMount(async () => {
    try {
      chosenTarget = await commands.suggestRepair(linkPath, searchRoots);
    } catch {
      chosenTarget = null; // never block the dialog on a failed lookup — just show "no suggestion"
    }
    phase = "ready";
  });

  async function browseForAnother() {
    try {
      const picked = await openFileDialog({ directory: false, multiple: false, title: translate(get(locale), "link.repairBrowse") });
      if (typeof picked === "string") chosenTarget = picked;
    } catch {
      // Cancelled or unavailable — leave the current choice untouched.
    }
  }

  function accept() {
    if (chosenTarget) phase = "confirming";
  }

  async function confirmReplace() {
    if (!chosenTarget) return;
    const loc = get(locale);
    phase = "repairing";
    error = "";
    try {
      // Re-verify the link is STILL broken right before the destructive delete. NOT because
      // remove_dir_all would recurse into and destroy a real target directory if a directory-symlink's
      // target reappeared — it wouldn't: on Windows, Rust's remove_dir_all special-cases a reparse point
      // (symlink or NTFS junction) and only unlinks the link entry itself, leaving the target directory
      // and its files intact (confirmed by the PR #844 security audit — CPE-1666 — which planted a
      // junction at a delete target and watched exactly that happen, same for a directory symlink). The
      // actual reason to abort is narrower: if the link's target reappeared since the context menu's
      // broken-check, this delete would remove a link that now correctly resolves to something real —
      // not what "repair a broken link" was asked to do. Abort instead of deleting.
      const recheck = await commands.linkStatus(linkPath);
      if (!recheck.broken) {
        throw new Error("This link is no longer broken — refresh the folder and try again.");
      }
      // `confirmed: true` (CPE-1651) is set only here, on the far side of the `phase === "confirming"`
      // step the user has to press through — `accept()` merely arms the dialog; this function is what
      // the confirm button runs. The backend refuses the call outright without it.
      const delRes = unwrap(await commands.deletePermanent([linkPath], true));
      const delFailed = delRes.find((r) => !r.ok);
      if (delFailed) {
        throw new Error(delFailed.error);
      }
      const res = await commands.createSymlink(chosenTarget, linkPath);
      if (res.status === "ok") {
        dispatch("repaired", chosenTarget);
      } else {
        throw new Error(String(res.error));
      }
    } catch (e) {
      const message = translate(loc, "link.repairFailed", { error: String((e as Error)?.message ?? e) });
      error = message;
      dispatch("error", message);
      phase = "ready";
    }
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label={$t("link.repairTitle")} on:click|stopPropagation>
    <h2>{$t("link.repairTitle")}</h2>
    <p class="intro">{$t("link.repairIntro")} <strong>{linkName}</strong></p>

    {#if phase === "loading"}
      <p class="dim" data-testid="repair-loading">{$t("link.repairLoading")}</p>
    {:else}
      {#if chosenTarget}
        <div class="suggestion" data-testid="repair-suggestion">
          <span class="field-label">{$t("link.repairSuggestionLabel")}</span>
          <span class="target-path">{chosenTarget}</span>
        </div>
      {:else}
        <p class="dim" data-testid="repair-no-suggestion">{$t("link.repairNoSuggestion")}</p>
      {/if}

      {#if phase === "confirming"}
        <p class="confirm-text" data-testid="repair-confirm-text">
          {translate($locale, "link.repairConfirm", { target: chosenTarget ?? "" })}
        </p>
        <div class="actions">
          <button class="btn" data-testid="repair-cancel" on:click={() => (phase = "ready")}>
            {$t("common.cancel")}
          </button>
          <button class="btn primary" data-testid="repair-confirm-yes" on:click={confirmReplace}>
            {$t("link.repairConfirmYes")}
          </button>
        </div>
      {:else}
        {#if error}<div class="err" data-testid="repair-error">{error}</div>{/if}
        <div class="actions">
          <button class="btn" data-testid="repair-close" on:click={() => dispatch("close")}>
            {$t("common.close")}
          </button>
          <button class="btn" data-testid="repair-browse" on:click={browseForAnother} disabled={phase === "repairing"}>
            {$t("link.repairBrowse")}
          </button>
          {#if chosenTarget}
            <button
              class="btn primary"
              data-testid="repair-accept"
              disabled={phase === "repairing"}
              on:click={accept}>{$t("link.repairAccept")}</button>
          {/if}
        </div>
      {/if}
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
    width: 460px;
    max-width: 90vw;
    background: var(--surface);
    border: 1px solid var(--dialog-border);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 { font-size: 16px; margin-bottom: 10px; }
  .intro { color: var(--text-dim); margin-bottom: 12px; line-height: 1.5; font-size: 12.5px; }
  .dim { color: var(--text-dim); font-size: 12.5px; }
  .field-label {
    display: block;
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 4px;
  }
  .suggestion {
    background: var(--surface-alt);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px;
  }
  .target-path {
    display: block;
    font-family: monospace;
    font-size: 12.5px;
    color: var(--text);
    word-break: break-all;
  }
  .confirm-text { margin-top: 12px; font-size: 12.5px; color: var(--text); line-height: 1.5; }
  .err {
    margin-top: 10px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text);
  }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  .btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .btn.primary:hover { background: var(--accent-hover); }
  .btn:disabled { opacity: 0.6; }
</style>
