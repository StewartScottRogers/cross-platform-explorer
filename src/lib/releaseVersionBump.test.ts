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
import { describe, it, expect, beforeAll } from "vitest";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, copyFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

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

interface Manifests {
  pkg: string;
  conf: string;
  cargo: string;
}

interface BumpResult {
  status: number | null;
  stdout: string;
  stderr: string;
  /** Raw bytes as written back, so BOM / CRLF / trailing-newline claims are checked at the byte level. */
  bytes: { pkg: Buffer; conf: Buffer; cargo: Buffer };
  text: { pkg: string; conf: string; cargo: string };
}

/** Stage `manifests` in a throwaway tree, run the real release.ps1 over it with `-BumpOnly`, and read
 *  back what it wrote. `scriptText`, when given, is written as the script instead of copying the real
 *  one -- used only by the red-proof block below, which re-runs the pre-fix regexes. */
function runBump(manifests: Manifests, version: string, scriptText?: string): BumpResult {
  const dir = mkdtempSync(join(tmpdir(), "cpe-1841-release-bump-"));
  try {
    mkdirSync(join(dir, "scripts"));
    mkdirSync(join(dir, "src-tauri"));

    const scriptPath = join(dir, "scripts", "release.ps1");
    if (scriptText === undefined) copyFileSync(RELEASE_PS1, scriptPath);
    else writeFileSync(scriptPath, Buffer.from(scriptText, "utf8"));

    const pkgPath = join(dir, "package.json");
    const confPath = join(dir, "src-tauri", "tauri.conf.json");
    const cargoPath = join(dir, "src-tauri", "Cargo.toml");
    writeFileSync(pkgPath, Buffer.from(crlf(manifests.pkg), "utf8"));
    writeFileSync(confPath, Buffer.from(crlf(manifests.conf), "utf8"));
    writeFileSync(cargoPath, Buffer.from(crlf(manifests.cargo), "utf8"));

    const run = spawnSync(psHost, ["-NoProfile", "-File", scriptPath, "-Version", version, "-BumpOnly"], {
      encoding: "utf8",
    });
    if (run.error) throw run.error;

    const bytes = {
      pkg: readFileSync(pkgPath),
      conf: readFileSync(confPath),
      cargo: readFileSync(cargoPath),
    };
    return {
      status: run.status,
      stdout: run.stdout ?? "",
      stderr: run.stderr ?? "",
      bytes,
      text: {
        pkg: bytes.pkg.toString("utf8"),
        conf: bytes.conf.toString("utf8"),
        cargo: bytes.cargo.toString("utf8"),
      },
    };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
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

const ALL_DECOYS: Manifests = { pkg: PKG_WITH_DECOYS, conf: CONF_WITH_DECOYS, cargo: CARGO_WITH_DECOYS };

describe("release.ps1 bumps the version it means (CPE-1841)", () => {
  it("rewrites the top-level version in all three manifests", () => {
    const r = runBump(ALL_DECOYS, NEW);
    expect(r.stderr, r.stderr).toBe("");
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.pkg).toContain(`"version": "${NEW}"`);
    expect(r.text.conf).toContain(`"version": "${NEW}"`);
    expect(r.text.cargo).toContain(`version = "${NEW}"`);
  });

  it("leaves a long-form Cargo dependency pin alone ([dependencies.somepkg] / version = \"1.2.3\")", () => {
    const r = runBump(ALL_DECOYS, NEW);
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.cargo).toContain('[dependencies.somepkg]\r\nversion = "1.2.3"');
    // The [package] version DID move, so this isn't a vacuous pass on an untouched file.
    expect(r.text.cargo).toContain(`[package]\r\nname = "demo"\r\nversion = "${NEW}"`);
  });

  it('leaves a nested tool version alone in package.json ("someTool": { "version": "3.2.1" })', () => {
    const r = runBump(ALL_DECOYS, NEW);
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.pkg).toContain('"someTool": { "version": "3.2.1" }');
    expect(r.text.pkg).toContain(`"version": "${NEW}"`);
  });

  it('leaves a nested tool version alone in tauri.conf.json ("wix": { "version": "3.11.2" })', () => {
    const r = runBump(ALL_DECOYS, NEW);
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.conf).toContain('"wix": { "version": "3.11.2" }');
    expect(r.text.conf).toContain(`"version": "${NEW}"`);
  });

  it("leaves a version-shaped string inside a description alone, in every manifest", () => {
    const r = runBump(ALL_DECOYS, NEW);
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.pkg).toContain('"description": "needs at least 4.5.6 of the thing"');
    expect(r.text.conf).toContain('"shortDescription": "requires 6.5.4 at runtime"');
    expect(r.text.cargo).toContain("pins 4.5.6 internally");
  });

  it("leaves a version-shaped string inside a URL alone, in every manifest", () => {
    const r = runBump(ALL_DECOYS, NEW);
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.pkg).toContain("https://example.com/downloads/7.8.9/index.html");
    expect(r.text.conf).toContain("https://example.com/downloads/7.8.9/index.html");
    expect(r.text.cargo).toContain("https://example.com/v/7.8.9");
  });

  it("leaves Cargo's rust-version alone (a version-shaped value on a DIFFERENT key inside [package])", () => {
    const r = runBump(ALL_DECOYS, NEW);
    expect(r.status, ctx(r)).toBe(0);
    expect(r.text.cargo).toContain('rust-version = "1.77.2"');
  });

  it("changes exactly ONE line per manifest, byte-for-byte identical everywhere else", () => {
    const r = runBump(ALL_DECOYS, NEW);
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
    const r = runBump(ALL_DECOYS, NEW);
    expect(r.status, ctx(r)).toBe(0);
    for (const key of ["pkg", "conf", "cargo"] as const) {
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
// Red-proof. The tests above only mean something if the fixtures actually trip the bug -- a decoy the
// broken code never touched would keep them green with the scoping reverted. So: re-run the EXACT pre-fix
// replacements (copied verbatim from scripts/release.ps1 at origin/main before this ticket) over the same
// fixtures, and assert every decoy IS clobbered.
//
// This mechanises the manual revert recorded in the ticket's Work Log: reverting the Cargo.toml locator
// call alone -- i.e. putting
//     $cargo = $cargo -replace '(?m)^(version\s*=\s*")[^"]+(")', "`${1}$Version`$2"
// back in place of `Update-ManifestVersion -Path ... -Locator 'Find-TomlPackageVersionValue'` -- reds the
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
  it("the un-scoped regexes rewrite the nested package.json tool version to the app version", () => {
    const r = runBump(ALL_DECOYS, NEW, PRE_FIX_SCRIPT);
    expect(r.status, r.stderr).toBe(0);
    expect(r.text.pkg).toContain(`"someTool": { "version": "${NEW}" }`);
    expect(r.text.pkg).not.toContain('"3.2.1"');
  });

  it("the un-scoped regexes rewrite the nested tauri.conf.json wix version to the app version", () => {
    const r = runBump(ALL_DECOYS, NEW, PRE_FIX_SCRIPT);
    expect(r.status, r.stderr).toBe(0);
    expect(r.text.conf).toContain(`"wix": { "version": "${NEW}" }`);
    expect(r.text.conf).not.toContain('"3.11.2"');
  });

  it("the un-scoped regex rewrites the long-form Cargo dependency pin to the app version", () => {
    const r = runBump(ALL_DECOYS, NEW, PRE_FIX_SCRIPT);
    expect(r.status, r.stderr).toBe(0);
    expect(r.text.cargo).toContain(`[dependencies.somepkg]\r\nversion = "${NEW}"`);
    expect(r.text.cargo).not.toContain('"1.2.3"');
  });

  it("the un-scoped replacements report success having bumped NOTHING when nothing matches", () => {
    const untouchable = { pkg: `{\n  "name": "demo"\n}\n`, conf: `{\n  "productName": "Demo"\n}\n`, cargo: `[package]\nname = "demo"\n` };
    const r = runBump(untouchable, NEW, PRE_FIX_SCRIPT);
    expect(r.status, ctx(r)).toBe(0); // the bug: exit 0
    expect(r.stdout).toContain("Bumped version to");
    expect(r.text.pkg).toBe(crlf(untouchable.pkg)); // ... having changed nothing at all
  });
});
