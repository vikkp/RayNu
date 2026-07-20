//! Credit-based scheduler algorithms — outside Proven Core.
//!
//! Pillar: none directly (platform utility) · keep outside (ADR-002)
//! VERIFICATION: N/A
//!
//! M4.1: quantum consume + replenish so ≥2 vCPUs alternate under preemption.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedError {
    NoRunnable,
}

/// Default credit granted on register / replenish (M4.1).
pub const DEFAULT_CREDIT: i32 = 10;

/// Marker when both guests have received a scheduled time-slice.
pub const M4_SCHED_OK_MARKER: &str = "RAYNU-V-M4-SCHED-OK";

/// Marker when G0 ran under the M4.1 scheduler.
pub const M4_SLICE_G0_MARKER: &str = "RAYNU-V-M4-SLICE-G0";

/// Marker when G1 ran under the M4.1 scheduler.
pub const M4_SLICE_G1_MARKER: &str = "RAYNU-V-M4-SLICE-G1";

/// Marker when G2 ran under the M4.2 scheduler.
pub const M4_SLICE_G2_MARKER: &str = "RAYNU-V-M4-SLICE-G2";

/// Marker when G3 ran under the M4.2 scheduler.
pub const M4_SLICE_G3_MARKER: &str = "RAYNU-V-M4-SLICE-G3";

/// Marker when ≥4 guests have progressed under the scheduler (M4.2).
pub const M4_NVM_OK_MARKER: &str = "RAYNU-V-M4-NVM-OK";

/// Minimal credit scheduler (M4.1).
pub struct CreditScheduler {
    credits: [i32; 8],
    count: usize,
}

impl CreditScheduler {
    pub const fn new() -> Self {
        Self {
            credits: [0; 8],
            count: 0,
        }
    }

    pub fn register_vcpu(&mut self, initial_credit: i32) -> Option<usize> {
        if self.count >= self.credits.len() {
            return None;
        }
        let id = self.count;
        self.credits[id] = initial_credit;
        self.count += 1;
        Some(id)
    }

    pub fn credit(&self, id: usize) -> Option<i32> {
        if id < self.count {
            Some(self.credits[id])
        } else {
            None
        }
    }

    pub fn vcpu_count(&self) -> usize {
        self.count
    }

    /// Pick the runnable vCPU with the highest positive credit.
    /// On a tie, prefer `prefer_not` when set (round-robin fairness).
    pub fn pick_next(&self) -> Result<usize, SchedError> {
        self.pick_next_fair(None)
    }

    pub fn pick_next_fair(&self, prefer_not: Option<usize>) -> Result<usize, SchedError> {
        let mut best: Option<(usize, i32)> = None;
        for id in 0..self.count {
            let c = self.credits[id];
            if c <= 0 {
                continue;
            }
            match best {
                None => best = Some((id, c)),
                Some((_, bc)) if c > bc => best = Some((id, c)),
                Some((bid, bc)) if c == bc => {
                    // Tie: prefer switching away from the current vCPU.
                    if prefer_not == Some(bid) {
                        best = Some((id, c));
                    }
                }
                _ => {}
            }
        }
        best.map(|(id, _)| id).ok_or(SchedError::NoRunnable)
    }

    /// End the current quantum: zero this vCPU's credit; replenish all if starved.
    pub fn consume_quantum(&mut self, id: usize) {
        self.consume_quantum_amount(id, DEFAULT_CREDIT);
    }

    pub fn consume_quantum_amount(&mut self, id: usize, replenish: i32) {
        if id < self.count {
            self.credits[id] = 0;
        }
        let any_runnable = (0..self.count).any(|i| self.credits[i] > 0);
        if !any_runnable {
            for i in 0..self.count {
                self.credits[i] = replenish;
            }
        }
    }
}

impl Default for CreditScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "scheduler_test.rs"]
mod scheduler_test;
