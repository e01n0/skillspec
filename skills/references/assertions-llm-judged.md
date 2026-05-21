# LLM-Judged Assertions

These assertions require semantic judgment. Be strict.

## `resembles "description"`

Does the output structurally resemble what's described? Focus on content and structure, not exact wording.

Example: `output.summary: resembles "A list of security findings with severity ratings"`
- Pass: "Found 3 issues: SQL injection (critical), XSS (high), open redirect (medium)"
- Fail: "The code looks fine" (no list, no findings, no severity)
- Fail: "security, security, security" (matches keywords but not structure)

## `satisfies "criterion"`

Does the output meet a semantic criterion? This is the most subjective assertion type.

Example: `output.response: satisfies "Uses formal, professional tone"`
- Pass: "We have identified three areas requiring attention."
- Fail: "yo found some bugs lol"

## Evaluation rules

1. Read the assertion description carefully
2. Reason about whether the actual output matches before deciding
3. Ambiguous cases FAIL. If you're unsure, the assertion did not pass
4. Write your reasoning before your pass/fail decision
5. Do not give partial credit

## When used with confidence/runs

For tests with `confidence` and `runs`, evaluate the assertion independently each time. Don't anchor on your first judgment. Each evaluation should be a fresh assessment.
