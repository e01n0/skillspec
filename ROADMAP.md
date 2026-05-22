# Roadmap

What's working, what's partially done, and where this goes next.

---

## Current state (v0.1.0)

The full compile pipeline is complete: lex → parse → check → compile (SKILL.md and native IR). Developer tooling (fmt, budget, deps, migrate), package management (pack, install), and structural diff are all shipped. 89 tests pass.

---

## Near-term

- **Remote package registry** — `skillspec install <name>` pulling from a hosted registry rather than local `.skillpkg` directories
- **LLM-powered test execution** (`skillspec-test` skill) — run the `tests {}` blocks against a live model and report pass/fail
- **Backport skill** — LLM-assisted `SKILL.md` → `.agent` reconciliation for skills that pre-date the language
- **Language server (LSP)** — IDE integration: go-to-definition, hover docs, inline diagnostics
- **Syntax highlighting** — VS Code and JetBrains grammar definitions for `.agent` files

## Medium-term

- **Formal grammar (EBNF)** — a machine-readable spec so third-party parsers and tools can target SkillSpec
- **Token budget optimisation suggestions** — `skillspec budget` currently reports; make it suggest reductions
- **Native runtime SDK** — a small library for runtimes that want to consume `.agentpkg` bundles directly without invoking the CLI

## Long-term

- **Skill marketplace / registry hosting** — versioned registry with search, install, and publish
- **Visual skill editor** — drag-and-drop step/stage/phase composer that emits valid `.agent` source
- **Formal verification of skill contracts** — prove that a skill's type constraints are satisfiable before it reaches a model
- **Multi-model testing** — run the same `tests {}` suite across different LLMs and diff the results
