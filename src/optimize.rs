use miette::Result;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::ast::{ContextBlock, Priority, SamplingDirective, SourceFile};
use crate::checker::Checker;
use crate::compiler_skillmd::SkillMdCompiler;
use crate::formatter::Formatter;
use crate::test_harness::prepare_split_data;

// ── Proxy protocol types ────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmRequest {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub id: String,
    pub role: String,
    pub phase: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmResponse {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatusEvent {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epoch: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum ProxyMessage {
    #[serde(rename = "llm_request")]
    LlmRequest {
        id: String,
        role: String,
        phase: String,
        messages: Vec<ChatMessage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        temperature: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_tokens: Option<u32>,
    },
    #[serde(rename = "status")]
    Status {
        phase: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        step: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        epoch: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        score: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "complete")]
    Complete {
        best_skill: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        score: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        steps_run: Option<u32>,
    },
    #[serde(rename = "error")]
    Error { message: String },
}

// ── CLI config ──────────────────────────────────────────────────────────────

pub struct OptimizeConfig {
    pub file: String,
    pub setup: bool,
    pub dry_run: bool,
    pub prepare: bool,
    pub step: bool,
    pub resume: Option<String>,
    pub response: Option<String>,
    pub epochs: u32,
    pub batch_size: u32,
    pub edit_budget: u32,
    pub scheduler: String,
    pub output: Option<String>,
    pub writeback: bool,
    pub no_overwrite: bool,
}

// ── Optimize directory layout ───────────────────────────────────────────────

fn optimize_dir() -> &'static str {
    "optimize"
}

fn venv_python() -> String {
    format!("{}/venv/bin/python", optimize_dir())
}

fn venv_exists() -> bool {
    Path::new(&venv_python()).exists()
}

fn skillopt_installed() -> bool {
    Path::new(&format!("{}/skillopt", optimize_dir())).exists()
}

// ── Entry point ─────────────────────────────────────────────────────────────

pub fn cmd_optimize(config: OptimizeConfig, ast: &SourceFile) -> Result<()> {
    if config.setup {
        return cmd_setup();
    }

    if config.writeback {
        return cmd_writeback(&config, ast);
    }

    if config.dry_run {
        return cmd_dry_run(&config, ast);
    }

    if config.prepare {
        return cmd_prepare(&config, ast);
    }

    if config.step {
        return cmd_step(&config);
    }

    Err(miette::miette!(
        "Specify a mode: --setup, --prepare, --step, --writeback, or --dry-run\n\n\
         Typical workflow:\n  \
         1. skillspec optimize foo.agent --prepare\n  \
         2. skillspec optimize foo.agent --step          (repeat in agent loop)\n  \
         3. skillspec optimize foo.agent --writeback      (apply results)"
    ))
}

// ── Setup ───────────────────────────────────────────────────────────────────

fn cmd_setup() -> Result<()> {
    eprintln!("Setting up SkillOpt optimization environment...\n");

    let opt_dir = optimize_dir();
    fs::create_dir_all(opt_dir)
        .map_err(|e| miette::miette!("Failed to create {}: {}", opt_dir, e))?;

    let has_uv = Command::new("uv")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok();

    // Create Python venv
    if !venv_exists() {
        eprintln!("Creating Python virtual environment...");
        let status = if has_uv {
            Command::new("uv")
                .args(["venv", &format!("{}/venv", opt_dir), "--python", "3.12"])
                .status()
                .map_err(|e| miette::miette!("uv venv failed: {}", e))?
        } else {
            let python = find_python()?;
            Command::new(&python)
                .args(["-m", "venv", &format!("{}/venv", opt_dir)])
                .status()
                .map_err(|e| miette::miette!("Failed to create venv: {}", e))?
        };
        if !status.success() {
            return Err(miette::miette!("Failed to create Python venv"));
        }
        eprintln!("  ✓ venv created{}", if has_uv { " (via uv)" } else { "" });
    } else {
        eprintln!("  ✓ venv already exists");
    }

    // Clone SkillOpt
    if !skillopt_installed() {
        eprintln!("Cloning SkillOpt...");
        let status = Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "https://github.com/microsoft/SkillOpt.git",
                &format!("{}/skillopt", opt_dir),
            ])
            .status()
            .map_err(|e| miette::miette!("Failed to clone SkillOpt: {}", e))?;
        if !status.success() {
            return Err(miette::miette!("Failed to clone SkillOpt repo"));
        }
        eprintln!("  ✓ SkillOpt cloned");
    } else {
        eprintln!("  ✓ SkillOpt already present");
    }

    // Install SkillOpt + pyyaml into venv
    eprintln!("Installing SkillOpt into venv...");
    let skillopt_path = format!("{}/skillopt", opt_dir);
    let status = if has_uv {
        let venv_path = format!("{}/venv", opt_dir);
        Command::new("uv")
            .env("VIRTUAL_ENV", &venv_path)
            .args(["pip", "install", "-e", &skillopt_path, "pyyaml"])
            .status()
            .map_err(|e| miette::miette!("uv pip install failed: {}", e))?
    } else {
        let pip = format!("{}/venv/bin/pip", opt_dir);
        Command::new(&pip)
            .args(["install", "-e", &skillopt_path, "pyyaml"])
            .status()
            .map_err(|e| miette::miette!("pip install failed: {}", e))?
    };
    if !status.success() {
        return Err(miette::miette!("SkillOpt installation failed"));
    }
    eprintln!("  ✓ SkillOpt installed");

    // Write adapter files if missing
    write_adapter_files(opt_dir)?;

    // Write default config
    write_default_config(opt_dir)?;

    eprintln!("\n✓ Setup complete. Next:\n  skillspec optimize <file.agent> --prepare");
    Ok(())
}

fn find_python() -> Result<String> {
    for candidate in ["python3.12", "python3.11", "python3.10", "python3"] {
        if Command::new(candidate)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            return Ok(candidate.to_string());
        }
    }
    Err(miette::miette!(
        "Python 3.10+ not found. Install Python and ensure it's on PATH."
    ))
}

// ── Prepare ─────────────────────────────────────────────────────────────────

fn cmd_prepare(config: &OptimizeConfig, ast: &SourceFile) -> Result<()> {
    let compiler = SkillMdCompiler::new();

    for skill in &ast.skills {
        if skill.tests.is_empty() {
            eprintln!("⚠ skill '{}' has no tests, skipping", skill.name);
            continue;
        }

        let out_dir = config
            .output
            .clone()
            .unwrap_or_else(|| format!("{}.optimized", skill.name));
        fs::create_dir_all(&out_dir)
            .map_err(|e| miette::miette!("Failed to create {}: {}", out_dir, e))?;

        // Write initial skill
        let compiled = compiler.compile(skill, ast);
        let skill_path = format!("{}/initial_skill.md", out_dir);
        fs::write(&skill_path, &compiled)
            .map_err(|e| miette::miette!("Failed to write {}: {}", skill_path, e))?;
        eprintln!("  ✓ initial skill → {}", skill_path);

        // Write data splits (train, val, valid_seen, valid_unseen)
        let split_data = prepare_split_data(skill);
        let splits = [
            ("train", &split_data.train_items),
            ("val", &split_data.val_items),
            ("valid_seen", &split_data.valid_seen_items),
            ("valid_unseen", &split_data.valid_unseen_items),
        ];

        for (name, items) in &splits {
            let dir = format!("{}/splits/{}", out_dir, name);
            fs::create_dir_all(&dir).map_err(|e| miette::miette!("mkdir {}: {}", dir, e))?;
            let json = serde_json::to_string_pretty(items)
                .map_err(|e| miette::miette!("JSON serialize: {}", e))?;
            fs::write(format!("{}/items.json", dir), &json)
                .map_err(|e| miette::miette!("write: {}", e))?;
        }

        eprintln!(
            "  ✓ splits → {} train, {} val/valid_seen/valid_unseen",
            split_data.train_items.len(),
            split_data.val_items.len()
        );

        // Write SkillOpt config
        let skillopt_config =
            generate_config(config, &out_dir, &skill.name, split_data.train_items.len());
        let config_path = format!("{}/config.yaml", out_dir);
        fs::write(&config_path, &skillopt_config)
            .map_err(|e| miette::miette!("write config: {}", e))?;
        eprintln!("  ✓ config → {}", config_path);

        eprintln!(
            "\n✓ Prepared '{}'. Next:\n  skillspec optimize {} --step",
            skill.name, config.file
        );
    }

    Ok(())
}

// ── Dry run ─────────────────────────────────────────────────────────────────

fn cmd_dry_run(config: &OptimizeConfig, ast: &SourceFile) -> Result<()> {
    eprintln!("=== Dry Run ===\n");

    for skill in &ast.skills {
        if skill.tests.is_empty() {
            eprintln!("Skill '{}': no tests, would be skipped\n", skill.name);
            continue;
        }

        let split_data = prepare_split_data(skill);

        eprintln!("Skill: {}", skill.name);
        eprintln!("  Tests: {}", skill.tests.len());
        eprintln!("  Train items: {}", split_data.train_items.len());
        eprintln!("  Val items: {}", split_data.val_items.len());
        eprintln!("  Epochs: {}", config.epochs);
        eprintln!("  Batch size: {}", config.batch_size);
        eprintln!("  Edit budget: {}", config.edit_budget);
        eprintln!("  Scheduler: {}", config.scheduler);

        let total_steps = (split_data.train_items.len() as f64 / config.batch_size as f64).ceil()
            as u32
            * config.epochs;
        let est_requests = total_steps * 5; // rollout + reflect + aggregate + rank + evaluate
        eprintln!("  Estimated steps: {}", total_steps);
        eprintln!("  Estimated LLM round-trips: ~{}", est_requests);
        eprintln!("  Cost: $0 (agent-native mode — all calls handled by hosting agent)");
        eprintln!();
    }

    if !venv_exists() {
        eprintln!("⚠ SkillOpt venv not found. Run: skillspec optimize --setup");
    }

    Ok(())
}

// ── Step (checkpoint-resume) ────────────────────────────────────────────────

fn cmd_step(config: &OptimizeConfig) -> Result<()> {
    if !venv_exists() {
        return Err(miette::miette!(
            "SkillOpt not set up. Run: skillspec optimize --setup"
        ));
    }

    let out_dir = config.output.clone().unwrap_or_else(|| {
        let stem = Path::new(&config.file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("skill");
        format!("{}.optimized", stem)
    });

    let state_file = format!("{}/runtime_state.json", out_dir);
    let config_file = format!("{}/config.yaml", out_dir);
    let pipe_path = format!("{}/response_pipe", out_dir);

    if !Path::new(&config_file).exists() {
        return Err(miette::miette!(
            "No config found at {}. Run --prepare first.",
            config_file
        ));
    }

    // Create a FIFO (named pipe) for the agent to write responses to.
    // The Python process reads from this pipe; the agent writes via:
    //   echo '{"content":"..."}' > <output>/response_pipe
    if !Path::new(&pipe_path).exists() {
        let status = Command::new("mkfifo")
            .arg(&pipe_path)
            .status()
            .map_err(|e| miette::miette!("mkfifo failed: {}", e))?;
        if !status.success() {
            return Err(miette::miette!("Failed to create FIFO at {}", pipe_path));
        }
    }

    // If a response was provided inline, write it to the pipe in the background
    // before starting the Python process (otherwise Python blocks on open).
    if let Some(ref response_json) = config.response {
        let pipe_clone = pipe_path.clone();
        let resp_clone = response_json.clone();
        std::thread::spawn(move || {
            if let Ok(mut f) = fs::OpenOptions::new().write(true).open(&pipe_clone) {
                use std::io::Write as _;
                let _ = writeln!(f, "{}", resp_clone);
            }
        });
    }

    let cmd_args = vec![
        format!("{}/skillspec_adapter.py", optimize_dir()),
        "--config".to_string(),
        config_file,
        "--output-dir".to_string(),
        out_dir.clone(),
        "--proxy-mode".to_string(),
        "--pipe".to_string(),
        pipe_path.clone(),
    ];

    let python = venv_python();
    let mut child = Command::new(&python)
        .args(&cmd_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| miette::miette!("Failed to start Python adapter: {}", e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| miette::miette!("Failed to capture Python stdout"))?;
    let reader = BufReader::new(stdout);

    let mut request_count = 0u32;

    for line in reader.lines() {
        let line = line.map_err(|e| miette::miette!("Read error: {}", e))?;
        if line.trim().is_empty() {
            continue;
        }

        let msg: ProxyMessage = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(_) => {
                eprintln!("[adapter] {}", line);
                continue;
            }
        };

        match msg {
            ProxyMessage::LlmRequest {
                id,
                role,
                phase,
                messages,
                temperature,
                max_tokens,
            } => {
                request_count += 1;
                let request = serde_json::json!({
                    "type": "llm_request",
                    "id": id,
                    "role": role,
                    "phase": phase,
                    "messages": messages,
                    "temperature": temperature,
                    "max_tokens": max_tokens,
                    "state_file": state_file,
                    "pipe": pipe_path,
                    "request_number": request_count,
                });
                println!("{}", serde_json::to_string(&request).unwrap());

                // The Python process is now blocking, reading from the FIFO.
                // The agent should write a response to the pipe:
                //   echo '{"content":"..."}' > <pipe_path>
                // Then continue reading stdout for the next request.
            }
            ProxyMessage::Status {
                phase,
                step,
                epoch,
                score,
                message,
            } => {
                if let Some(msg) = message {
                    eprintln!("[{}] {}", phase, msg);
                } else {
                    eprintln!(
                        "[{}] epoch={} step={} score={}",
                        phase,
                        epoch.map(|e| e.to_string()).unwrap_or_default(),
                        step.map(|s| s.to_string()).unwrap_or_default(),
                        score.map(|s| format!("{:.3}", s)).unwrap_or_default(),
                    );
                }
            }
            ProxyMessage::Complete {
                best_skill,
                score,
                steps_run,
            } => {
                let result = serde_json::json!({
                    "type": "complete",
                    "best_skill": best_skill,
                    "score": score,
                    "steps_run": steps_run,
                });
                println!("{}", serde_json::to_string_pretty(&result).unwrap());

                let best_path = format!("{}/best_skill.md", out_dir);
                fs::write(&best_path, &best_skill)
                    .map_err(|e| miette::miette!("write best_skill: {}", e))?;
                eprintln!("\n✓ Optimization complete → {}", best_path);

                // Clean up the FIFO
                let _ = fs::remove_file(&pipe_path);
            }
            ProxyMessage::Error { message } => {
                let _ = fs::remove_file(&pipe_path);
                return Err(miette::miette!("Adapter error: {}", message));
            }
        }
    }

    let status = child
        .wait()
        .map_err(|e| miette::miette!("Wait failed: {}", e))?;

    // Clean up FIFO on exit
    let _ = fs::remove_file(&pipe_path);

    if !status.success() {
        return Err(miette::miette!(
            "Adapter exited with code {:?}",
            status.code()
        ));
    }

    Ok(())
}

// ── Writeback ───────────────────────────────────────────────────────────────

fn cmd_writeback(config: &OptimizeConfig, ast: &SourceFile) -> Result<()> {
    let out_dir = config.output.clone().unwrap_or_else(|| {
        let stem = Path::new(&config.file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("skill");
        format!("{}.optimized", stem)
    });

    let best_path = format!("{}/best_skill.md", out_dir);
    let initial_path = format!("{}/initial_skill.md", out_dir);

    if !Path::new(&best_path).exists() {
        return Err(miette::miette!(
            "No best_skill.md found at {}. Run optimization first.",
            best_path
        ));
    }

    let best_text =
        fs::read_to_string(&best_path).map_err(|e| miette::miette!("read {}: {}", best_path, e))?;
    let initial_text = fs::read_to_string(&initial_path)
        .map_err(|e| miette::miette!("read {}: {}", initial_path, e))?;

    if best_text.trim() == initial_text.trim() {
        eprintln!("✓ No changes — best_skill.md is identical to initial_skill.md");
        return Ok(());
    }

    let mut mutated_ast = ast.clone();

    let best_sections = extract_skillmd_sections(&best_text);
    let initial_sections = extract_skillmd_sections(&initial_text);

    for skill in &mut mutated_ast.skills {
        let compiler = SkillMdCompiler::new();
        let compiled = compiler.compile(skill, ast);
        if compiled.trim() != initial_text.trim() {
            continue;
        }

        eprintln!("Applying writeback to skill '{}'...", skill.name);

        let best_preamble = best_sections
            .iter()
            .find(|s| s.step_name.is_none())
            .map(|s| &s.lines);
        let initial_preamble = initial_sections
            .iter()
            .find(|s| s.step_name.is_none())
            .map(|s| &s.lines);

        // Apply preamble context changes
        if let (Some(best_lines), Some(_initial_lines)) = (best_preamble, initial_preamble) {
            let best_contexts = parse_contexts_from_lines(best_lines);
            let prev_count = skill.body.contexts.len();
            apply_context_mutations(&best_contexts, &mut skill.body.contexts);
            // Add source_order entries for any new contexts
            for i in prev_count..skill.body.contexts.len() {
                skill
                    .body
                    .source_order
                    .push(crate::ast::BodyItemRef::Context(i));
            }
        }

        // Apply step context changes
        for best_section in &best_sections {
            if let Some(ref step_name) = best_section.step_name
                && let Some(step) = skill.body.steps.iter_mut().find(|s| s.name == *step_name)
            {
                let best_contexts = parse_contexts_from_lines(&best_section.lines);
                apply_context_mutations(&best_contexts, &mut step.contexts);
            }
        }

        // Apply sampling directive changes
        if let Some(sampling) = parse_sampling_from_sections(&best_sections) {
            skill.body.directives.sampling = Some(sampling);
            eprintln!("  ✓ updated sampling directive");
        }

        // Apply persona changes
        if let Some(persona) = parse_persona_from_sections(&best_sections)
            && skill.body.directives.persona.as_deref() != Some(&persona)
        {
            skill.body.directives.persona = Some(persona);
            eprintln!("  ✓ updated persona");
        }
    }

    // Validate the mutated AST
    let base_dir = std::path::Path::new(&config.file)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let mut checker = Checker::with_base_dir(base_dir);
    if let Err(errors) = checker.check(&mutated_ast) {
        for err in &errors {
            eprintln!("writeback validation error: {}", err);
        }
        return Err(miette::miette!(
            "Writeback produced {} validation error(s). The .agent source was NOT modified.",
            errors.len()
        ));
    }

    let formatted = Formatter::format(&mutated_ast);

    let out_path = if config.no_overwrite {
        config.file.replace(".agent", ".agent.optimized")
    } else {
        config.file.clone()
    };

    fs::write(&out_path, &formatted).map_err(|e| miette::miette!("write {}: {}", out_path, e))?;

    eprintln!("✓ Writeback applied → {}", out_path);
    Ok(())
}

// ── Section parser ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SkillMdSection {
    pub header: String,
    pub step_name: Option<String>,
    pub lines: Vec<String>,
}

pub fn extract_skillmd_sections(text: &str) -> Vec<SkillMdSection> {
    let mut sections = Vec::new();
    let mut current_step: Option<String> = None;
    let mut preamble_lines: Vec<String> = Vec::new();
    let mut step_lines: Vec<String> = Vec::new();

    let mut past_frontmatter = false;
    let mut in_frontmatter = false;
    let mut in_structural = false;

    let list_structural = [
        "## Output",
        "## Preconditions",
        "## Postconditions",
        "## Tools",
        "## Permissions",
    ];
    let block_structural = ["## Tests", "## Observability"];

    for line in text.lines() {
        if line == "---" && !past_frontmatter {
            in_frontmatter = !in_frontmatter;
            if !in_frontmatter {
                past_frontmatter = true;
            }
            continue;
        }
        if in_frontmatter {
            continue;
        }

        if line.starts_with("# ") && !line.starts_with("## ") {
            continue;
        }

        if line.starts_with("## ") {
            // Block-structural sections skip everything until the next ## header
            if block_structural.iter().any(|h| line.starts_with(h)) {
                in_structural = true;
                continue;
            }
            // List-structural sections only skip their list items (- **field**: type)
            if list_structural.iter().any(|h| line.starts_with(h)) {
                in_structural = false; // Will be handled per-line below
                continue;
            }

            in_structural = false;

            if line.starts_with("## Step: ") {
                // Save previous step
                if let Some(ref name) = current_step
                    && !step_lines.is_empty()
                {
                    sections.push(SkillMdSection {
                        header: format!("## Step: {}", name),
                        step_name: Some(name.clone()),
                        lines: std::mem::take(&mut step_lines),
                    });
                }
                current_step = Some(line.trim_start_matches("## Step: ").trim().to_string());
                step_lines.clear();
            }
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if in_structural {
            continue;
        }

        // Skip structural list items (## Output fields, etc.)
        if trimmed.starts_with("- **") && trimmed.contains("**:") {
            continue;
        }
        // Skip ### sub-headings (test names inside ## Tests that leaked)
        if trimmed.starts_with("### ") {
            continue;
        }

        if current_step.is_some() {
            step_lines.push(line.to_string());
        } else {
            preamble_lines.push(line.to_string());
        }
    }

    // Save final step
    if let Some(ref name) = current_step
        && !step_lines.is_empty()
    {
        sections.push(SkillMdSection {
            header: format!("## Step: {}", name),
            step_name: Some(name.clone()),
            lines: step_lines,
        });
    }

    // Preamble first
    if !preamble_lines.is_empty() {
        sections.insert(
            0,
            SkillMdSection {
                header: "<preamble>".to_string(),
                step_name: None,
                lines: preamble_lines,
            },
        );
    }

    sections
}

// ── Context parser (from SKILL.md lines) ────────────────────────────────────

#[derive(Debug, Clone)]
struct ParsedContext {
    text: String,
    priority: Option<Priority>,
}

fn parse_contexts_from_lines(lines: &[String]) -> Vec<ParsedContext> {
    let mut contexts = Vec::new();
    let mut current_text = String::new();
    let mut current_priority: Option<Priority> = None;

    let skip_prefixes = [
        "*Produces final output.*",
        "*Uses:",
        "*Loads reference:",
        "*Decay:",
        "*Active until step",
        "**Condition:**",
        "**Reasoning mode:**",
        "**Sampling:**",
        "**Format:**",
    ];

    for line in lines {
        let trimmed = line.trim();

        if skip_prefixes.iter().any(|p| trimmed.starts_with(p)) {
            continue;
        }

        // Persona lines (blockquote not starting with priority markers)
        if trimmed.starts_with("> ")
            && !trimmed.starts_with("> **CRITICAL:**")
            && !trimmed.starts_with("> **IMPORTANT:**")
        {
            continue;
        }

        // Detect priority markers
        if trimmed.starts_with("> **CRITICAL:** ") {
            if !current_text.trim().is_empty() {
                contexts.push(ParsedContext {
                    text: current_text.trim().to_string(),
                    priority: current_priority,
                });
            }
            current_text = trimmed.trim_start_matches("> **CRITICAL:** ").to_string();
            current_priority = Some(Priority::Critical);
            continue;
        }
        if trimmed.starts_with("> **IMPORTANT:** ") {
            if !current_text.trim().is_empty() {
                contexts.push(ParsedContext {
                    text: current_text.trim().to_string(),
                    priority: current_priority,
                });
            }
            current_text = trimmed.trim_start_matches("> **IMPORTANT:** ").to_string();
            current_priority = Some(Priority::Important);
            continue;
        }
        if trimmed.starts_with("*Optional context:* ") {
            if !current_text.trim().is_empty() {
                contexts.push(ParsedContext {
                    text: current_text.trim().to_string(),
                    priority: current_priority,
                });
            }
            current_text = trimmed
                .trim_start_matches("*Optional context:* ")
                .to_string();
            current_priority = Some(Priority::Optional);
            continue;
        }

        // Regular text — could be continuation or new supplementary context
        if trimmed.is_empty() {
            if !current_text.trim().is_empty() {
                contexts.push(ParsedContext {
                    text: current_text.trim().to_string(),
                    priority: current_priority,
                });
                current_text.clear();
                current_priority = None;
            }
        } else {
            if !current_text.is_empty() {
                current_text.push('\n');
            }
            current_text.push_str(trimmed);
        }
    }

    if !current_text.trim().is_empty() {
        contexts.push(ParsedContext {
            text: current_text.trim().to_string(),
            priority: current_priority,
        });
    }

    contexts
}

// ── Context matching ────────────────────────────────────────────────────────

fn longest_common_substring_len(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();
    if m == 0 || n == 0 {
        return 0;
    }
    let mut prev = vec![0usize; n + 1];
    let mut curr = vec![0usize; n + 1];
    let mut max_len = 0;

    for i in 1..=m {
        for j in 1..=n {
            if a_bytes[i - 1] == b_bytes[j - 1] {
                curr[j] = prev[j - 1] + 1;
                if curr[j] > max_len {
                    max_len = curr[j];
                }
            } else {
                curr[j] = 0;
            }
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.iter_mut().for_each(|v| *v = 0);
    }

    max_len
}

fn similarity_score(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }

    // Word-level overlap (handles expansions/rewrites that preserve key words)
    let words_a: std::collections::HashSet<&str> =
        a.split_whitespace().filter(|w| w.len() > 2).collect();
    let words_b: std::collections::HashSet<&str> =
        b.split_whitespace().filter(|w| w.len() > 2).collect();
    let word_overlap = if !words_a.is_empty() && !words_b.is_empty() {
        let intersection = words_a.intersection(&words_b).count();
        let smaller = words_a.len().min(words_b.len());
        intersection as f64 / smaller as f64
    } else {
        0.0
    };

    // Character-level LCS
    let lcs = longest_common_substring_len(a, b);
    let char_score = lcs as f64 / max_len as f64;

    // Take the better of the two signals
    char_score.max(word_overlap)
}

fn find_best_context_match(text: &str, candidates: &[ContextBlock]) -> Option<(usize, f64)> {
    let mut best_idx = None;
    let mut best_score = 0.0f64;

    for (i, candidate) in candidates.iter().enumerate() {
        let score = similarity_score(text, candidate.text.trim());
        if score > best_score {
            best_score = score;
            best_idx = Some(i);
        }
    }

    if best_score >= 0.3 {
        best_idx.map(|i| (i, best_score))
    } else {
        None
    }
}

// ── Mutation application ────────────────────────────────────────────────────

fn apply_context_mutations(optimized: &[ParsedContext], source_contexts: &mut Vec<ContextBlock>) {
    use crate::token::Span;

    let mut matched = vec![false; source_contexts.len()];

    for opt_ctx in optimized {
        if let Some((idx, score)) = find_best_context_match(&opt_ctx.text, source_contexts) {
            matched[idx] = true;
            let source = &mut source_contexts[idx];

            if source.text.trim() != opt_ctx.text {
                eprintln!(
                    "  ✓ updated context (similarity {:.0}%): {:?}...",
                    score * 100.0,
                    &opt_ctx.text.chars().take(50).collect::<String>()
                );
                source.text = opt_ctx.text.clone();
            }

            if opt_ctx.priority.is_some() && opt_ctx.priority != source.priority {
                eprintln!(
                    "  ✓ priority changed: {:?} → {:?}",
                    source.priority.map(|p| p.label()),
                    opt_ctx.priority.map(|p| p.label())
                );
                source.priority = opt_ctx.priority;
            }
        } else {
            eprintln!(
                "  + new context from optimizer: {:?}...",
                &opt_ctx.text.chars().take(50).collect::<String>()
            );
            let new_idx = source_contexts.len();
            source_contexts.push(ContextBlock {
                priority: opt_ctx.priority.or(Some(Priority::Supplementary)),
                when: None,
                decay: None,
                until: None,
                text: opt_ctx.text.clone(),
                span: Span {
                    start: 0,
                    end: 0,
                    line: 0,
                    col: 0,
                },
            });
            // Mark for source_order fixup (caller handles this)
            let _ = new_idx;
        }
    }
}

// ── Directive parsers ───────────────────────────────────────────────────────

fn parse_sampling_from_sections(sections: &[SkillMdSection]) -> Option<SamplingDirective> {
    for section in sections {
        for line in &section.lines {
            if line.contains("**Sampling:**") {
                let mut temp = None;
                let mut top_p = None;
                if let Some(t_start) = line.find("temperature=") {
                    let rest = &line[t_start + 12..];
                    let val_str: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '.')
                        .collect();
                    temp = val_str.parse::<f64>().ok();
                }
                if let Some(p_start) = line.find("top_p=") {
                    let rest = &line[p_start + 6..];
                    let val_str: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '.')
                        .collect();
                    top_p = val_str.parse::<f64>().ok();
                }
                if temp.is_some() || top_p.is_some() {
                    return Some(SamplingDirective {
                        temperature: temp,
                        top_p,
                    });
                }
            }
        }
    }
    None
}

fn parse_persona_from_sections(sections: &[SkillMdSection]) -> Option<String> {
    for section in sections {
        let persona_lines: Vec<&str> = section
            .lines
            .iter()
            .filter(|l| l.trim().starts_with("> ") && !l.trim().starts_with("> **"))
            .map(|l| l.trim().trim_start_matches("> "))
            .collect();

        if !persona_lines.is_empty() {
            return Some(persona_lines.join("\n"));
        }
    }
    None
}

// ── Config generation ───────────────────────────────────────────────────────

fn generate_config(
    config: &OptimizeConfig,
    out_dir: &str,
    skill_name: &str,
    train_size: usize,
) -> String {
    format!(
        r#"# SkillOpt config for {skill_name}
# Generated by: skillspec optimize --prepare
# All LLM calls route through the hosting agent — no API keys needed.

model:
  backend: agent_proxy
  optimizer_backend: agent_proxy
  target_backend: agent_proxy
  optimizer: agent-proxy
  target: agent-proxy
  reasoning_effort: medium

train:
  num_epochs: {epochs}
  batch_size: {batch_size}
  accumulation: 1
  train_size: {train_size}
  seed: 42

gradient:
  minibatch_size: {batch_size}
  merge_batch_size: {batch_size}
  analyst_workers: 1
  max_analyst_rounds: 3
  failure_only: false

optimizer:
  learning_rate: {edit_budget}
  scheduler: {scheduler}
  skill_update_mode: patch
  validation_patience: 2

evaluation:
  sel_env_num: 0
  test_env_num: 0
  eval_test: false

slow_update:
  enabled: false

meta_skill:
  enabled: false

env:
  name: skillspec
  out_root: {out_dir}
  skill_name: {skill_name}
  skill_init: {out_dir}/initial_skill.md
  split_mode: split_dir
  split_dir: {out_dir}/splits
  max_turns: 1
  workers: 1
  limit: 0
"#,
        skill_name = skill_name,
        out_dir = out_dir,
        epochs = config.epochs,
        batch_size = config.batch_size,
        edit_budget = config.edit_budget,
        scheduler = config.scheduler,
        train_size = train_size,
    )
}

// ── Adapter file scaffolding ────────────────────────────────────────────────

fn write_adapter_files(opt_dir: &str) -> Result<()> {
    let adapter_path = format!("{}/skillspec_adapter.py", opt_dir);
    if !Path::new(&adapter_path).exists() {
        fs::write(&adapter_path, ADAPTER_PY)
            .map_err(|e| miette::miette!("write adapter: {}", e))?;
        eprintln!("  ✓ adapter → {}", adapter_path);
    }

    let proxy_path = format!("{}/agent_proxy.py", opt_dir);
    if !Path::new(&proxy_path).exists() {
        fs::write(&proxy_path, AGENT_PROXY_PY)
            .map_err(|e| miette::miette!("write proxy: {}", e))?;
        eprintln!("  ✓ proxy backend → {}", proxy_path);
    }

    let req_path = format!("{}/requirements.txt", opt_dir);
    if !Path::new(&req_path).exists() {
        fs::write(
            &req_path,
            "# SkillOpt is installed from local clone via setup\npyyaml>=6.0\n",
        )
        .map_err(|e| miette::miette!("write requirements: {}", e))?;
    }

    Ok(())
}

fn write_default_config(opt_dir: &str) -> Result<()> {
    let config_dir = format!("{}/configs", opt_dir);
    fs::create_dir_all(&config_dir).map_err(|e| miette::miette!("mkdir: {}", e))?;

    let default_config = format!("{}/default.yaml", config_dir);
    if !Path::new(&default_config).exists() {
        fs::write(&default_config, DEFAULT_CONFIG_YAML)
            .map_err(|e| miette::miette!("write config: {}", e))?;
        eprintln!("  ✓ default config → {}", default_config);
    }

    Ok(())
}

// ── Embedded Python sources ─────────────────────────────────────────────────

const ADAPTER_PY: &str = r##"#!/usr/bin/env python3
"""SkillOpt EnvAdapter for skillspec — routes through AgentProxyBackend."""

import json
import sys
import os
import argparse
from pathlib import Path

# Add SkillOpt to path
sys.path.insert(0, str(Path(__file__).parent / "skillopt"))

from agent_proxy import AgentProxyBackend

try:
    from skillopt.envs.base import EnvAdapter
    from skillopt.types import RolloutResult, RawPatch
except ImportError:
    print(json.dumps({"type": "error", "message": "SkillOpt not installed. Run: skillspec optimize --setup"}))
    sys.exit(1)


class SkillSpecAdapter(EnvAdapter):
    """Adapter that bridges SkillOpt's training loop with skillspec's test harness."""

    def __init__(self, config):
        self.config = config
        self.skill_name = config.get("skill_name", "unknown")
        self.split_dir = Path(config.get("split_dir", "."))
        self.proxy = AgentProxyBackend()

    def build_train_env(self, batch_size, seed):
        items_path = self.split_dir / "train" / "items.json"
        with open(items_path) as f:
            items = json.load(f)
        return SkillSpecEnvManager(items, batch_size, seed)

    def build_eval_env(self, env_num, split, seed):
        split_name = split if split else "val"
        items_path = self.split_dir / split_name / "items.json"
        with open(items_path) as f:
            items = json.load(f)
        return SkillSpecEnvManager(items, env_num, seed)

    def rollout(self, env_manager, skill_content, out_dir):
        """Execute each test item using the current skill, scored by the hosting agent."""
        results = []
        for item in env_manager.get_batch():
            # Build messages: skill as system prompt, test input as user message
            messages = [
                {"role": "system", "content": skill_content},
                {"role": "user", "content": json.dumps(item["input"])},
            ]

            response = self.proxy.chat(
                messages=messages,
                role="target",
                phase="rollout",
            )

            # Score: the agent evaluates the response against expected assertions
            score_messages = [
                {"role": "system", "content": "You are evaluating a skill's output against test assertions. Return JSON with 'hard' (0 or 1) and 'soft' (0.0-1.0) scores, plus 'fail_reason' if hard=0."},
                {"role": "user", "content": json.dumps({
                    "response": response,
                    "expected": item["expected_output"],
                    "test_id": item["id"],
                })},
            ]
            score_response = self.proxy.chat(
                messages=score_messages,
                role="optimizer",
                phase="evaluate",
            )

            try:
                scores = json.loads(score_response)
            except json.JSONDecodeError:
                scores = {"hard": 0, "soft": 0.0, "fail_reason": "Failed to parse score response"}

            results.append({
                "id": item["id"],
                "hard": scores.get("hard", 0),
                "soft": scores.get("soft", 0.0),
                "n_turns": 1,
                "fail_reason": scores.get("fail_reason", ""),
                "trace": [
                    {"role": "user", "content": json.dumps(item["input"])},
                    {"role": "assistant", "content": response},
                ],
            })

        return results

    def reflect(self, results, skill_content, out_dir):
        """Analyse trajectories and propose skill edits — handled by SkillOpt's standard reflection."""
        return None  # Let SkillOpt's built-in reflection handle this via the proxy

    def get_task_types(self):
        return [self.skill_name]


class SkillSpecEnvManager:
    """Minimal environment manager that serves test items as batches."""

    def __init__(self, items, batch_size, seed):
        self.items = items
        self.batch_size = batch_size
        self.seed = seed
        self._offset = 0

    def get_batch(self):
        batch = self.items[self._offset:self._offset + self.batch_size]
        self._offset += self.batch_size
        return batch

    def reset(self):
        self._offset = 0


def main():
    parser = argparse.ArgumentParser(description="SkillSpec SkillOpt adapter")
    parser.add_argument("--config", required=True, help="Path to config YAML")
    parser.add_argument("--output-dir", required=True, help="Output directory")
    parser.add_argument("--proxy-mode", action="store_true", help="Use agent proxy backend")
    parser.add_argument("--resume", help="Resume from state file")
    parser.add_argument("--response", help="JSON response to last LLM request")
    args = parser.parse_args()

    import yaml
    with open(args.config) as f:
        config = yaml.safe_load(f)

    adapter = SkillSpecAdapter(config)

    # If resuming with a response, feed it to the proxy backend
    if args.response:
        adapter.proxy.feed_response(args.response)

    # Emit status
    status = {"type": "status", "phase": "init", "message": f"Starting optimization for {config.get('skill_name', 'unknown')}"}
    print(json.dumps(status), flush=True)

    try:
        from skillopt.engine.trainer import Trainer
        from skillopt.config import load_config

        trainer_config = load_config(args.config, {})
        trainer = Trainer(trainer_config, adapter)

        if args.resume and Path(args.resume).exists():
            trainer.resume(args.resume)

        trainer.train()

        # Read best skill
        best_skill_path = Path(args.output_dir) / "best_skill.md"
        if best_skill_path.exists():
            best_skill = best_skill_path.read_text()
        else:
            best_skill = ""

        result = {
            "type": "complete",
            "best_skill": best_skill,
            "score": getattr(trainer, "best_score", None),
            "steps_run": getattr(trainer, "total_steps", None),
        }
        print(json.dumps(result), flush=True)

    except SystemExit as e:
        if e.code == 42:
            # Checkpoint exit — LLM request was emitted
            sys.exit(42)
        raise


if __name__ == "__main__":
    main()
"##;

const AGENT_PROXY_PY: &str = r##"#!/usr/bin/env python3
"""Agent Proxy Backend — routes SkillOpt LLM calls through the hosting agent via stdio."""

import json
import sys
import uuid
from pathlib import Path


class AgentProxyBackend:
    """Custom model backend that emits LLM requests to stdout and reads responses from stdin.

    When the hosting agent runs `skillspec optimize --step`, the Rust CLI starts this Python
    process. Every LLM call gets serialized as a JSON request on stdout. The process then
    checkpoints and exits with code 42. The agent processes the request, then re-invokes
    with --resume and --response to continue.
    """

    def __init__(self):
        self._pending_response = None
        self._state_dir = None

    def feed_response(self, response_json):
        """Pre-load a response for the next chat() call (used when resuming)."""
        if isinstance(response_json, str):
            try:
                parsed = json.loads(response_json)
                self._pending_response = parsed.get("content", response_json)
            except json.JSONDecodeError:
                self._pending_response = response_json
        else:
            self._pending_response = response_json

    def chat(self, messages, role="optimizer", phase="unknown", temperature=None, max_tokens=None):
        """Route an LLM call through the hosting agent.

        Emits a JSON request, saves state, and exits. On resume, returns the fed response.
        """
        # If we have a pre-loaded response, return it immediately
        if self._pending_response is not None:
            response = self._pending_response
            self._pending_response = None
            return response

        # Emit the request for the agent to process
        request = {
            "type": "llm_request",
            "id": str(uuid.uuid4())[:8],
            "role": role,
            "phase": phase,
            "messages": messages,
        }
        if temperature is not None:
            request["temperature"] = temperature
        if max_tokens is not None:
            request["max_tokens"] = max_tokens

        print(json.dumps(request), flush=True)

        # Checkpoint: exit with code 42 to signal "waiting for response"
        sys.exit(42)

    def chat_optimizer(self, messages, **kwargs):
        """SkillOpt optimizer model interface."""
        return self.chat(messages, role="optimizer", **kwargs)

    def chat_target(self, messages, **kwargs):
        """SkillOpt target model interface."""
        return self.chat(messages, role="target", **kwargs)
"##;

const DEFAULT_CONFIG_YAML: &str = r#"# Default SkillOpt configuration for skillspec
# Override per-skill with: skillspec optimize <file> --epochs 5 --batch-size 8

_base_: null

epochs: 3
batch_size: 4
edit_budget: 5
scheduler: constant

model:
  backend: agent_proxy

validation:
  gate: true
  patience: 2

slow_update:
  enabled: true

meta_skill:
  enabled: false
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_message_llm_request_roundtrip() {
        let msg = ProxyMessage::LlmRequest {
            id: "req-001".to_string(),
            role: "optimizer".to_string(),
            phase: "reflect".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "Analyse this.".to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: "trace data".to_string(),
                },
            ],
            temperature: Some(0.7),
            max_tokens: Some(4096),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ProxyMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ProxyMessage::LlmRequest {
                id,
                role,
                phase,
                messages,
                ..
            } => {
                assert_eq!(id, "req-001");
                assert_eq!(role, "optimizer");
                assert_eq!(phase, "reflect");
                assert_eq!(messages.len(), 2);
            }
            _ => panic!("Expected LlmRequest"),
        }
    }

    #[test]
    fn proxy_message_status_roundtrip() {
        let msg = ProxyMessage::Status {
            phase: "step".to_string(),
            step: Some(3),
            epoch: Some(1),
            score: Some(0.72),
            message: Some("Score improved 0.65 → 0.72".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ProxyMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ProxyMessage::Status {
                phase, step, score, ..
            } => {
                assert_eq!(phase, "step");
                assert_eq!(step, Some(3));
                assert!((score.unwrap() - 0.72).abs() < f64::EPSILON);
            }
            _ => panic!("Expected Status"),
        }
    }

    #[test]
    fn proxy_message_complete_roundtrip() {
        let msg = ProxyMessage::Complete {
            best_skill: "# Optimized skill\nDo the thing well.".to_string(),
            score: Some(0.95),
            steps_run: Some(12),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ProxyMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ProxyMessage::Complete {
                best_skill,
                score,
                steps_run,
            } => {
                assert!(best_skill.contains("Optimized skill"));
                assert!((score.unwrap() - 0.95).abs() < f64::EPSILON);
                assert_eq!(steps_run, Some(12));
            }
            _ => panic!("Expected Complete"),
        }
    }

    #[test]
    fn proxy_message_error_roundtrip() {
        let msg = ProxyMessage::Error {
            message: "venv not found".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ProxyMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ProxyMessage::Error { message } => {
                assert_eq!(message, "venv not found");
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn generate_config_includes_fields() {
        let config = OptimizeConfig {
            file: "test.agent".to_string(),
            setup: false,
            dry_run: false,
            prepare: false,
            step: false,
            resume: None,
            response: None,
            epochs: 5,
            batch_size: 8,
            edit_budget: 3,
            scheduler: "cosine".to_string(),
            output: None,
            writeback: false,
            no_overwrite: false,
        };
        let yaml = generate_config(&config, "out", "my-skill", 10);
        assert!(yaml.contains("num_epochs: 5"));
        assert!(yaml.contains("batch_size: 8"));
        assert!(yaml.contains("learning_rate: 3"));
        assert!(yaml.contains("scheduler: cosine"));
        assert!(yaml.contains("skill_name: my-skill"));
        assert!(yaml.contains("backend: agent_proxy"));
        assert!(yaml.contains("out_root: out"));
        assert!(yaml.contains("train_size: 10"));
    }

    // ── Section parser tests ────────────────────────────────────────────────

    #[test]
    fn extract_sections_preamble_and_steps() {
        let text = "\
---
name: greeter
---

# greeter

## Output

- **greeting**: string

> **IMPORTANT:** You generate greetings.

## Step: greet

*Produces final output.*

Answer warmly.
";
        let sections = extract_skillmd_sections(text);

        let preamble = sections.iter().find(|s| s.step_name.is_none());
        assert!(
            preamble.is_some(),
            "expected preamble section, got: {:?}",
            sections
        );
        assert!(
            preamble
                .unwrap()
                .lines
                .iter()
                .any(|l| l.contains("IMPORTANT"))
        );

        let step = sections
            .iter()
            .find(|s| s.step_name.as_deref() == Some("greet"));
        assert!(step.is_some(), "expected step 'greet' section");
        assert!(step.unwrap().lines.iter().any(|l| l.contains("warmly")));
    }

    #[test]
    fn extract_sections_empty_input() {
        let sections = extract_skillmd_sections("");
        assert!(sections.is_empty());
    }

    // ── Context parser tests ────────────────────────────────────────────────

    #[test]
    fn parse_contexts_detects_priority_markers() {
        let lines = vec![
            "> **CRITICAL:** Never forget the user's name.".to_string(),
            "".to_string(),
            "> **IMPORTANT:** Use formal language.".to_string(),
            "".to_string(),
            "Be concise.".to_string(),
            "".to_string(),
            "*Optional context:* Add emoji if requested.".to_string(),
        ];
        let contexts = parse_contexts_from_lines(&lines);
        assert_eq!(contexts.len(), 4);
        assert_eq!(contexts[0].priority, Some(Priority::Critical));
        assert!(contexts[0].text.contains("Never forget"));
        assert_eq!(contexts[1].priority, Some(Priority::Important));
        assert_eq!(contexts[2].priority, None);
        assert_eq!(contexts[3].priority, Some(Priority::Optional));
    }

    // ── Similarity tests ────────────────────────────────────────────────────

    #[test]
    fn similarity_exact_match() {
        assert!((similarity_score("hello world", "hello world") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_partial_match() {
        let score = similarity_score(
            "You generate greetings for users.",
            "You generate warm greetings for users.",
        );
        assert!(score > 0.5);
    }

    #[test]
    fn similarity_no_match() {
        let score = similarity_score("Completely different text", "Nothing in common here xyz");
        assert!(score < 0.3);
    }

    // ── Context matching tests ──────────────────────────────────────────────

    #[test]
    fn find_match_exact() {
        use crate::token::Span;
        let contexts = vec![ContextBlock {
            priority: Some(Priority::Critical),
            when: None,
            decay: None,
            until: None,
            text: "You generate greetings.".to_string(),
            span: Span {
                start: 0,
                end: 0,
                line: 0,
                col: 0,
            },
        }];
        let result = find_best_context_match("You generate greetings.", &contexts);
        assert!(result.is_some());
        let (idx, score) = result.unwrap();
        assert_eq!(idx, 0);
        assert!((score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn find_match_edited() {
        use crate::token::Span;
        let contexts = vec![
            ContextBlock {
                priority: Some(Priority::Important),
                when: None,
                decay: None,
                until: None,
                text: "Generate a greeting for the given name.".to_string(),
                span: Span {
                    start: 0,
                    end: 0,
                    line: 0,
                    col: 0,
                },
            },
            ContextBlock {
                priority: Some(Priority::Critical),
                when: None,
                decay: None,
                until: None,
                text: "You are a greeting specialist.".to_string(),
                span: Span {
                    start: 0,
                    end: 0,
                    line: 0,
                    col: 0,
                },
            },
        ];
        let result = find_best_context_match(
            "Generate a warm, personalized greeting for the given name.",
            &contexts,
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 0);
    }

    #[test]
    fn find_match_no_match_returns_none() {
        use crate::token::Span;
        let contexts = vec![ContextBlock {
            priority: None,
            when: None,
            decay: None,
            until: None,
            text: "Short.".to_string(),
            span: Span {
                start: 0,
                end: 0,
                line: 0,
                col: 0,
            },
        }];
        let result = find_best_context_match(
            "This is a completely different and much longer text about something else entirely.",
            &contexts,
        );
        assert!(result.is_none());
    }

    // ── Mutation tests ──────────────────────────────────────────────────────

    #[test]
    fn apply_mutations_updates_text() {
        use crate::token::Span;
        let mut contexts = vec![ContextBlock {
            priority: Some(Priority::Critical),
            when: None,
            decay: None,
            until: None,
            text: "You generate greetings.".to_string(),
            span: Span {
                start: 0,
                end: 0,
                line: 0,
                col: 0,
            },
        }];
        let optimized = vec![ParsedContext {
            text: "You generate warm, personalized greetings.".to_string(),
            priority: Some(Priority::Critical),
        }];
        apply_context_mutations(&optimized, &mut contexts);
        assert_eq!(
            contexts[0].text,
            "You generate warm, personalized greetings."
        );
    }

    #[test]
    fn apply_mutations_inserts_new_context() {
        use crate::token::Span;
        let mut contexts = vec![ContextBlock {
            priority: Some(Priority::Critical),
            when: None,
            decay: None,
            until: None,
            text: "You generate greetings.".to_string(),
            span: Span {
                start: 0,
                end: 0,
                line: 0,
                col: 0,
            },
        }];
        let optimized = vec![
            ParsedContext {
                text: "You generate greetings.".to_string(),
                priority: Some(Priority::Critical),
            },
            ParsedContext {
                text: "Always include the person's name in the greeting.".to_string(),
                priority: Some(Priority::Important),
            },
        ];
        apply_context_mutations(&optimized, &mut contexts);
        assert_eq!(contexts.len(), 2);
        assert!(contexts[1].text.contains("person's name"));
        assert_eq!(contexts[1].priority, Some(Priority::Important));
    }

    #[test]
    fn apply_mutations_changes_priority() {
        use crate::token::Span;
        let mut contexts = vec![ContextBlock {
            priority: Some(Priority::Supplementary),
            when: None,
            decay: None,
            until: None,
            text: "Be concise in your greetings.".to_string(),
            span: Span {
                start: 0,
                end: 0,
                line: 0,
                col: 0,
            },
        }];
        let optimized = vec![ParsedContext {
            text: "Be concise in your greetings.".to_string(),
            priority: Some(Priority::Critical),
        }];
        apply_context_mutations(&optimized, &mut contexts);
        assert_eq!(contexts[0].priority, Some(Priority::Critical));
    }

    // ── Directive parser tests ──────────────────────────────────────────────

    #[test]
    fn parse_sampling_extracts_values() {
        let sections = vec![SkillMdSection {
            header: "<preamble>".to_string(),
            step_name: None,
            lines: vec!["**Sampling:** temperature=0.3, top_p=0.9".to_string()],
        }];
        let sampling = parse_sampling_from_sections(&sections).unwrap();
        assert!((sampling.temperature.unwrap() - 0.3).abs() < f64::EPSILON);
        assert!((sampling.top_p.unwrap() - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_persona_extracts_blockquote() {
        let sections = vec![SkillMdSection {
            header: "<preamble>".to_string(),
            step_name: None,
            lines: vec![
                "> You are a warm greeting specialist.".to_string(),
                "> You take language and register seriously.".to_string(),
                "> **CRITICAL:** Always greet.".to_string(),
            ],
        }];
        let persona = parse_persona_from_sections(&sections).unwrap();
        assert!(persona.contains("warm greeting specialist"));
        assert!(persona.contains("register seriously"));
        assert!(!persona.contains("CRITICAL"));
    }

    #[test]
    fn no_changes_produces_no_mutations() {
        let text = "Same text.";
        let sections = extract_skillmd_sections(text);
        assert!(
            sections.is_empty()
                || sections
                    .iter()
                    .all(|s| s.lines.is_empty() || s.lines.iter().all(|l| l.trim() == text.trim()))
        );
    }
}
