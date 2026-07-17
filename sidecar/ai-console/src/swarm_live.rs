//! Live swarm driver (CPE-541, epic CPE-528) — the orchestration loop that turns coordinator
//! assignments into **real launched sessions** and folds their outcomes back. The *reporting* reducer is
//! pure ([`crate::swarm_driver::apply_outcome`]); this module owns the live parts: launching through a
//! [`SessionEngine`], detecting completion (a session's output channel closes on agent exit — EOF), and
//! driving the mission to quiescence.
//!
//! The one part that still needs the running app — turning a [`SwarmLaunch`] into a real agent
//! [`PtyLaunch`] (resolve the agent manifest/provider via `scope::build_launch`, write the roster, inject
//! the `--swarm-mcp` config) — is the injected [`LaunchPlanner`]. A test planner uses a trivial
//! subprocess, so the loop itself is proven end-to-end with **real processes**, headlessly; only the
//! planner's manifest-resolution + a real-agent run are GUI-QA.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::pty::PtyLaunch;
use crate::session_engine::SessionEngine;
use crate::swarm_bridge::{launch_spec_for, SwarmLaunch};
use crate::swarm_coordinator::{Assignment, Coordinator, Task};
use crate::swarm_driver::{apply_outcome, SessionOutcome};
use crate::swarm_team::TeamManifest;
use crate::usage::UsageScanner;

/// Turns a coordinator assignment (+ its concrete [`SwarmLaunch`]) into a scoped [`PtyLaunch`]. The live
/// implementation resolves the agent manifest/provider and injects the swarm MCP config; a test uses a
/// trivial command. `Err` fails just that assignment (reported as a failed outcome, so the coordinator
/// can retry/reassign).
pub trait LaunchPlanner: Send + Sync {
    fn plan(&self, assignment: &Assignment, spec: &SwarmLaunch) -> Result<PtyLaunch, String>;
}

/// Classifies a finished session as success (`true`) or failure from its collected output. Coding-agent
/// success can't be read from an exit code (`SessionIo` doesn't surface one), so the default treats a
/// clean end as success; the live app can supply a smarter classifier (e.g. scan for an error banner).
pub type OutcomeClassifier = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// The default classifier: a session that ended is a success. Honest about its limits — see the type doc.
pub fn assume_success() -> OutcomeClassifier {
    Arc::new(|_output: &str| true)
}

/// Extract a session's real spend from its terminal output as `(tokens, cost_millis)` for a
/// [`SessionOutcome`]. Reuses the same [`UsageScanner`] the live console uses (CPE-311), so the
/// swarm's budget caps see the same figures the UI does. Cost is dollars → integer milli-dollars.
fn usage_from_output(output: &str) -> (u64, u64) {
    let mut scanner = UsageScanner::new();
    let u = scanner.feed(output);
    let tokens = u.input_tokens.saturating_add(u.output_tokens);
    let cost_millis = (u.cost_usd.max(0.0) * 1000.0).round() as u64;
    (tokens, cost_millis)
}

/// How long the loop waits for a session to finish before giving up (a hung agent shouldn't hang the
/// mission forever).
const SESSION_TIMEOUT: Duration = Duration::from_secs(600);

pub struct Completion {
    pub task_id: String,
    pub agent_id: String,
    pub output: String,
}

/// Launches a planned session and reports its completion. Abstracts *where* a session runs and who
/// watches it finish: [`EngineRunner`] launches through a [`SessionEngine`] and reads the PTY directly
/// (the pure loop + headless tests); the console supplies a runner that instead **adopts** the session
/// into the UI (CPE-574) so it appears as a live, streaming, recorded session — the driver then never
/// contends for the single-consumer output channel.
pub trait SessionRunner: Send + Sync {
    /// Launch `launch` as session `task_id` (run by `agent_id`); when it finishes, send its collected
    /// output as a [`Completion`] on `done`. `Err` means it couldn't even start (an immediate failure).
    fn launch(
        &self,
        task_id: &str,
        agent_id: &str,
        launch: &PtyLaunch,
        done: Sender<Completion>,
    ) -> Result<(), String>;
}

/// The default runner: launch through a [`SessionEngine`] and read the PTY directly to detect
/// completion. This owns the cross-platform completion detection (see [`SessionRunner::launch`]).
pub struct EngineRunner {
    engine: Arc<dyn SessionEngine>,
}

impl EngineRunner {
    pub fn new(engine: Arc<dyn SessionEngine>) -> EngineRunner {
        EngineRunner { engine }
    }
}

impl SessionRunner for EngineRunner {
    fn launch(
        &self,
        task_id: &str,
        agent_id: &str,
        launch: &PtyLaunch,
        done: Sender<Completion>,
    ) -> Result<(), String> {
        let io = self.engine.launch(task_id, launch)?;
        let out_rx = io.take_output().ok_or_else(|| "session has no output channel".to_string())?;
        let task_id = task_id.to_string();
        let agent_id = agent_id.to_string();
        thread::spawn(move || {
            // Wait for the session to finish while collecting its output. Completion is whichever comes
            // first: the child exits (polled via `try_wait` — reliable on Windows ConPTY, where the
            // output channel doesn't EOF while the master is held), or the output channel closes (EOF —
            // the signal on Unix PTYs and the daemon path, where `try_wait` is unsupported). On a
            // detected exit we `kill` to close the PTY so the buffered output drains to EOF.
            let mut output = String::new();
            loop {
                match out_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(chunk) => output.push_str(&String::from_utf8_lossy(&chunk)),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break, // EOF (Unix / after master close)
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if io.try_wait().is_some() {
                    // Child exited (authoritative on Windows ConPTY, where the reader won't EOF while the
                    // master is held). Briefly drain any buffered output, then finish — dropping `io`
                    // below closes the PTY, which ends the engine's reader thread.
                    let grace = Instant::now() + Duration::from_millis(150);
                    while Instant::now() < grace {
                        match out_rx.try_recv() {
                            Ok(chunk) => output.push_str(&String::from_utf8_lossy(&chunk)),
                            Err(mpsc::TryRecvError::Empty) => thread::sleep(Duration::from_millis(10)),
                            Err(mpsc::TryRecvError::Disconnected) => break,
                        }
                    }
                    let _ = io.kill();
                    break;
                }
            }
            let _ = done.send(Completion { task_id, agent_id, output });
            drop(io);
        });
        Ok(())
    }
}

/// Drives a mission's coordinator by launching each assignment through a [`SessionRunner`].
pub struct SwarmDriver {
    coord: Coordinator,
    team: TeamManifest,
    task_text: HashMap<String, String>,
    runner: Arc<dyn SessionRunner>,
    planner: Arc<dyn LaunchPlanner>,
    classify: OutcomeClassifier,
    timeout: Duration,
}

impl SwarmDriver {
    /// Build a driver for `tasks` staffed by `team`, launching through `engine` with `planner` (spec →
    /// `PtyLaunch`) and `classify` (output → success). Errors if the team can't staff the mission.
    pub fn new(
        team: TeamManifest,
        tasks: Vec<Task>,
        runner: Arc<dyn SessionRunner>,
        planner: Arc<dyn LaunchPlanner>,
        classify: OutcomeClassifier,
    ) -> Result<SwarmDriver, String> {
        let task_text = tasks.iter().map(|t| (t.id.clone(), t.description.clone())).collect();
        let coord = Coordinator::new(&team, tasks)?;
        Ok(SwarmDriver { coord, team, task_text, runner, planner, classify, timeout: SESSION_TIMEOUT })
    }

    /// Override the per-session wait (tests use a short one).
    pub fn with_timeout(mut self, timeout: Duration) -> SwarmDriver {
        self.timeout = timeout;
        self
    }

    /// Run the mission to quiescence: launch the coordinator's assignments as real sessions, and each
    /// time one finishes fold its outcome back and launch whatever comes next, until nothing is pending
    /// or in flight. Returns the final coordinator so the caller can inspect progress/failures.
    pub fn run(mut self) -> Result<Coordinator, String> {
        let (tx, rx) = mpsc::channel::<Completion>();
        let mut pending: VecDeque<Assignment> = self.coord.start().into();
        let mut in_flight = 0usize;

        loop {
            // Launch everything pending; a launch that can't even start is an immediate failure.
            while let Some(a) = pending.pop_front() {
                match self.try_launch(&a, &tx) {
                    Ok(()) => in_flight += 1,
                    Err(_e) => {
                        let outcome = SessionOutcome::failure(&a.task_id, &a.agent_id, 0, 0);
                        pending.extend(apply_outcome(&mut self.coord, &outcome));
                    }
                }
            }
            if in_flight == 0 {
                break; // nothing pending and nothing running → the mission has settled
            }
            let c = rx.recv_timeout(self.timeout).map_err(|_| "driver timed out waiting for a session".to_string())?;
            in_flight -= 1;
            let success = (self.classify)(&c.output);
            // Real usage: scan what the agent actually printed (Claude Code & co. report tokens/cost in
            // their session output) so budget caps bite on real spend — via `apply_outcome`, which reports
            // usage before completion. Zero when the agent printed nothing recognizable.
            let (tokens, cost_millis) = usage_from_output(&c.output);
            let outcome = if success {
                SessionOutcome::success(&c.task_id, &c.agent_id, tokens, cost_millis)
            } else {
                SessionOutcome::failure(&c.task_id, &c.agent_id, tokens, cost_millis)
            };
            pending.extend(apply_outcome(&mut self.coord, &outcome));
        }
        Ok(self.coord)
    }

    /// Plan an assignment into a concrete launch and hand it to the [`SessionRunner`], which spawns the
    /// real session and reports its completion back on `tx`. A plan/launch that can't even start is an
    /// error the caller turns into an immediate failed outcome.
    fn try_launch(&self, a: &Assignment, tx: &Sender<Completion>) -> Result<(), String> {
        let task = self.task_text.get(&a.task_id).cloned().unwrap_or_default();
        let spec = launch_spec_for(&a.agent_id, &self.team, &task);
        let launch = self.planner.plan(a, &spec)?;
        self.runner.launch(&a.task_id, &a.agent_id, &launch, tx.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_engine::LocalEngine;
    use crate::swarm_coordinator::{Gate, TaskState};
    use crate::swarm_team::{Role, RoleSpec};
    use std::collections::BTreeMap;

    fn team(builders: u32) -> TeamManifest {
        TeamManifest {
            name: "T".into(),
            description: String::new(),
            roles: vec![
                RoleSpec { role: Role::Coordinator, agent: "claude".into(), model: None, count: 1 },
                RoleSpec { role: Role::Builder, agent: "claude".into(), model: None, count: builders },
            ],
        }
    }

    fn task(id: &str, globs: &[&str]) -> Task {
        Task {
            id: id.into(),
            description: format!("do {id}"),
            role: Role::Builder,
            globs: globs.iter().map(|s| s.to_string()).collect(),
            gate: Gate::None,
        }
    }

    /// A planner that runs a trivial, fast-exiting subprocess — real process, real PTY, real EOF.
    struct EchoPlanner;
    impl LaunchPlanner for EchoPlanner {
        fn plan(&self, _a: &Assignment, spec: &SwarmLaunch) -> Result<PtyLaunch, String> {
            let (program, args) = crate::pty::shell_command(&format!("echo ran {}", spec.task));
            Ok(PtyLaunch { program, args, cwd: None, env: BTreeMap::new(), rows: 24, cols: 80 })
        }
    }

    fn driver(team: TeamManifest, tasks: Vec<Task>, planner: Arc<dyn LaunchPlanner>, classify: OutcomeClassifier) -> SwarmDriver {
        let runner = Arc::new(EngineRunner::new(Arc::new(LocalEngine)));
        SwarmDriver::new(team, tasks, runner, planner, classify)
            .unwrap()
            .with_timeout(Duration::from_secs(20))
    }

    #[test]
    fn drives_serialized_tasks_to_completion_with_real_subprocesses() {
        // One builder serializes two tasks: the loop must finish t1's real session, then launch + finish
        // t2, and settle with the mission complete.
        let d = driver(team(1), vec![task("t1", &["a/**"]), task("t2", &["b/**"])], Arc::new(EchoPlanner), assume_success());
        let coord = d.run().expect("driver runs to completion");
        assert!(coord.is_complete(), "every task should be Done");
        assert_eq!(coord.progress(), (2, 2));
        assert!(!coord.has_failure());
    }

    #[test]
    fn parallel_disjoint_tasks_both_complete() {
        let d = driver(team(2), vec![task("t1", &["a/**"]), task("t2", &["b/**"])], Arc::new(EchoPlanner), assume_success());
        let coord = d.run().unwrap();
        assert_eq!(coord.progress(), (2, 2));
    }

    #[test]
    fn a_launch_that_cannot_start_is_reported_as_a_failure_and_retried_to_terminal() {
        // A planner that always fails to plan → every attempt is a failed outcome; with retries exhausted
        // the task ends terminally Failed (the loop still settles, never hangs).
        struct FailPlanner;
        impl LaunchPlanner for FailPlanner {
            fn plan(&self, _a: &Assignment, _s: &SwarmLaunch) -> Result<PtyLaunch, String> {
                Err("cannot resolve agent".into())
            }
        }
        let mut d = driver(team(1), vec![task("t1", &["a/**"])], Arc::new(FailPlanner), assume_success());
        d.coord.set_max_retries(1);
        let coord = d.run().expect("loop settles even when launches fail");
        assert_eq!(coord.state_of("t1"), Some(TaskState::Failed));
        assert!(coord.has_failure());
    }

    #[test]
    fn a_classifier_that_marks_failure_drives_the_failure_path() {
        // The session runs (real subprocess) but the classifier judges it failed → retried then terminal.
        let mut d = driver(team(1), vec![task("t1", &["a/**"])], Arc::new(EchoPlanner), Arc::new(|_o: &str| false));
        d.coord.set_max_retries(1);
        let coord = d.run().unwrap();
        assert_eq!(coord.state_of("t1"), Some(TaskState::Failed));
    }

    #[test]
    fn usage_is_parsed_from_a_sessions_output() {
        let (tokens, cost_millis) = usage_from_output("thinking...\ninput: 1000 output: 500 total cost: $0.05\ndone\n");
        assert_eq!(tokens, 1500, "input+output tokens");
        assert_eq!(cost_millis, 50, "$0.05 → 50 milli-dollars");
        assert_eq!(usage_from_output("no figures here\n"), (0, 0), "silent output → zero spend");
    }

    #[test]
    fn a_sessions_reported_spend_reaches_the_coordinator() {
        // A planner whose agent prints a real usage line → the driver folds that spend into the
        // coordinator (not the hardcoded zero), so budget caps would see it.
        struct UsagePlanner;
        impl LaunchPlanner for UsagePlanner {
            fn plan(&self, _a: &Assignment, _s: &SwarmLaunch) -> Result<PtyLaunch, String> {
                // Tokens only — a `$` cost figure isn't shell-safe (Unix `sh -c` expands `$0`); the
                // `$cost` parse is covered deterministically by `usage_is_parsed_from_a_sessions_output`.
                let (program, args) = crate::pty::shell_command("echo input: 2000 output: 1000");
                Ok(PtyLaunch { program, args, cwd: None, env: BTreeMap::new(), rows: 24, cols: 80 })
            }
        }
        let coord = driver(team(1), vec![task("t1", &["a/**"])], Arc::new(UsagePlanner), assume_success()).run().unwrap();
        assert!(coord.is_complete());
        assert_eq!(coord.spend(), (3000, 0), "the agent's printed tokens reached the coordinator");
    }
}
