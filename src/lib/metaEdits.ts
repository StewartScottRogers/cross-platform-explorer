// Pure edit-map helpers for the Metadata Studio (CPE-1041). The studio tracks pending edits in a
// string-keyed record; a metadata field is identified by (group, key). A key can contain spaces
// ("Album Artist", "Date Created"), so the composite map key joins group+key on a NUL separator — a
// character that can never appear in either — and splits on the FIRST NUL. Splitting on a space (or any
// character the data can contain) would silently corrupt those fields, so this logic is centralized and
// unit-tested rather than inlined in the component.
import type { MetaEdit } from "./bindings.gen";

/** Separator that cannot appear in a metadata group or key. */
export const FIELD_SEP = "\u0000";

/** Composite map key for a (group, key) field. */
export function joinFieldKey(group: string, key: string): string {
  return `${group}${FIELD_SEP}${key}`;
}

/** Split a composite key back into its group and key, on the first separator (so a key containing the
 *  separator — which can't happen for real data, but be defensive — never loses its tail). */
export function splitFieldKey(composite: string): { group: string; key: string } {
  const i = composite.indexOf(FIELD_SEP);
  if (i < 0) return { group: composite, key: "" };
  return { group: composite.slice(0, i), key: composite.slice(i + 1) };
}

/** Turn a pending-edits record (composite-key → new value) into the backend `MetaEdit[]`. An empty/
 *  whitespace-only value becomes a `clear` (remove the field); any other value is a `set`. */
export function buildMetaEdits(edited: Record<string, string>): MetaEdit[] {
  return Object.entries(edited).map(([composite, value]) => {
    const { group, key } = splitFieldKey(composite);
    return value.trim() === ""
      ? ({ edit: "clear", group, key } as MetaEdit)
      : ({ edit: "set", group, key, value } as MetaEdit);
  });
}
