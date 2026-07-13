# Adding a coding agent, provider, or plugin — no code required

The AI Console is **CLI-agnostic and manifest-extensible**: agents, providers, and
plugins are described by declarative JSON that the console loads at runtime. Adding one
is dropping a file into your user registry — no rebuild. This guide shows how.

## Where files go

The console loads manifests from a **bundled** directory (ships with the app) and a
**user** directory (yours). The user directory overrides the bundled one by `id`, so you
can add new agents or tweak existing ones. Drop `*.json` files there; malformed or
incompatible files are skipped with a logged reason (they never break the others).

## 1. Add an agent

An agent manifest describes how to detect / install / update / uninstall / run a CLI,
per OS, plus which providers it supports. Example `my-agent.json`:

```json
{
  "schema_version": 1,
  "id": "my-agent",
  "name": "My Agent",
  "detect":    { "windows": { "command": "myagent", "args": ["--version"] },
                 "macos":   { "command": "myagent", "args": ["--version"] },
                 "linux":   { "command": "myagent", "args": ["--version"] } },
  "install":   { "windows": { "command": "npm", "args": ["i", "-g", "my-agent@latest"] },
                 "macos":   { "command": "npm", "args": ["i", "-g", "my-agent@latest"] },
                 "linux":   { "command": "npm", "args": ["i", "-g", "my-agent@latest"] } },
  "uninstall": { "windows": { "command": "npm", "args": ["rm", "-g", "my-agent"] },
                 "macos":   { "command": "npm", "args": ["rm", "-g", "my-agent"] },
                 "linux":   { "command": "npm", "args": ["rm", "-g", "my-agent"] } },
  "run":       { "windows": { "command": "myagent" },
                 "macos":   { "command": "myagent" },
                 "linux":   { "command": "myagent" } },
  "providers": ["native"],
  "provider_recipes": {
    "native": { "env": {}, "args": [] }
  },
  "default_model": "my-default-model"
}
```

- **`detect`** must exit 0 when installed (its first stdout line is shown as the version).
- **`install`/`update`/`uninstall`** run the platform's package manager — the console
  orchestrates it in Rust; no shell scripts. `update` falls back to `install` if omitted.
- Every id you list in **`providers`** must have a matching entry in
  **`provider_recipes`** (see below), or the console will report it can't route there.

## 2. Add a provider recipe

A provider recipe composes the launch **environment + args** for an agent, as templates
with `{model}`, `{small_model}`, `{api_key}`, `{base_url}` placeholders. A referenced
value that isn't supplied is a loud error (so you never launch unauthenticated). Example
adding OpenRouter to an Anthropic-compatible agent:

```json
"provider_recipes": {
  "native": { "env": {}, "args": ["--model", "{model}"] },
  "openrouter": {
    "env": {
      "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
      "ANTHROPIC_AUTH_TOKEN": "{api_key}",
      "ANTHROPIC_SMALL_FAST_MODEL": "{small_model}"
    },
    "args": ["--model", "{model}"]
  }
}
```

The `{api_key}` comes from your **credential profile** (a named set of ENV-VAR → vault-key
references; values live only in the OS keychain, never in the profile or a log).

## 3. Add a plugin

A plugin (e.g. an MCP server) extends every agent that `supports` it. Installing it fans
across all supporting installed agents. Example `cipher-mcp.json`:

```json
{
  "schema_version": 1,
  "id": "cipher-mcp",
  "name": "Cipher (memory)",
  "kind": "mcp-server",
  "description": "Memory layer for coding agents.",
  "supports": ["claude", "codex", "gemini"]
}
```

## Worked example, end to end

1. Save the agent JSON from step 1 into your user registry directory.
2. Reopen the console → **My Agent** appears; if it isn't installed, click Install
   (runs your `install` recipe).
3. Pick **My Agent × native × your model** and launch — the console composes the env from
   the recipe and opens a terminal session in the repo you have open.
4. No app rebuild was needed at any point.

## Validate it

The console skips a bad manifest rather than failing, so check the console's diagnostics
for a skip reason if your agent doesn't appear. The bundled catalog under
`sidecar/ai-console/agents/` is a set of working examples to copy from.
