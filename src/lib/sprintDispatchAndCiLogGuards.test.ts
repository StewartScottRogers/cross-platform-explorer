// CPE-1848 + CPE-1868: two harness-level defects, both fixed by hardening the PROSE the Foreman reads
// from `.claude/commands/sprint.md` / `sprint-batched.md` — there is no application code for either
// ticket to exercise. Without a guard, that prose can silently rot: someone tightens a sentence, drops
// a clause during a rewrite, and the exact failure these tickets closed is free to recur with nothing
// red to say so ("a guard that stays green when the thing it guards is broken is worse than no guard" —
// this repo's own most-repeated finding). These tests read the REAL skill files (same pattern as
// `epicsQueueLayout.test.ts` reading real ticket files) and assert the load-bearing phrases survive.
//
// CPE-1848: a dispatched sub-agent (Worker/Reviewer/UAT) never receives a background task notification —
// observed three times in one batch, each stalling forever on a stub. Every dispatch prompt must state
// that plainly, with the bounded-poll idiom inline, and the reporting contract must recognise a stub
// report ("a monitor is armed") as a stall rather than progress.
//
// CPE-1868: `gh run view --log` truncates around ~4 MB with no marker, and a worker drew the wrong
// conclusion from the cut (CPE-1859). A related family — an empty check board reading as green
// (CPE-1846) and a pending count that dips before it rises (CPE-1863) — costs the same kind of wrong
// conclusion from a poll instead of a log. sprint.md must carry the fetch idiom and the poll idiom that
// cannot silently return a partial truth.
//
// CPE-1856: worktrees isolate the filesystem, not the MACHINE. A worker on CPE-1842 installed and then
// correctly uninstalled PowerShell 7 into the shared `~/.dotnet/tools`; a sibling worker on CPE-1841 was
// running its suite against that same shim, went red the moment it was removed, then went silently green
// again on a DIFFERENT host (Windows PowerShell 5.1) because the harness's tool probe fell back without
// announcing it -- two hours were lost to wrong diagnoses before a worker reconstructed the real cause
// from directory mtimes. sprint.md must state that a machine-global tool install/uninstall/upgrade is a
// shared-resource change (removal is the harmful half), that a measurement claim records its host/tool
// version, that a harness tool probe must announce what it resolved to, and must enumerate the other
// machine-global state (env vars, global git config, ports, the cargo cache, %TEMP%) beyond tool installs.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const SPRINT_MD = readFileSync(join(process.cwd(), ".claude", "commands", "sprint.md"), "utf8");
const SPRINT_BATCHED_MD = readFileSync(join(process.cwd(), ".claude", "commands", "sprint-batched.md"), "utf8");

/**
 * The dispatch contract subsection only, so the assertions below can be NEGATIVE without colliding with
 * the CI-poll traps section further down the file, which still legitimately names `gh pr checks --watch`
 * (as a trap the Foreman must know about, not as an idiom a sub-agent may use).
 */
const DISPATCH_CONTRACT = (() => {
  const start = SPRINT_MD.indexOf("### Dispatch contract");
  if (start < 0) throw new Error("sprint.md no longer has a '### Dispatch contract' section");
  const next = SPRINT_MD.indexOf("\n### ", start + 1);
  return SPRINT_MD.slice(start, next < 0 ? undefined : next);
})();

describe("sprint.md dispatch contract states the no-background-notifications rule (CPE-1848)", () => {
  it("tells a dispatched sub-agent it receives no background notifications", () => {
    expect(SPRINT_MD).toMatch(/receive[s]? NO background task notifications/i);
  });

  // CPE-1880 REPLACES the assertion that used to live here. CPE-1848's contract prescribed
  // `gh run watch <run-id> --interval 30` as "the bounded-poll idiom" and this guard PINNED it — but the
  // command is not bounded at all: it blocks until CI finishes, and of the 71 `ci.yml` runs that
  // succeeded between 2026-08-23 and 2026-08-26, ZERO finished inside the harness's 600 s Bash-tool cap
  // (median 58.9 min). Every such call is auto-backgrounded, which parks the sub-agent on a notification
  // it cannot receive. So the guard was faithfully protecting the defect. It now asserts the opposite:
  // the contract must NOT hand a sub-agent an unbounded watch, and must name the bounded replacement.
  it("does NOT prescribe an unbounded CI watch to sub-agents — that command is what caused the stalls", () => {
    expect(DISPATCH_CONTRACT).not.toMatch(/To watch CI:\s*`gh run watch/);
    expect(DISPATCH_CONTRACT).toMatch(/Never run `gh run watch` or `gh pr checks --watch`/);
  });

  it("names the 600s auto-background cause, not just the symptom", () => {
    expect(DISPATCH_CONTRACT).toMatch(/600\s*s tool\s*[\r\n]?\s*>?\s*cap is auto-backgrounded/i);
    expect(DISPATCH_CONTRACT).toMatch(/auto-backgrounded, not killed/i);
  });

  it("carries the measurement that makes the rule non-negotiable (0 of 71 runs inside the cap)", () => {
    expect(DISPATCH_CONTRACT).toMatch(/71 runs that\s*[\r\n]?\s*succeeded, ZERO finished inside 600 s/i);
  });

  it("warns off the `timeout 570 gh run watch` mitigation that was measured and did not hold", () => {
    expect(DISPATCH_CONTRACT).toMatch(/timeout 570 gh run watch/);
    expect(DISPATCH_CONTRACT).toMatch(/it is the fix everyone tries and it did not hold/i);
  });

  it("makes the Foreman, not the worker, the owner of every CI wait", () => {
    expect(DISPATCH_CONTRACT).toMatch(/\*\*The Foreman owns CI\. Do not watch, poll, or monitor it\.\*\*/);
    expect(DISPATCH_CONTRACT).toMatch(/Your report is complete\s*[\r\n]?\s*> without a CI verdict/i);
  });

  it("gives a bounded, always-returning idiom in place of the banned one", () => {
    expect(DISPATCH_CONTRACT).toContain("node scripts/ci-poll.mjs --run <run-id>");
    expect(DISPATCH_CONTRACT).toMatch(/clamped below the cap by\s*[\r\n]?\s*> construction/i);
  });

  it("says what to do on a genuinely long wait: return with an explicit pending-SHA note", () => {
    expect(DISPATCH_CONTRACT).toContain("CI still pending on <SHA>");
  });

  it("explicitly bans returning a stub / an armed monitor", () => {
    expect(SPRINT_MD).toMatch(/never.{0,20}return a stub/i);
  });

  it("binds the Foreman to the other half of the bargain: it must actually pick CI up", () => {
    expect(DISPATCH_CONTRACT).toMatch(/The Foreman's own side of the bargain/i);
    expect(DISPATCH_CONTRACT).toContain("node scripts/ci-poll.mjs --pr <n>");
  });

  it("bounds the re-invoke loop at one retry, then take-over (CPE-1880 AC 4)", () => {
    expect(DISPATCH_CONTRACT).toMatch(/escalation is bounded at one retry/i);
    expect(DISPATCH_CONTRACT).toMatch(/kill it and take over its PR yourself/i);
    expect(DISPATCH_CONTRACT).toMatch(/Do not re-invoke a\s*[\r\n]?\s*third time/i);
  });

  it("mandates the mechanical arrival check rather than the Foreman's eye", () => {
    expect(DISPATCH_CONTRACT).toContain("node scripts/stall-check.mjs");
    expect(SPRINT_MD).toMatch(/Do not eyeball this — run\s*[\r\n]?\s*it:/i);
  });

  it("preserves the re-check-by-SHA idiom, not PR number alone (the gh pr checks --watch trap)", () => {
    expect(SPRINT_MD).toMatch(/by \*\*SHA\*\*/);
    expect(SPRINT_MD).toContain("gh pr checks --watch");
    expect(SPRINT_MD).toMatch(/exits 0 when the branch moves under it/);
  });

  it("makes a stub report recognisable as a stall in the reporting contract, not a status update", () => {
    expect(SPRINT_MD).toMatch(/report naming a pending background task instead of results is incomplete/i);
    expect(SPRINT_MD).toMatch(/a monitor is armed/i);
  });

  it("carries the dispatch contract into the batched-run skill too (CPE-1848 AC: check sprint-batched)", () => {
    expect(SPRINT_BATCHED_MD).toMatch(/dispatch contract.*CPE-1848/i);
    expect(SPRINT_BATCHED_MD).toMatch(/no background notifications/i);
  });

  it("carries the CPE-1880 correction into the batched-run skill too — it is the highest-risk case", () => {
    expect(SPRINT_BATCHED_MD).toMatch(/the Foreman owns CI — do not watch, poll, or monitor it/i);
    expect(SPRINT_BATCHED_MD).toContain("node scripts/stall-check.mjs");
    expect(SPRINT_BATCHED_MD).toMatch(/kill-and-take-over/i);
  });
});

describe("sprint.md carries a fetch idiom that cannot silently return a log prefix (CPE-1868)", () => {
  it("names the truncation cause: gh run view --log cuts off around ~4 MB", () => {
    // `.` does not span the line-wraps a prose paragraph carries, so match tolerantly across them
    // rather than asserting exact adjacency the markdown source-wraps away.
    expect(SPRINT_MD).toMatch(/gh run view --job[\s\S]{0,20}--log/);
    expect(SPRINT_MD).toMatch(/truncates around[\s\S]{0,10}~4\s*MB/i);
  });

  it("gives the untruncated fetch idiom (the raw gh api logs endpoint)", () => {
    // The file is CRLF (this repo's checked-in line ending), so tolerate any single line-wrap between
    // the two halves rather than asserting a literal "\n".
    expect(SPRINT_MD).toMatch(/gh api[\s\S]{0,10}repos\/:owner\/:repo\/actions\/jobs\/<job-id>\/logs/);
  });

  it("requires stating the total line count and that the fetch reached the end", () => {
    expect(SPRINT_MD).toMatch(/total line count/i);
    expect(SPRINT_MD).toMatch(/wc -l job\.log/);
  });
});

describe("sprint.md carries a CI poll idiom that can't mistake an empty or moving board for green (CPE-1868)", () => {
  it("requires reading total_count and mergeable, never pending alone", () => {
    expect(SPRINT_MD).toMatch(/total_count.*and.*mergeable.*together with the pending count/i);
  });

  it("names the empty-board trap (CONFLICTING PRs scheduling zero checks)", () => {
    expect(SPRINT_MD).toMatch(/total_count == 0/);
    expect(SPRINT_MD).toMatch(/CONFLICTING/);
  });

  it("names the pending-count-dips trap and requires total_count to be stable across reads", () => {
    expect(SPRINT_MD).toMatch(/pending == 0.*only means "done" once.*total_count.*has stopped moving/i);
    expect(SPRINT_MD).toMatch(/stable across[\s\S]{0,10}at least two reads/i);
  });

  it("covers the gh api pagination neighbour named in the ticket", () => {
    expect(SPRINT_MD).toMatch(/--paginate/);
  });
});

describe("sprint.md treats machine-global tool installs as a shared-resource change (CPE-1856)", () => {
  it("names the incident: PowerShell 7 removed from under a sibling agent's green suite", () => {
    expect(SPRINT_MD).toMatch(/dotnet tool install --tool-path/);
    expect(SPRINT_MD).toMatch(/~\/\.dotnet\/tools/);
    expect(SPRINT_MD).toMatch(/22:14:04/);
  });

  it("states that a machine-global install affects every other agent, not just the one making it", () => {
    expect(SPRINT_MD).toMatch(/machine-global/);
    expect(SPRINT_MD).toMatch(/affects \*\*every other agent running/i);
  });

  it("names removal, not install, as the harmful half, and requires leaving a shared install in place with a note", () => {
    expect(SPRINT_MD).toMatch(/removal, not install, is the harmful half/i);
    expect(SPRINT_MD).toMatch(/leave it installed and say so in your Work\s*\n?\s*> Log/i);
  });

  it("requires a provenance note per measurement claim: which host/tool version, and how determined", () => {
    expect(SPRINT_MD).toMatch(/measurement or benchmark claim records which host\/tool version/i);
    expect(SPRINT_MD).toMatch(/provenance note per claim/i);
  });

  it("requires a harness tool probe to pin once and announce the resolved host/version, never fall through silently", () => {
    expect(SPRINT_MD).toMatch(/Harness tool probes must not fall through silently/i);
    expect(SPRINT_MD).toMatch(/findPowerShellHost\(\)/);
    expect(SPRINT_MD).toMatch(/announce the resolved host and version/i);
    expect(SPRINT_MD).toMatch(/must fail loudly/i);
  });

  it("sweeps beyond tool installs: env vars, global git config, ports, the cargo cache, %TEMP%", () => {
    expect(SPRINT_MD).toMatch(/global git config/i);
    expect(SPRINT_MD).toMatch(/listening\s*\nports|listening ports/i);
    expect(SPRINT_MD).toMatch(/cargo registry\/build cache/i);
    expect(SPRINT_MD).toMatch(/%TEMP%/);
  });
});
