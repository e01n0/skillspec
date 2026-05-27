# SkillSpec

A typed, composable language for AI agent skills. `.agent` files compile to `SKILL.md` and other runtime formats.

## Build & Test

```sh
cargo build --release
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

Binary: `./target/release/skillspec`

## Architecture

The compiler pipeline: **lex** → **parse** → **check** → **compile**

| Module | File | Role |
|--------|------|------|
| Lexer | `src/lexer.rs` | Tokenises `.agent` source |
| Parser | `src/parser.rs` | Recursive descent → AST (`src/ast.rs`) |
| Checker | `src/checker.rs` | Semantic validation (types, cycles, refs) |
| SkillMd compiler | `src/compiler_skillmd.rs` | AST → `SKILL.md` (primary target) |
| Cursor compiler | `src/compiler_cursor.rs` | AST → `.cursorrules` |
| Cline compiler | `src/compiler_clinerules.rs` | AST → `.clinerules` |
| System prompt | `src/compiler_systemprompt.rs` | AST → plain text (Codex) |
| Native IR | `src/compiler_ir.rs` | AST → `.agentpkg` bundle |
| Formatter | `src/formatter.rs` | Canonical formatting |
| Linter | `src/lint.rs` | Quality rules beyond structural validity |
| Budget | `src/budget.rs` | Token estimation and trimming |
| Diff | `src/diff.rs` | Structural diff + semver classification |
| Migrate | `src/migrate.rs` | SKILL.md → `.agent.partial` extraction |
| Optimize | `src/optimize.rs` | SkillOpt integration |
| Deps | `src/deps.rs` | Step dependency graph |
| Resolver | `src/resolve.rs` | Multi-file import resolution |
| Test harness | `src/test_harness.rs` | Test block preparation and evaluation |

## Adding a language feature

Follow the pipeline in order:
1. Token (`src/token.rs`) → 2. Lexer (`src/lexer.rs`) → 3. AST (`src/ast.rs`) → 4. Parser (`src/parser.rs`) → 5. Checker (`src/checker.rs`) → 6. Compiler(s) → 7. Formatter (`src/formatter.rs`) → 8. Tests

## Adding a CLI command

1. Module in `src/` with a public entry-point
2. Subcommand variant in `Commands` enum in `src/main.rs`
3. Match + dispatch in `main()`
4. Export from `src/lib.rs` if it needs testing

## Conventions

- `cargo fmt` before committing
- No clippy warnings
- Tests: unit tests in `mod tests`, integration tests in `tests/integration_tests.rs`
- Fixtures go in `tests/fixtures/`
- Error messages: lowercase, no trailing punctuation, actionable
