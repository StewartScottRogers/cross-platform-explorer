//! Swarm live-session driver — the **reporting seam** (CPE-541, epic CPE-528).
//!
//! The [`Coordinator`](crate::swarm_coordinator::Coordinator) emits abstract
//! [`Assignment`](crate::swarm_coordinator::Assignment)s; [`swarm_bridge`](crate::swarm_bridge) turns
//! each into a concrete [`SwarmLaunch`](crate::swarm_bridge::SwarmLaunch); and
//! [`scope::build_launch`](crate::scope::build_launch) composes that into a scoped
//! [`PtyLaunch`](crate::pty::PtyLaunch). Actually **spawning** that launch as an Agent-Grid session and
//! watching it run to completion is *live* cross-process behavior — only verifiable in the running app,
//! and tracked as the live-driver + GUI-QA remainder of CPE-541.
//!
//! What is **pure** — and therefore lives here, unit-tested — is the other half of "report results back
//! into the coordinator": the reducer that folds a *finished* session's outcome into the coordinator's
//! state machine and surfaces the assignments to launch next. The live driver owns the process; this
//! module owns the bookkeeping, so the bookkeeping can be proven without a live process.

use crate::swarm_coordinator::{Assignment, Coordinator};

/// The result of a finished live agent session, as the live driver observes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOutcome {
    /// The task the session was launched for (matches [`Assignment::task_id`]).
    pub task_id: String,
    /// The agent instance that ran it (as staffed, e.g. `claude#builder1`) — for budget attribution
    /// (matches [`Assignment::agent_id`]).
    pub agent_id: String,
    /// Whether the session finished successfully — drives `on_done` vs `on_failed`.
    pub success: bool,
    /// Tokens the session consumed (`0` if the provider reported none / unknown).
    pub tokens: u64,
    /// Cost in milli-dollars (`0` if unknown).
    pub cost_millis: u64,
}

impl SessionOutcome {
    /// A successful outcome for `task_id` run by `agent_id`, carrying the provider's usage.
    pub fn success(task_id: &str, agent_id: &str, tokens: u64, cost_millis: u64) -> SessionOutcome {
        SessionOutcome { task_id: task_id.into(), agent_id: agent_id.into(), success: true, tokens, cost_millis }
    }

    /// A failed outcome for `task_id` run by `agent_id`, carrying whatever usage was spent before it
    /// failed (a crashed session can still have burned budget).
    pub fn failure(task_id: &str, agent_id: &str, tokens: u64, cost_millis: u64) -> SessionOutcome {
        SessionOutcome { task_id: task_id.into(), agent_id: agent_id.into(), success: false, tokens, cost_millis }
    }
}

/// Fold a finished session's outcome into the coordinator, returning the assignments to launch next.
///
/// Usage is reported **first**, so a budget cap trips (pausing the agent or the mission) *before* the
/// freed agent could be re-dispatched by the `on_done`/`on_failed` that follows. Then the task is closed
/// as done or failed. This is the pure heart of CPE-541's "report results back into the coordinator";
/// the live driver calls it exactly once per finished session and then launches the returned
/// assignments.
pub fn apply_outcome(coord: &mut Coordinator, outcome: &SessionOutcome) -> Vec<Assignment> {
    if outcome.tokens > 0 || outcome.cost_millis > 0 {
        coord.on_usage(&outcome.agent_id, outcome.tokens, outcome.cost_millis);
    }
    if outcome.success {
        coord.on_done(&outcome.task_id)
    } else {
        coord.on_failed(&outcome.task_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm_coordinator::{Budget, Task, TaskState};
    use crate::swarm_team::{Role, RoleSpec, TeamManifest};

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
            gate: crate::swarm_coordinator::Gate::None,
        }
    }

    fn agent_for(coord: &Coordinator, task_id: &str) -> String {
        coord.assignee_of(task_id).expect("task should be assigned").to_string()
    }

    #[test]
    fn a_successful_outcome_marks_the_task_done_and_dispatches_the_next() {
        // One builder serializes two disjoint tasks: finishing the first should launch the second.
        let mut c = Coordinator::new(&team(1), vec![task("t1", &["a/**"]), task("t2", &["b/**"])]).unwrap();
        c.start();
        let running = if c.state_of("t1") == Some(TaskState::Running) { "t1" } else { "t2" };
        let other = if running == "t1" { "t2" } else { "t1" };
        let agent = agent_for(&c, running);

        let next = apply_outcome(&mut c, &SessionOutcome::success(running, &agent, 100, 5));
        assert_eq!(c.state_of(running), Some(TaskState::Done));
        assert_eq!(next.len(), 1, "the freed builder should pick up the other task");
        assert_eq!(next[0].task_id, other);
        assert_eq!(c.spend(), (100, 5), "usage was attributed");
    }

    #[test]
    fn a_failed_outcome_retries_then_terminally_fails_within_the_retry_budget() {
        let mut c = Coordinator::new(&team(1), vec![task("t1", &["a/**"])]).unwrap();
        c.set_max_retries(1);
        c.start();
        let agent = agent_for(&c, "t1");

        // First failure is under the retry budget → task re-dispatched, not terminal.
        let after_first = apply_outcome(&mut c, &SessionOutcome::failure("t1", &agent, 0, 0));
        assert_eq!(after_first.len(), 1);
        assert_eq!(c.state_of("t1"), Some(TaskState::Running));

        // Second failure exceeds max_retries → terminal Failed, mission has a failure.
        let agent = agent_for(&c, "t1");
        apply_outcome(&mut c, &SessionOutcome::failure("t1", &agent, 0, 0));
        assert_eq!(c.state_of("t1"), Some(TaskState::Failed));
        assert!(c.has_failure());
    }

    #[test]
    fn usage_is_applied_before_completion_so_a_budget_cap_holds_back_the_freed_agent() {
        // Two disjoint tasks, two builders, but a mission cost cap that the first session blows through.
        let mut c = Coordinator::new(&team(2), vec![task("t1", &["a/**"]), task("t2", &["b/**"])]).unwrap();
        c.set_budget(Budget { max_tokens: 0, max_cost_millis: 500 }, Budget::default());
        let dispatched = c.start();
        // With two disjoint tasks + two builders both start immediately.
        assert_eq!(dispatched.len(), 2);
        let agent = agent_for(&c, "t1");

        // Finishing t1 while blowing the mission cost cap must pause the mission: because usage is
        // reported before on_done, no further assignment is handed back out.
        let next = apply_outcome(&mut c, &SessionOutcome::success("t1", &agent, 0, 600));
        assert_eq!(c.state_of("t1"), Some(TaskState::Done));
        assert!(c.is_mission_paused(), "the cost cap should have paused the mission");
        assert!(next.is_empty(), "a paused mission dispatches nothing");
    }

    #[test]
    fn a_zero_usage_outcome_does_not_disturb_spend() {
        let mut c = Coordinator::new(&team(1), vec![task("t1", &["a/**"])]).unwrap();
        c.start();
        let agent = agent_for(&c, "t1");
        apply_outcome(&mut c, &SessionOutcome::success("t1", &agent, 0, 0));
        assert_eq!(c.spend(), (0, 0));
        assert!(c.is_complete());
    }
}
