// Pure audit-log export + filter (CPE-799, epic CPE-733). Filter a list of session audit events and render
// them to JSON / CSV / Markdown with correct escaping. No DOM/IO — unit-tested — so the history browser +
// export UI (CPE-801) is a thin render. Shares the event model with replay (CPE-728).

/** One filesystem-activity event in the audit log. */
export interface AuditEvent {
  /** Epoch ms. */
  ts: number;
  /** Session id. */
  session: string;
  /** Activity kind (created/modified/removed/renamed/read). */
  kind: string;
  /** Absolute path. */
  path: string;
  /** Optional extra detail (e.g. rename target, diff summary). */
  detail?: string;
}

export interface FilterOptions {
  /** Keep only these kinds (all when omitted/empty). */
  kinds?: string[];
  /** Inclusive lower bound on `ts`. */
  since?: number;
  /** Inclusive upper bound on `ts`. */
  until?: number;
  /** Keep only events whose path contains this substring (case-insensitive). */
  pathIncludes?: string;
}

/** Filter events by kind / time-range / path substring. Pure. */
export function filterEvents(events: AuditEvent[], opts: FilterOptions = {}): AuditEvent[] {
  const kinds = opts.kinds && opts.kinds.length ? new Set(opts.kinds) : null;
  const needle = opts.pathIncludes?.toLowerCase();
  return events.filter((e) => {
    if (kinds && !kinds.has(e.kind)) return false;
    if (opts.since !== undefined && e.ts < opts.since) return false;
    if (opts.until !== undefined && e.ts > opts.until) return false;
    if (needle && !e.path.toLowerCase().includes(needle)) return false;
    return true;
  });
}

export function toJson(events: AuditEvent[]): string {
  return JSON.stringify(events, null, 2);
}

const COLUMNS = ["ts", "session", "kind", "path", "detail"] as const;

function csvCell(value: string): string {
  // Quote when the value contains a comma, quote, CR, or LF; double embedded quotes (RFC 4180).
  return /[",\r\n]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
}

export function toCsv(events: AuditEvent[]): string {
  const rows = [COLUMNS.join(",")];
  for (const e of events) {
    rows.push([String(e.ts), e.session, e.kind, e.path, e.detail ?? ""].map(csvCell).join(","));
  }
  return rows.join("\n");
}

function mdCell(value: string): string {
  // Escape pipes and collapse newlines so a cell can't break the table.
  return value.replace(/\|/g, "\\|").replace(/\r?\n/g, " ");
}

export function toMarkdown(events: AuditEvent[]): string {
  const header = `| ${COLUMNS.join(" | ")} |`;
  const sep = `| ${COLUMNS.map(() => "---").join(" | ")} |`;
  const body = events.map(
    (e) => `| ${[String(e.ts), e.session, e.kind, e.path, e.detail ?? ""].map(mdCell).join(" | ")} |`,
  );
  return [header, sep, ...body].join("\n");
}
