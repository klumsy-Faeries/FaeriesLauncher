//! Startup timing (§43).
//!
//! Records how long each initialization stage takes and logs a report, so a
//! slow launch names the stage responsible instead of leaving you guessing.
//! The cost is one `Instant::now()` per stage — nothing worth gating.

use std::time::{Duration, Instant};

pub struct Startup {
    began: Instant,
    last: Instant,
    stages: Vec<(&'static str, Duration)>,
}

impl Startup {
    pub fn begin() -> Self {
        let now = Instant::now();
        Self {
            began: now,
            last: now,
            stages: Vec::new(),
        }
    }

    /// Record the time since the previous stage.
    pub fn stage(&mut self, name: &'static str) {
        let now = Instant::now();
        self.stages.push((name, now.duration_since(self.last)));
        self.last = now;
    }

    pub fn total(&self) -> Duration {
        self.began.elapsed()
    }

    /// One line per stage plus a total, at INFO so it lands in every log.
    pub fn report(&self) {
        let total = self.total();
        let breakdown = self
            .stages
            .iter()
            .map(|(name, elapsed)| format!("{name} {:.1}ms", elapsed.as_secs_f64() * 1000.0))
            .collect::<Vec<_>>()
            .join(" · ");
        tracing::info!(
            total_ms = total.as_millis() as u64,
            "startup complete in {:.1}ms — {breakdown}",
            total.as_secs_f64() * 1000.0
        );
    }

    /// Stage timings, for the performance dashboard.
    pub fn timings(&self) -> Vec<(String, f64)> {
        self.stages
            .iter()
            .map(|(name, elapsed)| (name.to_string(), elapsed.as_secs_f64() * 1000.0))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_are_recorded_in_order_and_sum_to_the_total() {
        let mut startup = Startup::begin();
        std::thread::sleep(Duration::from_millis(5));
        startup.stage("first");
        std::thread::sleep(Duration::from_millis(5));
        startup.stage("second");

        let timings = startup.timings();
        assert_eq!(timings.len(), 2);
        assert_eq!(timings[0].0, "first");
        assert_eq!(timings[1].0, "second");
        assert!(
            timings[0].1 >= 4.0,
            "first stage measured: {}",
            timings[0].1
        );

        let summed: f64 = timings.iter().map(|(_, ms)| ms).sum();
        let total = startup.total().as_secs_f64() * 1000.0;
        assert!(
            summed <= total + 1.0,
            "stages ({summed:.1}ms) cannot exceed the total ({total:.1}ms)"
        );
    }

    #[test]
    fn a_startup_with_no_stages_still_reports() {
        let startup = Startup::begin();
        assert!(startup.timings().is_empty());
        startup.report();
    }
}
