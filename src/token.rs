use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    Skill,
    Input,
    Output,
    Body,
    Context,
    Step,
    Requires,
    When,
    Use,
    Let,
    Emit,
    Import,
    From,
    Type,
    Pre,
    Post,
    Assert,
    Message,
    OnError,
    AllSteps,
    Extends,

    // Lazy context
    Lazy,
    Ref,
    Summary,
    Index,
    Section,
    Load,

    // Pipeline / Orchestration
    Pipeline,
    Stage,
    Orchestration,
    Agents,
    Phase,
    Shared,
    Rules,
    Cancel,
    Timeout,

    // Mixin / composition
    Mixin,
    Include,

    // Package management
    Package,
    Version,
    Description,
    Exports,

    // Prompt directives
    Reasoning,
    Examples,
    Example,
    Note,
    Format,
    Reinforce,
    Every,
    On,
    Sampling,
    Persona,

    // Tools / permissions
    Tools,
    Require,
    Optional,
    Mcp,
    Tool,
    Allow,
    Deny,
    Permissions,

    // Test framework
    Tests,
    Test,
    Given,
    Mock,
    Expect,
    Confidence,
    Runs,
    Snapshot,
    Compare,
    Equals,
    Contains,
    Matches,
    Resembles,
    Satisfies,
    Between,
    Unavailable,
    Failing,
    Slow,

    // Observability
    Observe,
    EmitEvent,
    Metric,

    // Control flow
    If,
    Retry,
    Backoff,

    // Primitives
    StringType,
    IntType,
    FloatType,
    BoolType,
    Enum,
    Map,

    // Literals
    StringLit(String),
    IntLit(i64),
    FloatLit(f64),
    BoolLit(bool),
    TripleString(String),

    // Identifiers
    Ident(String),

    // Punctuation
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Colon,
    Comma,
    Dot,
    Question,
    Eq,
    EqEq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Amp,
    AmpAmp,
    Pipe,
    PipePipe,
    Bang,
    Arrow,

    // Special
    Priority,
    Decay,
    Interpolation(String),

    // Meta
    Comment(String),
    Eof,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_display() {
        let span = Span {
            start: 0,
            end: 5,
            line: 1,
            col: 1,
        };
        assert_eq!(format!("{span}"), "1:1");
    }

    #[test]
    fn token_kind_keywords() {
        assert_eq!(format!("{:?}", TokenKind::Skill), "Skill");
        assert_eq!(format!("{:?}", TokenKind::Step), "Step");
    }
}
