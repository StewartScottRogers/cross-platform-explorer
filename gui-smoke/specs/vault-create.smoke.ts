// CPE-1250 (epic CPE-738) — headless GUI smoke pin + Visual Critic screenshot for the "Create
// encrypted vault…" flow (VaultCreateDialog.svelte). Drives the REAL built app: navigates into a seeded
// plaintext source folder (wdio.conf.ts#seedVaultCreateFixture), right-clicks it, opens the create
// action, enters MATCHING passphrases with "securely delete the original" left OFF (never destroy the
// fixture in CI), confirms, and asserts a real `.cpevault` appears in the listing with the LOCKED badge.
//
// Fixture isolation (learning from CPE-1249's full-suite gate): the source is seeded as a subfolder
// INSIDE the CPE-1241 shred folder, NOT as a new top-level folder — a new top-level folder shifts every
// root file row down one and can push fold-sensitive rows (archive-browse/-password's root `.tar.gz`)
// below the visible fold. Nesting keeps the tmpDir ROOT listing byte-identical to main.
//
// Falsifiable core (the ticket's AC): a mismatched confirm keeps the Create button disabled and never
// calls the backend; matching passphrases + confirm actually seal the folder, so a `payload.cpevault`
// blob row (with VaultBadge locked) renders — if `vault_create` were broken (bad invoke / failed seal)
// that row would never appear and this fails loudly.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { rightClick, doubleClick, type Point } from "../lib/mouse.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
// Duplicated literals rather than importing across the runner/worker boundary — matches this suite's
// established convention. "Keep in sync with wdio.conf.ts#seedVaultCreateFixture" is no longer left to
// the reader: `src/lib/guiSmokeFixtureLiterals.test.ts` (root vitest, every PR) reads these four
// declarations and wdio.conf.ts's exported ones and compares them, so a drift reds there instead of
// showing up here as a spec timing out looking for a folder the seeder never wrote (CPE-1950). The
// duplication stays deliberate; only the unchecked claim about it is gone.
const SHRED_DIR_NAME = "CPE-1241-shred-folder"; // the (same-epic) host folder the source is nested in
const VAULT_CREATE_PARENT_DIR = "CPE-1250-vault-create";
const VAULT_CREATE_SRC_DIR = "payload";
const VAULT_CREATE_BLOB_NAME = "payload.cpevault";
const PASSPHRASE = "correct horse battery staple";

/** Viewport-space centre of the FIRST `.row` whose rendered HTML contains `name`, or `null` — the
 *  getHTML-scan primitive shred-dialog.smoke.ts uses (no scroll needed here: every row visited sits in a
 *  tiny listing well above the fold). */
async function pointOfRowNamed(name: string): Promise<Point | null> {
  const rows = $$(".row");
  for await (const row of rows) {
    if ((await row.getHTML({ includeSelectorTag: false })).includes(name)) {
      const loc = await row.getLocation();
      const size = await row.getSize();
      return { x: Math.round(loc.x + Math.min(60, size.width / 2)), y: Math.round(loc.y + size.height / 2) };
    }
  }
  return null;
}

async function doubleClickRow(name: string): Promise<void> {
  const pt = await pointOfRowNamed(name);
  expect(pt, `expected a row for "${name}"`).to.not.equal(null);
  await doubleClick(pt!);
}

describe("CPE-1250 — headless GUI smoke: create an encrypted vault from a folder", () => {
  let tmpDir = "";

  before(() => {
    ({ tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string });
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`vault-create-fail.png`) — the
  // inline `snap("vault-create")` below is only reached on a pass. Non-arrow fn so Mocha binds `this`.
  afterEach(async function () {
    await snapFailure(this.currentTest, "vault-create");
  });

  it("navigates to the seeded source folder", async () => {
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // plain, non-interactive <span> — just moves focus off any input

    await doubleClickRow(SHRED_DIR_NAME);
    await browser.waitUntil(async () => (await pointOfRowNamed(VAULT_CREATE_PARENT_DIR)) !== null, {
      timeout: 15_000,
      timeoutMsg: `expected the "${VAULT_CREATE_PARENT_DIR}" folder after entering "${SHRED_DIR_NAME}"`,
    });
    await doubleClickRow(VAULT_CREATE_PARENT_DIR);
    await browser.waitUntil(async () => (await pointOfRowNamed(VAULT_CREATE_SRC_DIR)) !== null, {
      timeout: 15_000,
      timeoutMsg: `expected the "${VAULT_CREATE_SRC_DIR}" folder to seal`,
    });
  });

  it("opens the create action, requires a matching confirm, then seals — a locked .cpevault appears", async () => {
    // Right-click the source folder → the app's own context menu (ContextMenu.svelte), which offers the
    // `vaultable`-gated "Create encrypted vault…" row for a single selected folder.
    const srcPt = await pointOfRowNamed(VAULT_CREATE_SRC_DIR);
    expect(srcPt, `expected the source folder row "${VAULT_CREATE_SRC_DIR}"`).to.not.equal(null);
    await rightClick(srcPt!);

    const ctx = await $(".ctx");
    await ctx.waitForExist({ timeout: 10_000, timeoutMsg: "expected the item context menu (.ctx) to open" });

    let createBtn: WebdriverIO.Element | undefined;
    for await (const btn of await $$(".ctx button.row")) {
      if ((await btn.getHTML({ includeSelectorTag: false })).includes("Create encrypted vault")) {
        createBtn = btn;
        break;
      }
    }
    expect(createBtn, 'expected a ".ctx button.row" labelled "Create encrypted vault…"').to.not.equal(undefined);
    await createBtn!.waitForClickable({ timeout: 10_000 });
    await createBtn!.click();

    // The create dialog mounted (VaultCreateDialog.svelte's static aria-label).
    const dialog = await $('[aria-label="Create encrypted vault"]');
    await dialog.waitForExist({ timeout: 10_000, timeoutMsg: "expected the VaultCreateDialog to render" });

    // The destination defaults to a SIBLING <foldername>.cpevault.
    const destField = await $('[data-testid="vault-dest"]');
    expect(await destField.getValue(), "expected the default dest to be the sibling payload.cpevault").to.match(
      /payload\.cpevault$/,
    );

    // Falsifiable gate #1: a MISMATCHED confirm keeps Create disabled + shows the inline mismatch error.
    await (await $('[data-testid="vault-passphrase"]')).setValue(PASSPHRASE);
    await (await $('[data-testid="vault-passphrase-confirm"]')).setValue("not-the-same");
    const confirmBtn = await $('[data-testid="vault-create-confirm"]');
    await browser.waitUntil(async () => (await $('[data-testid="vault-passphrase-error"]').isExisting()) === true, {
      timeout: 5_000,
      timeoutMsg: "expected an inline passphrase-mismatch error",
    });
    expect(await confirmBtn.isEnabled(), "Create must be disabled while the passphrases don't match").to.equal(false);

    // Fix the confirm → the error clears and Create enables.
    await (await $('[data-testid="vault-passphrase-confirm"]')).setValue(PASSPHRASE);
    await browser.waitUntil(async () => (await confirmBtn.isEnabled()) === true, {
      timeout: 5_000,
      timeoutMsg: "expected Create to enable once the passphrases match",
    });

    // "Securely delete the original" is left OFF (default) — CI must never destroy the fixture.
    expect(await $('[data-testid="vault-shred"]').isSelected(), "shred-original must default OFF").to.equal(false);

    // CPE-1148/1149: capture the ready-to-create dialog (matching passphrases, sibling dest, shred OFF)
    // for the Visual Critic BEFORE confirming.
    await snap("vault-create");

    await confirmBtn.click();

    // Core assertion: the dialog closes and a real `.cpevault` blob row appears in the listing, carrying
    // the LOCKED VaultBadge — proof `vault_create` actually sealed the folder.
    await dialog.waitForExist({ timeout: 30_000, reverse: true, timeoutMsg: "expected the dialog to close after Create" });
    await browser.waitUntil(async () => (await pointOfRowNamed(VAULT_CREATE_BLOB_NAME)) !== null, {
      timeout: 30_000,
      timeoutMsg: `expected the sealed "${VAULT_CREATE_BLOB_NAME}" to appear in the listing`,
    });
    const badge = await $('[data-testid="vault-badge"]');
    await badge.waitForExist({ timeout: 10_000, timeoutMsg: "expected the new vault's lock badge to render" });
    expect(await badge.getAttribute("data-vault-state"), "expected the freshly-created vault to be LOCKED").to.equal(
      "locked",
    );

    // Falsifiable proof the destructive path never ran: the original source folder + its files survive.
    const srcOnDisk = path.join(tmpDir, SHRED_DIR_NAME, VAULT_CREATE_PARENT_DIR, VAULT_CREATE_SRC_DIR);
    expect(fs.existsSync(path.join(srcOnDisk, "CPE-1250-secret.txt")), "the original plaintext must survive (shred OFF)").to.equal(
      true,
    );
  });
});
