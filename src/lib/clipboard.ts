/**
 * In-app file clipboard: a set of paths plus whether they were cut or copied.
 * Pure and immutable so the paste rules can be unit-tested without touching disk.
 */
export type ClipMode = "copy" | "cut";

export interface Clipboard {
  paths: string[];
  mode: ClipMode;
}

export function emptyClipboard(): Clipboard {
  return { paths: [], mode: "copy" };
}

export function isEmpty(c: Clipboard): boolean {
  return c.paths.length === 0;
}

export function stage(paths: string[], mode: ClipMode): Clipboard {
  return { paths: [...paths], mode };
}

/** Is `path` currently staged for a cut? (Cut items render dimmed.) */
export function isCut(c: Clipboard, path: string): boolean {
  return c.mode === "cut" && c.paths.includes(path);
}

/** Normalise separators so parent/child comparisons work on both platforms. */
function norm(p: string): string {
  return p.replace(/\\/g, "/").replace(/\/+$/, "");
}

/** Is `dest` the same as, or inside, `src`? */
export function isSelfOrDescendant(src: string, dest: string): boolean {
  const s = norm(src);
  const d = norm(dest);
  return d === s || d.startsWith(s + "/");
}

export interface PasteCheck {
  allowed: boolean;
  reason: string;
}

/**
 * Can this clipboard be pasted into `dest`?
 *
 * The important rule: pasting a CUT folder into itself or one of its own
 * descendants would move a directory inside itself. The backend refuses it too,
 * but catching it here lets us say something useful instead of surfacing a
 * per-item error after the fact.
 */
export function canPaste(c: Clipboard, dest: string): PasteCheck {
  if (isEmpty(c)) {
    return { allowed: false, reason: "Clipboard is empty" };
  }
  if (!dest) {
    return { allowed: false, reason: "No destination folder" };
  }

  for (const p of c.paths) {
    if (isSelfOrDescendant(p, dest)) {
      const verb = c.mode === "cut" ? "move" : "copy";
      return {
        allowed: false,
        reason: `Can't ${verb} a folder into itself`,
      };
    }
  }

  // Cutting and pasting into the same folder is a no-op, not an error worth
  // performing — it would just churn the file.
  if (c.mode === "cut") {
    const allFromDest = c.paths.every((p) => {
      const parent = norm(p).split("/").slice(0, -1).join("/");
      return parent === norm(dest);
    });
    if (allFromDest) {
      return { allowed: false, reason: "Items are already in this folder" };
    }
  }

  return { allowed: true, reason: "" };
}
