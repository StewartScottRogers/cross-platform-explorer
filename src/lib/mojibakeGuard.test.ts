// CPE-1771: the whole-repo mojibake guard. Its predecessor scans were directory-scoped -- CPE-1752's
// tree scan covered only `crates/`, `src-tauri/src/`, `src/`, `docs/`, which is exactly the boundary
// `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` sat just outside of (they're under `src-tauri/`
// but not `src-tauri/src/`). This guard walks the tree with a small, justified exclusion list instead of
// a hand-picked inclusion list, so the next stray file can't hide in a directory nobody thought to scan.
//
// Two kinds of test live here:
//   - Unit tests of `findMojibake`/`hasLeadingBom` against synthetic strings (fast, always-relevant sanity
//     that the detector itself is correct -- independent of what the repo currently contains).
//   - The tree-wide scan, which is the actual CI guard: it fails if ANY scanned file contains the
//     mojibake signature or opens with a UTF-8 BOM, unless the exact (file, line) is in `ALLOWLIST` with
//     a recorded reason.
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, extname, relative } from "node:path";
import { findMojibake, hasLeadingBom, mojibakeRegex } from "./mojibakeGuard";

const ROOT = process.cwd();

describe("mojibakeRegex / findMojibake (CPE-1771)", () => {
  it("catches the em-dash mojibake this repo has actually shipped", () => {
    // "\u00e2\u20ac\u201d" is the exact three-character corruption em dash (U+2014) turns into when its
    // UTF-8 bytes (E2 80 94) are misread as CP1252 -- this is what src-tauri/Cargo.toml and
    // src-tauri/tauri.conf.json contained before CPE-1771's repair.
    const corrupted = "A normal dependency \u00e2\u20ac\u201d it pulls only serde.";
    expect(findMojibake(corrupted)).toEqual([{ line: 1, match: "\u00e2\u20ac" }]);
  });

  it("catches the ellipsis mojibake found in src-tauri/Cargo.toml", () => {
    const corrupted = "a remote URI (sftp://\u00e2\u20ac\u00a6, webdav://\u00e2\u20ac\u00a6)";
    expect(findMojibake(corrupted)).toHaveLength(2);
  });

  it("catches the arrow mojibake found in CLAUDE.md and crates/server/src/dispatch.rs", () => {
    // "\u00e2\u2020\u2019" is what a rightwards arrow (U+2192, UTF-8 E2 86 92) turns into the same way.
    const corrupted = "window (`start \u00e2\u2020\u2019 end`)";
    expect(findMojibake(corrupted)).toEqual([{ line: 1, match: "\u00e2\u2020" }]);
  });

  it("reports 1-based line numbers across multiple lines", () => {
    const corrupted = "clean line one\nclean line two\nbad \u00e2\u20ac\u201d line three";
    expect(findMojibake(corrupted).map((o) => o.line)).toEqual([3]);
  });

  it("does NOT flag a bare accented letter used as an ordinary letter", () => {
    // Romanian, Portuguese, French all use U+00E2 ("a" with circumflex) as a normal letter. None of
    // these are followed by a CP1252 upper-range artifact character, so none should match.
    const legit = ["Rom\u00e2n\u0103", "C\u00e2mera", "Dist\u00e2ncia focal", "T\u00e2ches"];
    for (const s of legit) {
      expect(findMojibake(s), s).toEqual([]);
    }
  });

  it("does NOT flag the real CPE-1771 false positive: i18n.ts's Portuguese \"N\u00c3O\"", () => {
    // \u00c3 (A-tilde) followed by a plain ASCII letter is real Portuguese, not a double-encoded artifact.
    expect(findMojibake('"prop.noMatchTip": "O arquivo N\u00c3O corresponde"')).toEqual([]);
  });

  it("mojibakeRegex() returns a fresh global regex each call (no shared lastIndex state)", () => {
    const a = mojibakeRegex();
    const b = mojibakeRegex();
    expect(a).not.toBe(b);
    expect(a.global).toBe(true);
  });
});

describe("hasLeadingBom (CPE-1771)", () => {
  it("detects a UTF-8 BOM", () => {
    expect(hasLeadingBom(new Uint8Array([0xef, 0xbb, 0xbf, 0x7b]))).toBe(true);
  });

  it("does not false-positive on ordinary content", () => {
    expect(hasLeadingBom(new Uint8Array([0x7b, 0x0a, 0x20]))).toBe(false);
  });

  it("does not throw on a too-short buffer", () => {
    expect(hasLeadingBom(new Uint8Array([0xef]))).toBe(false);
    expect(hasLeadingBom(new Uint8Array([]))).toBe(false);
  });
});

// ---------------------------------------------------------------------------------------------------
// The tree-wide CI guard.
// ---------------------------------------------------------------------------------------------------

/** Directories never scanned, each for a stated reason -- NOT a directory-scoped allowlist of the kind
 *  CPE-1771 exists to close (that let the two manifests through by naming `src-tauri/src/` instead of
 *  `src-tauri/`). These are build output / vendored / out-of-repo-governance trees, not shipped source. */
const EXCLUDE_DIRS = new Set([
  "node_modules", // installed deps, never hand-edited
  "dist", // build output
  "target", // Rust build output (src-tauri/target, crates/*/target, sidecar/*/target)
  ".git",
  "worktrees", // .claude/worktrees: gitignored, transient sub-agent checkouts (never present in a clean CI checkout)
  // Ticketing/Tickets & Ticketing/Epics: measured at CPE-1771 time to carry 683 pre-existing mojibake
  // occurrences across 14 files and a UTF-8 BOM in 12 files -- a much larger, separate cleanup (filed as
  // CPE-1784) than this ticket's two named manifests. Scanning it here would red CI for a backlog this
  // ticket didn't create and isn't sized to clear. Remove this exclusion when CPE-1784 lands.
  "Ticketing",
]);

/** File extensions worth scanning: source, config, and docs -- the same kind of content the two repaired
 *  manifests are. An INCLUDE list (not an exclude-binary list) on purpose: `samples/**`, `src-tauri/icons/
 *  **`, and vendored minified JS all contain byte sequences that coincidentally decode to the mojibake
 *  signature when read as UTF-8 text they aren't (binary image/audio bytes, or a keyboard-layout table
 *  whose legitimate accented-letter VALUES aren't followed by an artifact character, so they wouldn't
 *  match anyway -- but there's no reason to decode arbitrary binary as UTF-8 in the first place). */
const SCAN_EXTENSIONS = new Set([
  ".ts",
  ".tsx",
  ".js",
  ".jsx",
  ".cjs",
  ".mjs",
  ".svelte",
  ".rs",
  ".toml",
  ".json",
  ".md",
  ".yml",
  ".yaml",
  ".css",
  ".html",
  ".sh",
  ".ps1",
  ".cmd",
  ".txt",
]);

interface AllowlistEntry {
  file: string; // repo-relative, forward-slash
  line: number;
  reason: string;
  /** A distinctive substring the entry's line must still contain -- the staleness check below verifies
   *  this instead of re-matching the mojibake regex, so a genuine false-positive entry (which never
   *  matched the regex in the first place) doesn't read as "stale" just for being harmless. If a line no
   *  longer contains this substring, the entry has drifted (the file changed) and must be re-verified. */
  contains: string;
}

/** Exact (file, line) pairs excluded from the mojibake-signature check, each with a recorded reason --
 *  per CPE-1771's acceptance criteria, "allowlisted by location with a reason", not by weakening the
 *  regex. Two classes of entry:
 *   - Genuine false positives: text that legitimately looks like the signature but isn't corruption.
 *     These never actually match `mojibakeRegex()` (that's WHY they're not corruption) -- recorded here
 *     anyway, by exact location, so the "false positive" claim is auditable rather than just asserted.
 *   - Known, tracked, currently-out-of-scope real corruption: `crates/` was off-limits during CPE-1771's
 *     sprint slot (a concurrent worker was live in it), so these three lines ARE real, un-repaired
 *     mojibake that the guard would otherwise (correctly) fail on. Tracked via CPE-1783 instead of fixed
 *     here -- the reason says so plainly; it does not claim they're false positives. */
const ALLOWLIST: AllowlistEntry[] = [
  {
    file: "src/lib/i18n.ts",
    line: 5320,
    reason:
      'Legitimate Portuguese "NÃO" (N + A-tilde + O) -- Ã followed by the ASCII letter "O", not a CP1252 ' +
      "artifact character. Confirmed by the #927/CPE-1752 review and re-verified for CPE-1771.",
    contains: "NÃO",
  },
  {
    file: "src/lib/docs.ts",
    line: 25,
    reason:
      "Literal U+FEFF (BOM) character embedded in a regex literal (`raw.replace(/^\\uFEFF/, \"\")`) that " +
      "strips a BOM from loaded doc markdown -- deliberate BOM-handling code, not a mojibake artifact.",
    contains: "raw.replace(/^",
  },
  {
    file: "src/lib/epicsQueueLayout.test.ts",
    line: 24,
    reason: "Same literal U+FEFF BOM-stripping regex as src/lib/docs.ts:25, duplicated for this guard's own frontmatter parsing.",
    contains: "const body = md.replace(/^",
  },
  {
    file: "src/lib/epicsQueueLayout.test.ts",
    line: 35,
    reason: "Same literal U+FEFF BOM-stripping regex as src/lib/docs.ts:25, a second call site in this file.",
    contains: "md.replace(/^",
  },
  {
    file: "crates/server/src/dispatch.rs",
    line: 6,
    reason:
      "Real, un-repaired mojibake (an arrow, U+2192, corrupted the same way as CLAUDE.md's) that CPE-1752's " +
      "repair of this same file missed. crates/ was off-limits during CPE-1771's sprint slot (a concurrent " +
      "worker was live in it) -- tracked as CPE-1783, NOT a false positive. Remove this entry when CPE-1783 lands.",
    contains: "an unknown method",
  },
  {
    file: "crates/server/src/dispatch.rs",
    line: 7,
    reason: "Same CPE-1783 arrow corruption as line 6, a second occurrence two lines down.",
    contains: "deserialize",
  },
  {
    file: "crates/server/src/dispatch.rs",
    line: 11,
    reason: "Same CPE-1783 arrow corruption as line 6, a third occurrence.",
    contains: "A handler never panics the dispatcher.",
  },
];

function isAllowlisted(relFile: string, line: number): boolean {
  return ALLOWLIST.some((e) => e.file === relFile && e.line === line);
}

interface Violation {
  file: string;
  line: number;
  kind: "mojibake" | "bom";
  detail: string;
}

/** Recursively collect every file under `dir` (repo-relative paths, forward-slash) whose extension is in
 *  {@link SCAN_EXTENSIONS}, skipping {@link EXCLUDE_DIRS} at any depth. */
function collectScanFiles(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    if (EXCLUDE_DIRS.has(name)) continue;
    const full = join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) {
      out.push(...collectScanFiles(full));
    } else if (SCAN_EXTENSIONS.has(extname(name))) {
      out.push(full);
    }
  }
  return out;
}

function scanRepo(): Violation[] {
  const violations: Violation[] = [];
  for (const abs of collectScanFiles(ROOT)) {
    const relFile = relative(ROOT, abs).split("\\").join("/");
    const bytes = readFileSync(abs);
    if (hasLeadingBom(bytes) && !isAllowlisted(relFile, 1)) {
      violations.push({ file: relFile, line: 1, kind: "bom", detail: "file opens with a UTF-8 BOM (EF BB BF)" });
    }
    let text: string;
    try {
      text = bytes.toString("utf8");
    } catch {
      continue; // not decodable as UTF-8 at all -- not this guard's problem
    }
    for (const offender of findMojibake(text)) {
      if (isAllowlisted(relFile, offender.line)) continue;
      violations.push({
        file: relFile,
        line: offender.line,
        kind: "mojibake",
        detail: `contains the mojibake signature "${offender.match}"`,
      });
    }
  }
  return violations;
}

describe("repo-wide mojibake guard (CPE-1771)", () => {
  it("has no un-allowlisted mojibake signature or UTF-8 BOM anywhere in the scanned tree", () => {
    const violations = scanRepo();
    if (violations.length > 0) {
      const lines = violations.map((v) => `  ${v.file}:${v.line} -- ${v.detail}`).join("\n");
      throw new Error(
        `Found ${violations.length} un-allowlisted mojibake/BOM violation(s) (CPE-1771):\n${lines}\n\n` +
          "If this is a genuine corruption, repair it byte-exact (iconv/sed/python/an editor tool -- " +
          "never a PowerShell text round-trip, which is the root cause of this bug class). If it's a " +
          "deliberate literal (an example in a comment, a BOM-handling regex, non-English prose that " +
          "happens to share a lead byte), add it to ALLOWLIST in src/lib/mojibakeGuard.test.ts with a reason.",
      );
    }
    expect(violations).toEqual([]);
  });

  it("every ALLOWLIST entry still points at the line it describes (no stale entries)", () => {
    // An entry whose `contains` substring is no longer on the recorded line means the file moved on
    // without the allowlist -- either the corruption was fixed (delete the entry; see CPE-1783) or lines
    // shifted (re-point it). Checking `contains` rather than re-matching the mojibake regex means a
    // genuine false-positive entry (i18n.ts, docs.ts, epicsQueueLayout.test.ts -- none of which ever
    // matched the regex, that's WHY they're false positives) doesn't itself read as stale.
    const stale: string[] = [];
    for (const entry of ALLOWLIST) {
      const abs = join(ROOT, ...entry.file.split("/"));
      const text = readFileSync(abs, "utf8");
      const lineText = text.split(/\r?\n/)[entry.line - 1] ?? "";
      if (!lineText.includes(entry.contains)) stale.push(`${entry.file}:${entry.line}`);
    }
    expect(stale, `stale ALLOWLIST entries (no longer match their recorded content): ${stale.join(", ")}`).toEqual([]);
  });
});
