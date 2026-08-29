# Vault security review (CPE-738)

Security design + review record for the **encrypted vaults** feature: per-folder authenticated
encryption that locks/unlocks with a passphrase and mounts transparently for browsing.

> **Status — honest bottom line.** The vault uses well-established, audited cryptographic primitives (it
> does **not** hand-roll crypto) and has undergone a thorough **internal adversarial review** by an
> independent reviewer across every slice (see "Adversarial review record"). That materially de-risks it,
> but it is **not a substitute for a professional external cryptography audit**, which is **recommended
> before shipping vaults to real users at GA**. Until then, treat vaults as "carefully built, internally
> reviewed, not externally audited."

---

## 1. What a vault is

A vault is a folder that has been sealed into a single encrypted file, `<name>.cpevault`. Sealing encrypts
the whole tree; unlocking decrypts it (with the passphrase) into a session directory the explorer browses
as a normal location; locking **re-seals that session directory back into the blob** and then wipes it
(CPE-1645 — see §5). The `.cpevault` blob is the only at-rest artifact once sealed.

## 2. Threat model

**Protects against (at rest, vault locked):**
- Disclosure of the folder's *contents*, *file names*, *file sizes*, and *file count* to anyone who can
  read the `.cpevault` file but does not know the passphrase (a stolen/backed-up/synced file, another
  user on the machine, cloud-sync exposure). The single-blob format hides structure, not just bytes.
- Silent tampering: any modification of the ciphertext is detected on decrypt (authenticated encryption).

**Does NOT protect against (out of scope for v1 — stated plainly):**
- An attacker with **read access while the vault is UNLOCKED** — the decrypted plaintext lives in a
  session directory on disk during that window (see §5, the "mount tradeoff").
- A compromised OS / malware / a keylogger / memory scraping on the live machine (the passphrase and
  decrypted data necessarily exist in memory when in use).
- A weak user passphrase (scrypt raises the cost of guessing but cannot save a trivially guessable
  passphrase — see §4).
- Rubber-hose / coercion, cold-boot memory attacks, or side channels beyond what the underlying `age`
  primitives address.

## 3. Cryptographic design

- **Library:** the [`age`](https://crates.io/crates/age) crate, **v0.12.1**, in **passphrase mode**. This
  is a deliberate choice to avoid hand-rolling AEAD/KDF/nonce management. `age` provides:
  - **AEAD:** ChaCha20-Poly1305 (authenticated encryption — confidentiality + integrity).
  - **Passphrase KDF:** scrypt, with a work factor (see §4).
  - Chunked STREAM AEAD framing with correct per-chunk nonce handling (no nonce-reuse footgun). Note:
    `age` streams *internally*, but the v1 cpe wrapper **buffers the whole tree in memory** when sealing/
    opening (each file is read fully, packed into one plaintext stream, then encrypted; opening reads the
    whole blob + plaintext together) — so peak memory scales with total vault size (~2-3×).
    Streaming-from-disk is future work. See §5.
- **Blob format (`.cpevault`):** `MAGIC (b"CPEVLT1") || u16 schema_version (LE, =1) || age-ciphertext`.
  The magic + version are checked before any crypto; a bad magic / unsupported version yields a distinct,
  non-crypto error (never a panic).
- **Tree serialization:** the folder tree is serialized to one plaintext stream by a deterministic,
  dependency-light internal framing (`kind || path_len || path || data_len || data`, sorted by path) —
  chosen over `tar` for determinism (no mtime/uid/gid/ordering noise) and a trivially auditable,
  panic-free parser. Symlinks/devices/FIFOs are skipped (documented). Non-UTF-8 filenames are rejected
  at encrypt time (v1).
- **Path safety (seal⟺extract symmetry — a hard invariant):** every entry path is validated with the
  **same** `sanitize_rel_path` at **encrypt** time that extraction enforces — rejecting `..` traversal,
  absolute paths, empty/`.` components, backslash/UNC components, and Windows drive-letter components
  (`^[A-Za-z]:`). This guarantees **anything that can be sealed can be extracted** (no "seal-but-never-
  open" data-loss trap), and extraction sanitizes again (defense-in-depth) so a crafted blob cannot write
  outside the output directory (zip-slip defense).
- **Extraction atomicity:** decrypt fully authenticates in memory, then writes into a sibling temp dir and
  renames into place on success; any error removes the staging dir — a failed decrypt never leaves a
  half-populated output. A non-empty target is refused rather than clobbered.
- **DoS hardening:** the scrypt work factor is capped on decrypt (`max_work_factor`) so a crafted blob
  cannot force an unbounded KDF; the framing parser bounds-checks every length (no huge-allocation or
  slice-panic on hostile input).

## 4. Passphrase & key handling

- The passphrase is wrapped in `age`'s `SecretString` at the IPC boundary and flows only to the crypto
  core; it is **never written to a plaintext file or a log**, and never placed in an error message.
- **OS keychain (optional):** if the user opts in ("Remember passphrase in this device's keychain"), the
  passphrase is stored via the [`keyring`](https://crates.io/crates/keyring) v3 crate — Windows Credential
  Manager / macOS Keychain / Linux Secret Service — under service `cpe.vault`, account = SHA-256 of the
  blob path. This is the **only** place a passphrase persists. It is off by a Settings preference by
  default. (Consequence, documented in the user docs: moving/renaming a `.cpevault` orphans its stored
  passphrase under the old path's account.)
- **KDF cost:** scrypt at `age`'s calibrated work factor (~1s on typical hardware). This is a deliberate
  brute-force speed-bump; it does **not** rescue a weak passphrase. The UI warns that a forgotten
  passphrase makes the data **unrecoverable** (there is no backdoor / recovery key by design).

## 5. Known limits & honest caveats

- **Plaintext on disk while UNLOCKED (the "mount tradeoff").** Unlocking decrypts into an app-private
  session directory (`appCacheDir()/vault-sessions/<uuid>`) so the explorer can browse it as a real
  location. While unlocked, that plaintext is on disk. Locking securely wipes it. A future in-memory or
  OS-level (FUSE/dokan) mount could avoid on-disk plaintext but was out of scope for v1.
- **The passphrase is held in memory while UNLOCKED (CPE-1645).** Locking re-seals, and sealing needs the
  passphrase, so the live `Session` holds it as an `age` `SecretString` (zeroize-on-drop, redacted in
  `Debug`) until the vault locks. It still never persists — no file, no log, no status struct, no IPC
  payload — and the exposure is strictly smaller than the one the mount tradeoff above already accepts:
  for that same window the entire decrypted tree is sitting on disk, where an attacker who could scrape
  the passphrase out of process memory could simply read the files instead. Consistent with §2's stated
  non-goal (a compromised OS / memory scraping on the live machine is out of scope).
- **Locking re-seals, verify-before-destroy (CPE-1645).** Until CPE-1645, `encrypt_tree` was called only
  from `create_vault`: `lock` shredded the session directory and left the blob exactly as it was at
  creation, so everything the user wrote while the vault was unlocked was destroyed **silently** — while
  both the user docs and the code comment promised locking would "re-seal". Closed by re-sealing on lock
  under the same discipline `create_vault` applies to its shred: encrypt the session tree, write it to a
  staging file **beside the blob**, re-read that file **from disk** and decrypt it in full
  (`verify_blob`, in memory, no plaintext written), and only then rename it over the vault — and only
  then wipe the working copy. Every failure (encrypt, write, verify, rename) returns `Err` having wiped
  nothing, removed the staging file, and left the old blob byte-for-byte intact, with the mapping kept so
  the lock stays retryable and the user's edits stay reachable. Two refusals guard the re-seal itself: a
  session path that is a link is refused before anything is read through it (it would seal a stranger's
  files INTO the vault as well as shredding them), and a vault file living *inside* the session directory
  is refused (re-sealing there would write a good vault and then shred it with the working copy) — the
  same guard shape as `create_vault`'s `resolves_inside`. A session directory that has vanished re-seals
  nothing and locks cleanly rather than wedging the vault "unlocked" forever.
  - **`vault_lock` can now empty a vault — by design, and it is the one thing here that destroys data.**
    "Always re-seal, never diff" means a deletion made while unlocked is carried into the blob; carried to
    its limit, emptying the session directory's *contents* (the directory itself survives, so containment,
    the link guards and the alias guard all correctly pass) re-seals an empty tree over the vault, and the
    lock reports success. Note the asymmetry with the case above: a **vanished** session directory
    preserves the blob, an **emptied** one replaces it. Before CPE-1645 neither could touch the blob at
    all. The alternative — refusing when the result would be empty — was rejected: it would contradict the
    "a deletion must be carried" rule the whole design rests on, and it is a heuristic, whereas the
    verify-before-destroy ordering is a guarantee. Deleting everything and locking is a legitimate thing
    for a user to do; it is stated in `src/docs/20-vaults.md` and pinned by
    `emptying_the_session_dir_empties_the_vault_and_this_is_deliberate`.
  - **Residual, stated plainly:** re-sealing happens at lock, not continuously. Killing the app while a
    vault is unlocked still loses the edits made in that session (the startup sweep wipes the orphaned
    session dir), and `VaultRegistry::unlock` called again on an already-unlocked vault still supersedes
    (and best-effort wipes) the prior session dir — CPE-1249's deliberate no-orphaned-plaintext
    behaviour — which discards any edits in it. The frontend never re-unlocks an unlocked vault
    (`App.tryUnlockVault` navigates to the live session instead), so that path is reachable only from a
    devtools/automation caller. Symlinks created *inside* an unlocked vault are skipped by the crypto
    core's walk (as they always have been) and then removed with the working copy, so they do not survive
    a lock — also stated in the user docs.
  - **The staging file is created exclusively, under an unpredictable name** (SEC-847 finding 1). The
    first version composed a **deterministic** name (`<blob>.cpe-reseal-tmp`) and wrote it with
    `std::fs::write` — `CREATE_ALWAYS`/`O_CREAT|O_TRUNC`, which follows a symlink and writes *through* a
    hard link. That was a plant-once-and-wait primitive requiring no race and no privilege:
    `create_hard_link(victim, "<blob>.cpe-reseal-tmp")` (a registered IPC command, unelevated on NTFS),
    and the next time the **user** clicked Lock the victim's inode was truncated and filled with vault
    ciphertext — verified byte-for-byte as `CPEVLT1\x01` + `age-encryption.org/v1`, with the UI reporting
    "Locked". Closed by `create_new(true)` (`O_EXCL`: fails `AlreadyExists` on a regular file, a hard link
    *and* a symlink, with no check-then-open window) plus a per-attempt nonce in the name, so the trap
    cannot be set in advance. Stale staging debris from an interrupted lock is swept at the start of the
    next re-seal, but only after `symlink_metadata` proves it is a regular non-symlink file **and**
    `hard_link_count` proves exactly one name points at it — this module never deletes an object it
    cannot prove it created. That link check was `#[cfg(unix)]` until the round-3 audit, i.e. unenforced
    on **Windows, the platform where the unprivileged hard-link primitive actually exists**; no data was
    ever at risk (unlinking one name of an inode destroys nothing), but the stated rule was not the
    shipped rule, and it was deletable with the whole vault suite green. Pinned now by
    `the_sweep_leaves_a_hard_link_planted_at_a_staging_name`.
  - **A hard link inside the session directory is refused** (SEC-847 finding 2). Every other guard here
    reasons about links you can *see* in a directory entry; a hard link is simply another **name** for an
    inode and is indistinguishable from an ordinary file, so the crypto core's skip-every-reparse-point
    walk reads straight through it. `create_hard_link(victim, "<session>/loot.xlsx")` therefore both
    sealed a file from anywhere on the volume into a vault whose passphrase the attacker chose
    (confidentiality — verified by reading the victim's plaintext back out of the blob) and let the wipe's
    shredder overwrite the victim's real file through the alias (integrity). `ensure_no_aliased_files`
    now refuses — never silently skips, which would be the same quiet data loss CPE-1645 exists to end —
    any session-tree file whose link count is not exactly 1, and fails **closed** when the count cannot be
    read (`st_nlink` on Unix; `GetFileInformationByHandle` on Windows, where the std accessor is still
    unstable, reusing `batch_media`'s audited probe details). Scoped to the session re-seal: `create_vault`
    still seals a user-chosen folder as it always has, where hard links are the user's own arrangement of
    their own files.
  - **…and the destructive step re-checks for itself, per file, immediately before writing** (SEC-847
    round-3 audit). The guard above was a **check-then-USE**: it walked the tree once, at the top of the
    re-seal, and the "use" was the shredder several seconds later, after encrypt + the exclusive staging
    write + `sync_all` + a full verifying decrypt (a real scrypt KDF, ~1s **by design**) + the rename.
    `shred_tree` re-walked at the end and overwrote every regular file it found, aliases included, with no
    link check of its own — the one destructive step in this module that depended on a caller's earlier
    check. There was no race to win either: the staging file appearing beside the `.cpevault` is a
    publicly observable **starting gun** proving the guard has already passed, so the attacker polls for
    it and then plants `create_hard_link(victim, "<session>/loot.xlsx")`. The auditor demonstrated a
    victim file zero-filled while `lock` returned `Ok(())` and the UI said "Locked". Two changes:
      - The **session wipe** re-reads each file's link count immediately before overwriting *that* file —
        no window at all between check and write — and **unlinks** rather than overwrites anything that
        is not provably a single-named file. A name that has another name is not ours to destroy, and
        unlinking one of an inode's names destroys no data. Fails **closed against destruction**: an
        unreadable count disposes exactly like a known alias. That closes the integrity half completely.
        `create_vault`'s optional shred-original keeps its old behaviour (`AliasPolicy::ShredEveryFile`) —
        that folder is the user's own pick, not an app-owned session tree.
      - `ensure_no_aliased_files` runs a **second time**, after `encrypt_tree` and before the staging file
        exists, which shrinks the confidentiality half (a victim's plaintext sealed into the attacker's
        vault) to the encrypt walk itself and refuses before the blob is replaced — and before the
        starting gun is fired.
    **This did not close the class, and the round-3 re-audit proved it (CPE-1672).** The wipe was
    **collect-then-shred**: it froze absolute paths, then called `hard_link_count` and `shred_file`, each
    of which re-resolved the whole path again — and the per-file link check had no-follow semantics on the
    **final component only**, with every parent component resolved by the OS. So the attacker skipped hard
    links entirely: plant an innocuous *real* subdirectory before locking (link count 1, not a reparse
    point, so every alias walk passes and it is sealed into the blob), wait for the first shredded file to
    vanish, then `remove_dir_all` it and drop a **junction** in its place pointing at `Documents`. The
    frozen path resolved through the junction, the victim's link count read `One`, and it was securely
    overwritten and unlinked — reproduced 3/3 through the public `VaultRegistry::lock`, with `lock`
    returning `Ok(())` and the UI saying "Locked". Strictly worse than the hard-link variant above: there
    the victim's inode kept its other name, so nothing was lost; here the victim's only name was destroyed.
    Closed by **pinning objects instead of names** — see the next bullet.
    Pinned by `the_session_wipe_unlinks_an_alias_instead_of_overwriting_it` (deterministic),
    `an_alias_planted_after_the_alias_guards_is_unlinked_not_shredded_through` (the auditor's own timing
    exploit, assertion flipped), `an_alias_appearing_during_the_encrypt_walk_is_caught_before_the_blob_is_replaced`
    (via an `after_encrypt` seam, so it needs no thread) and `the_wipe_never_overwrites_a_file_it_cannot_prove_is_ours`.
  - **The wipe destroys objects, not names (CPE-1672).** The shredder no longer collects paths and then
    revisits them. It walks and destroys **inline**, and every destructive step is decided on the
    *object*: each overwrite goes through a single handle opened no-follow whose filesystem identity
    (`(dev, ino)` / volume serial + file index) must match the identity probed when that entry was
    enumerated, and each descent into a subdirectory compares the same way. Per directory the order is
    (1) enumerate and probe — purely observational, so every subdirectory's identity is captured *before*
    the first byte is overwritten anywhere in that directory, i.e. before the attacker's starting gun can
    have fired; (2) re-pin the directory itself, before anything is destroyed; (3) overwrite the files;
    (4) descend, re-pinned. The hard-link count that decides is likewise re-read from the handle rather
    than taken from the earlier probe. **Nothing unlinks by path at all**: the single
    `remove_dir_all(root)` at the end does every removal, and std hardened that against exactly this swap
    in 1.58.1 (CVE-2022-21658) — it recurses through directory handles rather than re-resolving path
    strings, and deletes a reparse point instead of descending into it. This is the same "one handle, one
    answer" shape PR #848 adopted for the Batch Media write path after the identical class of finding, and
    it reuses that module's `FileIdentity`/`handle_facts` primitives rather than a second copy.

    **The residual, with its real size — this is not "no window", and the attacker is not blind.**
    Between a directory's re-pin at the **top of `shred_dir_pinned`** — against the identity its parent
    recorded at enumeration — and the `read_dir` on the next line sits a single syscall; the re-pin after
    the enumeration brackets the far side. So a swap must be **in and back out** between the two, and the
    same must hold one level down: a **four-phase alignment across two gaps**, both of which must land.

    Three claims an earlier draft made here were wrong and are withdrawn (SEC-861 blocking 2, PR #861
    review). **(a)** "No observable signal — the starting gun is gone, because nothing is unlinked
    mid-walk." False: the gun changed *form* rather than disappearing. The overwrite step changes file
    **contents** in a directory the attacker can read, which fires at the same instant the old "file
    vanished" signal did — the regression test in this repo arms off exactly that, so the doc contradicted
    the code shipping beside it. The auditor armed off it too: **400/400**. **(b)** "Restoring an object
    with the same identity" was presented as the hard part; it is free — `rename` aside and back preserves
    the identity, one syscall per phase. **(c)** The gap was mislocated at the per-entry probe inside the
    parent's enumeration, which is *wider* than the truth (it would enclose the post-enumeration re-pin
    and every overwrite) — wrong in the unsafe direction.

    What is actually hard is the timing, and it is **measured, not asserted**: armed with the
    content-change signal and this exact four-phase pattern, the auditor achieved **0 victims in 600
    rounds**, 91 of them refused outright. The honest claim is not "no signal" but "the signal does not
    buy enough". Closing the remainder entirely needs handle-relative traversal (`openat`/`NtCreateFile`
    with a root directory handle), which std does not expose and which is not worth a new dependency here.

    Every one of these guards was neutralised individually and each turned a **distinct** test red:
    the identity arm → `the_wipe_refuses_a_directory_that_is_not_the_object_it_was_told_to_wipe`; the link
    arm → `a_link_is_refused_even_when_there_is_no_identity_to_compare_it_against` and
    `shred_tree_refuses_a_root_that_is_itself_a_link`; the handle identity check →
    `the_overwrite_refuses_a_name_that_now_denotes_a_different_object`; the handle link-count re-read →
    `the_overwrite_re_reads_the_link_count_from_the_handle_it_will_write_through`; the probe-side decline →
    `an_alias_is_declined_before_a_write_handle_is_ever_taken_on_it`. Neutralising the identity arm **and**
    the link arm together — each is independently sufficient — reproduces the original exploit in
    `the_wipe_refuses_a_{junction,symlink}_swapped_in_at_a_parent_directory_mid_wipe`, which rebuild the
    auditor's reproduction from real filesystem objects and read the victim's bytes back off disk. One
    **guard** that no test could tell apart was **removed** rather than kept as reassurance — a duplicate
    root-is-a-link check in `shred_tree`, which consumed the same probe it would have compared and was
    therefore information-free (the root is still refused twice independently, by `wipe_session_dir`'s
    `symlink_metadata` check and by `shred_dir_pinned`'s entry probe, whose reparse-point attribute also
    catches junctions `is_symlink()` may not). Alongside it, a **message phrase** — "was a real one when
    the wipe started" — was dropped for being untrue of the wipe's own root. Calling both of those
    "guards" was loose in a paragraph whose point is precision about what is load-bearing (PR #861 review).
  - **Alternate data streams — the wipe used to report success over retained plaintext** (CPE-1986).
    Found by PR #1101's Security Auditor while confirming a *different* fix (CPE-1957, the cloud-placeholder
    half of the same shape), measured under the **production** alias policy
    (`UnlinkAliasesInsteadOfOverwriting`, the one `wipe_session_dir` passes) and reproduced here before
    anything was changed: `wipe_ok=true main_all_zero=true ads_readable=true ads_still_secret=true`. The
    lock said "Locked", the default stream was genuinely zeroed, and the secret was still on the volume.

    The mechanism is that `read_dir` returns **names** and an overwrite through a name writes the
    **default (`::$DATA`) data stream**. On NTFS a file — *and a directory* — may carry any number of
    **named `$DATA` streams** beside it, each its own run of extents. `remove_dir_all` then unlinks the
    file record and frees those extents **without writing them**. Nothing in the walk could see them and
    nothing in the module mentioned them, so this was an **unstated** residual rather than a declared one,
    which is what made it a defect: as in CPE-1957, *a skip is indistinguishable from a success at the
    API*, and every assertion that existed on this path was satisfied by not touching the data. Streams
    need no privilege to plant (`type secret > file.txt:hidden`) and survive a copy onto NTFS, and they
    also arrive by ordinary means — `Zone.Identifier` from any browser or mail client, `AFP_AfpInfo` from
    a Mac over SMB — so "only odd tooling does this" was never a safe assumption.

    Closed by `vault_manager::shred_alternate_streams`: `FindFirstStreamW`/`FindNextStreamW`
    (`FindStreamInfoStandard`) enumerate the object's streams, and each **named** `$DATA` stream is
    overwritten through `overwrite_pinned_file` — the same function, and therefore the same refusals, the
    default stream already goes through. That reuse is sound because a stream is not a second object:
    measured, a handle opened at `file:name` reports the same volume serial and file index as the file
    itself, the file's link count, no directory bit (even on a directory's stream) and no reparse tag, and
    `metadata().len()` returns the **stream's** length. Called from `shred_dir_pinned` — for each file
    **and for the directory itself** — never from inside `overwrite_pinned_file`, which would recurse.

    Four decisions, each taken deliberately rather than by default:
    - **An unshreddable stream refuses the whole wipe**, retryable, exactly as a busy default stream
      already does. Over-refusing at a wipe costs retained plaintext (CPE-1957's lesson), but a refusal
      happens *before* `remove_dir_all`, so what is retained is plaintext still sitting in the session
      directory — visible, in a known place, retryable — rather than plaintext in extents with no name.
      A silent skip is the one answer that is never right, because it is the defect.
    - **An aliased file's streams are left alone**, like its default stream: they live in the same file
      record, reachable through the other name, so writing them would destroy the other name's data. This
      is SEC-847's hard-link rule applied one level in, and it is asked *before* enumeration so an
      enumeration failure cannot refuse a wipe over an object that was never going to be touched.
    - **Enumeration failure splits by alias policy**, for the reason `same_object_or_refuse` already
      splits: refused for the session tree (the app's own directory on a local volume, where the call does
      not fail), waved through for `create_vault`'s shred-original (a folder the *user* picked, possibly
      on FAT/exFAT, where refusing would break a legitimate feature against an attacker its threat model
      says is absent). `ERROR_HANDLE_EOF` is "no streams", not a failure — measured, that is what a
      directory with none returns. **Not measured:** no FAT-formatted volume was available, so what
      `FindFirstStreamW` returns on one is not quoted here.
    - **Only `$DATA` streams are shredded.** `FindStreamInfoStandard` returned nothing else in any
      measurement taken (an EFS-encrypted file reported `::$DATA` alone; so did one carrying a GUID
      reparse point), so the filter is an **unexercised safety valve**, kept so that a build or filter
      driver reporting a non-`$DATA` attribute cannot turn every wipe into a refusal. Non-`$DATA` NTFS
      attributes (`$EFS`, `$INDEX_ALLOCATION`, `$BITMAP`, `$REPARSE_POINT`) are filesystem metadata, not
      places an ordinary write puts a user's plaintext: a **declared residual**.

    **Windows only, and the two residuals that leaves are declared, not implied.** *(1)* Streams are NTFS;
    on Linux and macOS the analogue is **extended attributes**, including `com.apple.ResourceFork` (where
    a macOS resource fork lives) and `com.apple.FinderInfo`. They have the same property — writing the
    file's data does not touch them, and `unlink` frees their storage unwritten — so the same class of
    residue exists there and is **not** closed. An xattr cannot be overwritten in place through any
    portable API (setting a same-length zeroed value is a request ext4/APFS may satisfy by allocating
    elsewhere), so a Unix arm would buy a weaker guarantee while reading like this one; it wants its own
    ticket. *(2)* `secure_shred::shred_file` — the explorer's user-facing **Shred** command, a different
    feature — has the identical residual on both platforms and was deliberately left alone rather than
    widened into by this ticket; it is stated at that function.

    CPE-1929 sabotage pair on the new refusal, run by hand on **Windows 11** (`cargo test --lib`,
    `crates/server`, baseline 2,461 / 0 / 14 at `2f7b3206` and re-measured **identical** at `9bfb21d7`
    after rebasing — where all three figures below were re-run and came back the same; 2,466 in the
    shipping tree): disabling it is
    **2,465 / 1**, forcing its predicate to lie is **2,439 / 27** — both legs red, so it is reachable and
    covered rather than shadowed. Red-proof of the wiring: removing both `shred_alternate_streams` calls
    from `shred_dir_pinned` is **2,464 / 2**. On Linux and macOS the whole arm is `#[cfg]`'d out and
    neither number exists, which is why the platform is named beside every one of them.
  - **One lock at a time, per vault** (SEC-847 reviewer blocker A). The re-seal and the wipe are slow and
    hold no mutex, so two concurrent `lock` calls for the same vault interleaved: the second re-sealed the
    tree the first was already shredding and wrote *that* over the vault, **both returning `Ok`** over a
    vault of zero bytes. It needed no attacker — the Lock button fires un-awaited and stays mounted across
    a re-seal that is slow by design, so a double-click on a large vault did it. The registry now claims an
    in-flight slot for the blob in the same mutex acquisition that reads the session, releases it via RAII
    on every exit including a panic, and refuses a second caller with `LockFailureCode::AlreadyLocking`
    having done nothing at all; the banner's button is disabled for the duration as well.
  - **The replacement is durable before the original is destroyed** (SEC-847 reviewer blocker C). The
    staging blob is `sync_all`ed before it is verified — verifying a page-cache copy proves the bytes
    parse, not that they reached the disk; the ordering is asserted, not merely reviewed, by
    `the_staging_blob_is_fsynced_before_it_is_verified` (there is no portable way to interrogate the OS
    after the fact, so `sync_durably` counts its calls in test builds and the injected verifier reads the
    counter at the moment it runs) — and on Unix the vault's parent directory is fsynced after the
    rename, so the new directory entry is durable too. Without both, a power loss between the rename and
    the wipe could leave the vault's name pointing at unwritten data with the plaintext already securely
    shredded.
  - **…and so does `create_vault` (CPE-1669).** It had the identical gap — `std::fs::write` then a verify
    that re-read the page cache, then a shred that `sync_all`s every pass: the *destruction* was durable
    and the *replacement* was not. It now takes the same four steps through the same helpers (stage
    exclusively beside the destination → `sync_all` → verify the bytes that landed → rename → fsync the
    parent on Unix), so the fsync provably precedes the verify and therefore any shred. Pinned by
    `create_vault_fsyncs_the_blob_before_it_is_verified_and_therefore_before_any_shred` and, on Unix,
    `create_vault_fsyncs_the_destination_directory_after_the_rename`.

    **Closed on Unix, only narrowed on Windows** (SEC-861 finding 5). The larger half is closed on both:
    the blob's bytes are `sync_all`ed before the verify, so the vault's name can never point at unwritten
    *data*. The remaining half is the directory **entry** the rename creates. On Unix that is fsynced. On
    Windows `sync_parent_dir` is a no-op, and the justification it used to carry — "`rename`'s ordering is
    already provided" — is false: `MoveFileEx` without `MOVEFILE_WRITE_THROUGH` is not durable, so a power
    loss in that window can leave the vault name missing after the plaintext was shredded. Closing it
    needs `MOVEFILE_WRITE_THROUGH` via a direct `MoveFileExW`; that is a follow-up, and the shipped state
    is stated here rather than implied to be handled. Both counters are now
    **thread-local** rather than a process-wide atomic: as an `AtomicUsize` another test running in
    parallel could satisfy "the count went up" without this call site having synced at all — caught by
    neutralising the create-side write and watching its ordering test stay green, where the thread-local
    version fails.

    **What this does *not* mean — and the correction to the correction (SEC-861 blocking 3, then the
    re-review).** This paragraph has now been wrong twice in opposite directions, so here is the measured
    version. An earlier draft called #847's `the_staging_blob_is_fsynced_before_it_is_verified` "quietly
    weaker than it read" — imprecise. The first correction replaced that with "genuinely falsifiable
    against the mutation it exists to catch" — precise, and **false for the mutation that matters**.

    Both readings picked the wrong mutation. Against **removal** of `sync_durably`, the test on `main` did
    always fail: the re-seal was the only increment source, so deleting the guard deleted every increment.
    But the test is named `..._is_fsynced_before_it_is_verified`; what it pins is **ordering**. The
    reviewer rebuilt a faithful `main` — process-wide atomic, re-seal as sole increment source,
    `create_vault` back on `fs::write` — and moved the fsync to *after* the verify, ten times:

    ```
    GREEN (mutation MASKED) = 4      RED (mutation caught) = 6
    ```

    **40% masked on `main`, with no CPE-1669 in the picture at all.** A shared counter makes falsification
    depend on parallel interleaving, which is why the first attempt at this reconstruction came back green
    and the second red.

    So: on `main` the test was falsifiable against removal but only ~60% reliable against the ordering
    mutation it names. CPE-1669's second increment source did not create that weakness — it made it
    **deterministic** (`before=1`). The thread-local fixes the cross-thread half; the load-bearing
    before-snapshot at the re-seal site fixes the same-thread half. The "introduced by the fix" framing
    holds for the *deterministic* form only.

    That coupling has a second edge, same cause: `sealed_vault` (the re-seal test's own fixture) calls
    `create_vault`, which now fsyncs **on the same thread**, so the re-seal test's before-snapshot is
    *load-bearing* rather than defensive — measured as `RESEAL-TEST before=1` against `CREATE-TEST
    before=0`. Demonstrated: give the re-seal a writer that forgets to sync while `create_vault` keeps
    syncing, and the test catches it; delete the snapshot and the identical mutation passes, masked by
    the fixture's own create. The thread-local fixed the cross-thread masking; the snapshot is what stops
    the same masking reappearing same-thread. Both call sites carry a comment saying which of the two
    they are, because they differ.
  - **A symlinked `.cpevault` path is replaced, not written through — and both ends now agree
    (CPE-1670).** The lock-time re-seal finishes with `rename`, which replaces the *link itself*;
    `create_vault` used `std::fs::write`, which **follows** a symlink and updates its target. The two
    halves of one feature disagreed about what a symlinked vault path means. Settled in favour of
    **replace**, and `create_vault` now stages and renames through the same code path. Replace was chosen
    over resolving the link (which would let a re-seal be redirected into writing a vault somewhere the
    user never chose) and over refusing (which would wedge a legitimately-linked vault "unlocked" with its
    plaintext still on disk). The consequence, stated in `src/docs/20-vaults.md`: a deliberately-symlinked
    `.cpevault` stops being a link the first time it is created or locked, the file at the far end keeps
    whatever it last held, and the path the user opens holds the current contents. Nothing is destroyed —
    both files exist and both decrypt. *Reads* still follow a link (unlock reads the blob with
    `std::fs::read`); only writes replace it. Pinned end to end (seal → unlock → edit → lock → unlock,
    plus the create half) by `a_symlinked_vault_path_is_replaced_by_both_create_and_lock_never_written_through`,
    which skips loudly where the OS will not create a file symlink.

    **It also closed a sharper, unnamed instance of the same data-loss class (SEC-861 re-audit).** The
    write-through hazard was never only about symlinks: a `.cpevault` **hard-linked** to a name inside the
    folder being shredded had the same shape, and needs **no elevation and no Developer Mode on NTFS** —
    unlike the symlink form. On `main`, `fs::write` wrote the vault *through* the link into the shared
    inode and `shred_tree` then overwrote that inode, so `create_vault` returned `Ok(())` with **both**
    copies gone (measured: `plaintext_survives=false vault_somewhere=false`). Stage-beside + `rename`
    gives the vault a fresh inode, so the shred of the inside name cannot reach it (`vault_somewhere=true`
    on this branch). The replace-don't-follow decision is therefore load-bearing against the *cheaper*
    variant, not only the privileged one.
  - **Lock failures are reported by a structured code, not by matching text** (SEC-847 finding 3). The
    frontend's recovery differs completely between the failure shapes — one clears the "unlocked" banner
    and refuses a retry, the others must keep the banner and offer one — and the messages interpolate
    **full file paths**. Classifying on wording therefore let a file *inside the vault* choose its own
    name to impersonate a tamper refusal: a file called `why my landlord can no longer be trusted.txt`,
    held open by another program, turned an ordinary wipe failure into "the vault is sealed and nothing
    was deleted" with the banner cleared and the entire decrypted tree still on disk. `vault_lock` now
    returns `LockError { code, message }`, the code is decided by *which step failed*, and the four code
    strings are pinned across the language boundary by a guard test that reads `src/lib/vaultStore.ts`
    (blocker B: the reciprocal doc comments were documentation, not a guard — a reviewer changed the
    wording and all 62 Rust and 13 TS tests stayed green while every tamper refusal silently
    reclassified). The guard enumerates the variants through an **exhaustive `match`**
    (`every_lock_failure_code`), not a hand-written list: `classifyLockError` has a `default:` arm, so a
    fifth variant added later would otherwise compile, regenerate `bindings.gen.ts` cleanly, classify as
    `transient` in the UI, and leave both guards green.
- **Link debris in the sessions root (CPE-1653).** A refused lock correctly leaves the planted link alone
  (it shreds nothing at a path it has decided not to trust), so the link accumulates in the app's own
  `vault-sessions` root. The startup sweep now **unlinks** a link-shaped child — `remove_file`, else
  `remove_dir`, both of which operate on the reparse point itself — never traversing it and never touching
  its target. The `is_dir()` filter that keeps the sweep from following reparse points is unchanged; the
  link case is handled *before* it, not by loosening it.
- **Orphaned sessions on abnormal exit.** If the app is killed while a vault is unlocked, the session dir
  can linger. **CPE-1252** (filed) adds a startup sweep that securely wipes any `vault-sessions/*` not in
  the live registry (the registry is empty at boot, so all are orphans).
- **The session dir is contained, not caller-chosen (CPE-1647, closed).** `vault_unlock`'s `session_dir` is
  untrusted IPC input, and locking *securely shreds* whatever it names — so an unvalidated one was a
  "shred any directory" primitive (`vault_unlock(blob, pass, "…/Documents")` then `vault_lock(blob)`).
  `unlock_to_session` refuses any path that does not resolve **strictly inside**
  `appCacheDir()/vault-sessions` (`ensure_session_dir_contained`, the same guard shape `create_vault`'s
  `resolves_inside` uses). Both sides are canonicalized before a component-wise comparison, so `..`,
  symlinks/junctions and the `vault-sessions` / `vault-sessions-evil` prefix trap are all caught; the root
  itself is refused (wiping it would destroy every live session); and every resolution failure fails
  **closed**. A refused unlock records no mapping, so a later `lock` has nothing to act on. The guard is
  **pure** — it reads the filesystem and never creates anything, so a refusal leaves no directory behind.

  **What is guaranteed, precisely.** Checking only at unlock would contain the caller's path *string*, not
  the *directory that gets shredded* — the two are separated in time, and `deletePermanent`/`moveExact`
  plus `createJunction` (all registered commands, and a Windows junction needs neither Developer Mode nor
  elevation) can swap a link in at the validated path afterwards, with the attacker choosing when `lock`
  runs. So containment is enforced **twice**, and the guarantee is:
    1. **At unlock** — `session_dir` must resolve strictly inside the app's `vault-sessions` root, checked
       before the blob is read and before anything is decrypted.
    2. **At lock, immediately before the re-seal and the wipe** — the root is stored alongside the
       session dir in the registry and the *same* check is re-run against the path as it resolves *now*.
       A swapped-in link canonicalizes to its target, which then fails `starts_with(root)`. A failed
       re-check re-seals nothing, shreds nothing, drops the mapping (so the vault is not left wedged
       "unlocked" pointing at a path we have decided not to trust), and returns a clear error rather than
       reporting a successful lock. Since CPE-1645 it also guards the re-seal: following a planted link
       would pull the target's files INTO the user's vault, replacing its real contents.
    3. **Independently, at the wipe itself** — `wipe_session_dir` refuses outright when
       `symlink_metadata` reports the session path is a symlink or junction. A genuine session dir is a
       real directory this module extracted into and is never a link, while `exists()` and `read_dir()`
       both silently follow reparse points. Belt-and-braces: (2) and (3) fail closed independently.
- **Secure-delete of the original is best-effort.** The optional "securely delete the original after
  sealing" overwrites then removes, but on SSDs, copy-on-write, wear-levelled, or journalled filesystems
  the OS may retain remnants — the UI states this honestly. It only runs **after** the vault is verified
  decryptable (verify-before-shred), and never when the destination blob is inside the folder being
  shredded.
- **Session-dir wipe pass.** The transient session dir is wiped with a single Zero pass (it's short-lived
  extracted plaintext); the destructive original-shred defaults to a stronger scheme. Both are honest,
  documented tradeoffs.
- **Whole-vault in-memory buffering (v1).** Sealing reads every file fully into memory and packs the whole
  tree into one plaintext stream before encrypting; opening holds the whole blob and decrypted plaintext
  together. Peak RAM scales with total vault size (~2-3×), so a very large (multi-GB) vault can exhaust
  memory — a scalability limit (and minor availability consideration). Streaming-from-disk is future work.
- **Dependency weight.** `age` (with `default-features=false`) adds ~90 transitive crates. Accepted as the
  cost of not hand-rolling crypto for a security feature; vaults are an additive mode (zero cost unused).

## 6. Adversarial review record (internal)

An **independent** reviewer (not the implementer) adversarially reviewed each slice and drove fixes:

- **Crypto core (CPE-1247):** found + fixed a colon-in-filename seal/extract asymmetry (data-availability)
  and partial-extraction-without-rollback; re-probed 11 hostile paths (traversal/UNC/drive/verbatim) — all
  contained; confirmed authenticate-before-write, no plaintext leak, panic-free parsing, correct
  wrong-passphrase → `BadPassphrase` and tamper → `Corrupt` mapping.
- **Lifecycle (CPE-1248):** found + fixed (a) a dest-blob-inside-the-shredded-folder data-loss, (b) a
  verify step that wrote full plaintext to `%TEMP%` during the shred flow (now verified **in memory** via
  `verify_blob`, no disk write), (c) `lock` dropping its registry mapping before wiping (now wipe-first,
  retryable on failure), and (d) a regression where the in-memory verify skipped path sanitization —
  closed by enforcing seal⟺extract symmetry at encrypt time.
- **Mount/browse (CPE-1249):** found + fixed a re-unlock that orphaned decrypted plaintext (frontend
  already-unlocked guard + backend best-effort superseded-dir wipe) and a failed-lock that stranded the
  retry out of UI reach.
- **Session containment (CPE-1647):** found + fixed an uncontained `session_dir` on the IPC boundary — a
  caller with a vault and its passphrase could unlock into any directory and have `lock` shred it. Closed
  by canonicalizing and requiring strict containment under the app's own `vault-sessions` root (§5). Proven
  by tests that read the victim's bytes back **off disk** after both the refused unlock and the follow-up
  lock, cover the `..`/symlink/prefix-sibling/root-itself/unresolvable-root variants, and keep a negative
  control showing a legitimate fresh session still unlocks and is still wiped.
- **Session containment, review #1 (CPE-1647):** the first fix checked containment at **unlock only**, so
  what it actually contained was the path *string* at unlock time, not the directory at wipe time. An
  independent review demonstrated the gap end-to-end with no elevation and no race: unlock legitimately
  into `<vault-sessions>/<uuid>`, `deletePermanent` that directory, `createJunction` a junction at the
  same path pointing at the user's Documents, then `vault_lock` — every file under Documents was securely
  shredded. Closed by (a) storing the session root with the session and **re-running the containment check
  in `lock`, immediately before the wipe**, and (b) `wipe_session_dir` refusing a session path that is
  itself a symlink/junction. Both changes fail closed independently. Regression tests rebuild the exploit
  from real filesystem objects (junction on Windows, symlink elsewhere) and assert the victim's bytes are
  still readable off disk.

  **Each guard is independently pinned red by the suite — with the symptom differing per guard**
  (corrected under CPE-1654 §B; the earlier text claimed "it was DESTROYED" for both, which is right for
  only one of them). Remove `wipe_session_dir`'s symlink refusal and the swap tests fail on
  `assert_precious_intact` — the victim's files really are shredded, "it was DESTROYED". Remove the
  lock-time containment re-check instead and the *other* guard still saves the bytes, so the same tests
  fail one assertion later, on the **wedged-unlocked** check (`vault_manager.rs`'s "a refused lock must
  not leave the vault wedged 'unlocked'…"). The substantive claim — that neither guard is unpinned — was
  verified both times; only the stated symptom was wrong for one.
- **Lock destroyed edits made while unlocked (CPE-1645):** found by the independent UAT of CPE-1630 while
  sanity-checking the one ungated shred caller — pre-existing, and a live data-loss path on a feature
  whose purpose is protecting files. Locking never called `encrypt_tree`, so it shredded the working copy
  and left the blob at its creation-time contents, silently discarding everything the user had written
  while unlocked, against a documentation promise to "re-seal". Closed by re-sealing on lock with
  `create_vault`'s verify-before-destroy discipline (§5). Pinned by a test that performs the reporter's
  exact five-step sequence (seal → unlock → write/edit → lock → unlock again) and reads the edits back off
  disk; it fails on the unfixed code with "a file CREATED while the vault was unlocked was DESTROYED by
  locking".
- **A refused lock was reported as a busy file, and navigated into the tampered path (CPE-1654):** the
  frontend surfaced *every* failed lock as "some files may still be in use. Try again." and then navigated
  back into `sessionDir` — wrong on both counts for a containment refusal, where retrying can never help
  and the path now resolves somewhere else entirely (the user's own Documents, in the demonstrated
  exploit), while the backend had already dropped the mapping. Closed by `classifyLockError`
  (`src/lib/vaultStore.ts`), which sorts a lock failure into tamper / re-seal / transient on wording the
  backend produces deliberately (`UNTRUSTED_SESSION`, whose doc comment names the frontend function, and
  `reseal_failed`); only the tamper case clears the store entry, and only a retryable one navigates back
  into the session dir.
- **The session wipe followed a junction swapped in at a *parent* directory (CPE-1672):** found by the
  independent security auditor re-auditing PR #847, and reproduced 3/3 end-to-end through the public
  `VaultRegistry::lock`. Pre-existing on `main` — the collect-then-shred structure was not introduced by
  CPE-1645, which is why that ticket landed with this filed rather than growing a fourth round. Closed by
  pinning every destructive step to a filesystem **identity** captured before anything is destroyed, and
  by writing through one no-follow handle per file (§5). The regression tests rebuild the auditor's
  reproduction from real filesystem objects (junction on Windows, symlink elsewhere) and print his own
  probe line on red and green runs alike; on the unfixed code they report
  `swapped=true lock_ok=true victim_exists=false victim_dir_exists=true bystander_exists=true` — the
  un-named bystander surviving is what proves it was the shredder writing *through* the junctioned parent
  rather than `remove_dir_all` recursing into it.
- Verify-before-shred is enforced with an injectable verifier and a falsifiable test proving the original
  survives a failed verify; the re-seal on lock has the same seam (`reseal_session_with_verifier`) and the
  same falsifiable test, proving the old blob survives a failed verify byte-for-byte and the working copy
  is never wiped.

## 7. Recommendation

- **Before GA / real-user data:** commission a **professional external cryptography audit** of the format,
  the `age` usage, the key/keychain handling, and the mount/shred lifecycle. The internal review above is
  thorough but is performed by the same project and does not replace an independent expert audit.
- **Before then:** present vaults as internally-reviewed and appropriate for reducing casual/at-rest
  exposure, with the §5 limits stated to users (they are, in `src/docs/20-vaults.md`).
- Complete **CPE-1252** (orphan-session sweep) to close the crash-leaves-plaintext gap.
