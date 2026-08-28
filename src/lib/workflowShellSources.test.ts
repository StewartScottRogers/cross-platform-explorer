// CPE-1969 — the red-proofs for the enumeration itself.
//
// A derived list is only worth more than a hard-coded one if it (a) actually finds the files the
// hard-coded list was missing and (b) REFUSES rather than reporting clean when it finds nothing.
// CLAUDE.md singles out (b) as the half that gets left off, so it is tested here against a real
// fixture directory rather than argued for in a comment.
//
// Every fixture is built under `.claude/tmp/` (this project's standing rule: scratch lives inside the
// working tree, never the system temp dir) and removed afterwards.
import { describe, it, expect, afterEach } from "vitest";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  MIN_EXPECTED_WORKFLOWS,
  MIN_EXPECTED_WORKFLOW_SCRIPTS,
  WORKFLOWS_DIR,
  WORKFLOW_SCRIPTS_DIR,
  allShellUnits,
  discoverWorkflowScripts,
  discoverWorkflows,
  scriptUnit,
  workflowStepUnits,
} from "./workflowShellSources";

const ROOT = process.cwd();

const scratch: string[] = [];
afterEach(() => {
  while (scratch.length > 0) rmSync(scratch.pop()!, { recursive: true, force: true });
});

/** A throwaway repo-shaped root: `<root>/.github/workflows[/scripts]`, populated by the caller. */
function fixtureRoot(files: Record<string, string>): string {
  const base = join(ROOT, ".claude", "tmp");
  mkdirSync(base, { recursive: true });
  const root = mkdtempSync(join(base, "cpe1969-"));
  scratch.push(root);
  mkdirSync(join(root, WORKFLOW_SCRIPTS_DIR), { recursive: true });
  for (const [rel, body] of Object.entries(files)) writeFileSync(join(root, rel), body, "utf8");
  return root;
}

/** A minimal but real workflow document the in-repo YAML parser accepts. */
function workflow(runScript: string): string {
  return [
    "name: demo",
    "jobs:",
    "  demo:",
    "    steps:",
    "      - name: do the thing",
    "        run: |",
    ...runScript.split("\n").map((l) => `          ${l}`),
    "",
  ].join("\n");
}

describe("the workflow/script enumeration is derived, not recalled (CPE-1969)", () => {
  it("finds every workflow in the real repo, including the three the old five-file list missed", () => {
    const found = discoverWorkflows(ROOT);
    // The hard-coded list `lockfileLockedGuard.test.ts` carried before this ticket.
    const oldHardCodedList = [
      ".github/workflows/ci.yml",
      ".github/workflows/release.yml",
      ".github/workflows/release-sidecar.yml",
      ".github/workflows/gui-smoke.yml",
      ".github/workflows/model-snapshot.yml",
    ];
    for (const f of oldHardCodedList) expect(found).toContain(f);
    // The three it never looked at. Named explicitly so this test reds if one is renamed, rather
    // than quietly measuring a smaller repo.
    expect(found).toContain(".github/workflows/catalog-freshness.yml");
    expect(found).toContain(".github/workflows/ffmpeg-pin-freshness.yml");
    expect(found).toContain(".github/workflows/release-pipeline-watchdog.yml");
    expect(found.length).toBeGreaterThanOrEqual(MIN_EXPECTED_WORKFLOWS);
  });

  it("finds the three extracted shell scripts no consumer used to read", () => {
    expect(discoverWorkflowScripts(ROOT)).toEqual([
      ".github/workflows/scripts/catalog-freshness-check.sh",
      ".github/workflows/scripts/catalog-version.sh",
      ".github/workflows/scripts/ffmpeg-anchor-check.sh",
    ]);
  });

  it("does not mistake the scripts/ subdirectory for a workflow", () => {
    expect(discoverWorkflows(ROOT).some((f) => f.includes("/scripts/"))).toBe(false);
  });

  it("picks up a SIXTH workflow the moment one appears — the whole point of deriving the list", () => {
    const files: Record<string, string> = {};
    for (let i = 0; i < MIN_EXPECTED_WORKFLOWS; i += 1) {
      files[`${WORKFLOWS_DIR}/w${i}.yml`] = workflow("echo hi");
    }
    files[`${WORKFLOWS_DIR}/newcomer.yml`] = workflow("cargo build --release");
    const root = fixtureRoot(files);
    const found = discoverWorkflows(root);
    expect(found).toContain(`${WORKFLOWS_DIR}/newcomer.yml`);
    expect(workflowStepUnits(`${WORKFLOWS_DIR}/newcomer.yml`, root).map((u) => u.run.trim())).toEqual([
      "cargo build --release",
    ]);
  });

  it("a .yaml-spelled workflow is enumerated too — the extension is derived, not assumed", () => {
    const files: Record<string, string> = {};
    for (let i = 0; i < MIN_EXPECTED_WORKFLOWS - 1; i += 1) {
      files[`${WORKFLOWS_DIR}/w${i}.yml`] = workflow("echo hi");
    }
    files[`${WORKFLOWS_DIR}/spelled-out.yaml`] = workflow("echo hi");
    expect(discoverWorkflows(fixtureRoot(files))).toContain(`${WORKFLOWS_DIR}/spelled-out.yaml`);
  });
});

// The half CLAUDE.md says gets left off. Each of these is a real call against a real directory, not
// an assertion about what the code "would" do.
describe("the near-empty refusal fires rather than reporting clean over nothing (CPE-1969)", () => {
  it("an EMPTY workflows directory throws instead of enumerating zero workflows", () => {
    const root = fixtureRoot({});
    expect(() => discoverWorkflows(root)).toThrow(/near-empty/);
  });

  it("a MISSING .github tree throws too — a wrong working directory is the likeliest cause", () => {
    const root = fixtureRoot({});
    rmSync(join(root, ".github"), { recursive: true, force: true });
    expect(() => discoverWorkflows(root)).toThrow(/near-empty/);
    expect(() => discoverWorkflowScripts(root)).toThrow(/near-empty/);
  });

  it("a PARTIAL enumeration throws too — one survivor must not read as a working scan", () => {
    // The subtler failure: discovery still runs, but stops classifying most of what it finds. A
    // `> 0` check passes that; a floor does not.
    const root = fixtureRoot({ [`${WORKFLOW_SCRIPTS_DIR}/only-one.sh`]: "#!/usr/bin/env bash\necho hi\n" });
    expect(() => discoverWorkflowScripts(root)).toThrow(/near-empty/);
    expect(() => discoverWorkflowScripts(root)).toThrow(
      new RegExp(`floor is ${MIN_EXPECTED_WORKFLOW_SCRIPTS}`),
    );
  });

  it("an empty scripts directory throws instead of reporting zero scripts scanned", () => {
    expect(() => discoverWorkflowScripts(fixtureRoot({}))).toThrow(/near-empty/);
  });

  it("the refusal names what it found, so the reader can tell 'broken' from 'genuinely retired'", () => {
    const root = fixtureRoot({ [`${WORKFLOWS_DIR}/solo.yml`]: workflow("echo hi") });
    expect(() => discoverWorkflows(root)).toThrow(/solo\.yml/);
  });
});

describe("a file nobody classified is a file nobody scans — so it fails loudly (CPE-1969)", () => {
  it("a non-shell, non-doc file in scripts/ is refused rather than silently skipped", () => {
    const files: Record<string, string> = {};
    for (const n of ["a", "b", "c"]) files[`${WORKFLOW_SCRIPTS_DIR}/${n}.sh`] = "echo hi\n";
    files[`${WORKFLOW_SCRIPTS_DIR}/helper.py`] = "print('hi')\n";
    expect(() => discoverWorkflowScripts(fixtureRoot(files))).toThrow(/helper\.py/);
  });

  it("a README alongside the scripts is documentation, not an unclassified file", () => {
    const files: Record<string, string> = {};
    for (const n of ["a", "b", "c"]) files[`${WORKFLOW_SCRIPTS_DIR}/${n}.sh`] = "echo hi\n";
    files[`${WORKFLOW_SCRIPTS_DIR}/README.md`] = "# notes\n";
    expect(discoverWorkflowScripts(fixtureRoot(files))).toHaveLength(3);
  });

  it("an extensionless script with a bash shebang is recognised as shell", () => {
    const files: Record<string, string> = {};
    for (const n of ["a", "b", "c"]) files[`${WORKFLOW_SCRIPTS_DIR}/${n}.sh`] = "echo hi\n";
    files[`${WORKFLOW_SCRIPTS_DIR}/bare`] = "#!/usr/bin/env bash\necho hi\n";
    expect(discoverWorkflowScripts(fixtureRoot(files))).toContain(`${WORKFLOW_SCRIPTS_DIR}/bare`);
  });
});

describe("one .sh file maps to exactly one unit (CPE-1969)", () => {
  it("a script is one unit carrying the WHOLE file, so heredoc state cannot be cut mid-flight", () => {
    // The shape that makes finer units wrong: a heredoc that spans what would be two "blocks".
    const body = ["#!/usr/bin/env bash", "f() {", "  cat <<'EOF'", "  cargo build", "EOF", "}", "cargo test --locked"].join(
      "\n",
    );
    const root = fixtureRoot({ [`${WORKFLOW_SCRIPTS_DIR}/one.sh`]: body });
    const unit = scriptUnit(`${WORKFLOW_SCRIPTS_DIR}/one.sh`, root);
    expect(unit.kind).toBe("script");
    expect(unit.job).toBeUndefined();
    expect(unit.step).toBeUndefined();
    expect(unit.where).toBe(`${WORKFLOW_SCRIPTS_DIR}/one.sh (whole script)`);
    expect(unit.run).toBe(body);
  });

  it("the real repo's units cover every workflow step AND all three scripts", () => {
    const units = allShellUnits(ROOT);
    const scripts = units.filter((u) => u.kind === "script");
    expect(scripts.map((u) => u.file)).toEqual(discoverWorkflowScripts(ROOT));
    // Every workflow that has any `run:` step at all must contribute step units.
    const filesWithSteps = new Set(units.filter((u) => u.kind === "step").map((u) => u.file));
    for (const f of discoverWorkflows(ROOT)) expect(filesWithSteps.has(f)).toBe(true);
  });

  it("a step unit carries its job and step names for reporting", () => {
    const root = fixtureRoot({ [`${WORKFLOWS_DIR}/one.yml`]: workflow("echo hi") });
    const [unit] = workflowStepUnits(`${WORKFLOWS_DIR}/one.yml`, root);
    expect(unit.kind).toBe("step");
    expect(unit.job).toBe("demo");
    expect(unit.step).toBe("do the thing");
    expect(unit.where).toBe(`${WORKFLOWS_DIR}/one.yml [demo / do the thing]`);
  });
});

// A guard whose fixture directory does not resolve is a guard that silently tests nothing.
it("the fixture root helper really builds a repo-shaped tree inside the working tree", () => {
  const root = fixtureRoot({});
  expect(resolve(root).startsWith(resolve(ROOT, ".claude"))).toBe(true);
});
