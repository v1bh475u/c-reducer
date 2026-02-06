//! Validator for C Program Reduction
//!
//! This crate provides compilation, execution, and coverage validation
//! for reduced C programs.

mod compiler;
mod coverage;
mod error;
mod executor;
mod oracle;

pub use compiler::{CompilationResult, Compiler, CompilerConfig};
pub use coverage::{CoverageAnalyzer, CoverageReport, LineCoverage};
pub use error::{ValidationError, ValidationResult};
pub use executor::{ExecutionResult, Executor, TimeoutConfig};
pub use oracle::{Oracle, OracleConfig};
