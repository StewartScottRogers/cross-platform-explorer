# Adding an AI model reseller (CPE-467 / CPE-479)

A **reseller** is an OpenRouter-like gateway: one API key fronting many models over a
protocol-compatible endpoint. The Agent Deck treats resellers as **data** — adding one is (almost)
just a manifest, no host or per-agent code. This guide shows exactly what to touch.

There are two capabilities a reseller can have, and you opt into each with data:

1. **Model listing** — the model picker enumerates the reseller's `/models`. Needs a
   `models_endpoint` + a host egress allow-list entry.
2. **Launching** — an agent runs against the reseller as its provider. Needs `protocol` +
   `launch_base_url`, and the agent must declare a matching `reseller_recipes[protocol]`.

## 1. The reseller manifest — `sidecar/ai-console/resellers/<id>.json`

```json
{
  "schema_version": 1,
  "id": "acme",
  "name": "Acme AI",
  "models_endpoint": "https://api.acme.example/v1/models",
  "auth": "bearer",
  "api_hosts": ["api.acme.example"],
  "normalizer": "openai",
  "web_base": "https://api.acme.example",
  "protocol": "openai",
  "launch_base_url": "https://api.acme.example/v1"
}
```

- **`normalizer`** — one of `openrouter` | `openai` | `github`. Most OpenAI-compatible resellers use
  `"openai"` (the OpenRouter parser is a superset, so `"openrouter"` also works).
- **`api_hosts`** — every host the reseller uses. **The conformance test requires the
  `models_endpoint` and `launch_base_url` hosts to be listed here** (`tests/reseller_conformance.rs`).
- **`protocol` + `launch_base_url`** — OPTIONAL. Omit both for a **model-list-only** reseller. Present
  = launch-capable: `protocol` is `openai` or `anthropic`, `launch_base_url` fills `{base_url}` in the
  agent's reseller recipe. (Don't half-declare — the conformance test rejects protocol XOR base_url.)

## 2. The host egress allow-list — `src-tauri/src/models_egress.rs`

The host, **not** the sidecar manifest, is the authoritative SSRF boundary — so the model-list
endpoint must be added by hand to `models_endpoint()`:

```rust
} else if is("acme") {
    bearer("https://api.acme.example/v1/models")
}
```

Add `"acme"` to the `every_advertised_reseller_resolves_and_uses_https` test list too. If the reseller
needs constant headers (like `github-models`' API-version), return a full `ModelsEndpoint { extra }`.

## 3. Wire-up lists + tests

- Add the id to `KNOWN_RESELLERS` in `sidecar/ai-console/src/console.rs` (so the Keys panel offers a
  key slot).
- If launch-capable, add the id to the bundled-descriptors assertion in
  `sidecar/ai-console/src/model_catalog.rs` (`the_bundled_resellers_expose_launch_descriptors`).
- The conformance kit (`tests/reseller_conformance.rs`) picks the new manifest up automatically.

## 4. Agent side (launch-capable resellers)

A reseller only *launches* an agent if that agent declares how it speaks the protocol, in its
`agents/<id>.json`:

```json
"reseller_recipes": {
  "openai": {
    "env": { "OPENAI_API_KEY": "{api_key}", "OPENAI_BASE_URL": "{base_url}", "OPENAI_MODEL": "{model}" },
    "args": ["--model", "{model}"]
  }
}
```

At launch, `{base_url}` is filled from the selected reseller's `launch_base_url`, `{api_key}` from the
stored reseller key. The launcher shows every reseller whose protocol the agent speaks as a
"<name> (reseller)" provider option (see `renderProviders` + `/api/catalog`'s `resellers` /
`resellerProtocols`).

## 5. Verify

```
cargo test -p ai-console model_catalog reseller_conformance
cd src-tauri && cargo test --features sidecar-platform models_egress
cargo clippy --all-targets -- -D warnings        # both crates; host in both feature modes
```

## When to skip a reseller

If a gateway has **no public `/models` list** (e.g. Perplexity) or needs a per-request **config
header/virtual-key** to enumerate models (e.g. Portkey, Unify), don't add it with the uniform
bearer-`/models` pattern — it would ship a broken picker. Those need a bespoke mechanism; leave them
out rather than half-working.
