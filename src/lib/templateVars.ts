// Substitution variables for stamping a folder template (CPE-837, epic CPE-740). The core's `substitute`
// replaces `{token}`s; this fills the small default vocabulary the UI offers. Pure + unit-tested.

/** Build the `{token}` map passed to a stamp: `{date}` = today (ISO `YYYY-MM-DD`), plus `{name}` when the
 *  user supplied one (trimmed). Kept tiny + pure so both the dialog and its test share one source. */
export function buildVars(name: string): Record<string, string> {
  const vars: Record<string, string> = { date: new Date().toISOString().slice(0, 10) };
  if (name.trim()) vars.name = name.trim();
  return vars;
}
