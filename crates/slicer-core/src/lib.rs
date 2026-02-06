//! Core reduction pipeline for the C Program Slicer.
//!
//! This crate provides the main orchestration logic for reducing C programs
//! while preserving their behavior.
//!
//! # Architecture
//!
//! The slicer uses a serial pipeline of reduction passes:
//!
//! 1. Each pass generates candidates (potential reductions)
//! 2. Candidates are tested one at a time via the oracle
//! 3. Valid reductions are applied, transforming the source
//! 4. The next pass works on the transformed source
//! 5. Process repeats until no more reductions are possible

pub mod config;
pub mod context;
pub mod pass;
pub mod pipeline;
pub mod result;

pub use config::{PipelineConfig, PipelineConfigBuilder, Policy};
pub use context::{AlwaysValidOracle, Context, FnOracle, ValidationOracle};
pub use pass::{Candidate, ReductionPass};
pub use pipeline::Pipeline;
pub use result::{PassStats, ReductionResult};
