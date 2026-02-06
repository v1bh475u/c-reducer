//! AST types and parsing using libclang.

use clang::{Clang, Entity, EntityKind, Index, TranslationUnit};
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use tempfile::TempDir;

use crate::error::{ParseError, ParseResult};
use crate::span::ByteRange;

// Thread-local Clang instance (each thread gets its own).
thread_local! {
    static CLANG: RefCell<Option<Clang>> = const { RefCell::new(None) };
}

fn with_clang<F, R>(f: F) -> ParseResult<R>
where
    F: FnOnce(&Clang) -> ParseResult<R>,
{
    CLANG.with(|cell| {
        let mut clang_opt = cell.borrow_mut();
        if clang_opt.is_none() {
            *clang_opt = Some(Clang::new().map_err(|e| ParseError::clang(format!("{:?}", e)))?);
        }
        f(clang_opt.as_ref().unwrap())
    })
}

/// C Parser using libclang.
pub struct CParser {
    temp_dir: TempDir,
    args: Vec<String>,
}

impl CParser {
    pub fn new() -> ParseResult<Self> {
        let temp_dir = TempDir::new()?;

        Ok(Self {
            temp_dir,
            args: vec!["-std=c11".to_string(), "-ferror-limit=0".to_string()],
        })
    }

    pub fn with_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn with_includes(mut self, paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Self {
        for path in paths {
            self.args.push(format!("-I{}", path.as_ref().display()));
        }
        self
    }

    /// Parse source code from a string.
    ///
    /// Returns a ParsedUnit that extracts all information eagerly
    /// so there are no lifetime issues.
    pub fn parse(&self, source: &str) -> ParseResult<ParsedUnit> {
        // Write source to temp file
        let source_path = self.temp_dir.path().join("input.c");
        std::fs::write(&source_path, source)?;

        let args = self.args.clone();

        with_clang(|clang| {
            let index = Index::new(clang, false, true);

            let tu = index
                .parser(&source_path)
                .arguments(&args)
                .parse()
                .map_err(|e| ParseError::TranslationUnit(format!("{:?}", e)))?;

            let main_file = source_path.to_string_lossy().to_string();

            // Extract all information eagerly
            Ok(ParsedUnit::extract(&tu, source, &main_file))
        })
    }
}

impl Default for CParser {
    fn default() -> Self {
        Self::new().expect("failed to create C parser")
    }
}

/// Context for collecting information during AST traversal.
struct VisitorContext {
    functions: Vec<Function>,
    declarations: Vec<Declaration>,
    function_calls: HashSet<String>,
    used_identifiers: HashSet<String>,
    includes: Vec<ByteRange>,
    statements: Vec<Statement>,
    expressions: Vec<Expression>,
    /// Track the end of header region (last extern or typedef before first function definition)
    header_end: usize,
    /// Whether we've seen a function definition yet
    seen_function_def: bool,
}

impl VisitorContext {
    fn new() -> Self {
        Self {
            functions: Vec::new(),
            declarations: Vec::new(),
            function_calls: HashSet::new(),
            used_identifiers: HashSet::new(),
            includes: Vec::new(),
            statements: Vec::new(),
            expressions: Vec::new(),
            header_end: 0,
            seen_function_def: false,
        }
    }
}

/// A parsed C translation unit.
///
/// This struct contains all the extracted information from parsing,
/// with no lifetime dependencies on the clang objects.
#[derive(Debug, Clone)]
pub struct ParsedUnit {
    source: String,
    main_file: String,
    functions: Vec<Function>,
    declarations: Vec<Declaration>,
    function_calls: HashSet<String>,
    used_identifiers: HashSet<String>,
    includes: Vec<ByteRange>,
    statements: Vec<Statement>,
    expressions: Vec<Expression>,
    /// Byte offset where user code begins (after headers)
    header_end: usize,
    has_errors: bool,
    diagnostics: Vec<String>,
}

impl ParsedUnit {
    /// Extract all information from a TranslationUnit.
    fn extract(tu: &TranslationUnit<'_>, source: &str, main_file: &str) -> Self {
        let mut ctx = VisitorContext::new();
        let root = tu.get_entity();

        // Extract all information in a single pass
        Self::visit_entity(&root, source, main_file, &mut ctx);

        // Only consider errors from the main file (not system headers)
        let has_errors = tu.get_diagnostics().iter().any(|d| {
            if d.get_severity() < clang::diagnostic::Severity::Error {
                return false;
            }
            // Check if the error is in the main file
            d.get_location()
                .get_file_location()
                .file
                .map(|f| f.get_path().to_string_lossy() == main_file)
                .unwrap_or(false)
        });

        let diagnostics = tu.get_diagnostics().iter().map(|d| d.get_text()).collect();

        Self {
            source: source.to_string(),
            main_file: main_file.to_string(),
            functions: ctx.functions,
            declarations: ctx.declarations,
            function_calls: ctx.function_calls,
            used_identifiers: ctx.used_identifiers,
            includes: ctx.includes,
            statements: ctx.statements,
            expressions: ctx.expressions,
            header_end: ctx.header_end,
            has_errors,
            diagnostics,
        }
    }

    fn visit_entity(entity: &Entity<'_>, source: &str, main_file: &str, ctx: &mut VisitorContext) {
        let is_in_main_file = entity
            .get_location()
            .and_then(|loc| loc.get_file_location().file)
            .map(|f| f.get_path().to_string_lossy() == main_file)
            .unwrap_or(false);

        // Get range for this entity
        let entity_range = entity.get_range().map(|r| {
            let start = r.get_start().get_file_location();
            let end = r.get_end().get_file_location();
            ByteRange::new(start.offset as usize, end.offset as usize)
        });

        match entity.get_kind() {
            EntityKind::FunctionDecl if is_in_main_file && entity.is_definition() => {
                // Mark that we've seen a function definition - header region ends here
                if !ctx.seen_function_def {
                    ctx.seen_function_def = true;
                    if let Some(ref range) = entity_range {
                        ctx.header_end = range.start;
                    }
                }
                ctx.functions.push(Function::from_entity(entity, source));
            },
            EntityKind::FunctionDecl if is_in_main_file && !entity.is_definition() => {
                // Extern function declaration - still in header region
                if !ctx.seen_function_def {
                    if let Some(ref range) = entity_range {
                        ctx.header_end = range.end;
                    }
                }
            },
            EntityKind::VarDecl
            | EntityKind::TypedefDecl
            | EntityKind::StructDecl
            | EntityKind::EnumDecl
                if is_in_main_file =>
            {
                // Track header end for non-function declarations
                if !ctx.seen_function_def {
                    if let Some(ref range) = entity_range {
                        ctx.header_end = range.end;
                    }
                }
                if let Some(decl) = Declaration::from_entity(entity, source) {
                    ctx.declarations.push(decl);
                }
            },
            EntityKind::InclusionDirective if is_in_main_file => {
                if let Some(range) = entity.get_range() {
                    let start = range.get_start().get_file_location();
                    let end = range.get_end().get_file_location();
                    ctx.includes
                        .push(ByteRange::new(start.offset as usize, end.offset as usize));
                }
            },
            EntityKind::CallExpr => {
                if let Some(referenced) = entity.get_reference() {
                    if let Some(name) = referenced.get_name() {
                        ctx.function_calls.insert(name);
                    }
                }
                // Collect as expression
                if is_in_main_file {
                    if let Some(range) = entity_range {
                        let func_name = entity.get_reference()
                            .and_then(|r| r.get_name())
                            .unwrap_or_default();
                        let arg_count = entity.get_children().len().saturating_sub(1);
                        ctx.expressions.push(Expression {
                            kind: ExpressionKind::Call { function: func_name, arg_count },
                            range,
                            type_info: entity.get_type().map(TypeInfo::from_type),
                        });
                    }
                }
            },
            EntityKind::BinaryOperator if is_in_main_file => {
                if let Some(range) = entity_range {
                    // Get operator from the source text
                    let text = range.extract(source).unwrap_or("");
                    let operator = Self::extract_binary_operator(text);
                    ctx.expressions.push(Expression {
                        kind: ExpressionKind::Binary { operator },
                        range,
                        type_info: entity.get_type().map(TypeInfo::from_type),
                    });
                }
            },
            EntityKind::UnaryOperator if is_in_main_file => {
                if let Some(range) = entity_range {
                    let text = range.extract(source).unwrap_or("");
                    let operator = Self::extract_unary_operator(text);
                    ctx.expressions.push(Expression {
                        kind: ExpressionKind::Unary { operator },
                        range,
                        type_info: entity.get_type().map(TypeInfo::from_type),
                    });
                }
            },
            EntityKind::ParenExpr if is_in_main_file => {
                if let Some(range) = entity_range {
                    ctx.expressions.push(Expression {
                        kind: ExpressionKind::Paren,
                        range,
                        type_info: entity.get_type().map(TypeInfo::from_type),
                    });
                }
            },
            EntityKind::DeclRefExpr => {
                if let Some(referenced) = entity.get_reference() {
                    if let Some(name) = referenced.get_name() {
                        ctx.used_identifiers.insert(name.clone());
                        if is_in_main_file {
                            if let Some(range) = entity_range {
                                ctx.expressions.push(Expression {
                                    kind: ExpressionKind::Identifier(name),
                                    range,
                                    type_info: entity.get_type().map(TypeInfo::from_type),
                                });
                            }
                        }
                    }
                }
            },
            EntityKind::IntegerLiteral if is_in_main_file => {
                if let Some(range) = entity_range {
                    ctx.expressions.push(Expression {
                        kind: ExpressionKind::IntegerLiteral(0), // We don't parse the value
                        range,
                        type_info: entity.get_type().map(TypeInfo::from_type),
                    });
                }
            },
            // Collect statements
            EntityKind::CompoundStmt if is_in_main_file => {
                if let Some(range) = entity_range {
                    ctx.statements.push(Statement {
                        kind: StatementKind::Compound,
                        range,
                    });

                    // Identify expression statements: children of a CompoundStmt
                    // that are expressions (not other statement kinds).
                    // In Clang's AST, `a = 1;` is a BinaryOperator child of CompoundStmt
                    // with no wrapping ExprStmt node.
                    for child in entity.get_children() {
                        let is_expr_stmt = matches!(
                            child.get_kind(),
                            EntityKind::BinaryOperator
                                | EntityKind::UnaryOperator
                                | EntityKind::CallExpr
                                | EntityKind::CompoundAssignOperator
                                | EntityKind::ConditionalOperator
                                | EntityKind::ParenExpr
                        );
                        if is_expr_stmt {
                            if let Some(child_range) = child.get_range() {
                                let start = child_range.get_start().get_file_location();
                                let end = child_range.get_end().get_file_location();
                                let mut end_offset = end.offset as usize;
                                // Extend past the trailing `;`
                                if source.as_bytes().get(end_offset) == Some(&b';') {
                                    end_offset += 1;
                                }
                                ctx.statements.push(Statement {
                                    kind: StatementKind::Expression,
                                    range: ByteRange::new(start.offset as usize, end_offset),
                                });
                            }
                        }
                    }
                }
            },
            EntityKind::DeclStmt if is_in_main_file => {
                if let Some(range) = entity_range {
                    ctx.statements.push(Statement {
                        kind: StatementKind::Declaration,
                        range,
                    });
                }
            },
            EntityKind::ReturnStmt if is_in_main_file => {
                if let Some(range) = entity_range {
                    ctx.statements.push(Statement {
                        kind: StatementKind::Return,
                        range,
                    });
                }
            },
            EntityKind::IfStmt if is_in_main_file => {
                if let Some(range) = entity_range {
                    let has_else = entity.get_children().len() > 2;
                    ctx.statements.push(Statement {
                        kind: StatementKind::If { has_else },
                        range,
                    });
                }
            },
            EntityKind::WhileStmt if is_in_main_file => {
                if let Some(range) = entity_range {
                    ctx.statements.push(Statement {
                        kind: StatementKind::While,
                        range,
                    });
                }
            },
            EntityKind::ForStmt if is_in_main_file => {
                if let Some(range) = entity_range {
                    ctx.statements.push(Statement {
                        kind: StatementKind::For,
                        range,
                    });
                }
            },
            EntityKind::DoStmt if is_in_main_file => {
                if let Some(range) = entity_range {
                    ctx.statements.push(Statement {
                        kind: StatementKind::DoWhile,
                        range,
                    });
                }
            },
            EntityKind::SwitchStmt if is_in_main_file => {
                if let Some(range) = entity_range {
                    ctx.statements.push(Statement {
                        kind: StatementKind::Switch,
                        range,
                    });
                }
            },
            EntityKind::BreakStmt if is_in_main_file => {
                if let Some(range) = entity_range {
                    ctx.statements.push(Statement {
                        kind: StatementKind::Break,
                        range,
                    });
                }
            },
            EntityKind::ContinueStmt if is_in_main_file => {
                if let Some(range) = entity_range {
                    ctx.statements.push(Statement {
                        kind: StatementKind::Continue,
                        range,
                    });
                }
            },
            EntityKind::NullStmt if is_in_main_file => {
                if let Some(range) = entity_range {
                    ctx.statements.push(Statement {
                        kind: StatementKind::Null,
                        range,
                    });
                }
            },
            _ => {},
        }

        // Recurse into children
        for child in entity.get_children() {
            Self::visit_entity(&child, source, main_file, ctx);
        }
    }

    /// Extract binary operator from expression text
    fn extract_binary_operator(text: &str) -> String {
        let operators = ["&&", "||", "==", "!=", "<=", ">=", "<<", ">>", 
                         "+", "-", "*", "/", "%", "&", "|", "^", "<", ">", "="];
        for op in operators {
            if text.contains(op) {
                return op.to_string();
            }
        }
        "?".to_string()
    }

    /// Extract unary operator from expression text
    fn extract_unary_operator(text: &str) -> String {
        if text.starts_with("++") || text.ends_with("++") { "++".to_string() }
        else if text.starts_with("--") || text.ends_with("--") { "--".to_string() }
        else if text.starts_with('!') { "!".to_string() }
        else if text.starts_with('~') { "~".to_string() }
        else if text.starts_with('-') { "-".to_string() }
        else if text.starts_with('&') { "&".to_string() }
        else if text.starts_with('*') { "*".to_string() }
        else { "?".to_string() }
    }

    /// Get the source code.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get the main file path.
    pub fn main_file(&self) -> &str {
        &self.main_file
    }

    /// Check if the translation unit has any errors.
    pub fn has_errors(&self) -> bool {
        self.has_errors
    }

    /// Get all diagnostics.
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Get all functions defined in the main file.
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }

    /// Get all top-level declarations in the main file.
    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
    }

    /// Find all function calls in the translation unit.
    pub fn function_calls(&self) -> &HashSet<String> {
        &self.function_calls
    }

    /// Find all identifiers used in the translation unit.
    pub fn used_identifiers(&self) -> &HashSet<String> {
        &self.used_identifiers
    }

    /// Get all include directives.
    pub fn includes(&self) -> &[ByteRange] {
        &self.includes
    }

    /// Get all typedef declarations.
    pub fn typedefs(&self) -> Vec<Declaration> {
        self.declarations
            .iter()
            .filter(|d| matches!(d.kind, DeclarationKind::Typedef { .. }))
            .cloned()
            .collect()
    }

    /// Find the main function.
    pub fn main_function(&self) -> Option<Function> {
        self.functions.iter().find(|f| f.name == "main").cloned()
    }

    /// Get all statements in the main file.
    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }

    /// Get all expressions in the main file.
    pub fn expressions(&self) -> &[Expression] {
        &self.expressions
    }

    /// Get the byte offset where headers end and user code begins.
    pub fn header_end(&self) -> usize {
        self.header_end
    }

    /// Check if a byte offset is in the header region.
    pub fn is_in_header(&self, offset: usize) -> bool {
        offset < self.header_end
    }
}

/// A function in the source code.
#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub return_type: TypeInfo,
    pub parameters: Vec<Parameter>,
    pub range: ByteRange,
    pub body_range: Option<ByteRange>,
    pub is_definition: bool,
}

impl Function {
    fn from_entity(entity: &Entity<'_>, source: &str) -> Self {
        let name = entity.get_name().unwrap_or_default();
        let return_type = entity
            .get_result_type()
            .map(TypeInfo::from_type)
            .unwrap_or_else(TypeInfo::void);

        let parameters: Vec<_> = entity
            .get_arguments()
            .map(|args| args.iter().map(Parameter::from_entity).collect())
            .unwrap_or_default();

        let range = entity
            .get_range()
            .map(|r| {
                let start = r.get_start().get_file_location();
                let end = r.get_end().get_file_location();
                ByteRange::new(start.offset as usize, end.offset as usize)
            })
            .unwrap_or_else(|| ByteRange::new(0, 0));

        // Try to find the body range by looking for braces
        let body_range = if entity.is_definition() {
            let text = range.extract(source).unwrap_or("");
            if let Some(brace_pos) = text.find('{') {
                let body_start = range.start + brace_pos;
                Some(ByteRange::new(body_start, range.end))
            } else {
                None
            }
        } else {
            None
        };

        let is_definition = entity.is_definition();

        Self {
            name,
            return_type,
            parameters,
            range,
            body_range,
            is_definition,
        }
    }
}

/// A function parameter.
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub type_info: TypeInfo,
}

impl Parameter {
    fn from_entity(entity: &Entity<'_>) -> Self {
        Self {
            name: entity.get_name().unwrap_or_default(),
            type_info: entity
                .get_type()
                .map(TypeInfo::from_type)
                .unwrap_or_else(TypeInfo::void),
        }
    }
}

/// Type information.
#[derive(Debug, Clone)]
pub struct TypeInfo {
    pub name: String,
    pub is_pointer: bool,
    pub is_const: bool,
}

impl TypeInfo {
    fn from_type(ty: clang::Type<'_>) -> Self {
        Self {
            name: ty.get_display_name(),
            is_pointer: ty.get_kind() == clang::TypeKind::Pointer,
            is_const: ty.is_const_qualified(),
        }
    }

    fn void() -> Self {
        Self {
            name: "void".to_string(),
            is_pointer: false,
            is_const: false,
        }
    }
}

/// A declaration in the source code.
#[derive(Debug, Clone)]
pub struct Declaration {
    pub name: String,
    pub kind: DeclarationKind,
    pub range: ByteRange,
}

impl Declaration {
    fn from_entity(entity: &Entity<'_>, _source: &str) -> Option<Self> {
        let name = entity.get_name()?;

        let kind = match entity.get_kind() {
            EntityKind::VarDecl => {
                let type_info = entity
                    .get_type()
                    .map(TypeInfo::from_type)
                    .unwrap_or_else(TypeInfo::void);
                DeclarationKind::Variable { type_info }
            },
            EntityKind::TypedefDecl => {
                let underlying = entity
                    .get_typedef_underlying_type()
                    .map(TypeInfo::from_type)
                    .unwrap_or_else(TypeInfo::void);
                DeclarationKind::Typedef { underlying }
            },
            EntityKind::StructDecl => DeclarationKind::Struct,
            EntityKind::EnumDecl => DeclarationKind::Enum,
            _ => return None,
        };

        let range = entity
            .get_range()
            .map(|r| {
                let start = r.get_start().get_file_location();
                let end = r.get_end().get_file_location();
                ByteRange::new(start.offset as usize, end.offset as usize)
            })
            .unwrap_or_else(|| ByteRange::new(0, 0));

        Some(Self { name, kind, range })
    }
}

/// Kind of declaration.
#[derive(Debug, Clone)]
pub enum DeclarationKind {
    Function { return_type: TypeInfo },
    Variable { type_info: TypeInfo },
    Typedef { underlying: TypeInfo },
    Struct,
    Enum,
    Include { path: String, is_system: bool },
    Macro { definition: String },
}

/// A statement in the source code.
#[derive(Debug, Clone)]
pub struct Statement {
    pub kind: StatementKind,
    pub range: ByteRange,
}

/// Kind of statement.
#[derive(Debug, Clone)]
pub enum StatementKind {
    Declaration,
    Expression,
    If { has_else: bool },
    While,
    For,
    DoWhile,
    Switch,
    Return,
    Break,
    Continue,
    Goto { label: String },
    Label { name: String },
    Compound,
    Null,
}

/// An expression in the source code.
#[derive(Debug, Clone)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub range: ByteRange,
    pub type_info: Option<TypeInfo>,
}

/// Kind of expression.
#[derive(Debug, Clone)]
pub enum ExpressionKind {
    Identifier(String),
    IntegerLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    Binary { operator: String },
    Unary { operator: String },
    Call { function: String, arg_count: usize },
    Subscript,
    Member { field: String },
    Cast { target_type: TypeInfo },
    Conditional,
    Comma,
    Sizeof,
    Paren,
}

/// Common trait for AST nodes.
pub trait AstNode {
    fn range(&self) -> ByteRange;

    fn text<'a>(&self, source: &'a str) -> Option<&'a str> {
        self.range().extract(source)
    }
}

impl AstNode for Function {
    fn range(&self) -> ByteRange {
        self.range
    }
}

impl AstNode for Declaration {
    fn range(&self) -> ByteRange {
        self.range
    }
}

impl AstNode for Statement {
    fn range(&self) -> ByteRange {
        self.range
    }
}

impl AstNode for Expression {
    fn range(&self) -> ByteRange {
        self.range
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let parser = CParser::default();
        let source = "int main() { return 0; }";

        let unit = parser.parse(source).unwrap();
        let functions = unit.functions();

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "main");
        assert!(functions[0].is_definition);
    }

    #[test]
    fn test_find_function_calls() {
        let parser = CParser::default();
        let source = r#"
void foo() {}
void bar() {}
int main() {
    foo();
    return 0;
}
"#;

        let unit = parser.parse(source).unwrap();
        let calls = unit.function_calls();

        assert!(calls.contains("foo"));
        assert!(!calls.contains("bar"));
    }

    #[test]
    fn test_multiple_functions() {
        let parser = CParser::default();
        let source = r#"
int add(int a, int b) {
    return a + b;
}

int main() {
    return add(1, 2);
}
"#;

        let unit = parser.parse(source).unwrap();
        let functions = unit.functions();

        assert_eq!(functions.len(), 2);

        let names: Vec<_> = functions.iter().map(|f| &f.name).collect();
        assert!(names.contains(&&"add".to_string()));
        assert!(names.contains(&&"main".to_string()));
    }
}
