/**
 * MenuBar language switching regression guard (CPE-553). A user reported es/de/fr not applying in the
 * packaged app; this proves the Svelte-level path is correct (store + the real picker-click flow), so a
 * future regression here fails CI. (The packaged-app failure is tracked separately as environment-specific.)
 */
import { describe, it, expect, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import MenuBar from "./MenuBar.svelte";
import { locale } from "../i18n";

beforeEach(() => {
  try { localStorage.clear(); } catch { /* ignore */ }
  locale.set("en");
});

describe("MenuBar language switching (CPE-553)", () => {
  it("re-renders menu titles when the locale store changes to Spanish", async () => {
    const { findByText, queryByText } = render(MenuBar);
    expect(await findByText("File")).toBeTruthy();
    locale.set("es");
    expect(await findByText("Archivo")).toBeTruthy();
    expect(queryByText("File")).toBeNull();
  });

  it("switches language via the real picker-click path (open 🌐, click Español)", async () => {
    const { findByText, getByText } = render(MenuBar);
    expect(await findByText("File")).toBeTruthy();
    await fireEvent.click(getByText(/Language/)); // open the 🌐 menu
    await fireEvent.click(getByText("Español")); // pick Spanish
    expect(await findByText("Archivo")).toBeTruthy();
  });

  it("switches to German and French too", async () => {
    const { findByText } = render(MenuBar);
    locale.set("de");
    expect(await findByText("Datei")).toBeTruthy(); // menu.file → German
    locale.set("fr");
    expect(await findByText("Fichier")).toBeTruthy(); // menu.file → French
  });
});
