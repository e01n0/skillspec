# SkillSpec

A typed, composable language for AI agent skills and workflows. Compiles to SKILL.md for backwards compatibility with Claude Code, Codex, Cursor, Gemini CLI, and 30+ other agent runtimes.

## Who This Is For

SkillSpec is for **teams maintaining shared skill libraries** — the people who have 20+ skills that compose, evolve, and get shared across developers. If you're writing a one-off skill for personal use, markdown is fine. If you're maintaining a skill ecosystem where changes ripple across dependencies, where multiple people edit the same skills, and where "it worked last week" is a recurring incident — that's where SkillSpec earns its keep.

## The Problem

A skill starts as a clean, focused markdown file. Then someone adds an edge case. Then a conditional. Then a 200-line reference section. Then another developer copy-pastes half of it into a new skill and changes three lines. Six months later, both skills have drifted, neither matches the original intent, and nobody knows which version is authoritative.

The failure modes are well-documented:

- **Context rot.** Skills bloat until the core intent is buried. LLMs deprioritise instructions in the middle of their context window, so instructions you added last silently suppress the ones you wrote first.
- **No contracts.** A skill that expects `files: string[]` will happily receive `files: 42` and produce garbage. There's no way to declare what a skill needs or what invariants it guarantees.
- **No composition.** Reuse means copy-paste. Update means update everywhere. Miss one? Good luck debugging.
- **No versioning.** Skills evolve but there's no diff, no changelog, no way to review what changed. A "small tweak" to a shared skill silently breaks every workflow that depends on it.

SkillSpec fixes this with a structured language that compiles down to the same SKILL.md format agents already understand.

## Why SkillSpec

### Skills as Software

A SkillSpec `.agent` file isn't a document — it's a program. It has typed inputs and outputs, pre/post contracts, composable steps, and inline tests. You can `diff` two versions structurally, `fmt` for consistent style, and `check` for type errors before deployment. Your skills get the same engineering rigour as the code they operate on.

### Versioning & Evolution

Skills change. SkillSpec makes that manageable:

- **Package versioning.** Every package has a semver version. `skillspec pack` produces versioned `.skillpkg` archives. Consumers pin to `@^1.0` and get compatible updates without breaking changes.
- **Structural diff.** `skillspec diff v1.agent v2.agent` shows exactly what changed — added steps, removed fields, modified context priorities — not just line-level text diffs.
- **Compiled output diffing.** `skillspec diff skill.agent deployed/SKILL.md --against-skillmd` detects when a deployed skill has drifted from its source. The backport skill (LLM-powered) can then reconcile the changes.
- **Type-checked evolution.** Add a required field to a skill's input? The compiler flags every caller that doesn't provide it. Rename a step? Every `requires` reference that's now broken shows up as a compile error, not a silent runtime failure.

### Team Collaboration

When multiple people work on skills, markdown falls apart. SkillSpec gives you the tools teams expect:

- **`skillspec fmt`** enforces canonical style — no more style wars in code review. Every `.agent` file formats the same way.
- **Shared packages.** Extract common patterns (logging, error handling, standard types) into packages that teams import. Update the package, and every skill that imports it gets the fix.
- **Code review with structure.** `skillspec diff` in CI shows reviewers exactly what a PR changes at the semantic level: "added step `validate`", "changed priority on security context from 80 to 95", "removed optional field `debug_mode`". Not "changed lines 47-52".
- **Contracts as documentation.** `pre { assert input.files != [] message "No files provided" }` is simultaneously a runtime check, a documentation statement, and a test assertion. It can't go stale because the compiler enforces it.

### Multi-File Complex Workflows

Real-world agent systems aren't single skills. They're ecosystems — shared types, reusable patterns, multi-stage pipelines, multi-agent orchestrations. SkillSpec handles this at every scale:

- **Imports and packages.** `import { Finding, Severity } from "@types/review"` — shared types across skills. Change the type definition once, every consumer adapts or gets a compile error.
- **Skill composition.** `use static_analysis(files: input.files)` calls another skill with type-checked arguments. The compiler verifies the called skill exists and the types match.
- **Mixins.** `mixin observability { ... }` defines reusable step patterns. `include observability` injects them into any skill. Update the mixin, every includer gets the update.
- **Pipelines.** Multi-skill workflows where stages can run in parallel, with typed data flow between them and shared error handling.
- **Orchestrations.** Multi-agent coordination with role assignments, shared state, and reactive rules. For when the problem needs more than one agent thinking.

### The Upgrade Path

You don't have to rewrite everything. SkillSpec meets you where you are:

1. **`skillspec migrate existing/SKILL.md`** — mechanically extracts what it can (frontmatter, section headings, conditional patterns) into `.agent.partial` files with TODO markers. This is scaffolding, not magic — it handles maybe 10-20% of a real migration.
2. **The migrate skill** (LLM-powered, runs in your agent runtime) does the real work — inferring types, step dependencies, and context priorities from your prose. Migration quality is bottlenecked by LLM quality, not SkillSpec quality. The structured `.agent.partial` gives the LLM a better target to aim at.
3. **`skillspec build --target skillmd`** compiles back to SKILL.md — your existing runtimes don't need to change. The `.agent` file is the source of truth; the SKILL.md is the build artifact.
4. **Adopt incrementally.** Start with types and contracts on your most critical skills. Add context management when you hit token budget issues. Add test definitions when you need regression detection. Add pipelines when your workflows outgrow single skills.

## Quick Start

```bash
cargo install skillspec
skillspec init my-skill        # scaffold a new .agent file
skillspec check my-skill.agent # type-check and validate
skillspec build my-skill.agent # compile to SKILL.md
```

## Hello World

The minimal skill is three lines:

```skillspec
skill "hello" {
  context { "Greet the user warmly." }
}
```

That's a valid `.agent` file. It compiles, it type-checks, it works. Everything else is opt-in.

## A Real Skill

Here's a condensed version of the brainstorming skill from `examples/brainstorming.agent` — showing typed I/O, prioritised context, lazy loading, conditional blocks, and a step DAG:

```skillspec
type Design {
  title: string
  summary: string
  approach: string
  tradeoffs: string
  open_questions: string[]
}

skill "brainstorming" {
  input {
    idea: string
    constraints?: string
  }
  output {
    design: Design
    ready_for_planning: bool
  }
  tools {
    require Read
    require Bash
  }
  body {
    persona {
      """
      You are a pragmatic software architect who values simplicity
      over cleverness. You push back on over-engineering and always
      ask "do we actually need this?" before adding complexity.
      """
    }
    reasoning extended

    context(priority: 100) {
      """
      Help the user turn a raw idea into a well-formed design.
      Explore the problem space before jumping to solutions.
      """
    }
    context(priority: 75, when: input.constraints) {
      """
      The user has specified constraints. Respect these as hard
      boundaries — do not propose solutions that violate them.
      """
    }
    lazy context "patterns-catalog" (priority: 40) {
      summary "Common design patterns and when to use them."
      ref "./references/design-patterns.md"
    }

    step explore {
      context(priority: 90) {
        "Understand the problem deeply. Ask one clarifying question at a time."
      }
    }
    step propose {
      requires explore
      load "patterns-catalog"
      context(priority: 80) {
        "Propose 2-3 approaches with clear tradeoffs."
      }
    }
    step finalise {
      requires propose
      emit output
      context { "Produce the final Design output." }
    }
  }
}
```

## Language Features

### Types

Custom types, enums, optionals, and arrays. Types are checked at compile time — no runtime surprises.

```skillspec
type Issue {
  severity: enum("critical", "warning", "info")
  section: string
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

### Steps & Dependencies

Steps form a DAG. The compiler validates the graph is acyclic, determines topological order, and catches unreachable steps. Conditional steps use `when` guards.

```skillspec
step analyse {
  context { "Read the file and understand its structure." }
}
step review_types {
  requires analyse
  when input.focus == "all" || input.focus == "types"
  context { "Check for type correctness." }
}
step synthesise {
  requires review_types | review_context
  emit output
  context { "Combine all findings." }
}
```

The `|` operator means "whichever of these completed" — a join on parallel branches.

### Context Management

Every context block has a priority (0-100). Higher priority contexts survive when the token budget gets tight. Conditional contexts only load when their guard is true. Lazy contexts stay on disk until a step explicitly loads them.

```skillspec
context(priority: 100) {
  "This is always included and always first."
}
context(priority: 60, when: input.verbose) {
  "Only included when verbose mode is on."
}
lazy context "reference-docs" (priority: 30) {
  summary "API documentation — loaded on demand."
  ref "./docs/api-reference.md"
}
```

The `budget` command estimates token usage across all eager and lazy contexts:

```bash
skillspec budget my-skill.agent
```

### Prompt Directives

Control how the LLM behaves — persona, reasoning mode, sampling, output format, few-shot examples, and reinforcement messages that repeat at intervals.

```skillspec
body {
  persona { "You are a senior code reviewer." }
  reasoning extended
  sampling { temperature: 0.3 }
  format { style: json, structure: output }
  reinforce every 3 steps {
    "Stay focused on correctness, not style."
  }
  examples {
    example "off-by-one" {
      input: "for i in range(len(items))"
      output: "Flag: potential off-by-one if used with index access"
    }
  }
}
```

### Tools & Permissions

Declare which tools a skill needs and what permissions it requires. The compiler enforces that compositions don't escalate permissions silently.

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

### Composition

Reuse skills with `use`, extend them with `extend`, and share behaviour across skills with `mixin` and `include`. **Important caveat:** when compiling to SKILL.md, composition is expressed as prose annotations (`*Uses: X*`, `*Includes mixin: Y*`). The LLM interprets these as instructions — there's no runtime dispatch. Real executable composition requires the native target (`.agentpkg`) and a runtime that supports it.

```skillspec
mixin conversation_logging {
  step log_outcome {
    requires all_steps
    context { "Record the final decision for future reference." }
  }
}

skill "design-session" {
  include conversation_logging
  // ...
}
```

### Pipelines

Multi-skill composition with explicit data flow between stages. Stages can run in parallel or declare dependencies.

```skillspec
pipeline "design-review" {
  input { design_doc: string }
  output { approved: bool }

  stage technical {
    use technical_review(doc: input.design_doc)
  }
  stage feasibility {
    use feasibility_check(doc: input.design_doc)
  }
  stage approval {
    requires technical & feasibility
    use final_approval(
      technical: technical.result,
      feasibility: feasibility.result
    )
  }
  timeout 1h
}
```

### Orchestrations

Multi-agent coordination with role assignments, shared state, and communication patterns. For when a single skill isn't enough.

```skillspec
orchestration "code-review" {
  agents {
    reviewer: use code_reviewer
    security: use security_scanner
    lead: use review_lead
  }
  phases {
    phase scan {
      parallel reviewer, security
    }
    phase decide {
      requires scan
      agent lead
    }
  }
}
```

### Testing

Test definitions are a first-class language feature — they live inside the skill file, get type-checked, and survive compilation. **What exists today:** the compiler parses test blocks, validates their structure, and `skillspec test` lists them. **What doesn't exist yet:** actual test execution. Running LLM-judged assertions with confidence thresholds requires LLM integration, which the CLI deliberately avoids (it's a deterministic tool). Test execution is a roadmap item — it will be a SkillSpec skill that runs in your agent runtime, not a CLI command.

```skillspec
tests {
  test "catches missing types" {
    given {
      source_file: "fixtures/no_types.agent"
      review_focus: "types"
    }
    expect {
      output.review.suggestions: contains(where: .category == "types")
    }
  }
  test "scores bad priorities low" {
    given { source_file: "fixtures/bad_priorities.agent" }
    expect {
      output.review.overall_score: <= 60
    }
    confidence 0.8
    runs 5
  }
}
```

List tests without running them:

```bash
skillspec test my-skill.agent
```

### Packages

Package declarations enable distribution. The `pack` command bundles a skill and its dependencies into a `.skillpkg` archive. The `install` command places it into `.skillspec/packages/` for local use.

```bash
skillspec pack my-skill.agent -o dist/
skillspec install dist/my-skill.skillpkg
```

## CLI Reference

| Command   | Description                                                              |
|-----------|--------------------------------------------------------------------------|
| `check`   | Type-check and validate an `.agent` file                                 |
| `build`   | Compile an `.agent` file to the target format                            |
| `init`    | Scaffold a new `.agent` skill file                                       |
| `fmt`     | Format an `.agent` file with canonical style                             |
| `budget`  | Estimate token budget for skills in an `.agent` file                     |
| `deps`    | Print dependency graph of steps, stages, and phases                      |
| `migrate` | Mechanically extract a SKILL.md into a `.agent.partial` file             |
| `pack`    | Package a skill into a `.skillpkg` archive                               |
| `install` | Install a `.skillpkg` package into `.skillspec/packages/`                |
| `test`    | List all tests defined in an `.agent` file                               |
| `diff`    | Show structural diff between two `.agent` files (or compiled vs SKILL.md)|
| `help`    | Print help for any command                                               |

The CLI is purely deterministic — no LLM calls, no network requests. Tasks that need reasoning (migration refinement, test execution, backporting) are handled by SkillSpec skills run in agent runtimes.

## Compilation Targets

### `skillmd` (default)

Compiles to a `SKILL.md` file — the standard format understood by Claude Code, Codex, Cursor, Gemini CLI, and other agent runtimes. Full backwards compatibility: any existing agent runtime can consume the output without modification.

```bash
skillspec build my-skill.agent --target skillmd
```

### `native`

Compiles to a `.agentpkg` archive containing an intermediate representation, metadata, and bundled references. For use with runtimes that support the native SkillSpec format directly.

```bash
skillspec build my-skill.agent --target native
```

## Design Principles

**Skills are functions, not documents.** A skill has typed inputs, typed outputs, pre/post contracts, and composable steps. It can be called, tested, and reasoned about like a function — even though its body is mostly natural language.

**Prose is first-class.** SkillSpec doesn't fight natural language — it embraces it. Instructions are written in plain English inside structured blocks. The language adds types and composition *around* prose, not instead of it.

**Progressive disclosure.** A valid skill is three lines. Types, steps, tools, tests, context priorities, lazy loading, pipelines, orchestrations — all opt-in. You pay syntax cost only for the features you use.

## Limitations

Being honest about what SkillSpec is and isn't:

- **Composition is compile-time, not runtime.** When targeting SKILL.md, `use`, `pipeline`, and `orchestration` compile to prose annotations. The LLM interprets them as instructions — there's no executable dispatch. Real runtime composition requires the native target (`.agentpkg`) and runtimes that support it, which don't exist yet.
- **Test execution doesn't exist yet.** Test blocks are parsed, type-checked, and included in compiled output. But `skillspec test` only lists tests — it doesn't run them. LLM-judged assertions (`resembles`, `satisfies`) with confidence thresholds are the hardest and most interesting part of the testing story, and they're not built. This is a roadmap item.
- **Migration is scaffolding, not magic.** `skillspec migrate` does mechanical extraction. For real skills, the LLM-powered migrate skill does 90% of the work. Migration quality depends on your LLM, not on SkillSpec.
- **The value inflects at scale.** If you maintain 3 simple skills, the overhead isn't worth it. SkillSpec pays for itself when you have enough skills, enough shared ownership, and enough evolution over time that the engineering rigour saves you from the failure modes markdown can't prevent.

## Roadmap

- **Test execution** — a `skillspec-test` skill that runs test blocks against LLMs with result aggregation and CI-friendly output
- **Remote registry** — `skillspec publish` / `skillspec install` from a central package registry
- **Language server / IDE support** — LSP for syntax highlighting, completions, and inline diagnostics
- **Formal grammar specification** — a complete EBNF grammar for the `.agent` format
- **Native runtime SDK** — libraries for runtimes to consume `.agentpkg` with real executable composition

## Contributing

Contributions welcome. The project is Rust — `cargo test` runs the suite, `cargo build --release` produces the binary. Check the `tests/` directory for integration tests and `examples/` for reference `.agent` files.

## License

MIT
