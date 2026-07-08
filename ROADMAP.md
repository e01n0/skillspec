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

- **Rules engine tiers 2–3** — `skillspec rules` currently runs the deterministic tiers (dedup, drift, polarity clash, priority mismatch). Next: embedding-based candidate pairing and a local NLI cross-encoder behind a `--semantic` flag (local ONNX models, still no API calls), benchmarked against a hand-labeled set of real skill conflicts first — see docs/research-conflict-detection.md for why off-the-shelf NLI accuracy on imperative text must be measured, not assumed.
- **Trigger-based co-activation analysis** — derive which skills can be loaded simultaneously from their descriptions/triggers, so conflict checks only run within genuinely co-active sets.

- **Remote package registry** — `skillspec install <name>` pulling from a hosted registry rather than local `.skillpkg` directories
- **LLM-powered test execution** (`skillspec-test` skill) — run the `tests {}` blocks against a live model and report pass/fail
- **Language server (LSP)** — IDE integration: go-to-definition, hover docs, inline diagnostics
- **Tree-sitter grammar** — unlocks highlighting on github.com/Neovim/Zed and forms the parsing backbone for the LSP (a TextMate grammar for VS Code and other editors ships in `editors/`)

## Medium-term

- **Token budget optimisation suggestions** — `skillspec budget` currently reports; make it suggest reductions
- **Native runtime SDK** — a small library for runtimes that want to consume `.agentpkg` bundles directly without invoking the CLI

## Long-term

- **Skill marketplace / registry hosting** — versioned registry with search, install, and publish
- **Visual skill editor** — drag-and-drop step/stage/phase composer that emits valid `.agent` source
- **Formal verification of skill contracts** — prove that a skill's type constraints are satisfiable before it reaches a model
- **Multi-model testing** — run the same `tests {}` suite across different LLMs and diff the results
