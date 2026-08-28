// CPE-1960 — fails the build if WebdriverIO's `scrollIntoView` COMMAND comes back into this suite.
//
// That command does not call the DOM API: it injects a real mouse wheel through the driver. Since
// webdriverio 9.31.4 (pulled in by CPE-1945's `npm audit fix`, PR #1065) the wheel carries NO `origin`,
// so it lands at viewport (0, 0) with a real delta computed from the element's rect. On WebKitGTK that
// stray wheel relocates the webview's hover target; `Submenu.svelte`'s `on:mouseleave` closes the
// flyout the spec was about to click, and `macro-param-prompt.smoke.ts` died with
// `element (".ctx .flyout .row") still not existing after 5000ms` — the standing red on
// `gui-smoke-linux-verdict` that CPE-1960 fixed.
//
// ONSET AND RATE — measured, not inferred. THE DISCRIMINATOR IS LOCKFILE CONTENT, NOT MERGE TIME.
// Fingerprint each shard-2 job by what `npm ci` installed: `added 479 packages` = webdriverio 9.30.0,
// `added 489 packages` = 9.31.4 (both confirmed against `git show <sha>:gui-smoke/package-lock.json`).
// Over the 32 shard-2 jobs from 2026-08-27T19:12Z to 2026-08-28T00:42Z:
//
//     9.30.0 (479 pkgs)   13 complete (14/14) runs    0 failed this case
//     9.31.4 (489 pkgs)   11 complete (14/14) runs   10 failed this case
//
// First failure: job `98661503323`, 2026-08-27T20:33Z, on the CPE-1945 BRANCH (`c33a9609`) — about two
// hours BEFORE `48aa8697` merged to main at 22:27Z. Job `98669198175` (21:00Z) failed the same way,
// also pre-merge. An earlier draft of this comment placed the onset at the merge and called the failure
// 100% deterministic; both were wrong, and the second is the dangerous one:
//
// JOB `98681871872` IS A CLEAN, COMPLETE RUN *ON* 9.31.4. It checked out `f656f36` (PR #1065's own
// merge commit), installed 489 packages, and reported `14/14 … 24 passed, 0 failed`. Its rect probe is
// byte-identical to the failing run's (`elemRect {x:553.296875, y:589, width:178, height:32}`, viewport
// 1000x700, scroll 0,0) and it dispatched the same `wheel3` — the flyout simply survived that once.
// Same wheel, racy outcome, ~90% not 100%. SO: ONE GREEN CI RUN DOES NOT VERIFY THIS FIX. A green run
// already happened on the broken version.
//
// THE MECHANISM, derived from 9.31.4's installed source plus the failing run's own log (job
// `98705756557`, 23:37:02): the call was `scrollIntoView({ block: "center" })`, so
// `deltaY = 589 - (700 - 32) / 2 = 255`; `inline` was undefined, so `deltaX` kept its initial value
// `targetByOption.start.x = 553.296875`; rounded, `(553, 255)`. Non-zero, so the
// `if (deltaX === 0 && deltaY === 0) return` guard does not fire and the wheel lands at viewport (0, 0).
// `isVisibleY`/`isVisibleX` ARE computed, but they are consulted ONLY inside the `nearest` branches, so
// `block: "center"` bypasses the already-visible check entirely. In the log the row was present 250 ms
// before the wheel (`findElements` -> 1 element; `getHTML` -> `… CPE-1190 Ask Macro`) and gone 208 ms
// after (`findElements` -> [], and on every retry for 5 s). Only the 9.30.0 payload is in the CI logs
// verbatim (`{"type":"scroll","x":-249,"y":-334,"deltaX":0,"deltaY":0,"origin":{…element…}}`, job
// `98686079109`) — Node's inspector elides 9.31.4's as `actions: [Array]`, which is why the numbers
// above are derived from the log's own rect probe instead of quoted from a payload.
//
// `48aa8697` also carried `expect-webdriverio` 5.7.0 -> 6.0.9, a semver-major. Considered and excluded:
// the wheel trace accounts for the failure end to end, and `expect-webdriverio` is not on the
// `scrollIntoView` path at all.
//
// `lib/scrollIntoView.ts`'s `scrollIntoViewCentered()` is the replacement: the page's own
// `Element.scrollIntoView()`, run inside `element.execute`. It is a no-op for the fixed-position menu
// rows that never needed scrolling, and it scrolls the row's REAL scrollable ancestor (`.filelist-pane`)
// for the ones that do — which the wheel-at-(0,0) never did, because this app's document never scrolls.
//
// WHY A TEXT SCAN: the two calls are spelled almost identically and there is no type-level difference to
// lean on — `await el.scrollIntoView(...)` (the command, banned) vs
// `el.execute((node) => node.scrollIntoView(...))` (the DOM API, fine). The discriminator is that the
// command is awaited DIRECTLY on an element expression, so the regex requires `await` followed by an
// unbroken element expression and then `.scrollIntoView(`; a DOM call always sits inside an `execute`
// callback, where a space/arrow intervenes. `sanity check` below pins both directions of that so this
// guard cannot rot into one that matches nothing.
//
// SCOPE — narrower than "no `scrollIntoView` command anywhere", said here rather than implied. Because
// the regex requires a literal `await` immediately before an unbroken element expression, it catches
// `await el.scrollIntoView(…)` and `await Promise.all([row.scrollIntoView(…)])` but MISSES these,
// checked by hand against this exact regex rather than assumed:
//   * `const p = el.scrollIntoView(…); await p;`   — the `await` is detached from the expression
//   * `return el.scrollIntoView();`                 — returned rather than awaited
//   * `await (await $(".x")).scrollIntoView();`     — the expression starts with `(`
// It also enumerates `specs/` and `lib/` NON-RECURSIVELY. Both are flat today (`git ls-files` shows no
// subdirectory under either), so nothing is missed now, but a future subdirectory would go unscanned.
// None of these can produce a false red; they can only let a bad call through. Widening the regex is a
// code change and was out of scope for the prose round that wrote this note.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";

const LIB_DIR = path.dirname(fileURLToPath(import.meta.url));
const GUI_SMOKE_DIR = path.resolve(LIB_DIR, "..");

/** `await <element expression>.scrollIntoView(` — the WebdriverIO command, and only it. */
const WDIO_SCROLL_INTO_VIEW = /\bawait\s+[A-Za-z_$][\w$!.[\]()]*\.scrollIntoView\s*\(/;

/** This file, and only this file: its fixtures below spell the banned call out as data on purpose. */
const SELF = path.basename(fileURLToPath(import.meta.url));

function sourceFiles(): string[] {
  const dirs = [path.join(GUI_SMOKE_DIR, "specs"), LIB_DIR];
  const files: string[] = [];
  for (const dir of dirs) {
    for (const entry of fs.readdirSync(dir)) {
      if (entry.endsWith(".ts") && entry !== SELF) files.push(path.join(dir, entry));
    }
  }
  return files;
}

describe("gui-smoke scrollIntoView usage guard (CPE-1960)", () => {
  it("scans a non-trivial number of source files", () => {
    // A guard that silently scanned nothing would pass forever. Enumerate, don't recall.
    assert.ok(
      sourceFiles().length >= 20,
      `expected to scan the whole of gui-smoke/specs + gui-smoke/lib, found only ${sourceFiles().length} file(s)`,
    );
  });

  it("matches the WebdriverIO command and not the DOM API", () => {
    assert.match('await el.scrollIntoView({ block: "center" });', WDIO_SCROLL_INTO_VIEW);
    assert.match('await brokenRow!.scrollIntoView({ block: "center" });', WDIO_SCROLL_INTO_VIEW);
    assert.match("await rows[0].scrollIntoView();", WDIO_SCROLL_INTO_VIEW);
    assert.doesNotMatch(
      'await el.execute((node) => (node as HTMLElement).scrollIntoView({ block: "center" }));',
      WDIO_SCROLL_INTO_VIEW,
    );
    assert.doesNotMatch('row?.scrollIntoView({ block: "center" });', WDIO_SCROLL_INTO_VIEW);
    assert.doesNotMatch("await scrollIntoViewCentered(row);", WDIO_SCROLL_INTO_VIEW);
  });

  it("no spec or lib file calls WebdriverIO's scrollIntoView command", () => {
    const offenders: string[] = [];
    for (const file of sourceFiles()) {
      const lines = fs.readFileSync(file, "utf-8").split(/\r?\n/);
      lines.forEach((line, i) => {
        if (WDIO_SCROLL_INTO_VIEW.test(line)) {
          offenders.push(`${path.relative(GUI_SMOKE_DIR, file).replace(/\\/g, "/")}:${i + 1}: ${line.trim()}`);
        }
      });
    }

    assert.deepEqual(
      offenders,
      [],
      "WebdriverIO's `scrollIntoView` COMMAND is back in the suite:\n  " +
        offenders.join("\n  ") +
        "\nSince webdriverio 9.31.4 it injects a mouse wheel at viewport (0,0) with no `origin`, which " +
        "closes any hover-opened menu/flyout the spec is working with (CPE-1960 — a permanent red on " +
        "`gui-smoke-linux-verdict`). Use `scrollIntoViewCentered()` from lib/scrollIntoView.ts instead, " +
        "or call the DOM API yourself from inside a `browser.execute`/`element.execute` block.",
    );
  });
});
