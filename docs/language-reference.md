# Language Reference

File extension: `.agent`. Encoding: UTF-8. Formal grammar: [grammar.ebnf](grammar.ebnf).

---

## 1. File Structure

A `.agent` file is a sequence of top-level declarations in any order:

```
import { Symbol, ... } from "path"
type TypeName { ... }
mixin MixinName { ... }
package { ... }
skill "name" { ... }
pipeline "name" { ... }
orchestration "name" { ... }
```

Multiple skills, pipelines, and orchestrations may coexist in a single file.

---

## 2. Types

### Primitives

| Keyword | Description |
|---|---|
| `string` | UTF-8 string |
| `int` | 64-bit integer |
| `float` | 64-bit float |
| `bool` | `true` or `false` |

### Compound types

| Syntax | Description |
|---|---|
| `T[]` | Array of `T` |
| `map<K, V>` | Map with key type `K` and value type `V` |
| `enum("a", "b", ...)` | Closed set of string variants |

### Named types

```skillspec
type Finding {
  file: string
  line: int
  severity: enum("critical", "high", "medium", "low")
  suggestion?: string         // optional field
}
```

A named type is a record with named fields. Fields are required by default.

### Optionality and defaults

```skillspec
field_name?: Type               // optional, no default
field_name?: Type = value       // optional with default
```

Defaults are literal values (`"en"`, `42`, `true`, `[]`).

### Type inference

Types are not inferred. All fields must be explicitly typed.

### Imports

Import named types from packages or local files:

```skillspec
import { Finding, ReviewReport } from "@types/review"
import { Config } from "./shared/config"
```

Registry packages use `@scope/name` paths. Local imports use relative paths.

---

## 3. Skills

Full syntax:

```skillspec
skill "name" {
  input { ... }            // optional
  output { ... }           // optional
  tools { ... }            // optional
  permissions { ... }      // optional
  include MixinName        // zero or more
  pre { ... }              // optional
  post { ... }             // optional
  body { ... }             // required (or shorthand context directly in skill)
  tests { ... }            // optional
}
```

`extends` for inheritance:

```skillspec
skill "derived" extends "base" {
  body { ... }
}
```

### input / output

```skillspec
input {
  query: string
  files: string[]
  focus?: string
  severity?: enum("high", "medium", "low") = "medium"
}

output {
  result: string
  findings: Finding[]
}
```

Fields follow the same rules as type definitions.

### pre / post

```skillspec
pre {
  assert input.files != [] message "No files provided"
  assert when input.focus input.focus != "" message "Focus must not be empty"
}

post {
  assert output.result != "" message "Result must not be empty"
}
```

Each assertion:
```
assert [when <guard-expr>] <condition-expr> message "<string>"
```

`when` is an optional guard; the assertion is skipped if the guard is falsy.

### body (shorthand)

A minimal skill may use `context` directly without a `body` wrapper:

```skillspec
skill "hello" {
  context { "Greet the user warmly." }
}
```

---

## 4. Context Blocks

### Eager context

```skillspec
context { "prose" }
context(priority: critical) { "prose" }
context(priority: important, when: input.formal) { "prose" }
context(priority: supplementary, decay: 0.1) { "prose" }
context(priority: critical, until: discover) { "prose" }
```

Parameters (all optional, any combination):

| Parameter | Type | Description |
|---|---|---|
| `priority` | `critical` \| `important` \| `supplementary` \| `optional` | Controls ordering, trimming, and output annotations (see below) |
| `when` | expression | Only inject if expression is truthy |
| `decay` | float | Rate at which this context fades from the window over time |
| `until` | step name | Context is active until the named step completes; compiled output marks it as expired after that step |

**Priority flags:**

| Flag | Compiled annotation | Trimming behaviour |
|---|---|---|
| `critical` | `> **CRITICAL:**` prefix | Never trimmed |
| `important` | `> **IMPORTANT:**` prefix | Trimmed only under severe budget pressure |
| `supplementary` | No annotation | Default tier; trimmed before `important` |
| `optional` | `*Optional context:*` prefix | First to be trimmed |

When no priority is specified, the context is treated as `supplementary`. Higher priority contexts appear first in compiled output. The `critical-overuse` lint warns if more than 2 blocks are marked `critical` in a single skill.

**Lifecycle (`until`):** Body-level context blocks can declare `until: step_name`. The compiled output annotates the block with its lifecycle scope, and after that step heading, a note tells the agent the context is no longer active. This lets the agent mentally deprioritise setup instructions once they've been acted upon. The `until` target is validated by `skillspec check`.

Prose content is either a double-quoted string `"..."` or a triple-quoted
string `"""..."""`.

### Lazy context

Declared at the body level. Loaded on demand via `load` inside a step.

```skillspec
lazy context "name" (priority: supplementary) {
  summary "One-line description shown to the model instead of the full content."
  ref "./path/to/file.md"
}
```

Content variants:

**ref** loads a file path:
```skillspec
lazy context "patterns" (priority: supplementary) {
  summary "Design patterns reference."
  ref "./references/patterns.md"
}
```

**index** loads one of several named sections:
```skillspec
lazy context "catalog" (priority: supplementary) {
  summary "Error pattern catalog."
  index {
    section "security" {
      summary "Security vulnerability patterns."
      ref "./references/security.md"
    }
    section "performance" {
      summary "Performance anti-patterns."
      ref "./references/perf.md"
    }
  }
}
```

**Path validation:** `skillspec check` validates that `ref` paths point to existing files when the source file's directory is known. Missing files produce an `UnresolvedRef` error. This applies to both top-level `ref` and `ref` inside `index` sections. The check is skipped when running without a base directory (e.g. checking source from stdin).

**inline** embeds prose directly:
```skillspec
lazy context "note" (priority: optional) {
  summary "A reminder."
  "Only activate when the user seems confused."
}
```

---

## 5. Steps

```skillspec
step step_name {
  requires <dependency>     // optional
  when <expr>               // optional, skip step if false
  use skill_name(args)      // optional, delegate to another skill
  let name = <expr>         // zero or more local bindings
  load "lazy-context-name"  // zero or more lazy context loads
  emit output               // optional, signals this step produces final output
  context { ... }           // zero or more context blocks
}
```

### requires

| Form | Meaning |
|---|---|
| `requires step_a` | single prerequisite |
| `requires a & b & c` | all of the listed steps must complete |
| `requires a \| b \| c` | any one of the listed steps suffices |
| `requires all_steps` | run after every other step |

### when

```skillspec
step review_focus {
  when input.focus == "all" || input.focus == "types"
  context { "Review type usage." }
}
```

### use

```skillspec
step analyse {
  use static_analysis(files: input.files, mode: "strict")
}
```

Calls another skill or tool with named arguments.

### let

```skillspec
step normalise {
  let clean_name = input.name
  context { "Use the normalised name." }
}
```

Binds a name to an expression for use in contexts or downstream steps.

### load

```skillspec
step deep_review {
  requires analyse
  load "style-guide"
  load "error-catalog"
  context { "Cross-reference findings against the style guide." }
}
```

Triggers eager loading of a named lazy context.

### emit output

Marks the step as the one that produces the skill's final output. At most one
step should `emit output`. Typically the final step in the dependency chain.

---

## 6. Expressions

### Literals

| Type | Example |
|---|---|
| string | `"hello"`, `"""multi-line"""` |
| integer | `42`, `-1` |
| float | `0.9`, `3.14` |
| bool | `true`, `false` |
| array | `["a", "b", "c"]`, `[]` |

### Field access

```
input.field_name
output.nested.field
```

`input` and `output` are reserved identifiers referring to the skill's typed
input and output.

### Binary operators

**Comparison** (higher precedence):

| Operator | Meaning |
|---|---|
| `==` | equal |
| `!=` | not equal |
| `<` | less than |
| `>` | greater than |
| `<=` | less than or equal |
| `>=` | greater than or equal |

**Logical** (lower precedence, left-associative):

| Operator | Meaning |
|---|---|
| `&&` | logical AND |
| `\|\|` | logical OR |

`&&` binds more tightly than `||` (C-style precedence).

**Negation:**

```
!input.formal
```

### Leading-dot shorthand (where-clauses)

Inside quantifier assertions, `.field` is shorthand for the current array
element's field:

```skillspec
output.findings: contains(where: .severity == "critical")
```

---

## 7. Prompt Directives

Prompt directives appear inside `body`. All are optional.

### persona

```skillspec
persona {
  """
  You are a senior code reviewer focused on security and correctness.
  """
}
```

Sets the model's role. Rendered as a blockquote in compiled output.

### reasoning

```skillspec
reasoning extended
reasoning standard
reasoning none
```

Controls chain-of-thought depth.

### sampling

```skillspec
sampling {
  temperature: 0.3
  top_p: 0.9
}
```

Both fields are optional floats.

### format

```skillspec
format {
  style: json
  structure: output
}
```

`style` and `structure` are unquoted identifiers. Common values: `json`,
`markdown`, `plain`; `output`, `free`.

### reinforce

```skillspec
reinforce every 3 steps {
  "Stay focused on the review task."
}

reinforce on context_shift {
  "Re-read the original requirements."
}

reinforce when input.strict {
  "Apply strict mode rules."
}
```

Triggers:

| Form | When |
|---|---|
| `every N steps` | after every N steps |
| `on context_shift` | when the context window shifts |
| `when <expr>` | when expression is truthy |

### examples

```skillspec
examples {
  example "simple case" {
    input: "a short description"
    output: "a structured result"
    note: "Optional guidance for the model"
  }
}
```

`note` is optional. `output` accepts a string or a braced object literal.

---

## 8. Tools

```skillspec
tools {
  require Read
  require Bash
  require mcp("github") {
    pr_diff(repo: string, pr: int) -> string
    post_comment(repo: string, pr: int, body: string) -> void
  }
  optional mcp("slack") {
    send_message(channel: string, text: string) -> void
  }
}
```

### require vs optional

- `require`: the skill cannot run without this tool.
- `optional`: the skill degrades gracefully if unavailable.

### Builtin tools

Referenced by identifier: `Read`, `Bash`, `Edit`, `Write`, etc.

```skillspec
require Read
require Bash
```

### MCP tools

```skillspec
require mcp("server-name") {
  method_name(param: type, ...) -> return_type
}
```

The method block is optional; omitting it accepts all methods from the server.
`void` is a valid return type.

### allow / deny

Not currently surfaced in `.agent` syntax. Reserved for future use.

---

## 9. Permissions

```skillspec
permissions {
  filesystem: read_write("src/**", "tests/**")
  network: outbound("api.github.com")
  secrets: ["GITHUB_TOKEN", "SLACK_TOKEN"]
}
```

### filesystem

```skillspec
filesystem: <mode>("<pattern>", ...)
```

Modes: `read_only`, `read_write`, `write_only`. Patterns are glob strings.

### network

```skillspec
network: outbound("api.example.com")
```

Modes: `outbound`, `inbound`, `none`.

### secrets

```skillspec
secrets: ["ENV_VAR_NAME", ...]
```

Lists environment variables the skill is permitted to read.

---

## 10. Tests

```skillspec
tests {
  test "test name" {
    given { field: value, ... }
    mock tool_or_mcp { ... }     // optional
    expect { path: assertion, ... }
    confidence 0.9               // optional
    runs 5                       // optional
    snapshot "path/to/file.snap" // optional
  }
}
```

### given

Named input fields passed to the skill:

```skillspec
given {
  source_file: "fixtures/minimal.agent"
  review_focus: "types"
}
```

### mock

Mock a tool's responses:

```skillspec
mock github {
  pr_diff(repo: "my/repo", pr: 42) -> "diff content"
}
```

Special mock types:

```skillspec
mock slack { unavailable }
mock database { failing "connection refused" }
mock slow_api { slow "5s" }
```

### expect

```skillspec
expect {
  output.score: >= 70
  output.issues: none(where: .severity == "critical")
  output.summary: contains "well-structured"
  output.rating: resembles "A short positive assessment"
}
```

### Assertion forms

| Form | Description |
|---|---|
| `equals value` | Exact equality |
| `contains value` | Substring (string) or element (array) |
| `matches "regex"` | Regex match on string |
| `resembles "description"` | LLM-judged structural similarity |
| `satisfies "criterion"` | LLM-judged semantic criterion |
| `>= value`, `<= value`, `> value`, `< value`, `== value`, `!= value` | Numeric or string comparison |
| `contains(where: <expr>)` | At least one array element satisfies expression |
| `all(where: <expr>)` | All array elements satisfy expression |
| `none(where: <expr>)` | No array element satisfies expression |

In `where` expressions, `.field` refers to the current element's field
(shorthand for `_item.field`).

### confidence and runs

```skillspec
confidence 0.85
runs 5
```

`confidence` (0.0 to 1.0): minimum fraction of runs that must pass for the test
to be considered passing. `runs` defaults to 1. Applies to `resembles` and
`satisfies` assertions.

### snapshot

```skillspec
snapshot "snapshots/my-test.snap"
```

On first run, writes the output to the snapshot file. On subsequent runs,
compares output against the stored snapshot.

---

## 11. Pipelines

```skillspec
pipeline "name" {
  input { ... }          // optional
  output { ... }         // optional
  stage name { ... }     // one or more
  on_error { ... }       // optional
  timeout 30m            // optional
}
```

### stages

```skillspec
stage lint {
  use linter(repo: input.repo)
}

stage security {
  use security_scan(repo: input.repo)
}

stage review {
  requires lint & security
  use code_review(files: input.files)
}
```

Stage inputs and outputs are passed via named arguments. Reference a prior
stage's result as `stage_name.result`.

`requires` uses the same `&` / `|` syntax as skill steps.

Stages without `requires` run concurrently (subject to runtime support).

### on_error

```skillspec
on_error {
  use notify(channel: "alerts", message: "Pipeline failed")
}
```

Called with the last error context if any stage fails.

### timeout

```skillspec
timeout 30m
timeout 1h
timeout 300s
```

---

## 12. Orchestrations

Orchestrations coordinate multiple agents across named phases.

```skillspec
orchestration "name" {
  agents { ... }
  input { ... }
  output { ... }
  shared { ... }       // optional
  phase name { ... }   // one or more
  timeout 1h           // optional
}
```

### agents

```skillspec
agents {
  reviewer: agent(skill: "code-review", model: "opus")
  security: agent(skill: "security-audit", model: "sonnet")
  lead:     agent(skill: "review-lead",   model: "opus")
}
```

Each agent is declared with a local name, a skill reference, and a model
identifier.

### phases

```skillspec
phase review {
  reviewer.run(files: input.pr_url)
  security.run(files: input.pr_url)
}

phase decide {
  requires review
  lead.run(review_findings: input.pr_url)
  emit output from lead.result
}
```

- `requires` follows the same `&` / `|` dependency syntax.
- `agent.run(args)` invokes the agent's skill.
- `emit output from agent.result` maps an agent's output to the orchestration
  output.

### shared

Declares shared state and event handlers across agents (advanced use):

```skillspec
shared {
  findings: Finding[]
  on reviewer.finding_found {
    // expression body
  }
}
```

### rules

Declarative rules that fire when conditions are met:

```skillspec
rules {
  when security.result.critical_count > 0 {
    // trigger action
  }
}
```

---

## 13. Mixins

```skillspec
mixin mixin_name {
  step step_a {
    context { "First step." }
  }
  step step_b {
    requires all_steps
    context { "Final step, always runs last." }
  }
}
```

Mixins may contain `step` and `context` blocks. They cannot contain `input`,
`output`, `tests`, `tools`, or `permissions`.

Include a mixin in a skill:

```skillspec
skill "my-skill" {
  include mixin_name
  // ...
}
```

All mixin steps are injected into the skill as if they were declared inline.

---

## 14. Packages

```skillspec
package {
  name: "@scope/package-name"
  version: "1.2.3"
  description: "Short description"
  exports: ["SkillName", "TypeName"]
}
```

`exports` lists the skill names (with hyphens replaced by underscores) and
type names to include in the compiled package.

CLI commands:

```sh
skillspec pack my-skills.agent        # produces <name>@<version>.skillpkg/
skillspec install <path-or-file>      # installs to .skillspec/packages/
```

Installed packages live at `.skillspec/packages/<name>@<version>/`.

---

## 15. Imports

```skillspec
import { Symbol } from "path"
import { A, B, C } from "path"
```

### Path forms

| Path | Resolution |
|---|---|
| `"./relative/path"` | Local file relative to the importing `.agent` file |
| `"@scope/package"` | Package installed in `.skillspec/packages/` |
| `"@scope/package/sub"` | Sub-path within a package |

### Registry paths

Remote registry installs are handled by `skillspec install <url>` (when a
registry URL is provided). After installation, imports use the standard
`@scope/package` form.

---

## CLI Reference

| Command | Description |
|---|---|
| `skillspec check <file>` | Type-check and validate |
| `skillspec build <file> [--target skillmd\|native] [-o dir] [--to target]` | Compile and optionally deploy |
| `skillspec init <name>` | Scaffold a new `.agent` file |
| `skillspec fmt <file>` | Format with canonical style |
| `skillspec budget <file>` | Estimate token budget |
| `skillspec deps <file>` | Print dependency graph |
| `skillspec test <file>` | List tests (LLM execution requires runtime) |
| `skillspec grammar` | Print formal EBNF grammar |
| `skillspec diff <a> <b> [--against-skillmd]` | Structural diff |
| `skillspec migrate <file>` | Extract a SKILL.md to `.agent.partial` |
| `skillspec pack <file> [-o dir]` | Pack to `.skillpkg` |
| `skillspec install <path>` | Install a `.skillpkg` or `.agent` file |

Default build target is `skillmd`. The `native` target produces a binary
`.agentpkg` (JSON IR wrapped in a zip archive).

### `--to` (deploy to runtime)

`--to` resolves a named runtime to its expected path and build target, then compiles and writes output there in one step. Use without a value for an interactive picker.

| Target | Output path | Implied build target |
|---|---|---|
| `claude` | `~/.claude/skills/` | skillmd |
| `claude-project` | `.claude/skills/` | skillmd |
| `cursor` | `.cursor/rules/` | cursor |
| `cline` | `./` | clinerules |
| `codex` | `.codex/` | system-prompt |
| any path | that path | unchanged |

`--to` and `-o`/`--output` are mutually exclusive. `--to` works with `--watch`.
