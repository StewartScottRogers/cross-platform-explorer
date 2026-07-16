//! Swarm inter-agent mailbox (CPE-516) — the coordination channel for [CPE-502] Swarm orchestration.
//! Agents post messages addressed to another agent, a whole role, or the broadcast channel, and read
//! their own ordered inbox. This module is the **pure substrate**: an in-process, ordered, contained
//! mailbox with no I/O and no egress. It is exposed to agent *processes* over the MCP layer by a thin
//! adapter (the tool surface below); the substrate itself is transport-agnostic and unit-testable.
//!
//! ## MCP tool surface (the adapter contract — CPE-516)
//! A mailbox MCP server maps two tools onto this substrate, so any agent that speaks MCP can coordinate:
//! - `mailbox.post { to: {agent|role|broadcast}, kind, body }` → [`Mailbox::post`]
//! - `mailbox.read { drain?: bool }` → [`Mailbox::read`] / [`Mailbox::drain`] for the calling agent
//!
//! Containment: delivery never leaves the process — there is no network path, so a message can't be
//! exfiltrated. Redaction: messages are plain data; the host's `Redactor` scrubs them at the logging
//! boundary exactly as elsewhere — the mailbox itself logs nothing. If MCP is unavailable the adapter
//! degrades (the agent simply can't reach the mailbox), while in-process users keep working.

use crate::swarm_team::Role;
use std::collections::HashMap;

/// Who a message is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recipient {
    /// One specific agent, by id.
    Agent(String),
    /// Every agent currently cast in this role (except the sender).
    Role(Role),
    /// Every registered agent (except the sender).
    Broadcast,
}

/// A coordination message. `seq` is assigned by the mailbox (monotonic) and orders each inbox; `ts` is
/// a caller-supplied timestamp (kept out of the pure core so it stays deterministic in tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub seq: u64,
    pub from: String,
    pub to: Recipient,
    pub kind: String,
    pub body: String,
    pub ts: u64,
}

/// In-process, ordered, contained inter-agent mailbox (CPE-516).
#[derive(Debug, Default)]
pub struct Mailbox {
    /// agent id → role, so role-addressed + broadcast messages can be resolved.
    members: HashMap<String, Role>,
    /// agent id → its ordered inbox.
    inboxes: HashMap<String, Vec<Message>>,
    next_seq: u64,
}

impl Mailbox {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or re-role) an agent so it can receive role/broadcast messages. Idempotent.
    pub fn register(&mut self, agent_id: &str, role: Role) {
        self.members.insert(agent_id.to_string(), role);
        self.inboxes.entry(agent_id.to_string()).or_default();
    }

    /// Remove an agent from the team (its inbox is dropped).
    pub fn unregister(&mut self, agent_id: &str) {
        self.members.remove(agent_id);
        self.inboxes.remove(agent_id);
    }

    pub fn members(&self) -> impl Iterator<Item = (&String, &Role)> {
        self.members.iter()
    }

    /// Resolve the concrete recipient agent ids for a `to`, excluding the sender for role/broadcast
    /// (you don't get your own broadcast). An explicit `Agent(id)` always resolves to that id.
    fn resolve(&self, from: &str, to: &Recipient) -> Vec<String> {
        match to {
            Recipient::Agent(id) => vec![id.clone()],
            Recipient::Role(r) => self
                .members
                .iter()
                .filter(|(id, role)| id.as_str() != from && *role == r)
                .map(|(id, _)| id.clone())
                .collect(),
            Recipient::Broadcast => self
                .members
                .keys()
                .filter(|id| id.as_str() != from)
                .cloned()
                .collect(),
        }
    }

    /// Post a message. Returns its assigned `seq`. A clone lands in each recipient's inbox in order;
    /// an `Agent(id)` recipient gets an inbox on demand even if it wasn't a registered member.
    pub fn post(&mut self, from: &str, to: Recipient, kind: &str, body: &str, ts: u64) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        for id in self.resolve(from, &to) {
            let msg = Message {
                seq,
                from: from.to_string(),
                to: to.clone(),
                kind: kind.to_string(),
                body: body.to_string(),
                ts,
            };
            self.inboxes.entry(id).or_default().push(msg);
        }
        seq
    }

    /// Peek an agent's inbox in order (does not clear it).
    pub fn read(&self, agent_id: &str) -> &[Message] {
        self.inboxes.get(agent_id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Take and clear an agent's inbox (ordered).
    pub fn drain(&mut self, agent_id: &str) -> Vec<Message> {
        self.inboxes.get_mut(agent_id).map(std::mem::take).unwrap_or_default()
    }

    pub fn inbox_len(&self, agent_id: &str) -> usize {
        self.inboxes.get(agent_id).map_or(0, |v| v.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team() -> Mailbox {
        let mut mb = Mailbox::new();
        mb.register("coord", Role::Coordinator);
        mb.register("b1", Role::Builder);
        mb.register("b2", Role::Builder);
        mb.register("rev", Role::Reviewer);
        mb
    }

    #[test]
    fn direct_message_reaches_only_the_addressee() {
        let mut mb = team();
        mb.post("coord", Recipient::Agent("b1".into()), "task", "build the parser", 0);
        assert_eq!(mb.inbox_len("b1"), 1);
        assert_eq!(mb.read("b1")[0].body, "build the parser");
        assert_eq!(mb.inbox_len("b2"), 0);
        assert_eq!(mb.inbox_len("coord"), 0);
    }

    #[test]
    fn role_message_reaches_every_agent_in_that_role_but_not_the_sender() {
        let mut mb = team();
        mb.post("coord", Recipient::Role(Role::Builder), "sync", "pull latest", 0);
        assert_eq!(mb.inbox_len("b1"), 1);
        assert_eq!(mb.inbox_len("b2"), 1);
        assert_eq!(mb.inbox_len("rev"), 0); // not a builder
        assert_eq!(mb.inbox_len("coord"), 0); // sender excluded
    }

    #[test]
    fn broadcast_reaches_everyone_except_the_sender() {
        let mut mb = team();
        mb.post("b1", Recipient::Broadcast, "note", "found a flake", 0);
        assert_eq!(mb.inbox_len("coord"), 1);
        assert_eq!(mb.inbox_len("b2"), 1);
        assert_eq!(mb.inbox_len("rev"), 1);
        assert_eq!(mb.inbox_len("b1"), 0); // sender excluded
    }

    #[test]
    fn inbox_preserves_post_order_per_recipient() {
        let mut mb = team();
        mb.post("coord", Recipient::Agent("b1".into()), "t", "first", 0);
        mb.post("rev", Recipient::Agent("b1".into()), "t", "second", 0);
        let seqs: Vec<u64> = mb.read("b1").iter().map(|m| m.seq).collect();
        assert_eq!(seqs, vec![0, 1]);
        assert_eq!(mb.read("b1")[0].body, "first");
        assert_eq!(mb.read("b1")[1].body, "second");
    }

    #[test]
    fn drain_returns_then_clears_the_inbox() {
        let mut mb = team();
        mb.post("coord", Recipient::Agent("b1".into()), "t", "x", 0);
        let got = mb.drain("b1");
        assert_eq!(got.len(), 1);
        assert_eq!(mb.inbox_len("b1"), 0);
        assert!(mb.drain("b1").is_empty()); // idempotent once drained
    }

    #[test]
    fn unregister_removes_a_member_from_role_and_broadcast() {
        let mut mb = team();
        mb.unregister("b2");
        mb.post("coord", Recipient::Role(Role::Builder), "t", "x", 0);
        assert_eq!(mb.inbox_len("b1"), 1);
        assert_eq!(mb.inbox_len("b2"), 0); // gone
    }

    #[test]
    fn addressing_an_unknown_agent_creates_its_inbox_on_demand() {
        let mut mb = Mailbox::new();
        mb.post("coord", Recipient::Agent("ghost".into()), "t", "hi", 0);
        assert_eq!(mb.inbox_len("ghost"), 1);
    }
}
