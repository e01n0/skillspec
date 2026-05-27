use crate::token::Span;
use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum SkillSpecError {
    #[error("Unexpected token '{found}' at {span}, expected {expected}")]
    UnexpectedToken {
        found: String,
        expected: String,
        span: Span,
    },

    #[error("Unknown type '{name}' at {span}")]
    UnknownType { name: String, span: Span },

    #[error("Duplicate field '{name}' at {span}")]
    DuplicateField { name: String, span: Span },

    #[error("Dependency cycle detected: {cycle}")]
    DependencyCycle { cycle: String },

    #[error("Multiple emit statements on the same execution path at {span}")]
    MultipleEmit { span: Span },

    #[error("Unknown step '{name}' in requires clause at {span}")]
    UnknownStep { name: String, span: Span },

    #[error("Unknown lazy context '{name}' referenced in load at {span}")]
    UnknownLazyContext { name: String, span: Span },

    #[error("Unknown mixin '{name}' referenced in include at {span}")]
    UnknownMixin { name: String, span: Span },

    #[error("Unknown agent '{name}' referenced in phase at {span}")]
    UnknownAgent { name: String, span: Span },

    #[error("Skill extends unknown skill '{name}' at {span}")]
    UnresolvedExtends { name: String, span: Span },

    #[error("Import symbol '{name}' shadows local type definition at {span}")]
    ShadowedImport { name: String, span: Span },

    #[error("Cannot resolve import path '{path}' at {span}")]
    UnresolvedImport { path: String, span: Span },

    #[error("Failed to parse imported file '{path}': {message} (at {span})")]
    ImportParseError {
        path: String,
        message: String,
        span: Span,
    },

    #[error("Symbol '{symbol}' not found in imported file '{path}' at {span}")]
    ImportSymbolNotFound {
        symbol: String,
        path: String,
        span: Span,
    },

    #[error("Lazy context '{name}' references missing file '{path}' at {span}")]
    UnresolvedRef {
        name: String,
        path: String,
        span: Span,
    },

    #[error("Unknown skill '{name}' referenced in use call at {span}")]
    UnknownSkill { name: String, span: Span },

    #[error("Argument mismatch in use call to '{skill_name}': {message} (at {span})")]
    MismatchedArg {
        skill_name: String,
        message: String,
        span: Span,
    },

    #[error("Test '{test_name}' references missing fixture '{path}' at {span}")]
    UnresolvedFixturePath {
        path: String,
        test_name: String,
        span: Span,
    },

    #[error("Test '{test_name}' fixture '{path}' failed to parse: {message} (at {span})")]
    FixtureParseError {
        path: String,
        message: String,
        test_name: String,
        span: Span,
    },

    #[error("Test '{test_name}' given key '{key}' is not a declared input field at {span}")]
    UnknownGivenKey {
        key: String,
        test_name: String,
        span: Span,
    },

    #[error(
        "Test '{test_name}' expects 'output.{field}' but '{field}' is not a declared output field at {span}"
    )]
    UnknownExpectField {
        field: String,
        test_name: String,
        span: Span,
    },

    #[error(
        "Test '{test_name}' mocks tool '{tool_path}' which is not declared in tools block at {span}"
    )]
    UnknownMockTool {
        tool_path: String,
        test_name: String,
        span: Span,
    },

    #[error("Lexer error: {message} at {span}")]
    LexerError { message: String, span: Span },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SkillSpecError>;
