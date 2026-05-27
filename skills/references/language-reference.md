# SkillSpec Language Quick Reference

File extension: `.agent`. Encoding: UTF-8.

## Top-Level Declarations

```
import { Symbol } from "path"
type TypeName { ... }
mixin MixinName { ... }
package { ... }
skill "name" { ... }
pipeline "name" { ... }
orchestration "name" { ... }
```

## Primitives

| Type | Description |
|------|-------------|
| `string` | UTF-8 text |
| `int` | 64-bit integer |
| `float` | 64-bit float |
| `bool` | `true` / `false` |

## Compound Types

| Syntax | Example |
|--------|---------|
| `T[]` | `string[]`, `Finding[]` |
| `map<K, V>` | `map<string, int>` |
| `enum("a", "b")` | `enum("high", "medium", "low")` |

## Named Types

```agent
type Finding {
  file: string
  line: int
  severity: enum("critical", "high", "medium", "low")
  suggestion?: string              // optional, no default
  count?: int = 0                  // optional with default
}
```

## Skill Structure

```agent
skill "name" {
  input { ... }            // optional
  output { ... }           // optional
  tools { ... }            // optional
  permissions { ... }      // optional
  include MixinName        // zero or more
  pre { ... }              // optional
  post { ... }             // optional
  body { ... }             // required (or shorthand: context directly in skill)
  tests { ... }            // optional
}
```

Minimal valid skill: `skill "hello" { context { "Greet warmly." } }`

Inheritance: `skill "derived" extends "base" { body { ... } }`

## Input / Output

```agent
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

## Steps

```agent
step step_name {
  requires <dep>            // optional: step_a, a & b, a | b, all_steps
  when <expr>               // optional: skip if false
  use skill_name(args)      // optional: delegate to another skill
  let name = <expr>         // zero or more local bindings
  load "lazy-context-name"  // zero or more
  emit output               // optional: marks this step as producing final output
  context { ... }           // zero or more context blocks
}
```

## Context Blocks

Eager: `context(priority: important, when: input.formal, decay: 0.1) { "prose" }`

All parameters optional. Priority: `critical`, `important`, `supplementary`, or `optional` — higher survives trimming and gets annotated in compiled output. `until: step_name` marks context as active only until that step completes.

Lazy (declared at body level, loaded via `load` in steps):

```agent
lazy context "name" (priority: supplementary) {
  summary "One-line description shown instead of full content."
  ref "./path/to/file.md"
}
```

Lazy with index (multiple sections):

```agent
lazy context "catalog" (priority: supplementary) {
  summary "Error pattern catalog."
  index {
    section "security" {
      summary "Security patterns."
      ref "./references/security.md"
    }
  }
}
```

Lazy with inline content: replace `ref` with `"inline prose"`.

## Pre / Post Assertions

```agent
pre {
  assert input.files != [] message "No files"
  assert when input.focus input.focus != "" message "Focus must not be empty"
}
post {
  assert output.result != "" message "Result required"
}
```

## Persona

```agent
persona { """You are a senior code reviewer.""" }
```

## Reasoning

```agent
reasoning extended    // deep chain-of-thought
reasoning standard    // normal
reasoning none        // suppress
```

## Sampling

```agent
sampling { temperature: 0.3  top_p: 0.9 }
```

## Format

```agent
format { style: json  structure: output }
```

Values: `json`, `markdown`, `plain`; `output`, `free`.

## Reinforce

```agent
reinforce every 3 steps { "Stay focused." }
reinforce on context_shift { "Re-read requirements." }
reinforce when input.strict { "Apply strict rules." }
```

## Examples

```agent
examples {
  example "case name" {
    input: "description"
    output: "result"
    note: "optional guidance"
  }
}
```

## Tools

```agent
tools {
  require Read
  require Bash
  require mcp("github") {
    pr_diff(repo: string, pr: int) -> string
  }
  optional mcp("slack") {
    send_message(channel: string, text: string) -> void
  }
}
```

## Permissions

```agent
permissions {
  filesystem: read_write("src/**", "tests/**")
  network: outbound("api.github.com")
  secrets: ["GITHUB_TOKEN"]
}
```

Filesystem modes: `read_only`, `read_write`, `write_only`.

## Tests

```agent
tests {
  test "name" {
    given { field: value }
    mock tool { method(args) -> "response" }
    mock tool { unavailable }
    mock tool { failing "error" }
    mock tool { slow "5s" }
    expect {
      output.field: equals value
      output.field: contains "substring"
      output.field: matches "regex"
      output.field: >= 42
      output.field: resembles "structural description"
      output.field: satisfies "semantic criterion"
      output.arr: contains(where: .field == value)
      output.arr: all(where: .field > 0)
      output.arr: none(where: .field == "bad")
    }
    confidence 0.85
    runs 5
    snapshot "path/to/file.snap"
  }
}
```

## Mixins

```agent
mixin name {
  step a { context { "..." } }
  step b { requires all_steps  context { "..." } }
}
```

Include in skill: `include name`. Steps are injected as if declared inline.

## Pipeline

```agent
pipeline "name" {
  input { ... }
  output { ... }
  stage name { use skill(args)  requires a & b }
  on_error { use handler(message: "failed") }
  timeout 30m
}
```

Stages without `requires` run concurrently. Reference prior stage: `stage_name.result`.

## Orchestration

```agent
orchestration "name" {
  agents {
    reviewer: agent(skill: "code-review", model: "opus")
  }
  input { ... }
  output { ... }
  shared { findings: Finding[] }
  phase review { reviewer.run(files: input.pr_url) }
  phase decide {
    requires review
    lead.run(findings: input.pr_url)
    emit output from lead.result
  }
  rules { when security.result.critical_count > 0 { ... } }
  timeout 1h
}
```

## Imports

```agent
import { Symbol } from "./relative/path"
import { Symbol } from "@scope/package"
```

## Package

```agent
package {
  name: "@scope/package-name"
  version: "1.2.3"
  description: "Short description"
  exports: ["SkillName", "TypeName"]
}
```
