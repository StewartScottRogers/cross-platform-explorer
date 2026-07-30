/**
 * Shared in-memory catalog of pickable metadata columns (CPE-1146, epic CPE-707). CPE-1145's
 * `metadata_columns_available` is documented as "in-memory enumeration only, no I/O", so it's fetched
 * ONCE here and cached in a module-level store rather than re-invoked per call site — dual-pane's two
 * `<ExplorerPane>` instances and the column-picker dialog all read the SAME fetch, and a folder with no
 * metadata columns active never triggers it at all (nothing subscribes until a picker/header needs it).
 */
import { writable } from "svelte/store";
import { commands } from "./bindings.gen";
import type { AvailableColumn } from "./bindings.gen";

/** The catalog, empty until {@link ensureMetaColumnCatalog} resolves. */
export const metaColumnCatalog = writable<AvailableColumn[]>([]);

let loaded = false;
let pending: Promise<void> | null = null;

/** Fetch the catalog into the store if it hasn't been already (or await the in-flight fetch). Safe to
 *  call from every mount site — a repeat call after the first successful load is a no-op. A failed
 *  fetch leaves the catalog empty (picker shows nothing to add; active columns just show no header
 *  label until it's retried) rather than throwing into a mount path. */
export function ensureMetaColumnCatalog(): Promise<void> {
  if (loaded) return Promise.resolve();
  if (!pending) {
    pending = commands
      .metadataColumnsAvailable()
      .then((cols) => {
        metaColumnCatalog.set(cols);
        loaded = true;
      })
      .catch(() => {
        // Leave the catalog empty; the next ensure() call (e.g. reopening the picker) retries.
      })
      .finally(() => {
        pending = null;
      });
  }
  return pending;
}

/** Test-only: reset the module-level cache so each test file gets a clean fetch. */
export function __resetMetaColumnCatalogForTests(): void {
  loaded = false;
  pending = null;
  metaColumnCatalog.set([]);
}
