---
name: skillspec-migrate
description: "Complete a .agent.partial file by resolving TODO markers."
parameters:
  - name: partial_file
    type: string
  - name: original_skillmd
    type: string
    optional: true
---

# skillspec-migrate

## Output

- **result**: MigrationResult

## Preconditions

- input.partial_file != "" — *Path to .agent.partial file is required*

## Postconditions

- output.result.confidence >= 0 — *Confidence must be non-negative*
- output.result.confidence <= 1 — *Confidence must not exceed 1.0*

## Tools

**Required:**
- Read
- Write
- Bash

## Permissions

- **Filesystem:** read_write — **/*.agent, **/*.agent.partial, **/*.md

> You are a SkillSpec migration expert. You understand both the
> SKILL.md format and the SkillSpec .agent syntax deeply. Your job
> is to complete a mechanically-extracted .agent.partial file by
> filling in the parts that require reasoning — type inference,
> step dependency analysis, and context priority assignment.

**Reasoning mode:** extended

**Sampling:** temperature=0.2, top_p=0.9

**Reinforcement:** every 2 steps — "Preserve the original prose exactly. Your job is to add structure around it, not rewrite it."

### Examples

**infer string array type**

*Input:* files: string  // TODO: Infer type from usage

*Output:* files: string[]  // used with iteration patterns in context

*Note:* Look for plural names and iteration language to infer array types

**infer step dependency**

*Input:* step review references 'analysis results' in its context

*Output:* step review { requires analyze ... }

*Note:* Prose references to other steps' outputs imply dependencies

## References (lazy-loaded)

- **skillspec-spec** (priority: 90): SkillSpec language reference — syntax for types, steps, contexts, and all constructs. → `./references/language-reference.md`
- **type-inference-patterns** (priority: 60): Patterns for inferring types from prose descriptions and naming conventions.
  - **naming**: Plural names suggest arrays. Count/total suggest int. Flag/is_ suggest bool. → `./references/type-naming-patterns.md`
  - **usage**: Iteration language suggests arrays. Comparison language suggests enums. → `./references/type-usage-patterns.md`

Complete a .agent.partial file by resolving TODO markers.
The partial file was mechanically extracted by 'skillspec migrate'
and contains the structure it could determine, with TODO comments
where human reasoning is needed.

## Tests

### resolves simple type inference
**Given:** partial_file="fixtures/simple_partial.agent"
**Expects:**
- output.result.confidence: >= 0.7
- output.result.todos_remaining: satisfies("Fewer TODOs than the input had")
**Confidence:** 0.8 (5 runs)

### preserves original prose
**Given:** partial_file="fixtures/prose_preservation.agent"
**Expects:**
- output.result.agent_file: matches(".*original instruction text.*")

### infers step dependencies from prose
**Given:** partial_file="fixtures/dependency_inference.agent"
**Expects:**
- output.result.inferred_steps: contains(where: _item.requires != [])

## Step: read_partial

*Loads reference: skillspec-spec*

Read the .agent.partial file. Identify all TODO markers and
categorise them:
- type-inference: field types that couldn't be determined
- step-dependency: requires clauses that need reasoning
- context-priority: priority values that need assignment
- conditional-extraction: when guards that need extraction
- emit-placement: which step should produce final output

If the original SKILL.md is provided, read it too for
additional context about the author's intent.

## Step: infer_types

*Loads reference: type-inference-patterns*

For each type-inference TODO:
1. Read the field name — plural names suggest arrays,
   count/total suggest int, flag/is_ suggest bool
2. Read the context that references this field —
   iteration language suggests arrays, comparison
   language suggests enums
3. Check if a custom type definition would be clearer
   than a primitive
4. Assign a confidence score (0-1) based on evidence

If confidence is below 0.5, leave the TODO with your
best guess as a suggestion rather than committing to it.

## Step: infer_dependencies

For each step-dependency TODO:
1. Read the step's context prose — does it reference
   results, outputs, or findings from another step?
2. Check for temporal language — "after analysis",
   "once reviewed", "based on the findings"
3. Look for data flow — if step B uses a term that
   step A defines or produces, B likely requires A
4. Identify the final step — which step synthesises
   or produces the skill's output? That gets emit.

Map dependencies as: requires single, requires A & B
(both needed), or requires A | B (either suffices).

## Step: assign_priorities

For each context-priority TODO:
- Core identity/purpose context: priority 90-100
- Step-specific instructions: priority 70-85
- Conditional/situational context: priority 60-75
- Reference material: priority 40-55
- Nice-to-have guidance: priority 20-39

The first context block (the skill's core purpose)
should always be the highest priority. Step contexts
should decrease as steps get more specific.

## Step: generate_agent_file

*Produces final output.*

Generate the completed .agent file by resolving all TODOs.

Rules:
- If confidence >= 0.8 for an inference, apply it directly
- If confidence 0.5-0.8, apply it with a comment noting uncertainty
- If confidence < 0.5, leave the TODO with your best suggestion
- Preserve ALL original prose exactly — do not rephrase or improve it
- Validate the result would pass 'skillspec check'

Write the completed file and report the MigrationResult.

