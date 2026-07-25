/**
 * Validate a proposed file or folder name. Returns a human-readable error
 * message if the name is invalid, or `null` if it is acceptable.
 *
 * The rules follow the strictest common denominator (Windows), applied on every
 * platform so behaviour is consistent: the backend filesystem call remains the
 * final authority, but this catches the obvious cases up front with a clear
 * message instead of a raw OS error.
 */

// Characters Windows forbids in a path component.
const ILLEGAL_CHARS = /[<>:"/\\|?*]/;

// Windows reserved device names (any casing, with or without an extension).
const RESERVED = new Set([
  "con", "prn", "aux", "nul",
  "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
  "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
]);

export function validateFileName(name: string): string | null {
  const trimmed = name.trim();

  if (trimmed === "") {
    return "Name cannot be empty.";
  }

  if (ILLEGAL_CHARS.test(name)) {
    return 'A name can\'t contain any of these characters: \\ / : * ? " < > |';
  }

  // Control characters (0–31) are also invalid in path components.
  for (let i = 0; i < name.length; i++) {
    if (name.charCodeAt(i) < 32) {
      return "A name can't contain control characters.";
    }
  }

  // A trailing dot or space is silently stripped by Windows and causes surprises.
  if (/[ .]$/.test(name)) {
    return "A name can't end with a space or a period.";
  }

  // Reserved device name — compare the part before the first dot.
  const stem = trimmed.split(".")[0].toLowerCase();
  if (RESERVED.has(stem)) {
    return `"${trimmed}" is a reserved name and can't be used.`;
  }

  return null;
}
