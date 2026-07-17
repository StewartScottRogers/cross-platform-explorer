// CPE-549: boundary guard for the busy cursor (CPE-547). Production code must reach Tauri `invoke`
// through the busy-tracking wrapper (src/lib/invoke.ts), NOT raw `@tauri-apps/api/core`, so a slow
// command anywhere raises the wait cursor. This test fails CI if a new call site bypasses the wrapper —
// turning the one-time coverage sweep into an enforced invariant. Streaming / self-progress sites that
// legitimately opt out (CPE-550) go in the allowlist below, each justified in that ticket.
import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";

const SRC = join(process.cwd(), "src");

/** The busy-cursor wrapper itself is the ONE place allowed to import raw core `invoke`. */
const WRAPPER = "lib/invoke.ts";

/**
 * Opt-out allowlist (CPE-550): src-relative POSIX paths permitted to import raw `invoke` because they
 * show their own progress and must not double-signal. Add entries here (one place) with a justification
 * recorded in CPE-550. Empty = every production call site currently routes through the wrapper.
 */
export const INVOKE_OPTOUT_ALLOWLIST: string[] = [];

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(ts|svelte)$/.test(name) && !/\.test\.ts$/.test(name)) out.push(p);
  }
  return out;
}

const CORE_IMPORT = /import\s*\{([^}]*)\}\s*from\s*["']@tauri-apps\/api\/core["']/g;

/** True if `src` has an `import { ... invoke ... } from "@tauri-apps/api/core"` (ignoring convertFileSrc etc.). */
function importsRawInvoke(src: string): boolean {
  CORE_IMPORT.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = CORE_IMPORT.exec(src)) !== null) {
    const names = m[1].split(",").map((s) => s.trim().split(/\s+as\s+/)[0].trim());
    if (names.includes("invoke")) return true;
  }
  return false;
}

describe("busy-cursor boundary guard (CPE-549)", () => {
  it("no production file imports raw `invoke` from @tauri-apps/api/core outside the allowlist", () => {
    const allow = new Set([WRAPPER, ...INVOKE_OPTOUT_ALLOWLIST]);
    const offenders = walk(SRC)
      .filter((p) => importsRawInvoke(readFileSync(p, "utf8")))
      .map((p) => relative(SRC, p).split(sep).join("/"))
      .filter((rel) => !allow.has(rel));
    expect(
      offenders,
      `these must import { invoke } from the wrapper (src/lib/invoke.ts) or be allowlisted: ${offenders.join(", ")}`,
    ).toEqual([]);
  });
});
