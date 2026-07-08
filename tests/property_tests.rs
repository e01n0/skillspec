//! Property tests: the formatter must be idempotent and its output must
//! always re-parse to the same canonical form, for any structurally valid
//! skill we can generate.

use proptest::prelude::*;
use skillspec_core::formatter::Formatter;
use skillspec_core::lexer::Lexer;
use skillspec_core::parser::Parser;

fn parse(input: &str) -> Result<skillspec_core::ast::SourceFile, String> {
    let tokens = Lexer::new(input)
        .tokenize()
        .map_err(|e| format!("lex: {e}"))?;
    Parser::new(tokens)
        .parse()
        .map_err(|e| format!("parse: {e}"))
}

fn fmt(input: &str) -> Result<String, String> {
    Ok(Formatter::format(&parse(input)?))
}

// ── Generators ──────────────────────────────────────────────────────────────

fn ident() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,10}".prop_filter("avoid keywords", |s| {
        // Any lexer keyword that can't appear as a bare identifier in the
        // positions we generate (step names, field names).
        !matches!(
            s.as_str(),
            "skill"
                | "input"
                | "output"
                | "body"
                | "context"
                | "step"
                | "requires"
                | "when"
                | "use"
                | "let"
                | "emit"
                | "import"
                | "from"
                | "type"
                | "pre"
                | "post"
                | "assert"
                | "message"
                | "on_error"
                | "on_fail"
                | "all_steps"
                | "extends"
                | "lazy"
                | "ref"
                | "summary"
                | "index"
                | "section"
                | "load"
                | "pipeline"
                | "stage"
                | "orchestration"
                | "agents"
                | "phase"
                | "shared"
                | "rules"
                | "cancel"
                | "timeout"
                | "mixin"
                | "include"
                | "package"
                | "version"
                | "description"
                | "exports"
                | "reasoning"
                | "examples"
                | "example"
                | "note"
                | "format"
                | "reinforce"
                | "every"
                | "on"
                | "sampling"
                | "persona"
                | "tools"
                | "require"
                | "optional"
                | "mcp"
                | "tool"
                | "allow"
                | "deny"
                | "permissions"
                | "tests"
                | "test"
                | "given"
                | "mock"
                | "expect"
                | "confidence"
                | "runs"
                | "snapshot"
                | "compare"
                | "equals"
                | "contains"
                | "matches"
                | "resembles"
                | "satisfies"
                | "between"
                | "unavailable"
                | "failing"
                | "slow"
                | "observe"
                | "emit_event"
                | "metric"
                | "if"
                | "retry"
                | "backoff"
                | "budget"
                | "string"
                | "int"
                | "float"
                | "bool"
                | "enum"
                | "map"
                | "true"
                | "false"
                | "in"
        )
    })
}

fn prose() -> impl Strategy<Value = String> {
    // Printable text without quotes/backslashes/braces so it survives
    // string-literal embedding untouched.
    "[a-zA-Z0-9 .,;!?-]{1,60}"
}

fn priority() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("critical"),
        Just("important"),
        Just("supplementary"),
        Just("optional"),
    ]
}

#[derive(Debug, Clone)]
struct GenSkill {
    name: String,
    version: Option<(u8, u8, u8)>,
    fields: Vec<(String, &'static str, bool)>,
    contexts: Vec<(Option<&'static str>, String)>,
    steps: Vec<GenStep>,
}

#[derive(Debug, Clone)]
struct GenStep {
    name: String,
    text: String,
    on_fail: Option<&'static str>,
}

fn gen_skill() -> impl Strategy<Value = GenSkill> {
    let field = (
        ident(),
        prop_oneof![Just("string"), Just("int"), Just("bool")],
        any::<bool>(),
    );
    let ctx = (proptest::option::of(priority()), prose());
    let step = (
        ident(),
        prose(),
        proptest::option::of(prop_oneof![
            Just("retry 2"),
            Just("escalate \"needs help\""),
            Just("abort"),
        ]),
    )
        .prop_map(|(name, text, on_fail)| GenStep {
            name,
            text,
            on_fail,
        });

    (
        ident(),
        proptest::option::of((0u8..9, 0u8..9, 0u8..9)),
        proptest::collection::vec(field, 0..4),
        proptest::collection::vec(ctx, 0..4),
        proptest::collection::vec(step, 0..4),
    )
        .prop_map(|(name, version, mut fields, contexts, mut steps)| {
            // Unique field and step names — duplicates are checker errors and
            // the parser itself must still handle them, but canonical-form
            // comparison only makes sense for valid inputs.
            fields.sort_by(|a, b| a.0.cmp(&b.0));
            fields.dedup_by(|a, b| a.0 == b.0);
            steps.sort_by(|a, b| a.name.cmp(&b.name));
            steps.dedup_by(|a, b| a.name == b.name);
            GenSkill {
                name,
                version,
                fields,
                contexts,
                steps,
            }
        })
}

fn render(skill: &GenSkill) -> String {
    let mut src = format!("skill \"{}\" {{\n", skill.name);
    if let Some((a, b, c)) = skill.version {
        src.push_str(&format!("  version \"{}.{}.{}\"\n", a, b, c));
    }
    if !skill.fields.is_empty() {
        src.push_str("  input {\n");
        for (name, ty, optional) in &skill.fields {
            let q = if *optional { "?" } else { "" };
            src.push_str(&format!("    {}{}: {}\n", name, q, ty));
        }
        src.push_str("  }\n");
    }
    src.push_str("  body {\n");
    for (prio, text) in &skill.contexts {
        match prio {
            Some(p) => src.push_str(&format!(
                "    context(priority: {}) {{ \"{}\" }}\n",
                p, text
            )),
            None => src.push_str(&format!("    context {{ \"{}\" }}\n", text)),
        }
    }
    // Chain steps so requires references are always valid
    for (i, step) in skill.steps.iter().enumerate() {
        src.push_str(&format!("    step {} {{\n", step.name));
        if i > 0 {
            src.push_str(&format!("      requires {}\n", skill.steps[i - 1].name));
        }
        if let Some(policy) = step.on_fail {
            src.push_str(&format!("      on_fail {}\n", policy));
        }
        src.push_str(&format!("      context {{ \"{}\" }}\n", step.text));
        src.push_str("    }\n");
    }
    if skill.contexts.is_empty() && skill.steps.is_empty() {
        src.push_str("    context { \"placeholder\" }\n");
    }
    src.push_str("  }\n}\n");
    src
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn generated_skills_parse(skill in gen_skill()) {
        let src = render(&skill);
        parse(&src).unwrap_or_else(|e| panic!("generated source failed to parse: {e}\n{src}"));
    }

    #[test]
    fn fmt_is_idempotent(skill in gen_skill()) {
        let src = render(&skill);
        let once = fmt(&src).expect("first format failed");
        let twice = fmt(&once).unwrap_or_else(|e| {
            panic!("formatted output failed to re-parse: {e}\n--- formatted ---\n{once}")
        });
        prop_assert_eq!(&once, &twice, "fmt(fmt(x)) != fmt(x)\n--- source ---\n{}", src);
    }

    #[test]
    fn fmt_preserves_structure(skill in gen_skill()) {
        let src = render(&skill);
        let ast_before = parse(&src).unwrap();
        let formatted = fmt(&src).unwrap();
        let ast_after = parse(&formatted).unwrap();

        prop_assert_eq!(ast_before.skills.len(), ast_after.skills.len());
        for (a, b) in ast_before.skills.iter().zip(ast_after.skills.iter()) {
            prop_assert_eq!(&a.name, &b.name);
            prop_assert_eq!(&a.version, &b.version);
            prop_assert_eq!(
                a.input.as_ref().map(|f| f.len()),
                b.input.as_ref().map(|f| f.len())
            );
            prop_assert_eq!(a.body.contexts.len(), b.body.contexts.len());
            prop_assert_eq!(a.body.steps.len(), b.body.steps.len());
            for (sa, sb) in a.body.steps.iter().zip(b.body.steps.iter()) {
                prop_assert_eq!(&sa.name, &sb.name);
                prop_assert_eq!(&sa.on_fail, &sb.on_fail);
            }
        }
    }
}
