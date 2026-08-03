---
title: AI File Copilot
order: 21
category: Explorer
categoryOrder: 2
---

# AI File Copilot

The **AI file copilot** turns a plain-language instruction for the folder you're viewing — "archive the
old screenshots", "put these into folders by file type" — into a concrete, reviewable plan, which you
confirm before anything on disk changes. Open it from the **command palette** (Ctrl/Cmd+K → *AI copilot —
organize this folder…*).

## Setting it up

The copilot needs a model endpoint before it can produce a plan. Open **Settings** (command palette →
*Settings*) and find the **AI file copilot** section:

- **Enable the AI file copilot** — off by default; nothing runs until you turn this on.
- **Endpoint URL** — any OpenAI-compatible **chat** endpoint. A local server such as
  [LM Studio](https://lmstudio.ai) needs no key (`http://localhost:1234/v1`); a hosted service like
  OpenAI needs the URL plus an API key.
- **Model** — the chat model name the server expects.
- **API key** — write-only: it's saved straight to your OS keychain and never shown again, only a
  "(saved)" indicator. Leave it blank for a local server.
- **Test connection** — sends a trivial planning request and confirms the endpoint answers with a
  parseable plan (or reports a clear reason it can't: unreachable, bad key, bad response).

If you open the copilot before it's configured, it shows a **"set it up in Settings"** prompt instead of
an error, with a button straight to this section.

## The plan — preview, then confirm

1. Type your instruction for the current folder and click **Plan**. The copilot lists the folder, asks
   the model for a plan, and validates it — nothing on disk changes yet.
2. The plan preview shows:
   - **Counts** — how many moves, renames, deletes, new folders, and copies the plan contains.
   - **The ordered operation list** — every op it will run, in order, with its kind and (for
     moves/renames/copies) the from → to paths.
3. If the plan isn't safe — it tries to touch something outside the folder, for example — the preview
   shows the **violations** instead of an op list, and there is **no Confirm button**. Nothing unsafe is
   ever offered for execution.
4. If the plan looks right, click **Confirm — run this plan**. This is the one deliberate action that
   runs anything; the copilot never executes a plan on its own.

The operation vocabulary is a **closed, whitelisted set** — move, rename, delete, mkdir, copy — so there
is no free-form command execution hiding behind the instruction.

## Running it — and undoing it

On Confirm, the backend re-validates the plan (in case anything about the folder changed since the
preview), takes a **checkpoint** of the folder, then applies each operation, skipping past any that fail
rather than aborting the rest. Afterwards you see:

- **Per-operation results** — which succeeded and which failed (with the reason).
- **Undo** — reverts the whole run using the checkpoint taken just before it started.

Any deletes the plan made went to the OS trash / Recycle Bin, not permanent deletion — mentioned right in
the results — so even without using Undo, a mistaken delete is recoverable the normal way.

## What this is (and isn't)

Like [Checkpoints & Rollback](16-checkpoints), this is the safety-first, palette-driven surface over the
copilot's command layer. The quality of the plan depends entirely on the model behind your configured
endpoint — a small local model may propose a poorer plan than a larger hosted one — but *safety* doesn't:
every plan, from any model, passes through the same scope + whitelist validation before it's ever offered,
and again before it runs.
