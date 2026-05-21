# Deterministic Assertions

These assertions are mechanical comparisons. No judgment needed.

## `equals value`

Exact equality. Strings compared character-by-character, numbers by value, bools by identity.

- `output.status: equals "success"` - passes only if status is exactly "success"
- `output.count: equals 5` - passes only if count is exactly 5

## `contains value`

For strings: substring match. For arrays: element presence.

- `output.message: contains "error"` - passes if "error" appears anywhere in the string
- `output.tags: contains "urgent"` - passes if the array includes "urgent"

## `matches "regex"`

Regex match against string output. Uses full-match semantics (anchored).

- `output.id: matches "^[a-f0-9]{8}$"` - passes if id is an 8-char hex string

## Numeric comparisons

`>= value`, `<= value`, `> value`, `< value`, `== value`, `!= value`

- `output.score: >= 70` - passes if score is 70 or above
- `output.errors: == 0` - passes if exactly zero errors

## Evaluation rules

1. Extract the actual value from the simulated output at the specified path
2. Apply the comparison operator
3. Pass or fail. No partial credit, no rounding, no "close enough"
