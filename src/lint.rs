use std::collections::HashSet;
use crate::ast::*;
use crate::token::Span;

#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    pub rule: String,
    pub severity: Severity,
    pub message: String,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Warning,
    Info,
}

pub trait LintRule {
    fn name(&self) -> &str;
    fn check(&self, file: &SourceFile) -> Vec<LintDiagnostic>;
}

pub struct LintEngine {
    rules: Vec<Box<dyn LintRule>>,
}

impl Default for LintEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LintEngine {
    pub fn new() -> Self {
        LintEngine {
            rules: vec![
                Box::new(UniformPriority),
                Box::new(LargeContext),
                Box::new(WhenGuardAlwaysTrue),
                Box::new(EmptyStep),
                Box::new(UnreachableStep),
                Box::new(UnusedLazyContext),
            ],
        }
    }

    pub fn run(&self, file: &SourceFile) -> Vec<LintDiagnostic> {
        self.rules.iter().flat_map(|rule| rule.check(file)).collect()
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

impl std::fmt::Display for LintDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} [{}]: {}", self.severity, self.rule, self.message)
    }
}

// ── Rule: uniform-priority ──────────────────────────────────────────────────

pub struct UniformPriority;

impl LintRule for UniformPriority {
    fn name(&self) -> &str { "uniform-priority" }

    fn check(&self, file: &SourceFile) -> Vec<LintDiagnostic> {
        let mut out = Vec::new();
        for skill in &file.skills {
            let priorities: Vec<Priority> = skill.body.contexts.iter()
                .filter_map(|c| c.priority)
                .collect();
            if priorities.len() >= 2 && priorities.iter().all(|p| *p == priorities[0]) {
                out.push(LintDiagnostic {
                    rule: self.name().to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "skill '{}': all {} context blocks share priority '{}'; differentiate so the model knows what to emphasise",
                        skill.name, priorities.len(), priorities[0]
                    ),
                    span: Some(skill.span),
                });
            }
            let critical_count = priorities.iter().filter(|p| **p == Priority::Critical).count();
            if critical_count > 2 {
                out.push(LintDiagnostic {
                    rule: "critical-overuse".to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "skill '{}': {} context blocks marked critical; emphasis loses effect beyond 2 — consider downgrading some to important",
                        skill.name, critical_count
                    ),
                    span: Some(skill.span),
                });
            }
        }
        out
    }
}

// ── Rule: context-too-large ─────────────────────────────────────────────────

pub struct LargeContext;

impl LintRule for LargeContext {
    fn name(&self) -> &str { "context-too-large" }

    fn check(&self, file: &SourceFile) -> Vec<LintDiagnostic> {
        let mut out = Vec::new();
        for skill in &file.skills {
            Self::check_contexts(&mut out, &skill.name, None, &skill.body.contexts);
            for step in &skill.body.steps {
                Self::check_contexts(&mut out, &skill.name, Some(&step.name), &step.contexts);
            }
        }
        out
    }
}

impl LargeContext {
    fn check_contexts(out: &mut Vec<LintDiagnostic>, skill: &str, step: Option<&str>, contexts: &[ContextBlock]) {
        for ctx in contexts {
            if ctx.text.len() > 2000 {
                let tokens = ctx.text.len() / 4;
                let location = match step {
                    Some(s) => format!("skill '{}', step '{}'", skill, s),
                    None => format!("skill '{}'", skill),
                };
                out.push(LintDiagnostic {
                    rule: "context-too-large".to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "{}: context block is {} chars (~{} tokens); consider splitting or using a lazy context",
                        location, ctx.text.len(), tokens
                    ),
                    span: Some(ctx.span),
                });
            }
        }
    }
}

// ── Rule: when-guard-always-true ────────────────────────────────────────────

pub struct WhenGuardAlwaysTrue;

impl LintRule for WhenGuardAlwaysTrue {
    fn name(&self) -> &str { "when-guard-always-true" }

    fn check(&self, file: &SourceFile) -> Vec<LintDiagnostic> {
        let mut out = Vec::new();
        for skill in &file.skills {
            let required: HashSet<String> = skill.input.as_ref()
                .map(|fields| fields.iter()
                    .filter(|f| !f.optional)
                    .map(|f| f.name.clone())
                    .collect())
                .unwrap_or_default();
            if required.is_empty() { continue; }

            for ctx in &skill.body.contexts {
                if let Some(field) = ctx.when.as_ref().and_then(extract_input_field) {
                    if required.contains(&field) {
                        out.push(LintDiagnostic {
                            rule: self.name().to_string(),
                            severity: Severity::Warning,
                            message: format!(
                                "skill '{}': when guard references required field 'input.{}' — condition is always true",
                                skill.name, field
                            ),
                            span: Some(ctx.span),
                        });
                    }
                }
            }
        }
        out
    }
}

fn extract_input_field(expr: &Expr) -> Option<String> {
    if let Expr::FieldAccess(base, field) = expr {
        if let Expr::Ident(name) = base.as_ref() {
            if name == "input" {
                return Some(field.clone());
            }
        }
    }
    None
}

// ── Rule: empty-step ────────────────────────────────────────────────────────

pub struct EmptyStep;

impl LintRule for EmptyStep {
    fn name(&self) -> &str { "empty-step" }

    fn check(&self, file: &SourceFile) -> Vec<LintDiagnostic> {
        let mut out = Vec::new();
        for skill in &file.skills {
            for step in &skill.body.steps {
                if step.contexts.is_empty() && step.use_call.is_none() {
                    out.push(LintDiagnostic {
                        rule: self.name().to_string(),
                        severity: Severity::Warning,
                        message: format!(
                            "skill '{}': step '{}' has no context and no use call — it provides no instructions",
                            skill.name, step.name
                        ),
                        span: Some(step.span),
                    });
                }
            }
        }
        out
    }
}

// ── Rule: unreachable-step ──────────────────────────────────────────────────

pub struct UnreachableStep;

impl LintRule for UnreachableStep {
    fn name(&self) -> &str { "unreachable-step" }

    fn check(&self, file: &SourceFile) -> Vec<LintDiagnostic> {
        let mut out = Vec::new();
        for skill in &file.skills {
            let steps = &skill.body.steps;
            if steps.len() <= 1 { continue; }

            let mut referenced: HashSet<&str> = HashSet::new();
            for step in steps {
                if let Some(dep) = &step.requires {
                    match dep {
                        Dependency::Single(n) => { referenced.insert(n); }
                        Dependency::All(ns) | Dependency::Any(ns) => {
                            for n in ns { referenced.insert(n); }
                        }
                        Dependency::AllSteps => {
                            for s in steps { referenced.insert(&s.name); }
                        }
                    }
                }
            }

            let first = &steps[0].name;
            for step in steps {
                if step.requires.is_none()
                    && !referenced.contains(step.name.as_str())
                    && step.name != *first
                {
                    out.push(LintDiagnostic {
                        rule: self.name().to_string(),
                        severity: Severity::Warning,
                        message: format!(
                            "skill '{}': step '{}' is unreachable — nothing depends on it and it depends on nothing",
                            skill.name, step.name
                        ),
                        span: Some(step.span),
                    });
                }
            }
        }
        out
    }
}

// ── Rule: unused-lazy-context ───────────────────────────────────────────────

pub struct UnusedLazyContext;

impl LintRule for UnusedLazyContext {
    fn name(&self) -> &str { "unused-lazy-context" }

    fn check(&self, file: &SourceFile) -> Vec<LintDiagnostic> {
        let mut out = Vec::new();
        for skill in &file.skills {
            let loaded: HashSet<&str> = skill.body.steps.iter()
                .flat_map(|s| s.loads.iter().map(|l| l.as_str()))
                .collect();

            for lc in &skill.body.lazy_contexts {
                if !loaded.contains(lc.name.as_str()) {
                    out.push(LintDiagnostic {
                        rule: self.name().to_string(),
                        severity: Severity::Warning,
                        message: format!(
                            "skill '{}': lazy context '{}' is declared but never loaded by any step",
                            skill.name, lc.name
                        ),
                        span: Some(lc.span),
                    });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn parse(input: &str) -> SourceFile {
        let tokens = Lexer::new(input).tokenize().unwrap();
        Parser::new(tokens).parse().unwrap()
    }

    fn lint(input: &str) -> Vec<LintDiagnostic> {
        let file = parse(input);
        LintEngine::new().run(&file)
    }

    #[test]
    fn lint_all_same_priority_warns() {
        let diags = lint(r#"
            skill "x" {
                body {
                    context(priority: supplementary) { "A" }
                    context(priority: supplementary) { "B" }
                    context(priority: supplementary) { "C" }
                }
            }
        "#);
        assert!(diags.iter().any(|d| d.rule == "uniform-priority"),
            "expected uniform-priority warning, got: {:?}", diags.iter().map(|d| &d.rule).collect::<Vec<_>>());
    }

    #[test]
    fn lint_large_context_warns() {
        let big = "x".repeat(2500);
        let src = format!(r#"skill "x" {{ body {{ context {{ "{big}" }} }} }}"#);
        let diags = lint(&src);
        assert!(diags.iter().any(|d| d.rule == "context-too-large"),
            "expected context-too-large warning, got: {:?}", diags.iter().map(|d| &d.rule).collect::<Vec<_>>());
        let msg = &diags.iter().find(|d| d.rule == "context-too-large").unwrap().message;
        assert!(msg.contains("token"), "message should mention estimated tokens: {msg}");
    }

    #[test]
    fn lint_when_guard_on_required_field_warns() {
        let diags = lint(r#"
            skill "x" {
                input { files: string[] }
                body {
                    context(when: input.files) { "Only if files." }
                }
            }
        "#);
        assert!(diags.iter().any(|d| d.rule == "when-guard-always-true"),
            "expected when-guard-always-true warning, got: {:?}", diags.iter().map(|d| &d.rule).collect::<Vec<_>>());
    }

    #[test]
    fn lint_when_guard_on_optional_field_ok() {
        let diags = lint(r#"
            skill "x" {
                input { focus?: string }
                body {
                    context(when: input.focus) { "Optional." }
                }
            }
        "#);
        assert!(!diags.iter().any(|d| d.rule == "when-guard-always-true"),
            "optional field should not trigger: {:?}", diags.iter().map(|d| &d.rule).collect::<Vec<_>>());
    }

    #[test]
    fn lint_step_without_context_warns() {
        let diags = lint(r#"
            skill "x" {
                body {
                    context { "Base." }
                    step empty_one { emit output }
                }
            }
        "#);
        assert!(diags.iter().any(|d| d.rule == "empty-step"),
            "expected empty-step warning, got: {:?}", diags.iter().map(|d| &d.rule).collect::<Vec<_>>());
    }

    #[test]
    fn lint_unreachable_step_warns() {
        let diags = lint(r#"
            skill "x" {
                body {
                    step a { context { "A" } }
                    step b { requires a context { "B" } }
                    step c { requires b context { "C" } }
                    step d { context { "Isolated" } }
                }
            }
        "#);
        assert!(diags.iter().any(|d| d.rule == "unreachable-step" && d.message.contains("'d'")),
            "expected unreachable-step for 'd', got: {:?}", diags.iter().map(|d| (&d.rule, &d.message)).collect::<Vec<_>>());
    }

    #[test]
    fn lint_lazy_context_never_loaded_warns() {
        let diags = lint(r#"
            skill "x" {
                body {
                    lazy context "docs" (priority: supplementary) {
                        summary "API docs."
                        "Inline content."
                    }
                    step main { context { "Go." } }
                }
            }
        "#);
        assert!(diags.iter().any(|d| d.rule == "unused-lazy-context" && d.message.contains("docs")),
            "expected unused-lazy-context for 'docs', got: {:?}", diags.iter().map(|d| (&d.rule, &d.message)).collect::<Vec<_>>());
    }

    #[test]
    fn lint_clean_skill_no_warnings() {
        let diags = lint(r#"
            skill "clean" {
                input {
                    files: string[]
                    focus?: string
                }
                output {
                    summary: string
                }
                body {
                    context(priority: critical) { "You are a reviewer." }
                    context(priority: important, when: input.focus) { "Focus area." }
                    step analyze {
                        context(priority: supplementary) { "Analyze." }
                    }
                    step report {
                        requires analyze
                        emit output
                        context(priority: supplementary) { "Report findings." }
                    }
                }
            }
        "#);
        assert!(diags.is_empty(), "clean skill should have no warnings, got: {:?}",
            diags.iter().map(|d| (&d.rule, &d.message)).collect::<Vec<_>>());
    }
}
