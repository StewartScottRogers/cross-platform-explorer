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
