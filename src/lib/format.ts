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
