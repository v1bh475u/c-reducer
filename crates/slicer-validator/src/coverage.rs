//! Coverage analysis using gcov.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, trace, warn};

/// Regex for parsing gcov output lines. Format: "count:line_number:source"
static LINE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*([#\d-]+):\s*(\d+):").unwrap()
});

use crate::error::{ValidationError, ValidationResult};

/// Coverage information for a single line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCoverage {
    /// Line was executed the given number of times.
    Executed(u64),
    /// Line was not executed.
    NotExecuted,
    /// Line is not executable (comments, declarations, etc.).
    NonExecutable,
}

impl LineCoverage {
    pub fn is_executed(&self) -> bool {
        matches!(self, Self::Executed(_))
    }

    pub fn is_executable(&self) -> bool {
        !matches!(self, Self::NonExecutable)
    }

    pub fn count(&self) -> Option<u64> {
        match self {
            Self::Executed(n) => Some(*n),
            _ => None,
        }
    }
}

/// Coverage report for a source file.
#[derive(Debug, Clone)]
pub struct CoverageReport {
    pub source_path: PathBuf,
    pub lines: HashMap<usize, LineCoverage>,
    pub executable_lines: usize,
    pub executed_lines: usize,
}

impl CoverageReport {
    pub fn new(source_path: impl Into<PathBuf>) -> Self {
        Self {
            source_path: source_path.into(),
            lines: HashMap::new(),
            executable_lines: 0,
            executed_lines: 0,
        }
    }

    pub fn coverage_percentage(&self) -> f64 {
        if self.executable_lines == 0 {
            100.0
        } else {
            (self.executed_lines as f64 / self.executable_lines as f64) * 100.0
        }
    }

    pub fn line_coverage(&self, line: usize) -> LineCoverage {
        self.lines
            .get(&line)
            .copied()
            .unwrap_or(LineCoverage::NonExecutable)
    }

    pub fn is_line_executed(&self, line: usize) -> bool {
        self.line_coverage(line).is_executed()
    }

    pub fn executed_line_numbers(&self) -> Vec<usize> {
        self.lines
            .iter()
            .filter(|(_, cov)| cov.is_executed())
            .map(|(line, _)| *line)
            .collect()
    }
}

/// Coverage analyzer using gcov.
#[derive(Debug)]
pub struct CoverageAnalyzer {
    gcov_path: PathBuf,
    working_dir: PathBuf,
}

impl CoverageAnalyzer {
    pub fn new(working_dir: impl Into<PathBuf>) -> ValidationResult<Self> {
        let gcov_path = PathBuf::from("gcov");

        // Verify gcov exists
        if !Command::new("which")
            .arg(&gcov_path)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Err(ValidationError::CoverageFailed(
                "gcov not found in PATH".into(),
            ));
        }

        Ok(Self {
            gcov_path,
            working_dir: working_dir.into(),
        })
    }

    /// Run gcov and collect coverage data.
    pub fn collect(&self, source_file: &Path) -> ValidationResult<CoverageReport> {
        debug!("collecting coverage for {:?}", source_file);

        // Run gcov
        let output = Command::new(&self.gcov_path)
            .arg(source_file)
            .current_dir(&self.working_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("gcov failed: {}", stderr);
            return Err(ValidationError::CoverageFailed(format!(
                "gcov failed: {}",
                stderr
            )));
        }

        // Find and parse the .gcov file
        let gcov_file = self.working_dir.join(format!(
            "{}.gcov",
            source_file.file_name().unwrap().to_string_lossy()
        ));

        if !gcov_file.exists() {
            return Err(ValidationError::CoverageFailed(format!(
                "gcov output file not found: {:?}",
                gcov_file
            )));
        }

        self.parse_gcov_file(&gcov_file, source_file)
    }

    /// Parse a gcov output file.
    fn parse_gcov_file(
        &self,
        gcov_file: &Path,
        source_file: &Path,
    ) -> ValidationResult<CoverageReport> {
        let content = std::fs::read_to_string(gcov_file)?;
        let mut report = CoverageReport::new(source_file);

        for line in content.lines() {
            if let Some(caps) = LINE_REGEX.captures(line) {
                let count_str = &caps[1];
                let line_num: usize = caps[2].parse().unwrap_or(0);

                if line_num == 0 {
                    continue;
                }

                let coverage = if count_str == "-" {
                    LineCoverage::NonExecutable
                } else if count_str.starts_with('#') {
                    report.executable_lines += 1;
                    LineCoverage::NotExecuted
                } else if let Ok(count) = count_str.parse::<u64>() {
                    report.executable_lines += 1;
                    if count > 0 {
                        report.executed_lines += 1;
                    }
                    if count > 0 {
                        LineCoverage::Executed(count)
                    } else {
                        LineCoverage::NotExecuted
                    }
                } else {
                    LineCoverage::NonExecutable
                };

                report.lines.insert(line_num, coverage);
            }
        }

        trace!(
            "coverage: {}/{} lines ({:.1}%)",
            report.executed_lines,
            report.executable_lines,
            report.coverage_percentage()
        );

        Ok(report)
    }

    pub fn cleanup(&self) -> ValidationResult<()> {
        // Remove .gcov, .gcda, .gcno files
        for pattern in &["*.gcov", "*.gcda", "*.gcno"] {
            for path in glob::glob(&self.working_dir.join(pattern).to_string_lossy())
                .map_err(|e| ValidationError::CoverageFailed(e.to_string()))?
                .flatten()
            {
                let _ = std::fs::remove_file(path);
            }
        }
        Ok(())
    }
}

/// Get lines that are in original coverage but not in reduced.
pub fn missing_coverage(original: &CoverageReport, reduced: &CoverageReport) -> Vec<usize> {
    original
        .lines
        .iter()
        .filter(|(_, cov)| cov.is_executed())
        .filter(|(line, _)| !reduced.is_line_executed(**line))
        .map(|(line, _)| *line)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_coverage() {
        assert!(LineCoverage::Executed(5).is_executed());
        assert!(!LineCoverage::NotExecuted.is_executed());
        assert!(!LineCoverage::NonExecutable.is_executable());
        assert_eq!(LineCoverage::Executed(10).count(), Some(10));
    }

    #[test]
    fn test_coverage_report() {
        let mut report = CoverageReport::new("test.c");
        report.lines.insert(1, LineCoverage::Executed(1));
        report.lines.insert(2, LineCoverage::NotExecuted);
        report.lines.insert(3, LineCoverage::NonExecutable);
        report.executable_lines = 2;
        report.executed_lines = 1;

        assert_eq!(report.coverage_percentage(), 50.0);
        assert!(report.is_line_executed(1));
        assert!(!report.is_line_executed(2));
    }
}
