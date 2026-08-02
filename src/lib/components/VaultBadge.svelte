<script lang="ts">
  /**
   * Locked/unlocked indicator badge for a `.cpevault` FileList row (CPE-1249, epic CPE-738).
   *
   * Mounted ONLY for rows whose entry is a `.cpevault` (by extension — cheap, see App.svelte's activation
   * which confirms via `vault_is` before actually unlocking), so a folder with no vaults renders zero of
   * these and the hot listing path is untouched (PURPOSE.md). Mirrors the sibling row badge pattern
   * (LinkBadge.svelte): a small fixed-size monochrome pill that never competes with the name for space
   * (tick-tacks convention — fixed size, never grows/shrinks).
   *
   * The state comes purely from the reactive `vaults` store (vaultStore.ts): a locked vault shows the
   * padlock glyph; while it is unlocked (browsable) it flips to the open-padlock glyph in the accent
   * colour. Theme variables only — no hard-coded colours (light-theme-only app).
   */
  import Icon from "./Icon.svelte";
  import { vaults, badgeFor } from "../vaultStore";

  /** Absolute path of the `.cpevault` entry. */
  export let path: string;

  $: state = badgeFor($vaults, path);
  $: unlocked = state === "unlocked";
  $: title = unlocked
    ? "Encrypted vault — unlocked (use the banner's Lock button to re-seal)"
    : "Encrypted vault — double-click or press Enter to unlock";
</script>

<span
  class="vault-badge"
  class:unlocked
  data-testid="vault-badge"
  data-vault-state={state}
  {title}
  aria-label={title}
>
  <Icon name={unlocked ? "lock-open" : "lock"} size={12} />
</span>

<style>
  /* Same shape/position/sizing as the other inline row badges (LinkBadge / tag chips) — fixed size,
     never grows or shrinks, never wraps (tick-tacks convention, see CLAUDE.md "Pills / chips / badges"). */
  .vault-badge {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-left: 6px;
    width: 18px;
    height: 18px;
    border-radius: 999px;
    color: var(--text-dim);
    background: var(--surface-alt);
    border: 1px solid var(--border);
  }
  /* Unlocked: a distinct, unmistakably "open" treatment — same shape/position so it never jumps the row
     layout, just recoloured to the accent (mirrors LinkBadge's broken-state recolour approach). */
  .vault-badge.unlocked {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-color: color-mix(in srgb, var(--accent) 40%, transparent);
  }
</style>
