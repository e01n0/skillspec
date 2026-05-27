# Common Antipatterns in .agent Files

## 1. All-Same-Priority

**Pattern:** Every context block has `priority: critical` (or no priority, defaulting
to the same value).

```agent
// Bad: priorities are meaningless
context(priority: critical) { "Review the code." }
context(priority: critical) { "Focus on security." }
context(priority: critical) { "Check performance." }
context(priority: critical) { "Verify error handling." }
```

**Why it is bad:** The priority system exists so the runtime can make intelligent
trim decisions under context pressure. When everything is `critical`, the runtime
cannot trim anything and the compiled output annotations lose meaning. You lose
control over what gets dropped first and what the agent emphasises.

**Fix:** Differentiate across the four priority flags. Core identity as `critical` (max 2).
Key instructions as `important`. Reference material as `supplementary`. Nice-to-haves as `optional`.

## 2. Eager-Everything

**Pattern:** All context is loaded eagerly, even large reference material.

```agent
// Bad: 2000 tokens of patterns loaded on every run
context(priority: supplementary) {
  """
  [500 lines of error patterns, security checklists, and style rules
  that are only needed if specific issues are found]
  """
}
```

**Why it is bad:** Wastes context window budget. If total eager context exceeds
~500 tokens, the model may lose focus on the actual task. Reference material
that is only sometimes relevant should be lazy.

**Fix:** Move large reference material to `lazy context` with `ref` pointing
to an external file. Load it in the step that needs it via `load`.

## 3. Empty Contracts

**Pattern:** Pre/post blocks with no meaningful assertions.

```agent
// Bad: asserts nothing useful
pre { }
post {
  assert output.result != "" message "Must have result"
}
```

**Why it is bad:** Empty pre/post blocks are dead weight (violates progressive
disclosure). The `!= ""` assertion is almost always too weak to catch real
issues. It passes for garbage output.

**Fix:** Either write meaningful assertions or remove the block entirely.
Good assertions check domain-specific invariants:
```agent
post {
  assert output.score >= 0 message "Score must be non-negative"
  assert output.score <= 100 message "Score must not exceed 100"
  assert output.findings != [] message "Must report at least one finding"
}
```

## 4. Linear-Chain Dependencies

**Pattern:** Steps form a strict A->B->C->D chain when some are independent.

```agent
// Bad: analyze and lint could run in parallel
step parse    { }
step analyze  { requires parse }
step lint     { requires analyze }   // lint does not actually need analyze
step report   { requires lint }
```

**Why it is bad:** Artificial serialization. If `analyze` and `lint` both
depend only on `parse`, they should declare `requires parse` independently.
The final step should require both: `requires analyze & lint`.

**Fix:**
```agent
step parse    { }
step analyze  { requires parse }
step lint     { requires parse }
step report   { requires analyze & lint }
```

## 5. Type Over-Engineering

**Pattern:** Custom types wrapping a single primitive, or deeply nested types
for simple data.

```agent
// Bad: wrapper type for one string
type FileName {
  value: string
}
type FileList {
  items: FileName[]
}
```

**Why it is bad:** Adds indirection without value. Named types should exist
when they have 2+ fields AND are used in multiple places. Single-field wrappers
make the skill harder to read and compose.

**Fix:** Use the primitive directly: `files: string[]`.

## 6. Test-Without-Confidence

**Pattern:** LLM-judged assertions (`resembles`, `satisfies`) without
`confidence` and `runs` declarations.

```agent
// Bad: non-deterministic assertion with no statistical backing
test "checks quality" {
  given { source_file: "test.agent" }
  expect {
    output.summary: satisfies "Provides actionable feedback"
  }
}
```

**Why it is bad:** `satisfies` involves LLM judgment, which is non-deterministic.
A single run might pass or fail by chance. Without `confidence` and `runs`,
you have no statistical reliability.

**Fix:**
```agent
test "checks quality" {
  given { source_file: "test.agent" }
  expect {
    output.summary: satisfies "Provides actionable feedback"
  }
  confidence 0.8
  runs 5
}
```

## 7. Context Fragmentation

**Pattern:** Breaking coherent prose into many tiny context blocks.

```agent
// Bad: 6 blocks that should be 1-2
context(priority: important) { "Review the code." }
context(priority: important) { "Look for bugs." }
context(priority: important) { "Look for security issues." }
context(priority: important) { "Look for performance issues." }
context(priority: important) { "Explain each finding." }
context(priority: important) { "Suggest fixes." }
```

**Why it is bad:** Destroys prose flow. Each context block adds overhead.
Priorities 85-90 are effectively the same (the runtime will not meaningfully
differentiate a 1-point difference). See the prose-first-class principle.

**Fix:** Group related instructions into cohesive blocks:
```agent
context(priority: important) {
  """
  Review the code for bugs, security issues, and performance problems.
  For each finding, explain the risk and suggest a fix.
  """
}
```

## 8. Phantom Dependencies

**Pattern:** Steps that declare `requires` on a step whose output they
do not actually use.

```agent
step validate { requires parse }
step format   { requires validate }  // format does not use validate's work
```

**Why it is bad:** Creates unnecessary serialization and obscures the real
dependency graph. If `format` only needs `parse` output, it should say so.

**Fix:** Trace actual data flow. A step should require exactly the steps
whose outputs it reads or whose side effects it depends on.

## 9. Unused Lazy Contexts

**Pattern:** Declaring lazy contexts that no step ever loads.

```agent
lazy context "security-patterns" (priority: supplementary) {
  summary "Known security vulnerabilities."
  ref "./references/security.md"
}
// No step ever calls: load "security-patterns"
```

**Why it is bad:** Dead code. The lazy context will never be loaded. It
wastes a summary line in the compiled output and confuses readers about
what the skill actually uses.

**Fix:** Either add a `load` call in the step that needs it, or remove the
lazy context declaration.

## 10. God Step

**Pattern:** A single step that does everything the skill needs.

```agent
step do_everything {
  emit output
  context(priority: critical) {
    """
    [300 lines of instructions covering parsing, analysis,
    validation, reporting, and output formatting]
    """
  }
}
```

**Why it is bad:** No composability, no parallelism, untestable in parts.
If any sub-task fails, the entire step fails. Cannot reuse individual phases
in other skills.

**Fix:** Break into steps with clear responsibilities. Each step should have
one primary job. Use `requires` to express the actual dependency graph.
