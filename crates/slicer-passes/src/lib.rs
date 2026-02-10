mod argument_removal;
mod clangd;
mod dead_code;
mod dead_function;
mod enum_struct_removal;
mod global_removal;
mod header_removal;
mod statement_merge;
mod struct_member_removal;
mod typedef;
mod unused_variable;
mod util;

pub use argument_removal::ArgumentRemovalPass;
pub use dead_code::DeadCodePass;
pub use dead_function::DeadFunctionPass;
pub use enum_struct_removal::EnumStructRemovalPass;
pub use global_removal::GlobalRemovalPass;
pub use header_removal::HeaderRemovalPass;
pub use statement_merge::StatementMergePass;
pub use struct_member_removal::StructMemberRemovalPass;
pub use typedef::TypedefPass;
pub use unused_variable::UnusedVariablePass;

use slicer_core::pass::ReductionPass;

pub fn all_passes() -> Vec<Box<dyn ReductionPass>> {
    vec![
        Box::new(DeadFunctionPass),
        Box::new(DeadCodePass),
        Box::new(UnusedVariablePass),
        Box::new(ArgumentRemovalPass),
        Box::new(GlobalRemovalPass),
        Box::new(StructMemberRemovalPass),
        Box::new(StatementMergePass),
        Box::new(TypedefPass),
        Box::new(EnumStructRemovalPass),
        Box::new(HeaderRemovalPass),
    ]
}
