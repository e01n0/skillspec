# Quickstart

## Install

```sh
git clone git@github.com:e01n0/skillspec.git
cd skillspec
cargo install --path .
```

`skillspec --version` should print `skillspec 0.1.0`.

## Scaffold and build

```sh
skillspec init code-helper
```

That gives you `code-helper.agent`:

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

Edit it into something useful:

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

Then:

```sh
skillspec check code-helper.agent   # type-check
skillspec build code-helper.agent   # compile to code-helper/SKILL.md
skillspec budget code-helper.agent  # token estimate
skillspec deps code-helper.agent    # step graph
skillspec fmt code-helper.agent     # canonical formatting
```

## Deploy to a runtime

`--to` builds and deploys in one step. Pass a named runtime or any path:

```sh
skillspec build code-helper.agent --to claude          # ~/.claude/skills/
skillspec build code-helper.agent --to claude-project   # .claude/skills/ (repo-scoped)
skillspec build code-helper.agent --to cursor           # .cursor/rules/ (auto-selects cursor target)
skillspec build code-helper.agent --to cline            # ./ as .clinerules
skillspec build code-helper.agent --to codex            # .codex/
skillspec build code-helper.agent --to /my/custom/path  # anywhere
skillspec build code-helper.agent --to                  # interactive menu
```

Combine with `--watch` to auto-redeploy on save:

```sh
skillspec build code-helper.agent --to claude --watch
```

`--to` and `-o` are mutually exclusive. `-o` sets a raw output directory; `--to` resolves named runtimes and auto-selects the build target (e.g. `--to cursor` implies `--target cursor`).

## Where to go from here

| Want to... | Read |
|-----------|------|
| Add lazy context loading | [Guide: Context](guide.md#context) |
| Declare tool dependencies | [Guide: Tools and permissions](guide.md#tools-and-permissions) |
| Compose skills into pipelines | [Guide: Pipelines](guide.md#pipelines) |
| Write inline tests | [Guide: Tests](guide.md#tests) |
| Run tests against an LLM | `skills/skillspec-test.agent` (runs in your runtime) |
| Review a skill for quality | `skills/skill-writer.agent` |
| Migrate an existing SKILL.md | `skillspec migrate existing/SKILL.md`, then `skills/skillspec-migrate.agent` |
| Full language syntax | [Language Reference](language-reference.md) |

## Bundled skills

These run in your agent runtime, not the CLI:

| Skill | Purpose |
|-------|---------|
| `skills/skillspec-test.agent` | Run test blocks (deterministic + LLM-judged) |
| `skills/skill-writer.agent` | Review `.agent` files for quality |
| `skills/skillspec-migrate.agent` | Complete `.agent.partial` files from migration |
| `skills/skillspec-backport.agent` | Map SKILL.md edits back to `.agent` source |
