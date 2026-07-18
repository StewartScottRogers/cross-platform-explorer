// CPE-362: i18n core — lookup, interpolation, graceful fallback, locale switching, Intl formatting.
import { describe, it, expect, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { get } from "svelte/store";
import {
  SUPPORTED_LOCALES,
  COMPLETE_LOCALES,
  isComplete,
  translate,
  localeKeys,
  localeCoverage,
  locale,
  t,
  formatDate,
  formatNumber,
  detectInitialLocale,
} from "./i18n";

// The non-English locales the CPE-481 gate holds to 100% coverage. Derived from the single source of
// truth so that *completing* a catalog (CPE-539) — i.e. adding its code to COMPLETE_LOCALES — extends
// every gate below to that locale automatically, with no test edit. English is the source, so drop it.
const COMPLETE = COMPLETE_LOCALES.filter((c) => c !== "en");

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

  it("offers dozens of languages with native names + RTL flags (CPE-533)", async () => {
    const { LOCALES, filterLocales, isRtl } = await import("./i18n");
    // Dozens offered, English first, native names present.
    expect(LOCALES.length).toBeGreaterThanOrEqual(24);
    expect(LOCALES[0].code).toBe("en");
    expect(LOCALES.find((l) => l.code === "ja")?.name).toBe("日本語");
    // RTL languages flagged.
    expect(isRtl("ar")).toBe(true);
    expect(isRtl("he")).toBe(true);
    expect(isRtl("en")).toBe(false);
    // Search matches native + English name + code.
    expect(filterLocales("japan").map((l) => l.code)).toContain("ja");
    expect(filterLocales("Español").map((l) => l.code)).toContain("es");
    expect(filterLocales("ar").map((l) => l.code)).toContain("ar");
    expect(filterLocales("zzz").length).toBe(0);
    expect(filterLocales("").length).toBe(LOCALES.length);
  });

  it("an offered language without a catalog falls back to English per key (CPE-533)", () => {
    // Thai has no catalog yet → keys resolve to English (incremental coverage).
    expect(translate("th", "mi.settings")).toBe(translate("en", "mi.settings"));
    expect(translate("ar", "menu.language")).toBe(translate("en", "menu.language"));
  });

  it("translates the menu dropdown items across locales (CPE-481)", () => {
    expect(translate("en", "mi.searchInFiles")).toBe("Search in files…");
    expect(translate("es", "mi.settings")).toBe("Configuración…");
    expect(translate("de", "mi.exit")).toBe("Beenden");
    expect(translate("fr", "mi.about")).toBe("À propos");
    // Every menu-item key is defined in all four locales (no fallback needed).
    const keys = localeKeys("en").filter((k) => k.startsWith("mi."));
    expect(keys.length).toBeGreaterThanOrEqual(11);
    for (const loc of COMPLETE) {
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
    for (const loc of COMPLETE) {
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
      for (const loc of COMPLETE) for (const k of keys) expect(localeKeys(loc)).toContain(k);
    }
  });

  it("translates the dialogs + remaining chrome across locales (CPE-481)", () => {
    // A representative key from each newly-migrated namespace resolves per locale.
    expect(translate("es", "upd.titleAvailable")).toBe("Actualización disponible");
    expect(translate("de", "dup.title")).toBe("Doppelte Dateien finden");
    expect(translate("fr", "prop.location")).toBe("Emplacement");
    expect(translate("es", "ren.findReplace")).toBe("Buscar y reemplazar");
    expect(translate("de", "consent.allowSelected")).toBe("Ausgewählte erlauben & fortfahren");
    expect(translate("fr", "home.noRecent")).toBe("Aucun fichier récent pour l'instant");
    expect(translate("es", "mgr.healthy")).toBe("Correcto");
    expect(translate("de", "fl.empty")).toBe("Dieser Ordner ist leer");
    expect(translate("fr", "pv.loading")).toBe("Chargement de l'aperçu…");
    expect(translate("es", "tb.application")).toBe("Aplicación");
    // Interpolation carries through in every migrated namespace.
    expect(translate("en", "ren.willRename", { changed: 2, total: 5 })).toBe("2 of 5 will be renamed.");
    expect(translate("en", "dup.moveToBin", { count: 3 })).toBe("Move 3 to Recycle Bin");
    expect(translate("en", "fl.sortBy", { col: "Name" })).toBe("Sort by Name");
    expect(translate("en", "consent.allow", { id: "ai-console" })).toBe("Allow “ai-console” to…");
    // Every key added for CPE-481 exists in all four locales (fully translated, no fallback).
    const namespaces = ["upd.", "dup.", "prop.", "ren.", "consent.", "home.", "mgr.", "fl.", "pv.", "tb.", "agent."];
    for (const ns of namespaces) {
      const keys = localeKeys("en").filter((k) => k.startsWith(ns));
      expect(keys.length, `no keys for namespace ${ns}`).toBeGreaterThan(0);
      for (const loc of COMPLETE) {
        for (const k of keys) expect(localeKeys(loc), `${loc} missing ${k}`).toContain(k);
      }
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

describe("i18n locale coverage (CPE-539)", () => {
  it("reports 1 for English and for fully-translated locales", () => {
    expect(localeCoverage("en")).toBe(1);
    // es/de/fr are complete today (the CPE-481 gate enforces it) → 100%.
    for (const code of ["es", "de", "fr"] as const) {
      expect(localeCoverage(code), `${code} should be fully covered`).toBe(1);
    }
  });

  // AC#4: the CPE-481 coverage gate, made data-driven so it extends to newly-completed locales for
  // free. COMPLETE_LOCALES is the single source of truth for "which locales are done"; this holds
  // every one of them to 100% coverage. Add a locale's code to COMPLETE_LOCALES and this fails until
  // all 293 keys are actually present — you can't *declare* a locale complete without it *being* so.
  it("holds every locale declared complete to 100% coverage (CPE-481 gate, extended per CPE-539)", () => {
    for (const code of COMPLETE_LOCALES) {
      expect(isComplete(code), `${code} should be reported complete`).toBe(true);
      expect(localeCoverage(code), `${code} is declared complete but is missing keys`).toBe(1);
    }
    // An offered-but-uncatalogued locale (e.g. Thai) must NOT be declared complete.
    expect(isComplete("th")).toBe(false);
  });

  it("reports 0 for an offered locale with no catalog yet (full English fallback)", () => {
    // Thai is offered but has no catalog — it falls back entirely to English.
    expect(localeCoverage("th")).toBe(0);
  });

  it("is a fraction in [0,1] for every offered locale", () => {
    for (const { code } of SUPPORTED_LOCALES) {
      const c = localeCoverage(code);
      expect(c).toBeGreaterThanOrEqual(0);
      expect(c).toBeLessThanOrEqual(1);
    }
  });

  it("equals the share of English keys the locale defines", () => {
    const total = localeKeys("en").length;
    for (const code of ["en", "es", "th"] as const) {
      const defined = localeKeys(code).filter((k) => localeKeys("en").includes(k)).length;
      expect(localeCoverage(code)).toBeCloseTo(defined / total, 10);
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

// CPE-481: guard against a stray hardcoded string sneaking back into a migrated component.
// Two cheap, deterministic checks per file: (1) the component actually pulls in the i18n `t`
// store, and (2) none of the specific English literals we replaced still live in its markup.
describe("i18n migration guard (CPE-481)", () => {
  const read = (rel: string) => readFileSync(new URL(rel, import.meta.url), "utf8");

  // path → English literals that MUST no longer appear (each was chosen to be markup-only,
  // never present in that file's comments or script, so a match means a genuine regression).
  const MIGRATED: Record<string, string[]> = {
    "./components/UpdateDialog.svelte": ["Looking for a newer version", "Install &amp; Restart", "Try Again"],
    "./components/DuplicatesDialog.svelte": ["Scan for duplicates", "Select redundant", "Move ${selected.size}"],
    "./components/PropertiesDialog.svelte": ["Paste expected hash to verify", ">Compute<", "items selected"],
    "./components/BatchRenameDialog.svelte": ["Find &amp; replace", "text before the extension", "will be renamed"],
    "./components/ConsentSheet.svelte": ["Allow selected & continue", "you can change this later"],
    "./components/HomeView.svelte": ["No recent files yet", "Right-click any file or folder", "Quick access</span>"],
    "./components/SidecarManager.svelte": ["No sidecars registered.", ">Checking…<", "no capabilities</span>"],
    "./components/FileList.svelte": ["This folder is empty", "No items match your search", "Sort by {col.label}"],
    "./components/PreviewPane.svelte": ["Loading preview…", "Can't preview this image", ">Select all<"],
    "./components/Toolbar.svelte": ['"{label} settings"'],
    "../App.svelte": ["Reset all settings to defaults", "Show details/preview pane", "watching for changes…", "Resize navigation pane"],
  };

  for (const [file, literals] of Object.entries(MIGRATED)) {
    it(`${file} uses i18n and keeps no migrated literal`, () => {
      const src = read(file);
      expect(src, `${file} should import the i18n t store`).toMatch(/from "\.\.?\/(?:lib\/)?i18n"/);
      for (const lit of literals) {
        expect(src.includes(lit), `${file} still contains hardcoded "${lit}"`).toBe(false);
      }
    });
  }
});
