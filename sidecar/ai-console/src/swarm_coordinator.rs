//! Swarm coordinator (CPE-517) — the keystone of [CPE-502] Swarm orchestration. It ties the wave-1
//! substrates together: it staffs a team from a manifest ([`crate::swarm_team`]), assigns each mission
//! task to a role agent, gates concurrency with the file-ownership lock manager
//! ([`crate::swarm_locks`]) so agents never collide, and announces assignments over the mailbox
//! ([`crate::swarm_mailbox`]). Disjoint tasks run in parallel; tasks that share files are sequenced.
//!
//! This module is the **pure scheduling state machine**. It emits **dispatch intents**
//! ([`Assignment`]) — "run task T on agent A" — and advances on `on_done` / `on_failed` events. Actually
//! launching an agent session (an Agent-Grid pane) for an assignment and reporting back is the live
//! integration layer that drives this core; that seam is deliberately outside the pure logic so the
//! orchestration is unit-testable headlessly.

use crate::swarm_locks::LockManager;
use crate::swarm_mailbox::{Mailbox, Recipient};
use crate::swarm_team::{Role, TeamManifest};
use std::collections::{HashMap, HashSet};

/// The quality gate a task must pass before it counts as done (CPE-518).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// No gate — finishing the work is done.
    None,
    /// A test suite must pass (run by the live driver, which reports `on_gate_pass`/`on_gate_fail`).
    Tests,
    /// A reviewer agent must approve (the coordinator requests it over the mailbox).
    Review,
}

/// A unit of work in a mission: a description, the role that should do it, the file globs it will
/// exclusively own while running (its lock claim), and the quality gate it must pass to be done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub role: Role,
    pub globs: Vec<String>,
    pub gate: Gate,
}

/// A task's lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Running,
    /// Work finished, awaiting its quality gate (CPE-518); locks stay held so a reopen is safe.
    Gating,
    Done,
    Failed,
}

/// A dispatch intent: the coordinator wants `task_id` run on agent `agent_id`. The live layer turns
/// this into an actual agent session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub task_id: String,
    pub agent_id: String,
}

/// Staff a team manifest into concrete agent instances (`agent#role{n}`) with their role.
fn staff(team: &TeamManifest) -> Vec<(String, Role)> {
    let mut out = Vec::new();
    let mut per_role: HashMap<Role, u32> = HashMap::new();
    for spec in &team.roles {
        for _ in 0..spec.count {
            let n = per_role.entry(spec.role).or_insert(0);
            *n += 1;
            out.push((format!("{}#{:?}{}", spec.agent, spec.role, n).to_lowercase(), spec.role));
        }
    }
    out
}

/// Drives a mission to completion across a role-based team, collision-free (CPE-517).
#[derive(Debug)]
pub struct Coordinator {
    tasks: Vec<Task>,
    state: HashMap<String, TaskState>,
    assignee: HashMap<String, String>, // task id → agent instance id
    busy: HashSet<String>,             // agents currently running a task
    locks: LockManager,
    mailbox: Mailbox,
}

impl Coordinator {
    /// Build a coordinator for `tasks` staffed by `team`. Errors if a task needs a role the team has no
    /// agent for.
    pub fn new(team: &TeamManifest, tasks: Vec<Task>) -> Result<Coordinator, String> {
        let agents = staff(team);
        let mut mailbox = Mailbox::new();
        let mut by_role: HashMap<Role, Vec<String>> = HashMap::new();
        for (id, role) in &agents {
            mailbox.register(id, *role);
            by_role.entry(*role).or_default().push(id.clone());
        }

        let mut assignee = HashMap::new();
        let mut rr: HashMap<Role, usize> = HashMap::new();
        let mut state = HashMap::new();
        for task in &tasks {
            let pool = by_role
                .get(&task.role)
                .filter(|p| !p.is_empty())
                .ok_or_else(|| format!("no agent for role {:?} (task {})", task.role, task.id))?;
            let i = rr.entry(task.role).or_insert(0);
            assignee.insert(task.id.clone(), pool[*i % pool.len()].clone());
            *i += 1;
            state.insert(task.id.clone(), TaskState::Pending);
        }

        Ok(Coordinator {
            tasks,
            state,
            assignee,
            busy: HashSet::new(),
            locks: LockManager::new(),
            mailbox,
        })
    }

    /// Kick off the mission: dispatch every task that can start now. Returns the dispatch intents.
    pub fn start(&mut self) -> Vec<Assignment> {
        self.advance()
    }

    /// Dispatch every Pending task whose assigned agent is free and whose files are free, claiming its
    /// locks and posting its assignment. Repeats to a fixpoint (dispatching one can't free anything, so
    /// a single pass suffices, but the loop is robust). Returns the newly-dispatched intents.
    fn advance(&mut self) -> Vec<Assignment> {
        let mut dispatched = Vec::new();
        loop {
            let mut progressed = false;
            // Collect candidates first to avoid borrowing self.tasks while mutating self.
            let ready: Vec<(String, String, Vec<String>)> = self
                .tasks
                .iter()
                .filter(|t| self.state[&t.id] == TaskState::Pending)
                .filter_map(|t| {
                    let agent = self.assignee[&t.id].clone();
                    if self.busy.contains(&agent) {
                        None
                    } else {
                        Some((t.id.clone(), agent, t.globs.clone()))
                    }
                })
                .collect();
            for (task_id, agent, globs) in ready {
                // Re-check the agent (an earlier candidate this pass may have made it busy).
                if self.busy.contains(&agent) {
                    continue;
                }
                // A reopened task (CPE-518 gate fail) still holds its lock — re-dispatch without
                // re-claiming; a fresh task must acquire its files.
                if self.locks.is_held(&task_id) || self.locks.try_claim(&task_id, globs) {
                    self.state.insert(task_id.clone(), TaskState::Running);
                    self.busy.insert(agent.clone());
                    let desc =
                        self.tasks.iter().find(|t| t.id == task_id).map(|t| t.description.clone()).unwrap_or_default();
                    self.mailbox.post("coordinator", Recipient::Agent(agent.clone()), "assign", &desc, 0);
                    dispatched.push(Assignment { task_id, agent_id: agent });
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
        dispatched
    }

    fn gate_of(&self, task_id: &str) -> Gate {
        self.tasks.iter().find(|t| t.id == task_id).map(|t| t.gate).unwrap_or(Gate::None)
    }
    fn desc_of(&self, task_id: &str) -> String {
        self.tasks.iter().find(|t| t.id == task_id).map(|t| t.description.clone()).unwrap_or_default()
    }

    /// Report a task's **work** finished. Its agent is freed. A task with no gate is Done immediately;
    /// a gated task moves to `Gating` (keeping its file locks) and its gate is requested — a `Review`
    /// gate posts a review request to the reviewers, a `Tests` gate waits for the driver to run tests.
    /// The gate result arrives via [`on_gate_pass`] / [`on_gate_fail`]. (CPE-518)
    pub fn on_done(&mut self, task_id: &str) -> Vec<Assignment> {
        if let Some(a) = self.assignee.get(task_id) {
            self.busy.remove(a);
        }
        match self.gate_of(task_id) {
            Gate::None => {
                self.locks.release(task_id);
                self.state.insert(task_id.to_string(), TaskState::Done);
            }
            gate => {
                self.state.insert(task_id.to_string(), TaskState::Gating); // locks stay held
                if gate == Gate::Review {
                    let desc = self.desc_of(task_id);
                    self.mailbox.post("coordinator", Recipient::Role(Role::Reviewer), "review", &desc, 0);
                }
            }
        }
        self.advance()
    }

    /// The task's quality gate passed → accept it: release its locks, mark Done, dispatch unblocked. (CPE-518)
    pub fn on_gate_pass(&mut self, task_id: &str) -> Vec<Assignment> {
        self.locks.release(task_id);
        self.state.insert(task_id.to_string(), TaskState::Done);
        self.advance()
    }

    /// The gate failed → **reopen** the task (back to Pending, keeping its locks) so its agent redoes
    /// the work; it will be re-dispatched. (CPE-518)
    pub fn on_gate_fail(&mut self, task_id: &str) -> Vec<Assignment> {
        self.state.insert(task_id.to_string(), TaskState::Pending);
        self.advance()
    }

    /// Report a task failed outright: free its agent + release locks, mark Failed, dispatch unblocked.
    /// (Retry policy is CPE-519.)
    pub fn on_failed(&mut self, task_id: &str) -> Vec<Assignment> {
        if let Some(a) = self.assignee.get(task_id) {
            self.busy.remove(a);
        }
        self.locks.release(task_id);
        self.state.insert(task_id.to_string(), TaskState::Failed);
        self.advance()
    }

    pub fn state_of(&self, task_id: &str) -> Option<TaskState> {
        self.state.get(task_id).copied()
    }
    /// (completed, total). "Completed" counts terminal tasks (Done or Failed).
    pub fn progress(&self) -> (usize, usize) {
        let done = self.state.values().filter(|s| matches!(s, TaskState::Done | TaskState::Failed)).count();
        (done, self.tasks.len())
    }
    /// Every task reached Done — the mission succeeded.
    pub fn is_complete(&self) -> bool {
        !self.tasks.is_empty() && self.state.values().all(|s| *s == TaskState::Done)
    }
    /// Any task Failed.
    pub fn has_failure(&self) -> bool {
        self.state.values().any(|s| *s == TaskState::Failed)
    }
    /// The mailbox (so the live layer / tests can read what agents were told).
    pub fn mailbox(&self) -> &Mailbox {
        &self.mailbox
    }
    pub fn assignee_of(&self, task_id: &str) -> Option<&str> {
        self.assignee.get(task_id).map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm_team::{default_team, RoleSpec, TeamManifest};

    fn task(id: &str, globs: &[&str]) -> Task {
        gated(id, globs, Gate::None)
    }
    fn gated(id: &str, globs: &[&str], gate: Gate) -> Task {
        Task {
            id: id.into(),
            description: format!("do {id}"),
            role: Role::Builder,
            globs: globs.iter().map(|s| s.to_string()).collect(),
            gate,
        }
    }
    fn team_with_reviewer() -> TeamManifest {
        TeamManifest {
            name: "T".into(),
            description: String::new(),
            roles: vec![
                RoleSpec { role: Role::Coordinator, agent: "claude".into(), model: None, count: 1 },
                RoleSpec { role: Role::Builder, agent: "claude".into(), model: None, count: 1 },
                RoleSpec { role: Role::Reviewer, agent: "claude".into(), model: None, count: 1 },
            ],
        }
    }

    fn team_with_builders(n: u32) -> TeamManifest {
        TeamManifest {
            name: "T".into(),
            description: String::new(),
            roles: vec![
                RoleSpec { role: Role::Coordinator, agent: "claude".into(), model: None, count: 1 },
                RoleSpec { role: Role::Builder, agent: "claude".into(), model: None, count: n },
            ],
        }
    }

    #[test]
    fn disjoint_tasks_run_in_parallel_when_agents_are_free() {
        let mut c = Coordinator::new(&team_with_builders(2), vec![task("t1", &["a/**"]), task("t2", &["b/**"])]).unwrap();
        let dispatched = c.start();
        assert_eq!(dispatched.len(), 2); // both builders, disjoint files → parallel
        assert_eq!(c.state_of("t1"), Some(TaskState::Running));
        assert_eq!(c.state_of("t2"), Some(TaskState::Running));
    }

    #[test]
    fn tasks_sharing_files_are_sequenced() {
        let mut c = Coordinator::new(&team_with_builders(2), vec![task("t1", &["src/**"]), task("t2", &["src/lib.rs"])]).unwrap();
        let first = c.start();
        assert_eq!(first.len(), 1); // only one — they overlap on src/
        // The other stays Pending until the first frees the files.
        let running = if c.state_of("t1") == Some(TaskState::Running) { "t1" } else { "t2" };
        let other = if running == "t1" { "t2" } else { "t1" };
        assert_eq!(c.state_of(other), Some(TaskState::Pending));
        let next = c.on_done(running);
        assert_eq!(next.len(), 1);
        assert_eq!(c.state_of(other), Some(TaskState::Running));
    }

    #[test]
    fn a_single_agent_serializes_even_disjoint_tasks() {
        let mut c = Coordinator::new(&team_with_builders(1), vec![task("t1", &["a/**"]), task("t2", &["b/**"])]).unwrap();
        assert_eq!(c.start().len(), 1); // one builder → one at a time
        let running = if c.state_of("t1") == Some(TaskState::Running) { "t1" } else { "t2" };
        let other = if running == "t1" { "t2" } else { "t1" };
        let next = c.on_done(running);
        assert_eq!(next.len(), 1);
        assert_eq!(c.state_of(other), Some(TaskState::Running));
    }

    #[test]
    fn mission_completes_when_every_task_is_done() {
        let mut c = Coordinator::new(&team_with_builders(2), vec![task("t1", &["a/**"]), task("t2", &["b/**"])]).unwrap();
        c.start();
        c.on_done("t1");
        assert!(!c.is_complete());
        c.on_done("t2");
        assert!(c.is_complete());
        assert_eq!(c.progress(), (2, 2));
    }

    #[test]
    fn a_dispatch_posts_an_assignment_to_the_agent_via_the_mailbox() {
        let mut c = Coordinator::new(&team_with_builders(1), vec![task("t1", &["a/**"])]).unwrap();
        c.start();
        let agent = c.assignee_of("t1").unwrap().to_string();
        let inbox = c.mailbox().read(&agent);
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].kind, "assign");
        assert_eq!(inbox[0].body, "do t1");
    }

    #[test]
    fn a_failure_frees_resources_and_lets_the_mission_continue() {
        let mut c = Coordinator::new(&team_with_builders(1), vec![task("t1", &["src/**"]), task("t2", &["src/x.rs"])]).unwrap();
        c.start();
        let running = if c.state_of("t1") == Some(TaskState::Running) { "t1" } else { "t2" };
        let other = if running == "t1" { "t2" } else { "t1" };
        c.on_failed(running);
        assert_eq!(c.state_of(running), Some(TaskState::Failed));
        assert_eq!(c.state_of(other), Some(TaskState::Running)); // its files + agent freed up
        assert!(c.has_failure());
    }

    #[test]
    fn new_errors_when_a_task_needs_an_unstaffed_role() {
        let scout_task = Task { id: "s".into(), description: "scan".into(), role: Role::Scout, globs: vec![], gate: Gate::None };
        // default_team has no scout.
        assert!(Coordinator::new(&default_team(), vec![scout_task]).is_err());
    }

    // --- Quality gates (CPE-518) --------------------------------------------------------
    #[test]
    fn a_gated_task_awaits_its_gate_before_done() {
        let mut c = Coordinator::new(&team_with_reviewer(), vec![gated("t1", &["a/**"], Gate::Tests)]).unwrap();
        c.start();
        c.on_done("t1"); // work finished, but a gate is required
        assert_eq!(c.state_of("t1"), Some(TaskState::Gating));
        assert!(!c.is_complete()); // NOT done until the gate passes
        c.on_gate_pass("t1");
        assert_eq!(c.state_of("t1"), Some(TaskState::Done));
        assert!(c.is_complete());
    }

    #[test]
    fn a_failed_gate_reopens_the_task_for_rework() {
        let mut c = Coordinator::new(&team_with_reviewer(), vec![gated("t1", &["a/**"], Gate::Tests)]).unwrap();
        c.start();
        c.on_done("t1");
        let redispatch = c.on_gate_fail("t1"); // gate failed → reopen
        assert_eq!(redispatch.len(), 1); // the same task is dispatched again to fix it
        assert_eq!(c.state_of("t1"), Some(TaskState::Running));
        // Fixing + passing the gate finally completes it.
        c.on_done("t1");
        c.on_gate_pass("t1");
        assert!(c.is_complete());
    }

    #[test]
    fn a_review_gate_asks_a_reviewer_over_the_mailbox() {
        let mut c = Coordinator::new(&team_with_reviewer(), vec![gated("t1", &["a/**"], Gate::Review)]).unwrap();
        c.start();
        c.on_done("t1");
        // The reviewer instance got a "review" request.
        let reviewer = "claude#reviewer1";
        let inbox = c.mailbox().read(reviewer);
        assert!(inbox.iter().any(|m| m.kind == "review" && m.body == "do t1"));
    }

    #[test]
    fn a_gating_task_keeps_its_files_locked_so_overlaps_still_wait() {
        // One builder, two tasks sharing files; the first is gated. While it's Gating, the second must
        // still wait (its files are held), even though the agent is free.
        let mut c = Coordinator::new(
            &team_with_reviewer(),
            vec![gated("t1", &["src/**"], Gate::Tests), gated("t2", &["src/lib.rs"], Gate::None)],
        )
        .unwrap();
        c.start(); // t1 runs (assigned first)
        assert_eq!(c.state_of("t1"), Some(TaskState::Running));
        c.on_done("t1"); // → Gating, locks held
        assert_eq!(c.state_of("t2"), Some(TaskState::Pending)); // still blocked on files
        c.on_gate_pass("t1"); // releases files
        assert_eq!(c.state_of("t2"), Some(TaskState::Running));
    }
}
