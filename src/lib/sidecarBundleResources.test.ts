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
import { readFileSync, existsSync } from "node:fs";
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

// CPE-1873 finding 6 (round 3 — independent Security Auditor, DEMONSTRATED): Tauri merges a
// per-platform config file AUTOMATICALLY, with no `--config` flag involved at all --
// `tauri-utils::config::parse::read_from` reads `tauri.conf.json` and then looks for
// `tauri.macos.conf.json` / `tauri.linux.conf.json` / `tauri.windows.conf.json` next to it and merges
// each via RFC 7396, unconditionally, on every build. None of the three exists in this repo today, and
// none is in CONFIG_CHAIN (which only lists overlays release-sidecar.yml explicitly passes via
// `--config`). Proven: a `src-tauri/tauri.windows.conf.json` containing only a `plugins.updater`
// override left every guard in this file AND crates/updater-verify's entire suite green, while shipping
// an attacker's pubkey/endpoints on every Windows build -- plain channel and sidecar both, since this
// mechanism has nothing to do with `--config` and fires on a plain `tauri.conf.json` read too.
//
// This does not try to merge/validate those files' content -- it just refuses their EXISTENCE with a
// `plugins.updater` key, since none is supposed to exist at all right now. Mirrors
// `no_automatic_per_platform_config_overrides_the_updater_pin` in
// crates/updater-verify/tests/pinned_pubkey_guard.rs (same check, Rust side, covers the base/tag path).
describe("shipped bundle — no automatic per-platform Tauri config overrides the updater pin (CPE-1873 finding 6)", () => {
  (["tauri.windows.conf.json", "tauri.macos.conf.json", "tauri.linux.conf.json"] as const).forEach(
    (fileName) => {
      it(`${fileName}: does not exist, or exists without a plugins.updater key`, () => {
        const path = join(SRC_TAURI, fileName);
        if (!existsSync(path)) return; // doesn't exist -- the expected, safe state today.
        const config = loadConfig(fileName);
        const hasUpdaterKey =
          isPlainObject(config) && isPlainObject(config.plugins) && "updater" in config.plugins;
        expect(
          hasUpdaterKey,
          `src-tauri/${fileName} exists and sets plugins.updater. Tauri merges this file into the ` +
            `build AUTOMATICALLY (no --config flag needed) on every OS build that matches its name, so ` +
            `it can silently override the pinned pubkey/endpoints exactly like a --config overlay can, ` +
            `without ever appearing in CONFIG_CHAIN or being reachable by any --config-based guard. If ` +
            `this is deliberate: it must not set plugins.updater at all -- put any real key/endpoint ` +
            `change through tauri.conf.json (or a --config overlay already in CONFIG_CHAIN) so the ` +
            `existing pins actually see it. If not: STOP, this file's plugins.updater block is not ` +
            `trustworthy (CPE-1873).`,
        ).toBe(false);
      });
    },
  );
});
