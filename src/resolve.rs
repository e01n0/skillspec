use std::path::{Path, PathBuf};

use crate::ast::SourceFile;
use crate::error::SkillSpecError;
use crate::lexer::Lexer;
use crate::parser::Parser;

/// Resolve an import path to a file path on disk.
///
/// Resolution strategy:
/// 1. `"./foo"` or `"../foo"` — relative to base_dir, try `.agent` extension
/// 2. `"@scope/name"` — strip `@`, try `scope/name.agent` relative to base_dir,
///    then search `.skillspec/packages/` walking up from base_dir
/// 3. `"bare/path"` — relative to base_dir, try `.agent` extension
pub fn resolve_import_path(import_path: &str, base_dir: &Path) -> Option<PathBuf> {
    let relative = import_path.strip_prefix('@').unwrap_or(import_path);

    // Try relative to base_dir
    if let Some(found) = try_resolve(relative, base_dir) {
        return Some(found);
    }

    // For @-prefixed paths, also search .skillspec/packages/
    if import_path.starts_with('@')
        && let Some(found) = resolve_in_packages(import_path, base_dir)
    {
        return Some(found);
    }

    None
}

fn try_resolve(relative: &str, base_dir: &Path) -> Option<PathBuf> {
    let candidate = base_dir.join(relative);

    if candidate.is_file() && is_within(base_dir, &candidate) {
        return Some(candidate);
    }

    let with_ext = candidate.with_extension("agent");
    if with_ext.is_file() && is_within(base_dir, &with_ext) {
        return Some(with_ext);
    }

    None
}

fn is_within(base: &Path, target: &Path) -> bool {
    let Ok(canonical_base) = base.canonicalize() else {
        return false;
    };
    let Ok(canonical_target) = target.canonicalize() else {
        return false;
    };
    canonical_target.starts_with(&canonical_base)
}

/// Walk up from base_dir looking for `.skillspec/packages/<path>.agent`.
fn resolve_in_packages(import_path: &str, start_dir: &Path) -> Option<PathBuf> {
    let stripped = import_path.strip_prefix('@').unwrap_or(import_path);
    // @scope/name → look for .skillspec/packages/scope/name/ containing .agent files
    // Also try the flat form: .skillspec/packages/scope/name.agent
    let mut dir = start_dir.to_path_buf();
    loop {
        let pkg_base = dir.join(".skillspec").join("packages");
        if pkg_base.is_dir() {
            // Try: .skillspec/packages/<path>.agent
            if let Some(found) = try_resolve(stripped, &pkg_base) {
                return Some(found);
            }
            // Try: .skillspec/packages/<path>/<stem>.agent (packaged skill directory)
            let pkg_dir = pkg_base.join(stripped);
            if pkg_dir.is_dir() {
                if let Some(stem) = std::path::Path::new(stripped)
                    .file_name()
                    .and_then(|n| n.to_str())
                {
                    let named = pkg_dir.join(format!("{}.agent", stem));
                    if named.is_file() {
                        return Some(named);
                    }
                }
                if let Ok(entries) = std::fs::read_dir(&pkg_dir) {
                    let agent_files: Vec<std::path::PathBuf> = entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.extension().map(|e| e == "agent").unwrap_or(false))
                        .collect();
                    if agent_files.len() == 1 {
                        return Some(agent_files.into_iter().next().unwrap());
                    }
                }
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Read and parse a file into a SourceFile AST.
pub fn parse_file(path: &Path) -> Result<SourceFile, SkillSpecError> {
    let source = std::fs::read_to_string(path)?;
    let tokens = Lexer::new(&source).tokenize()?;
    let ast = Parser::new(tokens).parse()?;
    Ok(ast)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolves_at_scoped_path() {
        let dir = std::env::temp_dir().join("skillspec_resolve_test_at");
        let types_dir = dir.join("types");
        fs::create_dir_all(&types_dir).unwrap();
        fs::write(
            types_dir.join("review.agent"),
            "type Finding { file: string }",
        )
        .unwrap();

        let result = resolve_import_path("@types/review", &dir);
        assert!(result.is_some(), "should resolve @types/review");
        assert!(result.unwrap().ends_with("review.agent"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolves_relative_path() {
        let dir = std::env::temp_dir().join("skillspec_resolve_test_rel");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("shared.agent"), "type Shared { value: string }").unwrap();

        let result = resolve_import_path("./shared", &dir);
        assert!(result.is_some(), "should resolve ./shared");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn returns_none_for_missing_path() {
        let dir = std::env::temp_dir().join("skillspec_resolve_test_none");
        fs::create_dir_all(&dir).unwrap();

        let result = resolve_import_path("@types/nonexistent", &dir);
        assert!(result.is_none());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolves_from_skillspec_packages() {
        let dir = std::env::temp_dir().join("skillspec_resolve_test_pkg");
        let pkg_dir = dir
            .join(".skillspec")
            .join("packages")
            .join("my-lib")
            .join("types");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("shared.agent"), "type Shared { val: string }").unwrap();

        let result = resolve_import_path("@my-lib/types/shared", &dir);
        assert!(result.is_some(), "should resolve from .skillspec/packages/");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn package_dir_prefers_named_file() {
        let dir = std::env::temp_dir().join("skillspec_resolve_test_named");
        let pkg_dir = dir
            .join(".skillspec")
            .join("packages")
            .join("my-lib")
            .join("utils");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("utils.agent"), "type U { x: string }").unwrap();
        fs::write(pkg_dir.join("other.agent"), "type O { y: string }").unwrap();

        let result = resolve_import_path("@my-lib/utils", &dir);
        assert!(result.is_some(), "should resolve from package dir");
        let path = result.unwrap();
        assert!(
            path.ends_with("utils.agent"),
            "should pick utils.agent, not other.agent: {:?}",
            path
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn package_dir_single_file_fallback() {
        let dir = std::env::temp_dir().join("skillspec_resolve_test_single");
        let pkg_dir = dir
            .join(".skillspec")
            .join("packages")
            .join("my-lib")
            .join("core");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("main.agent"), "type M { x: string }").unwrap();

        let result = resolve_import_path("@my-lib/core", &dir);
        assert!(result.is_some(), "should resolve single .agent file");
        assert!(result.unwrap().ends_with("main.agent"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn package_dir_ambiguous_returns_none() {
        let dir = std::env::temp_dir().join("skillspec_resolve_test_ambig");
        let pkg_dir = dir
            .join(".skillspec")
            .join("packages")
            .join("my-lib")
            .join("stuff");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("a.agent"), "type A { x: string }").unwrap();
        fs::write(pkg_dir.join("b.agent"), "type B { y: string }").unwrap();

        let result = resolve_import_path("@my-lib/stuff", &dir);
        assert!(
            result.is_none(),
            "should return None when multiple .agent files and no name match: {:?}",
            result
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_file_works() {
        let dir = std::env::temp_dir().join("skillspec_resolve_test_parse");
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("types.agent");
        fs::write(
            &file_path,
            r#"
                type Finding {
                    file: string
                    severity: string
                }
            "#,
        )
        .unwrap();

        let ast = parse_file(&file_path).expect("should parse");
        assert_eq!(ast.type_defs.len(), 1);
        assert_eq!(ast.type_defs[0].name, "Finding");

        fs::remove_dir_all(&dir).ok();
    }
}
