// CPE-1249 (epic CPE-738) — headless GUI smoke pin + Visual Critic screenshot for the encrypted-vault
// mount/browse flow: drives the REAL built app against a genuinely pre-sealed `.cpevault` blob
// (wdio.conf.ts#seedVaultFixture — a real `vault_crypto` blob with a KNOWN passphrase, not a mock),
// unlocks it via its in-app trigger, asserts the decrypted tree becomes browsable, then Locks it and
// asserts the view returns and the badge is locked again.
//
// Reachability (mirrors shred-dialog.smoke.ts): navigate into the dedicated seeded subfolder by
// double-clicking its folder row, then activate the `.cpevault` row by double-click (App.svelte's
// `open` → `tryUnlockVault`), which opens the shared PasswordPromptDialog (CPE-1179). Row/element
// locating uses the getHTML-scan primitive the rest of this suite uses (script-injected text locators
// don't reliably resolve against wry's classic-WebDriver webview — see spotlight/shred specs' notes).
//
// Falsifiable core (the ticket's AC): a wrong passphrase surfaces the distinct "wrong password" copy and
// does NOT navigate; the correct passphrase decrypts into a session dir the explorer browses, so a known
// inner file row (CPE-1249-inside.txt) appears — if `vault_unlock` were broken (bad invoke, wrong dir,
// failed decrypt) that row would never render and this fails loudly. Lock then returns to the folder and
// the row's badge flips back to locked.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
// Duplicated literals rather than importing across the runner/worker boundary — matches this harness's
// established convention (see shred-dialog.smoke.ts / context-menu.smoke.ts's identical notes). Keep in
// sync with wdio.conf.ts#seedVaultFixture.
const VAULT_DIR_NAME = "CPE-1249-vault-folder";
const VAULT_FIXTURE_NAME = "CPE-1249-secret.cpevault";
const VAULT_FIXTURE_PASSPHRASE = "open-sesame-1249";
const VAULT_FIXTURE_INNER_NAME = "CPE-1249-inside.txt";

/** The FIRST `.row` element whose rendered HTML contains `name`, or `undefined` — the getHTML-scan
 *  primitive the rest of this suite uses. */
async function rowNamed(name: string): Promise<WebdriverIO.Element | undefined> {
  for await (const row of await $$(".row")) {
    if ((await row.getHTML({ includeSelectorTag: false })).includes(name)) return row;
  }
  return undefined;
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

    const folderRow = await rowNamed(VAULT_DIR_NAME);
    expect(folderRow, `expected a row for the seeded "${VAULT_DIR_NAME}"`).to.not.equal(undefined);
    await folderRow!.doubleClick();

    // The vault row renders with the LOCKED badge (VaultBadge.svelte, derived from the empty vaults store).
    await browser.waitUntil(async () => (await rowNamed(VAULT_FIXTURE_NAME)) !== undefined, {
      timeout: 15_000,
      timeoutMsg: `expected a row for the seeded "${VAULT_FIXTURE_NAME}" after navigating in`,
    });
    const badge = await $('[data-testid="vault-badge"]');
    await badge.waitForExist({ timeout: 10_000, timeoutMsg: "expected the vault lock badge to render" });
    expect(await badge.getAttribute("data-vault-state"), "expected the badge to start LOCKED").to.equal("locked");
  });

  it("rejects a wrong passphrase with distinct copy and does not navigate", async () => {
    const vaultRow = await rowNamed(VAULT_FIXTURE_NAME);
    expect(vaultRow, `expected the vault row for "${VAULT_FIXTURE_NAME}"`).to.not.equal(undefined);
    await vaultRow!.doubleClick();

    // The shared password prompt (PasswordPromptDialog.svelte, CPE-1179) mounted.
    const field = await $('[data-testid="password-field"]');
    await field.waitForExist({ timeout: 10_000, timeoutMsg: "expected the passphrase dialog to open on activation" });

    await field.setValue("definitely-the-wrong-passphrase");
    await $('[data-testid="ok-btn"]').click();

    // Distinct BadPassphrase copy (vaultStore.classifyUnlockError), and the dialog stays open — a failed
    // unlock must NOT navigate and must leave no half-open state.
    const errEl = await $('[data-testid="password-error"]');
    await errEl.waitForExist({ timeout: 15_000, timeoutMsg: "expected a wrong-password error line after a bad passphrase" });
    expect((await errEl.getText()).toLowerCase(), "expected the distinct wrong-password copy").to.include("wrong password");
    // Still on the vault folder (the vault row is still present), not navigated into a session dir.
    expect(await rowNamed(VAULT_FIXTURE_NAME), "a failed unlock must not navigate away").to.not.equal(undefined);
  });

  it("unlocks with the correct passphrase, browses the decrypted tree, snaps, then locks", async () => {
    // The dialog is still open from the previous step — type the correct passphrase (setValue replaces
    // the wrong one still in the field).
    const field = await $('[data-testid="password-field"]');
    await field.setValue(VAULT_FIXTURE_PASSPHRASE);
    await $('[data-testid="ok-btn"]').click();

    // Core falsifiable assertion: the decrypted tree is browsable — a KNOWN inner file row appears. This
    // only renders if `vault_unlock` really decrypted the blob into the session dir the app then
    // navigated into.
    await browser.waitUntil(async () => (await rowNamed(VAULT_FIXTURE_INNER_NAME)) !== undefined, {
      timeout: 30_000,
      timeoutMsg: `expected the decrypted inner file "${VAULT_FIXTURE_INNER_NAME}" to be browsable after unlock`,
    });

    // The unlocked-vault banner (VaultBanner.svelte) is shown with its Lock button while browsing inside.
    const banner = await $('[data-testid="vault-banner"]');
    await banner.waitForExist({ timeout: 10_000, timeoutMsg: "expected the unlocked-vault banner to render" });
    expect(await banner.isDisplayed(), "expected the unlocked-vault banner to be visible").to.equal(true);
    const lockBtn = await $('[data-testid="vault-lock"]');
    expect(await lockBtn.isExisting(), "expected the banner's Lock button to render").to.equal(true);

    // CPE-1148/1149: capture the unlocked, browsable state (inner file row + banner) for the Visual Critic
    // BEFORE locking. On a FAILING run this line is never reached — the afterEach captures vault-fail.png.
    await snap("vault");

    // Lock: App navigates OUT of the session dir first, then wipes it. The view returns to the vault
    // folder and the badge flips back to locked.
    await lockBtn.click();
    await browser.waitUntil(async () => (await rowNamed(VAULT_FIXTURE_NAME)) !== undefined, {
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
