# Contributing

## Build from source

```bash
git clone <repo>
cd skillspec
cargo build --release
```

The binary lands at `./target/release/skillspec`.

## Run tests

```bash
cargo test
```

All 89 tests must pass before a PR merges. Add tests for anything new.

## Adding a CLI command

1. Create a module in `src/` (e.g. `src/mycommand.rs`) and expose a public entry-point function.
2. Add the subcommand variant to the `Commands` enum in `src/main.rs`.
3. Match on it in the `main` dispatch block and call your function.
4. Export the module from `src/lib.rs` if it needs to be tested.

Follow the pattern of existing commands like `budget` or `deps`.

## Adding a language feature

Follow the pipeline in order — every layer depends on the one before it:

1. **Token** (`src/token.rs`) — add the new keyword or symbol variant.
2. **Lexer** (`src/lexer.rs`) — recognise and emit the token.
3. **AST** (`src/ast.rs`) — add the node type(s) the parser will produce.
4. **Parser** (`src/parser.rs`) — parse the token stream into AST nodes.
5. **Checker** (`src/checker.rs`) — validate the new nodes semantically.
6. **Compiler** (`src/compiler_skillmd.rs` and/or `src/compiler_ir.rs`) — emit output for the new nodes.
7. **Formatter** (`src/formatter.rs`) — add canonical formatting rules.
8. Add a fixture file under `tests/fixtures/` and cover the new path in the integration tests.

## Code style

- Run `rustfmt` before committing (`cargo fmt`).
- No clippy warnings (`cargo clippy -- -D warnings`).
- Follow the patterns already in each module — consistency over cleverness.
- Keep error messages user-facing: lowercase, no trailing punctuation, actionable where possible.

## Pull requests

- Describe what changed and why, not just what files you touched.
- Include tests — unit tests in the relevant `mod tests` block, integration tests in `tests/`.
- One logical change per PR. If you're fixing a bug and adding a feature, split them.
- The CI gate is `cargo test && cargo clippy -- -D warnings && cargo fmt --check`.
