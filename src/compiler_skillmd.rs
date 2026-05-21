use crate::ast::*;
use std::collections::{HashMap, HashSet, VecDeque};

pub struct SkillMdCompiler;

impl SkillMdCompiler {
    pub fn new() -> Self {
        Self
    }

    pub fn compile(&self, skill: &Skill) -> String {
        let mut out = String::new();

        // ── YAML frontmatter ──────────────────────────────────────────────
        out.push_str("---\n");
        out.push_str(&format!("name: {}\n", skill.name));

        if let Some(desc) = self.extract_description(skill) {
            out.push_str(&format!("description: \"{}\"\n", desc.replace('"', "\\\"")));
        }

        if let Some(input_fields) = &skill.input {
            if !input_fields.is_empty() {
                out.push_str("parameters:\n");
                for field in input_fields {
                    out.push_str(&format!("  - name: {}\n", field.name));
                    out.push_str(&format!("    type: {}\n", self.type_expr_to_string(&field.ty)));
                    if field.optional {
                        out.push_str("    optional: true\n");
                    }
                    if let Some(default) = &field.default {
                        out.push_str(&format!(
                            "    default: {}\n",
                            self.expr_to_string(default)
                        ));
                    }
                }
            }
        }

        out.push_str("---\n\n");

        // ── Title ─────────────────────────────────────────────────────────
        out.push_str(&format!("# {}\n\n", skill.name));

        // ── Output section ────────────────────────────────────────────────
        if let Some(output_fields) = &skill.output {
            if !output_fields.is_empty() {
                out.push_str("## Output\n\n");
                for field in output_fields {
                    let mut annotation = String::new();
                    if field.optional {
                        annotation.push_str(" (optional)");
                    }
                    if let Some(default) = &field.default {
                        annotation.push_str(&format!(
                            " (default: {})",
                            self.expr_to_string(default)
                        ));
                    }
                    out.push_str(&format!(
                        "- **{}**: {}{}\n",
                        field.name,
                        self.type_expr_to_string(&field.ty),
                        annotation
                    ));
                }
                out.push('\n');
            }
        }

        // ── Preconditions ─────────────────────────────────────────────────
        if !skill.pre.is_empty() {
            out.push_str("## Preconditions\n\n");
            for assertion in &skill.pre {
                out.push_str(&format!(
                    "- {} — *{}*\n",
                    self.expr_to_string(&assertion.condition),
                    assertion.message
                ));
            }
            out.push('\n');
        }

        // ── Postconditions ────────────────────────────────────────────────
        if !skill.post.is_empty() {
            out.push_str("## Postconditions\n\n");
            for assertion in &skill.post {
                out.push_str(&format!(
                    "- {} — *{}*\n",
                    self.expr_to_string(&assertion.condition),
                    assertion.message
                ));
            }
            out.push('\n');
        }

        // ── Tools ─────────────────────────────────────────────────────────
        if let Some(tools_block) = &skill.tools {
            out.push_str(&self.emit_tools_section(tools_block));
        }

        // ── Permissions ───────────────────────────────────────────────────
        if let Some(perms) = &skill.permissions {
            out.push_str(&self.emit_permissions_section(perms));
        }

        // ── Mixin includes ────────────────────────────────────────────────
        if !skill.includes.is_empty() {
            for mixin_name in &skill.includes {
                out.push_str(&format!("*Includes mixin: {}*\n\n", mixin_name));
            }
        }

        // ── Prompt directives ─────────────────────────────────────────────
        out.push_str(&self.emit_prompt_directives(&skill.body.directives));

        // ── Lazy contexts → References section ────────────────────────────
        if !skill.body.lazy_contexts.is_empty() {
            out.push_str(&self.emit_lazy_contexts(&skill.body.lazy_contexts));
        }

        // ── Skill-level context blocks (sorted by priority desc) ──────────
        let mut skill_contexts: Vec<&ContextBlock> = skill.body.contexts.iter().collect();
        skill_contexts.sort_by(|a, b| {
            let pa = a.priority.unwrap_or(0);
            let pb = b.priority.unwrap_or(0);
            pb.cmp(&pa)
        });

        for ctx in &skill_contexts {
            out.push_str(&self.dedent(&ctx.text));
            out.push_str("\n\n");
        }

        // ── Tests ─────────────────────────────────────────────────────────
        if !skill.tests.is_empty() {
            out.push_str(&self.emit_tests_section(&skill.tests));
        }

        // ── Steps (topologically sorted) ──────────────────────────────────
        let sorted_steps = self.topo_sort(&skill.body.steps);

        for step in sorted_steps {
            out.push_str(&format!("## Step: {}\n\n", step.name));

            if let Some(use_call) = &step.use_call {
                out.push_str(&format!("*Uses: {}*\n\n", use_call.skill_name));
            }

            if step.emit {
                out.push_str("*Produces final output.*\n\n");
            }

            let mut step_contexts: Vec<&ContextBlock> = step.contexts.iter().collect();
            step_contexts.sort_by(|a, b| {
                let pa = a.priority.unwrap_or(0);
                let pb = b.priority.unwrap_or(0);
                pb.cmp(&pa)
            });

            for ctx in &step_contexts {
                out.push_str(&self.dedent(&ctx.text));
                out.push_str("\n\n");
            }
        }

        out
    }

    // ── Pipeline compiler ─────────────────────────────────────────────────────

    pub fn compile_pipeline(&self, pipeline: &Pipeline) -> String {
        let mut out = String::new();

        // YAML frontmatter
        out.push_str("---\n");
        out.push_str(&format!("name: {}\n", pipeline.name));
        out.push_str("type: pipeline\n");
        out.push_str("---\n\n");

        // Title
        out.push_str(&format!("# Pipeline: {}\n\n", pipeline.name));

        // Input/Output
        if let Some(input_fields) = &pipeline.input {
            if !input_fields.is_empty() {
                out.push_str("## Input\n\n");
                for field in input_fields {
                    out.push_str(&format!(
                        "- **{}**: {}\n",
                        field.name,
                        self.type_expr_to_string(&field.ty)
                    ));
                }
                out.push('\n');
            }
        }

        if let Some(output_fields) = &pipeline.output {
            if !output_fields.is_empty() {
                out.push_str("## Output\n\n");
                for field in output_fields {
                    out.push_str(&format!(
                        "- **{}**: {}\n",
                        field.name,
                        self.type_expr_to_string(&field.ty)
                    ));
                }
                out.push('\n');
            }
        }

        // Stages
        for stage in &pipeline.stages {
            out.push_str(&format!("## Stage: {}\n\n", stage.name));

            if let Some(dep) = &stage.requires {
                out.push_str(&format!("*Requires: {}*\n\n", self.dep_to_string(dep)));
            }

            // Use call
            let use_call = &stage.use_call;
            if use_call.args.is_empty() {
                out.push_str(&format!("*Uses: {}*\n\n", use_call.skill_name));
            } else {
                let args: Vec<String> = use_call
                    .args
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, self.expr_to_string(v)))
                    .collect();
                out.push_str(&format!(
                    "*Uses: {}({})*\n\n",
                    use_call.skill_name,
                    args.join(", ")
                ));
            }
        }

        // Timeout
        if let Some(timeout) = &pipeline.timeout {
            out.push_str(&format!("**Timeout:** {}\n\n", timeout));
        }

        out
    }

    // ── Orchestration compiler ────────────────────────────────────────────────

    pub fn compile_orchestration(&self, orch: &Orchestration) -> String {
        let mut out = String::new();

        // YAML frontmatter
        out.push_str("---\n");
        out.push_str(&format!("name: {}\n", orch.name));
        out.push_str("type: orchestration\n");
        out.push_str("---\n\n");

        // Title
        out.push_str(&format!("# Orchestration: {}\n\n", orch.name));

        // Agents section
        if !orch.agents.is_empty() {
            out.push_str("## Agents\n\n");
            for agent in &orch.agents {
                out.push_str(&format!(
                    "- **{}**: {} ({})\n",
                    agent.name, agent.skill, agent.model
                ));
            }
            out.push('\n');
        }

        // Input/Output
        if let Some(input_fields) = &orch.input {
            if !input_fields.is_empty() {
                out.push_str("## Input\n\n");
                for field in input_fields {
                    out.push_str(&format!(
                        "- **{}**: {}\n",
                        field.name,
                        self.type_expr_to_string(&field.ty)
                    ));
                }
                out.push('\n');
            }
        }

        if let Some(output_fields) = &orch.output {
            if !output_fields.is_empty() {
                out.push_str("## Output\n\n");
                for field in output_fields {
                    out.push_str(&format!(
                        "- **{}**: {}\n",
                        field.name,
                        self.type_expr_to_string(&field.ty)
                    ));
                }
                out.push('\n');
            }
        }

        // Phases
        for phase in &orch.phases {
            out.push_str(&format!("## Phase: {}\n\n", phase.name));

            if let Some(dep) = &phase.requires {
                out.push_str(&format!("*Requires: {}*\n\n", self.dep_to_string(dep)));
            }

            for action in &phase.actions {
                if action.args.is_empty() {
                    out.push_str(&format!("{}.{}()\n\n", action.agent_name, action.method));
                } else {
                    let args: Vec<String> = action
                        .args
                        .iter()
                        .map(|(k, v)| format!("{}: {}", k, self.expr_to_string(v)))
                        .collect();
                    out.push_str(&format!(
                        "{}.{}({})\n\n",
                        action.agent_name,
                        action.method,
                        args.join(", ")
                    ));
                }
            }

            if let Some(emit_source) = &phase.emit {
                out.push_str(&format!(
                    "*Produces final output from {}*\n\n",
                    emit_source
                ));
            }
        }

        // Timeout
        if let Some(timeout) = &orch.timeout {
            out.push_str(&format!("**Timeout:** {}\n\n", timeout));
        }

        out
    }

    // ── Phase 2 section emitters ──────────────────────────────────────────────

    fn emit_lazy_contexts(&self, lazy_contexts: &[LazyContext]) -> String {
        let mut out = String::new();
        out.push_str("## References (lazy-loaded)\n\n");

        // Sort by priority desc
        let mut sorted: Vec<&LazyContext> = lazy_contexts.iter().collect();
        sorted.sort_by(|a, b| {
            let pa = a.priority.unwrap_or(0);
            let pb = b.priority.unwrap_or(0);
            pb.cmp(&pa)
        });

        for lc in sorted {
            let priority_str = if let Some(p) = lc.priority {
                format!(" (priority: {})", p)
            } else {
                String::new()
            };

            match &lc.content {
                LazyContent::Ref(path) => {
                    out.push_str(&format!(
                        "- **{}**{}: {} → `{}`\n",
                        lc.name, priority_str, lc.summary, path
                    ));
                }
                LazyContent::Inline(text) => {
                    out.push_str(&format!(
                        "- **{}**{}: {}\n  *Inline content: {}*\n",
                        lc.name,
                        priority_str,
                        lc.summary,
                        text.trim()
                    ));
                }
                LazyContent::Index(sections) => {
                    out.push_str(&format!(
                        "- **{}**{}: {}\n",
                        lc.name, priority_str, lc.summary
                    ));
                    for section in sections {
                        out.push_str(&format!(
                            "  - **{}**: {} → `{}`\n",
                            section.name, section.summary, section.ref_path
                        ));
                    }
                }
            }
        }

        out.push('\n');
        out
    }

    fn emit_tools_section(&self, tools: &ToolsBlock) -> String {
        let mut out = String::new();
        out.push_str("## Tools\n\n");

        if !tools.required.is_empty() {
            out.push_str("**Required:**\n");
            for tool in &tools.required {
                out.push_str(&format!("- {}\n", self.tool_decl_to_string(tool)));
            }
            out.push('\n');
        }

        if !tools.optional.is_empty() {
            out.push_str("**Optional:**\n");
            for tool in &tools.optional {
                out.push_str(&format!("- {}\n", self.tool_decl_to_string(tool)));
            }
            out.push('\n');
        }

        out
    }

    fn tool_decl_to_string(&self, tool: &ToolDecl) -> String {
        let base = match &tool.kind {
            ToolKind::Builtin => tool.name.clone(),
            ToolKind::Mcp(server) => format!("mcp(\"{}\")", server),
            ToolKind::Generic => tool.name.clone(),
        };

        if tool.methods.is_empty() {
            base
        } else {
            let methods: Vec<String> = tool
                .methods
                .iter()
                .map(|m| {
                    let params: Vec<String> = m
                        .params
                        .iter()
                        .map(|(name, ty, optional)| {
                            if *optional {
                                format!("{}?: {}", name, self.type_expr_to_string(ty))
                            } else {
                                format!("{}: {}", name, self.type_expr_to_string(ty))
                            }
                        })
                        .collect();
                    format!(
                        "{}({}) → {}",
                        m.name,
                        params.join(", "),
                        self.type_expr_to_string(&m.return_type)
                    )
                })
                .collect();
            format!("{}: {}", base, methods.join(", "))
        }
    }

    fn emit_permissions_section(&self, perms: &PermissionsBlock) -> String {
        let mut out = String::new();
        out.push_str("## Permissions\n\n");

        if let Some((mode, patterns)) = &perms.filesystem {
            out.push_str(&format!(
                "- **Filesystem:** {} — {}\n",
                mode,
                patterns.join(", ")
            ));
        }

        if let Some((mode, hosts)) = &perms.network {
            out.push_str(&format!(
                "- **Network:** {} — {}\n",
                mode,
                hosts.join(", ")
            ));
        }

        for secret in &perms.secrets {
            out.push_str(&format!("- **Secrets:** {}\n", secret));
        }

        out.push('\n');
        out
    }

    fn emit_tests_section(&self, tests: &[TestBlock]) -> String {
        let mut out = String::new();
        out.push_str("## Tests\n\n");

        for test in tests {
            out.push_str(&format!("### {}\n", test.name));

            if !test.given.is_empty() {
                let given_parts: Vec<String> = test
                    .given
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, self.expr_to_string(v)))
                    .collect();
                out.push_str(&format!("**Given:** {}\n", given_parts.join(", ")));
            }

            if !test.mocks.is_empty() {
                let mock_parts: Vec<String> = test
                    .mocks
                    .iter()
                    .map(|m| {
                        let desc = match &m.mock_type {
                            MockType::Responses(_) => "custom responses".to_string(),
                            MockType::Unavailable => "unavailable".to_string(),
                            MockType::Failing(reason) => {
                                if reason.is_empty() {
                                    "failing".to_string()
                                } else {
                                    format!("failing({})", reason)
                                }
                            }
                            MockType::Slow(duration) => {
                                if duration.is_empty() {
                                    "slow".to_string()
                                } else {
                                    format!("slow({})", duration)
                                }
                            }
                        };
                        format!("{} ({})", m.tool_path, desc)
                    })
                    .collect();
                out.push_str(&format!("**Mocks:** {}\n", mock_parts.join(", ")));
            }

            if !test.expectations.is_empty() {
                out.push_str("**Expects:**\n");
                for exp in &test.expectations {
                    out.push_str(&format!(
                        "- {}: {}\n",
                        exp.path,
                        self.assertion_to_string(&exp.assertion)
                    ));
                }
            }

            match (test.confidence, test.runs) {
                (Some(c), Some(r)) => {
                    out.push_str(&format!("**Confidence:** {} ({} runs)\n", c, r));
                }
                (Some(c), None) => {
                    out.push_str(&format!("**Confidence:** {}\n", c));
                }
                (None, Some(r)) => {
                    out.push_str(&format!("**Runs:** {}\n", r));
                }
                (None, None) => {}
            }

            out.push('\n');
        }

        out
    }

    fn assertion_to_string(&self, assertion: &AssertionExpr) -> String {
        match assertion {
            AssertionExpr::Equals(expr) => format!("equals({})", self.expr_to_string(expr)),
            AssertionExpr::Contains(expr) => format!("contains({})", self.expr_to_string(expr)),
            AssertionExpr::Matches(pattern) => format!("matches(\"{}\")", pattern),
            AssertionExpr::Resembles(desc) => format!("resembles(\"{}\")", desc),
            AssertionExpr::Satisfies(desc) => format!("satisfies(\"{}\")", desc),
            AssertionExpr::Between(low, high) => {
                format!(
                    "between({}, {})",
                    self.expr_to_string(low),
                    self.expr_to_string(high)
                )
            }
            AssertionExpr::Comparison(op, val) => {
                let op_str = match op {
                    BinOp::Eq => "==",
                    BinOp::NotEq => "!=",
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::LtEq => "<=",
                    BinOp::GtEq => ">=",
                    BinOp::In => "in",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                };
                format!("{} {}", op_str, self.expr_to_string(val))
            }
            AssertionExpr::ContainsWhere(expr) => {
                format!("contains(where: {})", self.expr_to_string(expr))
            }
            AssertionExpr::AllWhere(expr) => {
                format!("all(where: {})", self.expr_to_string(expr))
            }
            AssertionExpr::NoneWhere(expr) => {
                format!("none(where: {})", self.expr_to_string(expr))
            }
        }
    }

    fn emit_prompt_directives(&self, directives: &PromptDirectives) -> String {
        let mut out = String::new();

        // Persona first (as blockquote)
        if let Some(persona) = &directives.persona {
            let dedented = self.dedent(persona);
            for line in dedented.lines() {
                if line.trim().is_empty() {
                    out.push_str(">\n");
                } else {
                    out.push_str(&format!("> {}\n", line));
                }
            }
            out.push('\n');
        }

        // Reasoning mode
        if let Some(mode) = &directives.reasoning {
            out.push_str(&format!("**Reasoning mode:** {}\n\n", mode));
        }

        // Sampling
        if let Some(sampling) = &directives.sampling {
            let mut parts = Vec::new();
            if let Some(t) = sampling.temperature {
                parts.push(format!("temperature={}", t));
            }
            if let Some(p) = sampling.top_p {
                parts.push(format!("top_p={}", p));
            }
            if !parts.is_empty() {
                out.push_str(&format!("**Sampling:** {}\n\n", parts.join(", ")));
            }
        }

        // Format
        if let Some(fmt) = &directives.format {
            out.push_str(&format!(
                "**Output format:** {} ({})\n\n",
                fmt.style, fmt.structure
            ));
        }

        // Reinforcements
        for reinf in &directives.reinforcements {
            let trigger_str = match &reinf.trigger {
                ReinforceTrigger::EveryNSteps(n) => format!("every {} steps", n),
                ReinforceTrigger::OnContextShift => "on context shift".to_string(),
                ReinforceTrigger::WhenCondition(expr) => {
                    format!("when {}", self.expr_to_string(expr))
                }
            };
            out.push_str(&format!(
                "**Reinforcement:** {} — \"{}\"\n\n",
                trigger_str,
                reinf.text.trim()
            ));
        }

        // Examples
        if !directives.examples.is_empty() {
            out.push_str("### Examples\n\n");
            for example in &directives.examples {
                out.push_str(&format!("**{}**\n\n", example.name));
                out.push_str(&format!("*Input:* {}\n\n", example.input.trim()));
                out.push_str(&format!("*Output:* {}\n\n", example.output.trim()));
                if let Some(note) = &example.note {
                    out.push_str(&format!("*Note:* {}\n\n", note.trim()));
                }
            }
        }

        out
    }

    fn dep_to_string(&self, dep: &Dependency) -> String {
        match dep {
            Dependency::Single(name) => name.clone(),
            Dependency::All(names) => format!("all({})", names.join(", ")),
            Dependency::Any(names) => format!("any({})", names.join(", ")),
            Dependency::AllSteps => "*".to_string(),
        }
    }

    /// Kahn's algorithm (BFS) topological sort of steps by their `requires` deps.
    fn topo_sort<'a>(&self, steps: &'a [Step]) -> Vec<&'a Step> {
        if steps.is_empty() {
            return Vec::new();
        }

        // Build name -> index map
        let name_to_idx: HashMap<&str, usize> = steps
            .iter()
            .enumerate()
            .map(|(i, s)| (s.name.as_str(), i))
            .collect();

        let n = steps.len();
        let mut in_degree = vec![0usize; n];
        // adjacency list: dep -> dependents
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];

        // Collect all non-AllSteps step indices; AllSteps will be handled separately
        let mut allsteps_indices: Vec<usize> = Vec::new();

        for (i, step) in steps.iter().enumerate() {
            if let Some(dep) = &step.requires {
                match dep {
                    Dependency::AllSteps => {
                        // depends on ALL other steps — add after we know all edges
                        allsteps_indices.push(i);
                    }
                    Dependency::Single(name) => {
                        if let Some(&j) = name_to_idx.get(name.as_str()) {
                            adj[j].push(i);
                            in_degree[i] += 1;
                        }
                    }
                    Dependency::All(names) | Dependency::Any(names) => {
                        for name in names {
                            if let Some(&j) = name_to_idx.get(name.as_str()) {
                                adj[j].push(i);
                                in_degree[i] += 1;
                            }
                        }
                    }
                }
            }
        }

        // AllSteps nodes depend on every other step
        for &i in &allsteps_indices {
            for j in 0..n {
                if j != i {
                    adj[j].push(i);
                    in_degree[i] += 1;
                }
            }
        }

        // BFS (Kahn)
        let mut queue: VecDeque<usize> = VecDeque::new();
        for i in 0..n {
            if in_degree[i] == 0 {
                queue.push_back(i);
            }
        }

        let mut result: Vec<&'a Step> = Vec::with_capacity(n);
        let mut visited: HashSet<usize> = HashSet::new();

        while let Some(idx) = queue.pop_front() {
            if visited.contains(&idx) {
                continue;
            }
            visited.insert(idx);
            result.push(&steps[idx]);

            for &dep in &adj[idx] {
                if in_degree[dep] > 0 {
                    in_degree[dep] -= 1;
                    if in_degree[dep] == 0 {
                        queue.push_back(dep);
                    }
                }
            }
        }

        // If there was a cycle (shouldn't happen after checker), append remaining
        for i in 0..n {
            if !visited.contains(&i) {
                result.push(&steps[i]);
            }
        }

        result
    }

    fn dedent(&self, text: &str) -> String {
        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return String::new();
        }

        let min_indent = lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(0);

        lines
            .iter()
            .map(|l| {
                if l.len() >= min_indent {
                    &l[min_indent..]
                } else {
                    l.trim()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn extract_description(&self, skill: &Skill) -> Option<String> {
        let ctx = skill
            .body
            .contexts
            .iter()
            .max_by_key(|c| c.priority.unwrap_or(0))
            .or_else(|| {
                skill
                    .body
                    .steps
                    .iter()
                    .flat_map(|s| s.contexts.iter())
                    .max_by_key(|c| c.priority.unwrap_or(0))
            })?;

        let dedented = self.dedent(&ctx.text);
        let full = dedented.lines().collect::<Vec<_>>().join(" ");
        let full = full.trim();

        if let Some(end) = full.find(". ") {
            Some(full[..=end].to_string())
        } else if full.ends_with('.') {
            Some(full.to_string())
        } else {
            full.lines().next().map(|l| l.trim().to_string())
        }
    }

    fn type_expr_to_string(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::String => "string".to_string(),
            TypeExpr::Int => "int".to_string(),
            TypeExpr::Float => "float".to_string(),
            TypeExpr::Bool => "bool".to_string(),
            TypeExpr::Array(inner) => format!("{}[]", self.type_expr_to_string(inner)),
            TypeExpr::Map(k, v) => format!(
                "map<{}, {}>",
                self.type_expr_to_string(k),
                self.type_expr_to_string(v)
            ),
            TypeExpr::Enum(variants) => format!("enum({})", variants.join(" | ")),
            TypeExpr::Named(name) => name.clone(),
        }
    }

    fn expr_to_string(&self, expr: &Expr) -> String {
        match expr {
            Expr::StringLit(s) => format!("\"{}\"", s),
            Expr::IntLit(n) => n.to_string(),
            Expr::FloatLit(f) => f.to_string(),
            Expr::BoolLit(b) => b.to_string(),
            Expr::Ident(name) => name.clone(),
            Expr::FieldAccess(obj, field) => {
                format!("{}.{}", self.expr_to_string(obj), field)
            }
            Expr::ArrayLit(items) => {
                let parts: Vec<String> = items.iter().map(|e| self.expr_to_string(e)).collect();
                format!("[{}]", parts.join(", "))
            }
            Expr::BinOp(lhs, op, rhs) => {
                let op_str = match op {
                    BinOp::Eq => "==",
                    BinOp::NotEq => "!=",
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::LtEq => "<=",
                    BinOp::GtEq => ">=",
                    BinOp::In => "in",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                };
                format!(
                    "{} {} {}",
                    self.expr_to_string(lhs),
                    op_str,
                    self.expr_to_string(rhs)
                )
            }
            Expr::Not(inner) => format!("!{}", self.expr_to_string(inner)),
            Expr::FnCall(name, args) => {
                let arg_parts: Vec<String> = args
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, self.expr_to_string(v)))
                    .collect();
                format!("{}({})", name, arg_parts.join(", "))
            }
            Expr::Interpolated(s) => format!("`{}`", s),
        }
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
        let compiler = SkillMdCompiler::new();
        compiler.compile(&ast.skills[0])
    }

    #[test]
    fn minimal_skill_output() {
        let md = compile(r#"skill "hello" { context { "Greet the user warmly." } }"#);
        assert!(md.contains("---"));
        assert!(md.contains("name: hello"));
        assert!(md.contains("Greet the user warmly."));
    }

    #[test]
    fn skill_with_input_output() {
        let md = compile(r#"
            skill "review" {
                input {
                    files: string[]
                    severity?: string
                }
                output {
                    summary: string
                }
                body {
                    context { "Review the code." }
                }
            }
        "#);
        assert!(md.contains("name: review"));
        assert!(md.contains("files"));
        assert!(md.contains("string[]"));
        assert!(md.contains("optional"));
        assert!(md.contains("Review the code."));
    }

    #[test]
    fn steps_become_sections() {
        let md = compile(r#"
            skill "test" {
                body {
                    context(priority: 100) { "You are a reviewer." }
                    step analyze {
                        context { "Analyze the code." }
                    }
                    step report {
                        requires analyze
                        emit output
                        context { "Write the report." }
                    }
                }
            }
        "#);
        assert!(md.contains("## Step: analyze"));
        assert!(md.contains("## Step: report"));
        assert!(md.contains("You are a reviewer."));
        assert!(md.contains("Analyze the code."));
        assert!(md.contains("Write the report."));
    }

    #[test]
    fn context_priority_ordering() {
        let md = compile(r#"
            skill "test" {
                body {
                    context(priority: 50) { "Low priority." }
                    context(priority: 90) { "High priority." }
                }
            }
        "#);
        let high_pos = md.find("High priority.").unwrap();
        let low_pos = md.find("Low priority.").unwrap();
        assert!(high_pos < low_pos, "Higher priority context should appear first");
    }

    #[test]
    fn lazy_context_in_output() {
        let md = compile(r#"
            skill "x" {
                body {
                    lazy context "docs" (priority: 50) {
                        summary "API reference."
                        ref "./api.md"
                    }
                    context { "Use the docs." }
                }
            }
        "#);
        assert!(md.contains("References"), "should have References section: {}", md);
        assert!(md.contains("docs"), "should mention docs: {}", md);
        assert!(md.contains("API reference"), "should include summary: {}", md);
        assert!(md.contains("./api.md"), "should include ref path: {}", md);
    }

    #[test]
    fn tools_in_output() {
        let md = compile(r#"
            skill "x" {
                tools {
                    require Bash
                    require mcp("github") {
                        pr_diff(repo: string, pr: int) -> string
                    }
                }
                body { context { "ok" } }
            }
        "#);
        assert!(md.contains("Tools"), "should have Tools section: {}", md);
        assert!(md.contains("Bash"), "should mention Bash: {}", md);
        assert!(md.contains("github"), "should mention github MCP: {}", md);
        assert!(md.contains("pr_diff"), "should list pr_diff method: {}", md);
    }

    #[test]
    fn prompt_directives_in_output() {
        let md = compile(r#"
            skill "x" {
                body {
                    reasoning extended
                    persona { "You are an expert." }
                    context { "Do stuff." }
                }
            }
        "#);
        assert!(md.contains("Reasoning mode"), "should have Reasoning mode: {}", md);
        assert!(md.contains("extended"), "should say extended: {}", md);
        assert!(md.contains("You are an expert"), "should include persona: {}", md);
    }

    #[test]
    fn persona_dedented_in_output() {
        let md = compile(r#"
            skill "x" {
                body {
                    persona {
                        """
                        You are an expert.
                        You value simplicity.
                        """
                    }
                    context { "ok" }
                }
            }
        "#);
        assert!(md.contains("> You are an expert."));
        assert!(md.contains("> You value simplicity."));
        assert!(!md.contains(">             You")); // no leftover indentation
    }

    #[test]
    fn quantifier_assertions_in_output() {
        let md = compile(r#"
            skill "x" {
                body { context { "ok" } }
                tests {
                    test "q" {
                        given { x: "y" }
                        expect {
                            output.items: contains(where: .status == "active")
                        }
                    }
                }
            }
        "#);
        assert!(md.contains("contains(where:"));
    }

    #[test]
    fn tests_in_compiled_output() {
        let md = compile(r#"
            skill "x" {
                body { context { "ok" } }
                tests {
                    test "basic" {
                        given {
                            query: "hello"
                        }
                        expect {
                            output.result: equals("world")
                        }
                    }
                }
            }
        "#);
        assert!(md.contains("## Tests"), "should have Tests section: {}", md);
        assert!(md.contains("basic"), "should contain test name: {}", md);
        assert!(md.contains("query"), "should contain given field: {}", md);
        assert!(md.contains("equals"), "should contain assertion: {}", md);
    }

    #[test]
    fn tests_with_mocks_and_confidence_compiled() {
        let md = compile(r#"
            skill "x" {
                body { context { "ok" } }
                tests {
                    test "complex" {
                        given {
                            files: ["a.py"]
                        }
                        mock tools.mcp.slack: unavailable
                        mock tools.mcp.github {
                            pr_diff(repo: "org/app") -> "diff content"
                        }
                        expect {
                            output.status: equals("done")
                            output.count: >= 1
                        }
                        confidence 0.9
                        runs 10
                    }
                }
            }
        "#);
        assert!(md.contains("## Tests"), "should have Tests section: {}", md);
        assert!(md.contains("complex"), "should contain test name: {}", md);
        assert!(md.contains("unavailable"), "should contain mock type: {}", md);
        assert!(md.contains("custom responses"), "should describe mock responses: {}", md);
        assert!(md.contains("0.9"), "should contain confidence: {}", md);
        assert!(md.contains("10 runs"), "should contain runs: {}", md);
    }

    #[test]
    fn pipeline_compiles() {
        let tokens = Lexer::new(r#"
            pipeline "review" {
                input { repo: string }
                stage lint { use linter(repo: input.repo) }
                stage check {
                    requires lint
                    use checker(results: lint.result)
                }
                timeout 30m
            }
        "#).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = SkillMdCompiler::new();
        let md = compiler.compile_pipeline(&ast.pipelines[0]);
        assert!(md.contains("Pipeline: review"), "should have pipeline title: {}", md);
        assert!(md.contains("Stage: lint"), "should have lint stage: {}", md);
        assert!(md.contains("Stage: check"), "should have check stage: {}", md);
        assert!(md.contains("30m"), "should include timeout: {}", md);
    }
}
