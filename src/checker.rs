use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use crate::ast::*;
use crate::error::SkillSpecError;
use crate::resolve;
use crate::token::Span;
use crate::types::{ResolvedType, TypeRegistry};

pub struct Checker {
    registry: TypeRegistry,
    errors: Vec<SkillSpecError>,
    base_dir: Option<PathBuf>,
    resolved_files: HashSet<PathBuf>,
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

impl Checker {
    pub fn new() -> Self {
        Checker {
            registry: TypeRegistry::new(),
            errors: Vec::new(),
            base_dir: None,
            resolved_files: HashSet::new(),
        }
    }

    pub fn with_base_dir(base_dir: PathBuf) -> Self {
        Checker {
            registry: TypeRegistry::new(),
            errors: Vec::new(),
            base_dir: Some(base_dir),
            resolved_files: HashSet::new(),
        }
    }

    pub fn check(&mut self, file: &SourceFile) -> Result<(), Vec<SkillSpecError>> {
        // Resolve imports first — register imported types before local types
        self.resolve_imports(file);

        // Register all local type definitions
        for type_def in &file.type_defs {
            self.register_type(type_def);
        }

        // Collect mixin names for reference validation
        let mixin_names: HashSet<String> = file.mixins.iter().map(|m| m.name.clone()).collect();

        // Collect skill names for extends validation
        let skill_names: HashSet<String> = file.skills.iter().map(|s| s.name.clone()).collect();

        // Check for shadowed imports
        let type_names: HashSet<String> = file.type_defs.iter().map(|t| t.name.clone()).collect();
        for import in &file.imports {
            for symbol in &import.symbols {
                if type_names.contains(symbol) {
                    self.errors.push(SkillSpecError::ShadowedImport {
                        name: symbol.clone(),
                        span: import.span,
                    });
                }
            }
        }

        // Build skill input signatures for use-call validation.
        // Register under both the declared name and the underscore-normalised
        // form so `use target_skill(...)` matches `skill "target-skill"`.
        let mut skill_sigs: HashMap<String, Vec<Field>> = HashMap::new();
        for s in &file.skills {
            let fields = s.input.clone().unwrap_or_default();
            let normalised = s.name.replace('-', "_");
            skill_sigs.insert(s.name.clone(), fields.clone());
            if normalised != s.name {
                skill_sigs.insert(normalised, fields);
            }
        }

        // Second pass: check each skill (with access to all skills/mixins for inheritance resolution)
        for skill in &file.skills {
            self.check_skill(skill, &mixin_names, &skill_names, &file.skills, &file.mixins);
            self.check_use_calls(skill, &skill_sigs);
        }

        // Check for extends cycles across all skills
        self.check_extends_cycles(&file.skills);

        // Check pipeline stage use calls
        for pipeline in &file.pipelines {
            for stage in &pipeline.stages {
                self.check_single_use_call(&stage.use_call, &skill_sigs);
            }
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

    fn resolve_imports(&mut self, file: &SourceFile) {
        let base_dir = match &self.base_dir {
            Some(dir) => dir.clone(),
            None => return,
        };

        for import in &file.imports {
            let resolved_path = match resolve::resolve_import_path(&import.path, &base_dir) {
                Some(p) => p,
                None => {
                    self.errors.push(SkillSpecError::UnresolvedImport {
                        path: import.path.clone(),
                        span: import.span,
                    });
                    continue;
                }
            };

            let canonical = match resolved_path.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    self.errors.push(SkillSpecError::UnresolvedImport {
                        path: import.path.clone(),
                        span: import.span,
                    });
                    continue;
                }
            };

            if self.resolved_files.contains(&canonical) {
                continue;
            }
            self.resolved_files.insert(canonical.clone());

            let imported_ast = match resolve::parse_file(&canonical) {
                Ok(ast) => ast,
                Err(e) => {
                    self.errors.push(SkillSpecError::ImportParseError {
                        path: import.path.clone(),
                        message: format!("{}", e),
                        span: import.span,
                    });
                    continue;
                }
            };

            // Recursively resolve the imported file's own imports
            let prev_base = self.base_dir.take();
            self.base_dir = canonical.parent().map(|p| p.to_path_buf());
            self.resolve_imports(&imported_ast);
            self.base_dir = prev_base;

            // Collect type names available in the imported file
            let available: HashSet<&str> = imported_ast
                .type_defs
                .iter()
                .map(|t| t.name.as_str())
                .collect();

            for symbol in &import.symbols {
                if available.contains(symbol.as_str()) {
                    if let Some(td) = imported_ast.type_defs.iter().find(|t| t.name == *symbol) {
                        self.register_type(td);
                    }
                } else {
                    self.errors.push(SkillSpecError::ImportSymbolNotFound {
                        symbol: symbol.clone(),
                        path: import.path.clone(),
                        span: import.span,
                    });
                }
            }
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

    fn check_skill(&mut self, skill: &Skill, mixin_names: &HashSet<String>, skill_names: &HashSet<String>, all_skills: &[Skill], all_mixins: &[Mixin]) {
        // Validate extends references an existing skill
        if let Some(base_name) = &skill.extends
            && !skill_names.contains(base_name) {
                self.errors.push(SkillSpecError::UnresolvedExtends {
                    name: base_name.clone(),
                    span: skill.span,
                });
            }

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

        // Resolve inherited members for body validation
        let mut inherited_steps: HashSet<String> = HashSet::new();
        let mut inherited_lazy: HashSet<String> = HashSet::new();

        let ancestors = Self::resolve_skill_ancestry(skill, all_skills);
        for ancestor in &ancestors {
            for step in &ancestor.body.steps {
                inherited_steps.insert(step.name.clone());
            }
            for lc in &ancestor.body.lazy_contexts {
                inherited_lazy.insert(lc.name.clone());
            }
        }

        let child_step_names: HashSet<&str> = skill.body.steps.iter()
            .map(|s| s.name.as_str()).collect();
        let mut mixin_step_sources: HashMap<String, Vec<String>> = HashMap::new();
        for include_name in &skill.includes {
            if let Some(mixin) = all_mixins.iter().find(|m| &m.name == include_name) {
                for step in &mixin.steps {
                    inherited_steps.insert(step.name.clone());
                    mixin_step_sources.entry(step.name.clone())
                        .or_default()
                        .push(mixin.name.clone());
                }
            }
        }

        for (step_name, sources) in &mixin_step_sources {
            if sources.len() > 1 && !child_step_names.contains(step_name.as_str()) {
                self.errors.push(SkillSpecError::DuplicateField {
                    name: step_name.clone(),
                    span: skill.span,
                });
            }
        }

        // Check for multiple unconditional emit across extends chain
        let ancestor_unconditional_emits: Vec<&Step> = ancestors.iter()
            .flat_map(|a| a.body.steps.iter())
            .filter(|s| s.emit && s.when.is_none())
            .filter(|s| !child_step_names.contains(s.name.as_str()))
            .collect();
        let child_unconditional_emits: Vec<&Step> = skill.body.steps.iter()
            .filter(|s| s.emit && s.when.is_none())
            .collect();
        let total_unconditional = ancestor_unconditional_emits.len() + child_unconditional_emits.len();
        if total_unconditional >= 2 {
            let span = child_unconditional_emits.last()
                .map(|s| s.span)
                .unwrap_or(skill.span);
            self.errors.push(SkillSpecError::MultipleEmit { span });
        }

        self.check_body(&skill.body, &inherited_steps, &inherited_lazy);
        self.check_tests(skill);
    }

    fn check_body(&mut self, body: &Body, inherited_steps: &HashSet<String>, inherited_lazy: &HashSet<String>) {
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

        // Validate lazy context ref paths point to files that exist
        if let Some(base) = &self.base_dir {
            for lc in &body.lazy_contexts {
                let refs_to_check: Vec<&str> = match &lc.content {
                    crate::ast::LazyContent::Ref(path) => vec![path.as_str()],
                    crate::ast::LazyContent::Index(sections) => {
                        sections.iter().map(|s| s.ref_path.as_str()).collect()
                    }
                    crate::ast::LazyContent::Inline(_) => vec![],
                };
                for ref_path in refs_to_check {
                    let resolved = base.join(ref_path);
                    if !resolved.is_file() {
                        self.errors.push(SkillSpecError::UnresolvedRef {
                            name: lc.name.clone(),
                            path: ref_path.to_string(),
                            span: lc.span,
                        });
                    }
                }
            }
        }

        // Validate load references in steps (own + inherited lazy contexts)
        for step in &body.steps {
            for load_name in &step.loads {
                if !lazy_names.contains(load_name) && !inherited_lazy.contains(load_name) {
                    self.errors.push(SkillSpecError::UnknownLazyContext {
                        name: load_name.clone(),
                        span: step.span,
                    });
                }
            }
        }

        let step_names: HashSet<&str> = body.steps.iter()
            .map(|s| s.name.as_str())
            .chain(inherited_steps.iter().map(|s| s.as_str()))
            .collect();
        for ctx in &body.contexts {
            if let Some(until) = &ctx.until {
                if !step_names.contains(until.as_str()) {
                    self.errors.push(SkillSpecError::UnknownStep {
                        name: until.clone(),
                        span: ctx.span,
                    });
                }
            }
        }

        if let Some(observe) = &body.observe {
            let mut seen_metrics: HashMap<String, Span> = HashMap::new();
            for metric in &observe.metrics {
                if let Some(_existing) = seen_metrics.get(&metric.name) {
                    self.errors.push(SkillSpecError::DuplicateField {
                        name: metric.name.clone(),
                        span: metric.span,
                    });
                } else {
                    seen_metrics.insert(metric.name.clone(), metric.span);
                }
            }
        }

        self.check_steps(body, inherited_steps);
    }

    fn check_use_calls(&mut self, skill: &Skill, sigs: &HashMap<String, Vec<Field>>) {
        for step in &skill.body.steps {
            if let Some(use_call) = &step.use_call {
                self.check_single_use_call(use_call, sigs);
            }
        }
    }

    fn check_single_use_call(&mut self, use_call: &UseCall, sigs: &HashMap<String, Vec<Field>>) {
        let target_fields = match sigs.get(&use_call.skill_name) {
            Some(fields) => fields,
            None => return, // not defined locally — could be external
        };

        let expected: HashMap<&str, &Field> = target_fields.iter()
            .map(|f| (f.name.as_str(), f))
            .collect();

        let provided: HashSet<&str> = use_call.args.iter()
            .map(|(name, _)| name.as_str())
            .collect();

        // Check for missing required arguments (fields with defaults are effectively optional)
        for field in target_fields {
            if !field.optional && field.default.is_none() && !provided.contains(field.name.as_str()) {
                self.errors.push(SkillSpecError::MismatchedArg {
                    skill_name: use_call.skill_name.clone(),
                    message: format!("missing required argument '{}'", field.name),
                    span: use_call.span,
                });
            }
        }

        // Check for unknown arguments
        for (arg_name, _) in &use_call.args {
            if !expected.contains_key(arg_name.as_str()) {
                self.errors.push(SkillSpecError::MismatchedArg {
                    skill_name: use_call.skill_name.clone(),
                    message: format!("unknown argument '{}'", arg_name),
                    span: use_call.span,
                });
            }
        }
    }

    fn check_steps(&mut self, body: &Body, inherited_steps: &HashSet<String>) {
        let steps = &body.steps;

        // Detect duplicate step names within this body
        let mut seen_names: HashMap<String, Span> = HashMap::new();
        for step in steps {
            if let Some(existing_span) = seen_names.get(&step.name) {
                self.errors.push(SkillSpecError::DuplicateField {
                    name: step.name.clone(),
                    span: step.span,
                });
                let _ = existing_span;
            } else {
                seen_names.insert(step.name.clone(), step.span);
            }
        }

        let own_names: HashSet<String> = steps.iter().map(|s| s.name.clone()).collect();

        // Validate requires references (own + inherited steps)
        for step in steps {
            if let Some(dep) = &step.requires {
                let referenced = dep_names(dep);
                for name in referenced {
                    if !own_names.contains(&name) && !inherited_steps.contains(&name) {
                        self.errors.push(SkillSpecError::UnknownStep {
                            name: name.clone(),
                            span: step.span,
                        });
                    }
                }
            }
        }

        // Cycle check within this skill's own steps only
        self.check_cycles(steps);

        // Check for multiple unconditional emit statements
        let emit_steps: Vec<&Step> = steps.iter().filter(|s| s.emit).collect();
        if emit_steps.len() >= 2 {
            let unconditional_emits: Vec<&&Step> =
                emit_steps.iter().filter(|s| s.when.is_none()).collect();
            if unconditional_emits.len() >= 2 {
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

    fn resolve_skill_ancestry<'a>(skill: &'a Skill, all_skills: &'a [Skill]) -> Vec<&'a Skill> {
        crate::ast::resolve_ancestry(skill, all_skills)
    }

    fn check_extends_cycles(&mut self, skills: &[Skill]) {
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for skill in skills {
            let deps = match &skill.extends {
                Some(base) => vec![base.clone()],
                None => vec![],
            };
            adj.insert(skill.name.clone(), deps);
        }

        let mut visited: HashSet<String> = HashSet::new();
        let mut in_stack: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = Vec::new();

        for skill in skills {
            if !visited.contains(&skill.name)
                && let Some(cycle) =
                    Self::dfs_cycle(&skill.name, &adj, &mut visited, &mut in_stack, &mut stack)
                {
                    self.errors.push(SkillSpecError::DependencyCycle {
                        cycle: cycle.join(" -> "),
                    });
                    return;
                }
        }
    }

    /// Generic DAG cycle check for any sequence of (name, optional requires, span).
    /// Reuses dfs_cycle logic; reports DependencyCycle on the first cycle found.
    fn check_cycles_named(&mut self, nodes: &[(String, Option<Dependency>, Span)]) {
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for (name, dep, _) in nodes {
            let deps = dep
                .as_ref()
                .map(dep_names)
                .unwrap_or_default();
            adj.insert(name.clone(), deps);
        }

        let mut visited: HashSet<String> = HashSet::new();
        let mut in_stack: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = Vec::new();

        for (name, _, _) in nodes {
            if !visited.contains(name)
                && let Some(cycle) =
                    Self::dfs_cycle(name, &adj, &mut visited, &mut in_stack, &mut stack)
                {
                    self.errors.push(SkillSpecError::DependencyCycle {
                        cycle: cycle.join(" -> "),
                    });
                    return;
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
                .map(dep_names)
                .unwrap_or_default();
            adj.insert(step.name.clone(), deps);
        }

        let mut visited: HashSet<String> = HashSet::new();
        let mut in_stack: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = Vec::new();

        let names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();

        for name in &names {
            if !visited.contains(name)
                && let Some(cycle) = Self::dfs_cycle(name, &adj, &mut visited, &mut in_stack, &mut stack) {
                    self.errors.push(SkillSpecError::DependencyCycle {
                        cycle: cycle.join(" -> "),
                    });
                    // Only report the first cycle found
                    return;
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
                if !visited.contains(neighbor)
                    && let Some(cycle) = Self::dfs_cycle(neighbor, adj, visited, in_stack, stack) {
                        stack.pop();
                        in_stack.remove(node);
                        return Some(cycle);
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

    fn check_tests(&mut self, skill: &Skill) {
        for test in &skill.tests {
            self.check_test_mocks(test, skill);
            self.check_test_fixture_paths(test);
            self.check_test_given_keys(test, skill);
            self.check_test_expectations(test, skill);
        }
    }

    fn check_test_mocks(&mut self, test: &TestBlock, skill: &Skill) {
        if test.mocks.is_empty() {
            return;
        }

        let valid_tools: HashSet<String> = match &skill.tools {
            Some(tools_block) => {
                let mut set = HashSet::new();
                for decl in tools_block.required.iter().chain(tools_block.optional.iter()) {
                    match &decl.kind {
                        ToolKind::Mcp(_) => {
                            set.insert(decl.name.clone());
                            set.insert(format!("tools.mcp.{}", decl.name));
                        }
                        ToolKind::Builtin | ToolKind::Generic => {
                            set.insert(decl.name.clone());
                            set.insert(format!("tools.{}", decl.name));
                        }
                    }
                }
                set
            }
            None => HashSet::new(),
        };

        for mock in &test.mocks {
            if !valid_tools.contains(&mock.tool_path) {
                self.errors.push(SkillSpecError::UnknownMockTool {
                    tool_path: mock.tool_path.clone(),
                    test_name: test.name.clone(),
                    span: test.span,
                });
            }
        }
    }

    fn check_test_fixture_paths(&mut self, test: &TestBlock) {
        let base = match &self.base_dir {
            Some(dir) => dir.clone(),
            None => return,
        };

        for (_, value) in &test.given {
            if let Expr::StringLit(s) = value
                && (s.ends_with(".agent") || s.ends_with(".md"))
            {
                let resolved = base.join(s);
                if !resolved.is_file() {
                    self.errors.push(SkillSpecError::UnresolvedFixturePath {
                        path: s.clone(),
                        test_name: test.name.clone(),
                        span: test.span,
                    });
                } else if s.ends_with(".agent")
                    && let Err(e) = resolve::parse_file(&resolved)
                {
                    self.errors.push(SkillSpecError::FixtureParseError {
                        path: s.clone(),
                        message: format!("{e}"),
                        test_name: test.name.clone(),
                        span: test.span,
                    });
                }
            }
        }
    }

    fn check_test_given_keys(&mut self, test: &TestBlock, skill: &Skill) {
        let input_names: Option<HashSet<&str>> = skill.input.as_ref().map(|fields| {
            fields.iter().map(|f| f.name.as_str()).collect()
        });

        for (key, _) in &test.given {
            match &input_names {
                Some(names) if names.contains(key.as_str()) => {}
                _ => {
                    self.errors.push(SkillSpecError::UnknownGivenKey {
                        key: key.clone(),
                        test_name: test.name.clone(),
                        span: test.span,
                    });
                }
            }
        }
    }

    fn check_test_expectations(&mut self, test: &TestBlock, skill: &Skill) {
        let output_names: Option<HashSet<&str>> = skill.output.as_ref().map(|fields| {
            fields.iter().map(|f| f.name.as_str()).collect()
        });

        for expectation in &test.expectations {
            let segments: Vec<&str> = expectation.path.split('.').collect();
            if segments.len() >= 2 && segments[0] == "output" {
                let field = segments[1];
                match &output_names {
                    Some(names) if names.contains(field) => {}
                    _ => {
                        self.errors.push(SkillSpecError::UnknownExpectField {
                            field: field.to_string(),
                            test_name: test.name.clone(),
                            span: test.span,
                        });
                    }
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

    fn check_with_base(input: &str, base: &std::path::Path) -> std::result::Result<(), Vec<SkillSpecError>> {
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let mut checker = Checker::with_base_dir(base.to_path_buf());
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
                    lazy context "docs" (priority: supplementary) {
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

    #[test]
    fn extends_unknown_skill_errors() {
        let result = check(r#"
            skill "child" extends "nonexistent" {
                body { context { "ok" } }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::UnresolvedExtends { .. })),
            "should report UnresolvedExtends: {:?}",
            errs
        );
    }

    #[test]
    fn extends_valid_skill_passes() {
        let result = check(r#"
            skill "base" {
                body { context { "base." } }
            }
            skill "child" extends "base" {
                body { context { "child." } }
            }
        "#);
        assert!(result.is_ok());
    }

    #[test]
    fn extends_self_cycle_errors() {
        let result = check(r#"
            skill "ouroboros" extends "ouroboros" {
                body { context { "ok" } }
            }
        "#);
        assert!(result.is_err());
    }

    #[test]
    fn extends_mutual_cycle_errors() {
        let result = check(r#"
            skill "a" extends "b" {
                body { context { "a" } }
            }
            skill "b" extends "a" {
                body { context { "b" } }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::DependencyCycle { .. })),
            "should detect mutual extends cycle: {:?}",
            errs
        );
    }

    #[test]
    fn extends_three_way_cycle_errors() {
        let result = check(r#"
            skill "a" extends "b" {
                body { context { "a" } }
            }
            skill "b" extends "c" {
                body { context { "b" } }
            }
            skill "c" extends "a" {
                body { context { "c" } }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::DependencyCycle { .. })),
            "should detect 3-way extends cycle: {:?}",
            errs
        );
    }

    #[test]
    fn extends_chain_no_cycle_passes() {
        let result = check(r#"
            skill "grandparent" {
                body { context { "gp" } }
            }
            skill "parent" extends "grandparent" {
                body { context { "p" } }
            }
            skill "child" extends "parent" {
                body { context { "c" } }
            }
        "#);
        assert!(result.is_ok());
    }

    #[test]
    fn requires_inherited_step_passes() {
        let result = check(r#"
            skill "base" {
                body {
                    step analyze { context { "Analyze." } }
                }
            }
            skill "child" extends "base" {
                body {
                    step report {
                        requires analyze
                        context { "Report." }
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "requires on inherited step should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn requires_deeply_inherited_step_passes() {
        let result = check(r#"
            skill "grandparent" {
                body {
                    step setup { context { "Setup." } }
                }
            }
            skill "parent" extends "grandparent" {
                body { context { "Middle." } }
            }
            skill "child" extends "parent" {
                body {
                    step work {
                        requires setup
                        context { "Work." }
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "requires on grandparent step should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn load_inherited_lazy_context_passes() {
        let result = check(r#"
            skill "base" {
                body {
                    lazy context "docs" (priority: supplementary) {
                        summary "API docs."
                        ref "./api.md"
                    }
                    context { "Base." }
                }
            }
            skill "child" extends "base" {
                body {
                    step main {
                        load "docs"
                        context { "Use docs." }
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "load on inherited lazy context should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn requires_mixin_step_passes() {
        let result = check(r#"
            mixin logging {
                step log_start { context { "Log start." } }
            }
            skill "x" {
                include logging
                body {
                    step work {
                        requires log_start
                        context { "Work." }
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "requires on mixin step should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn cross_mixin_step_collision_errors() {
        let result = check(r#"
            mixin a {
                step setup { context { "A setup." } }
            }
            mixin b {
                step setup { context { "B setup." } }
            }
            skill "x" {
                include a
                include b
                body { context { "Work." } }
            }
        "#);
        assert!(result.is_err(), "cross-mixin step collision should error");
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::DuplicateField { .. })),
            "should report DuplicateField for mixin collision: {:?}",
            errs
        );
    }

    #[test]
    fn cross_mixin_collision_ok_if_child_overrides() {
        let result = check(r#"
            mixin a {
                step setup { context { "A setup." } }
            }
            mixin b {
                step setup { context { "B setup." } }
            }
            skill "x" {
                include a
                include b
                body {
                    step setup { context { "Child setup." } }
                }
            }
        "#);
        assert!(result.is_ok(), "mixin collision should be ok if child overrides: {:?}", result.unwrap_err());
    }

    #[test]
    fn shadowed_import_errors() {
        let result = check(r#"
            import { Finding } from "@types/review"
            type Finding {
                file: string
                severity: string
            }
            skill "x" {
                input { f: Finding }
                body { context { "ok" } }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::ShadowedImport { .. })),
            "should report ShadowedImport: {:?}",
            errs
        );
    }

    #[test]
    fn import_without_shadow_passes() {
        let result = check(r#"
            import { OtherThing } from "@types/misc"
            type Finding {
                file: string
                severity: string
            }
            skill "x" {
                input { f: Finding }
                body { context { "ok" } }
            }
        "#);
        assert!(result.is_ok());
    }

    #[test]
    fn multiple_emit_across_extends_errors() {
        let result = check(r#"
            skill "base" {
                body {
                    step produce { emit output context { "base output" } }
                }
            }
            skill "child" extends "base" {
                body {
                    step also_produce { emit output context { "child output" } }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, SkillSpecError::MultipleEmit { .. })));
    }

    #[test]
    fn single_emit_across_extends_ok() {
        let result = check(r#"
            skill "base" {
                body {
                    step analyse { context { "analyse" } }
                }
            }
            skill "child" extends "base" {
                body {
                    step produce { emit output context { "child output" } }
                }
            }
        "#);
        assert!(result.is_ok());
    }

    #[test]
    fn child_overrides_base_emit_ok() {
        let result = check(r#"
            skill "base" {
                body {
                    step produce { emit output context { "base" } }
                }
            }
            skill "child" extends "base" {
                body {
                    step produce { emit output context { "overridden" } }
                }
            }
        "#);
        assert!(result.is_ok());
    }

    #[test]
    fn unresolved_ref_errors() {
        let base = std::env::temp_dir();
        let result = check_with_base(r#"
            skill "x" {
                body {
                    lazy context "ghost" (priority: supplementary) {
                        summary "Points to nothing."
                        ref "./does-not-exist.md"
                    }
                    step main {
                        load "ghost"
                        context { "ok" }
                    }
                }
            }
        "#, &base);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, SkillSpecError::UnresolvedRef { .. })));
    }

    #[test]
    fn unresolved_index_ref_errors() {
        let base = std::env::temp_dir();
        let result = check_with_base(r#"
            skill "x" {
                body {
                    lazy context "catalog" (priority: supplementary) {
                        summary "Indexed sections."
                        index {
                            section "missing" {
                                summary "Not on disk."
                                ref "./nope.md"
                            }
                        }
                    }
                    step main {
                        load "catalog"
                        context { "ok" }
                    }
                }
            }
        "#, &base);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, SkillSpecError::UnresolvedRef { .. })));
    }

    // ── Cross-skill use-call validation ─────────────────────────────

    #[test]
    fn use_call_known_skill_passes() {
        let result = check(r#"
            skill "analyzer" {
                input { files: string[] }
                body { context { "Analyze." } }
            }
            skill "reviewer" {
                body {
                    step review {
                        use analyzer(files: input.files)
                        context { "Review." }
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "use call to known skill with matching args should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn use_call_wrong_arg_name_errors() {
        let result = check(r#"
            skill "analyzer" {
                input { files: string[] }
                body { context { "Analyze." } }
            }
            skill "caller" {
                body {
                    step s {
                        use analyzer(repos: input.files)
                        context { "Go." }
                    }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, SkillSpecError::MismatchedArg { .. })),
            "should report MismatchedArg: {:?}", errs);
    }

    #[test]
    fn use_call_missing_required_arg_errors() {
        let result = check(r#"
            skill "analyzer" {
                input { files: string[] }
                body { context { "Analyze." } }
            }
            skill "caller" {
                body {
                    step s {
                        use analyzer()
                        context { "Go." }
                    }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, SkillSpecError::MismatchedArg { .. })),
            "should report missing required arg: {:?}", errs);
    }

    #[test]
    fn use_call_extra_arg_errors() {
        let result = check(r#"
            skill "analyzer" {
                input { files: string[] }
                body { context { "Analyze." } }
            }
            skill "caller" {
                body {
                    step s {
                        use analyzer(files: input.files, extra: input.x)
                        context { "Go." }
                    }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, SkillSpecError::MismatchedArg { .. })),
            "should report unknown arg: {:?}", errs);
    }

    #[test]
    fn use_call_optional_arg_omitted_passes() {
        let result = check(r#"
            skill "analyzer" {
                input { files: string[] focus?: string }
                body { context { "Analyze." } }
            }
            skill "caller" {
                body {
                    step s {
                        use analyzer(files: input.files)
                        context { "Go." }
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "omitting optional arg should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn use_call_external_skill_passes() {
        let result = check(r#"
            skill "caller" {
                body {
                    step s {
                        use external_skill(x: input.y)
                        context { "Go." }
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "external (unknown) skill should not error: {:?}", result.unwrap_err());
    }

    #[test]
    fn ref_skipped_without_base_dir() {
        let result = check(r#"
            skill "x" {
                body {
                    lazy context "ghost" (priority: supplementary) {
                        summary "No base dir, no check."
                        ref "./does-not-exist.md"
                    }
                    step main {
                        load "ghost"
                        context { "ok" }
                    }
                }
            }
        "#);
        assert!(result.is_ok());
    }

    // ── Test block validation: mock tool paths (Gap 5) ──────────────────────

    #[test]
    fn mock_tool_path_valid() {
        let result = check(r#"
            skill "x" {
                tools {
                    require mcp("github") {
                        pr_diff(repo: string, pr: int) -> string
                    }
                }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        mock tools.mcp.github: unavailable
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "valid mock tool path should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn mock_tool_path_unknown_errors() {
        let result = check(r#"
            skill "x" {
                tools { require Bash }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        mock tools.mcp.nonexistent: unavailable
                    }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::UnknownMockTool { .. })),
            "should report UnknownMockTool: {:?}", errs
        );
    }

    #[test]
    fn mock_without_tools_block_errors() {
        let result = check(r#"
            skill "x" {
                body { context { "ok" } }
                tests {
                    test "basic" {
                        mock tools.mcp.github: unavailable
                    }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::UnknownMockTool { .. })),
            "should report UnknownMockTool when no tools block: {:?}", errs
        );
    }

    #[test]
    fn mock_optional_tool_valid() {
        let result = check(r#"
            skill "x" {
                tools {
                    optional mcp("slack") {
                        send(channel: string, text: string) -> void
                    }
                }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        mock tools.mcp.slack: unavailable
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "mock of optional tool should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn mock_builtin_tool_valid() {
        let result = check(r#"
            skill "x" {
                tools { require Bash }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        mock tools.Bash: unavailable
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "mock of builtin tool should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn mock_short_name_valid() {
        let result = check(r#"
            skill "x" {
                tools {
                    require mcp("github") {
                        pr_diff(repo: string, pr: int) -> string
                    }
                }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        mock github {
                            pr_diff(repo: "org/app", pr: 1) -> "diff"
                        }
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "mock with short tool name should pass: {:?}", result.unwrap_err());
    }

    // ── Test block validation: fixture file paths (Gap 1) ───────────────────

    #[test]
    fn fixture_path_valid() {
        let dir = std::env::temp_dir().join("skillspec_test_fixture_valid");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("good.agent"), r#"skill "s" { body { context { "ok" } } }"#).unwrap();

        let result = check_with_base(r#"
            skill "x" {
                input { src: string }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        given { src: "good.agent" }
                    }
                }
            }
        "#, &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "valid fixture path should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn fixture_path_missing_errors() {
        let dir = std::env::temp_dir().join("skillspec_test_fixture_missing");
        let _ = std::fs::create_dir_all(&dir);

        let result = check_with_base(r#"
            skill "x" {
                input { src: string }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        given { src: "ghost.agent" }
                    }
                }
            }
        "#, &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::UnresolvedFixturePath { .. })),
            "should report UnresolvedFixturePath: {:?}", errs
        );
    }

    #[test]
    fn fixture_path_skipped_without_base_dir() {
        let result = check(r#"
            skill "x" {
                input { src: string }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        given { src: "ghost.agent" }
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "fixture path check should skip without base_dir");
    }

    #[test]
    fn fixture_path_unparseable_agent_errors() {
        let dir = std::env::temp_dir().join("skillspec_test_fixture_bad");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("bad.agent"), "this is not valid agent syntax {{{").unwrap();

        let result = check_with_base(r#"
            skill "x" {
                input { src: string }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        given { src: "bad.agent" }
                    }
                }
            }
        "#, &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::FixtureParseError { .. })),
            "should report FixtureParseError: {:?}", errs
        );
    }

    #[test]
    fn fixture_path_md_exists_passes() {
        let dir = std::env::temp_dir().join("skillspec_test_fixture_md");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("readme.md"), "# Hello").unwrap();

        let result = check_with_base(r#"
            skill "x" {
                input { doc: string }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        given { doc: "readme.md" }
                    }
                }
            }
        "#, &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "existing .md fixture should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn non_path_string_in_given_skipped() {
        let dir = std::env::temp_dir().join("skillspec_test_fixture_nonpath");
        let _ = std::fs::create_dir_all(&dir);

        let result = check_with_base(r#"
            skill "x" {
                input { query: string }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        given { query: "SELECT * FROM users" }
                    }
                }
            }
        "#, &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "non-path string should not trigger fixture check");
    }

    // ── Test block validation: given key vs input contract (Gap 3) ──────────

    #[test]
    fn given_keys_match_input_contract() {
        let result = check(r#"
            skill "x" {
                input {
                    files: string[]
                    focus?: string
                }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        given {
                            files: ["a.py"]
                            focus: "security"
                        }
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "valid given keys should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn given_unknown_key_errors() {
        let result = check(r#"
            skill "x" {
                input { files: string[] }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        given {
                            files: ["a.py"]
                            nonexistent_field: "x"
                        }
                    }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::UnknownGivenKey { key, .. } if key == "nonexistent_field")),
            "should report UnknownGivenKey for nonexistent_field: {:?}", errs
        );
    }

    #[test]
    fn given_on_skill_without_input_errors() {
        let result = check(r#"
            skill "x" {
                body { context { "ok" } }
                tests {
                    test "basic" {
                        given { files: ["a.py"] }
                    }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::UnknownGivenKey { .. })),
            "should report UnknownGivenKey when no input block: {:?}", errs
        );
    }

    // ── Test block validation: expect path vs output contract (Gap 4) ───────

    #[test]
    fn expect_output_field_valid() {
        let result = check(r#"
            type Finding { file: string }
            skill "x" {
                output {
                    findings: Finding[]
                    summary: string
                }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        expect {
                            output.findings: >= 1
                            output.summary: matches(".*")
                        }
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "valid expect output fields should pass: {:?}", result.unwrap_err());
    }

    #[test]
    fn expect_unknown_output_field_errors() {
        let result = check(r#"
            skill "x" {
                output { summary: string }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        expect {
                            output.nonexistent: equals("x")
                        }
                    }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::UnknownExpectField { field, .. } if field == "nonexistent")),
            "should report UnknownExpectField: {:?}", errs
        );
    }

    #[test]
    fn expect_on_skill_without_output_errors() {
        let result = check(r#"
            skill "x" {
                body { context { "ok" } }
                tests {
                    test "basic" {
                        expect {
                            output.result: equals("x")
                        }
                    }
                }
            }
        "#);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(e, SkillSpecError::UnknownExpectField { .. })),
            "should report UnknownExpectField when no output block: {:?}", errs
        );
    }

    #[test]
    fn expect_nested_path_only_checks_first_segment() {
        let result = check(r#"
            type TestResult { total: int }
            skill "x" {
                output { result: TestResult }
                body { context { "ok" } }
                tests {
                    test "basic" {
                        expect {
                            output.result.total: >= 1
                        }
                    }
                }
            }
        "#);
        assert!(result.is_ok(), "deep path should only validate first segment: {:?}", result.unwrap_err());
    }

    // ── Regression guards ───────────────────────────────────────────────────

    #[test]
    fn no_tests_block_passes() {
        let result = check(r#"
            skill "x" {
                body { context { "ok" } }
            }
        "#);
        assert!(result.is_ok());
    }

    #[test]
    fn empty_tests_block_passes() {
        let result = check(r#"
            skill "x" {
                body { context { "ok" } }
                tests { }
            }
        "#);
        assert!(result.is_ok());
    }
}
