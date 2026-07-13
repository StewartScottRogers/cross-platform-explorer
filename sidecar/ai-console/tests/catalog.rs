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
    assert!(reg.len() >= 8, "expected the seed catalog, got {}", reg.len());
    for id in ["claude", "codex", "gemini", "qwen", "opencode", "grok", "aider", "mistral"] {
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
