// {ask:label} prompt-parameter resolution (CPE-1190 UI half, epic CPE-739). The backend model
// (`crates/server/src/action_macro.rs`) already defines the `{ask:label}` token and a pure
// `plan_with_params` substitution, but the exposed `macro_plan`/`macro_run` Tauri commands (and the
// CPE-1187 executor, `macro_run::resolve`, that backs `macro_run`) don't yet accept a params map —
// see that file's CPE-1190 work log. Rather than touch the backend on this frontend-only branch, this
// module performs the SAME substitution client-side: given a macro plus a resolved `label -> value`
// map (collected via `MacroParamPrompt.svelte`), it returns a NEW `ActionMacro` with every
// `{ask:label}` occurrence in a step's string field replaced by `params[label]` — dropped (replaced
// with `""`) when the label wasn't answered, exactly mirroring the backend's "absent param resolves
// to nothing, never a panic, never a literal `{ask:...}` leaking through" rule. The result is what
// gets sent to `commands.macroPlan`/`commands.macroRun`, so a macro with no `{ask:...}` tokens at all
// round-trips byte-for-byte unchanged (no behaviour change for every macro predating CPE-1190).
import type { ActionMacro, MacroStep } from "./bindings.gen";

const ASK_TOKEN = /\{ask:([^}]+)\}/g;

function stepStrings(step: MacroStep): string[] {
  if ("rename" in step) return [step.rename.template];
  if ("move" in step) return [step.move.dest];
  if ("tag" in step) return [step.tag.label];
  if ("convert" in step) return [step.convert.to_ext];
  return [];
}

function substituteStep(step: MacroStep, params: Record<string, string>): MacroStep {
  const sub = (s: string) => s.replace(ASK_TOKEN, (_all, label: string) => params[label] ?? "");
  if ("rename" in step) return { rename: { template: sub(step.rename.template) } };
  if ("move" in step) return { move: { dest: sub(step.move.dest) } };
  if ("tag" in step) return { tag: { label: sub(step.tag.label) } };
  if ("convert" in step) return { convert: { to_ext: sub(step.convert.to_ext) } };
  return step;
}

/** Every distinct `{ask:label}` label referenced across `macro`'s steps, in first-appearance order. */
export function extractAskLabels(macro: ActionMacro): string[] {
  const seen: string[] = [];
  for (const step of macro.steps) {
    for (const text of stepStrings(step)) {
      for (const m of text.matchAll(ASK_TOKEN)) {
        const label = m[1];
        if (!seen.includes(label)) seen.push(label);
      }
    }
  }
  return seen;
}

/** True when the macro has at least one `{ask:label}` token anywhere in its steps — the run flow
 *  (CPE-1191) uses this to decide whether `MacroParamPrompt` must run before the dry-run/confirm. */
export function hasAskParams(macro: ActionMacro): boolean {
  return extractAskLabels(macro).length > 0;
}

/** Substitute every `{ask:label}` in `macro`'s steps with `params[label]` (dropped — `""` — if
 *  absent), returning a NEW `ActionMacro`. See module doc for why this happens client-side. */
export function resolveAskParams(macro: ActionMacro, params: Record<string, string>): ActionMacro {
  return { ...macro, steps: macro.steps.map((s) => substituteStep(s, params)) };
}
