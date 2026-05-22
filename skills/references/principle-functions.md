# Principle: Skills Are Functions, Not Documents

A skill is a callable unit with a typed signature, not a blob of instructions.
This principle is the foundation of SkillSpec's design.

## What This Means

### Typed Input/Output

A skill should declare what it takes and what it produces, with explicit types:

```agent
skill "analyze" {
  input {
    files: string[]
    severity: enum("high", "medium", "low")
  }
  output {
    findings: Finding[]
    score: int
  }
}
```

This lets the compiler verify that callers pass the right arguments and
consumers use the right fields. It also lets other skills compose with this
one via `use analyze(files: input.files, severity: "high")`.

Without typed I/O, a skill is a black box. You cannot compose black boxes
reliably. You cannot test black boxes mechanically. You cannot refactor
black boxes with confidence.

### Composable Steps

Steps are the internal "functions" of a skill. Each step has:
- A name (for reference)
- Dependencies (what it needs before it can run)
- A clear responsibility (via its context blocks)

Steps that can run independently SHOULD be independent (no false dependencies).
Steps that produce final output SHOULD `emit output` to make the data flow
explicit.

Bad: a single step that does everything.
Bad: a linear chain A->B->C->D when B and C are independent.
Good: B and C run in parallel after A, D waits for both.

### Testable Contracts

Pre/post assertions are contracts, not documentation:

```agent
pre {
  assert input.files != [] message "No files to analyze"
}
post {
  assert output.score >= 0 message "Score must be non-negative"
  assert output.score <= 100 message "Score must not exceed 100"
}
```

These express invariants that the compiler can check statically (for some
cases) and the test runner can verify at execution time.

Good contracts catch real mistakes:
- `assert input.files != []` prevents a meaningless run
- `assert output.score >= 0` catches sign errors in scoring

Bad contracts are tautological or meaningless:
- `assert true message "always passes"` (worthless)
- `assert output.result != "" message "must exist"` (too vague to be useful)

### Clear Signatures for Verification

When a skill has typed I/O and contracts, the compiler can verify:
- All input fields are used somewhere (no dead parameters)
- Output types match what the emit step produces
- Pre-conditions reference valid input fields
- Post-conditions reference valid output fields
- `use` calls pass arguments matching the target skill's input types

This is compile-time verification. It catches errors before any LLM runs.

## How to Evaluate

When reviewing a skill against this principle:

| Quality | Indicator |
|---------|-----------|
| Strong | Typed I/O with meaningful types, not just `string` for everything |
| Strong | Pre/post assertions that catch real edge cases |
| Strong | Steps with clear dependency DAG, parallelism where possible |
| Strong | `use` calls that compose with other typed skills |
| Weak | No input/output block (acceptable for trivial skills) |
| Weak | All fields typed as `string` when richer types exist |
| Weak | Empty pre/post blocks (remove them if you have nothing to assert) |
| Weak | Single monolithic step that does everything |
| Anti | Pre/post assertions that are tautological |
| Anti | Circular step dependencies (compiler rejects these) |
| Anti | Type definitions that wrap a single string field |

## The Escape Hatch

Not every skill needs full ceremony. A 3-line skill with just a context block
is valid and sometimes correct. The principle is: WHEN you need types, steps,
and contracts, the language supports them cleanly. You should not reach for
them when a simple context block would suffice.
