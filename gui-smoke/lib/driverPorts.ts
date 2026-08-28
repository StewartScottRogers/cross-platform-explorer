// CPE-1910 round 2 — the ONE declaration of the two fixed ports tauri-driver occupies.
//
// They lived in `wdio.conf.ts` as file-local constants, which was fine while `wdio.conf.ts` was their
// only consumer. `scripts/run-suite.ts` now needs them too: between two job-level suite attempts it has
// to wait for the previous attempt's driver to actually release these exact ports before spawning the
// next one (see `waitForPortFree`). Copying `4444`/`4445` into that script with a comment saying "same
// as wdio.conf.ts" would be a provenance claim that nothing checks and that goes stale in silence
// (CPE-1933) — and CPE-1950's preferred answer to a removable duplication is to remove it, not to derive
// it. So the constants move here and both files import them.
//
// CPE-1832: tauri-driver is a two-port intermediary, not a single listener, and both ports are named
// explicitly (passed to `spawn` as `--port`/`--native-port` in `wdio.conf.ts#startTauriDriver`) rather
// than left to its internal defaults — see `beforeSession`'s comment for why that distinction is the
// actual fix.
//
// CPE-1843: because these two constants and those two flags are a contract with a binary CI installs
// from crates.io, `gui-smoke.yml` pins `cargo install tauri-driver --version 2.0.6` at BOTH of its
// install sites rather than taking whatever is newest. Note WHICH parts of that contract can rot
// quietly: `--port`/`--native-port` are passed explicitly, so their upstream defaults are overridden and
// only the flag NAMES matter — and a rename fails loudly, since tauri-driver rejects unknown args. The
// silent one is `--native-host`, which we do NOT pass: the readiness waits poll "127.0.0.1" purely
// because that is its default, so a future re-default would send the native driver somewhere nothing
// ever looks. Re-verify `src/cli.rs` when bumping that pin.

/** tauri-driver's OWN front door — the port wdio talks to (`config.port` in `wdio.conf.ts`, and
 *  tauri-driver 2.0.6's own `--port` default, `cli.rs`). */
export const TAURI_DRIVER_PORT = 4444;

/** The port tauri-driver spawns the REAL platform WebDriver on (WebKitWebDriver on Linux, msedgedriver
 *  on Windows) and proxies every request to (`cli.rs`'s `--native-port` default, also 4445 — verified by
 *  reading the vendored tauri-driver-2.0.6 source in the local cargo registry cache,
 *  `src/{main,cli,server}.rs`).
 *
 *  The awkward one to recover from. It is a GRANDCHILD of the wdio worker — spawned by tauri-driver, not
 *  by us — so nothing in our teardown signals it directly, and on the CPE-1910 failure path it is
 *  already the process in a bad state. */
export const NATIVE_DRIVER_PORT = 4445;

/** Both, in the order a start-up waits on them (front door first, back door second) — so a caller that
 *  cares about "tauri-driver's ports" as a set cannot enumerate one and forget the other. */
export const DRIVER_PORTS: readonly { port: number; label: string }[] = [
  { port: TAURI_DRIVER_PORT, label: "tauri-driver" },
  { port: NATIVE_DRIVER_PORT, label: "the native WebDriver (WebKitWebDriver/msedgedriver)" },
];
