use crate::ast::*;
use crate::compiler::TargetCompiler;

pub struct CursorCompiler;

impl TargetCompiler for CursorCompiler {
    fn name(&self) -> &str { "cursor" }
    fn file_extension(&self) -> &str { "cursorrules" }

    fn compile_skill(&self, skill: &Skill, source: &SourceFile) -> String {
        let mut out = String::new();
        let ancestors = resolve_ancestry(skill, &source.skills);

        out.push_str(&format!("# {}\n\n", skill.name));

        if let Some(persona) = &skill.body.directives.persona {
            out.push_str("## Role\n\n");
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
            out.push_str("## Rules\n\n");
            for ctx in &all_contexts {
                out.push_str("- ");
                out.push_str(ctx.text.trim());
                out.push('\n');
            }
            out.push('\n');
        }

        if let Some(tools) = &skill.tools {
            out.push_str("## Tools\n\n");
            for tool in &tools.required {
                out.push_str(&format!("- {} (required)\n", tool.name));
            }
            for tool in &tools.optional {
                out.push_str(&format!("- {} (optional)\n", tool.name));
            }
            out.push('\n');
        }

        if !skill.body.steps.is_empty() {
            out.push_str("## Steps\n\n");
            for (i, step) in skill.body.steps.iter().enumerate() {
                out.push_str(&format!("{}. **{}**", i + 1, step.name));
                if !step.contexts.is_empty() {
                    out.push_str(": ");
                    out.push_str(step.contexts[0].text.trim());
                }
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
        CursorCompiler.compile_skill(&ast.skills[0], &ast)
    }

    #[test]
    fn cursor_rules_format() {
        let out = compile(r#"
            skill "reviewer" {
                body {
                    context(priority: 90) { "Be thorough." }
                    step analyze { context { "Analyze code." } }
                    step report { requires analyze context { "Write report." } }
                }
            }
        "#);
        assert!(out.contains("# reviewer"));
        assert!(out.contains("## Rules"));
        assert!(out.contains("## Steps"));
    }

    #[test]
    fn cursor_includes_tools() {
        let out = compile(r#"
            skill "x" {
                tools {
                    require Bash
                    require Read
                    optional mcp("github") {
                        get_pr(repo: string) -> string
                    }
                }
                body { context { "Work." } }
            }
        "#);
        assert!(out.contains("## Tools"));
        assert!(out.contains("Bash"));
        assert!(out.contains("github"));
    }
}
