<script lang="ts">
  // Standalone Agent Board window root (CPE-843, epic CPE-841). Mounted by main.ts when the `?board`
  // marker is present — mirroring the `?float` torn-off preview window — so this window renders ONLY the
  // board, with no explorer chrome. BoardView is self-contained: it loads its cards and moves them via
  // the existing `ticket_board` commands and remembers its own project root (`cpe.boardRoot`), so no data
  // wiring is needed here. `windowed` makes it fill the window.
  //
  // The launcher, app-wide singleton, and capability entry that let the explorer open/focus this window
  // are CPE-844; the cross-window agent-launch + in-window docs are handled there and in CPE-845, so those
  // events are intentionally no-ops for now (the board's read/move workflow is fully functional).
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import BoardView from "./BoardView.svelte";

  function closeWindow() {
    void getCurrentWindow().close();
  }
</script>

<BoardView
  windowed
  root=""
  on:close={closeWindow}
  on:launch={() => {/* cross-window agent launch: CPE-844 */}}
  on:help={() => {/* in-window docs: CPE-845 */}}
/>
