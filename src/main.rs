use std::fs;
use std::io::{self, BufRead, Write as IoWrite};
use std::path::Path;
use clap::Parser;
use miette::Result;
use skillspec_core::ast::{Dependency, SourceFile};
use skillspec_core::checker::Checker;
use skillspec_core::compiler_skillmd::SkillMdCompiler;
use skillspec_core::compiler_ir::IrCompiler;
use skillspec_core::formatter::Formatter;
use skillspec_core::budget;
use skillspec_core::diff::{structural_diff, skillmd_diff, classify_semver};
use skillspec_core::lint::LintEngine;
use skillspec_core::deps::emit_mermaid;
use skillspec_core::compiler::TargetCompiler;
use skillspec_core::compiler_systemprompt::SystemPromptCompiler;
use skillspec_core::compiler_cursor::CursorCompiler;
use skillspec_core::compiler_clinerules::ClineRulesCompiler;
use skillspec_core::migrate;
use skillspec_core::lexer::Lexer;
use skillspec_core::parser;

#[derive(Parser)]
#[command(name = "skillspec", about = "A typed, composable language for AI agent skills and workflows", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Type-check and validate an .agent file
    Check { file: String },
    /// Compile an .agent file to the target format
    Build {
        file: String,
        #[arg(long, default_value = "skillmd")]
        target: String,
        #[arg(short, long)]
        output: Option<String>,
        /// Deploy to a runtime: claude, claude-project, cursor, cline, codex, or a custom path. Use --to without a value for an interactive menu.
        #[arg(long, num_args = 0..=1, default_missing_value = "menu")]
        to: Option<String>,
        /// Watch for file changes and rebuild automatically
        #[arg(long)]
        watch: bool,
        /// Token budget: drop lowest-priority contexts to fit within this limit
        #[arg(long)]
        budget: Option<usize>,
        /// Emit a JSON schema of all declared telemetry events and metrics
        #[arg(long)]
        emit_telemetry_schema: bool,
    },
    /// Scaffold a new .agent skill file
    Init { name: String },
    /// Format an .agent file with canonical style
    Fmt { file: String },
    /// Estimate token budget for skills in an .agent file
    Budget { file: String },
    /// Print dependency graph of steps/stages/phases
    Deps {
        file: String,
        /// Output format: "text" (default) or "mermaid"
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Mechanically extract a SKILL.md (or directory of skills) into a .agent.partial file
    Migrate { path: String },
    /// Package an .agent file containing a package declaration into a .skillpkg directory
    Pack {
        file: String,
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Install a .skillpkg directory (or .agent file with package declaration) into .skillspec/packages/
    Install {
        /// Path to a .skillpkg directory or an .agent file with a package declaration
        path: String,
    },
    /// List, prepare, or evaluate tests in an .agent file
    Test {
        file: String,
        /// Generate a test execution SKILL.md
        #[arg(long)]
        prepare: bool,
        /// Evaluate test results from a JSON file
        #[arg(long)]
        evaluate: Option<String>,
    },
    /// Run lint rules to catch quality issues beyond structural validity
    Lint { file: String },
    /// Print the formal EBNF grammar for the .agent language
    Grammar,
    /// Show structural diff between two .agent files (or compiled vs SKILL.md)
    Diff {
        file_a: String,
        file_b: String,
        /// Compare compiled output of file_a against file_b as a SKILL.md
        #[arg(long)]
        against_skillmd: bool,
        /// Classify changes as MAJOR/MINOR/PATCH semver bumps
        #[arg(long)]
        semver: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Check { file } => cmd_check(&file),
        Commands::Build { file, target, output, to, watch, budget, emit_telemetry_schema } => {
            if output.is_some() && to.is_some() {
                return Err(miette::miette!("--to and --output cannot both be specified"));
            }

            let (effective_target, effective_output) = if let Some(to_value) = to {
                let deploy = resolve_deploy_target(&to_value)?;
                let t = deploy.target_override.unwrap_or(target);
                (t, Some(deploy.path))
            } else {
                (target, output)
            };

            if watch {
                cmd_build_watch(&file, &effective_target, effective_output.as_deref())
            } else if emit_telemetry_schema {
                cmd_emit_telemetry(&file)
            } else {
                cmd_build(&file, &effective_target, effective_output.as_deref(), budget)
            }
        }
        Commands::Init { name } => cmd_init(&name),
        Commands::Fmt { file } => cmd_fmt(&file),
        Commands::Budget { file } => cmd_budget(&file),
        Commands::Deps { file, format } => cmd_deps(&file, &format),
        Commands::Migrate { path } => cmd_migrate(&path),
        Commands::Pack { file, output } => cmd_pack(&file, output.as_deref()),
        Commands::Install { path } => cmd_install(&path),
        Commands::Test { file, prepare, evaluate } => cmd_test(&file, prepare, evaluate.as_deref()),
        Commands::Grammar => cmd_grammar(),
        Commands::Lint { file } => cmd_lint(&file),
        Commands::Diff { file_a, file_b, against_skillmd, semver } => cmd_diff(&file_a, &file_b, against_skillmd, semver),
    }
}

/// Read a file, lex it, and parse it into an AST.
fn read_and_parse(path: &str) -> Result<SourceFile> {
    let source = fs::read_to_string(path)
        .map_err(|e| miette::miette!("Failed to read '{}': {}", path, e))?;

    let tokens = Lexer::new(&source)
        .tokenize()
        .map_err(|e| miette::miette!("Lex error in '{}': {}", path, e))?;

    let ast = parser::Parser::new(tokens)
        .parse()
        .map_err(|e| miette::miette!("Parse error in '{}': {}", path, e))?;

    Ok(ast)
}

fn cmd_grammar() -> Result<()> {
    print!("{}", include_str!("../docs/grammar.ebnf"));
    Ok(())
}

fn cmd_check(path: &str) -> Result<()> {
    let ast = read_and_parse(path)?;
    let base_dir = std::path::Path::new(path)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let mut checker = Checker::with_base_dir(base_dir);
    match checker.check(&ast) {
        Ok(()) => {
            println!("✓ {}: no errors", path);
            Ok(())
        }
        Err(errors) => {
            for err in &errors {
                eprintln!("error: {}", err);
            }
            Err(miette::miette!(
                "{} error(s) found in '{}'",
                errors.len(),
                path
            ))
        }
    }
}

fn cmd_lint(path: &str) -> Result<()> {
    let ast = read_and_parse(path)?;
    let base_dir = std::path::Path::new(path)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let mut checker = Checker::with_base_dir(base_dir);
    if let Err(errors) = checker.check(&ast) {
        for err in &errors {
            eprintln!("error: {}", err);
        }
        return Err(miette::miette!(
            "{} error(s) found in '{}'; fix them before linting",
            errors.len(),
            path
        ));
    }

    let engine = LintEngine::new();
    let diagnostics = engine.run(&ast);

    if diagnostics.is_empty() {
        println!("✓ {}: no lint warnings", path);
    } else {
        for diag in &diagnostics {
            eprintln!("{}", diag);
        }
        eprintln!("\n{} warning(s) in '{}'", diagnostics.len(), path);
    }

    Ok(())
}

struct DeployTarget {
    path: String,
    target_override: Option<String>,
}

fn resolve_deploy_target(value: &str) -> Result<DeployTarget> {
    match value {
        "menu" => show_deploy_menu(),
        "claude" => {
            let home = std::env::var("HOME")
                .map_err(|_| miette::miette!("HOME not set — pass a path to --to instead"))?;
            Ok(DeployTarget {
                path: format!("{home}/.claude/skills"),
                target_override: None,
            })
        }
        "claude-project" => Ok(DeployTarget {
            path: ".claude/skills".to_string(),
            target_override: None,
        }),
        "cursor" => Ok(DeployTarget {
            path: ".cursor/rules".to_string(),
            target_override: Some("cursor".to_string()),
        }),
        "cline" => Ok(DeployTarget {
            path: ".".to_string(),
            target_override: Some("clinerules".to_string()),
        }),
        "codex" => Ok(DeployTarget {
            path: ".codex".to_string(),
            target_override: Some("system-prompt".to_string()),
        }),
        custom => Ok(DeployTarget {
            path: custom.to_string(),
            target_override: None,
        }),
    }
}

fn show_deploy_menu() -> Result<DeployTarget> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());

    eprintln!("Deploy to:");
    eprintln!("  1) Claude Code (global)   → {}/.claude/skills/", home);
    eprintln!("  2) Claude Code (project)  → .claude/skills/");
    eprintln!("  3) Cursor                 → .cursor/rules/");
    eprintln!("  4) Cline                  → ./");
    eprintln!("  5) Codex                  → .codex/");
    eprintln!("  6) Custom path");
    eprint!("Pick a target [1-6]: ");
    io::stderr().flush().map_err(|e| miette::miette!("flush: {e}"))?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)
        .map_err(|e| miette::miette!("Failed to read input: {e}"))?;

    match input.trim() {
        "1" => resolve_deploy_target("claude"),
        "2" => resolve_deploy_target("claude-project"),
        "3" => resolve_deploy_target("cursor"),
        "4" => resolve_deploy_target("cline"),
        "5" => resolve_deploy_target("codex"),
        "6" => {
            eprint!("Path: ");
            io::stderr().flush().map_err(|e| miette::miette!("flush: {e}"))?;
            let mut path_input = String::new();
            io::stdin().lock().read_line(&mut path_input)
                .map_err(|e| miette::miette!("Failed to read input: {e}"))?;
            let trimmed = path_input.trim();
            if trimmed.is_empty() {
                return Err(miette::miette!("No path provided"));
            }
            Ok(DeployTarget {
                path: trimmed.to_string(),
                target_override: None,
            })
        }
        other => Err(miette::miette!("Invalid choice: '{}'; expected 1-6", other)),
    }
}

fn cmd_build(path: &str, target: &str, output: Option<&str>, token_budget: Option<usize>) -> Result<()> {
    match target {
        "skillmd" => {
            let mut ast = read_and_parse(path)?;
            let base_dir = std::path::Path::new(path)
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            let mut checker = Checker::with_base_dir(base_dir);
            if let Err(errors) = checker.check(&ast) {
                for err in &errors {
                    eprintln!("error: {}", err);
                }
                return Err(miette::miette!(
                    "{} error(s) found in '{}'; fix them before building",
                    errors.len(),
                    path
                ));
            }

            if let Some(budget) = token_budget {
                for skill in &mut ast.skills {
                    let trimmed = budget::trim_to_budget(&mut skill.body.contexts, budget);
                    for t in &trimmed {
                        eprintln!(
                            "⚠ trimmed context (priority {:?}, ~{} tokens): {}",
                            t.priority, t.estimated_tokens, t.snippet
                        );
                    }
                }
            }

            let compiler = SkillMdCompiler::new();
            let out_base = output.unwrap_or(".");

            for skill in &ast.skills {
                let skill_dir = Path::new(out_base).join(&skill.name);
                fs::create_dir_all(&skill_dir).map_err(|e| {
                    miette::miette!(
                        "Failed to create output directory '{}': {}",
                        skill_dir.display(),
                        e
                    )
                })?;

                let out_path = skill_dir.join("SKILL.md");
                let content = compiler.compile(skill, &ast);
                fs::write(&out_path, &content).map_err(|e| {
                    miette::miette!(
                        "Failed to write '{}': {}",
                        out_path.display(),
                        e
                    )
                })?;

                println!("✓ {} → {}", path, out_path.display());
            }
            Ok(())
        }
        "native" => {
            let source_text = fs::read_to_string(path)
                .map_err(|e| miette::miette!("Failed to read '{}': {}", path, e))?;
            let ast = read_and_parse(path)?;
            let compiler = IrCompiler::new();
            let pkg = compiler.compile_pkg(&ast, &source_text);
            let out_dir = output.unwrap_or(".");
            let stem = Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path)
                .replace(".agent", "");
            let pkg_dir = Path::new(out_dir).join(format!("{stem}.agentpkg"));
            compiler.write_to_dir(&pkg, &pkg_dir)
                .map_err(|e| miette::miette!("Failed to write native package: {e}"))?;
            eprintln!("✓ {path} → {}", pkg_dir.display());
            Ok(())
        }
        "system-prompt" | "cursor" | "clinerules" => {
            let ast = read_and_parse(path)?;
            let compiler: Box<dyn TargetCompiler> = match target {
                "system-prompt" => Box::new(SystemPromptCompiler),
                "cursor" => Box::new(CursorCompiler),
                "clinerules" => Box::new(ClineRulesCompiler),
                _ => unreachable!(),
            };
            let out_base = output.unwrap_or(".");
            fs::create_dir_all(out_base).map_err(|e| {
                miette::miette!("Failed to create output directory '{}': {}", out_base, e)
            })?;
            for skill in &ast.skills {
                let content = compiler.compile_skill(skill, &ast);
                let ext = compiler.file_extension();
                let out_path = Path::new(out_base).join(format!("{}.{}", skill.name, ext));
                fs::write(&out_path, &content).map_err(|e| {
                    miette::miette!("Failed to write '{}': {}", out_path.display(), e)
                })?;
                println!("✓ {} → {}", path, out_path.display());
            }
            Ok(())
        }
        other => Err(miette::miette!(
            "unknown target '{}'; supported: skillmd, native, system-prompt, cursor, clinerules",
            other
        )),
    }
}

fn cmd_emit_telemetry(path: &str) -> Result<()> {
    let ast = read_and_parse(path)?;
    let mut schema = serde_json::json!({ "skills": {} });
    for skill in &ast.skills {
        if let Some(observe) = &skill.body.observe {
            let events: Vec<serde_json::Value> = observe.events.iter()
                .map(|e| serde_json::json!({ "trigger": e.trigger, "event_name": e.event_name }))
                .collect();
            let metrics: Vec<serde_json::Value> = observe.metrics.iter()
                .map(|m| serde_json::json!({ "name": m.name }))
                .collect();
            schema["skills"][&skill.name] = serde_json::json!({
                "events": events,
                "metrics": metrics,
            });
        }
    }
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    Ok(())
}

fn cmd_build_watch(path: &str, target: &str, output: Option<&str>) -> Result<()> {
    use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
    use std::sync::mpsc;
    use std::time::Duration;

    eprintln!("Watching {} for changes (Ctrl-C to stop)...", path);
    if let Err(e) = cmd_build(path, target, output, None) {
        eprintln!("{}", e);
    }

    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(500), tx)
        .map_err(|e| miette::miette!("Failed to start file watcher: {e}"))?;

    let watch_path = Path::new(path).canonicalize()
        .map_err(|e| miette::miette!("Failed to resolve path '{}': {e}", path))?;
    let watch_dir = watch_path.parent().unwrap_or(Path::new("."));

    debouncer.watcher().watch(watch_dir, notify::RecursiveMode::Recursive)
        .map_err(|e| miette::miette!("Failed to watch '{}': {e}", watch_dir.display()))?;

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                let dominated = events.iter().any(|e| {
                    e.kind == DebouncedEventKind::Any
                        && e.path.extension().is_some_and(|ext| ext == "agent")
                });
                if dominated {
                    eprintln!("\n[{}] Change detected, rebuilding...",
                        chrono_now());
                    if let Err(e) = cmd_build(path, target, output, None) {
                        eprintln!("{}", e);
                    }
                }
            }
            Ok(Err(errs)) => {
                eprintln!("watch error: {errs}");
            }
            Err(_) => break,
        }
    }

    Ok(())
}

fn chrono_now() -> String {
    use std::time::SystemTime;
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() % 86400;
    format!("{:02}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
}

fn cmd_init(name: &str) -> Result<()> {
    let filename = format!("{}.agent", name);
    if Path::new(&filename).exists() {
        return Err(miette::miette!(
            "File '{}' already exists",
            filename
        ));
    }

    let template = format!(
        r#"skill "{name}" {{
  input {{
    query: string
  }}
  output {{
    result: string
  }}
  body {{
    context {{ "You are a helpful assistant." }}
    step main {{
      emit output
      context {{ "Answer the query provided in the input." }}
    }}
  }}
}}
"#,
        name = name
    );

    fs::write(&filename, &template).map_err(|e| {
        miette::miette!("Failed to write '{}': {}", filename, e)
    })?;

    println!("✓ created {}", filename);
    Ok(())
}

fn cmd_fmt(path: &str) -> Result<()> {
    let ast = read_and_parse(path)?;
    let formatted = Formatter::format(&ast);
    fs::write(path, &formatted).map_err(|e| {
        miette::miette!("Failed to write '{}': {}", path, e)
    })?;
    println!("✓ formatted {}", path);
    Ok(())
}

fn cmd_budget(path: &str) -> Result<()> {
    let ast = read_and_parse(path)?;
    let report = budget::estimate_budget(&ast);
    print!("{}", report);
    Ok(())
}

fn cmd_deps(path: &str, format: &str) -> Result<()> {
    let ast = read_and_parse(path)?;

    match format {
        "mermaid" => {
            print!("{}", emit_mermaid(&ast));
            Ok(())
        }
        "text" => {
            for skill in &ast.skills {
                println!("Skill: {}", skill.name);
                for step in &skill.body.steps {
                    let deps = format_dep(&step.requires);
                    if deps.is_empty() {
                        println!("  {}", step.name);
                    } else {
                        println!("  {} → {}", step.name, deps);
                    }
                }
                println!();
            }

            for pipeline in &ast.pipelines {
                println!("Pipeline: {}", pipeline.name);
                for stage in &pipeline.stages {
                    let deps = format_dep(&stage.requires);
                    if deps.is_empty() {
                        println!("  {}", stage.name);
                    } else {
                        println!("  {} → {}", stage.name, deps);
                    }
                }
                println!();
            }

            for orch in &ast.orchestrations {
                println!("Orchestration: {}", orch.name);
                for phase in &orch.phases {
                    let deps = format_dep(&phase.requires);
                    if deps.is_empty() {
                        println!("  {}", phase.name);
                    } else {
                        println!("  {} → {}", phase.name, deps);
                    }
                }
                println!();
            }

            Ok(())
        }
        other => Err(miette::miette!("unknown format '{}'; supported: text, mermaid", other)),
    }
}

fn format_dep(dep: &Option<Dependency>) -> String {
    match dep {
        None => String::new(),
        Some(Dependency::Single(name)) => name.clone(),
        Some(Dependency::All(names)) => names.join(", "),
        Some(Dependency::Any(names)) => format!("any({})", names.join(", ")),
        Some(Dependency::AllSteps) => "all_steps".to_string(),
    }
}

fn cmd_migrate(path: &str) -> Result<()> {
    let p = Path::new(path);

    if p.is_dir() {
        return cmd_migrate_directory(p);
    }

    let source = fs::read_to_string(path)
        .map_err(|e| miette::miette!("Failed to read '{}': {}", path, e))?;

    let result = migrate::migrate_skillmd(&source, path);

    let out_path = if path.ends_with(".md") {
        path.replace(".md", ".agent.partial")
    } else {
        format!("{}.agent.partial", path)
    };

    fs::write(&out_path, &result.output).map_err(|e| {
        miette::miette!("Failed to write '{}': {}", out_path, e)
    })?;

    println!("✓ migrated {} → {}", path, out_path);
    println!("\nPreview:");
    println!("{}", result.output);
    Ok(())
}

fn cmd_migrate_directory(dir: &Path) -> Result<()> {
    let result = match migrate::migrate_directory(dir) {
        Ok(r) => r,
        Err(e) if e.contains("No SKILL.md found") => {
            return cmd_migrate_batch(dir);
        }
        Err(e) => return Err(miette::miette!("{}", e)),
    };

    let dir_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "migrated".to_string());

    let out_path = dir.join(format!("{}.agent.partial", dir_name));

    fs::write(&out_path, &result.output).map_err(|e| {
        miette::miette!("Failed to write '{}': {}", out_path.display(), e)
    })?;

    for warning in &result.warnings {
        eprintln!("⚠ {}", warning);
    }

    println!(
        "✓ migrated {} → {} (found {} additional file(s), {} truncated)",
        dir.display(),
        out_path.display(),
        result.files_found,
        result.files_truncated,
    );
    println!("\nPreview:");
    println!("{}", result.output);
    Ok(())
}

fn cmd_migrate_batch(dir: &Path) -> Result<()> {
    let mut subdirs: Vec<_> = fs::read_dir(dir)
        .map_err(|e| miette::miette!("Failed to read '{}': {}", dir.display(), e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();

    subdirs.sort();

    let skill_dirs: Vec<&Path> = subdirs.iter()
        .filter(|p| p.join("SKILL.md").exists())
        .map(|p| p.as_path())
        .collect();
    let non_skill_dirs: Vec<&Path> = subdirs.iter()
        .filter(|p| !p.join("SKILL.md").exists())
        .map(|p| p.as_path())
        .collect();

    if skill_dirs.is_empty() {
        return Err(miette::miette!(
            "No SKILL.md found in '{}' or any immediate subdirectory",
            dir.display()
        ));
    }

    // Build sibling skill summaries (name + description from each SKILL.md frontmatter)
    let sibling_summaries: Vec<(String, String, String)> = skill_dirs.iter()
        .filter_map(|d| {
            let skill_md = d.join("SKILL.md");
            let content = fs::read_to_string(&skill_md).ok()?;
            let dir_name = d.file_name()?.to_string_lossy().to_string();
            let (fm, _, _) = migrate::parse_frontmatter_pub(&content);
            let name = fm.get("name").cloned().unwrap_or_else(|| dir_name.clone());
            let desc = fm.get("description").cloned().unwrap_or_default();
            Some((dir_name, name, desc))
        })
        .collect();

    // Collect shared (non-skill) directories as cross-reference context
    let shared_files: Vec<migrate::CollectedFile> = non_skill_dirs.iter()
        .flat_map(|d| {
            let mut files = migrate::collect_directory_files(d);
            // Rewrite paths relative to the parent dir
            for f in &mut files {
                let dir_name = d.file_name().unwrap_or_default().to_string_lossy();
                f.relative_path = f.relative_path.replacen("./", &format!("./{}/", dir_name), 1);
            }
            files
        })
        .collect();

    let has_shared = !shared_files.is_empty();

    println!(
        "Found {} skill directories in {}{}",
        skill_dirs.len(),
        dir.display(),
        if has_shared {
            format!(" (+{} shared file(s) from {} non-skill dir(s))",
                shared_files.len(),
                non_skill_dirs.len())
        } else {
            String::new()
        }
    );
    println!();

    let mut succeeded = 0;
    let mut failed = 0;

    for subdir in &skill_dirs {
        let current_dir_name = subdir.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        match migrate::migrate_directory(subdir) {
            Ok(mut result) => {
                let insertion_point = result.output.rfind("  }\n}\n").unwrap_or(result.output.len());
                let mut extra_context = String::new();

                // Append sibling skills context
                if sibling_summaries.len() > 1 {
                    extra_context.push_str("\n    // === SIBLING SKILLS ===\n");
                    extra_context.push_str("    // Other skills in this directory that this skill may reference or orchestrate.\n");
                    extra_context.push_str("    // TODO: The skillspec-migrate skill should determine if this skill\n");
                    extra_context.push_str("    //   orchestrates, pipelines, or chains any of these siblings.\n\n");
                    for (dir_name, name, desc) in &sibling_summaries {
                        if dir_name == &current_dir_name {
                            continue;
                        }
                        extra_context.push_str(&format!(
                            "    // @{} (dir: {}/): {}\n",
                            name, dir_name, desc
                        ));
                    }
                    extra_context.push('\n');
                }

                // Append shared cross-reference context
                if has_shared {
                    extra_context.push_str("    // === CROSS-REFERENCE CONTEXT (shared directories) ===\n");
                    extra_context.push_str("    // These files are from sibling directories referenced by this skill.\n\n");
                    for f in &shared_files {
                        extra_context.push_str(&format!(
                            "    // --- {} ({} lines{}) ---\n",
                            f.relative_path,
                            f.line_count,
                            if f.truncated { ", truncated" } else { "" }
                        ));
                        for line in f.content.lines() {
                            extra_context.push_str(&format!("    // {}\n", line));
                        }
                        extra_context.push('\n');
                    }
                }

                if !extra_context.is_empty() {
                    result.output.insert_str(insertion_point, &extra_context);
                }

                let dir_name = subdir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "migrated".to_string());

                let out_path = subdir.join(format!("{}.agent.partial", dir_name));

                if let Err(e) = fs::write(&out_path, &result.output) {
                    eprintln!("✗ {} — failed to write: {}", subdir.display(), e);
                    failed += 1;
                    continue;
                }

                for warning in &result.warnings {
                    eprintln!("  ⚠ {}", warning);
                }

                println!(
                    "  ✓ {} → {} ({} additional file(s){})",
                    subdir.file_name().unwrap_or_default().to_string_lossy(),
                    out_path.file_name().unwrap_or_default().to_string_lossy(),
                    result.files_found,
                    if has_shared { format!(", +{} shared", shared_files.len()) } else { String::new() },
                );
                succeeded += 1;
            }
            Err(e) => {
                eprintln!(
                    "  ✗ {} — {}",
                    subdir.file_name().unwrap_or_default().to_string_lossy(),
                    e
                );
                failed += 1;
            }
        }
    }

    // Generate top-level orchestration partial
    if skill_dirs.len() > 1 {
        let parent_name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "orchestration".to_string());

        let mut orch = String::new();
        orch.push_str(&format!("// Auto-generated orchestration scaffold for: {}\n", dir.display()));
        orch.push_str("// TODO: The skillspec-migrate skill should determine the orchestration structure.\n");
        orch.push_str("//   - Which skills form a pipeline (sequential validation flow)?\n");
        orch.push_str("//   - Which skill is the router/orchestrator (e.g., playbook)?\n");
        orch.push_str("//   - What are the conditional routing rules?\n");
        orch.push_str("//   - Which skills are independent (invoked ad-hoc)?\n\n");

        orch.push_str("// === SKILLS IN THIS DIRECTORY ===\n\n");
        for (dir_name, name, desc) in &sibling_summaries {
            orch.push_str(&format!("// @{} (dir: {}/)\n", name, dir_name));
            orch.push_str(&format!("//   {}\n\n", desc));
        }

        // Look for orchestration signals in the SKILL.md files
        orch.push_str("// === ORCHESTRATION SIGNALS ===\n");
        orch.push_str("// Cross-references between skills found in SKILL.md files:\n\n");
        for skill_dir in &skill_dirs {
            let skill_md = skill_dir.join("SKILL.md");
            if let Ok(content) = fs::read_to_string(&skill_md) {
                let skill_name = skill_dir.file_name()
                    .unwrap_or_default().to_string_lossy().to_string();
                let mut refs_found = Vec::new();
                for (other_dir, other_name, _) in &sibling_summaries {
                    if other_dir == &skill_name {
                        continue;
                    }
                    let at_ref = format!("@{}", other_name);
                    let dir_ref = format!("../{}/", other_dir);
                    if content.contains(&at_ref) || content.contains(&dir_ref) {
                        refs_found.push(other_name.as_str());
                    }
                }
                if !refs_found.is_empty() {
                    orch.push_str(&format!(
                        "// {} references: {}\n",
                        skill_name,
                        refs_found.join(", ")
                    ));
                }
            }
        }

        // Check for pipeline/sequence patterns
        orch.push_str("\n// === DETECTED PATTERNS ===\n");
        for skill_dir in &skill_dirs {
            let skill_md = skill_dir.join("SKILL.md");
            if let Ok(content) = fs::read_to_string(&skill_md) {
                let skill_name = skill_dir.file_name()
                    .unwrap_or_default().to_string_lossy().to_string();
                if content.contains("->") || content.contains("→") {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if (trimmed.contains("->") || trimmed.contains("→"))
                            && trimmed.contains('@')
                        {
                            orch.push_str(&format!(
                                "// [{}] pipeline signal: {}\n",
                                skill_name, trimmed
                            ));
                        }
                    }
                }
                if content.contains("routing") || content.contains("Routing") || content.contains("triage") {
                    orch.push_str(&format!(
                        "// [{}] appears to be a router/orchestrator (contains routing/triage language)\n",
                        skill_name
                    ));
                }
            }
        }

        orch.push_str("\n// TODO: Based on the above signals, the skillspec-migrate skill should produce:\n");
        orch.push_str("//   - A pipeline construct for any sequential skill chains\n");
        orch.push_str("//   - An orchestration construct if there's a router skill dispatching to specialists\n");
        orch.push_str("//   - import statements for each skill\n");
        orch.push('\n');

        let orch_path = dir.join(format!("{}.orchestration.agent.partial", parent_name));
        fs::write(&orch_path, &orch).map_err(|e| {
            miette::miette!("Failed to write orchestration partial: {}", e)
        })?;
        println!(
            "  ✓ {} (orchestration scaffold)",
            orch_path.file_name().unwrap_or_default().to_string_lossy()
        );
    }

    println!(
        "\nBatch complete: {} succeeded, {} failed",
        succeeded, failed
    );

    if failed > 0 && succeeded == 0 {
        Err(miette::miette!("All migrations failed"))
    } else {
        Ok(())
    }
}

fn cmd_pack(path: &str, output: Option<&str>) -> Result<()> {
    let ast = read_and_parse(path)?;

    if ast.packages.is_empty() {
        return Err(miette::miette!(
            "'{}' contains no package declaration",
            path
        ));
    }

    let pkg = &ast.packages[0];

    // Validate: every exported symbol must exist as a skill or type
    let skill_names: std::collections::HashSet<String> = ast
        .skills
        .iter()
        .map(|s| {
            // Skills are addressed by their name with hyphens replaced by underscores
            // in exports (common convention). Accept both forms.
            s.name.replace('-', "_")
        })
        .collect();
    let type_names: std::collections::HashSet<String> =
        ast.type_defs.iter().map(|t| t.name.clone()).collect();

    for export in &pkg.exports {
        if !skill_names.contains(export.as_str()) && !type_names.contains(export.as_str()) {
            return Err(miette::miette!(
                "Exported symbol '{}' not found in '{}' (declare a skill or type with that name)",
                export,
                path
            ));
        }
    }

    // Determine output directory: <name_sanitised>@<version>.skillpkg/
    let pkg_dir_name = format!(
        "{}@{}.skillpkg",
        pkg.name.replace('/', "_").replace('@', ""),
        pkg.version
    );
    let out_base = output.unwrap_or(".");
    let pkg_dir = Path::new(out_base).join(&pkg_dir_name);

    fs::create_dir_all(&pkg_dir).map_err(|e| {
        miette::miette!("Failed to create package directory '{}': {}", pkg_dir.display(), e)
    })?;

    // Write package.json metadata
    let metadata = serde_json::json!({
        "name": pkg.name,
        "version": pkg.version,
        "description": pkg.description,
        "exports": pkg.exports,
    });
    let pkg_json_path = pkg_dir.join("package.json");
    fs::write(&pkg_json_path, serde_json::to_string_pretty(&metadata).unwrap()).map_err(|e| {
        miette::miette!("Failed to write '{}': {}", pkg_json_path.display(), e)
    })?;
    println!("  ✓ {}", pkg_json_path.display());

    // Compile each exported skill and write its SKILL.md
    let compiler = SkillMdCompiler::new();
    for skill in &ast.skills {
        let normalised_name = skill.name.replace('-', "_");
        if pkg.exports.contains(&normalised_name) {
            let skill_dir = pkg_dir.join(&skill.name);
            fs::create_dir_all(&skill_dir).map_err(|e| {
                miette::miette!("Failed to create skill dir '{}': {}", skill_dir.display(), e)
            })?;
            let skill_md_path = skill_dir.join("SKILL.md");
            let content = compiler.compile(skill, &ast);
            fs::write(&skill_md_path, &content).map_err(|e| {
                miette::miette!("Failed to write '{}': {}", skill_md_path.display(), e)
            })?;
            println!("  ✓ {}", skill_md_path.display());
        }
    }

    // Write type definitions as .types.json
    let exported_types: Vec<serde_json::Value> = ast
        .type_defs
        .iter()
        .filter(|t| pkg.exports.contains(&t.name))
        .map(|t| {
            let fields: Vec<serde_json::Value> = t
                .fields
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "optional": f.optional,
                    })
                })
                .collect();
            serde_json::json!({
                "name": t.name,
                "fields": fields,
            })
        })
        .collect();

    if !exported_types.is_empty() {
        let types_path = pkg_dir.join(".types.json");
        fs::write(&types_path, serde_json::to_string_pretty(&exported_types).unwrap()).map_err(
            |e| miette::miette!("Failed to write '{}': {}", types_path.display(), e),
        )?;
        println!("  ✓ {}", types_path.display());
    }

    println!("✓ packed '{}' → {}/", pkg.name, pkg_dir.display());
    Ok(())
}

fn cmd_install(path: &str) -> Result<()> {
    // Determine whether path is a .skillpkg dir or an .agent file
    let pkg_source = Path::new(path);

    let (pkg_name, pkg_version, pkg_dir_to_copy): (String, String, std::path::PathBuf) =
        if pkg_source.is_dir() {
            // It's already a .skillpkg directory — read metadata from package.json
            let pkg_json = pkg_source.join("package.json");
            let content = fs::read_to_string(&pkg_json).map_err(|e| {
                miette::miette!("Failed to read '{}': {}", pkg_json.display(), e)
            })?;
            let meta: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
                miette::miette!("Failed to parse '{}': {}", pkg_json.display(), e)
            })?;
            let name = meta["name"]
                .as_str()
                .ok_or_else(|| miette::miette!("'name' missing from package.json"))?
                .to_string();
            let version = meta["version"]
                .as_str()
                .ok_or_else(|| miette::miette!("'version' missing from package.json"))?
                .to_string();
            (name, version, pkg_source.to_path_buf())
        } else {
            // It's an .agent file — pack it first into a temp location, then install
            let ast = read_and_parse(path)?;
            if ast.packages.is_empty() {
                return Err(miette::miette!(
                    "'{}' contains no package declaration",
                    path
                ));
            }
            let pkg = &ast.packages[0];
            let temp_dir = std::env::temp_dir().join(format!(
                "skillspec_pack_{}",
                std::process::id()
            ));
            cmd_pack(path, Some(temp_dir.to_str().unwrap()))?;

            let pkg_dir_name = format!(
                "{}@{}.skillpkg",
                pkg.name.replace('/', "_").replace('@', ""),
                pkg.version
            );
            let built_dir = temp_dir.join(&pkg_dir_name);
            (pkg.name.clone(), pkg.version.clone(), built_dir)
        };

    // Destination: .skillspec/packages/<name>@<version>/
    // Sanitise name for filesystem use
    let safe_name = pkg_name.replace('/', "_").replace('@', "");
    let dest_dir = Path::new(".skillspec")
        .join("packages")
        .join(format!("{}@{}", safe_name, pkg_version));

    fs::create_dir_all(&dest_dir).map_err(|e| {
        miette::miette!(
            "Failed to create install directory '{}': {}",
            dest_dir.display(),
            e
        )
    })?;

    // Recursively copy the .skillpkg directory contents into dest_dir
    copy_dir_all(&pkg_dir_to_copy, &dest_dir)?;

    println!(
        "✓ installed '{}@{}' → {}",
        pkg_name,
        pkg_version,
        dest_dir.display()
    );
    Ok(())
}

/// Recursively copy directory contents from `src` to `dst`.
fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    for entry in fs::read_dir(src).map_err(|e| {
        miette::miette!("Failed to read directory '{}': {}", src.display(), e)
    })? {
        let entry = entry.map_err(|e| miette::miette!("Directory entry error: {}", e))?;
        let entry_path = entry.path();
        let dest_path = dst.join(entry.file_name());

        if entry_path.is_dir() {
            fs::create_dir_all(&dest_path).map_err(|e| {
                miette::miette!("Failed to create dir '{}': {}", dest_path.display(), e)
            })?;
            copy_dir_all(&entry_path, &dest_path)?;
        } else {
            fs::copy(&entry_path, &dest_path).map_err(|e| {
                miette::miette!(
                    "Failed to copy '{}' → '{}': {}",
                    entry_path.display(),
                    dest_path.display(),
                    e
                )
            })?;
        }
    }
    Ok(())
}

fn cmd_test(path: &str, prepare: bool, evaluate: Option<&str>) -> Result<()> {
    let ast = read_and_parse(path)?;

    if prepare {
        use skillspec_core::test_harness::prepare_test_skill;
        for skill in &ast.skills {
            if skill.tests.is_empty() { continue; }
            let test_skill = prepare_test_skill(skill, &ast);
            let out_dir = format!("{}.test", skill.name);
            fs::create_dir_all(&out_dir).map_err(|e| miette::miette!("mkdir: {e}"))?;
            let out_path = format!("{}/SKILL.md", out_dir);
            fs::write(&out_path, &test_skill).map_err(|e| miette::miette!("write: {e}"))?;
            println!("✓ test skill → {}", out_path);
        }
        return Ok(());
    }

    if let Some(results_path) = evaluate {
        use skillspec_core::test_harness::{evaluate_assertion, evaluate_confidence};
        let results_text = fs::read_to_string(results_path)
            .map_err(|e| miette::miette!("Failed to read '{}': {}", results_path, e))?;
        let results: serde_json::Value = serde_json::from_str(&results_text)
            .map_err(|e| miette::miette!("Failed to parse results JSON: {}", e))?;

        let test_cases = results["test_cases"].as_array()
            .ok_or_else(|| miette::miette!("results JSON missing 'test_cases' array"))?;

        let mut total_pass = 0;
        let mut total_fail = 0;

        for skill in &ast.skills {
            for test in &skill.tests {
                let case = test_cases.iter()
                    .find(|tc| tc["name"].as_str() == Some(&test.name));
                let case = match case {
                    Some(c) => c,
                    None => {
                        eprintln!("⚠ test '{}' not found in results", test.name);
                        total_fail += 1;
                        continue;
                    }
                };

                let runs = case["runs"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
                let mut run_results = Vec::new();

                for run in runs {
                    let mut all_pass = true;
                    for exp in &test.expectations {
                        let actual = match &exp.assertion {
                            skillspec_core::ast::AssertionExpr::Resembles(_) => {
                                run.get("resembles_verdicts")
                                    .and_then(|v| v.get(&exp.path))
                                    .cloned()
                                    .map(|v| serde_json::json!({"resembles_verdict": v}))
                                    .unwrap_or_else(|| navigate_json(run, &exp.path))
                            }
                            skillspec_core::ast::AssertionExpr::Satisfies(_) => {
                                run.get("satisfies_verdicts")
                                    .and_then(|v| v.get(&exp.path))
                                    .cloned()
                                    .map(|v| serde_json::json!({"satisfies_verdict": v}))
                                    .unwrap_or_else(|| navigate_json(run, &exp.path))
                            }
                            _ => navigate_json(run, &exp.path),
                        };
                        let r = evaluate_assertion(&exp.assertion, &actual);
                        if !r.passed {
                            eprintln!("  ✗ {}.{}: {}", test.name, exp.path, r.message);
                            all_pass = false;
                        }
                    }
                    run_results.push(all_pass);
                }

                if let Some(conf) = test.confidence {
                    let met = evaluate_confidence(&run_results, conf);
                    if met {
                        eprintln!("  ✓ {} (confidence {:.0}% met)", test.name, conf * 100.0);
                        total_pass += 1;
                    } else {
                        let pass_rate = run_results.iter().filter(|&&r| r).count() as f64 / run_results.len().max(1) as f64;
                        eprintln!("  ✗ {} (confidence {:.0}% needed, got {:.0}%)", test.name, conf * 100.0, pass_rate * 100.0);
                        total_fail += 1;
                    }
                } else if run_results.iter().all(|&r| r) {
                    eprintln!("  ✓ {}", test.name);
                    total_pass += 1;
                } else {
                    total_fail += 1;
                }
            }
        }

        eprintln!("\n{} passed, {} failed", total_pass, total_fail);
        if total_fail > 0 {
            return Err(miette::miette!("{} test(s) failed", total_fail));
        }
        return Ok(());
    }

    let mut total = 0;
    for skill in &ast.skills {
        if skill.tests.is_empty() { continue; }
        eprintln!("Skill: {} ({} tests)", skill.name, skill.tests.len());
        for test in &skill.tests {
            eprintln!("  - {}", test.name);
            total += 1;
        }
        eprintln!();
    }

    if total == 0 {
        eprintln!("No tests found in '{path}'.");
    } else {
        eprintln!("To execute tests:\n  skillspec test {path} --prepare\n  <run the test skill in your agent runtime>\n  skillspec test {path} --evaluate results.json");
    }

    Ok(())
}

fn navigate_json<'a>(value: &'a serde_json::Value, path: &str) -> serde_json::Value {
    let mut current = value.clone();
    for part in path.split('.') {
        current = current.get(part).cloned().unwrap_or(serde_json::Value::Null);
    }
    current
}

fn cmd_diff(path_a: &str, path_b: &str, against_skillmd: bool, semver: bool) -> Result<()> {
    if against_skillmd {
        let ast = read_and_parse(path_a)?;
        let compiler = SkillMdCompiler::new();
        let actual = fs::read_to_string(path_b)
            .map_err(|e| miette::miette!("Failed to read '{path_b}': {e}"))?;

        for skill in &ast.skills {
            let compiled = compiler.compile(skill, &ast);
            let report = skillmd_diff(&compiled, &actual);
            if report.is_empty() {
                eprintln!("✓ No differences between compiled '{}' and '{path_b}'", skill.name);
            } else {
                eprint!("{}", report.display());
            }
        }
    } else if semver {
        let ast_a = read_and_parse(path_a)?;
        let ast_b = read_and_parse(path_b)?;
        let report = classify_semver(&ast_a, &ast_b);
        eprint!("{}", report.display());
    } else {
        let ast_a = read_and_parse(path_a)?;
        let ast_b = read_and_parse(path_b)?;
        let report = structural_diff(&ast_a, &ast_b);

        if report.is_empty() {
            eprintln!("✓ No structural differences between '{path_a}' and '{path_b}'");
        } else {
            eprintln!("Structural diff: {path_a} vs {path_b}");
            eprint!("{}", report.display());
        }
    }

    Ok(())
}
