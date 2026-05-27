use crate::ast::*;
use std::collections::{HashMap, HashSet, VecDeque};

pub struct SkillMdCompiler;

impl Default for SkillMdCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillMdCompiler {
    pub fn new() -> Self {
        Self
    }

    pub fn compile(&self, skill: &Skill, source: &SourceFile) -> String {
        let mut out = String::new();

        // ── Resolution ───────────────────────────────────────────────────
        let ancestors = self.resolve_ancestry(skill, source);

        let included_mixins: Vec<&Mixin> = skill.includes.iter()
            .filter_map(|name| source.mixins.iter().find(|m| &m.name == name))
            .collect();

        let mut acc_input: Option<Vec<Field>> = None;
        let mut acc_output: Option<Vec<Field>> = None;
        for ancestor in &ancestors {
            acc_input = self.merge_fields(acc_input.as_deref(), ancestor.input.as_deref());
            acc_output = self.merge_fields(acc_output.as_deref(), ancestor.output.as_deref());
        }
        let merged_input = self.merge_fields(acc_input.as_deref(), skill.input.as_deref());
        let merged_output = self.merge_fields(acc_output.as_deref(), skill.output.as_deref());

        // ── YAML frontmatter ──────────────────────────────────────────────
        out.push_str("---\n");
        out.push_str(&format!("name: {}\n", skill.name));

        if let Some(base) = &skill.extends {
            out.push_str(&format!("extends: {}\n", base));
        }

        if let Some(desc) = self.extract_description(skill, &ancestors) {
            out.push_str(&format!("description: \"{}\"\n", desc.replace('"', "\\\"")));
        }

        if let Some(input_fields) = &merged_input
            && !input_fields.is_empty() {
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

        out.push_str("---\n\n");

        // ── Title ─────────────────────────────────────────────────────────
        out.push_str(&format!("# {}\n\n", skill.name));

        // ── Output section ────────────────────────────────────────────────
        if let Some(output_fields) = &merged_output
            && !output_fields.is_empty() {
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

        // ── Preconditions (ancestors + child, with when_guard) ────────────
        let mut all_pre: Vec<&Assertion> = Vec::new();
        for ancestor in &ancestors {
            all_pre.extend(ancestor.pre.iter());
        }
        all_pre.extend(skill.pre.iter());

        if !all_pre.is_empty() {
            out.push_str("## Preconditions\n\n");
            for assertion in &all_pre {
                out.push_str(&self.emit_assertion(assertion));
            }
            out.push('\n');
        }

        // ── Postconditions (ancestors + child, with when_guard) ───────────
        let mut all_post: Vec<&Assertion> = Vec::new();
        for ancestor in &ancestors {
            all_post.extend(ancestor.post.iter());
        }
        all_post.extend(skill.post.iter());

        if !all_post.is_empty() {
            out.push_str("## Postconditions\n\n");
            for assertion in &all_post {
                out.push_str(&self.emit_assertion(assertion));
            }
            out.push('\n');
        }

        // ── Tools (merged ancestors + child) ──────────────────────────────
        let mut acc_tools: Option<ToolsBlock> = None;
        for ancestor in &ancestors {
            acc_tools = self.merge_tools(acc_tools.as_ref(), ancestor.tools.as_ref());
        }
        let merged_tools = self.merge_tools(acc_tools.as_ref(), skill.tools.as_ref());
        if let Some(tools) = &merged_tools {
            out.push_str(&self.emit_tools_section(tools));
        }

        // ── Permissions (merged ancestors + child) ────────────────────────
        let mut acc_perms: Option<PermissionsBlock> = None;
        for ancestor in &ancestors {
            acc_perms = self.merge_permissions(acc_perms.as_ref(), ancestor.permissions.as_ref());
        }
        let merged_perms = self.merge_permissions(acc_perms.as_ref(), skill.permissions.as_ref());
        if let Some(perms) = &merged_perms {
            out.push_str(&self.emit_permissions_section(perms));
        }

        // ── Prompt directives (child overrides ancestors field-by-field) ───
        let mut acc_directives = PromptDirectives::default();
        for ancestor in &ancestors {
            acc_directives = self.merge_directives(Some(&acc_directives), &ancestor.body.directives);
        }
        let merged_directives = self.merge_directives(Some(&acc_directives), &skill.body.directives);
        out.push_str(&self.emit_prompt_directives(&merged_directives));

        // ── Lazy contexts (merged ancestors + child, child wins) ──────────
        let child_lazy_names: HashSet<&str> = skill.body.lazy_contexts.iter()
            .map(|lc| lc.name.as_str()).collect();
        let mut all_lazy: Vec<LazyContext> = Vec::new();
        for ancestor in &ancestors {
            let ancestor_lazy_names: HashSet<&str> = ancestor.body.lazy_contexts.iter()
                .map(|lc| lc.name.as_str()).collect();
            all_lazy.retain(|lc| !ancestor_lazy_names.contains(lc.name.as_str()));
            for lc in &ancestor.body.lazy_contexts {
                if !child_lazy_names.contains(lc.name.as_str()) {
                    all_lazy.push(lc.clone());
                }
            }
        }
        all_lazy.extend(skill.body.lazy_contexts.iter().cloned());
        if !all_lazy.is_empty() {
            out.push_str(&self.emit_lazy_contexts(&all_lazy));
        }

        // ── Skill-level contexts (merged, sorted, with when/decay) ────────
        let mut all_contexts: Vec<&ContextBlock> = Vec::new();
        for ancestor in &ancestors {
            all_contexts.extend(ancestor.body.contexts.iter());
        }
        all_contexts.extend(skill.body.contexts.iter());
        for mixin in &included_mixins {
            all_contexts.extend(mixin.contexts.iter());
        }
        all_contexts.sort_by(|a, b| {
            let pa = a.priority.unwrap_or(Priority::Supplementary).rank();
            let pb = b.priority.unwrap_or(Priority::Supplementary).rank();
            pb.cmp(&pa)
        });

        for ctx in &all_contexts {
            out.push_str(&self.emit_context_with_metadata(ctx));
        }

        // ── Observability ─────────────────────────────────────────────────
        if let Some(observe) = &skill.body.observe {
            out.push_str(&self.emit_observability_section(observe));
        }

        // ── Tests ─────────────────────────────────────────────────────────
        if !skill.tests.is_empty() {
            out.push_str(&self.emit_tests_section(&skill.tests));
        }

        // ── Steps (merged: ancestors + mixin + child, topo sorted) ────────
        // Child steps win on name collision; later ancestors override earlier.
        let child_names: HashSet<&str> = skill.body.steps.iter()
            .map(|s| s.name.as_str())
            .collect();
        let mut all_steps: Vec<Step> = Vec::new();
        for ancestor in &ancestors {
            let ancestor_names: HashSet<&str> = ancestor.body.steps.iter()
                .map(|s| s.name.as_str()).collect();
            all_steps.retain(|s| !ancestor_names.contains(s.name.as_str()));
            for step in &ancestor.body.steps {
                if !child_names.contains(step.name.as_str()) {
                    all_steps.push(step.clone());
                }
            }
        }
        for mixin in &included_mixins {
            for step in &mixin.steps {
                if !child_names.contains(step.name.as_str()) {
                    all_steps.push(step.clone());
                }
            }
        }
        all_steps.extend(skill.body.steps.iter().cloned());

        let sorted_steps = self.topo_sort(&all_steps);

        let mut visited = HashSet::new();
        visited.insert(skill.name.clone());

        for step in sorted_steps {
            out.push_str(&format!("## Step: {}\n\n", step.name));

            let expired: Vec<&&ContextBlock> = all_contexts.iter()
                .filter(|c| c.until.as_deref() == Some(step.name.as_str()))
                .collect();
            if !expired.is_empty() {
                out.push_str("*The following setup context is no longer active after this step:*\n\n");
                for ctx in &expired {
                    let snippet = ctx.text.trim();
                    let snippet = if snippet.len() > 80 { &snippet[..80] } else { snippet };
                    out.push_str(&format!("- ~{}~\n", snippet));
                }
                out.push('\n');
            }

            if let Some(use_call) = &step.use_call {
                out.push_str(&format!("*Uses: {}*\n\n", use_call.skill_name));
                out.push_str(&self.expand_use_target(use_call, source, &mut visited));
            }

            if step.emit {
                out.push_str("*Produces final output.*\n\n");
            }

            for load_name in &step.loads {
                out.push_str(&format!("*Loads reference: {}*\n\n", load_name));
            }

            let mut step_contexts: Vec<&ContextBlock> = step.contexts.iter().collect();
            step_contexts.sort_by(|a, b| {
                let pa = a.priority.unwrap_or(Priority::Supplementary).rank();
                let pb = b.priority.unwrap_or(Priority::Supplementary).rank();
                pb.cmp(&pa)
            });

            for ctx in &step_contexts {
                out.push_str(&self.emit_context_with_metadata(ctx));
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
        if let Some(input_fields) = &pipeline.input
            && !input_fields.is_empty() {
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

        if let Some(output_fields) = &pipeline.output
            && !output_fields.is_empty() {
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
        if let Some(input_fields) = &orch.input
            && !input_fields.is_empty() {
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

        if let Some(output_fields) = &orch.output
            && !output_fields.is_empty() {
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

    // ── Context / assertion emitters with metadata ─────────────────────────────

    fn emit_context_with_metadata(&self, ctx: &ContextBlock) -> String {
        let mut out = String::new();

        if let Some(when) = &ctx.when {
            out.push_str(&format!(
                "**Condition:** `{}`\n\n",
                self.expr_to_string(when)
            ));
        }

        if let Some(decay) = ctx.decay {
            out.push_str(&format!("*Decay: {}*\n\n", decay));
        }

        if let Some(until) = &ctx.until {
            out.push_str(&format!("*Active until step `{}` is complete.*\n\n", until));
        }

        match ctx.priority {
            Some(Priority::Critical) => {
                out.push_str("> **CRITICAL:** ");
                out.push_str(&self.dedent(&ctx.text));
                out.push_str("\n\n");
            }
            Some(Priority::Important) => {
                out.push_str("> **IMPORTANT:** ");
                out.push_str(&self.dedent(&ctx.text));
                out.push_str("\n\n");
            }
            Some(Priority::Optional) => {
                out.push_str("*Optional context:* ");
                out.push_str(&self.dedent(&ctx.text));
                out.push_str("\n\n");
            }
            _ => {
                out.push_str(&self.dedent(&ctx.text));
                out.push_str("\n\n");
            }
        }

        out
    }

    // ── Use-call inline expansion ───────────────────────────────────────────────

    fn expand_use_target(
        &self,
        use_call: &UseCall,
        source: &SourceFile,
        visited: &mut HashSet<String>,
    ) -> String {
        let target_name = use_call.skill_name.replace('_', "-");

        if !visited.insert(target_name.clone()) {
            return String::new();
        }

        let target = match source.skills.iter().find(|s| s.name == target_name) {
            Some(s) => s,
            None => {
                visited.remove(&target_name);
                return String::new();
            }
        };

        let binding = self.build_binding_map(use_call);
        let mut out = String::new();

        // Inline target's body-level contexts
        let mut body_ctxs: Vec<&ContextBlock> = target.body.contexts.iter().collect();
        body_ctxs.sort_by(|a, b| {
            b.priority.unwrap_or(Priority::Supplementary).rank().cmp(&a.priority.unwrap_or(Priority::Supplementary).rank())
        });
        for ctx in &body_ctxs {
            out.push_str(&self.emit_context_with_binding(ctx, &binding));
        }

        // Inline target's steps
        let sorted_steps = self.topo_sort(&target.body.steps);
        for step in sorted_steps {
            out.push_str(&format!("### {}\n\n", step.name));

            if let Some(nested_use) = &step.use_call {
                out.push_str(&format!("*Uses: {}*\n\n", nested_use.skill_name));
                out.push_str(&self.expand_use_target(nested_use, source, visited));
            }

            if step.emit {
                out.push_str("*Produces final output.*\n\n");
            }

            for load_name in &step.loads {
                out.push_str(&format!("*Loads reference: {}*\n\n", load_name));
            }

            let mut step_contexts: Vec<&ContextBlock> = step.contexts.iter().collect();
            step_contexts.sort_by(|a, b| {
                b.priority.unwrap_or(Priority::Supplementary).rank().cmp(&a.priority.unwrap_or(Priority::Supplementary).rank())
            });
            for ctx in &step_contexts {
                out.push_str(&self.emit_context_with_binding(ctx, &binding));
            }
        }

        visited.remove(&target_name);
        out
    }

    fn build_binding_map(&self, use_call: &UseCall) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for (param_name, expr) in &use_call.args {
            map.insert(param_name.clone(), self.expr_to_string(expr));
        }
        map
    }

    fn apply_binding_to_string(&self, s: &str, binding: &HashMap<String, String>) -> String {
        let mut result = s.to_string();
        for (param, replacement) in binding {
            let pattern = format!("input.{}", param);
            result = result.replace(&pattern, replacement);
        }
        result
    }

    fn emit_context_with_binding(
        &self,
        ctx: &ContextBlock,
        binding: &HashMap<String, String>,
    ) -> String {
        let mut out = String::new();
        if let Some(when) = &ctx.when {
            let expr_str = self.expr_to_string(when);
            let bound = self.apply_binding_to_string(&expr_str, binding);
            out.push_str(&format!("**Condition:** `{}`\n\n", bound));
        }
        if let Some(decay) = ctx.decay {
            out.push_str(&format!("*Decay: {}*\n\n", decay));
        }
        let text = self.apply_binding_to_string(&self.dedent(&ctx.text), binding);
        out.push_str(&text);
        out.push_str("\n\n");
        out
    }

    fn emit_assertion(&self, assertion: &Assertion) -> String {
        if let Some(guard) = &assertion.when_guard {
            format!(
                "- **When** `{}`: {} — *{}*\n",
                self.expr_to_string(guard),
                self.expr_to_string(&assertion.condition),
                assertion.message
            )
        } else {
            format!(
                "- {} — *{}*\n",
                self.expr_to_string(&assertion.condition),
                assertion.message
            )
        }
    }

    // ── Ancestry resolution ─────────────────────────────────────────────────

    fn resolve_ancestry<'a>(&self, skill: &'a Skill, source: &'a SourceFile) -> Vec<&'a Skill> {
        crate::ast::resolve_ancestry(skill, &source.skills)
    }

    // ── Merge helpers for extends resolution ─────────────────────────────────

    fn merge_fields(&self, base: Option<&[Field]>, child: Option<&[Field]>) -> Option<Vec<Field>> {
        match (base, child) {
            (None, None) => None,
            (None, Some(c)) => Some(c.to_vec()),
            (Some(b), None) => Some(b.to_vec()),
            (Some(b), Some(c)) => {
                let child_names: HashSet<&str> =
                    c.iter().map(|f| f.name.as_str()).collect();
                let mut merged: Vec<Field> = b
                    .iter()
                    .filter(|f| !child_names.contains(f.name.as_str()))
                    .cloned()
                    .collect();
                merged.extend(c.iter().cloned());
                Some(merged)
            }
        }
    }

    fn merge_tools(
        &self,
        base: Option<&ToolsBlock>,
        child: Option<&ToolsBlock>,
    ) -> Option<ToolsBlock> {
        match (base, child) {
            (None, None) => None,
            (None, Some(c)) => Some(c.clone()),
            (Some(b), None) => Some(b.clone()),
            (Some(b), Some(c)) => {
                let child_req: HashSet<&str> =
                    c.required.iter().map(|t| t.name.as_str()).collect();
                let child_opt: HashSet<&str> =
                    c.optional.iter().map(|t| t.name.as_str()).collect();

                let mut merged = ToolsBlock {
                    required: b
                        .required
                        .iter()
                        .filter(|t| !child_req.contains(t.name.as_str()))
                        .cloned()
                        .collect(),
                    optional: b
                        .optional
                        .iter()
                        .filter(|t| !child_opt.contains(t.name.as_str()))
                        .cloned()
                        .collect(),
                };
                merged.required.extend(c.required.iter().cloned());
                merged.optional.extend(c.optional.iter().cloned());
                Some(merged)
            }
        }
    }

    fn merge_permissions(
        &self,
        base: Option<&PermissionsBlock>,
        child: Option<&PermissionsBlock>,
    ) -> Option<PermissionsBlock> {
        match (base, child) {
            (None, None) => None,
            (None, Some(c)) => Some(c.clone()),
            (Some(b), None) => Some(b.clone()),
            (Some(b), Some(c)) => {
                let mut secrets = b.secrets.clone();
                for s in &c.secrets {
                    if !secrets.contains(s) {
                        secrets.push(s.clone());
                    }
                }
                Some(PermissionsBlock {
                    filesystem: c.filesystem.clone().or_else(|| b.filesystem.clone()),
                    network: c.network.clone().or_else(|| b.network.clone()),
                    secrets,
                })
            }
        }
    }

    fn merge_directives(
        &self,
        base: Option<&PromptDirectives>,
        child: &PromptDirectives,
    ) -> PromptDirectives {
        let base = match base {
            Some(b) => b,
            None => return child.clone(),
        };

        PromptDirectives {
            reasoning: child.reasoning.clone().or_else(|| base.reasoning.clone()),
            persona: child.persona.clone().or_else(|| base.persona.clone()),
            sampling: child.sampling.clone().or_else(|| base.sampling.clone()),
            format: child.format.clone().or_else(|| base.format.clone()),
            reinforcements: if child.reinforcements.is_empty() {
                base.reinforcements.clone()
            } else {
                child.reinforcements.clone()
            },
            examples: if child.examples.is_empty() {
                base.examples.clone()
            } else {
                child.examples.clone()
            },
        }
    }

    // ── Phase 2 section emitters ──────────────────────────────────────────────

    fn emit_lazy_contexts(&self, lazy_contexts: &[LazyContext]) -> String {
        let mut out = String::new();
        out.push_str("## References (lazy-loaded)\n\n");

        // Sort by priority desc
        let mut sorted: Vec<&LazyContext> = lazy_contexts.iter().collect();
        sorted.sort_by(|a, b| {
            let pa = a.priority.unwrap_or(Priority::Supplementary).rank();
            let pb = b.priority.unwrap_or(Priority::Supplementary).rank();
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

    fn emit_observability_section(&self, observe: &ObserveBlock) -> String {
        let mut out = String::new();
        out.push_str("## Observability\n\n");

        if !observe.events.is_empty() {
            out.push_str("### Events\n\n");
            out.push_str("| Trigger | Event |\n");
            out.push_str("|---------|-------|\n");
            for event in &observe.events {
                out.push_str(&format!("| {} | {} |\n", event.trigger, event.event_name));
            }
            out.push('\n');
        }

        if !observe.metrics.is_empty() {
            out.push_str("### Metrics\n\n");
            out.push_str("| Metric | Source |\n");
            out.push_str("|--------|--------|\n");
            for metric in &observe.metrics {
                out.push_str(&format!("| {} | `{}` |\n", metric.name, self.expr_to_string(&metric.source)));
            }
            out.push('\n');
        }

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
            for (j, adj_j) in adj.iter_mut().enumerate() {
                if j != i {
                    adj_j.push(i);
                    in_degree[i] += 1;
                }
            }
        }

        // BFS (Kahn)
        let mut queue: VecDeque<usize> = VecDeque::new();
        for (i, &deg) in in_degree.iter().enumerate() {
            if deg == 0 {
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
        for (i, step) in steps.iter().enumerate() {
            if !visited.contains(&i) {
                result.push(step);
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

    fn extract_description(&self, skill: &Skill, ancestors: &[&Skill]) -> Option<String> {
        // Look through merged contexts (ancestors + child) for the highest-priority one
        let all_contexts: Vec<&ContextBlock> = ancestors
            .iter()
            .flat_map(|a| a.body.contexts.iter())
            .chain(skill.body.contexts.iter())
            .collect();

        let ctx = all_contexts
            .iter()
            .max_by_key(|c| c.priority.unwrap_or(Priority::Supplementary).rank())
            .copied()
            .or_else(|| {
                ancestors
                    .iter()
                    .flat_map(|a| a.body.steps.iter())
                    .chain(skill.body.steps.iter())
                    .flat_map(|s| s.contexts.iter())
                    .max_by_key(|c| c.priority.unwrap_or(Priority::Supplementary).rank())
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

impl crate::compiler::TargetCompiler for SkillMdCompiler {
    fn name(&self) -> &str { "skillmd" }
    fn file_extension(&self) -> &str { "md" }
    fn compile_skill(&self, skill: &Skill, source: &SourceFile) -> String {
        self.compile(skill, source)
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
        compiler.compile(&ast.skills[0], &ast)
    }

    fn compile_named(input: &str, name: &str) -> String {
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = SkillMdCompiler::new();
        let skill = ast.skills.iter().find(|s| s.name == name)
            .unwrap_or_else(|| panic!("skill '{}' not found", name));
        compiler.compile(skill, &ast)
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
                    context(priority: critical) { "You are a reviewer." }
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
                    context(priority: supplementary) { "Low priority." }
                    context(priority: important) { "High priority." }
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
                    lazy context "docs" (priority: supplementary) {
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

    // ── Fix #1: load directives emitted ─────────────────────────────

    #[test]
    fn step_loads_emitted() {
        let md = compile(r#"
            skill "x" {
                body {
                    lazy context "docs" (priority: supplementary) {
                        summary "API docs."
                        ref "./api.md"
                    }
                    context { "Use the docs." }
                    step main {
                        load "docs"
                        context { "Check the docs." }
                    }
                }
            }
        "#);
        assert!(
            md.contains("Loads reference: docs"),
            "compiled output should contain load annotation: {}",
            md
        );
    }

    // ── Fix #2: when guards on context blocks ───────────────────────

    #[test]
    fn context_when_guard_emitted() {
        let md = compile(r#"
            skill "x" {
                input { focus?: string }
                body {
                    context(priority: critical) { "Always included." }
                    context(priority: important, when: input.focus) {
                        "Focus on the requested area."
                    }
                }
            }
        "#);
        assert!(
            md.contains("Condition:"),
            "compiled output should contain condition annotation: {}",
            md
        );
        assert!(
            md.contains("input.focus"),
            "condition should reference the guard expression: {}",
            md
        );
    }

    // ── Fix #3: decay on context blocks ─────────────────────────────

    #[test]
    fn context_decay_emitted() {
        let md = compile(r#"
            skill "x" {
                body {
                    context(priority: important, decay: 0.5) { "Fading instruction." }
                }
            }
        "#);
        assert!(
            md.contains("Decay: 0.5"),
            "compiled output should contain decay annotation: {}",
            md
        );
    }

    // ── Fix #4: extends resolves base skill ─────────────────────────

    #[test]
    fn extends_merges_base_skill() {
        let input = r#"
            skill "base" {
                input { files: string[] }
                pre {
                    assert input.files != [] message "No files"
                }
                body {
                    context(priority: critical) { "Base instructions." }
                    step analyze {
                        context { "Analyze." }
                    }
                }
            }
            skill "child" extends "base" {
                input { severity?: string }
                body {
                    context(priority: important) { "Child-specific." }
                    step report {
                        requires analyze
                        emit output
                        context { "Report." }
                    }
                }
            }
        "#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = SkillMdCompiler::new();
        let child = ast.skills.iter().find(|s| s.name == "child").unwrap();
        let md = compiler.compile(child, &ast);

        assert!(md.contains("extends: base"), "frontmatter should note extends: {}", md);
        assert!(md.contains("Base instructions."), "should include base context: {}", md);
        assert!(md.contains("Child-specific."), "should include child context: {}", md);
        assert!(md.contains("Step: analyze"), "should include base steps: {}", md);
        assert!(md.contains("Step: report"), "should include child steps: {}", md);
        assert!(md.contains("No files"), "should include base preconditions: {}", md);
        assert!(md.contains("files"), "should inherit base input fields: {}", md);
        assert!(md.contains("severity"), "should have child input fields: {}", md);
    }

    #[test]
    fn extends_three_level_chain() {
        let input = r#"
            skill "grandparent" {
                input { x: string }
                body {
                    context(priority: critical) { "Grandparent context." }
                    step gp_step {
                        context { "GP step." }
                    }
                }
            }
            skill "parent" extends "grandparent" {
                input { y: string }
                body {
                    context(priority: important) { "Parent context." }
                    step p_step {
                        context { "Parent step." }
                    }
                }
            }
            skill "child" extends "parent" {
                input { z: string }
                body {
                    context(priority: important) { "Child context." }
                    step c_step {
                        requires gp_step
                        context { "Child step." }
                    }
                }
            }
        "#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = SkillMdCompiler::new();
        let child = ast.skills.iter().find(|s| s.name == "child").unwrap();
        let md = compiler.compile(child, &ast);

        assert!(md.contains("x"), "should inherit grandparent input field 'x': {}", md);
        assert!(md.contains("y"), "should inherit parent input field 'y': {}", md);
        assert!(md.contains("z"), "should have child input field 'z': {}", md);
        assert!(md.contains("Grandparent context."), "should inherit grandparent context: {}", md);
        assert!(md.contains("Parent context."), "should inherit parent context: {}", md);
        assert!(md.contains("Child context."), "should have child context: {}", md);
        assert!(md.contains("Step: gp_step"), "should inherit grandparent step: {}", md);
        assert!(md.contains("Step: p_step"), "should inherit parent step: {}", md);
        assert!(md.contains("Step: c_step"), "should have child step: {}", md);
    }

    #[test]
    fn extends_three_level_step_override() {
        let input = r#"
            skill "grandparent" {
                body {
                    step shared {
                        context { "GP version." }
                    }
                }
            }
            skill "parent" extends "grandparent" {
                body {
                    step shared {
                        context { "Parent version." }
                    }
                }
            }
            skill "child" extends "parent" {
                body {
                    context { "Child." }
                }
            }
        "#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = SkillMdCompiler::new();
        let child = ast.skills.iter().find(|s| s.name == "child").unwrap();
        let md = compiler.compile(child, &ast);

        assert!(md.contains("Parent version."), "parent should override grandparent step: {}", md);
        assert!(!md.contains("GP version."), "grandparent step should be overridden: {}", md);
        assert_eq!(md.matches("Step: shared").count(), 1, "should have exactly one 'shared' step");
    }

    #[test]
    fn extends_three_level_directives() {
        let input = r#"
            skill "grandparent" {
                body {
                    persona { "GP persona." }
                    reasoning extended
                    context { "GP." }
                }
            }
            skill "parent" extends "grandparent" {
                body {
                    context { "Parent." }
                }
            }
            skill "child" extends "parent" {
                body {
                    context { "Child." }
                }
            }
        "#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = SkillMdCompiler::new();
        let child = ast.skills.iter().find(|s| s.name == "child").unwrap();
        let md = compiler.compile(child, &ast);

        assert!(md.contains("GP persona"), "grandparent persona should propagate through 2 levels: {}", md);
        assert!(md.contains("extended"), "grandparent reasoning should propagate: {}", md);
    }

    #[test]
    fn extends_lazy_context_child_wins() {
        let input = r#"
            skill "base" {
                body {
                    lazy context "docs" (priority: supplementary) {
                        summary "Base docs."
                        ref "./base-api.md"
                    }
                    context { "Base." }
                }
            }
            skill "child" extends "base" {
                body {
                    lazy context "docs" (priority: important) {
                        summary "Child docs."
                        ref "./child-api.md"
                    }
                    context { "Child." }
                }
            }
        "#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = SkillMdCompiler::new();
        let child = ast.skills.iter().find(|s| s.name == "child").unwrap();
        let md = compiler.compile(child, &ast);

        assert!(md.contains("Child docs."), "child lazy context should appear: {}", md);
        assert!(!md.contains("Base docs."), "base lazy context should be overridden: {}", md);
        assert_eq!(md.matches("docs").count() - md.matches("Child docs").count(),
            md.matches("docs").count() - md.matches("Child docs").count(),
            "should have exactly one 'docs' lazy context entry");
    }

    #[test]
    fn extends_merges_directives() {
        let input = r#"
            skill "base" {
                body {
                    persona { "You are the base persona." }
                    reasoning extended
                    context { "Base." }
                }
            }
            skill "child" extends "base" {
                body {
                    context { "Child." }
                }
            }
        "#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = SkillMdCompiler::new();
        let child = ast.skills.iter().find(|s| s.name == "child").unwrap();
        let md = compiler.compile(child, &ast);

        assert!(
            md.contains("base persona"),
            "child should inherit base persona: {}",
            md
        );
        assert!(
            md.contains("extended"),
            "child should inherit base reasoning: {}",
            md
        );
    }

    #[test]
    fn extends_child_overrides_directives() {
        let input = r#"
            skill "base" {
                body {
                    persona { "Base persona." }
                    reasoning extended
                    context { "Base." }
                }
            }
            skill "child" extends "base" {
                body {
                    persona { "Child persona." }
                    context { "Child." }
                }
            }
        "#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = SkillMdCompiler::new();
        let child = ast.skills.iter().find(|s| s.name == "child").unwrap();
        let md = compiler.compile(child, &ast);

        assert!(
            md.contains("Child persona"),
            "child should override base persona: {}",
            md
        );
        assert!(
            !md.contains("Base persona"),
            "base persona should NOT appear: {}",
            md
        );
        assert!(
            md.contains("extended"),
            "non-overridden directives should inherit: {}",
            md
        );
    }

    #[test]
    fn mixin_step_name_collision_child_wins() {
        let input = r#"
            mixin defaults {
                step analyze {
                    context { "Mixin analyze." }
                }
            }
            skill "x" {
                include defaults
                body {
                    step analyze {
                        context { "Skill's own analyze." }
                    }
                }
            }
        "#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = SkillMdCompiler::new();
        let md = compiler.compile(&ast.skills[0], &ast);

        assert!(
            md.contains("Skill's own analyze"),
            "child step should win: {}",
            md
        );
        assert!(
            !md.contains("Mixin analyze"),
            "mixin step should be overridden: {}",
            md
        );
        let count = md.matches("Step: analyze").count();
        assert_eq!(count, 1, "should have exactly one analyze step, got {}", count);
    }

    // ── Fix #5: mixin include injects actual content ────────────────

    #[test]
    fn mixin_include_injects_steps() {
        let input = r#"
            mixin logging {
                step log_start {
                    context { "Log that the skill is starting." }
                }
                step log_end {
                    requires all_steps
                    context { "Log that the skill completed." }
                }
            }
            skill "x" {
                include logging
                body {
                    context { "Do the work." }
                    step main {
                        emit output
                        context { "Main work." }
                    }
                }
            }
        "#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = SkillMdCompiler::new();
        let md = compiler.compile(&ast.skills[0], &ast);

        assert!(
            md.contains("Step: log_start"),
            "should inject mixin step log_start: {}",
            md
        );
        assert!(
            md.contains("Step: log_end"),
            "should inject mixin step log_end: {}",
            md
        );
        assert!(
            md.contains("Log that the skill is starting"),
            "should include mixin step content: {}",
            md
        );
        assert!(
            !md.contains("*Includes mixin:"),
            "should NOT emit cosmetic include note: {}",
            md
        );
    }

    #[test]
    fn mixin_include_injects_contexts() {
        let input = r#"
            mixin safety {
                context(priority: critical) { "Always check for safety issues." }
            }
            skill "x" {
                include safety
                body {
                    context(priority: important) { "Review code." }
                }
            }
        "#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = SkillMdCompiler::new();
        let md = compiler.compile(&ast.skills[0], &ast);

        assert!(
            md.contains("Always check for safety issues"),
            "should inject mixin context: {}",
            md
        );
        let body_start = md.find("# x").unwrap();
        let body = &md[body_start..];
        let safety_pos = body.find("safety issues").unwrap();
        let review_pos = body.find("Review code").unwrap();
        assert!(
            safety_pos < review_pos,
            "higher-priority mixin context should appear first: {}",
            md
        );
    }

    // ── Fix #9: pre/post when_guard emitted ─────────────────────────

    #[test]
    fn assertion_when_guard_emitted() {
        let md = compile(r#"
            skill "x" {
                input { focus?: string }
                pre {
                    assert when input.focus input.focus != "" message "Focus cannot be empty"
                }
                body { context { "ok" } }
            }
        "#);
        assert!(
            md.contains("**When**"),
            "should contain When annotation: {}",
            md
        );
        assert!(
            md.contains("input.focus"),
            "should reference guard expression: {}",
            md
        );
        assert!(
            md.contains("Focus cannot be empty"),
            "should contain assertion message: {}",
            md
        );
    }

    #[test]
    fn extends_description_uses_base_context() {
        let md = compile_named(r#"
            skill "base" {
                body {
                    context(priority: critical) {
                        "Review code for bugs and security issues."
                    }
                }
            }
            skill "child" extends "base" {
                body {
                    context(priority: supplementary, when: input.focus) {
                        "Focus on the specified severity."
                    }
                }
            }
        "#, "child");

        assert!(
            md.contains("Review code for bugs and security issues"),
            "description should come from base's high-priority context, not child's conditional: {}",
            md
        );
    }

    // ── Step B: inline use-target expansion ──────────────────────────────

    #[test]
    fn use_call_inlines_target_steps() {
        let md = compile_named(r#"
            skill "sub-skill" {
                body {
                    step alpha {
                        context(priority: important) { "Alpha instruction." }
                    }
                    step beta {
                        requires alpha
                        context(priority: important) { "Beta instruction." }
                    }
                }
            }
            skill "main" {
                body {
                    step do_work {
                        use sub_skill()
                        context { "Wrapper context." }
                    }
                }
            }
        "#, "main");
        assert!(
            md.contains("Alpha instruction."),
            "should inline target's alpha step content: {}", md
        );
        assert!(
            md.contains("Beta instruction."),
            "should inline target's beta step content: {}", md
        );
    }

    #[test]
    fn use_call_preserves_uses_annotation() {
        let md = compile_named(r#"
            skill "helper" {
                body {
                    step work {
                        context { "Do the work." }
                    }
                }
            }
            skill "main" {
                body {
                    step invoke {
                        use helper()
                        context { "Call helper." }
                    }
                }
            }
        "#, "main");
        assert!(
            md.contains("*Uses: helper*"),
            "should preserve the Uses annotation: {}", md
        );
        assert!(
            md.contains("Do the work."),
            "should also inline the content: {}", md
        );
    }

    #[test]
    fn use_call_argument_binding_renames_references() {
        let md = compile_named(r#"
            skill "extract" {
                input {
                    paths: string[]
                }
                body {
                    step scan {
                        context(priority: important, when: input.paths) {
                            "Scan the provided paths for issues."
                        }
                    }
                }
            }
            skill "main" {
                input {
                    source_files: string[]
                }
                body {
                    step do_extract {
                        use extract(paths: input.source_files)
                        context { "Run extraction." }
                    }
                }
            }
        "#, "main");
        assert!(
            md.contains("input.source_files"),
            "should rewrite input.paths → input.source_files in condition: {}", md
        );
        assert!(
            !md.contains("input.paths"),
            "should NOT contain the callee's input.paths after binding: {}", md
        );
    }

    #[test]
    fn use_call_argument_binding_identity() {
        let md = compile_named(r#"
            skill "helper" {
                input {
                    files: string[]
                }
                body {
                    step scan {
                        context(priority: important, when: input.files) {
                            "Scan input.files for issues."
                        }
                    }
                }
            }
            skill "main" {
                input {
                    files: string[]
                }
                body {
                    step invoke {
                        use helper(files: input.files)
                        context { "Call helper." }
                    }
                }
            }
        "#, "main");
        assert!(
            md.contains("input.files"),
            "identity binding should preserve input.files: {}", md
        );
    }

    #[test]
    fn use_call_missing_target_graceful_fallback() {
        let md = compile(r#"
            skill "main" {
                body {
                    step invoke {
                        use nonexistent_skill()
                        context { "Call something." }
                    }
                }
            }
        "#);
        assert!(
            md.contains("*Uses: nonexistent_skill*"),
            "should fall back to annotation when target not found: {}", md
        );
    }

    #[test]
    fn use_call_cycle_guard() {
        let md = compile_named(r#"
            skill "a" {
                body {
                    step work {
                        use b()
                        context { "A does work." }
                    }
                }
            }
            skill "b" {
                body {
                    step work {
                        use a()
                        context { "B does work." }
                    }
                }
            }
        "#, "a");
        assert!(md.contains("A does work."), "should contain own content: {}", md);
        assert!(md.contains("B does work."), "should expand B once: {}", md);
    }

    #[test]
    fn use_call_nested_expansion() {
        let md = compile_named(r#"
            skill "c" {
                body {
                    step leaf {
                        context { "Leaf instruction." }
                    }
                }
            }
            skill "b" {
                body {
                    step mid {
                        use c()
                        context { "Middle instruction." }
                    }
                }
            }
            skill "a" {
                body {
                    step top {
                        use b()
                        context { "Top instruction." }
                    }
                }
            }
        "#, "a");
        assert!(
            md.contains("Leaf instruction."),
            "should recursively inline through a→b→c: {}", md
        );
    }

    #[test]
    fn use_call_same_skill_used_twice() {
        let md = compile_named(r#"
            skill "helper" {
                body {
                    step work {
                        context { "Helper work." }
                    }
                }
            }
            skill "main" {
                body {
                    step first {
                        use helper()
                        context { "First call." }
                    }
                    step second {
                        requires first
                        use helper()
                        context { "Second call." }
                    }
                }
            }
        "#, "main");
        let count = md.matches("Helper work.").count();
        assert_eq!(
            count, 2,
            "should inline helper twice (once per step): {}", md
        );
    }

    #[test]
    fn use_call_preserves_context_priority_ordering() {
        let md = compile_named(r#"
            skill "target" {
                body {
                    step work {
                        context(priority: supplementary) { "Low priority." }
                        context(priority: important) { "High priority." }
                    }
                }
            }
            skill "main" {
                body {
                    step invoke {
                        use target()
                        context { "Wrapper." }
                    }
                }
            }
        "#, "main");
        let high_pos = md.find("High priority.").expect("should contain high priority");
        let low_pos = md.find("Low priority.").expect("should contain low priority");
        assert!(
            high_pos < low_pos,
            "higher priority context should appear first in inlined output: {}", md
        );
    }
}
