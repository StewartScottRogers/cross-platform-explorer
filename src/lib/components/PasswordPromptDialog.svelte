<script lang="ts">
  /**
   * Shared masked-input password prompt (CPE-1179, epic CPE-705). There was no reusable text-input
   * dialog — `ConfirmDialog` is message-only. This is the owner of the "type a password" modal;
   * CPE-1182 consumes it for encrypted-archive extract/create (wiring lives there, not here).
   */
  import { createEventDispatcher, onMount, tick } from "svelte";
  import { displaySafeName } from "../filename";

  // CPE-1790: `title`/`message`/`error` arrive as free-text strings a caller has already composed
  // around an escaped name (e.g. `` `Unlock ${displaySafeName(entry.name)}` ``) — this dialog has no way
  // to know which substring, if any, is filesystem-derived, so every render below escapes the WHOLE
  // string on arrival instead of trusting the caller. That's safe unconditionally: `displaySafeName`
  // only replaces the twelve bidi/format control characters (never an ordinary letter), so it's a no-op
  // on plain prose and idempotent on a caller's own already-escaped substring (its replacement text is
  // plain ASCII, containing none of the characters it looks for — see `src/lib/filename.ts`). See
  // `ConfirmDialog.svelte`'s matching comment for the full argument.
  export let title = "Password required";
  export let message = "";
  export let confirmLabel = "OK";
  /** Optional inline error line, e.g. "Wrong password — try again". */
  export let error = "";

  const dispatch = createEventDispatcher<{ submit: string; cancel: void }>();

  let value = "";
  let fieldEl: HTMLInputElement;
  // Submit-guard (CPE-1249 review #B): once a submit is in flight, the OK button disables and Enter is
  // ignored, so an impatient double-click / Enter+click can't fire `submit` twice and launch two
  // concurrent operations (e.g. two vault unlocks racing). One submit per shown dialog; callers that
  // re-prompt do so by remounting this component (a fresh instance re-arms the guard).
  let submitting = false;

  onMount(async () => {
    await tick();
    fieldEl?.focus();
  });

  function submit() {
    if (submitting) return;
    submitting = true;
    dispatch("submit", value);
  }

  function onKeydown(e: KeyboardEvent) {
    // Only Enter here. Escape is handled once, globally, by the `svelte:window` listener below
    // (matching ConfirmDialog). Handling it here too would fire `cancel` twice, since a keydown in
    // the field bubbles up to window.
    if (e.key === "Enter") submit();
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label={displaySafeName(title)} on:click|stopPropagation>
    <h2>{displaySafeName(title)}</h2>
    {#if message}<p>{displaySafeName(message)}</p>{/if}

    <input
      class="field"
      type="password"
      bind:this={fieldEl}
      bind:value
      on:keydown={onKeydown}
      aria-label="Password"
      data-testid="password-field"
      autocomplete="current-password"
    />

    {#if error}<div class="err" data-testid="password-error">{displaySafeName(error)}</div>{/if}

    <div class="actions">
      <button class="btn" data-testid="cancel-btn" on:click={() => dispatch("cancel")}>Cancel</button>
      <button class="btn primary" data-testid="ok-btn" on:click={submit} disabled={submitting}>{confirmLabel}</button>
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
    width: 380px;
    max-width: 90vw;
    background: var(--surface);
    border: 1px solid var(--dialog-border);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 { font-size: 16px; margin-bottom: 10px; }
  p { color: var(--text-dim); margin-bottom: 14px; line-height: 1.5; font-size: 12.5px; }
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
  .err {
    margin-top: 8px;
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
  /* Submit-guard disabled state (theme-only, matches ShredConfirmDialog's disabled treatment). */
  .btn:disabled { opacity: 0.6; cursor: default; }
</style>
