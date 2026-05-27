use std::path::{Path, PathBuf};

/// Mechanical SKILL.md to .agent.partial migration.
///
/// This does NOT invoke any LLM. It performs mechanical extraction:
/// parse YAML frontmatter for name, description, parameters;
/// split markdown by ## headers into candidate steps;
/// detect conditional patterns (if/when language);
/// output a `.agent.partial` file with TODO markers.
pub struct MigrateResult {
    pub output: String,
    pub source_path: String,
}

const TRUNCATION_THRESHOLD: usize = 500;
const TRUNCATION_PREVIEW_LINES: usize = 50;

pub struct CollectedFile {
    pub relative_path: String,
    pub content: String,
    pub truncated: bool,
    pub line_count: usize,
}

#[derive(Debug)]
pub struct MigrateDirectoryResult {
    pub output: String,
    pub source_dir: String,
    pub warnings: Vec<String>,
    pub files_found: usize,
    pub files_truncated: usize,
}

pub fn migrate_skillmd(source: &str, source_path: &str) -> MigrateResult {
    let mut output = String::new();

    // Parse frontmatter
    let (frontmatter, body, raw_fm) = parse_frontmatter(source);
    let name = frontmatter
        .get("name")
        .cloned()
        .unwrap_or_else(|| "unnamed".to_string());
    let description = frontmatter.get("description").cloned();
    let parameters = parse_parameters_from_yaml(&raw_fm);

    // Header comment
    output.push_str(&format!("// Auto-migrated from: {}\n", source_path));
    output.push_str("// TODO: Review and complete this migration\n\n");

    // Skill block
    output.push_str(&format!("skill \"{}\" {{\n", name));

    // Input from parameters
    if !parameters.is_empty() {
        output.push_str("  input {\n");
        for param in &parameters {
            output.push_str("    // TODO: Infer types from parameter descriptions\n");
            let opt = if param.optional { "?" } else { "" };
            let ty = &param.param_type;
            if let Some(default) = &param.default {
                output.push_str(&format!(
                    "    {}{}: {} = \"{}\"\n",
                    param.name, opt, ty, default
                ));
            } else {
                output.push_str(&format!("    {}{}: {}\n", param.name, opt, ty));
            }
        }
        output.push_str("  }\n\n");
    }

    // Body
    output.push_str("  body {\n");

    // If there's a description, use it as the top-level context
    if let Some(desc) = &description {
        output.push_str("    context {\n");
        output.push_str("      \"\"\"\n");
        output.push_str(&format!("      {}\n", desc.trim()));
        output.push_str("      \"\"\"\n");
        output.push_str("    }\n\n");
    }

    // Split body by ## headings
    let sections = split_by_headings(&body);

    if sections.is_empty() && !body.trim().is_empty() {
        // No headings — emit the whole body as a context block
        output.push_str("    context {\n");
        output.push_str("      \"\"\"\n");
        for line in body.trim().lines() {
            output.push_str(&format!("      {}\n", line));
        }
        output.push_str("      \"\"\"\n");
        output.push_str("    }\n");
    } else {
        output.push_str("    // TODO: Determine step dependencies\n");
        for (i, section) in sections.iter().enumerate() {
            let step_name = sanitize_step_name(&section.heading);
            let has_conditional = detect_conditional(&section.content);

            output.push_str(&format!("    step {} {{\n", step_name));

            if has_conditional {
                output.push_str("      // TODO: Extract conditional logic into `when` clause\n");
            }

            output.push_str("      context {\n");
            output.push_str("        \"\"\"\n");
            for line in section.content.trim().lines() {
                output.push_str(&format!("        {}\n", line));
            }
            output.push_str("        \"\"\"\n");
            output.push_str("      }\n");

            // If this is the last step, suggest emit
            if i == sections.len() - 1 {
                output.push_str(
                    "      // TODO: Add `emit output` if this step produces the final result\n",
                );
            }

            output.push_str("    }\n");
        }
    }

    output.push_str("  }\n");
    output.push_str("}\n");

    MigrateResult {
        output,
        source_path: source_path.to_string(),
    }
}

// ── Frontmatter parsing ──────────────────────────────────────────────

pub fn parse_frontmatter_pub(
    source: &str,
) -> (std::collections::HashMap<String, String>, String, String) {
    parse_frontmatter(source)
}

fn parse_frontmatter(source: &str) -> (std::collections::HashMap<String, String>, String, String) {
    let mut map = std::collections::HashMap::new();

    if !source.trim_start().starts_with("---") {
        return (map, source.to_string(), String::new());
    }

    let after_first = &source.trim_start()[3..];
    if let Some(end_idx) = after_first.find("---") {
        let fm_text = &after_first[..end_idx];
        let body = &after_first[end_idx + 3..];

        for line in fm_text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(colon_pos) = line.find(':') {
                let key = line[..colon_pos].trim().to_string();
                let value = line[colon_pos + 1..].trim().trim_matches('"').to_string();
                if !value.starts_with('-') && !key.is_empty() {
                    map.insert(key, value);
                }
            }
        }

        (map, body.to_string(), fm_text.to_string())
    } else {
        (map, source.to_string(), String::new())
    }
}

struct Parameter {
    name: String,
    param_type: String,
    optional: bool,
    default: Option<String>,
}

fn parse_parameters_from_yaml(raw_frontmatter: &str) -> Vec<Parameter> {
    let mut params = Vec::new();
    let mut in_parameters = false;
    let mut current_fields: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for line in raw_frontmatter.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("parameters:") {
            in_parameters = true;
            continue;
        }

        if !in_parameters {
            continue;
        }

        // Non-indented, non-empty line means we've left the parameters block
        if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
            break;
        }

        if trimmed.starts_with("- ") {
            // Save the previous parameter
            if let Some(p) = build_parameter(&current_fields) {
                params.push(p);
            }
            current_fields.clear();

            let rest = trimmed.strip_prefix("- ").unwrap();
            if let Some(colon) = rest.find(':') {
                let key = rest[..colon].trim().to_string();
                let value = rest[colon + 1..].trim().trim_matches('"').to_string();
                current_fields.insert(key, value);
            }
        } else if !current_fields.is_empty()
            && let Some(colon) = trimmed.find(':')
        {
            let key = trimmed[..colon].trim().to_string();
            let value = trimmed[colon + 1..].trim().trim_matches('"').to_string();
            current_fields.insert(key, value);
        }
    }

    if let Some(p) = build_parameter(&current_fields) {
        params.push(p);
    }

    params
}

fn build_parameter(map: &std::collections::HashMap<String, String>) -> Option<Parameter> {
    let name = map.get("name")?.to_string();
    let param_type = map
        .get("type")
        .cloned()
        .unwrap_or_else(|| "string".to_string());
    let optional = map.get("optional").map(|v| v == "true").unwrap_or(false);
    let default = map.get("default").cloned();
    Some(Parameter {
        name,
        param_type,
        optional,
        default,
    })
}

// ── Section splitting ────────────────────────────────────────────────

struct MarkdownSection {
    heading: String,
    content: String,
}

fn split_by_headings(body: &str) -> Vec<MarkdownSection> {
    let mut sections = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_content = String::new();

    for line in body.lines() {
        if line.starts_with("## ") {
            // Save previous section
            if let Some(heading) = current_heading.take() {
                sections.push(MarkdownSection {
                    heading,
                    content: current_content.clone(),
                });
            }
            current_heading = Some(line.strip_prefix("## ").unwrap().trim().to_string());
            current_content.clear();
        } else if current_heading.is_some() {
            current_content.push_str(line);
            current_content.push('\n');
        }
        // Skip content before first ## heading (already captured in description)
    }

    // Don't forget the last section
    if let Some(heading) = current_heading {
        sections.push(MarkdownSection {
            heading,
            content: current_content,
        });
    }

    sections
}

fn sanitize_step_name(heading: &str) -> String {
    // Remove "Step: " prefix if present
    let cleaned = heading
        .strip_prefix("Step: ")
        .or_else(|| heading.strip_prefix("Step "))
        .unwrap_or(heading);

    // Convert to snake_case identifier
    cleaned
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

// ── Directory migration ─────────────────────────────────────────────

pub fn collect_directory_files(dir: &Path) -> Vec<CollectedFile> {
    let mut files = Vec::new();
    collect_md_files_recursive(dir, dir, &mut files);
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    files
}

fn collect_md_files_recursive(base: &Path, current: &Path, files: &mut Vec<CollectedFile>) {
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_md_files_recursive(base, &path, files);
        } else if path.extension().map(|e| e == "md").unwrap_or(false) {
            let relative = path.strip_prefix(base).unwrap_or(&path);
            let relative_str = format!("./{}", relative.display());
            if let Ok(raw_content) = std::fs::read_to_string(&path) {
                let line_count = raw_content.lines().count();
                let (content, truncated) = if line_count > TRUNCATION_THRESHOLD {
                    let preview: String = raw_content
                        .lines()
                        .take(TRUNCATION_PREVIEW_LINES)
                        .collect::<Vec<_>>()
                        .join("\n");
                    (
                        format!(
                            "{}\n// ... (truncated, full file at {})",
                            preview, relative_str
                        ),
                        true,
                    )
                } else {
                    (raw_content, false)
                };
                files.push(CollectedFile {
                    relative_path: relative_str,
                    content,
                    truncated,
                    line_count,
                });
            }
        }
    }
}

pub fn migrate_directory(dir: &Path) -> Result<MigrateDirectoryResult, String> {
    let skill_md_path = dir.join("SKILL.md");

    let (main_path, main_content) = if skill_md_path.exists() {
        (
            skill_md_path.clone(),
            std::fs::read_to_string(&skill_md_path)
                .map_err(|e| format!("Failed to read SKILL.md: {}", e))?,
        )
    } else {
        find_frontmatter_file(dir)?
    };

    let main_relative = main_path
        .strip_prefix(dir)
        .unwrap_or(&main_path)
        .display()
        .to_string();

    let base_result = migrate_skillmd(&main_content, &main_relative);
    let mut output = base_result.output;

    let all_files = collect_directory_files(dir);
    let other_files: Vec<&CollectedFile> = all_files
        .iter()
        .filter(|f| {
            let normalized = f.relative_path.trim_start_matches("./");
            normalized != main_relative && normalized != main_relative.trim_start_matches("./")
        })
        .collect();

    let files_found = other_files.len();
    let files_truncated = other_files.iter().filter(|f| f.truncated).count();

    if !other_files.is_empty() {
        let insertion_point = output.rfind("  }\n}\n").unwrap_or(output.len());
        let mut context_section = String::new();
        context_section.push_str(&format!("\n    // source_dir: {}\n", dir.display()));
        context_section.push_str(&format!(
            "    // {} additional file(s) found — the migrate skill should read them directly:\n",
            other_files.len()
        ));
        for file in &other_files {
            context_section.push_str(&format!(
                "    //   {} ({} lines)\n",
                file.relative_path, file.line_count,
            ));
        }
        context_section.push('\n');

        output.insert_str(insertion_point, &context_section);
    }

    let mut warnings = Vec::new();
    if files_truncated > 0 {
        warnings.push(format!(
            "{} file(s) were truncated (>{} lines)",
            files_truncated, TRUNCATION_THRESHOLD
        ));
    }

    Ok(MigrateDirectoryResult {
        output,
        source_dir: dir.display().to_string(),
        warnings,
        files_found,
        files_truncated,
    })
}

fn find_frontmatter_file(dir: &Path) -> Result<(PathBuf, String), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("Failed to read directory: {}", e))? {
        let entry = entry.map_err(|e| format!("Directory entry error: {}", e))?;
        let path = entry.path();
        if path.extension().map(|e| e == "md").unwrap_or(false)
            && let Ok(content) = std::fs::read_to_string(&path)
            && content.trim_start().starts_with("---")
            && content.contains("name:")
        {
            return Ok((path, content));
        }
    }
    Err(format!(
        "No SKILL.md found in '{}', and no .md file with name: in frontmatter",
        dir.display()
    ))
}

fn detect_conditional(content: &str) -> bool {
    let lower = content.to_lowercase();
    // Look for conditional language patterns
    let patterns = [
        "if the ",
        "if this ",
        "when the ",
        "when this ",
        "only if ",
        "unless ",
        "in case ",
        "depending on ",
        "conditionally",
        "skip if",
        "skip when",
    ];
    patterns.iter().any(|p| lower.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn migrate_simple_skillmd() {
        let source = r#"---
name: greeting
description: "Greet users warmly"
---

# greeting

## Introduction

Welcome the user and ask how they're doing.

## Respond

Provide a helpful response based on their needs.
"#;
        let result = migrate_skillmd(source, "greeting/SKILL.md");
        assert!(result.output.contains("skill \"greeting\""));
        assert!(result.output.contains("Auto-migrated from"));
        assert!(result.output.contains("TODO"));
        assert!(result.output.contains("step introduction"));
        assert!(result.output.contains("step respond"));
    }

    #[test]
    fn migrate_detects_conditionals() {
        let source = r#"---
name: deploy
description: "Deploy to production"
---

# deploy

## Check

If the branch is main, proceed with deployment.

## Deploy

Run the deployment pipeline.
"#;
        let result = migrate_skillmd(source, "deploy/SKILL.md");
        assert!(result.output.contains("TODO: Extract conditional logic"));
    }

    #[test]
    fn migrate_no_frontmatter() {
        let source = r#"# My Skill

## First Step

Do the first thing.

## Second Step

Do the second thing.
"#;
        let result = migrate_skillmd(source, "nofm/SKILL.md");
        assert!(result.output.contains("skill \"unnamed\""));
        assert!(result.output.contains("step first_step"));
        assert!(result.output.contains("step second_step"));
    }

    #[test]
    fn migrate_parses_yaml_parameters() {
        let source = r#"---
name: review
description: "Review code"
parameters:
  - name: files
    type: string[]
  - name: severity
    type: string
    optional: true
    default: medium
---

# review

## Analyze

Look at the code.
"#;
        let result = migrate_skillmd(source, "review/SKILL.md");
        assert!(
            result.output.contains("files"),
            "should have 'files' param: {}",
            result.output
        );
        assert!(
            result.output.contains("string[]"),
            "should have string[] type: {}",
            result.output
        );
        assert!(
            result.output.contains("severity"),
            "should have 'severity' param: {}",
            result.output
        );
        assert!(
            result.output.contains("medium"),
            "should have default value: {}",
            result.output
        );
        assert!(
            !result.output.contains("param1"),
            "should NOT use dummy placeholder: {}",
            result.output
        );
    }

    #[test]
    fn migrate_preserves_content() {
        let source = r#"---
name: review
description: "Review code changes"
---

# review

## Analyze

Look at the code diff carefully.
Check for security vulnerabilities.
Verify test coverage.
"#;
        let result = migrate_skillmd(source, "review/SKILL.md");
        assert!(result.output.contains("security vulnerabilities"));
        assert!(result.output.contains("test coverage"));
    }

    // ── Directory migration tests ───────────────────────────────────

    #[test]
    fn collect_directory_files_finds_md_only() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        fs::write(dir_path.join("SKILL.md"), "# Main skill").unwrap();
        fs::create_dir_all(dir_path.join("refs")).unwrap();
        fs::write(dir_path.join("refs/a.md"), "# Reference A").unwrap();
        fs::write(dir_path.join("refs/b.md"), "# Reference B").unwrap();
        fs::write(dir_path.join("other.txt"), "not markdown").unwrap();

        let files = collect_directory_files(dir_path);
        assert_eq!(files.len(), 3);
        let paths: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(paths.contains(&"./SKILL.md"));
        assert!(paths.contains(&"./refs/a.md"));
        assert!(paths.contains(&"./refs/b.md"));
        assert!(!paths.iter().any(|p| p.contains("other.txt")));
    }

    #[test]
    fn migrate_directory_bundles_context() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        fs::write(
            dir_path.join("SKILL.md"),
            "---\nname: test-skill\ndescription: \"A test\"\n---\n\n# test\n\n## Do Thing\n\nDo the thing.\n",
        ).unwrap();
        fs::create_dir_all(dir_path.join("refs")).unwrap();
        fs::write(
            dir_path.join("refs/guide.md"),
            "# Style Guide\n\nUse consistent naming.\n",
        )
        .unwrap();

        let result = migrate_directory(dir_path).unwrap();
        assert!(result.output.contains("skill \"test-skill\""));
        assert!(result.output.contains("refs/guide.md"));
        assert!(result.output.contains("1 additional file(s) found"));
        assert_eq!(result.files_found, 1);
    }

    #[test]
    fn migrate_directory_missing_skillmd() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        fs::create_dir_all(dir_path.join("refs")).unwrap();
        fs::write(
            dir_path.join("refs/guide.md"),
            "# Just a guide\n\nNo frontmatter.\n",
        )
        .unwrap();

        let result = migrate_directory(dir_path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("SKILL.md"),
            "Error should mention SKILL.md: {}",
            err
        );
    }

    #[test]
    fn migrate_directory_truncates_large_files() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path();

        fs::write(
            dir_path.join("SKILL.md"),
            "---\nname: big-skill\n---\n\n# big\n\n## Step\n\nDo stuff.\n",
        )
        .unwrap();
        fs::create_dir_all(dir_path.join("refs")).unwrap();

        let huge_content: String = (0..1000)
            .map(|i| format!("Line {} of the huge file\n", i))
            .collect();
        fs::write(dir_path.join("refs/huge.md"), &huge_content).unwrap();

        let result = migrate_directory(dir_path).unwrap();
        assert!(result.output.contains("refs/huge.md"));
        assert!(result.output.contains("1000 lines"));
        assert!(!result.output.contains("Line 500 of the huge file"));
        assert_eq!(result.files_truncated, 1);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn migrate_single_file_unchanged() {
        let source = "---\nname: simple\n---\n\n# simple\n\n## Act\n\nDo the thing.\n";
        let result = migrate_skillmd(source, "simple/SKILL.md");
        assert!(result.output.contains("skill \"simple\""));
        assert!(result.output.contains("step act"));
        assert!(!result.output.contains("DIRECTORY CONTEXT"));
    }
}
