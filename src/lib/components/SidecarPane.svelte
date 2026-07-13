<script lang="ts">
  /**
   * Host pane that embeds a sidecar's OWN served UI in a sandboxed iframe (CPE-271,
   * host half). The `url` is the loopback address a sidecar announces via its
   * `ui:<url>` Status event once it's running.
   *
   * The iframe is sandboxed to `allow-scripts allow-forms allow-same-origin` (CPE-334):
   * `allow-same-origin` is required for the terminal's clipboard (copy/paste) and WebGL
   * to work. This is a deliberate, scoped trade-off — the page is FIRST-PARTY content the
   * sidecar serves on loopback (127.0.0.1) only; it still cannot navigate or script the
   * host window, and the sidecar remains a separate OS process with brokered capabilities.
   * Documented in docs/security/threat-model.md.
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
      sandbox="allow-scripts allow-forms allow-same-origin"
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
