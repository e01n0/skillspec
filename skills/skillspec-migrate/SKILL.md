---
name: skillspec-migrate
description: "Complete a .agent.partial file by resolving TODO markers."
parameters:
  - name: partial_file
    type: string
    optional: true
  - name: original_skillmd
    type: string
    optional: true
  - name: source_dir
    type: string
    optional: true
---

# skillspec-migrate

## Output

- **result**: MigrationResult

## Preconditions

- input.partial_file != "" || input.source_dir != "" — *Either partial_file or source_dir is required*

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

**single file migration**

*Input:* partial_file='code-review/code-review.agent.partial'

*Output:* One .agent file with resolved TODOs, typed fields, step dependencies, and context priorities

*Note:* No source_dir — work only from the partial content

**directory migration with references**

*Input:* partial_file='advanced-patterns/advanced-patterns.agent.partial' source_dir='advanced-patterns/'

*Output:* One .agent file with lazy context refs pointing at ./reference/*.md and ./EXAMPLES.md

*Note:* Use source_dir to discover and read reference files. Classify each as lazy context ref, import, or additional construct.

**batch migration — detect orchestration**

*Input:* partial_file='playbook/playbook.agent.partial' source_dir='playbook/'

*Output:* A skill .agent file plus a pipeline or orchestration .agent file if the skill routes to or chains siblings

*Note:* Read sibling SKILL.md files via parent_dir pointer. If this skill orchestrates others, produce an orchestration construct. If it defines a sequential chain, produce a pipeline.

**full tree migration — source_dir only**

*Input:* source_dir='.assistant/skills/'

*Output:* One .agent file per skill subdirectory, plus pipeline and orchestration files for cross-skill relationships

*Note:* No partial_file needed. Explore the entire tree: find all SKILL.md files, grep for .md cross-references, read shared directories. Produce the complete set of .agent files in one pass.

## References (lazy-loaded)

- **skillspec-spec** (priority: important): SkillSpec language reference — syntax for types, steps, contexts, and all constructs. → `./references/language-reference.md`
- **type-inference-patterns** (priority: supplementary): Patterns for inferring types from prose descriptions and naming conventions.
  - **naming**: Plural names suggest arrays. Count/total suggest int. Flag/is_ suggest bool. → `./references/type-naming-patterns.md`
  - **usage**: Iteration language suggests arrays. Comparison language suggests enums. → `./references/type-usage-patterns.md`

> **CRITICAL:** Complete a .agent.partial file by resolving TODO markers.
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
- output.result.agent_files: matches(".*original instruction text.*")

### infers step dependencies from prose
**Given:** partial_file="fixtures/dependency_inference.agent"
**Expects:**
- output.result.inferred_steps: contains(where: _item.requires != [])

### directory single skill with refs
**Given:** partial_file="fixtures/directory_single_skill.agent.partial"
**Expects:**
- output.result.directory_analysis.relationship: == "single-skill"
- output.result.agent_files: matches(".*lazy context.*ref.*")

### directory detects pipeline
**Given:** partial_file="fixtures/directory_pipeline.agent.partial"
**Expects:**
- output.result.directory_analysis.relationship: == "pipeline"
- output.result.agent_files: matches(".*pipeline.*")

### directory detects orchestration
**Given:** partial_file="fixtures/directory_orchestration.agent.partial"
**Expects:**
- output.result.directory_analysis.relationship: == "orchestration"
- output.result.agent_files: matches(".*orchestration.*")

## Step: read_partial

*Loads reference: skillspec-spec*

> **CRITICAL:** Two modes of operation:

**Mode A — single partial:** If partial_file is provided,
read it and identify all TODO markers. Categorise them:
- type-inference: field types that couldn't be determined
- step-dependency: requires clauses that need reasoning
- context-priority: priority values that need assignment
- conditional-extraction: when guards that need extraction
- emit-placement: which step should produce final output

If source_dir is also provided, use Bash to list the
directory contents. Read any file that looks relevant.

**Mode B — full directory tree:** If only source_dir is
provided (no partial_file), explore the entire directory:

1. Run `find <source_dir> -name 'SKILL.md'` to discover
   all skill directories
2. Read each SKILL.md to understand the full skill set
3. Grep all SKILL.md files for `.md` references to find
   cross-references between skills and to shared files:
   `grep -rn '\.md' <source_dir>/*/SKILL.md`
4. Read referenced .md files to understand shared context
5. You will produce .agent files for each skill, plus any
   orchestration or pipeline constructs the structure needs

If the original SKILL.md is provided, read it too for
additional context about the author's intent.

## Step: analyze_directory_context

*Loads reference: skillspec-spec*

> **CRITICAL:** If source_dir is provided, explore the directory:

1. Grep all SKILL.md files for `.md` references to build
   the cross-reference graph:
   `grep -rhn '\.md' <source_dir>/*/SKILL.md`
   This catches references to sibling skills, shared
   reference files, and documentation.

2. Identify non-skill directories (dirs without SKILL.md)
   that are referenced by skills — these are shared
   libraries (e.g. shared-reference/).

3. Look for orchestration signals:
   - Skills referencing other skills by @name
   - Routing tables or pipeline sequences (-> arrows)
   - Hub-and-spoke patterns (one skill references many)

4. **Classify the relationship** — single skill with docs,
   pipeline of chained skills, multi-agent orchestration,
   or independent skills sharing a directory.

5. **Map to constructs** — using the language reference,
   decide which SkillSpec constructs to produce. You are
   not limited to a single skill. Use whatever combination
   best represents the folder's intent.

In Mode B (full tree), you are producing .agent files for
ALL skills in the tree, not just one. Plan the full set
of constructs before generating any files.

If source_dir is not provided, fall back to analyzing
comment blocks in the partial. If neither is available,
skip this step.

## Step: infer_types

*Loads reference: type-inference-patterns*

> **IMPORTANT:** For each type-inference TODO:
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

> **IMPORTANT:** For each step-dependency TODO:
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

> **IMPORTANT:** For each context-priority TODO:
- Core identity/purpose context: priority critical (max 2 per skill)
- Step-specific instructions: priority important
- Conditional/situational context: priority important or supplementary
- Reference material: priority supplementary
- Nice-to-have guidance: priority optional

The first context block (the skill's core purpose)
should always be the highest priority. Step contexts
should decrease as steps get more specific.

## Step: generate_agent_file

*Produces final output.*

*Loads reference: skillspec-spec*

> **IMPORTANT:** Generate the completed .agent file(s) by resolving all TODOs.

Rules:
- If confidence >= 0.8 for an inference, apply it directly
- If confidence 0.5-0.8, apply it with a comment noting uncertainty
- If confidence < 0.5, leave the TODO with your best suggestion
- Preserve ALL original prose exactly — do not rephrase or improve it

If directory analysis was performed:
- Use the language reference to produce the right constructs
- Reference docs become lazy context blocks with ref paths
- Shared type/mixin files become imports
- Multiple skills may produce pipeline or orchestration blocks
- The output is not limited to a single skill — use whatever
  combination of constructs best represents the folder

In Mode B (full tree), generate one .agent file per skill
directory, plus any pipeline or orchestration files that
tie them together. Write each file into its skill directory.
List all generated files in agent_files.

After writing each .agent file, validate it:

1. Run `skillspec check <path>` via Bash
2. If it fails (non-zero exit), read the error, fix the
   issue in the .agent file, and re-run the check
3. Repeat up to 3 times per file
4. If still failing, leave as-is and set confidence below 0.5

Then run `skillspec build <path>` on files that pass check
to verify they compile to valid SKILL.md output.

Report all generated files in agent_files.

