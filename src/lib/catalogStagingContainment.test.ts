/**
 * CPE-1952 — the deterministic CI guard that `do_fetch_catalog` never regrows a staging directory.
 *
 * ## What this exists to stop
 *
 * The catalog fetch used to stage its downloaded bundle at
 * `std::env::temp_dir().join(format!("cpe-catalog-stage-{}", std::process::id()))` and materialise
 * it with `std::fs::create_dir_all`. That path is **predictable** (a shared namespace plus a pid),
 * it is **outside the project**, and `create_dir_all` **follows a junction or symlink** and creates
 * missing components — so a local process that plants a link there first chooses where the fetch
 * writes. Reproduced on Windows (junction) and Linux (symlink, real ext4): the staged `index.json`
 * landed in the attacker's directory and the call returned `Ok`.
 *
 * The fix removed the directory rather than hardening it — the bundle is assembled in memory
 * (`sidecar_host::catalog::MemBundle`) and applied via `apply_bundle_source_at`. **A directory that
 * is never created cannot be redirected.**
 *
 * `sidecar/host/tests/catalog_staging_containment.rs` is the behavioural half: it plants a real
 * junction/symlink, shows the old primitive writing through it (the sensitivity control), and shows
 * the new apply path leaving the attacker's directory empty. What it cannot cover is `src-tauri`,
 * which needs a Tauri `AppHandle` and a network fetch to reach. So this file covers the caller, and
 * it covers it by **reading the real source** (CPE-1933: derive, do not claim) rather than by
 * asserting a comment.
 *
 * ## Anchoring
 *
 * Comments are blanked first (`stripRustComments`), because the source under test is *full* of prose
 * naming the very primitives this guard forbids — the fix's own explanation says `create_dir_all`
 * three times. A guard that scanned raw text would fail on the fix it is guarding, and the obvious
 * "fix" for that would be to weaken the guard. Strip first; anchor on code.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { stripRustComments } from "./rustSource";

/** The bodies-only slice of a top-level Rust fn: from its signature to the next `}` in column 0. */
function fnBody(src: string, signature: string): string {
  const start = src.indexOf(signature);
  expect(start, `${signature} not found — this guard is reading the wrong source`).toBeGreaterThan(-1);
  const end = src.indexOf("\n}\n", start);
  expect(end, `${signature} has no column-0 closing brace`).toBeGreaterThan(start);
  return src.slice(start, end);
}

/**
 * Filesystem primitives that must not appear in the fetch. Each one is either the defect itself
 * (`temp_dir`, `create_dir_all`) or the shape it would come back as (any other directory create, or
 * the cleanup that only exists because a directory does).
 */
const FORBIDDEN = ["temp_dir", "create_dir_all", "create_dir(", "remove_dir_all", "fs::write"];

const LIB_RS = join(process.cwd(), "src-tauri", "src", "lib.rs");

describe("CPE-1952: the catalog fetch stages nothing on disk", () => {
  // Newlines normalised first: `lib.rs` is checked out CRLF on Windows and LF on the Linux/macOS CI
  // legs, and the column-0 `\n}\n` anchor below must mean the same thing on all three.
  const raw = readFileSync(LIB_RS, "utf8").replace(/\r\n/g, "\n");
  const src = stripRustComments(raw);

  it("reads a source that is actually there", () => {
    // Fail loudly rather than passing on an empty read — a guard whose input vanished must not read
    // as a clean bill of health (the `npm audit` lesson, CLAUDE.md).
    expect(raw.length).toBeGreaterThan(100_000);
    // And the file as a whole DOES still contain the forbidden primitives elsewhere, so an absence
    // inside the fetch is a real fact about the fetch rather than about a broken stripper.
    for (const needle of ["temp_dir", "create_dir_all"]) {
      expect(src, `${needle} vanished from the whole file — the stripper ate the code`).toContain(
        needle,
      );
    }
  });

  const body = fnBody(src, "fn do_fetch_catalog(");

  it("finds a plausible do_fetch_catalog body", () => {
    expect(body.length).toBeGreaterThan(1_000);
  });

  it.each(FORBIDDEN)("does not call %s", (needle) => {
    expect(
      body,
      `do_fetch_catalog calls ${needle} — CPE-1952 removed on-disk staging from this function; ` +
        `the bundle is a MemBundle and must stay one`,
    ).not.toContain(needle);
  });

  it("still assembles the bundle in memory and applies it through the memory entry point", () => {
    // The other half of the guard. Without this, deleting or renaming the function would make every
    // assertion above pass by vacuity — "no forbidden call" is trivially true of nothing.
    expect(body).toContain("MemBundle::new()");
    expect(body).toContain("apply_bundle_source_at");
  });
});

describe("the guard's own anchor", () => {
  // Red-proof by construction: the extractor must react to CODE and ignore PROSE. Both directions
  // are asserted, because a stripper that blanked too much would silently pass everything.
  const hostile = [
    "fn do_fetch_catalog(a: u8) -> u8 {",
    "    // CPE-1952 removed the std::env::temp_dir() staging dir and its create_dir_all.",
    "    let x = 1; /* create_dir_all lived here */",
    "    let msg = \"create_dir_all\"; // a trailing comment mentioning create_dir_all",
    "    x",
    "}",
    "",
  ].join("\n");

  it("a comment naming the forbidden primitive does not trip the guard", () => {
    const body = fnBody(stripRustComments(hostile), "fn do_fetch_catalog(");
    expect(body).not.toContain("temp_dir");
    // The string literal survives stripping (it is code, not a comment) — that is correct, and it is
    // why the real assertion list is about calls rather than about mentions.
    expect(body).toContain('"create_dir_all"');
  });

  it("a real call does trip it", () => {
    const real = hostile.replace("let x = 1;", "let x = std::fs::create_dir_all(p).is_ok() as u8;");
    const body = fnBody(stripRustComments(real), "fn do_fetch_catalog(");
    expect(body).toContain("create_dir_all(p)");
  });
});
