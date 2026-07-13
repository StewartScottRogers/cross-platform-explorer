<script lang="ts">
  /**
   * Host pane that embeds a sidecar's OWN served UI in a sandboxed iframe (CPE-271,
   * host half). The `url` is the loopback address a sidecar announces via its
   * `ui:<url>` Status event once it's running. The iframe is sandboxed WITHOUT
   * `allow-same-origin`, so the sidecar's page runs in an opaque origin and cannot
   * reach the explorer's window — the isolation the ADR requires.
   *
   * Note: for the frame to load, the app CSP (tauri.conf.json) must permit
   * `frame-src http://127.0.0.1:*` — wired with the runtime spawn plumbing.
   */
  export let url: string | null = null;
  export let title = "Sidecar";
</script>

<div class="sidecar-pane">
  {#if url}
    <!-- svelte-ignore a11y-missing-attribute -->
    <iframe
      class="frame"
      src={url}
      title={`${title} UI`}
      sandbox="allow-scripts allow-forms"
      referrerpolicy="no-referrer"
    />
  {:else}
    <div class="empty">No sidecar UI mounted.</div>
  {/if}
</div>

<style>
  .sidecar-pane {
    width: 100%;
    height: 100%;
    display: flex;
  }
  .frame {
    flex: 1;
    border: 0;
    width: 100%;
    height: 100%;
    background: var(--surface);
  }
  .empty {
    margin: auto;
    color: var(--text-dim);
    font-size: 13px;
  }
</style>
