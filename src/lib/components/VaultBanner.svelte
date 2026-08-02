<script lang="ts">
  /**
   * "Unlocked vault" browsing banner (CPE-1249, epic CPE-738).
   *
   * Shown while the explorer is navigated INSIDE an unlocked vault's session directory (App.svelte derives
   * this from `vaultOfSessionPath` over the `vaults` store). It makes the mount state unmistakable — the
   * plaintext you're browsing lives in a temporary decrypted session dir — and offers the one-click Lock
   * that re-seals it (App navigates out first, then wipes it). Theme variables only, clearly bordered.
   */
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";

  /** Friendly vault name (blob base name minus `.cpevault`). */
  export let name: string;

  const dispatch = createEventDispatcher<{ lock: void }>();
</script>

<div class="vault-banner" role="status" data-testid="vault-banner">
  <span class="vb-icon" aria-hidden="true"><Icon name="lock-open" size={15} /></span>
  <span class="vb-text"><strong data-testid="vault-banner-name">{name}</strong> — unlocked</span>
  <button class="vb-lock" data-testid="vault-lock" on:click={() => dispatch("lock")}>
    <Icon name="lock" size={13} />
    <span>Lock</span>
  </button>
</div>

<style>
  /* Accent-tinted strip with a clearly-visible border (not just a shadow — [[dialogs-need-visible-border]]
     spirit for prominent surfaces), theme colours only. Reflows nothing (single-line row of items). */
  .vault-banner {
    display: flex;
    align-items: center;
    gap: 8px;
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
  .vb-lock:hover {
    background: var(--surface);
  }
</style>
