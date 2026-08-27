// CPE-1822 — headless GUI smoke coverage for the Trash view (TrashView.svelte), which had NONE before
// this ticket: verified 2026-08-20, nothing under `specs/` referenced `TrashView`, `.tv-`, or the Trash
// toolbar entry. Three visual tickets (CPE-1803, CPE-1804/1805, CPE-1816) shipped changes to what this
// view looks like with no screenshot taken — the Visual Critic on CPE-1816 had to render the component
// itself, headlessly, from the extracted `<style>` block, because no harness existed to photograph the
// real thing. This spec is that harness.
//
// SCOPE: Linux-only (`process.platform === "linux"`), deliberately. Two independent reasons converge on
// the same answer:
//   1. `gui-smoke.yml`'s `windows-latest` leg runs with `continue-on-error: true` and is explicitly
//      documented there as "a deferral, not a decision" (WebView2 crashes at startup on stock
//      `windows-latest`, unrelated to this app) — it is NOT the blocking gate. `ubuntu-latest` (sharded,
//      CPE-1753/CPE-1594) is. This ticket's own acceptance criteria says so explicitly: run on the
//      blocking `GUI smoke (ubuntu-latest)` shards, never `known-failing.json`.
//   2. The technique this spec uses to reach every state below — writing real `.trashinfo` files
//      straight into the freedesktop.org Trash directory (`$XDG_DATA_HOME/Trash` or
//      `~/.local/share/Trash`) — is exactly what `src-tauri/src/lib.rs`'s own CPE-1791/CPE-1804 backend
//      tests do for the SAME reason: it is the one seam that can reliably PRODUCE a degraded or
//      thousands-of-entries-large listing without depending on a foreign tool or minutes of native
//      Recycle-Bin churn. `trash_listing_degrades_to_empty_instead_of_crashing_on_a_malformed_trashinfo_file`
//      (the CPE-1791 panic-boundary pin) is itself `#[cfg(target_os = "linux")]`-only for the identical
//      reason — the Windows Recycle Bin is not a plain-text directory a test can hand-construct, only a
//      COM-mediated store a real (slow) OS call can populate. `trash-titlebar.smoke.ts`'s own header
//      comment left this exact gap open: "the streaming and degraded listing states (no seam to inject
//      either through the real OS Trash short of a genuinely large or broken one)" — this spec is that
//      seam.
//
// THE SEAM, IN DETAIL. `trash::os_limited::list()` (the same dependency `list_trash_stream` calls) reads
// `info/<name>.trashinfo` files and matches each one to a `files/<name>` payload. Every fixture below is
// written directly into that structure via plain `fs.writeFileSync` — same principle every OTHER
// `seedXFixture()` in `wdio.conf.ts` already relies on (constructing INPUT DATA directly on disk, never
// mocking the render): what the app renders is what it genuinely computes from files that genuinely sit
// where the OS trash format says they should. Nothing here goes through the app's own delete UI or a
// native OS trash tool; that is a deliberate, documented choice (see the sizing/degraded notes below),
// not a shortcut around "seeding is honest" — the ticket's own AC points at `cost-ledger.smoke.ts`'s
// "real store seam" for the template, and the on-disk freedesktop Trash format IS that seam here: it is
// exactly what `trash::os_limited::list()` parses in production, byte for byte.
//
// STATES COVERED (CPE-1822's AC, "at minimum", plus one extra the MANUAL-TEST-BURNDOWN.md row this
// ticket retires also names — see that row's own text):
//   - genuinely empty Trash (`trash.empty`)
//   - a populated Trash (real rows, real fields)
//   - the degraded notice with NO entries (CPE-1803's original shape) — extra, not in this ticket's own
//     AC, added because the burndown row this ticket closes names it explicitly and the fixture is
//     nearly free given the sibling degraded-with-entries test below.
//   - the degraded notice WITH entries present — CPE-1805's ordinary shape (`.tv-degraded-banner`) —
//     reached via CPE-1804's per-item skip route: one entry whose `.trashinfo` FILE ITSELF (the OS
//     directory entry `trash::os_limited::list()` reads to build `TrashItem.id`) carries a single raw
//     byte 0xFF — not valid UTF-8, but a perfectly legal filename byte on Linux, the same fact
//     `item_with_undecodable`'s own doc comment states in `src-tauri/src/lib.rs`. That Rust fixture
//     fabricates a `TrashItem` struct directly in memory and never writes a non-UTF-8 `.trashinfo` file
//     or runs `list()` over one — this spec is the first thing in this repo that actually does, over the
//     REAL dependency, which is new coverage rather than a restatement of an existing test. (`name` ends
//     up decodable here — `trash`'s freedesktop backend derives it from the `Path=` value, not from the
//     `.trashinfo` filename — so the skip fires on `id`, not `name`; `skipped == 1` either way, which is
//     the only thing `TrashView.svelte` reads.)
//   - the CPE-1816 mid-stream state (title bar reads "Still loading…", not a count) — see "WHY A LARGE
//     TRASH" below for why this is the honest way to make an inherently transient window observable.
//   - CPE-1816 round-2's sticky-header fix: with the degraded banner showing and the list scrolled, the
//     column header (and its Select-all checkbox) must still be on-screen and hit-testable — pins
//     `.tv-sticky-stack` doing its job under REAL layout, which is exactly what
//     `TrashView.test.ts`'s own structural pin (jsdom, no real layout) cannot verify — see that test's
//     comment and CPE-1816's Work Log, "round 3" section, which explicitly deferred this to CPE-1822.
//
// WHY A LARGE TRASH (mid-stream test). `list_trash_stream`'s whole body — `os_limited::list()` (already
// materialized) then a per-item `metadata()` (an OS lookup, TRASH_LIST_BATCH = 256 per channel flush) —
// runs synchronously inside ONE `spawn_blocking` closure; there is no `.await` between batches. The
// ONLY thing that can make "first batch rendered, summary not yet resolved" observable from outside the
// process is real wall-clock cost: per-item OS lookups for everything past the first 256, PLUS (this
// spec's actual lever) unvirtualized DOM insertion of every rendered `.tv-row` — TrashView.svelte's own
// header comment says so explicitly: "No virtualized DOM windowing here... a listing is bounded by
// what's literally sitting in the Recycle Bin." Real layout/paint work for ~2,500 rows under a headless
// WebKitGTK/Xvfb session is genuinely slow (this suite's OTHER Linux-specific comments — CPE-1481,
// CPE-1507 — already establish that this exact rendering stack is measurably slower than a native
// desktop compositor), and that work runs on the SAME JS main thread that must also process the
// channel's later batches and the command's final resolution — so a big enough listing widens the
// window rather than merely hoping to win a race against a near-instant one. This is reasoned, not
// empirically timed against real CI (this ticket's own environment has no Linux runner to rehearse
// against — see the ticket's Work Log for the honest account of what could and could not be verified
// locally); if evidence from real runs ever shows this case flaking, that is a distinct, visible
// follow-up (an `"intermittent": true` `known-failing.json` entry with cited runs, per that file's own
// documented convention) rather than something to quietly loosen here.
//
// CLEANUP. Every fixture this spec writes is removed again at the end of its own `it()` (not deferred to
// an `after` hook alone, though one exists as a safety net) — this spec runs directly against the
// REAL, shared OS trash directory (same as `trash-titlebar.smoke.ts`'s single seeded file), and unlike
// that one file, this spec's fixtures range into the thousands, so leaving them behind would corrupt
// every other Trash-touching spec that might share this worker's Trash directory. In practice
// `trash-titlebar.smoke.ts` and `trash.smoke.ts` land on DIFFERENT shards (each its own runner, its own
// filesystem, its own Trash directory — `lib/shard.ts`'s cost-based bin-packing, not sort order, decides
// this and can change run to run), so today neither can observe the other's fixtures regardless of which
// runs first. This spec's own cleanup discipline does not depend on that separation holding, though — it
// wipes/removes what it wrote either way, on the off chance a future shard rebalance ever puts the two
// specs on the same worker.
//
// CPE-1819 (a separate, open ticket) is about the copy-pasted gui-smoke COMMAND-PALETTE-open block —
// this spec never opens the palette at all (Trash is reached via the Sidebar's own "Open Trash" row,
// same entry point `trash-titlebar.smoke.ts` already uses), so nothing here is a candidate for that
// extraction.
import { expect } from "chai";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { setTheme } from "../lib/theme.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// See the file header's "SCOPE" section for why this spec only runs its real assertions on Linux.
const IS_LINUX = process.platform === "linux";

/** `$XDG_DATA_HOME/Trash` (or `~/.local/share/Trash` when unset) — same resolution
 *  `wdio.conf.ts#resolveAppDataDir`'s Linux branch already uses for a sibling purpose, and the exact
 *  directory `trash::os_limited::list()`'s `home_trash()` reads (see that fn's own doc comment in
 *  `src-tauri/src/lib.rs`). */
function linuxTrashDir(): string {
  const xdgData = process.env.XDG_DATA_HOME ?? path.join(os.homedir(), ".local", "share");
  return path.join(xdgData, "Trash");
}

/** `YYYY-MM-DDThh:mm:ss` — the freedesktop.org Trash spec's `DeletionDate` shape (no timezone, no
 *  fractional seconds), matching what `move_to_trash` itself writes (per `src-tauri/src/lib.rs`'s
 *  CPE-1791 module comment: "after `move_to_trash`'s own first `writeln!("[Trash Info]")` and before its
 *  second `writeln!("Path=…")`"). */
function trashInfoDate(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** One fabricated Trash entry: a `<trashDir>/info/<name>.trashinfo` file plus its matching
 *  `<trashDir>/files/<name>` payload, written as raw `Buffer` paths so `name` can carry bytes that
 *  aren't valid UTF-8 (the CPE-1804 skip route) exactly like this repo's own Rust fixture,
 *  `item_with_undecodable`, constructs on Linux: "0xFF is not a legal UTF-8 lead byte, but is a
 *  perfectly legal filename byte." `originalPath` is a plain ASCII string deliberately (this spec never
 *  needs the `Path=` field itself to be undecodable, and ASCII needs no percent-encoding under the
 *  freedesktop spec) — which means the skip this construction exercises fires on the entry's `id`
 *  (derived from the `.trashinfo` file's own OS path, which carries `name`'s raw bytes), not on `name`
 *  itself (the `trash` crate's freedesktop backend derives `TrashItem.name` from the decoded `Path=`
 *  value's basename, not from the `.trashinfo` filename) — see the file header's "STATES COVERED" note
 *  for why that distinction doesn't change what this spec is actually proving. */
function fabricateTrashEntry(
  trashDir: string,
  name: string | Buffer,
  originalPath: string,
  deletedAt: Date,
): { trashinfoPath: Buffer; filesPath: Buffer } {
  const infoDir = path.join(trashDir, "info");
  const filesDir = path.join(trashDir, "files");
  const nameBuf = Buffer.isBuffer(name) ? name : Buffer.from(name, "utf-8");
  const trashinfoPath = Buffer.concat([Buffer.from(infoDir + path.sep), nameBuf, Buffer.from(".trashinfo")]);
  const filesPath = Buffer.concat([Buffer.from(filesDir + path.sep), nameBuf]);
  const content = `[Trash Info]\nPath=${originalPath}\nDeletionDate=${trashInfoDate(deletedAt)}\n`;
  fs.writeFileSync(trashinfoPath, content, "utf-8");
  fs.writeFileSync(filesPath, "CPE-1822 gui-smoke fixture\n", "utf-8");
  return { trashinfoPath, filesPath };
}

/** Fabricate `count` genuinely decodable entries, named `cpe-1822-<label>-NNNN.txt`, each with its own
 *  distinct `original_path` (so `.tv-path` renders something real and non-repeating per row). */
function fabricateManyDecodable(
  trashDir: string,
  count: number,
  label: string,
): Array<{ trashinfoPath: Buffer; filesPath: Buffer }> {
  const dir = path.join(trashDir, "info"); // ensure both dirs exist before the loop below
  fs.mkdirSync(dir, { recursive: true });
  fs.mkdirSync(path.join(trashDir, "files"), { recursive: true });
  const now = new Date();
  const out: Array<{ trashinfoPath: Buffer; filesPath: Buffer }> = [];
  for (let i = 0; i < count; i++) {
    const n = String(i).padStart(5, "0");
    const name = `cpe-1822-${label}-${n}.txt`;
    out.push(fabricateTrashEntry(trashDir, name, `/tmp/cpe-1822-${label}/${name}`, now));
  }
  return out;
}

function removeFabricated(entries: Array<{ trashinfoPath: Buffer; filesPath: Buffer }>): void {
  for (const e of entries) {
    try {
      fs.unlinkSync(e.trashinfoPath);
    } catch {
      /* best-effort cleanup — nothing downstream depends on this succeeding */
    }
    try {
      fs.unlinkSync(e.filesPath);
    } catch {
      /* same */
    }
  }
}

/** Wipe every entry currently in the real Trash directory (both `info/` and `files/`), so the "genuinely
 *  empty" test starts from a known-clean baseline regardless of what a sibling spec (or a prior run on a
 *  non-ephemeral machine) left behind — `trash-titlebar.smoke.ts` seeds one real entry and never restores
 *  it. Safe on CI's ephemeral runners (nothing else is using this Trash); on a real developer's own Linux
 *  machine this would be destructive, so the ONLY call site (this spec's `before()` hook) additionally
 *  gates on `process.env.CI` before calling this at all — the rust equivalent
 *  (`src-tauri/src/lib.rs`'s `lock_real_trash`) redirects `XDG_DATA_HOME` to a private scratch dir for
 *  exactly this reason; this spec can't redirect the already-launched app process's own environment, so
 *  it refuses to wipe anything outside CI instead (CPE-1822 review round 2).
 *
 *  `recursive: true` on the `rmSync` below (CPE-1822 review round 2): a trashed DIRECTORY's `files/`
 *  entry is itself a directory, and without it `rmSync` throws `ERR_FS_EISDIR`, silently swallowed by
 *  the `catch`, leaving the entry behind. */
function wipeTrashDir(trashDir: string): void {
  for (const sub of ["info", "files"]) {
    const dir = path.join(trashDir, sub);
    fs.mkdirSync(dir, { recursive: true });
    for (const entry of fs.readdirSync(dir)) {
      try {
        fs.rmSync(path.join(dir, entry), { recursive: true, force: true });
      } catch {
        /* best-effort */
      }
    }
  }
}

/** Same Sidebar-driven entry point `trash-titlebar.smoke.ts` already established: the "Open Trash" row
 *  is a `.nav-item.fav-item` located by its own tooltip text, not by label (classic-WebDriver text
 *  locators don't reliably resolve against wry's webview — see that spec's identical note). */
async function openTrash(): Promise<void> {
  let openTrashBtn: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      for await (const btn of await $$(".nav-item.fav-item")) {
        if ((await btn.getAttribute("title")) === "Browse deleted files and folders") {
          openTrashBtn = btn;
          return true;
        }
      }
      return false;
    },
    { timeout: 10_000, timeoutMsg: 'expected the Sidebar\'s "Open Trash" row' },
  );
  await openTrashBtn!.click();
  await $(".tv-panel").waitForExist({ timeout: 10_000, timeoutMsg: "expected TrashView's .tv-panel to render" });
}

async function closeTrash(): Promise<void> {
  if (await $(".tv-panel").isExisting()) {
    await $(".tv-x").click();
    await $(".tv-panel").waitForExist({ timeout: 5_000, reverse: true, timeoutMsg: "expected TrashView to close" });
  }
}

describe("CPE-1822 — headless GUI smoke: the Trash view's empty/populated/degraded/mid-stream states render, in both themes", () => {
  let trashDir = "";

  before(() => {
    // CPE-1822 review round 2 should-fix: the IS_LINUX guard must come FIRST. `STATE_FILE` is written
    // unconditionally by `wdio.conf.ts#onPrepare` so it always exists on every platform this harness
    // runs on — but reading/parsing it before the guard meant a hypothetical failure to write it would
    // throw here instead of skipping cleanly, on a platform where every `it()` below immediately
    // `this.skip()`s anyway.
    if (!IS_LINUX) return;
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")); // sanity: the shared app process is up
    trashDir = linuxTrashDir();
    // CPE-1822 review round 2 should-fix: only wipe the real Trash directory under CI — see
    // `wipeTrashDir`'s own doc comment for why a bare `IS_LINUX` gate was unsafe for a Linux
    // contributor running this spec locally against their own real Trash.
    if (process.env.CI) {
      wipeTrashDir(trashDir);
    }
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "trash");
    // CPE-1822 review round 2 BLOCKER (failure cascade): closing here — not inside each `it()`'s own
    // `try`/`finally` — means a FAILURE mid-test still closes the overlay before the NEXT `it()` runs.
    // This harness's `resetAppState` runs once per spec FILE, not per test, so a failure that left
    // `.tv-overlay` (z-index 60) sitting over the Sidebar would intercept the next `it()`'s
    // `openTrash()` click — the exact `WebDriverError: element not interactable` shape observed on a
    // live shard-4 run of this suite (PR #1038, a different spec, the same failure SHAPE). `closeTrash`
    // is already defensive (it checks `.tv-panel` exists before acting), so this is a safe no-op once
    // the view is already closed, and the `catch` below never masks the real failure `snapFailure`
    // above already reported.
    try {
      await closeTrash();
    } catch {
      /* best-effort */
    }
    // CPE-1822 review round 2 should-fix: `resetAppState` deliberately does NOT reset theme between
    // tests — `gui-smoke/lib/resetAppState.ts` names `preview-pane.smoke.ts`'s own
    // `afterEach { setTheme("light") }` as "the model to copy, not a coincidence" — and every test
    // below ends on whichever theme it last set. Reset it the same way.
    await setTheme("light");
  });

  // Safety-net cleanup in case an `it()` below throws before reaching its own cleanup — best-effort,
  // never masks the real failure. Same CI gate as the `before()` wipe above, for the same reason.
  after(() => {
    if (!IS_LINUX || !process.env.CI) return;
    try {
      wipeTrashDir(trashDir);
    } catch {
      /* best-effort */
    }
  });

  it("a genuinely empty Trash renders trash.empty, never the degraded state, in both themes", async function () {
    if (!IS_LINUX) {
      this.skip();
      return;
    }
    await openTrash();

    const body = await $(".tv-body");
    await browser.waitUntil(
      async () => (await body.getHTML({ includeSelectorTag: false })).includes("Trash is empty"),
      { timeout: 15_000, timeoutMsg: 'expected the empty-Trash pane to read "Trash is empty"' },
    );
    // CPE-1803's whole point: an unreadable Trash and an empty one must never share a render. Assert the
    // negative half here so a future regression that always shows the degraded note (making THIS test
    // vacuously pass on the wrong branch) is caught.
    expect(await $(".tv-degraded-note").isExisting(), "a genuinely empty Trash must not show the degraded note").to.equal(
      false,
    );
    expect(await $$(".tv-row").length, "a genuinely empty Trash has zero rows").to.equal(0);

    await setTheme("light");
    await snap("trash-empty-light");
    await setTheme("dark");
    await snap("trash-empty-dark");
  });

  it("a populated Trash renders real rows (name/original path/deleted date), in both themes", async function () {
    if (!IS_LINUX) {
      this.skip();
      return;
    }
    const entries = fabricateManyDecodable(trashDir, 3, "populated");
    try {
      await openTrash();
      await browser.waitUntil(async () => (await $$(".tv-row").length) === 3, {
        timeout: 15_000,
        timeoutMsg: "expected 3 rows in the populated Trash listing",
      });

      const headRow = await $(".tv-head-row");
      expect(await headRow.isExisting(), "the populated listing renders its column head row").to.equal(true);
      const countSpan = await $(".tv-count");
      await browser.waitUntil(async () => (await countSpan.getText()).includes("3 items"), {
        timeout: 5_000,
        timeoutMsg: 'expected the title bar to read "3 items" once the pass resolves',
      });

      const firstRowHtml = await (await $$(".tv-row"))[0].getHTML({ includeSelectorTag: false });
      expect(firstRowHtml, "a populated row shows its real fabricated filename").to.match(/cpe-1822-populated-\d{5}\.txt/);
      expect(firstRowHtml, "a populated row shows its real fabricated original path").to.match(
        /\/tmp\/cpe-1822-populated\//,
      );

      await setTheme("light");
      await snap("trash-populated-light");
      await setTheme("dark");
      await snap("trash-populated-dark");
    } finally {
      removeFabricated(entries);
    }
  });

  it("CPE-1803: a degraded listing with NO entries renders its own distinct note, never trash.empty, in both themes", async function () {
    if (!IS_LINUX) {
      this.skip();
      return;
    }
    // The exact construction `src-tauri/src/lib.rs`'s own CPE-1791 pin uses
    // (`trash_listing_degrades_to_empty_instead_of_crashing_on_a_malformed_trashinfo_file`): a
    // `.trashinfo` body whose second line has no `=` — `trash::os_limited::list()` panics parsing it
    // (`freedesktop.rs:139-140`, `split.next().unwrap()`), the app catches that panic and degrades the
    // WHOLE pass to empty (CPE-1791/CPE-1803) rather than crashing. Not in this ticket's own AC (which
    // lists "the degraded notice WITH entries present" as the minimum), but the
    // `MANUAL-TEST-BURNDOWN.md` row this ticket retires (CPE-1803/1804/1805) names this exact state
    // too, and the fixture is nearly free once the sibling degraded-with-entries test already exists.
    fs.mkdirSync(path.join(trashDir, "info"), { recursive: true });
    fs.mkdirSync(path.join(trashDir, "files"), { recursive: true });
    const malformed = path.join(trashDir, "info", "cpe-1822-malformed.trashinfo");
    fs.writeFileSync(malformed, "[Trash Info]\nPath\n", "utf-8");
    try {
      await openTrash();

      // CPE-1822 review round 2 silent-pass fix: wait for the DEGRADED wording specifically, not just
      // for `.tv-degraded-note` to exist. The SAME element renders `trash.stillLoading` ("Still
      // loading…") for the brief window before the pass resolves (TrashView.svelte's `noticeMessage`:
      // `degraded ? degradedMessage : stillLoading`) — a bare existence check plus the
      // `.to.not.include("Trash is empty")` assertion alone would ALSO pass on that transient
      // mid-stream render, proving nothing about CPE-1803's actual wording ever appearing.
      const note = await $(".tv-degraded-note");
      await browser.waitUntil(
        async () => (await note.isExisting()) && (await note.getText()).includes("couldn't be fully read"),
        {
          timeout: 15_000,
          timeoutMsg:
            'expected the degraded-with-no-entries note to resolve to CPE-1803\'s own wording ("...couldn\'t be fully read...")',
        },
      );
      const noteText = await note.getText();
      expect(noteText, "the degraded-empty note must NOT read trash.empty's wording").to.not.include("Trash is empty");
      expect(await $$(".tv-row").length, "a degraded-empty pass has zero rows").to.equal(0);

      await setTheme("light");
      await snap("trash-degraded-empty-light");
      await setTheme("dark");
      await snap("trash-degraded-empty-dark");
    } finally {
      try {
        fs.unlinkSync(malformed);
      } catch {
        /* best-effort */
      }
    }
  });

  it("CPE-1805: a degraded listing WITH entries renders the banner above the rows in both themes, and the sticky header + its checkbox survive a scroll (scrolled screenshot: dark theme only — see the snap() call below)", async function () {
    if (!IS_LINUX) {
      this.skip();
      return;
    }

    // One entry whose `.trashinfo` file carries a raw non-UTF-8 byte (skipped per CPE-1804's rule —
    // see the file header's "STATES COVERED" note for exactly which field this trips and why, and how
    // it differs from `item_with_undecodable` in src-tauri/src/lib.rs) plus enough decodable siblings to
    // make the list genuinely scrollable, so the sticky-header assertion below means something real.
    const decodable = fabricateManyDecodable(trashDir, 30, "degraded");
    const undecodable = fabricateTrashEntry(
      trashDir,
      Buffer.from([0xff]),
      "/tmp/cpe-1822-degraded/undecodable-item",
      new Date(),
    );
    const entries = [...decodable, undecodable];
    try {
      await openTrash();

      const banner = await $(".tv-degraded-banner");
      await banner.waitForExist({ timeout: 15_000, timeoutMsg: "expected the degraded-with-entries banner (.tv-degraded-banner)" });
      expect(await $(".tv-degraded-note").getText(), "the banner's message names the one skipped item").to.include(
        "1 item",
      );
      // The 30 decodable siblings must still list — degraded is driven by the flag alone, never inferred
      // from entries.length (CPE-1804/CPE-1805's whole point).
      await browser.waitUntil(async () => (await $$(".tv-row").length) === 30, {
        timeout: 15_000,
        timeoutMsg: "expected the 30 decodable siblings to still list despite the one undecodable skip",
      });
      // CPE-1803's suppression rule: a degraded pass must never assert a count.
      const countText = await (await $(".tv-count")).getText();
      expect(countText, "a degraded pass must not show an item count").to.not.match(/^\d+ items?/);

      await setTheme("light");
      await snap("trash-degraded-top-light");
      await setTheme("dark");
      await snap("trash-degraded-top-dark");

      // Scroll the listing down — the sticky-header assertion CPE-1816 review round 2 fixed
      // (`.tv-sticky-stack`) and `TrashView.test.ts`'s own structural pin cannot verify under jsdom
      // (no real layout there — see that test's comment).
      //
      // CPE-1822 review round 2 silent-pass fix: assert the scroll actually MOVED. A renamed
      // `.tv-body` (guarded by no test anywhere), or a list that simply isn't taller than its own
      // container, would make the assignment below a no-op — and the sticky-header check that follows
      // would then pass on an UNSCROLLED list, verifying nothing.
      const scrollTop = (await browser.execute(() => {
        const body = document.querySelector(".tv-body") as HTMLElement | null;
        if (!body) return -1;
        body.scrollTop = body.scrollHeight;
        return body.scrollTop;
      })) as number;
      expect(
        scrollTop,
        "the listing must have genuinely scrolled — otherwise the sticky-header check below verifies nothing",
      ).to.be.greaterThan(0);

      const headRow = await $(".tv-head-row");
      const headCheckbox = await $(".tv-head-row .tv-check input");
      await browser.waitUntil(
        async () => {
          if (!(await headRow.isDisplayed()) || !(await headCheckbox.isDisplayed())) return false;
          const loc = await headRow.getLocation();
          const size = await headRow.getSize();
          const vh = (await browser.execute(() => window.innerHeight)) as number;
          const vw = (await browser.execute(() => window.innerWidth)) as number;
          return loc.x >= 0 && loc.y >= 0 && loc.x + size.width <= vw && loc.y + size.height <= vh;
        },
        {
          timeout: 5_000,
          timeoutMsg:
            "expected .tv-head-row (and its Select-all checkbox) to stay fully on-screen and displayed after scrolling a degraded, banner-showing listing",
        },
      );
      // Also directly hit-testable, not merely "on screen" (a transparent overlay could sit above it).
      const hit = (await browser.execute(() => {
        const el = document.querySelector(".tv-head-row .tv-check input");
        if (!el) return null;
        const r = el.getBoundingClientRect();
        const atPoint = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        return atPoint === el || (atPoint != null && el.contains(atPoint));
      })) as boolean | null;
      expect(hit, "the select-all checkbox must be the actual hit target at its own screen position, not covered by the banner").to.equal(
        true,
      );

      // Dark-theme only, deliberately (UAT round 2 note): this is one extra screenshot of the SCROLLED
      // state on top of the mandatory top-of-list light+dark pair already captured above, not a second
      // full light+dark pass — the filename says so (`-dark`), rather than the earlier round's generic
      // `trash-degraded-scrolled`, which read as covering both themes when it only ever captured one.
      await snap("trash-degraded-scrolled-dark");
    } finally {
      removeFabricated(entries);
    }
  });

  // CPE-1822 review round 2 BLOCKER: `this.timeout()` called inside an `it()` BODY is not reliably
  // honoured by `@wdio/mocha-framework` — `wdio.conf.ts`'s own CPE-1702 comment cites real CI evidence:
  // CPE-1679's stress harness called `this.timeout(2_060_000)` and still died at wall-clock ~90.000s on
  // all three of its real runs, WDIO's own mocha-timeout teardown (`deleteSession()`) firing mid-loop
  // every time. The original version of this describe block had exactly that violation (a single
  // `it()` calling `this.timeout(180_000)` to cover fabricating/cleaning up ~2,500 real files plus TWO
  // full listing passes) — replaced with the compliant shape `preview-pane.smoke.ts:179` already uses
  // one layer up (a SUITE-level `this.timeout()`) and, per `wdio.conf.ts`'s own instruction for a
  // per-test budget this large ("split it across multiple `it()`s so each gets a fresh 90s budget"),
  // SPLIT into one `it()` per theme. The shared, expensive fixture setup/teardown moves into
  // `before()`/`after()` HOOKS, where `this.timeout()` reliably widens THAT hook's own budget (unlike
  // inside an `it()` body — see `preview-pane.smoke.ts:179`'s own comment on the same distinction).
  describe("CPE-1816: the mid-stream 'Still loading…' state on a real, large streaming pass", () => {
    let entries: Array<{ trashinfoPath: Buffer; filesPath: Buffer }> = [];

    before(function () {
      if (!IS_LINUX) return;
      this.timeout(120_000);
      entries = fabricateManyDecodable(trashDir, 2_500, "midstream");
    });

    after(function () {
      if (!IS_LINUX || entries.length === 0) return;
      this.timeout(60_000);
      removeFabricated(entries);
    });

    for (const theme of ["light", "dark"] as const) {
      it(`the ${theme}-theme pass: rows render and the title bar reads "Still loading…" before the pass resolves`, async function () {
        if (!IS_LINUX) {
          this.skip();
          return;
        }
        await setTheme(theme);
        await openTrash();

        // The core, falsifiable assertion (CPE-1816): while this pass is still in flight, the title
        // bar's `.tv-count` must read "Still loading…" (`trash.stillLoading`), not a number — and rows
        // must already be visible (the first batch landed, `loading` is false) at the same time.
        //
        // CPE-1822 review round 2: the poll condition now runs entirely inside ONE `browser.execute`
        // call rather than a `getText()` round trip plus a separate `$$(".tv-row").length` round trip.
        // The latter was the dominant cost this test paid for nothing: classic WebDriver's
        // `findElements` serializes one element HANDLE per matched row on EVERY poll tick — up to 2,500
        // of them here — while `document.querySelectorAll(...).length`, computed entirely inside the
        // page, returns one number over one round trip.
        //
        // Kept on the rendered TEXT rather than the `.tv-count-loading` class alone — a deliberate
        // red-proof probe on this ticket found the class can be renamed without redding
        // `TrashView.test.ts`'s own suite (which asserts on text), so the class alone is not the whole
        // load-bearing contract; the visible string is, and it's what the Visual Critic actually judges
        // in the screenshot below. If the pass resolves before this ever observes the combination, the
        // wait throws and this test goes RED — see the file header for why a large real listing is the
        // honest way to make this window observable rather than fabricating it.
        await browser.waitUntil(
          async () => {
            const observed = await browser.execute(() => {
              const span = document.querySelector(".tv-count");
              const text = span ? span.textContent || "" : "";
              return text.includes("Still loading") && document.querySelectorAll(".tv-row").length > 0;
            });
            return observed as boolean;
          },
          {
            timeout: 20_000,
            interval: 15,
            timeoutMsg: `expected the title bar to read "Still loading…" together with rendered rows while the ${theme}-theme 2,500-item Trash pass was still streaming`,
          },
        );

        await snap(`trash-mid-stream-${theme}`);

        // Deliberately does NOT wait for the pass to fully resolve before returning (the original
        // version did, at a 60s budget alone — most of why this test used to sit on the 90s cliff).
        // Nothing downstream needs it to: `afterEach`'s `closeTrash()` closes the view unconditionally,
        // and TrashView.svelte's own `loadGen` supersession means the NEXT test's `openTrash()` starts
        // a genuinely fresh `list_trash_stream` call regardless of whether this pass's summary ever
        // arrived — a still-in-flight stream's later batches are silently dropped once superseded, so
        // there is nothing this wait was protecting.
      });
    }
  });
});
