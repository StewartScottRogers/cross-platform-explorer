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
 * lookbehind/lookahead require a non-word/non-hyphen/non-slash character (or line start/end) on both
 * sides, so this does NOT match `apt` inside a longer identifier — `apt-transport-https`, `adapter`,
 * `apt-get-wrapper` — only the bare command token.
 *
 * CPE-1916 added `/` to the excluded lookbehind: `sudo rm -f /etc/apt/sources.list.d/…` (the
 * unused-Microsoft-repo cleanup that ticket introduced) contains `apt` as a bare path SEGMENT, not a
 * command word, and an `rm` line is not an apt invocation to harden in the first place — without
 * excluding `/`, this filter mistook that path for a sixth unhardened apt-get site and false-failed
 * the regression guard.
 *
 * CPE-1969 added `/` to the excluded LOOKAHEAD as well, for the mirror-image reason. Widening the
 * "no apt invocation is left unhardened" scan from three remembered files to every workflow and every
 * extracted script (see `releaseHangHardening.test.ts`) brought `gui-smoke.yml` into range, and its
 * apt-lock wait step contains:
 *
 *     echo "waiting for background apt/dpkg lock (attempt $i/24)..."
 *
 * Here `apt` is a bare path/prose SEGMENT followed by `/`, exactly as `/etc/apt/...` is one preceded
 * by `/`, and the old lookahead accepted it — so the widened scan would have reported an `echo` as a
 * fifth unhardened apt site and false-failed on its first run. A real command word is never followed
 * by `/`: `apt-get update`, `apt install -y foo`, `apt-get -o …` all have whitespace next. Symmetry
 * with the lookbehind is the point — a slash on either side means this is a path, not a command.
 *
 * Regexes are stateless here (no `g` flag), so sharing one instance across suites is safe.
 */
export const APT_COMMAND_WORD = /(?<![\w\-/])apt(?:-get)?(?![\w\-/])/;
