use crate::ast::*;
use crate::compiler_skillmd::SkillMdCompiler;
use regex::Regex;

// ── Assertion evaluator ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AssertionResult {
    pub passed: bool,
    pub message: String,
}

pub fn evaluate_assertion(assertion: &AssertionExpr, actual: &serde_json::Value) -> AssertionResult {
    match assertion {
        AssertionExpr::Equals(expected) => {
            let expected_val = expr_to_json(expected);
            let passed = json_eq(actual, &expected_val);
            AssertionResult {
                passed,
                message: if passed {
                    "equals: pass".into()
                } else {
                    format!("equals: expected {}, got {}", expected_val, actual)
                },
            }
        }
        AssertionExpr::Contains(needle) => {
            let needle_str = expr_to_string_val(needle);
            let actual_str = actual.as_str().unwrap_or("");
            let passed = actual_str.contains(&needle_str);
            AssertionResult {
                passed,
                message: if passed {
                    "contains: pass".into()
                } else {
                    format!("contains: '{}' not found in '{}'", needle_str, actual_str)
                },
            }
        }
        AssertionExpr::Matches(pattern) => {
            let actual_str = actual.as_str().unwrap_or("");
            match Regex::new(pattern) {
                Ok(re) => {
                    let passed = re.is_match(actual_str);
                    AssertionResult {
                        passed,
                        message: if passed {
                            "matches: pass".into()
                        } else {
                            format!("matches: '{}' did not match /{}/", actual_str, pattern)
                        },
                    }
                }
                Err(e) => AssertionResult {
                    passed: false,
                    message: format!("matches: invalid regex '{}': {}", pattern, e),
                },
            }
        }
        AssertionExpr::Between(lo, hi) => {
            let lo_val = expr_to_f64(lo);
            let hi_val = expr_to_f64(hi);
            let actual_f = actual.as_f64().or_else(|| actual.as_i64().map(|i| i as f64)).unwrap_or(f64::NAN);
            let passed = actual_f >= lo_val && actual_f <= hi_val;
            AssertionResult {
                passed,
                message: if passed {
                    "between: pass".into()
                } else {
                    format!("between: {} not in [{}, {}]", actual_f, lo_val, hi_val)
                },
            }
        }
        AssertionExpr::Comparison(op, expected) => {
            let expected_f = expr_to_f64(expected);
            let actual_f = actual.as_f64().or_else(|| actual.as_i64().map(|i| i as f64)).unwrap_or(f64::NAN);
            let passed = match op {
                BinOp::Lt => actual_f < expected_f,
                BinOp::Gt => actual_f > expected_f,
                BinOp::LtEq => actual_f <= expected_f,
                BinOp::GtEq => actual_f >= expected_f,
                BinOp::Eq => (actual_f - expected_f).abs() < f64::EPSILON,
                BinOp::NotEq => (actual_f - expected_f).abs() >= f64::EPSILON,
                _ => false,
            };
            AssertionResult {
                passed,
                message: if passed {
                    "comparison: pass".into()
                } else {
                    format!("comparison: {} {:?} {} failed", actual_f, op, expected_f)
                },
            }
        }
        AssertionExpr::ContainsWhere(predicate) => {
            eval_quantifier(actual, predicate, "contains_where", |items, pred| {
                items.iter().any(|item| eval_predicate(item, pred))
            })
        }
        AssertionExpr::AllWhere(predicate) => {
            eval_quantifier(actual, predicate, "all_where", |items, pred| {
                items.iter().all(|item| eval_predicate(item, pred))
            })
        }
        AssertionExpr::NoneWhere(predicate) => {
            eval_quantifier(actual, predicate, "none_where", |items, pred| {
                !items.iter().any(|item| eval_predicate(item, pred))
            })
        }
        AssertionExpr::Resembles(_) => {
            let verdict = actual.get("resembles_verdict")
                .or_else(|| actual.as_bool().map(|_| actual))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            AssertionResult {
                passed: verdict,
                message: if verdict { "resembles: pass (LLM verdict)".into() } else { "resembles: fail (LLM verdict)".into() },
            }
        }
        AssertionExpr::Satisfies(_) => {
            let verdict = actual.get("satisfies_verdict")
                .and_then(|v| v.as_object())
                .and_then(|obj| obj.get("verdict"))
                .and_then(|v| v.as_bool())
                .or_else(|| actual.get("satisfies_verdict").and_then(|v| v.as_bool()))
                .unwrap_or(false);
            let reason = actual.get("satisfies_verdict")
                .and_then(|v| v.as_object())
                .and_then(|obj| obj.get("reason"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            AssertionResult {
                passed: verdict,
                message: if verdict {
                    "satisfies: pass (LLM verdict)".into()
                } else {
                    format!("satisfies: fail — {}", reason)
                },
            }
        }
    }
}

pub fn evaluate_confidence(results: &[bool], threshold: f64) -> bool {
    if results.is_empty() { return false; }
    let pass_rate = results.iter().filter(|&&r| r).count() as f64 / results.len() as f64;
    pass_rate >= threshold
}

// ── Test preparation ────────────────────────────────────────────────────────

pub fn prepare_test_skill(skill: &Skill, source: &SourceFile) -> String {
    let compiler = SkillMdCompiler::new();
    let compiled = compiler.compile(skill, source);

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("name: {}-test\n", skill.name));
    out.push_str(&format!("description: Test execution skill for {}\n", skill.name));
    out.push_str("---\n\n");

    out.push_str("# Test Execution Skill\n\n");
    out.push_str(&format!("This skill tests `{}`.\n\n", skill.name));

    out.push_str("## Skill Under Test\n\n");
    out.push_str("<details>\n<summary>Compiled SKILL.md</summary>\n\n");
    out.push_str(&compiled);
    out.push_str("\n</details>\n\n");

    out.push_str("## Test Cases\n\n");
    for test in &skill.tests {
        out.push_str(&format!("### {}\n\n", test.name));

        if !test.given.is_empty() {
            out.push_str("**Given inputs:**\n");
            for (name, val) in &test.given {
                out.push_str(&format!("- `{}` = `{:?}`\n", name, val));
            }
            out.push('\n');
        }

        if !test.mocks.is_empty() {
            out.push_str("**Mocks:**\n");
            for mock in &test.mocks {
                out.push_str(&format!("- Simulate `{}` as {:?}\n", mock.tool_path, mock.mock_type));
            }
            out.push('\n');
        }

        if let Some(runs) = test.runs {
            out.push_str(&format!("**Execute {} times** and return all results.\n\n", runs));
        }

        out.push_str("**Assertions (evaluate after execution):**\n");
        for exp in &test.expectations {
            out.push_str(&format!("- `{}`: {:?}\n", exp.path, exp.assertion));
        }

        for exp in &test.expectations {
            match &exp.assertion {
                AssertionExpr::Resembles(desc) => {
                    out.push_str(&format!("\n**LLM Judge instruction:** Evaluate whether `{}` resembles \"{}\". Include `\"resembles_verdict\": true/false` in the result JSON.\n", exp.path, desc));
                }
                AssertionExpr::Satisfies(criteria) => {
                    out.push_str(&format!("\n**LLM Judge instruction:** Evaluate whether `{}` satisfies \"{}\". Include `\"satisfies_verdict\": {{\"verdict\": true/false, \"reason\": \"...\"}}` in the result JSON.\n", exp.path, criteria));
                }
                _ => {}
            }
        }
        out.push('\n');
    }

    out.push_str("## Output Format\n\n");
    out.push_str("Return results as JSON matching this schema:\n\n");
    out.push_str("```json\n");
    out.push_str("{\n");
    out.push_str(&format!("  \"skill\": \"{}\",\n", skill.name));
    out.push_str("  \"test_cases\": [\n");
    out.push_str("    {\n");
    out.push_str("      \"name\": \"test name\",\n");
    out.push_str("      \"runs\": [\n");
    out.push_str("        { \"output\": { ... }, \"resembles_verdicts\": {}, \"satisfies_verdicts\": {} }\n");
    out.push_str("      ]\n");
    out.push_str("    }\n");
    out.push_str("  ]\n");
    out.push_str("}\n");
    out.push_str("```\n");

    out
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn expr_to_json(expr: &Expr) -> serde_json::Value {
    match expr {
        Expr::StringLit(s) => serde_json::Value::String(s.clone()),
        Expr::IntLit(i) => serde_json::json!(*i),
        Expr::FloatLit(f) => serde_json::json!(*f),
        Expr::BoolLit(b) => serde_json::json!(*b),
        _ => serde_json::Value::String(format!("{:?}", expr)),
    }
}

fn expr_to_string_val(expr: &Expr) -> String {
    match expr {
        Expr::StringLit(s) => s.clone(),
        Expr::Interpolated(s) => s.clone(),
        other => format!("{:?}", other),
    }
}

fn expr_to_f64(expr: &Expr) -> f64 {
    match expr {
        Expr::IntLit(i) => *i as f64,
        Expr::FloatLit(f) => *f,
        _ => f64::NAN,
    }
}

fn json_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a, b) {
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a == b,
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            a.as_f64().unwrap_or(f64::NAN) == b.as_f64().unwrap_or(f64::NAN)
        }
        (serde_json::Value::Bool(a), serde_json::Value::Bool(b)) => a == b,
        _ => a == b,
    }
}

fn eval_predicate(item: &serde_json::Value, predicate: &Expr) -> bool {
    if let Expr::BinOp(lhs, BinOp::Eq, rhs) = predicate {
        if let Expr::FieldAccess(_, field) = lhs.as_ref() {
            let actual = item.get(field).cloned().unwrap_or(serde_json::Value::Null);
            let expected = expr_to_json(rhs);
            return json_eq(&actual, &expected);
        }
    }
    false
}

fn eval_quantifier(
    actual: &serde_json::Value,
    predicate: &Expr,
    name: &str,
    check: impl Fn(&[serde_json::Value], &Expr) -> bool,
) -> AssertionResult {
    match actual.as_array() {
        Some(items) => {
            let passed = check(items, predicate);
            AssertionResult {
                passed,
                message: if passed {
                    format!("{}: pass", name)
                } else {
                    format!("{}: failed over {} items", name, items.len())
                },
            }
        }
        None => AssertionResult {
            passed: false,
            message: format!("{}: value is not an array", name),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str_val(s: &str) -> serde_json::Value { serde_json::Value::String(s.into()) }
    fn int_val(i: i64) -> serde_json::Value { serde_json::json!(i) }

    #[test]
    fn eval_equals_pass() {
        let r = evaluate_assertion(
            &AssertionExpr::Equals(Expr::StringLit("hello".into())),
            &str_val("hello"),
        );
        assert!(r.passed);
    }

    #[test]
    fn eval_equals_fail() {
        let r = evaluate_assertion(
            &AssertionExpr::Equals(Expr::StringLit("hello".into())),
            &str_val("world"),
        );
        assert!(!r.passed);
        assert!(r.message.contains("expected"));
    }

    #[test]
    fn eval_contains_pass() {
        let r = evaluate_assertion(
            &AssertionExpr::Contains(Expr::StringLit("foo".into())),
            &str_val("foo bar baz"),
        );
        assert!(r.passed);
    }

    #[test]
    fn eval_matches_regex_pass() {
        let r = evaluate_assertion(
            &AssertionExpr::Matches(r"\d+".into()),
            &str_val("42"),
        );
        assert!(r.passed);
    }

    #[test]
    fn eval_matches_regex_fail() {
        let r = evaluate_assertion(
            &AssertionExpr::Matches(r"^\d+$".into()),
            &str_val("abc"),
        );
        assert!(!r.passed);
    }

    #[test]
    fn eval_between_pass() {
        let r = evaluate_assertion(
            &AssertionExpr::Between(Expr::IntLit(1), Expr::IntLit(10)),
            &int_val(5),
        );
        assert!(r.passed);
    }

    #[test]
    fn eval_comparison_gte_pass() {
        let r = evaluate_assertion(
            &AssertionExpr::Comparison(BinOp::GtEq, Expr::IntLit(3)),
            &int_val(5),
        );
        assert!(r.passed);
    }

    #[test]
    fn eval_comparison_lt_fail() {
        let r = evaluate_assertion(
            &AssertionExpr::Comparison(BinOp::Lt, Expr::IntLit(3)),
            &int_val(5),
        );
        assert!(!r.passed);
    }

    #[test]
    fn eval_contains_where_pass() {
        let arr = serde_json::json!([{"status": "active"}, {"status": "inactive"}]);
        let pred = Expr::BinOp(
            Box::new(Expr::FieldAccess(Box::new(Expr::Ident("item".into())), "status".into())),
            BinOp::Eq,
            Box::new(Expr::StringLit("active".into())),
        );
        let r = evaluate_assertion(&AssertionExpr::ContainsWhere(pred), &arr);
        assert!(r.passed);
    }

    #[test]
    fn eval_all_where_fail() {
        let arr = serde_json::json!([{"status": "active"}, {"status": "inactive"}]);
        let pred = Expr::BinOp(
            Box::new(Expr::FieldAccess(Box::new(Expr::Ident("item".into())), "status".into())),
            BinOp::Eq,
            Box::new(Expr::StringLit("active".into())),
        );
        let r = evaluate_assertion(&AssertionExpr::AllWhere(pred), &arr);
        assert!(!r.passed);
    }

    #[test]
    fn eval_none_where_pass() {
        let arr = serde_json::json!([{"status": "active"}]);
        let pred = Expr::BinOp(
            Box::new(Expr::FieldAccess(Box::new(Expr::Ident("item".into())), "status".into())),
            BinOp::Eq,
            Box::new(Expr::StringLit("deleted".into())),
        );
        let r = evaluate_assertion(&AssertionExpr::NoneWhere(pred), &arr);
        assert!(r.passed);
    }

    #[test]
    fn eval_resembles_reads_llm_verdict_pass() {
        let result = serde_json::json!({"resembles_verdict": true});
        let r = evaluate_assertion(&AssertionExpr::Resembles("a greeting".into()), &result);
        assert!(r.passed);
    }

    #[test]
    fn eval_satisfies_reads_llm_verdict_fail() {
        let result = serde_json::json!({
            "satisfies_verdict": { "verdict": false, "reason": "contains mild language" }
        });
        let r = evaluate_assertion(&AssertionExpr::Satisfies("no profanity".into()), &result);
        assert!(!r.passed);
        assert!(r.message.contains("mild language"));
    }

    #[test]
    fn eval_confidence_met() {
        let results = vec![true, true, true, true, true, true, true, true, true, false];
        assert!(evaluate_confidence(&results, 0.8));
    }

    #[test]
    fn eval_confidence_not_met() {
        let results = vec![true, true, true, true, true, true, true, false, false, false];
        assert!(!evaluate_confidence(&results, 0.9));
    }

    #[test]
    fn prepare_generates_valid_skillmd() {
        let source = r#"
            skill "x" {
                input { query: string }
                body { context { "Answer." } }
                tests {
                    test "basic" {
                        given { query: "hello" }
                        expect {
                            output.result: equals("world")
                        }
                    }
                    test "other" {
                        given { query: "foo" }
                        expect {
                            output.result: contains("bar")
                        }
                    }
                }
            }
        "#;
        let tokens = crate::lexer::Lexer::new(source).tokenize().unwrap();
        let ast = crate::parser::Parser::new(tokens).parse().unwrap();
        let result = prepare_test_skill(&ast.skills[0], &ast);
        assert!(result.contains("name: x-test"));
        assert!(result.contains("basic"));
        assert!(result.contains("other"));
        assert!(result.contains("Output Format"));
    }
}
