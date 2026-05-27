# Compilation: .agent to SKILL.md Sections

When `skillspec build` compiles a `.agent` file, the body of the SKILL.md is
built from steps, prompt directives, and supporting blocks. Each maps to a
specific section structure.

## Section Ordering in Compiled Output

The compiled SKILL.md follows this order:

1. YAML frontmatter (name, description, parameters)
2. Persona blockquote (if present)
3. Skill-level context blocks (ordered by priority descending)
4. Step sections (in topological dependency order)
5. `## Output` section (from output block)
6. `## Tools` section (from tools block)
7. `## Permissions` section (from permissions block)
8. `## Tests` section (from tests block)

## Step Sections

Each step compiles to a `## Step: step_name` section.

```agent
step analyze {
  requires parse
  load "style-guide"
  context(priority: important) {
    "Run analysis on the parsed files."
  }
  context(priority: supplementary) {
    "Cross-reference against the style guide."
  }
}
```

Compiles to:

```markdown
## Step: analyze

*Requires: parse*

Run analysis on the parsed files.

Cross-reference against the style guide.
```

- `requires` becomes an italic annotation: `*Requires: parse*`
- `requires a & b` becomes `*Requires: a, b (all)*`
- `requires a | b` becomes `*Requires: a, b (any)*`
- `when` guard becomes: `*When: input.focus == "types"*`
- `use` call becomes: `*Delegates to: skill_name(args)*`
- `emit output` becomes: `*Produces: output*`
- `load` is not rendered directly (the lazy context content is inlined)
- Context blocks within the step are rendered as prose paragraphs,
  ordered by priority descending
- `let` bindings are not rendered (compile-time only)

## Topological Order

Steps appear in dependency order. If A requires B, B appears first.
Steps with no dependency relationship are ordered by their source position.

For the DAG: `parse -> analyze -> synthesise` (with analyze also requiring validate):
```
## Step: parse
## Step: validate
## Step: analyze       (requires parse, validate)
## Step: synthesise    (requires analyze)
```

## Persona

```agent
persona {
  """
  You are a senior code reviewer.
  """
}
```

Compiles to a blockquote before any context:

```markdown
> You are a senior code reviewer.
```

## Prompt Directives

Most prompt directives compile to metadata annotations at the top of the
SKILL.md body, after the persona:

| Directive | Compiled Form |
|-----------|---------------|
| `reasoning extended` | `*Reasoning: extended*` |
| `sampling { temperature: 0.3 }` | `*Temperature: 0.3, Top-P: 0.9*` |
| `format { style: json }` | `*Format: json (structure: output)*` |
| `reinforce every 3 steps { "X" }` | Inserted as a reminder paragraph after every 3rd step section |
| `examples { ... }` | `## Examples` section with each example as a subsection |

## Output Section

```agent
output {
  report: ReviewReport
  score: int
}
```

Compiles to:

```markdown
## Output

| Field | Type | Required |
|-------|------|----------|
| report | ReviewReport | yes |
| score | integer | yes |
```

## Tools Section

```agent
tools {
  require Read
  require Bash
  optional mcp("slack") { send_message(...) -> void }
}
```

Compiles to:

```markdown
## Tools

**Required:** Read, Bash

**Optional:** slack (MCP)
- `send_message(channel: string, text: string) -> void`
```

## Permissions Section

```agent
permissions {
  filesystem: read_write("src/**")
  network: outbound("api.github.com")
  secrets: ["GITHUB_TOKEN"]
}
```

Compiles to:

```markdown
## Permissions

- **Filesystem:** read_write — `src/**`
- **Network:** outbound — `api.github.com`
- **Secrets:** GITHUB_TOKEN
```

## Tests Section

Each test compiles to a subsection under `## Tests`:

```markdown
## Tests

### Test: basic case
**Given:** source_file = "fixtures/minimal.agent"
**Expect:**
- output.score >= 70
- output.issues: none(where: .severity == "critical")
**Confidence:** 0.8 over 5 runs
```

## Backporting Changes

| SKILL.md Change | .agent Location |
|----------------|-----------------|
| New `## Step: X` section | New `step X { }` in body (flag: dependencies unknown) |
| Removed `## Step: X` | Remove `step X { }` (flag: check requires chains) |
| Text added under `## Step: X` | New `context { }` block in `step X` |
| Text modified under `## Step: X` | Edit matching `context { }` in `step X` |
| Blockquote changed | Edit `persona { }` block |
| `## Output` table changed | Edit `output { }` block |
| `## Tools` changed | Edit `tools { }` block |
| `## Tests` changed | Edit `tests { }` block |
| New text between steps | New skill-level `context { }` in body |
