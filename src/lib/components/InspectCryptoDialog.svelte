<script lang="ts">
  /**
   * Inspect crypto file… dialog (CPE-1438, epic CPE-1417): the INSPECT side of certificate management
   * made to work in dual-pane mode. In single-pane the "Inspect" / "Inspect JWT" context-menu action
   * decodes the selected cert/CSR/JWT file inline in the preview pane (CPE-1422/1424); but in dual-pane
   * that preview slot is occupied by pane B's ExplorerPane, so the action was a silent no-op (the bug).
   *
   * This overlay reuses the SAME viewer components the preview pane hosts — JwtPreview / CertPreview —
   * inside a centered dialog shell, the same "a modal works in dual-pane" pattern CreateCertDialog /
   * SignCertDialog already use. The viewers are self-contained: each fetches its own decode from `path`
   * via the `jwt_preview` / `cert_decode` backend commands, so this shell only routes the file kind to
   * the right component. Read-only, like the inline previews — no signature/trust verification.
   *
   * Dialog conventions (CLAUDE.md): visible border (`--dialog-border`), Esc + click-outside to close,
   * focus moved to the dialog on mount, light-theme variables throughout.
   */
  import { createEventDispatcher, onMount, tick } from "svelte";
  import Icon from "./Icon.svelte";
  import JwtPreview from "./JwtPreview.svelte";
  import CertPreview from "./CertPreview.svelte";
  import { displaySafeName, displaySafePath } from "../filename";

  /** Full path of the cert/CSR/JWT file to inspect. */
  export let path: string;
  /** Which viewer to mount — `"jwt"` for a `.jwt`/`.jws` token, `"cert"` for a cert/CSR/key file. */
  export let kind: "jwt" | "cert";

  const dispatch = createEventDispatcher<{ close: void }>();

  /** Basename of the inspected file, separator-agnostic — shown in the dialog title. */
  $: fileName = (() => {
    const idx = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    return idx < 0 ? path : path.slice(idx + 1);
  })();

  let dialogEl: HTMLDivElement;
  onMount(async () => {
    await tick();
    dialogEl?.focus();
  });
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Inspect {displaySafeName(fileName)}"
    tabindex="-1"
    bind:this={dialogEl}
    data-testid="crypto-inspect-dialog"
    on:click|stopPropagation
  >
    <div class="hd">
      <h2>
        <span class="hd-icon"><Icon name={kind === "jwt" ? "lock" : "certificate"} size={18} /></span>
        <span class="hd-title" title={displaySafePath(path)}>{displaySafeName(fileName)}</span>
      </h2>
      <button class="close-x" type="button" aria-label="Close" data-testid="crypto-inspect-close" on:click={() => dispatch("close")}>
        <Icon name="close" size={16} />
      </button>
    </div>

    <div class="body">
      {#if kind === "jwt"}
        <JwtPreview {path} />
      {:else}
        <CertPreview {path} />
      {/if}
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
    width: 560px;
    max-width: 90vw;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--dialog-border);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    outline: none;
  }
  .hd {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 14px 16px 12px;
    border-bottom: 1px solid var(--border);
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 15px;
    min-width: 0;
  }
  .hd-icon { color: var(--text); display: grid; place-items: center; flex: 0 0 auto; }
  .hd-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .close-x {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: var(--radius);
    color: var(--text-dim);
    background: transparent;
    border: 1px solid transparent;
    flex: 0 0 auto;
    cursor: pointer;
  }
  .close-x:hover { background: var(--hover); color: var(--text); }
  .body { overflow-y: auto; }
</style>
