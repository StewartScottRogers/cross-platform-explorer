/**
 * The apt-get hardening vocabulary the CI guards share (CPE-1787, widened by CPE-1916).
 *
 * CPE-1950 moved these here from `ciAptGetHardening.test.ts`. `releaseHangHardening.test.ts` used to
 * carry its own copies under the comment *"Verbatim from ciAptGetHardening.test.ts … reused rather
 * than re-derived"* — and that claim was **already false**: CPE-1916 added `/` to the command-word
 * lookbehind on one side only, so the two regexes had silently diverged to
 * `(?<![\w\-/])` vs `(?<![\w-])` while both suites stayed green and both comments still said
 * "verbatim". Exactly the decay the provenance rule describes, on a guard whose whole job is to stop
 * a CI job hanging for six hours on an IPv6 apt mirror.
 *
 * There is now one declaration. Neither guard re-declares these; they import them.
 *
 * **Red-proofed, not assumed.** Changing `Acquire::Retries=3` to `=4` here reds BOTH suites (6 of 6 in
 * `ciAptGetHardening.test.ts`, 5 of 26 in `releaseHangHardening.test.ts`) — which is the point: both
 * consume this one declaration, so neither can drift from it. Reverted.
 */

/** The full option string every hardened apt-get invocation in this repo carries, verbatim. */
export const HARDENING_FLAGS =
  "-o Acquire::ForceIPv4=true -o Acquire::Retries=3 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20";

/**
 * Matches `apt` or `apt-get` as an isolated COMMAND WORD — e.g. `sudo apt-get update` or
 * `sudo apt install -y foo` — not as a substring of something else. `apt` and `apt-get` are
 * functionally identical aliases a future site could plausibly be written with either way (a Reviewer
 * round on CPE-1787 proved it: injecting a brand-new unhardened `apt update` / `apt install` step
 * sailed straight through an earlier filter that only checked for the substring `"apt-get"`). The
 * lookbehind and lookahead both require a non-word/non-hyphen character (or line start/end), so this
 * does NOT match `apt` inside a longer identifier — `apt-transport-https`, `adapter`,
 * `apt-get-wrapper` — only the bare command token. The lookahead additionally excludes `/` and `.`,
 * and the lookbehind deliberately does NOT exclude `/`; the next section is entirely about why those
 * two sides are asymmetric, because a symmetric version of this regex has now been wrong twice.
 *
 * ## The trailing-separator rule, and why the leading one was wrong (CPE-1916 → CPE-1969)
 *
 * CPE-1916 excluded `/` on the LEFT (the lookbehind): `sudo rm -f /etc/apt/sources.list.d/…` (the
 * unused-Microsoft-repo cleanup that ticket introduced) contains `apt` as a bare path SEGMENT, not a
 * command word, and an `rm` line is not an apt invocation to harden in the first place — without
 * that exclusion, the filter mistook the path for a sixth unhardened apt-get site and false-failed
 * the regression guard.
 *
 * CPE-1969 round 1 excluded `/` on the RIGHT too, for a mirror-image false positive. Widening the
 * "no apt invocation is left unhardened" scan from three remembered files to every workflow and every
 * extracted script (see `releaseHangHardening.test.ts`) brought `gui-smoke.yml` into range, and its
 * apt-lock wait step contains `echo "waiting for background apt/dpkg lock (attempt $i/24)..."` — a
 * prose/path segment the scan would have reported as a fifth unhardened site on its first run.
 *
 * CPE-1969 round 2 (Reviewer N4, folded in on the Foreman's call) then found that the LEFT exclusion
 * was **over-corrected all along**, and had been since CPE-1916:
 *
 *     sudo /usr/bin/apt-get update
 *
 * is a completely real, completely unhardened apt invocation, and it matched **neither** regex,
 * because the lookbehind saw the `/` of `/usr/bin/` and refused. The same defect the round-1
 * lookahead fixed, pointing the other way — and the dangerous way, since a missed invocation is
 * silent where a false positive is a red test. So the left exclusion is **gone**, and the whole job
 * of distinguishing a path from a command now rests on what FOLLOWS:
 *
 *   * a path SEGMENT is followed by `/` or `.` — `/etc/apt/sources.list.d/…`, `apt/dpkg lock`,
 *     `/etc/apt/apt.conf.d/99custom`, `/etc/apt.conf`. Rejected.
 *   * a COMMAND WORD is followed by whitespace or end of line — `apt-get update`, `apt install -y
 *     foo`, `apt-get -o …`, and equally `sudo /usr/bin/apt-get update`. Accepted.
 *
 * `.` joined `/` in the lookahead here for exactly that reason: dropping the left exclusion made
 * `cat /etc/apt/apt.conf.d/99custom` match on `apt.conf`, which is the one unintended cell the
 * old/current/new sweep moved. It was the only one; see the Work Log for the 26-shape table.
 *
 * ## The residue, stated rather than papered over
 *
 * One shape stays genuinely undecidable from the token alone: a path TAIL. `/usr/bin/apt` (a command)
 * and `/etc/apt`, `/var/cache/apt` (directories) are the same string shape — `/`, then `apt`, then
 * whitespace — and nothing in the line distinguishes them. This regex resolves that ambiguity
 * **towards matching**, deliberately, because the two errors are not equal: a false positive is a red
 * test naming the exact offending line and costs one reviewed exclusion, while a false negative is a
 * CI job hanging for six hours on an IPv6 apt mirror with every guard green — which is the failure
 * this whole vocabulary exists to prevent, and which is what shipped between CPE-1916 and now.
 *
 * Measured 2026-08-27: no line in the repo (8 workflows, 3 extracted scripts, all `.sh`/`.yml`/`.mjs`
 * — 52 matching lines) hits that ambiguous shape, so this costs nothing today. **If one ever does,
 * exclude that line at the CALL SITE with a named reason. Do not put `/` back in the lookbehind** —
 * that is precisely the widening that created the hole this comment is about, and it would take all
 * five absolute-path invocation forms back down with it.
 *
 * Regexes are stateless here (no `g` flag), so sharing one instance across suites is safe.
 */
export const APT_COMMAND_WORD = /(?<![\w-])apt(?:-get)?(?![\w\-/.])/;
