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

// Unicode bidi/format control characters that can change how a name RENDERS without changing a
// single byte of it (CPE-1712). `U+202E RIGHT-TO-LEFT OVERRIDE` is the reported case — a remote leaf
// `<RLO>gnp.txt` downloads intact and Windows Explorer displays it as `txt.png`, because RLO tells
// the bidi renderer to draw everything after it right-to-left. It is Unicode category **Cf** (format),
// not **Cc** (control), so a `char::is_control()`-style guard never sees it — the same gap CPE-1709
// closed for `Cc`/Windows-illegal characters left wide open here.
//
// The full set, enumerated rather than stopping at the reported character (CPE-1709's own lesson,
// stated explicitly in this ticket): the five embeddings/overrides `U+202A`-`U+202E`, the four
// isolates `U+2066`-`U+2069`, and the two directional marks `U+200E`/`U+200F`.
//
// Built from `String.fromCharCode`, deliberately, rather than written as literal characters or even
// `\uXXXX` string escapes in this file: a literal RLO/LRO sitting in the source would itself reorder
// the surrounding source text for anyone reading or diffing it -- the exact hazard this ticket exists
// to defuse -- and would be invisible to a reviewer scanning the diff. Building it from decimal code
// points keeps this file itself plain ASCII, auditable, and immune to the very trick it detects.
const BIDI_FORMAT_CODES: Record<number, string> = {
  0x202a: "LRE", 0x202b: "RLE", 0x202c: "PDF", 0x202d: "LRO", 0x202e: "RLO",
  0x2066: "LRI", 0x2067: "RLI", 0x2068: "FSI", 0x2069: "PDI",
  0x200e: "LRM", 0x200f: "RLM",
};
const BIDI_FORMAT_NAMES: Record<string, string> = Object.fromEntries(
  Object.entries(BIDI_FORMAT_CODES).map(([code, abbr]) => [String.fromCharCode(Number(code)), abbr])
);
const BIDI_FORMAT_CLASS = Object.keys(BIDI_FORMAT_CODES)
  .map((code) => String.fromCharCode(Number(code)))
  .join("");
const HAS_BIDI_FORMAT = new RegExp(`[${BIDI_FORMAT_CLASS}]`);
const BIDI_FORMAT_G = new RegExp(`[${BIDI_FORMAT_CLASS}]`, "g");

/**
 * The text to actually DRAW for a file/folder name (CPE-1712) — never the bytes on disk or the
 * remote name, and never what a rename input edits. Every bidi/format control character is replaced
 * by its bracketed three-letter abbreviation (`[RLO]`, `[LRM]`, …), so `\u{202E}gnp.txt` renders as
 * the literal `[RLO]gnp.txt` — the TRUE byte order, plus a visible flag that something unusual is
 * embedded — instead of letting the browser's own bidi algorithm draw it as the deceptive `txt.png`.
 *
 * **Decision, recorded here because the on-disk half of it lives in a different language:** the
 * on-disk/remote name is deliberately left untouched (`crates/server/src/transfer.rs`, next to
 * `windows_safe_segment`) — these code points are legal on every target filesystem, so nothing forces
 * a rewrite the way an actual write failure forced CPE-1709's, and a real Arabic/Hebrew filename can
 * legitimately carry `LRM`/`RLM` at a script boundary (commonly right before a Latin extension) that
 * an eager on-disk rewrite would mangle for users who never see the spoof it exists to stop. Rendering
 * is a presentation choice, reversible on every redraw and touching no one's data, which is exactly
 * why it is the more surgical of the two levers this ticket asks about.
 *
 * Deliberately NOT applied to a rename input's *value* — a user editing a genuinely RTL name must see
 * and edit their real characters, not this escaped stand-in.
 */
export function displaySafeName(name: string): string {
  if (!HAS_BIDI_FORMAT.test(name)) return name;
  return name.replace(BIDI_FORMAT_G, (c) => `[${BIDI_FORMAT_NAMES[c]}]`);
}

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
