//! Agent execution scoping (CPE-306).
//!
//! A launched coding agent is arbitrary code with the user's injected credentials, so
//! its execution is *scoped*: the working directory is pinned to the chosen repo, and
//! the environment is exactly the routing-recipe env plus the selected credential
//! profile's env — nothing the session didn't ask for. This module composes that
//! scoped [`PtyLaunch`] (without spawning, so it's unit-testable) and surfaces any
//! **dangerous run flags** so the UI can require explicit opt-in.

use std::collections::BTreeMap;

use crate::agents::AgentManifest;
use crate::pty::PtyLaunch;
use crate::routing::{compose_launch, LaunchContext};

/// Known "trust me" flags across agents (auto-approve edits / skip permission prompts).
/// Presence means the user is extending broad trust — the UI should confirm.
const DANGEROUS_FLAGS: &[&str] = &[
    "--yolo",
    "--dangerously-skip-permissions",
    "--force",
    "--auto-approve",
    "--full-auto",
];

/// A request to launch an agent session.
pub struct AgentLaunchRequest<'a> {
    pub agent: &'a AgentManifest,
    pub provider: &'a str,
    /// When `Some`, launch against a **reseller** gateway (CPE-469): the launch is composed from the
    /// agent's `reseller_recipes[protocol]` with the reseller's `base_url`, instead of the per-agent
    /// `provider_recipes[provider]`. `provider` is then just the reseller's id (for logging/presets).
    pub reseller: Option<crate::routing::ResellerDescriptor>,
    /// Model/keys/urls for the provider recipe (api_key resolved from the vault).
    pub ctx: LaunchContext,
    /// Extra env from the selected credential profile (resolved via the vault).
    pub profile_env: BTreeMap<String, String>,
    /// The repo/folder the session is scoped to.
    pub cwd: String,
    /// Any extra user-supplied run args (e.g. a task, or a dangerous flag).
    pub extra_args: Vec<String>,
    pub rows: u16,
    pub cols: u16,
}

/// Any dangerous flags present in `args`, for disclosure/opt-in.
pub fn dangerous_flags(args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|a| DANGEROUS_FLAGS.iter().any(|d| d == a))
        .cloned()
        .collect()
}

/// Compose the scoped [`PtyLaunch`] for a request: the agent's run command, its args
/// plus the provider-recipe args plus the user's extra args, cwd pinned to the repo, and
/// env = routing-recipe env merged with the credential-profile env. Errors if the agent
/// can't run on this OS or doesn't support the provider.
pub fn build_launch(req: &AgentLaunchRequest) -> Result<PtyLaunch, String> {
    let run = req
        .agent
        .run_for_current_os()
        .ok_or_else(|| format!("agent '{}' has no run command for this OS", req.agent.id))?;

    let routed = match &req.reseller {
        Some(r) => crate::routing::compose_reseller_launch(req.agent, r, &req.ctx)?,
        None => compose_launch(req.agent, req.provider, &req.ctx)?,
    };

    // Env: routing recipe first, then the profile env (profile wins on conflict — it's
    // the user's explicit choice). Nothing else is added.
    let mut env = routed.env;
    for (k, v) in &req.profile_env {
        env.insert(k.clone(), v.clone());
    }

    let mut args = run.args.clone();
    args.extend(routed.args);
    args.extend(req.extra_args.iter().cloned());

    Ok(PtyLaunch {
        program: run.command.clone(),
        args,
        cwd: Some(req.cwd.clone()),
        env,
        rows: req.rows,
        cols: req.cols,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentRegistry;
    use std::path::PathBuf;

    fn claude() -> AgentManifest {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agents");
        AgentRegistry::load_from_dirs(&[dir]).get("claude").unwrap().clone()
    }

    #[test]
    fn scopes_cwd_and_merges_env() {
        let agent = claude();
        let mut profile_env = BTreeMap::new();
        profile_env.insert("EXTRA".to_string(), "1".to_string());
        let req = AgentLaunchRequest {
            agent: &agent,
            provider: "openrouter",
            reseller: None,
            ctx: LaunchContext {
                model: Some("anthropic/claude-sonnet-4.5".into()),
                small_model: Some("anthropic/claude-haiku-4.5".into()),
                api_key: Some("sk-or".into()),
                base_url: None,
            },
            profile_env,
            cwd: "/repo".into(),
            extra_args: vec!["--print".into()],
            rows: 24,
            cols: 80,
        };
        let launch = build_launch(&req).unwrap();
        assert_eq!(launch.program, "claude");
        assert_eq!(launch.cwd.as_deref(), Some("/repo"));
        // Routing env + profile env both present.
        assert_eq!(launch.env["ANTHROPIC_AUTH_TOKEN"], "sk-or");
        assert_eq!(launch.env["EXTRA"], "1");
        // run args (none) + routed args (--model X) + extra (--print).
        assert!(launch.args.contains(&"--model".to_string()));
        assert!(launch.args.contains(&"--print".to_string()));
    }

    #[test]
    fn build_launch_uses_the_reseller_path_when_a_descriptor_is_set() {
        // The real qwen manifest declares an `openai` reseller recipe (CPE-469); launched against a
        // Groq descriptor it must fill OPENAI_BASE_URL from the reseller, not a provider recipe.
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agents");
        let agent = AgentRegistry::load_from_dirs(&[dir]).get("qwen").unwrap().clone();
        assert!(agent.supports_reseller("openai"), "qwen should declare an openai reseller recipe");
        let reseller = crate::routing::ResellerDescriptor {
            id: "groq".into(),
            name: "Groq".into(),
            protocol: "openai".into(),
            base_url: "https://api.groq.com/openai/v1".into(),
        };
        let req = AgentLaunchRequest {
            agent: &agent,
            provider: "groq",
            reseller: Some(reseller),
            ctx: LaunchContext { model: Some("llama-3.3-70b".into()), api_key: Some("gsk".into()), ..Default::default() },
            profile_env: BTreeMap::new(),
            cwd: "/repo".into(),
            extra_args: vec![],
            rows: 24,
            cols: 80,
        };
        let launch = build_launch(&req).unwrap();
        assert_eq!(launch.env["OPENAI_BASE_URL"], "https://api.groq.com/openai/v1"); // reseller wins
        assert_eq!(launch.env["OPENAI_API_KEY"], "gsk");
        assert!(launch.args.contains(&"llama-3.3-70b".to_string()));
    }

    #[test]
    fn errors_on_unsupported_provider() {
        let agent = claude();
        let req = AgentLaunchRequest {
            agent: &agent,
            provider: "bedrock",
            reseller: None,
            ctx: LaunchContext::default(),
            profile_env: BTreeMap::new(),
            cwd: "/repo".into(),
            extra_args: vec![],
            rows: 24,
            cols: 80,
        };
        assert!(build_launch(&req).is_err());
    }

    #[test]
    fn detects_dangerous_flags() {
        let flags = dangerous_flags(&[
            "--model".into(),
            "--dangerously-skip-permissions".into(),
            "--yolo".into(),
        ]);
        assert_eq!(flags, vec!["--dangerously-skip-permissions", "--yolo"]);
        assert!(dangerous_flags(&["--model".into(), "gpt".into()]).is_empty());
    }
}
