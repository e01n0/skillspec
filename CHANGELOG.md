# Changelog

All notable changes to SkillSpec are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [Unreleased]

### Added

- **`--to` flag on `build`** — deploy compiled skills directly to a runtime. Accepts named targets (`claude`, `claude-project`, `cursor`, `cline`, `codex`) or any custom path. Use `--to` without a value for an interactive picker. Auto-selects the correct build target per runtime (e.g. `--to cursor` implies `--target cursor`). Works with `--watch` for auto-redeploy on save. Mutually exclusive with `--output`.

---

## [0.1.0] — 2026-05-21

Initial release. 89 tests passing.

### Added

#### Core pipeline
- Token types and error foundation (`token.rs`)
- AST node types and type system representation (`ast.rs`, `types.rs`)
- Lexer with keyword recognition and triple-string support (`lexer.rs`)
- Recursive descent parser for the SkillSpec grammar (`parser.rs`)
- Semantic type checker for AST validation (`checker.rs`)
- SKILL.md compiler with topologically-sorted steps and priority ordering (`compiler_skillmd.rs`)
- CLI skeleton wired to `check`, `build`, and `init` subcommands

#### Extended language (Phase 2)
- Tokens, AST nodes, and lexer support for six new grammar constructs
- Parser extensions: lazy context blocks, tool declarations, prompt directives, pipeline stages, orchestration phases, and mixins
- Checker and compiler support for all Phase 2 constructs
- Integration test fixture covering all Phase 2 features

#### Test framework
- `tests {}` block parsing and compilation (Phase 3)
- `skillspec test` — list all tests defined in an `.agent` file

#### Developer tools
- `skillspec fmt` — format an `.agent` file with canonical style
- `skillspec budget` — estimate token budget for skills in an `.agent` file
- `skillspec deps` — print dependency graph of steps, stages, and phases
- `skillspec migrate` — mechanically extract a `SKILL.md` into an `.agent.partial` scaffold

#### Package management
- `skillspec pack` — bundle an `.agent` file with a `package` declaration into a `.skillpkg` directory
- `skillspec install` — install a `.skillpkg` or `.agent` file into `.skillspec/packages/`

#### Native IR compilation target
- `compiler_ir.rs` — native IR compilation producing `.agentpkg` bundles (Gap 4)

#### Structural diff
- `skillspec diff` — show structural diff between two `.agent` files, or compare a compiled result against a `SKILL.md` (Gap 8)

#### Example skills
- `code-review` canonical fixture
- `skill-writer` meta-skill (SkillSpec written in SkillSpec)
- `brainstorming` example skill

### Fixed

- Dedent `context` text blocks and extract sentence descriptions; add `pre`/`post` sections with consistent field formatting
- Handle multi-byte UTF-8 characters in the lexer
- Dedent `persona` blocks in SKILL.md output
- `&&` and `||` logical operators in `when` expressions
- `contains(where:)`, `all(where:)`, `none(where:)` quantifier assertions in conditions

### Changed

- Renamed project from AgentLang to SkillSpec
