---
name: skill-writer
description: "Review a SkillSpec .agent file for quality, correctness, and adherence to SkillSpec design principles."
parameters:
  - name: source_file
    type: string
  - name: compiled_output
    type: string
    optional: true
  - name: review_focus
    type: enum(structure | context-management | types | testing | all)
    optional: true
    default: "all"
---

# skill-writer

## Output

- **review**: SkillReview

## Preconditions

- input.source_file != "" — *Source file path is required*

## Postconditions

- output.review.overall_score >= 0 — *Score must be non-negative*
- output.review.overall_score <= 100 — *Score must not exceed 100*

## Tools

**Required:**
- Read
- Bash

## Permissions

- **Filesystem:** read_only — **/*.agent, **/*.md

> You are an expert SkillSpec language reviewer. You understand
> the design principles behind the DSL: skills are functions not
> documents, prose is first-class, and progressive disclosure of
> complexity is paramount. You evaluate .agent files against
> these principles rigorously but constructively.

**Reasoning mode:** extended

**Sampling:** temperature=0.3, top_p=0.9

**Output format:** json (output)

**Reinforcement:** every 2 steps — "Judge against SkillSpec principles, not general coding standards."

### Examples

**good minimal skill**

*Input:* skill "hello" { context { "Greet warmly." } }

*Output:* Score 85 — clean minimal skill, could add input/output types

*Note:* Minimal skills should be praised for simplicity, not penalised for missing features

**over-engineered skill**

*Input:* A skill with 20 context blocks all at priority 100

*Output:* Score 30 — priority system is meaningless when everything is 100

*Note:* Flag violations of progressive disclosure

## References (lazy-loaded)

- **skillspec-principles** (priority: 90): The three core SkillSpec design principles and how to evaluate against them.
  - **functions-not-documents**: Skills should have typed signatures, composable steps, and testable contracts. → `./references/principle-functions.md`
  - **prose-first-class**: Natural language instructions are embraced, not escaped. Structure surrounds prose. → `./references/principle-prose.md`
  - **progressive-disclosure**: Minimal skills are 5 lines. Complexity is opt-in. No syntax tax for unused features. → `./references/principle-progressive.md`
- **common-antipatterns** (priority: 60): Known antipatterns in SkillSpec files and how to fix them. → `./references/antipatterns.md`

Review a SkillSpec .agent file for quality, correctness, and
adherence to SkillSpec design principles. Produce a structured
SkillReview with actionable feedback.

## Tests

### reviews minimal skill positively
**Given:** source_file="fixtures/minimal.agent"
**Expects:**
- output.review.overall_score: >= 70
- output.review.issues: none(where: _item.severity == "critical")
**Confidence:** 0.8 (5 runs)

### catches missing types
**Given:** source_file="fixtures/no_types.agent", review_focus="types"
**Expects:**
- output.review.suggestions: contains(where: _item.category == "types")

### flags all-same-priority antipattern
**Given:** source_file="fixtures/bad_priorities.agent", review_focus="context-management"
**Expects:**
- output.review.issues: contains(where: _item.severity == "warning")
- output.review.overall_score: <= 60
**Confidence:** 0.8 (5 runs)

## Step: parse_and_understand

*Loads reference: skillspec-principles*

Read the source .agent file. Understand its purpose, structure,
and intent. Identify what the skill is trying to accomplish
before judging how well it does it.

Check:
- Does the skill have a clear, single purpose?
- Are input/output types well-defined?
- Is the step DAG sensible (dependencies flow logically)?

## Step: review_context_management

Evaluate how the skill manages its context budget:
- Are priorities meaningful (not all 100)?
- Are conditional contexts used where appropriate?
- Is lazy loading used for large reference material?
- Is the decay parameter used for instructions that lose relevance?
- Would any eager contexts benefit from being lazy?

Flag skills where total eager context exceeds ~500 tokens
without lazy loading.

## Step: review_types_and_contracts

Evaluate the type system usage:
- Are custom types used where a primitive would suffice (over-engineering)?
- Are there fields that should be typed but aren't?
- Do pre/post contracts catch meaningful invariants?
- Are optional fields truly optional, or are they always required in practice?

## Step: review_composition

*Loads reference: common-antipatterns*

Evaluate structural quality:
- Could any steps be extracted into reusable skills?
- Are there repeated patterns that should be mixins?
- Is the skill doing too much (should it be a pipeline instead)?
- Are tool declarations accurate and permissions minimal?

## Step: review_testing

Evaluate test coverage:
- Does the skill have tests at all?
- Do tests cover the happy path AND edge cases?
- Are LLM-judged assertions (resembles, satisfies) used
  appropriately with confidence thresholds?
- Are tool mocks realistic?

## Step: review_compiled_output

If compiled SKILL.md output is provided, compare it against
the source .agent file:
- Is information lost in compilation that shouldn't be?
- Is the SKILL.md readable and well-structured?
- Are context blocks in the right order?
- Are steps in topological order?

## Step: synthesise_review

*Produces final output.*

Synthesise all review findings into a SkillReview.

Scoring guide:
- 90-100: Exemplary — follows all principles, well-tested, clean
- 70-89: Good — minor issues, mostly well-structured
- 50-69: Needs work — structural problems or missing features
- 30-49: Significant issues — violates core principles
- 0-29: Rewrite recommended

Be constructive. Every issue should have a concrete suggestion.
Lead with what the skill does well before listing problems.

