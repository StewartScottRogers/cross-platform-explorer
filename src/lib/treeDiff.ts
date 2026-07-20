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
