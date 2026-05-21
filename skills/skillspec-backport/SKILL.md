---
name: skillspec-backport
description: "Reconcile changes from a modified SKILL.md back into the .agent source file."
parameters:
  - name: agent_file
    type: string
  - name: skillmd_file
    type: string
  - name: changeset
    type: string
    optional: true
---

# skillspec-backport

## Output

- **result**: BackportResult

## Preconditions

- input.agent_file != "" — *Path to .agent source file is required*
- input.skillmd_file != "" — *Path to modified SKILL.md is required*

## Tools

**Required:**
- Read
- Write
- Bash

## Permissions

- **Filesystem:** read_write — **/*.agent, **/*.md, **/*.backport

> You are a SkillSpec backport specialist. You understand the
> mapping between compiled SKILL.md output and .agent source
> structure. Your job is to take changes made to a deployed
> SKILL.md and map them back to the correct locations in the
> .agent source file — preserving structure, types, and
> contracts that don't exist in the markdown format.

**Reasoning mode:** extended

**Sampling:** temperature=0.2, top_p=0.9

**Reinforcement:** every 2 steps — "Map changes to the most specific .agent location. A new paragraph in a step section maps to that step's context block, not to a skill-level context."

## References (lazy-loaded)

- **skillspec-compilation-rules** (priority: 85): How SkillSpec compiles .agent to SKILL.md — the mapping rules for reverse engineering changes.
  - **frontmatter**: YAML frontmatter maps to skill name, input fields, and description extraction. → `./references/compilation-frontmatter.md`
  - **sections**: ## headers map to steps (topo-sorted), Output/Tools/Permissions/Tests map to skill blocks. → `./references/compilation-sections.md`
  - **context-ordering**: Context blocks are ordered by priority descending. Skill-level before step-level. → `./references/compilation-context.md`

Reconcile changes from a modified SKILL.md back into the
.agent source file. The SKILL.md was compiled from the .agent
file and then edited downstream. Your job is to figure out
WHERE each change belongs in the structured source.

## Tests

### maps new frontmatter field to input
**Given:** agent_file="fixtures/simple.agent", skillmd_file="fixtures/simple_with_new_param.md"
**Expects:**
- output.result.applied: contains(where: _item.change_type == "added")
- output.result.conflicts: satisfies("No conflicts for simple additions")

### flags ambiguous section as conflict
**Given:** agent_file="fixtures/multi_step.agent", skillmd_file="fixtures/multi_step_ambiguous_change.md"
**Expects:**
- output.result.conflicts: contains(where: _item.reason == "ambiguous")
**Confidence:** 0.8 (5 runs)

### preserves types and contracts
**Given:** agent_file="fixtures/typed.agent", skillmd_file="fixtures/typed_modified.md"
**Expects:**
- output.result.updated_file: matches(".*pre \{.*")
- output.result.updated_file: matches(".*output \{.*")

## Step: generate_diff

If a changeset file is provided, read it directly.

Otherwise, generate the diff by reading both files:
1. Read the .agent source file
2. Read the modified SKILL.md
3. Compare them structurally: identify which sections,
   parameters, and prose blocks differ

Categorise each change as: added section, modified text,
removed section, or modified frontmatter.

## Step: classify_changes

*Loads reference: skillspec-compilation-rules*

For each change in the diff, determine where it maps in
the .agent source:

Mapping rules:
- New frontmatter parameter → new field in input block
- Modified description → first line of highest-priority context changed
- New text under '## Step: X' → new context block in step X
- Modified text under '## Step: X' → edit step X's context
- New '## Step: Y' section → new step Y (dependencies unknown — flag as conflict)
- Removed '## Step: X' → remove step X (flag as conflict if other steps require it)
- Changes in '## Tools' → changes in tools block
- Changes in '## Tests' → changes in tests block
- New text between skill-level contexts → new context block at skill level

For each mapping:
- If it maps cleanly to ONE location → mark as 'apply'
- If it could go in multiple places → mark as 'conflict'
  with both options described

## Step: apply_changes

Apply all non-conflicted changes to the .agent source:

1. Read the current .agent file
2. For each 'apply' change, make the modification at the
   identified location
3. For each 'conflict', insert a CONFLICT marker:
   // CONFLICT: [description]
   // Option A: [location/change]
   // Option B: [location/change]
   // Resolve manually and remove this marker.
4. Validate the result with 'skillspec check'

If the modified file fails type checking, investigate:
- Did a type change in the SKILL.md break a contract?
- Did a removed step break a requires chain?
Report these as additional conflicts rather than producing
an invalid file.

## Step: produce_result

*Produces final output.*

Write the updated .agent file and produce the BackportResult.
Include:
- All changes that were applied cleanly
- All conflicts that need manual resolution
- A human-readable summary of what changed and what needs attention

