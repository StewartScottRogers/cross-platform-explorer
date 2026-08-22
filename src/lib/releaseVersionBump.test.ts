// CPE-1841: scripts/release.ps1's version bump must rewrite ONE version per manifest -- the app's own --
// and leave every other version-shaped string in the file exactly as it found it.
//
// Before this ticket the three bumps were un-anchored `-replace` calls over the whole file text:
//
//     $pkg   = $pkg   -replace '("version"\s*:\s*")[^"]+(")',      "`${1}$Version`$2"
//     $conf  = $conf  -replace '("version"\s*:\s*")[^"]+(")',      "`${1}$Version`$2"
//     $cargo = $cargo -replace '(?m)^(version\s*=\s*")[^"]+(")',   "`${1}$Version`$2"
//
// None of the three was scoped to the key it meant, so a nested `"someTool": { "version": "3.2.1" }`,
// a `"wix": { "version": "3.11.2" }`, or a long-form `[dependencies.somepkg]` / `version = "1.2.3"` pin
// was silently rewritten to the release version. Dormant on today's manifests only because none of them
// happens to contain such a decoy -- and the long-form dependency table is the ordinary way to express a
// dependency with features, so the trap is one edit away on the least-exercised path in the repo.
//
// These tests drive the REAL scripts/release.ps1, copied into a throwaway scratch tree and invoked with
// its `-BumpOnly` switch (which stops before git add/commit/tag/push), rather than re-implementing its
// regexes in TypeScript -- a re-implementation would happily stay green while the shipped script rotted.
// Fixtures live in an OS temp directory, never in the repo tree, matching src/lib/mojibakeGuard.test.ts.
//
// Each decoy gets its OWN test. A single combined "leaves everything alone" case would go red as a unit
// and hide which of the three shapes actually regressed.
import { describe, it, expect, beforeAll, vi } from "vitest";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, copyFileSync, rmSync, chmodSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

// Every case here spawns a real PowerShell process, which idles at roughly 0.4-0.6s on this repo's
// Windows dev machines and is slower again under a loaded parallel run of the full 321-file suite.
// vitest's default per-test timeout is 5000ms and vite.config.ts does not override it, so one slow spawn
// is enough to red a test that is working perfectly. Raise it explicitly for this file rather than
// leaving a process-spawning suite on a default sized for pure-module tests.
vi.setConfig({ testTimeout: 60_000, hookTimeout: 120_000 });

const ROOT = process.cwd();
const RELEASE_PS1 = join(ROOT, "scripts", "release.ps1");

/** The PowerShell host to drive the script with: `pwsh` (PowerShell 7, what CI's ubuntu runner and this
 *  repo's own release workflow use) preferred, `powershell` (Windows PowerShell 5.1) as the local Windows
 *  fallback -- release.ps1 must behave identically on both, which is why either is acceptable here.
 *
 *  If NEITHER is present this throws rather than skipping. A skipped test that reports as a pass is the
 *  exact "succeeds having checked nothing" shape this repo keeps closing; if a runner ever loses its
 *  PowerShell, that must be visible as a red, not absorbed into a green run. */
function findPowerShellHost(): string {
  const tried: string[] = [];
  for (const exe of ["pwsh", "powershell"]) {
    tried.push(exe);
    const probe = spawnSync(exe, ["-NoProfile", "-Command", "exit 0"], { stdio: "ignore" });
    if (!probe.error && probe.status === 0) return exe;
  }
  throw new Error(
    `No PowerShell host found (tried: ${tried.join(", ")}). scripts/release.ps1 is a PowerShell script, ` +
      "so these tests cannot verify it without one. Install PowerShell 7 (`pwsh`) rather than letting this " +
      "suite skip -- a skipped guard reports as a pass.",
  );
}

let psHost: string;
beforeAll(() => {
  psHost = findPowerShellHost();
});

/** Manifest text with `\n` rewritten to `\r\n`. The real manifests are CRLF in the working tree
 *  (core.autocrlf=true on this repo), and CRLF preservation through the bump is part of what's under
 *  test, so the fixtures must be CRLF too -- an LF fixture would not exercise it. */
function crlf(text: string): string {
  return text.replace(/\r?\n/g, "\r\n");
}

/** The five files CLAUDE.md requires to carry the same version, keyed the way this file refers to them.
 *  `lock` and `cargoLock` (CPE-1853) default to the decoy fixtures below, so every pre-existing case
 *  keeps staging a complete, valid file set without restating them. */
interface Manifests {
  pkg: string;
  conf: string;
  cargo: string;
  lock?: string;
  cargoLock?: string;
}

/** One key per version-synchronised file. Everything that loops over "all of them" loops over THIS, so
 *  a sixth file is added in one place rather than in a dozen array literals. */
const FILE_KEYS = ["pkg", "conf", "cargo", "lock", "cargoLock"] as const;
type FileKey = (typeof FILE_KEYS)[number];

/** Repo-relative path of each, matching the paths release.ps1 and CLAUDE.md name. */
const FILE_PATHS: Record<FileKey, string> = {
  pkg: "package.json",
  conf: "src-tauri/tauri.conf.json",
  cargo: "src-tauri/Cargo.toml",
  lock: "package-lock.json",
  cargoLock: "src-tauri/Cargo.lock",
};

/** How many places in each file hold the app's own version. package-lock.json is the odd one: the root
 *  object's "version" AND packages[""]'s. Bumping one and not the other is the specific way that file
 *  goes stale, so the count is asserted rather than assumed. */
const VERSION_PLACES: Record<FileKey, number> = { pkg: 1, conf: 1, cargo: 1, lock: 2, cargoLock: 1 };

interface BumpResult {
  status: number | null;
  stdout: string;
  stderr: string;
  /** Raw bytes as written back, so BOM / CRLF / trailing-newline claims are checked at the byte level. */
  bytes: Record<FileKey, Buffer>;
  text: Record<FileKey, string>;
}

interface BumpOptions {
  /** Written as the script instead of copying the real one -- used only by the red-proof block below,
   *  which re-runs the verbatim pre-fix regexes. */
  scriptText?: string;
  /** Prefix each staged manifest's BYTES with a UTF-8 BOM (EF BB BF), to exercise release.ps1's
   *  BOM-preserving write path. */
  bom?: boolean;
  /** Drop the `-BumpOnly` switch, i.e. drive the FULL release path. CPE-1852 uses this to prove the
   *  abort-before-any-write property belongs to the shared writer rather than to the dry-run switch.
   *  Only ever used with a fixture that must FAIL validation: validation aborts before the first `git`
   *  call, so no repository is ever touched. (The scratch tree is an OS temp directory and not a git
   *  repo at all, so a regression that reached `git add` would die there rather than commit anything.) */
  bumpOnly?: boolean;
  /** chmod one staged manifest to 0444 AFTER staging it, so the PLAN phase can still read it and the
   *  WRITE phase fails on it. The only way to reach release.ps1's post-validation write failure --
   *  the one path where a partial bump is genuinely possible and all the script can do is report it
   *  truthfully. Restored to 0666 in the `finally` before rmSync, or cleanup fails on Windows. */
  readOnly?: FileKey;
  /** Stage this one file with LF line endings instead of CRLF. Not hypothetical: `src-tauri/Cargo.lock`
   *  and `package.json` are CRLF in a fresh checkout (core.autocrlf=true) and LF the moment `cargo build`
   *  or `npm install` rewrites them, so the bump has to be endings-agnostic rather than CRLF-assuming
   *  (CPE-1853). */
  lf?: FileKey;
}

/** Stage `manifests` in a throwaway tree, run the real release.ps1 over it with `-BumpOnly`, and read
 *  back what it wrote. */
function runBump(manifests: Manifests, version: string, opts: BumpOptions = {}): BumpResult {
  const { scriptText, bom = false, bumpOnly = true, readOnly, lf } = opts;
  const stage = (text: string, key: FileKey): Buffer => {
    const body = Buffer.from(key === lf ? text.replace(/\r\n/g, "\n") : crlf(text), "utf8");
    return bom ? Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), body]) : body;
  };
  const dir = mkdtempSync(join(tmpdir(), "cpe-1841-release-bump-"));
  let lockedPath: string | undefined;
  try {
    mkdirSync(join(dir, "scripts"));
    mkdirSync(join(dir, "src-tauri"));

    const scriptPath = join(dir, "scripts", "release.ps1");
    if (scriptText === undefined) copyFileSync(RELEASE_PS1, scriptPath);
    else writeFileSync(scriptPath, Buffer.from(scriptText, "utf8"));

    // CPE-1853: all FIVE version-synchronised files are staged, not three. release.ps1 now plans every
    // one of them before writing any, so a missing package-lock.json or Cargo.lock would abort the run
    // in the plan phase and red every unrelated case in this file with a "does not exist" message.
    const source: Record<FileKey, string> = {
      pkg: manifests.pkg,
      conf: manifests.conf,
      cargo: manifests.cargo,
      lock: manifests.lock ?? LOCK_WITH_DECOYS,
      cargoLock: manifests.cargoLock ?? CARGO_LOCK_WITH_DECOYS,
    };
    const paths = {} as Record<FileKey, string>;
    for (const key of FILE_KEYS) {
      paths[key] = join(dir, ...FILE_PATHS[key].split("/"));
      writeFileSync(paths[key], stage(source[key], key));
    }

    if (readOnly) {
      lockedPath = paths[readOnly];
      chmodSync(lockedPath, 0o444);
    }

    const args = ["-NoProfile", "-File", scriptPath, "-Version", version];
    if (bumpOnly) args.push("-BumpOnly");
    const run = spawnSync(psHost, args, { encoding: "utf8" });
    if (run.error) throw run.error;

    const bytes = {} as Record<FileKey, Buffer>;
    const text = {} as Record<FileKey, string>;
    for (const key of FILE_KEYS) {
      bytes[key] = readFileSync(paths[key]);
      text[key] = bytes[key].toString("utf8");
    }
    return { status: run.status, stdout: run.stdout ?? "", stderr: run.stderr ?? "", bytes, text };
  } finally {
    // Undo the chmod BEFORE rmSync: on Windows a read-only file survives a recursive delete and the
    // cleanup throws, turning a passing test into a confusing red in whatever runs next.
    if (lockedPath) {
      try {
        chmodSync(lockedPath, 0o666);
      } catch {
        /* best effort -- rmSync's force flag is the backstop */
      }
    }
    rmSync(dir, { recursive: true, force: true });
  }
}

/** PowerShell renders an uncaught `throw` through its own error formatter, which HARD-WRAPS the message
 *  to the host's console width -- and PowerShell 7 additionally interleaves ANSI colour escapes at the
 *  wrap points. So a phrase in the message can arrive split across lines, with escape sequences sitting
 *  in the gap, and the split lands in a different place on Windows PowerShell 5.1 than on `pwsh`. Any
 *  assertion about WORDING must therefore normalise first: strip the escapes, collapse whitespace runs.
 *
 *  Learned the hard way on CPE-1852 -- `/no manifest was written/` passed locally under 5.1 and reded on
 *  CI's ubuntu (`pwsh`) leg purely because the wrap fell between "No" and "manifest" there. */
function flat(text: string): string {
  // The ESC is written as the escape sequence \x1b, never as a raw control byte in this file, and
  // matched explicitly: a bare /\[[0-9;]*[A-Za-z]/ would eat the literal "[p" out of "[package]",
  // which is a phrase these very assertions check for.
  return (
    text
      // eslint-disable-next-line no-control-regex
      .replace(/\x1b\[[0-9;]*[A-Za-z]/g, "")
      // PowerShell 7's ConciseView prefixes every CONTINUATION line of a wrapped error with a "|"
      // gutter, so the wrap does not merely insert whitespace -- it inserts "| " mid-phrase. Windows
      // PowerShell 5.1 does not. Strip the gutter before collapsing whitespace, or the wording match
      // still misses, on "No | manifest was written".
      //
      // CAVEAT for anyone reusing flat(): this DOES destroy meaning when a line-leading "|" is
      // legitimate. A markdown table -- "| package.json | 1d81 |\n| conf | 65cb |" -- comes back as
      // " package.json | 1d81 | conf | 65cb |", row boundary silently gone. Nothing asserted in this
      // file depends on a leading "|", so it is safe HERE. It is not a general-purpose normaliser.
      .replace(/^[ \t]*\|[ \t]?/gm, " ")
      .replace(/\s+/g, " ")
  );
}

/** Everything a failed exit-status assertion needs to be diagnosable at a glance -- without it a red
 *  reads only as "expected 0, got 1" and says nothing about which manifest the script objected to. */
function ctx(r: BumpResult): string {
  return `release.ps1 exit=${r.status}
--- stdout ---
${r.stdout}
--- stderr ---
${r.stderr}`;
}

// ---------------------------------------------------------------------------------------------------
// The decoy fixtures. Deliberately shaped after real manifests: the app's own version, plus one
// version-shaped string of each kind the un-scoped regexes used to eat.
// ---------------------------------------------------------------------------------------------------

const OLD = "0.1.0";
const NEW = "9.9.9";

const PKG_WITH_DECOYS = `{
  "name": "demo",
  "version": "${OLD}",
  "description": "needs at least 4.5.6 of the thing",
  "homepage": "https://example.com/downloads/7.8.9/index.html",
  "devDependencies": {
    "someTool": { "version": "3.2.1" }
  }
}
`;

const CONF_WITH_DECOYS = `{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Demo",
  "version": "${OLD}",
  "bundle": {
    "windows": { "wix": { "version": "3.11.2" } },
    "shortDescription": "requires 6.5.4 at runtime",
    "homepage": "https://example.com/downloads/7.8.9/index.html"
  }
}
`;

const CARGO_WITH_DECOYS = `[package]
name = "demo"
version = "${OLD}"
description = "pins 4.5.6 internally, see https://example.com/v/7.8.9"
rust-version = "1.77.2"

[dependencies.somepkg]
version = "1.2.3"
features = ["a"]

[dependencies]
serde = "1"
`;

// ---------------------------------------------------------------------------------------------------
// CPE-1853's two fixtures: the lockfiles the script did not touch until this ticket.
//
// Both are shaped after the real files, and both carry a decoy whose version is EXACTLY the app's own
// (`OLD`). That is the case that matters: a locator scoped by "looks like the app version" rather than
// by identity would rewrite a dependency pin and leave no evidence, and in a LOCK file the pin is what
// actually gets resolved and built.
// ---------------------------------------------------------------------------------------------------

/** lockfileVersion 3, the shape npm 9+ writes and the shape the real package-lock.json has.
 *
 *  The app version lives in TWO places: the root object's "version" and `packages[""]`'s. The decoys:
 *   - `node_modules/decoy` sits at the SAME depth as `packages[""]`'s version and holds the SAME value,
 *     so nothing but the empty-string key distinguishes them;
 *   - `node_modules/other` holds a different pin plus a version-shaped string inside a funding URL;
 *   - `"deprecated"` carries `{`, `}` and an escaped `"` INSIDE a string, so a walker that tracks
 *     nesting without consuming string tokens miscounts depth from that point on and every later
 *     decision is wrong;
 *   - `"lockfileVersion": 3` is an unquoted number on a version-shaped key at the ROOT. */
const LOCK_WITH_DECOYS = `{
  "name": "demo",
  "version": "${OLD}",
  "lockfileVersion": 3,
  "requires": true,
  "packages": {
    "": {
      "name": "demo",
      "version": "${OLD}",
      "dependencies": { "decoy": "^0.1.0" },
      "engines": { "node": ">=18" }
    },
    "node_modules/decoy": {
      "version": "${OLD}",
      "resolved": "https://registry.npmjs.org/decoy/-/decoy-0.1.0.tgz",
      "deprecated": "use {legacy} instead -- see \\"docs\\"",
      "license": "MIT"
    },
    "node_modules/other": {
      "version": "3.2.1",
      "funding": [{ "type": "opencollective", "url": "https://opencollective.com/other/7.8.9" }]
    }
  }
}
`;

/** Cargo.lock's real shape: a `version = 3` format marker before any table, then `[[package]]` blocks.
 *
 *  The decoys, each of which a less-scoped locator would eat:
 *   - `decoy-crate`, a `[[package]]` at EXACTLY the app's version -- the case the ticket names;
 *   - `serde`, an ordinary pin;
 *   - `[[patch.unused]]` carrying the app's OWN NAME and version -- name-matching without checking the
 *     table type rewrites it, and it is not the package entry;
 *   - the top-of-file `version = 3`, which is not a package version at all;
 *   - `[metadata]`, whose keys embed version-shaped text. */
const CARGO_LOCK_WITH_DECOYS = `# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 3

[[package]]
name = "decoy-crate"
version = "${OLD}"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaaa"

[[package]]
name = "cross-platform-explorer"
version = "${OLD}"
dependencies = [
 "decoy-crate",
 "serde",
]

[[package]]
name = "serde"
version = "1.0.219"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[patch.unused]]
name = "cross-platform-explorer"
version = "${OLD}"
source = "registry+https://github.com/rust-lang/crates.io-index"

[metadata]
"checksum decoy-crate 0.1.0" = "aaaa"
`;

const ALL_DECOYS: Manifests = {
  pkg: PKG_WITH_DECOYS,
  conf: CONF_WITH_DECOYS,
  cargo: CARGO_WITH_DECOYS,
  lock: LOCK_WITH_DECOYS,
  cargoLock: CARGO_LOCK_WITH_DECOYS,
};

describe("release.ps1 bumps the version it means (CPE-1841)", () => {
  // ONE bump of the shared decoy fixture, reused by every assertion in this block. These nine cases each
  // ran the identical `runBump(ALL_DECOYS, NEW)` before -- nine identical PowerShell spawns producing
  // nine byte-identical results, for no added coverage, and nine more chances for one slow spawn to red
  // a working test. Hoisting it keeps each decoy on its own named test (so a regression still names
  // itself) while spawning once.
  let r: BumpResult;
  beforeAll(() => {
    r = runBump(ALL_DECOYS, NEW);
  });

  it("rewrites the top-level version in all three manifests", () => {
    expect(r.stderr, r.stderr).toBe("");
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.pkg).toContain(`"version": "${NEW}"`);
    expect(r.text.conf).toContain(`"version": "${NEW}"`);
    expect(r.text.cargo).toContain(`version = "${NEW}"`);
  });

  it("leaves a long-form Cargo dependency pin alone ([dependencies.somepkg] / version = \"1.2.3\")", () => {
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.cargo).toContain('[dependencies.somepkg]\r\nversion = "1.2.3"');
    // The [package] version DID move, so this isn't a vacuous pass on an untouched file.
    expect(r.text.cargo).toContain(`[package]\r\nname = "demo"\r\nversion = "${NEW}"`);
  });

  it('leaves a nested tool version alone in package.json ("someTool": { "version": "3.2.1" })', () => {
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.pkg).toContain('"someTool": { "version": "3.2.1" }');
    expect(r.text.pkg).toContain(`"version": "${NEW}"`);
  });

  it('leaves a nested tool version alone in tauri.conf.json ("wix": { "version": "3.11.2" })', () => {
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.conf).toContain('"wix": { "version": "3.11.2" }');
    expect(r.text.conf).toContain(`"version": "${NEW}"`);
  });

  it("leaves a version-shaped string inside a description alone, in every manifest", () => {
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.pkg).toContain('"description": "needs at least 4.5.6 of the thing"');
    expect(r.text.conf).toContain('"shortDescription": "requires 6.5.4 at runtime"');
    expect(r.text.cargo).toContain("pins 4.5.6 internally");
  });

  it("leaves a version-shaped string inside a URL alone, in every manifest", () => {
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.pkg).toContain("https://example.com/downloads/7.8.9/index.html");
    expect(r.text.conf).toContain("https://example.com/downloads/7.8.9/index.html");
    expect(r.text.cargo).toContain("https://example.com/v/7.8.9");
  });

  it("leaves Cargo's rust-version alone (a version-shaped value on a DIFFERENT key inside [package])", () => {
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.cargo).toContain('rust-version = "1.77.2"');
  });

  it("changes exactly ONE line per manifest, byte-for-byte identical everywhere else", () => {
    expect(r.status, ctx(r)).toBe(0);
    const before: Record<string, string> = { pkg: crlf(PKG_WITH_DECOYS), conf: crlf(CONF_WITH_DECOYS), cargo: crlf(CARGO_WITH_DECOYS) };
    for (const key of ["pkg", "conf", "cargo"] as const) {
      const a = before[key].split("\r\n");
      const b = r.text[key].split("\r\n");
      expect(b.length, `${key}: line count changed`).toBe(a.length);
      const changed = a.map((line, i) => (line === b[i] ? -1 : i)).filter((i) => i >= 0);
      expect(changed.length, `${key}: expected exactly one changed line, got ${changed.length}`).toBe(1);
      expect(b[changed[0]]).toContain(NEW);
    }
  });

  it("preserves CRLF, the trailing newline, and the absence of a BOM", () => {
    expect(r.status, ctx(r)).toBe(0);
    // All FIVE (CPE-1853), not just the three manifests: the lockfiles go through the same writer and
    // a re-encoded lockfile is exactly the kind of "unrelated noise" in a release diff this whole
    // sequence of tickets exists to prevent.
    for (const key of FILE_KEYS) {
      const buf = r.bytes[key];
      const lf = buf.filter((b) => b === 0x0a).length;
      const crlfCount = [...buf].filter((b, i) => b === 0x0a && i > 0 && buf[i - 1] === 0x0d).length;
      expect(lf, `${key}: a lone LF appeared`).toBe(crlfCount);
      expect(buf.subarray(buf.length - 2).toString("latin1"), `${key}: trailing newline lost`).toBe("\r\n");
      // Byte-level, not a U+FEFF string literal -- writing one into this file would put a BOM character
      // in the repo tree that src/lib/mojibakeGuard.test.ts then has to reason about.
      expect([...buf.subarray(0, 3)], `${key}: a BOM was added`).not.toEqual([0xef, 0xbb, 0xbf]);
    }
  });
});

// ---------------------------------------------------------------------------------------------------
// The BOM-preserving write path (release.ps1's `$hadBom` branch).
//
// `[System.IO.File]::ReadAllText($path, $encoding)` does NOT honour its encoding argument for BOM
// purposes: the underlying StreamReader auto-detects and STRIPS a leading BOM even when an explicit
// BOM-less UTF8Encoding is passed. So a naive read-modify-write silently DELETES a BOM the file had --
// a second changed line in a supposedly one-line version bump, and an encoding change nobody asked a
// release script to make. release.ps1 sniffs the raw bytes and writes back with UTF8Encoding($true) when
// the source had one.
//
// Not a live case (no manifest carries a BOM today), but it is on the release path, so it gets a guard
// rather than only a comment. The non-ASCII payload below is the CPE-1834 half of the same story: a
// BOM'd file must come back with its multi-byte characters intact too, not merely its BOM -- those are
// separate failure modes (BOM handling vs. the ANSI/CP1252 round trip) that a BOM-only assertion would
// not tell apart.
// ---------------------------------------------------------------------------------------------------

/** Characters chosen to span the encodings that break differently: 2-byte accented Latin, the CP1252
 *  symbol block (em dash -- the exact character CPE-1834 measured collapsing to a single 0x97 byte), a
 *  3-byte CJK pair CP1252 cannot represent at all, and a 4-byte astral emoji. Written as escapes rather
 *  than literal glyphs so this file stays plain ASCII on disk and never has to be reasoned about by
 *  src/lib/mojibakeGuard.test.ts. */
const NON_ASCII = "caf\u00e9 \u2014 \u4e2d\u6587 \ud83c\udf89";

const BOM_PKG = `{
  "name": "demo",
  "version": "${OLD}",
  "description": "${NON_ASCII}",
  "devDependencies": {
    "someTool": { "version": "3.2.1" }
  }
}
`;

describe("release.ps1 preserves a manifest's existing BOM (CPE-1841 / CPE-1834)", () => {
  let r: BumpResult;
  beforeAll(() => {
    r = runBump({ ...ALL_DECOYS, pkg: BOM_PKG }, NEW, { bom: true });
  });

  it("still bumps the top-level version in a BOM'd manifest", () => {
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.pkg).toContain(`"version": "${NEW}"`);
  });

  it("leaves the BOM bytes (EF BB BF) at offset 0 instead of silently stripping them", () => {
    expect(r.status, ctx(r)).toBe(0);
    for (const key of FILE_KEYS) {
      expect([...r.bytes[key].subarray(0, 3)], `${key}: the source BOM was dropped`).toEqual([0xef, 0xbb, 0xbf]);
    }
  });

  it("adds exactly ONE BOM, not a second one in front of the first", () => {
    // UTF8Encoding($true) emits a preamble of its own; if the string handed to it still carried the
    // U+FEFF the reader had stripped, the file would come back with EF BB BF EF BB BF.
    expect([...r.bytes.pkg.subarray(3, 6)], "a second BOM was prepended").not.toEqual([0xef, 0xbb, 0xbf]);
    expect(r.bytes.pkg.subarray(3, 4).toString("latin1"), "content should resume at '{' right after the BOM").toBe("{");
  });

  it("carries the non-ASCII payload through byte-exact (no CP1252 round trip, no mojibake)", () => {
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.pkg).toContain(`"description": "${NON_ASCII}"`);
    // And at byte level: the em dash must still be its 3-byte UTF-8 sequence, not CPE-1834's lone 0x97.
    expect(r.bytes.pkg.includes(Buffer.from([0xe2, 0x80, 0x94])), "em dash lost its UTF-8 encoding").toBe(true);
    expect(r.bytes.pkg.includes(0x97), "an em dash was re-encoded as the CP1252 byte 0x97").toBe(false);
  });

  it("still changes exactly one line, with CRLF and the trailing newline intact", () => {
    expect(r.status, ctx(r)).toBe(0);
    // The BOM is staged at the BYTE level, so the decoded text opens with U+FEFF while the fixture
    // string does not. Assert that difference explicitly and account for it, rather than letting it
    // count as a second changed line and quietly weakening the one-line claim to "at most two".
    const FEFF = String.fromCharCode(0xfeff);
    expect(r.text.pkg.startsWith(FEFF), "decoded text should open with exactly one U+FEFF").toBe(true);
    expect(r.text.pkg.slice(1).startsWith(FEFF), "a second U+FEFF followed the first").toBe(false);
    const a = crlf(BOM_PKG).split("\r\n");
    const b = r.text.pkg.slice(1).split("\r\n");
    expect(b.length).toBe(a.length);
    const changed = a.map((line, i) => (line === b[i] ? -1 : i)).filter((i) => i >= 0);
    expect(changed.length, `expected exactly one changed line, got ${changed.length}`).toBe(1);
    expect(b[changed[0]]).toContain(NEW);
    expect(r.bytes.pkg.subarray(r.bytes.pkg.length - 2).toString("latin1")).toBe("\r\n");
  });

  it("does NOT add a BOM to a manifest that never had one (the branch is conditional, not unconditional)", () => {
    // The companion to the cases above: identical fixture, `bom: false`. Without it, a `$hadBom` stuck
    // at $true -- or an unconditional UTF8Encoding($true) write -- would keep every test above green.
    const clean = runBump({ ...ALL_DECOYS, pkg: BOM_PKG }, NEW, { bom: false });
    expect(clean.status, ctx(clean)).toBe(0);
    expect([...clean.bytes.pkg.subarray(0, 3)]).not.toEqual([0xef, 0xbb, 0xbf]);
    expect(clean.text.pkg).toContain(`"description": "${NON_ASCII}"`);
  });
});

describe("release.ps1 fails loudly rather than bumping nothing (CPE-1841)", () => {
  // Before this ticket the bumps were `-replace` calls, which return the input unchanged when nothing
  // matches. A manifest whose version key had been renamed, removed, or restructured was written back
  // byte-identical and the script still printed "Bumped version to X" and exited 0.
  const NO_VERSION_PKG = `{\n  "name": "demo"\n}\n`;
  const NO_VERSION_CONF = `{\n  "productName": "Demo"\n}\n`;
  const NO_VERSION_CARGO = `[package]\nname = "demo"\n\n[dependencies]\nserde = "1"\n`;

  it("exits non-zero when package.json has no top-level version key", () => {
    const r = runBump({ ...ALL_DECOYS, pkg: NO_VERSION_PKG }, NEW);
    expect(r.status, ctx(r)).not.toBe(0);
    expect(r.stderr + r.stdout).toMatch(/expected exactly one/);
    expect(r.text.pkg).toBe(crlf(NO_VERSION_PKG)); // and wrote nothing
  });

  it("exits non-zero when tauri.conf.json has no top-level version key", () => {
    const r = runBump({ ...ALL_DECOYS, conf: NO_VERSION_CONF }, NEW);
    expect(r.status, ctx(r)).not.toBe(0);
    expect(r.stderr + r.stdout).toMatch(/expected exactly one/);
    expect(r.text.conf).toBe(crlf(NO_VERSION_CONF));
  });

  it("exits non-zero when Cargo.toml's [package] table has no version", () => {
    const r = runBump({ ...ALL_DECOYS, cargo: NO_VERSION_CARGO }, NEW);
    expect(r.status, ctx(r)).not.toBe(0);
    expect(r.stderr + r.stdout).toMatch(/expected exactly one/);
    expect(r.text.cargo).toBe(crlf(NO_VERSION_CARGO));
  });

  it("exits non-zero on an AMBIGUOUS manifest (two top-level version keys) instead of picking one", () => {
    const ambiguous = `{\n  "version": "${OLD}",\n  "name": "demo",\n  "version": "${OLD}"\n}\n`;
    const r = runBump({ ...ALL_DECOYS, pkg: ambiguous }, NEW);
    expect(r.status, ctx(r)).not.toBe(0);
    expect(r.stderr + r.stdout).toMatch(/expected exactly one/);
    expect(r.text.pkg).toBe(crlf(ambiguous));
  });

  it("exits non-zero on two version lines inside Cargo's [package] table", () => {
    const ambiguous = `[package]\nname = "demo"\nversion = "${OLD}"\nversion = "${OLD}"\n\n[dependencies]\nserde = "1"\n`;
    const r = runBump({ ...ALL_DECOYS, cargo: ambiguous }, NEW);
    expect(r.status, ctx(r)).not.toBe(0);
    expect(r.stderr + r.stdout).toMatch(/expected exactly one/);
    expect(r.text.cargo).toBe(crlf(ambiguous));
  });

  it("exits non-zero when Cargo.toml has no [package] table at all", () => {
    const noPackage = `[dependencies.somepkg]\nversion = "1.2.3"\n`;
    const r = runBump({ ...ALL_DECOYS, cargo: noPackage }, NEW);
    expect(r.status, ctx(r)).not.toBe(0);
    // Specifically the "found 0" message, not some incidental crash: the no-[package] early return
    // has to hand the caller ZERO hits. Returning `, $hits` there would hand it a one-element array
    // wrapping the empty list, which reads as "found 1" and then dies on a null offset instead.
    expect(r.stderr + r.stdout).toMatch(/expected exactly one .*found 0/s);
    expect(r.text.cargo).toBe(crlf(noPackage));
  });
});

// ---------------------------------------------------------------------------------------------------
// CPE-1852: the bump is ALL-OR-NOTHING across the manifest set.
//
// CPE-1841 gave each manifest a guard that throws before writing THAT file. But the writer ran per file,
// in order, so a failure on the third manifest landed after the first two were already on disk:
//
//     package.json     -> 9.9.9   (written)
//     tauri.conf.json  -> 9.9.9   (written)
//     Cargo.toml       -> 0.1.0   (guard fired)
//     exit 1, "... found 0. Refusing to write ..."
//
// The message is true of the file it names and reads as though nothing was written. Two of the five
// version-synchronised files are then bumped, on disk, uncommitted -- the dirty-tree-reads-as-noise
// hazard CLAUDE.md records by name, and how package-lock.json ended up three releases behind.
//
// The fix validates every manifest and computes every replacement BEFORE writing any of them, so the
// operation is atomic in the way the message already claimed. These tests pin BOTH halves: the bytes on
// disk, and the wording of the failure.
// ---------------------------------------------------------------------------------------------------

describe("release.ps1 leaves no half-bumped tree when a LATER manifest fails (CPE-1852)", () => {
  const NO_VERSION_CARGO = `[package]\nname = "demo"\n\n[dependencies]\nserde = "1"\n`;
  const NO_VERSION_CONF = `{\n  "productName": "Demo"\n}\n`;

  // The THIRD manifest fails. package.json and tauri.conf.json are perfectly valid and would each be
  // written by a per-file writer before the guard ever looked at Cargo.toml.
  let third: BumpResult;
  beforeAll(() => {
    third = runBump({ ...ALL_DECOYS, cargo: NO_VERSION_CARGO }, NEW);
  });

  it("writes NOTHING when the third manifest fails: the first two are byte-identical on disk", () => {
    expect(third.status, ctx(third)).not.toBe(0);
    // Byte-level, not just "does not contain 9.9.9" -- the claim is that the files were never opened
    // for writing at all, and a re-encoded but textually-equal file would still be a released change.
    expect(third.bytes.pkg.equals(Buffer.from(crlf(PKG_WITH_DECOYS), "utf8")), "package.json was written despite the abort").toBe(true);
    expect(third.bytes.conf.equals(Buffer.from(crlf(CONF_WITH_DECOYS), "utf8")), "tauri.conf.json was written despite the abort").toBe(true);
    expect(third.bytes.cargo.equals(Buffer.from(crlf(NO_VERSION_CARGO), "utf8")), "Cargo.toml was written despite the abort").toBe(true);
  });

  it("does not print a per-file 'old -> new' line for a manifest it never wrote", () => {
    // The old script disclosed the half-bump only through those lines. Now there is nothing to disclose,
    // so there must be no line -- otherwise the output implies a write that did not happen, which is the
    // same class of lie in the other direction.
    expect(third.stdout, ctx(third)).not.toMatch(/package\.json: .* -> /);
    expect(third.stdout, ctx(third)).not.toMatch(/tauri\.conf\.json: .* -> /);
    expect(third.stdout, ctx(third)).not.toContain("Bumped version to");
  });

  it("says something TRUE OF THE WHOLE RUN, not 'refusing to write' about one file", () => {
    // flat(): PowerShell hard-wraps its error rendering to the console width, and the wrap falls in a
    // DIFFERENT place under pwsh than under Windows PowerShell 5.1 -- /no manifest was written/ passed
    // locally and reded on CI purely because pwsh split it after "No". Normalise before matching wording.
    const out = flat(third.stderr + third.stdout);
    expect(out, out).toMatch(/expected exactly one .*found 0/s);
    expect(out, out).toMatch(/no manifest was written/i);
    // ... and the old, run-untrue wording is gone.
    expect(out, out).not.toMatch(/refusing to write/i);
    // HAZARD, recorded rather than fixed: PowerShell echoes the OFFENDING SOURCE LINE above the
    // rendered message, and release.ps1's throw literally contains the phrase
    // `No manifest was written -- every `. If that echo were ever complete, this assertion would
    // pass off the script's own source text rather than off the message the operator sees. Both
    // hosts truncate the echo with an ellipsis before the phrase today (checked in the CI capture
    // and locally under 5.1), so it is honest -- but it is one console width or one string reflow
    // away from being self-satisfying. The BYTE-LEVEL assertions in this block are what carry the
    // real weight; treat this one as corroboration, not proof.
  });

  it("also leaves package.json alone when the SECOND manifest is the one that fails", () => {
    // Same property, one position earlier: proves the first case is not passing because Cargo.toml
    // happens to be validated first for some unrelated reason.
    const second = runBump({ ...ALL_DECOYS, conf: NO_VERSION_CONF }, NEW);
    expect(second.status, ctx(second)).not.toBe(0);
    expect(second.bytes.pkg.equals(Buffer.from(crlf(PKG_WITH_DECOYS), "utf8")), "package.json was written despite the abort").toBe(true);
    expect(second.bytes.conf.equals(Buffer.from(crlf(NO_VERSION_CONF), "utf8"))).toBe(true);
  });

  it("holds on the FULL release path too, not only under -BumpOnly (they share the writer)", () => {
    // Without -BumpOnly the script would go on to git add/commit/tag/push. It never gets there: the
    // validation pass aborts first, which is the point -- the atomicity is a property of the writer,
    // not of the dry-run switch. Nothing git-shaped runs, and the scratch tree is not a repo anyway.
    const full = runBump({ ...ALL_DECOYS, cargo: NO_VERSION_CARGO }, NEW, { bumpOnly: false });
    expect(full.status, ctx(full)).not.toBe(0);
    expect(full.bytes.pkg.equals(Buffer.from(crlf(PKG_WITH_DECOYS), "utf8")), "package.json was written despite the abort").toBe(true);
    expect(full.bytes.conf.equals(Buffer.from(crlf(CONF_WITH_DECOYS), "utf8")), "tauri.conf.json was written despite the abort").toBe(true);
    expect(full.stdout + full.stderr, ctx(full)).not.toMatch(/release v9\.9\.9|git .*failed with exit code/);
  });

  // The one case that CANNOT be made atomic: every manifest validated, then a WRITE fails. No
  // PowerShell scheme makes three WriteAllText calls transactional, so all the script can do is
  // report truthfully -- which is the entire subject of this ticket, so it gets tested rather than
  // asserted-by-construction. A 0444 manifest is readable (the plan phase succeeds) and unwritable.
  it("a write that fails AFTER validation names the files that DID land", () => {
    const r = runBump(ALL_DECOYS, NEW, { readOnly: "cargo" });
    expect(r.status, "expected the 0444 write to fail; as root it would not\n" + ctx(r)).not.toBe(0);
    const out = flat(r.stderr + r.stdout);
    expect(out, out).toMatch(/PARTIAL BUMP/);
    expect(out, out).toMatch(/Already written at v9\.9\.9: .*package\.json.*tauri\.conf\.json/s);
    expect(out, out).toMatch(/Revert those files/i);
    // ... and the report is TRUE: exactly those two are at the new version, the third is untouched.
    expect(r.text.pkg).toContain(`"version": "${NEW}"`);
    expect(r.text.conf).toContain(`"version": "${NEW}"`);
    expect(r.bytes.cargo.equals(Buffer.from(crlf(CARGO_WITH_DECOYS), "utf8")), "the unwritable manifest changed").toBe(true);
  });

  it("does NOT say 'revert those files' when the FIRST write fails and nothing landed", () => {
    // CPE-1852's own defect, inverted: "Already written: none. Revert those files" tells an operator
    // to undo nothing. Same run-untrue class as the "Refusing to write" this ticket deleted.
    const r = runBump(ALL_DECOYS, NEW, { readOnly: "pkg" });
    expect(r.status, "expected the 0444 write to fail; as root it would not\n" + ctx(r)).not.toBe(0);
    const out = flat(r.stderr + r.stdout);
    expect(out, out).toMatch(/no manifest was written/i);
    expect(out, out).not.toMatch(/PARTIAL BUMP/);
    expect(out, out).not.toMatch(/Revert those files/i);
    expect(out, out).not.toMatch(/Already written at .{0,20}none/i);
    // ... and the claim is TRUE: all three byte-identical, so there really is nothing to revert.
    expect(r.bytes.pkg.equals(Buffer.from(crlf(PKG_WITH_DECOYS), "utf8")), "package.json changed").toBe(true);
    expect(r.bytes.conf.equals(Buffer.from(crlf(CONF_WITH_DECOYS), "utf8")), "tauri.conf.json changed").toBe(true);
    expect(r.bytes.cargo.equals(Buffer.from(crlf(CARGO_WITH_DECOYS), "utf8")), "Cargo.toml changed").toBe(true);
  });

  it("still writes all three together on the success path (the fix is not 'write nothing, ever')", () => {
    // The companion arm: an atomicity fix that simply stopped writing would keep every case above green.
    const ok = runBump(ALL_DECOYS, NEW);
    expect(ok.status, ctx(ok)).toBe(0);
    expect(ok.text.pkg).toContain(`"version": "${NEW}"`);
    expect(ok.text.conf).toContain(`"version": "${NEW}"`);
    expect(ok.text.cargo).toContain(`version = "${NEW}"`);
  });
});

// ---------------------------------------------------------------------------------------------------
// CPE-1853: the two LOCKFILES, which release.ps1 did not touch until this ticket.
//
// CLAUDE.md requires five files to carry the same version. The script bumped three; `package-lock.json`
// (TWO places -- the root object's "version" and `packages[""]`'s) and `src-tauri/Cargo.lock` (the
// `cross-platform-explorer` package entry) stayed manual. They are the two that get missed, and
// CLAUDE.md already says why: NOTHING FAILS WHEN THEY DRIFT. Neither build passes `--locked`, so both
// lockfiles are silently rewritten at build time and the stale version never surfaces as an error -- it
// surfaces as a dirty working tree, which reads as unrelated noise and gets committed by accident or
// discarded with real work. It has happened: package-lock.json sat three releases behind (0.57.64 vs
// 0.57.67) as of 2026-08-20.
//
// Cargo.lock is the highest-stakes locator in the script: ~1000 `[[package]]` blocks, of which exactly
// one is the app and every other version is a dependency pin. Rewriting one is the defect CPE-1841
// existed to fix, in the file with the most of them -- and worse here than in Cargo.toml, because a bad
// pin in a LOCK file is what actually gets resolved and built.
// ---------------------------------------------------------------------------------------------------

/** The `version` of a named `[[package]]` block in a Cargo.lock, read independently of the script's own
 *  locator (a shared implementation would agree with the script about a shared mistake). */
function cargoLockPackageVersion(text: string, name: string, table = "package"): string | undefined {
  const header = table === "package" ? "\\[\\[package\\]\\]" : "\\[\\[patch\\.unused\\]\\]";
  const m = new RegExp(`${header}\\r?\\nname = "${name}"\\r?\\nversion = "([^"]+)"`).exec(text);
  return m?.[1];
}

/** Every place in a file that is supposed to hold the APP's version, read back after a bump. Keyed by
 *  FILE_KEYS so the all-five guard below cannot silently stop covering a file. */
const VERSION_READERS: Record<FileKey, (text: string) => (string | undefined)[]> = {
  pkg: (t) => [JSON.parse(t).version],
  conf: (t) => [JSON.parse(t).version],
  cargo: (t) => [/^\[package\][\s\S]*?^version = "([^"]+)"/m.exec(t)?.[1]],
  lock: (t) => {
    const j = JSON.parse(t);
    return [j.version, j.packages[""].version];
  },
  cargoLock: (t) => [cargoLockPackageVersion(t, "cross-platform-explorer")],
};

describe("release.ps1 bumps package-lock.json's BOTH version fields (CPE-1853)", () => {
  let r: BumpResult;
  beforeAll(() => {
    r = runBump(ALL_DECOYS, NEW);
  });

  it("bumps the ROOT version AND packages[\"\"] -- failing if only one of the two lands", () => {
    expect(r.status, ctx(r)).toBe(0);
    const j = JSON.parse(r.text.lock);
    // Asserted separately and named separately: half-landed is the specific way this file goes stale,
    // so a red must say WHICH half, not "the lockfile is wrong somewhere".
    expect(j.version, 'package-lock.json root "version" was not bumped').toBe(NEW);
    expect(j.packages[""].version, 'package-lock.json packages[""].version was not bumped').toBe(NEW);
  });

  it("bumps exactly TWO values -- no third place picked up along the way", () => {
    expect(r.status, ctx(r)).toBe(0);
    const occurrences = r.text.lock.split(`"${NEW}"`).length - 1;
    expect(occurrences, `expected exactly 2 occurrences of "${NEW}" in package-lock.json`).toBe(2);
  });

  it("leaves a dependency pin at the SAME depth and the SAME version alone (node_modules/decoy)", () => {
    expect(r.status, ctx(r)).toBe(0);
    const j = JSON.parse(r.text.lock);
    // The decoy's version is OLD -- byte-identical to what the app's was. Only the empty-string key
    // distinguishes the root package's entry from this one.
    expect(j.packages["node_modules/decoy"].version, "a dependency pin was rewritten to the app version").toBe(OLD);
    expect(j.packages["node_modules/other"].version).toBe("3.2.1");
  });

  it("is not fooled by braces or escaped quotes INSIDE a string value", () => {
    expect(r.status, ctx(r)).toBe(0);
    const j = JSON.parse(r.text.lock);
    expect(j.packages["node_modules/decoy"].deprecated).toBe('use {legacy} instead -- see "docs"');
    // If the walker had miscounted depth on those braces, the decoy's own version (which follows the
    // structure it corrupts in file order... it precedes it here, so assert the file-order successor)
    // would have moved. `node_modules/other` sits after the adversarial string.
    expect(j.packages["node_modules/other"].version).toBe("3.2.1");
  });

  it("leaves lockfileVersion and version-shaped URL text alone", () => {
    expect(r.status, ctx(r)).toBe(0);
    const j = JSON.parse(r.text.lock);
    expect(j.lockfileVersion, "the unquoted lockfileVersion number was touched").toBe(3);
    expect(r.text.lock).toContain("https://opencollective.com/other/7.8.9");
    expect(r.text.lock).toContain("https://registry.npmjs.org/decoy/-/decoy-0.1.0.tgz");
  });

  it("changes exactly TWO lines in package-lock.json and ONE in Cargo.lock, bytes identical elsewhere", () => {
    expect(r.status, ctx(r)).toBe(0);
    for (const key of ["lock", "cargoLock"] as const) {
      const before = crlf(key === "lock" ? LOCK_WITH_DECOYS : CARGO_LOCK_WITH_DECOYS).split("\r\n");
      const after = r.text[key].split("\r\n");
      expect(after.length, `${key}: line count changed`).toBe(before.length);
      const changed = before.map((line, i) => (line === after[i] ? -1 : i)).filter((i) => i >= 0);
      expect(changed.length, `${key}: expected ${VERSION_PLACES[key]} changed lines, got ${changed.length}`).toBe(
        VERSION_PLACES[key],
      );
      for (const i of changed) expect(after[i]).toContain(NEW);
    }
  });
});

describe("release.ps1 scopes Cargo.lock to the app's own [[package]] entry (CPE-1853)", () => {
  let r: BumpResult;
  beforeAll(() => {
    r = runBump(ALL_DECOYS, NEW);
  });

  it("bumps the cross-platform-explorer [[package]] entry", () => {
    expect(r.status, ctx(r)).toBe(0);
    expect(cargoLockPackageVersion(r.text.cargoLock, "cross-platform-explorer")).toBe(NEW);
  });

  it("leaves a decoy [[package]] whose version EXACTLY matches the app's alone (decoy-crate)", () => {
    expect(r.status, ctx(r)).toBe(0);
    expect(cargoLockPackageVersion(r.text.cargoLock, "decoy-crate"), "a dependency pin was rewritten").toBe(OLD);
  });

  it("leaves ordinary dependency pins alone (serde)", () => {
    expect(r.status, ctx(r)).toBe(0);
    expect(cargoLockPackageVersion(r.text.cargoLock, "serde")).toBe("1.0.219");
  });

  it("leaves a [[patch.unused]] entry carrying the app's OWN NAME alone", () => {
    expect(r.status, ctx(r)).toBe(0);
    // Scoping by name alone would rewrite this. It is not the package entry, so it must not move.
    expect(cargoLockPackageVersion(r.text.cargoLock, "cross-platform-explorer", "patch.unused")).toBe(OLD);
  });

  it("leaves the top-of-file `version = 3` lockfile-format marker alone", () => {
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.cargoLock).toContain("version = 3\r\n");
    expect(r.text.cargoLock.startsWith("# This file is automatically @generated by Cargo.")).toBe(true);
  });

  it("rewrites exactly ONE version-shaped value in the whole lockfile", () => {
    expect(r.status, ctx(r)).toBe(0);
    const occurrences = r.text.cargoLock.split(`"${NEW}"`).length - 1;
    expect(occurrences, "more than the app's own entry was rewritten").toBe(1);
  });

  it("preserves LF endings on a Cargo.lock that cargo itself last wrote", () => {
    // Not hypothetical: Cargo.lock is CRLF in a fresh checkout here (core.autocrlf=true) and LF the
    // moment `cargo build` rewrites it. Measured on this machine's main worktree, 2026-08-22:
    // Cargo.lock had 10802 lone LFs while package-lock.json had 0. The bump must not convert either way.
    const lfRun = runBump(ALL_DECOYS, NEW, { lf: "cargoLock" });
    expect(lfRun.status, ctx(lfRun)).toBe(0);
    expect(cargoLockPackageVersion(lfRun.text.cargoLock, "cross-platform-explorer")).toBe(NEW);
    expect(lfRun.bytes.cargoLock.includes(0x0d), "an LF Cargo.lock came back with CR bytes").toBe(false);
    expect(lfRun.bytes.cargoLock.subarray(-1).toString("latin1")).toBe("\n");
    // ... and the CRLF file staged alongside it in the same run is still CRLF.
    const lockBytes = lfRun.bytes.lock;
    expect([...lockBytes].filter((b, i) => b === 0x0a && lockBytes[i - 1] !== 0x0d).length).toBe(0);
  });
});

describe("release.ps1 fails loudly on a lockfile it cannot locate (CPE-1853)", () => {
  it("exits non-zero when package-lock.json carries the version in only ONE place", () => {
    // The realistic decay: someone hand-edits the root version, or a tool writes a lock without the
    // packages[""] entry. One place is not this file's shape, and half a bump is how it goes stale.
    const halfLock = `{\n  "name": "demo",\n  "version": "${OLD}",\n  "lockfileVersion": 3,\n  "packages": {\n    "node_modules/decoy": { "version": "${OLD}" }\n  }\n}\n`;
    const r = runBump({ ...ALL_DECOYS, lock: halfLock }, NEW);
    expect(r.status, ctx(r)).not.toBe(0);
    const out = flat(r.stderr + r.stdout);
    expect(out, out).toMatch(/expected exactly two .*found 1/s);
    expect(r.text.lock, "package-lock.json was written despite the abort").toBe(crlf(halfLock));
  });

  it("exits non-zero when package-lock.json has no app version at all", () => {
    const noVersion = `{\n  "name": "demo",\n  "lockfileVersion": 3,\n  "packages": {\n    "": { "name": "demo" }\n  }\n}\n`;
    const r = runBump({ ...ALL_DECOYS, lock: noVersion }, NEW);
    expect(r.status, ctx(r)).not.toBe(0);
    expect(flat(r.stderr + r.stdout)).toMatch(/expected exactly two .*found 0/s);
    expect(r.text.lock).toBe(crlf(noVersion));
  });

  it("exits non-zero when Cargo.lock has no cross-platform-explorer [[package]] entry", () => {
    // A crate rename, or a Cargo.lock regenerated for a different workspace root. Aborting loudly is
    // the intended outcome: a release script that cannot find the app in its own lockfile must not
    // report a bump. Note the fixture DOES contain the name -- under [[patch.unused]] -- so this also
    // proves the table type is part of the match.
    const renamed = CARGO_LOCK_WITH_DECOYS.replace(
      `[[package]]\nname = "cross-platform-explorer"`,
      `[[package]]\nname = "cross-platform-explorer-renamed"`,
    );
    const r = runBump({ ...ALL_DECOYS, cargoLock: renamed }, NEW);
    expect(r.status, ctx(r)).not.toBe(0);
    expect(flat(r.stderr + r.stdout)).toMatch(/expected exactly one .*found 0/s);
    expect(r.text.cargoLock).toBe(crlf(renamed));
  });

  it("exits non-zero on an AMBIGUOUS Cargo.lock (the app's block carrying two version lines)", () => {
    const ambiguous = CARGO_LOCK_WITH_DECOYS.replace(
      `name = "cross-platform-explorer"\nversion = "${OLD}"\ndependencies`,
      `name = "cross-platform-explorer"\nversion = "${OLD}"\nversion = "${OLD}"\ndependencies`,
    );
    const r = runBump({ ...ALL_DECOYS, cargoLock: ambiguous }, NEW);
    expect(r.status, ctx(r)).not.toBe(0);
    expect(flat(r.stderr + r.stdout)).toMatch(/expected exactly one .*found 2/s);
    expect(r.text.cargoLock).toBe(crlf(ambiguous));
  });

  it("writes NOTHING when the FOURTH file (package-lock.json) fails -- the first three are untouched", () => {
    // CPE-1852's atomicity, extended to the files this ticket adds. The three manifests are valid and
    // a per-file writer would have all three at the new version by the time it reached the lockfile.
    const noVersion = `{\n  "name": "demo",\n  "lockfileVersion": 3,\n  "packages": { "": { "name": "demo" } }\n}\n`;
    const r = runBump({ ...ALL_DECOYS, lock: noVersion }, NEW);
    expect(r.status, ctx(r)).not.toBe(0);
    expect(r.bytes.pkg.equals(Buffer.from(crlf(PKG_WITH_DECOYS), "utf8")), "package.json was written").toBe(true);
    expect(r.bytes.conf.equals(Buffer.from(crlf(CONF_WITH_DECOYS), "utf8")), "tauri.conf.json was written").toBe(true);
    expect(r.bytes.cargo.equals(Buffer.from(crlf(CARGO_WITH_DECOYS), "utf8")), "Cargo.toml was written").toBe(true);
    expect(flat(r.stderr + r.stdout)).toMatch(/no manifest was written/i);
    expect(r.stdout).not.toContain("Bumped version to");
  });

  it("writes NOTHING when the FIFTH file (Cargo.lock) fails -- the first four are untouched", () => {
    const renamed = CARGO_LOCK_WITH_DECOYS.replace(`name = "cross-platform-explorer"\nversion`, `name = "other"\nversion`);
    const r = runBump({ ...ALL_DECOYS, cargoLock: renamed }, NEW);
    expect(r.status, ctx(r)).not.toBe(0);
    for (const key of ["pkg", "conf", "cargo", "lock"] as const) {
      const staged = { pkg: PKG_WITH_DECOYS, conf: CONF_WITH_DECOYS, cargo: CARGO_WITH_DECOYS, lock: LOCK_WITH_DECOYS }[key];
      expect(r.bytes[key].equals(Buffer.from(crlf(staged), "utf8")), `${FILE_PATHS[key]} was written despite the abort`).toBe(true);
    }
    expect(flat(r.stderr + r.stdout)).toMatch(/no manifest was written/i);
  });
});

// ---------------------------------------------------------------------------------------------------
// The single guard that makes "five files in sync" a checked property rather than a documented wish.
//
// The point is NOT to re-assert what the blocks above already assert file by file. It is that the LIST
// itself is checked: CLAUDE.md's list, release.ps1's plan set, its `git add` set, and this file's
// FILE_KEYS must all name the same files. Add a sixth file to CLAUDE.md and forget the script, or add
// it to the script and forget the commit, and this reds -- which is precisely how files 4 and 5 came to
// be missed for three releases.
// ---------------------------------------------------------------------------------------------------
describe("all five version-synchronised files stay in sync (CPE-1853)", () => {
  let r: BumpResult;
  beforeAll(() => {
    r = runBump(ALL_DECOYS, NEW);
  });

  it("every one of the five reads back at the new version after a single bump", () => {
    expect(r.status, ctx(r)).toBe(0);
    const found: Record<string, (string | undefined)[]> = {};
    for (const key of FILE_KEYS) found[FILE_PATHS[key]] = VERSION_READERS[key](r.text[key]);
    // Reported as one object so a red shows every file's version at once -- which of the five drifted
    // is the whole question, and five separate reds would each answer a fifth of it.
    expect(found).toEqual({
      "package.json": [NEW],
      "src-tauri/tauri.conf.json": [NEW],
      "src-tauri/Cargo.toml": [NEW],
      "package-lock.json": [NEW, NEW],
      "src-tauri/Cargo.lock": [NEW],
    });
    // ... and the expectation above is itself checked against the declared place counts, so a future
    // edit cannot quietly drop package-lock.json's second field from the literal.
    for (const key of FILE_KEYS) expect(found[FILE_PATHS[key]].length, FILE_PATHS[key]).toBe(VERSION_PLACES[key]);
  });

  it("release.ps1 plans exactly the files CLAUDE.md's five-files-in-sync list names", () => {
    const claudeMd = readFileSync(join(ROOT, "CLAUDE.md"), "utf8");
    const section = /^## Versioning[^\n]*\n([\s\S]*?)\n## /m.exec(claudeMd);
    expect(section, "CLAUDE.md no longer has a '## Versioning' section for this test to read").not.toBeNull();
    const listed = [...section![1].matchAll(/^\d+\.\s+`([^`]+)`/gm)].map((m) => m[1]);

    const script = readFileSync(RELEASE_PS1, "utf8");
    const planned = [...script.matchAll(/New-ManifestVersionPlan -Path \(Join-Path \$repo "([^"]+)"\)/g)].map((m) => m[1]);

    const expected = [...Object.values(FILE_PATHS)].sort();
    expect([...listed].sort(), "CLAUDE.md's list and this test's FILE_PATHS disagree").toEqual(expected);
    expect([...planned].sort(), "release.ps1 does not bump every file CLAUDE.md requires in sync").toEqual(expected);
  });

  it("release.ps1 stages all five in the release commit, not just the ones it used to bump", () => {
    // Bumping a file and not committing it is the same drift one step later: the tag would ship with
    // the lockfiles left behind in the working tree.
    const script = readFileSync(RELEASE_PS1, "utf8");
    const addLine = /^Invoke-Git add (.+)$/m.exec(script);
    expect(addLine, "release.ps1 no longer has an `Invoke-Git add` line").not.toBeNull();
    for (const path of Object.values(FILE_PATHS)) {
      expect(addLine![1], `${path} is bumped but never staged for the release commit`).toContain(path);
    }
  });
});

// ---------------------------------------------------------------------------------------------------
// Red-proof. The tests above only mean something if the fixtures actually trip the bug -- a decoy the
// broken code never touched would keep them green with the scoping reverted. So: re-run the EXACT pre-fix
// replacements (copied verbatim from scripts/release.ps1 at origin/main before this ticket) over the same
// fixtures, and assert every decoy IS clobbered.
//
// What this block DOES guard: that each decoy is a shape the old code genuinely ate, so the "leaves X
// alone" tests above cannot be vacuously green against a fixture nothing would have touched anyway.
// What it does NOT guard: any future regression. PRE_FIX_SCRIPT is a frozen transcription of one
// historical script, so a differently-broken future release.ps1 leaves this block green while breaking
// the real thing. The other tests in this file -- which drive the ACTUAL scripts/release.ps1 -- are what
// carry that load. Do not read a green here as "the current script is correct".
//
// This mechanises the manual revert recorded in the ticket's Work Log: reverting the Cargo.toml locator
// call alone -- i.e. putting
//     $cargo = $cargo -replace '(?m)^(version\s*=\s*")[^"]+(")', "`${1}$Version`$2"
// back in place of `New-ManifestVersionPlan -Path ... -Locator 'Find-TomlPackageVersionValue'` -- reds the
// "leaves a long-form Cargo dependency pin alone" test above, and nothing else.
// ---------------------------------------------------------------------------------------------------

/** The pre-CPE-1841 bump, verbatim, wrapped in just enough script to be runnable with `-BumpOnly`. */
const PRE_FIX_SCRIPT = `param(
  [Parameter(Mandatory = $true)][string]$Version,
  [switch]$BumpOnly
)
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

$pkgPath = Join-Path $repo "package.json"
$pkg = [System.IO.File]::ReadAllText($pkgPath, $utf8NoBom)
$pkg = $pkg -replace '("version"\\s*:\\s*")[^"]+(")', "\`\${1}$Version\`$2"
[System.IO.File]::WriteAllText($pkgPath, $pkg, $utf8NoBom)

$confPath = Join-Path $repo "src-tauri/tauri.conf.json"
$conf = [System.IO.File]::ReadAllText($confPath, $utf8NoBom)
$conf = $conf -replace '("version"\\s*:\\s*")[^"]+(")', "\`\${1}$Version\`$2"
[System.IO.File]::WriteAllText($confPath, $conf, $utf8NoBom)

$cargoPath = Join-Path $repo "src-tauri/Cargo.toml"
$cargo = [System.IO.File]::ReadAllText($cargoPath, $utf8NoBom)
$cargo = $cargo -replace '(?m)^(version\\s*=\\s*")[^"]+(")', "\`\${1}$Version\`$2"
[System.IO.File]::WriteAllText($cargoPath, $cargo, $utf8NoBom)

Write-Host "Bumped version to $Version in package.json, tauri.conf.json, Cargo.toml"
`;

describe("CPE-1841 red-proof: the pre-fix replacements really do clobber these fixtures", () => {
  // Same one-spawn-many-assertions treatment as the block above.
  let r: BumpResult;
  beforeAll(() => {
    r = runBump(ALL_DECOYS, NEW, { scriptText: PRE_FIX_SCRIPT });
  });

  it("the un-scoped regexes rewrite the nested package.json tool version to the app version", () => {
    expect(r.status, r.stderr).toBe(0);
    expect(r.text.pkg).toContain(`"someTool": { "version": "${NEW}" }`);
    expect(r.text.pkg).not.toContain('"3.2.1"');
  });

  it("the un-scoped regexes rewrite the nested tauri.conf.json wix version to the app version", () => {
    expect(r.status, r.stderr).toBe(0);
    expect(r.text.conf).toContain(`"wix": { "version": "${NEW}" }`);
    expect(r.text.conf).not.toContain('"3.11.2"');
  });

  it("the un-scoped regex rewrites the long-form Cargo dependency pin to the app version", () => {
    expect(r.status, r.stderr).toBe(0);
    expect(r.text.cargo).toContain(`[dependencies.somepkg]\r\nversion = "${NEW}"`);
    expect(r.text.cargo).not.toContain('"1.2.3"');
  });

  it("the un-scoped replacements report success having bumped NOTHING when nothing matches", () => {
    const untouchable = { pkg: `{\n  "name": "demo"\n}\n`, conf: `{\n  "productName": "Demo"\n}\n`, cargo: `[package]\nname = "demo"\n` };
    const r = runBump(untouchable, NEW, { scriptText: PRE_FIX_SCRIPT });
    expect(r.status, ctx(r)).toBe(0); // the bug: exit 0
    expect(r.stdout).toContain("Bumped version to");
    expect(r.text.pkg).toBe(crlf(untouchable.pkg)); // ... having changed nothing at all
  });
});

// ---------------------------------------------------------------------------------------------------
// CPE-1853 red-proof: the two obvious WRONG ways to extend the script to the lockfiles.
//
// Unlike PRE_FIX_SCRIPT above, these are not transcriptions of anything that ever shipped -- there was
// no lockfile code to transcribe, which is the whole point of the ticket. They are the two mistakes a
// minimal extension actually makes, run against the same fixtures, to prove those fixtures are traps
// rather than decoration:
//
//   1. NAIVE_LOCKFILE_SCRIPT -- reuse the un-scoped `-replace` shapes. PowerShell's `-replace` rewrites
//      EVERY match, so this clobbers ~1000 dependency pins in the real Cargo.lock and every package
//      entry in the real package-lock.json.
//   2. TOP_LEVEL_ONLY_SCRIPT -- do the scoped, careful thing... once. package-lock.json's root version
//      moves and `packages[""]` stays behind. This is the specific stale shape the ticket names, and it
//      is the one a reviewer would most easily wave through.
//
// As with the block above: a green here does NOT mean the shipped script is correct. The suites that
// drive scripts/release.ps1 itself carry that.
// ---------------------------------------------------------------------------------------------------

const NAIVE_LOCKFILE_SCRIPT = `param(
  [Parameter(Mandatory = $true)][string]$Version,
  [switch]$BumpOnly
)
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

$lockPath = Join-Path $repo "package-lock.json"
$lock = [System.IO.File]::ReadAllText($lockPath, $utf8NoBom)
$lock = $lock -replace '("version"\\s*:\\s*")[^"]+(")', "\`\${1}$Version\`$2"
[System.IO.File]::WriteAllText($lockPath, $lock, $utf8NoBom)

$cargoLockPath = Join-Path $repo "src-tauri/Cargo.lock"
$cargoLock = [System.IO.File]::ReadAllText($cargoLockPath, $utf8NoBom)
$cargoLock = $cargoLock -replace '(?m)^(version\\s*=\\s*")[^"]+(")', "\`\${1}$Version\`$2"
[System.IO.File]::WriteAllText($cargoLockPath, $cargoLock, $utf8NoBom)

Write-Host "Bumped version to $Version"
`;

/** Scoped and careful -- and applied to the FIRST match only, so packages[""] is left behind. */
const TOP_LEVEL_ONLY_SCRIPT = `param(
  [Parameter(Mandatory = $true)][string]$Version,
  [switch]$BumpOnly
)
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

$lockPath = Join-Path $repo "package-lock.json"
$lock = [System.IO.File]::ReadAllText($lockPath, $utf8NoBom)
$re = [regex]'("version"\\s*:\\s*")[^"]+(")'
$lock = $re.Replace($lock, "\`\${1}$Version\`$2", 1)
[System.IO.File]::WriteAllText($lockPath, $lock, $utf8NoBom)

Write-Host "Bumped version to $Version"
`;

describe("CPE-1853 red-proof: the naive lockfile bumps really do clobber these fixtures", () => {
  let naive: BumpResult;
  beforeAll(() => {
    naive = runBump(ALL_DECOYS, NEW, { scriptText: NAIVE_LOCKFILE_SCRIPT });
  });

  it("an un-scoped package-lock.json replace rewrites every dependency pin to the app version", () => {
    expect(naive.status, ctx(naive)).toBe(0);
    const j = JSON.parse(naive.text.lock);
    expect(j.packages["node_modules/decoy"].version, "the fixture's decoy pin is not actually a trap").toBe(NEW);
    expect(j.packages["node_modules/other"].version).toBe(NEW);
  });

  it("an un-scoped Cargo.lock replace rewrites decoy-crate, serde AND the [[patch.unused]] entry", () => {
    expect(naive.status, ctx(naive)).toBe(0);
    expect(cargoLockPackageVersion(naive.text.cargoLock, "decoy-crate"), "the decoy crate is not a trap").toBe(NEW);
    expect(cargoLockPackageVersion(naive.text.cargoLock, "serde")).toBe(NEW);
    expect(cargoLockPackageVersion(naive.text.cargoLock, "cross-platform-explorer", "patch.unused")).toBe(NEW);
  });

  it("a top-level-only package-lock.json bump leaves packages[\"\"] stale -- exactly how this file drifts", () => {
    const half = runBump(ALL_DECOYS, NEW, { scriptText: TOP_LEVEL_ONLY_SCRIPT });
    expect(half.status, ctx(half)).toBe(0);
    const j = JSON.parse(half.text.lock);
    expect(j.version, "the fixture would not detect a half bump").toBe(NEW);
    expect(j.packages[""].version, "the fixture would not detect a half bump").toBe(OLD);
  });
});
