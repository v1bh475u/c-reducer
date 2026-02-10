use slicer_parser::CParser;

use crate::compiler::{Compiler, CompilerConfig};
use crate::coverage::{missing_coverage, CoverageAnalyzer, CoverageReport};
use crate::error::{ValidationError, ValidationResult};
use crate::executor::{Executor, TimeoutConfig};

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
    pub fn without_coverage(mut self) -> Self {
        self.check_coverage = false;
        self
    }
}

pub struct Oracle {
    config: OracleConfig,
    compiler: Compiler,
    executor: Executor,
    parser: CParser,
    original_coverage: Option<CoverageReport>,
    expect_timeout: bool,
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
            expect_timeout: false,
        })
    }

    pub fn initialize(&mut self, original_source: &str) -> ValidationResult<()> {
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
            self.expect_timeout = true;
            self.config.check_coverage = false;
            self.config.expected_stdout = None;
            self.config.expected_stderr = None;
            self.config.expected_exit_code = None;
            return Ok(());
        }

        if self.config.expected_stdout.is_none() {
            self.config.expected_stdout = Some(exec_result.stdout.clone());
        }
        if self.config.expected_stderr.is_none() {
            self.config.expected_stderr = Some(exec_result.stderr.clone());
        }
        if self.config.expected_exit_code.is_none() {
            self.config.expected_exit_code = exec_result.exit_code;
        }

        if self.config.check_coverage {
            let compile_result = self.compiler.compile_with_coverage(original_source)?;
            if compile_result.success {
                let binary_path = compile_result.binary_path.unwrap();
                let _ = self.executor.execute(&binary_path)?;

                if let Some(temp_dir) = self.compiler.temp_dir() {
                    let analyzer = CoverageAnalyzer::new(temp_dir)?;
                    let source_path = temp_dir.join("input.c");
                    if let Ok(coverage) = analyzer.collect(&source_path) {
                        self.original_coverage = Some(coverage);
                    }
                }
            }
        }

        Ok(())
    }

    pub fn validate(&mut self, reduced_source: &str) -> ValidationResult<bool> {
        match self.parser.parse(reduced_source) {
            Ok(unit) => {
                if unit.has_errors() {
                    return Ok(false);
                }
            }
            Err(_) => return Ok(false),
        }

        let compile_result = self.compiler.compile(reduced_source)?;
        if !compile_result.success {
            return Ok(false);
        }

        let binary_path = compile_result.binary_path.unwrap();
        let exec_result = self.executor.execute(&binary_path)?;

        if self.expect_timeout {
            return Ok(exec_result.timed_out);
        }

        if exec_result.timed_out || !exec_result.completed {
            return Ok(false);
        }

        if let Some(ref expected_stdout) = self.config.expected_stdout {
            if exec_result.stdout.trim() != expected_stdout.trim() {
                return Ok(false);
            }
        }

        if let Some(ref expected_stderr) = self.config.expected_stderr {
            if exec_result.stderr.trim() != expected_stderr.trim() {
                return Ok(false);
            }
        }

        if let Some(expected_code) = self.config.expected_exit_code {
            if exec_result.exit_code != Some(expected_code) {
                return Ok(false);
            }
        }

        if self.config.check_coverage && self.original_coverage.is_some() {
            let original_coverage = self.original_coverage.clone().unwrap();
            let coverage_ok = self.check_coverage_preserved(reduced_source, &original_coverage)?;
            if !coverage_ok {
                return Ok(false);
            }
        }

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
                    Ok(missing.is_empty())
                }
                Err(_) => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    pub fn original_coverage(&self) -> Option<&CoverageReport> {
        self.original_coverage.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oracle_validate_simple() {
        let config = OracleConfig::default().without_coverage();
        let mut oracle = Oracle::new(config).unwrap();
        oracle.initialize("int main() { return 0; }").unwrap();
        assert!(oracle.validate("int main() { return 0; }").unwrap());
        assert!(!oracle.validate("int main() { return 1; }").unwrap());
    }
}
