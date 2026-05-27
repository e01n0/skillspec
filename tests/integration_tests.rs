use skillspec_core::ast::Package;
use skillspec_core::checker::Checker;
use skillspec_core::compiler_skillmd::SkillMdCompiler;
use skillspec_core::lexer::Lexer;
use skillspec_core::lint::LintEngine;
use skillspec_core::parser::Parser;

fn full_pipeline(input: &str) -> String {
    let tokens = Lexer::new(input).tokenize().expect("lexer failed");
    let ast = Parser::new(tokens).parse().expect("parser failed");
    let mut checker = Checker::new();
    checker.check(&ast).expect("checker failed");
    let compiler = SkillMdCompiler::new();
    compiler.compile(&ast.skills[0], &ast)
}

fn full_pipeline_from_file(path: &str) -> String {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("Failed to read {}", path));
    let tokens = Lexer::new(&source).tokenize().expect("lexer failed");
    let ast = Parser::new(tokens).parse().expect("parser failed");
    let base_dir = std::path::Path::new(path)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let mut checker = Checker::with_base_dir(base_dir);
    checker.check(&ast).expect("checker failed");
    let compiler = SkillMdCompiler::new();
    compiler.compile(&ast.skills[0], &ast)
}

#[test]
fn minimal_skill_round_trip() {
    let md = full_pipeline(r#"skill "hello" { context { "Greet warmly." } }"#);
    assert!(md.contains("name: hello"));
    assert!(md.contains("Greet warmly."));
}

#[test]
fn code_review_fixture() {
    let fixture_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/code_review.agent");
    let md = full_pipeline_from_file(fixture_path);

    // Frontmatter
    assert!(md.contains("name: code-review"));
    assert!(md.contains("files"));
    assert!(md.contains("string[]"));

    // Output section
    assert!(md.contains("findings"));
    assert!(md.contains("summary"));

    // Context blocks present
    assert!(md.contains("senior code reviewer"));

    // Steps in topological order
    let analyze_pos = md.find("Step: analyze").expect("analyze step missing");
    let review_pos = md.find("Step: review").expect("review step missing");
    let synth_pos = md.find("Step: synthesise").expect("synthesise step missing");
    assert!(
        analyze_pos < review_pos && review_pos < synth_pos,
        "Steps should be in topological order"
    );
}

#[test]
fn type_error_caught() {
    let source = r#"
        skill "bad" {
            input { files: NonexistentType[] }
            body { context { "x" } }
        }
    "#;
    let tokens = Lexer::new(source).tokenize().unwrap();
    let ast = Parser::new(tokens).parse().unwrap();
    let mut checker = Checker::new();
    assert!(checker.check(&ast).is_err());
}

#[test]
fn cycle_detected() {
    let source = r#"
        skill "cycle" {
            body {
                step a { requires b context { "a" } }
                step b { requires a context { "b" } }
            }
        }
    "#;
    let tokens = Lexer::new(source).tokenize().unwrap();
    let ast = Parser::new(tokens).parse().unwrap();
    let mut checker = Checker::new();
    let result = checker.check(&ast);
    assert!(result.is_err());
    let errs = result.unwrap_err();
    assert!(errs.iter().any(|e| format!("{e}").contains("cycle")));
}

#[test]
fn priority_ordering_in_output() {
    let md = full_pipeline(r#"
        skill "test" {
            body {
                context(priority: optional) { "Low." }
                context(priority: critical) { "High." }
                context(priority: supplementary) { "Medium." }
            }
        }
    "#);
    let high = md.find("High.").unwrap();
    let med = md.find("Medium.").unwrap();
    let low = md.find("Low.").unwrap();
    assert!(high < med && med < low);
}

#[test]
fn conditional_context_preserved() {
    let md = full_pipeline(r#"
        skill "test" {
            input { focus?: string }
            body {
                context(priority: important) { "Always here." }
                context(priority: supplementary, when: input.focus) { "Only when focused." }
            }
        }
    "#);
    assert!(md.contains("Always here."));
    assert!(md.contains("Only when focused."));
    assert!(md.contains("Condition:"), "when guard should be emitted as condition annotation");
    assert!(md.contains("input.focus"), "condition should reference the guard expression");
}

#[test]
fn full_featured_fixture() {
    let fixture_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/full_featured.agent");
    let source = std::fs::read_to_string(fixture_path).expect("read failed");
    let tokens = Lexer::new(&source).tokenize().expect("lexer failed");
    let ast = Parser::new(tokens).parse().expect("parser failed");

    // Verify top-level structure (types now imported, not local)
    assert_eq!(ast.imports.len(), 1, "expected 1 import statement");
    assert_eq!(ast.type_defs.len(), 0, "types are imported, not locally defined");
    assert_eq!(ast.mixins.len(), 1, "expected 1 mixin (observability)");
    assert_eq!(ast.skills.len(), 1, "expected 1 skill (full-review)");
    assert_eq!(ast.pipelines.len(), 1, "expected 1 pipeline (ci-review)");
    assert_eq!(ast.orchestrations.len(), 1, "expected 1 orchestration (team-review)");

    // Verify skill features
    let skill = &ast.skills[0];
    assert_eq!(skill.name, "full-review");
    assert!(skill.tools.is_some(), "skill should have a tools block");
    assert!(skill.permissions.is_some(), "skill should have a permissions block");
    assert_eq!(skill.includes, vec!["observability"], "skill should include observability mixin");
    assert_eq!(skill.pre.len(), 1, "skill should have 1 precondition");
    assert_eq!(skill.post.len(), 1, "skill should have 1 postcondition");

    // Verify body features
    let body = &skill.body;
    assert_eq!(body.lazy_contexts.len(), 2, "body should have 2 lazy contexts");
    assert_eq!(body.steps.len(), 3, "body should have 3 steps");
    assert!(body.directives.reasoning.is_some(), "body should have reasoning directive");
    assert!(body.directives.persona.is_some(), "body should have persona directive");
    assert!(body.directives.sampling.is_some(), "body should have sampling directive");
    assert_eq!(body.directives.reinforcements.len(), 1, "body should have 1 reinforcement");

    // Verify lazy context names
    let lazy_names: Vec<&str> = body.lazy_contexts.iter().map(|lc| lc.name.as_str()).collect();
    assert!(lazy_names.contains(&"style-guide"), "should have style-guide lazy context");
    assert!(lazy_names.contains(&"error-catalog"), "should have error-catalog lazy context");

    // Verify step names and ordering
    assert_eq!(body.steps[0].name, "analyze");
    assert_eq!(body.steps[1].name, "deep_review");
    assert_eq!(body.steps[2].name, "synthesise");
    assert!(body.steps[2].emit, "synthesise step should emit output");

    // Verify tools
    let tools = skill.tools.as_ref().unwrap();
    assert_eq!(tools.required.len(), 3, "should have 3 required tools (Bash, Read, mcp(github))");
    assert_eq!(tools.optional.len(), 1, "should have 1 optional tool (mcp(slack))");

    // Verify pipeline
    let pipeline = &ast.pipelines[0];
    assert_eq!(pipeline.name, "ci-review");
    assert_eq!(pipeline.stages.len(), 3, "pipeline should have 3 stages");
    assert_eq!(pipeline.timeout.as_deref(), Some("30m"));

    // Verify orchestration
    let orch = &ast.orchestrations[0];
    assert_eq!(orch.name, "team-review");
    assert_eq!(orch.agents.len(), 3, "orchestration should have 3 agents");
    assert_eq!(orch.phases.len(), 2, "orchestration should have 2 phases");
    assert_eq!(orch.timeout.as_deref(), Some("1h"));

    // Run checker with base_dir so imports resolve
    let base_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut checker = Checker::with_base_dir(base_dir);
    checker.check(&ast).expect("checker failed");

    // Compile skill to SkillMd
    let compiler = SkillMdCompiler::new();
    let md = compiler.compile(&ast.skills[0], &ast);

    // Verify compiled skill output
    assert!(md.contains("name: full-review"), "frontmatter should have skill name");
    assert!(md.contains("Tools"), "should have Tools section");
    assert!(md.contains("Permissions"), "should have Permissions section");
    assert!(md.contains("Preconditions"), "should have Preconditions section");
    assert!(md.contains("References"), "should have References (lazy contexts) section");
    assert!(md.contains("Reasoning mode"), "should have Reasoning mode directive");
    assert!(md.contains("senior code reviewer"), "persona text should appear");
    assert!(md.contains("Step: analyze"), "should have analyze step section");
    assert!(md.contains("Step: deep_review"), "should have deep_review step section");
    assert!(md.contains("Step: synthesise"), "should have synthesise step section");
    assert!(md.contains("Step: log_start"), "should inject mixin step log_start");
    assert!(md.contains("Step: log_end"), "should inject mixin step log_end");

    // Steps should appear in topological order
    let analyze_pos = md.find("Step: analyze").expect("analyze step missing");
    let deep_review_pos = md.find("Step: deep_review").expect("deep_review step missing");
    let synthesise_pos = md.find("Step: synthesise").expect("synthesise step missing");
    assert!(
        analyze_pos < deep_review_pos && deep_review_pos < synthesise_pos,
        "steps should appear in topological order"
    );

    // Lazy context load references should appear
    assert!(md.contains("style-guide"), "style-guide lazy context should appear");
    assert!(md.contains("error-catalog"), "error-catalog lazy context should appear");

    // Compile pipeline
    let pipeline_md = compiler.compile_pipeline(&ast.pipelines[0]);
    assert!(pipeline_md.contains("Pipeline: ci-review"), "should have pipeline title");
    assert!(pipeline_md.contains("Stage: lint"), "should have lint stage");
    assert!(pipeline_md.contains("Stage: security"), "should have security stage");
    assert!(pipeline_md.contains("Stage: review"), "should have review stage");
    assert!(pipeline_md.contains("30m"), "should include pipeline timeout");

    // Compile orchestration
    let orch_md = compiler.compile_orchestration(&ast.orchestrations[0]);
    assert!(orch_md.contains("Orchestration: team-review"), "should have orchestration title");
    assert!(orch_md.contains("reviewer"), "should list reviewer agent");
    assert!(orch_md.contains("security"), "should list security agent");
    assert!(orch_md.contains("lead"), "should list lead agent");
    assert!(orch_md.contains("1h"), "should include orchestration timeout");
}

// ── Coverage gap: checker reports all errors, not just the first ────

#[test]
fn checker_reports_all_errors_not_just_first() {
    // A file with multiple errors should report all of them
    let source = r#"
        skill "multi-error" {
            input {
                files: UnknownType1[]
                data: UnknownType2[]
            }
            body {
                step a { requires nonexistent context { "a" } }
                step b { requires also_nonexistent context { "b" } }
            }
        }
    "#;
    let tokens = Lexer::new(source).tokenize().unwrap();
    let ast = Parser::new(tokens).parse().unwrap();
    let mut checker = Checker::new();
    let result = checker.check(&ast);
    assert!(result.is_err());
    let errs = result.unwrap_err();
    assert!(errs.len() >= 4, "Expected at least 4 errors (2 unknown types + 2 unknown steps), got {}", errs.len());
}

#[test]
fn all_example_skills_pass_check() {
    for path in &[
        "skills/skill-writer.agent",
        "skills/skillspec-migrate.agent",
        "skills/skillspec-backport.agent",
        "skills/skillspec-test.agent",
        "examples/brainstorming.agent",
    ] {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("Failed to read {}", path));
        let tokens = Lexer::new(&source).tokenize()
            .unwrap_or_else(|e| panic!("{}: lexer failed: {}", path, e));
        let ast = Parser::new(tokens).parse()
            .unwrap_or_else(|e| panic!("{}: parser failed: {}", path, e));
        let base_dir = std::path::Path::new(path)
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let mut checker = Checker::with_base_dir(base_dir);
        checker.check(&ast)
            .unwrap_or_else(|errs| panic!("{}: checker failed with {} errors: {:?}", path, errs.len(), errs));
    }
}

#[test]
fn all_example_skills_compile_to_skillmd() {
    let compiler = SkillMdCompiler::new();
    for path in &[
        "skills/skill-writer.agent",
        "skills/skillspec-migrate.agent",
        "skills/skillspec-backport.agent",
        "skills/skillspec-test.agent",
        "examples/brainstorming.agent",
    ] {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("Failed to read {}", path));
        let tokens = Lexer::new(&source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let md = compiler.compile(&ast.skills[0], &ast);
        assert!(md.contains("---"), "{}: missing frontmatter", path);
        assert!(md.contains("name:"), "{}: missing name in frontmatter", path);
        assert!(!md.is_empty(), "{}: empty output", path);
    }
}

#[test]
fn package_declaration_parsed_and_exported_skill_compiled() {
    let source = r#"
        package "@tools/review" {
            version = "1.0.0"
            description = "Code review toolkit"
            exports {
                code_review
                Finding
            }
        }
        type Finding {
            file: string
            severity: string
        }
        skill "code-review" {
            body { context { "Review code carefully." } }
        }
    "#;
    let tokens = Lexer::new(source).tokenize().expect("lexer failed");
    let ast = Parser::new(tokens).parse().expect("parser failed");

    // Package should be parsed
    assert_eq!(ast.packages.len(), 1);
    let pkg: &Package = &ast.packages[0];
    assert_eq!(pkg.name, "@tools/review");
    assert_eq!(pkg.version, "1.0.0");
    assert_eq!(pkg.description, "Code review toolkit");
    assert_eq!(pkg.exports, vec!["code_review", "Finding"]);

    // Type and skill also parsed
    assert_eq!(ast.type_defs.len(), 1);
    assert_eq!(ast.skills.len(), 1);
    assert_eq!(ast.skills[0].name, "code-review");

    // Exported skill compiles successfully to SkillMd
    let mut checker = Checker::new();
    checker.check(&ast).expect("checker failed");
    let compiler = SkillMdCompiler::new();
    let md = compiler.compile(&ast.skills[0], &ast);
    assert!(md.contains("name: code-review"));
    assert!(md.contains("Review code carefully."));
}

// ── Multi-file import resolution ───────────────────────────────────

#[test]
fn multi_file_import_resolves_types() {
    let fixture_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/code_review.agent"
    );
    let source = std::fs::read_to_string(fixture_path).expect("read failed");
    let tokens = Lexer::new(&source).tokenize().expect("lexer failed");
    let ast = Parser::new(tokens).parse().expect("parser failed");

    assert_eq!(ast.imports.len(), 1, "should have 1 import");
    assert_eq!(ast.type_defs.len(), 0, "types should NOT be locally defined");
    assert!(
        ast.imports[0].symbols.contains(&"Finding".to_string()),
        "should import Finding"
    );

    let base_dir = std::path::Path::new(fixture_path)
        .parent()
        .unwrap()
        .to_path_buf();
    let mut checker = Checker::with_base_dir(base_dir);
    checker
        .check(&ast)
        .expect("checker should resolve imported Finding type");
}

#[test]
fn multi_file_import_missing_symbol_errors() {
    let dir = std::env::temp_dir().join("skillspec_import_test_missing_sym");
    let types_dir = dir.join("types");
    std::fs::create_dir_all(&types_dir).unwrap();
    std::fs::write(
        types_dir.join("review.agent"),
        "type Finding { file: string }",
    )
    .unwrap();
    std::fs::write(
        dir.join("skill.agent"),
        r#"
            import { Finding, NonExistent } from "@types/review"
            skill "x" {
                input { f: Finding }
                body { context { "ok" } }
            }
        "#,
    )
    .unwrap();

    let source = std::fs::read_to_string(dir.join("skill.agent")).unwrap();
    let tokens = Lexer::new(&source).tokenize().unwrap();
    let ast = Parser::new(tokens).parse().unwrap();
    let mut checker = Checker::with_base_dir(dir.clone());
    let result = checker.check(&ast);

    assert!(result.is_err(), "should fail — NonExistent is not in the types file");
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| format!("{}", e).contains("NonExistent")),
        "should report NonExistent as missing: {:?}",
        errs
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn multi_file_import_unresolved_path_errors() {
    let dir = std::env::temp_dir().join("skillspec_import_test_bad_path");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("skill.agent"),
        r#"
            import { Foo } from "@types/nonexistent"
            skill "x" {
                body { context { "ok" } }
            }
        "#,
    )
    .unwrap();

    let source = std::fs::read_to_string(dir.join("skill.agent")).unwrap();
    let tokens = Lexer::new(&source).tokenize().unwrap();
    let ast = Parser::new(tokens).parse().unwrap();
    let mut checker = Checker::with_base_dir(dir.clone());
    let result = checker.check(&ast);

    assert!(result.is_err(), "should fail — import path doesn't exist");
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| format!("{}", e).contains("resolve")),
        "should report unresolved import: {:?}",
        errs
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn multi_file_transitive_imports() {
    let dir = std::env::temp_dir().join("skillspec_import_test_transitive");
    let types_dir = dir.join("types");
    std::fs::create_dir_all(&types_dir).unwrap();

    // base.agent defines Severity
    std::fs::write(
        types_dir.join("base.agent"),
        r#"type Severity { level: string }"#,
    )
    .unwrap();

    // review.agent imports Severity and defines Finding using it
    std::fs::write(
        types_dir.join("review.agent"),
        r#"
            import { Severity } from "./base"
            type Finding {
                file: string
                severity: Severity
            }
        "#,
    )
    .unwrap();

    // skill imports Finding (which depends on Severity transitively)
    std::fs::write(
        dir.join("skill.agent"),
        r#"
            import { Finding } from "@types/review"
            skill "x" {
                input { f: Finding }
                body { context { "ok" } }
            }
        "#,
    )
    .unwrap();

    let source = std::fs::read_to_string(dir.join("skill.agent")).unwrap();
    let tokens = Lexer::new(&source).tokenize().unwrap();
    let ast = Parser::new(tokens).parse().unwrap();
    let mut checker = Checker::with_base_dir(dir.clone());
    checker
        .check(&ast)
        .expect("transitive imports should resolve");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn extends_with_inherited_requires_full_pipeline() {
    let source = r#"
        skill "base" {
            input { files: string[] }
            body {
                lazy context "docs" (priority: supplementary) {
                    summary "API docs."
                    ref "./api.md"
                }
                step analyze {
                    context { "Analyze the files." }
                }
            }
        }
        skill "child" extends "base" {
            body {
                step report {
                    requires analyze
                    load "docs"
                    emit output
                    context { "Write the report." }
                }
            }
        }
    "#;
    let tokens = Lexer::new(source).tokenize().expect("lexer failed");
    let ast = Parser::new(tokens).parse().expect("parser failed");
    let mut checker = Checker::new();
    checker.check(&ast).expect("checker should accept requires on inherited step");
    let compiler = SkillMdCompiler::new();
    let child = ast.skills.iter().find(|s| s.name == "child").unwrap();
    let md = compiler.compile(child, &ast);
    assert!(md.contains("Step: analyze"), "should inherit base step");
    assert!(md.contains("Step: report"), "should have child step");
    assert!(md.contains("files"), "should inherit base input");
}

#[test]
fn mixin_requires_full_pipeline() {
    let md = full_pipeline(r#"
        mixin logging {
            step log_start { context { "Log start." } }
        }
        skill "x" {
            include logging
            body {
                step work {
                    requires log_start
                    emit output
                    context { "Do work." }
                }
            }
        }
    "#);
    assert!(md.contains("Step: log_start"), "should have mixin step");
    assert!(md.contains("Step: work"), "should have child step");
}

#[test]
fn brainstorming_example_imports_resolve() {
    let example_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/brainstorming.agent"
    );
    let md = full_pipeline_from_file(example_path);
    assert!(md.contains("name: brainstorming"));
    assert!(md.contains("design"), "output should reference Design type");
}

// ── Lint: fixture integration tests ──────────────────────────────────

// ── Watch mode: flag acceptance ──────────────────────────────────────

#[test]
fn watch_flag_accepted() {
    use std::process::Command;
    let bin = env!("CARGO_BIN_EXE_skillspec");
    let output = Command::new(bin)
        .args(["build", "--help"])
        .output()
        .expect("failed to run skillspec");
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("--watch"), "build --help should mention --watch flag");
}

#[test]
#[ignore] // requires real filesystem watcher + timing; run with `cargo test -- --ignored`
fn rebuild_on_change() {
    use std::process::{Command, Stdio};
    use std::io::BufRead;

    let dir = std::env::temp_dir().join("skillspec_watch_test");
    std::fs::create_dir_all(&dir).unwrap();
    let agent_path = dir.join("test.agent");
    std::fs::write(&agent_path, r#"skill "test" { body { context { "v1" } } }"#).unwrap();

    let bin = env!("CARGO_BIN_EXE_skillspec");
    let mut child = Command::new(bin)
        .args(["build", agent_path.to_str().unwrap(), "--watch"])
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn skillspec");

    std::thread::sleep(std::time::Duration::from_secs(1));
    std::fs::write(&agent_path, r#"skill "test" { body { context { "v2" } } }"#).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(2));

    child.kill().ok();
    let stderr = child.stderr.take().unwrap();
    let output: Vec<String> = std::io::BufReader::new(stderr)
        .lines()
        .map_while(|l| l.ok())
        .collect();
    let rebuild_count = output.iter().filter(|l| l.contains("Change detected")).count();
    assert!(rebuild_count >= 1, "expected at least 1 rebuild trigger, got output: {:?}", output);

    std::fs::remove_dir_all(&dir).ok();
}

// ── Budget-aware build ──────────────────────────────────────────────

#[test]
fn budget_flag_trims_output() {
    let source = r#"
        skill "big" {
            body {
                context(priority: critical) { "High priority context that must survive the trim." }
                context(priority: supplementary) { "Medium priority text that might get cut depending on budget." }
                context(priority: optional) { "Low priority filler that should be the first to go when budget is tight." }
            }
        }
    "#;
    let tokens = Lexer::new(source).tokenize().unwrap();
    let mut ast = Parser::new(tokens).parse().unwrap();

    let before = skillspec_core::budget::estimate_context_tokens(&ast.skills[0].body.contexts);
    let trimmed = skillspec_core::budget::trim_to_budget(&mut ast.skills[0].body.contexts, before / 2);
    let after = skillspec_core::budget::estimate_context_tokens(&ast.skills[0].body.contexts);

    assert!(!trimmed.is_empty(), "should have trimmed at least one context");
    assert!(after <= before / 2, "after trimming should be within budget: {} <= {}", after, before / 2);
    assert!(trimmed[0].priority == Some(skillspec_core::ast::Priority::Optional), "lowest priority should be trimmed first");
}

#[test]
fn budget_does_not_trim_step_contexts() {
    let source = r#"
        skill "x" {
            body {
                context(priority: supplementary) { "Body context that can be trimmed." }
                step main {
                    context { "Step context that should not be trimmed by budget." }
                }
            }
        }
    "#;
    let tokens = Lexer::new(source).tokenize().unwrap();
    let mut ast = Parser::new(tokens).parse().unwrap();

    let _trimmed = skillspec_core::budget::trim_to_budget(&mut ast.skills[0].body.contexts, 1);
    assert!(ast.skills[0].body.contexts.is_empty(), "body contexts should be trimmed");
    assert!(!ast.skills[0].body.steps[0].contexts.is_empty(), "step contexts should be preserved");
}

// ── Observability ────────────────────────────────────────────────────

#[test]
fn checker_duplicate_metric_name_errors() {
    let source = r#"
        skill "x" {
            body {
                context { "Base." }
                observe {
                    metric "findings" from output.a
                    metric "findings" from output.b
                }
            }
        }
    "#;
    let tokens = Lexer::new(source).tokenize().unwrap();
    let ast = Parser::new(tokens).parse().unwrap();
    let mut checker = Checker::new();
    let result = checker.check(&ast);
    assert!(result.is_err(), "duplicate metric names should error");
    let errs = result.unwrap_err();
    assert!(errs.iter().any(|e| format!("{e}").contains("Duplicate")));
}

#[test]
fn compile_observability_section() {
    let source = r#"
        skill "x" {
            body {
                context { "Base." }
                observe {
                    on step_complete { emit_event "step.done" }
                    metric "review.score" from output.score
                }
            }
        }
    "#;
    let tokens = Lexer::new(source).tokenize().unwrap();
    let ast = Parser::new(tokens).parse().unwrap();
    let compiler = SkillMdCompiler::new();
    let md = compiler.compile(&ast.skills[0], &ast);
    assert!(md.contains("## Observability"), "should have Observability section");
    assert!(md.contains("step.done"), "should contain event name");
    assert!(md.contains("review.score"), "should contain metric name");
}

#[test]
fn observe_block_round_trips_through_ir() {
    let source = r#"
        skill "x" {
            body {
                context { "Base." }
                observe {
                    on step_complete { emit_event "step.done" }
                    metric "count" from output.n
                }
            }
        }
    "#;
    let tokens = Lexer::new(source).tokenize().unwrap();
    let ast = Parser::new(tokens).parse().unwrap();
    let ir = serde_json::to_string(&ast).unwrap();
    let round: skillspec_core::ast::SourceFile = serde_json::from_str(&ir).unwrap();
    let observe = round.skills[0].body.observe.as_ref().expect("observe should survive serialization");
    assert_eq!(observe.events.len(), 1);
    assert_eq!(observe.metrics.len(), 1);
}

// ── Multi-target compilation ─────────────────────────────────────────

#[test]
fn target_trait_compile_matches_direct() {
    use skillspec_core::compiler::TargetCompiler;
    let source = r#"skill "x" { body { context { "Be helpful." } } }"#;
    let tokens = Lexer::new(source).tokenize().unwrap();
    let ast = Parser::new(tokens).parse().unwrap();
    let compiler = SkillMdCompiler::new();
    let direct = compiler.compile(&ast.skills[0], &ast);
    let via_trait: &dyn TargetCompiler = &compiler;
    let trait_output = via_trait.compile_skill(&ast.skills[0], &ast);
    assert_eq!(direct, trait_output);
}

#[test]
fn unknown_target_errors() {
    let bin = env!("CARGO_BIN_EXE_skillspec");
    let dir = std::env::temp_dir().join("skillspec_unknown_target");
    std::fs::create_dir_all(&dir).unwrap();
    let agent = dir.join("test.agent");
    std::fs::write(&agent, r#"skill "x" { body { context { "ok" } } }"#).unwrap();
    let output = std::process::Command::new(bin)
        .args(["build", agent.to_str().unwrap(), "--target", "nonexistent"])
        .output()
        .expect("failed to run");
    assert!(!output.status.success(), "should fail for unknown target");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown target"), "should mention unknown target: {}", stderr);
    std::fs::remove_dir_all(&dir).ok();
}

fn lint_file(path: &str) -> Vec<skillspec_core::lint::LintDiagnostic> {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("Failed to read {}", path));
    let tokens = Lexer::new(&source).tokenize().expect("lexer failed");
    let ast = Parser::new(tokens).parse().expect("parser failed");
    LintEngine::new().run(&ast)
}

#[test]
fn lint_clean_skill_no_warnings() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/code_review.agent");
    let diags = lint_file(path);
    assert!(diags.is_empty(), "code_review.agent should have zero lint warnings, got: {:?}",
        diags.iter().map(|d| (&d.rule, &d.message)).collect::<Vec<_>>());
}

#[test]
fn lint_full_featured_fixture() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/full_featured.agent");
    let diags = lint_file(path);
    let warnings: Vec<_> = diags.iter()
        .filter(|d| d.severity == skillspec_core::lint::Severity::Warning)
        .collect();
    assert!(warnings.is_empty(),
        "full_featured.agent should have no warnings, got: {:?}",
        warnings.iter().map(|d| (&d.rule, &d.message)).collect::<Vec<_>>());
}

// ── Migrate directory integration ───────────────────────────────────

#[test]
fn migrate_directory_integration() {
    let dir = std::env::temp_dir().join("skillspec_migrate_dir_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("refs")).unwrap();

    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: integ-test\ndescription: \"Integration test skill\"\n---\n\n# integ-test\n\n## Analyze\n\nAnalyze the input.\n",
    ).unwrap();
    std::fs::write(
        dir.join("refs/patterns.md"),
        "# Patterns\n\nCommon patterns to look for.\n",
    ).unwrap();

    let bin = env!("CARGO_BIN_EXE_skillspec");
    let output = std::process::Command::new(bin)
        .args(["migrate", dir.to_str().unwrap()])
        .output()
        .expect("failed to run skillspec migrate");

    assert!(output.status.success(), "migrate should succeed, stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("migrated"), "should report migration: {}", stdout);
    assert!(stdout.contains("1 additional file"), "should report file count: {}", stdout);

    let dir_name = dir.file_name().unwrap().to_string_lossy().to_string();
    let partial_path = dir.join(format!("{}.agent.partial", dir_name));
    assert!(partial_path.exists(), "should create .agent.partial at {}", partial_path.display());

    let content = std::fs::read_to_string(&partial_path).unwrap();
    assert!(content.contains("skill \"integ-test\""));
    assert!(content.contains("additional file(s) found"));
    assert!(content.contains("patterns.md"));

    std::fs::remove_dir_all(&dir).ok();
}
