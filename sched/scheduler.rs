//! Credit-based scheduler algorithms — outside Proven Core.
//!
//! Pillar: none directly (platform utility) · keep outside (ADR-002)
//! VERIFICATION: N/A

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedError {
    NoRunnable,
}

/// Minimal credit scheduler stub (M4+).
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

    pub fn pick_next(&self) -> Result<usize, SchedError> {
        let mut best: Option<(usize, i32)> = None;
        for id in 0..self.count {
            let c = self.credits[id];
            if c <= 0 {
                continue;
            }
            match best {
                None => best = Some((id, c)),
                Some((_, bc)) if c > bc => best = Some((id, c)),
                _ => {}
            }
        }
        best.map(|(id, _)| id).ok_or(SchedError::NoRunnable)
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
