//! Main reduction pipeline.

use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::{
    config::PipelineConfig,
    context::Context,
    pass::{Candidate, ReductionPass},
    result::{PassStats, ReductionResult},
};

/// The main reduction pipeline.
///
/// Runs passes serially in priority order until convergence.
pub struct Pipeline {
    config: PipelineConfig,
    passes: Vec<Box<dyn ReductionPass>>,
}

impl Pipeline {
    pub fn new(config: PipelineConfig) -> anyhow::Result<Self> {
        Ok(Self {
            config,
            passes: Vec::new(),
        })
    }

    pub fn register_pass(&mut self, pass: Box<dyn ReductionPass>) {
        if self.config.is_pass_enabled(pass.name()) {
            self.passes.push(pass);
            // Keep sorted by priority (higher first)
            self.passes.sort_by_key(|p| std::cmp::Reverse(p.priority()));
        }
    }

    /// Run the reduction pipeline.
    ///
    /// Passes run serially in priority order. Each pass generates candidates,
    /// and we try each candidate in order until we find one that's valid.
    pub fn reduce(&self, source: &str, ctx: &mut Context) -> anyhow::Result<ReductionResult> {
        let start_time = Instant::now();
        let total_timeout = if self.config.total_timeout_secs > 0 {
            Some(Duration::from_secs(self.config.total_timeout_secs))
        } else {
            None
        };
        let mut current = source.to_string();
        let mut pass_stats: Vec<PassStats> = Vec::new();
        let mut iteration = 0;
        let mut converged = false;
        let mut timed_out = false;

        info!(
            "Starting reduction: {} lines, {} passes enabled, timeout: {}",
            count_lines(&current),
            self.passes.len(),
            total_timeout
                .map(|d| format!("{}s", d.as_secs()))
                .unwrap_or_else(|| "none".to_string())
        );

        'outer: for iter in 0..self.config.max_iterations {
            // Check total timeout at the start of each iteration
            if let Some(timeout) = total_timeout {
                if start_time.elapsed() > timeout {
                    warn!(
                        "Total timeout of {}s exceeded, stopping reduction",
                        timeout.as_secs()
                    );
                    timed_out = true;
                    break;
                }
            }

            iteration = iter + 1;
            ctx.iteration = iteration;

            let prev_lines = count_lines(&current);
            let iter_start = Instant::now();

            debug!("Iteration {}: {} lines", iteration, prev_lines);

            // Run each pass serially
            for pass in &self.passes {
                // Check timeout before each pass
                if let Some(timeout) = total_timeout {
                    if start_time.elapsed() > timeout {
                        warn!(
                            "Total timeout of {}s exceeded during pass, stopping reduction",
                            timeout.as_secs()
                        );
                        timed_out = true;
                        break 'outer;
                    }
                }

                let pass_start = Instant::now();
                let prev_len = current.len();
                let mut candidates_tried = 0u32;
                let mut candidates_valid = 0u32;

                // Get candidates from the pass
                let candidates = pass.apply(&current, ctx);

                // Sort candidates by position DESCENDING (end of file first)
                // This ensures earlier byte ranges remain valid after applying later ones
                let mut sorted_candidates: Vec<_> = candidates.into_iter().collect();
                sorted_candidates.sort_by_key(|c| std::cmp::Reverse(c.range.start));

                // Filter out overlapping candidates (keep higher-positioned ones)
                let non_overlapping = filter_overlapping_candidates(sorted_candidates);

                // Try each candidate (applied from end to start)
                for candidate in &non_overlapping {
                    // Check timeout periodically during candidate processing
                    if candidates_tried.is_multiple_of(100) {
                        if let Some(timeout) = total_timeout {
                            if start_time.elapsed() > timeout {
                                warn!(
                                    "Total timeout of {}s exceeded during candidate evaluation",
                                    timeout.as_secs()
                                );
                                timed_out = true;
                                break 'outer;
                            }
                        }
                    }

                    candidates_tried += 1;

                    // Apply the candidate (skip if out of bounds)
                    let reduced = match candidate.apply(&current) {
                        Some(r) => r,
                        None => continue,
                    };

                    // Validate with the oracle
                    if ctx.is_valid(&reduced) {
                        // Valid reduction!
                        let lines_removed =
                            count_lines(&current) as i32 - count_lines(&reduced) as i32;

                        if lines_removed > 0 {
                            debug!(
                                "  {}: {} -> -{} lines",
                                pass.name(),
                                candidate.description,
                                lines_removed
                            );
                        }

                        current = reduced;
                        candidates_valid += 1;
                    }
                }

                let lines_removed = prev_len.saturating_sub(current.len()) as u32;
                if lines_removed > 0 || candidates_tried > 0 {
                    info!(
                        "  {}: tried {} candidates, {} valid, -{} bytes",
                        pass.name(),
                        candidates_tried,
                        candidates_valid,
                        lines_removed
                    );
                }

                pass_stats.push(PassStats {
                    name: pass.name().to_string(),
                    lines_removed,
                    duration: pass_start.elapsed(),
                    candidates_tried,
                    candidates_valid,
                });
            }

            let current_lines = count_lines(&current);
            let lines_reduced = prev_lines.saturating_sub(current_lines);

            debug!(
                "Iteration {} complete: {} → {} lines (-{}) in {:?}",
                iteration,
                prev_lines,
                current_lines,
                lines_reduced,
                iter_start.elapsed()
            );

            // Check convergence
            if lines_reduced <= self.config.convergence_threshold {
                info!(
                    "Converged after {} iterations (threshold: {})",
                    iteration, self.config.convergence_threshold
                );
                converged = true;
                break;
            }
        }

        if !converged && !timed_out {
            warn!(
                "Did not converge after {} iterations",
                self.config.max_iterations
            );
        }

        let result = ReductionResult::new(source, &current)
            .with_iterations(iteration, converged || timed_out)
            .with_pass_stats(pass_stats)
            .with_duration(start_time.elapsed());

        info!("{}", result.summary());

        Ok(result)
    }

    /// Run a single pass and return all valid reductions.
    /// Useful for testing and debugging.
    pub fn run_pass(
        &self,
        pass: &dyn ReductionPass,
        source: &str,
        ctx: &mut Context,
    ) -> Vec<(Candidate, String)> {
        let candidates = pass.apply(source, ctx);
        let mut results = Vec::new();

        for candidate in candidates {
            let reduced = match candidate.apply(source) {
                Some(r) => r,
                None => continue,
            };

            if ctx.is_valid(&reduced) {
                results.push((candidate, reduced));
            }
        }

        results
    }

    pub fn enabled_passes(&self) -> Vec<&str> {
        self.passes.iter().map(|p| p.name()).collect()
    }
}

/// Count non-empty lines.
fn count_lines(source: &str) -> u32 {
    source.lines().filter(|l| !l.trim().is_empty()).count() as u32
}

/// Filter out overlapping candidates.
/// Assumes candidates are already sorted by range.start DESCENDING.
/// Keeps the first (highest position) candidate when ranges overlap.
fn filter_overlapping_candidates(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut result = Vec::new();
    let mut protected_start = usize::MAX; // No byte before this can be touched

    for candidate in candidates {
        // Check if this candidate overlaps with any already-accepted candidate
        // Since we process from end to start, we just need to check if this
        // candidate's end is <= the earliest protected position
        if candidate.range.end <= protected_start {
            protected_start = candidate.range.start;
            result.push(candidate);
        }
        // If overlapping, skip this candidate
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyPass {
        name: &'static str,
        priority: u32,
    }

    impl ReductionPass for DummyPass {
        fn name(&self) -> &'static str {
            self.name
        }

        fn priority(&self) -> u32 {
            self.priority
        }

        fn apply(&self, source: &str, _context: &crate::context::Context) -> Vec<Candidate> {
            // Generate a candidate to remove the first line
            let lines: Vec<_> = source.lines().collect();
            if lines.len() > 1 {
                // Find the end of the first line
                let first_line_end = lines[0].len() + 1; // +1 for newline
                vec![Candidate::removal(0..first_line_end.min(source.len()))]
            } else {
                Vec::new()
            }
        }
    }

    #[test]
    fn test_pipeline_creation() {
        let config = PipelineConfig::default();
        let pipeline = Pipeline::new(config).unwrap();
        assert!(pipeline.passes.is_empty());
    }

    #[test]
    fn test_pipeline_with_passes() {
        let config = PipelineConfig::default();
        let mut pipeline = Pipeline::new(config).unwrap();

        pipeline.register_pass(Box::new(DummyPass {
            name: "pass1",
            priority: 10,
        }));
        pipeline.register_pass(Box::new(DummyPass {
            name: "pass2",
            priority: 5,
        }));

        // Should be sorted by priority (higher first)
        let names: Vec<_> = pipeline.enabled_passes();
        assert_eq!(names, vec!["pass1", "pass2"]);
    }

    #[test]
    fn test_pass_generates_candidates() {
        let pass = DummyPass {
            name: "test",
            priority: 10,
        };

        let source = "line1\nline2\nline3";
        let ctx = crate::Context::new(Box::new(crate::AlwaysValidOracle));
        let candidates = pass.apply(source, &ctx);

        assert!(!candidates.is_empty());

        let reduced = candidates[0].apply(source).expect("should apply");
        assert!(!reduced.contains("line1"));
        assert!(reduced.contains("line2"));
    }
}
