//! Oracle for validating reduced programs.
//!
//! The oracle is responsible for determining whether a reduced program
//! is equivalent to the original based on:
//! 1. libclang syntax validation (fast pre-check)
//! 2. Compilation success
//! 3. Execution output matching
//! 4. Coverage preservation

use slicer_parser::CParser;
use tracing::{debug, info, trace};

use crate::compiler::{Compiler, CompilerConfig};
use crate::coverage::{missing_coverage, CoverageAnalyzer, CoverageReport};
use crate::error::{ValidationError, ValidationResult};
use crate::executor::{Executor, TimeoutConfig};

/// Oracle configuration.
#[derive(Debug, Clone)]
pub struct OracleConfig {
    pub compiler: CompilerConfig,
    pub timeout: TimeoutConfig,
    pub check_coverage: bool,
    pub expected_stdout: Option<String>,
    pub expected_stderr: Option<String>,
    pub expected_exit_code: Option<i32>,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            compiler: CompilerConfig::default(),
            timeout: TimeoutConfig::default(),
            check_coverage: true,
            expected_stdout: None,
            expected_stderr: None,
            expected_exit_code: Some(0),
        }
    }
}

impl OracleConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_expected_stdout(mut self, stdout: impl Into<String>) -> Self {
        self.expected_stdout = Some(stdout.into());
        self
    }

    pub fn with_expected_stderr(mut self, stderr: impl Into<String>) -> Self {
        self.expected_stderr = Some(stderr.into());
        self
    }

    pub fn with_expected_exit_code(mut self, code: i32) -> Self {
        self.expected_exit_code = Some(code);
        self
    }

    pub fn without_coverage(mut self) -> Self {
        self.check_coverage = false;
        self
    }
}

/// Validation oracle for reduced programs.
pub struct Oracle {
    config: OracleConfig,
    compiler: Compiler,
    executor: Executor,
    parser: CParser,
    original_coverage: Option<CoverageReport>,
    stats: OracleStats,
}

/// Oracle statistics.
#[derive(Debug, Default, Clone)]
pub struct OracleStats {
    pub total_validations: u64,
    pub successful: u64,
    /// libclang syntax check failures (fast path).
    pub syntax_failures: u64,
    pub compile_failures: u64,
    pub exec_failures: u64,
    pub timeouts: u64,
    pub coverage_mismatches: u64,
    pub output_mismatches: u64,
}

impl Oracle {
    pub fn new(config: OracleConfig) -> ValidationResult<Self> {
        let compiler = Compiler::with_config(config.compiler.clone())?;
        let executor = Executor::new(config.timeout);
        let parser = CParser::default();

        Ok(Self {
            config,
            compiler,
            executor,
            parser,
            original_coverage: None,
            stats: OracleStats::default(),
        })
    }

    /// Initialize the oracle with the original program.
    /// This captures the expected output and coverage.
    pub fn initialize(&mut self, original_source: &str) -> ValidationResult<()> {
        info!("initializing oracle with original program");

        // Compile and run the original to get expected output
        let compile_result = self.compiler.compile(original_source)?;
        if !compile_result.success {
            return Err(ValidationError::compilation_failed(
                "original program failed to compile",
                &compile_result.stderr,
                compile_result.exit_code,
            ));
        }

        let binary_path = compile_result.binary_path.unwrap();
        let exec_result = self.executor.execute(&binary_path)?;

        if exec_result.timed_out {
            return Err(ValidationError::timeout(exec_result.duration_secs));
        }

        // Store expected values
        if self.config.expected_stdout.is_none() {
            self.config.expected_stdout = Some(exec_result.stdout.clone());
        }
        if self.config.expected_stderr.is_none() {
            self.config.expected_stderr = Some(exec_result.stderr.clone());
        }
        if self.config.expected_exit_code.is_none() {
            self.config.expected_exit_code = exec_result.exit_code;
        }

        debug!(
            "original output: {:?}, stderr: {:?}, exit code: {:?}",
            self.config.expected_stdout, self.config.expected_stderr, self.config.expected_exit_code
        );

        // Get coverage if enabled
        if self.config.check_coverage {
            let compile_result = self.compiler.compile_with_coverage(original_source)?;
            if compile_result.success {
                let binary_path = compile_result.binary_path.unwrap();
                let _ = self.executor.execute(&binary_path)?;

                if let Some(temp_dir) = self.compiler.temp_dir() {
                    let analyzer = CoverageAnalyzer::new(temp_dir)?;
                    let source_path = temp_dir.join("input.c");
                    match analyzer.collect(&source_path) {
                        Ok(coverage) => {
                            debug!(
                                "original coverage: {:.1}%",
                                coverage.coverage_percentage()
                            );
                            self.original_coverage = Some(coverage);
                        }
                        Err(e) => {
                            debug!("failed to collect original coverage: {}", e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn validate(&mut self, reduced_source: &str) -> ValidationResult<bool> {
        self.stats.total_validations += 1;

        // Step 0: Fast libclang syntax check (much faster than gcc)
        match self.parser.parse(reduced_source) {
            Ok(unit) => {
                if unit.has_errors() {
                    self.stats.syntax_failures += 1;
                    trace!("validation failed: libclang syntax errors");
                    return Ok(false);
                }
            }
            Err(_) => {
                self.stats.syntax_failures += 1;
                trace!("validation failed: libclang parse error");
                return Ok(false);
            }
        }

        // Step 1: Compile
        let compile_result = self.compiler.compile(reduced_source)?;
        if !compile_result.success {
            self.stats.compile_failures += 1;
            trace!("validation failed: compilation error");
            return Ok(false);
        }

        let binary_path = compile_result.binary_path.unwrap();

        // Step 2: Execute
        let exec_result = self.executor.execute(&binary_path)?;

        if exec_result.timed_out {
            self.stats.timeouts += 1;
            trace!("validation failed: timeout");
            return Ok(false);
        }

        if !exec_result.completed {
            self.stats.exec_failures += 1;
            trace!("validation failed: execution error");
            return Ok(false);
        }

        // Step 3: Check stdout
        if let Some(ref expected_stdout) = self.config.expected_stdout {
            if exec_result.stdout.trim() != expected_stdout.trim() {
                self.stats.output_mismatches += 1;
                trace!("validation failed: stdout mismatch");
                return Ok(false);
            }
        }

        // Step 4: Check stderr
        if let Some(ref expected_stderr) = self.config.expected_stderr {
            if exec_result.stderr.trim() != expected_stderr.trim() {
                self.stats.output_mismatches += 1;
                trace!("validation failed: stderr mismatch");
                return Ok(false);
            }
        }

        // Step 5: Check exit code
        if let Some(expected_code) = self.config.expected_exit_code {
            if exec_result.exit_code != Some(expected_code) {
                self.stats.output_mismatches += 1;
                trace!("validation failed: exit code mismatch");
                return Ok(false);
            }
        }

        // Step 6: Check coverage (if enabled and original coverage available)
        if self.config.check_coverage && self.original_coverage.is_some() {
            // Clone once to avoid borrow conflict with check_coverage_preserved
            let original_coverage = self.original_coverage.clone().unwrap();
            let coverage_ok = self.check_coverage_preserved(reduced_source, &original_coverage)?;
            if !coverage_ok {
                self.stats.coverage_mismatches += 1;
                trace!("validation failed: coverage mismatch");
                return Ok(false);
            }
        }

        self.stats.successful += 1;
        trace!("validation successful");
        Ok(true)
    }

    fn check_coverage_preserved(
        &mut self,
        reduced_source: &str,
        original_coverage: &CoverageReport,
    ) -> ValidationResult<bool> {
        let compile_result = self.compiler.compile_with_coverage(reduced_source)?;
        if !compile_result.success {
            return Ok(false);
        }

        let binary_path = compile_result.binary_path.unwrap();
        let _ = self.executor.execute(&binary_path)?;

        if let Some(temp_dir) = self.compiler.temp_dir() {
            let analyzer = CoverageAnalyzer::new(temp_dir)?;
            let source_path = temp_dir.join("input.c");

            match analyzer.collect(&source_path) {
                Ok(reduced_coverage) => {
                    let missing = missing_coverage(original_coverage, &reduced_coverage);
                    if !missing.is_empty() {
                        trace!("missing coverage on lines: {:?}", missing);
                        Ok(false)
                    } else {
                        Ok(true)
                    }
                }
                Err(_) => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    pub fn stats(&self) -> &OracleStats {
        &self.stats
    }

    pub fn original_coverage(&self) -> Option<&CoverageReport> {
        self.original_coverage.as_ref()
    }

    pub fn expected_stdout(&self) -> Option<&str> {
        self.config.expected_stdout.as_deref()
    }

    pub fn expected_exit_code(&self) -> Option<i32> {
        self.config.expected_exit_code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oracle_config() {
        let config = OracleConfig::new()
            .with_expected_stdout("hello")
            .with_expected_exit_code(0)
            .without_coverage();

        assert_eq!(config.expected_stdout, Some("hello".into()));
        assert_eq!(config.expected_exit_code, Some(0));
        assert!(!config.check_coverage);
    }

    #[test]
    fn test_oracle_validate_simple() {
        let config = OracleConfig::new().without_coverage();
        let mut oracle = Oracle::new(config).unwrap();

        // Initialize with original
        oracle.initialize("int main() { return 0; }").unwrap();

        // Validate same program
        assert!(oracle.validate("int main() { return 0; }").unwrap());

        // Validate modified program with different exit code
        assert!(!oracle.validate("int main() { return 1; }").unwrap());
    }

    #[test]
    fn test_oracle_stats() {
        let config = OracleConfig::new().without_coverage();
        let mut oracle = Oracle::new(config).unwrap();
        oracle.initialize("int main() { return 0; }").unwrap();

        oracle.validate("int main() { return 0; }").unwrap();
        oracle.validate("this is invalid").unwrap();

        let stats = oracle.stats();
        assert_eq!(stats.total_validations, 2);
        assert_eq!(stats.successful, 1);
        // The invalid source is caught by libclang syntax check now
        assert_eq!(stats.syntax_failures, 1);
    }
}
