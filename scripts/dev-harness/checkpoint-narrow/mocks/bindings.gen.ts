// Dev-only mock for `src/lib/bindings.gen.ts`, used ONLY by the CPE-1635 narrow-width verification
// harness (scripts/dev-harness/checkpoint-narrow). Supplies just the `commands` surface
// CheckpointDialog.svelte calls, with canned data covering both a real checkpoint row (CPE-1125) and a
// failed pre-write attempt row (CPE-1600) — the two shapes the ticket asks to confirm both read
// correctly at 420px in both themes. Types are `import type`-only in the component, so they're erased
// before bundling and don't need mirroring here.

type Ok<T> = { status: "ok"; data: T };

const now = Date.now();

const checkpoints = [
  {
    manifest_id: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6",
    label: "Before refactor of the metadata pipeline",
    ts: now - 5 * 60_000,
  },
  {
    manifest_id: "9f8e7d6c5b4a39281706f5e4d3c2b1a0",
    label: "",
    ts: now - 60 * 60_000,
  },
];

const checkpointFailures = [
  {
    operation: "Before batch media overwrite",
    reason: "disk quota exceeded while staging the snapshot manifest",
    ts: now - 2 * 60_000,
  },
];

export const commands = {
  async checkpointList(_root: string): Promise<Ok<typeof checkpoints>> {
    return { status: "ok", data: checkpoints };
  },
  async checkpointFailuresList(_root: string): Promise<Ok<typeof checkpointFailures>> {
    return { status: "ok", data: checkpointFailures };
  },
  async checkpointCreate(_root: string, label: string) {
    return {
      status: "ok" as const,
      data: {
        checkpoint: { manifest_id: "new-manifest-id", label, ts: Date.now() },
        new_blobs: 3,
        reused_blobs: 1,
        added_bytes: 4096,
        skipped: [],
      },
    };
  },
  async checkpointPreviewRevert(_root: string, _manifestId: string, _sessionId: string | null) {
    return {
      status: "ok" as const,
      data: {
        creates: 1,
        overwrites: 2,
        deletes: 0,
        bytes_written: 20480,
        drift_count: 1,
        drift_paths: ["src/example/drifted-file.ts"],
      },
    };
  },
  async checkpointDiffFile(_root: string, _manifestId: string, _relPath: string) {
    return { status: "ok" as const, data: { before: "old\n", after: "new\n" } };
  },
  async checkpointRevertOne(_root: string, _manifestId: string, _relPath: string) {
    return { status: "ok" as const, data: { applied: 1, skipped: [] } };
  },
  async checkpointRevert(_root: string, _manifestId: string) {
    return { status: "ok" as const, data: { applied: 3, skipped: [] } };
  },
};
