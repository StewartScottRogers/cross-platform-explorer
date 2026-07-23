//! Thumbnail request queue (CPE-950, epic CPE-718): the pure priority scheduler for the thumbnail
//! pipeline — enqueue thumbnail keys at a priority (Visible on-screen > Prefetch > Background), dedupe,
//! **promote** a re-requested key to a higher priority, and dequeue the highest-priority next key
//! (FIFO within a priority). Pairs with [`crate::thumb_cache`] (CPE-939). No image work here.

use std::collections::{HashMap, VecDeque};

/// Request priority, highest first. `Visible` = an on-screen row the user is looking at now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Visible,
    Prefetch,
    Background,
}

/// A dedupe-ing, priority-ordered queue of thumbnail keys.
#[derive(Debug, Clone, Default)]
pub struct ThumbQueue {
    visible: VecDeque<String>,
    prefetch: VecDeque<String>,
    background: VecDeque<String>,
    /// The current priority of every queued key (for dedupe + promotion).
    at: HashMap<String, Priority>,
}

impl ThumbQueue {
    pub fn new() -> Self {
        Self::default()
    }

    fn lane(&mut self, p: Priority) -> &mut VecDeque<String> {
        match p {
            Priority::Visible => &mut self.visible,
            Priority::Prefetch => &mut self.prefetch,
            Priority::Background => &mut self.background,
        }
    }

    /// Enqueue `key` at `priority`. New keys are appended to their lane. A key already queued at a
    /// **lower** priority is **promoted** (moved to the higher lane, at the back); one already at an
    /// equal-or-higher priority is left where it is (no duplicate).
    pub fn enqueue(&mut self, key: &str, priority: Priority) {
        match self.at.get(key).copied() {
            Some(existing) if existing <= priority => {} // already same-or-higher priority (Visible < …)
            Some(existing) => {
                // Promote: remove from the old lane, add to the new.
                let old = self.lane(existing);
                if let Some(i) = old.iter().position(|k| k == key) {
                    old.remove(i);
                }
                self.lane(priority).push_back(key.to_string());
                self.at.insert(key.to_string(), priority);
            }
            None => {
                self.lane(priority).push_back(key.to_string());
                self.at.insert(key.to_string(), priority);
            }
        }
    }

    /// Pop the next key to render — highest priority first, FIFO within a priority. `None` when empty.
    /// (Not an `Iterator`: it drains a priority queue, it doesn't iterate a sequence.)
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<String> {
        let key = self
            .visible
            .pop_front()
            .or_else(|| self.prefetch.pop_front())
            .or_else(|| self.background.pop_front())?;
        self.at.remove(&key);
        Some(key)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.at.contains_key(key)
    }
    pub fn len(&self) -> usize {
        self.at.len()
    }
    pub fn is_empty(&self) -> bool {
        self.at.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dequeues_highest_priority_first_then_fifo() {
        let mut q = ThumbQueue::new();
        q.enqueue("bg1", Priority::Background);
        q.enqueue("vis1", Priority::Visible);
        q.enqueue("pf1", Priority::Prefetch);
        q.enqueue("vis2", Priority::Visible);
        assert_eq!(q.next(), Some("vis1".into())); // visible, in order
        assert_eq!(q.next(), Some("vis2".into()));
        assert_eq!(q.next(), Some("pf1".into())); // then prefetch
        assert_eq!(q.next(), Some("bg1".into())); // then background
        assert_eq!(q.next(), None);
    }

    #[test]
    fn enqueue_dedupes_by_key() {
        let mut q = ThumbQueue::new();
        q.enqueue("a", Priority::Background);
        q.enqueue("a", Priority::Background); // duplicate → ignored
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn re_request_promotes_but_never_demotes() {
        let mut q = ThumbQueue::new();
        q.enqueue("a", Priority::Background);
        q.enqueue("b", Priority::Background);
        q.enqueue("a", Priority::Visible); // 'a' scrolled on-screen → promote
        assert_eq!(q.len(), 2);
        assert_eq!(q.next(), Some("a".into())); // 'a' now visible, comes first
        assert_eq!(q.next(), Some("b".into()));
        // Demotion is a no-op: a visible key stays visible if re-queued as background.
        q.enqueue("c", Priority::Visible);
        q.enqueue("c", Priority::Background);
        assert_eq!(q.next(), Some("c".into())); // still popped from the visible lane
    }

    #[test]
    fn empty_queue_is_safe() {
        let mut q = ThumbQueue::new();
        assert!(q.is_empty());
        assert_eq!(q.next(), None);
        assert!(!q.contains("x"));
    }
}
