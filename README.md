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

Every context block has a priority flag (`critical`, `important`, `supplementary`, `optional`). Higher priorities survive when the token budget tightens and appear first in compiled output with annotations the agent can act on. `critical` blocks are never trimmed. This is how you deal with the lost-in-the-middle problem: mark what matters, and when the window fills up, the optional stuff drops first instead of your core instructions getting buried.

Conditional contexts load only when their guard is true. Lazy contexts stay on disk until a step pulls them in. The `until` parameter lets body-level context expire after a named step completes.

```skillspec
context(priority: critical) {
  "Always included, always first."
}
context(priority: supplementary, when: input.verbose) {
  "Only when verbose mode is on."
}
context(priority: important, until: discover) {
  "Ask clarifying questions before assuming."
}
lazy context "reference-docs" (priority: optional) {
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

## What else the language does

The four features above are the headline. The language also covers:

### Prompt directives

Persona, reasoning mode, sampling, output format, few-shot examples, and reinforcement messages that repeat at intervals.

```skillspec
body {
  persona { "You are a senior code reviewer." }
  reasoning extended
  sampling { temperature: 0.3 }
  reinforce every 3 steps {
    "Stay focused on correctness, not style."
  }
}
```

### Tools and permissions

Declare what tools a skill needs and what access it gets. The compiler checks that compositions don't silently escalate permissions.

```skillspec
tools {
  require Read
  require Bash
  optional mcp("github") {
    search_issues(query: string) -> string
  }
}
permissions {
  filesystem: read_write("src/**", "tests/**")
  network: outbound("api.github.com")
}
```

### Pre/post contracts

```skillspec
pre {
  assert input.files != [] message "No files provided"
}
post {
  assert output.review.score >= 0 message "Score must be non-negative"
}
```

### Composition

Call other skills with `use`, share step patterns with `mixin`, inherit with `extends`. When compiling to SKILL.md these become prose annotations the LLM interprets (real dispatch needs the native target, which doesn't exist yet).

```skillspec
mixin logging {
  step log_outcome {
    requires all_steps
    context { "Record the final decision." }
  }
}

skill "design-session" {
  include logging
  // ...
}
```

### Pipelines and orchestrations

Multi-skill workflows with typed data flow between stages, and multi-agent coordination with role assignments. Same caveat as composition: compiles to prose today, real dispatch is on the roadmap.

```skillspec
pipeline "review" {
  stage technical { use technical_review(doc: input.doc) }
  stage security  { use security_scan(doc: input.doc) }
  stage approval  {
    requires technical & security
    use final_approval(
      technical: technical.result,
      security: security.result
    )
  }
}
```

### Tests

Test blocks are parsed and type-checked. `skillspec test` lists them but doesn't run them (execution is a roadmap item that'll run as a skill in your runtime, not a CLI command).

```skillspec
tests {
  test "catches missing types" {
    given { source_file: "fixtures/no_types.agent" }
    expect {
      output.issues: contains(where: .category == "types")
    }
  }
  test "scores bad priorities low" {
    given { source_file: "fixtures/bad_priorities.agent" }
    expect { output.score: <= 60 }
    confidence 0.8
    runs 5
  }
}
```

### Packages

`skillspec pack` bundles a skill into a `.skillpkg` archive. `skillspec install` puts it in `.skillspec/packages/`. Import types across skills with `import { Finding } from "@types/review"`.

Full details for all of these in the [language reference](docs/language-reference.md), [formal grammar](docs/grammar.ebnf), and [user guide](docs/guide.md).

## Quick start

```bash
git clone git@github.com:e01n0/skillspec.git
cd skillspec && cargo install --path .

skillspec init my-skill         # scaffold
skillspec check my-skill.agent  # type-check
skillspec build my-skill.agent  # compile to SKILL.md
```

Deploy straight to your runtime:

```bash
skillspec build my-skill.agent --to claude           # → ~/.claude/skills/my-skill/SKILL.md
skillspec build my-skill.agent --to claude-project    # → .claude/skills/my-skill/SKILL.md
skillspec build my-skill.agent --to cursor            # → .cursor/rules/my-skill.cursorrules
skillspec build my-skill.agent --to /custom/path      # → any directory
skillspec build my-skill.agent --to                   # interactive menu
```

`--to` auto-selects the right build target for each runtime. Combine with `--watch` to redeploy on every save.

[Quickstart guide](docs/quickstart.md) has more. [Language reference](docs/language-reference.md) has everything.

## Migrating an existing skill

You don't have to start from scratch. If you've got SKILL.md files that work and you want to bring them under SkillSpec:

```bash
# Single file
skillspec migrate my-skill/SKILL.md
# -> my-skill.agent.partial

# Single skill directory (SKILL.md + reference docs, examples, etc.)
skillspec migrate my-skill/
# -> my-skill/my-skill.agent.partial (with file listing for the skill to read)

# Entire skills folder (multiple skill subdirs + shared references)
skillspec migrate .assistant/skills/
# -> one .agent.partial per skill subdir
# -> orchestration scaffold detecting cross-skill pipelines and routing
```

The CLI does mechanical extraction only — frontmatter, headings, obvious conditionals. It produces `.agent.partial` files with TODO markers and lightweight pointers to sibling skills and shared directories.

The real work happens in the migrate skill (`skills/skillspec-migrate.agent`). Run it in your agent runtime (Claude Code, Cursor, etc.):

```bash
# Single skill — give it the partial and the directory to explore
#   partial_file="my-skill/my-skill.agent.partial" source_dir="my-skill/"

# Full tree — just give it the parent directory, no partial needed
#   source_dir=".assistant/skills/"
# It finds all SKILL.md files, greps for .md cross-references,
# reads shared directories, and produces the complete set of .agent files
```

The skill reads files directly via `Read` and `Bash` — it explores the directory, detects orchestration patterns between skills, and produces whatever SkillSpec constructs fit (skills, pipelines, orchestrations, lazy context refs, imports). It validates its own output by running `skillspec check` and iterating on failures.

```bash
# When you're happy with the output
skillspec check my-skill.agent
skillspec build my-skill.agent
# -> my-skill/SKILL.md (your runtimes never notice the difference)
```

You don't need to migrate everything at once. Start with the skills that break most often or that multiple people edit. The rest can stay as markdown until you need them.

## CLI

| Command   | Does |
|-----------|------|
| `check`   | Type-check and validate |
| `build`   | Compile to `SKILL.md` or `.agentpkg`. `--to` deploys to a runtime |
| `grammar` | Print formal EBNF grammar for `.agent` |
| `diff`    | Structural diff between `.agent` files, or source vs deployed |
| `budget`  | Token estimate across contexts |
| `fmt`     | Canonical formatting |
| `deps`    | Step dependency graph |
| `init`    | Scaffold a new `.agent` file |
| `migrate` | Extract SKILL.md file, directory, or skill tree into `.agent.partial` |
| `pack` / `install` | Bundle and install `.skillpkg` archives |
| `test`    | List test blocks (doesn't run them) |
| `optimize` | Iterative skill improvement via [SkillOpt](https://github.com/microsoft/SkillOpt). `--writeback` applies changes to `.agent` source |

No LLM calls for core commands, no network. `optimize` uses SkillOpt and routes all LLM calls through the hosting agent session — zero external API cost.

## Roadmap

Designed but not shipped.

- **Runtime composition.** `use`, `pipeline`, `orchestration` currently compile to prose the LLM interprets. Real dispatch needs `.agentpkg` and runtimes that support it.
- **Test execution.** Test blocks parse and type-check; `skillspec test --prepare` generates execution skills. Full LLM-driven execution available via `skillspec optimize`.
- **Remote registry.** `publish` / `install` from a central registry.
- **Language server.** LSP for highlighting, completion, diagnostics.

## Contributing

Rust. `cargo test` (268 tests), `cargo build --release`. See `tests/` and `examples/`.

## License

MIT
