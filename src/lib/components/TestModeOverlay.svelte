<script lang="ts">
  /**
   * Automation test-mode badge (CPE-1046): rendered only when the app was launched with `--test-mode`
   * (the backend injects `window.__CPE_TEST_MODE__` — see App.svelte's synchronous read, mirroring the
   * `--open` mechanism from CPE-1043). Its whole job is to make an automated GUI run unmistakable to a
   * human sharing the screen, without getting in the way of the automation or the human at all:
   *
   * - A SMALL badge pinned to the upper-left corner, sized only to its own content — never a full-window
   *   frame/border and never anything crossing the center of the screen (explicit user requirement).
   * - `pointer-events: none` on the badge and everything inside it, so it can never intercept a click —
   *   the automation's clicks and the user's own clicks (incl. closing the window) pass straight through.
   * - No focusable elements anywhere in this component (no buttons/inputs/links, no `tabindex >= 0`), so
   *   it can never steal keyboard focus either. (The actual OS-level focus-stealing fix — launching the
   *   window itself unfocused — lives in the backend: `lib.rs` calls `.focused(false)` on the
   *   `WebviewWindowBuilder` when `--test-mode` is set. This badge is the visual half only.)
   *
   * Deliberately hardcoded English (not run through the i18n `$t` store): this is a test/dev-only
   * overlay, never seen by a real user in production use, so it's exempt from the 12-locale coverage
   * gate on purpose.
   *
   * Deliberately NOT theme-aware: fixed loud colors (amber/red) instead of `var(--...)` theme tokens,
   * because it is supposed to be jarring and stand out identically in light and dark.
   */
</script>

<!-- Critical inert/position properties are also set inline (redundant with the stylesheet rule below)
     so they hold even if the stylesheet fails to load, and so tests can assert them directly on the
     element without depending on a full CSS cascade. tabindex="-1" belt-and-braces: nothing here is
     interactive, but this guarantees the badge itself is never a tab stop. -->
<div
  class="test-mode-badge"
  role="status"
  aria-hidden="true"
  tabindex="-1"
  style="position: fixed; top: 8px; left: 8px; width: max-content; pointer-events: none;"
>
  <span class="glyph">🤖</span>
  <span class="label">AUTOMATED TEST — don't touch</span>
</div>

<style>
  .test-mode-badge {
    position: fixed;
    top: 8px;
    left: 8px;
    z-index: 2147483647; /* above every other overlay in the app */
    width: max-content;
    max-width: min(90vw, 420px);
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border-radius: 999px;
    background: #1a1a1a;
    color: #ffd54f;
    border: 2px solid #ff9800;
    font: 700 12px/1.3 ui-sans-serif, system-ui, sans-serif;
    letter-spacing: 0.01em;
    white-space: nowrap;
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.45);
    pointer-events: none;
    animation: badge-pulse 1.6s ease-in-out infinite;
  }

  @keyframes badge-pulse {
    0%,
    100% {
      border-color: #ff9800;
      box-shadow: 0 2px 10px rgba(0, 0, 0, 0.45), 0 0 0 2px rgba(255, 152, 0, 0.5);
    }
    50% {
      border-color: #e53935;
      box-shadow: 0 2px 10px rgba(0, 0, 0, 0.45), 0 0 0 4px rgba(229, 57, 53, 0.6);
    }
  }

  .glyph {
    font-size: 14px;
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
