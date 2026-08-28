<script lang="ts">
  /**
   * Press-to-set hotkey capture control (CPE-1549, epic CPE-1484 "hotkey customization"). A small,
   * self-contained, reusable button — no knowledge of the keymap registry or conflict detection,
   * just "show a chord, let the user press a new one." `KeyboardBindingsDialog.svelte` is the only
   * caller today; it owns `setChord`/`findConflicts`/`saveKeymap` and everything keymap-shaped.
   *
   * Click (or Enter/Space via native button activation) arms capture mode. A
   * `<svelte:window on:keydown|capture>` listener is mounted at all times (Svelte requires
   * `<svelte:window>` at the component's top level, not inside a block) but no-ops unless
   * `armed` — so it's only ACTIVE while armed. Once armed, the next real keystroke is
   * intercepted: `preventDefault`/`stopPropagation` so it never reaches the app underneath (no
   * typing leaks through, no built-in shortcut fires), `Escape`
   * cancels without emitting anything, a bare modifier key (Ctrl/Alt/Shift/Meta) is ignored so
   * capture keeps waiting for the actual key, and anything else is run through `hotkeyFromEvent`
   * (the same strict, Ctrl/Alt-required normalization `keymap.ts`'s `setChord` already enforces).
   * A chord that normalizes commits: capture disarms and a `set` event carries it. A chord that
   * `hotkeyFromEvent` rejects (e.g. a bare letter with no qualifying modifier) does NOT commit —
   * capture stays armed and a short inline hint explains why, so the user can just press the real
   * combo without re-clicking.
   */
  import { createEventDispatcher } from "svelte";
  import { hotkeyFromEvent } from "../macroBindings";

  /** Current chord to show at rest, already in whatever display form the caller wants (raw
   *  canonical form or a formatted glyph string) — this component never formats or interprets it,
   *  just renders it or falls back to "Click to set…" when empty. */
  export let display = "";
  export let testId = "";
  export let disabled = false;

  // `armchange` lets a host dialog with its own window-level Escape handler (e.g.
  // `KeyboardBindingsDialog`'s close-on-Escape) suppress itself while this control is armed, so
  // cancelling a capture with Escape doesn't ALSO close the dialog underneath it.
  const dispatch = createEventDispatcher<{ set: string; armchange: boolean }>();

  const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta", "OS", "AltGraph"]);

  let armed = false;
  let rejected = false;

  function setArmed(next: boolean) {
    armed = next;
    dispatch("armchange", next);
  }

  function arm() {
    if (disabled) return;
    setArmed(true);
    rejected = false;
  }

  function cancel() {
    setArmed(false);
    rejected = false;
  }

  function onKeydown(e: KeyboardEvent) {
    if (!armed) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      cancel();
      return;
    }
    if (MODIFIER_KEYS.has(e.key)) {
      // Modifier-only press — keep waiting for the real key.
      return;
    }
    const chord = hotkeyFromEvent(e);
    if (!chord) {
      // Rejected by the strict Ctrl/Alt-required rule (e.g. a bare letter) — stay armed, no `set`.
      rejected = true;
      return;
    }
    setArmed(false);
    rejected = false;
    dispatch("set", chord);
  }
</script>

<!-- Always mounted (svelte:window must live at the top level); onKeydown no-ops unless armed. -->
<svelte:window on:keydown|capture={onKeydown} />

<button
  type="button"
  class="capture"
  class:armed
  class:rejected
  {disabled}
  data-testid={testId || undefined}
  aria-label={armed ? "Press a key to set the shortcut, or Escape to cancel" : `Current shortcut: ${display || "unset"}. Click to change.`}
  on:click={arm}
>
  {#if armed}
    {rejected ? "Needs Ctrl or Alt…" : "Press a key…"}
  {:else}
    {display || "Click to set…"}
  {/if}
</button>

<style>
  .capture {
    min-width: 96px;
    height: 24px;
    padding: 0 8px;
    font-family: ui-monospace, monospace;
    font-size: 11.5px;
    color: var(--text);
    background: var(--surface-alt);
    border: 1px solid var(--border-strong);
    border-radius: 5px;
    text-align: center;
    white-space: nowrap;
    cursor: pointer;
  }
  .capture:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .capture:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .capture.armed {
    color: var(--accent-text);
    border-color: var(--accent);
    border-style: dashed;
    font-style: normal;
  }
  .capture.rejected {
    color: var(--danger);
    border-color: var(--danger);
  }
</style>
