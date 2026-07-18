<p align="center">
  <img src="brand/logo.svg" alt="Cross-Platform Explorer" width="440">
</p>

# Cross-Platform Explorer

[![CI](https://github.com/StewartScottRogers/cross-platform-explorer/actions/workflows/ci.yml/badge.svg)](https://github.com/StewartScottRogers/cross-platform-explorer/actions/workflows/ci.yml)

> Brand assets, usage guide, and license live in [`brand/`](brand/BRANDING.md).

A cross-platform desktop file explorer built with **Tauri v2** (Rust backend) and
**Svelte + TypeScript** (frontend). It ships as a one-click native installer on
Windows, macOS, and Linux, and updates itself automatically.

**Website:** https://stewartscottrogers.github.io/cross-platform-explorer/
**Downloads:** [latest release](https://github.com/StewartScottRogers/cross-platform-explorer/releases/latest)

## Features

Beyond a fast, native file explorer, the **sidecar-enabled** build adds:

- **Repositories** — a left-pane entry opens an in-app browser for **GitHub** and other forges
  (GitLab, Bitbucket, Codeberg…). Enter `owner/name`, **browse** the repo tree, and **clone** it to a
  chosen folder. Access is host-brokered and allow-listed (no SSRF); clones use hardened `git` flags
  (no hooks/filters/submodule-URL injection); a private-repo token can be **remembered** in the OS
  keychain. *(Native explorer feature — see the CPE-429 epic decision note for why it isn't a sidecar.)*
- **AI Console** — launch and manage agentic coding CLIs (Claude Code, aider, …) as sandboxed
  sidecars, with a live **Agent Watch** view of their filesystem activity in the left pane. Right-click
  an agent (or the AI Console button) to **close** consoles; each closes its own process tree.
- **Any model, any reseller** — the AI Console's **Model** field is an inline, type-to-filter dropdown
  populated with the full live model list from **OpenRouter** and other resellers (Together, Fireworks,
  Groq, DeepInfra, GitHub Models, …). Per-reseller API keys are stored in the OS keychain. Model lists
  are fetched through the host's allow-listed egress and can ride a signed catalog snapshot for
  offline use.

*(These are gated to the sidecar-enabled build; the plain explorer stays fast, small, and
sidecar-free by default.)*

## Why this stack

- **Tauri** produces small native binaries (single-digit MB), builds proper OS
  installers, and includes a signed auto-updater.
- **Svelte + TypeScript** keeps the UI light and easy to extend.
- **GitHub Actions** cross-compiles all three platforms and publishes signed
  releases on every version tag.

## Prerequisites

- [Node.js](https://nodejs.org) 20+
- [Rust](https://rustup.rs) (stable)
- Platform build tools — see the
  [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/).
  On Linux you also need `libwebkit2gtk-4.1-dev` and friends (see the release
  workflow for the exact list).

## Launch options — window geometry

Position and size the window from the command line (CPE-580). Every flag is independently optional;
omit one and that dimension keeps its default.

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

- **Units:** **logical pixels** by default (stable across DPI); `--physical` opts out.
- **Precedence:** `CLI flag > saved window state > config default`.
- **Foolproof:** an off-screen or oversized request is **clamped onto the monitor** (with a warning) so
  the window is always visible and grabbable; non-numeric / zero / negative values exit with an error.
- `cpe --help` lists every flag.

## Develop

```bash
npm install
npm run tauri dev
```

This launches the app with hot-reload. The window opens at your home directory;
double-click folders to navigate, and use ↑ / ⌂ in the toolbar.

## Build a local installer

```bash
npm run tauri build
```

Installers land in `src-tauri/target/release/bundle/`.

## App icons

Tauri needs icons before a production build. Generate them once from any square
PNG (1024×1024 recommended):

```bash
npm run tauri icon path/to/icon.png
```

This writes all required sizes into `src-tauri/icons/`.

## Auto-updates — one-time setup

1. **Generate an updater signing key** (keep the private key secret):

   ```bash
   npm run tauri signer generate -- -w ./updater.key
   ```

   This prints a **public key** and writes a **private key**.

2. Paste the public key into `src-tauri/tauri.conf.json` →
   `plugins.updater.pubkey`.

3. In the same file, set the `endpoints` URL to your repo, replacing `OWNER`.

4. Add repository secrets in GitHub (**Settings → Secrets and variables →
   Actions**):
   - `TAURI_SIGNING_PRIVATE_KEY` — contents of `updater.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the password you chose (blank if none)

5. **Never commit `updater.key`.** It is already covered by `.gitignore`.

## Agent catalog updates — one-time setup (optional)

The AI Console's roster of coding agents is described by signed manifests. The catalog can refresh
from your **GitHub Releases** (the bundle rides next to the installer) so new agents / changed
install recipes arrive without shipping a new app release. It's **off until you set up signing** —
until then the app ships with the bundled catalog only, exactly as before. To turn it on:

1. **Generate a catalog signing key** — a 32-byte ed25519 seed, hex. Any tool works; e.g. in a
   Rust scratch or `python -c "import os;print(os.urandom(32).hex())"` for the seed, then derive the
   public key by running the signer once locally:

   ```bash
   CPE_CATALOG_SIGNING_KEY=<seed-hex> \
     cargo run --manifest-path sidecar/host/Cargo.toml --bin catalog-sign -- \
     sidecar/ai-console/agents ./catalog-out 1
   ```

   (The `catalog-index.json` it writes is signed by your key; the matching **public** key is what you
   embed next.)

2. Put the **public key** (hex) into `CATALOG_TRUSTED_KEYS` in `src-tauri/src/lib.rs` (empty by
   default = feature dormant). This is a public value, safe to commit.

3. Add the repository secret **`CPE_CATALOG_SIGNING_KEY`** (the *private* seed hex). The release
   workflow's `catalog` job then signs `sidecar/ai-console/agents/*` and uploads the bundle as
   release assets. Without the secret, that job **skips** — releases are unaffected.

4. **Never commit the private seed.** The catalog fetch is host-mediated, TLS, proxy/offline-aware,
   verified + anti-rollback before anything is trusted (see `docs/design/CPE-308-agent-catalog-updates.md`).

## Releasing

```bash
# bump the version in package.json, src-tauri/Cargo.toml, and tauri.conf.json
git commit -am "release v0.1.1"
git tag v0.1.1
git push origin main --tags
```

The workflow builds every platform, signs the update artifacts, and creates a
**draft** GitHub Release plus `latest.json`. Review it, then publish. Installed
apps pick up the update on their next launch.

## Code signing (recommended for frictionless installs)

Without OS code signing, users see "unknown developer" warnings. To remove them:

- **macOS:** an Apple Developer account; set the `APPLE_*` secrets and uncomment
  that block in `.github/workflows/release.yml`.
- **Windows:** a code-signing certificate (OV or EV).

Updater signing (above) is separate and always required for auto-updates.

## Project layout

```
├── src/                 # Svelte frontend
│   ├── App.svelte       # File explorer UI + update check
│   ├── main.ts
│   └── app.css
├── src-tauri/           # Rust backend
│   ├── src/lib.rs       # filesystem commands (list_dir, home_dir, parent_dir)
│   ├── src/main.rs
│   ├── tauri.conf.json  # app + bundle + updater config
│   └── capabilities/    # permission grants for the frontend
├── .github/workflows/release.yml
└── package.json
```

See [CLAUDE.md](./CLAUDE.md) for AI-assistant maintenance notes.
