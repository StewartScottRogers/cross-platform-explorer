<p align="center">
  <img src="brand/logo.svg" alt="Cross-Platform Explorer" width="440">
</p>

# Cross-Platform Explorer

[![CI](https://github.com/StewartScottRogers/cross-platform-explorer/actions/workflows/ci.yml/badge.svg)](https://github.com/StewartScottRogers/cross-platform-explorer/actions/workflows/ci.yml)

A fast, tiny desktop **file explorer** for Windows, macOS, and Linux — with a live view of what your
**AI coding agents** are doing to your files. Built with **Tauri v2** (Rust) and **Svelte +
TypeScript**; it installs as a proper native app and updates itself automatically.

**⬇ [Download](https://stewartscottrogers.github.io/cross-platform-explorer/)** ·
**🌐 [Website](https://stewartscottrogers.github.io/cross-platform-explorer/)** ·
**📦 [All releases](https://github.com/StewartScottRogers/cross-platform-explorer/releases)**

> Developer? Jump to **[For developers](#for-developers)**. Brand assets and license live in
> [`brand/`](brand/BRANDING.md); AI-assistant maintenance notes are in [CLAUDE.md](./CLAUDE.md).

---

## Get started

1. **Download** the installer for your OS from the
   [website](https://stewartscottrogers.github.io/cross-platform-explorer/) (or the
   [releases page](https://github.com/StewartScottRogers/cross-platform-explorer/releases)) — a native
   `.msi`/`.exe` (Windows), `.dmg` (macOS), or `.AppImage`/`.deb` (Linux). It updates itself afterwards.
2. **Open it.** It starts at your home folder. Double-click to navigate; use the breadcrumb or
   **↑ / ← / →**; open tabs or a second pane for side-by-side work.
3. **Do more.** Search and filter, batch-rename, tag, extract archives, open the disk-usage treemap, and
   press **F1** anywhere for the built-in docs (every section has its own help button too).

## Features

**Browse & navigate** — tabs, an optional dual-pane commander mode, a breadcrumb path bar, full
keyboard navigation, and back / forward / up with history.

**Find & organize** — content search, filename search with glob patterns, a duplicates finder, saved
smart folders, tags (with import/export), pins & favorites, batch rename, and quick filtering.

**Files & archives** — copy, move, cut/paste, duplicate, trash, new file/folder, copy-as-path, zip
compress and extract, and drag files in and out of the OS.

**Preview & insight** — inline previews for images, PDF, markdown, code (syntax-highlighted), audio &
video and many more types; universal thumbnails; a **disk-usage Space analyzer** (treemap); a compare
studio; and properties / attributes / permissions.

**Experience** — light & dark themes with consistent cross-platform menus, streaming liveness so large
folders paint instantly, a diagnostics mode, and an in-app **contextual documentation library** (F1).

**AI agent tooling** *(sidecar-enabled build)* —

- **Agent Watch** — a live left-pane view of an AI coding agent's filesystem activity: the files it
  reads and the edits it makes, as it works, with diff peeks.
- **AI Console** — launch and manage agentic coding CLIs (Claude Code, aider, …) as sandboxed
  sidecars against any folder, with any provider and model (OpenRouter and other resellers; keys in the
  OS keychain). Close a console and its whole process tree goes with it.
- **Agent Grid / Board / Swarms** — tile every running agent, track work on a ticket board, or run a
  coordinated multi-agent swarm on one project.
- **Repositories & Workbench** — browse and clone GitHub and other forges in-app (host-brokered,
  allow-listed, hardened `git`), and review the working tree's git diff in the Workbench.

*The AI features are opt-in and gated to the sidecar-enabled build; with them off, the plain explorer
stays fast, small, and predictable by default.*

---

## For developers

A [Tauri v2](https://v2.tauri.app) app: Rust backend in `src-tauri/`, Svelte + TypeScript frontend in
`src/`. The frontend calls Rust via `invoke("command", args)`; commands live in `src-tauri/src/lib.rs`.

### Why this stack

- **Tauri** produces small native binaries (single-digit MB), builds proper OS installers, and includes
  a signed auto-updater.
- **Svelte + TypeScript** keeps the UI light and easy to extend.
- **GitHub Actions** cross-compiles all three platforms and publishes signed releases on every tag.

### Prerequisites

- [Node.js](https://nodejs.org) 20+
- [Rust](https://rustup.rs) (stable)
- Platform build tools — see the [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/).
  On Linux you also need `libwebkit2gtk-4.1-dev` and friends (see `.github/workflows/release.yml` for
  the exact list).

### Develop

```bash
npm install
npm run tauri dev      # app with hot-reload
npm run check          # type-check Svelte + TS
```

The window opens at your home directory; double-click folders to navigate, and use ↑ / ⌂ in the toolbar.

### Build a local installer

```bash
npm run tauri build    # installers land in src-tauri/target/release/bundle/
```

### App icons

Tauri needs icons before a production build. Generate them once from any square PNG (1024×1024
recommended):

```bash
npm run tauri icon path/to/icon.png   # writes all sizes into src-tauri/icons/
```

### Launch options — window geometry

Position and size the window from the command line (every flag is independently optional; omit one and
that dimension keeps its default):

```bash
cpe --x 100 --y 100 --width 1200 --height 800   # 1200×800 at (100,100)
cpe --position center --width 1000              # centred, 1000 wide, default height
cpe --monitor 1 --maximized                     # maximized on the second display
```

| Flag | Meaning |
|------|---------|
| `--x` / `--y` | Window position (left / top) |
| `--width` / `--height` | Window size |
| `--position <preset>` | `top-left` \| `top-right` \| `bottom-left` \| `bottom-right` \| `center` (explicit `--x`/`--y` override it) |
| `--monitor <n>` | Target display, 0-based |
| `--maximized` / `--fullscreen` | Start maximized / fullscreen |
| `--physical` | Treat sizes/positions as physical pixels |

Units are **logical pixels** by default (stable across DPI); `--physical` opts out. Precedence is
`CLI flag > saved window state > config default`. Off-screen/oversized requests are **clamped onto the
monitor** so the window is always visible; invalid values exit with an error. `cpe --help` lists every flag.

### Auto-updates — one-time setup

1. **Generate an updater signing key** (keep the private key secret):

   ```bash
   npm run tauri signer generate -- -w ./updater.key
   ```

2. Paste the printed **public key** into `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`.
3. In the same file, set the `endpoints` URL to your repo, replacing `OWNER`.
4. Add repository secrets (**Settings → Secrets and variables → Actions**):
   - `TAURI_SIGNING_PRIVATE_KEY` — contents of `updater.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the password you chose (blank if none)
5. **Never commit `updater.key`** — it is already covered by `.gitignore`.

### Agent catalog updates — one-time setup (optional)

The AI Console's roster of coding agents is described by signed manifests, which can refresh from your
**GitHub Releases** so new agents / changed install recipes arrive without shipping a new app release.
It's **off until you set up signing** — until then the app ships with the bundled catalog only.

1. **Generate a catalog signing key** — a 32-byte ed25519 seed (hex), e.g.
   `python -c "import os;print(os.urandom(32).hex())"`, then derive the public key by running the signer
   once locally:

   ```bash
   CPE_CATALOG_SIGNING_KEY=<seed-hex> \
     cargo run --manifest-path sidecar/host/Cargo.toml --bin catalog-sign -- \
     sidecar/ai-console/agents ./catalog-out 1
   ```

2. Put the **public key** (hex) into `CATALOG_TRUSTED_KEYS` in `src-tauri/src/lib.rs` (empty by default
   = feature dormant). This is a public value, safe to commit.
3. Add the repository secret **`CPE_CATALOG_SIGNING_KEY`** (the *private* seed hex). The release
   workflow's `catalog` job then signs `sidecar/ai-console/agents/*` and uploads the bundle. Without the
   secret, that job **skips** — releases are unaffected.
4. **Never commit the private seed.** The catalog fetch is host-mediated, TLS, proxy/offline-aware, and
   verified + anti-rollback before anything is trusted (see
   `docs/design/CPE-308-agent-catalog-updates.md`).

### Releasing

Keep the version in sync across `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`,
then tag:

```bash
git commit -am "release v0.1.1"
git tag v0.1.1
git push origin main --tags
```

The workflow builds every platform, signs the update artifacts, and creates a **draft** GitHub Release
plus `latest.json`. Review it, then publish. Installed apps pick up the update on their next launch.

### Code signing (recommended for frictionless installs)

Without OS code signing, users see "unknown developer" warnings. To remove them:

- **macOS:** an Apple Developer account; set the `APPLE_*` secrets and uncomment that block in
  `.github/workflows/release.yml`.
- **Windows:** a code-signing certificate (OV or EV).

Updater signing (above) is separate and always required for auto-updates.

### Project layout

```
├── src/                 # Svelte frontend (App.svelte, components, lib)
├── src-tauri/           # Rust backend
│   ├── src/lib.rs       # filesystem + OS commands (async, off the main thread)
│   ├── tauri.conf.json  # app + bundle + updater config
│   └── capabilities/    # permission grants for the frontend
├── sidecar/             # AI Console sidecar platform (host + ai-console)
├── docs/                # GitHub Pages site + design docs
└── .github/workflows/   # ci.yml, release.yml, release-sidecar.yml
```

See [CLAUDE.md](./CLAUDE.md) for architecture and AI-assistant maintenance notes.
