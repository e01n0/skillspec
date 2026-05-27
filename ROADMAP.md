# Roadmap

What's working, what's partially done, and where this goes next.

---

## Current state (v0.1.0)

The full compile pipeline is complete: lex → parse → check → compile (SKILL.md and native IR). Developer tooling (fmt, budget, deps, migrate, lint), package management (pack, install), structural diff, and `optimize` (SkillOpt integration) are all shipped.

Also shipped:
- **Formal grammar (EBNF)** — `skillspec grammar` prints the full machine-readable spec
- **Backport skill** — `skills/skillspec-backport.agent` maps SKILL.md edits back to `.agent` source

---

## Near-term

- **Remote package registry** — `skillspec install <name>` pulling from a hosted registry rather than local `.skillpkg` directories
- **LLM-powered test execution** (`skillspec-test` skill) — run the `tests {}` blocks against a live model and report pass/fail
- **Language server (LSP)** — IDE integration: go-to-definition, hover docs, inline diagnostics
- **Syntax highlighting** — VS Code and JetBrains grammar definitions for `.agent` files

## Medium-term

- **Token budget optimisation suggestions** — `skillspec budget` currently reports; make it suggest reductions
- **Native runtime SDK** — a small library for runtimes that want to consume `.agentpkg` bundles directly without invoking the CLI

## Long-term

- **Skill marketplace / registry hosting** — versioned registry with search, install, and publish
- **Visual skill editor** — drag-and-drop step/stage/phase composer that emits valid `.agent` source
- **Formal verification of skill contracts** — prove that a skill's type constraints are satisfiable before it reaches a model
- **Multi-model testing** — run the same `tests {}` suite across different LLMs and diff the results
