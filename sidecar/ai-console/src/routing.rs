//! Provider / model routing engine (CPE-285).
//!
//! Composes the concrete launch environment for *(agent × provider × model)* from the
//! declarative [`ProviderRecipe`]s in an agent's manifest. This is the "any provider,
//! any model" core: native vendor endpoint, OpenRouter, LM Studio, etc. are just
//! recipes of env-var templates — no per-agent code. Ported from the reference's
//! `*--openrouter.cmd` / `*--lmstudio.cmd` launchers.

use std::collections::BTreeMap;

use crate::agents::AgentManifest;

/// The values available to fill a recipe's `{placeholder}`s.
#[derive(Debug, Clone, Default)]
pub struct LaunchContext {
    pub model: Option<String>,
    pub small_model: Option<String>,
    /// Provider API key, resolved from the secret vault (never logged).
    pub api_key: Option<String>,
    /// Provider base URL (e.g. an auto-detected LM Studio endpoint).
    pub base_url: Option<String>,
}

/// A fully-resolved launch: the environment to set on the child, plus extra run args.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub env: BTreeMap<String, String>,
    pub args: Vec<String>,
}

/// Compose the launch env/args for running `agent` against `provider`. Errors if the
/// agent doesn't support the provider, has no recipe for it, or a template references a
/// value not present in `ctx` (e.g. an API key the provider requires).
pub fn compose_launch(
    agent: &AgentManifest,
    provider: &str,
    ctx: &LaunchContext,
) -> Result<Launch, String> {
    if !agent.supports_provider(provider) {
        return Err(format!("agent '{}' does not support provider '{provider}'", agent.id));
    }
    let recipe = agent
        .provider_recipes
        .get(provider)
        .ok_or_else(|| format!("agent '{}' has no recipe for provider '{provider}'", agent.id))?;
    // The `native` provider is the agent's own login (e.g. Claude Code): it wants a bare model
    // alias/id, not an OpenRouter-style `vendor/model` — which it rejects ("model may not exist").
    // Normalise so a model picked from the shared catalog still launches natively (CPE-589).
    if provider == "native" {
        let ctx = LaunchContext { model: ctx.model.as_deref().map(native_model), ..ctx.clone() };
        return apply_recipe(recipe, &ctx);
    }
    apply_recipe(recipe, ctx)
}

/// Map a model id to a value the **native** provider accepts. An OpenRouter-style `vendor/model`
/// (e.g. `anthropic/claude-opus-4.8`) is reduced to a family alias (`opus`/`sonnet`/`haiku`) — which the
/// native CLI resolves to its latest — falling back to the part after the slash. Bare/native ids (no
/// slash) pass through unchanged (CPE-589).
fn native_model(model: &str) -> String {
    let Some((_, rest)) = model.split_once('/') else { return model.to_string() };
    let lower = rest.to_ascii_lowercase();
    for family in ["opus", "sonnet", "haiku"] {
        if lower.contains(family) {
            return family.to_string();
        }
    }
    rest.to_string()
}

/// A reseller / aggregator gateway (OpenRouter-like) described as **data** (CPE-468): one API key
/// fronting many models over a protocol-compatible endpoint. Adding a reseller is a descriptor, not
/// per-agent code — an agent that declares a `reseller_recipes` entry for `protocol` can target it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResellerDescriptor {
    pub id: String,
    pub name: String,
    /// The API dialect this reseller speaks — matched against an agent's `reseller_recipes` keys.
    pub protocol: String,
    /// The base URL fed to the agent's reseller recipe as `{base_url}`.
    pub base_url: String,
}

/// Compose a launch for `agent` against `reseller` using the agent's generic reseller recipe for the
/// reseller's protocol (CPE-468). The reseller's `base_url` fills `{base_url}` (selecting a reseller
/// *means* using its endpoint, so it wins over any default/caller value); the caller supplies the key
/// + model in `ctx`. Errors if the agent doesn't speak the reseller's protocol.
pub fn compose_reseller_launch(
    agent: &AgentManifest,
    reseller: &ResellerDescriptor,
    ctx: &LaunchContext,
) -> Result<Launch, String> {
    let recipe = agent.reseller_recipes.get(&reseller.protocol).ok_or_else(|| {
        format!("agent '{}' can't use a '{}'-protocol reseller ({})", agent.id, reseller.protocol, reseller.id)
    })?;
    let ctx = LaunchContext { base_url: Some(reseller.base_url.clone()), ..ctx.clone() };
    apply_recipe(recipe, &ctx)
}

/// Fill a recipe's env + arg templates from `ctx`, backfilling unsupplied placeholders from the
/// recipe's defaults (CPE-328). A caller-supplied value always wins; `api_key` has no default (it's a
/// secret, never baked into a manifest). Shared by the provider + reseller launch paths.
fn apply_recipe(recipe: &crate::agents::ProviderRecipe, ctx: &LaunchContext) -> Result<Launch, String> {
    let d = &recipe.defaults;
    let ctx = LaunchContext {
        model: ctx.model.clone().or_else(|| d.model.clone()),
        small_model: ctx.small_model.clone().or_else(|| d.small_model.clone()),
        base_url: ctx.base_url.clone().or_else(|| d.base_url.clone()),
        api_key: ctx.api_key.clone(),
    };
    let mut env = BTreeMap::new();
    for (k, template) in &recipe.env {
        env.insert(k.clone(), fill(template, &ctx)?);
    }
    let mut args = Vec::with_capacity(recipe.args.len());
    for a in &recipe.args {
        args.push(fill(a, &ctx)?);
    }
    Ok(Launch { env, args })
}

/// Replace `{model}`, `{small_model}`, `{api_key}`, `{base_url}` in `template`. A
/// referenced placeholder whose value is `None` is an error — so a provider that needs
/// an API key fails loudly rather than launching unauthenticated.
fn fill(template: &str, ctx: &LaunchContext) -> Result<String, String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after
            .find('}')
            .ok_or_else(|| format!("unterminated placeholder in '{template}'"))?;
        let key = &after[..end];
        let value = match key {
            "model" => ctx.model.as_deref(),
            "small_model" => ctx.small_model.as_deref(),
            "api_key" => ctx.api_key.as_deref(),
            "base_url" => ctx.base_url.as_deref(),
            other => return Err(format!("unknown placeholder '{{{other}}}'")),
        }
        .ok_or_else(|| format!("provider requires '{{{key}}}' but it was not supplied"))?;
        out.push_str(value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentRegistry;
    use std::io::Write;
    use std::path::Path;

    fn write(dir: &Path, name: &str, json: &str) {
        let mut f = std::fs::File::create(dir.join(name)).unwrap();
        f.write_all(json.as_bytes()).unwrap();
    }

    /// A Claude manifest with native + openrouter recipes, mirroring the reference.
    fn manifest() -> AgentManifest {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "claude.json",
            r#"{
              "schema_version": 1, "id": "claude", "name": "Claude Code",
              "run": { "windows": { "command": "claude" }, "macos": { "command": "claude" }, "linux": { "command": "claude" } },
              "providers": ["native", "openrouter"],
              "provider_recipes": {
                "native": { "args": ["--model", "{model}"] },
                "openrouter": {
                  "env": {
                    "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                    "ANTHROPIC_AUTH_TOKEN": "{api_key}",
                    "ANTHROPIC_SMALL_FAST_MODEL": "{small_model}"
                  },
                  "args": ["--model", "{model}"]
                }
              }
            }"#,
        );
        AgentRegistry::load_from_dirs(&[d.path().to_path_buf()])
            .get("claude")
            .unwrap()
            .clone()
    }

    #[test]
    fn openrouter_recipe_fills_env_and_args() {
        let m = manifest();
        let ctx = LaunchContext {
            model: Some("anthropic/claude-sonnet-4.5".into()),
            small_model: Some("anthropic/claude-haiku-4.5".into()),
            api_key: Some("sk-or-secret".into()),
            base_url: None,
        };
        let launch = compose_launch(&m, "openrouter", &ctx).unwrap();
        assert_eq!(launch.env["ANTHROPIC_BASE_URL"], "https://openrouter.ai/api");
        assert_eq!(launch.env["ANTHROPIC_AUTH_TOKEN"], "sk-or-secret");
        assert_eq!(launch.env["ANTHROPIC_SMALL_FAST_MODEL"], "anthropic/claude-haiku-4.5");
        assert_eq!(launch.args, vec!["--model", "anthropic/claude-sonnet-4.5"]);
    }

    #[test]
    fn native_provider_normalises_an_openrouter_model_to_a_native_alias() {
        // The picker can hand `native` an OpenRouter-format id; the native CLI rejects `vendor/model`,
        // so it must be reduced to an alias it accepts (CPE-589).
        let m = manifest();
        let ctx = LaunchContext { model: Some("anthropic/claude-opus-4.8".into()), ..Default::default() };
        let launch = compose_launch(&m, "native", &ctx).unwrap();
        assert_eq!(launch.args, vec!["--model", "opus"]);

        // A bare/native id is left alone.
        let ctx = LaunchContext { model: Some("claude-sonnet-4-5".into()), ..Default::default() };
        assert_eq!(compose_launch(&m, "native", &ctx).unwrap().args, vec!["--model", "claude-sonnet-4-5"]);
    }

    #[test]
    fn native_model_maps_families_and_passes_bare_ids_through() {
        assert_eq!(native_model("anthropic/claude-opus-4.8"), "opus");
        assert_eq!(native_model("anthropic/claude-sonnet-4.5"), "sonnet");
        assert_eq!(native_model("openai/gpt-5"), "gpt-5"); // unknown family → part after the slash
        assert_eq!(native_model("claude-sonnet-4-5"), "claude-sonnet-4-5"); // already native
    }

    #[test]
    fn recipe_defaults_fill_unsupplied_placeholders() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "x.json",
            r#"{
              "schema_version": 1, "id": "x", "name": "X",
              "run": { "windows": { "command": "x" }, "macos": { "command": "x" }, "linux": { "command": "x" } },
              "providers": ["openrouter"],
              "provider_recipes": {
                "openrouter": {
                  "env": { "AUTH": "{api_key}", "SMALL": "{small_model}" },
                  "args": ["--model", "{model}"],
                  "defaults": { "model": "vendor/big", "small_model": "vendor/small" }
                }
              }
            }"#,
        );
        let m = AgentRegistry::load_from_dirs(&[d.path().to_path_buf()]).get("x").unwrap().clone();
        // Only the API key is supplied; model + small_model come from the recipe defaults.
        let ctx = LaunchContext { api_key: Some("k".into()), ..Default::default() };
        let launch = compose_launch(&m, "openrouter", &ctx).unwrap();
        assert_eq!(launch.args, vec!["--model", "vendor/big"]);
        assert_eq!(launch.env["SMALL"], "vendor/small");
        // A supplied value still overrides the default.
        let ctx2 = LaunchContext { api_key: Some("k".into()), model: Some("mine".into()), ..Default::default() };
        assert_eq!(compose_launch(&m, "openrouter", &ctx2).unwrap().args, vec!["--model", "mine"]);
    }

    #[test]
    fn native_recipe_needs_no_key() {
        let m = manifest();
        let ctx = LaunchContext { model: Some("claude-sonnet-4-5".into()), ..Default::default() };
        let launch = compose_launch(&m, "native", &ctx).unwrap();
        assert!(launch.env.is_empty());
        assert_eq!(launch.args, vec!["--model", "claude-sonnet-4-5"]);
    }

    #[test]
    fn a_missing_api_key_is_a_loud_error() {
        let m = manifest();
        let ctx = LaunchContext { model: Some("x".into()), ..Default::default() }; // no api_key
        let err = compose_launch(&m, "openrouter", &ctx).unwrap_err();
        assert!(err.contains("api_key"));
    }

    #[test]
    fn an_unsupported_provider_is_rejected() {
        let m = manifest();
        let err = compose_launch(&m, "bedrock", &LaunchContext::default()).unwrap_err();
        assert!(err.contains("does not support"));
    }

    #[test]
    fn fill_handles_multiple_and_adjacent_placeholders() {
        let ctx = LaunchContext {
            model: Some("m".into()),
            base_url: Some("http://x".into()),
            ..Default::default()
        };
        assert_eq!(fill("{base_url}/v1?model={model}", &ctx).unwrap(), "http://x/v1?model=m");
        assert_eq!(fill("no-placeholders", &ctx).unwrap(), "no-placeholders");
    }

    /// An agent with a generic anthropic-protocol reseller recipe (using `{base_url}`) — how a Claude
    /// Code-style agent consumes ANY OpenRouter-like reseller (CPE-468).
    fn reseller_agent() -> AgentManifest {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "a.json",
            r#"{
              "schema_version": 1, "id": "claude", "name": "Claude Code",
              "run": { "windows": { "command": "claude" }, "macos": { "command": "claude" }, "linux": { "command": "claude" } },
              "providers": ["native"],
              "reseller_recipes": {
                "anthropic": {
                  "env": { "ANTHROPIC_BASE_URL": "{base_url}", "ANTHROPIC_AUTH_TOKEN": "{api_key}" },
                  "args": ["--model", "{model}"]
                }
              }
            }"#,
        );
        AgentRegistry::load_from_dirs(&[d.path().to_path_buf()]).get("claude").unwrap().clone()
    }

    #[test]
    fn a_reseller_descriptor_composes_a_launch_via_the_protocol_recipe() {
        let m = reseller_agent();
        assert!(m.supports_reseller("anthropic"));
        assert_eq!(m.reseller_protocols(), vec!["anthropic"]);
        let together = ResellerDescriptor {
            id: "together".into(), name: "Together AI".into(),
            protocol: "anthropic".into(), base_url: "https://api.together.xyz/v1".into(),
        };
        let ctx = LaunchContext { model: Some("moonshotai/kimi-k2".into()), api_key: Some("k".into()), ..Default::default() };
        let launch = compose_reseller_launch(&m, &together, &ctx).unwrap();
        assert_eq!(launch.env["ANTHROPIC_BASE_URL"], "https://api.together.xyz/v1"); // reseller base_url wins
        assert_eq!(launch.env["ANTHROPIC_AUTH_TOKEN"], "k");
        assert_eq!(launch.args, vec!["--model", "moonshotai/kimi-k2"]);
    }

    #[test]
    fn the_same_agent_targets_a_different_reseller_with_no_code_change() {
        // The "add a reseller as pure data" claim: a second descriptor, same recipe, just works.
        let m = reseller_agent();
        let groq = ResellerDescriptor {
            id: "groq".into(), name: "Groq".into(),
            protocol: "anthropic".into(), base_url: "https://api.groq.com/openai/v1".into(),
        };
        let ctx = LaunchContext { model: Some("llama-3.3-70b".into()), api_key: Some("g".into()), ..Default::default() };
        let l = compose_reseller_launch(&m, &groq, &ctx).unwrap();
        assert_eq!(l.env["ANTHROPIC_BASE_URL"], "https://api.groq.com/openai/v1");
    }

    #[test]
    fn a_reseller_protocol_the_agent_cant_speak_is_rejected() {
        let m = reseller_agent(); // speaks anthropic only
        let openai_reseller = ResellerDescriptor {
            id: "fireworks".into(), name: "Fireworks".into(),
            protocol: "openai".into(), base_url: "https://api.fireworks.ai/inference/v1".into(),
        };
        let err = compose_reseller_launch(&m, &openai_reseller, &LaunchContext::default()).unwrap_err();
        assert!(err.contains("can't use") && err.contains("openai"), "got: {err}");
    }

    #[test]
    fn a_reseller_missing_its_api_key_is_a_loud_error() {
        let m = reseller_agent();
        let r = ResellerDescriptor {
            id: "x".into(), name: "X".into(), protocol: "anthropic".into(), base_url: "https://x/v1".into(),
        };
        let err = compose_reseller_launch(&m, &r, &LaunchContext { model: Some("m".into()), ..Default::default() }).unwrap_err();
        assert!(err.contains("api_key"), "got: {err}");
    }
}
