# SkillSpec User Guide

SkillSpec is a typed, composable language for defining AI agent skills and
workflows. You write `.agent` files; the compiler validates them and compiles
them to `SKILL.md` instruction documents that your agent runtime reads.

---

## 1. Installation

Build from source with Cargo:

```sh
git clone git@github.com:e01n0/skillspec.git
cd skillspec
cargo install --path .
```

Verify:

```sh
skillspec --version
```

---

## 2. Your First Skill

### Scaffold

```sh
skillspec init greeter
```

This creates `greeter.agent`:

```agent
skill "greeter" {
  input {
    query: string
  }
  output {
    result: string
  }
  body {
    context { "You are a helpful assistant." }
    step main {
      emit output
      context { "Answer the query provided in the input." }
    }
  }
}
```

### Validate

```sh
skillspec check greeter.agent
# ✓ greeter.agent: no errors
```

### Build

```sh
skillspec build greeter.agent
# ✓ greeter.agent → greeter/SKILL.md
```

The compiler writes `greeter/SKILL.md` — a structured Markdown file your agent
runtime loads.

### Format

```sh
skillspec fmt greeter.agent
```

Rewrites the file with canonical indentation and whitespace.

---

## 3. Adding Types

Define named types at the top of the file. Types can be used in `input`,
`output`, or other type definitions.

```agent
type Greeting {
  message: string
  language: string
  formal: bool
}

skill "greeter" {
  input {
    name: string
    language?: string          // optional field
    formal?: bool = false      // optional with default
  }
  output {
    greeting: Greeting
  }
  body {
    context { "Produce a greeting in the requested language and register." }
    step main {
      emit output
      context { "Fill the Greeting output with a suitable message." }
    }
  }
}
```

**Primitive types:** `string`, `int`, `float`, `bool`

**Compound types:**
- Array: `string[]`, `Finding[]`
- Map: `map<string, int>`
- Enum: `enum("critical", "high", "medium", "low")`

**Optional fields** use `?` suffix. **Defaults** use `= value` after the type.

Import types from packages:

```agent
import { Finding } from "@types/review"
```

---

## 4. Steps and Dependencies

Break a skill into named steps. Steps execute in the order their `requires`
dependencies are satisfied — the compiler detects cycles and rejects them.

```agent
skill "greeter" {
  input {
    name: string
    language?: string = "en"
  }
  output {
    greeting: Greeting
  }
  body {
    context { "You are a multilingual greeting assistant." }

    step detect_language {
      context { "Confirm the language to use. Default to English if unspecified." }
    }

    step compose {
      requires detect_language
      context { "Compose the greeting text in the detected language." }
    }

    step finalise {
      requires compose
      emit output
      context { "Populate the Greeting output." }
    }
  }
}
```

**Dependency forms:**

| Syntax | Meaning |
|---|---|
| `requires step_a` | single prerequisite |
| `requires a & b & c` | all of these must complete first |
| `requires a \| b \| c` | any one of these must complete first |
| `requires all_steps` | run after every other step |

**Inspect the dependency graph:**

```sh
skillspec deps greeter.agent
```

---

## 5. Context Management

Context blocks are the natural-language instructions passed to the model.
Every context block has an optional `priority` (0–100, higher = kept when
context is trimmed), a `when` condition, and a `decay` rate.

```agent
skill "greeter" {
  input {
    name: string
    language?: string = "en"
    formal?: bool = false
  }
  output {
    greeting: Greeting
  }
  body {
    // Always-present, high-priority instruction
    context(priority: 100) {
      """
      You are a multilingual greeting assistant.
      Produce warm, culturally appropriate greetings.
      """
    }

    // Only injected when input.formal is truthy
    context(priority: 80, when: input.formal) {
      """
      The user has requested a formal greeting.
      Use appropriate honorifics and register.
      """
    }

    // Fades from the context window over time
    context(priority: 60, decay: 0.1) {
      """
      Remember: the user's name is provided in the input.
      Use it naturally — do not repeat it excessively.
      """
    }

    step finalise {
      emit output
      context { "Produce the final Greeting output." }
    }
  }
}
```

### Lazy contexts

Large reference material (style guides, pattern catalogs) should be declared
as `lazy context` and loaded only when needed. This avoids bloating the
context window on every run.

```agent
    lazy context "style-guide" (priority: 40) {
      summary "Tone and vocabulary conventions for greetings."
      ref "./references/style-guide.md"
    }

    lazy context "language-phrases" (priority: 35) {
      summary "Common greeting phrases by language."
      index {
        section "romance" {
          summary "French, Spanish, Italian phrases."
          ref "./references/romance.md"
        }
        section "germanic" {
          summary "German, Dutch, Scandinavian phrases."
          ref "./references/germanic.md"
        }
      }
    }
```

Load a lazy context inside a step:

```agent
    step compose {
      requires detect_language
      load "language-phrases"
      context { "Use the loaded phrases to compose the greeting." }
    }
```

**Estimate token budget:**

```sh
skillspec budget greeter.agent
```

---

## 6. Adding Tests

Tests live in a `tests` block inside the skill. Each test declares inputs
(`given`), optional tool mocks (`mock`), and output assertions (`expect`).

```agent
skill "greeter" {
  input {
    name: string
    language?: string = "en"
    formal?: bool = false
  }
  output {
    greeting: Greeting
  }
  body {
    context { "Produce a warm greeting." }
    step main {
      emit output
      context { "Fill the Greeting output." }
    }
  }

  tests {
    test "basic english greeting" {
      given {
        name: "Alice"
      }
      expect {
        output.greeting.language: equals "en"
        output.greeting.message: contains "Alice"
      }
    }

    test "formal mode produces formal tone" {
      given {
        name: "Dr Smith"
        formal: true
      }
      expect {
        output.greeting.formal: equals true
        output.greeting.message: satisfies "Uses an honorific or formal address"
      }
      confidence 0.85
      runs 5
    }

    test "snapshot regression" {
      given { name: "Alice" }
      expect {
        output.greeting.message: matches "^(Hello|Hi|Good)"
      }
      snapshot "snapshots/greeting-alice.snap"
    }
  }
}
```

**Assertion types:**

| Form | When to use |
|---|---|
| `equals value` | exact match |
| `contains value` | substring / element presence |
| `matches "regex"` | regex match on string output |
| `resembles "description"` | LLM-judged structural similarity |
| `satisfies "criterion"` | LLM-judged semantic criterion |
| `>= value`, `<= value`, etc. | numeric comparison |
| `contains(where: .field == value)` | array element exists with condition |
| `all(where: .field == value)` | all array elements satisfy condition |
| `none(where: .field == value)` | no array element satisfies condition |

`confidence` sets the minimum pass rate across `runs` executions for
probabilistic assertions (`resembles`, `satisfies`).

List tests without executing them:

```sh
skillspec test greeter.agent
```

---

## 7. Prompt Directives

Prompt directives shape model behaviour. They go inside the `body` block,
alongside contexts and steps.

```agent
  body {
    persona {
      """
      You are a warm, culturally aware greeting specialist.
      You take language and register seriously.
      """
    }

    reasoning extended

    sampling {
      temperature: 0.6
      top_p: 0.9
    }

    format {
      style: json
      structure: output
    }

    reinforce every 3 steps {
      "Stay on task: produce a greeting, nothing else."
    }

    examples {
      example "casual english" {
        input: "name: Bob, language: en, formal: false"
        output: "message: Hey Bob!, language: en, formal: false"
        note: "Casual register uses first name only"
      }
    }

    // ... contexts and steps follow
  }
```

**Directive summary:**

| Directive | Values | Effect |
|---|---|---|
| `persona { ... }` | prose | Sets the model's role and character |
| `reasoning` | `none` / `standard` / `extended` | Controls chain-of-thought depth |
| `sampling { temperature: N top_p: N }` | floats | Sampling parameters |
| `format { style: X structure: Y }` | e.g. `json`, `markdown`; `output` | Output format hints |
| `reinforce every N steps { ... }` | interval + prose | Repeats a reminder at intervals |
| `reinforce on context_shift { ... }` | event + prose | Repeats on context transitions |
| `examples { example "name" { ... } }` | named examples | Few-shot examples |

---

## 8. Tools and Permissions

Declare which tools the skill requires (or can optionally use), and what
filesystem/network access it needs.

```agent
skill "greeter" {
  input { name: string }
  output { greeting: Greeting }

  tools {
    require Read
    require Bash
    optional mcp("translation-api") {
      translate(text: string, target_lang: string) -> string
      detect_language(text: string) -> string
    }
  }

  permissions {
    filesystem: read_write("phrases/**")
    network: outbound("api.translation.example.com")
    secrets: ["TRANSLATION_API_KEY"]
  }

  body {
    context { "Produce a greeting, translating if needed." }
    step main {
      emit output
      context { "Use the translation API if the language is not English." }
    }
  }
}
```

- `require` — the skill cannot run without this tool.
- `optional` — the skill degrades gracefully if unavailable.
- `mcp("name") { ... }` — declares an MCP server tool with explicit method
  signatures. Method signatures document expected input/output types but do
  not restrict the MCP server's actual schema.
- `filesystem:` modes: `read_only`, `read_write`, `write_only`
- `network:` modes: `outbound`, `inbound`, `none`
- `secrets:` lists environment variable names the skill may read.

---

## 9. Composition

### Using another skill inside a step

```agent
    step translate {
      use translation_skill(text: input.name, lang: input.language)
      context { "Use the translation result to compose the greeting." }
    }
```

### Mixins

Extract reusable step patterns into a `mixin`:

```agent
mixin audit_logging {
  step log_start {
    context { "Log that execution is beginning." }
  }
  step log_end {
    requires all_steps
    context { "Log the outcome." }
  }
}

skill "greeter" {
  include audit_logging
  // log_start and log_end are injected automatically

  input { name: string }
  output { greeting: Greeting }
  body {
    context { "Produce a greeting." }
    step main {
      requires log_start
      emit output
      context { "Fill the Greeting output." }
    }
  }
}
```

### Extending a skill

Inherit and override an existing skill:

```agent
skill "formal-greeter" extends "greeter" {
  body {
    persona {
      """
      You are a formal correspondence specialist.
      Always use honorifics and full names.
      """
    }
  }
}
```

---

## 10. Pipelines

A `pipeline` sequences skills into stages with explicit data flow. Stages
that share no dependencies run concurrently.

```agent
pipeline "full-greeting-flow" {
  input {
    name: string
    target_lang: string
  }
  output {
    delivered: bool
  }

  stage detect {
    use language_detector(text: input.name)
  }

  stage translate {
    use translator(text: input.name, lang: input.target_lang)
  }

  stage greet {
    requires detect & translate
    use greeter(name: translate.result, language: detect.result)
  }

  stage deliver {
    requires greet
    use notifier(message: greet.result)
  }

  on_error {
    use error_handler(message: "Pipeline failed")
  }

  timeout 5m
}
```

`requires` inside a stage uses the same `&` (all) and `|` (any) syntax as
steps. Reference a previous stage's output as `stage_name.result`.

---

## 11. Packaging

### Declare a package

Add a `package` block to your `.agent` file:

```agent
package {
  name: "@my-org/greetings"
  version: "1.0.0"
  description: "Multilingual greeting skills"
  exports: ["greeter", "Greeting"]
}

type Greeting {
  message: string
  language: string
  formal: bool
}

skill "greeter" {
  // ... as above
}
```

### Pack

```sh
skillspec pack greetings.agent
# Creates: my-org_greetings@1.0.0.skillpkg/
```

The `.skillpkg` directory contains `package.json`, a compiled `SKILL.md` for
each exported skill, and a `.types.json` for exported types.

### Install

```sh
# Install from a local .skillpkg directory
skillspec install my-org_greetings@1.0.0.skillpkg

# Install directly from an .agent file with a package declaration
skillspec install greetings.agent
```

Packages install into `.skillspec/packages/<name>@<version>/`.

### Import from a package

```agent
import { Greeting } from "@my-org/greetings"
```

---

## 12. Common Patterns

### Minimal skill (no ceremony)

A skill can be as simple as:

```agent
skill "hello" {
  context { "Greet the user warmly." }
}
```

Add `input`, `output`, steps, and tests only when you need them.

### Progressive context priority

Use distinct priorities so the runtime can make meaningful trim decisions.
A good spread:

| Priority | Purpose |
|---|---|
| 90–100 | Core task instructions, never drop |
| 70–89 | Important constraints and focus areas |
| 40–69 | Reference material, style guides |
| 10–39 | Nice-to-have background context |

Avoid assigning everything `priority: 100` — it defeats the system.

### Conditional contexts for optional inputs

Only inject context when the relevant input is present:

```agent
context(priority: 75, when: input.formal) {
  "Use formal register and honorifics."
}
```

### Keep eager context small

If total eager context exceeds ~500 tokens, move reference material to
`lazy context` and load it in the step that needs it.

### Pre/post contracts

Use `pre` and `post` to document and enforce invariants:

```agent
pre {
  assert input.name != "" message "Name is required"
}
post {
  assert output.greeting.message != "" message "Greeting must not be empty"
}
```

### `all_steps` for teardown

Use `requires all_steps` in a mixin step to guarantee it runs after
everything else — useful for logging, cleanup, or audit trails.

### Diff two versions

```sh
skillspec diff v1.agent v2.agent
```

Compare a compiled SKILL.md against the source:

```sh
skillspec diff greeter.agent greeter/SKILL.md --against-skillmd
```
