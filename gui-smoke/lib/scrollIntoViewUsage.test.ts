// CPE-1960 — fails the build if WebdriverIO's `scrollIntoView` COMMAND comes back into this suite.
//
// That command does not call the DOM API: it injects a real mouse wheel through the driver. Since
// webdriverio 9.31.4 (pulled in by CPE-1945's `npm audit fix`, PR #1065) the wheel carries NO `origin`,
// so it lands at viewport (0, 0) with a real delta computed from the element's rect — measured against
// Chrome 151 with that exact build, scrolling an already-on-screen `.ctx .flyout .row` emits
// `{"type":"scroll","x":0,"y":0,"deltaX":560,"deltaY":222}`. On WebKitGTK that stray wheel relocates the
// webview's hover target; `Submenu.svelte`'s `on:mouseleave` closes the flyout the spec was about to
// click, and `macro-param-prompt.smoke.ts` died with `element (".ctx .flyout .row") still not existing
// after 5000ms` on every completed shard-2 run — a permanent red on `gui-smoke-linux-verdict`.
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
