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
 *
 * **Known gap, stated rather than assumed away** (PR #1108 review): the numeric escapes are NOT
 * decoded — `\u{2014}` comes back as the literal characters `u{2014}`, and `\x41` as `x41`. It is a
 * silent wrong value, not a throw, so do not point this at a literal that uses them. Every value read
 * through it today is base64 or an ASCII URL, where those escapes cannot occur, which is why the gap
 * has never been reachable; decode them the day that stops being true.
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
 * The index of `anchor` in `src`, **refusing outright if it occurs more than once.**
 *
 * ## Why this is a refusal and not a "take the first one"
 *
 * CPE-1987, PR #1108 review, SEC-1 — **demonstrated end to end, not inferred.** Both readers below
 * used to take the FIRST textual occurrence. Before that PR nothing security-relevant read a Rust
 * file's *text*, so it did not matter. Then `sidecarBundleResources.test.ts` — the only guard that
 * sees the updater pin through the `--config` **overlay** chain — began deriving its entire pin from
 * `pinned_pubkey.rs`'s text, **while rustc resolves the same name by compiler rules.** Three shapes
 * make text-order and name-resolution disagree, and all three used to derive a decoy *silently*:
 *
 * 1. A **longer name with the anchor as its prefix** declared above the real one
 *    (`EXPECTED_TAURI_UPDATER_PUBKEY_LEGACY`). Two distinct items to rustc; one substring hit here.
 * 2. A **`#[cfg(…)]`-gated duplicate** above the real one. It never compiles on the host, so every
 *    Rust leg is untouched — and a text scan cannot see a cfg at all.
 * 3. The anchor text inside an earlier **raw string** (`r#"…"#`), which `stripRustComments`
 *    *correctly* preserves, because a raw string is code, not a comment.
 *
 * The reviewer built the full attack at this PR's first head: attacker `plugins.updater.pubkey` in
 * `tauri.sidecar.conf.json`, that overlay added to `release.yml`'s three matrix `args:`, and **one
 * decoy const** in `pinned_pubkey.rs` — **74/74 passed**, clippy clean, `cargo test -p
 * cpe-updater-verify` 8/8 ok, attacker root of trust on **all six shipped legs**. The Rust legs are
 * immune (the compiler resolves the name; the cfg decoy never compiles); only the TypeScript leg was
 * fooled, which is precisely the leg that covers overlays.
 *
 * **Counting occurrences closed all three of those, and was still the wrong shape of check — SEC-2,
 * round 3, demonstrated end to end against the same three files.** The first version of this function
 * refused a *second* occurrence but never asked whether the *one* it accepted was a **declaration**.
 * So: make the real declaration not match the anchor — **one extra space after `pub` is enough**, and
 * there is no `cargo fmt --check` anywhere in `ci.yml` to normalise it — then plant the anchor
 * **once**, somewhere `stripRustComments` preserves *by design* because it is code:
 *
 * ```rust
 * // variant D
 * pub const PIN_NOTE: &str =
 *     r#"pub const EXPECTED_TAURI_UPDATER_PUBKEY: &str = "<attacker>";"#;
 * pub  const EXPECTED_TAURI_UPDATER_PUBKEY: &str = "<real>";   // two spaces after `pub`
 *
 * // variant D2 — reads as documentation
 * #[doc = r#"Example: `pub const EXPECTED_TAURI_UPDATER_PUBKEY: &str = "<attacker>";`"#]
 * pub  const EXPECTED_TAURI_UPDATER_PUBKEY: &str = "<real>";
 * ```
 *
 * Anchor occurrences: **one**. The counting rule accepted it and derived the attacker value —
 * reader poisoned **6 failed / 77 passed**, and with the same overlay + `release.yml` `args:` edits as
 * before, the full attack was **83/83 passed**, whole suite green, clippy clean, `cargo test -p
 * cpe-updater-verify` 8/8 ok, attacker root of trust on all six shipped legs. D2 identical.
 *
 * **So it matches a DECLARATION and counts those**: line start, optional indent, the anchor with each
 * run of spaces widened to `[ \t]+` (which is what defeats the `pub  const` half without needing a
 * formatter job), then optional space and the `:` of the type annotation. That closes D and D2 —
 * neither decoy sits at a line start followed by `:` — **keeps** the `#[cfg]`-duplicate refusal, since
 * two real declarations are two matches, and turns the `…_LEGACY` shape from a refusal into a correct
 * read, because `pub const K_LEGACY` is not `pub const K` followed by `:`. A decoy that IS written as a
 * line-start declaration inside a raw string still trips the count and reds.
 *
 * **What a line-anchored match cannot see, stated rather than assumed away.** At least these: a
 * declaration produced by a macro; one written after something else on the same line; one split as
 * `pub\nconst NAME`. All report **not found** — loud, and the safe direction — but they are *not*
 * read, so do not point this at a file that generates its consts. The `cargo fmt --check` gap that
 * made the whitespace half of D invisible is real and is being ticketed separately; this function no
 * longer depends on it.
 *
 * **Both directions sabotage-measured, 2026-08-29, in `rustSource.test.ts` (42 tests), reverted.**
 * Force it to always refuse (`hits.length >= 1`) → **21 failed / 21 passed**, the complement legs
 * among them, so the refusal is not free and an over-eager rewrite cannot pass. Regress it to round
 * 2's substring counting (drop the line anchor and the `:`) → **11 failed / 31 passed**, and the four
 * that name variants **D** and **D2** are in that set — so the SEC-2 shapes are covered by tests that
 * genuinely discriminate, not by a comment. Against the real `pinned_pubkey.rs`, D and D2 each leave
 * the anchor occurring exactly ONCE while the round-2 rule derives `…IEFUVEFDS0VS` ("ATTACKER") and
 * this one derives the live key; a decoy written as a line-start declaration inside a raw string
 * instead reds with both declaration lines named.
 *
 * Refusing on a duplicate is right rather than merely convenient: with two declarations in the file
 * there is no reading of "the" const that a text scanner is entitled to pick, and guessing is how the
 * decoy wins.
 *
 * **This also revises a scope call made in that PR.** It said "no guard against a fourth *copy* of
 * the value appearing" was out of scope. That is no longer the shape of the risk: the derivation made
 * a **second declaration of the anchor** load-bearing, and that is guarded here, in the reader, by one
 * added line — not by editing every pin.
 *
 * **The honest net trade, recorded because the next reader should not have to re-derive it:** the TS
 * pin now trusts this file's *text*, where Rust trusts the compiler's name resolution and cfg — so it
 * is **strictly weaker than the independent literal it replaced was independent**. It is still net
 * positive, because it closes a three-file attack that needed no Rust edit at all and forces the third
 * edit into `pinned_pubkey.rs`, the most-reviewed file in this repo — **but that claim is only true
 * with this uniqueness check in place.**
 */
function uniqueAnchorIndex(src: string, anchor: string): number {
  const pattern = new RegExp(
    "^[ \\t]*" +
      anchor.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/ +/g, "[ \\t]+") +
      "[ \\t]*:",
    "gm",
  );
  const hits = [...src.matchAll(pattern)];
  if (hits.length === 0) {
    throw new Error(
      `anchor not found in Rust source: ${anchor} — no line begins with it and continues into a ` +
        `\`:\` type annotation. Note this is a DECLARATION match, not a substring search: a name that ` +
        `only appears mid-line, inside a string, or in an attribute does not count (CPE-1987 SEC-2).`,
    );
  }
  if (hits.length > 1) {
    const lines = hits.map((h) => src.slice(0, h.index).split("\n").length);
    throw new Error(
      `anchor is not unique in Rust source: ${anchor} — it is declared at line(s) ${lines.join(", ")}. ` +
        `Refusing to guess which declaration is the live one: a text scan takes the FIRST, rustc takes ` +
        `the one its name resolution and \`cfg\`s select, and a second declaration is exactly how those ` +
        `two are made to disagree (CPE-1987 SEC-1). If this is a legitimate second declaration, give ` +
        `the caller an anchor that names only the live one.`,
    );
  }
  return hits[0].index;
}

/**
 * Every string literal inside the `&[ … ]` slice literal that follows `anchor` in `src` — e.g. the
 * elements of a `pub const FOO: &[&str] = &["a", "b"];`.
 *
 * `src` must already be comment-stripped ([`stripRustComments`]); passing raw source is exactly the
 * hole that stripping exists to close. Throws if the anchor or the slice is missing, so a renamed
 * const reds loudly rather than deriving an empty list that vacuously matches nothing — **and throws
 * if the anchor occurs more than once**; see [`uniqueAnchorIndex`] for the demonstrated attack that
 * rule closes. `EXPECTED_TAURI_UPDATER_ENDPOINTS` had the identical hole its `&str` sibling did.
 */
export function rustStrSliceAfter(src: string, anchor: string): string[] {
  const at = uniqueAnchorIndex(src, anchor);
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

/**
 * The value bound by a `pub const NAME: &str = "…";` that follows `anchor` in `src` — the scalar
 * sibling of [`rustStrSliceAfter`], added by CPE-1987 for the updater root-of-trust pin
 * (`EXPECTED_TAURI_UPDATER_PUBKEY`), which is a `&str` rather than a `&[&str]`.
 *
 * `src` must already be comment-stripped ([`stripRustComments`]); passing raw source is the hole
 * stripping exists to close, and `rustSource.test.ts` pins that with a hostile fixture whose comment
 * quotes an old value.
 *
 * **It refuses anything that is not a plain string-literal binding.** Everything between the `=` and
 * the opening `"` must be whitespace, so `= OTHER_CONST;`, `= concat!("a", "b")` and a deleted const
 * (where the next `"` in the file belongs to some *later* declaration) all throw instead of returning
 * a value that came from somewhere the caller did not mean. A silently wrong pin is the whole defect
 * class; a loud throw is the only acceptable failure here. **It also refuses an anchor that occurs
 * more than once** — see [`uniqueAnchorIndex`], which is where that rule and the attack it closes are
 * written down.
 *
 * **CPE-1929, run rather than reasoned about (2026-08-28).** A second, obvious-looking guard — "the
 * literal must also appear before the const's `;`" — was written first, measured, and then DELETED as
 * *shadowed*, rather than kept as belt-and-braces. The two sabotages, and what each one actually says:
 *
 * **Shadowed is a property of the ORDER, so the position has to be stated:** re-inserted **after** the
 * whitespace check it is 26/26 green both with and without it — shadowed. Put it **before** that
 * check and it reds 1. Everything below is the "after" position.
 *
 * - **Disable it** (`if (false && semi >= 0 && quote > semi)`): 26/26 `rustSource.test.ts` green, and
 *   the file behaves identically with the guard present (26/26) and absent (26/26). Nothing reaches it.
 * - **Force its predicate to lie** — the permissive direction of that lie *is* the bullet above. For
 *   the restrictive direction, mind the spelling: `semi >= -1` alone is still **26/26 green**, because
 *   `&& quote > semi` keeps gating it (PR #1108 review, CLAIM-2 — the first write-up of this quoted
 *   4 reds for that spelling and was wrong). The always-refuse spelling is
 *   `semi >= -1 || quote > semi`, and **that** reds 4 — but read those honestly: the four are the
 *   *valid* shapes now being refused, so it proves the LINE executes, not that its predicate can ever
 *   be true.
 *
 * The reachability argument is structural, which is why the pair was believed: a `"` sitting past the
 * const's `;` puts at least that `;` into the `between` slice, so the whitespace check refuses first,
 * on the same underlying fact — there is no input that reaches the second guard with its predicate
 * true. Leaving it in would have read as coverage while being unreachable, which CLAUDE.md calls the
 * one wrong answer.
 */
export function rustStrConstAfter(src: string, anchor: string): string {
  const at = uniqueAnchorIndex(src, anchor);
  const eq = src.indexOf("=", at);
  if (eq < 0) throw new Error(`no \`=\` follows ${anchor}`);
  const quote = src.indexOf('"', eq);
  if (quote < 0) throw new Error(`no string literal follows ${anchor}`);
  const between = src.slice(eq + 1, quote).trim();
  if (between !== "") {
    throw new Error(
      `${anchor} is not bound to a plain string literal: found ${JSON.stringify(
        between.slice(0, 60),
      )} between its \`=\` and the next \`"\`. Refusing rather than reading a literal that belongs ` +
        `to something else.`,
    );
  }
  return rustStringLiteralAfter(src, quote);
}
