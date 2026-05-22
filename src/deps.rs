use crate::ast::{Dependency, SourceFile};

pub fn emit_mermaid(ast: &SourceFile) -> String {
    let mut out = String::from("graph TD\n");
    let all_nodes: Vec<&str> = collect_all_node_names(ast);

    for skill in &ast.skills {
        for step in &skill.body.steps {
            emit_edges(&mut out, &step.name, &step.requires, &all_nodes);
        }
    }
    for pipeline in &ast.pipelines {
        for stage in &pipeline.stages {
            emit_edges(&mut out, &stage.name, &stage.requires, &all_nodes);
        }
    }
    for orch in &ast.orchestrations {
        for phase in &orch.phases {
            emit_edges(&mut out, &phase.name, &phase.requires, &all_nodes);
        }
    }
    out
}

fn collect_all_node_names(ast: &SourceFile) -> Vec<&str> {
    let mut names = Vec::new();
    for skill in &ast.skills {
        for step in &skill.body.steps {
            names.push(step.name.as_str());
        }
    }
    for pipeline in &ast.pipelines {
        for stage in &pipeline.stages {
            names.push(stage.name.as_str());
        }
    }
    for orch in &ast.orchestrations {
        for phase in &orch.phases {
            names.push(phase.name.as_str());
        }
    }
    names
}

fn emit_edges(out: &mut String, name: &str, dep: &Option<Dependency>, all_nodes: &[&str]) {
    match dep {
        None => {}
        Some(Dependency::Single(from)) => {
            out.push_str(&format!("    {} --> {}\n", from, name));
        }
        Some(Dependency::All(froms)) => {
            for from in froms {
                out.push_str(&format!("    {} --> {}\n", from, name));
            }
        }
        Some(Dependency::Any(froms)) => {
            for from in froms {
                out.push_str(&format!("    {} -.-> {}\n", from, name));
            }
        }
        Some(Dependency::AllSteps) => {
            for &node in all_nodes {
                if node != name {
                    out.push_str(&format!("    {} --> {}\n", node, name));
                }
            }
        }
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

    #[test]
    fn mermaid_linear_steps() {
        let ast = parse(r#"
            skill "x" {
                body {
                    step a { context { "a" } }
                    step b { requires a context { "b" } }
                    step c { requires b context { "c" } }
                }
            }
        "#);
        let out = emit_mermaid(&ast);
        assert!(out.contains("graph TD"));
        assert!(out.contains("a --> b"));
        assert!(out.contains("b --> c"));
    }

    #[test]
    fn mermaid_parallel_steps() {
        let ast = parse(r#"
            skill "x" {
                body {
                    step a { context { "a" } }
                    step b { context { "b" } }
                    step c { requires a & b context { "c" } }
                }
            }
        "#);
        let out = emit_mermaid(&ast);
        assert!(out.contains("a --> c"));
        assert!(out.contains("b --> c"));
        assert!(!out.contains("a --> b"), "a and b are parallel, no edge between them");
        assert!(!out.contains("b --> a"));
    }

    #[test]
    fn mermaid_pipeline_stages() {
        let ast = parse(r#"
            pipeline "ci" {
                stage lint { use linter(q: input.q) }
                stage security { use scanner(q: input.q) }
                stage review { requires lint & security use reviewer(q: input.q) }
            }
        "#);
        let out = emit_mermaid(&ast);
        assert!(out.contains("lint --> review"));
        assert!(out.contains("security --> review"));
    }

    #[test]
    fn mermaid_all_steps_dep() {
        let ast = parse(r#"
            skill "x" {
                body {
                    step a { context { "a" } }
                    step b { context { "b" } }
                    step final { requires all_steps context { "final" } }
                }
            }
        "#);
        let out = emit_mermaid(&ast);
        assert!(out.contains("a --> final"));
        assert!(out.contains("b --> final"));
        assert!(!out.contains("final --> final"), "should not self-reference");
    }

    #[test]
    fn mermaid_any_dep() {
        let ast = parse(r#"
            skill "x" {
                body {
                    step a { context { "a" } }
                    step b { context { "b" } }
                    step c { requires a | b context { "c" } }
                }
            }
        "#);
        let out = emit_mermaid(&ast);
        assert!(out.contains("a -.-> c"), "any deps should use dashed arrows");
        assert!(out.contains("b -.-> c"));
    }
}
