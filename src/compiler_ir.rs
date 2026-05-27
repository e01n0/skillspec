use crate::ast::SourceFile;
use crate::compiler_skillmd::SkillMdCompiler;
use std::io::Write;
use std::path::Path;

pub struct AgentPkg {
    pub manifest: serde_json::Value,
    pub skills: Vec<(String, String)>,
    pub types: serde_json::Value,
    pub source: String,
}

pub struct IrCompiler;

impl Default for IrCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl IrCompiler {
    pub fn new() -> Self {
        Self
    }

    pub fn compile_pkg(&self, file: &SourceFile, source_text: &str) -> AgentPkg {
        let manifest = serde_json::json!({
            "ir_version": 1,
            "skills": file.skills.iter().map(|s| &s.name).collect::<Vec<_>>(),
            "pipelines": file.pipelines.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "orchestrations": file.orchestrations.iter().map(|o| &o.name).collect::<Vec<_>>(),
            "types": file.type_defs.iter().map(|t| &t.name).collect::<Vec<_>>(),
            "mixins": file.mixins.iter().map(|m| &m.name).collect::<Vec<_>>(),
        });

        let compiler = SkillMdCompiler::new();
        let skills: Vec<(String, String)> = file
            .skills
            .iter()
            .map(|s| (s.name.clone(), compiler.compile(s, file)))
            .collect();

        let types: serde_json::Value = file
            .type_defs
            .iter()
            .map(|t| {
                let fields: Vec<serde_json::Value> = t
                    .fields
                    .iter()
                    .map(|f| serde_json::json!({ "name": f.name, "optional": f.optional }))
                    .collect();
                serde_json::json!({ "name": t.name, "fields": fields })
            })
            .collect();

        AgentPkg {
            manifest,
            skills,
            types,
            source: source_text.to_string(),
        }
    }

    pub fn write_to_dir(&self, pkg: &AgentPkg, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("create dir: {e}"))?;

        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_string_pretty(&pkg.manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("write manifest: {e}"))?;

        std::fs::write(dir.join("source.agent"), &pkg.source)
            .map_err(|e| format!("write source: {e}"))?;

        if pkg.types.as_array().is_some_and(|a| !a.is_empty()) {
            std::fs::write(
                dir.join(".types.json"),
                serde_json::to_string_pretty(&pkg.types).map_err(|e| e.to_string())?,
            )
            .map_err(|e| format!("write types: {e}"))?;
        }

        for (name, content) in &pkg.skills {
            let skill_dir = dir.join(name);
            std::fs::create_dir_all(&skill_dir).map_err(|e| format!("create skill dir: {e}"))?;
            std::fs::write(skill_dir.join("SKILL.md"), content)
                .map_err(|e| format!("write SKILL.md: {e}"))?;
        }

        Ok(())
    }

    /// Legacy zip format — kept for backward compatibility.
    pub fn compile(&self, file: &SourceFile) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default();

            // manifest.json
            let manifest = serde_json::json!({
                "ir_version": 1,
                "skills": file.skills.iter().map(|s| &s.name).collect::<Vec<_>>(),
                "pipelines": file.pipelines.iter().map(|p| &p.name).collect::<Vec<_>>(),
                "orchestrations": file.orchestrations.iter().map(|o| &o.name).collect::<Vec<_>>(),
                "types": file.type_defs.iter().map(|t| &t.name).collect::<Vec<_>>(),
                "mixins": file.mixins.iter().map(|m| &m.name).collect::<Vec<_>>(),
            });
            zip.start_file("manifest.json", options)
                .map_err(|e| e.to_string())?;
            let manifest_bytes =
                serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
            zip.write_all(manifest_bytes.as_bytes())
                .map_err(|e| e.to_string())?;

            // ir.json — the full AST
            zip.start_file("ir.json", options)
                .map_err(|e| e.to_string())?;
            let ir_bytes = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
            zip.write_all(ir_bytes.as_bytes())
                .map_err(|e| e.to_string())?;

            zip.finish().map_err(|e| e.to_string())?;
        }
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    #[test]
    fn compiles_to_valid_zip() {
        let source = r#"skill "hello" { context { "Greet warmly." } }"#;
        let tokens = Lexer::new(source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = IrCompiler::new();
        let bytes = compiler.compile(&ast).unwrap();

        // Verify it's a valid zip
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        assert!(archive.by_name("manifest.json").is_ok());
        assert!(archive.by_name("ir.json").is_ok());
    }

    #[test]
    fn manifest_has_ir_version() {
        let source = r#"skill "hello" { context { "Greet warmly." } }"#;
        let tokens = Lexer::new(source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = IrCompiler::new();
        let bytes = compiler.compile(&ast).unwrap();

        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let manifest = archive.by_name("manifest.json").unwrap();
        let manifest: serde_json::Value = serde_json::from_reader(manifest).unwrap();
        assert_eq!(manifest["ir_version"], 1);
        assert_eq!(manifest["skills"][0], "hello");
    }

    #[test]
    fn ir_round_trips() {
        let source = r#"
            type Finding {
                file: string
                severity: string
            }
            skill "review" {
                input { files: string[] }
                output { findings: Finding[] }
                body { context { "Review code." } }
            }
        "#;
        let tokens = Lexer::new(source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = IrCompiler::new();
        let bytes = compiler.compile(&ast).unwrap();

        // Read back the IR and verify structure
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let ir = archive.by_name("ir.json").unwrap();
        let deserialized: SourceFile = serde_json::from_reader(ir).unwrap();
        assert_eq!(deserialized.skills.len(), 1);
        assert_eq!(deserialized.skills[0].name, "review");
        assert_eq!(deserialized.type_defs.len(), 1);
    }

    #[test]
    fn ir_round_trips_full_featured() {
        let source = include_str!("../tests/fixtures/full_featured.agent");
        let tokens = Lexer::new(source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = IrCompiler::new();
        let bytes = compiler.compile(&ast).unwrap();

        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let ir = archive.by_name("ir.json").unwrap();
        let deserialized: SourceFile = serde_json::from_reader(ir).unwrap();

        // Verify complex structure survived
        assert_eq!(deserialized.skills.len(), ast.skills.len());
        assert_eq!(deserialized.pipelines.len(), ast.pipelines.len());
        assert_eq!(deserialized.orchestrations.len(), ast.orchestrations.len());
        assert_eq!(deserialized.type_defs.len(), ast.type_defs.len());
        assert_eq!(deserialized.mixins.len(), ast.mixins.len());

        // Verify skill internals
        let skill = &deserialized.skills[0];
        assert!(skill.tools.is_some());
        assert!(skill.permissions.is_some());
        assert!(!skill.includes.is_empty());
        assert!(!skill.pre.is_empty());
        assert!(!skill.post.is_empty());
        assert!(!skill.body.lazy_contexts.is_empty());
        assert!(!skill.body.steps.is_empty());
        assert!(skill.body.directives.reasoning.is_some());
        assert!(skill.body.directives.persona.is_some());
    }

    #[test]
    fn manifest_lists_all_constructs() {
        let source = include_str!("../tests/fixtures/full_featured.agent");
        let tokens = Lexer::new(source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = IrCompiler::new();
        let bytes = compiler.compile(&ast).unwrap();

        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let manifest = archive.by_name("manifest.json").unwrap();
        let manifest: serde_json::Value = serde_json::from_reader(manifest).unwrap();

        assert_eq!(manifest["ir_version"], 1);
        assert_eq!(
            manifest["skills"].as_array().unwrap().len(),
            ast.skills.len()
        );
        assert_eq!(
            manifest["pipelines"].as_array().unwrap().len(),
            ast.pipelines.len()
        );
        assert_eq!(
            manifest["orchestrations"].as_array().unwrap().len(),
            ast.orchestrations.len()
        );
        assert_eq!(
            manifest["types"].as_array().unwrap().len(),
            ast.type_defs.len()
        );
        assert_eq!(
            manifest["mixins"].as_array().unwrap().len(),
            ast.mixins.len()
        );
    }

    // ── Directory-based native format ────────────────────────────────

    #[test]
    fn native_creates_directory() {
        let source = r#"skill "hello" { body { context { "Greet." } } }"#;
        let tokens = Lexer::new(source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = IrCompiler::new();
        let pkg = compiler.compile_pkg(&ast, source);
        let dir = std::env::temp_dir().join("skillspec_native_test_dir");
        let _ = std::fs::remove_dir_all(&dir);
        compiler.write_to_dir(&pkg, &dir).unwrap();
        assert!(dir.is_dir(), "output should be a directory");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn native_contains_manifest() {
        let source = r#"skill "hello" { body { context { "Greet." } } }"#;
        let tokens = Lexer::new(source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = IrCompiler::new();
        let pkg = compiler.compile_pkg(&ast, source);
        let dir = std::env::temp_dir().join("skillspec_native_test_manifest");
        let _ = std::fs::remove_dir_all(&dir);
        compiler.write_to_dir(&pkg, &dir).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["ir_version"], 1);
        assert_eq!(manifest["skills"][0], "hello");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn native_contains_skillmd() {
        let source = r#"skill "hello" { body { context { "Greet." } } }"#;
        let tokens = Lexer::new(source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = IrCompiler::new();
        let pkg = compiler.compile_pkg(&ast, source);
        let dir = std::env::temp_dir().join("skillspec_native_test_skillmd");
        let _ = std::fs::remove_dir_all(&dir);
        compiler.write_to_dir(&pkg, &dir).unwrap();
        let skill_md = std::fs::read_to_string(dir.join("hello/SKILL.md")).unwrap();
        assert!(skill_md.contains("name: hello"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn native_contains_source() {
        let source = r#"skill "hello" { body { context { "Greet." } } }"#;
        let tokens = Lexer::new(source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = IrCompiler::new();
        let pkg = compiler.compile_pkg(&ast, source);
        let dir = std::env::temp_dir().join("skillspec_native_test_source");
        let _ = std::fs::remove_dir_all(&dir);
        compiler.write_to_dir(&pkg, &dir).unwrap();
        let saved = std::fs::read_to_string(dir.join("source.agent")).unwrap();
        assert_eq!(saved, source);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn native_contains_types_json() {
        let source = r#"
            type Finding { file: string severity: string }
            skill "review" {
                input { findings: Finding[] }
                body { context { "Review." } }
            }
        "#;
        let tokens = Lexer::new(source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = IrCompiler::new();
        let pkg = compiler.compile_pkg(&ast, source);
        let dir = std::env::temp_dir().join("skillspec_native_test_types");
        let _ = std::fs::remove_dir_all(&dir);
        compiler.write_to_dir(&pkg, &dir).unwrap();
        let types: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(".types.json")).unwrap())
                .unwrap();
        assert_eq!(types[0]["name"], "Finding");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn native_no_types_file_when_no_types() {
        let source = r#"skill "hello" { body { context { "Greet." } } }"#;
        let tokens = Lexer::new(source).tokenize().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        let compiler = IrCompiler::new();
        let pkg = compiler.compile_pkg(&ast, source);
        let dir = std::env::temp_dir().join("skillspec_native_test_notypes");
        let _ = std::fs::remove_dir_all(&dir);
        compiler.write_to_dir(&pkg, &dir).unwrap();
        assert!(
            !dir.join(".types.json").exists(),
            ".types.json should not exist when no types"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
