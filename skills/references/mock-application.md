# Applying Mock Declarations in Tests

Mock declarations substitute tool behavior during test execution. They let
you test skill logic without real tool access.

## Standard Mock: Fixed Response

```agent
mock github {
  pr_diff(repo: "my/repo", pr: 42) -> "diff content here"
}
```

**How to apply:**

1. When the skill would call `github.pr_diff`, check if the arguments match
   the mock declaration.
2. If arguments match: return the mocked response immediately. Do not
   simulate any actual tool call.
3. If arguments do NOT match: this is an unexpected call. Report it as a
   test warning. Return the mocked response anyway (best-effort), or fail
   the test if strict mode is enabled.

**Argument matching:**
- Match by exact value: `repo: "my/repo"` matches only `"my/repo"`.
- If the mock omits an argument, it matches any value for that argument.
- If the mock specifies all arguments, only exact matches trigger it.

**Multiple methods on the same tool:**

```agent
mock github {
  pr_diff(repo: "my/repo", pr: 42) -> "diff content"
  post_comment(repo: "my/repo", pr: 42, body: "LGTM") -> "ok"
}
```

Each method is matched independently. A call to `pr_diff` uses the first
mock; a call to `post_comment` uses the second.

**Unmocked methods:** If the skill calls a method on a mocked tool that has
no mock declaration for that method, treat it as an unexpected call. The
skill should not be calling methods that the test does not anticipate.

## unavailable

```agent
mock slack { unavailable }
```

**How to apply:**

1. When the skill would call any method on `slack`, simulate the tool not
   being available.
2. The skill should handle this gracefully if `slack` is declared as
   `optional` in the tools block.
3. If the skill declared `slack` as `require` in tools, this tests the
   failure path. The skill should either error cleanly or the test should
   expect a failure.

**What "unavailable" means:**
- The tool does not respond at all.
- It is not "returns an error" -- it is "does not exist in the environment".
- Skills with `optional` tools should have fallback behavior.
- Skills with `require` tools should fail with a clear error.

**Use case:** Testing degraded-mode behavior. Does the skill still produce
useful output when an optional tool is missing?

## failing

```agent
mock database { failing "connection refused" }
```

**How to apply:**

1. When the skill would call any method on `database`, simulate a tool
   failure with the specified error message.
2. Every call to the tool returns the error. There is no "works on retry".
3. The skill's error handling should be exercised.

**Difference from unavailable:**
- `unavailable`: tool does not exist. The skill should not even try to call it.
- `failing`: tool exists and accepts calls, but every call returns an error.

**What to check:**
- Does the skill catch the error?
- Does it produce a meaningful error message (not a raw stack trace)?
- Does it fail gracefully or crash?
- If the skill has an `on_error` handler (in pipelines), is it invoked?

**Use case:** Testing error handling and resilience.

## slow

```agent
mock api { slow "5s" }
```

**How to apply:**

1. When the skill would call any method on `api`, simulate a delayed
   response. The call eventually succeeds but takes the specified duration.
2. Parse the duration: "5s" = 5 seconds, "500ms" = 500 milliseconds,
   "1m" = 1 minute.
3. During simulation, account for the delay when evaluating:
   - Does the skill timeout? (if it has a timeout declaration)
   - Does the skill handle slow responses gracefully?
   - Is the total execution time affected?

**What to check:**
- If the pipeline has a `timeout`, does the slow mock cause it to trigger?
- Does the skill retry? If so, does the total time exceed expectations?
- Is the eventual response handled correctly despite the delay?

**Use case:** Testing timeout behavior and latency tolerance.

## Combining Mocks in One Test

A single test can mock multiple tools differently:

```agent
test "handles mixed tool states" {
  given { files: ["a.py"] }
  mock github {
    pr_diff(repo: "org/repo", pr: 1) -> "+added line\n-removed line"
  }
  mock slack { unavailable }
  mock metrics { slow "3s" }
  expect {
    output.result.findings: contains(where: .severity != "")
  }
}
```

Each tool's mock is independent. The skill should handle all three
conditions simultaneously.

## Mock Scope

Mocks apply only within the test case that declares them. They do not
leak between tests. Each test starts with a clean environment.

## What Cannot Be Mocked

- The LLM itself (the model's reasoning is the skill's execution)
- Built-in tools (Read, Bash, Edit, Write) when used for file I/O within
  the test fixture setup -- but they CAN be mocked if the skill under test
  uses them as tools
- Pre/post assertions (these always run against actual output)

## Verifying Mock Usage

After test execution, check:
- Were all declared mocks used? Unused mocks suggest the test's `given`
  inputs do not exercise the expected code path.
- Were there unexpected tool calls? Calls to unmocked tools suggest the
  skill has dependencies the test did not anticipate.

Report both as warnings in the test result, not as failures (unless strict
mode is configured).
