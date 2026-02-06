//! Binary execution for validation.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tracing::{debug, trace};

use crate::error::ValidationResult;

/// Configuration for execution timeouts.
#[derive(Debug, Clone, Copy)]
pub struct TimeoutConfig {
    pub execution_timeout: Duration,
    pub total_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            execution_timeout: Duration::from_secs(5),
            total_timeout: Duration::from_secs(30),
        }
    }
}

impl TimeoutConfig {
    pub fn new(execution_secs: u64, total_secs: u64) -> Self {
        Self {
            execution_timeout: Duration::from_secs(execution_secs),
            total_timeout: Duration::from_secs(total_secs),
        }
    }

    pub fn uniform(secs: u64) -> Self {
        let duration = Duration::from_secs(secs);
        Self {
            execution_timeout: duration,
            total_timeout: duration,
        }
    }
}

/// Result of executing a binary.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub completed: bool,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_secs: f64,
}

impl ExecutionResult {
    pub fn success(&self) -> bool {
        self.completed && self.exit_code == Some(0)
    }

    pub fn timeout(duration_secs: f64) -> Self {
        Self {
            completed: false,
            timed_out: true,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            duration_secs,
        }
    }

    pub fn completed(
        stdout: String,
        stderr: String,
        exit_code: Option<i32>,
        duration_secs: f64,
    ) -> Self {
        Self {
            completed: true,
            timed_out: false,
            stdout,
            stderr,
            exit_code,
            duration_secs,
        }
    }
}

/// Executor for running compiled binaries.
#[derive(Debug, Clone)]
pub struct Executor {
    timeout_config: TimeoutConfig,
    args: Vec<String>,
    env: Vec<(String, String)>,
    working_dir: Option<std::path::PathBuf>,
}

impl Default for Executor {
    fn default() -> Self {
        Self::new(TimeoutConfig::default())
    }
}

impl Executor {
    pub fn new(timeout_config: TimeoutConfig) -> Self {
        Self {
            timeout_config,
            args: Vec::new(),
            env: Vec::new(),
            working_dir: None,
        }
    }

    pub fn with_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn with_working_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    pub fn execute(&self, binary_path: &Path) -> ValidationResult<ExecutionResult> {
        self.execute_with_input(binary_path, None)
    }

    pub fn execute_with_input(
        &self,
        binary_path: &Path,
        input: Option<&str>,
    ) -> ValidationResult<ExecutionResult> {
        let start = Instant::now();

        let mut cmd = Command::new(binary_path);
        cmd.args(&self.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if input.is_some() {
            cmd.stdin(Stdio::piped());
        }

        if let Some(ref dir) = self.working_dir {
            cmd.current_dir(dir);
        }

        for (key, value) in &self.env {
            cmd.env(key, value);
        }

        debug!("executing: {:?}", cmd);

        // Spawn the process
        let mut child = cmd.spawn()?;

        // Write input if provided
        if let Some(input_data) = input {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(input_data.as_bytes())?;
            }
        }

        // Wait with timeout
        self.wait_with_timeout(&mut child, start)
    }

    fn wait_with_timeout(
        &self,
        child: &mut std::process::Child,
        start: Instant,
    ) -> ValidationResult<ExecutionResult> {
        let timeout = self.timeout_config.execution_timeout;

        // Simple polling-based timeout (could be improved with platform-specific APIs)
        loop {
            match child.try_wait()? {
                Some(status) => {
                    let duration = start.elapsed().as_secs_f64();
                    let output = self.collect_output(child)?;

                    trace!(
                        "execution completed in {:.3}s with status {:?}",
                        duration,
                        status
                    );

                    return Ok(ExecutionResult::completed(
                        output.0,
                        output.1,
                        status.code(),
                        duration,
                    ));
                },
                None => {
                    if start.elapsed() > timeout {
                        // Kill the process
                        let _ = child.kill();
                        let _ = child.wait();

                        let duration = start.elapsed().as_secs_f64();
                        debug!("execution timed out after {:.3}s", duration);

                        return Ok(ExecutionResult::timeout(duration));
                    }
                    // Sleep a bit before checking again
                    std::thread::sleep(Duration::from_millis(10));
                },
            }
        }
    }

    fn collect_output(
        &self,
        child: &mut std::process::Child,
    ) -> ValidationResult<(String, String)> {
        use std::io::Read;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let Some(ref mut stdout_pipe) = child.stdout {
            stdout_pipe.read_to_string(&mut stdout)?;
        }
        if let Some(ref mut stderr_pipe) = child.stderr {
            stderr_pipe.read_to_string(&mut stderr)?;
        }

        Ok((stdout, stderr))
    }

    pub fn timeout_config(&self) -> &TimeoutConfig {
        &self.timeout_config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler;

    #[test]
    fn test_execute_simple() {
        let mut compiler = Compiler::new().unwrap();
        let result = compiler.compile("int main() { return 42; }").unwrap();

        assert!(result.success);

        let executor = Executor::default();
        let exec_result = executor
            .execute(result.binary_path.as_ref().unwrap())
            .unwrap();

        assert!(exec_result.completed);
        assert_eq!(exec_result.exit_code, Some(42));
    }

    #[test]
    fn test_execute_with_output() {
        let mut compiler = Compiler::new().unwrap();
        let source = r#"
            #include <stdio.h>
            int main() {
                printf("hello\n");
                return 0;
            }
        "#;
        let result = compiler.compile(source).unwrap();

        if result.success {
            let executor = Executor::default();
            let exec_result = executor
                .execute(result.binary_path.as_ref().unwrap())
                .unwrap();

            assert!(exec_result.completed);
            assert_eq!(exec_result.stdout.trim(), "hello");
        }
    }

    #[test]
    fn test_timeout_config() {
        let config = TimeoutConfig::new(10, 60);
        assert_eq!(config.execution_timeout, Duration::from_secs(10));
        assert_eq!(config.total_timeout, Duration::from_secs(60));
    }
}
