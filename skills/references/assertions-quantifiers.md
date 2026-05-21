# Quantifier Assertions

Quantifiers iterate over arrays and test predicates against elements.

## `contains(where: .field == value)`

At least one element in the array satisfies the predicate.

```
output.findings: contains(where: .severity == "critical")
```

Passes if any element in `findings` has `severity` equal to `"critical"`.

## `all(where: .field == value)`

Every element in the array satisfies the predicate.

```
output.findings: all(where: .severity != "info")
```

Passes only if no element has severity "info".

## `none(where: .field == value)`

No element in the array satisfies the predicate.

```
output.findings: none(where: .severity == "critical")
```

Passes only if zero elements have severity "critical".

## Dot syntax

`.field` is shorthand for the current array element's field. In `contains(where: .severity == "critical")`, `.severity` refers to each element's `severity` field as the quantifier iterates.

## Evaluation

1. Identify the array at the specified output path
2. Iterate over each element
3. Test the predicate against each element
4. Apply the quantifier logic (any/all/none)
5. Empty arrays: `contains` fails, `all` passes (vacuous truth), `none` passes
