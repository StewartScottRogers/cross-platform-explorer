// Pure recursive folder-tree diff (CPE-777, epic CPE-722). Compare two directory trees and classify every
// node as added / removed / changed / identical — no DOM/IO, unit-tested — so the folder-compare view
// (CPE-779) is a thin render. Files are compared by size + mtime (no content hash, per the epic); a
// directory is `changed` iff any descendant differs, else `identical`.

/** A node the caller builds from a backend listing (children present for dirs). */
export interface CompareNode {
  name: string;
  isDir: boolean;
  size?: number;
  modified?: number | null;
  children?: CompareNode[];
}

export type DiffStatus = "added" | "removed" | "changed" | "identical";

/** A classified node in the diff tree. `left`/`right` echo the inputs where present. */
export interface DiffNode {
  name: string;
  isDir: boolean;
  status: DiffStatus;
  children?: DiffNode[];
}

/** Dirs first, then case-sensitive name — a stable, predictable order for the compare view. */
function ordered(nodes: DiffNode[]): DiffNode[] {
  return nodes.sort((a, b) => Number(b.isDir) - Number(a.isDir) || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
}

function fileChanged(a: CompareNode, b: CompareNode): boolean {
  return (a.size ?? 0) !== (b.size ?? 0) || (a.modified ?? null) !== (b.modified ?? null);
}

/** Mark a whole subtree with a single status (for added/removed dirs — everything under them shares it). */
function markSubtree(node: CompareNode, status: "added" | "removed"): DiffNode {
  const out: DiffNode = { name: node.name, isDir: node.isDir, status };
  if (node.isDir) {
    out.children = ordered((node.children ?? []).map((c) => markSubtree(c, status)));
  }
  return out;
}

/**
 * Diff two lists of sibling nodes, matched by name. Returns the classified children in a stable order.
 * Pure and recursive.
 */
export function diffTrees(left: CompareNode[], right: CompareNode[]): DiffNode[] {
  const byName = (list: CompareNode[]) => new Map(list.map((n) => [n.name, n]));
  const L = byName(left);
  const R = byName(right);
  const names = new Set<string>([...L.keys(), ...R.keys()]);
  const out: DiffNode[] = [];

  for (const name of names) {
    const l = L.get(name);
    const r = R.get(name);

    if (l && !r) {
      out.push(markSubtree(l, "removed"));
    } else if (!l && r) {
      out.push(markSubtree(r, "added"));
    } else if (l && r) {
      if (l.isDir !== r.isDir) {
        // file-vs-dir at the same name — a type change.
        out.push({ name, isDir: r.isDir, status: "changed" });
      } else if (l.isDir) {
        const children = diffTrees(l.children ?? [], r.children ?? []);
        const changed = children.some((c) => c.status !== "identical");
        out.push({ name, isDir: true, status: changed ? "changed" : "identical", children });
      } else {
        out.push({ name, isDir: false, status: fileChanged(l, r) ? "changed" : "identical" });
      }
    }
  }

  return ordered(out);
}

/** Per-status file counts for a diff, for the compare header. */
export interface DiffSummary {
  added: number;
  removed: number;
  changed: number;
  identical: number;
}

/**
 * Count the **leaves** of a diff tree by status for the compare header ("12 added, 3 changed…"). A dir with
 * children is a container — it contributes its descendants (which `diffTrees` already marked), not itself —
 * but a *childless* node counts as one: a plain file, an empty added/removed folder, or a file↔dir **type
 * change** (which `diffTrees` emits as `changed` with no children). Pure and recursive.
 */
export function summarizeDiff(nodes: DiffNode[]): DiffSummary {
  const s: DiffSummary = { added: 0, removed: 0, changed: 0, identical: 0 };
  const walk = (list: DiffNode[]): void => {
    for (const n of list) {
      if (n.isDir && n.children && n.children.length > 0) walk(n.children);
      else s[n.status] += 1; // leaf: file, empty dir, or type-change node
    }
  };
  walk(nodes);
  return s;
}

/** One flattened, indented row for the (virtualized) tree-compare view. */
export interface DiffRow {
  node: DiffNode;
  /** Nesting depth; roots are 0. */
  depth: number;
  /** `/`-joined path from the root, used as the collapse key and a stable row id. */
  path: string;
  /** A dir with at least one child (so it can be collapsed/expanded). */
  hasChildren: boolean;
}

/**
 * Flatten a diff tree to the rows a tree view renders top-to-bottom. A directory whose `path` is in
 * `collapsed` still yields its own row but not its descendants (so the view can hide subtrees without
 * rebuilding the diff). Pure and recursive.
 */
export function flattenDiff(
  nodes: DiffNode[],
  collapsed: Set<string> = new Set(),
  prefix = "",
  depth = 0,
): DiffRow[] {
  const rows: DiffRow[] = [];
  for (const n of nodes) {
    const path = prefix ? `${prefix}/${n.name}` : n.name;
    const hasChildren = n.isDir && (n.children?.length ?? 0) > 0;
    rows.push({ node: n, depth, path, hasChildren });
    if (hasChildren && !collapsed.has(path)) {
      rows.push(...flattenDiff(n.children ?? [], collapsed, path, depth + 1));
    }
  }
  return rows;
}
