use skillspec_core::ast::Package;
use skillspec_core::checker::Checker;
use skillspec_core::compiler_skillmd::SkillMdCompiler;
use skillspec_core::lexer::Lexer;
use skillspec_core::parser::Parser;

fn full_pipeline(input: &str) -> String {
    let tokens = Lexer::new(input).tokenize().expect("lexer failed");
    let ast = Parser::new(tokens).parse().expect("parser failed");
    let mut checker = Checker::new();
    checker.check(&ast).expect("checker failed");
    let compiler = SkillMdCompiler::new();
    compiler.compile(&ast.skills[0])
}

#[test]
fn minimal_skill_round_trip() {
    let md = full_pipeline(r#"skill "hello" { context { "Greet warmly." } }"#);
    assert!(md.contains("name: hello"));
    assert!(md.contains("Greet warmly."));
}

#[test]
fn code_review_fixture() {
    let source = include_str!("fixtures/code_review.agent");
    let md = full_pipeline(source);

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
                context(priority: 30) { "Low." }
                context(priority: 95) { "High." }
                context(priority: 60) { "Medium." }
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
                context(priority: 90) { "Always here." }
                context(priority: 70, when: input.focus) { "Only when focused." }
            }
        }
    "#);
    assert!(md.contains("Always here."));
    assert!(md.contains("Only when focused."));
}

#[test]
fn full_featured_fixture() {
    let source = include_str!("fixtures/full_featured.agent");
    let tokens = Lexer::new(source).tokenize().expect("lexer failed");
    let ast = Parser::new(tokens).parse().expect("parser failed");

    // Verify top-level structure
    assert_eq!(ast.type_defs.len(), 2, "expected 2 type defs (Finding, ReviewReport)");
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

    // Run checker — all types and references must resolve cleanly
    let mut checker = Checker::new();
    checker.check(&ast).expect("checker failed");

    // Compile skill to SkillMd
    let compiler = SkillMdCompiler::new();
    let md = compiler.compile(&ast.skills[0]);

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
    assert!(md.contains("Includes mixin: observability"), "should note mixin inclusion");

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
        "examples/brainstorming.agent",
    ] {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("Failed to read {}", path));
        let tokens = Lexer::new(&source).tokenize()
            .unwrap_or_else(|e| panic!("{}: lexer failed: {}", path, e));
        let ast = Parser::new(tokens).parse()
            .unwrap_or_else(|e| panic!("{}: parser failed: {}", path, e));
        let mut checker = Checker::new();
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
        "examples/brainstorming.agent",
    ] {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("Failed to read {}", path));
        let tokens = Lexer::new(&source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let md = compiler.compile(&ast.skills[0]);
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
    let md = compiler.compile(&ast.skills[0]);
    assert!(md.contains("name: code-review"));
    assert!(md.contains("Review code carefully."));
}
