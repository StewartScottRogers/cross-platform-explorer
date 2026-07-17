// Integrated workbench — browser URL validation (CPE-527). The embedded browser opens in a dedicated
// webview window (safe under the app's strict CSP — not an iframe), but only for http/https/localhost.
// Pure + unit-tested.

/** Add an `http://` scheme to a bare host/localhost/IP; leave an existing scheme untouched. */
export function normalizeUrl(input: string): string {
  const t = input.trim();
  if (!t) return "";
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(t)) return t; // already has a scheme
  if (/^(localhost|\d{1,3}(\.\d{1,3}){3})(:\d+)?(\/|$)/i.test(t)) return "http://" + t;
  return "http://" + t; // best-effort: treat a bare token as an http host
}

/** True only for a well-formed http/https URL (after normalization). No file:, javascript:, etc. */
export function isBrowsableUrl(input: string): boolean {
  try {
    const url = new URL(normalizeUrl(input));
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

// --- Workbench diff view state (CPE-535): friendly edge cases. ------------------------------------
export type WbState = "loading" | "no-folder" | "git-missing" | "not-a-repo" | "error" | "clean" | "changes";

/** Which state the Diff view should show, from the load result. Pure — drives the friendly messages. */
export function workbenchState(opts: {
  loading?: boolean;
  error?: string;
  isRepo?: boolean;
  fileCount?: number;
}): WbState {
  if (opts.loading) return "loading";
  if (opts.error) {
    if (opts.error === "no-folder") return "no-folder";
    if (opts.error.startsWith("git-missing")) return "git-missing";
    return "error";
  }
  if (!opts.isRepo) return "not-a-repo";
  return (opts.fileCount ?? 0) > 0 ? "changes" : "clean";
}
