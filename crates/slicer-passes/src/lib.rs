mod clangd;
mod dead_code;
mod dead_function;
mod header_removal;
mod statement_merge;
mod typedef;
mod unused_variable;
mod util;

pub use dead_code::DeadCodePass;
pub use dead_function::DeadFunctionPass;
pub use header_removal::HeaderRemovalPass;
pub use statement_merge::StatementMergePass;
pub use typedef::TypedefPass;
pub use unused_variable::UnusedVariablePass;

use slicer_core::pass::ReductionPass;

pub fn all_passes() -> Vec<Box<dyn ReductionPass>> {
    vec![
        Box::new(DeadFunctionPass),
        Box::new(DeadCodePass),
        Box::new(UnusedVariablePass),
        Box::new(StatementMergePass),
        Box::new(TypedefPass),
        Box::new(HeaderRemovalPass),
    ]
}
