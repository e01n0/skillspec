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

pub fn migrate_skillmd(source: &str, source_path: &str) -> MigrateResult {
    let mut output = String::new();

    // Parse frontmatter
    let (frontmatter, body, raw_fm) = parse_frontmatter(source);
    let name = frontmatter.get("name").cloned().unwrap_or_else(|| "unnamed".to_string());
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
                output.push_str(&format!("    {}{}: {} = \"{}\"\n", param.name, opt, ty, default));
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
                output.push_str("      // TODO: Add `emit output` if this step produces the final result\n");
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
            && let Some(colon) = trimmed.find(':') {
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

fn detect_conditional(content: &str) -> bool {
    let lower = content.to_lowercase();
    // Look for conditional language patterns
    let patterns = [
        "if the ", "if this ", "when the ", "when this ",
        "only if ", "unless ", "in case ", "depending on ",
        "conditionally", "skip if", "skip when",
    ];
    patterns.iter().any(|p| lower.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
