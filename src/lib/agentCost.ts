import { writable, type Readable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";

/**
 * Live per-session usage store for the Agent Watch cost ledger (CPE-1098).
 *
 * The host bridges the sidecar's best-effort PTY usage scrape (CPE-1097) as an
 * `ai-console://agent-cost` event carrying `{sessionId, inputTokens, outputTokens, costUsd}`; here we
 * fold the latest figures per session for the ledger tab to render. This is NOT the richer
 * `SessionMetrics` ledger (wall-clock, churn, per-model rollup, budget) — that stays unwired dead code
 * (see `.claude/research-library/entries/sidecar-live-cost-usage-scanner.md`); these are advisory,
 * best-effort figures scraped from the agent's own printed output, never billing. Strictly additive +
 * idle-by-default: the store is empty and no listener runs until watching starts, and it clears on
 * stop (AGENT-WATCH.md: "off means off"). The event payload isn't in `bindings.gen.ts` (it's emitted
 * ad hoc from the reader thread, like `fs-activity`/`session`), so the type is hand-written here to
 * mirror `agentActivity.ts`/`agentDiffs.ts`.
 */

/** One session's latest live usage snapshot. Advisory — best-effort text-scraped, not billing. */
export interface AgentCost {
  sessionId: string;
  inputTokens: number;
  outputTokens: number;
  costUsd: number;
}

const store = writable<Record<string, AgentCost>>({});

/** Reactive per-session usage map (keyed by sessionId). Empty when not watching / no usage reported. */
export const agentCost: Readable<Record<string, AgentCost>> = store;

/** Normalize a raw `ai-console://agent-cost` payload into a clean `AgentCost`, or `null` if malformed
 *  (missing/wrong-typed field, non-finite number). Pure so the wire format is unit-testable headlessly
 *  (sibling of `normalizeFsDiff`). */
export function normalizeAgentCost(payload: unknown): AgentCost | null {
  const p = payload as
    | { sessionId?: unknown; inputTokens?: unknown; outputTokens?: unknown; costUsd?: unknown }
    | null
    | undefined;
  const sessionId = p?.sessionId;
  const inputTokens = p?.inputTokens;
  const outputTokens = p?.outputTokens;
  const costUsd = p?.costUsd;
  if (
    typeof sessionId !== "string" ||
    !sessionId ||
    typeof inputTokens !== "number" ||
    !Number.isFinite(inputTokens) ||
    typeof outputTokens !== "number" ||
    !Number.isFinite(outputTokens) ||
    typeof costUsd !== "number" ||
    !Number.isFinite(costUsd)
  ) {
    return null;
  }
  return { sessionId, inputTokens, outputTokens, costUsd };
}

/** Fold one event into the previous per-session map (pure): latest snapshot per `sessionId` wins. */
export function foldCost(
  prev: Record<string, AgentCost>,
  event: AgentCost,
): Record<string, AgentCost> {
  return { ...prev, [event.sessionId]: event };
}

/** input + output tokens, NaN/negative-safe (a malformed field never renders `NaN` in the ledger). */
export function totalTokens(c: Pick<AgentCost, "inputTokens" | "outputTokens">): number {
  const input = Number.isFinite(c.inputTokens) && c.inputTokens > 0 ? c.inputTokens : 0;
  const output = Number.isFinite(c.outputTokens) && c.outputTokens > 0 ? c.outputTokens : 0;
  return input + output;
}

/** A token count with thousands separators, NaN/negative-safe (renders "0" rather than "NaN"/"-5"). */
export function formatTokens(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "0";
  return Math.trunc(n).toLocaleString();
}

/** A USD amount to 4dp with a leading `$`, NaN/negative-safe (renders "$0.0000" rather than "$NaN"). */
export function formatUsd(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "$0.0000";
  return `$${n.toFixed(4)}`;
}

/** Fold a raw `ai-console://agent-cost` payload into the store (exposed for headless tests). */
export function ingestCost(payload: unknown): void {
  const event = normalizeAgentCost(payload);
  if (event) store.update((prev) => foldCost(prev, event));
}

/** Test/introspection helper: the current per-session usage map synchronously. */
export function currentCost(): Record<string, AgentCost> {
  let snapshot: Record<string, AgentCost> = {};
  store.subscribe((v) => (snapshot = v))();
  return snapshot;
}

/** Clear the cost store (on stop-watching), so a stale session's figures never bleed into a new one. */
export function clearAgentCost(): void {
  store.set({});
}

/**
 * Start consuming `ai-console://agent-cost` events. Returns a teardown that unlistens and clears the
 * store. Call when watching begins; call the teardown when it ends (mirrors `initAgentDiffs`).
 */
export async function initAgentCost(): Promise<() => void> {
  const unlisten = await listen("ai-console://agent-cost", (e) => ingestCost(e.payload));
  return () => {
    unlisten();
    clearAgentCost();
  };
}
