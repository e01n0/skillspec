//! Cross-skill rule extraction and conflict detection.
//!
//! Extracts atomic imperative rules from every skill in a tree — both
//! `.agent` sources and plain `SKILL.md` files — and runs the deterministic
//! conflict tiers over the whole fleet:
//!
//! - Tier 0: exact duplicates and drifted near-duplicates (the "fixed it in
//!   one place, stale copy survives elsewhere" class)
//! - Tier 1: polarity clashes on shared subjects, and priority mismatches
//!   between near-identical rules
//!
//! Findings are pinned in a `rules.lock` baseline so CI can fail on *new*
//! conflicts only (`--check`), and false positives can be permanently
//! accepted. Design informed by docs/research-conflict-detection.md:
//! deterministic tiers target only the "easy" contradiction types
//! (polarity, duplication), scope filtering runs before conflict features,
//! and ~84% precision is the expected ceiling — hence the baseline.

use crate::ast::Priority;
use crate::lexer::Lexer;
use crate::parser::Parser;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};

// Similarity thresholds. Token-set Jaccard is over normalized tokens.
// 0.65 ≈ "clearly the same rule, edited"; below that, treat as different
// rules and let the polarity tier decide.
const DRIFT_JACCARD: f64 = 0.65;
// Polarity clashes need a meaningful shared subject: at least two shared
// content keywords and ~a third of the combined keyword set in common
// (0.30 keeps the 2-shared-of-6 boundary case, e.g. "run the linter" pairs).
const CLASH_MIN_SHARED: usize = 2;
const CLASH_JACCARD: f64 = 0.30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    Positive,
    Negative,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub text: String,
    pub skill: String,
    pub file: String,
    pub line: usize,
    pub priority: Option<Priority>,
    /// Compile target restriction, when the source context declared one.
    pub target: Option<String>,
    pub polarity: Polarity,
    /// Normalized full-token sequence (for duplicate/drift comparison).
    pub tokens: Vec<String>,
    /// Content keywords: tokens minus stopwords and modality markers.
    pub keywords: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    Duplicate,
    DriftedDuplicate,
    PolarityConflict,
    PriorityMismatch,
}

impl FindingKind {
    pub fn label(self) -> &'static str {
        match self {
            FindingKind::Duplicate => "duplicate-rule",
            FindingKind::DriftedDuplicate => "drifted-duplicate",
            FindingKind::PolarityConflict => "polarity-conflict",
            FindingKind::PriorityMismatch => "priority-mismatch",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub kind: FindingKind,
    pub a: usize,
    pub b: usize,
    /// Stable identity across line moves and reformatting: hash of kind +
    /// both normalized texts (order-independent).
    pub id: String,
    pub shared: Vec<String>,
}

pub struct RuleSet {
    pub rules: Vec<Rule>,
    pub files_scanned: usize,
    pub warnings: Vec<String>,
}

// ── Extraction ──────────────────────────────────────────────────────────────

/// Walk `root` collecting rules from every `.agent` file and every
/// `SKILL.md` that is not the compiled artifact of a sibling `.agent`.
pub fn extract_tree(root: &Path) -> Result<RuleSet, String> {
    let mut agent_files = Vec::new();
    let mut skillmd_files = Vec::new();
    if root.is_file() {
        classify_file(root, &mut agent_files, &mut skillmd_files);
    } else {
        walk(root, &mut agent_files, &mut skillmd_files, 0)?;
    }

    let mut rules = Vec::new();
    let mut warnings = Vec::new();
    let mut agent_skill_names: HashSet<String> = HashSet::new();
    let mut files_scanned = 0;

    for path in &agent_files {
        match extract_agent_file(path) {
            Ok((mut file_rules, names)) => {
                files_scanned += 1;
                agent_skill_names.extend(names);
                rules.append(&mut file_rules);
            }
            Err(e) => warnings.push(format!("{}: {}", path.display(), e)),
        }
    }

    for path in &skillmd_files {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                warnings.push(format!("{}: {}", path.display(), e));
                continue;
            }
        };
        let skill_name = markdown_skill_name(&source, path);
        // A SKILL.md whose name matches an .agent skill in the same tree is
        // a build artifact — the .agent file is the source of truth.
        if agent_skill_names.contains(&skill_name) {
            continue;
        }
        files_scanned += 1;
        rules.extend(extract_markdown(&source, &skill_name, path));
    }

    Ok(RuleSet {
        rules,
        files_scanned,
        warnings,
    })
}

fn classify_file(path: &Path, agents: &mut Vec<PathBuf>, skillmds: &mut Vec<PathBuf>) {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.ends_with(".agent") {
        agents.push(path.to_path_buf());
    } else if name == "SKILL.md" {
        skillmds.push(path.to_path_buf());
    }
}

fn walk(
    dir: &Path,
    agents: &mut Vec<PathBuf>,
    skillmds: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<(), String> {
    if depth > 12 {
        return Ok(());
    }
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("failed to read '{}': {}", dir.display(), e))?;
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            walk(&path, agents, skillmds, depth + 1)?;
        } else {
            classify_file(&path, agents, skillmds);
        }
    }
    Ok(())
}

fn extract_agent_file(path: &Path) -> Result<(Vec<Rule>, Vec<String>), String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("failed to read: {}", e))?;
    let tokens = Lexer::new(&source)
        .tokenize()
        .map_err(|e| format!("lex error: {}", e))?;
    let ast = Parser::new(tokens)
        .parse()
        .map_err(|e| format!("parse error: {}", e))?;

    let file = path.display().to_string();
    let mut rules = Vec::new();
    let mut names = Vec::new();

    for skill in &ast.skills {
        names.push(skill.name.clone());
        for ctx in &skill.body.contexts {
            extract_prose(
                &ctx.text,
                &skill.name,
                &file,
                ctx.span.line,
                ctx.priority,
                ctx.target.clone(),
                true,
                &mut rules,
            );
        }
        for step in &skill.body.steps {
            for ctx in &step.contexts {
                extract_prose(
                    &ctx.text,
                    &skill.name,
                    &file,
                    ctx.span.line,
                    ctx.priority,
                    ctx.target.clone(),
                    true,
                    &mut rules,
                );
            }
        }
    }
    Ok((rules, names))
}

fn markdown_skill_name(source: &str, path: &Path) -> String {
    // Frontmatter `name:` wins; fall back to the containing directory name.
    let mut lines = source.lines();
    if lines.next().map(str::trim) == Some("---") {
        for line in lines {
            let trimmed = line.trim();
            if trimmed == "---" {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("name:") {
                return rest.trim().to_string();
            }
        }
    }
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn extract_markdown(source: &str, skill: &str, path: &Path) -> Vec<Rule> {
    let file = path.display().to_string();
    let mut rules = Vec::new();
    let mut in_fence = false;
    let mut in_frontmatter = false;

    for (i, raw_line) in source.lines().enumerate() {
        let line_no = i + 1;
        let trimmed = raw_line.trim();

        if line_no == 1 && trimmed == "---" {
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter {
            if trimmed == "---" {
                in_frontmatter = false;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('|') {
            continue;
        }

        let content = trimmed.trim_start_matches("> ").trim();
        let (content, is_bullet) = strip_bullet(content);

        // Bullets in a skill document are rules by convention. Plain prose
        // sentences only count when they carry an imperative signal —
        // otherwise descriptions and examples flood the inventory.
        extract_prose(
            content, skill, &file, line_no, None, None, is_bullet, &mut rules,
        );
    }
    rules
}

fn strip_bullet(line: &str) -> (&str, bool) {
    for prefix in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return (rest.trim(), true);
        }
    }
    // Numbered list: "1. text" / "12. text"
    if let Some(dot) = line.find(". ")
        && dot <= 3
        && line[..dot].chars().all(|c| c.is_ascii_digit())
    {
        return (line[dot + 2..].trim(), true);
    }
    (line, false)
}

#[allow(clippy::too_many_arguments)]
fn extract_prose(
    text: &str,
    skill: &str,
    file: &str,
    line: usize,
    priority: Option<Priority>,
    target: Option<String>,
    unconditional: bool,
    out: &mut Vec<Rule>,
) {
    for sentence in split_sentences(text) {
        let tokens = normalize_tokens(&sentence);
        if tokens.len() < 3 {
            continue;
        }
        if !unconditional && !has_imperative_signal(&sentence) {
            continue;
        }
        let keywords = content_keywords(&sentence);
        if keywords.is_empty() {
            continue;
        }
        out.push(Rule {
            polarity: detect_polarity(&sentence),
            text: sentence,
            skill: skill.to_string(),
            file: file.to_string(),
            line,
            priority,
            target: target.clone(),
            tokens,
            keywords,
        });
    }
}

fn split_sentences(text: &str) -> Vec<String> {
    // Rejoin wrapped prose first: triple-string contexts hard-wrap
    // sentences across lines, and splitting per line produces fragments
    // that poison keyword overlap. Bullets stay their own unit; plain
    // lines merge into their paragraph.
    let mut units: Vec<String> = Vec::new();
    let mut paragraph = String::new();
    let flush = |p: &mut String, units: &mut Vec<String>| {
        if !p.trim().is_empty() {
            units.push(std::mem::take(p).trim().to_string());
        } else {
            p.clear();
        }
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(&mut paragraph, &mut units);
            continue;
        }
        let (content, is_bullet) = strip_bullet(trimmed);
        if is_bullet {
            flush(&mut paragraph, &mut units);
            units.push(content.to_string());
        } else {
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(trimmed);
        }
    }
    flush(&mut paragraph, &mut units);

    let mut out = Vec::new();
    for unit in units {
        for part in unit.split(". ") {
            let s = part.trim().trim_end_matches('.').trim();
            if !s.is_empty() {
                out.push(s.to_string());
            }
        }
    }
    out
}

/// Conditional branches ("If X ..." / "If not X ...") are scope-disjoint by
/// construction — opposite polarity between two guarded rules is usually a
/// pair of complementary branches, not a contradiction.
fn is_conditional(sentence: &str) -> bool {
    let lower = sentence.to_lowercase();
    lower.starts_with("if ") || lower.starts_with("when ") || lower.starts_with("unless ")
}

// Words that mark a sentence as an instruction rather than a description.
// "not"/"do"/"only" are excluded here — too common in plain prose — but
// still stripped from content keywords below.
const SIGNAL_MODALITY: &[&str] = &[
    "always", "never", "must", "should", "shall", "ensure", "avoid", "prefer", "don't", "dont",
];

// Modality-ish words excluded from content keywords so subject overlap is
// measured on what the rule is *about*, not how forcefully it says it.
const MODALITY: &[&str] = &[
    "always", "never", "must", "should", "shall", "ensure", "avoid", "prefer", "don't", "dont",
    "not", "only", "do",
];

const IMPERATIVE_STARTS: &[&str] = &[
    "use", "run", "keep", "write", "prefer", "avoid", "check", "ensure", "follow", "report",
    "include", "add", "remove", "treat", "mark", "return", "always", "never", "don't", "do",
    "make", "verify", "skip", "stop", "ask", "read", "test", "commit", "review",
];

fn has_imperative_signal(sentence: &str) -> bool {
    let lower = sentence.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    if let Some(first) = words.first()
        && IMPERATIVE_STARTS.contains(first)
    {
        return true;
    }
    words.iter().any(|w| SIGNAL_MODALITY.contains(w))
}

fn detect_polarity(sentence: &str) -> Polarity {
    let lower = format!(" {} ", sentence.to_lowercase());
    const NEGATIVE: &[&str] = &[
        " never ",
        " don't ",
        " dont ",
        " do not ",
        " must not ",
        " should not ",
        " shall not ",
        " cannot ",
        " can't ",
        " avoid ",
        " not ",
        " no ",
    ];
    if NEGATIVE.iter().any(|m| lower.contains(m)) || lower.starts_with(" never") {
        Polarity::Negative
    } else {
        Polarity::Positive
    }
}

const STOPWORDS: &[&str] = &[
    "the", "a", "an", "to", "of", "in", "on", "for", "with", "and", "or", "if", "is", "are", "be",
    "it", "this", "that", "these", "those", "you", "your", "when", "then", "as", "at", "by",
    "from", "into", "any", "all", "each", "every", "its", "their", "them", "they", "we", "i",
    "will", "can", "may", "might", "would", "could", "there", "here", "than", "so", "up", "out",
    "about", "before", "after", "while", "during",
];

/// Lowercased alphanumeric tokens, unstemmed. Stopword/modality filtering
/// must run on these — stemming first would turn "always" into "alway" and
/// let it slip past the modality list into content keywords.
fn raw_tokens(sentence: &str) -> Vec<String> {
    sentence
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(|s| s.trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn normalize_tokens(sentence: &str) -> Vec<String> {
    raw_tokens(sentence).iter().map(|t| stem(t)).collect()
}

/// Crude suffix stemmer — enough to make "linting"/"linted"/"lints" collide.
fn stem(word: &str) -> String {
    let w = word.trim_matches('\'');
    if w.len() > 5 && w.ends_with("ing") {
        return w[..w.len() - 3].to_string();
    }
    if w.len() > 4 && w.ends_with("ed") {
        return w[..w.len() - 2].to_string();
    }
    if w.len() > 4 && w.ends_with("es") {
        return w[..w.len() - 2].to_string();
    }
    if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") {
        return w[..w.len() - 1].to_string();
    }
    w.to_string()
}

fn content_keywords(sentence: &str) -> HashSet<String> {
    raw_tokens(sentence)
        .iter()
        .filter(|t| {
            t.len() > 1 && !STOPWORDS.contains(&t.as_str()) && !MODALITY.contains(&t.as_str())
        })
        .map(|t| stem(t))
        .collect()
}

// ── Detection ───────────────────────────────────────────────────────────────

pub fn detect(rules: &[Rule]) -> Vec<Finding> {
    let mut findings = Vec::new();

    for i in 0..rules.len() {
        for j in (i + 1)..rules.len() {
            let (a, b) = (&rules[i], &rules[j]);

            // Scope filter (co-activation): contexts pinned to different
            // compile targets can never be loaded together.
            if let (Some(ta), Some(tb)) = (&a.target, &b.target)
                && ta != tb
            {
                continue;
            }
            // A rule repeated at the same site is not a fleet problem.
            if a.file == b.file && a.line == b.line {
                continue;
            }

            let token_sim = jaccard_vec(&a.tokens, &b.tokens);

            if a.tokens == b.tokens {
                if a.skill != b.skill || a.file != b.file {
                    findings.push(make_finding(FindingKind::Duplicate, i, j, a, b, vec![]));
                }
                continue;
            }

            if token_sim >= DRIFT_JACCARD {
                findings.push(make_finding(
                    FindingKind::DriftedDuplicate,
                    i,
                    j,
                    a,
                    b,
                    vec![],
                ));
                if a.priority.is_some() && b.priority.is_some() && a.priority != b.priority {
                    findings.push(make_finding(
                        FindingKind::PriorityMismatch,
                        i,
                        j,
                        a,
                        b,
                        vec![],
                    ));
                }
                continue;
            }

            if a.polarity != b.polarity && !(is_conditional(&a.text) && is_conditional(&b.text)) {
                let shared: Vec<String> = {
                    let mut s: Vec<String> =
                        a.keywords.intersection(&b.keywords).cloned().collect();
                    s.sort();
                    s
                };
                let kw_sim = jaccard_set(&a.keywords, &b.keywords);
                if shared.len() >= CLASH_MIN_SHARED && kw_sim >= CLASH_JACCARD {
                    findings.push(make_finding(
                        FindingKind::PolarityConflict,
                        i,
                        j,
                        a,
                        b,
                        shared,
                    ));
                }
            }
        }
    }

    findings
}

fn make_finding(
    kind: FindingKind,
    i: usize,
    j: usize,
    a: &Rule,
    b: &Rule,
    shared: Vec<String>,
) -> Finding {
    let (first, second) = if a.tokens <= b.tokens {
        (&a.tokens, &b.tokens)
    } else {
        (&b.tokens, &a.tokens)
    };
    let identity = format!("{}|{}|{}", kind.label(), first.join(" "), second.join(" "));
    Finding {
        kind,
        a: i,
        b: j,
        id: format!("{:016x}", fnv1a64(identity.as_bytes())),
        shared,
    }
}

fn jaccard_vec(a: &[String], b: &[String]) -> f64 {
    let sa: HashSet<&String> = a.iter().collect();
    let sb: HashSet<&String> = b.iter().collect();
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

fn jaccard_set(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    let inter = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

// ── Lockfile ────────────────────────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RulesLock {
    pub version: u32,
    pub rule_count: usize,
    /// Finding id → human-readable description. Presence means "known and
    /// accepted": either a real conflict being tracked or a false positive.
    pub accepted: BTreeMap<String, String>,
}

pub fn write_lock(path: &Path, rules: &[Rule], findings: &[Finding]) -> Result<(), String> {
    let mut accepted = BTreeMap::new();
    for f in findings {
        accepted.insert(f.id.clone(), describe_short(f, rules));
    }
    let lock = RulesLock {
        version: 1,
        rule_count: rules.len(),
        accepted,
    };
    let json = serde_json::to_string_pretty(&lock).map_err(|e| e.to_string())?;
    std::fs::write(path, json + "\n")
        .map_err(|e| format!("failed to write '{}': {}", path.display(), e))
}

pub fn read_lock(path: &Path) -> Result<RulesLock, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read '{}': {}", path.display(), e))?;
    serde_json::from_str(&content).map_err(|e| format!("invalid lockfile: {}", e))
}

pub struct CheckOutcome {
    pub new: Vec<Finding>,
    pub known: usize,
    pub resolved: Vec<String>,
}

pub fn check_against_lock(lock: &RulesLock, findings: &[Finding]) -> CheckOutcome {
    let current: HashSet<&str> = findings.iter().map(|f| f.id.as_str()).collect();
    let new: Vec<Finding> = findings
        .iter()
        .filter(|f| !lock.accepted.contains_key(&f.id))
        .cloned()
        .collect();
    let resolved: Vec<String> = lock
        .accepted
        .iter()
        .filter(|(id, _)| !current.contains(id.as_str()))
        .map(|(_, desc)| desc.clone())
        .collect();
    CheckOutcome {
        known: findings.len() - new.len(),
        new,
        resolved,
    }
}

// ── Reporting ───────────────────────────────────────────────────────────────

fn describe_short(f: &Finding, rules: &[Rule]) -> String {
    let a = &rules[f.a];
    let b = &rules[f.b];
    format!(
        "{}: [{}] \"{}\" <-> [{}] \"{}\"",
        f.kind.label(),
        a.skill,
        truncate(&a.text, 60),
        b.skill,
        truncate(&b.text, 60),
    )
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{}…", cut)
    }
}

pub fn render_findings(findings: &[Finding], rules: &[Rule]) -> String {
    let mut out = String::new();
    for f in findings {
        let a = &rules[f.a];
        let b = &rules[f.b];
        let scope = if a.skill == b.skill {
            "within-skill"
        } else {
            "cross-skill"
        };
        let _ = writeln!(out, "{} [{}] (id {})", f.kind.label(), scope, f.id);
        let _ = writeln!(
            out,
            "  A {}:{} [{}] \"{}\"",
            a.file, a.line, a.skill, a.text
        );
        let _ = writeln!(
            out,
            "  B {}:{} [{}] \"{}\"",
            b.file, b.line, b.skill, b.text
        );
        if !f.shared.is_empty() {
            let _ = writeln!(out, "  shared subject: {}", f.shared.join(", "));
        }
        if let (Some(pa), Some(pb)) = (a.priority, b.priority)
            && pa != pb
        {
            let _ = writeln!(out, "  priorities: {} vs {}", pa, pb);
        }
        out.push('\n');
    }
    out
}

pub fn summarize(findings: &[Finding]) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for f in findings {
        *counts.entry(f.kind.label()).or_insert(0) += 1;
    }
    counts
        .iter()
        .map(|(k, v)| format!("{} {}", v, k))
        .collect::<Vec<_>>()
        .join(", ")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(text: &str, skill: &str, file: &str, line: usize) -> Rule {
        let tokens = normalize_tokens(text);
        let keywords = content_keywords(text);
        Rule {
            polarity: detect_polarity(text),
            text: text.to_string(),
            skill: skill.to_string(),
            file: file.to_string(),
            line,
            priority: None,
            target: None,
            tokens,
            keywords,
        }
    }

    #[test]
    fn detects_exact_duplicate_across_skills() {
        let rules = vec![
            rule(
                "Always run the test suite before committing",
                "a",
                "a.md",
                3,
            ),
            rule(
                "Always run the test suite before committing",
                "b",
                "b.md",
                7,
            ),
        ];
        let findings = detect(&rules);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, FindingKind::Duplicate);
    }

    #[test]
    fn detects_drifted_duplicate() {
        let rules = vec![
            rule(
                "Always run the full test suite before committing changes",
                "a",
                "a.md",
                3,
            ),
            rule(
                "Always run the full test suite before pushing changes",
                "b",
                "b.md",
                7,
            ),
        ];
        let findings = detect(&rules);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == FindingKind::DriftedDuplicate),
            "got: {:?}",
            findings.iter().map(|f| f.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn detects_polarity_conflict() {
        let rules = vec![
            rule("Always run the linter before committing", "a", "a.md", 3),
            rule("Never run the linter on generated files", "b", "b.md", 7),
        ];
        let findings = detect(&rules);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == FindingKind::PolarityConflict),
            "got: {:?}",
            findings.iter().map(|f| f.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn unrelated_rules_do_not_conflict() {
        let rules = vec![
            rule("Always write descriptive commit messages", "a", "a.md", 3),
            rule("Never expose secret keys in logs", "b", "b.md", 7),
        ];
        assert!(detect(&rules).is_empty());
    }

    #[test]
    fn different_targets_never_co_activate() {
        let mut a = rule("Always use four spaces for indentation", "a", "a.md", 3);
        let mut b = rule("Never use spaces for indentation", "b", "b.md", 7);
        a.target = Some("cursor".to_string());
        b.target = Some("agentsmd".to_string());
        assert!(
            detect(&[a, b]).is_empty(),
            "different targets must be skipped"
        );
    }

    #[test]
    fn priority_mismatch_on_near_duplicates() {
        let mut a = rule(
            "Always validate user input before processing it fully",
            "a",
            "a.agent",
            3,
        );
        let mut b = rule(
            "Always validate user input before processing it carefully",
            "b",
            "b.agent",
            7,
        );
        a.priority = Some(Priority::Critical);
        b.priority = Some(Priority::Optional);
        let findings = detect(&[a, b]);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == FindingKind::PriorityMismatch),
            "got: {:?}",
            findings.iter().map(|f| f.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn markdown_extraction_bullets_and_fences() {
        let md = "\
---
name: demo
---

# Title

Some descriptive text that is not a rule at all.

- Always run tests before committing.
- Never push directly to main.

```sh
never run this code fence line
```

You must ask before deleting files.
";
        let rules = extract_markdown(md, "demo", Path::new("demo/SKILL.md"));
        let texts: Vec<&str> = rules.iter().map(|r| r.text.as_str()).collect();
        assert!(texts.iter().any(|t| t.contains("Always run tests")));
        assert!(texts.iter().any(|t| t.contains("Never push directly")));
        assert!(texts.iter().any(|t| t.contains("must ask before deleting")));
        assert!(
            !texts.iter().any(|t| t.contains("code fence")),
            "fenced code must be skipped: {:?}",
            texts
        );
        assert!(
            !texts.iter().any(|t| t.contains("descriptive text")),
            "non-imperative prose must be skipped: {:?}",
            texts
        );
        let never = rules
            .iter()
            .find(|r| r.text.contains("Never push"))
            .unwrap();
        assert_eq!(never.polarity, Polarity::Negative);
        assert_eq!(never.skill, "demo");
    }

    #[test]
    fn agent_extraction_carries_priority_and_target() {
        let src = r#"
            skill "x" {
                body {
                    context(priority: critical) { "Always run the linter." }
                    context(target: cursor) { "Never use tabs anywhere here." }
                    step go { context { "Report every failure to the user." } }
                }
            }
        "#;
        let dir = std::env::temp_dir().join("skillspec_rules_agent_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.agent");
        std::fs::write(&path, src).unwrap();
        let (rules, names) = extract_agent_file(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(names, vec!["x".to_string()]);
        assert_eq!(rules.len(), 3);
        let lint = rules.iter().find(|r| r.text.contains("linter")).unwrap();
        assert_eq!(lint.priority, Some(Priority::Critical));
        let tabs = rules.iter().find(|r| r.text.contains("tabs")).unwrap();
        assert_eq!(tabs.target.as_deref(), Some("cursor"));
        assert_eq!(tabs.polarity, Polarity::Negative);
    }

    #[test]
    fn modality_words_never_leak_into_keywords() {
        // "always"/"never" must be filtered BEFORE stemming — stemmed forms
        // like "alway" previously slipped past the modality list and diluted
        // keyword overlap enough to hide real conflicts.
        let kws = content_keywords("Never write focused diffs, always large ones");
        assert!(!kws.contains("alway"), "got: {:?}", kws);
        assert!(!kws.contains("never"), "got: {:?}", kws);
        assert!(
            kws.contains("focus") || kws.contains("focused"),
            "got: {:?}",
            kws
        );
    }

    #[test]
    fn new_conflicting_rule_is_detected_against_existing() {
        let rules = vec![
            rule("Prefer small focused diffs", "a", "a.md", 6),
            rule(
                "Never write focused diffs, always large ones",
                "b",
                "b.md",
                9,
            ),
        ];
        let findings = detect(&rules);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == FindingKind::PolarityConflict),
            "got: {:?}",
            findings.iter().map(|f| f.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn conditional_branches_are_not_conflicts() {
        let rules = vec![
            rule(
                "If source_dir is provided, use Bash to list the files",
                "a",
                "a.md",
                3,
            ),
            rule(
                "If source_dir is not provided, fall back to analyzing files",
                "a",
                "a.md",
                9,
            ),
        ];
        assert!(
            detect(&rules)
                .iter()
                .all(|f| f.kind != FindingKind::PolarityConflict),
            "complementary if-branches must not be polarity conflicts"
        );
    }

    #[test]
    fn wrapped_prose_is_rejoined_before_sentence_split() {
        let sentences = split_sentences(
            "You process the request (you are\nthe model that handles it). Keep replies short.",
        );
        assert!(
            sentences.iter().any(|s| s.contains("(you are the model")),
            "wrapped lines must merge into one sentence: {:?}",
            sentences
        );
    }

    #[test]
    fn lock_roundtrip_and_check() {
        let rules = vec![
            rule("Always run the linter before committing", "a", "a.md", 3),
            rule("Never run the linter on generated files", "b", "b.md", 7),
        ];
        let findings = detect(&rules);
        assert!(!findings.is_empty());

        let dir = std::env::temp_dir().join("skillspec_rules_lock_test");
        std::fs::create_dir_all(&dir).unwrap();
        let lock_path = dir.join("rules.lock");
        write_lock(&lock_path, &rules, &findings).unwrap();
        let lock = read_lock(&lock_path).unwrap();

        // Same findings → nothing new
        let outcome = check_against_lock(&lock, &findings);
        assert!(outcome.new.is_empty());
        assert_eq!(outcome.known, findings.len());

        // A new conflicting rule → new finding not in baseline
        let mut more = rules.clone();
        more.push(rule(
            "Never run the linter before committing anything",
            "c",
            "c.md",
            2,
        ));
        let more_findings = detect(&more);
        let outcome2 = check_against_lock(&lock, &more_findings);
        assert!(
            !outcome2.new.is_empty(),
            "new conflict must not be masked by baseline"
        );

        // Finding ids are stable across line moves
        let mut moved = rules.clone();
        moved[0].line = 99;
        moved[1].file = "elsewhere.md".to_string();
        let moved_findings = detect(&moved);
        let outcome3 = check_against_lock(&lock, &moved_findings);
        assert!(
            outcome3.new.is_empty(),
            "line moves must not invalidate the baseline"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
