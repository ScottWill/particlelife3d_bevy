use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use std::collections::VecDeque;
use std::fmt::{Display, Formatter, Result};
use std::time::Duration;

const MAX_ITEMS: usize = 64;

#[derive(Default)]
pub struct AvgDuration {
    total: f32,
    durations: VecDeque<f32>,
}

impl Display for AvgDuration {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{:.3}ms", self.avg())
    }
}

impl AvgDuration {
    pub fn add(&mut self, duration: Duration) {
        if self.durations.len() == MAX_ITEMS {
            let old_ms = self.durations.pop_back().unwrap();
            self.total -= old_ms;
        }
        let ms = 1000.0 * duration.as_secs_f32();
        self.durations.push_front(ms);
        self.total += ms;
    }

    pub fn avg(&self) -> f32 {
        self.total / MAX_ITEMS as f32
    }
}

#[derive(Default, Resource)]
pub struct DebugDurations {
    durations: HashMap<String, AvgDuration>,
    order: Option<Vec<&'static str>>,
}

impl Display for DebugDurations {
    fn fmt(&self, f: &mut Formatter) -> Result {
        let mut result = Vec::with_capacity(self.durations.len());
        if let Some(order) = &self.order {
            for name in order {
                if let Some(duration) = self.durations.get(*name) {
                    result.push(format!("{name}: {duration}"));
                }
            }
        } else {
            result = self.durations
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>();
        }
        write!(f, "{}", result.join("\n"))
    }
}

impl DebugDurations {
    pub fn with_order(order: &[&'static str]) -> Self {
        Self {
            order: Some(order.to_owned()),
            ..Default::default()
        }
    }

    pub fn add(&mut self, name: &str, duration: Duration) {
        if let Some(avg) = self.durations.get_mut(name) {
            avg.add(duration);
        } else {
            let mut value = AvgDuration::default();
            value.add(duration);
            self.durations.insert(name.to_owned(), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        // Feature: egui-settings-panel, Property 4: Rolling average correctness
        /// **Validates: Requirements 3.4**
        #[test]
        fn rolling_average_correctness(
            durations_ms in proptest::collection::vec(1u64..10_000, 1..=128),
        ) {
            let mut tracker = AvgDuration::default();
            for &ms in &durations_ms {
                tracker.add(Duration::from_millis(ms));
            }

            // Expected average: sum of last min(n, 64) samples divided by MAX_ITEMS (64)
            // The avg() method always divides by MAX_ITEMS, not by actual count
            let window_size = durations_ms.len().min(MAX_ITEMS);
            let recent = &durations_ms[durations_ms.len() - window_size..];
            let expected_sum: f32 = recent.iter().map(|&ms| ms as f32).sum();
            let expected_avg = expected_sum / MAX_ITEMS as f32;
            let actual_avg = tracker.avg();

            // Allow small floating point tolerance
            let diff = (actual_avg - expected_avg).abs();
            prop_assert!(diff < 0.01,
                "Expected avg {:.3}, got {:.3} (diff: {:.6})", expected_avg, actual_avg, diff);
        }
    }
}
