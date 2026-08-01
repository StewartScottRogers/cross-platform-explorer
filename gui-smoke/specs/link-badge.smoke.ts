// CPE-1208 — headless GUI smoke: the FileList link badge (epic CPE-715, after CPE-1206/CPE-804).
//
// Drives the REAL built app over the seeded tmpDir (opened via `--open`, asserted in
// open-dir.smoke.ts), which wdio.conf.ts#onPrepare also seeds with an intact symlink
// (LINK_GOOD_NAME -> LINK_TARGET_NAME) and a permanently-broken symlink (LINK_BROKEN_NAME -> a name
// that's never created) via `seedLinkBadgeFixture`. Asserts:
//   - the intact symlink's row renders a `[data-testid="link-badge"]` WITHOUT the `.broken` class,
//   - the broken symlink's row renders one WITH the `.broken` class,
//   - the plain target file's row (not itself a symlink) renders no badge at all.
//
// Symlink creation is unprivileged on POSIX but needs Developer Mode/elevation on Windows —
// `seedLinkBadgeFixture` is best-effort and records whether it actually landed the symlinks in
// STATE_FILE's `linkBadgeFixture.supported`. When it's false (sandboxed Windows CI/dev box), this
// spec skips its assertions rather than failing the whole suite over a permission gap it can't
// control — the intact-POSIX-runner path (CI's Linux/macOS legs) is what actually exercises it.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Kept as literals (not imported from wdio.conf.ts) to match this suite's existing convention of
// duplicating seeded filenames rather than reaching across the runner/worker boundary (see
// open-dir.smoke.ts's identical note on FIXTURE_NAME).
const LINK_TARGET_NAME = "CPE-1208-target.txt";
const LINK_GOOD_NAME = "CPE-1208-good-link.txt";
const LINK_BROKEN_NAME = "CPE-1208-broken-link.txt";

describe("CPE-1208 — headless GUI smoke: the link badge + broken-target state", () => {
  let supported = false;

  before(() => {
    const state = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as {
      linkBadgeFixture?: { supported: boolean };
    };
    supported = state.linkBadgeFixture?.supported ?? false;
    if (!supported) {
      // eslint-disable-next-line no-console
      console.warn(
        "[link-badge.smoke] symlink creation was unprivileged on this runner — skipping CPE-1208 assertions",
      );
    }
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "link-badge");
  });

  it("an intact symlink shows the link badge without the broken state", async function () {
    if (!supported) return this.skip();

    // Wait for the listing to settle (mirrors column-picker.smoke.ts's crumb-ready wait).
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    let goodRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        for (const row of await $$(".row")) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes(LINK_GOOD_NAME)) {
            goodRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 15_000, timeoutMsg: `expected a row for "${LINK_GOOD_NAME}"` },
    );

    const badge = await goodRow!.$('[data-testid="link-badge"]');
    await badge.waitForExist({ timeout: 5_000, timeoutMsg: "expected the intact symlink row to show a link badge" });
    const cls = (await badge.getAttribute("class")) ?? "";
    expect(cls).to.not.include("broken");
  });

  it("a broken symlink shows the distinct broken-badge state", async () => {
    if (!supported) return;

    let brokenRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        for (const row of await $$(".row")) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes(LINK_BROKEN_NAME)) {
            brokenRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 15_000, timeoutMsg: `expected a row for "${LINK_BROKEN_NAME}"` },
    );

    const badge = await brokenRow!.$('[data-testid="link-badge"]');
    // The broken state only lands once the lazy `link_status` fetch resolves — poll the class rather
    // than asserting immediately after existence.
    await browser.waitUntil(
      async () => {
        const cls = (await badge.getAttribute("class")) ?? "";
        return cls.includes("broken");
      },
      { timeout: 15_000, timeoutMsg: "expected the broken symlink row's badge to gain the .broken class" },
    );

    await snap("link-badge");
  });

  it("the plain (non-symlink) target file shows no link badge", async () => {
    if (!supported) return;

    let targetRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        for (const row of await $$(".row")) {
          const html = await row.getHTML({ includeSelectorTag: false });
          // Match the target file's row but not the good-link row, which also contains this
          // filename inside its "resolves to" tooltip text once loaded — restrict to rows whose
          // visible name cell is exactly the target's own filename by excluding the link rows'
          // own filenames from the same scan.
          if (html.includes(LINK_TARGET_NAME) && !html.includes(LINK_GOOD_NAME) && !html.includes(LINK_BROKEN_NAME)) {
            targetRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 15_000, timeoutMsg: `expected a row for "${LINK_TARGET_NAME}"` },
    );

    const badge = await targetRow!.$('[data-testid="link-badge"]');
    expect(await badge.isExisting()).to.equal(false);
  });
});
