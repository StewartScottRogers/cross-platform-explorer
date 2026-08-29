import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  stripRustComments,
  rustStringLiteralAfter,
  rustStrSliceAfter,
  rustStrConstAfter,
} from "./rustSource";

/**
 * CPE-1950. `stripRustComments` / `rustStringLiteralAfter` came out of
 * `components/MacroRunConfirm.test.ts` (which still exercises them end-to-end against the real
 * `fsutil.rs`); `rustStrSliceAfter` is new here. These are the unit-level pins, including the
 * adversarial shapes that motivated the comment stripping in the first place.
 */
describe("stripRustComments", () => {
  it("blanks a line comment but keeps every offset", () => {
    const src = 'let a = 1; // let a = 2;\nlet b = 3;';
    const out = stripRustComments(src);
    expect(out.length).toBe(src.length);
    expect(out).not.toContain("let a = 2");
    expect(out).toContain("let b = 3");
  });

  it("blanks a block comment", () => {
    expect(stripRustComments("a /* hidden */ b")).toBe("a              b");
  });

  it("leaves a // inside a string literal alone", () => {
    expect(stripRustComments('let u = "https://example.com/x";')).toBe(
      'let u = "https://example.com/x";',
    );
  });
});

/**
 * PR #1067 review, Blocking 2. The old scanner tracked only `"` string literals, and its doc claimed
 * every desync failed **loudly**. Both were false, and false about files this module is pointed at:
 * `src-tauri/src/lib.rs:8253`'s `path.contains('"')` swallowed **142 `///` lines** (8268–8959), and
 * `crates/server/src/fsutil.rs:3379`'s backslash-terminated raw string swallowed **31** (from 3385) —
 * both reproduced here before the fix, both now 0. The decoy test below is the silent-wrong-value
 * proof: with a `'"'` upstream, a commented-out `TAURI_PLATFORM_TOKENS` beat the real one.
 */
describe("stripRustComments — the literal forms that used to desync it (PR #1067)", () => {
  it("a char literal holding a double quote does not open a phantom string", () => {
    const src = ['fn probe(s: &str) -> bool { s.contains(\'"\') }', "// hidden", "let x = 1;"].join(
      "\n",
    );
    const out = stripRustComments(src);
    expect(out).not.toContain("hidden");
    expect(out).toContain("let x = 1;");
  });

  it("...and the decoy that used to WIN: a commented-out token list behind a `'\"'`", () => {
    // The exact silent-wrong-value shape, on the updater root-of-trust guard's own const. Under the
    // old scanner `rustStrSliceAfter` returned ["macos","EVIL"] from the COMMENT.
    const src = [
      "fn probe(s: &str) -> bool { s.contains('\"') }",
      '// pub const TAURI_PLATFORM_TOKENS: &[&str] = &["macos", "EVIL"];',
      'pub const TAURI_PLATFORM_TOKENS: &[&str] = &["macos", "windows"];',
    ].join("\n");
    expect(rustStrSliceAfter(stripRustComments(src), "pub const TAURI_PLATFORM_TOKENS")).toEqual([
      "macos",
      "windows",
    ]);
  });

  it("a raw string ending in a backslash closes at its quote, not at an escape", () => {
    const src = ['let p = r"\\\\?\\UNC\\";', "// hidden", "let x = 1;"].join("\n");
    const out = stripRustComments(src);
    expect(out).not.toContain("hidden");
    expect(out).toContain("let x = 1;");
  });

  it("a hashed raw string may contain a bare quote without ending", () => {
    const src = ['let p = r#"say "hi" now"#;', "// hidden", "let x = 1;"].join("\n");
    expect(stripRustComments(src)).not.toContain("hidden");
  });

  it("`for` and `char` are not mistaken for a raw-string prefix", () => {
    const src = ['for c in "abc".chars() {}', "// hidden", "let x = 1;"].join("\n");
    expect(stripRustComments(src)).not.toContain("hidden");
  });

  it("an escaped char literal (a lone tick) does not desync", () => {
    const src = ["let q = '\\'';", "// hidden", "let x = 1;"].join("\n");
    expect(stripRustComments(src)).not.toContain("hidden");
  });

  it("a lifetime is not read as an unterminated char literal", () => {
    const src = ["fn f<'a>(x: &'a str) -> &'a str { x }", "// hidden", "let x = 1;"].join("\n");
    const out = stripRustComments(src);
    expect(out).not.toContain("hidden");
    expect(out).toContain("fn f<'a>");
  });

  it("nested block comments close at the OUTER marker, not the inner one", () => {
    // Legal Rust. A depth-less scanner ends the comment early and leaks the rest.
    const inner = ["/* outer", "  /* inner */", "  still commented: EVIL", "*/", "let x = 1;"];
    const out = stripRustComments(inner.join("\n"));
    expect(out).not.toContain("EVIL");
    expect(out).toContain("let x = 1;");
  });

  it("the post-strip invariant makes ANY desync loud, including an unmodelled one", () => {
    // A shape the scanner does not model: a byte-string prefix it does not know. Whatever the cause,
    // a surviving comment line throws instead of handing back a plausible wrong answer.
    const desynced = '"unterminated\n// this line survives\nlet x = 1;';
    expect(() => stripRustComments(desynced)).toThrow(/desynced/);
  });

  it("every Rust file this repo's derivations scan strips with zero surviving comment lines", () => {
    // The regression leg. These four are the files rustSource.ts is actually pointed at today; the
    // first two are the ones that leaked. `stripRustComments` throws on a leak, so reaching the
    // assertion at all is the proof.
    for (const parts of [
      ["src-tauri", "src", "lib.rs"],
      ["crates", "server", "src", "fsutil.rs"],
      ["crates", "updater-verify", "src", "platform_config_guard.rs"],
      ["crates", "server", "examples", "gen_vault_fixture.rs"],
    ]) {
      const stripped = stripRustComments(readFileSync(join(process.cwd(), ...parts), "utf8"));
      const survivors = stripped.split("\n").filter((l) => l.trimStart().startsWith("//"));
      expect(survivors, parts.join("/")).toEqual([]);
    }
  });
});

describe("rustStringLiteralAfter", () => {
  it("resolves \\\" and \\\\", () => {
    expect(rustStringLiteralAfter('x("a \\"q\\" b\\\\c")', 0)).toBe('a "q" b\\c');
  });

  it("swallows a backslash-at-end-of-line continuation AND the next line's indentation", () => {
    // The part a naive join gets wrong: without this the leading spaces land inside the string.
    const src = 'x("one \\\n                 two")';
    expect(rustStringLiteralAfter(src, 0)).toBe("one two");
  });
});

describe("rustStrSliceAfter", () => {
  const CONST = 'pub const T: &[&str] = &["macos", "windows", "linux"];';

  it("reads every element of a &[&str] slice literal", () => {
    expect(rustStrSliceAfter(CONST, "pub const T")).toEqual(["macos", "windows", "linux"]);
  });

  it("reads a slice written across several lines", () => {
    const multi = ["pub const T: &[&str] = &[", '    "a",', '    "b",', "];"].join("\n");
    expect(rustStrSliceAfter(multi, "pub const T")).toEqual(["a", "b"]);
  });

  it("throws — loudly — when the anchor is gone, rather than deriving an empty list", () => {
    // A renamed const must red. An empty derived list would match nothing and pass vacuously, which
    // is the failure mode CPE-1932 calls out: enumerate, don't recall.
    expect(() => rustStrSliceAfter(CONST, "pub const RENAMED")).toThrow(/anchor not found/);
  });

  it("a comment quoting the OLD list cannot be mistaken for the real one (stripped first)", () => {
    const hostile = [
      '// Was: pub const T: &[&str] = &["stale"];',
      'pub const T: &[&str] = &["current"];',
    ].join("\n");
    // Raw source: the comment's copy comes first. Until CPE-1987's SEC-1 fix this silently returned
    // ["stale"] -- the wrong-value class. It now throws, because the anchor occurs twice; the value
    // of stripping is unchanged, but the failure mode when someone forgets it is no longer silent.
    expect(() => rustStrSliceAfter(hostile, "pub const T")).toThrow(/anchor is not unique/);
    // Stripped first (what every caller does): the real declaration is what is read.
    expect(rustStrSliceAfter(stripRustComments(hostile), "pub const T")).toEqual(["current"]);
  });

  it("reads the real TAURI_PLATFORM_TOKENS out of the shipped guard", () => {
    const src = stripRustComments(
      readFileSync(
        join(process.cwd(), "crates", "updater-verify", "src", "platform_config_guard.rs"),
        "utf8",
      ),
    );
    expect(rustStrSliceAfter(src, "pub const TAURI_PLATFORM_TOKENS")).toEqual([
      "macos",
      "windows",
      "linux",
      "android",
      "ios",
    ]);
  });
});

/**
 * CPE-1987. The scalar sibling of `rustStrSliceAfter`, added so the updater root-of-trust pubkey pin
 * in `sidecarBundleResources.test.ts` could be READ out of `pinned_pubkey.rs` instead of asking a
 * comment to keep two literals in lockstep.
 */
describe("rustStrConstAfter", () => {
  const CONST = 'pub const K: &str = "abc";';

  it("reads the value a `&str` const binds", () => {
    expect(rustStrConstAfter(CONST, "pub const K")).toBe("abc");
  });

  it("throws — loudly — when the anchor is gone, rather than reading the next literal in the file", () => {
    // CPE-1932: a renamed or deleted const must red. Returning some other declaration's literal is
    // the silent-wrong-value class this module exists to close.
    expect(() => rustStrConstAfter(CONST, "pub const RENAMED")).toThrow(/anchor not found/);
  });

  it("a comment quoting the OLD value cannot be mistaken for the real one (stripped first)", () => {
    const hostile = [
      '// Was: pub const K: &str = "stale";',
      'pub const K: &str = "current";',
    ].join("\n");
    // Raw source: the comment's copy comes first. Since CPE-1987's SEC-1 fix that is a THROW rather
    // than a silent "stale" -- two occurrences of the anchor, so the reader refuses to pick one.
    expect(() => rustStrConstAfter(hostile, "pub const K")).toThrow(/anchor is not unique/);
    // Stripped first (what every caller does): the real declaration is what is read.
    expect(rustStrConstAfter(stripRustComments(hostile), "pub const K")).toBe("current");
  });

  it("refuses a const that is not bound to a plain string literal", () => {
    // The shape that matters: the const still exists but no longer HOLDS the value, so the next `"`
    // in the file belongs to a later declaration. Reading it would certify the wrong value silently.
    const indirect = ['pub const K: &str = OTHER;', 'pub const L: &str = "not mine";'].join("\n");
    expect(() => rustStrConstAfter(indirect, "pub const K")).toThrow(/not bound to a plain string/);
    expect(() => rustStrConstAfter('pub const K: &str = concat!("a", "b");', "pub const K")).toThrow(
      /not bound to a plain string/,
    );
  });

  it("still accepts the legitimate literal shapes (the complement of the refusal above)", () => {
    // CPE-1900 rule 2: when you tighten a matcher, write the test that fails the LAZIEST passing
    // implementation. An over-eager "must be exactly `= \"`" refusal would break both of these, which
    // are ordinary Rust and carry no indirection at all.
    expect(rustStrConstAfter('pub const K: &str =\n    "wrapped";', "pub const K")).toBe("wrapped");
    expect(rustStrConstAfter('pub const K: &str = "a \\"q\\" b";', "pub const K")).toBe('a "q" b');
  });

  it("reads the real EXPECTED_TAURI_UPDATER_PUBKEY out of the shipped pin", () => {
    const src = stripRustComments(
      readFileSync(
        join(process.cwd(), "crates", "updater-verify", "src", "pinned_pubkey.rs"),
        "utf8",
      ),
    );
    const pubkey = rustStrConstAfter(src, "pub const EXPECTED_TAURI_UPDATER_PUBKEY");
    // Deliberately NOT a copy of the key: asserting the value here would re-create the third literal
    // CPE-1987 deleted. This pins only that a minisign public key was actually read — the base64 of
    // "untrusted comment: minisign public key: " — and `sidecarBundleResources.test.ts` is where the
    // value itself is checked, against every shipped build leg's merged config.
    expect(pubkey.startsWith("dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6")).toBe(true);
    expect(pubkey.length).toBeGreaterThan(80);
  });
});

/**
 * CPE-1987, PR #1108 review SEC-1 — **the three shapes that made a text scan and rustc disagree, each
 * of which used to derive a decoy SILENTLY.** Demonstrated end to end by the reviewer at that PR's
 * first head: overlay + `release.yml`'s matrix `args:` + one decoy const, **74/74 passed** with an
 * attacker root of trust on all six shipped legs.
 *
 * Both readers are swept, because `EXPECTED_TAURI_UPDATER_ENDPOINTS` had the identical hole. The
 * `_LEGACY` and `#[cfg]` shapes are the ones a normal-looking PR could carry; the raw-string shape is
 * the one that survives comment stripping *by design*, because a raw string is code.
 *
 * The complement matters as much as the refusal here: an anchor that is genuinely unique must still
 * be read, including when a LONGER name that merely contains it is nowhere in the file. The last two
 * cases pin that, so a lazier "refuse if anything resembles the anchor twice" cannot pass.
 */
describe("a non-unique anchor is refused, never guessed (CPE-1987 SEC-1)", () => {
  const REAL_STR = 'pub const K: &str = "REAL";';
  const REAL_SLICE = 'pub const E: &[&str] = &["real"];';

  const shapes: Array<{ name: string; before: string }> = [
    { name: "a longer name with the anchor as its prefix", before: "_LEGACY" },
    { name: "a #[cfg]-gated duplicate that never compiles on this host", before: "cfg" },
    { name: "the anchor text inside an earlier raw string", before: "raw" },
  ];

  function plant(kind: string, decl: string, anchor: string, decoy: string): string {
    if (kind === "_LEGACY") return `${anchor}_LEGACY: ${decoy}\n${decl}`;
    if (kind === "cfg") return `#[cfg(target_os = "android")]\n${anchor}: ${decoy}\n${decl}`;
    return `pub const NOTE: &str = r#"see ${anchor}: ${decoy}"#;\n${decl}`;
  }

  shapes.forEach((shape) => {
    it(`${shape.name} — the &str reader refuses rather than deriving the decoy`, () => {
      const src = plant(shape.before, REAL_STR, "pub const K", '&str = "DECOY";');
      expect(() => rustStrConstAfter(stripRustComments(src), "pub const K")).toThrow(
        /anchor is not unique/,
      );
    });

    it(`${shape.name} — the &[&str] reader refuses too`, () => {
      const src = plant(shape.before, REAL_SLICE, "pub const E", '&[&str] = &["decoy"];');
      expect(() => rustStrSliceAfter(stripRustComments(src), "pub const E")).toThrow(
        /anchor is not unique/,
      );
    });
  });

  it("the refusal names the line of the second occurrence, so the decoy is findable", () => {
    const src = ['pub const K_LEGACY: &str = "DECOY";', 'pub const K: &str = "REAL";'].join("\n");
    expect(() => rustStrConstAfter(src, "pub const K")).toThrow(/also occurs at line 2/);
  });

  it("a genuinely unique anchor is still read — the complement of the refusal", () => {
    // The laziest passing implementation of the rule above refuses too much. These two are ordinary
    // single-declaration files and must keep working, or the guard would have taken the derivation
    // down with the decoy.
    expect(rustStrConstAfter(REAL_STR, "pub const K")).toBe("REAL");
    expect(rustStrSliceAfter(REAL_SLICE, "pub const E")).toEqual(["real"]);
  });

  it("a decoy sitting in a COMMENT is still handled by stripping, not by this rule", () => {
    // The two mechanisms are independent and must not be confused for one another: stripping removes
    // the comment entirely, so the anchor is unique again and the live value is read. If this ever
    // starts throwing "not unique", the strip has stopped running -- a different bug with a different
    // fix.
    const src = ['// pub const K: &str = "DECOY";', 'pub const K: &str = "REAL";'].join("\n");
    expect(rustStrConstAfter(stripRustComments(src), "pub const K")).toBe("REAL");
  });
});
