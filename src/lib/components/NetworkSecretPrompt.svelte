<script lang="ts">
  /** Inline secret prompt for connecting a saved SFTP/WebDAV connection (CPE-1513, epic CPE-1498): password
      or key passphrase, with a "remember" toggle → `connection_secret_set` on remember (CPE-1510). Mirrors
      SmartFolderMenu's popover shape (visible border per [[dialogs-need-visible-border]]) rather than a
      full-screen modal, so it stays an inline/instant control anchored at the row ([[prefer-inline-instant-controls]]).

      Backend note (see network.ts module docs + Sidebar's connect handler): there is no ephemeral/session-only
      credential channel yet — `list_dir`'s remote route reads a connection's secret from the OS keychain, not
      from the navigate call. So "Remember" is really "persist past this app session": the secret is always
      stashed in the keychain long enough for THIS connect to succeed, then deleted right back out again if the
      user left "Remember" unchecked (see `connectNetworkConnection` in App.svelte). */
  import { createEventDispatcher, onMount } from "svelte";

  export let x = 0;
  export let y = 0;
  export let name = "";
  /** "Password" or "Passphrase" — which the connection's auth method actually needs. */
  export let label = "Password";

  const dispatch = createEventDispatcher<{
    submit: { secret: string; remember: boolean };
    close: void;
  }>();

  let secret = "";
  let remember = true;
  let input: HTMLInputElement | undefined;
  onMount(() => input?.focus());

  function submit() {
    if (!secret) return;
    dispatch("submit", { secret, remember });
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="menu" role="dialog" aria-label={`${label} for ${name}`} style="left:{x}px; top:{y}px" on:click|stopPropagation>
    <div class="head">{label} for “{name}”</div>
    <input
      bind:this={input}
      type="password"
      class="secret"
      bind:value={secret}
      placeholder={label}
      on:keydown={(e) => e.key === "Enter" && submit()}
      spellcheck="false"
      autocomplete="off"
      aria-label={label}
    />
    <label class="remember">
      <input type="checkbox" bind:checked={remember} />
      Remember (store in the OS keychain)
    </label>
    <div class="row">
      <button class="btn primary" disabled={!secret} on:click={submit}>Connect</button>
      <button class="btn ghost" on:click={() => dispatch("close")}>Cancel</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; z-index: 220; }
  .menu {
    position: fixed; width: 260px;
    background: var(--surface); color: var(--text);
    border: 1px solid var(--border-strong); border-radius: 8px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.25); padding: 10px;
  }
  .head { font-size: 12px; color: var(--text-dim); margin-bottom: 6px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .secret { width: 100%; height: 30px; padding: 0 8px; font: inherit; font-size: 13px;
    border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .secret:focus { outline: none; border-color: var(--accent); }
  .remember { display: flex; align-items: center; gap: 6px; margin-top: 8px; font-size: 12px; color: var(--text-dim); }
  .row { display: flex; gap: 6px; margin-top: 10px; justify-content: flex-end; }
  .btn { height: 28px; padding: 0 10px; font: inherit; font-size: 12px; border-radius: var(--radius);
    border: 1px solid var(--border-strong); background: var(--surface-alt); color: var(--text); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.ghost { border-color: transparent; background: transparent; color: var(--text-dim); }
  .btn:hover:not(:disabled) { filter: brightness(1.05); }
  .btn:disabled { opacity: 0.5; cursor: default; }
</style>
