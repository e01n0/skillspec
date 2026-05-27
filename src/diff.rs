/// Structural diff for SkillSpec ASTs and compiled SKILL.md files.
///
/// Two public entry points:
///   - `structural_diff(a, b)` compares two parsed `SourceFile` ASTs.
///   - `skillmd_diff(compiled, actual)` compares a compiled SKILL.md string
///     against an on-disk SKILL.md, grouped by `## ` section headers.
use crate::ast::{BinOp, ContextBlock, Dependency, Expr, Field, SourceFile, TypeExpr};

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
    Relocated,
}

#[derive(Debug, Clone)]
pub struct Change {
    pub kind: ChangeKind,
    pub path: String,
    pub description: String,
}

#[derive(Debug, Default)]
pub struct DiffReport {
    pub changes: Vec<Change>,
}

impl DiffReport {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn display(&self) -> String {
        if self.changes.is_empty() {
            return "No changes detected.\n".to_string();
        }
        let mut out = String::new();
        for c in &self.changes {
            let symbol = match c.kind {
                ChangeKind::Added => "+",
                ChangeKind::Removed => "-",
                ChangeKind::Modified => "~",
                ChangeKind::Relocated => "=>",
            };
            out.push_str(&format!("{} {} — {}\n", symbol, c.path, c.description));
        }
        out
    }

    fn add(&mut self, kind: ChangeKind, path: impl Into<String>, description: impl Into<String>) {
        self.changes.push(Change {
            kind,
            path: path.into(),
            description: description.into(),
        });
    }
}

// ── Structural diff ───────────────────────────────────────────────────────────

/// Compare two parsed `SourceFile` ASTs and return a diff report.
pub fn structural_diff(a: &SourceFile, b: &SourceFile) -> DiffReport {
    let mut report = DiffReport::default();

    diff_skills(a, b, &mut report);
    diff_type_defs(a, b, &mut report);
    diff_pipelines(a, b, &mut report);
    diff_orchestrations(a, b, &mut report);

    report
}

// ── Skills ────────────────────────────────────────────────────────────────────

fn diff_skills(a: &SourceFile, b: &SourceFile, report: &mut DiffReport) {
    // Skills present in `a` but not `b` → Removed
    for skill in &a.skills {
        if !b.skills.iter().any(|s| s.name == skill.name) {
            report.add(
                ChangeKind::Removed,
                format!("skill.{}", skill.name),
                format!("skill '{}' was removed", skill.name),
            );
        }
    }

    // Skills present in `b` but not `a` → Added
    for skill in &b.skills {
        if !a.skills.iter().any(|s| s.name == skill.name) {
            report.add(
                ChangeKind::Added,
                format!("skill.{}", skill.name),
                format!("skill '{}' was added", skill.name),
            );
        }
    }

    // Skills present in both → diff internals
    for skill_a in &a.skills {
        if let Some(skill_b) = b.skills.iter().find(|s| s.name == skill_a.name) {
            let prefix = format!("skill.{}", skill_a.name);

            // Input fields
            diff_fields(
                skill_a.input.as_deref().unwrap_or(&[]),
                skill_b.input.as_deref().unwrap_or(&[]),
                &format!("{}.input", prefix),
                report,
            );

            // Output fields
            diff_fields(
                skill_a.output.as_deref().unwrap_or(&[]),
                skill_b.output.as_deref().unwrap_or(&[]),
                &format!("{}.output", prefix),
                report,
            );

            // Steps
            diff_steps(skill_a, skill_b, b, &prefix, report);

            // Body-level contexts (the ones not inside a step)
            diff_contexts(
                &skill_a.body.contexts,
                &skill_b.body.contexts,
                &format!("{}.body", prefix),
                report,
            );
        }
    }
}

fn diff_fields(a_fields: &[Field], b_fields: &[Field], path_prefix: &str, report: &mut DiffReport) {
    // Removed fields
    for f in a_fields {
        if !b_fields.iter().any(|bf| bf.name == f.name) {
            report.add(
                ChangeKind::Removed,
                format!("{}.{}", path_prefix, f.name),
                format!("field '{}' was removed", f.name),
            );
        }
    }

    // Added fields
    for f in b_fields {
        if !a_fields.iter().any(|af| af.name == f.name) {
            report.add(
                ChangeKind::Added,
                format!("{}.{}", path_prefix, f.name),
                format!("field '{}' was added", f.name),
            );
        }
    }

    // Modified fields (type or optional changed)
    for fa in a_fields {
        if let Some(fb) = b_fields.iter().find(|bf| bf.name == fa.name)
            && (!type_expr_eq(&fa.ty, &fb.ty) || fa.optional != fb.optional)
        {
            report.add(
                ChangeKind::Modified,
                format!("{}.{}", path_prefix, fa.name),
                format!("field '{}' changed: type or optionality modified", fa.name),
            );
        }
    }
}

fn diff_steps(
    skill_a: &crate::ast::Skill,
    skill_b: &crate::ast::Skill,
    b_source: &SourceFile,
    prefix: &str,
    report: &mut DiffReport,
) {
    let a_steps = &skill_a.body.steps;
    let b_steps = &skill_b.body.steps;

    // Removed steps — or Relocated into a use-call target
    for step in a_steps {
        if !b_steps.iter().any(|s| s.name == step.name) {
            if let Some((target_name, modifications)) =
                find_relocation_target(step, b_steps, b_source)
            {
                if modifications.is_empty() {
                    report.add(
                        ChangeKind::Relocated,
                        format!("{}.step.{}", prefix, step.name),
                        format!(
                            "step '{}' relocated to skill '{}' (behaviour preserved)",
                            step.name, target_name
                        ),
                    );
                } else {
                    report.add(
                        ChangeKind::Relocated,
                        format!("{}.step.{}", prefix, step.name),
                        format!(
                            "step '{}' relocated to skill '{}' (MODIFIED: {})",
                            step.name,
                            target_name,
                            modifications.join(", ")
                        ),
                    );
                }
            } else {
                report.add(
                    ChangeKind::Removed,
                    format!("{}.step.{}", prefix, step.name),
                    format!("step '{}' was removed", step.name),
                );
            }
        }
    }

    // Added steps
    for step in b_steps {
        if !a_steps.iter().any(|s| s.name == step.name) {
            report.add(
                ChangeKind::Added,
                format!("{}.step.{}", prefix, step.name),
                format!("step '{}' was added", step.name),
            );
        }
    }

    // Modified steps
    for step_a in a_steps {
        if let Some(step_b) = b_steps.iter().find(|s| s.name == step_a.name) {
            let step_path = format!("{}.step.{}", prefix, step_a.name);

            // Context changes inside the step
            diff_contexts(&step_a.contexts, &step_b.contexts, &step_path, report);

            // emit changed
            if step_a.emit != step_b.emit {
                report.add(
                    ChangeKind::Modified,
                    format!("{}.emit", step_path),
                    format!(
                        "step '{}' emit changed from {} to {}",
                        step_a.name, step_a.emit, step_b.emit
                    ),
                );
            }

            // requires changed
            if !dep_eq(&step_a.requires, &step_b.requires) {
                let desc_a = step_a
                    .requires
                    .as_ref()
                    .map(dep_to_string)
                    .unwrap_or_else(|| "none".to_string());
                let desc_b = step_b
                    .requires
                    .as_ref()
                    .map(dep_to_string)
                    .unwrap_or_else(|| "none".to_string());
                report.add(
                    ChangeKind::Modified,
                    format!("{}.requires", step_path),
                    format!(
                        "step '{}' requires changed from {} to {}",
                        step_a.name, desc_a, desc_b
                    ),
                );
            }

            // step-level when guard changed
            if !expr_eq(&step_a.when, &step_b.when) {
                let ga = step_a
                    .when
                    .as_ref()
                    .map(expr_to_string)
                    .unwrap_or_else(|| "none".to_string());
                let gb = step_b
                    .when
                    .as_ref()
                    .map(expr_to_string)
                    .unwrap_or_else(|| "none".to_string());
                report.add(
                    ChangeKind::Modified,
                    format!("{}.when", step_path),
                    format!(
                        "step '{}' when guard changed from {} to {}",
                        step_a.name, ga, gb
                    ),
                );
            }
        }
    }
}

fn diff_contexts(
    a_ctxs: &[ContextBlock],
    b_ctxs: &[ContextBlock],
    path_prefix: &str,
    report: &mut DiffReport,
) {
    // We compare contexts by index since they're anonymous and ordered.
    let len_a = a_ctxs.len();
    let len_b = b_ctxs.len();
    let min_len = len_a.min(len_b);

    // Check each shared-index context for modifications
    for i in 0..min_len {
        let ca = &a_ctxs[i];
        let cb = &b_ctxs[i];
        let when_changed = !expr_eq(&ca.when, &cb.when);
        if ca.text != cb.text
            || ca.priority != cb.priority
            || ca.decay != cb.decay
            || ca.until != cb.until
            || when_changed
        {
            report.add(
                ChangeKind::Modified,
                format!("{}.context[{}]", path_prefix, i),
                format!(
                    "context block {} changed (priority: {:?}→{:?}, until: {:?}→{:?}, text changed: {}, when changed: {})",
                    i,
                    ca.priority,
                    cb.priority,
                    ca.until,
                    cb.until,
                    ca.text != cb.text,
                    when_changed
                ),
            );
        }
    }

    // Extra contexts in `a` → removed
    for i in min_len..len_a {
        report.add(
            ChangeKind::Removed,
            format!("{}.context[{}]", path_prefix, i),
            format!("context block {} was removed", i),
        );
    }

    // Extra contexts in `b` → added
    for i in min_len..len_b {
        report.add(
            ChangeKind::Added,
            format!("{}.context[{}]", path_prefix, i),
            format!("context block {} was added", i),
        );
    }
}

// ── Type defs ─────────────────────────────────────────────────────────────────

fn diff_type_defs(a: &SourceFile, b: &SourceFile, report: &mut DiffReport) {
    // Removed types
    for td in &a.type_defs {
        if !b.type_defs.iter().any(|t| t.name == td.name) {
            report.add(
                ChangeKind::Removed,
                format!("type.{}", td.name),
                format!("type '{}' was removed", td.name),
            );
        }
    }

    // Added types
    for td in &b.type_defs {
        if !a.type_defs.iter().any(|t| t.name == td.name) {
            report.add(
                ChangeKind::Added,
                format!("type.{}", td.name),
                format!("type '{}' was added", td.name),
            );
        }
    }

    // Modified types (field-level diff)
    for tda in &a.type_defs {
        if let Some(tdb) = b.type_defs.iter().find(|t| t.name == tda.name) {
            diff_fields(
                &tda.fields,
                &tdb.fields,
                &format!("type.{}", tda.name),
                report,
            );
        }
    }
}

// ── Pipelines ─────────────────────────────────────────────────────────────────

fn diff_pipelines(a: &SourceFile, b: &SourceFile, report: &mut DiffReport) {
    // Removed pipelines
    for p in &a.pipelines {
        if !b.pipelines.iter().any(|q| q.name == p.name) {
            report.add(
                ChangeKind::Removed,
                format!("pipeline.{}", p.name),
                format!("pipeline '{}' was removed", p.name),
            );
        }
    }

    // Added pipelines
    for p in &b.pipelines {
        if !a.pipelines.iter().any(|q| q.name == p.name) {
            report.add(
                ChangeKind::Added,
                format!("pipeline.{}", p.name),
                format!("pipeline '{}' was added", p.name),
            );
        }
    }

    // Modified pipelines
    for pa in &a.pipelines {
        if let Some(pb) = b.pipelines.iter().find(|q| q.name == pa.name) {
            let prefix = format!("pipeline.{}", pa.name);

            // Input/output fields
            diff_fields(
                pa.input.as_deref().unwrap_or(&[]),
                pb.input.as_deref().unwrap_or(&[]),
                &format!("{}.input", prefix),
                report,
            );
            diff_fields(
                pa.output.as_deref().unwrap_or(&[]),
                pb.output.as_deref().unwrap_or(&[]),
                &format!("{}.output", prefix),
                report,
            );

            // Stages
            for stage in &pa.stages {
                if !pb.stages.iter().any(|s| s.name == stage.name) {
                    report.add(
                        ChangeKind::Removed,
                        format!("{}.stage.{}", prefix, stage.name),
                        format!("stage '{}' was removed", stage.name),
                    );
                }
            }
            for stage in &pb.stages {
                if !pa.stages.iter().any(|s| s.name == stage.name) {
                    report.add(
                        ChangeKind::Added,
                        format!("{}.stage.{}", prefix, stage.name),
                        format!("stage '{}' was added", stage.name),
                    );
                }
            }
        }
    }
}

// ── Orchestrations ────────────────────────────────────────────────────────────

fn diff_orchestrations(a: &SourceFile, b: &SourceFile, report: &mut DiffReport) {
    // Removed orchestrations
    for o in &a.orchestrations {
        if !b.orchestrations.iter().any(|q| q.name == o.name) {
            report.add(
                ChangeKind::Removed,
                format!("orchestration.{}", o.name),
                format!("orchestration '{}' was removed", o.name),
            );
        }
    }

    // Added orchestrations
    for o in &b.orchestrations {
        if !a.orchestrations.iter().any(|q| q.name == o.name) {
            report.add(
                ChangeKind::Added,
                format!("orchestration.{}", o.name),
                format!("orchestration '{}' was added", o.name),
            );
        }
    }

    // Modified orchestrations
    for oa in &a.orchestrations {
        if let Some(ob) = b.orchestrations.iter().find(|q| q.name == oa.name) {
            let prefix = format!("orchestration.{}", oa.name);

            // Input/output fields
            diff_fields(
                oa.input.as_deref().unwrap_or(&[]),
                ob.input.as_deref().unwrap_or(&[]),
                &format!("{}.input", prefix),
                report,
            );
            diff_fields(
                oa.output.as_deref().unwrap_or(&[]),
                ob.output.as_deref().unwrap_or(&[]),
                &format!("{}.output", prefix),
                report,
            );

            // Phases
            for phase in &oa.phases {
                if !ob.phases.iter().any(|p| p.name == phase.name) {
                    report.add(
                        ChangeKind::Removed,
                        format!("{}.phase.{}", prefix, phase.name),
                        format!("phase '{}' was removed", phase.name),
                    );
                }
            }
            for phase in &ob.phases {
                if !oa.phases.iter().any(|p| p.name == phase.name) {
                    report.add(
                        ChangeKind::Added,
                        format!("{}.phase.{}", prefix, phase.name),
                        format!("phase '{}' was added", phase.name),
                    );
                }
            }

            // Agents
            for agent in &oa.agents {
                if !ob.agents.iter().any(|a| a.name == agent.name) {
                    report.add(
                        ChangeKind::Removed,
                        format!("{}.agent.{}", prefix, agent.name),
                        format!("agent '{}' was removed", agent.name),
                    );
                }
            }
            for agent in &ob.agents {
                if !oa.agents.iter().any(|a| a.name == agent.name) {
                    report.add(
                        ChangeKind::Added,
                        format!("{}.agent.{}", prefix, agent.name),
                        format!("agent '{}' was added", agent.name),
                    );
                }
            }
        }
    }
}

// ── Helper: shallow TypeExpr equality ─────────────────────────────────────────

fn type_expr_eq(a: &TypeExpr, b: &TypeExpr) -> bool {
    match (a, b) {
        (TypeExpr::String, TypeExpr::String) => true,
        (TypeExpr::Int, TypeExpr::Int) => true,
        (TypeExpr::Float, TypeExpr::Float) => true,
        (TypeExpr::Bool, TypeExpr::Bool) => true,
        (TypeExpr::Named(x), TypeExpr::Named(y)) => x == y,
        (TypeExpr::Array(x), TypeExpr::Array(y)) => type_expr_eq(x, y),
        (TypeExpr::Enum(xs), TypeExpr::Enum(ys)) => xs == ys,
        (TypeExpr::Map(k1, v1), TypeExpr::Map(k2, v2)) => {
            type_expr_eq(k1, k2) && type_expr_eq(v1, v2)
        }
        _ => false,
    }
}

// ── Relocation detection ─────────────────────────────────────────────────────

fn find_relocation_target(
    removed_step: &crate::ast::Step,
    b_steps: &[crate::ast::Step],
    b_source: &SourceFile,
) -> Option<(String, Vec<String>)> {
    for candidate in b_steps {
        if let Some(use_call) = &candidate.use_call {
            let target_name = use_call.skill_name.replace('_', "-");
            if let Some(target_skill) = b_source.skills.iter().find(|s| s.name == target_name)
                && let Some(matching_step) = target_skill
                    .body
                    .steps
                    .iter()
                    .find(|s| s.name == removed_step.name)
            {
                let modifications =
                    compare_step_content(removed_step, matching_step, target_skill, use_call);
                return Some((target_name, modifications));
            }
        }
    }
    None
}

fn compare_step_content(
    original: &crate::ast::Step,
    relocated: &crate::ast::Step,
    target_skill: &crate::ast::Skill,
    use_call: &crate::ast::UseCall,
) -> Vec<String> {
    let mut mods = Vec::new();

    let target_step_names: std::collections::HashSet<&str> = target_skill
        .body
        .steps
        .iter()
        .map(|s| s.name.as_str())
        .collect();

    // Build binding: callee input name → caller expression string.
    // e.g. use extract(paths: input.source_files) → {"paths": "input.source_files"}
    let binding: std::collections::HashMap<String, String> = use_call
        .args
        .iter()
        .map(|(param, expr)| (param.clone(), expr_to_string(expr)))
        .collect();

    if !dep_eq(&original.requires, &relocated.requires) {
        let is_external_removal =
            is_external_dep_removal(&original.requires, &relocated.requires, &target_step_names);
        if !is_external_removal {
            let from = original
                .requires
                .as_ref()
                .map(dep_to_string)
                .unwrap_or_else(|| "none".to_string());
            let to = relocated
                .requires
                .as_ref()
                .map(dep_to_string)
                .unwrap_or_else(|| "none".to_string());
            mods.push(format!("requires {} → {}", from, to));
        }
    }

    if !expr_eq_with_binding(&original.when, &relocated.when, &binding) {
        mods.push("step when guard changed".to_string());
    }

    // emit changes are expected extraction adaptations

    let min_len = original.contexts.len().min(relocated.contexts.len());
    for i in 0..min_len {
        let ca = &original.contexts[i];
        let cb = &relocated.contexts[i];
        let bound_text = apply_binding(&cb.text, &binding);
        if ca.text != bound_text {
            mods.push(format!("context[{}] text changed", i));
        }
        if ca.priority != cb.priority {
            mods.push(format!(
                "context[{}] priority {:?} → {:?}",
                i, ca.priority, cb.priority
            ));
        }
        if !expr_eq_with_binding(&ca.when, &cb.when, &binding) {
            mods.push(format!("context[{}] when guard changed", i));
        }
        if ca.decay != cb.decay {
            mods.push(format!("context[{}] decay changed", i));
        }
    }
    if original.contexts.len() != relocated.contexts.len() {
        mods.push(format!(
            "context count {} → {}",
            original.contexts.len(),
            relocated.contexts.len()
        ));
    }

    mods
}

/// Compare two optional expressions, applying the argument binding to the
/// relocated expression before comparison. This accounts for name differences
/// across the use-call seam (e.g. callee's `input.strict` = caller's
/// `input.strict_mode` when bound via `use skill(strict: input.strict_mode)`).
fn expr_eq_with_binding(
    original: &Option<Expr>,
    relocated: &Option<Expr>,
    binding: &std::collections::HashMap<String, String>,
) -> bool {
    match (original, relocated) {
        (None, None) => true,
        (Some(eo), Some(er)) => {
            let bound = apply_binding(&expr_to_string(er), binding);
            expr_to_string(eo) == bound
        }
        _ => false,
    }
}

fn apply_binding(s: &str, binding: &std::collections::HashMap<String, String>) -> String {
    let mut result = s.to_string();
    for (param, replacement) in binding {
        let pattern = format!("input.{}", param);
        result = result.replace(&pattern, replacement);
    }
    result
}

/// Returns true if the requires difference is just an external dep being removed
/// (the original step depended on something outside the extraction, and the
/// relocated step dropped that dep because it's handled by the wrapper).
fn is_external_dep_removal(
    original: &Option<Dependency>,
    relocated: &Option<Dependency>,
    target_step_names: &std::collections::HashSet<&str>,
) -> bool {
    match (original, relocated) {
        // Had a dep, now has none — check if the dep was external
        (Some(dep), None) => all_deps_external(dep, target_step_names),
        // Had a dep, now has a different dep — check if only externals were dropped
        (Some(orig_dep), Some(new_dep)) => {
            let orig_names = dep_names(orig_dep);
            let new_names = dep_names(new_dep);
            let removed: Vec<&str> = orig_names
                .iter()
                .filter(|n| !new_names.contains(n))
                .copied()
                .collect();
            let added: Vec<&str> = new_names
                .iter()
                .filter(|n| !orig_names.contains(n))
                .copied()
                .collect();
            // All removed deps are external AND no new deps were added
            !removed.is_empty()
                && added.is_empty()
                && removed.iter().all(|n| !target_step_names.contains(n))
        }
        _ => false,
    }
}

fn all_deps_external(
    dep: &Dependency,
    target_step_names: &std::collections::HashSet<&str>,
) -> bool {
    dep_names(dep)
        .iter()
        .all(|n| !target_step_names.contains(n))
}

fn dep_names(dep: &Dependency) -> Vec<&str> {
    match dep {
        Dependency::Single(name) => vec![name.as_str()],
        Dependency::All(names) | Dependency::Any(names) => {
            names.iter().map(|s| s.as_str()).collect()
        }
        Dependency::AllSteps => vec![],
    }
}

// ── Dependency comparison ─────────────────────────────────────────────────────

fn dep_to_string(dep: &Dependency) -> String {
    match dep {
        Dependency::Single(name) => name.clone(),
        Dependency::All(names) => format!("all({})", names.join(", ")),
        Dependency::Any(names) => format!("any({})", names.join(", ")),
        Dependency::AllSteps => "*".to_string(),
    }
}

fn dep_eq(a: &Option<Dependency>, b: &Option<Dependency>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(da), Some(db)) => dep_to_string(da) == dep_to_string(db),
        _ => false,
    }
}

// ── Expr comparison ──────────────────────────────────────────────────────────

fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::StringLit(s) => format!("\"{}\"", s),
        Expr::IntLit(n) => n.to_string(),
        Expr::FloatLit(f) => f.to_string(),
        Expr::BoolLit(b) => b.to_string(),
        Expr::Ident(name) => name.clone(),
        Expr::FieldAccess(obj, field) => format!("{}.{}", expr_to_string(obj), field),
        Expr::ArrayLit(items) => {
            let parts: Vec<String> = items.iter().map(expr_to_string).collect();
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
            let arg_parts: Vec<String> = args
                .iter()
                .map(|(k, v)| format!("{}: {}", k, expr_to_string(v)))
                .collect();
            format!("{}({})", name, arg_parts.join(", "))
        }
        Expr::Interpolated(s) => format!("`{}`", s),
    }
}

fn expr_eq(a: &Option<Expr>, b: &Option<Expr>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(ea), Some(eb)) => expr_to_string(ea) == expr_to_string(eb),
        _ => false,
    }
}

// ── SKILL.md diff ─────────────────────────────────────────────────────────────

/// Compare a compiled SKILL.md string against an on-disk (or reference) SKILL.md.
///
/// Uses LCS (longest common subsequence) to produce a semantic diff that
/// correctly handles insertions and deletions without cascading false positives.
/// Changes are grouped under the nearest `## ` section header.
pub fn skillmd_diff(compiled: &str, actual: &str) -> DiffReport {
    let mut report = DiffReport::default();

    let compiled_lines: Vec<&str> = compiled.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let m = compiled_lines.len();
    let n = actual_lines.len();

    // Guard: LCS table is O(m*n) space. For very large files, fall back to
    // reporting a single "files differ" change rather than blowing up memory.
    const MAX_CELLS: usize = 500_000;
    if m.saturating_mul(n) > MAX_CELLS {
        if compiled != actual {
            report.add(
                ChangeKind::Modified,
                "<file>".to_string(),
                format!(
                    "files differ ({} vs {} lines; too large for line-level diff)",
                    m, n
                ),
            );
        }
        return report;
    }

    // Build LCS table
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if compiled_lines[i - 1] == actual_lines[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack to produce edit script
    enum Edit {
        Keep,
        Remove(usize),
        Add(usize),
    }

    let mut edits = Vec::new();
    let mut i = m;
    let mut j = n;

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && compiled_lines[i - 1] == actual_lines[j - 1] {
            edits.push(Edit::Keep);
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            edits.push(Edit::Add(j - 1));
            j -= 1;
        } else {
            edits.push(Edit::Remove(i - 1));
            i -= 1;
        }
    }

    edits.reverse();

    fn section_for(lines: &[&str], idx: usize) -> String {
        for i in (0..=idx).rev() {
            if lines[i].starts_with("## ") {
                return lines[i].to_string();
            }
        }
        "<preamble>".to_string()
    }

    // Collapse adjacent Remove+Add into Modified
    let mut idx = 0;
    while idx < edits.len() {
        match &edits[idx] {
            Edit::Keep => {}
            Edit::Remove(ci) => {
                if idx + 1 < edits.len()
                    && let Edit::Add(ai) = &edits[idx + 1]
                {
                    let section = section_for(&compiled_lines, *ci);
                    report.add(
                        ChangeKind::Modified,
                        format!("{} line {}", section, ci + 1),
                        format!(
                            "line changed:\n  - {}\n  + {}",
                            compiled_lines[*ci], actual_lines[*ai]
                        ),
                    );
                    idx += 2;
                    continue;
                }
                let section = section_for(&compiled_lines, *ci);
                report.add(
                    ChangeKind::Removed,
                    format!("{} line {}", section, ci + 1),
                    format!(
                        "line present in compiled but missing from actual: {:?}",
                        compiled_lines[*ci]
                    ),
                );
            }
            Edit::Add(ai) => {
                let section = section_for(&actual_lines, *ai);
                report.add(
                    ChangeKind::Added,
                    format!("{} line {}", section, ai + 1),
                    format!(
                        "line present in actual but missing from compiled: {:?}",
                        actual_lines[*ai]
                    ),
                );
            }
        }
        idx += 1;
    }

    report
}

// ── Semver classification ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemverLevel {
    Patch,
    Minor,
    Major,
}

impl std::fmt::Display for SemverLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SemverLevel::Patch => write!(f, "PATCH"),
            SemverLevel::Minor => write!(f, "MINOR"),
            SemverLevel::Major => write!(f, "MAJOR"),
        }
    }
}

#[derive(Debug)]
pub struct SemverReport {
    pub level: SemverLevel,
    pub breakdown: Vec<(SemverLevel, String)>,
}

impl SemverReport {
    pub fn display(&self) -> String {
        let mut out = format!("Semver bump: {}\n\n", self.level);
        for (level, desc) in &self.breakdown {
            out.push_str(&format!("  [{}] {}\n", level, desc));
        }
        out
    }
}

pub fn classify_semver(a: &SourceFile, b: &SourceFile) -> SemverReport {
    let report = structural_diff(a, b);
    let mut breakdown = Vec::new();

    for change in &report.changes {
        let level = classify_change(change, b);
        breakdown.push((
            level,
            format!(
                "{} {} — {}",
                change_symbol(&change.kind),
                change.path,
                change.description
            ),
        ));
    }

    let level = breakdown
        .iter()
        .map(|(l, _)| *l)
        .max()
        .unwrap_or(SemverLevel::Patch);

    SemverReport { level, breakdown }
}

fn change_symbol(kind: &ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Added => "+",
        ChangeKind::Removed => "-",
        ChangeKind::Modified => "~",
        ChangeKind::Relocated => "=>",
    }
}

fn classify_change(change: &Change, new_file: &SourceFile) -> SemverLevel {
    let path = &change.path;

    match change.kind {
        ChangeKind::Removed => {
            if path.contains(".input.") || path.contains(".output.") || is_top_level_construct(path)
            {
                SemverLevel::Major
            } else if path.contains(".step.")
                || path.contains(".stage.")
                || path.contains(".phase.")
            {
                SemverLevel::Minor
            } else {
                SemverLevel::Patch
            }
        }
        ChangeKind::Added => {
            if path.contains(".input.") {
                if is_field_optional_in(path, new_file) {
                    SemverLevel::Minor
                } else {
                    SemverLevel::Major
                }
            } else if path.contains(".output.")
                || path.contains(".step.")
                || path.contains(".stage.")
                || path.contains(".phase.")
                || is_top_level_construct(path)
            {
                SemverLevel::Minor
            } else {
                SemverLevel::Patch
            }
        }
        ChangeKind::Modified => {
            if path.contains(".input.") || path.contains(".output.") {
                SemverLevel::Major
            } else {
                SemverLevel::Patch
            }
        }
        ChangeKind::Relocated => {
            if change.description.contains("behaviour preserved") {
                SemverLevel::Patch
            } else {
                SemverLevel::Minor
            }
        }
    }
}

fn is_top_level_construct(path: &str) -> bool {
    let parts: Vec<&str> = path.split('.').collect();
    parts.len() == 2 && matches!(parts[0], "skill" | "pipeline" | "orchestration" | "type")
}

fn is_field_optional_in(path: &str, file: &SourceFile) -> bool {
    lookup_field_optional(path, file).unwrap_or(false)
}

fn lookup_field_optional(path: &str, file: &SourceFile) -> Option<bool> {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.len() < 4 {
        return None;
    }
    let (construct_type, construct_name, block, field_name) =
        (parts[0], parts[1], parts[2], parts[3]);

    let fields = match construct_type {
        "skill" => {
            let skill = file.skills.iter().find(|s| s.name == construct_name)?;
            match block {
                "input" => skill.input.as_deref(),
                "output" => skill.output.as_deref(),
                _ => None,
            }
        }
        "pipeline" => {
            let p = file.pipelines.iter().find(|p| p.name == construct_name)?;
            match block {
                "input" => p.input.as_deref(),
                "output" => p.output.as_deref(),
                _ => None,
            }
        }
        _ => None,
    };
    fields?
        .iter()
        .find(|f| f.name == field_name)
        .map(|f| f.optional)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

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
    fn detects_added_step() {
        let a = parse(r#"skill "x" { body { step a { context { "a" } } } }"#);
        let b = parse(
            r#"skill "x" { body { step a { context { "a" } } step b { context { "b" } } } }"#,
        );
        let report = structural_diff(&a, &b);
        assert!(!report.is_empty());
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Added) && c.path.contains("step.b"))
        );
    }

    #[test]
    fn detects_removed_field() {
        let a =
            parse(r#"skill "x" { input { name: string  age: int } body { context { "ok" } } }"#);
        let b = parse(r#"skill "x" { input { name: string } body { context { "ok" } } }"#);
        let report = structural_diff(&a, &b);
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Removed) && c.path.contains("age"))
        );
    }

    #[test]
    fn detects_modified_context() {
        let a = parse(r#"skill "x" { body { context(priority: important) { "Original." } } }"#);
        let b = parse(r#"skill "x" { body { context(priority: supplementary) { "Changed." } } }"#);
        let report = structural_diff(&a, &b);
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Modified))
        );
    }

    #[test]
    fn identical_files_no_changes() {
        let a = parse(r#"skill "x" { body { context { "same." } } }"#);
        let b = parse(r#"skill "x" { body { context { "same." } } }"#);
        let report = structural_diff(&a, &b);
        assert!(report.is_empty());
    }

    #[test]
    fn skillmd_diff_detects_section_change() {
        let compiled = "## Step: analyze\n\nOriginal text.\n";
        let actual = "## Step: analyze\n\nModified text.\n";
        let report = skillmd_diff(compiled, actual);
        assert!(!report.is_empty());
    }

    #[test]
    fn skillmd_diff_insertion_does_not_cascade() {
        let compiled = "line 1\nline 2\nline 3\n";
        let actual = "line 1\ninserted\nline 2\nline 3\n";
        let report = skillmd_diff(compiled, actual);
        assert_eq!(
            report.changes.len(),
            1,
            "single insertion should produce exactly 1 change, got: {}",
            report.display()
        );
        assert!(
            matches!(report.changes[0].kind, ChangeKind::Added),
            "should be an addition, not modification"
        );
    }

    #[test]
    fn skillmd_diff_deletion_does_not_cascade() {
        let compiled = "line 1\nremove me\nline 2\nline 3\n";
        let actual = "line 1\nline 2\nline 3\n";
        let report = skillmd_diff(compiled, actual);
        assert_eq!(
            report.changes.len(),
            1,
            "single deletion should produce exactly 1 change, got: {}",
            report.display()
        );
        assert!(
            matches!(report.changes[0].kind, ChangeKind::Removed),
            "should be a removal"
        );
    }

    #[test]
    fn skillmd_diff_large_input_guard() {
        let big_a: String = (0..1000).map(|i| format!("line {}\n", i)).collect();
        let big_b: String = (0..1000).map(|i| format!("line {}\n", i + 1)).collect();
        let report = skillmd_diff(&big_a, &big_b);
        assert!(
            !report.is_empty(),
            "should still report a difference for large inputs"
        );
    }

    #[test]
    fn skillmd_diff_modification_collapsed() {
        let compiled = "line 1\nold text\nline 3\n";
        let actual = "line 1\nnew text\nline 3\n";
        let report = skillmd_diff(compiled, actual);
        assert_eq!(
            report.changes.len(),
            1,
            "single line change should produce 1 modified change, got: {}",
            report.display()
        );
        assert!(
            matches!(report.changes[0].kind, ChangeKind::Modified),
            "should be a modification"
        );
    }

    // ── Semver tests ─────────────────────────────────────────────────────

    #[test]
    fn semver_identical_is_patch() {
        let a = parse(r#"skill "x" { input { f: string[] } body { context { "ok" } } }"#);
        let b = parse(r#"skill "x" { input { f: string[] } body { context { "ok" } } }"#);
        let report = classify_semver(&a, &b);
        assert!(report.breakdown.is_empty());
        assert_eq!(report.level, SemverLevel::Patch);
    }

    #[test]
    fn semver_removed_input_field_is_major() {
        let a = parse(
            r#"skill "x" { input { files: string[] name: string } body { context { "ok" } } }"#,
        );
        let b = parse(r#"skill "x" { input { files: string[] } body { context { "ok" } } }"#);
        let report = classify_semver(&a, &b);
        assert_eq!(report.level, SemverLevel::Major);
    }

    #[test]
    fn semver_added_required_input_is_major() {
        let a = parse(r#"skill "x" { input { files: string[] } body { context { "ok" } } }"#);
        let b = parse(
            r#"skill "x" { input { files: string[] name: string } body { context { "ok" } } }"#,
        );
        let report = classify_semver(&a, &b);
        assert_eq!(report.level, SemverLevel::Major);
    }

    #[test]
    fn semver_added_optional_input_is_minor() {
        let a = parse(r#"skill "x" { input { files: string[] } body { context { "ok" } } }"#);
        let b = parse(
            r#"skill "x" { input { files: string[] focus?: string } body { context { "ok" } } }"#,
        );
        let report = classify_semver(&a, &b);
        assert_eq!(report.level, SemverLevel::Minor);
    }

    #[test]
    fn semver_type_change_is_major() {
        let a = parse(r#"skill "x" { input { files: string[] } body { context { "ok" } } }"#);
        let b = parse(r#"skill "x" { input { files: int[] } body { context { "ok" } } }"#);
        let report = classify_semver(&a, &b);
        assert_eq!(report.level, SemverLevel::Major);
    }

    #[test]
    fn semver_new_step_is_minor() {
        let a = parse(r#"skill "x" { body { step a { context { "a" } } } }"#);
        let b = parse(
            r#"skill "x" { body { step a { context { "a" } } step b { context { "b" } } } }"#,
        );
        let report = classify_semver(&a, &b);
        assert_eq!(report.level, SemverLevel::Minor);
    }

    #[test]
    fn semver_context_text_change_is_patch() {
        let a = parse(r#"skill "x" { body { context { "old." } } }"#);
        let b = parse(r#"skill "x" { body { context { "new." } } }"#);
        let report = classify_semver(&a, &b);
        assert_eq!(report.level, SemverLevel::Patch);
    }

    #[test]
    fn semver_mixed_changes_reports_highest() {
        let a = parse(
            r#"skill "x" { input { files: string[] name: string } body { context { "old" } step a { context { "a" } } } }"#,
        );
        let b = parse(
            r#"skill "x" { input { files: string[] } body { context { "new" } step a { context { "a" } } step b { context { "b" } } } }"#,
        );
        let report = classify_semver(&a, &b);
        assert_eq!(
            report.level,
            SemverLevel::Major,
            "removed input should dominate: {:?}",
            report.breakdown
        );
    }

    // ── Step A: requires comparison ─────────────────────────────────────

    #[test]
    fn detects_requires_added() {
        let a = parse(
            r#"skill "x" { body { step a { context { "a" } } step b { context { "b" } } } }"#,
        );
        let b = parse(
            r#"skill "x" { body { step a { context { "a" } } step b { requires a context { "b" } } } }"#,
        );
        let report = structural_diff(&a, &b);
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Modified)
                    && c.path.contains("step.b")
                    && c.path.contains("requires")),
            "should detect requires added: {}",
            report.display()
        );
    }

    #[test]
    fn detects_requires_removed() {
        let a = parse(
            r#"skill "x" { body { step a { context { "a" } } step b { requires a context { "b" } } } }"#,
        );
        let b = parse(
            r#"skill "x" { body { step a { context { "a" } } step b { context { "b" } } } }"#,
        );
        let report = structural_diff(&a, &b);
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Modified)
                    && c.path.contains("step.b")
                    && c.path.contains("requires")),
            "should detect requires removed: {}",
            report.display()
        );
    }

    #[test]
    fn detects_requires_changed() {
        let a = parse(
            r#"skill "x" { body { step a { context { "a" } } step b { context { "b" } } step c { requires a context { "c" } } } }"#,
        );
        let b = parse(
            r#"skill "x" { body { step a { context { "a" } } step b { context { "b" } } step c { requires b context { "c" } } } }"#,
        );
        let report = structural_diff(&a, &b);
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Modified)
                    && c.path.contains("step.c")
                    && c.path.contains("requires")),
            "should detect requires changed: {}",
            report.display()
        );
    }

    #[test]
    fn identical_requires_no_change() {
        let a = parse(
            r#"skill "x" { body { step a { context { "a" } } step b { requires a context { "b" } } } }"#,
        );
        let b = parse(
            r#"skill "x" { body { step a { context { "a" } } step b { requires a context { "b" } } } }"#,
        );
        let report = structural_diff(&a, &b);
        assert!(
            report.is_empty(),
            "identical requires should produce no diff: {}",
            report.display()
        );
    }

    // ── Step A: when guard comparison ────────────────────────────────────

    #[test]
    fn detects_when_guard_removed() {
        let a = parse(
            r#"skill "x" { body { context(priority: important, when: input.focus) { "Focus area." } } }"#,
        );
        let b = parse(r#"skill "x" { body { context(priority: important) { "Focus area." } } }"#);
        let report = structural_diff(&a, &b);
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Modified) && c.path.contains("context")),
            "should detect when guard removed: {}",
            report.display()
        );
    }

    #[test]
    fn detects_when_guard_added() {
        let a = parse(r#"skill "x" { body { context(priority: important) { "Focus area." } } }"#);
        let b = parse(
            r#"skill "x" { body { context(priority: important, when: input.focus) { "Focus area." } } }"#,
        );
        let report = structural_diff(&a, &b);
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Modified) && c.path.contains("context")),
            "should detect when guard added: {}",
            report.display()
        );
    }

    #[test]
    fn detects_when_guard_changed() {
        let a = parse(
            r#"skill "x" { body { context(priority: important, when: input.focus) { "Focus area." } } }"#,
        );
        let b = parse(
            r#"skill "x" { body { context(priority: important, when: input.strict_mode) { "Focus area." } } }"#,
        );
        let report = structural_diff(&a, &b);
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Modified) && c.path.contains("context")),
            "should detect when guard changed: {}",
            report.display()
        );
    }

    #[test]
    fn identical_when_guard_no_change() {
        let a = parse(
            r#"skill "x" { body { context(priority: important, when: input.focus) { "Focus area." } } }"#,
        );
        let b = parse(
            r#"skill "x" { body { context(priority: important, when: input.focus) { "Focus area." } } }"#,
        );
        let report = structural_diff(&a, &b);
        assert!(
            report.is_empty(),
            "identical when guard should produce no diff: {}",
            report.display()
        );
    }

    // ── Step A: priority change verification ─────────────────────────────

    #[test]
    fn detects_priority_change() {
        let a = parse(r#"skill "x" { body { context(priority: important) { "Same text." } } }"#);
        let b = parse(r#"skill "x" { body { context(priority: critical) { "Same text." } } }"#);
        let report = structural_diff(&a, &b);
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Modified) && c.path.contains("context")),
            "should detect priority change: {}",
            report.display()
        );
    }

    // ── Step A: step-level when guard ────────────────────────────────────

    #[test]
    fn detects_step_when_guard_changed() {
        let a = parse(r#"skill "x" { body { step a { when input.ready context { "a" } } } }"#);
        let b = parse(r#"skill "x" { body { step a { context { "a" } } } }"#);
        let report = structural_diff(&a, &b);
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Modified) && c.path.contains("step.a")),
            "should detect step-level when guard change: {}",
            report.display()
        );
    }

    // ── Step C: Relocated change kind ────────────────────────────────────

    #[test]
    fn relocated_step_detected_behaviour_preserved() {
        let a = parse(
            r#"
            skill "main" {
                body {
                    step validate {
                        context(priority: important) { "Validate records." }
                    }
                    step transform {
                        requires validate
                        context(priority: important) { "Transform records." }
                    }
                    step export {
                        requires transform
                        emit output
                        context { "Export." }
                    }
                }
            }
        "#,
        );
        let b = parse(
            r#"
            skill "vt" {
                body {
                    step validate {
                        context(priority: important) { "Validate records." }
                    }
                    step transform {
                        requires validate
                        context(priority: important) { "Transform records." }
                    }
                }
            }
            skill "main" {
                body {
                    step run_vt {
                        use vt()
                        context { "Run extracted skill." }
                    }
                    step export {
                        requires run_vt
                        emit output
                        context { "Export." }
                    }
                }
            }
        "#,
        );
        let report = structural_diff(&a, &b);
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Relocated)
                    && c.path.contains("step.validate")
                    && c.description.contains("behaviour preserved")),
            "validate should be Relocated (behaviour preserved): {}",
            report.display()
        );
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Relocated)
                    && c.path.contains("step.transform")
                    && c.description.contains("behaviour preserved")),
            "transform should be Relocated (behaviour preserved): {}",
            report.display()
        );
    }

    #[test]
    fn relocated_step_detected_modified() {
        let a = parse(
            r#"
            skill "main" {
                body {
                    step validate {
                        context(priority: important) { "Validate records." }
                    }
                }
            }
        "#,
        );
        let b = parse(
            r#"
            skill "vt" {
                body {
                    step validate {
                        context(priority: supplementary) { "Validate records." }
                    }
                }
            }
            skill "main" {
                body {
                    step run_vt {
                        use vt()
                        context { "Run." }
                    }
                }
            }
        "#,
        );
        let report = structural_diff(&a, &b);
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Relocated)
                    && c.path.contains("step.validate")
                    && c.description.contains("MODIFIED")),
            "should be Relocated (MODIFIED) due to priority change: {}",
            report.display()
        );
    }

    #[test]
    fn relocated_vs_genuinely_removed() {
        let a = parse(
            r#"
            skill "main" {
                body {
                    step validate {
                        context(priority: important) { "Validate." }
                    }
                    step legacy {
                        context { "Legacy code." }
                    }
                    step export {
                        emit output
                        context { "Export." }
                    }
                }
            }
        "#,
        );
        let b = parse(
            r#"
            skill "vt" {
                body {
                    step validate {
                        context(priority: important) { "Validate." }
                    }
                }
            }
            skill "main" {
                body {
                    step run_vt {
                        use vt()
                        context { "Run." }
                    }
                    step export {
                        emit output
                        context { "Export." }
                    }
                }
            }
        "#,
        );
        let report = structural_diff(&a, &b);
        assert!(
            report.changes.iter().any(
                |c| matches!(c.kind, ChangeKind::Relocated) && c.path.contains("step.validate")
            ),
            "validate should be Relocated: {}",
            report.display()
        );
        assert!(
            report
                .changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::Removed) && c.path.contains("step.legacy")),
            "legacy should be Removed (not Relocated): {}",
            report.display()
        );
    }

    #[test]
    fn relocated_display_uses_arrow_symbol() {
        let mut report = DiffReport::default();
        report.add(
            ChangeKind::Relocated,
            "skill.x.step.a",
            "relocated to skill 'y' (behaviour preserved)",
        );
        let output = report.display();
        assert!(
            output.starts_with("=>"),
            "Relocated should display with => symbol: {}",
            output
        );
    }

    #[test]
    fn relocated_semver_is_patch_when_preserved() {
        let a = parse(
            r#"
            skill "main" {
                body {
                    step validate {
                        context { "Validate." }
                    }
                }
            }
        "#,
        );
        let b = parse(
            r#"
            skill "vt" {
                body {
                    step validate {
                        context { "Validate." }
                    }
                }
            }
            skill "main" {
                body {
                    step run_vt {
                        use vt()
                        context { "Run." }
                    }
                }
            }
        "#,
        );
        let report = classify_semver(&a, &b);
        let relocated_levels: Vec<_> = report
            .breakdown
            .iter()
            .filter(|(_, desc)| desc.contains("relocated"))
            .map(|(level, _)| *level)
            .collect();
        assert!(
            relocated_levels.iter().all(|l| *l == SemverLevel::Patch),
            "behaviour-preserved relocation should be Patch: {:?}",
            report.breakdown
        );
    }

    #[test]
    fn relocated_semver_is_minor_when_modified() {
        let a = parse(
            r#"
            skill "main" {
                body {
                    step validate {
                        context(priority: important) { "Validate." }
                    }
                }
            }
        "#,
        );
        let b = parse(
            r#"
            skill "vt" {
                body {
                    step validate {
                        context(priority: supplementary) { "Validate." }
                    }
                }
            }
            skill "main" {
                body {
                    step run_vt {
                        use vt()
                        context { "Run." }
                    }
                }
            }
        "#,
        );
        let report = classify_semver(&a, &b);
        let relocated_levels: Vec<_> = report
            .breakdown
            .iter()
            .filter(|(_, desc)| desc.contains("relocated"))
            .map(|(level, _)| *level)
            .collect();
        assert!(
            relocated_levels.iter().any(|l| *l == SemverLevel::Minor),
            "modified relocation should be Minor: {:?}",
            report.breakdown
        );
    }

    #[test]
    fn no_false_relocation_when_content_totally_different() {
        let a = parse(
            r#"
            skill "main" {
                body {
                    step validate {
                        context { "Check schema." }
                    }
                }
            }
        "#,
        );
        let b = parse(
            r#"
            skill "vt" {
                body {
                    step validate {
                        context { "Completely different text." }
                    }
                }
            }
            skill "main" {
                body {
                    step run_vt {
                        use vt()
                        context { "Run." }
                    }
                }
            }
        "#,
        );
        let report = structural_diff(&a, &b);
        // With totally different content, it should still be Relocated but MODIFIED
        // (the step name matches but content differs)
        let validate_change = report
            .changes
            .iter()
            .find(|c| c.path.contains("step.validate"))
            .expect("should have a change for validate step");
        assert!(
            matches!(validate_change.kind, ChangeKind::Relocated),
            "should still be Relocated (name match in target): {}",
            report.display()
        );
        assert!(
            validate_change.description.contains("MODIFIED"),
            "should report as MODIFIED since content differs: {}",
            report.display()
        );
    }

    #[test]
    fn relocated_detects_dropped_internal_requires() {
        // transform originally requires validate (both are in the extraction).
        // Dropping that INTERNAL requires is a genuine modification, not an adaptation.
        let a = parse(
            r#"
            skill "main" {
                body {
                    step validate { context { "Validate." } }
                    step transform {
                        requires validate
                        context { "Transform." }
                    }
                }
            }
        "#,
        );
        let b = parse(
            r#"
            skill "vt" {
                body {
                    step validate { context { "Validate." } }
                    step transform {
                        context { "Transform." }
                    }
                }
            }
            skill "main" {
                body {
                    step run_vt {
                        use vt()
                        context { "Run." }
                    }
                }
            }
        "#,
        );
        let report = structural_diff(&a, &b);
        let transform_change = report
            .changes
            .iter()
            .find(|c| c.path.contains("step.transform"))
            .expect("should have a change for transform");
        assert!(
            matches!(transform_change.kind, ChangeKind::Relocated),
            "should be Relocated: {}",
            report.display()
        );
        assert!(
            transform_change.description.contains("MODIFIED")
                && transform_change.description.contains("requires"),
            "internal requires drop should be flagged: {}",
            report.display()
        );
    }

    #[test]
    fn relocated_detects_removed_when_guard() {
        let a = parse(
            r#"
            skill "main" {
                body {
                    step validate {
                        context(priority: important, when: input.strict_mode) { "Validate." }
                    }
                }
            }
        "#,
        );
        let b = parse(
            r#"
            skill "vt" {
                body {
                    step validate {
                        context(priority: important) { "Validate." }
                    }
                }
            }
            skill "main" {
                body {
                    step run_vt {
                        use vt()
                        context { "Run." }
                    }
                }
            }
        "#,
        );
        let report = structural_diff(&a, &b);
        let validate_change = report
            .changes
            .iter()
            .find(|c| c.path.contains("step.validate"))
            .expect("should have a change for validate");
        assert!(
            matches!(validate_change.kind, ChangeKind::Relocated),
            "should be Relocated: {}",
            report.display()
        );
        assert!(
            validate_change.description.contains("MODIFIED")
                && validate_change.description.contains("when"),
            "should report when guard change: {}",
            report.display()
        );
    }

    // ── Extraction adaptation tests ──────────────────────────────────────

    #[test]
    fn external_requires_removal_is_adaptation_not_modification() {
        // validate originally requires ingest, but ingest isn't in the extracted skill.
        // Removing that outer-facing requires is an expected extraction adaptation.
        let a = parse(
            r#"
            skill "main" {
                body {
                    step ingest { context { "Ingest." } }
                    step validate {
                        requires ingest
                        context(priority: important) { "Validate." }
                    }
                    step transform {
                        requires validate
                        context(priority: important) { "Transform." }
                    }
                }
            }
        "#,
        );
        let b = parse(
            r#"
            skill "vt" {
                body {
                    step validate {
                        context(priority: important) { "Validate." }
                    }
                    step transform {
                        requires validate
                        context(priority: important) { "Transform." }
                    }
                }
            }
            skill "main" {
                body {
                    step ingest { context { "Ingest." } }
                    step run_vt {
                        requires ingest
                        use vt()
                        context { "Run." }
                    }
                }
            }
        "#,
        );
        let report = structural_diff(&a, &b);
        let validate_change = report
            .changes
            .iter()
            .find(|c| c.path.contains("step.validate"))
            .expect("should have a change for validate");
        assert!(
            matches!(validate_change.kind, ChangeKind::Relocated),
            "should be Relocated: {}",
            report.display()
        );
        assert!(
            validate_change.description.contains("behaviour preserved"),
            "external requires removal should be treated as adaptation, not MODIFIED: {}",
            report.display()
        );
    }

    #[test]
    fn internal_requires_removal_is_still_flagged() {
        // transform originally requires validate, and both are in the extracted skill.
        // Removing that internal requires IS a genuine modification.
        let a = parse(
            r#"
            skill "main" {
                body {
                    step validate {
                        context { "Validate." }
                    }
                    step transform {
                        requires validate
                        context { "Transform." }
                    }
                }
            }
        "#,
        );
        let b = parse(
            r#"
            skill "vt" {
                body {
                    step validate {
                        context { "Validate." }
                    }
                    step transform {
                        context { "Transform." }
                    }
                }
            }
            skill "main" {
                body {
                    step run_vt {
                        use vt()
                        context { "Run." }
                    }
                }
            }
        "#,
        );
        let report = structural_diff(&a, &b);
        let transform_change = report
            .changes
            .iter()
            .find(|c| c.path.contains("step.transform"))
            .expect("should have a change for transform");
        assert!(
            matches!(transform_change.kind, ChangeKind::Relocated),
            "should be Relocated: {}",
            report.display()
        );
        assert!(
            transform_change.description.contains("MODIFIED")
                && transform_change.description.contains("requires"),
            "internal requires removal should still be flagged: {}",
            report.display()
        );
    }

    #[test]
    fn emit_change_is_adaptation_not_modification() {
        // export had emit output in the original. When transform moves to the
        // extracted skill and gains emit output, that's an expected adaptation.
        let a = parse(
            r#"
            skill "main" {
                body {
                    step transform {
                        context { "Transform." }
                    }
                    step export {
                        requires transform
                        emit output
                        context { "Export." }
                    }
                }
            }
        "#,
        );
        let b = parse(
            r#"
            skill "vt" {
                body {
                    step transform {
                        emit output
                        context { "Transform." }
                    }
                }
            }
            skill "main" {
                body {
                    step run_vt {
                        use vt()
                        context { "Run." }
                    }
                    step export {
                        requires run_vt
                        emit output
                        context { "Export." }
                    }
                }
            }
        "#,
        );
        let report = structural_diff(&a, &b);
        let transform_change = report
            .changes
            .iter()
            .find(|c| c.path.contains("step.transform"))
            .expect("should have a change for transform");
        assert!(
            matches!(transform_change.kind, ChangeKind::Relocated),
            "should be Relocated: {}",
            report.display()
        );
        assert!(
            transform_change.description.contains("behaviour preserved"),
            "emit change should be treated as adaptation: {}",
            report.display()
        );
    }
}
