import "./app.css";
import "highlight.js/styles/github.css"; // token colours for code previews (CPE-065)
import App from "./App.svelte";
import FloatPreview from "./lib/components/FloatPreview.svelte";
import AgentBoardApp from "./lib/components/AgentBoardApp.svelte";
import AgentCardApp from "./lib/components/AgentCardApp.svelte";
import { bootMode } from "./lib/bootMode";
import { initSettings, loadTheme, loadContrast } from "./lib/settings";
import { applyTheme, watchSystemTheme } from "./lib/theme";
import { queryOsHighContrast } from "./lib/highContrastSignal";

const target = document.getElementById("app")!;

// The same bundle backs three windows, chosen by a URL marker (see bootMode): the torn-off preview
// (?float, CPE-234), the standalone Agent Board (?board, CPE-841), or the full explorer.
const mode = bootMode(location.search);

async function bootstrap(): Promise<void> {
  // The float preview is transient and needs no settings load (CPE-234).
  if (mode === "float") {
    new FloatPreview({ target });
    return;
  }
  // Load the single on-disk settings file (and migrate any legacy prefs) BEFORE the app reads settings
  // synchronously at init (CPE-226). A failure here is non-fatal — the app falls back to defaults. The
  // board window loads it too so it picks up the theme.
  await initSettings().catch(() => {});
  applyTheme(loadTheme(), loadContrast()); // stamp dataset.theme before mount, avoiding a flash (CPE-1535); contrast persists across restarts too (CPE-1545)
  // One-shot read of the OS high-contrast signal (CPE-1546): a fast, fail-open-to-false query so a persisted
  // `"system"` contrast preference follows the real OS state from first paint. Re-stamp with the signal, and
  // carry it into the light/dark watcher so an OS theme flip keeps honouring high contrast.
  const osHighContrastActive = await queryOsHighContrast();
  applyTheme(loadTheme(), loadContrast(), osHighContrastActive);
  watchSystemTheme(() => applyTheme(loadTheme(), loadContrast(), osHighContrastActive)); // live-follow OS light/dark flips (CPE-1540)
  if (mode === "board") {
    new AgentBoardApp({ target });
    return;
  }
  // A torn-off card-detail window (CPE-960): render just the card, full-frame.
  if (mode === "card") {
    new AgentCardApp({ target });
    return;
  }
  new App({ target });
}

void bootstrap();
