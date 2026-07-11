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
