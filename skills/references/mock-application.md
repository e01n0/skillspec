# Mock Application

Mocks substitute tool behaviour during test execution.

## Response mock

```
mock github {
  pr_diff(repo: "my/repo", pr: 42) -> "diff content here"
}
```

When the skill calls `github.pr_diff` with matching arguments, return the specified response instead of calling the real tool.

## Unavailable mock

```
mock slack { unavailable }
```

Simulates the tool not being present. The skill should degrade gracefully if the tool is declared as `optional`. If the tool is `required`, the skill should fail or report the missing tool.

## Failing mock

```
mock database { failing "connection refused" }
```

Simulates a tool error. The skill receives the error message as if the tool call failed.

## Slow mock

```
mock slow_api { slow "5s" }
```

Simulates latency. Useful for testing timeout handling. The response eventually succeeds after the specified delay.

## Argument matching

Mock arguments must match exactly. If the skill calls `pr_diff(repo: "other/repo", pr: 42)` but the mock specifies `repo: "my/repo"`, the mock does not match. When no mock matches a tool call, treat it as if the tool returned a generic empty response.
