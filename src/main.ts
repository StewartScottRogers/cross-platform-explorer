import "./app.css";
import "highlight.js/styles/github.css"; // token colours for code previews (CPE-065)
import App from "./App.svelte";
import FloatPreview from "./lib/components/FloatPreview.svelte";
import { initSettings } from "./lib/settings";

const target = document.getElementById("app")!;

// The torn-off preview window (CPE-234) loads the same bundle with ?float=1 and
// renders only the floating tabbed preview — no explorer, no settings load.
const isFloat = new URLSearchParams(location.search).has("float");

async function bootstrap(): Promise<void> {
  if (isFloat) {
    new FloatPreview({ target });
    return;
  }
  // Load the single on-disk settings file (and migrate any legacy prefs) BEFORE
  // the app reads settings synchronously at init (CPE-226). A failure here is
  // non-fatal — the app falls back to defaults.
  await initSettings().catch(() => {});
  new App({ target });
}

void bootstrap();
