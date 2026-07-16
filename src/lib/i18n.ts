// Internationalization (CPE-362): a tiny, dependency-free i18n layer built on Svelte stores.
//
// Design goals: no runtime deps (the app ships small — PURPOSE.md), reactive so a language change
// re-renders instantly, and a `t(key)` that falls back gracefully (chosen locale → English → the key
// itself) so a missing translation is never a blank label. Locale-aware date/number formatting uses
// the platform `Intl`. The chosen locale persists to `localStorage` and is restored on load.
//
// Adding a language = adding a catalog to `messages` + an entry in `SUPPORTED_LOCALES`. Adding a
// string = adding a key to every catalog (English is the source of truth + the fallback).

import { derived, writable } from "svelte/store";

export type Locale = "en" | "es" | "de" | "fr";

/** The languages offered in the settings picker. `en` is the source-of-truth + fallback. */
export const SUPPORTED_LOCALES: { code: Locale; name: string }[] = [
  { code: "en", name: "English" },
  { code: "es", name: "Español" },
  { code: "de", name: "Deutsch" },
  { code: "fr", name: "Français" },
];

const STORAGE_KEY = "cpe.locale";

/** Message catalogs. English is complete and authoritative; others may be partial (missing keys fall
 *  back to English). Keys are dotted namespaces (`menu.file`, `nav.back`, …). `{name}`-style
 *  placeholders are interpolated by `t(key, params)`. */
const messages: Record<Locale, Record<string, string>> = {
  en: {
    "app.newTab": "New tab",
    "app.closeTab": "Close tab",
    "nav.back": "Back",
    "nav.forward": "Forward",
    "nav.up": "Up",
    "nav.refresh": "Refresh",
    "nav.home": "Home",
    "nav.search": "Search",
    "menu.file": "File",
    "menu.tools": "Tools",
    "ctx.open": "Open",
    "ctx.execute": "Execute",
    "ctx.executeAdmin": "Execute as administrator",
    "ctx.openNewTab": "Open in new tab",
    "ctx.openInTerminal": "Open in Terminal",
    "ctx.workOnThis": "Work on this in AI Console",
    "ctx.duplicate": "Duplicate",
    "ctx.copyAsPath": "Copy as path",
    "ctx.copyToFolder": "Copy to folder…",
    "ctx.moveToFolder": "Move to folder…",
    "ctx.copyName": "Copy name",
    "ctx.rename": "Rename…",
    "ctx.compareFiles": "Compare files",
    "ctx.selectAllExt": "Select all .{ext}",
    "ctx.extract": "Extract",
    "ctx.compressZip": "Compress to ZIP",
    "ctx.pinToHome": "Pin to Home",
    "ctx.unpinFromHome": "Unpin from Home",
    "ctx.addFavorite": "Add to Favorites",
    "ctx.removeFavorite": "Remove from Favorites",
    "ctx.reveal": "Reveal in File Explorer",
    "ctx.properties": "Properties",
    "ctx.newFolder": "New folder",
    "ctx.newFile": "New file",
    "ctx.paste": "Paste",
    "ctx.selectAll": "Select all",
    "ctx.invertSelection": "Invert selection",
    "ctx.selectByPattern": "Select by pattern…",
    "ctx.refresh": "Refresh",
    "ctx.workOnFolder": "Work on this folder in AI Console",
    "mi.exit": "Exit",
    "mi.searchInFiles": "Search in files…",
    "mi.findDuplicates": "Find duplicate files…",
    "mi.copyFileNames": "Copy file names",
    "mi.copyFileList": "Copy file list (name + size)",
    "mi.saveFileList": "Save file list…",
    "mi.checkUpdates": "Check for Updates…",
    "mi.settings": "Settings…",
    "mi.shortcuts": "Keyboard shortcuts",
    "mi.documentation": "Documentation",
    "mi.about": "About",
    "menu.application": "Application",
    "menu.edit": "Edit",
    "menu.view": "View",
    "menu.go": "Go",
    "menu.help": "Help",
    "menu.newFolder": "New Folder",
    "menu.rename": "Rename",
    "menu.delete": "Delete",
    "menu.copy": "Copy",
    "menu.cut": "Cut",
    "menu.paste": "Paste",
    "menu.properties": "Properties",
    "menu.selectAll": "Select All",
    "menu.settings": "Settings",
    "menu.language": "Language",
    "sidebar.quickAccess": "Quick Access",
    "sidebar.drives": "Drives",
    "sidebar.repositories": "Repositories",
    "sidebar.agents": "Agents",
    "status.items": "{count} items",
    "status.selected": "{count} selected",
    "common.ok": "OK",
    "common.cancel": "Cancel",
    "common.close": "Close",
    "common.apply": "Apply",
  },
  es: {
    "app.newTab": "Nueva pestaña",
    "app.closeTab": "Cerrar pestaña",
    "nav.back": "Atrás",
    "nav.forward": "Adelante",
    "nav.up": "Arriba",
    "nav.refresh": "Actualizar",
    "nav.home": "Inicio",
    "nav.search": "Buscar",
    "menu.file": "Archivo",
    "menu.tools": "Herramientas",
    "ctx.open": "Abrir",
    "ctx.execute": "Ejecutar",
    "ctx.executeAdmin": "Ejecutar como administrador",
    "ctx.openNewTab": "Abrir en nueva pestaña",
    "ctx.openInTerminal": "Abrir en Terminal",
    "ctx.workOnThis": "Trabajar en esto en AI Console",
    "ctx.duplicate": "Duplicar",
    "ctx.copyAsPath": "Copiar como ruta",
    "ctx.copyToFolder": "Copiar a carpeta…",
    "ctx.moveToFolder": "Mover a carpeta…",
    "ctx.copyName": "Copiar nombre",
    "ctx.rename": "Renombrar…",
    "ctx.compareFiles": "Comparar archivos",
    "ctx.selectAllExt": "Seleccionar todos los .{ext}",
    "ctx.extract": "Extraer",
    "ctx.compressZip": "Comprimir a ZIP",
    "ctx.pinToHome": "Anclar a Inicio",
    "ctx.unpinFromHome": "Desanclar de Inicio",
    "ctx.addFavorite": "Añadir a favoritos",
    "ctx.removeFavorite": "Quitar de favoritos",
    "ctx.reveal": "Mostrar en el Explorador",
    "ctx.properties": "Propiedades",
    "ctx.newFolder": "Nueva carpeta",
    "ctx.newFile": "Nuevo archivo",
    "ctx.paste": "Pegar",
    "ctx.selectAll": "Seleccionar todo",
    "ctx.invertSelection": "Invertir selección",
    "ctx.selectByPattern": "Seleccionar por patrón…",
    "ctx.refresh": "Actualizar",
    "ctx.workOnFolder": "Trabajar en esta carpeta en AI Console",
    "mi.exit": "Salir",
    "mi.searchInFiles": "Buscar en archivos…",
    "mi.findDuplicates": "Buscar archivos duplicados…",
    "mi.copyFileNames": "Copiar nombres de archivo",
    "mi.copyFileList": "Copiar lista de archivos (nombre + tamaño)",
    "mi.saveFileList": "Guardar lista de archivos…",
    "mi.checkUpdates": "Buscar actualizaciones…",
    "mi.settings": "Configuración…",
    "mi.shortcuts": "Atajos de teclado",
    "mi.documentation": "Documentación",
    "mi.about": "Acerca de",
    "menu.application": "Aplicación",
    "menu.edit": "Editar",
    "menu.view": "Ver",
    "menu.go": "Ir",
    "menu.help": "Ayuda",
    "menu.newFolder": "Nueva carpeta",
    "menu.rename": "Renombrar",
    "menu.delete": "Eliminar",
    "menu.copy": "Copiar",
    "menu.cut": "Cortar",
    "menu.paste": "Pegar",
    "menu.properties": "Propiedades",
    "menu.selectAll": "Seleccionar todo",
    "menu.settings": "Configuración",
    "menu.language": "Idioma",
    "sidebar.quickAccess": "Acceso rápido",
    "sidebar.drives": "Unidades",
    "sidebar.repositories": "Repositorios",
    "sidebar.agents": "Agentes",
    "status.items": "{count} elementos",
    "status.selected": "{count} seleccionados",
    "common.ok": "Aceptar",
    "common.cancel": "Cancelar",
    "common.close": "Cerrar",
    "common.apply": "Aplicar",
  },
  de: {
    "app.newTab": "Neuer Tab",
    "app.closeTab": "Tab schließen",
    "nav.back": "Zurück",
    "nav.forward": "Vor",
    "nav.up": "Nach oben",
    "nav.refresh": "Aktualisieren",
    "nav.home": "Start",
    "nav.search": "Suchen",
    "menu.file": "Datei",
    "menu.tools": "Extras",
    "ctx.open": "Öffnen",
    "ctx.execute": "Ausführen",
    "ctx.executeAdmin": "Als Administrator ausführen",
    "ctx.openNewTab": "In neuem Tab öffnen",
    "ctx.openInTerminal": "Im Terminal öffnen",
    "ctx.workOnThis": "In der AI-Konsole bearbeiten",
    "ctx.duplicate": "Duplizieren",
    "ctx.copyAsPath": "Als Pfad kopieren",
    "ctx.copyToFolder": "In Ordner kopieren…",
    "ctx.moveToFolder": "In Ordner verschieben…",
    "ctx.copyName": "Namen kopieren",
    "ctx.rename": "Umbenennen…",
    "ctx.compareFiles": "Dateien vergleichen",
    "ctx.selectAllExt": "Alle .{ext} auswählen",
    "ctx.extract": "Extrahieren",
    "ctx.compressZip": "In ZIP komprimieren",
    "ctx.pinToHome": "An Start anheften",
    "ctx.unpinFromHome": "Von Start lösen",
    "ctx.addFavorite": "Zu Favoriten hinzufügen",
    "ctx.removeFavorite": "Aus Favoriten entfernen",
    "ctx.reveal": "Im Datei-Explorer anzeigen",
    "ctx.properties": "Eigenschaften",
    "ctx.newFolder": "Neuer Ordner",
    "ctx.newFile": "Neue Datei",
    "ctx.paste": "Einfügen",
    "ctx.selectAll": "Alles auswählen",
    "ctx.invertSelection": "Auswahl umkehren",
    "ctx.selectByPattern": "Nach Muster auswählen…",
    "ctx.refresh": "Aktualisieren",
    "ctx.workOnFolder": "An diesem Ordner in der AI-Konsole arbeiten",
    "mi.exit": "Beenden",
    "mi.searchInFiles": "In Dateien suchen…",
    "mi.findDuplicates": "Doppelte Dateien finden…",
    "mi.copyFileNames": "Dateinamen kopieren",
    "mi.copyFileList": "Dateiliste kopieren (Name + Größe)",
    "mi.saveFileList": "Dateiliste speichern…",
    "mi.checkUpdates": "Nach Updates suchen…",
    "mi.settings": "Einstellungen…",
    "mi.shortcuts": "Tastenkürzel",
    "mi.documentation": "Dokumentation",
    "mi.about": "Über",
    "menu.application": "Anwendung",
    "menu.edit": "Bearbeiten",
    "menu.view": "Ansicht",
    "menu.go": "Gehe zu",
    "menu.help": "Hilfe",
    "menu.newFolder": "Neuer Ordner",
    "menu.rename": "Umbenennen",
    "menu.delete": "Löschen",
    "menu.copy": "Kopieren",
    "menu.cut": "Ausschneiden",
    "menu.paste": "Einfügen",
    "menu.properties": "Eigenschaften",
    "menu.selectAll": "Alles auswählen",
    "menu.settings": "Einstellungen",
    "menu.language": "Sprache",
    "sidebar.quickAccess": "Schnellzugriff",
    "sidebar.drives": "Laufwerke",
    "sidebar.repositories": "Repositories",
    "sidebar.agents": "Agenten",
    "status.items": "{count} Elemente",
    "status.selected": "{count} ausgewählt",
    "common.ok": "OK",
    "common.cancel": "Abbrechen",
    "common.close": "Schließen",
    "common.apply": "Übernehmen",
  },
  fr: {
    "app.newTab": "Nouvel onglet",
    "app.closeTab": "Fermer l'onglet",
    "nav.back": "Précédent",
    "nav.forward": "Suivant",
    "nav.up": "Haut",
    "nav.refresh": "Actualiser",
    "nav.home": "Accueil",
    "nav.search": "Rechercher",
    "menu.file": "Fichier",
    "menu.tools": "Outils",
    "ctx.open": "Ouvrir",
    "ctx.execute": "Exécuter",
    "ctx.executeAdmin": "Exécuter en tant qu'administrateur",
    "ctx.openNewTab": "Ouvrir dans un nouvel onglet",
    "ctx.openInTerminal": "Ouvrir dans le terminal",
    "ctx.workOnThis": "Travailler dessus dans AI Console",
    "ctx.duplicate": "Dupliquer",
    "ctx.copyAsPath": "Copier en tant que chemin",
    "ctx.copyToFolder": "Copier vers le dossier…",
    "ctx.moveToFolder": "Déplacer vers le dossier…",
    "ctx.copyName": "Copier le nom",
    "ctx.rename": "Renommer…",
    "ctx.compareFiles": "Comparer les fichiers",
    "ctx.selectAllExt": "Sélectionner tous les .{ext}",
    "ctx.extract": "Extraire",
    "ctx.compressZip": "Compresser en ZIP",
    "ctx.pinToHome": "Épingler à l'accueil",
    "ctx.unpinFromHome": "Détacher de l'accueil",
    "ctx.addFavorite": "Ajouter aux favoris",
    "ctx.removeFavorite": "Retirer des favoris",
    "ctx.reveal": "Afficher dans l'Explorateur",
    "ctx.properties": "Propriétés",
    "ctx.newFolder": "Nouveau dossier",
    "ctx.newFile": "Nouveau fichier",
    "ctx.paste": "Coller",
    "ctx.selectAll": "Tout sélectionner",
    "ctx.invertSelection": "Inverser la sélection",
    "ctx.selectByPattern": "Sélectionner par motif…",
    "ctx.refresh": "Actualiser",
    "ctx.workOnFolder": "Travailler sur ce dossier dans AI Console",
    "mi.exit": "Quitter",
    "mi.searchInFiles": "Rechercher dans les fichiers…",
    "mi.findDuplicates": "Trouver les fichiers en double…",
    "mi.copyFileNames": "Copier les noms de fichiers",
    "mi.copyFileList": "Copier la liste des fichiers (nom + taille)",
    "mi.saveFileList": "Enregistrer la liste des fichiers…",
    "mi.checkUpdates": "Rechercher des mises à jour…",
    "mi.settings": "Paramètres…",
    "mi.shortcuts": "Raccourcis clavier",
    "mi.documentation": "Documentation",
    "mi.about": "À propos",
    "menu.application": "Application",
    "menu.edit": "Édition",
    "menu.view": "Affichage",
    "menu.go": "Aller",
    "menu.help": "Aide",
    "menu.newFolder": "Nouveau dossier",
    "menu.rename": "Renommer",
    "menu.delete": "Supprimer",
    "menu.copy": "Copier",
    "menu.cut": "Couper",
    "menu.paste": "Coller",
    "menu.properties": "Propriétés",
    "menu.selectAll": "Tout sélectionner",
    "menu.settings": "Paramètres",
    "menu.language": "Langue",
    "sidebar.quickAccess": "Accès rapide",
    "sidebar.drives": "Lecteurs",
    "sidebar.repositories": "Dépôts",
    "sidebar.agents": "Agents",
    "status.items": "{count} éléments",
    "status.selected": "{count} sélectionnés",
    "common.ok": "OK",
    "common.cancel": "Annuler",
    "common.close": "Fermer",
    "common.apply": "Appliquer",
  },
};

/** Pick the initial locale: a saved choice, else the browser/OS language if we ship it, else English. */
export function detectInitialLocale(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && isLocale(saved)) return saved;
  } catch {
    /* localStorage may be unavailable (SSR/sandbox) — fall through */
  }
  const nav = typeof navigator !== "undefined" ? navigator.language.slice(0, 2) : "en";
  return isLocale(nav) ? nav : "en";
}

function isLocale(v: string): v is Locale {
  return SUPPORTED_LOCALES.some((l) => l.code === v);
}

/** The active locale. Set it to switch languages app-wide; the choice persists. */
export const locale = writable<Locale>(detectInitialLocale());

locale.subscribe((loc) => {
  try {
    localStorage.setItem(STORAGE_KEY, loc);
  } catch {
    /* ignore */
  }
  if (typeof document !== "undefined") document.documentElement.lang = loc;
});

/** The message keys defined for a locale (introspection/test helper — used to guard against a
 *  translation carrying a key that English doesn't, which would never be reached). */
export function localeKeys(loc: Locale): string[] {
  return Object.keys(messages[loc] ?? {});
}

/** Look up + interpolate a message for a specific locale (exported for unit tests). */
export function translate(loc: Locale, key: string, params?: Record<string, string | number>): string {
  const msg = messages[loc]?.[key] ?? messages.en[key] ?? key;
  return interpolate(msg, params);
}

function interpolate(msg: string, params?: Record<string, string | number>): string {
  if (!params) return msg;
  return msg.replace(/\{(\w+)\}/g, (whole, name) => (name in params ? String(params[name]) : whole));
}

/** Reactive translator: `$t("menu.file")` in markup re-renders when the locale changes. */
export const t = derived(
  locale,
  ($locale) => (key: string, params?: Record<string, string | number>) => translate($locale, key, params),
);

/** Reactive locale-aware date formatter. `$formatDate(new Date(), { dateStyle: "medium" })`. */
export const formatDate = derived(
  locale,
  ($locale) => (value: Date | number, opts?: Intl.DateTimeFormatOptions) =>
    new Intl.DateTimeFormat($locale, opts).format(value),
);

/** Reactive locale-aware number formatter. */
export const formatNumber = derived(
  locale,
  ($locale) => (value: number, opts?: Intl.NumberFormatOptions) =>
    new Intl.NumberFormat($locale, opts).format(value),
);
