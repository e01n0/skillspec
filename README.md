# SkillSpec

There's a growing trend of encoding complex workflows into agent skills (SKILL.md, Cursor rules, etc). Works great until you try to merge them, version them, collaborate on them, or run any kind of CI. Markdown is a brilliant authoring format. As a production system format it's pretty brittle.

SkillSpec is a DSL that adds types, contracts, composition, and tests around agent skills, then compiles back down to the same SKILL.md that existing runtimes already understand. Once a skill works and you want to make it production-ready, you codify it into a `.agent` file and get versioning, structural diffs, type checking, and a path to CI/CD.

The minimal skill is three lines:

```skillspec
skill "hello" {
  context { "Greet the user warmly." }
}
```

That compiles, type-checks, and works. Everything else is opt-in.

## What works today

### Typed inputs and outputs

```skillspec
type Issue {
  severity: enum("critical", "warning", "info")
  message: string
  line_hint: int
}

skill "linter" {
  input {
    file: string
    focus?: enum("types", "context", "all") = "all"
  }
  output {
    issues: Issue[]
  }
}
```

Types compile to contract language in the SKILL.md. The contract is advisory today (the LLM reads it as instruction, not enforced signature) but it can't go stale without the compiler noticing.

### Validated step graph

Steps form a DAG. The compiler checks for cycles, finds topological order, flags unreachable steps.

```skillspec
step analyse {
  context { "Read the file and understand its structure." }
}
step review {
  requires analyse
  when input.focus == "all" || input.focus == "types"
  context { "Check for type correctness." }
}
step synthesise {
  requires review
  emit output
  context { "Combine all findings." }
}
```

Rename a step and every broken `requires` is a compile error.

### Context management

Every context block has a priority (0 to 100). Higher priorities survive when the token budget tightens. This is how you deal with the lost-in-the-middle problem: mark what matters, and when the window fills up, the low-priority stuff drops first instead of your core instructions getting buried.

Conditional contexts load only when their guard is true. Lazy contexts stay on disk until a step pulls them in.

```skillspec
context(priority: 100) {
  "Always included, always first."
}
context(priority: 60, when: input.verbose) {
  "Only when verbose mode is on."
}
lazy context "reference-docs" (priority: 30) {
  summary "API docs, loaded on demand."
  ref "./docs/api-reference.md"
}
```

`skillspec budget my-skill.agent` estimates token usage across all contexts.

### Structural diff

`diff` compares two skills semantically, not textually:

```bash
skillspec diff v1.agent v2.agent
# added step `validate`
# context "security" priority 80 -> 95
# removed optional field `debug_mode`
```

Also catches drift between source and deployed output:

```bash
skillspec diff skill.agent deployed/SKILL.md --against-skillmd
```

The `.agent` file is the source of truth. The SKILL.md is a build artifact.

## Quick start

```bash
git clone git@github.com:e01n0/skillspec.git
cd skillspec && cargo install --path .

skillspec init my-skill         # scaffold
skillspec check my-skill.agent  # type-check
skillspec build my-skill.agent  # compile to SKILL.md
```

[Quickstart guide](docs/quickstart.md) has more. [Language reference](docs/language-reference.md) has everything.

## CLI

| Command   | Does |
|-----------|------|
| `check`   | Type-check and validate |
| `build`   | Compile to `SKILL.md` or `.agentpkg` |
| `diff`    | Structural diff between `.agent` files, or source vs deployed |
| `budget`  | Token estimate across contexts |
| `fmt`     | Canonical formatting |
| `deps`    | Step dependency graph |
| `init`    | Scaffold a new `.agent` file |
| `migrate` | Extract a SKILL.md into `.agent.partial` |
| `pack` / `install` | Bundle and install `.skillpkg` archives |
| `test`    | List test blocks (doesn't run them) |

No LLM calls, no network. Anything that needs reasoning runs as a skill in your agent runtime.

## Migration

`skillspec migrate existing/SKILL.md` does mechanical extraction into `.agent.partial` with TODO markers. Gets you maybe 10-20% of the way. The migrate skill (`skills/skillspec-migrate.agent`, runs in your runtime) handles the rest. `skillspec build` compiles back to SKILL.md and your runtimes never notice.

## Roadmap

Designed but not shipped.

- **Runtime composition.** `use`, `pipeline`, `orchestration` currently compile to prose the LLM interprets. Real dispatch needs `.agentpkg` and runtimes that support it.
- **Test execution.** Test blocks parse and type-check; `skillspec test` lists them. Running them needs LLM integration, which will be a skill, not a CLI command.
- **Remote registry.** `publish` / `install` from a central registry.
- **Language server.** LSP for highlighting, completion, diagnostics.
- **Formal grammar.** Complete EBNF for `.agent`.

## Contributing

Rust. `cargo test` (152 tests), `cargo build --release`. See `tests/` and `examples/`.

## License

MIT
