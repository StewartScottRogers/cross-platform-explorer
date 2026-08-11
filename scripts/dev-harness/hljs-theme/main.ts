// CPE-1631 hljs theme verification harness — script for index.html. Sets `data-theme` the same way
// src/lib/theme.ts does in the real app, then runs the REAL highlight.ts against two representative
// samples (a TypeScript file for the plain code preview, a Python snippet for a notebook code cell)
// and injects the output into the real component markup, so a screenshot shows exactly what a user
// would see.
import {
  ensureLanguageForName,
  highlightForFile,
  ensureLanguage,
  highlightCode,
} from "../../../src/lib/preview/highlight";

const params = new URLSearchParams(location.search);
const theme = params.get("theme") === "dark" ? "dark" : "light";
document.documentElement.dataset.theme = theme;

// TypeScript sample exercising: keyword/selector-tag (import, interface, const, class, constructor,
// private, readonly, async, for, let, try, return, await, catch, throw, new, export), title (class
// name, function/method names), string + template literal, number, comment (line + block/doc),
// built_in/type (Promise, string, number, boolean), attribute-ish class fields.
const TS_SAMPLE = `// A representative snippet exercising most highlight.js token classes.
import { readFileSync } from "node:fs";

/** Doc comment: retry options for Fetcher. */
interface Options {
  verbose?: boolean;
  retries: number;
}

const DEFAULT_RETRIES: number = 3;

export class Fetcher {
  constructor(private readonly url: string, private opts: Options = { retries: DEFAULT_RETRIES }) {}

  async run(): Promise<string> {
    for (let i = 0; i < this.opts.retries; i++) {
      try {
        return await this.attempt();
      } catch (err) {
        console.warn(\`attempt \${i} failed:\`, err);
      }
    }
    throw new Error("out of retries");
  }
}
`;

// Python sample — a common notebook-cell language, exercising the same class families via a
// different grammar (def/class/import/for/if keywords, f-string, decorator, comment, number).
const PY_SAMPLE = `# Notebook code cell sample
import numpy as np

class Model:
    """Docstring: a tiny linear model."""

    def __init__(self, weight: float = 0.5):
        self.weight = weight

    def predict(self, x):
        # elementwise scale
        return np.asarray(x) * self.weight

m = Model(weight=1.25)
print(f"prediction: {m.predict([1, 2, 3])}")
`;

async function renderCodePreview() {
  // NOTE: PreviewPane.svelte's real `.cl-row`/`.preview-text`/`.code-view` layout rules live inside
  // that component's Svelte `<style>` block, which the Svelte compiler scopes to elements it renders
  // itself (a hash class on both selector and DOM node) — they intentionally do NOT apply to markup
  // built by hand outside a mounted Svelte component, so this harness doesn't attempt to reproduce
  // that scoped layout. What IS global (unscoped, in src/app.css, exactly what this ticket adds) is
  // the `.hljs-*` colour rule set — the thing actually under test — so a flat, wrapped block is
  // enough to see it apply for real. `splitHighlightedIntoLines` (used by the real component for its
  // per-row gutter/fold layout) isn't needed to prove the colours render.
  const el = document.getElementById("code-preview")!;
  await ensureLanguageForName("sample.ts");
  el.innerHTML = highlightForFile(TS_SAMPLE, "sample.ts");
}

async function renderNotebookCell() {
  const el = document.getElementById("nb-cell")!;
  await ensureLanguage("python");
  el.innerHTML = highlightCode(PY_SAMPLE, "python");
}

// CPE-1631 PR review round 2: a JSON sample (hljs-attr keys + hljs-punctuation {}:,[] — the two
// classes that measured 0% coverage before this fix) and a Svelte-shaped XML sample (hljs-attr
// attribute names on a real in-repo component tag), rendered the same way as the code preview panel.
const JSON_SAMPLE = `{
  "name": "cross-platform-explorer",
  "$comment": "attr keys + punctuation, both previously uncovered",
  "version": "0.57.64",
  "retries": 3,
  "verbose": false,
  "tags": ["ready", "big-design"]
}
`;
const SVELTE_SAMPLE = `<span class="link-badge" class:broken={isBroken} title={label}>
  <a href={url} target="_blank" rel="noreferrer">{label}</a>
</span>
`;

async function renderJson() {
  const el = document.getElementById("json-sample")!;
  await ensureLanguageForName("sample.json");
  el.innerHTML = highlightForFile(JSON_SAMPLE, "sample.json");
}
async function renderSvelte() {
  const el = document.getElementById("svelte-sample")!;
  await ensureLanguageForName("sample.svelte");
  el.innerHTML = highlightForFile(SVELTE_SAMPLE, "sample.svelte");
}

await Promise.all([renderCodePreview(), renderNotebookCell(), renderJson(), renderSvelte()]);
(window as unknown as { __hljsHarnessReady?: boolean }).__hljsHarnessReady = true;
