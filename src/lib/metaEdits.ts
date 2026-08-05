// Pure edit-map helpers for the Metadata Studio (CPE-1041). The studio tracks pending edits in a
// string-keyed record; a metadata field is identified by (group, key). A key can contain spaces
// ("Album Artist", "Date Created"), so the composite map key joins group+key on a NUL separator — a
// character that can never appear in either — and splits on the FIRST NUL. Splitting on a space (or any
// character the data can contain) would silently corrupt those fields, so this logic is centralized and
// unit-tested rather than inlined in the component.
import type { MetaEdit, MetaField } from "./bindings.gen";

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

/** Build a pending-edits record (the same composite-key shape `buildMetaEdits` consumes) staging every
 *  field in `fields` to a new value computed by `valueFor`. Shared by the Metadata Studio's two batch ops
 *  (CPE-1326): "Strip editable metadata" passes `() => ""` (an empty value becomes a `clear` via
 *  `buildMetaEdits` — no separate clear mechanism), and "Copy from first" passes `(f) => f.value` to stage
 *  the primary file's own values. Callers must pre-filter `fields` to the ones that are actually editable
 *  (writability is a per-file/per-format property the caller already has, not something this pure helper
 *  can see) — this just maps each field to its composite key deterministically, unconditionally (no
 *  "skip if unchanged" short-circuit), because the resulting record is later applied uniformly to every
 *  target file in a batch, whose current values may differ from the primary's even when a field looks
 *  unchanged relative to the primary. */
export function stageFieldEdits(
  fields: MetaField[],
  valueFor: (f: MetaField) => string,
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const f of fields) {
    out[joinFieldKey(f.group, f.key)] = valueFor(f);
  }
  return out;
}

/** Whether `f` has a pending edit staged in `edited` (its composite key is present) — i.e. the field is
 *  "dirty" relative to the value loaded from disk. Used by per-field revert (CPE-1327) to decide whether
 *  to show the revert control on a given row; kept here (rather than inlined at the call site) so the
 *  same composite-key lookup the rest of this module uses can't drift out of sync with `onEdit`'s own
 *  drop-when-equal-to-original invariant in the component. */
export function isFieldDirty(f: MetaField, edited: Record<string, string>): boolean {
  return joinFieldKey(f.group, f.key) in edited;
}

/** Drop `f`'s pending edit from `edited`, leaving every other field's edit untouched — the per-field
 *  "revert to original" action (CPE-1327). Once the composite key is gone, the component's existing
 *  `currentValue(f, edited)` falls back to `f.value` (the value loaded from disk), so this alone is
 *  enough to restore the field; no separate "original value" store is needed.
 *
 *  Returns a NEW record rather than mutating `edited` in place — callers MUST reassign
 *  (`edited = revertFieldEdit(f, edited)`), not just call this and discard the result, because Svelte's
 *  reactivity tracks *assignment*, not in-place mutation (the same rule CPE-1326's `stageFieldEdits`
 *  callers follow — see `currentValue`'s doc comment in MetadataStudioDialog.svelte). Returns the same
 *  reference when the field wasn't dirty, so a no-op revert never triggers a spurious re-render. */
export function revertFieldEdit(f: MetaField, edited: Record<string, string>): Record<string, string> {
  const k = joinFieldKey(f.group, f.key);
  if (!(k in edited)) return edited;
  const { [k]: _drop, ...rest } = edited;
  return rest;
}
