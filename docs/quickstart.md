# Quickstart: Zero to Compiled Skill in 5 Minutes

## Install

```sh
git clone git@github.com:e01n0/skillspec.git
cd skillspec
cargo install --path .
```

Verify: `skillspec --version` should print `skillspec 0.1.0`.

## Create a skill

```sh
skillspec init code-helper
```

This creates `code-helper.agent`:

```skillspec
skill "code-helper" {
  input {
    query: string
  }
  output {
    result: string
  }
  body {
    context { "You are a helpful assistant." }
    step main {
      emit output
      context { "Answer the query provided in the input." }
    }
  }
}
```

## Make it yours

Open `code-helper.agent` and replace the contents with something real:

```skillspec
type CodeReview {
    file: string
    issues: string[]
    score: int
}

skill "code-helper" {
    input {
        files: string[]
        language?: string
    }

    output {
        review: CodeReview
    }

    pre {
        assert input.files != [] message "Provide at least one file"
    }

    body {
        persona {
            """
            You are a pragmatic code reviewer. You focus on bugs and
            security issues, not style nitpicks.
            """
        }

        reasoning standard

        context(priority: 100) {
            """
            Review the provided files for bugs, security issues,
            and correctness problems.
            """
        }

        context(priority: 70, when: input.language) {
            """
            The code is written in the specified language.
            Apply language-specific best practices.
            """
        }

        step analyse {
            context(priority: 90) {
                """
                Read each file carefully. Identify concrete issues
                with line references. Do not flag style preferences.
                """
            }
        }

        step report {
            requires analyse
            emit output

            context {
                """
                Produce the CodeReview output. Score 0-100 where
                100 means no issues found.
                """
            }
        }
    }
}
```

## Validate

```sh
skillspec check code-helper.agent
```

```
✓ code-helper.agent: no errors
```

If you have a type error or broken dependency, the compiler tells you exactly what's wrong and where.

## Compile to SKILL.md

```sh
skillspec build code-helper.agent
```

This creates `code-helper/SKILL.md` — a standard skill file that Claude Code, Codex, Cursor, Gemini CLI, or any agent runtime can consume directly.

## Explore what you built

```sh
# Token budget estimate
skillspec budget code-helper.agent

# Step dependency graph
skillspec deps code-helper.agent

# Format with canonical style
skillspec fmt code-helper.agent

# List test cases (if you add any)
skillspec test code-helper.agent
```

## What's next

| Want to... | Read |
|-----------|------|
| Add lazy context loading | [User Guide: Context Management](guide.md#5-context-management) |
| Declare tool dependencies | [User Guide: Tools and Permissions](guide.md#8-tools-and-permissions) |
| Compose skills into pipelines | [User Guide: Pipelines](guide.md#10-pipelines) |
| Write inline tests | [User Guide: Adding Tests](guide.md#6-adding-tests) |
| Run tests against an LLM | Use the `skillspec-test` skill in `skills/skillspec-test.agent` |
| Review a skill for quality | Use the `skill-writer` skill in `skills/skill-writer.agent` |
| Migrate an existing SKILL.md | `skillspec migrate existing/SKILL.md`, then use `skills/skillspec-migrate.agent` |
| Full language syntax | [Language Reference](language-reference.md) |

## Core Skills

SkillSpec ships with four skills that extend the CLI with LLM-powered capabilities. These run in your agent runtime (Claude Code, Codex, etc.), not the CLI:

| Skill | What it does |
|-------|-------------|
| `skills/skillspec-test.agent` | Executes test blocks — deterministic + LLM-judged assertions |
| `skills/skill-writer.agent` | Reviews .agent files against SkillSpec design principles |
| `skills/skillspec-migrate.agent` | Completes .agent.partial files with inferred types and dependencies |
| `skills/skillspec-backport.agent` | Maps SKILL.md changes back to .agent source locations |
