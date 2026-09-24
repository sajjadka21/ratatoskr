use std::{fmt, time::Duration};

const MIN_CONNECTIONS: usize = 1;
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// A step up must add at least this much (in tenths) to count as a gain.
const GAIN_TENTHS: u64 = 11;
/// Observations held at a ceiling before one more connection is tried.
const RETRY_CEILING_AFTER: u32 = 12;

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

/// Throughput measured over one evaluation window while `connections`
/// streams were running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThroughputSample {
    pub connections: usize,
    pub bytes_per_second: u64,
}

/// Chooses how many connections a transfer should use.
///
/// It starts with one and doubles while every step brings a measured gain of
/// at least 10%, so a fast server reaches eight streams within a few
/// seconds instead of adding one at a time. The first step that brings no
/// gain is undone and remembered as a ceiling; after a while at that level
/// a single extra connection is tried again, because conditions change. A
/// 429 or 503 removes a connection immediately and backs off.
#[derive(Debug, Clone)]
pub struct AdaptiveController {
    max_connections: usize,
    target_connections: usize,
    /// The best level measured so far and its rate.
    best: Option<ThroughputSample>,
    /// A level that brought no gain; not tried again until it expires.
    ceiling: Option<usize>,
    held_at_ceiling: u32,
    reason: AdaptiveReason,
    backoff_attempts: u32,
}

impl AdaptiveController {
    pub fn new(max_connections: usize) -> Self {
        Self {
            max_connections: max_connections.max(MIN_CONNECTIONS),
            target_connections: MIN_CONNECTIONS,
            best: None,
            ceiling: None,
            held_at_ceiling: 0,
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
        self.backoff_attempts = 0;

        let Some(best) = self.best else {
            self.best = Some(sample);
            self.grow(false);
            return self.decision(None);
        };

        if sample.connections > best.connections {
            if gained(sample.bytes_per_second, best.bytes_per_second) {
                self.best = Some(sample);
                self.grow(false);
            } else {
                // The extra streams did not pay for themselves: go back to
                // the level that did, and remember not to overshoot again.
                self.target_connections = best.connections.max(MIN_CONNECTIONS);
                self.ceiling = Some(sample.connections);
                self.held_at_ceiling = 0;
                self.reason = AdaptiveReason::DiminishingReturns;
            }
        } else if sample.connections == best.connections {
            // Same level: follow the measured rate so a later comparison is
            // made against current conditions, not an old peak.
            self.best = Some(sample);
            if self.target_connections <= sample.connections {
                self.held_at_ceiling = self.held_at_ceiling.saturating_add(1);
                if self.ceiling.is_some() && self.held_at_ceiling >= RETRY_CEILING_AFTER {
                    self.ceiling = None;
                    self.held_at_ceiling = 0;
                    self.grow(true);
                } else if self.ceiling.is_none() {
                    self.grow(false);
                } else {
                    self.reason = AdaptiveReason::Stable;
                }
            }
        } else if self.target_connections >= best.connections {
            // Fewer streams than planned are running, usually near the end
            // of a file when little is left to share out. Nothing to learn.
            self.reason = AdaptiveReason::Stable;
        } else {
            // A back-off lowered the target below the best level; restart
            // the comparison from here.
            self.best = Some(sample);
            self.grow(false);
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
        self.ceiling = Some(self.target_connections + 1);
        self.held_at_ceiling = 0;
        self.reason = reason;
        self.backoff_attempts = self.backoff_attempts.saturating_add(1);
        Some(self.decision(Some(self.backoff_delay())))
    }

    pub fn backoff_delay(&self) -> Duration {
        let seconds = 2_u64.saturating_pow(self.backoff_attempts.min(5));
        Duration::from_secs(seconds).min(MAX_BACKOFF)
    }

    /// Raises the target: doubling while growing freely, one step at a time
    /// when re-testing a ceiling, never past the maximum or a ceiling.
    fn grow(&mut self, one_step: bool) {
        let limit = self
            .ceiling
            .map_or(self.max_connections, |ceiling| {
                ceiling.saturating_sub(1).max(MIN_CONNECTIONS)
            })
            .min(self.max_connections);
        let wanted = if one_step {
            self.target_connections + 1
        } else {
            self.target_connections.saturating_mul(2)
        };
        let next = wanted.min(limit);
        if next > self.target_connections {
            self.target_connections = next;
            self.reason = AdaptiveReason::ThroughputImproved;
        } else {
            self.reason = AdaptiveReason::Stable;
        }
    }

    fn decision(&self, backoff: Option<Duration>) -> AdaptiveDecision {
        AdaptiveDecision {
            target_connections: self.target_connections,
            reason: self.reason,
            backoff,
        }
    }
}

fn gained(now: u64, before: u64) -> bool {
    now.saturating_mul(10) >= before.saturating_mul(GAIN_TENTHS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(connections: usize, bytes_per_second: u64) -> ThroughputSample {
        ThroughputSample {
            connections,
            bytes_per_second,
        }
    }

    #[test]
    fn starts_with_one_connection_and_explains_it() {
        let controller = AdaptiveController::new(8);

        assert_eq!(controller.target_connections(), 1);
        assert_eq!(controller.reason(), AdaptiveReason::StartingConservatively);
    }

    #[test]
    fn doubles_while_every_step_brings_a_gain() {
        let mut controller = AdaptiveController::new(16);

        assert_eq!(controller.observe(sample(1, 100)).target_connections, 2);
        assert_eq!(controller.observe(sample(2, 190)).target_connections, 4);
        assert_eq!(controller.observe(sample(4, 350)).target_connections, 8);
        let decision = controller.observe(sample(8, 600));
        assert_eq!(decision.target_connections, 16);
        assert_eq!(decision.reason, AdaptiveReason::ThroughputImproved);
    }

    #[test]
    fn a_step_without_gain_is_undone_and_not_repeated_at_once() {
        let mut controller = AdaptiveController::new(16);
        controller.observe(sample(1, 100));
        controller.observe(sample(2, 190));

        let decision = controller.observe(sample(4, 195));
        assert_eq!(decision.target_connections, 2);
        assert_eq!(decision.reason, AdaptiveReason::DiminishingReturns);

        // Holding at two does not jump straight back to four.
        let decision = controller.observe(sample(2, 190));
        assert_eq!(decision.target_connections, 2);
        assert_eq!(decision.reason, AdaptiveReason::Stable);
    }

    #[test]
    fn a_ceiling_is_retried_one_step_after_a_while() {
        let mut controller = AdaptiveController::new(16);
        controller.observe(sample(1, 100));
        controller.observe(sample(2, 190));
        controller.observe(sample(4, 195));

        let mut last = controller.target_connections();
        for _ in 0..RETRY_CEILING_AFTER {
            last = controller.observe(sample(2, 190)).target_connections;
        }
        assert_eq!(last, 3);
    }

    #[test]
    fn never_exceeds_the_maximum() {
        let mut controller = AdaptiveController::new(3);
        controller.observe(sample(1, 100));
        let decision = controller.observe(sample(2, 200));
        assert_eq!(decision.target_connections, 3);
        let decision = controller.observe(sample(3, 300));
        assert_eq!(decision.target_connections, 3);
        assert_eq!(decision.reason, AdaptiveReason::Stable);
    }

    #[test]
    fn fewer_running_streams_near_the_end_change_nothing() {
        let mut controller = AdaptiveController::new(8);
        controller.observe(sample(1, 100));
        controller.observe(sample(2, 200));
        let decision = controller.observe(sample(1, 90));
        assert_eq!(decision.target_connections, 4);
        assert_eq!(decision.reason, AdaptiveReason::Stable);
    }

    #[test]
    fn rate_limit_reduces_target_and_backs_off_with_a_cap() {
        let mut controller = AdaptiveController::new(8);
        controller.observe(sample(1, 100));
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
