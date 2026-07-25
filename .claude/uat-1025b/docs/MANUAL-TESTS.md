# Manual tests — the headless backend features (2026-07 dayshift)

The features built in the 2026-07-21 dayshift live in the Tauri-free crates (`cpe-server`, `cpe-net`) and
are **not wired into the GUI yet** — that's the deferred attended work (CPE-828 / 834 / 837). So you test
them through small **runnable example programs** that exercise each end-to-end, and then confirm the
results with your OS's own tools where possible.

## Prerequisites

- Rust/cargo (the repo's toolchain). All commands run from the repo root.
- Everything below is real I/O on real files — use throwaway paths (the examples default to safe demo
  values). Nothing here touches the app or your settings.

Each example is a normal `cargo run --example` target, so it compiles on demand and is covered by
`clippy --all-targets` in CI. Pass example arguments after `--`.

---

## 1. Native metadata bridge — tags that live *on the file* (CPE-717)

**What it proves:** CPE can write a file's tags into the OS's own metadata — an **NTFS alternate data
stream** on Windows, a **POSIX xattr** on Linux, **Finder tags** on macOS — and read them back, so labels
travel with the file outside CPE's `tags.json`.

```bash
# Make a throwaway file, then tag it:
cargo run -p cpe-server --example native_tags_demo -- /path/to/file.txt quarterly finance
```

Expected output (Windows):

```
native attribute this OS writes tags under: cpe.tags
PUSHED tags ["quarterly", "finance"] + label "red" onto ...\file.txt
PULLED back (changed=true): tags ["finance", "quarterly"], label "red"
--- verify it yourself, independently of this program ---
PowerShell:  Get-Item -Path '...\file.txt' -Stream *
...
```

**Independent verification (the important part) — Windows:**

```powershell
Get-Item -Path 'C:\path\to\file.txt' -Stream *          # lists :$DATA (the file) + cpe.tags (the tags)
Get-Content -Path 'C:\path\to\file.txt' -Stream cpe.tags # → {"tags":["finance","quarterly"],"label":"red"}
```

The base file's contents and size (the `:$DATA` stream) are untouched — only a separate named stream was
added. On **Linux** verify with `getfattr -d file.txt`; on **macOS** with `xattr -l file.txt` (and the tags
show up in Finder's *Get Info*).

> Try it twice with different tags — the second push replaces the stream. Delete all tags to see the
> stream removed (that path is what `native_bridge::push` does when the store entry is empty).

---

## 2. Instant-search query core — ranked cross-folder find (CPE-831)

**What it proves:** the query grammar + relevance ranking that the future global search overlay will use —
name terms, `ext:` / `path:` filters, `*`/`?`/`{a,b}` globs, all ANDed, best-match-first.

```bash
# Files whose name contains "native", ranked:
cargo run -p cpe-server --example search_demo -- crates/server/src native

# AND semantics + an extension filter (a *main* file that is .rs):
cargo run -p cpe-server --example search_demo -- . "ext:rs main"

# A glob + a path filter:
cargo run -p cpe-server --example search_demo -- . "*.toml path:server"
```

Expected (first one):

```
query "native" parsed as Query { name_terms: ["native"], exts: [], path_terms: [] }
3 of 35 files match (best first):
  1. src\native_meta.rs
  2. src\native_tags.rs
  3. src\native_bridge.rs
```

**How to read it:** the demo walks the folder you point it at, then prints how the query *parsed* (so you
can see the filters it understood) and the ranked matches. Exact-name matches rank above prefix, above
whole-word, above substring; shorter paths break ties. Change the query and re-run — no index yet, it just
scans the folder each time (the persistent index is the big-design CPE-832).

---

## 3. Folder templates — capture a structure, stamp copies (CPE-835/836/839)

**What it proves:** capture any folder as a reusable template and stamp it out with `{token}`
substitution — the "create the same six subfolders again" chore, solved.

```bash
# Capture <source> and stamp it into <dest>, substituting {name} (and any key=value you pass):
cargo run -p cpe-server --example template_demo -- /path/to/source-folder /path/to/new-folder name=Acme date=2026-07-21
```

The demo prints the captured template as JSON, then the paths it created. `{name}` / `{date}` in **folder
names, file names, and text file contents** are replaced.

**Independent verification:** open `/path/to/new-folder` and look — the structure mirrors the source, with
tokens substituted. For example a source `src/main.rs` containing `fn main() { println!("{name}"); }`
stamps out as `fn main() { println!("Acme"); }` (this exact case is the bug CPE-839 fixed — a token inside
code braces). Stamping **refuses to overwrite** an existing file and **can't escape** the destination (a
token value like `../evil` is neutralised to a single folder name).

> The store side (CPE-836) — save/list/load/delete named templates in the config dir — is exercised by the
> `crates/server` unit tests (`cargo test -p cpe-server folder_template`); the demo shows the capture/stamp
> engine directly.

---

## 4. Network transport loop — drive the Server over TCP (CPE-825)

**What it proves:** the same explorer commands run **remotely** — `GUI → Client(Rust) → Server(Rust)` over
a real socket, with version negotiation and the security stack enforcing.

**Two terminals:**

```bash
# Terminal 1 — start the reference headless Server on a loopback port:
cargo run -p cpe-net --bin cpe-server-ref -- 127.0.0.1:9876
#   → prints "cpe-server-ref listening on 127.0.0.1:9876"

# Terminal 2 — connect a Rust client and list a directory over the wire:
cargo run -p cpe-net --example client -- 127.0.0.1:9876 crates/net/src
```

Expected (Terminal 2):

```
connected to 127.0.0.1:9876; negotiated contract version 1.0
list_dir("crates/net/src") over the wire → 5 entries:
  [dir]  bin
  [file] client.rs
  [file] lib.rs
  [file] server.rs
  [file] wire.rs
```

That directory listing was produced by the Server process and sent back over the socket — the identical
request/response the local in-process path uses. Stop the server with Ctrl-C.

### 4b. Security at the boundary (one command)

```bash
cargo run -p cpe-net --example security_demo
```

Expected:

```
Same `list_dir` request, two security chains:
local/passthrough chain : ALLOWED → list_dir returned N entries
default-deny chain       : DENIED → Unauthenticated: Transport denied: default-deny: no providers ...
The deny chain never reached the dispatcher — the request was refused at the boundary.
```

This proves the epic's invariant: security is evaluated **before** dispatch, so a request the chain
rejects never reaches the filesystem — it can't be left unsecured by forgetting to configure a command.

---

## What this does *not* cover (needs the GUI / a Mac / a server)

- Surfacing native tags in the **Properties** dialog / as a column, and the opt-in toggle — **CPE-828** (GUI).
- The **template gallery** + "New from template…" flow — **CPE-837** (GUI).
- The **global search overlay** + the persistent **index engine** — **CPE-834 / CPE-832** (GUI + big-design).
- **Real macOS Finder** byte-interop (the codec round-trips here, but confirming Finder reads our bytes
  needs a Mac) — folded into **CPE-828**.
- A **real remote** (non-loopback) network run and the frontend transport seam — **CPE-819 / CPE-820**.
