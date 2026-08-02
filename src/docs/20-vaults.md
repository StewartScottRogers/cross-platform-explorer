---
title: Encrypted Vaults
order: 20
category: Explorer
categoryOrder: 2
---

# Encrypted Vaults

An **encrypted vault** turns a whole folder into a single, password-protected file with a
`.cpevault` extension. Everything inside — every file and subfolder — is encrypted together, so the
contents can only be read by someone who knows the passphrase. It is a simple way to keep a set of
private files safe at rest: on a shared machine, a USB stick, or a backup.

## Creating a vault

1. Right-click a folder and choose **Create encrypted vault…**.
2. Enter a **passphrase**, then type it again to confirm. The two must match — there is no way to
   recover a mistyped or forgotten passphrase, so the confirmation guards against a typo.
3. The **vault file** defaults to `<foldername>.cpevault`, placed right next to the original folder.
   You can change where it goes with **Browse…**.
4. Choose your options (below), then **Create vault**.

The new `.cpevault` appears in the folder with a **locked** badge.

### Options

- **Securely delete the original folder after sealing** — *off by default.* When on, the original
  plaintext folder is deleted once the vault is created. This is **permanent** — it does not go to the
  Recycle Bin / Trash. As a safeguard, the app only deletes the original **after** it has verified
  that the new vault actually opens, so your data is never lost in the process. See *Honest limits*
  below for what "securely delete" can and cannot guarantee on modern storage.
- **Remember this passphrase in this device's keychain** — when on, the passphrase is saved in your
  operating system's secure keychain so you don't have to retype it to unlock this vault later. Its
  default comes from **Settings → Encrypted vaults → Remember vault passphrases in the OS keychain**.

## Unlocking, browsing, and locking

Double-click a `.cpevault` file and enter its passphrase to **unlock** it. While unlocked, the vault
behaves like an ordinary folder — you can browse, open, and edit its contents. A banner across the top
shows you are inside an unlocked vault and offers a **Lock** button.

Click **Lock** (or lock it from its badge) to re-seal the vault. Locking removes the decrypted copy
from disk and the badge returns to **locked**.

## Passphrase and keychain behavior

- Your passphrase is held **in memory only** while you work, and passed straight to the encryption —
  it is never written to a plaintext file and never logged.
- If you opt in to **Remember passphrase**, the passphrase is stored in your device's OS keychain
  (Windows Credential Manager, macOS Keychain, or the Linux secret service). That keychain entry is
  the *only* place a passphrase ever persists.
- You can turn the keychain default off in Settings at any time; existing saved passphrases are
  unaffected until you next create or forget one.

## Honest limits

Vaults are a practical convenience, not a guarantee against a determined forensic adversary. Be aware
of the tradeoffs:

- **Plaintext exists while unlocked.** To let you browse a vault like a normal folder, unlocking
  extracts its contents into a private temporary session folder on disk. That plaintext lives there
  until you lock the vault (locking securely wipes it). If the app crashes while a vault is unlocked,
  that temporary copy can linger until the next unlock/lock cleans it up.
- **A forgotten passphrase is unrecoverable.** There is no backdoor and no reset. If you lose the
  passphrase, the contents are gone for good. That is the whole point of encryption — but it means you
  must keep the passphrase somewhere safe.
- **"Securely delete" is best-effort.** Overwriting the original files before deleting them cannot be
  guaranteed on modern storage: SSDs and other flash media use wear-levelling, so the original cells
  may not actually be overwritten; copy-on-write filesystems (APFS, Btrfs, ZFS) may keep old data in
  snapshots; and copies in backups, temp files, or filesystem journals are never touched. For
  guaranteed erasure, use full-disk encryption in addition to a vault.
