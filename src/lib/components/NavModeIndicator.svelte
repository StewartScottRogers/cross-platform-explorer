<script lang="ts">
  /**
   * Navigation Mode's on-screen mode badge (CPE-1555, epic CPE-1487). Purely presentational: given
   * the current `NavMode` (+ optional pending chord/count buffers from `NavState`, CPE-1552's
   * `navMode.ts`), renders a small fixed pill so a user in the modal vim-style layer always knows
   * whether they're in NORMAL or VISUAL mode, and sees a buffered `g` / digit count before it
   * resolves into a motion.
   *
   * INERT: nothing mounts this yet — `App.svelte` is untouched. CPE-1556 wires a real `NavState`
   * into this component once the reducer is hooked into key handling.
   *
   * Colours reuse existing semantic tokens only (`var(--accent)` background, `var(--surface)`
   * foreground) — the same "filled pill" pairing already used for on-accent text elsewhere
   * (e.g. ConsentSheet's `.badge.granted`, `app.css`'s `.pill.active`), just built from tokens
   * instead of a literal white so it stays correct across light/dark/hc-light/hc-dark without a
   * new token pair. `--accent`/`--surface` clears >=5.4:1 in light+dark and >=7.7:1 in both hc
   * palettes (checked against `src/app.css`'s current values before picking this pairing).
   * `pointer-events: none` — it's a status readout, never a click target, so it never steals a
   * click from whatever's under it in the corner.
   */
  import type { NavMode } from "../navMode";

  /** The active mode — drives the NORMAL/VISUAL label. */
  export let mode: NavMode;
  /** Buffered numeric prefix from `NavState.pendingCount` (e.g. `"3"` before `j`), if any. */
  export let pendingCount = "";
  /** Buffered chord prefix from `NavState.pendingChord` (currently only `"g"`, awaiting `gg`), if any. */
  export let pendingChord = "";

  $: label = mode === "visual" ? "VISUAL" : "NORMAL";
  $: pending = pendingChord + pendingCount;
</script>

<div class="nav-mode-indicator" class:visual={mode === "visual"} data-testid="nav-mode-indicator">
  <span class="label" data-testid="nav-mode-label">{label}</span>
  {#if pending}
    <span class="pending" data-testid="nav-mode-pending">{pending}</span>
  {/if}
</div>

<style>
  .nav-mode-indicator {
    position: fixed;
    right: 10px;
    bottom: 10px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--surface);
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    white-space: nowrap; /* tick-tack: badge text stays on one line */
    flex: 0 0 auto;
    z-index: 50;
    pointer-events: none;
  }
  .pending {
    font-variant-numeric: tabular-nums;
    opacity: 0.85;
  }
</style>
