//! The bundled agent catalog (CPE-291) loads and is coherent. Validates the shipped
//! `agents/*.json` manifests against the registry, routing, and provider support.

use std::path::PathBuf;

use ai_console::agents::AgentRegistry;
use ai_console::routing::{compose_launch, LaunchContext};

fn catalog_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agents")
}

#[test]
fn the_bundled_catalog_loads_cleanly() {
    let reg = AgentRegistry::load_from_dirs(&[catalog_dir()]);
    // Every bundled manifest is valid — no load warnings.
    assert!(reg.warnings().is_empty(), "warnings: {:?}", reg.warnings());
    assert!(reg.len() >= 12, "expected the seed catalog, got {}", reg.len());
    for id in [
        "claude", "codex", "gemini", "qwen", "opencode", "grok", "aider", "mistral",
        "codebuff", "pi", "tau", "vtcode",
    ] {
        assert!(reg.get(id).is_some(), "missing agent '{id}'");
    }
}

#[test]
fn agents_have_run_and_install_for_this_os() {
    let reg = AgentRegistry::load_from_dirs(&[catalog_dir()]);
    for agent in reg.all() {
        assert!(agent.run_for_current_os().is_some(), "{} has no run", agent.id);
        assert!(agent.install_for_current_os().is_some(), "{} has no install", agent.id);
        assert!(agent.uninstall_for_current_os().is_some(), "{} has no uninstall", agent.id);
    }
}

#[test]
fn claude_supports_openrouter_and_routes() {
    let reg = AgentRegistry::load_from_dirs(&[catalog_dir()]);
    let claude = reg.get("claude").unwrap();
    assert!(claude.supports_provider("openrouter"));

    let launch = compose_launch(
        claude,
        "openrouter",
        &LaunchContext {
            model: Some("anthropic/claude-sonnet-4.5".into()),
            small_model: Some("anthropic/claude-haiku-4.5".into()),
            api_key: Some("sk-or-xxx".into()),
            base_url: None,
        },
    )
    .unwrap();
    assert_eq!(launch.env["ANTHROPIC_BASE_URL"], "https://openrouter.ai/api");
    assert_eq!(launch.env["ANTHROPIC_AUTH_TOKEN"], "sk-or-xxx");
    assert_eq!(launch.args, vec!["--model", "anthropic/claude-sonnet-4.5"]);
}

#[test]
fn claude_lmstudio_local_injects_detected_url_and_loaded_model() {
    // CPE-330: the lmstudio-local recipe points the agent at a *detected* endpoint via
    // {base_url} and runs the endpoint's actually-loaded model — no manual URL entry.
    let reg = AgentRegistry::load_from_dirs(&[catalog_dir()]);
    let claude = reg.get("claude").unwrap();
    assert!(claude.supports_provider("lmstudio-local"));

    // Emulate what the launch handler does: merge detection into the launch context.
    let detected = ai_console::LmStudio {
        base_url: "http://192.168.1.7:1234".into(),
        model: Some("qwen3-coder-30b".into()),
    };
    let (base_url, model) =
        ai_console::lmstudio::resolve_launch(None, None, Some(detected));
    let launch = compose_launch(
        claude,
        "lmstudio-local",
        &LaunchContext { model, base_url, ..Default::default() },
    )
    .unwrap();
    assert_eq!(launch.env["ANTHROPIC_BASE_URL"], "http://192.168.1.7:1234");
    assert_eq!(launch.env["ANTHROPIC_AUTH_TOKEN"], "lm-studio");
    assert_eq!(launch.args, vec!["--model", "qwen3-coder-30b"]);
}

#[test]
fn lmstudio_local_falls_back_to_recipe_base_url_when_undetected() {
    // With nothing detected and nothing supplied, the recipe's default base_url applies
    // so the launch still targets the conventional loopback endpoint.
    let reg = AgentRegistry::load_from_dirs(&[catalog_dir()]);
    let claude = reg.get("claude").unwrap();
    let (base_url, _model) = ai_console::lmstudio::resolve_launch(None, None, None);
    let launch = compose_launch(
        claude,
        "lmstudio-local",
        &LaunchContext { model: Some("m".into()), base_url, ..Default::default() },
    )
    .unwrap();
    assert_eq!(launch.env["ANTHROPIC_BASE_URL"], "http://127.0.0.1:1234");
}

#[test]
fn aider_installs_via_uv() {
    let reg = AgentRegistry::load_from_dirs(&[catalog_dir()]);
    let aider = reg.get("aider").unwrap();
    assert_eq!(aider.install_for_current_os().unwrap().command, "uv");
}

#[test]
fn every_listed_provider_has_a_working_recipe() {
    // compose_launch must succeed (given values) for every provider each agent lists —
    // i.e. no agent advertises a provider it can't actually route.
    let reg = AgentRegistry::load_from_dirs(&[catalog_dir()]);
    let ctx = LaunchContext {
        model: Some("m".into()),
        small_model: Some("sm".into()),
        api_key: Some("k".into()),
        base_url: Some("http://x".into()),
    };
    for agent in reg.all() {
        for provider in &agent.providers {
            assert!(
                compose_launch(agent, provider, &ctx).is_ok(),
                "{} / {provider} has no working recipe",
                agent.id
            );
        }
    }
}
