use crate::ast::*;

/// Approximate chars-per-token ratio (common heuristic for English text).
const CHARS_PER_TOKEN: f64 = 4.0;

pub struct BudgetReport {
    pub skill_name: String,
    pub eager_context_tokens: usize,
    pub lazy_summary_tokens: usize,
    pub lazy_body_tokens: usize,
    pub step_count: usize,
    pub step_tokens: usize,
    pub directive_tokens: usize,
}

impl BudgetReport {
    pub fn total_eager(&self) -> usize {
        self.eager_context_tokens + self.lazy_summary_tokens + self.directive_tokens
    }

    pub fn total_potential(&self) -> usize {
        self.total_eager() + self.lazy_body_tokens + self.step_tokens
    }

    pub fn display(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Skill: {}\n", self.skill_name));
        out.push_str(&format!(
            "  Eager context:   ~{} tokens\n",
            self.eager_context_tokens
        ));
        out.push_str(&format!(
            "  Lazy summaries:  ~{} tokens\n",
            self.lazy_summary_tokens
        ));
        out.push_str(&format!(
            "  Lazy bodies:     ~{} tokens (on demand)\n",
            self.lazy_body_tokens
        ));
        out.push_str(&format!(
            "  Directives:      ~{} tokens\n",
            self.directive_tokens
        ));
        out.push_str(&format!(
            "  Steps ({}):      ~{} tokens\n",
            self.step_count, self.step_tokens
        ));
        out.push_str(&format!(
            "  Total eager:     ~{} tokens\n",
            self.total_eager()
        ));
        out.push_str(&format!(
            "  Total potential: ~{} tokens\n",
            self.total_potential()
        ));
        out
    }
}

fn chars_to_tokens(chars: usize) -> usize {
    ((chars as f64) / CHARS_PER_TOKEN).ceil() as usize
}

fn context_block_chars(ctx: &ContextBlock) -> usize {
    ctx.text.len()
}

fn lazy_content_chars(lc: &LazyContext) -> usize {
    match &lc.content {
        LazyContent::Inline(text) => text.len(),
        LazyContent::Ref(_path) => {
            // We can't read the file at analysis time — estimate 500 chars
            500
        }
        LazyContent::Index(sections) => {
            // Estimate: each section ref ~500 chars
            sections.len() * 500
        }
    }
}

fn directive_chars(d: &PromptDirectives) -> usize {
    let mut total = 0;
    if let Some(persona) = &d.persona {
        total += persona.len();
    }
    if let Some(reasoning) = &d.reasoning {
        total += reasoning.len() + 10; // "reasoning " prefix
    }
    if let Some(sampling) = &d.sampling {
        total += 30; // sampling block is small
        if sampling.temperature.is_some() {
            total += 15;
        }
        if sampling.top_p.is_some() {
            total += 10;
        }
    }
    if let Some(fmt) = &d.format {
        total += fmt.style.len() + fmt.structure.len() + 20;
    }
    for reinf in &d.reinforcements {
        total += reinf.text.len() + 20;
    }
    for ex in &d.examples {
        total += ex.input.len() + ex.output.len() + ex.name.len() + 20;
        if let Some(note) = &ex.note {
            total += note.len();
        }
    }
    total
}

pub fn estimate_skill_budget(skill: &Skill) -> BudgetReport {
    // Eager context: all context blocks in body
    let eager_chars: usize = skill.body.contexts.iter().map(context_block_chars).sum();

    // Lazy summaries
    let lazy_summary_chars: usize = skill
        .body
        .lazy_contexts
        .iter()
        .map(|lc| lc.summary.len())
        .sum();

    // Lazy bodies
    let lazy_body_chars: usize = skill
        .body
        .lazy_contexts
        .iter()
        .map(lazy_content_chars)
        .sum();

    // Steps: context blocks within steps
    let step_chars: usize = skill
        .body
        .steps
        .iter()
        .flat_map(|s| s.contexts.iter())
        .map(context_block_chars)
        .sum();

    // Directives
    let dir_chars = directive_chars(&skill.body.directives);

    BudgetReport {
        skill_name: skill.name.clone(),
        eager_context_tokens: chars_to_tokens(eager_chars),
        lazy_summary_tokens: chars_to_tokens(lazy_summary_chars),
        lazy_body_tokens: chars_to_tokens(lazy_body_chars),
        step_count: skill.body.steps.len(),
        step_tokens: chars_to_tokens(step_chars),
        directive_tokens: chars_to_tokens(dir_chars),
    }
}

pub fn estimate_budget(file: &SourceFile) -> String {
    let mut out = String::new();
    for skill in &file.skills {
        let report = estimate_skill_budget(skill);
        out.push_str(&report.display());
        out.push('\n');
    }
    out
}

// ── Budget-aware trimming ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TrimmedContext {
    pub priority: Option<Priority>,
    pub snippet: String,
    pub estimated_tokens: usize,
}

pub fn trim_to_budget(contexts: &mut Vec<ContextBlock>, budget: usize) -> Vec<TrimmedContext> {
    let mut trimmed = Vec::new();

    fn total_tokens(ctxs: &[ContextBlock]) -> usize {
        ctxs.iter().map(|c| chars_to_tokens(c.text.len())).sum()
    }

    while total_tokens(contexts) > budget && !contexts.is_empty() {
        let lowest_idx = contexts
            .iter()
            .enumerate()
            .filter(|(_, c)| c.priority != Some(Priority::Critical))
            .min_by_key(|(_, c)| c.priority.unwrap_or(Priority::Supplementary).rank())
            .map(|(i, _)| i);

        let Some(lowest_idx) = lowest_idx else { break };

        let removed = contexts.remove(lowest_idx);
        let snippet = if removed.text.len() > 60 {
            format!("{}...", &removed.text[..60])
        } else {
            removed.text.clone()
        };
        trimmed.push(TrimmedContext {
            priority: removed.priority,
            snippet,
            estimated_tokens: chars_to_tokens(removed.text.len()),
        });
    }

    trimmed
}

pub fn estimate_context_tokens(contexts: &[ContextBlock]) -> usize {
    contexts.iter().map(|c| chars_to_tokens(c.text.len())).sum()
}

/// Convenience: parse source text and return budget report.
pub fn budget_from_source(source: &str) -> Result<String, String> {
    let tokens = crate::lexer::Lexer::new(source)
        .tokenize()
        .map_err(|e| format!("Lex error: {}", e))?;
    let ast = crate::parser::Parser::new(tokens)
        .parse()
        .map_err(|e| format!("Parse error: {}", e))?;
    Ok(estimate_budget(&ast))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_estimation_minimal() {
        let source = r#"skill "hello" { context { "Greet the user warmly." } }"#;
        let report = budget_from_source(source).unwrap();
        assert!(report.contains("Eager context"));
        assert!(report.contains("tokens"));
        assert!(report.contains("hello"));
    }

    #[test]
    fn budget_with_lazy_contexts() {
        let source = r#"
            skill "x" {
                body {
                    lazy context "docs" (priority: supplementary) {
                        summary "API reference documentation."
                        ref "./api.md"
                    }
                    context { "Use the docs." }
                    step main {
                        context { "Do the thing." }
                    }
                }
            }
        "#;
        let report = budget_from_source(source).unwrap();
        assert!(report.contains("Lazy summaries"));
        assert!(report.contains("Lazy bodies"));
        assert!(report.contains("on demand"));
    }

    fn make_context(text: &str, priority: Option<Priority>) -> ContextBlock {
        ContextBlock {
            priority,
            when: None,
            decay: None,
            until: None,
            text: text.to_string(),
            span: crate::token::Span {
                start: 0,
                end: 0,
                line: 0,
                col: 0,
            },
        }
    }

    #[test]
    fn trim_drops_lowest_priority_first() {
        let mut contexts = vec![
            make_context(&"h".repeat(80), Some(Priority::Critical)), // 20 tokens
            make_context(&"m".repeat(800), Some(Priority::Supplementary)), // 200 tokens
            make_context(&"l".repeat(800), Some(Priority::Optional)), // 200 tokens
        ];
        let trimmed = trim_to_budget(&mut contexts, 250);
        assert_eq!(contexts.len(), 2, "should keep 2 contexts");
        assert!(
            contexts
                .iter()
                .all(|c| c.priority != Some(Priority::Optional)),
            "optional should be dropped"
        );
        assert_eq!(trimmed.len(), 1);
        assert_eq!(trimmed[0].priority, Some(Priority::Optional));
    }

    #[test]
    fn trim_drops_multiple_until_budget_met() {
        let mut contexts = vec![
            make_context(&"a".repeat(400), Some(Priority::Critical)), // 100 tokens
            make_context(&"b".repeat(400), Some(Priority::Important)), // 100 tokens
            make_context(&"c".repeat(400), Some(Priority::Supplementary)), // 100 tokens
            make_context(&"d".repeat(400), Some(Priority::Optional)), // 100 tokens
        ];
        let trimmed = trim_to_budget(&mut contexts, 100);
        assert!(estimate_context_tokens(&contexts) <= 100);
        assert!(
            trimmed.len() >= 2,
            "should drop at least 2 to reach 100 tokens (critical is protected)"
        );
    }

    #[test]
    fn trim_reports_what_was_dropped() {
        let mut contexts = vec![
            make_context(&"keep".repeat(20), Some(Priority::Important)),
            make_context(&"drop-me", Some(Priority::Optional)),
        ];
        let trimmed = trim_to_budget(&mut contexts, 10);
        assert!(!trimmed.is_empty());
        assert!(trimmed[0].snippet.contains("drop-me"));
        assert!(trimmed[0].estimated_tokens > 0);
    }

    #[test]
    fn trim_under_budget_no_changes() {
        let mut contexts = vec![make_context("short", Some(Priority::Important))];
        let trimmed = trim_to_budget(&mut contexts, 1000);
        assert!(trimmed.is_empty());
        assert_eq!(contexts.len(), 1);
    }

    #[test]
    fn trim_never_drops_critical() {
        let mut contexts = vec![
            make_context(&"a".repeat(800), Some(Priority::Critical)), // 200 tokens
            make_context(&"b".repeat(800), Some(Priority::Critical)), // 200 tokens
        ];
        let trimmed = trim_to_budget(&mut contexts, 50);
        assert_eq!(
            contexts.len(),
            2,
            "critical contexts should never be trimmed"
        );
        assert!(trimmed.is_empty());
    }

    #[test]
    fn budget_full_brainstorming() {
        let source = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/brainstorming.agent"
        ))
        .unwrap();
        let report = budget_from_source(&source).unwrap();
        assert!(report.contains("brainstorming"));
        assert!(report.contains("Steps (4)"));
        assert!(report.contains("Total potential"));
    }
}
