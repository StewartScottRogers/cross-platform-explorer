<script lang="ts">
  // Standalone Agent Board card-detail window root (CPE-960). Mounted by main.ts on the `?card=<id>&root=`
  // marker — mirroring the `?board` standalone board window — so this window renders ONLY the card detail,
  // full-frame with no explorer chrome. `standalone` makes CardDetailDialog fill the window (no backdrop,
  // no nested pop-out button); closing closes the OS window.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import CardDetailDialog from "./CardDetailDialog.svelte";

  const params = new URLSearchParams(location.search);
  const id = params.get("card") ?? "";
  const root = params.get("root") ?? "";
  // Epics carry an `epic` marker so the standalone window can still offer "View tickets" — though drilling
  // there just filters this lone card window's… nothing; so in standalone we hide the drill (no board here).

  function closeWindow() {
    void getCurrentWindow().close();
  }
</script>

{#if id}
  <CardDetailDialog {root} {id} standalone on:close={closeWindow} />
{:else}
  <div class="cardwin-empty">No card specified.</div>
{/if}

<style>
  .cardwin-empty { display: grid; place-items: center; height: 100vh; color: var(--text-dim); }
</style>
