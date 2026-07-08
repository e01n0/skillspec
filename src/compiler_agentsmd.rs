use crate::ast::*;
use crate::compiler::TargetCompiler;

/// Compiles skills to the cross-tool AGENTS.md convention read by Codex,
/// Cursor, and other coding agents. All skills in a source file are joined
/// into a single document by the build command.
pub struct AgentsMdCompiler;

impl TargetCompiler for AgentsMdCompiler {
    fn name(&self) -> &str {
        "agentsmd"
    }
    fn file_extension(&self) -> &str {
        "md"
    }

    fn compile_skill(&self, skill: &Skill, source: &SourceFile) -> String {
        let mut out = String::new();
        let ancestors = resolve_ancestry(skill, &source.skills);

        out.push_str(&format!("# {}\n\n", skill.name));

        if let Some(persona) = &skill.body.directives.persona {
            out.push_str(persona.trim());
            out.push_str("\n\n");
        }

        let mut all_contexts: Vec<&ContextBlock> = Vec::new();
        for ancestor in &ancestors {
            all_contexts.extend(ancestor.body.contexts.iter());
        }
        all_contexts.extend(skill.body.contexts.iter());
        all_contexts.retain(|c| c.applies_to("agentsmd"));
        all_contexts.sort_by(|a, b| {
            let pa = a.priority.unwrap_or(Priority::Supplementary).rank();
            let pb = b.priority.unwrap_or(Priority::Supplementary).rank();
            pb.cmp(&pa)
        });

        if !all_contexts.is_empty() {
            out.push_str("## Guidelines\n\n");
            for ctx in &all_contexts {
                match ctx.priority {
                    Some(Priority::Critical) => out.push_str("- **MUST:** "),
                    Some(Priority::Important) => out.push_str("- **SHOULD:** "),
                    Some(Priority::Optional) => out.push_str("- *(optional)* "),
                    _ => out.push_str("- "),
                }
                out.push_str(ctx.text.trim());
                out.push('\n');
            }
            out.push('\n');
        }

        if !skill.body.steps.is_empty() {
            out.push_str("## Workflow\n\n");
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

        if let Some(perms) = &skill.permissions {
            out.push_str("## Boundaries\n\n");
            if let Some((mode, patterns)) = &perms.filesystem {
                out.push_str(&format!(
                    "- filesystem: {}({})\n",
                    mode,
                    patterns.join(", ")
                ));
            }
            if let Some((mode, hosts)) = &perms.network {
                out.push_str(&format!("- network: {}({})\n", mode, hosts.join(", ")));
            }
            for secret in &perms.secrets {
                out.push_str(&format!("- secret: {}\n", secret));
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
        AgentsMdCompiler.compile_skill(&ast.skills[0], &ast)
    }

    #[test]
    fn agentsmd_basic_sections() {
        let out = compile(
            r#"
            skill "reviewer" {
                body {
                    persona { "You are a senior reviewer." }
                    context(priority: critical) { "Never approve failing builds." }
                    context { "Prefer small diffs." }
                    step analyze { context { "Analyze the code." } }
                    step report { requires analyze context { "Write the report." } }
                }
            }
        "#,
        );
        assert!(out.starts_with("# reviewer"));
        assert!(out.contains("You are a senior reviewer."));
        assert!(out.contains("## Guidelines"));
        assert!(out.contains("**MUST:** Never approve failing builds."));
        assert!(out.contains("## Workflow"));
        assert!(out.contains("1. **analyze**"));
    }

    #[test]
    fn agentsmd_critical_first() {
        let out = compile(
            r#"
            skill "x" {
                body {
                    context(priority: optional) { "Nice to have." }
                    context(priority: critical) { "Non-negotiable." }
                }
            }
        "#,
        );
        let crit = out.find("Non-negotiable").unwrap();
        let opt = out.find("Nice to have").unwrap();
        assert!(crit < opt, "critical context must come first");
    }

    #[test]
    fn agentsmd_tools_and_permissions() {
        let out = compile(
            r#"
            skill "x" {
                tools {
                    require Read
                    optional mcp("github") {
                        get_pr(repo: string) -> string
                    }
                }
                permissions {
                    filesystem: read_write("src/**")
                }
                body { context { "Work." } }
            }
        "#,
        );
        assert!(out.contains("## Tools"));
        assert!(out.contains("- Read (required)"));
        assert!(out.contains("## Boundaries"));
        assert!(out.contains("filesystem"));
    }
}
