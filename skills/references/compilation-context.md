# Compilation: Context Ordering in SKILL.md

Context blocks are the core content of a compiled SKILL.md. Their ordering
determines what the model sees first (and retains longest under trimming).

## Ordering Rules

### Rule 1: Skill-level before step-level

Contexts declared directly in the `body` (not inside any step) appear before
any step sections in the compiled output.

```agent
body {
  context(priority: 100) { "Skill-level instruction." }      // appears first
  context(priority: 80)  { "Secondary instruction." }         // appears second

  step analyze {
    context(priority: 95) { "Step-level instruction." }       // appears under ## Step: analyze
  }
}
```

Even though the step context has priority 95 (higher than the skill-level 80),
it appears later because step contexts are scoped to their step section.

### Rule 2: Within each scope, priority descending

Within skill-level contexts: highest priority first.
Within a single step's contexts: highest priority first.

```agent
body {
  context(priority: 60) { "Lower priority." }
  context(priority: 100) { "Highest priority." }
  context(priority: 80) { "Medium priority." }
}
```

Compiled order:
1. "Highest priority." (100)
2. "Medium priority." (80)
3. "Lower priority." (60)

Source order does not matter. Priority is the sole ordering criterion.

### Rule 3: Conditional contexts get a prefix

Contexts with a `when` guard are rendered with a condition prefix:

```agent
context(priority: 75, when: input.formal) {
  "Use formal register and honorifics."
}
```

Compiles to:

```markdown
**When input.formal:**
Use formal register and honorifics.
```

The prefix makes the condition visible to the model so it can decide whether
the instruction applies. The context is always included in the compiled output
(the runtime evaluates the condition, but the compiled SKILL.md includes it
for transparency).

### Rule 4: Decay contexts get an annotation

```agent
context(priority: 60, decay: 0.1) {
  "Remember the user's name."
}
```

Compiles to:

```markdown
*[Decays at rate 0.1]*
Remember the user's name.
```

Decay is a hint to the runtime that this context becomes less relevant over
time. It does not affect ordering in the compiled output.

## Lazy Context in Compiled Output

Lazy contexts are NOT inlined at their declaration point. Instead:

### Simple ref

```agent
lazy context "style-guide" (priority: 40) {
  summary "Team style guide and conventions."
  ref "./references/style-guide.md"
}
```

Compiles to a summary line at the position determined by its priority:

```markdown
*[Lazy: style-guide — Team style guide and conventions. Load on demand.]*
```

### Indexed lazy context

```agent
lazy context "catalog" (priority: 35) {
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

Compiles to:

```markdown
*[Lazy: catalog — Error pattern catalog. Sections: security, performance. Load on demand.]*
```

### When a step loads a lazy context

If a step has `load "style-guide"`, the compiled step section includes a note:

```markdown
## Step: deep_review

*Requires: analyze*
*Loads: style-guide*

Cross-reference findings against the style guide.
```

The actual content of the lazy context is NOT inlined into the SKILL.md.
The runtime is responsible for loading and injecting it when the step executes.

## Priority Assignment Guidelines

For backporting: if you need to assign a priority to new content, use these
ranges as a guide:

| Priority Range | Typical Content |
|---------------|-----------------|
| 90-100 | Core task identity, primary instruction |
| 70-89 | Important step instructions, key constraints |
| 40-69 | Reference material, supplementary guidance |
| 10-39 | Nice-to-have background, low-priority reminders |

The first context block (the skill's "what you do" statement) should always
be the highest priority. Priorities should generally decrease as content
becomes more specific or supplementary.

## Backporting Context Changes

When text changes appear in a SKILL.md and you need to map them back:

1. **Determine the scope**: Is it before any `## Step:` header? Then it is
   a skill-level context. Is it under `## Step: X`? Then it belongs to step X.

2. **Determine position**: Its position relative to other contexts in the same
   scope tells you its approximate priority. Earlier = higher priority.

3. **Check for condition prefix**: If the text starts with `**When ...**:`,
   it maps to a context with a `when` guard.

4. **Check for lazy annotation**: If the text is `*[Lazy: ...]*`, it maps to
   a lazy context declaration, not an eager context.

5. **New text between existing contexts**: Create a new context block with a
   priority that slots it into the right position relative to its neighbours.
