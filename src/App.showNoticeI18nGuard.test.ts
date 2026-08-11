/**
 * CPE-1627 regrowth guard: keeps `showNotice()` calls in App.svelte routed through `$t()`, so the
 * 45-string localisation debt this ticket paid off can't silently regrow one new hardcoded literal at a
 * time. A `showNotice("...")` / `showNotice('...')` call — a raw string literal as the first argument —
 * bypasses the catalog entirely, exactly the bug this ticket fixed (a notice like that stays English in
 * all 11 non-English locales no matter what the user's language setting is).
 *
 * Escape hatch: a call that is genuinely not user-facing (e.g. a debug-only path no real user ever sees)
 * can be exempted by appending `// i18n-exempt: <reason>` on the SAME line as the `showNotice(` call —
 * the marker is deliberately loud and greppable (`grep -n i18n-exempt src/App.svelte`) so a reviewer can
 * audit every use of the escape hatch; it is not a way to quietly opt out of translating something a user
 * will actually read.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const appSveltePath = path.join(path.dirname(fileURLToPath(import.meta.url)), "App.svelte");
const source = readFileSync(appSveltePath, "utf-8");

describe("showNotice() i18n regrowth guard (CPE-1627)", () => {
  it("has no un-exempted showNotice(...) call whose first argument is a raw string literal", () => {
    const offenders: string[] = [];
    source.split("\n").forEach((line, i) => {
      // A raw `"` or `'` immediately (modulo whitespace) after `showNotice(` means the message bypasses
      // $t() — e.g. showNotice("Some text") or showNotice('Some text'). showNotice($t("key")) and
      // showNotice(`templated ${x}`) don't match: the character right after `(` is `$` or a backtick.
      if (/showNotice\(\s*["']/.test(line) && !line.includes("i18n-exempt")) {
        offenders.push(`line ${i + 1}: ${line.trim()}`);
      }
    });
    expect(offenders).toEqual([]);
  });
});
