<script lang="ts">
  /**
   * "Unlocked vault" browsing banner (CPE-1249, epic CPE-738).
   *
   * Shown while the explorer is navigated INSIDE an unlocked vault's session directory (App.svelte derives
   * this from `vaultOfSessionPath` over the `vaults` store). It makes the mount state unmistakable — the
   * plaintext you're browsing lives in a temporary decrypted session dir — and offers the one-click Lock
   * that re-seals it: since CPE-1645 that genuinely means "encrypt what's here back into the vault file,
   * then wipe this copy" (App navigates out first). Theme variables only, clearly bordered.
   */
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import { displaySafeName } from "../filename";

  /** Friendly vault name (blob base name minus `.cpevault`). */
  export let name: string;

  /** A lock for this vault is already in flight (SEC-847 reviewer blocker A). Locking re-seals the whole
   *  tree, so it is slow by design and this banner stays mounted throughout — without disabling the
   *  button, a second click started a second lock, and the two interleaved: one sealed the tree the other
   *  had already half-shredded, over the vault, with both reporting success. */
  export let locking = false;

  const dispatch = createEventDispatcher<{ lock: void }>();
</script>

<div class="vault-banner" role="status" data-testid="vault-banner">
  <span class="vb-icon" aria-hidden="true"><Icon name="lock-open" size={15} /></span>
  <span class="vb-text"><strong data-testid="vault-banner-name">{displaySafeName(name)}</strong> — unlocked</span>
  <button
    class="vb-lock"
    data-testid="vault-lock"
    disabled={locking}
    aria-busy={locking}
    title={locking ? $t("vault.lockingTitle") : $t("vault.lockTitle")}
    on:click={() => dispatch("lock")}
  >
    <Icon name="lock" size={13} />
    <span>{locking ? $t("vault.locking") : $t("vault.lock")}</span>
  </button>
</div>

<style>
  /* Accent-tinted strip with a clearly-visible border (not just a shadow — [[dialogs-need-visible-border]]
     spirit for prominent surfaces), theme colours only. Reflows nothing (single-line row of items). */
  .vault-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    /* Own the shrink contract fully so the row can never overflow its pane: the banner itself shrinks
       (`min-width: 0`), the name text truncates (`.vb-text` below), and the Lock button holds its size
       (`flex: 0 0 auto`) — so the control always stays inside the visible pane at any width. */
    min-width: 0;
    max-width: 100%;
    box-sizing: border-box;
    padding: 6px 10px;
    background: color-mix(in srgb, var(--accent) 10%, var(--surface));
    border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--border));
    border-radius: var(--radius);
    margin: 6px 8px 0;
    font-size: 12.5px;
    color: var(--text);
  }
  .vb-icon {
    display: grid;
    place-items: center;
    color: var(--accent);
    flex: 0 0 auto;
  }
  .vb-text {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
  }
  .vb-lock {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
    cursor: pointer;
  }
  /* Disabled while a lock is in flight (SEC-847 blocker A) — dimmed + not-allowed, theme tokens only, so
     the state reads the same in light and dark. `:hover` below is scoped past this so a disabled button
     does not light up under the pointer. */
  .vb-lock:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .vb-lock:not(:disabled):hover {
    background: var(--surface);
  }
</style>
