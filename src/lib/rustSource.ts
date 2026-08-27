/**
 * Reading facts out of Rust source, for the guards that derive a provenance claim instead of asserting
 * one (CLAUDE.md → "Derive provenance, don't claim it").
 *
 * `stripRustComments` and `rustStringLiteralAfter` were written for
 * `src/lib/components/MacroRunConfirm.test.ts` (CPE-1933, PR #1056 Finding 2) and lived inside it.
 * CPE-1950 needed them for `sidecarBundleResources.test.ts` and `RepoBrowser.test.ts` too, and this
 * repo has already written four separate hand-rolled strippers before the fifth was caught — so they
 * live here now, with one set of tests (`rustSource.test.ts`), and every Rust-source scanner imports
 * them rather than re-deriving the escape and comment rules. There is no Rust port of this module, so
 * nothing here is pinned by a shared case file the way `shellScriptLines.ts` is; it is a reader, not a
 * reimplementation.
 */

/** A line that, after stripping, still opens with `//` — i.e. the strip desynced. */
const SURVIVING_COMMENT_LINE = /^[ \t]*\/\//m;

/**
 * Blanks Rust line comments and block comments, preserving every offset (comment bytes become spaces)
 * so indices into the result still address the original file.
 *
 * CPE-1933. Anchoring a scan on "the first `format!(` after the fn" (or "the first `&[` after the
 * const") is beaten **silently** by a comment sitting between the signature and the real code that
 * contains the same token and quotes the OLD value: the extractor reads the comment, the fixture still
 * matches it, and the derivation certifies something the code no longer says — the whole purpose,
 * inverted. Every other adversarial shape a Reviewer tried failed loudly; that one did not.
 *
 * Stripping comments before scanning kills the class rather than that one shape, and is the same rule
 * `crates/updater-verify/src/workflow_scan.rs` applies to workflows: **anchor on code, never on text a
 * comment can also contain.**
 *
 * Handled, because a scanner that desyncs is worse than no scanner: `"` string literals (with `\`
 * escapes), **char literals** (`'x'`, `'\''`, `'"'`), **raw strings** (`r"…"`, `r#"…"#`, and the byte
 * forms `b"…"` / `br#"…"#`) which take no escapes at all, and **nested block comments**, which are
 * legal Rust and which a depth-less scanner closes one level too early (a block comment containing
 * another block comment ends at the SECOND close marker, not the first).
 *
 * ## The "loud failure" claim, corrected (PR #1067 review)
 *
 * An earlier version of this doc asserted that a desync fails **loudly** — "never the silent
 * wrong-value pass this stripping exists to prevent" — and listed char literals and raw strings as
 * accepted, hypothetical gaps. That was wrong on both counts, and it was wrong **about files this
 * module is pointed at today**:
 *
 * - `src-tauri/src/lib.rs:8253` contains `path.contains('"')`. Under the old scanner that char literal
 *   opened a phantom string that swallowed **142 `///` lines** (8268–8959) — a window that ends only
 *   3000 lines before `clone_host`, which `RepoBrowser.test.ts` reads.
 * - `crates/server/src/fsutil.rs:3379` contains a raw string ending in a backslash (`r"\\?\UNC\"`),
 *   which the old scanner read as an escape over the closing quote, swallowing **31 lines**.
 *
 * And the failure was **silent, not loud**: with a `'"'` earlier in the file, a commented-out decoy
 * `// pub const TAURI_PLATFORM_TOKENS: &[&str] = &["macos", "EVIL"];` beat the real declaration below
 * it — on the updater root-of-trust guard. The three derivations shipped in CPE-1950 happened to sit
 * outside the leaked windows, which is why they were green: **a parity coincidence, not a property.**
 *
 * So this function now (a) handles those shapes and (b) does not rely on being right. After stripping
 * it asserts the invariant that catches a desync of ANY cause, including one nobody has thought of:
 * **no line may still begin with `//`.** If the scanner walked out of sync, some comment line
 * survives, and that is what makes the failure genuinely loud — it throws here rather than handing a
 * caller a plausible wrong answer. It catches all 173 leaked lines above.
 *
 * ### What the invariant costs, concretely
 *
 * **False positives are real and already in this repo — not hypothetical.** The invariant cannot tell
 * a leaked comment from a string literal whose own content begins `//`, so a file containing one
 * throws even though the strip was correct. A sweep of all 352 `.rs` files (11.1 MB) found **no**
 * unhandled-literal desyncs and exactly **three** files of this kind, all of which throw today:
 *
 * - `crates/server/src/net_share.rs:552` — a `\`-continued string holding a `/proc/mounts` fixture
 *   whose line begins `//fileserver/media …`
 * - `sidecar/agent-board/src/ui.rs:209` — a raw string of embedded JS carrying `//` comments at line
 *   start. **This is the one most likely to bite:** `agent-board` is one of the two board
 *   implementations that must change in lockstep, so it is a prime future derivation target, and it
 *   is one `readFileSync` away from throwing at whoever points a scanner at it.
 * - `sidecar/host/src/scaffold.rs:56` — a raw string template of generated Rust with `//!` at column 0
 *
 * None of the files scanned today is one of them. If you need to scan one of the three, do not weaken
 * the invariant: exclude the known literal, or narrow the input to the region you are reading.
 *
 * **False negatives, stated so the guarantee is not overread:** the invariant checks LINE STARTS, so
 * it catches a leak of full-line comments only. A desync that leaks just a **trailing** comment, or
 * only a **block-comment body**, slips through and can still yield a silent wrong value. Both were
 * reproduced — but both probes required an **unterminated string literal**, which is not valid Rust,
 * and the 352-file sweep found no such shape. So this is a backstop against the realistic desyncs,
 * not a proof of correctness.
 *
 * One shape that is correct as written and needs no handling: string literals are not tracked *inside*
 * block comments, so a block comment ends at the first close marker even when that marker sits inside
 * what looks like a quoted string. That matches rustc, whose lexer nests on the open/close tokens
 * regardless of quoting.
 *
 * Remaining known gap, stated rather than assumed away: a **lifetime immediately followed by a quote**
 * (`'a'` as two tokens) would be read as a char literal. That shape is not valid Rust in the positions
 * that matter, and the invariant above backstops it.
 *
 * **Red-proofed, not assumed** (`rustSource.test.ts`). Disabling char-literal handling fails 3 tests
 * including the real-file regression leg; disabling raw-string handling fails 2; collapsing the block
 * comment depth counter fails the nesting test. Measured before and after over the four files this
 * module is pointed at: `lib.rs` 142 → 0 surviving `///` lines, `fsutil.rs` 31 → 0,
 * `platform_config_guard.rs` 0 → 0, `gen_vault_fixture.rs` 0 → 0. All red-proofs reverted.
 */
export function stripRustComments(src: string): string {
  const out = src.split("");
  let i = 0;
  while (i < src.length) {
    const ch = src[i];

    // Raw string: r"…" / r#"…"# / br#"…"# — no escapes inside, terminated by `"` plus the same run of
    // `#`. Only when the r/br is not the tail of an identifier (`for`, `char`, …).
    const raw = rawStringEnd(src, i);
    if (raw !== null) {
      i = raw;
      continue;
    }

    if (ch === '"') {
      i = normalStringEnd(src, i);
      continue;
    }

    if (ch === "'") {
      // `null` means it is a lifetime, not a char literal — step over the tick and carry on.
      i = charLiteralEnd(src, i) ?? i + 1;
      continue;
    }

    if (ch === "/" && src[i + 1] === "/") {
      while (i < src.length && src[i] !== "\n") {
        out[i] = " ";
        i += 1;
      }
      continue;
    }

    if (ch === "/" && src[i + 1] === "*") {
      // Nested block comments are legal Rust; count depth rather than taking the first `*/`.
      const from = i;
      let depth = 0;
      while (i < src.length) {
        if (src[i] === "/" && src[i + 1] === "*") {
          depth += 1;
          i += 2;
          continue;
        }
        if (src[i] === "*" && src[i + 1] === "/") {
          depth -= 1;
          i += 2;
          if (depth === 0) break;
          continue;
        }
        i += 1;
      }
      for (let j = from; j < i; j += 1) if (out[j] !== "\n") out[j] = " ";
      continue;
    }

    i += 1;
  }

  const stripped = out.join("");
  const leak = SURVIVING_COMMENT_LINE.exec(stripped);
  if (leak) {
    const line = stripped.slice(0, leak.index).split("\n").length;
    throw new Error(
      `stripRustComments desynced: a comment line survived the strip at line ${line}. Something in ` +
        `this source walked the scanner out of sync (an unhandled literal form, most likely), so the ` +
        `stripped text cannot be trusted and anything derived from it would be a plausible WRONG ` +
        `answer. Fix the scanner, not the caller — see this function's doc (CPE-1950, PR #1067).`,
    );
  }
  return stripped;
}

/** Index just past the normal `"…"` string literal opening at `i`. */
function normalStringEnd(src: string, i: number): number {
  let j = i + 1;
  while (j < src.length) {
    if (src[j] === "\\") {
      j += 2;
      continue;
    }
    if (src[j] === '"') return j + 1;
    j += 1;
  }
  return src.length;
}

/**
 * If a raw string opens at `i`, the index just past its terminator; otherwise `null`. The `r`/`br`
 * must not be the tail of a longer identifier, or `for` and `char` would each look like one.
 */
function rawStringEnd(src: string, i: number): number | null {
  let j = i;
  if (src[j] === "b") j += 1;
  if (src[j] !== "r") return null;
  if (i > 0 && /[A-Za-z0-9_]/.test(src[i - 1])) return null;
  j += 1;
  let hashes = 0;
  while (src[j] === "#") {
    hashes += 1;
    j += 1;
  }
  if (src[j] !== '"') return null;
  const terminator = `"${"#".repeat(hashes)}`;
  const close = src.indexOf(terminator, j + 1);
  return close < 0 ? src.length : close + terminator.length;
}

/**
 * If a CHAR literal opens at `i`, the index just past its closing tick; otherwise `null` (a lifetime).
 * Handles `'x'`, an escape (`'\n'`, `'\''`, `'\\'`), and an astral scalar stored as a surrogate pair.
 */
function charLiteralEnd(src: string, i: number): number | null {
  if (src[i + 1] === "\\") {
    const close = src.indexOf("'", i + 3);
    return close < 0 ? null : close + 1;
  }
  if (src[i + 2] === "'") return i + 3;
  const hi = src.charCodeAt(i + 1);
  if (hi >= 0xd800 && hi <= 0xdbff && src[i + 3] === "'") return i + 4;
  return null;
}

/**
 * Reads the Rust string literal starting at the first `"` at or after `fromIndex`, resolving the
 * escapes that actually appear in this repo's literals: `\"`, `\\`, `\n`, `\t`, and — the one that
 * matters — Rust's `\`-at-end-of-line continuation, which swallows the newline AND the next line's
 * indentation. A naive join gets that last one wrong and produces a string with the source's leading
 * spaces embedded in it.
 */
export function rustStringLiteralAfter(src: string, fromIndex: number): string {
  const start = src.indexOf('"', fromIndex);
  if (start < 0) throw new Error("no string literal found");
  let out = "";
  for (let i = start + 1; i < src.length; ) {
    const ch = src[i];
    if (ch === "\\") {
      const next = src[i + 1];
      if (next === "\n" || next === "\r") {
        i += 1;
        while (i < src.length && /\s/.test(src[i])) i += 1;
        continue;
      }
      out += next === "n" ? "\n" : next === "t" ? "\t" : next;
      i += 2;
      continue;
    }
    if (ch === '"') return out;
    out += ch;
    i += 1;
  }
  throw new Error("unterminated Rust string literal");
}

/**
 * Every string literal inside the `&[ … ]` slice literal that follows `anchor` in `src` — e.g. the
 * elements of a `pub const FOO: &[&str] = &["a", "b"];`.
 *
 * `src` must already be comment-stripped ([`stripRustComments`]); passing raw source is exactly the
 * hole that stripping exists to close. Throws if the anchor or the slice is missing, so a renamed
 * const reds loudly rather than deriving an empty list that vacuously matches nothing.
 */
export function rustStrSliceAfter(src: string, anchor: string): string[] {
  const at = src.indexOf(anchor);
  if (at < 0) throw new Error(`anchor not found in Rust source: ${anchor}`);
  // Start after the `=`, never at the first `&[` — the TYPE is written `&[&str]`, so scanning from
  // the anchor lands on the type's own brackets and derives an empty list. (Measured: it did.)
  const eq = src.indexOf("=", at);
  const semi = src.indexOf(";", at);
  const open = eq < 0 ? -1 : src.indexOf("&[", eq);
  if (open < 0 || (semi >= 0 && open > semi)) {
    throw new Error(`no &[…] slice literal follows ${anchor}`);
  }
  const close = src.indexOf("]", open);
  if (close < 0) throw new Error(`unterminated slice literal after ${anchor}`);
  const items: string[] = [];
  let i = open + 2;
  while (i < close) {
    const q = src.indexOf('"', i);
    if (q < 0 || q > close) break;
    const value = rustStringLiteralAfter(src, q);
    items.push(value);
    // Skip past the closing quote of the literal we just read. The literal is escape-free in every
    // slice this is used on, but walk with the same escape rule anyway so a `\"` cannot desync us.
    let j = q + 1;
    for (;;) {
      if (src[j] === "\\") {
        j += 2;
        continue;
      }
      if (src[j] === '"' || j >= close) break;
      j += 1;
    }
    i = j + 1;
  }
  if (items.length === 0) throw new Error(`slice literal after ${anchor} held no string literals`);
  return items;
}
