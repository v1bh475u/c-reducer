use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::ValidationResult;

#[derive(Debug, Clone, Copy)]
pub struct TimeoutConfig {
    pub execution_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            execution_timeout: Duration::from_secs(5),
        }
    }
}

impl TimeoutConfig {
    pub fn new(execution_secs: u64) -> Self {
        Self {
            execution_timeout: Duration::from_secs(execution_secs),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub completed: bool,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

impl ExecutionResult {
    pub fn success(&self) -> bool {
        self.completed && self.exit_code == Some(0)
    }
}

#[derive(Debug, Clone)]
pub struct Executor {
    timeout_config: TimeoutConfig,
}

impl Default for Executor {
    fn default() -> Self {
        Self::new(TimeoutConfig::default())
    }
}

impl Executor {
    pub fn new(timeout_config: TimeoutConfig) -> Self {
        Self { timeout_config }
    }

    pub fn execute(&self, binary_path: &Path) -> ValidationResult<ExecutionResult> {
        let start = Instant::now();
        let mut cmd = Command::new(binary_path);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let timeout = self.timeout_config.execution_timeout;

        loop {
            match child.try_wait()? {
                Some(status) => {
                    let output = self.collect_output(&mut child)?;
                    return Ok(ExecutionResult {
                        completed: true,
                        timed_out: false,
                        stdout: output.0,
                        stderr: output.1,
                        exit_code: status.code(),
                    });
                }
                None => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Ok(ExecutionResult {
                            completed: false,
                            timed_out: true,
                            stdout: String::new(),
                            stderr: String::new(),
                            exit_code: None,
                        });
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
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
        let exec_result = executor.execute(result.binary_path.as_ref().unwrap()).unwrap();
        assert!(exec_result.completed);
        assert_eq!(exec_result.exit_code, Some(42));
    }

    #[test]
    fn test_execute_with_output() {
        let mut compiler = Compiler::new().unwrap();
        let source = "#include <stdio.h>\nint main() { printf(\"hello\\n\"); return 0; }\n";
        let result = compiler.compile(source).unwrap();
        if result.success {
            let executor = Executor::default();
            let exec_result = executor.execute(result.binary_path.as_ref().unwrap()).unwrap();
            assert!(exec_result.completed);
            assert_eq!(exec_result.stdout.trim(), "hello");
        }
    }

    #[test]
    fn test_timeout_config() {
        let config = TimeoutConfig::new(10);
        assert_eq!(config.execution_timeout, Duration::from_secs(10));
    }
}
