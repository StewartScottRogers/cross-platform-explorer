import "./app.css";
import "highlight.js/styles/github.css"; // token colours for code previews (CPE-065)
import App from "./App.svelte";
import { initSettings } from "./lib/settings";

let app: App;

// Load the single on-disk settings file (and migrate any legacy prefs) BEFORE
// the app reads settings synchronously at init (CPE-226). A failure here is
// non-fatal — the app falls back to defaults.
async function bootstrap(): Promise<void> {
  await initSettings().catch(() => {});
  app = new App({ target: document.getElementById("app")! });
}

void bootstrap();

export default app!;
