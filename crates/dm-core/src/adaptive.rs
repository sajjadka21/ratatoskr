use std::{fmt, time::Duration};

const MIN_CONNECTIONS: usize = 1;
const MAX_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptiveReason {
    StartingConservatively,
    ThroughputImproved,
    DiminishingReturns,
    ServerRateLimited,
    ServerBusy,
    Stable,
}

impl AdaptiveReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StartingConservatively => "starting conservatively",
            Self::ThroughputImproved => "throughput improved",
            Self::DiminishingReturns => "no gain from additional streams",
            Self::ServerRateLimited => "server returned 429",
            Self::ServerBusy => "server returned 503",
            Self::Stable => "connection count stable",
        }
    }
}

impl fmt::Display for AdaptiveReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveDecision {
    pub target_connections: usize,
    pub reason: AdaptiveReason,
    pub backoff: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThroughputSample {
    pub connections: usize,
    pub bytes_per_second: u64,
}

#[derive(Debug, Clone)]
pub struct AdaptiveController {
    max_connections: usize,
    target_connections: usize,
    previous_sample: Option<ThroughputSample>,
    reason: AdaptiveReason,
    backoff_attempts: u32,
}

impl AdaptiveController {
    pub fn new(max_connections: usize) -> Self {
        Self {
            max_connections: max_connections.max(MIN_CONNECTIONS),
            target_connections: MIN_CONNECTIONS,
            previous_sample: None,
            reason: AdaptiveReason::StartingConservatively,
            backoff_attempts: 0,
        }
    }

    pub const fn target_connections(&self) -> usize {
        self.target_connections
    }

    pub const fn max_connections(&self) -> usize {
        self.max_connections
    }

    pub const fn reason(&self) -> AdaptiveReason {
        self.reason
    }

    pub fn observe(&mut self, sample: ThroughputSample) -> AdaptiveDecision {
        let previous = self.previous_sample.replace(sample);
        self.backoff_attempts = 0;

        let Some(previous) = previous else {
            self.reason = AdaptiveReason::StartingConservatively;
            return self.decision(None);
        };

        if sample.connections != previous.connections {
            self.reason =
                if sample.bytes_per_second >= previous.bytes_per_second.saturating_mul(11) / 10 {
                    AdaptiveReason::ThroughputImproved
                } else {
                    AdaptiveReason::DiminishingReturns
                };
            return self.decision(None);
        }

        if sample.bytes_per_second >= previous.bytes_per_second.saturating_mul(11) / 10
            && self.target_connections < self.max_connections
        {
            self.target_connections += 1;
            self.reason = AdaptiveReason::ThroughputImproved;
        } else if sample.bytes_per_second <= previous.bytes_per_second.saturating_mul(9) / 10
            && self.target_connections > MIN_CONNECTIONS
        {
            self.target_connections -= 1;
            self.reason = AdaptiveReason::DiminishingReturns;
        } else if self.target_connections < self.max_connections {
            // One sample establishes the baseline; the next sample probes
            // one additional stream so its measured result can be compared.
            self.target_connections += 1;
            self.reason = AdaptiveReason::ThroughputImproved;
        } else {
            self.reason = AdaptiveReason::Stable;
        }

        self.decision(None)
    }

    pub fn record_server_status(&mut self, status: u16) -> Option<AdaptiveDecision> {
        let reason = match status {
            429 => AdaptiveReason::ServerRateLimited,
            503 => AdaptiveReason::ServerBusy,
            _ => return None,
        };
        self.target_connections = self
            .target_connections
            .saturating_sub(1)
            .max(MIN_CONNECTIONS);
        self.reason = reason;
        self.backoff_attempts = self.backoff_attempts.saturating_add(1);
        Some(self.decision(Some(self.backoff_delay())))
    }

    pub fn backoff_delay(&self) -> Duration {
        let seconds = 2_u64.saturating_pow(self.backoff_attempts.min(5));
        Duration::from_secs(seconds).min(MAX_BACKOFF)
    }

    fn decision(&self, backoff: Option<Duration>) -> AdaptiveDecision {
        AdaptiveDecision {
            target_connections: self.target_connections,
            reason: self.reason,
            backoff,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_one_connection_and_explains_it() {
        let mut controller = AdaptiveController::new(8);

        assert_eq!(controller.target_connections(), 1);
        assert_eq!(controller.reason(), AdaptiveReason::StartingConservatively);
        assert_eq!(
            controller
                .observe(ThroughputSample {
                    connections: 1,
                    bytes_per_second: 100
                })
                .target_connections,
            1
        );
    }

    #[test]
    fn scales_up_only_after_measured_gain() {
        let mut controller = AdaptiveController::new(4);
        controller.observe(ThroughputSample {
            connections: 1,
            bytes_per_second: 100,
        });

        let decision = controller.observe(ThroughputSample {
            connections: 1,
            bytes_per_second: 120,
        });

        assert_eq!(decision.target_connections, 2);
        assert_eq!(decision.reason, AdaptiveReason::ThroughputImproved);
    }

    #[test]
    fn probes_one_more_connection_after_a_baseline_sample() {
        let mut controller = AdaptiveController::new(4);
        controller.observe(ThroughputSample {
            connections: 1,
            bytes_per_second: 100,
        });

        let decision = controller.observe(ThroughputSample {
            connections: 1,
            bytes_per_second: 105,
        });

        assert_eq!(decision.target_connections, 2);
        assert_eq!(decision.reason, AdaptiveReason::ThroughputImproved);
    }

    #[test]
    fn caps_target_and_reports_diminishing_returns() {
        let mut controller = AdaptiveController::new(2);
        controller.observe(ThroughputSample {
            connections: 1,
            bytes_per_second: 100,
        });
        controller.observe(ThroughputSample {
            connections: 1,
            bytes_per_second: 120,
        });

        let decision = controller.observe(ThroughputSample {
            connections: 2,
            bytes_per_second: 121,
        });

        assert_eq!(decision.target_connections, 2);
        assert_eq!(decision.reason, AdaptiveReason::DiminishingReturns);
    }

    #[test]
    fn rate_limit_reduces_target_and_backs_off_with_a_cap() {
        let mut controller = AdaptiveController::new(8);
        controller.observe(ThroughputSample {
            connections: 1,
            bytes_per_second: 100,
        });
        let decision = controller.record_server_status(429).unwrap();

        assert_eq!(decision.target_connections, 1);
        assert_eq!(decision.reason, AdaptiveReason::ServerRateLimited);
        assert_eq!(decision.backoff, Some(Duration::from_secs(2)));
    }

    #[test]
    fn unrelated_statuses_do_not_change_the_controller() {
        let mut controller = AdaptiveController::new(4);
        assert_eq!(controller.record_server_status(404), None);
        assert_eq!(controller.target_connections(), 1);
    }
}
