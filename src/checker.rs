use std::collections::{HashMap, HashSet};
use crate::ast::*;
use crate::error::SkillSpecError;
use crate::token::Span;
use crate::types::{ResolvedType, TypeRegistry};

pub struct Checker {
    registry: TypeRegistry,
    errors: Vec<SkillSpecError>,
}

impl Checker {
    pub fn new() -> Self {
        Checker {
            registry: TypeRegistry::new(),
            errors: Vec::new(),
        }
    }

    pub fn check(&mut self, file: &SourceFile) -> Result<(), Vec<SkillSpecError>> {
        // First pass: register all type definitions
        for type_def in &file.type_defs {
            self.register_type(type_def);
        }

        // Collect mixin names for reference validation
        let mixin_names: HashSet<String> = file.mixins.iter().map(|m| m.name.clone()).collect();

        // Second pass: check each skill
        for skill in &file.skills {
            self.check_skill(skill, &mixin_names);
        }

        // Check pipelines
        for pipeline in &file.pipelines {
            self.check_pipeline(pipeline);
        }

        // Check orchestrations
        for orch in &file.orchestrations {
            self.check_orchestration(orch);
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    fn register_type(&mut self, type_def: &TypeDef) {
        let fields: Vec<(String, ResolvedType, bool)> = type_def
            .fields
            .iter()
            .map(|f| {
                let resolved = self.resolve_type_expr(&f.ty);
                (f.name.clone(), resolved, f.optional)
            })
            .collect();
        self.registry.register(
            type_def.name.clone(),
            ResolvedType::Struct(type_def.name.clone(), fields),
        );
    }

    fn check_skill(&mut self, skill: &Skill, mixin_names: &HashSet<String>) {
        // Check input field types
        if let Some(input_fields) = &skill.input {
            for field in input_fields {
                self.check_type_exists(&field.ty, field.span);
            }
        }

        // Check output field types
        if let Some(output_fields) = &skill.output {
            for field in output_fields {
                self.check_type_exists(&field.ty, field.span);
            }
        }

        // Validate mixin includes
        for include_name in &skill.includes {
            if !mixin_names.contains(include_name) {
                self.errors.push(SkillSpecError::UnknownMixin {
                    name: include_name.clone(),
                    span: skill.span,
                });
            }
        }

        // Validate tool method types (MCP tools only)
        if let Some(tools_block) = &skill.tools {
            let all_tool_decls = tools_block
                .required
                .iter()
                .chain(tools_block.optional.iter());
            for tool_decl in all_tool_decls {
                if matches!(tool_decl.kind, ToolKind::Mcp(_)) {
                    for method in &tool_decl.methods {
                        for (_, param_ty, _) in &method.params {
                            self.check_type_exists(param_ty, skill.span);
                        }
                        self.check_type_exists(&method.return_type, skill.span);
                    }
                }
            }
        }

        // Check the body steps (with lazy context awareness)
        self.check_body(&skill.body);
    }

    fn check_body(&mut self, body: &Body) {
        // Collect lazy context names declared in this body
        let lazy_names: HashSet<String> = body
            .lazy_contexts
            .iter()
            .map(|lc| lc.name.clone())
            .collect();

        // Check for duplicate lazy context names
        let mut seen_lazy: HashMap<String, Span> = HashMap::new();
        for lc in &body.lazy_contexts {
            if let Some(_existing) = seen_lazy.get(&lc.name) {
                self.errors.push(SkillSpecError::DuplicateField {
                    name: lc.name.clone(),
                    span: lc.span,
                });
            } else {
                seen_lazy.insert(lc.name.clone(), lc.span);
            }
        }

        // Validate load references in steps
        for step in &body.steps {
            for load_name in &step.loads {
                if !lazy_names.contains(load_name) {
                    self.errors.push(SkillSpecError::UnknownLazyContext {
                        name: load_name.clone(),
                        span: step.span,
                    });
                }
            }
        }

        // Check the steps (DAG validation etc.)
        self.check_steps(body);
    }

    fn check_steps(&mut self, body: &Body) {
        let steps = &body.steps;

        // Build a set of all step names to detect duplicates and validate requires
        let mut seen_names: HashMap<String, Span> = HashMap::new();
        for step in steps {
            if let Some(existing_span) = seen_names.get(&step.name) {
                // Duplicate step name — report error using the second occurrence's span
                self.errors.push(SkillSpecError::DuplicateField {
                    name: step.name.clone(),
                    span: step.span,
                });
                let _ = existing_span; // already recorded
            } else {
                seen_names.insert(step.name.clone(), step.span);
            }
        }

        let all_names: HashSet<String> = steps.iter().map(|s| s.name.clone()).collect();

        // Validate requires references
        for step in steps {
            if let Some(dep) = &step.requires {
                let referenced = dep_names(dep);
                for name in referenced {
                    if !all_names.contains(&name) {
                        self.errors.push(SkillSpecError::UnknownStep {
                            name: name.clone(),
                            span: step.span,
                        });
                    }
                }
            }
        }

        // Check for dependency cycles
        self.check_cycles(steps);

        // Check for multiple unconditional emit statements
        let emit_steps: Vec<&Step> = steps.iter().filter(|s| s.emit).collect();
        if emit_steps.len() >= 2 {
            // Multiple emits are only OK if ALL of them have `when` guards
            let unconditional_emits: Vec<&&Step> =
                emit_steps.iter().filter(|s| s.when.is_none()).collect();
            if unconditional_emits.len() >= 2 {
                // Report error on the second unconditional emit
                self.errors.push(SkillSpecError::MultipleEmit {
                    span: unconditional_emits[1].span,
                });
            }
        }
    }

    fn check_pipeline(&mut self, pipeline: &Pipeline) {
        // Check input/output field types
        if let Some(input_fields) = &pipeline.input {
            for field in input_fields {
                self.check_type_exists(&field.ty, field.span);
            }
        }
        if let Some(output_fields) = &pipeline.output {
            for field in output_fields {
                self.check_type_exists(&field.ty, field.span);
            }
        }

        let stages = &pipeline.stages;

        // Duplicate stage names
        let mut seen: HashMap<String, Span> = HashMap::new();
        for stage in stages {
            if seen.contains_key(&stage.name) {
                self.errors.push(SkillSpecError::DuplicateField {
                    name: stage.name.clone(),
                    span: stage.span,
                });
            } else {
                seen.insert(stage.name.clone(), stage.span);
            }
        }

        let all_names: HashSet<String> = stages.iter().map(|s| s.name.clone()).collect();

        // Unknown requires references
        for stage in stages {
            if let Some(dep) = &stage.requires {
                for name in dep_names(dep) {
                    if !all_names.contains(&name) {
                        self.errors.push(SkillSpecError::UnknownStep {
                            name: name.clone(),
                            span: stage.span,
                        });
                    }
                }
            }
        }

        // Dependency cycle check for stages
        let named: Vec<(String, Option<Dependency>, Span)> = stages
            .iter()
            .map(|s| (s.name.clone(), s.requires.clone(), s.span))
            .collect();
        self.check_cycles_named(&named);
    }

    fn check_orchestration(&mut self, orch: &Orchestration) {
        // Check input/output field types
        if let Some(input_fields) = &orch.input {
            for field in input_fields {
                self.check_type_exists(&field.ty, field.span);
            }
        }
        if let Some(output_fields) = &orch.output {
            for field in output_fields {
                self.check_type_exists(&field.ty, field.span);
            }
        }

        // Collect declared agent names
        let agent_names: HashSet<String> = orch.agents.iter().map(|a| a.name.clone()).collect();

        let phases = &orch.phases;

        // Duplicate phase names
        let mut seen: HashMap<String, Span> = HashMap::new();
        for phase in phases {
            if seen.contains_key(&phase.name) {
                self.errors.push(SkillSpecError::DuplicateField {
                    name: phase.name.clone(),
                    span: phase.span,
                });
            } else {
                seen.insert(phase.name.clone(), phase.span);
            }
        }

        let all_phase_names: HashSet<String> = phases.iter().map(|p| p.name.clone()).collect();

        // Unknown requires references + unknown agent references
        for phase in phases {
            if let Some(dep) = &phase.requires {
                for name in dep_names(dep) {
                    if !all_phase_names.contains(&name) {
                        self.errors.push(SkillSpecError::UnknownStep {
                            name: name.clone(),
                            span: phase.span,
                        });
                    }
                }
            }

            for action in &phase.actions {
                if !agent_names.contains(&action.agent_name) {
                    self.errors.push(SkillSpecError::UnknownAgent {
                        name: action.agent_name.clone(),
                        span: phase.span,
                    });
                }
            }
        }

        // Dependency cycle check for phases
        let named: Vec<(String, Option<Dependency>, Span)> = phases
            .iter()
            .map(|p| (p.name.clone(), p.requires.clone(), p.span))
            .collect();
        self.check_cycles_named(&named);
    }

    /// Generic DAG cycle check for any sequence of (name, optional requires, span).
    /// Reuses dfs_cycle logic; reports DependencyCycle on the first cycle found.
    fn check_cycles_named(&mut self, nodes: &[(String, Option<Dependency>, Span)]) {
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for (name, dep, _) in nodes {
            let deps = dep
                .as_ref()
                .map(|d| dep_names(d))
                .unwrap_or_default();
            adj.insert(name.clone(), deps);
        }

        let mut visited: HashSet<String> = HashSet::new();
        let mut in_stack: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = Vec::new();

        for (name, _, _) in nodes {
            if !visited.contains(name) {
                if let Some(cycle) =
                    Self::dfs_cycle(name, &adj, &mut visited, &mut in_stack, &mut stack)
                {
                    self.errors.push(SkillSpecError::DependencyCycle {
                        cycle: cycle.join(" -> "),
                    });
                    return;
                }
            }
        }
    }

    fn check_cycles(&mut self, steps: &[Step]) {
        // Build adjacency map: step name -> list of required step names
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for step in steps {
            let deps = step
                .requires
                .as_ref()
                .map(|d| dep_names(d))
                .unwrap_or_default();
            adj.insert(step.name.clone(), deps);
        }

        let mut visited: HashSet<String> = HashSet::new();
        let mut in_stack: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = Vec::new();

        let names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();

        for name in &names {
            if !visited.contains(name) {
                if let Some(cycle) = Self::dfs_cycle(name, &adj, &mut visited, &mut in_stack, &mut stack) {
                    self.errors.push(SkillSpecError::DependencyCycle {
                        cycle: cycle.join(" -> "),
                    });
                    // Only report the first cycle found
                    return;
                }
            }
        }
    }

    fn dfs_cycle(
        node: &str,
        adj: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
        in_stack: &mut HashSet<String>,
        stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        visited.insert(node.to_string());
        in_stack.insert(node.to_string());
        stack.push(node.to_string());

        if let Some(neighbors) = adj.get(node) {
            for neighbor in neighbors {
                if in_stack.contains(neighbor) {
                    // Found a cycle — extract the cycle path from the stack
                    let cycle_start = stack.iter().position(|n| n == neighbor).unwrap_or(0);
                    let mut cycle: Vec<String> = stack[cycle_start..].to_vec();
                    cycle.push(neighbor.to_string());
                    stack.pop();
                    in_stack.remove(node);
                    return Some(cycle);
                }
                if !visited.contains(neighbor) {
                    if let Some(cycle) = Self::dfs_cycle(neighbor, adj, visited, in_stack, stack) {
                        stack.pop();
                        in_stack.remove(node);
                        return Some(cycle);
                    }
                }
            }
        }

        stack.pop();
        in_stack.remove(node);
        None
    }

    fn check_type_exists(&mut self, ty: &TypeExpr, span: Span) {
        match ty {
            TypeExpr::String | TypeExpr::Int | TypeExpr::Float | TypeExpr::Bool => {
                // Primitive types always exist
            }
            TypeExpr::Enum(_) => {
                // Inline enum always valid
            }
            TypeExpr::Array(inner) => {
                self.check_type_exists(inner, span);
            }
            TypeExpr::Map(k, v) => {
                self.check_type_exists(k, span);
                self.check_type_exists(v, span);
            }
            TypeExpr::Named(name) => {
                // "void" is a built-in pseudo-type used as a return type in tool methods
                if name == "void" {
                    return;
                }
                if self.registry.resolve(name).is_none() {
                    self.errors.push(SkillSpecError::UnknownType {
                        name: name.clone(),
                        span,
                    });
                }
            }
        }
    }

    fn resolve_type_expr(&self, ty: &TypeExpr) -> ResolvedType {
        match ty {
            TypeExpr::String => ResolvedType::String,
            TypeExpr::Int => ResolvedType::Int,
            TypeExpr::Float => ResolvedType::Float,
            TypeExpr::Bool => ResolvedType::Bool,
            TypeExpr::Array(inner) => ResolvedType::Array(Box::new(self.resolve_type_expr(inner))),
            TypeExpr::Map(k, v) => ResolvedType::Map(
                Box::new(self.resolve_type_expr(k)),
                Box::new(self.resolve_type_expr(v)),
            ),
            TypeExpr::Enum(variants) => ResolvedType::Enum(variants.clone()),
            TypeExpr::Named(name) => self
                .registry
                .resolve(name)
                .cloned()
                .unwrap_or(ResolvedType::Unknown),
        }
    }

}

/// Extract all step name references from a Dependency.
fn dep_names(dep: &Dependency) -> Vec<String> {
    match dep {
        Dependency::Single(name) => vec![name.clone()],
        Dependency::All(names) | Dependency::Any(names) => names.clone(),
        Dependency::AllSteps => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn check(input: &str) -> std::result::Result<(), Vec<SkillSpecError>> {
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let mut checker = Checker::new();
        checker.check(&ast)
    }

    #[test]
    fn valid_skill_passes() {
        let result = check(r#"
            type Finding {
                file: string
                severity: string
            }
            skill "review" {
                input { files: string[] }
                output { findings: Finding[] }
                body {
                    context { "Review code." }
                    step analyze { context { "Analyze." } }
                    step report {
                        requires analyze
                        emit output
                        context { "Report." }
                    }
                }
            }
        "#);
        assert!(result.is_ok());
    }

    #[test]
    fn unknown_type_errors() {
        let result = check(r#"
            skill "bad" {
                input { files: UnknownType[] }
                body { context { "x" } }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, SkillSpecError::UnknownType { .. })));
    }

    #[test]
    fn dependency_cycle_detected() {
        let result = check(r#"
            skill "cycle" {
                body {
                    step a { requires b context { "a" } }
                    step b { requires a context { "b" } }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, SkillSpecError::DependencyCycle { .. })));
    }

    #[test]
    fn unknown_step_in_requires() {
        let result = check(r#"
            skill "bad" {
                body {
                    step a { requires nonexistent context { "a" } }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, SkillSpecError::UnknownStep { .. })));
    }

    #[test]
    fn multiple_emit_errors() {
        let result = check(r#"
            skill "bad" {
                body {
                    step a { emit output context { "a" } }
                    step b { emit output context { "b" } }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, SkillSpecError::MultipleEmit { .. })));
    }

    #[test]
    fn duplicate_step_names() {
        let result = check(r#"
            skill "bad" {
                body {
                    step a { context { "first" } }
                    step a { context { "second" } }
                }
            }
        "#);
        assert!(result.is_err());
    }

    #[test]
    fn lazy_context_load_valid() {
        let result = check(r#"
            skill "x" {
                body {
                    lazy context "docs" (priority: 50) {
                        summary "API docs."
                        ref "./api.md"
                    }
                    step main {
                        load "docs"
                        context { "Use docs." }
                    }
                }
            }
        "#);
        assert!(result.is_ok());
    }

    #[test]
    fn lazy_context_load_invalid_ref() {
        let result = check(r#"
            skill "x" {
                body {
                    step main {
                        load "nonexistent"
                        context { "oops." }
                    }
                }
            }
        "#);
        assert!(result.is_err());
    }

    #[test]
    fn pipeline_stage_cycle() {
        let result = check(r#"
            pipeline "bad" {
                stage a { requires b use x(q: input.q) }
                stage b { requires a use y(q: input.q) }
            }
        "#);
        assert!(result.is_err());
    }

    #[test]
    fn valid_pipeline() {
        let result = check(r#"
            pipeline "good" {
                input { repo: string }
                stage lint { use linter(repo: input.repo) }
                stage review {
                    requires lint
                    use reviewer(results: lint.result)
                }
            }
        "#);
        assert!(result.is_ok());
    }

    #[test]
    fn mixin_include_valid() {
        let result = check(r#"
            mixin logging {
                step log { context { "log." } }
            }
            skill "x" {
                include logging
                body { context { "ok" } }
            }
        "#);
        assert!(result.is_ok());
    }

    #[test]
    fn mixin_include_unknown() {
        let result = check(r#"
            skill "x" {
                include nonexistent_mixin
                body { context { "ok" } }
            }
        "#);
        assert!(result.is_err());
    }
}
