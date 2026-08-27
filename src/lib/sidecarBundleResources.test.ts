// CPE-1271: guard the SHIPPED sidecar bundle against silently missing runtime resources.
//
// This bug class bit twice in one session:
//   - CPE-1258/1267: pdfium.dll path (thumbnails) — resource_dir/bundle mismatch.
//   - CPE-1270: `icons/icon.png` (drag-out preview) — the base tauri.conf.json's ARRAY-form
//     `bundle.resources` (`["icons/icon.png"]`) was silently REPLACED by the sidecar overlays'
//     OBJECT-form `bundle.resources`, because Tauri's `--config` merge treats an overlay whose
//     value at a key is a plain object as a wholesale REPLACEMENT of a base value that isn't
//     also a plain object (an array included) — it does NOT concatenate/union them.
//
// The app the user actually runs is the release-sidecar.yml build: the base `tauri.conf.json`
// with a chain of `--config` overlays applied on top, in a specific order. Nothing verified that
// the FINAL merged `bundle.resources` still contains every resource the runtime code resolves —
// this test is that guard. It mirrors the exact overlay chain release-sidecar.yml's
// `release-sidecar` job passes to `tauri-action` for each shipped OS (see its matrix + the
// `args:` line under "Build and publish sidecar-enabled release").
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const SRC_TAURI = join(process.cwd(), "src-tauri");

type ShipOS = "windows" | "linux" | "macos";

/**
 * The exact `--config` overlay chain release-sidecar.yml applies per shipped OS, base config
 * first (base is always implicit — tauri-action loads `tauri.conf.json` before any `--config`
 * overlay is applied). Keep this in lockstep with `.github/workflows/release-sidecar.yml`'s
 * `release-sidecar` job matrix (`overlay` / `pdfium_overlay`) and its `args:` line — if that
 * workflow's overlay chain changes, update this list to match or the guard stops reflecting what
 * actually ships.
 */
const CONFIG_CHAIN: Record<ShipOS, string[]> = {
  windows: [
    "tauri.conf.json",
    "tauri.sidecar.conf.json",
    "tauri.sidecar.windows.conf.json",
    "tauri.sidecar.pdfium.windows.conf.json",
  ],
  linux: [
    "tauri.conf.json",
    "tauri.sidecar.conf.json",
    "tauri.sidecar.unix.conf.json",
    "tauri.sidecar.pdfium.linux.conf.json",
  ],
  macos: [
    "tauri.conf.json",
    "tauri.sidecar.conf.json",
    "tauri.sidecar.unix.conf.json",
    "tauri.sidecar.pdfium.macos.conf.json",
  ],
};

function loadConfig(fileName: string): unknown {
  return JSON.parse(readFileSync(join(SRC_TAURI, fileName), "utf8"));
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/**
 * Tauri's own `--config` merge semantics, reduced to what this guard needs: when the SAME key is
 * a plain object on both sides, merge recursively (key-wise); otherwise the overlay's value wins
 * OUTRIGHT, replacing the base's value with no attempt to combine them. That "outright replace"
 * branch is exactly the CPE-1270 footgun: a base ARRAY meeting an overlay OBJECT at
 * `bundle.resources` doesn't union — the array is gone, and only the overlay's keys survive.
 */
function mergeJson(base: unknown, overlay: unknown): unknown {
  if (overlay === undefined) return base;
  if (isPlainObject(base) && isPlainObject(overlay)) {
    const merged: Record<string, unknown> = { ...base };
    for (const key of Object.keys(overlay)) {
      merged[key] = mergeJson(base[key], overlay[key]);
    }
    return merged;
  }
  return overlay;
}

/** The FULL merged config (base + every `--config` overlay, in order) for a shipped OS — the config
 *  that actually governs the release-sidecar.yml build for that OS. Shared by every guard in this
 *  file that needs to know what a shipped install's config really ends up containing. */
function mergedConfig(os: ShipOS): Record<string, unknown> {
  const configs = CONFIG_CHAIN[os].map(loadConfig);
  const merged = configs.reduce((acc, cfg) => mergeJson(acc, cfg));
  if (!isPlainObject(merged)) {
    throw new Error(`${os}: merged tauri config chain did not produce a JSON object`);
  }
  return merged;
}

/** Final `bundle.resources` value (array | object | undefined) after applying an OS's full overlay chain. */
function mergedBundleResources(os: ShipOS): unknown {
  const merged = mergedConfig(os);
  return isPlainObject(merged.bundle) ? merged.bundle.resources : undefined;
}

/**
 * Normalizes a merged `bundle.resources` value (array-form or object-form) down to the set of
 * DESTINATION paths that will actually exist inside the shipped bundle's resource root — what
 * `resource_dir()`-based runtime lookups resolve against. Array-form entries (Tauri's shorthand)
 * use the same string as both source and destination.
 */
function resourceDestinations(resources: unknown): Set<string> {
  if (Array.isArray(resources)) return new Set(resources.map(String));
  if (isPlainObject(resources)) return new Set(Object.values(resources).map(String));
  return new Set();
}

interface RequiredResource {
  /** Human label used in failure messages. */
  id: string;
  /** Where this resource is resolved/consumed at runtime — the one place to look when this fails,
   *  and the place a NEW resource dependency should be registered alongside this list. */
  consumer: string;
  /** Its expected destination path (relative to the bundle's resource root) for a given shipped OS. */
  dest: (os: ShipOS) => string;
}

/**
 * THE canonical list of runtime-required bundled resources (CPE-1271). Every resource the running
 * app resolves out of `resource_dir()` (or the frontend's `resolveResource`) must be registered
 * here, with a comment/consumer pointing at the code that resolves it — that's what keeps a future
 * resource dependency from silently missing the shipped bundle the way CPE-1258/1267/1270 did.
 */
const REQUIRED_RESOURCES: RequiredResource[] = [
  {
    id: "drag-out preview icon",
    consumer: "src/lib/dragOut.ts — DEFAULT_DRAG_ICON / resolveDragIcon()",
    dest: () => "icons/icon.png",
  },
  {
    id: "pdfium dynamic library",
    consumer: "crates/server/src/thumb_pdf.rs — resolve_bindings() (dylib next to a bundled native-dep dir)",
    dest: (os) => (os === "windows" ? "pdfium.dll" : os === "macos" ? "libpdfium.dylib" : "libpdfium.so"),
  },
  {
    id: "ffmpeg executable",
    consumer: "crates/server/src/thumb_video.rs — resolve_ffmpeg_bin()",
    dest: (os) => (os === "windows" ? "ffmpeg.exe" : "ffmpeg"),
  },
  {
    id: "ai-console sidecar binary",
    consumer: "src-tauri/src/lib.rs — resolve_ai_console_bin() / sidecar_dirs() (sidecars/<id>[.exe])",
    dest: (os) => `sidecars/ai-console${os === "windows" ? ".exe" : ""}`,
  },
  {
    id: "agent-board sidecar binary",
    consumer: "src-tauri/src/lib.rs — resolve_agent_board_bin() (sidecars/<id>[.exe])",
    dest: (os) => `sidecars/agent-board${os === "windows" ? ".exe" : ""}`,
  },
  {
    id: "repos sidecar binary",
    consumer: "src-tauri/src/lib.rs — resolve_sidecar_bin() (sidecars/<id>[.exe])",
    dest: (os) => `sidecars/repos${os === "windows" ? ".exe" : ""}`,
  },
];

describe("shipped sidecar bundle — every runtime-required resource is bundled (CPE-1271)", () => {
  (["windows", "linux", "macos"] as const).forEach((os) => {
    it(`${os}: merged bundle.resources includes every REQUIRED_RESOURCES entry`, () => {
      const destinations = resourceDestinations(mergedBundleResources(os));
      const missing = REQUIRED_RESOURCES.map((r) => ({ ...r, dest: r.dest(os) })).filter(
        (r) => !destinations.has(r.dest),
      );
      expect(
        missing,
        missing
          .map(
            (r) =>
              `${os}: the shipped sidecar bundle would NOT include "${r.dest}" — ${r.id}, resolved at ` +
              `runtime by ${r.consumer}`,
          )
          .join("\n"),
      ).toEqual([]);
    });
  });

  // Documents + locks in the exact footgun this ticket guards against: an overlay OBJECT at a key
  // whose base value is an ARRAY replaces it outright rather than merging — this is Tauri's real
  // `--config` behavior, not a quirk of this test's merge helper (CPE-1270's root cause).
  it("mergeJson: an overlay OBJECT replaces (not unions with) a base ARRAY at the same key — the CPE-1270 footgun", () => {
    const base = { resources: ["a.png"] };
    const overlay = { resources: { "b.dll": "b.dll" } };
    expect(mergeJson(base, overlay)).toEqual({ resources: { "b.dll": "b.dll" } });
  });
});

// CPE-1873 (round 2 — independent Security Auditor, DEMONSTRATED not inferred): the updater
// root-of-trust pin in crates/updater-verify only ever reads the BASE `tauri.conf.json`. The build
// every install actually ships is release-sidecar.yml's: base config + this file's own `CONFIG_CHAIN`
// of `--config` overlays. Tauri's `--config` merge (RFC 7386 recursive object merge — the same
// mechanism the guard above proves overrides `bundle.resources`, CPE-1270) lets any overlay in that
// chain override `plugins.updater.pubkey` / `.endpoints` too. Proven: adding an updater override block
// to `tauri.sidecar.conf.json` alone (base file untouched) left crates/updater-verify's entire test
// suite green, including its base-config pin, while the actual shipped sidecar channel's root of trust
// was attacker-controlled. This guard checks the full merged `--config` overlay chain instead, so an
// override anywhere in CONFIG_CHAIN is caught regardless of which file introduced it. It does NOT (by
// itself) cover a config file Tauri merges automatically outside of `--config` entirely — see the
// separate describe block below (CPE-1873 finding 6) for that.
//
// Keep these two literals in lockstep with crates/updater-verify/src/pinned_pubkey.rs's
// EXPECTED_TAURI_UPDATER_PUBKEY / EXPECTED_TAURI_UPDATER_ENDPOINTS — same value, same rotation
// procedure (documented in that file's module doc).
const EXPECTED_UPDATER_PUBKEY =
  "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDUyMUU1NzRGNjhFMjU2MUEKUldRYVZ1Sm9UMWNlVXYvc283NmRaeHVhYkQrNGpQKzZ5aitWL1ErWWRxUGFWRXlQdXJDTkNENG4K";
const EXPECTED_UPDATER_ENDPOINTS = [
  "https://github.com/StewartScottRogers/cross-platform-explorer/releases/latest/download/latest.json",
];

function mergedUpdaterConfig(os: ShipOS): { pubkey: unknown; endpoints: unknown } {
  const merged = mergedConfig(os);
  const plugins = isPlainObject(merged.plugins) ? merged.plugins : undefined;
  const updater = plugins && isPlainObject(plugins.updater) ? plugins.updater : undefined;
  return { pubkey: updater?.pubkey, endpoints: updater?.endpoints };
}

describe("shipped sidecar bundle — updater root of trust survives the FULL overlay merge (CPE-1873)", () => {
  (["windows", "linux", "macos"] as const).forEach((os) => {
    it(`${os}: merged plugins.updater.pubkey equals the pinned value (no overlay override)`, () => {
      const { pubkey } = mergedUpdaterConfig(os);
      expect(
        pubkey,
        `${os}: the FINAL merged config's plugins.updater.pubkey does not match the pinned value -- ` +
          `some file in the overlay chain (${CONFIG_CHAIN[os].join(" -> ")}) overrides it. This IS the ` +
          `shipped sidecar channel's actual root of trust; an override here is a live compromise, not a ` +
          `lint nit. See crates/updater-verify/src/pinned_pubkey.rs (CPE-1873) for the rotation ` +
          `procedure if this is deliberate -- update the pin there too, in the same commit.`,
      ).toEqual(EXPECTED_UPDATER_PUBKEY);
    });

    it(`${os}: merged plugins.updater.endpoints equals the pinned value (no overlay override)`, () => {
      const { endpoints } = mergedUpdaterConfig(os);
      expect(
        endpoints,
        `${os}: the FINAL merged config's plugins.updater.endpoints does not match the pinned value -- ` +
          `some file in the overlay chain (${CONFIG_CHAIN[os].join(" -> ")}) overrides it. A changed ` +
          `endpoint can silently downgrade users to an older, genuinely-signed but vulnerable build ` +
          `forever, even with the pubkey pin intact. See crates/updater-verify/src/pinned_pubkey.rs ` +
          `(CPE-1873).`,
      ).toEqual(EXPECTED_UPDATER_ENDPOINTS);
    });
  });
});

// CPE-1903 (supersedes CPE-1873 finding 6): Tauri merges a per-platform config file AUTOMATICALLY,
// with no `--config` flag and no workflow involvement — `tauri-utils::config::parse::read_from` reads
// `tauri.conf.json` and then merges a per-platform file from the same directory via RFC 7396, on every
// build for that platform. Whatever that file sets wins, `plugins.updater.pubkey`/`.endpoints`
// included, and it appears in no `--config` chain, so CONFIG_CHAIN above cannot see it.
//
// CPE-1873 round 3 closed this by hardcoding three `.json` filenames. Tauri's real surface is FIFTEEN:
// `ConfigFormat::into_platform_file_name` crosses three formats (`tauri.<t>.conf.json`,
// `tauri.<t>.conf.json5`, `Tauri.<t>.toml`) with five `Target` variants (macos/windows/linux/
// android/ios), `does_supported_file_name_exist` returns true if ANY enabled format's platform file
// exists, and `do_parse` falls through json -> json5 -> toml. Both non-`.json` formats were
// demonstrated ingesting an attacker config through this repo's own installed `@tauri-apps/cli` while
// every guard reported green.
//
// So this does not name files: it lists the directory and classifies what is actually there.
// `readdirSync` reports the on-disk spelling, so an ASCII-lowercased shape match behaves identically
// on a case-insensitive NTFS build runner and on the byte-exact `ubuntu-latest` host where
// `release-sidecar.yml`'s `verify-updater-pin` job runs this file. Round 3's `existsSync(join(dir,
// name))` was a LOOKUP, and therefore silently blind on exactly that host while Windows merged the
// file anyway.
//
// Mirrors `crates/updater-verify/src/platform_config_guard.rs` (the Rust side, which additionally runs
// inside `verify-release-artifacts` so it reaches `release.yml`'s tag path — no `#[test]` and no
// vitest can). Keep the two derivations in lockstep; the Rust module doc carries the full rationale.

/** The second dot-segment of every name `ConfigFormat::into_platform_file_name` can produce. */
const TAURI_PLATFORM_TOKENS: readonly string[] = ["macos", "windows", "linux", "android", "ios"];

/** ASCII-only case fold, so this matches the Rust side's `to_ascii_lowercase` exactly. */
function asciiLower(s: string): string {
  return s.replace(/[A-Z]/g, (c) => String.fromCharCode(c.charCodeAt(0) + 32));
}

/**
 * Does `fileName` name a config file Tauri merges automatically? Matched by shape — `tauri` `.`
 * <platform token> `.` <at least one more segment> — never by a particular spelling of the tail, so a
 * format Tauri adds tomorrow is covered without anyone editing a list. Deliberately NOT matched:
 * `tauri.conf.json` / `Tauri.toml` (the base config, pinned by value above) and `tauri.sidecar.*`
 * (the explicit `--config` overlays, covered by CONFIG_CHAIN) — their second segment is not a
 * platform token, which is exactly the property Tauri itself keys on.
 */
function isAutoMergedPlatformConfigName(fileName: string): boolean {
  const segments = asciiLower(fileName).split(".");
  return (
    segments.length >= 3 && segments[0] === "tauri" && TAURI_PLATFORM_TOKENS.includes(segments[1])
  );
}

/**
 * Why this per-platform config file must be refused, or `null` if it is clean.
 *
 * Refused for CARRYING a `plugins.updater` key — never for merely existing, never by comparing its
 * value to the pin. A per-platform file setting only `plugins.cli` stays allowed; a
 * `{"plugins":{"updater":null}}` is refused, because under RFC 7396 a `null` DELETES the base
 * config's updater block. Both were deliberate CPE-1873 round-3 choices, preserved verbatim.
 *
 * Strict JSON is inspected structurally. Anything else (JSON5, TOML, or a `.json`-named file holding
 * JSON5 — which Tauri's `do_parse` accepts) gets a conservative textual scan, because no JSON5/TOML
 * parser is carried here on purpose and a guard on the root of trust must not silently pass a format
 * it cannot read.
 */
function platformConfigUpdaterRefusal(text: string): string | null {
  let parsed: unknown;
  let parsedAsStrictJson = true;
  try {
    parsed = JSON.parse(text);
  } catch {
    parsedAsStrictJson = false;
  }
  if (parsedAsStrictJson) {
    const hasUpdaterKey =
      isPlainObject(parsed) && isPlainObject(parsed.plugins) && "updater" in parsed.plugins;
    return hasUpdaterKey
      ? "exists and sets a `plugins.updater` key. Tauri merges this file into the build automatically " +
          "via RFC 7396, so that key decides the shipped updater's root of trust -- and a `null` there " +
          "DELETES the pinned block just as effectively as a value replaces it"
      : null;
  }
  if (/updater/i.test(text)) {
    return (
      "exists, is not strict JSON (so this guard cannot parse it structurally -- no JSON5/TOML parser " +
      "is carried here on purpose), and mentions `updater`. Refused rather than guessed at"
    );
  }
  if (text.includes("\\")) {
    return (
      "exists, is not strict JSON (so this guard cannot parse it structurally), and contains backslash " +
      "escape sequences that could spell a `plugins.updater` key without the literal token ever " +
      "appearing. Refused rather than guessed at"
    );
  }
  return null;
}

/** Every auto-merged per-platform config in `src-tauri/` that sets `plugins.updater`. */
function scanForPlatformConfigUpdaterOverrides(): { fileName: string; reason: string }[] {
  return readdirSync(SRC_TAURI, { withFileTypes: true })
    .filter((e) => e.isFile() && isAutoMergedPlatformConfigName(e.name))
    .flatMap((e) => {
      const reason = platformConfigUpdaterRefusal(readFileSync(join(SRC_TAURI, e.name), "utf8"));
      return reason === null ? [] : [{ fileName: e.name, reason }];
    })
    .sort((a, b) => a.fileName.localeCompare(b.fileName));
}

describe("shipped bundle — no auto-merged per-platform Tauri config overrides the updater pin (CPE-1903)", () => {
  it("src-tauri/ holds no per-platform config file that sets plugins.updater, in any format or casing", () => {
    const hits = scanForPlatformConfigUpdaterOverrides();
    expect(
      hits,
      `SECURITY (CPE-1903): a per-platform Tauri config file in src-tauri/ can override the updater's ` +
        `root of trust:\n` +
        hits.map((h) => `  - ${h.fileName} ${h.reason}`).join("\n") +
        `\n\nTauri picks these up with NO --config flag: read_from() merges ` +
        `tauri.<platform>.conf.json / .json5 / Tauri.<platform>.toml next to tauri.conf.json via RFC ` +
        `7396 on every build for that platform, so such a file is invisible to the base-config pin, ` +
        `invisible to the CONFIG_CHAIN guard above, and ships on the plain AND sidecar channels alike. ` +
        `If deliberate: it must not set plugins.updater at all -- route any real key/endpoint change ` +
        `through tauri.conf.json (or a --config overlay already in CONFIG_CHAIN) and update ` +
        `crates/updater-verify/src/pinned_pubkey.rs in the same commit. If not deliberate: STOP, this ` +
        `commit's builds are not trustworthy.`,
    ).toEqual([]);
  });

  // The derivation itself, exercised directly. These are the cases round 3's three-filename list let
  // through, asserted here so the next variant has to beat the SHAPE, not a spelling.
  it("matches every filename Tauri can auto-merge — 3 formats x 5 targets, any casing", () => {
    const everyName = TAURI_PLATFORM_TOKENS.flatMap((t) => [
      `tauri.${t}.conf.json`,
      `tauri.${t}.conf.json5`,
      `Tauri.${t}.toml`,
    ]);
    expect(everyName).toHaveLength(15);
    for (const name of everyName) expect(isAutoMergedPlatformConfigName(name), name).toBe(true);
    for (const name of [
      "Tauri.Windows.Conf.json",
      "TAURI.WINDOWS.CONF.JSON",
      "tauri.WINDOWS.conf.JSON5",
      "Tauri.LINUX.Toml",
      "tauri.windows.conf.yaml", // a format Tauri has not shipped yet
    ]) {
      expect(isAutoMergedPlatformConfigName(name), name).toBe(true);
    }
  });

  it("does not match the base config or the explicit --config overlays", () => {
    for (const name of [
      "tauri.conf.json",
      "tauri.conf.json5",
      "Tauri.toml",
      "tauri.sidecar.conf.json",
      "tauri.sidecar.windows.conf.json",
      "tauri.sidecar.pdfium.macos.conf.json",
      "Cargo.toml",
      "tauri.windows",
      "notauri.windows.conf.json",
    ]) {
      expect(isAutoMergedPlatformConfigName(name), name).toBe(false);
    }
  });

  it("refuses a plugins.updater key in every format, including an RFC 7396 delete", () => {
    expect(platformConfigUpdaterRefusal(`{"plugins":{"updater":{"pubkey":"x"}}}`)).not.toBeNull();
    expect(platformConfigUpdaterRefusal(`{"plugins":{"updater":null}}`)).not.toBeNull();
    expect(
      platformConfigUpdaterRefusal(`// c\n{ plugins: { updater: { pubkey: 'x' } } }`),
    ).not.toBeNull();
    expect(platformConfigUpdaterRefusal(`[plugins.updater]\npubkey = "x"\n`)).not.toBeNull();
    expect(platformConfigUpdaterRefusal(`plugins.updater.pubkey = "x"\n`)).not.toBeNull();
    expect(platformConfigUpdaterRefusal(`{ plugins: { "\\u0075pdater": {} } }`)).not.toBeNull();
  });

  it("still allows a per-platform file that does not touch the updater", () => {
    expect(platformConfigUpdaterRefusal(`{"plugins":{"cli":{"args":[]}}}`)).toBeNull();
    expect(platformConfigUpdaterRefusal(`{"bundle":{"targets":["msi"]}}`)).toBeNull();
    expect(platformConfigUpdaterRefusal(`[plugins.cli]\ndescription = "hi"\n`)).toBeNull();
  });
});
