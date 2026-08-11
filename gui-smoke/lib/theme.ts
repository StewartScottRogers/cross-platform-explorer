// CPE-1629 — real-theme toggling for gui-smoke specs (epic CPE-1492 "light/dark theme").
//
// The app's own runtime (`src/lib/theme.ts#applyTheme`) resolves a persisted preference and then does
// exactly one thing to the DOM: `document.documentElement.dataset.theme = "light" | "dark"` (or
// `"hc-light"/"hc-dark"` for the orthogonal high-contrast axis, not used here). Every themed CSS layer
// in the app (`:root[data-theme="light"]` / `:root[data-theme="dark"]`, CPE-1534/1539) selects off that
// one attribute — nothing else needs to change for a screenshot to render in the other theme. So this
// helper reproduces that exact write via `browser.execute`, rather than going through
// `localStorage.setItem("cpe.theme", …)` + a page reload: same DOM effect the production code path
// produces, without the extra reload round-trip (a real cost on the already-flake-prone Linux/WebKitGTK
// leg — CPE-1507/1595) or any risk of losing the in-flight navigation/selection state a spec has
// already set up.
import { browser } from "@wdio/globals";

export type GuiSmokeTheme = "light" | "dark";

/** Stamp `data-theme` on `<html>` directly (see module header for why this — not a settings round-trip
 *  — is the right level to intervene at). Returns once the attribute write has been applied. */
export async function setTheme(theme: GuiSmokeTheme): Promise<void> {
  await browser.execute((t) => {
    document.documentElement.dataset.theme = t;
  }, theme);
}
