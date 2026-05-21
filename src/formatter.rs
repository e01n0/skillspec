use crate::ast::*;

pub struct Formatter {
    indent: usize,
    output: String,
}

impl Formatter {
    pub fn new() -> Self {
        Formatter {
            indent: 0,
            output: String::new(),
        }
    }

    pub fn format(file: &SourceFile) -> String {
        let mut f = Formatter::new();
        f.emit_source_file(file);
        f.output
    }

    fn push(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn indent_str(&self) -> String {
        "  ".repeat(self.indent)
    }

    fn line(&mut self, s: &str) {
        self.push(&self.indent_str());
        self.push(s);
        self.push("\n");
    }

    fn blank(&mut self) {
        self.push("\n");
    }

    // ── Top-level ────────────────────────────────────────────────────

    fn emit_source_file(&mut self, file: &SourceFile) {
        let mut first = true;

        for import in &file.imports {
            if !first { self.blank(); }
            first = false;
            self.emit_import(import);
        }

        for td in &file.type_defs {
            if !first { self.blank(); }
            first = false;
            self.emit_type_def(td);
        }

        for mixin in &file.mixins {
            if !first { self.blank(); }
            first = false;
            self.emit_mixin(mixin);
        }

        for skill in &file.skills {
            if !first { self.blank(); }
            first = false;
            self.emit_skill(skill);
        }

        for pipeline in &file.pipelines {
            if !first { self.blank(); }
            first = false;
            self.emit_pipeline(pipeline);
        }

        for orch in &file.orchestrations {
            if !first { self.blank(); }
            first = false;
            self.emit_orchestration(orch);
        }
    }

    // ── Import ───────────────────────────────────────────────────────

    fn emit_import(&mut self, import: &Import) {
        let syms = import.symbols.join(", ");
        self.line(&format!("import {{ {} }} from \"{}\"", syms, import.path));
    }

    // ── Type definition ──────────────────────────────────────────────

    fn emit_type_def(&mut self, td: &TypeDef) {
        self.line(&format!("type {} {{", td.name));
        self.indent += 1;
        for field in &td.fields {
            self.emit_field(field);
        }
        self.indent -= 1;
        self.line("}");
    }

    fn emit_field(&mut self, field: &Field) {
        let opt = if field.optional { "?" } else { "" };
        let ty = type_expr_to_string(&field.ty);
        if let Some(default) = &field.default {
            self.line(&format!("{}{}: {} = {}", field.name, opt, ty, expr_to_string(default)));
        } else {
            self.line(&format!("{}{}: {}", field.name, opt, ty));
        }
    }

    // ── Mixin ────────────────────────────────────────────────────────

    fn emit_mixin(&mut self, mixin: &Mixin) {
        self.line(&format!("mixin {} {{", mixin.name));
        self.indent += 1;
        for ctx in &mixin.contexts {
            self.emit_context_block(ctx);
        }
        for step in &mixin.steps {
            self.emit_step(step);
        }
        self.indent -= 1;
        self.line("}");
    }

    // ── Skill ────────────────────────────────────────────────────────

    fn emit_skill(&mut self, skill: &Skill) {
        let extends = if let Some(ref base) = skill.extends {
            format!(" extends \"{}\"", base)
        } else {
            String::new()
        };
        self.line(&format!("skill \"{}\"{} {{", skill.name, extends));
        self.indent += 1;

        // Canonical order: input, output, tools, permissions, include, pre, post, body, tests
        if let Some(input) = &skill.input {
            self.emit_fields_block("input", input);
        }
        if let Some(output) = &skill.output {
            self.emit_fields_block("output", output);
        }
        if let Some(tools) = &skill.tools {
            self.emit_tools_block(tools);
        }
        if let Some(perms) = &skill.permissions {
            self.emit_permissions_block(perms);
        }
        for inc in &skill.includes {
            self.line(&format!("include {}", inc));
        }
        if !skill.pre.is_empty() {
            self.emit_assertions_block("pre", &skill.pre);
        }
        if !skill.post.is_empty() {
            self.emit_assertions_block("post", &skill.post);
        }
        self.emit_body(&skill.body);
        if !skill.tests.is_empty() {
            self.emit_tests_block(&skill.tests);
        }

        self.indent -= 1;
        self.line("}");
    }

    fn emit_fields_block(&mut self, name: &str, fields: &[Field]) {
        self.line(&format!("{} {{", name));
        self.indent += 1;
        for field in fields {
            self.emit_field(field);
        }
        self.indent -= 1;
        self.line("}");
    }

    fn emit_tools_block(&mut self, tools: &ToolsBlock) {
        self.line("tools {");
        self.indent += 1;
        for tool in &tools.required {
            self.emit_tool_decl("require", tool);
        }
        for tool in &tools.optional {
            self.emit_tool_decl("optional", tool);
        }
        self.indent -= 1;
        self.line("}");
    }

    fn emit_tool_decl(&mut self, keyword: &str, tool: &ToolDecl) {
        match &tool.kind {
            ToolKind::Builtin | ToolKind::Generic => {
                self.line(&format!("{} {}", keyword, tool.name));
            }
            ToolKind::Mcp(server) => {
                if tool.methods.is_empty() {
                    self.line(&format!("{} mcp(\"{}\")", keyword, server));
                } else {
                    self.line(&format!("{} mcp(\"{}\") {{", keyword, server));
                    self.indent += 1;
                    for method in &tool.methods {
                        let params: Vec<String> = method.params.iter().map(|(name, ty, opt)| {
                            if *opt {
                                format!("{}?: {}", name, type_expr_to_string(ty))
                            } else {
                                format!("{}: {}", name, type_expr_to_string(ty))
                            }
                        }).collect();
                        self.line(&format!(
                            "{}({}) -> {}",
                            method.name,
                            params.join(", "),
                            type_expr_to_string(&method.return_type)
                        ));
                    }
                    self.indent -= 1;
                    self.line("}");
                }
            }
        }
    }

    fn emit_permissions_block(&mut self, perms: &PermissionsBlock) {
        self.line("permissions {");
        self.indent += 1;
        if let Some((mode, patterns)) = &perms.filesystem {
            let pats: Vec<String> = patterns.iter().map(|p| format!("\"{}\"", p)).collect();
            self.line(&format!("filesystem: {}({})", mode, pats.join(", ")));
        }
        if let Some((mode, hosts)) = &perms.network {
            let h: Vec<String> = hosts.iter().map(|h| format!("\"{}\"", h)).collect();
            self.line(&format!("network: {}({})", mode, h.join(", ")));
        }
        if !perms.secrets.is_empty() {
            let s: Vec<String> = perms.secrets.iter().map(|s| format!("\"{}\"", s)).collect();
            self.line(&format!("secrets: [{}]", s.join(", ")));
        }
        self.indent -= 1;
        self.line("}");
    }

    fn emit_assertions_block(&mut self, name: &str, assertions: &[Assertion]) {
        self.line(&format!("{} {{", name));
        self.indent += 1;
        for a in assertions {
            if let Some(guard) = &a.when_guard {
                self.line(&format!(
                    "assert when {} {} message \"{}\"",
                    expr_to_string(guard),
                    expr_to_string(&a.condition),
                    a.message
                ));
            } else {
                self.line(&format!(
                    "assert {} message \"{}\"",
                    expr_to_string(&a.condition),
                    a.message
                ));
            }
        }
        self.indent -= 1;
        self.line("}");
    }

    // ── Body ─────────────────────────────────────────────────────────

    fn emit_body(&mut self, body: &Body) {
        self.line("body {");
        self.indent += 1;

        // Directives first (persona, reasoning, sampling, format, reinforce, examples)
        let d = &body.directives;

        if let Some(persona) = &d.persona {
            self.emit_persona(persona);
        }
        if let Some(reasoning) = &d.reasoning {
            self.line(&format!("reasoning {}", reasoning));
        }
        if let Some(sampling) = &d.sampling {
            self.emit_sampling(sampling);
        }
        if let Some(fmt) = &d.format {
            self.emit_format(fmt);
        }
        for reinf in &d.reinforcements {
            self.emit_reinforcement(reinf);
        }
        if !d.examples.is_empty() {
            self.emit_examples(&d.examples);
        }

        // Contexts, lazy contexts, and steps in source order
        if !body.source_order.is_empty() {
            for item in &body.source_order {
                match item {
                    BodyItemRef::Context(i) => {
                        if let Some(ctx) = body.contexts.get(*i) {
                            self.emit_context_block(ctx);
                        }
                    }
                    BodyItemRef::LazyContext(i) => {
                        if let Some(lc) = body.lazy_contexts.get(*i) {
                            self.emit_lazy_context(lc);
                        }
                    }
                    BodyItemRef::Step(i) => {
                        if let Some(step) = body.steps.get(*i) {
                            self.emit_step(step);
                        }
                    }
                }
            }
        } else {
            // Fallback for programmatically constructed ASTs without source_order
            for ctx in &body.contexts {
                self.emit_context_block(ctx);
            }
            for lc in &body.lazy_contexts {
                self.emit_lazy_context(lc);
            }
            for step in &body.steps {
                self.emit_step(step);
            }
        }

        // on_error
        if let Some(on_error) = &body.on_error {
            self.line("on_error {");
            self.indent += 1;
            for call in on_error {
                self.emit_use_call(call);
            }
            self.indent -= 1;
            self.line("}");
        }

        self.indent -= 1;
        self.line("}");
    }

    fn emit_persona(&mut self, text: &str) {
        self.line("persona {");
        self.indent += 1;
        self.emit_text_block(text);
        self.indent -= 1;
        self.line("}");
    }

    fn emit_sampling(&mut self, sampling: &SamplingDirective) {
        self.line("sampling {");
        self.indent += 1;
        if let Some(t) = sampling.temperature {
            self.line(&format!("temperature: {}", format_f64(t)));
        }
        if let Some(p) = sampling.top_p {
            self.line(&format!("top_p: {}", format_f64(p)));
        }
        self.indent -= 1;
        self.line("}");
    }

    fn emit_format(&mut self, fmt: &FormatDirective) {
        self.line("format {");
        self.indent += 1;
        self.line(&format!("style: {}", fmt.style));
        self.line(&format!("structure: {}", fmt.structure));
        self.indent -= 1;
        self.line("}");
    }

    fn emit_reinforcement(&mut self, reinf: &Reinforcement) {
        let trigger = match &reinf.trigger {
            ReinforceTrigger::EveryNSteps(n) => format!("every {} steps", n),
            ReinforceTrigger::OnContextShift => "on context_shift".to_string(),
            ReinforceTrigger::WhenCondition(expr) => format!("when {}", expr_to_string(expr)),
        };
        self.line(&format!("reinforce {} {{", trigger));
        self.indent += 1;
        self.emit_text_block(&reinf.text);
        self.indent -= 1;
        self.line("}");
    }

    fn emit_examples(&mut self, examples: &[PromptExample]) {
        self.line("examples {");
        self.indent += 1;
        for ex in examples {
            self.line(&format!("example \"{}\" {{", ex.name));
            self.indent += 1;
            self.line(&format!("input: \"{}\"", escape_string(&ex.input)));
            self.line(&format!("output: \"{}\"", escape_string(&ex.output)));
            if let Some(note) = &ex.note {
                self.line(&format!("note: \"{}\"", escape_string(note)));
            }
            self.indent -= 1;
            self.line("}");
        }
        self.indent -= 1;
        self.line("}");
    }

    // ── Context ──────────────────────────────────────────────────────

    fn emit_context_block(&mut self, ctx: &ContextBlock) {
        let mut params = Vec::new();
        if let Some(p) = ctx.priority {
            params.push(format!("priority: {}", p));
        }
        if let Some(when) = &ctx.when {
            params.push(format!("when: {}", expr_to_string(when)));
        }
        if let Some(decay) = ctx.decay {
            params.push(format!("decay: {}", format_f64(decay)));
        }

        let params_str = if params.is_empty() {
            String::new()
        } else {
            format!("({})", params.join(", "))
        };

        self.line(&format!("context{} {{", params_str));
        self.indent += 1;
        self.emit_text_block(&ctx.text);
        self.indent -= 1;
        self.line("}");
    }

    fn emit_lazy_context(&mut self, lc: &LazyContext) {
        let priority = if let Some(p) = lc.priority {
            format!(" (priority: {})", p)
        } else {
            String::new()
        };

        self.line(&format!("lazy context \"{}\"{} {{", lc.name, priority));
        self.indent += 1;
        self.line(&format!("summary \"{}\"", escape_string(&lc.summary)));

        match &lc.content {
            LazyContent::Ref(path) => {
                self.line(&format!("ref \"{}\"", path));
            }
            LazyContent::Inline(text) => {
                self.emit_text_block(text);
            }
            LazyContent::Index(sections) => {
                self.line("index {");
                self.indent += 1;
                for section in sections {
                    self.line(&format!("section \"{}\" {{", section.name));
                    self.indent += 1;
                    self.line(&format!("summary \"{}\"", escape_string(&section.summary)));
                    self.line(&format!("ref \"{}\"", section.ref_path));
                    self.indent -= 1;
                    self.line("}");
                }
                self.indent -= 1;
                self.line("}");
            }
        }

        self.indent -= 1;
        self.line("}");
    }

    // ── Step ─────────────────────────────────────────────────────────

    fn emit_step(&mut self, step: &Step) {
        self.line(&format!("step {} {{", step.name));
        self.indent += 1;

        if let Some(dep) = &step.requires {
            self.line(&format!("requires {}", dep_to_string(dep)));
        }
        if let Some(when) = &step.when {
            self.line(&format!("when {}", expr_to_string(when)));
        }
        for load in &step.loads {
            self.line(&format!("load \"{}\"", load));
        }
        if let Some(use_call) = &step.use_call {
            self.emit_use_call(use_call);
        }
        for binding in &step.lets {
            self.line(&format!("let {} = {}", binding.name, expr_to_string(&binding.value)));
        }
        if step.emit {
            self.line("emit output");
        }
        for ctx in &step.contexts {
            self.emit_context_block(ctx);
        }

        self.indent -= 1;
        self.line("}");
    }

    fn emit_use_call(&mut self, call: &UseCall) {
        if call.args.is_empty() {
            self.line(&format!("use {}()", call.skill_name));
        } else {
            let args: Vec<String> = call.args.iter()
                .map(|(k, v)| format!("{}: {}", k, expr_to_string(v)))
                .collect();
            self.line(&format!("use {}({})", call.skill_name, args.join(", ")));
        }
    }

    // ── Pipeline ─────────────────────────────────────────────────────

    fn emit_pipeline(&mut self, pipeline: &Pipeline) {
        self.line(&format!("pipeline \"{}\" {{", pipeline.name));
        self.indent += 1;

        if let Some(input) = &pipeline.input {
            self.emit_fields_block("input", input);
        }
        if let Some(output) = &pipeline.output {
            self.emit_fields_block("output", output);
        }

        for stage in &pipeline.stages {
            self.line(&format!("stage {} {{", stage.name));
            self.indent += 1;
            if let Some(dep) = &stage.requires {
                self.line(&format!("requires {}", dep_to_string(dep)));
            }
            self.emit_use_call(&stage.use_call);
            self.indent -= 1;
            self.line("}");
        }

        if let Some(on_error) = &pipeline.on_error {
            self.line("on_error {");
            self.indent += 1;
            for call in on_error {
                self.emit_use_call(call);
            }
            self.indent -= 1;
            self.line("}");
        }

        if let Some(timeout) = &pipeline.timeout {
            self.line(&format!("timeout {}", timeout));
        }

        self.indent -= 1;
        self.line("}");
    }

    // ── Orchestration ────────────────────────────────────────────────

    fn emit_orchestration(&mut self, orch: &Orchestration) {
        self.line(&format!("orchestration \"{}\" {{", orch.name));
        self.indent += 1;

        if !orch.agents.is_empty() {
            self.line("agents {");
            self.indent += 1;
            for agent in &orch.agents {
                self.line(&format!(
                    "{}: agent(skill: \"{}\", model: \"{}\")",
                    agent.name, agent.skill, agent.model
                ));
            }
            self.indent -= 1;
            self.line("}");
        }

        if let Some(input) = &orch.input {
            self.emit_fields_block("input", input);
        }
        if let Some(output) = &orch.output {
            self.emit_fields_block("output", output);
        }

        for phase in &orch.phases {
            self.line(&format!("phase {} {{", phase.name));
            self.indent += 1;
            if let Some(dep) = &phase.requires {
                self.line(&format!("requires {}", dep_to_string(dep)));
            }
            for action in &phase.actions {
                if action.args.is_empty() {
                    self.line(&format!("{}.{}()", action.agent_name, action.method));
                } else {
                    let args: Vec<String> = action.args.iter()
                        .map(|(k, v)| format!("{}: {}", k, expr_to_string(v)))
                        .collect();
                    self.line(&format!(
                        "{}.{}({})",
                        action.agent_name, action.method, args.join(", ")
                    ));
                }
            }
            if let Some(emit) = &phase.emit {
                self.line(&format!("emit output from {}", emit));
            }
            self.indent -= 1;
            self.line("}");
        }

        if let Some(timeout) = &orch.timeout {
            self.line(&format!("timeout {}", timeout));
        }

        self.indent -= 1;
        self.line("}");
    }

    // ── Tests ────────────────────────────────────────────────────────

    fn emit_tests_block(&mut self, tests: &[TestBlock]) {
        self.line("tests {");
        self.indent += 1;
        for test in tests {
            self.line(&format!("test \"{}\" {{", test.name));
            self.indent += 1;

            if !test.given.is_empty() {
                self.line("given {");
                self.indent += 1;
                for (k, v) in &test.given {
                    self.line(&format!("{}: {}", k, expr_to_string(v)));
                }
                self.indent -= 1;
                self.line("}");
            }

            for mock in &test.mocks {
                self.emit_mock(mock);
            }

            if !test.expectations.is_empty() {
                self.line("expect {");
                self.indent += 1;
                for exp in &test.expectations {
                    self.line(&format!("{}: {}", exp.path, assertion_to_string(&exp.assertion)));
                }
                self.indent -= 1;
                self.line("}");
            }

            if let Some(c) = test.confidence {
                self.line(&format!("confidence {}", format_f64(c)));
            }
            if let Some(r) = test.runs {
                self.line(&format!("runs {}", r));
            }
            if let Some(s) = &test.snapshot {
                self.line(&format!("snapshot \"{}\"", s));
            }

            self.indent -= 1;
            self.line("}");
        }
        self.indent -= 1;
        self.line("}");
    }

    fn emit_mock(&mut self, mock: &MockDecl) {
        match &mock.mock_type {
            MockType::Unavailable => {
                self.line(&format!("mock {}: unavailable", mock.tool_path));
            }
            MockType::Failing(reason) => {
                if reason.is_empty() {
                    self.line(&format!("mock {}: failing", mock.tool_path));
                } else {
                    self.line(&format!("mock {}: failing \"{}\"", mock.tool_path, reason));
                }
            }
            MockType::Slow(duration) => {
                if duration.is_empty() {
                    self.line(&format!("mock {}: slow", mock.tool_path));
                } else {
                    self.line(&format!("mock {}: slow \"{}\"", mock.tool_path, duration));
                }
            }
            MockType::Responses(responses) => {
                self.line(&format!("mock {} {{", mock.tool_path));
                self.indent += 1;
                for resp in responses {
                    let args: Vec<String> = resp.args.iter()
                        .map(|(k, v)| format!("{}: {}", k, expr_to_string(v)))
                        .collect();
                    self.line(&format!(
                        "{}({}) -> {}",
                        resp.method,
                        args.join(", "),
                        expr_to_string(&resp.response)
                    ));
                }
                self.indent -= 1;
                self.line("}");
            }
        }
    }

    // ── Text helpers ─────────────────────────────────────────────────

    fn emit_text_block(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.contains('\n') || trimmed.len() > 60 {
            self.line("\"\"\"");
            for line in trimmed.lines() {
                self.line(line.trim_start());
            }
            self.line("\"\"\"");
        } else {
            self.line(&format!("\"{}\"", escape_string(trimmed)));
        }
    }
}

// ── Free functions ───────────────────────────────────────────────────

fn format_f64(f: f64) -> String {
    if f == f.floor() && f.abs() < 1e15 {
        format!("{:.1}", f)
    } else {
        format!("{}", f)
    }
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn type_expr_to_string(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::String => "string".to_string(),
        TypeExpr::Int => "int".to_string(),
        TypeExpr::Float => "float".to_string(),
        TypeExpr::Bool => "bool".to_string(),
        TypeExpr::Array(inner) => format!("{}[]", type_expr_to_string(inner)),
        TypeExpr::Map(k, v) => format!("map<{}, {}>", type_expr_to_string(k), type_expr_to_string(v)),
        TypeExpr::Enum(variants) => {
            let vs: Vec<String> = variants.iter().map(|v| format!("\"{}\"", v)).collect();
            format!("enum({})", vs.join(", "))
        }
        TypeExpr::Named(name) => name.clone(),
    }
}

fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::StringLit(s) => format!("\"{}\"", escape_string(s)),
        Expr::IntLit(n) => n.to_string(),
        Expr::FloatLit(f) => format_f64(*f),
        Expr::BoolLit(b) => b.to_string(),
        Expr::Ident(name) => name.clone(),
        Expr::FieldAccess(obj, field) => format!("{}.{}", expr_to_string(obj), field),
        Expr::ArrayLit(items) => {
            let parts: Vec<String> = items.iter().map(|e| expr_to_string(e)).collect();
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
            format!("{} {} {}", expr_to_string(lhs), op_str, expr_to_string(rhs))
        }
        Expr::Not(inner) => format!("!{}", expr_to_string(inner)),
        Expr::FnCall(name, args) => {
            let parts: Vec<String> = args.iter()
                .map(|(k, v)| format!("{}: {}", k, expr_to_string(v)))
                .collect();
            format!("{}({})", name, parts.join(", "))
        }
        Expr::Interpolated(s) => format!("`{}`", s),
    }
}

fn dep_to_string(dep: &Dependency) -> String {
    match dep {
        Dependency::Single(name) => name.clone(),
        Dependency::All(names) => names.join(" & "),
        Dependency::Any(names) => names.join(" | "),
        Dependency::AllSteps => "all_steps".to_string(),
    }
}

fn assertion_to_string(a: &AssertionExpr) -> String {
    match a {
        AssertionExpr::Equals(e) => format!("equals({})", expr_to_string(e)),
        AssertionExpr::Contains(e) => format!("contains({})", expr_to_string(e)),
        AssertionExpr::Matches(p) => format!("matches(\"{}\")", p),
        AssertionExpr::Resembles(d) => format!("resembles(\"{}\")", d),
        AssertionExpr::Satisfies(d) => format!("satisfies(\"{}\")", d),
        AssertionExpr::Between(lo, hi) => format!("between({}, {})", expr_to_string(lo), expr_to_string(hi)),
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
            format!("{} {}", op_str, expr_to_string(val))
        }
        AssertionExpr::ContainsWhere(expr) => {
            format!("contains(where: {})", expr_to_string(expr))
        }
        AssertionExpr::AllWhere(expr) => {
            format!("all(where: {})", expr_to_string(expr))
        }
        AssertionExpr::NoneWhere(expr) => {
            format!("none(where: {})", expr_to_string(expr))
        }
    }
}

/// Convenience: parse a source string and format it.
pub fn format_source(source: &str) -> Result<String, String> {
    let tokens = crate::lexer::Lexer::new(source)
        .tokenize()
        .map_err(|e| format!("Lex error: {}", e))?;
    let ast = crate::parser::Parser::new(tokens)
        .parse()
        .map_err(|e| format!("Parse error: {}", e))?;
    Ok(Formatter::format(&ast))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_minimal() {
        let input = r#"skill "hello" {
  context {
    "Greet warmly."
  }
}
"#;
        let formatted = format_source(input).unwrap();
        assert!(formatted.contains("skill \"hello\""));
        assert!(formatted.contains("context"));
        assert!(formatted.contains("Greet warmly."));
    }

    #[test]
    fn round_trip_parse_consistency() {
        // Parse, format, parse again — the second AST should have the same structure
        let input = r#"
            skill "test" {
                input { query: string }
                output { result: string }
                body {
                    context(priority: 80) { "You are helpful." }
                    step main {
                        emit output
                        context { "Answer the query." }
                    }
                }
            }
        "#;
        let formatted = format_source(input).unwrap();
        // Parse the formatted output — should not error
        let tokens2 = crate::lexer::Lexer::new(&formatted).tokenize().unwrap();
        let ast2 = crate::parser::Parser::new(tokens2).parse().unwrap();
        assert_eq!(ast2.skills.len(), 1);
        assert_eq!(ast2.skills[0].name, "test");
        assert_eq!(ast2.skills[0].body.steps.len(), 1);
    }

    #[test]
    fn formats_pipeline() {
        let input = r#"
            pipeline "review" {
                input { repo: string }
                stage lint { use linter(repo: input.repo) }
                stage check {
                    requires lint
                    use checker(results: lint.result)
                }
                timeout 30m
            }
        "#;
        let formatted = format_source(input).unwrap();
        assert!(formatted.contains("pipeline \"review\""));
        assert!(formatted.contains("stage lint"));
        assert!(formatted.contains("timeout 30m"));
    }

    #[test]
    fn formats_full_brainstorming() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/brainstorming.agent"))
            .unwrap();
        let formatted = format_source(&source).unwrap();
        // Should round-trip parse
        let tokens = crate::lexer::Lexer::new(&formatted).tokenize().unwrap();
        let ast = crate::parser::Parser::new(tokens).parse().unwrap();
        assert_eq!(ast.skills.len(), 1);
        assert_eq!(ast.skills[0].name, "brainstorming");
        assert_eq!(ast.pipelines.len(), 1);
    }

    #[test]
    fn canonical_section_ordering() {
        // input should come before body even if written in reverse
        let input = r#"
            skill "x" {
                body { context { "Do stuff." } }
                input { query: string }
            }
        "#;
        let formatted = format_source(input).unwrap();
        let input_pos = formatted.find("input {").unwrap();
        let body_pos = formatted.find("body {").unwrap();
        assert!(input_pos < body_pos, "input should come before body in canonical output");
    }

    // ── Round-trip property tests for fixtures ──────────────────────

    /// Assert that parse(fmt(source)) produces a structurally equivalent AST to parse(source).
    /// We can't compare ASTs directly (Span has no PartialEq), so we compare structural properties.
    fn assert_round_trips(source: &str) {
        let tokens1 = crate::lexer::Lexer::new(source).tokenize().unwrap();
        let ast1 = crate::parser::Parser::new(tokens1).parse().unwrap();
        let formatted = Formatter::format(&ast1);

        let tokens2 = crate::lexer::Lexer::new(&formatted).tokenize()
            .unwrap_or_else(|e| {
                for (i, line) in formatted.lines().enumerate() {
                    eprintln!("{:>4}: {}", i + 1, line);
                }
                panic!("Lexer failed on formatted output: {}", e);
            });
        let ast2 = crate::parser::Parser::new(tokens2).parse()
            .unwrap_or_else(|e| {
                for (i, line) in formatted.lines().enumerate() {
                    eprintln!("{:>4}: {}", i + 1, line);
                }
                panic!("Parser failed on formatted output: {}", e);
            });

        // Same number of top-level constructs
        assert_eq!(ast1.skills.len(), ast2.skills.len(), "skill count mismatch");
        assert_eq!(ast1.pipelines.len(), ast2.pipelines.len(), "pipeline count mismatch");
        assert_eq!(ast1.orchestrations.len(), ast2.orchestrations.len(), "orchestration count mismatch");
        assert_eq!(ast1.type_defs.len(), ast2.type_defs.len(), "type_defs count mismatch");
        assert_eq!(ast1.mixins.len(), ast2.mixins.len(), "mixin count mismatch");
        assert_eq!(ast1.imports.len(), ast2.imports.len(), "import count mismatch");

        // Same skill names and step counts
        for (s1, s2) in ast1.skills.iter().zip(ast2.skills.iter()) {
            assert_eq!(s1.name, s2.name, "skill name mismatch");
            assert_eq!(s1.body.steps.len(), s2.body.steps.len(),
                "step count mismatch in skill '{}'", s1.name);
            for (st1, st2) in s1.body.steps.iter().zip(s2.body.steps.iter()) {
                assert_eq!(st1.name, st2.name,
                    "step name mismatch in skill '{}'", s1.name);
            }
        }

        // Same type names
        for (t1, t2) in ast1.type_defs.iter().zip(ast2.type_defs.iter()) {
            assert_eq!(t1.name, t2.name, "type name mismatch");
        }

        // Same mixin names
        for (m1, m2) in ast1.mixins.iter().zip(ast2.mixins.iter()) {
            assert_eq!(m1.name, m2.name, "mixin name mismatch");
        }

        // Same pipeline names and stage counts
        for (p1, p2) in ast1.pipelines.iter().zip(ast2.pipelines.iter()) {
            assert_eq!(p1.name, p2.name, "pipeline name mismatch");
            assert_eq!(p1.stages.len(), p2.stages.len(),
                "stage count mismatch in pipeline '{}'", p1.name);
        }

        // Same orchestration names and phase counts
        for (o1, o2) in ast1.orchestrations.iter().zip(ast2.orchestrations.iter()) {
            assert_eq!(o1.name, o2.name, "orchestration name mismatch");
            assert_eq!(o1.phases.len(), o2.phases.len(),
                "phase count mismatch in orchestration '{}'", o1.name);
        }
    }

    #[test]
    fn round_trip_code_review_fixture() {
        let source = include_str!("../tests/fixtures/code_review.agent");
        assert_round_trips(source);
    }

    #[test]
    fn round_trip_full_featured_fixture() {
        let source = include_str!("../tests/fixtures/full_featured.agent");
        assert_round_trips(source);
    }

    #[test]
    fn round_trip_minimal_fixture() {
        let source = include_str!("../tests/fixtures/minimal.agent");
        assert_round_trips(source);
    }

    #[test]
    fn preserves_interleaved_context_step_order() {
        let input = r#"
            skill "x" {
                body {
                    context(priority: 100) { "Top-level instruction." }
                    step explore {
                        context { "Explore." }
                    }
                    context(priority: 75, when: input.constraints) { "Constraint context." }
                    step propose {
                        context { "Propose." }
                    }
                }
            }
        "#;
        let formatted = format_source(input).unwrap();

        let top_ctx = formatted.find("Top-level instruction").unwrap();
        let explore = formatted.find("step explore").unwrap();
        let constraint = formatted.find("Constraint context").unwrap();
        let propose = formatted.find("step propose").unwrap();

        assert!(
            top_ctx < explore,
            "first context should precede explore step"
        );
        assert!(
            explore < constraint,
            "explore step should precede interleaved context:\n{}",
            formatted
        );
        assert!(
            constraint < propose,
            "interleaved context should precede propose step:\n{}",
            formatted
        );
    }
}
