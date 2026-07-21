// Which root surface the frontend mounts, chosen from the window URL (CPE-843, epic CPE-841).
//
// The same bundle backs three windows, distinguished by a query marker so a secondary window renders
// only its surface with no explorer chrome:
//   - `?float=1`  → the torn-off tabbed preview (CPE-234)
//   - `?board=1`  → the standalone Agent Board window (CPE-841)
//   - (neither)   → the full file explorer
//
// Kept pure (takes the raw `location.search` string) so the decision is unit-testable without a DOM.

export type BootMode = "float" | "board" | "explorer";

/** Decide the boot surface from a URL query string (e.g. `location.search`). Float wins over board if
 *  both markers are somehow present; absent both, it's the explorer. */
export function bootMode(search: string): BootMode {
  const params = new URLSearchParams(search);
  if (params.has("float")) return "float";
  if (params.has("board")) return "board";
  return "explorer";
}
