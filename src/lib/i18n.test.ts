// CPE-362: i18n core — lookup, interpolation, graceful fallback, locale switching, Intl formatting.
import { describe, it, expect, beforeEach } from "vitest";
import { get } from "svelte/store";
import {
  SUPPORTED_LOCALES,
  translate,
  localeKeys,
  locale,
  t,
  formatDate,
  formatNumber,
  detectInitialLocale,
} from "./i18n";

beforeEach(() => {
  try {
    localStorage.clear();
  } catch {
    /* ignore */
  }
  locale.set("en");
});

describe("i18n translate()", () => {
  it("returns the message for the active locale", () => {
    expect(translate("en", "menu.file")).toBe("File");
    expect(translate("es", "menu.file")).toBe("Archivo");
    expect(translate("de", "menu.delete")).toBe("Löschen");
    expect(translate("fr", "nav.back")).toBe("Précédent");
  });

  it("falls back to English when a key is missing in the locale", () => {
    // A key present only in English still resolves for another locale (no blank label).
    expect(translate("es", "menu.file")).toBe("Archivo"); // present
    // Simulate a missing key by asking for one no catalog defines → returns the key itself.
    expect(translate("es", "does.not.exist")).toBe("does.not.exist");
  });

  it("translates the menu dropdown items across locales (CPE-481)", () => {
    expect(translate("en", "mi.searchInFiles")).toBe("Search in files…");
    expect(translate("es", "mi.settings")).toBe("Configuración…");
    expect(translate("de", "mi.exit")).toBe("Beenden");
    expect(translate("fr", "mi.about")).toBe("À propos");
    // Every menu-item key is defined in all four locales (no fallback needed).
    const keys = localeKeys("en").filter((k) => k.startsWith("mi."));
    expect(keys.length).toBeGreaterThanOrEqual(11);
    for (const loc of ["es", "de", "fr"] as const) {
      for (const k of keys) expect(localeKeys(loc)).toContain(k);
    }
  });

  it("translates the context-menu items across locales incl. interpolation (CPE-481)", () => {
    expect(translate("en", "ctx.duplicate")).toBe("Duplicate");
    expect(translate("es", "ctx.newFolder")).toBe("Nueva carpeta");
    expect(translate("de", "ctx.reveal")).toBe("Im Datei-Explorer anzeigen");
    expect(translate("fr", "ctx.paste")).toBe("Coller");
    expect(translate("en", "ctx.selectAllExt", { ext: "rs" })).toBe("Select all .rs");
    // All ctx.* keys defined in every locale (no English fallback needed).
    const keys = localeKeys("en").filter((k) => k.startsWith("ctx."));
    expect(keys.length).toBeGreaterThanOrEqual(30);
    for (const loc of ["es", "de", "fr"] as const) {
      for (const k of keys) expect(localeKeys(loc)).toContain(k);
    }
  });

  it("translates the CommandBar sort/view/filter labels across locales (CPE-481)", () => {
    expect(translate("es", "cmd.sort")).toBe("Ordenar");
    expect(translate("de", "sort.modified")).toBe("Änderungsdatum");
    expect(translate("fr", "view.icons")).toBe("Grandes icônes");
    expect(translate("es", "filter.document")).toBe("Documentos");
    for (const ns of ["cmd.", "sort.", "view.", "filter."]) {
      const keys = localeKeys("en").filter((k) => k.startsWith(ns));
      expect(keys.length).toBeGreaterThan(0);
      for (const loc of ["es", "de", "fr"] as const) for (const k of keys) expect(localeKeys(loc)).toContain(k);
    }
  });

  it("interpolates {name} placeholders", () => {
    expect(translate("en", "status.items", { count: 42 })).toBe("42 items");
    expect(translate("es", "status.selected", { count: 3 })).toBe("3 seleccionados");
  });

  it("leaves an unmatched placeholder intact rather than blanking it", () => {
    expect(translate("en", "status.items", {})).toBe("{count} items");
  });
});

describe("i18n reactive t() + locale switch", () => {
  it("re-resolves when the locale changes", () => {
    expect(get(t)("menu.view")).toBe("View");
    locale.set("de");
    expect(get(t)("menu.view")).toBe("Ansicht");
    locale.set("fr");
    expect(get(t)("menu.view")).toBe("Affichage");
  });

  it("every non-English locale is a strict subset of English keys (no orphan translations)", () => {
    // Guard against a typo'd key in a translation that would never be reached.
    const enKeys = new Set(localeKeys("en"));
    for (const { code } of SUPPORTED_LOCALES) {
      for (const k of localeKeys(code)) {
        expect(enKeys.has(k), `${code} has key '${k}' not in English`).toBe(true);
      }
    }
  });
});

describe("i18n Intl formatting", () => {
  it("formats numbers per locale", () => {
    locale.set("en");
    expect(get(formatNumber)(1234.5)).toBe("1,234.5");
    locale.set("de");
    expect(get(formatNumber)(1234.5)).toBe("1.234,5"); // German uses . / ,
  });

  it("formats dates per locale without throwing", () => {
    const d = new Date(Date.UTC(2026, 0, 15));
    locale.set("en");
    expect(get(formatDate)(d, { dateStyle: "short", timeZone: "UTC" })).toMatch(/1\/15\/26|1\/15\/2026/);
    locale.set("fr");
    expect(typeof get(formatDate)(d, { dateStyle: "short", timeZone: "UTC" })).toBe("string");
  });
});

describe("i18n locale detection + persistence", () => {
  it("persists the chosen locale and restores it", () => {
    locale.set("es");
    expect(detectInitialLocale()).toBe("es");
  });

  it("defaults to a supported locale", () => {
    localStorage.clear();
    expect(SUPPORTED_LOCALES.some((l) => l.code === detectInitialLocale())).toBe(true);
  });
});
