use crate::ast::*;
use crate::compiler::TargetCompiler;

pub struct ClineRulesCompiler;

impl TargetCompiler for ClineRulesCompiler {
    fn name(&self) -> &str { "clinerules" }
    fn file_extension(&self) -> &str { "clinerules" }

    fn compile_skill(&self, skill: &Skill, source: &SourceFile) -> String {
        let mut out = String::new();
        let ancestors = resolve_ancestry(skill, &source.skills);

        if let Some(persona) = &skill.body.directives.persona {
            out.push_str("# Role\n");
            out.push_str(persona.trim());
            out.push_str("\n\n");
        }

        let mut all_contexts: Vec<&ContextBlock> = Vec::new();
        for ancestor in &ancestors {
            all_contexts.extend(ancestor.body.contexts.iter());
        }
        all_contexts.extend(skill.body.contexts.iter());
        all_contexts.sort_by(|a, b| {
            b.priority.unwrap_or(0).cmp(&a.priority.unwrap_or(0))
        });

        if !all_contexts.is_empty() {
            out.push_str("# Instructions\n");
            for ctx in &all_contexts {
                out.push_str(ctx.text.trim());
                out.push_str("\n\n");
            }
        }

        if !skill.body.steps.is_empty() {
            out.push_str("# Workflow\n");
            for step in &skill.body.steps {
                out.push_str(&format!("## {}\n", step.name));
                for ctx in &step.contexts {
                    out.push_str(ctx.text.trim());
                    out.push('\n');
                }
                out.push('\n');
            }
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
        ClineRulesCompiler.compile_skill(&ast.skills[0], &ast)
    }

    #[test]
    fn clinerules_format() {
        let out = compile(r#"
            skill "x" {
                body {
                    context { "Be helpful." }
                    step analyze { context { "Analyze." } }
                }
            }
        "#);
        assert!(out.contains("# Instructions"));
        assert!(out.contains("# Workflow"));
        assert!(out.contains("## analyze"));
    }
}
