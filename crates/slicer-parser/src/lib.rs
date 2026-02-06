//! C Parser for the Slicer using libclang
//!
//! This crate provides libclang-based parsing and AST operations for C source code.
//! It supports:
//!
//! - Full macro expansion
//! - Type information
//! - Include resolution
//! - Semantic analysis

mod ast;
mod error;
mod span;

pub use ast::{
    AstNode, CParser, Declaration, DeclarationKind, Expression, ExpressionKind, Function,
    ParsedUnit, Statement, StatementKind, TypeInfo,
};
pub use error::{ParseError, ParseResult};
pub use span::ByteRange;
