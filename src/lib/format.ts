/**
 * Format a byte count as a human-readable size string.
 * Returns an empty string for 0 bytes (used for directories, which report size 0).
 */
export function formatSize(bytes: number): string {
  if (bytes === 0) return "";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let n = bytes;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

/**
 * "12.3 GB free of 500 GB" for the status bar (CPE-403). Empty when total is unknown/0, so callers
 * can render it unconditionally. Unlike {@link formatSize}, a zero component still shows "0 B".
 */
export function formatDiskFree(free: number, total: number): string {
  if (!total || total <= 0) return "";
  const size = (n: number) => (n <= 0 ? "0 B" : formatSize(n));
  return `${size(free)} free of ${size(total)}`;
}

/**
 * Used-percentage + severity for a drive usage bar (CPE-406). `severity` drives the bar colour:
 * "full" when under 5% free, "warn" under 15%, else "ok". Guards a missing/zero total.
 */
export function diskUsage(
  free: number,
  total: number,
): { usedPct: number; severity: "ok" | "warn" | "full" } {
  if (!total || total <= 0) return { usedPct: 0, severity: "ok" };
  const freeRatio = Math.max(0, Math.min(1, free / total));
  const usedPct = Math.round((1 - freeRatio) * 100);
  const severity = freeRatio < 0.05 ? "full" : freeRatio < 0.15 ? "warn" : "ok";
  return { usedPct, severity };
}

/**
 * Map a raw backend error string to a friendly, user-facing message.
 */
export function friendlyError(raw: string): string {
  const lower = raw.toLowerCase();
  if (
    lower.includes("denied") ||
    lower.includes("os error 5") || // Windows: access is denied
    lower.includes("os error 13") // Unix: permission denied
  ) {
    return "Can't open this folder — permission denied.";
  }
  if (lower.includes("os error 2") || lower.includes("not found")) {
    return "This folder no longer exists.";
  }
  return "Can't open this folder.";
}

/**
 * Format paths for the OS clipboard the way Explorer's "Copy as path" does:
 * each path wrapped in double quotes, one per line.
 */
export function formatPathsForClipboard(paths: string[]): string {
  return paths.map((p) => `"${p}"`).join("\n");
}

export interface PathSegment {
  /** Display label for this segment (e.g. "Users", or the drive/root). */
  name: string;
  /** Absolute path this segment navigates to when clicked. */
  path: string;
}

/**
 * Split an absolute path into cumulative, navigable breadcrumb segments.
 * Handles both Windows (`C:\Users\me`) and POSIX (`/home/me`) paths, including
 * forward slashes used on Windows.
 */
export function splitPath(fullPath: string): PathSegment[] {
  if (!fullPath) return [];

  // UNC path (\\server\share\...) — accept either slash style for the leading pair.
  if (/^[\\/]{2}/.test(fullPath)) {
    const sep = "\\";
    const parts = fullPath
      .replace(/\//g, sep)
      .split(sep)
      .filter((p) => p.length > 0);

    const server = parts.shift();
    const share = parts.shift();
    // A bare "\\server" with no share isn't a navigable location; treat the
    // server (plus share if present) as the root segment.
    const root = sep + sep + [server, share].filter(Boolean).join(sep);
    const segments: PathSegment[] = [{ name: root, path: root }];

    let acc = root;
    for (const part of parts) {
      acc = acc + sep + part;
      segments.push({ name: part, path: acc });
    }
    return segments;
  }

  const isWindows = /^[a-zA-Z]:/.test(fullPath);

  if (isWindows) {
    const sep = "\\";
    const parts = fullPath
      .replace(/\//g, sep)
      .split(sep)
      .filter((p) => p.length > 0);

    const drive = parts.shift() as string; // e.g. "C:"
    const segments: PathSegment[] = [{ name: drive, path: drive + sep }];

    let acc = drive + sep;
    for (const part of parts) {
      acc = acc.endsWith(sep) ? acc + part : acc + sep + part;
      segments.push({ name: part, path: acc });
    }
    return segments;
  }

  // POSIX
  const parts = fullPath.split("/").filter((p) => p.length > 0);
  const segments: PathSegment[] = [{ name: "/", path: "/" }];

  let acc = "";
  for (const part of parts) {
    acc = `${acc}/${part}`;
    segments.push({ name: part, path: acc });
  }
  return segments;
}
