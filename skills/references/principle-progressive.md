# Principle: Progressive Disclosure of Complexity

A valid SkillSpec skill can be 3 lines. You pay syntax cost only for the
features you actually use. The language never forces ceremony.

## What This Means

### The Minimal Skill

```agent
skill "hello" {
  context { "Greet the user warmly." }
}
```

This is a complete, valid, compilable skill. It has no input, no output, no
steps, no tests, no tools, no permissions. It compiles to a SKILL.md that
tells the model to greet warmly.

### Growing a Skill

Features are added incrementally, each one earning its syntax cost:

```agent
// Level 1: Just prose
skill "greet" {
  context { "Greet the user warmly." }
}

// Level 2: Add typed I/O when you need a contract
skill "greet" {
  input { name: string }
  output { greeting: string }
  body {
    context { "Greet the user by name." }
    step main {
      emit output
      context { "Produce the greeting." }
    }
  }
}

// Level 3: Add steps when you need sequencing
// Level 4: Add tests when you need verification
// Level 5: Add tools/permissions when you need access control
// Level 6: Add pipelines when you need multi-skill orchestration
```

Each level adds syntax only for the capability it introduces. A Level 1
skill never needs to declare empty `input {}`, empty `tests {}`, or empty
`tools {}` blocks.

### The Syntax Tax Test

Before adding a construct to a skill, ask: "What does this buy me?"

| Construct | Worth it when... | Not worth it when... |
|-----------|------------------|---------------------|
| `input { }` | You need typed parameters or pre-conditions | The skill takes no meaningful input |
| `output { }` | You need typed results or post-conditions | The skill's output is freeform text |
| `step X { }` | You have distinct phases with dependencies | The skill does one thing linearly |
| `pre { }` | You have real invariants to enforce | You would write `assert true` |
| `post { }` | You have real output constraints | You would write a tautology |
| `tests { }` | You need regression or behavior verification | The skill is trivial/experimental |
| `tools { }` | You need specific tool access | The skill uses no tools |
| `permissions { }` | You need to restrict access | No access restrictions needed |
| `lazy context` | Large reference material (>~500 tokens) | A short paragraph of guidance |
| `pipeline` | Multi-skill workflows with error handling | A single skill would suffice |
| `orchestration` | Multi-agent coordination with shared state | A pipeline would suffice |

## How to Evaluate

### Good Progressive Disclosure

- A simple skill that is actually simple (3-20 lines)
- A complex skill that grew its complexity for clear reasons
- Each block earns its existence by enabling something specific

### Antipattern: Boilerplate-Heavy Skill

```agent
skill "simple-task" {
  input { }            // empty
  output { }           // empty
  tools { }            // empty
  permissions { }      // empty
  pre { }              // empty
  post { }             // empty
  body {
    context { "Do the thing." }
  }
  tests { }            // empty
}
```

Every empty block is dead weight. The skill should be:

```agent
skill "simple-task" {
  context { "Do the thing." }
}
```

### Antipattern: Premature Complexity

Using pipelines or orchestrations when a single skill with steps would work:

```agent
// Over-engineered: pipeline for what is really one skill
pipeline "greet-flow" {
  stage detect { use language_detector(text: input.name) }
  stage greet  {
    requires detect
    use greeter(name: input.name, lang: detect.result)
  }
}
```

If `language_detector` is not a real standalone skill that exists and is
reused elsewhere, this should be two steps inside a single skill.

### Antipattern: Over-Typed Trivial Skills

```agent
type GreetingConfig {
  warmth_level: int     // single field wrapper
}

type GreetingOutput {
  text: string          // single field wrapper
}

skill "greet" {
  input { config: GreetingConfig }
  output { result: GreetingOutput }
  ...
}
```

A named type that wraps a single primitive field is almost never justified.
Use the primitive directly: `input { warmth_level: int }`.

Named types earn their existence when they have 2+ fields AND are referenced
in multiple places.

## The Goldilocks Zone

Most useful skills land at Level 2-3:
- Typed I/O (contract for callers)
- 2-5 steps (clear workflow)
- A few context blocks (instructions at different priorities)
- Maybe a test or two (regression protection)

Going below this for a real production skill risks fragility.
Going above this without clear justification risks over-engineering.
