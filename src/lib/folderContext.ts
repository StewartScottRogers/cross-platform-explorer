/**
 * Folder-context plugin registry (CPE-235). Mirrors the file-type provider idea
 * one level up: each provider cheaply inspects a folder's *listing* (marker
 * files/dirs — never a deep scan) and, if it recognizes the folder, claims a
 * context with a label, icon, optional detail, and actions. A folder may match
 * several providers; results are aggregated. Detection runs on the folder the
 * user is currently viewing, so its entries are already in hand — no extra I/O.
 *
 * Extensible by design: add a provider to the list below. Actions carry a `kind`
 * the app knows how to run, so providers contribute behaviour, not just labels.
 */
import type { DirEntry } from "./types";

export type FolderActionKind = "open-path" | "open-github";

export interface FolderAction {
  id: string;
  label: string;
  kind: FolderActionKind;
  /** File/folder path (open-path) or repo folder path (open-github). */
  target: string;
}

export interface FolderContext {
  id: string;
  label: string;
  icon: string;
  detail?: string;
  actions: FolderAction[];
}

export interface FolderProbe {
  path: string;
  entries: DirEntry[];
}

const hasDir = (es: DirEntry[], name: string) =>
  es.some((e) => e.is_dir && e.name.toLowerCase() === name);
const fileEntry = (es: DirEntry[], name: string) =>
  es.find((e) => !e.is_dir && e.name.toLowerCase() === name);
const byExt = (es: DirEntry[], ext: string) =>
  es.find((e) => !e.is_dir && e.name.toLowerCase().endsWith(ext));

type Provider = (p: FolderProbe) => FolderContext | null;

const PROVIDERS: Provider[] = [
  // Git / GitHub repository — the original "Repository" idea.
  (p) =>
    hasDir(p.entries, ".git")
      ? {
          id: "git",
          label: "Git repository",
          icon: "code",
          actions: [{ id: "git-open", label: "Open on GitHub", kind: "open-github", target: p.path }],
        }
      : null,

  // Visual Studio solution.
  (p) => {
    const sln = byExt(p.entries, ".sln");
    return sln
      ? {
          id: "vs",
          label: "Visual Studio solution",
          icon: "code",
          detail: sln.name,
          actions: [{ id: "vs-open", label: "Open solution", kind: "open-path", target: sln.path }],
        }
      : null;
  },

  // Web site / page.
  (p) => {
    const idx = fileEntry(p.entries, "index.html");
    return idx
      ? {
          id: "web",
          label: "Web page",
          icon: "web",
          actions: [{ id: "web-open", label: "Open in browser", kind: "open-path", target: idx.path }],
        }
      : null;
  },

  // Node project.
  (p) => {
    const pkg = fileEntry(p.entries, "package.json");
    return pkg
      ? {
          id: "node",
          label: "Node project",
          icon: "code",
          actions: [{ id: "node-open", label: "Open package.json", kind: "open-path", target: pkg.path }],
        }
      : null;
  },

  // Rust crate.
  (p) => {
    const cargo = fileEntry(p.entries, "cargo.toml");
    return cargo
      ? {
          id: "rust",
          label: "Rust crate",
          icon: "code",
          actions: [{ id: "rust-open", label: "Open Cargo.toml", kind: "open-path", target: cargo.path }],
        }
      : null;
  },

  // Python project.
  (p) => {
    const py = fileEntry(p.entries, "pyproject.toml") ?? fileEntry(p.entries, "requirements.txt");
    return py
      ? {
          id: "python",
          label: "Python project",
          icon: "code",
          detail: py.name,
          actions: [{ id: "py-open", label: `Open ${py.name}`, kind: "open-path", target: py.path }],
        }
      : null;
  },
];

/** Aggregate every context a folder matches (empty for plain folders). */
export function detectContexts(probe: FolderProbe): FolderContext[] {
  return PROVIDERS.map((f) => f(probe)).filter((c): c is FolderContext => c !== null);
}
