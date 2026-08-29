// CPE-1900 — the derivation itself, exercised directly.
//
// `sidecarBundleResources.test.ts` asserts things about the CHAIN this module produces. This file
// asserts things about the PRODUCER, because the two fail in different ways: a wrong chain reds over
// there loudly, but a producer that quietly returns fewer overlays — or the same overlays in a sorted
// order — produces a chain that looks entirely reasonable and describes a build nobody runs.
import { describe, it, expect } from "vitest";
import { parseYaml } from "./preview/yaml";
import {
  BASE_CONFIG,
  MIN_EXPECTED_BUILD_LEGS,
  MIN_EXPECTED_BUILD_WORKFLOWS,
  TAURI_PROJECT_DIR,
  configOverlaysFromArgs,
  deriveBuildLegs,
  legsFromWorkflowDoc,
  matrixLegs,
  resolveMatrixRefs,
  shipOSForRunner,
} from "./tauriConfigChain";

const W = "test";

function parse(yaml: string): unknown {
  const result = parseYaml(yaml);
  if (!result.ok) throw new Error(`fixture did not parse: ${result.error}`);
  return result.value;
}

describe("configOverlaysFromArgs — the --config chain out of one args string", () => {
  it("returns the overlays in the order given, ignoring every other flag", () => {
    expect(
      configOverlaysFromArgs(
        "--features sidecar-platform --config src-tauri/a.json --target x --config src-tauri/b.json",
        W,
      ),
    ).toEqual(["src-tauri/a.json", "src-tauri/b.json"]);
  });

  // The one property a membership check cannot express. If the extractor ever sorts, dedupes or
  // reverses, this is the test that says so — the set is identical in every one of those cases.
  it("carries order rather than sorting it (a reversed input reverses the output)", () => {
    const forward = "--config z.json --config a.json --config m.json";
    const backward = "--config m.json --config a.json --config z.json";
    expect(configOverlaysFromArgs(forward, W)).toEqual(["z.json", "a.json", "m.json"]);
    expect(configOverlaysFromArgs(backward, W)).toEqual(["m.json", "a.json", "z.json"]);
  });

  it("does not dedupe (the same file twice is a real, order-dependent chain)", () => {
    expect(configOverlaysFromArgs("--config a.json --config b.json --config a.json", W)).toEqual([
      "a.json",
      "b.json",
      "a.json",
    ]);
  });

  it("understands every spelling the Tauri CLI accepts, long and short, attached and separate", () => {
    expect(configOverlaysFromArgs("--config=a.json -c b.json -c=c.json --config d.json", W)).toEqual(
      ["a.json", "b.json", "c.json", "d.json"],
    );
  });

  it("reads no overlays out of a string that has none", () => {
    expect(configOverlaysFromArgs("", W)).toEqual([]);
    expect(configOverlaysFromArgs("--target universal-apple-darwin", W)).toEqual([]);
  });

  // A `--configure`/`--config-dir` style flag must not be read as `--config`, and `-config` is not
  // the short form of anything.
  it("does not mistake a longer flag that starts with the same letters", () => {
    expect(configOverlaysFromArgs("--configuration Release --config-dir x", W)).toEqual([]);
  });

  it("refuses rather than guesses: a dangling flag, an empty value, any quoting", () => {
    expect(() => configOverlaysFromArgs("--config", W)).toThrow(/not followed by a config path/);
    expect(() => configOverlaysFromArgs("--config --target x", W)).toThrow(
      /not followed by a config path/,
    );
    expect(() => configOverlaysFromArgs("--config=", W)).toThrow(/empty --config value/);
    expect(() => configOverlaysFromArgs(`--config "a b.json"`, W)).toThrow(/quote character/);
    expect(() => configOverlaysFromArgs(`--config 'a.json'`, W)).toThrow(/quote character/);
  });
});

describe("resolveMatrixRefs — matrix substitution, or a loud refusal", () => {
  it("substitutes every matrix reference, whitespace inside the braces and all", () => {
    expect(resolveMatrixRefs("a.${{ matrix.o }}.json ${{matrix.p}}", { o: "unix", p: "x" }, W)).toBe(
      "a.unix.json x",
    );
  });

  it("throws on a reference the leg does not define", () => {
    expect(() => resolveMatrixRefs("${{ matrix.missing }}", { o: "unix" }, W)).toThrow(
      /does not define/,
    );
  });

  // `${{ needs.* }}`, `${{ env.* }}`, `${{ inputs.* }}` — none of them resolvable from the workflow
  // file alone. A chain derived from a half-substituted string describes a build nobody runs.
  it("throws on any OTHER expression left behind, rather than deriving from a half-resolved string", () => {
    expect(() => resolveMatrixRefs("--config ${{ env.OVERLAY }}", {}, W)).toThrow(/unresolved/);
    expect(() => resolveMatrixRefs("${{ matrix.a }} ${{ inputs.b }}", { a: "1" }, W)).toThrow(
      /unresolved/,
    );
  });
});

describe("matrixLegs — matrix expansion, or a loud refusal", () => {
  it("expands an include-style matrix, and treats a job with no matrix as one leg", () => {
    expect(
      matrixLegs({ strategy: { matrix: { include: [{ platform: "a" }, { platform: "b" }] } } }, W),
    ).toEqual([{ platform: "a" }, { platform: "b" }]);
    expect(matrixLegs({}, W)).toEqual([{}]);
  });

  // Half-expanding a matrix guards fewer builds than ship while looking like it worked — the exact
  // failure mode this module exists to remove, so it is a throw and not a best effort.
  it("throws on a product axis or an exclude rather than expanding part of it", () => {
    expect(() => matrixLegs({ strategy: { matrix: { os: ["a", "b"] } } }, W)).toThrow(/besides/);
    expect(() =>
      matrixLegs({ strategy: { matrix: { include: [{ a: "1" }], exclude: [{ a: "1" }] } } }, W),
    ).toThrow(/besides/);
  });

  it("throws on an empty or non-mapping include", () => {
    expect(() => matrixLegs({ strategy: { matrix: { include: [] } } }, W)).toThrow(/non-empty/);
    expect(() => matrixLegs({ strategy: { matrix: { include: ["oops"] } } }, W)).toThrow(
      /not a mapping/,
    );
  });
});

describe("shipOSForRunner — runner label to shipped OS", () => {
  it("classifies the labels in use and the ones a version bump would produce", () => {
    for (const [label, os] of [
      ["windows-latest", "windows"],
      ["windows-2022", "windows"],
      ["ubuntu-latest", "linux"],
      ["ubuntu-22.04", "linux"],
      ["self-hosted-linux-arm64", "linux"],
      ["macos-latest", "macos"],
      ["macos-15", "macos"],
    ] as const) {
      expect(shipOSForRunner(label, W), label).toBe(os);
    }
  });

  // Dropping an unclassifiable leg would narrow the guard silently, which is this ticket's own defect.
  it("throws on a label it cannot classify rather than dropping the build leg", () => {
    expect(() => shipOSForRunner("some-new-runner", W)).toThrow(/does not name a recognised OS/);
  });
});

/**
 * CPE-1933 rule 2 — *anchor on code, never on prose* — pinned rather than asserted.
 *
 * `release-sidecar.yml` is more comment than YAML, and several of its comments discuss `--config`
 * overlays by name. A line-scanning extractor would read them as live arguments. This one cannot,
 * because it never sees a line: `parseYaml` has already discarded every comment, and only the VALUES
 * of `with.args` and `runs-on` are read.
 *
 * **Red-proofed, and the scanners are COMMITTED rather than described.** The first version of this
 * block stated "4 of 4 / 2 of 4 / 0 of 4" in prose, with the scanners that produced those numbers
 * living only in a scratch file that was deleted. That is CLAUDE.md's *"if you cannot commit the
 * generator, you have not measured anything a reviewer can check"* at small scale — the numbers were
 * reproducible, but nothing in the tree let anyone check them, and nothing would notice if the fixture
 * later drifted so that they stopped holding. {@link NAIVE_SCANNERS} and the test below now compute
 * all three counts on every run.
 *
 * That drift is the failure it now catches, red-proofed (2026-08-28): deleting the TRAILING decoy
 * from the fixture — the single most plausible tidy-up, since it makes a long line shorter — takes
 * `tauriConfigChain.test.ts` to 1 failed / 23 passed, naming the decoys the bare scanner still found.
 * Without this test that edit was silent, and it would have quietly turned the assertion above into a
 * tautology about a fixture with nothing left to defeat. Reverted.
 */
describe("the derivation is comment-blind and script-blind by construction (CPE-1933 rule 2)", () => {
  const FIXTURE = `
name: decoys
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      # --config src-tauri/tauri.sidecar.decoy-whole-line.conf.json
      - name: a run block that mentions the flag
        run: |
          echo "we used to pass --config src-tauri/tauri.sidecar.decoy-heredoc.conf.json here"
      - name: Build
        uses: tauri-apps/tauri-action@v0
        with:
          # --config src-tauri/tauri.sidecar.decoy-inside-with.conf.json
          args: --config src-tauri/tauri.sidecar.real.conf.json # --config src-tauri/tauri.sidecar.decoy-trailing.conf.json
`;

  /** The one real overlay in {@link FIXTURE}; everything else matching `--config` is a decoy. */
  const REAL = "src-tauri/tauri.sidecar.real.conf.json";

  /**
   * The two line scanners this repo keeps having to delete, kept here so their numbers are computed
   * rather than quoted. Neither is used by anything — they exist to be WRONG, on the record.
   */
  const NAIVE_SCANNERS: { name: string; scan: (yaml: string) => string[] }[] = [
    {
      name: "a bare /--config\\s+(\\S+)/ per line",
      scan: (yaml) =>
        yaml
          .split("\n")
          .flatMap((line) => [...line.matchAll(/--config\s+(\S+)/g)].map((m) => m[1])),
    },
    {
      name: "the same scan with a WHOLE-LINE comment filter",
      scan: (yaml) =>
        yaml
          .split("\n")
          .filter((line) => !line.trim().startsWith("#"))
          .flatMap((line) => [...line.matchAll(/--config\s+(\S+)/g)].map((m) => m[1])),
    },
  ];

  it("reads only the args VALUE — not a whole-line comment, a trailing comment, or a run: body", () => {
    const legs = legsFromWorkflowDoc("fixture.yml", parse(FIXTURE));
    expect(legs).toHaveLength(1);
    expect(legs[0].overlays).toEqual([REAL]);
    expect(legs[0].os).toBe("linux");
  });

  // The fixture is only worth anything if it really does contain four things a line scanner falls for.
  // Asserting that here means a future edit that softens the fixture (drops a decoy, moves the
  // trailing comment) reds, instead of quietly turning the test above into a tautology.
  it("the fixture really does defeat line scanners: 4 of 4, then 2 of 4, against 0 of 4", () => {
    const decoysFound = (paths: string[]) => paths.filter((p) => p !== REAL);

    const [bare, wholeLineFiltered] = NAIVE_SCANNERS.map((s) => decoysFound(s.scan(FIXTURE)));

    expect(bare, `${NAIVE_SCANNERS[0].name}: found ${bare.join(", ")}`).toHaveLength(4);

    // CLAUDE.md, CPE-1933 rule 2: "a whole-line-comment filter is NOT enough — a trailing comment
    // walks straight through it". These are the two survivors, named rather than counted, so the
    // assertion says WHICH shapes beat the filter.
    expect(wholeLineFiltered.sort()).toEqual([
      "src-tauri/tauri.sidecar.decoy-heredoc.conf.json",
      "src-tauri/tauri.sidecar.decoy-trailing.conf.json",
    ]);

    const structural = legsFromWorkflowDoc("fixture.yml", parse(FIXTURE)).flatMap((l) => l.overlays);
    expect(decoysFound(structural), "the structural parse must read none of them").toEqual([]);
    expect(structural).toEqual([REAL]);
  });

  it("a step that is not tauri-action contributes no chain, however it is spelled", () => {
    const legs = legsFromWorkflowDoc(
      "fixture.yml",
      parse(`
jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
        with:
          args: --config src-tauri/not-a-build.conf.json
`),
    );
    expect(legs).toEqual([]);
  });

  function legsForArgs(args: string) {
    return legsFromWorkflowDoc(
      "fixture.yml",
      parse(`
jobs:
  build:
    runs-on: macos-latest
    steps:
      - uses: tauri-apps/tauri-action@v0
        with:
          args: ${args}
`),
    );
  }

  it("refuses a --config that points outside the Tauri project directory", () => {
    expect(() => legsForArgs("--config /tmp/planted.json")).toThrow(
      new RegExp(`outside ${TAURI_PROJECT_DIR}`),
    );
  });

  // Reviewer F3, measured: `startsWith("src-tauri/")` is a STRING test, and every path below
  // satisfies it while naming a file somewhere else entirely. The consequence is mild — such a file
  // is still loaded, merged and pinned, so the updater assertions fire regardless — but the refusal
  // was reporting clean on the input its own doc comment named, which reads as coverage it did not
  // have. Fixed by rejecting a `..` SEGMENT before the prefix test.
  it("refuses a --config that walks back OUT of the project directory with ..", () => {
    for (const path of [
      "src-tauri/../../planted.conf.json",
      "src-tauri/../planted.conf.json",
      "src-tauri/sub/../../../planted.conf.json",
      "src-tauri\\..\\..\\planted.conf.json",
    ]) {
      expect(() => legsForArgs(`--config ${path}`), path).toThrow(/".." path segment/);
    }
  });

  // The complement, so the refusal is not just "throws on everything with a dot-dot in it": a
  // filename that merely CONTAINS `..` is not a traversal and must still be accepted.
  it("does not refuse a filename that merely contains dots", () => {
    expect(legsForArgs("--config src-tauri/tauri..odd.conf.json")[0].overlays).toEqual([
      "src-tauri/tauri..odd.conf.json",
    ]);
  });
});

describe("the real repository's build legs", () => {
  const legs = deriveBuildLegs();

  it("discovers at least the floors, across both release channels", () => {
    expect(legs.length).toBeGreaterThanOrEqual(MIN_EXPECTED_BUILD_LEGS);
    expect(new Set(legs.map((l) => l.workflow)).size).toBeGreaterThanOrEqual(
      MIN_EXPECTED_BUILD_WORKFLOWS,
    );
  });

  it("every derived overlay is a committed file under the Tauri project directory", () => {
    // Deliberately NOT a list of the expected filenames: that literal is what this ticket deleted.
    // What is asserted is the SHAPE, which cannot go stale when a real overlay is added.
    for (const leg of legs) {
      for (const overlay of leg.overlays) {
        expect(overlay.startsWith(`${TAURI_PROJECT_DIR}/`), `${leg.where}: ${overlay}`).toBe(true);
        expect(overlay, `${leg.where}: an overlay must not be the base config`).not.toBe(
          BASE_CONFIG,
        );
      }
    }
  });

  it("every leg's chain is derivable back out of its own args string, in the same order", () => {
    for (const leg of legs) {
      expect(configOverlaysFromArgs(leg.args, leg.where), leg.where).toEqual(leg.overlays);
    }
  });
});
