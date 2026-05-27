# Bundled Skills

These `.agent` files are skills that run inside your agent runtime (Claude Code, Cursor, etc.), not CLI commands.

| Skill | Purpose | When to use |
|-------|---------|-------------|
| `skill-writer.agent` | Review `.agent` files for quality | After writing a new skill — catches antipatterns, missing contexts, naming issues |
| `skillspec-migrate.agent` | Complete `.agent.partial` files from migration | After `skillspec migrate` produces a partial — the skill reads the source directory and fills in SkillSpec constructs |
| `skillspec-backport.agent` | Map SKILL.md edits back to `.agent` source | When someone edits the deployed SKILL.md directly and you need to reconcile |
| `skillspec-test.agent` | Run test blocks against a live model | After `skillspec test --prepare` generates a test execution SKILL.md |
| `skillspec-optimize.agent` | Drive the SkillOpt optimisation loop | Orchestrates `skillspec optimize --step` calls for iterative improvement |

## Structure

Each skill has a corresponding directory with its compiled `SKILL.md` and any reference documents:

```
skills/
├── skill-writer.agent          # source
├── skill-writer/
│   └── SKILL.md                # compiled output
├── skillspec-migrate.agent
├── skillspec-migrate/
│   └── ...
```

The `.agent` file is the source of truth. The directory contains the compiled output and reference docs the skill loads at runtime.

## Using a bundled skill

Build and deploy to your runtime:

```sh
skillspec build skills/skill-writer.agent --to claude
```

Or use directly by pointing your agent at the compiled SKILL.md in the skill's directory.
