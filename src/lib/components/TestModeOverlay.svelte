<script lang="ts">
  /**
   * Automation test-mode halo overlay (CPE-1046): rendered only when the app was launched with
   * `--test-mode` (the backend injects `window.__CPE_TEST_MODE__` — see App.svelte's synchronous read,
   * mirroring the `--open` mechanism from CPE-1043). Its whole job is to make an automated GUI run
   * unmistakable to a human sharing the screen — a thick pulsing halo border + a loud banner — so they
   * never worry about, or interfere with, a window the automation is driving.
   *
   * Deliberately hardcoded English (not run through the i18n `$t` store): this is a test/dev-only
   * overlay, never seen by a real user in production use, so it's exempt from the 12-locale coverage
   * gate on purpose.
   *
   * Deliberately NOT theme-aware: the halo uses fixed loud colors (amber/red) instead of `var(--...)`
   * theme tokens, because it is supposed to be jarring and stand out identically in light and dark.
   *
   * Entirely `pointer-events: none` (including this element and everything inside it) so it can never
   * block the automation's clicks or the user's ability to close the window.
   */
</script>

<!-- Critical inert/position/stacking properties are also set inline (redundant with the stylesheet
     rules below) so they hold even if the stylesheet fails to load, and so tests can assert them
     directly on the element without depending on a full CSS cascade. -->
<div
  class="test-mode-halo"
  aria-hidden="true"
  style="position: fixed; inset: 0; z-index: 2147483647; pointer-events: none;"
>
  <div class="test-mode-banner" style="position: fixed; pointer-events: none;">
    <span class="glyph">🤖</span>
    AUTOMATED TEST IN PROGRESS — please don't touch this window
  </div>
</div>

<style>
  .test-mode-halo {
    position: fixed;
    inset: 0;
    z-index: 2147483647; /* above every other overlay in the app */
    pointer-events: none;
    box-shadow: inset 0 0 0 6px #ff9800;
    animation: halo-pulse 1.6s ease-in-out infinite;
  }

  @keyframes halo-pulse {
    0%,
    100% {
      box-shadow: inset 0 0 0 6px #ff9800, inset 0 0 24px 4px rgba(255, 152, 0, 0.55);
    }
    50% {
      box-shadow: inset 0 0 0 6px #e53935, inset 0 0 40px 10px rgba(229, 57, 53, 0.75);
    }
  }

  .test-mode-banner {
    position: fixed;
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 18px;
    border-radius: 999px;
    background: #1a1a1a;
    color: #ffd54f;
    border: 2px solid #ff9800;
    font: 700 13px/1.3 ui-sans-serif, system-ui, sans-serif;
    letter-spacing: 0.02em;
    white-space: nowrap;
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.5);
    pointer-events: none;
  }

  .glyph {
    font-size: 16px;
  }
</style>
