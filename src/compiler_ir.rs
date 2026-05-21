use std::io::Write;
use crate::ast::SourceFile;

pub struct IrCompiler;

impl IrCompiler {
    pub fn new() -> Self {
        Self
    }

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
        assert_eq!(manifest["skills"].as_array().unwrap().len(), ast.skills.len());
        assert_eq!(manifest["pipelines"].as_array().unwrap().len(), ast.pipelines.len());
        assert_eq!(manifest["orchestrations"].as_array().unwrap().len(), ast.orchestrations.len());
        assert_eq!(manifest["types"].as_array().unwrap().len(), ast.type_defs.len());
        assert_eq!(manifest["mixins"].as_array().unwrap().len(), ast.mixins.len());
    }
}
