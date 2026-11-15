//! Per-connection viewing preferences; a bar connection alone is not a viewer.
use std::{collections::HashMap, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub background: bool,
    pub seconds: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            background: true,
            seconds: 900,
        }
    }
}

impl Policy {
    pub fn enabled(self) -> bool {
        !self.background || self.seconds != 0
    }
    pub fn interval(self) -> Duration {
        Duration::from_secs(self.seconds)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Preferences {
    pub visible: bool,
    pub background_seconds: u64,
    pub active_seconds: u64,
}

impl Preferences {
    pub fn validate(self) -> Result<Self, String> {
        if !(5..=3600).contains(&self.active_seconds) {
            return Err("activeSeconds must be an integer from 5 to 3600".into());
        }
        if self.background_seconds != 0 && !(60..=86400).contains(&self.background_seconds) {
            return Err("backgroundSeconds must be 0 or an integer from 60 to 86400".into());
        }
        Ok(self)
    }
}

pub fn effective(clients: &HashMap<u64, Preferences>) -> Policy {
    if let Some(seconds) = clients
        .values()
        .filter(|p| p.visible)
        .map(|p| p.active_seconds)
        .min()
    {
        return Policy {
            background: false,
            seconds,
        };
    }
    Policy {
        background: true,
        seconds: clients
            .values()
            .map(|p| p.background_seconds)
            .min()
            .unwrap_or(900),
    }
}

/// Rate-limit discovery/listing rounds, while letting one round fetch its chunk batch.
pub struct Cadence {
    next: tokio::time::Instant,
}
impl Cadence {
    pub fn new() -> Self {
        Self {
            next: tokio::time::Instant::now(),
        }
    }
    pub async fn wait(&mut self, interval: Duration) {
        tokio::time::sleep_until(self.next).await;
        self.next = tokio::time::Instant::now() + interval;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewers_override_the_bar_until_the_last_viewer_disconnects() {
        let mut clients = HashMap::new();
        let bar = Preferences {
            visible: false,
            background_seconds: 1200,
            active_seconds: 10,
        };
        clients.insert(1, bar);
        assert_eq!(
            effective(&clients),
            Policy {
                background: true,
                seconds: 1200
            }
        );
        clients.insert(
            2,
            Preferences {
                visible: true,
                ..bar
            },
        );
        clients.insert(
            3,
            Preferences {
                visible: true,
                active_seconds: 5,
                ..bar
            },
        );
        assert_eq!(
            effective(&clients),
            Policy {
                background: false,
                seconds: 5
            }
        );
        clients.remove(&3);
        assert_eq!(
            effective(&clients),
            Policy {
                background: false,
                seconds: 10
            }
        );
        clients.remove(&2);
        assert_eq!(
            effective(&clients),
            Policy {
                background: true,
                seconds: 1200
            }
        );
        clients.clear();
        assert_eq!(effective(&clients), Policy::default());
    }

    #[test]
    fn disabling_background_does_not_disable_an_open_viewer() {
        let mut clients = HashMap::new();
        let p = Preferences {
            visible: false,
            background_seconds: 0,
            active_seconds: 10,
        };
        clients.insert(0, p);
        assert!(!effective(&clients).enabled());
        clients.insert(1, Preferences { visible: true, ..p });
        assert!(effective(&clients).enabled());
        assert!(!effective(&clients).background);
    }

    #[test]
    fn cadence_waits_between_checks_without_catching_up_after_slow_work() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap()
            .block_on(async {
                let mut cadence = Cadence::new();
                let interval = Duration::from_secs(10);
                let start = tokio::time::Instant::now();
                cadence.wait(interval).await;
                cadence.wait(interval).await;
                assert_eq!(start.elapsed(), interval);
                tokio::time::advance(Duration::from_secs(20)).await;
                cadence.wait(interval).await;
                assert_eq!(start.elapsed(), Duration::from_secs(30));
                cadence.wait(interval).await;
                assert_eq!(start.elapsed(), Duration::from_secs(40));
            });
    }

    #[test]
    fn invalid_intervals_are_rejected() {
        for (background_seconds, active_seconds, valid) in [
            (900, 10, true),
            (1200, 5, true),
            (0, 10, true),
            (59, 10, false),
            (86401, 10, false),
            (900, 2, false),
            (900, 3601, false),
        ] {
            assert_eq!(
                Preferences {
                    visible: false,
                    background_seconds,
                    active_seconds
                }
                .validate()
                .is_ok(),
                valid
            );
        }
    }
}
