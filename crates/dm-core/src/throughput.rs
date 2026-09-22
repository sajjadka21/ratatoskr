use std::time::{Duration, Instant};

/// Shortest window that produces a rate sample. Anything shorter is dominated
/// by chunk boundaries rather than by actual throughput.
const MIN_SAMPLE_WINDOW: Duration = Duration::from_millis(250);

/// Weight given to the newest window. Low enough that a single slow or fast
/// window does not make the displayed rate jump.
const SMOOTHING: f64 = 0.35;

/// Smoothed transfer-rate estimate built from the byte counts the engine
/// actually wrote. Deliberately simple: an exponentially weighted moving
/// average over fixed-length windows, which is stable enough to display and
/// cheap enough to run inside the progress callback.
///
/// Phase 5 will read throughput history from this same measurement point when
/// it decides whether extra connections are helping.
#[derive(Debug, Clone)]
pub struct ThroughputMeter {
    window_started_at: Instant,
    window_start_bytes: u64,
    smoothed_bytes_per_second: Option<f64>,
}

impl ThroughputMeter {
    pub fn new(started_at: Instant, initial_bytes: u64) -> Self {
        Self {
            window_started_at: started_at,
            window_start_bytes: initial_bytes,
            smoothed_bytes_per_second: None,
        }
    }

    /// Feeds a cumulative byte count in. Returns the current estimate, which
    /// only changes once a full sample window has elapsed.
    pub fn sample(&mut self, downloaded_bytes: u64, at: Instant) -> Option<u64> {
        let elapsed = at.saturating_duration_since(self.window_started_at);

        if elapsed < MIN_SAMPLE_WINDOW {
            return self.bytes_per_second();
        }

        let window_bytes = downloaded_bytes.saturating_sub(self.window_start_bytes);
        let window_rate = window_bytes as f64 / elapsed.as_secs_f64();

        self.smoothed_bytes_per_second = Some(match self.smoothed_bytes_per_second {
            Some(previous) => SMOOTHING * window_rate + (1.0 - SMOOTHING) * previous,
            None => window_rate,
        });

        self.window_started_at = at;
        self.window_start_bytes = downloaded_bytes;

        self.bytes_per_second()
    }

    pub fn bytes_per_second(&self) -> Option<u64> {
        let rate = self.smoothed_bytes_per_second?;

        if rate.is_finite() && rate >= 1.0 {
            Some(rate as u64)
        } else {
            Some(0)
        }
    }

    /// Seconds remaining at the current rate, or `None` while the total size or
    /// the rate is unknown. A stalled transfer reports no estimate rather than
    /// an infinite one.
    pub fn eta_seconds(&self, downloaded_bytes: u64, total_bytes: Option<u64>) -> Option<u64> {
        let total = total_bytes?;
        let remaining = total.saturating_sub(downloaded_bytes);

        if remaining == 0 {
            return Some(0);
        }

        let rate = self.smoothed_bytes_per_second?;

        if !rate.is_finite() || rate < 1.0 {
            return None;
        }

        Some((remaining as f64 / rate).ceil() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_no_rate_before_a_full_sample_window() {
        let start = Instant::now();
        let mut meter = ThroughputMeter::new(start, 0);

        assert_eq!(meter.sample(1_024, start + Duration::from_millis(50)), None);
    }

    #[test]
    fn measures_the_rate_of_the_first_complete_window() {
        let start = Instant::now();
        let mut meter = ThroughputMeter::new(start, 0);

        let rate = meter
            .sample(1_000_000, start + Duration::from_secs(1))
            .unwrap();

        assert_eq!(rate, 1_000_000);
    }

    #[test]
    fn smooths_instead_of_following_a_single_slow_window() {
        let start = Instant::now();
        let mut meter = ThroughputMeter::new(start, 0);

        meter.sample(1_000_000, start + Duration::from_secs(1));
        let rate = meter
            .sample(1_000_000, start + Duration::from_secs(2))
            .unwrap();

        assert!(
            (600_000..1_000_000).contains(&rate),
            "a single idle window must not drop the estimate to zero: {rate}"
        );
    }

    #[test]
    fn estimates_remaining_time_from_the_measured_rate() {
        let start = Instant::now();
        let mut meter = ThroughputMeter::new(start, 0);

        meter.sample(1_000_000, start + Duration::from_secs(1));

        assert_eq!(
            meter.eta_seconds(1_000_000, Some(3_000_000)),
            Some(2),
            "two million bytes left at one million bytes per second"
        );
    }

    #[test]
    fn reports_no_estimate_without_a_known_total() {
        let start = Instant::now();
        let mut meter = ThroughputMeter::new(start, 0);

        meter.sample(1_000_000, start + Duration::from_secs(1));

        assert_eq!(meter.eta_seconds(1_000_000, None), None);
    }

    #[test]
    fn a_finished_transfer_has_no_time_remaining() {
        let start = Instant::now();
        let meter = ThroughputMeter::new(start, 0);

        assert_eq!(meter.eta_seconds(4_096, Some(4_096)), Some(0));
    }

    #[test]
    fn a_stalled_transfer_reports_no_estimate() {
        let start = Instant::now();
        let mut meter = ThroughputMeter::new(start, 0);

        for second in 1..=8 {
            meter.sample(0, start + Duration::from_secs(second));
        }

        assert_eq!(meter.bytes_per_second(), Some(0));
        assert_eq!(meter.eta_seconds(0, Some(1_024)), None);
    }
}
