//! Resource governance & performance budgets (CPE-297).
//!
//! "Off = off" protects the explorer when a sidecar is disabled; this protects it when
//! one is *enabled*. A runaway sidecar (or a spawned agent) must not degrade the
//! explorer, so the supervisor samples each sidecar's memory against a budget and can
//! throttle/restart a breacher. Sampling is abstracted behind [`MemorySampler`] for
//! testability; [`SysinfoSampler`] does the real per-process read. The IPC channel is
//! separately bounded (see `supervisor`) so PTY/log output applies backpressure rather
//! than buffering without limit.

/// A per-sidecar resource budget.
#[derive(Debug, Clone, Copy)]
pub struct ResourceBudget {
    /// Resident memory ceiling in bytes for the sidecar process (and, conceptually, the
    /// tree it spawns).
    pub max_memory_bytes: u64,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        // A generous default ceiling; the host may tighten per sidecar.
        Self { max_memory_bytes: 1024 * 1024 * 1024 }
    }
}

/// The result of checking a sidecar against its budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Within budget (carries the sampled usage).
    Within { used: u64 },
    /// Over budget — the supervisor should warn and throttle/restart.
    Over { used: u64, limit: u64 },
    /// The process could not be sampled (already gone).
    Unknown,
}

/// Samples a process's resident memory in bytes.
pub trait MemorySampler {
    fn sample_rss(&mut self, pid: u32) -> Option<u64>;
}

/// Check `pid` against `budget` using `sampler`.
pub fn check(sampler: &mut dyn MemorySampler, pid: u32, budget: &ResourceBudget) -> Verdict {
    match sampler.sample_rss(pid) {
        Some(used) if used > budget.max_memory_bytes => {
            Verdict::Over { used, limit: budget.max_memory_bytes }
        }
        Some(used) => Verdict::Within { used },
        None => Verdict::Unknown,
    }
}

/// The real sampler, backed by `sysinfo`.
pub struct SysinfoSampler {
    sys: sysinfo::System,
}

impl Default for SysinfoSampler {
    fn default() -> Self {
        Self { sys: sysinfo::System::new() }
    }
}

impl MemorySampler for SysinfoSampler {
    fn sample_rss(&mut self, pid: u32) -> Option<u64> {
        let pid = sysinfo::Pid::from_u32(pid);
        self.sys.refresh_process(pid);
        self.sys.process(pid).map(|p| p.memory())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedSampler(Option<u64>);
    impl MemorySampler for FixedSampler {
        fn sample_rss(&mut self, _pid: u32) -> Option<u64> {
            self.0
        }
    }

    #[test]
    fn within_budget() {
        let mut s = FixedSampler(Some(100));
        assert_eq!(check(&mut s, 1, &ResourceBudget { max_memory_bytes: 200 }), Verdict::Within { used: 100 });
    }

    #[test]
    fn over_budget_is_flagged_with_usage_and_limit() {
        let mut s = FixedSampler(Some(300));
        assert_eq!(
            check(&mut s, 1, &ResourceBudget { max_memory_bytes: 200 }),
            Verdict::Over { used: 300, limit: 200 }
        );
    }

    #[test]
    fn a_gone_process_is_unknown() {
        let mut s = FixedSampler(None);
        assert_eq!(check(&mut s, 1, &ResourceBudget::default()), Verdict::Unknown);
    }

    #[test]
    fn the_real_sampler_reads_this_process_memory() {
        let mut s = SysinfoSampler::default();
        let used = s.sample_rss(std::process::id());
        assert!(used.is_some_and(|b| b > 0), "should sample this process's own memory");
    }
}
