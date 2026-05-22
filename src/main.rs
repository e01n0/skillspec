use std::fs;
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
        /// Watch for file changes and rebuild automatically
        #[arg(long)]
        watch: bool,
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
    /// Mechanically extract a SKILL.md into a .agent.partial file
    Migrate { file: String },
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
    /// List all tests defined in an .agent file
    Test { file: String },
    /// Run lint rules to catch quality issues beyond structural validity
    Lint { file: String },
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
        Commands::Build { file, target, output, watch } => {
            if watch {
                cmd_build_watch(&file, &target, output.as_deref())
            } else {
                cmd_build(&file, &target, output.as_deref())
            }
        }
        Commands::Init { name } => cmd_init(&name),
        Commands::Fmt { file } => cmd_fmt(&file),
        Commands::Budget { file } => cmd_budget(&file),
        Commands::Deps { file, format } => cmd_deps(&file, &format),
        Commands::Migrate { file } => cmd_migrate(&file),
        Commands::Pack { file, output } => cmd_pack(&file, output.as_deref()),
        Commands::Install { path } => cmd_install(&path),
        Commands::Test { file } => cmd_test(&file),
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

fn cmd_build(path: &str, target: &str, output: Option<&str>) -> Result<()> {
    match target {
        "skillmd" => {
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
                    "{} error(s) found in '{}'; fix them before building",
                    errors.len(),
                    path
                ));
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
            let ast = read_and_parse(path)?;
            let compiler = IrCompiler::new();
            let bytes = compiler
                .compile(&ast)
                .map_err(|e| miette::miette!("IR compilation failed: {e}"))?;
            let out_dir = output.unwrap_or(".");
            // Extract just the file stem so absolute input paths don't corrupt the output path.
            let stem = Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path)
                .replace(".agent", "");
            let out_path = format!("{out_dir}/{stem}.agentpkg");
            std::fs::write(&out_path, &bytes)
                .map_err(|e| miette::miette!("Failed to write {out_path}: {e}"))?;
            eprintln!("✓ {path} → {out_path}");
            Ok(())
        }
        other => Err(miette::miette!("unknown target '{}'; supported: skillmd, native", other)),
    }
}

fn cmd_build_watch(path: &str, target: &str, output: Option<&str>) -> Result<()> {
    use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
    use std::sync::mpsc;
    use std::time::Duration;

    eprintln!("Watching {} for changes (Ctrl-C to stop)...", path);
    if let Err(e) = cmd_build(path, target, output) {
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
                    if let Err(e) = cmd_build(path, target, output) {
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
    let source = fs::read_to_string(path)
        .map_err(|e| miette::miette!("Failed to read '{}': {}", path, e))?;

    let result = migrate::migrate_skillmd(&source, path);

    // Write to .agent.partial alongside the source
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

fn cmd_test(path: &str) -> Result<()> {
    let ast = read_and_parse(path)?;

    let mut total = 0;
    for skill in &ast.skills {
        if skill.tests.is_empty() {
            continue;
        }
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
        eprintln!("To execute tests, run the skillspec-test skill in your agent runtime.");
    }

    Ok(())
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
