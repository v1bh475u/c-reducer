//! C Compiler wrapper for validation.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;
use tracing::{debug, trace};

use crate::error::{ValidationError, ValidationResult};

/// Result of a compilation attempt.
#[derive(Debug)]
pub struct CompilationResult {
    pub success: bool,
    pub binary_path: Option<PathBuf>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_secs: f64,
}

impl CompilationResult {
    pub fn success(binary_path: PathBuf, duration_secs: f64) -> Self {
        Self {
            success: true,
            binary_path: Some(binary_path),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_secs,
        }
    }

    pub fn failure(
        stdout: String,
        stderr: String,
        exit_code: Option<i32>,
        duration_secs: f64,
    ) -> Self {
        Self {
            success: false,
            binary_path: None,
            stdout,
            stderr,
            exit_code,
            duration_secs,
        }
    }
}

/// Compiler configuration.
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    pub compiler: PathBuf,
    pub flags: Vec<String>,
    pub include_paths: Vec<PathBuf>,
    pub lib_paths: Vec<PathBuf>,
    pub libs: Vec<String>,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            compiler: PathBuf::from("gcc"),
            flags: vec!["-w".into()], // Suppress warnings by default
            include_paths: Vec::new(),
            lib_paths: Vec::new(),
            libs: Vec::new(),
        }
    }
}

impl CompilerConfig {
    pub fn new(compiler: impl Into<PathBuf>) -> Self {
        Self {
            compiler: compiler.into(),
            ..Default::default()
        }
    }

    pub fn with_flag(mut self, flag: impl Into<String>) -> Self {
        self.flags.push(flag.into());
        self
    }

    pub fn with_flags(mut self, flags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.flags.extend(flags.into_iter().map(Into::into));
        self
    }

    pub fn with_include(mut self, path: impl Into<PathBuf>) -> Self {
        self.include_paths.push(path.into());
        self
    }

    pub fn with_coverage(self) -> Self {
        self.with_flags(["--coverage", "-fprofile-arcs", "-ftest-coverage"])
    }
}

/// C Compiler wrapper.
#[derive(Debug)]
pub struct Compiler {
    config: CompilerConfig,
    temp_dir: Option<TempDir>,
}

impl Compiler {
    pub fn new() -> ValidationResult<Self> {
        Self::with_config(CompilerConfig::default())
    }

    pub fn with_config(config: CompilerConfig) -> ValidationResult<Self> {
        // Verify compiler exists
        if !Self::compiler_exists(&config.compiler) {
            return Err(ValidationError::CompilerNotFound(
                config.compiler.display().to_string(),
            ));
        }

        Ok(Self {
            config,
            temp_dir: None,
        })
    }

    fn compiler_exists(compiler: &Path) -> bool {
        Command::new("which")
            .arg(compiler)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn ensure_temp_dir(&mut self) -> ValidationResult<&Path> {
        if self.temp_dir.is_none() {
            self.temp_dir = Some(TempDir::new()?);
        }
        Ok(self.temp_dir.as_ref().unwrap().path())
    }

    pub fn compile(&mut self, source: &str) -> ValidationResult<CompilationResult> {
        let temp_dir = self.ensure_temp_dir()?;
        let source_path = temp_dir.join("input.c");
        let binary_path = temp_dir.join("a.out");

        std::fs::write(&source_path, source)?;
        self.compile_file(&source_path, &binary_path)
    }

    pub fn compile_file(
        &self,
        source_path: &Path,
        output_path: &Path,
    ) -> ValidationResult<CompilationResult> {
        let start = std::time::Instant::now();

        let mut cmd = Command::new(&self.config.compiler);
        cmd.arg(source_path).arg("-o").arg(output_path);

        // Add flags
        for flag in &self.config.flags {
            cmd.arg(flag);
        }

        // Add include paths
        for path in &self.config.include_paths {
            cmd.arg("-I").arg(path);
        }

        // Add library paths
        for path in &self.config.lib_paths {
            cmd.arg("-L").arg(path);
        }

        // Add libraries
        for lib in &self.config.libs {
            cmd.arg("-l").arg(lib);
        }

        debug!("compiling: {:?}", cmd);

        let output: Output = cmd.output()?;
        let duration = start.elapsed().as_secs_f64();

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code();

        trace!("compilation took {:.3}s", duration);

        if output.status.success() {
            Ok(CompilationResult::success(
                output_path.to_path_buf(),
                duration,
            ))
        } else {
            Ok(CompilationResult::failure(
                stdout, stderr, exit_code, duration,
            ))
        }
    }

    pub fn compile_with_coverage(&mut self, source: &str) -> ValidationResult<CompilationResult> {
        let temp_dir = self.ensure_temp_dir()?;
        let source_path = temp_dir.join("input.c");
        let binary_path = temp_dir.join("a.out");

        std::fs::write(&source_path, source)?;

        // Add coverage flags temporarily
        let original_flags = self.config.flags.clone();
        self.config.flags.extend([
            "--coverage".into(),
            "-fprofile-arcs".into(),
            "-ftest-coverage".into(),
        ]);

        let result = self.compile_file(&source_path, &binary_path);

        // Restore original flags
        self.config.flags = original_flags;

        result
    }

    pub fn temp_dir(&self) -> Option<&Path> {
        self.temp_dir.as_ref().map(|d| d.path())
    }

    pub fn config(&self) -> &CompilerConfig {
        &self.config
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new().expect("default compiler should exist")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_simple() {
        let mut compiler = Compiler::new().unwrap();
        let result = compiler.compile("int main() { return 0; }").unwrap();

        assert!(result.success);
        assert!(result.binary_path.is_some());
    }

    #[test]
    fn test_compile_error() {
        let mut compiler = Compiler::new().unwrap();
        let result = compiler.compile("this is not valid c code").unwrap();

        assert!(!result.success);
        assert!(!result.stderr.is_empty());
    }

    #[test]
    fn test_compiler_config() {
        let config = CompilerConfig::new("gcc").with_flag("-O2").with_coverage();

        assert!(config.flags.contains(&"-O2".to_string()));
        assert!(config.flags.contains(&"--coverage".to_string()));
    }
}
