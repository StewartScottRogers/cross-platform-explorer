<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { displaySafeName } from "../filename";

  // CPE-1790: `title`/`message` arrive as free-text strings a caller has already composed around an
  // escaped filesystem name (`App.svelte`'s callers wrap the name with displaySafeName before building
  // the sentence) — this dialog has no way to know which substring, if any, is filesystem-derived.
  // `confirmLabel` is escaped too: every caller today passes a static verb ("Delete"/"Extract"/…), but
  // that is only true BY CONVENTION — an ordinary caller-supplied prop, not a value static by
  // construction the way a `$t(...)` call is — and this ticket exists specifically to stop leaving a
  // free-text render slot protected by convention instead of by the leaf. Escaping the WHOLE string on
  // arrival is safe regardless of which slot: `displaySafeName` only replaces the twelve bidi/format
  // control characters (never an ordinary letter), so it's a no-op on plain prose and idempotent on a
  // caller's own already-escaped substring (its replacement text — `[RLO]`, `[LRM]`, … — is plain ASCII,
  // containing none of the characters it looks for, so a second pass finds nothing left to replace; see
  // `src/lib/filename.ts`'s own doc comment). That makes this LEAF the single point of truth instead of a
  // caller-remembered convention: every render below is provably safe to `bidiEscape.guard.test.ts`
  // (`src/lib/bidiRenderScan.ts`) whether or not the caller wrapped its own name first.
  export let title = "Are you sure?";
  export let message = "";
  export let confirmLabel = "OK";
  export let danger = false;

  const dispatch = createEventDispatcher<{ confirm: void; cancel: void }>();
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <h2>
      {#if danger}<span class="warn"><Icon name="delete" size={18} /></span>{/if}
      {displaySafeName(title)}
    </h2>
    <p>{displaySafeName(message)}</p>
    <div class="actions">
      <button class="btn" on:click={() => dispatch("cancel")}>Cancel</button>
      <button class="btn primary" class:danger on:click={() => dispatch("confirm")}>
        {displaySafeName(confirmLabel)}
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
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 16px;
    margin-bottom: 10px;
  }
  .warn { color: var(--danger); display: grid; place-items: center; }
  p { color: var(--text-dim); margin-bottom: 18px; line-height: 1.5; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
  .btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .btn.primary:hover { background: var(--accent-hover); }
  .btn.primary.danger { background: var(--danger-fill); border-color: var(--danger-fill); }
  .btn.primary.danger:hover { background: var(--danger-hover); }
</style>
