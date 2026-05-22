# Principle: Prose Is First-Class

Natural language instructions are embraced in SkillSpec, not escaped. The DSL
adds structure AROUND prose, not instead of it.

## What This Means

### Context Blocks Are Instructions

The content of a context block is natural language that the model reads
directly. It is not a comment, not metadata, not a template. It IS the
instruction:

```agent
context(priority: 90) {
  """
  Review the code for security vulnerabilities. Focus on injection
  attacks, authentication bypass, and data exposure. For each finding,
  explain the risk and suggest a fix.
  """
}
```

This prose is the skill's core value. The type system, step DAG, and priority
annotations are scaffolding that helps the prose reach the model in the right
order, at the right time, with the right emphasis.

### Structure Wraps Prose

The DSL provides containers for prose, not replacements:

| Container | Purpose |
|-----------|---------|
| `context { }` | Holds an instruction or piece of guidance |
| `persona { }` | Holds the model's role description |
| `reinforce { }` | Holds a reminder that repeats |
| `examples { }` | Holds input/output pairs with optional notes |

Each container adds metadata (priority, when-guard, decay, trigger interval)
that controls HOW and WHEN the prose is delivered. The prose itself stays
readable natural language.

### Good Prose, Well Structured

A well-written skill reads like a clear briefing document when you strip away
the syntax:

1. Here is who you are (persona)
2. Here is your core mission (high-priority context)
3. Here is what to do first (step 1 context)
4. Here is what to do next (step 2 context)
5. Here are edge cases to watch for (conditional contexts)
6. Here is reference material if you need it (lazy contexts)

The structure makes this progression explicit and machine-verifiable, but
the content remains human-authored prose.

### The Prose Preservation Rule

When migrating, backporting, or refactoring a skill, PRESERVE THE ORIGINAL
PROSE EXACTLY. Restructuring a skill means changing which containers hold
which prose and how they relate. It does NOT mean rewriting the instructions.

Reasons:
- The author chose specific words for a reason
- Subtle phrasing ("focus specifically on" vs "consider") affects model behavior
- Rewriting during migration introduces untested changes
- The original SKILL.md was presumably working; changing prose risks regressions

## How to Evaluate

| Quality | Indicator |
|---------|-----------|
| Strong | Context blocks contain clear, specific instructions |
| Strong | Prose reads naturally without the DSL syntax |
| Strong | Each context block has a single clear purpose |
| Strong | Conditional contexts use `when` guards for truly conditional instructions |
| Weak | Context blocks that are single vague sentences ("Do the thing.") |
| Weak | Prose that repeats the same instruction in multiple contexts |
| Anti | Over-fragmentation: 20 tiny context blocks that should be 3-4 cohesive ones |
| Anti | Pseudo-code in prose: "IF x THEN do A ELSE do B" (use `when` guards instead) |
| Anti | Template language: "Replace {FIELD} with the value" (use typed I/O instead) |

## The Over-Structuring Antipattern

The most common violation of this principle: breaking natural prose into so
many tiny context blocks that the flow is destroyed.

Bad:
```agent
context(priority: 90) { "Review the code." }
context(priority: 89) { "Look for security issues." }
context(priority: 88) { "Look for performance issues." }
context(priority: 87) { "Look for maintainability issues." }
context(priority: 86) { "For each finding, explain the risk." }
context(priority: 85) { "For each finding, suggest a fix." }
```

Good:
```agent
context(priority: 90) {
  """
  Review the code for security, performance, and maintainability
  issues. For each finding, explain the risk and suggest a fix.
  """
}
```

Use separate context blocks when the content has DIFFERENT delivery
requirements (different priorities, different conditions, different decay
rates). Do not split prose just because it covers multiple points.

## The Under-Structuring Antipattern

The opposite extreme: a single massive context block with everything. This
defeats the priority system, prevents conditional delivery, and makes the
skill impossible to compose or test incrementally.

If a context block exceeds ~200 tokens and contains instructions for multiple
distinct phases of work, split it along phase boundaries and assign
appropriate priorities.
