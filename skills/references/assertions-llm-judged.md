# Evaluating LLM-Judged Assertions

LLM-judged assertions require semantic reasoning to determine pass/fail.
They are inherently non-deterministic, which is why they should be paired
with `confidence` and `runs` in test declarations.

## resembles

```agent
output.summary: resembles "A list of security findings with severity ratings"
output.report: resembles "A structured review with scores and recommendations"
```

**What it checks:** Does the actual output STRUCTURALLY resemble what the
description says? Focus on structure and content type, not exact wording.

**Evaluation process:**

1. Read the description carefully. Identify the structural claims:
   - "A list of" -> expects multiple items, not a single paragraph
   - "security findings" -> expects content about security, not performance
   - "with severity ratings" -> expects each finding to have a severity

2. Read the actual output. Check each structural claim:
   - Is there a list (multiple items)? Yes/no.
   - Are they about security? Yes/no.
   - Do they have severity ratings? Yes/no.

3. ALL structural claims must be satisfied for a pass.

**Examples:**

Assertion: `resembles "A list of security findings with severity ratings"`

- PASS: `"Found 3 issues: (1) SQL injection [critical], (2) XSS [high], (3) weak password policy [medium]"`
  Reasoning: Multiple items, about security, each has a severity.

- PASS: `"Security findings:\n- Critical: Unsanitized input in login handler\n- High: Missing CSRF tokens"`
  Reasoning: List format, security topic, severity labels present.

- FAIL: `"The code looks good overall with minor style issues."`
  Reasoning: Not about security findings. No list. No severity ratings.

- FAIL: `"SQL injection vulnerability found in the login handler."`
  Reasoning: Single finding, not a list. Has no severity rating.

**Strictness:** Be strict. "Resembles" means the structure is recognizably
the same kind of thing. It does NOT mean "vaguely related to the topic".

## satisfies

```agent
output.summary: satisfies "Provides actionable feedback with specific line references"
output.message: satisfies "Uses a professional and constructive tone"
```

**What it checks:** Does the actual output SEMANTICALLY meet the stated
criterion? This is a judgment call about meaning and quality.

**Evaluation process:**

1. Parse the criterion into testable sub-criteria:
   - "Provides actionable feedback" -> Is the feedback specific enough to act on?
   - "with specific line references" -> Are file/line numbers mentioned?

2. Evaluate each sub-criterion against the actual output.

3. ALL sub-criteria must be met for a pass.

**Examples:**

Assertion: `satisfies "Provides actionable feedback with specific line references"`

- PASS: `"Line 42 in auth.py: The SQL query uses string concatenation. Use parameterized queries instead."`
  Reasoning: Actionable (says what to do instead). Has a line reference (line 42).

- FAIL: `"There are some security issues that should be fixed."`
  Reasoning: Not actionable (no specifics). No line references.

- FAIL: `"Line 42 has an issue."`
  Reasoning: Has a line reference but is not actionable (does not say what the issue is or how to fix it).

Assertion: `satisfies "Uses a professional and constructive tone"`

- PASS: `"Consider using parameterized queries here to prevent SQL injection."`
  Reasoning: Professional ("consider"), constructive (suggests alternative).

- FAIL: `"This code is terrible and whoever wrote it should feel bad."`
  Reasoning: Neither professional nor constructive.

**Strictness:** Be strict on `satisfies`. Ambiguous cases should FAIL.
The criterion is a bar to clear, not a suggestion. If you are unsure whether
the output meets the criterion, it does not.

## Reasoning Protocol

For both `resembles` and `satisfies`, follow this protocol:

1. **State the criterion** -- what exactly needs to be true?
2. **Break it down** -- what are the individual claims or requirements?
3. **Evaluate each** -- does the actual output meet each one? Cite evidence.
4. **Decide** -- pass only if ALL sub-criteria are met.
5. **Explain** -- state your reasoning before the verdict, not after.

This protocol matters for reproducibility. When running multiple times
(for confidence thresholds), consistent reasoning reduces variance.

## Interaction with confidence/runs

Because LLM-judged assertions are non-deterministic, tests using them should
declare `confidence` and `runs`:

```agent
test "quality check" {
  given { ... }
  expect {
    output.summary: satisfies "Provides actionable feedback"
  }
  confidence 0.8
  runs 5
}
```

This means: run the simulation 5 times. The assertion must pass in at least
4 out of 5 runs (0.8 * 5 = 4). This provides statistical confidence that
the skill reliably meets the criterion, not just occasionally.

When evaluating across multiple runs, vary your reasoning slightly (consider
different interpretations, weight edge cases differently) to get genuine
variance. Do not copy-paste the same reasoning 5 times.

## Mixed Assertions

A test can mix deterministic and LLM-judged assertions:

```agent
expect {
  output.score: >= 70              // deterministic
  output.summary: satisfies "..."  // LLM-judged
}
```

The deterministic assertions must pass on every run. The LLM-judged assertions
are subject to the confidence threshold. If a deterministic assertion fails on
any run, that run fails regardless of the LLM-judged results.
