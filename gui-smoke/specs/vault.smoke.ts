// CPE-1249 (epic CPE-738) — headless GUI smoke pin + Visual Critic screenshot for the encrypted-vault
// mount/browse flow: drives the REAL built app against a genuinely pre-sealed `.cpevault` blob
// (wdio.conf.ts#seedVaultFixture — a real `vault_crypto` blob with a KNOWN passphrase, not a mock),
// unlocks it via its in-app trigger, asserts the decrypted tree becomes browsable, then Locks it and
// asserts the view returns and the badge is locked again.
//
// Reachability: navigate into the dedicated seeded subfolder and activate the `.cpevault` row with a
// non-grabbing CDP double-click (../lib/mouse.js — the reliable primitive under wry's classic-WebDriver
// webview, same as archive-browse.smoke.ts; WebdriverIO's own `.doubleClick()` intermittently registers
// as a single select-click here). App.svelte's `open` → `tryUnlockVault` opens the shared
// PasswordPromptDialog (CPE-1179). Rows are located by the textContent scan the rest of this suite uses.
//
// Falsifiable core (the ticket's AC): a wrong passphrase surfaces the distinct "wrong password" copy and
// does NOT navigate; the correct passphrase decrypts into a session dir the explorer browses, so a known
// inner file row (CPE-1249-inside.txt) appears — if `vault_unlock` were broken (bad invoke, wrong dir,
// failed decrypt) that row would never render and this fails loudly. Lock then returns to the folder and
// the row's badge flips back to locked.
//
// Layout regression guard (UAT on PR #552): the unlocked-vault banner's Lock button must render fully
// inside the viewport — a horizontal overflow pushed it off the right edge and made it unclickable. The
// unlock test asserts no page horizontal overflow AND the Lock button's right edge is within innerWidth.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { doubleClick, type Point } from "../lib/mouse.js"; // CPE-1155: non-grabbing CDP double-click

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
// Duplicated literals rather than importing across the runner/worker boundary — matches this harness's
// established convention (see shred-dialog.smoke.ts / context-menu.smoke.ts's identical notes). Keep in
// sync with wdio.conf.ts#seedVaultFixture.
const VAULT_DIR_NAME = "CPE-1249-vault-folder";
const VAULT_FIXTURE_NAME = "CPE-1249-secret.cpevault";
const VAULT_FIXTURE_PASSPHRASE = "open-sesame-1249";
const VAULT_FIXTURE_INNER_NAME = "CPE-1249-inside.txt";

/** Viewport-space centre of the FIRST `.rows .row` whose text content includes `name` (or `null`) — the
 *  textContent-scan primitive archive-browse.smoke.ts uses for a faithful CDP click under wry. */
async function pointOfRow(name: string): Promise<Point | null> {
  return browser.execute((n) => {
    const rows = Array.from(document.querySelectorAll(".rows .row"));
    const row = rows.find((r) => (r.textContent || "").includes(n));
    if (!row) return null;
    const rect = row.getBoundingClientRect();
    return { x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2) };
  }, name) as Promise<Point | null>;
}

describe("CPE-1249 — headless GUI smoke: unlock a .cpevault, browse it, then lock it", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`vault-fail.png`) — the inline
  // `snap("vault")` below is only reached on a pass. Non-arrow fn so Mocha binds `this`.
  afterEach(async function () {
    await snapFailure(this.currentTest, "vault");
  });

  it("navigates into the dedicated vault fixture folder and shows the locked badge", async () => {
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // plain, non-interactive <span> — just moves focus off any input

    const folderPt = await pointOfRow(VAULT_DIR_NAME);
    expect(folderPt, `expected a row for the seeded "${VAULT_DIR_NAME}"`).to.not.equal(null);
    await doubleClick(folderPt!);

    // The vault row renders with the LOCKED badge (VaultBadge.svelte, derived from the empty vaults store).
    await browser.waitUntil(async () => (await pointOfRow(VAULT_FIXTURE_NAME)) !== null, {
      timeout: 15_000,
      timeoutMsg: `expected a row for the seeded "${VAULT_FIXTURE_NAME}" after navigating in`,
    });
    const badge = await $('[data-testid="vault-badge"]');
    await badge.waitForExist({ timeout: 10_000, timeoutMsg: "expected the vault lock badge to render" });
    expect(await badge.getAttribute("data-vault-state"), "expected the badge to start LOCKED").to.equal("locked");
  });

  it("rejects a wrong passphrase with distinct copy and does not navigate", async () => {
    const vaultPt = await pointOfRow(VAULT_FIXTURE_NAME);
    expect(vaultPt, `expected the vault row for "${VAULT_FIXTURE_NAME}"`).to.not.equal(null);
    await doubleClick(vaultPt!);

    // The shared password prompt (PasswordPromptDialog.svelte, CPE-1179) mounted.
    const field = await $('[data-testid="password-field"]');
    await field.waitForExist({ timeout: 10_000, timeoutMsg: "expected the passphrase dialog to open on activation" });

    await field.setValue("definitely-the-wrong-passphrase");
    await $('[data-testid="ok-btn"]').click();

    // Distinct BadPassphrase copy (vaultStore.classifyUnlockError), and the dialog stays open (remounted
    // clean) — a failed unlock must NOT navigate and must leave no half-open state.
    const errEl = await $('[data-testid="password-error"]');
    await errEl.waitForExist({ timeout: 15_000, timeoutMsg: "expected a wrong-password error line after a bad passphrase" });
    expect((await errEl.getText()).toLowerCase(), "expected the distinct wrong-password copy").to.include("wrong password");
    // Still on the vault folder (the vault row is still present behind the dialog), not in a session dir.
    expect(await pointOfRow(VAULT_FIXTURE_NAME), "a failed unlock must not navigate away").to.not.equal(null);
  });

  it("unlocks with the correct passphrase, browses the decrypted tree, snaps, then locks", async () => {
    // The dialog is still open from the previous step — but it REMOUNTED after the wrong-password
    // re-prompt (fresh empty field), so re-query and type the correct passphrase.
    const field = await $('[data-testid="password-field"]');
    await field.waitForExist({ timeout: 10_000, timeoutMsg: "expected the (remounted) passphrase dialog to still be open" });
    await field.setValue(VAULT_FIXTURE_PASSPHRASE);
    await $('[data-testid="ok-btn"]').click();

    // Core falsifiable assertion: the decrypted tree is browsable — a KNOWN inner file row appears. This
    // only renders if `vault_unlock` really decrypted the blob into the session dir the app navigated into.
    await browser.waitUntil(async () => (await pointOfRow(VAULT_FIXTURE_INNER_NAME)) !== null, {
      timeout: 30_000,
      timeoutMsg: `expected the decrypted inner file "${VAULT_FIXTURE_INNER_NAME}" to be browsable after unlock`,
    });

    // The unlocked-vault banner (VaultBanner.svelte) is shown with its Lock button while browsing inside.
    const banner = await $('[data-testid="vault-banner"]');
    await banner.waitForExist({ timeout: 10_000, timeoutMsg: "expected the unlocked-vault banner to render" });
    expect(await banner.isDisplayed(), "expected the unlocked-vault banner to be visible").to.equal(true);
    const lockBtn = await $('[data-testid="vault-lock"]');
    expect(await lockBtn.isExisting(), "expected the banner's Lock button to render").to.equal(true);

    // Layout regression guard (UAT on PR #552): the banner must not blow the page out horizontally, and
    // the Lock button must sit fully INSIDE the viewport — otherwise it renders off the right edge and is
    // unclickable. Assert (a) no horizontal overflow and (b) the button's right edge is within innerWidth.
    const view = await browser.execute(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      innerWidth: window.innerWidth,
    }));
    expect(
      view.scrollWidth,
      `expected no horizontal overflow (scrollWidth ${view.scrollWidth} <= innerWidth ${view.innerWidth})`,
    ).to.be.at.most(view.innerWidth);
    const btnLoc = await lockBtn.getLocation();
    const btnSize = await lockBtn.getSize();
    expect(
      btnLoc.x + btnSize.width,
      `expected the Lock button's right edge (${Math.round(btnLoc.x + btnSize.width)}) to be within the viewport (${view.innerWidth})`,
    ).to.be.at.most(view.innerWidth);
    expect(await lockBtn.isClickable(), "expected the Lock button to be clickable (inside the viewport)").to.equal(true);

    // CPE-1148/1149: capture the unlocked, browsable state (inner file row + banner) for the Visual Critic
    // BEFORE locking. On a FAILING run this line is never reached — the afterEach captures vault-fail.png.
    await snap("vault");

    // Lock: App navigates OUT of the session dir first, then wipes it. The view returns to the vault
    // folder and the badge flips back to locked.
    await lockBtn.click();
    await browser.waitUntil(async () => (await pointOfRow(VAULT_FIXTURE_NAME)) !== null, {
      timeout: 15_000,
      timeoutMsg: "expected to return to the vault folder after locking",
    });
    const badge = await $('[data-testid="vault-badge"]');
    await badge.waitForExist({ timeout: 10_000, timeoutMsg: "expected the vault row (and its badge) after locking" });
    await browser.waitUntil(async () => (await badge.getAttribute("data-vault-state")) === "locked", {
      timeout: 10_000,
      timeoutMsg: "expected the badge to be LOCKED again after locking",
    });
    // The banner is gone (we're no longer inside the session dir).
    expect(await $('[data-testid="vault-banner"]').isExisting(), "expected the unlocked banner to disappear after lock").to.equal(false);
  });
});
