# User Guide

## Installation

```sh
git clone git@github.com:e01n0/skillspec.git
cd skillspec
cargo install --path .
```

---

## Your first skill

```sh
skillspec init greeter
```

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

```sh
skillspec check greeter.agent   # ✓ greeter.agent: no errors
skillspec build greeter.agent   # ✓ greeter.agent → greeter/SKILL.md
skillspec fmt greeter.agent     # canonical formatting
```

---

## Types

Named types go at the top of the file:

```agent
type Greeting {
  message: string
  language: string
  formal: bool
}

skill "greeter" {
  input {
    name: string
    language?: string          // optional
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

Primitives: `string`, `int`, `float`, `bool`

Compounds:
- `string[]`, `Finding[]` (arrays)
- `map<string, int>` (maps)
- `enum("critical", "high", "medium", "low")` (closed enum)

`?` makes a field optional. `= value` sets a default.

```agent
import { Finding } from "@types/review"
```

---

## Steps and dependencies

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

| Syntax | Meaning |
|---|---|
| `requires step_a` | single prerequisite |
| `requires a & b & c` | all must complete |
| `requires a \| b \| c` | any one suffices |
| `requires all_steps` | runs last |

```sh
skillspec deps greeter.agent
```

---

## Context

Every context block takes an optional `priority` (0 to 100), `when` guard, and `decay` rate. Higher priority survives trimming.

```agent
context(priority: 100) {
  """
  You are a multilingual greeting assistant.
  Produce warm, culturally appropriate greetings.
  """
}

context(priority: 80, when: input.formal) {
  """
  The user has requested a formal greeting.
  Use appropriate honorifics and register.
  """
}

context(priority: 60, decay: 0.1) {
  """
  Remember: the user's name is provided in the input.
  Use it naturally. Do not repeat it excessively.
  """
}
```

### Lazy contexts

For large reference material. Stays on disk until a step pulls it in with `load`.

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

```agent
step compose {
  requires detect_language
  load "language-phrases"
  context { "Use the loaded phrases to compose the greeting." }
}
```

```sh
skillspec budget greeter.agent
```

### Priority ranges

| Priority | Use for |
|---|---|
| 90-100 | Core instructions, never drop |
| 70-89 | Constraints and focus areas |
| 40-69 | Reference material, style guides |
| 10-39 | Nice-to-have background |

Don't assign everything `priority: 100`. It defeats the system.

---

## Tests

```agent
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
```

| Assertion | What it checks |
|---|---|
| `equals value` | exact match |
| `contains value` | substring or element |
| `matches "regex"` | regex on string |
| `resembles "desc"` | LLM-judged structural similarity |
| `satisfies "criterion"` | LLM-judged semantic check |
| `>= value`, `<= value`, etc. | numeric comparison |
| `contains(where: .field == value)` | any array element matches |
| `all(where: .field == value)` | every element matches |
| `none(where: .field == value)` | no element matches |

`confidence` (0 to 1) sets the minimum pass rate across `runs` for probabilistic assertions.

```sh
skillspec test greeter.agent   # lists tests, doesn't run them
```

---

## Prompt directives

All optional. Go inside `body`.

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
}
```

| Directive | Effect |
|---|---|
| `persona { ... }` | Model's role and character |
| `reasoning none/standard/extended` | Chain-of-thought depth |
| `sampling { temperature: N top_p: N }` | Sampling parameters |
| `format { style: X structure: Y }` | Output format hints |
| `reinforce every N steps { ... }` | Repeated reminder |
| `reinforce on context_shift { ... }` | Reminder on context transitions |
| `examples { ... }` | Few-shot examples |

---

## Tools and permissions

```agent
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
```

- `require`: skill can't run without it.
- `optional`: degrades gracefully.
- `mcp("name") { ... }`: MCP server with method signatures. Signatures are documentation, not enforcement.
- `filesystem:` `read_only`, `read_write`, `write_only` with glob patterns.
- `network:` `outbound`, `inbound`, `none`.
- `secrets:` env vars the skill may read.

---

## Composition

### Calling another skill

```agent
step translate {
  use translation_skill(text: input.name, lang: input.language)
  context { "Use the translation result to compose the greeting." }
}
```

### Mixins

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

### Extending

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

## Pipelines

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

Same `&` / `|` dependency syntax as steps. Reference a stage's output as `stage_name.result`. Stages without `requires` run concurrently.

---

## Packaging

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
  // ...
}
```

```sh
skillspec pack greetings.agent                      # → my-org_greetings@1.0.0.skillpkg/
skillspec install my-org_greetings@1.0.0.skillpkg   # from .skillpkg
skillspec install greetings.agent                    # from source
```

Packages install to `.skillspec/packages/<name>@<version>/`. The `.skillpkg` contains `package.json`, compiled `SKILL.md` per skill, and `.types.json` for exported types.

```agent
import { Greeting } from "@my-org/greetings"
```

---

## Contracts

```agent
pre {
  assert input.name != "" message "Name is required"
}
post {
  assert output.greeting.message != "" message "Greeting must not be empty"
}
```

---

## Diff

```sh
skillspec diff v1.agent v2.agent
skillspec diff greeter.agent greeter/SKILL.md --against-skillmd
```
