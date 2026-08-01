<script lang="ts">
  /**
   * New Link… dialog (CPE-1207, epic CPE-715): create a symlink or hardlink inside `targetDir`. Modelled
   * on `PasswordPromptDialog.svelte`'s small-input-modal shape; App.svelte owns the reload + inline-rename
   * step after `created` fires (`pendingRenamePath`), mirroring `createNewItem`'s create-then-rename flow.
   *
   * This dialog OWNS its backend call (like `DuplicatesDialog`'s `commands.deleteToTrash`) rather than
   * just dispatching a "please create" request, so it can tell a plain validation error (empty field,
   * kept purely local — no need to bother `showNotice`) apart from a genuine backend failure. The
   * backend failure case matters: on Windows, `create_symlink` can fail without Developer Mode / admin
   * privilege, and that error must never be swallowed ([[avoid-modal-permission-popups]] — surfaced as
   * plain text, not an elevation modal). It's shown inline AND dispatched as `error` so the caller
   * (App.svelte) can also route it through the app-wide `showNotice` toast, same as every other
   * filesystem-op failure.
   */
  import { createEventDispatcher, onMount, tick } from "svelte";
  import { get } from "svelte/store";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import { commands } from "../bindings.gen";
  import { t, translate, locale } from "../i18n";

  /** Folder the new link is created in (CPE-1207 scope: always the folder currently in view — the
   *  empty-area context menu and command palette are the only two openers). */
  export let targetDir: string;

  const dispatch = createEventDispatcher<{ created: string; error: string; close: void }>();

  /** Windows-only detection (CPE-1210) for the "Junction" kind — same `navigator.platform`/`userAgent`
   *  sniff `ShellIntegration.svelte` uses for its Windows-only shell-integration row, so the two stay
   *  consistent rather than inventing a second detection path. */
  const platform = typeof navigator !== "undefined" ? navigator.platform || navigator.userAgent : "";
  const isWindows = /win/i.test(platform);

  let kind: "symlink" | "hardlink" | "junction" = "symlink";
  let target = "";
  let name = "";
  let creating = false;
  let error = "";
  let nameEl: HTMLInputElement;

  onMount(async () => {
    await tick();
    nameEl?.focus();
  });

  /** Native picker for the link's target (CPE-1207, [[path-inputs-need-picker]]) — defaults to a FILE
   *  picker: hardlinks can only ever target a file (no filesystem supports a directory hardlink), and a
   *  file target is by far the common symlink case too. A symlink to a folder can still be typed by hand
   *  into the field below. A junction (CPE-1210) can only ever target a *directory*, so that kind flips
   *  the picker to directory mode instead. */
  async function browseForTarget() {
    try {
      const picked = await openFileDialog({
        directory: kind === "junction",
        multiple: false,
        title: translate(get(locale), "link.browse"),
      });
      if (typeof picked === "string") target = picked;
    } catch {
      // Cancelled or unavailable — leave the current value untouched.
    }
  }

  function joinPath(dir: string, base: string): string {
    const sep = dir.includes("\\") ? "\\" : "/";
    return `${dir.replace(/[\\/]+$/, "")}${sep}${base}`;
  }

  async function create() {
    error = "";
    const loc = get(locale);
    if (!target.trim()) {
      error = translate(loc, "link.targetRequired");
      return;
    }
    if (!name.trim()) {
      error = translate(loc, "link.nameRequired");
      return;
    }
    const linkPath = joinPath(targetDir, name.trim());
    creating = true;
    try {
      const res = kind === "symlink"
        ? await commands.createSymlink(target.trim(), linkPath)
        : kind === "hardlink"
        ? await commands.createHardLink(target.trim(), linkPath)
        : await commands.createJunction(target.trim(), linkPath);
      if (res.status === "ok") {
        dispatch("created", linkPath);
      } else {
        const message = translate(loc, "link.createFailed", { error: String(res.error) });
        error = message;
        dispatch("error", message);
      }
    } catch (e) {
      const message = translate(loc, "link.createFailed", { error: String(e) });
      error = message;
      dispatch("error", message);
    } finally {
      creating = false;
    }
  }

  function onFieldKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") create();
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label={$t("link.newLinkTitle")} on:click|stopPropagation>
    <h2>{$t("link.newLinkTitle")}</h2>

    <label class="field-label" for="new-link-kind-field">{$t("link.kindLabel")}</label>
    <select id="new-link-kind-field" class="field" bind:value={kind} data-testid="new-link-kind">
      <option value="symlink">{$t("link.kindSymlink")}</option>
      <option value="hardlink">{$t("link.kindHardlink")}</option>
      {#if isWindows}
        <option value="junction">{$t("link.kindJunction")}</option>
      {/if}
    </select>

    <label class="field-label" for="new-link-target-field">{$t("link.targetLabel")}</label>
    <div class="row-input">
      <input
        id="new-link-target-field"
        class="field"
        type="text"
        bind:value={target}
        on:keydown={onFieldKeydown}
        data-testid="new-link-target"
      />
      <button class="btn" type="button" on:click={browseForTarget} data-testid="new-link-browse">
        {$t("link.browse")}
      </button>
    </div>
    {#if kind === "junction"}
      <div class="hint" data-testid="new-link-junction-hint">{$t("link.junctionTargetHint")}</div>
    {/if}

    <label class="field-label" for="new-link-name-field">{$t("link.nameLabel")}</label>
    <input
      id="new-link-name-field"
      class="field"
      type="text"
      bind:this={nameEl}
      bind:value={name}
      on:keydown={onFieldKeydown}
      data-testid="new-link-name"
    />

    {#if error}<div class="err" data-testid="new-link-error">{error}</div>{/if}

    <div class="actions">
      <button class="btn" data-testid="new-link-cancel" on:click={() => dispatch("close")}>
        {$t("common.cancel")}
      </button>
      <button class="btn primary" disabled={creating} data-testid="new-link-create" on:click={create}>
        {$t("link.create")}
      </button>
    </div>
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
    width: 420px;
    max-width: 90vw;
    background: var(--surface);
    border: 1px solid var(--dialog-border);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 { font-size: 16px; margin-bottom: 14px; }
  .field-label {
    display: block;
    margin: 10px 0 4px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .field {
    width: 100%;
    height: 32px;
    padding: 0 8px;
    font: inherit;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-sizing: border-box;
  }
  .row-input {
    display: flex;
    gap: 6px;
  }
  .row-input .field { flex: 1; }
  .hint {
    margin-top: 4px;
    font-size: 11.5px;
    color: var(--text-dim);
  }
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
