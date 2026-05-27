use crate::ast::*;
use crate::compiler::TargetCompiler;

pub struct SystemPromptCompiler;

impl TargetCompiler for SystemPromptCompiler {
    fn name(&self) -> &str {
        "system-prompt"
    }
    fn file_extension(&self) -> &str {
        "txt"
    }

    fn compile_skill(&self, skill: &Skill, source: &SourceFile) -> String {
        let mut out = String::new();
        let ancestors = resolve_ancestry(skill, &source.skills);

        if let Some(persona) = &skill.body.directives.persona {
            out.push_str(persona.trim());
            out.push_str("\n\n");
        }

        let mut all_contexts: Vec<&ContextBlock> = Vec::new();
        for ancestor in &ancestors {
            all_contexts.extend(ancestor.body.contexts.iter());
        }
        all_contexts.extend(skill.body.contexts.iter());
        all_contexts.sort_by(|a, b| {
            let pa = a.priority.unwrap_or(Priority::Supplementary).rank();
            let pb = b.priority.unwrap_or(Priority::Supplementary).rank();
            pb.cmp(&pa)
        });

        for ctx in &all_contexts {
            if let Some(p) = ctx.priority {
                match p {
                    Priority::Critical => out.push_str("CRITICAL: "),
                    Priority::Important => out.push_str("IMPORTANT: "),
                    Priority::Optional => out.push_str("(Optional) "),
                    Priority::Supplementary => {}
                }
            }
            out.push_str(ctx.text.trim());
            out.push_str("\n\n");
        }

        for step in &skill.body.steps {
            out.push_str(&format!("{}:\n", step.name));
            for ctx in &step.contexts {
                out.push_str(ctx.text.trim());
                out.push('\n');
            }
            out.push('\n');
        }

        out.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn compile(input: &str) -> String {
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        SystemPromptCompiler.compile_skill(&ast.skills[0], &ast)
    }

    #[test]
    fn no_frontmatter() {
        let out = compile(r#"skill "x" { body { context { "You are helpful." } } }"#);
        assert!(
            !out.contains("---"),
            "system-prompt should have no YAML frontmatter"
        );
        assert!(
            !out.contains("# "),
            "system-prompt should have no markdown headers"
        );
    }

    #[test]
    fn preserves_content() {
        let out = compile(
            r#"
            skill "x" {
                body {
                    persona { "You are an expert." }
                    context(priority: important) { "Review carefully." }
                    step analyze { context { "Analyze." } }
                }
            }
        "#,
        );
        assert!(out.contains("You are an expert."));
        assert!(out.contains("Review carefully."));
        assert!(out.contains("analyze:"));
        assert!(out.contains("Analyze."));
    }
}
