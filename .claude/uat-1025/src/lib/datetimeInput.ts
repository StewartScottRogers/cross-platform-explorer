// Conversions between an epoch-ms timestamp and the value of an `<input type="datetime-local">`
// (CPE-786 timestamp editing). datetime-local carries no timezone — its value is wall-clock *local*
// time — so we format/parse in the local zone (minute precision, which is all the control offers).
// Pure + unit-tested so the attributes editor stays a thin shell.

/** Epoch ms → `YYYY-MM-DDTHH:mm` in local time, or "" for null/invalid (an empty input). */
export function msToLocalInput(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms)) return "";
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** `YYYY-MM-DDTHH:mm[:ss]` (local) → epoch ms, or null if empty/unparseable. */
export function localInputToMs(value: string): number | null {
  if (!value) return null;
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$/.exec(value.trim());
  if (!m) return null;
  const [, y, mo, d, h, mi, s] = m;
  const dt = new Date(Number(y), Number(mo) - 1, Number(d), Number(h), Number(mi), Number(s ?? 0));
  return Number.isNaN(dt.getTime()) ? null : dt.getTime();
}
