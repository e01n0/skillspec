# Evaluating Quantifier Assertions

Quantifier assertions test predicates across array elements. They check
whether some, all, or no elements satisfy a condition.

## contains(where: ...)

```agent
output.findings: contains(where: .severity == "critical")
output.steps: contains(where: .has_emit == true)
output.results: contains(where: .score >= 80)
```

**Semantics:** At least ONE element in the array satisfies the predicate.
Equivalent to "exists" / "any".

**Evaluation:**

1. Verify the field is an array. If not, report an error.
2. Iterate through each element.
3. For each element, evaluate the predicate (substituting `.field` with the
   element's field value).
4. If ANY element satisfies the predicate, PASS.
5. If NO element satisfies the predicate, FAIL.

**Example:**

Array: `[{severity: "high", file: "a.py"}, {severity: "critical", file: "b.py"}]`
Assertion: `contains(where: .severity == "critical")`

- Element 0: `.severity == "critical"` -> `"high" == "critical"` -> false
- Element 1: `.severity == "critical"` -> `"critical" == "critical"` -> true
- Result: PASS (at least one element matched)

**Edge case:** Empty array always FAILs for `contains` (no element can satisfy
any predicate in an empty collection).

## all(where: ...)

```agent
output.findings: all(where: .severity != "")
output.scores: all(where: . >= 0)
output.steps: all(where: .confidence > 0.5)
```

**Semantics:** EVERY element in the array satisfies the predicate.

**Evaluation:**

1. Verify the field is an array. If not, report an error.
2. Iterate through each element.
3. For each element, evaluate the predicate.
4. If ALL elements satisfy the predicate, PASS.
5. If ANY element fails the predicate, FAIL. Report which element(s) failed.

**Example:**

Array: `[{score: 85}, {score: 72}, {score: 91}]`
Assertion: `all(where: .score >= 70)`

- Element 0: `.score >= 70` -> `85 >= 70` -> true
- Element 1: `.score >= 70` -> `72 >= 70` -> true
- Element 2: `.score >= 70` -> `91 >= 70` -> true
- Result: PASS (all elements matched)

**Edge case:** Empty array always PASSes for `all` (vacuous truth -- there
are no elements to violate the predicate).

## none(where: ...)

```agent
output.findings: none(where: .severity == "critical")
output.errors: none(where: .unhandled == true)
output.steps: none(where: .confidence < 0.3)
```

**Semantics:** NO element in the array satisfies the predicate. The logical
negation of `contains(where: ...)`.

**Evaluation:**

1. Verify the field is an array. If not, report an error.
2. Iterate through each element.
3. For each element, evaluate the predicate.
4. If NO element satisfies the predicate, PASS.
5. If ANY element satisfies the predicate, FAIL. Report which element(s)
   matched when none should have.

**Example:**

Array: `[{severity: "high"}, {severity: "medium"}, {severity: "low"}]`
Assertion: `none(where: .severity == "critical")`

- Element 0: `"high" == "critical"` -> false
- Element 1: `"medium" == "critical"` -> false
- Element 2: `"low" == "critical"` -> false
- Result: PASS (no elements matched)

**Edge case:** Empty array always PASSes for `none` (no elements to match).

## The Leading-Dot Syntax

Inside `where:` expressions, `.field` is shorthand for the current element's
field. The full form would be `_item.field`, but the shorthand is preferred.

| Shorthand | Full Form | Meaning |
|-----------|-----------|---------|
| `.severity` | `_item.severity` | Current element's severity field |
| `.score` | `_item.score` | Current element's score field |
| `.` | `_item` | The current element itself (for primitive arrays) |

For arrays of primitives (e.g. `string[]`), use bare `.`:

```agent
output.tags: contains(where: . == "urgent")
output.scores: all(where: . >= 0)
```

For arrays of named types or objects, use `.field`:

```agent
output.findings: contains(where: .severity == "critical")
output.findings: all(where: .file != "")
```

## Nested Field Access

The dot syntax supports nested fields:

```agent
output.results: contains(where: .detail.category == "security")
```

This accesses `element.detail.category` for each element.

## Compound Predicates

Predicates can use logical operators:

```agent
output.findings: contains(where: .severity == "critical" && .file != "")
output.findings: none(where: .severity == "critical" || .severity == "high")
output.steps: all(where: .confidence >= 0.5 && .has_emit == true)
```

Evaluate compound predicates using standard boolean logic:
- `&&`: both sides must be true
- `||`: either side must be true
- Comparison operators have higher precedence than logical operators

## Reporting

For quantifier assertions, report:
- `path`: the array field path (e.g. `output.findings`)
- `assertion`: the full assertion (e.g. `contains(where: .severity == "critical")`)
- `passed`: true/false
- `actual`: a summary of the array (e.g. "3 elements, severities: [high, medium, low]")
- `expected`: the quantifier condition
- `error`: for failures, identify which elements violated the expectation
  - `contains` failure: "0 of N elements matched .severity == 'critical'"
  - `all` failure: "elements at index 1, 3 failed .score >= 70 (actual: 65, 42)"
  - `none` failure: "element at index 2 matched .severity == 'critical'"
