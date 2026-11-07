//! ROLE: Stopwatch of the Linux diagnostic: one duration per stage, each against its budget,
//! so that a Linux gain or loss is proven on the Raspberry Pi without anyone watching.
//! OWNER: `lib::diagnostic` and the editor's scripted run fill it; the report ends the run.
//! INVARIANTS: Budgets are milliseconds on the Raspberry Pi 5 at 1080p, two to four times
//! what the lot D of 2026-11-07 measured there, with room for the other programs it keeps
//! running (`docs/contexte/audit-performance-2026-11-07.md`, « Mise en œuvre du lot D ») ;
//! a stage without a budget is only informative.
use std::time::Duration;

/// Loading the system fonts and the embedded annotation font, on their own thread.
pub const FONTS: Budget = Budget::new("fonts", Some(100.));
/// Waiting for that thread once the capture is in hand and the event loop exists.
pub const FONTS_WAIT: Budget = Budget::new("fonts wait", Some(50.));
/// The portal's round trip: the compositor's time, not ours.
pub const CAPTURE: Budget = Budget::new("capture", None);
/// Decoding the PNG of a full-screen capture.
pub const DECODE: Budget = Budget::new("decode", Some(60.));
/// Encoding a full-screen capture as a PNG file.
pub const ENCODE: Budget = Budget::new("encode", Some(120.));
/// From the capture in hand to the first frame on screen: fonts, window, paint, present.
pub const FIRST_FRAME: Budget = Budget::new("first frame", Some(120.));
/// One frame of the loaded editor (selection, shapes, text being typed, toolbar), painted
/// and presented, median of the measured frames.
pub const FRAME: Budget = Budget::new("frame", Some(30.));
/// Presenting a painted frame to the window, median of the measured frames.
pub const PRESENT: Budget = Budget::new("present", Some(6.));
/// Flattening the annotated capture for its export.
pub const EXPORT: Budget = Budget::new("export", Some(30.));

/// A named stage and the most it may take, in milliseconds; `None` is only reported.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub name: &'static str,
    pub limit_ms: Option<f64>,
}
impl Budget {
    const fn new(name: &'static str, limit_ms: Option<f64>) -> Self {
        Self { name, limit_ms }
    }
}

#[derive(Clone, Copy, Debug)]
struct Stage {
    budget: Budget,
    ms: f64,
}

/// The stages measured so far, in the order they were recorded.
#[derive(Default, Debug)]
pub struct Timings {
    stages: Vec<Stage>,
}
impl Timings {
    pub fn new() -> Self {
        Self::default()
    }
    /// Records `taken` for `budget`'s stage.
    pub fn record(&mut self, budget: Budget, taken: Duration) {
        self.stages.push(Stage {
            budget,
            ms: taken.as_secs_f64() * 1000.,
        });
    }
    /// Records the median of `samples` for `budget`'s stage; nothing without a sample.
    pub fn record_median(&mut self, budget: Budget, samples: &[Duration]) {
        if let Some(median) = median(samples) {
            self.record(budget, median);
        }
    }
    /// One line per stage, as the diagnostic prints them.
    pub fn lines(&self) -> Vec<String> {
        self.stages
            .iter()
            .map(|s| match s.budget.limit_ms {
                Some(limit) => format!(
                    "timing: {} {:.1} ms (budget {limit:.0} ms)",
                    s.budget.name, s.ms
                ),
                None => format!("timing: {} {:.1} ms", s.budget.name, s.ms),
            })
            .collect()
    }
    /// The stages over their budget, as "name: taken ms over budget ms".
    pub fn over_budget(&self) -> Vec<String> {
        self.stages
            .iter()
            .filter(|s| s.budget.limit_ms.is_some_and(|limit| s.ms > limit))
            .map(|s| {
                format!(
                    "{}: {:.1} ms over {:.0} ms",
                    s.budget.name,
                    s.ms,
                    s.budget.limit_ms.unwrap_or_default()
                )
            })
            .collect()
    }
    /// Prints every stage, then fails naming each one over its budget.
    pub fn report(&self) -> Result<(), String> {
        for line in self.lines() {
            println!("{line}");
        }
        let over = self.over_budget();
        if over.is_empty() {
            println!("timing: every budgeted stage within budget");
            Ok(())
        } else {
            Err(format!("over budget: {}", over.join(", ")))
        }
    }
}

/// The median of `samples`: the upper middle one of an even count, so that a measure of two
/// frames reports the slower.
fn median(samples: &[Duration]) -> Option<Duration> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort();
    Some(sorted[sorted.len() / 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIGHT: Budget = Budget::new("tight", Some(10.));
    const LOOSE: Budget = Budget::new("loose", None);

    #[test]
    fn a_stage_over_its_budget_fails_the_report_and_names_itself() {
        let mut timings = Timings::new();
        timings.record(TIGHT, Duration::from_millis(25));
        timings.record(LOOSE, Duration::from_secs(3));
        assert_eq!(
            timings.lines(),
            [
                "timing: tight 25.0 ms (budget 10 ms)",
                "timing: loose 3000.0 ms",
            ]
        );
        assert_eq!(timings.over_budget(), ["tight: 25.0 ms over 10 ms"]);
        assert!(timings.report().is_err());
    }
    #[test]
    fn stages_within_budget_and_informative_ones_pass() {
        let mut timings = Timings::new();
        timings.record(TIGHT, Duration::from_millis(10));
        timings.record(LOOSE, Duration::from_secs(3));
        assert!(timings.over_budget().is_empty());
        assert!(timings.report().is_ok());
    }
    #[test]
    fn the_median_of_frames_is_recorded_not_their_best() {
        let mut timings = Timings::new();
        let samples: Vec<Duration> = [5, 50, 7, 6]
            .into_iter()
            .map(Duration::from_millis)
            .collect();
        timings.record_median(TIGHT, &samples);
        assert_eq!(timings.lines(), ["timing: tight 7.0 ms (budget 10 ms)"]);
        timings.record_median(LOOSE, &[]);
        assert_eq!(timings.lines().len(), 1, "no sample, no stage");
    }
}
