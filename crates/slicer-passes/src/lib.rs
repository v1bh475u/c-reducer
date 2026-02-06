//! Reduction Passes for C Program Slicing
//!
//! This crate provides a collection of reduction passes that transform
//! C source code to produce smaller, equivalent programs.

mod dead_code;
mod dead_function;
mod expression;
mod include;
mod statement;
mod statement_merge;
mod typedef;
mod util;

pub use dead_code::DeadCodePass;
pub use dead_function::DeadFunctionPass;
pub use expression::ExpressionSimplifyPass;
pub use include::IncludePass;
pub use statement::StatementPass;
pub use statement_merge::StatementMergePass;
pub use typedef::TypedefPass;

use slicer_core::pass::ReductionPass;

pub fn all_passes() -> Vec<Box<dyn ReductionPass>> {
    vec![
        Box::new(DeadFunctionPass::new()),
        Box::new(DeadCodePass::new()),
        Box::new(StatementPass::new()),
        Box::new(StatementMergePass::new()),
        Box::new(IncludePass::new()),
        Box::new(TypedefPass::new()),
        Box::new(ExpressionSimplifyPass::new()),
    ]
}
