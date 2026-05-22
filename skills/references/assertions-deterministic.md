# Evaluating Deterministic Assertions

Deterministic assertions are mechanical checks. They compare an actual value
against an expected value using a fixed rule. No judgment is needed -- the
result is always unambiguous.

## equals

```agent
output.status: equals "success"
output.count: equals 42
output.passed: equals true
```

**Evaluation:** Exact equality. Type-sensitive.
- String equality: `"success" == "success"` -> pass. `"Success" == "success"` -> FAIL (case-sensitive).
- Integer equality: `42 == 42` -> pass. `42 == 42.0` -> type mismatch, FAIL.
- Boolean equality: `true == true` -> pass.
- Array equality: element-by-element, same order, same length.

**Edge cases:**
- `null` / missing field equals nothing except another explicit null.
- Empty string `""` does not equal missing/null.
- `0` does not equal `false`.

## contains

```agent
output.summary: contains "security"
output.tags: contains "urgent"
```

**Evaluation:** Depends on the actual value's type.

For strings: substring check. `"Found 3 security issues"` contains `"security"` -> pass.
Case-sensitive. `"Security"` does not match `contains "security"`.

For arrays: element presence. `["urgent", "review"]` contains `"urgent"` -> pass.
Uses exact equality for each element comparison.

**Edge cases:**
- Empty string is contained in every string.
- Empty array contains nothing.
- `contains` on a non-string, non-array type is an error.

## matches

```agent
output.filename: matches "^src/.*\\.rs$"
output.version: matches "\\d+\\.\\d+\\.\\d+"
```

**Evaluation:** Regex match against the string value. The regex must match
somewhere in the string (not necessarily the full string) unless anchored
with `^` and `$`.

- `"src/main.rs"` matches `"^src/.*\\.rs$"` -> pass.
- `"src/main.rs"` matches `"main"` -> pass (substring match).
- `"src/main.rs"` matches `"^main"` -> FAIL (anchored at start).

**Edge cases:**
- Invalid regex is an error (report as assertion error, not pass/fail).
- Matching against a non-string type is an error.
- Backslashes in the regex string must be escaped: `"\\d+"` not `"\d+"`.

## Numeric Comparisons

```agent
output.score: >= 70
output.score: <= 100
output.score: > 0
output.score: < 50
output.score: == 42
output.score: != 0
```

**Evaluation:** Standard numeric comparison. Both sides must be numeric
(int or float).

- `75 >= 70` -> pass.
- `70 >= 70` -> pass (inclusive).
- `69 >= 70` -> FAIL.

**Type coercion:** `int` and `float` can be compared. `42 == 42.0` passes
for numeric comparison (unlike `equals`, which is type-strict).

**Edge cases:**
- Comparing a non-numeric value is an error.
- `NaN` comparisons always fail (NaN != NaN).
- Negative zero equals positive zero.

## Compound Deterministic Assertions

When multiple assertions apply to the same test case, ALL must pass for the
test to pass. There is no short-circuit -- evaluate all assertions and report
each result individually.

```agent
expect {
  output.score: >= 0
  output.score: <= 100
  output.status: equals "complete"
  output.summary: contains "review"
}
```

All four are evaluated. If `score` is 75, `status` is "complete", and
`summary` is "Code review complete", all pass. If `summary` is "Analysis
done", the `contains "review"` assertion fails while the others pass.

## Reporting

For each deterministic assertion, report:
- `path`: the field path (e.g. `output.score`)
- `assertion`: the assertion text (e.g. `>= 70`)
- `passed`: true/false
- `actual`: the actual value (e.g. `65`)
- `expected`: the expected value/condition (e.g. `>= 70`)
- `error`: null if passed, explanation if failed (e.g. `65 < 70`)

Keep error messages factual: state what was expected and what was found.
No hedging, no "almost passed", no partial credit.
