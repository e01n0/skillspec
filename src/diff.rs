/// Structural diff for SkillSpec ASTs and compiled SKILL.md files.
///
/// Two public entry points:
///   - `structural_diff(a, b)` compares two parsed `SourceFile` ASTs.
///   - `skillmd_diff(compiled, actual)` compares a compiled SKILL.md string
///     against an on-disk SKILL.md, grouped by `## ` section headers.
use crate::ast::{ContextBlock, Field, SourceFile, TypeExpr};

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
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
            diff_steps(skill_a, skill_b, &prefix, report);

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
            && (!type_expr_eq(&fa.ty, &fb.ty) || fa.optional != fb.optional) {
                report.add(
                    ChangeKind::Modified,
                    format!("{}.{}", path_prefix, fa.name),
                    format!(
                        "field '{}' changed: type or optionality modified",
                        fa.name
                    ),
                );
            }
    }
}

fn diff_steps(
    skill_a: &crate::ast::Skill,
    skill_b: &crate::ast::Skill,
    prefix: &str,
    report: &mut DiffReport,
) {
    let a_steps = &skill_a.body.steps;
    let b_steps = &skill_b.body.steps;

    // Removed steps
    for step in a_steps {
        if !b_steps.iter().any(|s| s.name == step.name) {
            report.add(
                ChangeKind::Removed,
                format!("{}.step.{}", prefix, step.name),
                format!("step '{}' was removed", step.name),
            );
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
        if ca.text != cb.text || ca.priority != cb.priority || ca.decay != cb.decay {
            report.add(
                ChangeKind::Modified,
                format!("{}.context[{}]", path_prefix, i),
                format!(
                    "context block {} changed (priority: {:?}→{:?}, text changed: {})",
                    i,
                    ca.priority,
                    cb.priority,
                    ca.text != cb.text
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
                    && let Edit::Add(ai) = &edits[idx + 1] {
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
        assert!(report
            .changes
            .iter()
            .any(|c| matches!(c.kind, ChangeKind::Added) && c.path.contains("step.b")));
    }

    #[test]
    fn detects_removed_field() {
        let a = parse(
            r#"skill "x" { input { name: string  age: int } body { context { "ok" } } }"#,
        );
        let b = parse(r#"skill "x" { input { name: string } body { context { "ok" } } }"#);
        let report = structural_diff(&a, &b);
        assert!(report
            .changes
            .iter()
            .any(|c| matches!(c.kind, ChangeKind::Removed) && c.path.contains("age")));
    }

    #[test]
    fn detects_modified_context() {
        let a = parse(r#"skill "x" { body { context(priority: 90) { "Original." } } }"#);
        let b = parse(r#"skill "x" { body { context(priority: 50) { "Changed." } } }"#);
        let report = structural_diff(&a, &b);
        assert!(report
            .changes
            .iter()
            .any(|c| matches!(c.kind, ChangeKind::Modified)));
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
}
