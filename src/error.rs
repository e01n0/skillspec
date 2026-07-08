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

    #[error("Unknown step '{name}' in requires clause at {span}{suggestion}")]
    UnknownStep {
        name: String,
        span: Span,
        suggestion: String,
    },

    #[error("Unknown lazy context '{name}' referenced in load at {span}{suggestion}")]
    UnknownLazyContext {
        name: String,
        span: Span,
        suggestion: String,
    },

    #[error("Unknown mixin '{name}' referenced in include at {span}{suggestion}")]
    UnknownMixin {
        name: String,
        span: Span,
        suggestion: String,
    },

    #[error("Unknown agent '{name}' referenced in phase at {span}")]
    UnknownAgent { name: String, span: Span },

    #[error("Skill extends unknown skill '{name}' at {span}{suggestion}")]
    UnresolvedExtends {
        name: String,
        span: Span,
        suggestion: String,
    },

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

    #[error("Invalid name '{name}': must not contain path separators or '..' at {span}")]
    InvalidName { name: String, span: Span },

    #[error(
        "Skill '{skill_name}' exceeds its token budget: ~{estimated} estimated vs {max_tokens} declared at {span}"
    )]
    BudgetExceeded {
        skill_name: String,
        estimated: usize,
        max_tokens: i64,
        span: Span,
    },

    #[error("Budget max_tokens must be positive, got {max_tokens} at {span}")]
    InvalidBudget { max_tokens: i64, span: Span },

    #[error("Unknown compile target '{name}' in context at {span}, expected one of: {known}")]
    UnknownTargetName {
        name: String,
        known: String,
        span: Span,
    },

    #[error(
        "Context references '{{{placeholder}}}' but '{field}' is not a declared {section} field at {span}"
    )]
    UnknownPlaceholder {
        placeholder: String,
        field: String,
        section: String,
        span: Span,
    },

    #[error("Invalid version '{version}' at {span}: expected MAJOR.MINOR.PATCH")]
    InvalidVersion { version: String, span: Span },

    #[error("on_fail retry count must be at least 1, got {count} at {span}")]
    InvalidRetryCount { count: i64, span: Span },

    #[error("Lexer error: {message} at {span}")]
    LexerError { message: String, span: Span },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl SkillSpecError {
    /// The source span this error points at, when it has one.
    pub fn span(&self) -> Option<Span> {
        match self {
            SkillSpecError::UnexpectedToken { span, .. }
            | SkillSpecError::UnknownType { span, .. }
            | SkillSpecError::DuplicateField { span, .. }
            | SkillSpecError::MultipleEmit { span }
            | SkillSpecError::UnknownStep { span, .. }
            | SkillSpecError::UnknownLazyContext { span, .. }
            | SkillSpecError::UnknownMixin { span, .. }
            | SkillSpecError::UnknownAgent { span, .. }
            | SkillSpecError::UnresolvedExtends { span, .. }
            | SkillSpecError::ShadowedImport { span, .. }
            | SkillSpecError::UnresolvedImport { span, .. }
            | SkillSpecError::ImportParseError { span, .. }
            | SkillSpecError::ImportSymbolNotFound { span, .. }
            | SkillSpecError::UnresolvedRef { span, .. }
            | SkillSpecError::UnknownSkill { span, .. }
            | SkillSpecError::MismatchedArg { span, .. }
            | SkillSpecError::UnresolvedFixturePath { span, .. }
            | SkillSpecError::FixtureParseError { span, .. }
            | SkillSpecError::UnknownGivenKey { span, .. }
            | SkillSpecError::UnknownExpectField { span, .. }
            | SkillSpecError::UnknownMockTool { span, .. }
            | SkillSpecError::InvalidName { span, .. }
            | SkillSpecError::BudgetExceeded { span, .. }
            | SkillSpecError::InvalidBudget { span, .. }
            | SkillSpecError::UnknownTargetName { span, .. }
            | SkillSpecError::UnknownPlaceholder { span, .. }
            | SkillSpecError::InvalidVersion { span, .. }
            | SkillSpecError::InvalidRetryCount { span, .. }
            | SkillSpecError::LexerError { span, .. } => Some(*span),
            SkillSpecError::DependencyCycle { .. } | SkillSpecError::Io(_) => None,
        }
    }
}

/// Format a ", did you mean 'x'?" suffix when a close match exists among
/// the candidates, or an empty string otherwise.
pub fn did_you_mean<'a, I>(name: &str, candidates: I) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let mut best: Option<(usize, &str)> = None;
    for cand in candidates {
        let dist = edit_distance(name, cand);
        if best.is_none_or(|(d, _)| dist < d) {
            best = Some((dist, cand));
        }
    }
    match best {
        // Only suggest close matches — a distance beyond a third of the
        // name's length reads as noise, not help.
        Some((dist, cand)) if dist > 0 && dist <= (name.len() / 3).max(2) => {
            format!(", did you mean '{}'?", cand)
        }
        _ => String::new(),
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(curr[j] + 1);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Render the source line an error points at, with a caret underline:
///
/// ```text
///    12 |       requires analyz
///       |       ^
/// ```
pub fn render_snippet(source: &str, span: Span) -> Option<String> {
    if span.line == 0 {
        return None;
    }
    let line_text = source.lines().nth(span.line - 1)?;
    let gutter = span.line.to_string();
    let col = span.col.max(1);
    let underline_len = span.end.saturating_sub(span.start).max(1);
    let underline_len = underline_len
        .min(line_text.chars().count().saturating_sub(col - 1))
        .max(1);
    Some(format!(
        "  {} | {}\n  {} | {}{}",
        gutter,
        line_text,
        " ".repeat(gutter.len()),
        " ".repeat(col - 1),
        "^".repeat(underline_len),
    ))
}

pub type Result<T> = std::result::Result<T, SkillSpecError>;
